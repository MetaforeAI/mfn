package server

import (
	"context"
	"encoding/json"
	"fmt"
	"hash/fnv"
	"net/http"
	"strconv"
	"sync/atomic"
	"time"

	"github.com/mfn/layer3_alm/internal/alm"
	"github.com/mfn/layer3_alm/internal/config"
)

// Server provides HTTP API for the ALM
type Server struct {
	alm        *alm.ALM
	config     *config.ServerConfig
	httpServer *http.Server
	idCounter  uint64 // atomic counter for ID generation
}

// NewServer creates a new HTTP server
func NewServer(almInstance *alm.ALM, cfg *config.ServerConfig) *Server {
	server := &Server{
		alm:    almInstance,
		config: cfg,
	}

	mux := http.NewServeMux()
	server.setupRoutes(mux)

	server.httpServer = &http.Server{
		Addr:         fmt.Sprintf(":%d", cfg.Port),
		Handler:      mux,
		ReadTimeout:  cfg.ReadTimeout,
		WriteTimeout: cfg.WriteTimeout,
		IdleTimeout:  cfg.IdleTimeout,
	}

	return server
}

// setupRoutes configures HTTP routes
func (s *Server) setupRoutes(mux *http.ServeMux) {
	// Memory operations
	mux.HandleFunc("/memories", s.handleMemories)
	mux.HandleFunc("/memories/", s.handleMemoryByID)
	
	// Association operations
	mux.HandleFunc("/associations", s.handleAssociations)
	
	// Search operations
	mux.HandleFunc("/search", s.handleSearch)
	mux.HandleFunc("/search/associative", s.handleAssociativeSearch)
	
	// Graph operations
	mux.HandleFunc("/graph/stats", s.handleGraphStats)
	mux.HandleFunc("/graph/neighbors/", s.handleNeighbors)
	mux.HandleFunc("/graph/components", s.handleConnectedComponents)
	
	// Performance and monitoring
	mux.HandleFunc("/performance", s.handlePerformance)
	mux.HandleFunc("/health", s.handleHealth)
	
	// Root endpoint
	mux.HandleFunc("/", s.handleRoot)
}

// Start starts the HTTP server
func (s *Server) Start() error {
	return s.httpServer.ListenAndServe()
}

// Shutdown gracefully shuts down the server
func (s *Server) Shutdown(ctx context.Context) error {
	return s.httpServer.Shutdown(ctx)
}

// handleMemories handles memory collection operations
func (s *Server) handleMemories(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		s.handleAddMemory(w, r)
	case http.MethodGet:
		s.handleListMemories(w, r)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleAddMemory adds a new memory
func (s *Server) handleAddMemory(w http.ResponseWriter, r *http.Request) {
	var memory alm.Memory
	if err := json.NewDecoder(r.Body).Decode(&memory); err != nil {
		http.Error(w, fmt.Sprintf("Invalid JSON: %v", err), http.StatusBadRequest)
		return
	}

	// Auto-generate ID if not provided
	if memory.ID == 0 {
		memory.ID = s.generateMemoryID()
	}

	// Validate required fields
	if len(memory.Content) == 0 {
		http.Error(w, "Memory content is required", http.StatusBadRequest)
		return
	}

	if err := s.alm.AddMemory(&memory); err != nil {
		http.Error(w, fmt.Sprintf("Failed to add memory: %v", err), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"success": true,
		"memory":  memory,
	})
}

// handleListMemories lists all memories (be careful with large datasets)
func (s *Server) handleListMemories(w http.ResponseWriter, r *http.Request) {
	// TODO: Add pagination for production use
	memories := s.alm.GetGraph().GetAllMemories()
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"memories": memories,
		"count":    len(memories),
	})
}

// handleMemoryByID handles single memory operations
func (s *Server) handleMemoryByID(w http.ResponseWriter, r *http.Request) {
	// Extract memory ID from path
	path := r.URL.Path[len("/memories/"):]
	memoryID, err := strconv.ParseUint(path, 10, 64)
	if err != nil {
		http.Error(w, "Invalid memory ID", http.StatusBadRequest)
		return
	}

	switch r.Method {
	case http.MethodGet:
		memory, err := s.alm.GetMemory(memoryID)
		if err != nil {
			http.Error(w, err.Error(), http.StatusNotFound)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(memory)

	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleAssociations handles association collection operations
func (s *Server) handleAssociations(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodPost:
		s.handleAddAssociation(w, r)
	default:
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
	}
}

// handleAddAssociation adds a new association
func (s *Server) handleAddAssociation(w http.ResponseWriter, r *http.Request) {
	var association alm.Association
	if err := json.NewDecoder(r.Body).Decode(&association); err != nil {
		http.Error(w, fmt.Sprintf("Invalid JSON: %v", err), http.StatusBadRequest)
		return
	}

	if err := s.alm.AddAssociation(&association); err != nil {
		http.Error(w, fmt.Sprintf("Failed to add association: %v", err), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"success":     true,
		"association": association,
	})
}

// handleAssociativeSearch handles associative search requests
func (s *Server) handleAssociativeSearch(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var query alm.SearchQuery
	if err := json.NewDecoder(r.Body).Decode(&query); err != nil {
		http.Error(w, fmt.Sprintf("Invalid JSON: %v", err), http.StatusBadRequest)
		return
	}

	// Apply default search timeout if not specified
	if query.Timeout == 0 {
		query.Timeout = 10 * time.Second
	}

	results, err := s.alm.SearchAssociative(r.Context(), &query)
	if err != nil {
		http.Error(w, fmt.Sprintf("Search failed: %v", err), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(results)
}

// handleSearch handles general search operations
func (s *Server) handleSearch(w http.ResponseWriter, r *http.Request) {
	// For now, redirect to associative search
	s.handleAssociativeSearch(w, r)
}

// handleGraphStats returns graph statistics
func (s *Server) handleGraphStats(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	stats := s.alm.GetGraphStats()
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(stats)
}

// handleNeighbors returns neighbors of a specific memory
func (s *Server) handleNeighbors(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	// Extract memory ID from path
	path := r.URL.Path[len("/graph/neighbors/"):]
	memoryID, err := strconv.ParseUint(path, 10, 64)
	if err != nil {
		http.Error(w, "Invalid memory ID", http.StatusBadRequest)
		return
	}

	neighbors, associations := s.alm.GetGraph().GetNeighbors(memoryID)
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"memory_id":    memoryID,
		"neighbors":    neighbors,
		"associations": associations,
		"count":        len(neighbors),
	})
}

// handleConnectedComponents returns connected components
func (s *Server) handleConnectedComponents(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	components := s.alm.GetGraph().GetConnectedComponents()
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"components": components,
		"count":      len(components),
	})
}

// handlePerformance returns performance metrics
func (s *Server) handlePerformance(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	metrics := s.alm.GetPerformanceMetrics()
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(metrics)
}

// handleHealth returns health status
func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
		return
	}

	stats := s.alm.GetGraphStats()
	metrics := s.alm.GetPerformanceMetrics()
	
	health := map[string]interface{}{
		"status":             "healthy",
		"timestamp":          time.Now(),
		"memories_count":     stats.TotalMemories,
		"associations_count": stats.TotalAssociations,
		"total_searches":     metrics.TotalSearches,
		"average_search_ms":  float64(metrics.AverageSearchTime.Nanoseconds()) / 1e6,
	}
	
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(health)
}

// handleRoot provides API information
func (s *Server) handleRoot(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path != "/" {
		http.NotFound(w, r)
		return
	}

	info := map[string]interface{}{
		"service":     "Memory Flow Network - Layer 3: Associative Link Mesh",
		"version":     "1.0.0",
		"description": "Graph-based associative memory with concurrent path finding",
		"endpoints": map[string]string{
			"POST /memories":             "Add a new memory",
			"GET /memories":              "List all memories",
			"GET /memories/{id}":         "Get specific memory",
			"POST /associations":         "Add a new association",
			"POST /search/associative":   "Perform associative search",
			"GET /graph/stats":           "Get graph statistics",
			"GET /graph/neighbors/{id}":  "Get memory neighbors",
			"GET /graph/components":      "Get connected components",
			"GET /performance":           "Get performance metrics",
			"GET /health":                "Health check",
		},
		"search_modes": []string{
			"depth_first",
			"breadth_first", 
			"best_first",
			"random",
		},
		"association_types": []string{
			"semantic",
			"temporal",
			"causal",
			"spatial",
			"conceptual",
			"hierarchical",
			"functional",
			"domain",
			"cognitive",
		},
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(info)
}

// generateMemoryID generates a fast, unique memory ID using hash + counter
func (s *Server) generateMemoryID() uint64 {
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