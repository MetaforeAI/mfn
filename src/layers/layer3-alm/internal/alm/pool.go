package alm

import (
	"sync"
	"time"
)

// ObjectPool provides reusable memory pools for frequently allocated objects
type ObjectPool struct {
	searchResults    *sync.Pool
	pathSteps        *sync.Pool  
	searchItems      *sync.Pool
	searchContexts   *sync.Pool
	stringSlices     *sync.Pool
	uint64Slices     *sync.Pool
}

// NewObjectPool creates a new object pool with optimized allocation patterns
func NewObjectPool() *ObjectPool {
	pool := &ObjectPool{
		searchResults: &sync.Pool{
			New: func() interface{} {
				return make([]*SearchResult, 0, 32) // Pre-allocate capacity
			},
		},
		pathSteps: &sync.Pool{
			New: func() interface{} {
				return make([]*PathStep, 0, 16) // Pre-allocate capacity
			},
		},
		searchItems: &sync.Pool{
			New: func() interface{} {
				return make([]*SearchItem, 0, 64) // Pre-allocate capacity
			},
		},
		searchContexts: &sync.Pool{
			New: func() interface{} {
				return &SearchContext{
					Results: make([]*SearchResult, 0, 32),
				}
			},
		},
		stringSlices: &sync.Pool{
			New: func() interface{} {
				return make([]string, 0, 16)
			},
		},
		uint64Slices: &sync.Pool{
			New: func() interface{} {
				return make([]uint64, 0, 64)
			},
		},
	}
	return pool
}

// GetSearchResults returns a reusable SearchResult slice
func (p *ObjectPool) GetSearchResults() []*SearchResult {
	return p.searchResults.Get().([]*SearchResult)[:0]
}

// PutSearchResults returns a SearchResult slice to the pool
func (p *ObjectPool) PutSearchResults(results []*SearchResult) {
	if cap(results) < 1024 { // Don't pool very large slices
		p.searchResults.Put(results)
	}
}

// GetPathSteps returns a reusable PathStep slice
func (p *ObjectPool) GetPathSteps() []*PathStep {
	return p.pathSteps.Get().([]*PathStep)[:0]
}

// PutPathSteps returns a PathStep slice to the pool
func (p *ObjectPool) PutPathSteps(steps []*PathStep) {
	if cap(steps) < 256 { // Don't pool very large slices
		p.pathSteps.Put(steps)
	}
}

// GetSearchItems returns a reusable SearchItem slice
func (p *ObjectPool) GetSearchItems() []*SearchItem {
	return p.searchItems.Get().([]*SearchItem)[:0]
}

// PutSearchItems returns a SearchItem slice to the pool
func (p *ObjectPool) PutSearchItems(items []*SearchItem) {
	if cap(items) < 512 { // Don't pool very large slices
		p.searchItems.Put(items)
	}
}

// GetSearchContext returns a reusable SearchContext
func (p *ObjectPool) GetSearchContext() *SearchContext {
	ctx := p.searchContexts.Get().(*SearchContext)
	ctx.NodesExplored = 0
	ctx.Results = ctx.Results[:0]
	ctx.StartTime = time.Now()
	return ctx
}

// PutSearchContext returns a SearchContext to the pool
func (p *ObjectPool) PutSearchContext(ctx *SearchContext) {
	ctx.Query = nil
	ctx.Visited = nil
	p.searchContexts.Put(ctx)
}

// GetStringSlice returns a reusable string slice
func (p *ObjectPool) GetStringSlice() []string {
	return p.stringSlices.Get().([]string)[:0]
}

// PutStringSlice returns a string slice to the pool
func (p *ObjectPool) PutStringSlice(slice []string) {
	if cap(slice) < 128 { // Don't pool very large slices
		p.stringSlices.Put(slice)
	}
}

// GetUint64Slice returns a reusable uint64 slice
func (p *ObjectPool) GetUint64Slice() []uint64 {
	return p.uint64Slices.Get().([]uint64)[:0]
}

// PutUint64Slice returns a uint64 slice to the pool
func (p *ObjectPool) PutUint64Slice(slice []uint64) {
	if cap(slice) < 256 { // Don't pool very large slices
		p.uint64Slices.Put(slice)
	}
}

// MemoryCache provides LRU caching for frequently accessed memories and search results
type MemoryCache struct {
	memoryCache   *LRUCache
	searchCache   *LRUCache
	pathCache     *LRUCache
	mu            sync.RWMutex
	hitCount      int64
	missCount     int64
}

// NewMemoryCache creates a new memory cache with specified capacities
func NewMemoryCache(memoryCapacity, searchCapacity, pathCapacity int) *MemoryCache {
	return &MemoryCache{
		memoryCache: NewLRUCache(memoryCapacity),
		searchCache: NewLRUCache(searchCapacity),  
		pathCache:   NewLRUCache(pathCapacity),
	}
}

// GetMemory retrieves a cached memory
func (mc *MemoryCache) GetMemory(id uint64) *Memory {
	mc.mu.RLock()
	defer mc.mu.RUnlock()
	
	if item, found := mc.memoryCache.Get(id); found {
		mc.hitCount++
		return item.(*Memory)
	}
	mc.missCount++
	return nil
}

// SetMemory caches a memory
func (mc *MemoryCache) SetMemory(memory *Memory) {
	mc.mu.Lock()
	defer mc.mu.Unlock()
	
	// Create a copy to avoid shared memory issues
	cachedMemory := &Memory{
		ID:           memory.ID,
		Content:      memory.Content,
		Tags:         append([]string(nil), memory.Tags...), // Copy slice
		CreatedAt:    memory.CreatedAt,
		LastAccessed: memory.LastAccessed,
		AccessCount:  memory.AccessCount,
	}
	if memory.Metadata != nil {
		cachedMemory.Metadata = make(map[string]string)
		for k, v := range memory.Metadata {
			cachedMemory.Metadata[k] = v
		}
	}
	
	mc.memoryCache.Set(memory.ID, cachedMemory)
}

// GetSearchResults retrieves cached search results
func (mc *MemoryCache) GetSearchResults(queryHash string) *SearchResults {
	mc.mu.RLock()
	defer mc.mu.RUnlock()
	
	if item, found := mc.searchCache.Get(queryHash); found {
		mc.hitCount++
		return item.(*SearchResults)
	}
	mc.missCount++
	return nil
}

// SetSearchResults caches search results
func (mc *MemoryCache) SetSearchResults(queryHash string, results *SearchResults) {
	mc.mu.Lock()
	defer mc.mu.Unlock()
	
	// Only cache if results are not too large
	if len(results.Results) <= 100 {
		mc.searchCache.Set(queryHash, results)
	}
}

// GetCacheStats returns cache performance statistics
func (mc *MemoryCache) GetCacheStats() map[string]interface{} {
	mc.mu.RLock()
	defer mc.mu.RUnlock()
	
	total := mc.hitCount + mc.missCount
	hitRate := float64(0)
	if total > 0 {
		hitRate = float64(mc.hitCount) / float64(total)
	}
	
	return map[string]interface{}{
		"hit_count":    mc.hitCount,
		"miss_count":   mc.missCount,
		"hit_rate":     hitRate,
		"memory_size":  mc.memoryCache.Size(),
		"search_size":  mc.searchCache.Size(),
		"path_size":    mc.pathCache.Size(),
	}
}

// LRUCache implements a simple LRU cache
type LRUCache struct {
	capacity int
	items    map[interface{}]*cacheItem
	head     *cacheItem
	tail     *cacheItem
}

type cacheItem struct {
	key   interface{}
	value interface{}
	prev  *cacheItem
	next  *cacheItem
	time  time.Time
}

// NewLRUCache creates a new LRU cache
func NewLRUCache(capacity int) *LRUCache {
	cache := &LRUCache{
		capacity: capacity,
		items:    make(map[interface{}]*cacheItem),
	}
	
	// Create sentinel nodes
	cache.head = &cacheItem{}
	cache.tail = &cacheItem{}
	cache.head.next = cache.tail
	cache.tail.prev = cache.head
	
	return cache
}

// Get retrieves an item from the cache
func (c *LRUCache) Get(key interface{}) (interface{}, bool) {
	if item, exists := c.items[key]; exists {
		// Move to front
		c.moveToFront(item)
		item.time = time.Now()
		return item.value, true
	}
	return nil, false
}

// Set adds an item to the cache
func (c *LRUCache) Set(key interface{}, value interface{}) {
	if item, exists := c.items[key]; exists {
		// Update existing item
		item.value = value
		item.time = time.Now()
		c.moveToFront(item)
		return
	}
	
	// Add new item
	item := &cacheItem{
		key:   key,
		value: value,
		time:  time.Now(),
	}
	
	c.items[key] = item
	c.addToFront(item)
	
	// Remove excess items
	if len(c.items) > c.capacity {
		c.removeTail()
	}
}

// Size returns the current cache size
func (c *LRUCache) Size() int {
	return len(c.items)
}

// moveToFront moves an item to the front of the list
func (c *LRUCache) moveToFront(item *cacheItem) {
	c.removeFromList(item)
	c.addToFront(item)
}

// addToFront adds an item to the front of the list
func (c *LRUCache) addToFront(item *cacheItem) {
	item.prev = c.head
	item.next = c.head.next
	c.head.next.prev = item
	c.head.next = item
}

// removeFromList removes an item from the list
func (c *LRUCache) removeFromList(item *cacheItem) {
	item.prev.next = item.next
	item.next.prev = item.prev
}

// removeTail removes the tail item
func (c *LRUCache) removeTail() {
	if c.tail.prev != c.head {
		item := c.tail.prev
		delete(c.items, item.key)
		c.removeFromList(item)
	}
}