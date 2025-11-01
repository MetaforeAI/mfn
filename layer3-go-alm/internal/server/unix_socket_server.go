// Unix Socket Server for Layer 3 ALM
// Provides high-performance Unix domain socket interface for associative queries

package server

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
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
	DefaultSocketPath = "/tmp/mfn_layer3.sock"
	MaxConnections    = 100
	BufferSize        = 8192
	RequestTimeout    = 30 * time.Second
)

// UnixSocketServer handles Unix domain socket connections for Layer 3
type UnixSocketServer struct {
	alm        *alm.ALM
	socketPath string
	listener   net.Listener

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
}

// SocketRequest represents an incoming request via Unix socket
type SocketRequest struct {
	Type      string                 `json:"type"`
	RequestID string                 `json:"request_id"`
	Query     string                 `json:"query,omitempty"`
	Content   string                 `json:"content,omitempty"`
	Limit     int                    `json:"limit,omitempty"`
	MinConfidence float32            `json:"min_confidence,omitempty"`
	Metadata  map[string]interface{} `json:"metadata,omitempty"`
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
func NewUnixSocketServer(almInstance *alm.ALM, socketPath string) *UnixSocketServer {
	if socketPath == "" {
		socketPath = DefaultSocketPath
	}

	ctx, cancel := context.WithCancel(context.Background())

	return &UnixSocketServer{
		alm:        almInstance,
		socketPath: socketPath,
		maxConns:   MaxConnections,
		ctx:        ctx,
		cancel:     cancel,
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
	if err := os.Chmod(s.socketPath, 0666); err != nil {
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
		s.wg.Done()
	}()

	log.Printf("🔗 New connection: %s", connID)

	reader := bufio.NewReader(conn)
	writer := bufio.NewWriter(conn)

	for {
		select {
		case <-s.ctx.Done():
			return
		default:
		}

		// Set read deadline
		conn.SetReadDeadline(time.Now().Add(RequestTimeout))

		// Read request line
		line, err := reader.ReadBytes('\n')
		if err != nil {
			if err != net.ErrClosed && s.ctx.Err() == nil {
				log.Printf("Error reading from connection %s: %v", connID, err)
			}
			return
		}

		// Parse JSON request
		var req SocketRequest
		if err := json.Unmarshal(line, &req); err != nil {
			s.sendError(writer, "", fmt.Sprintf("Invalid JSON: %v", err))
			continue
		}

		// Process request
		atomic.AddUint64(&s.totalRequests, 1)
		s.processRequest(writer, &req)
	}
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
	// Prepare search parameters
	limit := req.Limit
	if limit <= 0 {
		limit = 10
	}

	// Perform associative search
	results, err := s.alm.Search(req.Query, limit)
	if err != nil {
		s.sendError(writer, req.RequestID, fmt.Sprintf("Search failed: %v", err))
		return
	}

	// Convert results
	searchResults := make([]SearchResult, 0, len(results))
	totalScore := float32(0)

	for _, r := range results {
		searchResults = append(searchResults, SearchResult{
			ID:       r.ID,
			Content:  r.Content,
			Score:    r.Score,
			Distance: r.Distance,
			Metadata: r.Metadata,
		})
		totalScore += r.Score
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

// handleAddMemory processes memory addition requests
func (s *UnixSocketServer) handleAddMemory(writer *bufio.Writer, req *SocketRequest, startTime time.Time) {
	// Add memory to ALM
	memoryID, err := s.alm.AddMemory(req.Content, req.Metadata)
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
	// Extract source and target IDs from metadata
	sourceID, sourceOk := req.Metadata["source_id"].(float64)
	targetID, targetOk := req.Metadata["target_id"].(float64)

	if !sourceOk || !targetOk {
		s.sendError(writer, req.RequestID, "Missing or invalid source_id/target_id")
		return
	}

	strength := float32(1.0)
	if s, ok := req.Metadata["strength"].(float64); ok {
		strength = float32(s)
	}

	// Add association to ALM
	err := s.alm.AddAssociation(uint64(sourceID), uint64(targetID), strength)
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
	stats := s.alm.GetStats()
	processingTime := float32(time.Since(startTime).Milliseconds())

	resp := SocketResponse{
		Type:             "stats_response",
		RequestID:        req.RequestID,
		Success:          true,
		ProcessingTimeMs: processingTime,
		Metadata: map[string]interface{}{
			"total_memories":    stats.TotalMemories,
			"total_associations": stats.TotalAssociations,
			"total_queries":     atomic.LoadUint64(&s.totalRequests),
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