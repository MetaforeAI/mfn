package ffi

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"time"

	"github.com/mfn/layer3_alm/internal/alm"
)

// FFIServer provides inter-layer communication via Unix domain sockets
type FFIServer struct {
	alm      *alm.ALM
	listener net.Listener
	running  bool
	
	// Configuration
	socketPath string
}

// FFIRequest represents an incoming FFI request
type FFIRequest struct {
	Type      string      `json:"type"`
	RequestID string      `json:"request_id"`
	Payload   interface{} `json:"payload"`
}

// FFIResponse represents an outgoing FFI response
type FFIResponse struct {
	Type      string      `json:"type"`
	RequestID string      `json:"request_id"`
	Success   bool        `json:"success"`
	Data      interface{} `json:"data,omitempty"`
	Error     string      `json:"error,omitempty"`
}

// AssociativeSearchRequest represents a search request from other layers
type AssociativeSearchRequest struct {
	StartMemoryIDs []uint64 `json:"start_memory_ids"`
	MaxDepth       int      `json:"max_depth"`
	MaxResults     int      `json:"max_results"`
	MinWeight      float64  `json:"min_weight"`
	TimeoutMS      int      `json:"timeout_ms"`
	SearchMode     string   `json:"search_mode"`
}

// AddMemoryRequest represents a request to add a memory
type AddMemoryRequest struct {
	ID       uint64            `json:"id"`
	Content  string            `json:"content"`
	Tags     []string          `json:"tags,omitempty"`
	Metadata map[string]string `json:"metadata,omitempty"`
}

// AddAssociationRequest represents a request to add an association
type AddAssociationRequest struct {
	FromMemoryID uint64  `json:"from_memory_id"`
	ToMemoryID   uint64  `json:"to_memory_id"`
	Type         string  `json:"type"`
	Weight       float64 `json:"weight"`
	Reason       string  `json:"reason"`
}

// NewFFIServer creates a new FFI server
func NewFFIServer(almInstance *alm.ALM) *FFIServer {
	return &FFIServer{
		alm:        almInstance,
		socketPath: "/tmp/mfn_layer3.sock",
	}
}

// Start starts the FFI server
func (f *FFIServer) Start() error {
	// Remove existing socket file
	if err := f.removeSocket(); err != nil {
		log.Printf("Warning: Failed to remove existing socket: %v", err)
	}
	
	// Create Unix domain socket listener
	listener, err := net.Listen("unix", f.socketPath)
	if err != nil {
		return fmt.Errorf("failed to create Unix socket: %w", err)
	}
	
	f.listener = listener
	f.running = true
	
	log.Printf("FFI server listening on %s", f.socketPath)
	
	// Start accepting connections
	go f.acceptConnections()
	
	return nil
}

// Stop stops the FFI server
func (f *FFIServer) Stop() error {
	f.running = false
	
	if f.listener != nil {
		f.listener.Close()
	}
	
	return f.removeSocket()
}

// removeSocket removes the Unix socket file
func (f *FFIServer) removeSocket() error {
	// TODO: Use os.Remove when available
	return nil
}

// acceptConnections accepts incoming connections
func (f *FFIServer) acceptConnections() {
	for f.running {
		conn, err := f.listener.Accept()
		if err != nil {
			if f.running {
				log.Printf("FFI accept error: %v", err)
			}
			continue
		}
		
		go f.handleConnection(conn)
	}
}

// handleConnection handles a single connection
func (f *FFIServer) handleConnection(conn net.Conn) {
	defer conn.Close()
	
	decoder := json.NewDecoder(conn)
	encoder := json.NewEncoder(conn)
	
	for {
		var request FFIRequest
		if err := decoder.Decode(&request); err != nil {
			// Connection closed or invalid JSON
			break
		}
		
		response := f.processRequest(&request)
		
		if err := encoder.Encode(response); err != nil {
			log.Printf("FFI response error: %v", err)
			break
		}
	}
}

// processRequest processes a single FFI request
func (f *FFIServer) processRequest(request *FFIRequest) *FFIResponse {
	response := &FFIResponse{
		Type:      request.Type,
		RequestID: request.RequestID,
	}
	
	switch request.Type {
	case "associative_search":
		f.handleAssociativeSearch(request, response)
	case "add_memory":
		f.handleAddMemory(request, response)
	case "add_association":
		f.handleAddAssociation(request, response)
	case "get_memory":
		f.handleGetMemory(request, response)
	case "get_stats":
		f.handleGetStats(request, response)
	case "ping":
		f.handlePing(request, response)
	default:
		response.Success = false
		response.Error = fmt.Sprintf("unknown request type: %s", request.Type)
	}
	
	return response
}

// handleAssociativeSearch processes an associative search request
func (f *FFIServer) handleAssociativeSearch(request *FFIRequest, response *FFIResponse) {
	// Parse request payload
	payloadBytes, err := json.Marshal(request.Payload)
	if err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("failed to marshal payload: %v", err)
		return
	}
	
	var searchReq AssociativeSearchRequest
	if err := json.Unmarshal(payloadBytes, &searchReq); err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("invalid search request: %v", err)
		return
	}
	
	// Convert to ALM search query
	query := &alm.SearchQuery{
		StartMemoryIDs: searchReq.StartMemoryIDs,
		MaxDepth:       searchReq.MaxDepth,
		MaxResults:     searchReq.MaxResults,
		MinWeight:      searchReq.MinWeight,
		Timeout:        time.Duration(searchReq.TimeoutMS) * time.Millisecond,
		SearchMode:     alm.SearchMode(searchReq.SearchMode),
	}
	
	// Apply defaults
	if query.MaxDepth == 0 {
		query.MaxDepth = 3
	}
	if query.MaxResults == 0 {
		query.MaxResults = 10
	}
	if query.SearchMode == "" {
		query.SearchMode = alm.SearchModeBestFirst
	}
	
	// Perform search
	ctx, cancel := context.WithTimeout(context.Background(), query.Timeout)
	defer cancel()
	
	results, err := f.alm.SearchAssociative(ctx, query)
	if err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("search failed: %v", err)
		return
	}
	
	response.Success = true
	response.Data = results
}

// handleAddMemory processes an add memory request
func (f *FFIServer) handleAddMemory(request *FFIRequest, response *FFIResponse) {
	// Parse request payload
	payloadBytes, err := json.Marshal(request.Payload)
	if err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("failed to marshal payload: %v", err)
		return
	}
	
	var memoryReq AddMemoryRequest
	if err := json.Unmarshal(payloadBytes, &memoryReq); err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("invalid memory request: %v", err)
		return
	}
	
	// Create memory object
	memory := &alm.Memory{
		ID:       memoryReq.ID,
		Content:  memoryReq.Content,
		Tags:     memoryReq.Tags,
		Metadata: memoryReq.Metadata,
	}
	
	// Add to ALM
	if err := f.alm.AddMemory(memory); err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("failed to add memory: %v", err)
		return
	}
	
	response.Success = true
	response.Data = memory
}

// handleAddAssociation processes an add association request
func (f *FFIServer) handleAddAssociation(request *FFIRequest, response *FFIResponse) {
	// Parse request payload
	payloadBytes, err := json.Marshal(request.Payload)
	if err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("failed to marshal payload: %v", err)
		return
	}
	
	var assocReq AddAssociationRequest
	if err := json.Unmarshal(payloadBytes, &assocReq); err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("invalid association request: %v", err)
		return
	}
	
	// Create association object
	association := &alm.Association{
		FromMemoryID: assocReq.FromMemoryID,
		ToMemoryID:   assocReq.ToMemoryID,
		Type:         assocReq.Type,
		Weight:       assocReq.Weight,
		Reason:       assocReq.Reason,
	}
	
	// Add to ALM
	if err := f.alm.AddAssociation(association); err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("failed to add association: %v", err)
		return
	}
	
	response.Success = true
	response.Data = association
}

// handleGetMemory processes a get memory request
func (f *FFIServer) handleGetMemory(request *FFIRequest, response *FFIResponse) {
	// Parse memory ID from payload
	memoryIDFloat, ok := request.Payload.(float64)
	if !ok {
		response.Success = false
		response.Error = "invalid memory ID format"
		return
	}
	
	memoryID := uint64(memoryIDFloat)
	
	memory, err := f.alm.GetMemory(memoryID)
	if err != nil {
		response.Success = false
		response.Error = fmt.Sprintf("memory not found: %v", err)
		return
	}
	
	response.Success = true
	response.Data = memory
}

// handleGetStats processes a get stats request
func (f *FFIServer) handleGetStats(request *FFIRequest, response *FFIResponse) {
	stats := f.alm.GetGraphStats()
	metrics := f.alm.GetPerformanceMetrics()
	
	data := map[string]interface{}{
		"graph_stats":          stats,
		"performance_metrics":  metrics,
	}
	
	response.Success = true
	response.Data = data
}

// handlePing processes a ping request
func (f *FFIServer) handlePing(request *FFIRequest, response *FFIResponse) {
	response.Success = true
	response.Data = map[string]interface{}{
		"pong":      true,
		"timestamp": time.Now(),
		"layer":     "Layer 3: Associative Link Mesh",
	}
}