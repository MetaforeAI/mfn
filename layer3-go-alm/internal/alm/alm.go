package alm

import (
	"context"
	"fmt"
	"runtime"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/mfn/layer3_alm/internal/config"
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
}

// NewALM creates a new Associative Link Mesh instance
func NewALM(config *config.ALMConfig) (*ALM, error) {
	ctx, cancel := context.WithCancel(context.Background())
	
	alm := &ALM{
		config:             config,
		ctx:                ctx,
		cancel:             cancel,
		metrics:            &PerformanceMetrics{},
		monitor:            NewPerformanceMonitor(),
		useOptimizedSearch: true, // Enable optimized search by default
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
	
	alm.metrics.MemoriesAdded++
	memoriesAddedCounter.Inc()
	
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
	associationsAddedCounter.Inc()
	
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

// discoverAssociations automatically discovers associations for a new memory
func (alm *ALM) discoverAssociations(memory *Memory) {
	// This is a placeholder for more sophisticated association discovery
	// In a real implementation, this might use NLP, embedding similarity, etc.
	
	alm.mu.RLock()
	defer alm.mu.RUnlock()
	
	// Find memories with similar tags
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
				assoc := &Association{
					ID:           uuid.New().String(),
					FromMemoryID: memory.ID,
					ToMemoryID:   otherMemory.ID,
					Type:         string(AssociationSemantic),
					Weight:       weight,
					Reason:       fmt.Sprintf("Common tags: %d", commonTags),
					CreatedAt:    time.Now(),
				}
				
				// Add association (unlock to avoid deadlock)
				alm.mu.RUnlock()
				alm.AddAssociation(assoc)
				alm.mu.RLock()
			}
		}
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
	searchDurationHistogram.Observe(searchTime.Seconds())
	searchCounter.Inc()
}

// Prometheus metrics
var (
	searchCounter = promauto.NewCounter(prometheus.CounterOpts{
		Name: "alm_searches_total",
		Help: "Total number of associative searches performed",
	})
	
	searchDurationHistogram = promauto.NewHistogram(prometheus.HistogramOpts{
		Name: "alm_search_duration_seconds",
		Help: "Duration of associative searches",
		Buckets: prometheus.ExponentialBuckets(0.001, 2, 10), // 1ms to ~1s
	})
	
	memoriesAddedCounter = promauto.NewCounter(prometheus.CounterOpts{
		Name: "alm_memories_added_total", 
		Help: "Total number of memories added",
	})
	
	associationsAddedCounter = promauto.NewCounter(prometheus.CounterOpts{
		Name: "alm_associations_added_total",
		Help: "Total number of associations added",
	})
	
	memoryGraphSize = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "alm_graph_size",
		Help: "Size of the memory graph",
	}, []string{"type"})
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
				memoryGraphSize.WithLabelValues("memories").Set(float64(stats.TotalMemories))
				memoryGraphSize.WithLabelValues("associations").Set(float64(stats.TotalAssociations))
			}
		}
	}()
}