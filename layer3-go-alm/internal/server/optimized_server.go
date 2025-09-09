package server

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"fmt"
	"hash/fnv"
	"io"
	"net"
	"net/http"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/mfn/layer3_alm/internal/alm"
	"github.com/mfn/layer3_alm/internal/config"
)

// OptimizedServer provides high-performance HTTP API with connection pooling
type OptimizedServer struct {
	alm        *alm.ALM
	config     *config.ServerConfig
	httpServer *http.Server
	listener   net.Listener
	
	// Performance monitoring
	activeConnections int64
	totalRequests     int64
	totalErrors       int64
	idCounter        uint64 // atomic counter for ID generation
	
	// Connection management
	connectionPool *sync.Pool
	
	// Response compression
	gzipWriterPool *sync.Pool
	
	// Request/Response pooling
	responseWriterPool *sync.Pool
}

// gzipResponseWriter wraps http.ResponseWriter with gzip compression
type gzipResponseWriter struct {
	io.Writer
	http.ResponseWriter
	gzipWriter *gzip.Writer
}

func (w *gzipResponseWriter) Write(b []byte) (int, error) {
	return w.Writer.Write(b)
}

func (w *gzipResponseWriter) Close() error {
	if w.gzipWriter != nil {
		return w.gzipWriter.Close()
	}
	return nil
}

// NewOptimizedServer creates a high-performance HTTP server
func NewOptimizedServer(almInstance *alm.ALM, cfg *config.ServerConfig) *OptimizedServer {
	server := &OptimizedServer{
		alm:    almInstance,
		config: cfg,
	}
	
	// Initialize object pools
	server.gzipWriterPool = &sync.Pool{
		New: func() interface{} {
			return gzip.NewWriter(io.Discard)
		},
	}
	
	server.responseWriterPool = &sync.Pool{
		New: func() interface{} {
			return &gzipResponseWriter{}
		},
	}
	
	server.connectionPool = &sync.Pool{
		New: func() interface{} {
			return &http.Client{
				Timeout: 30 * time.Second,
				Transport: &http.Transport{
					MaxIdleConns:        100,
					MaxIdleConnsPerHost: 10,
					IdleConnTimeout:     90 * time.Second,
				},
			}
		},
	}
	
	// Setup optimized HTTP server
	mux := http.NewServeMux()
	server.setupOptimizedRoutes(mux)
	
	server.httpServer = &http.Server{
		Handler:      server.withMiddleware(mux),
		ReadTimeout:  cfg.ReadTimeout,
		WriteTimeout: cfg.WriteTimeout,
		IdleTimeout:  cfg.IdleTimeout,
		
		// Connection optimizations
		ReadHeaderTimeout: 5 * time.Second,
		MaxHeaderBytes:    1 << 20, // 1MB
	}
	
	// Configure TCP optimizations if keep-alive is enabled
	if cfg.EnableKeepAlive {
		server.httpServer.SetKeepAlivesEnabled(true)
	}
	
	return server
}

// setupOptimizedRoutes configures routes with optimizations
func (s *OptimizedServer) setupOptimizedRoutes(mux *http.ServeMux) {
	// Memory operations with caching headers
	mux.HandleFunc("/memories", s.handleMemoriesOptimized)
	mux.HandleFunc("/memories/", s.handleMemoryByIDOptimized)
	
	// Association operations
	mux.HandleFunc("/associations", s.handleAssociationsOptimized)
	
	// High-performance search operations
	mux.HandleFunc("/search", s.handleSearchOptimized)
	mux.HandleFunc("/search/associative", s.handleAssociativeSearchOptimized)
	mux.HandleFunc("/search/batch", s.handleBatchSearchOptimized)
	
	// Graph operations with caching
	mux.HandleFunc("/graph/stats", s.handleGraphStatsOptimized)
	mux.HandleFunc("/graph/neighbors/", s.handleNeighborsOptimized)
	
	// Performance monitoring
	mux.HandleFunc("/performance", s.handlePerformanceOptimized)
	mux.HandleFunc("/health", s.handleHealthOptimized)
	
	// Root endpoint with compression
	mux.HandleFunc("/", s.handleRootOptimized)
}

// withMiddleware applies performance middleware
func (s *OptimizedServer) withMiddleware(handler http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		
		// Track connections
		atomic.AddInt64(&s.activeConnections, 1)
		defer atomic.AddInt64(&s.activeConnections, -1)
		
		// Track requests
		atomic.AddInt64(&s.totalRequests, 1)
		
		// Apply compression if enabled and supported
		if s.config.EnableCompression && s.supportsGzip(r) {
			gzw := s.gzipWriterPool.Get().(*gzip.Writer)
			defer s.gzipWriterPool.Put(gzw)
			
			gzw.Reset(w)
			defer gzw.Close()
			
			wrapper := s.responseWriterPool.Get().(*gzipResponseWriter)
			defer s.responseWriterPool.Put(wrapper)
			
			wrapper.ResponseWriter = w
			wrapper.Writer = gzw
			wrapper.gzipWriter = gzw
			
			w.Header().Set("Content-Encoding", "gzip")
			w = wrapper
		}
		
		// Set performance headers
		w.Header().Set("X-Response-Time", "")
		if s.config.EnableKeepAlive {
			w.Header().Set("Connection", "keep-alive")
			w.Header().Set("Keep-Alive", "timeout=120, max=1000")
		}
		
		// CORS headers for API access
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type")
		
		// Handle preflight requests
		if r.Method == "OPTIONS" {
			w.WriteHeader(http.StatusOK)
			return
		}
		
		// Call the actual handler
		handler.ServeHTTP(w, r)
		
		// Set response time header
		duration := time.Since(start)
		w.Header().Set("X-Response-Time", fmt.Sprintf("%.2fms", float64(duration.Nanoseconds())/1e6))
	})
}

// supportsGzip checks if the client supports gzip compression
func (s *OptimizedServer) supportsGzip(r *http.Request) bool {
	return strings.Contains(r.Header.Get("Accept-Encoding"), "gzip")
}

// Start starts the optimized HTTP server with custom listener
func (s *OptimizedServer) Start() error {
	// Create custom listener with optimizations
	var err error
	s.listener, err = net.Listen("tcp", fmt.Sprintf(":%d", s.config.Port))
	if err != nil {
		return fmt.Errorf("failed to create listener: %w", err)
	}
	
	// Apply TCP optimizations
	if tcpListener, ok := s.listener.(*net.TCPListener); ok {
		s.listener = &tcpKeepAliveListener{
			TCPListener:     tcpListener,
			keepAliveConfig: s.config,
		}
	}
	
	return s.httpServer.Serve(s.listener)
}

// tcpKeepAliveListener implements TCP keep-alive optimizations
type tcpKeepAliveListener struct {
	*net.TCPListener
	keepAliveConfig *config.ServerConfig
}

func (ln *tcpKeepAliveListener) Accept() (net.Conn, error) {
	tc, err := ln.AcceptTCP()
	if err != nil {
		return nil, err
	}
	
	if ln.keepAliveConfig.EnableKeepAlive {
		tc.SetKeepAlive(true)
		tc.SetKeepAlivePeriod(30 * time.Second)
		
		// Additional TCP optimizations
		tc.SetNoDelay(true) // Disable Nagle's algorithm for low latency
	}
	
	return tc, nil
}

// Optimized handler implementations

// handleSearchOptimized provides high-performance search with caching
func (s *OptimizedServer) handleSearchOptimized(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	
	start := time.Now()
	
	var query alm.SearchQuery
	if err := json.NewDecoder(r.Body).Decode(&query); err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, fmt.Sprintf("Invalid JSON: %v", err), http.StatusBadRequest)
		return
	}
	
	// Apply optimized timeout
	if query.Timeout == 0 {
		query.Timeout = 10 * time.Millisecond // Aggressive timeout
	}
	
	// Set cache headers for repeated queries
	cacheKey := fmt.Sprintf("search_%x", s.hashQuery(&query))
	w.Header().Set("ETag", cacheKey)
	
	if r.Header.Get("If-None-Match") == cacheKey {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	
	results, err := s.alm.SearchAssociative(r.Context(), &query)
	if err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, fmt.Sprintf("Search failed: %v", err), http.StatusInternalServerError)
		return
	}
	
	// Set caching headers for successful results
	w.Header().Set("Cache-Control", "max-age=60") // Cache for 1 minute
	w.Header().Set("Content-Type", "application/json")
	
	// Stream JSON response for better performance
	encoder := json.NewEncoder(w)
	encoder.SetIndent("", "")
	if err := encoder.Encode(results); err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		return
	}
	
	// Log performance for monitoring
	duration := time.Since(start)
	if duration > 15*time.Millisecond {
		fmt.Printf("Slow search: %v (duration: %v)\n", query, duration)
	}
}

// handleBatchSearchOptimized handles multiple search requests in a single call
func (s *OptimizedServer) handleBatchSearchOptimized(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	
	var batchQueries []alm.SearchQuery
	if err := json.NewDecoder(r.Body).Decode(&batchQueries); err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, fmt.Sprintf("Invalid JSON: %v", err), http.StatusBadRequest)
		return
	}
	
	// Limit batch size
	if len(batchQueries) > 10 {
		http.Error(w, "Batch size too large (max 10)", http.StatusBadRequest)
		return
	}
	
	results := make([]*alm.SearchResults, len(batchQueries))
	errors := make([]string, len(batchQueries))
	
	// Process queries in parallel
	var wg sync.WaitGroup
	for i, query := range batchQueries {
		wg.Add(1)
		go func(idx int, q alm.SearchQuery) {
			defer wg.Done()
			
			if q.Timeout == 0 {
				q.Timeout = 10 * time.Millisecond
			}
			
			result, err := s.alm.SearchAssociative(r.Context(), &q)
			if err != nil {
				errors[idx] = err.Error()
			} else {
				results[idx] = result
			}
		}(i, query)
	}
	
	wg.Wait()
	
	response := map[string]interface{}{
		"results": results,
		"errors":  errors,
		"count":   len(batchQueries),
	}
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

// handleAssociativeSearchOptimized provides optimized associative search
func (s *OptimizedServer) handleAssociativeSearchOptimized(w http.ResponseWriter, r *http.Request) {
	s.handleSearchOptimized(w, r) // Delegate to optimized search
}

// handleMemoriesOptimized handles memory operations with caching
func (s *OptimizedServer) handleMemoriesOptimized(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		s.handleAddMemoryOptimized(w, r)
	case http.MethodGet:
		s.handleListMemoriesOptimized(w, r)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleAddMemoryOptimized adds memory with validation and caching
func (s *OptimizedServer) handleAddMemoryOptimized(w http.ResponseWriter, r *http.Request) {
	var memory alm.Memory
	if err := json.NewDecoder(r.Body).Decode(&memory); err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, fmt.Sprintf("Invalid JSON: %v", err), http.StatusBadRequest)
		return
	}
	
	// Auto-generate ID if not provided
	if memory.ID == 0 {
		memory.ID = s.generateMemoryID()
	}
	
	// Validate memory before adding
	if len(memory.Content) == 0 {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, "Memory content is required", http.StatusBadRequest)
		return
	}
	
	if err := s.alm.AddMemory(&memory); err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, fmt.Sprintf("Failed to add memory: %v", err), http.StatusInternalServerError)
		return
	}
	
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Location", fmt.Sprintf("/memories/%d", memory.ID))
	w.WriteHeader(http.StatusCreated)
	
	json.NewEncoder(w).Encode(map[string]interface{}{
		"success": true,
		"memory":  memory,
	})
}

// handleListMemoriesOptimized lists memories with pagination
func (s *OptimizedServer) handleListMemoriesOptimized(w http.ResponseWriter, r *http.Request) {
	// Add pagination support
	limit := 100 // default
	if l := r.URL.Query().Get("limit"); l != "" {
		if parsed, err := strconv.Atoi(l); err == nil && parsed > 0 && parsed <= 1000 {
			limit = parsed
		}
	}
	
	memories := s.alm.GetGraph().GetAllMemories()
	
	// Apply limit
	if len(memories) > limit {
		memories = memories[:limit]
	}
	
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "max-age=30") // Cache for 30 seconds
	
	json.NewEncoder(w).Encode(map[string]interface{}{
		"memories": memories,
		"count":    len(memories),
		"limit":    limit,
	})
}

// handleMemoryByIDOptimized retrieves memory with caching
func (s *OptimizedServer) handleMemoryByIDOptimized(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	
	// Extract memory ID from path
	path := r.URL.Path[len("/memories/"):]
	memoryID, err := strconv.ParseUint(path, 10, 64)
	if err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, "Invalid memory ID", http.StatusBadRequest)
		return
	}
	
	// Set ETag for caching
	etag := fmt.Sprintf("memory_%d", memoryID)
	w.Header().Set("ETag", etag)
	
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	
	memory, err := s.alm.GetMemory(memoryID)
	if err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, err.Error(), http.StatusNotFound)
		return
	}
	
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "max-age=300") // Cache for 5 minutes
	json.NewEncoder(w).Encode(memory)
}

// handleAssociationsOptimized handles associations with validation
func (s *OptimizedServer) handleAssociationsOptimized(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	
	var association alm.Association
	if err := json.NewDecoder(r.Body).Decode(&association); err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, fmt.Sprintf("Invalid JSON: %v", err), http.StatusBadRequest)
		return
	}
	
	// Validate association
	if association.FromMemoryID == 0 || association.ToMemoryID == 0 {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, "Invalid association: FromMemoryID and ToMemoryID required", http.StatusBadRequest)
		return
	}
	
	if association.Weight < 0 || association.Weight > 1 {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, "Invalid association: Weight must be between 0 and 1", http.StatusBadRequest)
		return
	}
	
	if err := s.alm.AddAssociation(&association); err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, fmt.Sprintf("Failed to add association: %v", err), http.StatusInternalServerError)
		return
	}
	
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusCreated)
	json.NewEncoder(w).Encode(map[string]interface{}{
		"success":     true,
		"association": association,
	})
}

// handleGraphStatsOptimized returns cached graph statistics
func (s *OptimizedServer) handleGraphStatsOptimized(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	
	stats := s.alm.GetGraphStats()
	
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "max-age=10") // Cache for 10 seconds
	json.NewEncoder(w).Encode(stats)
}

// handleNeighborsOptimized returns neighbors with caching
func (s *OptimizedServer) handleNeighborsOptimized(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	
	// Extract memory ID from path
	path := r.URL.Path[len("/graph/neighbors/"):]
	memoryID, err := strconv.ParseUint(path, 10, 64)
	if err != nil {
		atomic.AddInt64(&s.totalErrors, 1)
		http.Error(w, "Invalid memory ID", http.StatusBadRequest)
		return
	}
	
	neighbors, associations := s.alm.GetGraph().GetNeighbors(memoryID)
	
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "max-age=60") // Cache for 1 minute
	json.NewEncoder(w).Encode(map[string]interface{}{
		"memory_id":    memoryID,
		"neighbors":    neighbors,
		"associations": associations,
		"count":        len(neighbors),
	})
}

// handlePerformanceOptimized returns enhanced performance metrics
func (s *OptimizedServer) handlePerformanceOptimized(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	
	metrics := s.alm.GetPerformanceMetrics()
	
	// Add server-specific metrics
	serverMetrics := map[string]interface{}{
		"active_connections": atomic.LoadInt64(&s.activeConnections),
		"total_requests":     atomic.LoadInt64(&s.totalRequests),
		"total_errors":       atomic.LoadInt64(&s.totalErrors),
		"goroutines":         runtime.NumGoroutine(),
		"memory_usage":       s.getMemoryUsage(),
	}
	
	response := map[string]interface{}{
		"alm_metrics":    metrics,
		"server_metrics": serverMetrics,
		"timestamp":      time.Now(),
	}
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(response)
}

// handleHealthOptimized provides detailed health check
func (s *OptimizedServer) handleHealthOptimized(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}
	
	stats := s.alm.GetGraphStats()
	metrics := s.alm.GetPerformanceMetrics()
	
	// Determine health status
	status := "healthy"
	if atomic.LoadInt64(&s.totalErrors) > atomic.LoadInt64(&s.totalRequests)/10 {
		status = "degraded" // Error rate > 10%
	}
	
	health := map[string]interface{}{
		"status":               status,
		"timestamp":            time.Now(),
		"memories_count":       stats.TotalMemories,
		"associations_count":   stats.TotalAssociations,
		"total_searches":       metrics.TotalSearches,
		"average_search_ms":    float64(metrics.AverageSearchTime.Nanoseconds()) / 1e6,
		"active_connections":   atomic.LoadInt64(&s.activeConnections),
		"error_rate":          float64(atomic.LoadInt64(&s.totalErrors)) / float64(atomic.LoadInt64(&s.totalRequests)+1),
		"uptime_seconds":      time.Since(time.Now()).Seconds(), // This would be calculated from start time
	}
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(health)
}

// handleRootOptimized provides compressed API information
func (s *OptimizedServer) handleRootOptimized(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path != "/" {
		http.NotFound(w, r)
		return
	}
	
	info := map[string]interface{}{
		"service":     "Memory Flow Network - Layer 3: Associative Link Mesh",
		"version":     "1.1.0-optimized",
		"description": "High-performance graph-based associative memory with concurrent path finding",
		"optimizations": []string{
			"Object pooling and memory reuse",
			"Multi-level caching (memory, query, path)",
			"Parallel search algorithms (BFS, A*)",
			"HTTP keep-alive and compression",
			"Connection pooling",
			"Optimized JSON streaming",
		},
		"endpoints": map[string]string{
			"POST /memories":             "Add a new memory",
			"GET /memories":              "List memories (with pagination)",
			"GET /memories/{id}":         "Get specific memory (cached)",
			"POST /associations":         "Add a new association",
			"POST /search/associative":   "Perform associative search (cached)",
			"POST /search/batch":         "Batch search operations",
			"GET /graph/stats":           "Get graph statistics (cached)",
			"GET /graph/neighbors/{id}":  "Get memory neighbors (cached)",
			"GET /performance":           "Get performance metrics",
			"GET /health":                "Health check with detailed status",
		},
		"performance_targets": map[string]string{
			"search_time":      "<15ms average",
			"memory_lookup":    "<1ms average",
			"throughput":       ">1000 req/sec",
			"error_rate":       "<1%",
		},
	}
	
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "max-age=3600") // Cache for 1 hour
	json.NewEncoder(w).Encode(info)
}

// Shutdown gracefully shuts down the optimized server
func (s *OptimizedServer) Shutdown(ctx context.Context) error {
	if s.listener != nil {
		s.listener.Close()
	}
	return s.httpServer.Shutdown(ctx)
}

// Helper functions

// hashQuery creates a simple hash of the query for caching
func (s *OptimizedServer) hashQuery(query *alm.SearchQuery) uint32 {
	var hash uint32 = 2166136261
	
	for _, id := range query.StartMemoryIDs {
		hash ^= uint32(id)
		hash *= 16777619
	}
	
	for _, tag := range query.Tags {
		for _, b := range []byte(tag) {
			hash ^= uint32(b)
			hash *= 16777619
		}
	}
	
	return hash
}

// getMemoryUsage returns current memory usage statistics
func (s *OptimizedServer) getMemoryUsage() map[string]interface{} {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)
	
	return map[string]interface{}{
		"alloc_mb":      float64(m.Alloc) / 1024 / 1024,
		"total_alloc_mb": float64(m.TotalAlloc) / 1024 / 1024,
		"sys_mb":        float64(m.Sys) / 1024 / 1024,
		"gc_cycles":     m.NumGC,
		"goroutines":    runtime.NumGoroutine(),
	}
}

// generateMemoryID generates a fast, unique memory ID using hash + counter
func (s *OptimizedServer) generateMemoryID() uint64 {
	// Fast ID generation using FNV hash of timestamp + atomic counter
	counter := atomic.AddUint64(&s.idCounter, 1)
	now := uint64(time.Now().UnixNano())
	
	// Combine timestamp and counter for uniqueness
	h := fnv.New64a()
	
	// Write timestamp bytes
	for i := 0; i < 8; i++ {
		h.Write([]byte{byte(now >> (i * 8))})
	}
	
	// Write counter bytes  
	for i := 0; i < 8; i++ {
		h.Write([]byte{byte(counter >> (i * 8))})
	}
	
	id := h.Sum64()
	
	// Ensure ID is not zero (reserved value)
	if id == 0 {
		id = 1
	}
	
	return id
}