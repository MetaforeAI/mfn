package alm

import (
	"context"
	"fmt"
	"log"
	"runtime"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/mfn/layer3_alm/internal/config"
	"github.com/mfn/layer3_alm/internal/persistence"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

// ALM implements the Associative Link Mesh for Layer 3
type ALM struct {
	config            *config.ALMConfig
	graph             *MemoryGraph
	searcher          *AssociativeSearcher
	optimizedSearcher *OptimizedSearcher
	metrics           *PerformanceMetrics
	monitor           *PerformanceMonitor
	pool              *ObjectPool
	cache             *MemoryCache
	goroutinePool     *GoroutinePool  // Goroutine pool to prevent exhaustion

	// Concurrency control
	mu        sync.RWMutex
	ctx       context.Context
	cancel    context.CancelFunc

	// Background tasks
	gcTicker  *time.Ticker
	wg        sync.WaitGroup

	// Performance optimizations
	useOptimizedSearch bool

	// Persistence (optional)
	persistenceConfig *persistence.Config
	aofHandle         *persistence.AofHandle
	aofEntryChan      chan *persistence.AofEntry
	snapshotCreator   *persistence.SnapshotCreator
	snapshotTicker    *time.Ticker

	// Pool identification for metrics
	poolID            string
}

// NewALM creates a new Associative Link Mesh instance
func NewALM(config *config.ALMConfig) (*ALM, error) {
	return NewALMWithPersistence(config, nil)
}

// NewALMWithPersistence creates a new Associative Link Mesh instance with optional persistence
func NewALMWithPersistence(config *config.ALMConfig, persistConfig *persistence.Config) (*ALM, error) {
	ctx, cancel := context.WithCancel(context.Background())

	poolID := "default"
	if persistConfig != nil {
		poolID = persistConfig.PoolID
	}

	alm := &ALM{
		config:             config,
		ctx:                ctx,
		cancel:             cancel,
		metrics:            &PerformanceMetrics{},
		monitor:            nil, // Disabled for multi-pool to avoid metric collisions
		useOptimizedSearch: true, // Enable optimized search by default
		poolID:             poolID,
	}

	// Initialize memory graph with full configuration
	graph, err := NewMemoryGraphWithConfig(
		config.MaxMemories,
		config.MaxAssociations,
		config.MaxEdgesPerNode,
		config.EdgeTTL,
	)
	if err != nil {
		cancel()
		return nil, fmt.Errorf("failed to create memory graph: %w", err)
	}
	alm.graph = graph

	// Initialize performance optimizations
	alm.pool = NewObjectPool()
	alm.cache = NewMemoryCache(10000, 5000, 2000)

	// Initialize goroutine pool
	alm.goroutinePool = NewGoroutinePool(config.MaxGoroutines)

	// Initialize both searchers
	alm.searcher = NewAssociativeSearcher(alm.graph, config)
	alm.optimizedSearcher = NewOptimizedSearcher(alm.graph, config)

	// Optional persistence initialization
	if persistConfig != nil {
		log.Printf("Initializing persistence: data_dir=%s, pool_id=%s",
			persistConfig.DataDir, persistConfig.PoolID)

		// Create AOF writer
		aofHandle, entryChan := persistence.NewAofHandle()
		aofWriter, err := persistence.NewAofWriter(
			persistConfig.AofPath(),
			entryChan,
			persistConfig.FsyncIntervalMs,
			persistConfig.AofBufferSize,
		)
		if err != nil {
			cancel()
			return nil, fmt.Errorf("failed to create AOF writer: %w", err)
		}

		// Start AOF writer in background
		go func() {
			if err := aofWriter.Run(); err != nil {
				log.Printf("AOF writer error: %v", err)
			}
		}()

		// Create snapshot creator
		snapshotCreator, err := persistence.NewSnapshotCreator(persistConfig.SnapshotPath())
		if err != nil {
			cancel()
			return nil, fmt.Errorf("failed to create snapshot creator: %w", err)
		}

		alm.persistenceConfig = persistConfig
		alm.aofHandle = aofHandle
		alm.aofEntryChan = entryChan
		alm.snapshotCreator = snapshotCreator

		// Start background snapshot task
		alm.startSnapshotTask()

		log.Printf("Persistence enabled: AOF=%s, snapshots every %ds",
			persistConfig.AofPath(), persistConfig.SnapshotIntervalSecs)
	}

	// Start background tasks
	alm.startBackgroundTasks()

	// Register Prometheus metrics
	alm.registerMetrics()

	return alm, nil
}

// AddMemory adds a new memory to the ALM
func (alm *ALM) AddMemory(memory *Memory) error {
	start := time.Now()
	defer func() {
		if alm.monitor != nil {
			alm.monitor.RecordMemoryLatency(time.Since(start))
		}
	}()
	
	alm.mu.Lock()
	defer alm.mu.Unlock()
	
	if memory.ID == 0 {
		err := fmt.Errorf("memory ID cannot be zero")
		if alm.monitor != nil {
			alm.monitor.RecordError(err, "validation")
		}
		return err
	}
	
	memory.CreatedAt = time.Now()
	memory.LastAccessed = time.Now()
	memory.AccessCount = 0
	
	if err := alm.graph.AddMemory(memory); err != nil {
		wrappedErr := fmt.Errorf("failed to add memory to graph: %w", err)
		if alm.monitor != nil {
			alm.monitor.RecordError(wrappedErr, "graph")
		}
		return wrappedErr
	}

	// Log to AOF (non-blocking)
	if alm.aofHandle != nil {
		if err := alm.aofHandle.LogAddMemory(persistence.MemoryId(memory.ID), memory.Content, nil); err != nil {
			log.Printf("Warning: Failed to log to AOF: %v", err)
		}
	}

	alm.metrics.MemoriesAdded++
	memoriesAddedCounter.WithLabelValues(alm.poolID).Inc()
	
	// Auto-discover associations if enabled
	if alm.config.EnableAutoDiscovery && alm.goroutinePool != nil {
		// Use goroutine pool to prevent exhaustion
		memCopy := *memory // Copy to avoid closure issues
		alm.goroutinePool.Submit(func() {
			alm.discoverAssociations(&memCopy)
		})
	}
	
	return nil
}

// AddAssociation adds a new association between memories
func (alm *ALM) AddAssociation(assoc *Association) error {
	alm.mu.Lock()
	defer alm.mu.Unlock()
	
	if assoc.ID == "" {
		assoc.ID = uuid.New().String()
	}
	
	assoc.CreatedAt = time.Now()
	assoc.LastUsed = time.Now()
	assoc.UsageCount = 0
	
	if err := alm.graph.AddAssociation(assoc); err != nil {
		return fmt.Errorf("failed to add association to graph: %w", err)
	}
	
	alm.metrics.AssociationsAdded++
	associationsAddedCounter.WithLabelValues(alm.poolID).Inc()
	
	return nil
}

// GetMemory retrieves a memory by ID with caching
func (alm *ALM) GetMemory(id uint64) (*Memory, error) {
	start := time.Now()
	defer func() {
		if alm.monitor != nil {
			alm.monitor.RecordMemoryLatency(time.Since(start))
		}
	}()
	
	// Try cache first
	if alm.cache != nil {
		if cached := alm.cache.GetMemory(id); cached != nil {
			if alm.monitor != nil {
				alm.monitor.RecordCacheHit()
			}
			// Update access statistics
			cached.LastAccessed = time.Now()
			cached.AccessCount++
			alm.metrics.MemoriesAccessed++
			return cached, nil
		}
		if alm.monitor != nil {
			alm.monitor.RecordCacheMiss()
		}
	}
	
	alm.mu.RLock()
	defer alm.mu.RUnlock()
	
	memory := alm.graph.GetMemory(id)
	if memory == nil {
		err := fmt.Errorf("memory %d not found", id)
		if alm.monitor != nil {
			alm.monitor.RecordError(err, "not_found")
		}
		return nil, err
	}
	
	// Update access statistics
	memory.LastAccessed = time.Now()
	memory.AccessCount++
	alm.metrics.MemoriesAccessed++
	
	// Cache the memory
	if alm.cache != nil {
		alm.cache.SetMemory(memory)
	}
	
	return memory, nil
}

// SearchAssociative performs associative search from starting memories
func (alm *ALM) SearchAssociative(ctx context.Context, query *SearchQuery) (*SearchResults, error) {
	startTime := time.Now()
	defer func() {
		if alm.monitor != nil {
			alm.monitor.RecordSearchLatency(time.Since(startTime))
		}
	}()
	
	// Apply default timeout if not specified
	if query.Timeout == 0 {
		query.Timeout = alm.config.SearchTimeout
	}
	
	// Create search context with timeout
	searchCtx, cancel := context.WithTimeout(ctx, query.Timeout)
	defer cancel()
	
	var results *SearchResults
	var err error
	
	// Use optimized searcher if enabled
	if alm.useOptimizedSearch && alm.optimizedSearcher != nil {
		results, err = alm.optimizedSearcher.Search(searchCtx, query)
	} else {
		results, err = alm.searcher.Search(searchCtx, query)
	}
	
	if err != nil {
		wrappedErr := fmt.Errorf("associative search failed: %w", err)
		if alm.monitor != nil {
			alm.monitor.RecordError(wrappedErr, "search")
		}
		return nil, wrappedErr
	}
	
	// Update performance metrics
	searchTime := time.Since(startTime)
	alm.updateSearchMetrics(searchTime)
	
	return results, nil
}

// SearchByText performs text-based search across all memories, scoring by relevance.
// It tokenizes the query into lowercase words and scores each memory by how many
// query tokens appear in its content (case-insensitive substring match).
// Memories that match are returned directly as results, and optionally their
// graph neighbors are also included for associative context.
func (alm *ALM) SearchByText(ctx context.Context, queryText string, maxResults int) (*SearchResults, error) {
	startTime := time.Now()

	if queryText == "" {
		return &SearchResults{
			Results:    []*SearchResult{},
			TotalFound: 0,
			SearchTime: time.Since(startTime),
		}, nil
	}

	alm.mu.RLock()
	defer alm.mu.RUnlock()

	allMemories := alm.graph.GetAllMemories()

	if len(allMemories) == 0 {
		return &SearchResults{
			Results:    []*SearchResult{},
			TotalFound: 0,
			SearchTime: time.Since(startTime),
		}, nil
	}

	// Tokenize query into lowercase words for matching
	queryLower := strings.ToLower(queryText)
	queryTokens := strings.Fields(queryLower)

	if len(queryTokens) == 0 {
		return &SearchResults{
			Results:    []*SearchResult{},
			TotalFound: 0,
			SearchTime: time.Since(startTime),
		}, nil
	}

	type scoredMemory struct {
		memory *Memory
		score  float64
	}

	scored := make([]scoredMemory, 0, len(allMemories))

	for _, mem := range allMemories {
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		default:
		}

		contentLower := strings.ToLower(mem.Content)

		// Score: fraction of query tokens found in content
		matchCount := 0
		for _, token := range queryTokens {
			if strings.Contains(contentLower, token) {
				matchCount++
			}
		}

		if matchCount == 0 {
			// Also check if the full query appears as a substring
			if !strings.Contains(contentLower, queryLower) {
				continue
			}
			matchCount = len(queryTokens) // Full match gets max token score
		}

		score := float64(matchCount) / float64(len(queryTokens))
		scored = append(scored, scoredMemory{memory: mem, score: score})
	}

	// Sort by score descending
	sort.Slice(scored, func(i, j int) bool {
		return scored[i].score > scored[j].score
	})

	if len(scored) > maxResults {
		scored = scored[:maxResults]
	}

	results := make([]*SearchResult, 0, len(scored))
	for _, sm := range scored {
		results = append(results, &SearchResult{
			Memory:      sm.memory,
			Path:        []*PathStep{},
			TotalWeight: sm.score,
			Depth:       0,
			SearchTime:  time.Since(startTime),
		})
	}

	searchTime := time.Since(startTime)
	alm.updateSearchMetrics(searchTime)

	return &SearchResults{
		Results:       results,
		TotalFound:    len(results),
		SearchTime:    searchTime,
		NodesExplored: len(allMemories),
		PathsFound:    len(results),
	}, nil
}

// GetGraphStats returns statistics about the memory graph
func (alm *ALM) GetGraphStats() *GraphStats {
	alm.mu.RLock()
	defer alm.mu.RUnlock()
	
	return alm.graph.GetStats()
}

// GetPerformanceMetrics returns current performance metrics
func (alm *ALM) GetPerformanceMetrics() *PerformanceMetrics {
	alm.mu.RLock()
	defer alm.mu.RUnlock()
	
	// Create a copy to avoid race conditions
	metrics := *alm.metrics
	return &metrics
}

// GetComprehensiveMetrics returns detailed performance metrics
func (alm *ALM) GetComprehensiveMetrics() map[string]interface{} {
	metrics := make(map[string]interface{})

	// Add monitor stats if available
	if alm.monitor != nil {
		for k, v := range alm.monitor.GetComprehensiveStats() {
			metrics[k] = v
		}
	} else {
		// Fallback to basic metrics
		metrics["basic_metrics"] = alm.GetPerformanceMetrics()
	}

	// Add goroutine pool stats
	if alm.goroutinePool != nil {
		poolStats := alm.goroutinePool.GetStats()
		metrics["goroutine_pool"] = map[string]interface{}{
			"worker_count":   poolStats.WorkerCount,
			"active_count":   poolStats.ActiveCount,
			"queue_length":   poolStats.QueueLength,
			"total_executed": poolStats.TotalExecuted,
			"max_workers":    poolStats.MaxWorkers,
		}
	}

	// Add memory stats
	var m runtime.MemStats
	runtime.ReadMemStats(&m)
	metrics["memory"] = map[string]interface{}{
		"alloc_mb":       m.Alloc / 1024 / 1024,
		"total_alloc_mb": m.TotalAlloc / 1024 / 1024,
		"sys_mb":         m.Sys / 1024 / 1024,
		"num_gc":         m.NumGC,
		"goroutines":     runtime.NumGoroutine(),
	}

	metrics["timestamp"] = time.Now()
	return metrics
}

// GetGraph returns the underlying memory graph
func (alm *ALM) GetGraph() *MemoryGraph {
	return alm.graph
}

// Close shuts down the ALM and releases resources
func (alm *ALM) Close() error {
	alm.cancel()

	if alm.gcTicker != nil {
		alm.gcTicker.Stop()
	}

	// Stop persistence
	if alm.snapshotTicker != nil {
		alm.snapshotTicker.Stop()
	}
	if alm.aofHandle != nil {
		alm.aofHandle.Close()
	}
	if alm.snapshotCreator != nil {
		alm.snapshotCreator.Close()
	}

	// Close goroutine pool
	if alm.goroutinePool != nil {
		alm.goroutinePool.Close()
	}

	// Close optimized searcher
	if alm.optimizedSearcher != nil {
		alm.optimizedSearcher.Close()
	}

	// Close performance monitor
	if alm.monitor != nil {
		alm.monitor.Stop()
	}

	alm.wg.Wait()
	return nil
}

// startBackgroundTasks starts maintenance tasks
func (alm *ALM) startBackgroundTasks() {
	// Garbage collection task
	if alm.config.GCInterval > 0 {
		alm.gcTicker = time.NewTicker(alm.config.GCInterval)
		alm.wg.Add(1)

		go func() {
			defer alm.wg.Done()
			for {
				select {
				case <-alm.ctx.Done():
					return
				case <-alm.gcTicker.C:
					alm.performGC()
				}
			}
		}()
	}

	// TTL eviction task
	if alm.config.EvictionInterval > 0 {
		alm.wg.Add(1)

		go func() {
			defer alm.wg.Done()
			ticker := time.NewTicker(alm.config.EvictionInterval)
			defer ticker.Stop()

			for {
				select {
				case <-alm.ctx.Done():
					return
				case <-ticker.C:
					evicted := alm.graph.EvictExpiredEdges()
					if evicted > 0 {
						fmt.Printf("TTL Eviction: Removed %d expired edges\n", evicted)
					}
				}
			}
		}()
	}

	// Weight decay task
	if alm.config.EnableWeightDecay && alm.config.WeightDecayRate > 0 {
		alm.wg.Add(1)

		go func() {
			defer alm.wg.Done()
			ticker := time.NewTicker(1 * time.Minute)
			defer ticker.Stop()

			for {
				select {
				case <-alm.ctx.Done():
					return
				case <-ticker.C:
					alm.performWeightDecay()
				}
			}
		}()
	}
}

// performGC performs garbage collection of unused memories and associations
func (alm *ALM) performGC() {
	alm.mu.Lock()
	defer alm.mu.Unlock()
	
	cutoffTime := time.Now().Add(-alm.config.MaxIdleTime)
	
	// Remove unused memories
	removed := alm.graph.RemoveUnusedMemories(cutoffTime)
	if removed > 0 {
		fmt.Printf("GC: Removed %d unused memories\n", removed)
	}
	
	// Remove weak associations
	weakAssocs := alm.graph.RemoveWeakAssociations(alm.config.MinAssocThreshold)
	if weakAssocs > 0 {
		fmt.Printf("GC: Removed %d weak associations\n", weakAssocs)
	}
}

// performWeightDecay applies decay to association weights
func (alm *ALM) performWeightDecay() {
	alm.mu.Lock()
	defer alm.mu.Unlock()

	alm.graph.ApplyWeightDecay(alm.config.WeightDecayRate)
}

// startSnapshotTask starts the background snapshot task
func (alm *ALM) startSnapshotTask() {
	if alm.persistenceConfig == nil || alm.snapshotCreator == nil {
		return
	}

	alm.snapshotTicker = time.NewTicker(
		time.Duration(alm.persistenceConfig.SnapshotIntervalSecs) * time.Second,
	)

	alm.wg.Add(1)
	go func() {
		defer alm.wg.Done()
		for {
			select {
			case <-alm.snapshotTicker.C:
				// Create snapshot from current graph state
				edges := alm.getEdgesForSnapshot()
				if err := alm.snapshotCreator.CreateSnapshot(edges); err != nil {
					log.Printf("Failed to create snapshot: %v", err)
				} else {
					log.Printf("Snapshot created: %d edges", len(edges))
				}
			case <-alm.ctx.Done():
				return
			}
		}
	}()
}

// getEdgesForSnapshot extracts edges from the graph for snapshotting
func (alm *ALM) getEdgesForSnapshot() map[persistence.MemoryId]*persistence.EdgeSnapshot {
	alm.mu.RLock()
	defer alm.mu.RUnlock()

	edges := make(map[persistence.MemoryId]*persistence.EdgeSnapshot)

	// Get all memories from graph
	memories := alm.graph.GetAllMemories()
	for _, memory := range memories {
		var connectionID *string
		// Note: We don't track connection_id at ALM level yet, so it's nil
		edge := &persistence.EdgeSnapshot{
			MemoryId:                persistence.MemoryId(memory.ID),
			Content:                 memory.Content,
			Strength:                1.0, // Default strength
			ActivationCount:         uint64(memory.AccessCount),
			ConnectionId:            connectionID,
			CreatedTimestampMs:      memory.CreatedAt.UnixMilli(),
			LastAccessedTimestampMs: memory.LastAccessed.UnixMilli(),
		}
		edges[persistence.MemoryId(memory.ID)] = edge
	}

	return edges
}

// discoverAssociations automatically discovers associations for a new memory.
// It collects all associations to add while holding the RLock, then releases
// the lock before adding them to avoid lock-unlock-relock cycles.
func (alm *ALM) discoverAssociations(memory *Memory) {
	var toAdd []*Association

	alm.mu.RLock()
	// Find memories with similar tags and collect associations to create
	for _, otherMemory := range alm.graph.GetAllMemories() {
		if otherMemory.ID == memory.ID {
			continue
		}

		// Calculate tag similarity
		commonTags := 0
		for _, tag1 := range memory.Tags {
			for _, tag2 := range otherMemory.Tags {
				if tag1 == tag2 {
					commonTags++
					break
				}
			}
		}

		if commonTags > 0 {
			weight := float64(commonTags) / float64(len(memory.Tags)+len(otherMemory.Tags)-commonTags)
			if weight >= alm.config.MinAssocThreshold {
				toAdd = append(toAdd, &Association{
					ID:           uuid.New().String(),
					FromMemoryID: memory.ID,
					ToMemoryID:   otherMemory.ID,
					Type:         string(AssociationSemantic),
					Weight:       weight,
					Reason:       fmt.Sprintf("Common tags: %d", commonTags),
					CreatedAt:    time.Now(),
				})
			}
		}
	}
	alm.mu.RUnlock()

	// Add all discovered associations without holding any lock
	for _, assoc := range toAdd {
		alm.AddAssociation(assoc)
	}
}

// updateSearchMetrics updates search performance metrics
func (alm *ALM) updateSearchMetrics(searchTime time.Duration) {
	alm.metrics.TotalSearches++
	
	if alm.metrics.TotalSearches == 1 {
		alm.metrics.AverageSearchTime = searchTime
		alm.metrics.FastestSearch = searchTime
		alm.metrics.SlowestSearch = searchTime
	} else {
		// Update running average
		totalTime := time.Duration(alm.metrics.TotalSearches-1) * alm.metrics.AverageSearchTime + searchTime
		alm.metrics.AverageSearchTime = totalTime / time.Duration(alm.metrics.TotalSearches)
		
		if searchTime < alm.metrics.FastestSearch {
			alm.metrics.FastestSearch = searchTime
		}
		
		if searchTime > alm.metrics.SlowestSearch {
			alm.metrics.SlowestSearch = searchTime
		}
	}
	
	// Update Prometheus metrics
	searchDurationHistogram.WithLabelValues(alm.poolID).Observe(searchTime.Seconds())
	searchCounter.WithLabelValues(alm.poolID).Inc()
}

// Prometheus metrics (shared across all pools)
var (
	searchCounter = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "alm_searches_total",
		Help: "Total number of associative searches performed",
	}, []string{"pool_id"})

	searchDurationHistogram = promauto.NewHistogramVec(prometheus.HistogramOpts{
		Name: "alm_search_duration_seconds",
		Help: "Duration of associative searches",
		Buckets: prometheus.ExponentialBuckets(0.001, 2, 10), // 1ms to ~1s
	}, []string{"pool_id"})

	memoriesAddedCounter = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "alm_memories_added_total",
		Help: "Total number of memories added",
	}, []string{"pool_id"})

	associationsAddedCounter = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "alm_associations_added_total",
		Help: "Total number of associations added",
	}, []string{"pool_id"})

	memoryGraphSize = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "alm_graph_size",
		Help: "Size of the memory graph",
	}, []string{"pool_id", "type"})
)

var (
	metricsRegistered bool
	metricsOnce       sync.Once
)

// registerMetrics registers Prometheus metrics
func (alm *ALM) registerMetrics() {
	// Update graph size metrics periodically
	go func() {
		ticker := time.NewTicker(30 * time.Second)
		defer ticker.Stop()

		for {
			select {
			case <-alm.ctx.Done():
				return
			case <-ticker.C:
				stats := alm.GetGraphStats()
				memoryGraphSize.WithLabelValues(alm.poolID, "memories").Set(float64(stats.TotalMemories))
				memoryGraphSize.WithLabelValues(alm.poolID, "associations").Set(float64(stats.TotalAssociations))
			}
		}
	}()
}