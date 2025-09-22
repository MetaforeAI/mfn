package alm

import (
	"container/heap"
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/mfn/layer3_alm/internal/config"
)

// AssociativeSearcher implements concurrent path finding for associative memory
type AssociativeSearcher struct {
	graph  *MemoryGraph
	config *config.ALMConfig
	
	// Worker pools
	searchWorkerPool  *WorkerPool
	pathfindWorkerPool *WorkerPool
}

// WorkerPool manages a pool of worker goroutines
type WorkerPool struct {
	workers   int
	workChan  chan func()
	quit      chan struct{}
	wg        sync.WaitGroup
}

// NewAssociativeSearcher creates a new associative searcher
func NewAssociativeSearcher(graph *MemoryGraph, config *config.ALMConfig) *AssociativeSearcher {
	searcher := &AssociativeSearcher{
		graph:  graph,
		config: config,
	}
	
	// Initialize worker pools
	searcher.searchWorkerPool = NewWorkerPool(config.MaxSearchWorkers)
	searcher.pathfindWorkerPool = NewWorkerPool(config.MaxPathfindWorkers)
	
	return searcher
}

// NewWorkerPool creates a new worker pool
func NewWorkerPool(workers int) *WorkerPool {
	pool := &WorkerPool{
		workers:  workers,
		workChan: make(chan func(), workers*2), // Buffered channel
		quit:     make(chan struct{}),
	}
	
	// Start worker goroutines
	for i := 0; i < workers; i++ {
		pool.wg.Add(1)
		go pool.worker()
	}
	
	return pool
}

// worker runs in a separate goroutine
func (p *WorkerPool) worker() {
	defer p.wg.Done()
	
	for {
		select {
		case work := <-p.workChan:
			work()
		case <-p.quit:
			return
		}
	}
}

// Submit adds work to the pool
func (p *WorkerPool) Submit(work func()) {
	select {
	case p.workChan <- work:
	case <-p.quit:
		// Pool is shutting down
	}
}

// Close shuts down the worker pool
func (p *WorkerPool) Close() {
	close(p.quit)
	p.wg.Wait()
}

// Close shuts down the searcher and its worker pools
func (s *AssociativeSearcher) Close() {
	if s.searchWorkerPool != nil {
		s.searchWorkerPool.Close()
	}
	if s.pathfindWorkerPool != nil {
		s.pathfindWorkerPool.Close()
	}
}

// Search performs associative search using the specified algorithm
func (s *AssociativeSearcher) Search(ctx context.Context, query *SearchQuery) (*SearchResults, error) {
	startTime := time.Now()
	
	searchCtx := &SearchContext{
		Query:         query,
		Visited:       nil, // Not used in concurrent DFS
		Results:       make([]*SearchResult, 0),
		StartTime:     startTime,
		NodesExplored: 0,
	}
	
	// Choose search algorithm based on mode
	var results []*SearchResult
	var err error
	
	switch query.SearchMode {
	case SearchModeDepthFirst:
		results, err = s.depthFirstSearch(ctx, searchCtx)
	case SearchModeBreadthFirst:
		results, err = s.breadthFirstSearch(ctx, searchCtx)
	case SearchModeBestFirst:
		results, err = s.bestFirstSearch(ctx, searchCtx)
	case SearchModeRandom:
		results, err = s.randomSearch(ctx, searchCtx)
	default:
		return nil, fmt.Errorf("unsupported search mode: %s", query.SearchMode)
	}
	
	if err != nil {
		return nil, fmt.Errorf("search failed: %w", err)
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
	
	return searchResults, nil
}

// depthFirstSearch performs depth-first associative search
func (s *AssociativeSearcher) depthFirstSearch(ctx context.Context, searchCtx *SearchContext) ([]*SearchResult, error) {
	results := make([]*SearchResult, 0)
	resultMu := sync.Mutex{}
	
	// Create work for each starting memory
	var wg sync.WaitGroup
	
	for _, startMemoryID := range searchCtx.Query.StartMemoryIDs {
		wg.Add(1)
		
		startMemoryIDCopy := startMemoryID // Capture for goroutine
		s.searchWorkerPool.Submit(func() {
			defer wg.Done()
			
			localResults := s.dfsFromNode(ctx, searchCtx, startMemoryIDCopy, make([]*PathStep, 0), 0, 1.0)
			
			resultMu.Lock()
			results = append(results, localResults...)
			resultMu.Unlock()
		})
	}
	
	wg.Wait()
	
	// Sort and limit results
	return s.processResults(results, searchCtx.Query), nil
}

// dfsFromNode performs DFS from a specific node
func (s *AssociativeSearcher) dfsFromNode(ctx context.Context, searchCtx *SearchContext, memoryID uint64, path []*PathStep, depth int, totalWeight float64) []*SearchResult {
	// Check context cancellation
	select {
	case <-ctx.Done():
		return nil
	default:
	}
	
	// Check depth limit
	if depth > searchCtx.Query.MaxDepth {
		return nil
	}
	
	// For each goroutine, maintain its own visited map to avoid race conditions
	// We create a local copy that includes the path history
	localVisited := make(map[uint64]bool)
	for _, step := range path {
		localVisited[step.FromMemoryID] = true
	}
	
	// Check if already visited in this path (to prevent cycles)
	if localVisited[memoryID] {
		return nil
	}
	
	// Mark current node as visited in this path
	localVisited[memoryID] = true
	
	// Safely increment counter
	searchCtx.mu.Lock()
	searchCtx.NodesExplored++
	searchCtx.mu.Unlock()
	
	memory := s.graph.GetMemory(memoryID)
	if memory == nil {
		return nil
	}
	
	results := make([]*SearchResult, 0)
	
	// Add current memory as a result (except for depth 0)
	if depth > 0 {
		result := &SearchResult{
			Memory:      memory,
			Path:        make([]*PathStep, len(path)),
			TotalWeight: totalWeight,
			Depth:       depth,
			SearchTime:  time.Since(searchCtx.StartTime),
		}
		copy(result.Path, path)
		results = append(results, result)
	}
	
	// Get neighbors and their associations
	neighbors, associations := s.graph.GetNeighbors(memoryID)
	
	// Continue searching from neighbors
	for i, neighbor := range neighbors {
		assoc := associations[i]
		
		// Apply filters
		if !s.passesFilters(assoc, memory, neighbor, searchCtx.Query) {
			continue
		}
		
		// Create new path step
		pathStep := &PathStep{
			FromMemoryID: memoryID,
			ToMemoryID:   neighbor.ID,
			Association:  assoc,
			StepWeight:   assoc.Weight,
		}
		
		// Add to current path
		newPath := make([]*PathStep, len(path)+1)
		copy(newPath, path)
		newPath[len(path)] = pathStep
		
		// Calculate new total weight (multiplicative for path strength)
		newTotalWeight := totalWeight * assoc.Weight
		
		// Recursive search
		childResults := s.dfsFromNode(ctx, searchCtx, neighbor.ID, newPath, depth+1, newTotalWeight)
		results = append(results, childResults...)
	}
	
	return results
}

// breadthFirstSearch performs breadth-first associative search
func (s *AssociativeSearcher) breadthFirstSearch(ctx context.Context, searchCtx *SearchContext) ([]*SearchResult, error) {
	results := make([]*SearchResult, 0)
	queue := make([]*SearchItem, 0)
	visited := make(map[uint64]bool)
	
	// Initialize queue with starting memories
	for _, startMemoryID := range searchCtx.Query.StartMemoryIDs {
		queue = append(queue, &SearchItem{
			MemoryID:    startMemoryID,
			Path:        make([]*PathStep, 0),
			TotalWeight: 1.0,
			Depth:       0,
			Priority:    1.0,
		})
	}
	
	for len(queue) > 0 {
		// Check context cancellation
		select {
		case <-ctx.Done():
			return results, ctx.Err()
		default:
		}
		
		// Dequeue
		current := queue[0]
		queue = queue[1:]
		
		// Check depth limit
		if current.Depth > searchCtx.Query.MaxDepth {
			continue
		}
		
		// Skip if already visited
		if visited[current.MemoryID] {
			continue
		}
		visited[current.MemoryID] = true
		
		searchCtx.NodesExplored++
		
		memory := s.graph.GetMemory(current.MemoryID)
		if memory == nil {
			continue
		}
		
		// Add as result (except depth 0)
		if current.Depth > 0 {
			result := &SearchResult{
				Memory:      memory,
				Path:        make([]*PathStep, len(current.Path)),
				TotalWeight: current.TotalWeight,
				Depth:       current.Depth,
				SearchTime:  time.Since(searchCtx.StartTime),
			}
			copy(result.Path, current.Path)
			results = append(results, result)
		}
		
		// Check if we have enough results
		if len(results) >= searchCtx.Query.MaxResults {
			break
		}
		
		// Add neighbors to queue
		neighbors, associations := s.graph.GetNeighbors(current.MemoryID)
		
		for i, neighbor := range neighbors {
			assoc := associations[i]
			
			// Apply filters
			if !s.passesFilters(assoc, memory, neighbor, searchCtx.Query) {
				continue
			}
			
			// Skip if already visited
			if visited[neighbor.ID] {
				continue
			}
			
			// Create new path step
			pathStep := &PathStep{
				FromMemoryID: current.MemoryID,
				ToMemoryID:   neighbor.ID,
				Association:  assoc,
				StepWeight:   assoc.Weight,
			}
			
			// Add to queue
			newPath := make([]*PathStep, len(current.Path)+1)
			copy(newPath, current.Path)
			newPath[len(current.Path)] = pathStep
			
			queue = append(queue, &SearchItem{
				MemoryID:    neighbor.ID,
				Path:        newPath,
				TotalWeight: current.TotalWeight * assoc.Weight,
				Depth:       current.Depth + 1,
				Priority:    current.TotalWeight * assoc.Weight,
			})
		}
	}
	
	return s.processResults(results, searchCtx.Query), nil
}

// bestFirstSearch performs best-first search using priority queue
func (s *AssociativeSearcher) bestFirstSearch(ctx context.Context, searchCtx *SearchContext) ([]*SearchResult, error) {
	results := make([]*SearchResult, 0)
	pq := &PriorityQueue{}
	heap.Init(pq)
	visited := make(map[uint64]bool)
	
	// Initialize priority queue with starting memories
	for _, startMemoryID := range searchCtx.Query.StartMemoryIDs {
		item := &SearchItem{
			MemoryID:    startMemoryID,
			Path:        make([]*PathStep, 0),
			TotalWeight: 1.0,
			Depth:       0,
			Priority:    1.0,
		}
		heap.Push(pq, item)
	}
	
	for pq.Len() > 0 {
		// Check context cancellation
		select {
		case <-ctx.Done():
			return results, ctx.Err()
		default:
		}
		
		// Pop highest priority item
		current := heap.Pop(pq).(*SearchItem)
		
		// Check depth limit
		if current.Depth > searchCtx.Query.MaxDepth {
			continue
		}
		
		// Skip if already visited
		if visited[current.MemoryID] {
			continue
		}
		visited[current.MemoryID] = true
		
		searchCtx.NodesExplored++
		
		memory := s.graph.GetMemory(current.MemoryID)
		if memory == nil {
			continue
		}
		
		// Add as result (except depth 0)
		if current.Depth > 0 {
			result := &SearchResult{
				Memory:      memory,
				Path:        make([]*PathStep, len(current.Path)),
				TotalWeight: current.TotalWeight,
				Depth:       current.Depth,
				SearchTime:  time.Since(searchCtx.StartTime),
			}
			copy(result.Path, current.Path)
			results = append(results, result)
		}
		
		// Check if we have enough results
		if len(results) >= searchCtx.Query.MaxResults {
			break
		}
		
		// Add neighbors to priority queue
		neighbors, associations := s.graph.GetNeighbors(current.MemoryID)
		
		for i, neighbor := range neighbors {
			assoc := associations[i]
			
			// Apply filters
			if !s.passesFilters(assoc, memory, neighbor, searchCtx.Query) {
				continue
			}
			
			// Skip if already visited
			if visited[neighbor.ID] {
				continue
			}
			
			// Create new path step
			pathStep := &PathStep{
				FromMemoryID: current.MemoryID,
				ToMemoryID:   neighbor.ID,
				Association:  assoc,
				StepWeight:   assoc.Weight,
			}
			
			// Create new search item
			newPath := make([]*PathStep, len(current.Path)+1)
			copy(newPath, current.Path)
			newPath[len(current.Path)] = pathStep
			
			newTotalWeight := current.TotalWeight * assoc.Weight
			
			item := &SearchItem{
				MemoryID:    neighbor.ID,
				Path:        newPath,
				TotalWeight: newTotalWeight,
				Depth:       current.Depth + 1,
				Priority:    newTotalWeight, // Higher weight = higher priority
			}
			
			heap.Push(pq, item)
		}
	}
	
	return s.processResults(results, searchCtx.Query), nil
}

// randomSearch performs random walk search
func (s *AssociativeSearcher) randomSearch(ctx context.Context, searchCtx *SearchContext) ([]*SearchResult, error) {
	// TODO: Implement random search with weighted random selection
	// For now, fall back to breadth-first search
	return s.breadthFirstSearch(ctx, searchCtx)
}

// passesFilters checks if an association passes the query filters
func (s *AssociativeSearcher) passesFilters(assoc *Association, fromMemory, toMemory *Memory, query *SearchQuery) bool {
	// Check minimum weight threshold
	if assoc.Weight < query.MinWeight {
		return false
	}
	
	// Check association type filter
	if len(query.AssocTypes) > 0 {
		found := false
		for _, acceptedType := range query.AssocTypes {
			if AssociationType(assoc.Type) == acceptedType {
				found = true
				break
			}
		}
		if !found {
			return false
		}
	}
	
	// Check tag filter
	if len(query.Tags) > 0 {
		found := false
		for _, queryTag := range query.Tags {
			for _, memoryTag := range toMemory.Tags {
				if queryTag == memoryTag {
					found = true
					break
				}
			}
			if found {
				break
			}
		}
		if !found {
			return false
		}
	}
	
	return true
}

// processResults sorts and limits search results
func (s *AssociativeSearcher) processResults(results []*SearchResult, query *SearchQuery) []*SearchResult {
	// Sort by total weight (descending)
	for i := 0; i < len(results)-1; i++ {
		for j := i + 1; j < len(results); j++ {
			if results[i].TotalWeight < results[j].TotalWeight {
				results[i], results[j] = results[j], results[i]
			}
		}
	}
	
	// Limit results
	if len(results) > query.MaxResults {
		results = results[:query.MaxResults]
	}
	
	return results
}

// PriorityQueue implements a max-heap for SearchItem
type PriorityQueue []*SearchItem

func (pq PriorityQueue) Len() int { return len(pq) }

func (pq PriorityQueue) Less(i, j int) bool {
	// Max-heap: higher priority first
	return pq[i].Priority > pq[j].Priority
}

func (pq PriorityQueue) Swap(i, j int) {
	pq[i], pq[j] = pq[j], pq[i]
}

func (pq *PriorityQueue) Push(x interface{}) {
	*pq = append(*pq, x.(*SearchItem))
}

func (pq *PriorityQueue) Pop() interface{} {
	old := *pq
	n := len(old)
	item := old[n-1]
	*pq = old[0 : n-1]
	return item
}