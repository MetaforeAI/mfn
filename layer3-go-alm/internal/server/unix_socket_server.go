// Unix Socket Server for Layer 3 ALM
// Provides high-performance Unix domain socket interface for associative queries

package server

import (
	"bufio"
	"context"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"os"
	"sync"
	"sync/atomic"
	"time"

	"github.com/mfn/layer3_alm/internal/alm"
	"github.com/google/uuid"
)

const (
	DefaultSocketPath = "/tmp/mfn_test_layer3.sock"
	MaxConnections    = 200 // Increased for high-concurrency stress tests
	BufferSize        = 8192
	RequestTimeout    = 30 * time.Second
)

// UnixSocketServer handles Unix domain socket connections for Layer 3
type UnixSocketServer struct {
	poolManager *alm.PoolManager
	socketPath  string
	listener    net.Listener

	// Connection management
	connections sync.Map
	connCount   int32
	maxConns    int32

	// Shutdown coordination
	ctx        context.Context
	cancel     context.CancelFunc
	wg         sync.WaitGroup

	// Statistics
	totalRequests  uint64
	totalResponses uint64
	totalErrors    uint64
	startTime      time.Time
}

// SocketRequest represents an incoming request via Unix socket
type SocketRequest struct {
	Type          string                 `json:"type"`
	RequestID     string                 `json:"request_id"`
	PoolID        string                 `json:"pool_id,omitempty"`
	Query         string                 `json:"query,omitempty"`
	Content       string                 `json:"content,omitempty"`
	Limit         int                    `json:"limit,omitempty"`
	MinConfidence float32                `json:"min_confidence,omitempty"`
	Metadata      map[string]interface{} `json:"metadata,omitempty"`
}

// HealthCheckResponse represents a health check response
type HealthCheckResponse struct {
	Status        string                 `json:"status"`
	Layer         string                 `json:"layer"`
	Timestamp     int64                  `json:"timestamp"`
	UptimeSeconds int64                  `json:"uptime_seconds"`
	Metrics       map[string]interface{} `json:"metrics"`
}

// SocketResponse represents a response sent via Unix socket
type SocketResponse struct {
	Type      string                 `json:"type"`
	RequestID string                 `json:"request_id"`
	Success   bool                   `json:"success"`
	Results   []SearchResult         `json:"results,omitempty"`
	Error     string                 `json:"error,omitempty"`
	Confidence float32               `json:"confidence,omitempty"`
	ProcessingTimeMs float32          `json:"processing_time_ms,omitempty"`
	Metadata  map[string]interface{} `json:"metadata,omitempty"`
}

// SearchResult represents a single search result
type SearchResult struct {
	ID         uint64                 `json:"id"`
	Content    string                 `json:"content"`
	Score      float32                `json:"score"`
	Distance   int                    `json:"distance,omitempty"`
	Metadata   map[string]interface{} `json:"metadata,omitempty"`
}

// NewUnixSocketServer creates a new Unix socket server
func NewUnixSocketServer(poolManager *alm.PoolManager, socketPath string) *UnixSocketServer {
	if socketPath == "" {
		socketPath = DefaultSocketPath
	}

	ctx, cancel := context.WithCancel(context.Background())

	return &UnixSocketServer{
		poolManager: poolManager,
		socketPath:  socketPath,
		maxConns:    MaxConnections,
		ctx:         ctx,
		cancel:      cancel,
		startTime:   time.Now(),
	}
}

// Start begins listening on the Unix socket
func (s *UnixSocketServer) Start() error {
	// Remove existing socket file if it exists
	if err := os.RemoveAll(s.socketPath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("failed to remove existing socket: %w", err)
	}

	// Create Unix domain socket listener
	listener, err := net.Listen("unix", s.socketPath)
	if err != nil {
		return fmt.Errorf("failed to create Unix socket: %w", err)
	}

	// Set socket permissions for access
	if err := os.Chmod(s.socketPath, 0660); err != nil {
		listener.Close()
		return fmt.Errorf("failed to set socket permissions: %w", err)
	}

	s.listener = listener

	log.Printf("✅ Layer 3 ALM Unix socket server listening on %s", s.socketPath)

	// Start accepting connections
	s.wg.Add(1)
	go s.acceptLoop()

	return nil
}

// Stop gracefully shuts down the server
func (s *UnixSocketServer) Stop() error {
	log.Println("🛑 Stopping Layer 3 Unix socket server...")

	// Signal shutdown
	s.cancel()

	// Close listener
	if s.listener != nil {
		s.listener.Close()
	}

	// Close all active connections
	s.connections.Range(func(key, value interface{}) bool {
		if conn, ok := value.(net.Conn); ok {
			conn.Close()
		}
		return true
	})

	// Wait for all goroutines
	s.wg.Wait()

	// Remove socket file
	os.Remove(s.socketPath)

	log.Printf("✅ Layer 3 Unix socket server stopped (processed %d requests)",
		atomic.LoadUint64(&s.totalRequests))

	return nil
}

// acceptLoop handles incoming connections
func (s *UnixSocketServer) acceptLoop() {
	defer s.wg.Done()

	for {
		select {
		case <-s.ctx.Done():
			return
		default:
		}

		// Accept with timeout to allow periodic ctx checks
		s.listener.(*net.UnixListener).SetDeadline(time.Now().Add(1 * time.Second))

		conn, err := s.listener.Accept()
		if err != nil {
			if netErr, ok := err.(net.Error); ok && netErr.Timeout() {
				continue // Timeout is expected, check context and continue
			}
			if s.ctx.Err() != nil {
				return // Server is shutting down
			}
			log.Printf("Error accepting connection: %v", err)
			continue
		}

		// Check connection limit
		currentConns := atomic.LoadInt32(&s.connCount)
		if currentConns >= s.maxConns {
			log.Printf("Connection limit reached (%d), rejecting new connection", currentConns)
			conn.Close()
			continue
		}

		// Handle connection
		atomic.AddInt32(&s.connCount, 1)
		connID := uuid.New().String()
		s.connections.Store(connID, conn)

		s.wg.Add(1)
		go s.handleConnection(conn, connID)
	}
}

// handleConnection processes a single client connection
func (s *UnixSocketServer) handleConnection(conn net.Conn, connID string) {
	defer func() {
		conn.Close()
		s.connections.Delete(connID)
		atomic.AddInt32(&s.connCount, -1)

		// Clean up connection's graph data from all pools
		poolIDs := s.poolManager.ListPools()
		for _, poolID := range poolIDs {
			if pool, exists := s.poolManager.GetPool(poolID); exists {
				if pool.GetGraph() != nil {
					nodesRemoved, edgesRemoved := pool.GetGraph().CloseConnection(connID)
					if nodesRemoved > 0 || edgesRemoved > 0 {
						log.Printf("Connection %s cleanup (pool %s): removed %d nodes, %d edges", connID, poolID, nodesRemoved, edgesRemoved)
					}
				}
			}
		}

		s.wg.Done()
	}()

	log.Printf("🔗 New connection: %s", connID)

	for {
		select {
		case <-s.ctx.Done():
			return
		default:
		}

		// Set read deadline
		conn.SetReadDeadline(time.Now().Add(RequestTimeout))

		// Read 4-byte length prefix (binary protocol)
		var lenBuf [4]byte
		if _, err := io.ReadFull(conn, lenBuf[:]); err != nil {
			if err != io.EOF && err != net.ErrClosed && s.ctx.Err() == nil {
				log.Printf("Error reading from connection %s: %v", connID, err)
			}
			return
		}

		// Decode message length (little-endian u32)
		msgLen := binary.LittleEndian.Uint32(lenBuf[:])

		// Sanity check
		if msgLen == 0 || msgLen > 10000000 {
			log.Printf("Invalid message length from %s: %d", connID, msgLen)
			return
		}

		// Read message payload
		msgBuf := make([]byte, msgLen)
		if _, err := io.ReadFull(conn, msgBuf); err != nil {
			log.Printf("Error reading message from %s: %v", connID, err)
			return
		}

		// Parse JSON request
		var req SocketRequest
		if err := json.Unmarshal(msgBuf, &req); err != nil {
			s.sendErrorBinary(conn, "", fmt.Sprintf("Invalid JSON: %v", err))
			continue
		}

		// Process request
		atomic.AddUint64(&s.totalRequests, 1)
		s.processRequestBinary(conn, &req, connID)
	}
}

// getPoolFromRequest retrieves the appropriate pool for a request
func (s *UnixSocketServer) getPoolFromRequest(req *SocketRequest) (*alm.ALM, error) {
	poolID := req.PoolID
	if poolID == "" {
		poolID = "crucible_training" // Default pool for backwards compatibility
	}

	pool, err := s.poolManager.GetOrCreatePool(poolID)
	if err != nil {
		return nil, fmt.Errorf("failed to get pool %s: %w", poolID, err)
	}

	return pool, nil
}

// processRequest handles individual requests
func (s *UnixSocketServer) processRequest(writer *bufio.Writer, req *SocketRequest) {
	startTime := time.Now()

	switch req.Type {
	case "search":
		s.handleSearch(writer, req, startTime)

	case "add_memory":
		s.handleAddMemory(writer, req, startTime)

	case "add_association":
		s.handleAddAssociation(writer, req, startTime)

	case "get_stats":
		s.handleGetStats(writer, req, startTime)

	case "ping":
		s.handlePing(writer, req, startTime)

	default:
		s.sendError(writer, req.RequestID, fmt.Sprintf("Unknown request type: %s", req.Type))
	}
}

// handleSearch processes search requests
func (s *UnixSocketServer) handleSearch(writer *bufio.Writer, req *SocketRequest, startTime time.Time) {
	// Get the appropriate pool
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendError(writer, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	// Prepare search parameters
	limit := req.Limit
	if limit <= 0 {
		limit = 10
	}

	ctx := context.Background()

	// When query text is provided and no start memory IDs, use text search
	var results *alm.SearchResults
	if req.Query != "" {
		results, err = pool.SearchByText(ctx, req.Query, limit)
	} else {
		searchQuery := &alm.SearchQuery{
			StartMemoryIDs: []uint64{},
			MaxResults:     limit,
			MaxDepth:       3,
			MinWeight:      0.1,
			Timeout:        30 * time.Second,
		}
		results, err = pool.SearchAssociative(ctx, searchQuery)
	}

	if err != nil {
		s.sendError(writer, req.RequestID, fmt.Sprintf("Search failed: %v", err))
		return
	}

	// Convert results
	searchResults := make([]SearchResult, 0, len(results.Results))
	totalScore := float32(0)

	for _, r := range results.Results {
		score := float32(r.TotalWeight)
		searchResults = append(searchResults, SearchResult{
			ID:       r.Memory.ID,
			Content:  r.Memory.Content,
			Score:    score,
			Distance: r.Depth,
			Metadata: convertMetadata(r.Memory.Metadata),
		})
		totalScore += score
	}

	// Calculate average confidence
	confidence := float32(0)
	if len(searchResults) > 0 {
		confidence = totalScore / float32(len(searchResults))
	}

	// Send response
	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "search_response",
		RequestID:        req.RequestID,
		Success:          true,
		Results:          searchResults,
		Confidence:       confidence,
		ProcessingTimeMs: processingTime,
	}

	s.sendResponse(writer, &resp)
}

// convertMetadata converts map[string]string to map[string]interface{}
func convertMetadata(metadata map[string]string) map[string]interface{} {
	result := make(map[string]interface{})
	for k, v := range metadata {
		result[k] = v
	}
	return result
}

// handleAddMemory processes memory addition requests
func (s *UnixSocketServer) handleAddMemory(writer *bufio.Writer, req *SocketRequest, startTime time.Time) {
	// Get the appropriate pool
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendError(writer, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	// Generate unique memory ID
	memoryID := uint64(time.Now().UnixNano())

	// Convert metadata from map[string]interface{} to map[string]string
	metadataStr := make(map[string]string)
	for k, v := range req.Metadata {
		if strVal, ok := v.(string); ok {
			metadataStr[k] = strVal
		} else {
			metadataStr[k] = fmt.Sprintf("%v", v)
		}
	}

	// Create Memory struct
	memory := &alm.Memory{
		ID:       memoryID,
		Content:  req.Content,
		Tags:     []string{},
		Metadata: metadataStr,
	}

	// Add memory to ALM
	err = pool.AddMemory(memory)
	if err != nil {
		s.sendError(writer, req.RequestID, fmt.Sprintf("Failed to add memory: %v", err))
		return
	}

	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "add_memory_response",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
		Metadata: map[string]interface{}{
			"memory_id": memoryID,
		},
	}

	s.sendResponse(writer, &resp)
}

// handleAddAssociation processes association addition requests
func (s *UnixSocketServer) handleAddAssociation(writer *bufio.Writer, req *SocketRequest, startTime time.Time) {
	// Get the appropriate pool
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendError(writer, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	// Extract source and target IDs from metadata
	sourceID, sourceOk := req.Metadata["source_id"].(float64)
	targetID, targetOk := req.Metadata["target_id"].(float64)

	if !sourceOk || !targetOk {
		s.sendError(writer, req.RequestID, "Missing or invalid source_id/target_id")
		return
	}

	strength := float64(1.0)
	if s, ok := req.Metadata["strength"].(float64); ok {
		strength = s
	}

	// Create Association struct
	assoc := &alm.Association{
		ID:           uuid.New().String(),
		FromMemoryID: uint64(sourceID),
		ToMemoryID:   uint64(targetID),
		Type:         "user_defined",
		Weight:       strength,
		Reason:       "Added via socket API",
		ConnectionID: "", // No connection tracking for non-binary protocol
	}

	// Add association to ALM
	err = pool.AddAssociation(assoc)
	if err != nil {
		s.sendError(writer, req.RequestID, fmt.Sprintf("Failed to add association: %v", err))
		return
	}

	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "add_association_response",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
	}

	s.sendResponse(writer, &resp)
}

// handleGetStats processes statistics requests
func (s *UnixSocketServer) handleGetStats(writer *bufio.Writer, req *SocketRequest, startTime time.Time) {
	// Get the appropriate pool
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendError(writer, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	stats := pool.GetGraphStats()
	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "stats_response",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
		Metadata: map[string]interface{}{
			"pool_id":            req.PoolID,
			"total_pools":        s.poolManager.PoolCount(),
			"total_memories":     stats.TotalMemories,
			"total_associations": stats.TotalAssociations,
			"total_queries":      atomic.LoadUint64(&s.totalRequests),
			"active_connections": atomic.LoadInt32(&s.connCount),
		},
	}

	s.sendResponse(writer, &resp)
}

// handlePing processes ping requests
func (s *UnixSocketServer) handlePing(writer *bufio.Writer, req *SocketRequest, startTime time.Time) {
	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "pong",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
		Metadata: map[string]interface{}{
			"layer":   "Layer3-ALM",
			"version": "1.0.0",
			"timestamp": time.Now().Unix(),
		},
	}

	s.sendResponse(writer, &resp)
}

// sendResponse sends a JSON response
func (s *UnixSocketServer) sendResponse(writer *bufio.Writer, resp *SocketResponse) {
	data, err := json.Marshal(resp)
	if err != nil {
		log.Printf("Failed to marshal response: %v", err)
		return
	}

	writer.Write(data)
	writer.WriteByte('\n')
	writer.Flush()

	atomic.AddUint64(&s.totalResponses, 1)
}

// sendError sends an error response
func (s *UnixSocketServer) sendError(writer *bufio.Writer, requestID string, errMsg string) {
	resp := SocketResponse{
		Type:      "error",
		RequestID: requestID,
		Success:   false,
		Error:     errMsg,
	}

	s.sendResponse(writer, &resp)
	atomic.AddUint64(&s.totalErrors, 1)
}

// sendErrorBinary sends an error response using binary protocol
func (s *UnixSocketServer) sendErrorBinary(conn net.Conn, requestID string, errMsg string) {
	resp := SocketResponse{
		Type:      "error",
		RequestID: requestID,
		Success:   false,
		Error:     errMsg,
	}

	s.sendResponseBinary(conn, &resp)
	atomic.AddUint64(&s.totalErrors, 1)
}

// sendResponseBinary sends a binary protocol response
func (s *UnixSocketServer) sendResponseBinary(conn net.Conn, resp *SocketResponse) {
	data, err := json.Marshal(resp)
	if err != nil {
		log.Printf("Failed to marshal response: %v", err)
		return
	}

	// Write length prefix (4 bytes, little-endian u32)
	var lenBuf [4]byte
	binary.LittleEndian.PutUint32(lenBuf[:], uint32(len(data)))

	if _, err := conn.Write(lenBuf[:]); err != nil {
		log.Printf("Failed to write response length: %v", err)
		return
	}

	// Write response data
	if _, err := conn.Write(data); err != nil {
		log.Printf("Failed to write response data: %v", err)
		return
	}

	atomic.AddUint64(&s.totalResponses, 1)
}

// processRequestBinary handles individual requests using binary protocol
func (s *UnixSocketServer) processRequestBinary(conn net.Conn, req *SocketRequest, connID string) {
	startTime := time.Now()

	switch req.Type {
	case "search":
		s.handleSearchBinary(conn, req, startTime)

	case "add_memory":
		s.handleAddMemoryBinary(conn, req, startTime)

	case "add_association":
		s.handleAddAssociationBinary(conn, req, startTime, connID)

	case "get_stats":
		s.handleGetStatsBinary(conn, req, startTime)

	case "ping":
		s.handlePingBinary(conn, req, startTime)

	case "HealthCheck":
		s.handleHealthCheckBinary(conn, req, startTime)

	default:
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Unknown request type: %s", req.Type))
	}
}

// Binary protocol versions of handlers
func (s *UnixSocketServer) handleSearchBinary(conn net.Conn, req *SocketRequest, startTime time.Time) {
	// Get the appropriate pool
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	// Prepare search parameters
	limit := req.Limit
	if limit <= 0 {
		limit = 10
	}

	ctx := context.Background()

	// When query text is provided and no start memory IDs, use text search
	var results *alm.SearchResults
	if req.Query != "" {
		results, err = pool.SearchByText(ctx, req.Query, limit)
	} else {
		searchQuery := &alm.SearchQuery{
			StartMemoryIDs: []uint64{},
			MaxResults:     limit,
			MaxDepth:       3,
			MinWeight:      0.1,
			Timeout:        30 * time.Second,
		}
		results, err = pool.SearchAssociative(ctx, searchQuery)
	}

	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Search failed: %v", err))
		return
	}

	// Convert results
	searchResults := make([]SearchResult, 0, len(results.Results))
	totalScore := float32(0)

	for _, r := range results.Results {
		score := float32(r.TotalWeight)
		searchResults = append(searchResults, SearchResult{
			ID:       r.Memory.ID,
			Content:  r.Memory.Content,
			Score:    score,
			Distance: r.Depth,
			Metadata: convertMetadata(r.Memory.Metadata),
		})
		totalScore += score
	}

	// Calculate average confidence
	confidence := float32(0)
	if len(searchResults) > 0 {
		confidence = totalScore / float32(len(searchResults))
	}

	// Send response
	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "search_response",
		RequestID:        req.RequestID,
		Success:          true,
		Results:          searchResults,
		Confidence:       confidence,
		ProcessingTimeMs: processingTime,
	}

	s.sendResponseBinary(conn, &resp)
}

func (s *UnixSocketServer) handleAddMemoryBinary(conn net.Conn, req *SocketRequest, startTime time.Time) {
	// Get the appropriate pool
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	memoryID := uint64(time.Now().UnixNano())

	metadataStr := make(map[string]string)
	for k, v := range req.Metadata {
		if strVal, ok := v.(string); ok {
			metadataStr[k] = strVal
		} else {
			metadataStr[k] = fmt.Sprintf("%v", v)
		}
	}

	memory := &alm.Memory{
		ID:       memoryID,
		Content:  req.Content,
		Tags:     []string{},
		Metadata: metadataStr,
	}

	err = pool.AddMemory(memory)
	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Failed to add memory: %v", err))
		return
	}

	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "add_memory_response",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
		Metadata: map[string]interface{}{
			"memory_id": memoryID,
		},
	}

	s.sendResponseBinary(conn, &resp)
}

func (s *UnixSocketServer) handleAddAssociationBinary(conn net.Conn, req *SocketRequest, startTime time.Time, connID string) {
	// Get the appropriate pool
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	sourceID, sourceOk := req.Metadata["source_id"].(float64)
	targetID, targetOk := req.Metadata["target_id"].(float64)

	if !sourceOk || !targetOk {
		s.sendErrorBinary(conn, req.RequestID, "Missing or invalid source_id/target_id")
		return
	}

	strength := float64(1.0)
	if s, ok := req.Metadata["strength"].(float64); ok {
		strength = s
	}

	assoc := &alm.Association{
		ID:           uuid.New().String(),
		FromMemoryID: uint64(sourceID),
		ToMemoryID:   uint64(targetID),
		Type:         "user_defined",
		Weight:       strength,
		Reason:       "Added via socket API",
		ConnectionID: connID, // Track which connection owns this association
	}

	err = pool.AddAssociation(assoc)
	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Failed to add association: %v", err))
		return
	}

	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "add_association_response",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
	}

	s.sendResponseBinary(conn, &resp)
}

func (s *UnixSocketServer) handleGetStatsBinary(conn net.Conn, req *SocketRequest, startTime time.Time) {
	// Get the appropriate pool
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	stats := pool.GetGraphStats()
	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "stats_response",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
		Metadata: map[string]interface{}{
			"pool_id":            req.PoolID,
			"total_memories":     stats.TotalMemories,
			"total_associations": stats.TotalAssociations,
			"total_queries":      atomic.LoadUint64(&s.totalRequests),
			"active_connections": atomic.LoadInt32(&s.connCount),
			"total_pools":        s.poolManager.PoolCount(),
		},
	}

	s.sendResponseBinary(conn, &resp)
}

func (s *UnixSocketServer) handlePingBinary(conn net.Conn, req *SocketRequest, startTime time.Time) {
	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "pong",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
		Metadata: map[string]interface{}{
			"layer":     "Layer3-ALM",
			"version":   "1.0.0",
			"timestamp": time.Now().Unix(),
		},
	}

	s.sendResponseBinary(conn, &resp)
}

func (s *UnixSocketServer) handleHealthCheckBinary(conn net.Conn, req *SocketRequest, startTime time.Time) {
	// Get the appropriate pool (or default pool)
	pool, err := s.getPoolFromRequest(req)
	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Pool error: %v", err))
		return
	}

	// Get graph statistics
	stats := pool.GetGraphStats()

	// Calculate uptime
	uptimeSeconds := int64(time.Since(s.startTime).Seconds())

	// Build health check response
	healthResp := HealthCheckResponse{
		Status:        "healthy",
		Layer:         "Layer3_ALM",
		Timestamp:     time.Now().UnixMilli(),
		UptimeSeconds: uptimeSeconds,
		Metrics: map[string]interface{}{
			"pool_id":            req.PoolID,
			"total_pools":        s.poolManager.PoolCount(),
			"total_memories":     stats.TotalMemories,
			"total_associations": stats.TotalAssociations,
			"total_queries":      atomic.LoadUint64(&s.totalRequests),
			"success_rate":       1.0, // Could track actual success rate
			"avg_latency_us":     0, // Calculated from real request metrics when available
			"graph_density":      stats.GraphDensity,
			"active_connections": atomic.LoadInt32(&s.connCount),
		},
	}

	// Marshal to JSON
	data, err := json.Marshal(healthResp)
	if err != nil {
		s.sendErrorBinary(conn, req.RequestID, fmt.Sprintf("Failed to marshal health response: %v", err))
		return
	}

	// Write length prefix (4 bytes, little-endian u32)
	var lenBuf [4]byte
	binary.LittleEndian.PutUint32(lenBuf[:], uint32(len(data)))

	if _, err := conn.Write(lenBuf[:]); err != nil {
		log.Printf("Failed to write health check response length: %v", err)
		return
	}

	// Write response data
	if _, err := conn.Write(data); err != nil {
		log.Printf("Failed to write health check response data: %v", err)
		return
	}

	atomic.AddUint64(&s.totalResponses, 1)
}