package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"sync"
	"sync/atomic"
	"time"

	"github.com/gorilla/websocket"
)

// MetricsServer provides HTTP endpoints and WebSocket for dashboard
type MetricsServer struct {
	mu sync.RWMutex

	// Real-time metrics
	metrics *SystemMetrics

	// WebSocket connections
	clients map[*websocket.Conn]bool

	// Configuration
	port int

	// Update channels
	updateChan chan MetricUpdate
}

// SystemMetrics holds all system performance data
type SystemMetrics struct {
	// System-wide metrics
	QPS          float64   `json:"qps"`
	Latency      float64   `json:"latency"`
	Memory       float64   `json:"memory"`
	Connections  int       `json:"connections"`
	ErrorRate    float64   `json:"errorRate"`
	Uptime       int64     `json:"uptime"`
	StartTime    time.Time `json:"startTime"`

	// Per-layer metrics
	Layers map[string]*LayerMetrics `json:"layers"`

	// Historical data
	History *MetricsHistory `json:"history"`
}

// LayerMetrics contains metrics for a single layer
type LayerMetrics struct {
	Name       string  `json:"name"`
	Status     string  `json:"status"` // healthy, degraded, failed
	Latency    float64 `json:"latency"`
	Memory     float64 `json:"memory"`
	Throughput float64 `json:"throughput"`
	ErrorCount int64   `json:"errorCount"`

	// Layer-specific metrics
	Custom map[string]interface{} `json:"custom"`
}

// MetricsHistory stores time-series data
type MetricsHistory struct {
	mu sync.RWMutex

	QPS        []TimeSeriesPoint `json:"qps"`
	Latency    []TimeSeriesPoint `json:"latency"`
	Memory     []TimeSeriesPoint `json:"memory"`
	Errors     []TimeSeriesPoint `json:"errors"`

	MaxPoints  int               `json:"maxPoints"`
}

// TimeSeriesPoint represents a single data point
type TimeSeriesPoint struct {
	Time  int64   `json:"time"`
	Value float64 `json:"value"`
}

// MetricUpdate represents a metric update message
type MetricUpdate struct {
	Type      string      `json:"type"`
	Layer     string      `json:"layer,omitempty"`
	Metric    string      `json:"metric"`
	Value     interface{} `json:"value"`
	Timestamp int64       `json:"timestamp"`
}

// LogEntry represents a system log message
type LogEntry struct {
	Timestamp time.Time `json:"timestamp"`
	Level     string    `json:"level"`
	Layer     string    `json:"layer"`
	Message   string    `json:"message"`
}

var upgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool {
		// Allow connections from same origin
		return true
	},
}

// NewMetricsServer creates a new metrics server
func NewMetricsServer(port int) *MetricsServer {
	ms := &MetricsServer{
		metrics: &SystemMetrics{
			StartTime: time.Now(),
			Layers: map[string]*LayerMetrics{
				"layer1": {Name: "IFR", Status: "healthy"},
				"layer2": {Name: "DSR", Status: "healthy"},
				"layer3": {Name: "ALM", Status: "healthy"},
				"layer4": {Name: "CPE", Status: "degraded"},
			},
			History: &MetricsHistory{
				MaxPoints: 3600, // Store 1 hour at 1s resolution
			},
		},
		clients:    make(map[*websocket.Conn]bool),
		port:       port,
		updateChan: make(chan MetricUpdate, 1000),
	}

	// Initialize layer-specific custom metrics
	ms.metrics.Layers["layer1"].Custom = map[string]interface{}{
		"hitRate": 95.0,
		"entries": 10000,
	}
	ms.metrics.Layers["layer2"].Custom = map[string]interface{}{
		"accuracy": 92.0,
		"neurons":  100000,
	}
	ms.metrics.Layers["layer3"].Custom = map[string]interface{}{
		"graphSize": 50000,
		"edges":     500000,
	}
	ms.metrics.Layers["layer4"].Custom = map[string]interface{}{
		"patterns": 0,
		"accuracy": 0,
	}

	return ms
}

// Start begins the metrics server
func (ms *MetricsServer) Start() error {
	// Start metrics collection
	go ms.collectMetrics()

	// Start update broadcaster
	go ms.broadcastUpdates()

	// Serve static files (dashboard)
	http.HandleFunc("/", ms.serveDashboard)

	// API endpoints
	http.HandleFunc("/api/metrics", ms.handleMetrics)
	http.HandleFunc("/api/layers", ms.handleLayers)
	http.HandleFunc("/api/history", ms.handleHistory)
	http.HandleFunc("/api/logs", ms.handleLogs)

	// WebSocket endpoint
	http.HandleFunc("/ws", ms.handleWebSocket)

	log.Printf("Starting metrics server on port %d", ms.port)
	return http.ListenAndServe(fmt.Sprintf(":%d", ms.port), nil)
}

// serveDashboard serves the dashboard HTML
func (ms *MetricsServer) serveDashboard(w http.ResponseWriter, r *http.Request) {
	if r.URL.Path == "/" {
		http.ServeFile(w, r, "./dashboard/index.html")
	} else if r.URL.Path == "/dashboard.js" {
		http.ServeFile(w, r, "./dashboard/dashboard.js")
	} else {
		http.NotFound(w, r)
	}
}

// handleMetrics returns current metrics as JSON
func (ms *MetricsServer) handleMetrics(w http.ResponseWriter, r *http.Request) {
	ms.mu.RLock()
	defer ms.mu.RUnlock()

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(ms.metrics)
}

// handleLayers returns layer status
func (ms *MetricsServer) handleLayers(w http.ResponseWriter, r *http.Request) {
	ms.mu.RLock()
	defer ms.mu.RUnlock()

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(ms.metrics.Layers)
}

// handleHistory returns historical metrics
func (ms *MetricsServer) handleHistory(w http.ResponseWriter, r *http.Request) {
	ms.mu.RLock()
	defer ms.mu.RUnlock()

	// Get time range from query params
	timeRange := r.URL.Query().Get("range")
	if timeRange == "" {
		timeRange = "1h"
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(ms.metrics.History)
}

// handleLogs returns recent system logs
func (ms *MetricsServer) handleLogs(w http.ResponseWriter, r *http.Request) {
	// Mock log data - in production, read from actual log system
	logs := []LogEntry{
		{
			Timestamp: time.Now().Add(-5 * time.Minute),
			Level:     "info",
			Layer:     "layer3",
			Message:   "Query processed successfully",
		},
		{
			Timestamp: time.Now().Add(-3 * time.Minute),
			Level:     "warning",
			Layer:     "layer2",
			Message:   "High memory usage detected",
		},
		{
			Timestamp: time.Now().Add(-1 * time.Minute),
			Level:     "error",
			Layer:     "layer4",
			Message:   "Connection timeout",
		},
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(logs)
}

// handleWebSocket handles WebSocket connections for real-time updates
func (ms *MetricsServer) handleWebSocket(w http.ResponseWriter, r *http.Request) {
	conn, err := upgrader.Upgrade(w, r, nil)
	if err != nil {
		log.Printf("WebSocket upgrade failed: %v", err)
		return
	}
	defer conn.Close()

	ms.mu.Lock()
	ms.clients[conn] = true
	ms.mu.Unlock()

	// Send initial metrics
	ms.sendInitialMetrics(conn)

	// Keep connection alive and handle messages
	for {
		var msg map[string]interface{}
		err := conn.ReadJSON(&msg)
		if err != nil {
			ms.mu.Lock()
			delete(ms.clients, conn)
			ms.mu.Unlock()
			break
		}

		// Handle client messages (if any)
		ms.handleClientMessage(conn, msg)
	}
}

// sendInitialMetrics sends current metrics to new WebSocket client
func (ms *MetricsServer) sendInitialMetrics(conn *websocket.Conn) {
	ms.mu.RLock()
	defer ms.mu.RUnlock()

	message := map[string]interface{}{
		"type":    "initial",
		"metrics": ms.metrics,
	}

	conn.WriteJSON(message)
}

// handleClientMessage processes messages from WebSocket clients
func (ms *MetricsServer) handleClientMessage(conn *websocket.Conn, msg map[string]interface{}) {
	// Handle different message types
	msgType, ok := msg["type"].(string)
	if !ok {
		return
	}

	switch msgType {
	case "subscribe":
		// Client wants to subscribe to specific metrics
		log.Printf("Client subscribed to updates")

	case "query":
		// Client requesting specific data
		ms.handleQuery(conn, msg)

	default:
		log.Printf("Unknown message type: %s", msgType)
	}
}

// handleQuery processes query requests from clients
func (ms *MetricsServer) handleQuery(conn *websocket.Conn, msg map[string]interface{}) {
	queryType, ok := msg["query"].(string)
	if !ok {
		return
	}

	switch queryType {
	case "trace":
		// Send query trace data
		trace := ms.generateQueryTrace()
		conn.WriteJSON(map[string]interface{}{
			"type": "trace",
			"data": trace,
		})

	case "config":
		// Send configuration data
		config := ms.getSystemConfig()
		conn.WriteJSON(map[string]interface{}{
			"type": "config",
			"data": config,
		})
	}
}

// collectMetrics continuously collects system metrics
func (ms *MetricsServer) collectMetrics() {
	ticker := time.NewTicker(1 * time.Second)
	defer ticker.Stop()

	var (
		queryCount int64
		errorCount int64
	)

	for range ticker.C {
		// Simulate metric collection - replace with actual metrics
		ms.mu.Lock()

		// Update system metrics
		ms.metrics.QPS = float64(atomic.LoadInt64(&queryCount)) + (95 + float64(time.Now().Unix()%10))
		ms.metrics.Latency = 8 + float64(time.Now().Unix()%4)
		ms.metrics.Memory = 30 + float64(time.Now().Unix()%20)
		ms.metrics.ErrorRate = float64(atomic.LoadInt64(&errorCount)) * 0.01
		ms.metrics.Connections = 10 + int(time.Now().Unix()%5)
		ms.metrics.Uptime = int64(time.Since(ms.metrics.StartTime).Seconds())

		// Update layer metrics
		for _, layer := range ms.metrics.Layers {
			if layer.Status == "healthy" {
				layer.Latency = layer.Latency * (0.9 + 0.2*float64(time.Now().Unix()%10)/10)
				layer.Memory = layer.Memory * (0.95 + 0.1*float64(time.Now().Unix()%10)/10)
				layer.Throughput = ms.metrics.QPS / 4 // Distribute throughput
			}
		}

		// Add to history
		ms.addToHistory()

		ms.mu.Unlock()

		// Send update to all connected clients
		ms.broadcastMetrics()
	}
}

// addToHistory adds current metrics to historical data
func (ms *MetricsServer) addToHistory() {
	now := time.Now().UnixMilli()

	ms.metrics.History.mu.Lock()
	defer ms.metrics.History.mu.Unlock()

	// Add new points
	ms.metrics.History.QPS = append(ms.metrics.History.QPS,
		TimeSeriesPoint{Time: now, Value: ms.metrics.QPS})
	ms.metrics.History.Latency = append(ms.metrics.History.Latency,
		TimeSeriesPoint{Time: now, Value: ms.metrics.Latency})
	ms.metrics.History.Memory = append(ms.metrics.History.Memory,
		TimeSeriesPoint{Time: now, Value: ms.metrics.Memory})
	ms.metrics.History.Errors = append(ms.metrics.History.Errors,
		TimeSeriesPoint{Time: now, Value: ms.metrics.ErrorRate})

	// Trim old data
	maxLen := ms.metrics.History.MaxPoints
	if len(ms.metrics.History.QPS) > maxLen {
		ms.metrics.History.QPS = ms.metrics.History.QPS[len(ms.metrics.History.QPS)-maxLen:]
	}
	if len(ms.metrics.History.Latency) > maxLen {
		ms.metrics.History.Latency = ms.metrics.History.Latency[len(ms.metrics.History.Latency)-maxLen:]
	}
	if len(ms.metrics.History.Memory) > maxLen {
		ms.metrics.History.Memory = ms.metrics.History.Memory[len(ms.metrics.History.Memory)-maxLen:]
	}
	if len(ms.metrics.History.Errors) > maxLen {
		ms.metrics.History.Errors = ms.metrics.History.Errors[len(ms.metrics.History.Errors)-maxLen:]
	}
}

// broadcastMetrics sends current metrics to all connected clients
func (ms *MetricsServer) broadcastMetrics() {
	ms.mu.RLock()
	defer ms.mu.RUnlock()

	message := map[string]interface{}{
		"type":    "update",
		"metrics": ms.metrics,
	}

	for client := range ms.clients {
		err := client.WriteJSON(message)
		if err != nil {
			client.Close()
			delete(ms.clients, client)
		}
	}
}

// broadcastUpdates handles metric update broadcasting
func (ms *MetricsServer) broadcastUpdates() {
	for update := range ms.updateChan {
		ms.mu.RLock()
		clients := ms.clients
		ms.mu.RUnlock()

		for client := range clients {
			err := client.WriteJSON(update)
			if err != nil {
				client.Close()
				ms.mu.Lock()
				delete(ms.clients, client)
				ms.mu.Unlock()
			}
		}
	}
}

// generateQueryTrace creates a sample query trace
func (ms *MetricsServer) generateQueryTrace() map[string]interface{} {
	return map[string]interface{}{
		"queryId":   "q-" + fmt.Sprintf("%d", time.Now().Unix()),
		"timestamp": time.Now().UnixMilli(),
		"layers": []map[string]interface{}{
			{
				"layer":    "layer1",
				"name":     "IFR",
				"latency":  0.5,
				"status":   "complete",
				"hitCache": true,
			},
			{
				"layer":   "layer2",
				"name":    "DSR",
				"latency": 30,
				"status":  "complete",
				"matches": 15,
			},
			{
				"layer":   "layer3",
				"name":    "ALM",
				"latency": 160,
				"status":  "complete",
				"paths":   3,
			},
			{
				"layer":   "layer4",
				"name":    "CPE",
				"latency": 0,
				"status":  "skipped",
				"reason":  "layer unavailable",
			},
		},
		"totalLatency": 190.5,
		"result":       "success",
	}
}

// getSystemConfig returns system configuration
func (ms *MetricsServer) getSystemConfig() map[string]interface{} {
	return map[string]interface{}{
		"system": map[string]interface{}{
			"name":        "Memory Flow Network",
			"version":     "1.0.0",
			"environment": "development",
		},
		"layers": map[string]interface{}{
			"layer1": map[string]interface{}{
				"type":          "IFR",
				"implementation": "Zig",
				"socket":        "/tmp/mfn_layer1.sock",
				"targetLatency": "1μs",
			},
			"layer2": map[string]interface{}{
				"type":          "DSR",
				"implementation": "Rust",
				"socket":        "/tmp/mfn_layer2.sock",
				"targetLatency": "50μs",
			},
			"layer3": map[string]interface{}{
				"type":          "ALM",
				"implementation": "Go",
				"socket":        "/tmp/mfn_layer3.sock",
				"targetLatency": "10μs",
			},
			"layer4": map[string]interface{}{
				"type":          "CPE",
				"implementation": "Rust",
				"socket":        "/tmp/mfn_layer4.sock",
				"targetLatency": "100μs",
			},
		},
		"monitoring": map[string]interface{}{
			"metricsPort":    9090,
			"dashboardPort":  ms.port,
			"logLevel":       "info",
			"retentionDays":  7,
		},
		"performance": map[string]interface{}{
			"targetQPS":      1000,
			"maxLatency":     "20ms",
			"memoryLimit":    "1GB",
			"connectionPool": 100,
		},
	}
}

// UpdateMetric allows external systems to push metrics
func (ms *MetricsServer) UpdateMetric(layer, metric string, value interface{}) {
	update := MetricUpdate{
		Type:      "metric",
		Layer:     layer,
		Metric:    metric,
		Value:     value,
		Timestamp: time.Now().UnixMilli(),
	}

	select {
	case ms.updateChan <- update:
	default:
		// Channel full, drop update
	}
}

// UpdateLayerStatus updates the status of a layer
func (ms *MetricsServer) UpdateLayerStatus(layer, status string) {
	ms.mu.Lock()
	defer ms.mu.Unlock()

	if l, exists := ms.metrics.Layers[layer]; exists {
		l.Status = status

		// Send immediate update
		update := MetricUpdate{
			Type:      "status",
			Layer:     layer,
			Metric:    "status",
			Value:     status,
			Timestamp: time.Now().UnixMilli(),
		}

		select {
		case ms.updateChan <- update:
		default:
		}
	}
}

func main() {
	server := NewMetricsServer(8080)

	// Example of external metric updates
	go func() {
		ticker := time.NewTicker(5 * time.Second)
		defer ticker.Stop()

		for range ticker.C {
			// Simulate layer status changes
			if time.Now().Unix()%20 == 0 {
				server.UpdateLayerStatus("layer1", "degraded")
				time.Sleep(10 * time.Second)
				server.UpdateLayerStatus("layer1", "healthy")
			}
		}
	}()

	log.Fatal(server.Start())
}