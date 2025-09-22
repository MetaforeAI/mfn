package alm

import (
	"context"
	"fmt"
	"hash/fnv"
	"runtime"
	"sort"
	"sync"
	"sync/atomic"
	"time"

	"github.com/mfn/layer3_alm/internal/config"
)

// OptimizedSearcher provides high-performance associative search with parallel processing
type OptimizedSearcher struct {
	graph         *MemoryGraph
	config        *config.ALMConfig
	pool          *ObjectPool
	cache         *MemoryCache
	
	// Enhanced worker pools with priority scheduling
	searchWorkers    *PriorityWorkerPool
	traversalWorkers *PriorityWorkerPool
	
	// Performance optimizations
	hotPathCache     sync.Map // Cache for frequently traversed paths
	queryCache       sync.Map // Cache for query results
	edgeIndex        sync.Map // Index of edges by weight for fast filtering
	
	// Statistics
	cacheHits        int64
	cacheMisses      int64
	paralleilzedOps  int64
}

// PriorityWorkerPool manages workers with task prioritization
type PriorityWorkerPool struct {
	workers      int
	highPrioWork chan func()
	lowPrioWork  chan func()
	quit         chan struct{}
	wg           sync.WaitGroup
}

// NewOptimizedSearcher creates a high-performance searcher
func NewOptimizedSearcher(graph *MemoryGraph, config *config.ALMConfig) *OptimizedSearcher {
	searcher := &OptimizedSearcher{
		graph:  graph,
		config: config,
		pool:   NewObjectPool(),
		cache:  NewMemoryCache(10000, 5000, 2000), // Larger caches
	}
	
	// Create priority worker pools with more workers
	numCPU := runtime.NumCPU()
	searcher.searchWorkers = NewPriorityWorkerPool(numCPU * 2)
	searcher.traversalWorkers = NewPriorityWorkerPool(numCPU * 4)
	
	return searcher
}

// NewPriorityWorkerPool creates a worker pool with priority scheduling
func NewPriorityWorkerPool(workers int) *PriorityWorkerPool {
	pool := &PriorityWorkerPool{
		workers:      workers,
		highPrioWork: make(chan func(), workers*4),
		lowPrioWork:  make(chan func(), workers*8),
		quit:         make(chan struct{}),
	}
	
	// Start worker goroutines
	for i := 0; i < workers; i++ {
		pool.wg.Add(1)
		go pool.worker()
	}
	
	return pool
}

// worker runs tasks with priority scheduling
func (p *PriorityWorkerPool) worker() {
	defer p.wg.Done()
	
	for {
		select {
		case work := <-p.highPrioWork:
			work()
		case work := <-p.lowPrioWork:
			work()
		case <-p.quit:
			return
		}
	}
}

// SubmitHighPriority submits high priority work
func (p *PriorityWorkerPool) SubmitHighPriority(work func()) {
	select {
	case p.highPrioWork <- work:
	case <-p.quit:
	}
}

// SubmitLowPriority submits low priority work
func (p *PriorityWorkerPool) SubmitLowPriority(work func()) {
	select {
	case p.lowPrioWork <- work:
	case <-p.quit:
	}
}

// Close shuts down the worker pool
func (p *PriorityWorkerPool) Close() {
	close(p.quit)
	p.wg.Wait()
}

// Close shuts down the optimized searcher
func (s *OptimizedSearcher) Close() {
	if s.searchWorkers != nil {
		s.searchWorkers.Close()
	}
	if s.traversalWorkers != nil {
		s.traversalWorkers.Close()
	}
}

// Search performs optimized associative search
func (s *OptimizedSearcher) Search(ctx context.Context, query *SearchQuery) (*SearchResults, error) {
	startTime := time.Now()
	
	// Generate cache key for query
	queryKey := s.generateQueryHash(query)
	
	// Check query cache first
	if cached, found := s.queryCache.Load(queryKey); found {
		atomic.AddInt64(&s.cacheHits, 1)
		cachedResults := cached.(*SearchResults)
		// Return fresh copy with updated search time
		return &SearchResults{
			Results:       cachedResults.Results,
			Query:         query,
			TotalFound:    cachedResults.TotalFound,
			SearchTime:    time.Since(startTime),
			NodesExplored: cachedResults.NodesExplored,
			PathsFound:    cachedResults.PathsFound,
		}, nil
	}
	atomic.AddInt64(&s.cacheMisses, 1)
	
	// Create search context with object pooling
	searchCtx := s.pool.GetSearchContext()
	defer s.pool.PutSearchContext(searchCtx)
	
	searchCtx.Query = query
	searchCtx.StartTime = startTime
	
	var results []*SearchResult
	var err error
	
	// Choose optimized algorithm based on query characteristics
	switch {
	case len(query.StartMemoryIDs) == 1 && query.MaxDepth <= 2:
		// Single-source, shallow search - use optimized BFS
		results, err = s.optimizedBFS(ctx, searchCtx)
	case len(query.StartMemoryIDs) > 1 && query.MaxDepth <= 3:
		// Multi-source search - use parallel BFS
		results, err = s.parallelMultiSourceBFS(ctx, searchCtx)
	case query.MaxDepth > 3:
		// Deep search - use A* with heuristics
		results, err = s.aStarSearch(ctx, searchCtx)
	default:
		// Default to parallel breadth-first search
		results, err = s.parallelBreadthFirstSearch(ctx, searchCtx)
	}
	
	if err != nil {
		return nil, fmt.Errorf("optimized search failed: %w", err)
	}
	
	// Prepare final results
	searchResults := &SearchResults{
		Results:       results,
		Query:         query,
		TotalFound:    len(results),
		SearchTime:    time.Since(startTime),
		NodesExplored: searchCtx.NodesExplored,
		PathsFound:    len(results),
	}
	
	// Cache results if they're not too large and search was successful
	if len(results) > 0 && len(results) <= 50 && searchResults.SearchTime < 100*time.Millisecond {
		s.queryCache.Store(queryKey, searchResults)
	}
	
	return searchResults, nil
}

// optimizedBFS performs single-source BFS with optimizations
func (s *OptimizedSearcher) optimizedBFS(ctx context.Context, searchCtx *SearchContext) ([]*SearchResult, error) {
	query := searchCtx.Query
	results := s.pool.GetSearchResults()
	defer s.pool.PutSearchResults(results)
	
	// Use object pooling for queue
	queue := s.pool.GetSearchItems()
	defer s.pool.PutSearchItems(queue)
	
	visited := make(map[uint64]bool, query.MaxResults*2) // Pre-size map
	
	// Initialize with starting memory
	startID := query.StartMemoryIDs[0]
	queue = append(queue, &SearchItem{
		MemoryID:    startID,
		Path:        s.pool.GetPathSteps(),
		TotalWeight: 1.0,
		Depth:       0,
		Priority:    1.0,
	})
	
	for len(queue) > 0 && len(results) < query.MaxResults {
		select {
		case <-ctx.Done():
			return results, ctx.Err()
		default:
		}
		
		// Dequeue with minimal allocations
		current := queue[0]
		queue[0] = nil // Help GC
		queue = queue[1:]
		
		if current.Depth > query.MaxDepth || visited[current.MemoryID] {
			s.pool.PutPathSteps(current.Path)
			continue
		}
		
		visited[current.MemoryID] = true
		searchCtx.mu.Lock()
		searchCtx.NodesExplored++
		searchCtx.mu.Unlock()
		
		// Get memory (try cache first)
		memory := s.cache.GetMemory(current.MemoryID)
		if memory == nil {
			memory = s.graph.GetMemory(current.MemoryID)
			if memory == nil {
				s.pool.PutPathSteps(current.Path)
				continue
			}
			s.cache.SetMemory(memory)
		}
		
		// Add as result (except depth 0)
		if current.Depth > 0 {
			result := &SearchResult{
				Memory:      memory,
				Path:        append([]*PathStep(nil), current.Path...), // Copy path
				TotalWeight: current.TotalWeight,
				Depth:       current.Depth,
				SearchTime:  time.Since(searchCtx.StartTime),
			}
			results = append(results, result)
		}
		
		// Get neighbors with edge optimization
		s.addNeighborsOptimized(current, &queue, visited, query)
		s.pool.PutPathSteps(current.Path)
	}
	
	// Sort results by weight efficiently
	s.sortResultsByWeight(results)
	
	// Return limited results
	if len(results) > query.MaxResults {
		results = results[:query.MaxResults]
	}
	
	return append([]*SearchResult(nil), results...), nil // Return copy
}

// parallelMultiSourceBFS performs parallel BFS from multiple starting points
func (s *OptimizedSearcher) parallelMultiSourceBFS(ctx context.Context, searchCtx *SearchContext) ([]*SearchResult, error) {
	query := searchCtx.Query
	
	// Channel to collect results from parallel workers
	resultsChan := make(chan []*SearchResult, len(query.StartMemoryIDs))
	var wg sync.WaitGroup
	
	// Launch parallel searches from each starting point
	for _, startID := range query.StartMemoryIDs {
		wg.Add(1)
		
		startIDCopy := startID
		s.searchWorkers.SubmitHighPriority(func() {
			defer wg.Done()
			
			// Create sub-query for this starting point
			subQuery := *query
			subQuery.StartMemoryIDs = []uint64{startIDCopy}
			subQuery.MaxResults = query.MaxResults / len(query.StartMemoryIDs) + 10 // Buffer
			
			subCtx := s.pool.GetSearchContext()
			subCtx.Query = &subQuery
			subCtx.StartTime = searchCtx.StartTime
			
			results, err := s.optimizedBFS(ctx, subCtx)
			s.pool.PutSearchContext(subCtx)
			
			if err == nil && len(results) > 0 {
				resultsChan <- results
			} else {
				resultsChan <- []*SearchResult{}
			}
		})
	}
	
	// Wait for all searches to complete
	go func() {
		wg.Wait()
		close(resultsChan)
	}()
	
	// Collect and merge results
	allResults := s.pool.GetSearchResults()
	defer s.pool.PutSearchResults(allResults)
	
	for results := range resultsChan {
		allResults = append(allResults, results...)
		searchCtx.mu.Lock()
		searchCtx.NodesExplored += len(results)
		searchCtx.mu.Unlock()
	}
	
	// Sort merged results by weight
	s.sortResultsByWeight(allResults)
	
	// Limit results
	if len(allResults) > query.MaxResults {
		allResults = allResults[:query.MaxResults]
	}
	
	return append([]*SearchResult(nil), allResults...), nil // Return copy
}

// aStarSearch implements A* algorithm for deep searches
func (s *OptimizedSearcher) aStarSearch(ctx context.Context, searchCtx *SearchContext) ([]*SearchResult, error) {
	query := searchCtx.Query
	results := s.pool.GetSearchResults()
	defer s.pool.PutSearchResults(results)
	
	// Priority queue for A* (implement as sorted slice for simplicity)
	openSet := s.pool.GetSearchItems()
	defer s.pool.PutSearchItems(openSet)
	
	visited := make(map[uint64]bool)
	gScore := make(map[uint64]float64) // Cost from start
	fScore := make(map[uint64]float64) // Estimated total cost
	
	// Initialize with starting memories
	for _, startID := range query.StartMemoryIDs {
		item := &SearchItem{
			MemoryID:    startID,
			Path:        s.pool.GetPathSteps(),
			TotalWeight: 1.0,
			Depth:       0,
			Priority:    s.heuristicCost(startID, query),
		}
		openSet = append(openSet, item)
		gScore[startID] = 0.0
		fScore[startID] = item.Priority
	}
	
	for len(openSet) > 0 && len(results) < query.MaxResults {
		select {
		case <-ctx.Done():
			return results, ctx.Err()
		default:
		}
		
		// Get item with lowest f-score (best first)
		sort.Slice(openSet, func(i, j int) bool {
			return fScore[openSet[i].MemoryID] < fScore[openSet[j].MemoryID]
		})
		
		current := openSet[0]
		openSet = openSet[1:]
		
		if current.Depth > query.MaxDepth || visited[current.MemoryID] {
			s.pool.PutPathSteps(current.Path)
			continue
		}
		
		visited[current.MemoryID] = true
		searchCtx.mu.Lock()
		searchCtx.NodesExplored++
		searchCtx.mu.Unlock()
		
		// Get memory
		memory := s.cache.GetMemory(current.MemoryID)
		if memory == nil {
			memory = s.graph.GetMemory(current.MemoryID)
			if memory == nil {
				s.pool.PutPathSteps(current.Path)
				continue
			}
			s.cache.SetMemory(memory)
		}
		
		// Add as result
		if current.Depth > 0 {
			result := &SearchResult{
				Memory:      memory,
				Path:        append([]*PathStep(nil), current.Path...), // Copy
				TotalWeight: current.TotalWeight,
				Depth:       current.Depth,
				SearchTime:  time.Since(searchCtx.StartTime),
			}
			results = append(results, result)
		}
		
		// Explore neighbors
		neighbors, associations := s.graph.GetNeighbors(current.MemoryID)
		for i, neighbor := range neighbors {
			if visited[neighbor.ID] {
				continue
			}
			
			assoc := associations[i]
			if !s.passesFilters(assoc, memory, neighbor, query) {
				continue
			}
			
			tentativeG := gScore[current.MemoryID] + (1.0 - assoc.Weight) // Lower weight = higher cost
			
			if existingG, exists := gScore[neighbor.ID]; !exists || tentativeG < existingG {
				// Better path found
				gScore[neighbor.ID] = tentativeG
				h := s.heuristicCost(neighbor.ID, query)
				fScore[neighbor.ID] = tentativeG + h
				
				// Create path step
				pathStep := &PathStep{
					FromMemoryID: current.MemoryID,
					ToMemoryID:   neighbor.ID,
					Association:  assoc,
					StepWeight:   assoc.Weight,
				}
				
				newPath := s.pool.GetPathSteps()
				newPath = append(newPath, current.Path...)
				newPath = append(newPath, pathStep)
				
				item := &SearchItem{
					MemoryID:    neighbor.ID,
					Path:        newPath,
					TotalWeight: current.TotalWeight * assoc.Weight,
					Depth:       current.Depth + 1,
					Priority:    fScore[neighbor.ID],
				}
				
				openSet = append(openSet, item)
			}
		}
		
		s.pool.PutPathSteps(current.Path)
	}
	
	s.sortResultsByWeight(results)
	
	if len(results) > query.MaxResults {
		results = results[:query.MaxResults]
	}
	
	return append([]*SearchResult(nil), results...), nil // Return copy
}

// parallelBreadthFirstSearch performs parallel BFS with edge processing
func (s *OptimizedSearcher) parallelBreadthFirstSearch(ctx context.Context, searchCtx *SearchContext) ([]*SearchResult, error) {
	// This is a fallback - use optimized BFS for single source or parallel multi-source for multiple
	if len(searchCtx.Query.StartMemoryIDs) == 1 {
		return s.optimizedBFS(ctx, searchCtx)
	} else {
		return s.parallelMultiSourceBFS(ctx, searchCtx)
	}
}

// heuristicCost calculates heuristic cost for A* algorithm
func (s *OptimizedSearcher) heuristicCost(memoryID uint64, query *SearchQuery) float64 {
	// Simple heuristic: prefer memories with tags that match query
	memory := s.cache.GetMemory(memoryID)
	if memory == nil {
		memory = s.graph.GetMemory(memoryID)
		if memory == nil {
			return 1.0
		}
	}
	
	if len(query.Tags) == 0 {
		return 0.0
	}
	
	// Count matching tags
	matches := 0
	for _, queryTag := range query.Tags {
		for _, memoryTag := range memory.Tags {
			if queryTag == memoryTag {
				matches++
				break
			}
		}
	}
	
	// Return inverse of match ratio (lower is better for A*)
	matchRatio := float64(matches) / float64(len(query.Tags))
	return 1.0 - matchRatio
}

// addNeighborsOptimized adds neighbors to queue with optimizations
func (s *OptimizedSearcher) addNeighborsOptimized(current *SearchItem, queue *[]*SearchItem, visited map[uint64]bool, query *SearchQuery) {
	memory := s.graph.GetMemory(current.MemoryID)
	if memory == nil {
		return
	}
	
	neighbors, associations := s.graph.GetNeighbors(current.MemoryID)
	
	for i, neighbor := range neighbors {
		if visited[neighbor.ID] {
			continue
		}
		
		assoc := associations[i]
		if !s.passesFilters(assoc, memory, neighbor, query) {
			continue
		}
		
		// Create path step
		pathStep := &PathStep{
			FromMemoryID: current.MemoryID,
			ToMemoryID:   neighbor.ID,
			Association:  assoc,
			StepWeight:   assoc.Weight,
		}
		
		// Create new path (reuse slice from pool)
		newPath := s.pool.GetPathSteps()
		newPath = append(newPath, current.Path...)
		newPath = append(newPath, pathStep)
		
		item := &SearchItem{
			MemoryID:    neighbor.ID,
			Path:        newPath,
			TotalWeight: current.TotalWeight * assoc.Weight,
			Depth:       current.Depth + 1,
			Priority:    current.TotalWeight * assoc.Weight,
		}
		
		*queue = append(*queue, item)
	}
}

// sortResultsByWeight sorts results by weight using efficient algorithm
func (s *OptimizedSearcher) sortResultsByWeight(results []*SearchResult) {
	// Use sort.Slice which is optimized
	sort.Slice(results, func(i, j int) bool {
		return results[i].TotalWeight > results[j].TotalWeight
	})
}

// passesFilters checks if association passes query filters (optimized version)
func (s *OptimizedSearcher) passesFilters(assoc *Association, fromMemory, toMemory *Memory, query *SearchQuery) bool {
	// Early return for weight check (most common filter)
	if assoc.Weight < query.MinWeight {
		return false
	}
	
	// Check association type filter
	if len(query.AssocTypes) > 0 {
		found := false
		assocType := AssociationType(assoc.Type)
		for _, acceptedType := range query.AssocTypes {
			if assocType == acceptedType {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}
	
	// Check tag filter (optimized with early termination)
	if len(query.Tags) > 0 {
		for _, queryTag := range query.Tags {
			for _, memoryTag := range toMemory.Tags {
				if queryTag == memoryTag {
					return true // Found at least one matching tag
				}
			}
		}
		return false // No matching tags found
	}
	
	return true
}

// generateQueryHash creates a hash key for query caching
func (s *OptimizedSearcher) generateQueryHash(query *SearchQuery) string {
	h := fnv.New64a()
	
	// Hash the query parameters
	for _, id := range query.StartMemoryIDs {
		h.Write([]byte(fmt.Sprintf("%d", id)))
	}
	for _, tag := range query.Tags {
		h.Write([]byte(tag))
	}
	for _, assocType := range query.AssocTypes {
		h.Write([]byte(string(assocType)))
	}
	
	h.Write([]byte(fmt.Sprintf("%d%d%f%s", 
		query.MaxDepth, 
		query.MaxResults, 
		query.MinWeight,
		string(query.SearchMode))))
	
	return fmt.Sprintf("%x", h.Sum64())
}

// GetPerformanceStats returns performance statistics
func (s *OptimizedSearcher) GetPerformanceStats() map[string]interface{} {
	cacheStats := s.cache.GetCacheStats()
	
	return map[string]interface{}{
		"cache_hits":        atomic.LoadInt64(&s.cacheHits),
		"cache_misses":      atomic.LoadInt64(&s.cacheMisses),
		"parallelized_ops":  atomic.LoadInt64(&s.paralleilzedOps),
		"memory_cache":      cacheStats,
		"hot_paths_cached":  s.getMapSize(&s.hotPathCache),
		"queries_cached":    s.getMapSize(&s.queryCache),
	}
}

// getMapSize returns the approximate size of a sync.Map
func (s *OptimizedSearcher) getMapSize(m *sync.Map) int {
	count := 0
	m.Range(func(_, _ interface{}) bool {
		count++
		return true
	})
	return count
}