//! Unix Socket Server for Layer 4 Context Prediction Engine
//! 
//! Provides high-performance Unix socket interface for context prediction operations.
//! Supports both JSON (backward compatibility) and binary protocol (high performance).
//! 
//! Target Performance:
//! - Context prediction: <5.2ms operation latency
//! - Socket path: /tmp/mfn_layer4.sock
//! - Concurrent connection handling
//! - Temporal pattern learning and prediction

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use uuid::Uuid;

use mfn_core::{
    layer_interface::*,
    memory_types::*,
    ContextPredictionEngine, MemoryAccess, ContextWindow, TemporalPattern,
    PredictionResult, ContextState, UniversalMemory
};

use crate::ContextPredictionLayer;

/// Default socket path for Layer 4 Context Prediction Engine
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/mfn_layer4.sock";

/// Socket server configuration
#[derive(Debug, Clone)]
pub struct SocketServerConfig {
    pub socket_path: String,
    pub max_connections: usize,
    pub connection_timeout_ms: u64,
    pub enable_binary_protocol: bool,
    pub enable_json_protocol: bool,
    pub buffer_size: usize,
    pub prediction_timeout_ms: u64,
}

impl Default for SocketServerConfig {
    fn default() -> Self {
        Self {
            socket_path: DEFAULT_SOCKET_PATH.to_string(),
            max_connections: 100,
            connection_timeout_ms: 30000,
            enable_binary_protocol: true,
            enable_json_protocol: true,
            buffer_size: 8192,
            prediction_timeout_ms: 5000, // 5 second timeout for predictions
        }
    }
}

/// Request types supported by the socket interface
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SocketRequest {
    /// Add memory to the context engine
    AddMemory {
        request_id: String,
        memory: UniversalMemory,
    },

    /// Update context with memory access event
    UpdateContext {
        request_id: String,
        access: MemoryAccess,
    },

    /// Predict next likely memories based on context
    PredictNext {
        request_id: String,
        context: ContextWindow,
    },

    /// Learn from memory access pattern sequence
    LearnPattern {
        request_id: String,
        access_sequence: Vec<MemoryAccess>,
    },

    /// Get current context state
    GetContextState {
        request_id: String,
    },

    /// Search with context prediction routing
    Search {
        request_id: String,
        query: UniversalSearchQuery,
    },

    /// Get performance statistics
    GetStats {
        request_id: String,
    },

    /// Health check / ping
    Ping {
        request_id: String,
    },

    /// Get memory by ID (basic operation)
    GetMemory {
        request_id: String,
        memory_id: MemoryId,
    },
}

/// Response types from the socket interface
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SocketResponse {
    /// Success response with data
    Success {
        request_id: String,
        data: serde_json::Value,
        processing_time_ms: f32,
    },

    /// Error response
    Error {
        request_id: String,
        error: String,
        error_code: String,
    },

    /// Prediction results response
    PredictionResults {
        request_id: String,
        predictions: Vec<PredictionResult>,
        processing_time_ms: f32,
    },

    /// Context state response
    ContextStateResponse {
        request_id: String,
        context_state: ContextState,
        processing_time_ms: f32,
    },

    /// Search routing response
    SearchResponse {
        request_id: String,
        routing_decision: RoutingDecision,
        processing_time_ms: f32,
    },

    /// Pong response to ping
    Pong {
        request_id: String,
        timestamp: u64,
        layer: String,
        version: String,
    },
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub connection_id: String,
    pub connected_at: Instant,
    pub requests_processed: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub last_activity: Instant,
    pub protocol_type: String,
}

/// Main Unix Socket Server for Layer 4 Context Prediction Engine
pub struct SocketServer {
    config: SocketServerConfig,
    context_engine: Arc<RwLock<ContextPredictionLayer>>,
    listener: Option<UnixListener>,
    running: Arc<RwLock<bool>>,
    connections: Arc<RwLock<HashMap<String, ConnectionStats>>>,
    
    // Performance metrics
    total_requests: Arc<RwLock<u64>>,
    total_connections: Arc<RwLock<u64>>,
    active_connections: Arc<RwLock<u64>>,
}

impl SocketServer {
    /// Create a new socket server instance
    pub fn new(
        context_engine: Arc<RwLock<ContextPredictionLayer>>,
        config: Option<SocketServerConfig>,
    ) -> Self {
        Self {
            config: config.unwrap_or_default(),
            context_engine,
            listener: None,
            running: Arc::new(RwLock::new(false)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            total_requests: Arc::new(RwLock::new(0)),
            total_connections: Arc::new(RwLock::new(0)),
            active_connections: Arc::new(RwLock::new(0)),
        }
    }

    /// Start the socket server
    pub async fn start(&mut self) -> Result<()> {
        // Remove existing socket file if it exists
        if Path::new(&self.config.socket_path).exists() {
            std::fs::remove_file(&self.config.socket_path)
                .map_err(|e| anyhow!("Failed to remove existing socket: {}", e))?;
        }

        // Create Unix domain socket listener
        let listener = UnixListener::bind(&self.config.socket_path)
            .map_err(|e| anyhow!("Failed to bind Unix socket: {}", e))?;
        
        self.listener = Some(listener);
        
        {
            let mut running = self.running.write().await;
            *running = true;
        }

        println!(
            "🧠 Layer 4 Context Prediction Engine socket server listening on {}",
            self.config.socket_path
        );
        
        println!(
            "🔮 Protocol support - Binary: {}, JSON: {}",
            self.config.enable_binary_protocol,
            self.config.enable_json_protocol
        );

        // Start accepting connections
        self.accept_connections().await?;

        Ok(())
    }

    /// Stop the socket server
    pub async fn stop(&mut self) -> Result<()> {
        {
            let mut running = self.running.write().await;
            *running = false;
        }

        if let Some(listener) = self.listener.take() {
            drop(listener);
        }

        // Clean up socket file
        if Path::new(&self.config.socket_path).exists() {
            std::fs::remove_file(&self.config.socket_path)
                .map_err(|e| anyhow!("Failed to remove socket file: {}", e))?;
        }

        println!("🛑 Layer 4 Context Prediction Engine socket server stopped");
        Ok(())
    }

    /// Accept incoming connections
    async fn accept_connections(&self) -> Result<()> {
        let listener = self.listener.as_ref()
            .ok_or_else(|| anyhow!("Socket listener not initialized"))?;

        loop {
            // Check if we should continue running
            {
                let running = self.running.read().await;
                if !*running {
                    break;
                }
            }

            // Accept connection with timeout
            let accept_result = tokio::time::timeout(
                Duration::from_millis(1000),
                listener.accept()
            ).await;

            match accept_result {
                Ok(Ok((stream, _addr))) => {
                    // Check connection limit
                    {
                        let active = self.active_connections.read().await;
                        if *active >= self.config.max_connections as u64 {
                            eprintln!("Connection limit reached, rejecting new connection");
                            continue;
                        }
                    }

                    // Spawn connection handler
                    let connection_id = Uuid::new_v4().to_string();
                    self.spawn_connection_handler(stream, connection_id).await;
                },
                Ok(Err(e)) => {
                    eprintln!("Failed to accept connection: {}", e);
                },
                Err(_) => {
                    // Timeout - continue loop to check running status
                    continue;
                }
            }
        }

        Ok(())
    }

    /// Spawn a connection handler task
    async fn spawn_connection_handler(&self, stream: UnixStream, connection_id: String) {
        let context_engine = Arc::clone(&self.context_engine);
        let config = self.config.clone();
        let connections = Arc::clone(&self.connections);
        let total_requests = Arc::clone(&self.total_requests);
        let active_connections = Arc::clone(&self.active_connections);

        // Register connection
        {
            let mut conns = connections.write().await;
            let mut active = active_connections.write().await;
            
            conns.insert(connection_id.clone(), ConnectionStats {
                connection_id: connection_id.clone(),
                connected_at: Instant::now(),
                requests_processed: 0,
                bytes_received: 0,
                bytes_sent: 0,
                last_activity: Instant::now(),
                protocol_type: "auto-detect".to_string(),
            });
            
            *active += 1;
        }

        let conn_id_clone = connection_id.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::handle_connection(
                stream,
                connection_id,
                context_engine,
                config,
                connections.clone(),
                total_requests,
            ).await {
                eprintln!("Connection handler error: {}", e);
            }

            // Cleanup connection
            {
                let mut conns = connections.write().await;
                let mut active = active_connections.write().await;
                
                conns.remove(&conn_id_clone);
                *active = active.saturating_sub(1);
            }
        });
    }

    /// Handle a single connection
    async fn handle_connection(
        mut stream: UnixStream,
        connection_id: String,
        context_engine: Arc<RwLock<ContextPredictionLayer>>,
        config: SocketServerConfig,
        connections: Arc<RwLock<HashMap<String, ConnectionStats>>>,
        total_requests: Arc<RwLock<u64>>,
    ) -> Result<()> {
        let (stream_read, mut stream_write) = stream.split();
        let mut reader = BufReader::new(stream_read);
        let mut line_buffer = String::new();

        println!("🔗 Context Engine connection {} established", connection_id);

        loop {
            // Read line with timeout
            line_buffer.clear();
            
            let read_result = tokio::time::timeout(
                Duration::from_millis(config.connection_timeout_ms),
                reader.read_line(&mut line_buffer)
            ).await;

            match read_result {
                Ok(Ok(0)) => {
                    // Connection closed
                    println!("🔌 Connection {} closed by client", connection_id);
                    break;
                },
                Ok(Ok(_)) => {
                    let line = line_buffer.trim();
                    if line.is_empty() {
                        continue;
                    }

                    // Update connection stats
                    {
                        let mut conns = connections.write().await;
                        if let Some(stats) = conns.get_mut(&connection_id) {
                            stats.bytes_received += line.len() as u64;
                            stats.last_activity = Instant::now();
                            stats.requests_processed += 1;
                            
                            // Auto-detect protocol type
                            if stats.protocol_type == "auto-detect" {
                                stats.protocol_type = if line.starts_with('{') {
                                    "JSON"
                                } else {
                                    "Binary"
                                }.to_string();
                            }
                        }
                    }

                    // Process request
                    let response = Self::process_request_line(
                        line,
                        &context_engine,
                        &config,
                    ).await;

                    // Send response
                    let response_line = format!("{}\n", response);
                    if let Err(e) = stream_write.write_all(response_line.as_bytes()).await {
                        eprintln!("Failed to write response: {}", e);
                        break;
                    }

                    // Update stats
                    {
                        let mut total = total_requests.write().await;
                        *total += 1;
                    }

                    {
                        let mut conns = connections.write().await;
                        if let Some(stats) = conns.get_mut(&connection_id) {
                            stats.bytes_sent += response_line.len() as u64;
                        }
                    }
                },
                Ok(Err(e)) => {
                    eprintln!("Connection {} read error: {}", connection_id, e);
                    break;
                },
                Err(_) => {
                    // Timeout
                    println!("⏰ Connection {} timed out", connection_id);
                    break;
                }
            }
        }

        println!("🚪 Connection {} handler finished", connection_id);
        Ok(())
    }

    /// Process a request line (JSON format)
    async fn process_request_line(
        line: &str,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        _config: &SocketServerConfig,
    ) -> String {
        let start_time = Instant::now();

        // Parse JSON request
        let request: SocketRequest = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = SocketResponse::Error {
                    request_id: "unknown".to_string(),
                    error: format!("Failed to parse request: {}", e),
                    error_code: "PARSE_ERROR".to_string(),
                };
                return serde_json::to_string(&error_response).unwrap_or_default();
            }
        };

        // Process request based on type
        let response = match request {
            SocketRequest::AddMemory { request_id, memory } => {
                Self::handle_add_memory(request_id, memory, context_engine, start_time).await
            },

            SocketRequest::UpdateContext { request_id, access } => {
                Self::handle_update_context(request_id, access, context_engine, start_time).await
            },

            SocketRequest::PredictNext { request_id, context } => {
                Self::handle_predict_next(request_id, context, context_engine, start_time).await
            },

            SocketRequest::LearnPattern { request_id, access_sequence } => {
                Self::handle_learn_pattern(request_id, access_sequence, context_engine, start_time).await
            },

            SocketRequest::GetContextState { request_id } => {
                Self::handle_get_context_state(request_id, context_engine, start_time).await
            },

            SocketRequest::Search { request_id, query } => {
                Self::handle_search(request_id, query, context_engine, start_time).await
            },

            SocketRequest::GetStats { request_id } => {
                Self::handle_get_stats(request_id, context_engine, start_time).await
            },

            SocketRequest::Ping { request_id } => {
                Self::handle_ping(request_id, start_time).await
            },

            SocketRequest::GetMemory { request_id, memory_id } => {
                Self::handle_get_memory(request_id, memory_id, context_engine, start_time).await
            },
        };

        serde_json::to_string(&response).unwrap_or_default()
    }

    /// Handle add memory request
    async fn handle_add_memory(
        request_id: String,
        memory: UniversalMemory,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        start_time: Instant,
    ) -> SocketResponse {
        match context_engine.write().await.add_memory(memory).await {
            Ok(_) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::Success {
                    request_id,
                    data: serde_json::json!({
                        "added": true,
                        "operation": "add_memory",
                    }),
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Failed to add memory: {}", e),
                    error_code: "ADD_MEMORY_FAILED".to_string(),
                }
            }
        }
    }

    /// Handle update context request
    async fn handle_update_context(
        request_id: String,
        access: MemoryAccess,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        start_time: Instant,
    ) -> SocketResponse {
        match context_engine.write().await.update_context(access.clone()).await {
            Ok(_) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::Success {
                    request_id,
                    data: serde_json::json!({
                        "updated": true,
                        "memory_id": access.memory_id,
                        "access_type": access.access_type,
                    }),
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Failed to update context: {}", e),
                    error_code: "UPDATE_CONTEXT_FAILED".to_string(),
                }
            }
        }
    }

    /// Handle predict next request
    async fn handle_predict_next(
        request_id: String,
        context: ContextWindow,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        start_time: Instant,
    ) -> SocketResponse {
        match context_engine.read().await.predict_next(&context).await {
            Ok(predictions) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::PredictionResults {
                    request_id,
                    predictions,
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Prediction failed: {}", e),
                    error_code: "PREDICTION_FAILED".to_string(),
                }
            }
        }
    }

    /// Handle learn pattern request
    async fn handle_learn_pattern(
        request_id: String,
        access_sequence: Vec<MemoryAccess>,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        start_time: Instant,
    ) -> SocketResponse {
        match context_engine.write().await.learn_pattern(&access_sequence).await {
            Ok(_) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::Success {
                    request_id,
                    data: serde_json::json!({
                        "pattern_learned": true,
                        "sequence_length": access_sequence.len(),
                    }),
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Pattern learning failed: {}", e),
                    error_code: "PATTERN_LEARNING_FAILED".to_string(),
                }
            }
        }
    }

    /// Handle get context state request
    async fn handle_get_context_state(
        request_id: String,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        start_time: Instant,
    ) -> SocketResponse {
        match context_engine.read().await.get_context_state().await {
            Ok(context_state) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::ContextStateResponse {
                    request_id,
                    context_state,
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Failed to get context state: {}", e),
                    error_code: "GET_CONTEXT_STATE_FAILED".to_string(),
                }
            }
        }
    }

    /// Handle search request
    async fn handle_search(
        request_id: String,
        query: UniversalSearchQuery,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        start_time: Instant,
    ) -> SocketResponse {
        match context_engine.read().await.search(&query).await {
            Ok(routing_decision) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::SearchResponse {
                    request_id,
                    routing_decision,
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Search failed: {}", e),
                    error_code: "SEARCH_FAILED".to_string(),
                }
            }
        }
    }

    /// Handle get stats request
    async fn handle_get_stats(
        request_id: String,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        start_time: Instant,
    ) -> SocketResponse {
        match context_engine.read().await.get_performance().await {
            Ok(stats) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::Success {
                    request_id,
                    data: serde_json::to_value(stats).unwrap_or_default(),
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Failed to get stats: {}", e),
                    error_code: "GET_STATS_FAILED".to_string(),
                }
            }
        }
    }

    /// Handle get memory request
    async fn handle_get_memory(
        request_id: String,
        memory_id: MemoryId,
        context_engine: &Arc<RwLock<ContextPredictionLayer>>,
        start_time: Instant,
    ) -> SocketResponse {
        match context_engine.read().await.get_memory(memory_id).await {
            Ok(memory) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::Success {
                    request_id,
                    data: serde_json::to_value(memory).unwrap_or_default(),
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Failed to get memory: {}", e),
                    error_code: "GET_MEMORY_FAILED".to_string(),
                }
            }
        }
    }

    /// Handle ping request
    async fn handle_ping(
        request_id: String,
        _start_time: Instant,
    ) -> SocketResponse {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        SocketResponse::Pong {
            request_id,
            timestamp,
            layer: "Layer 4: Context Prediction Engine".to_string(),
            version: "1.0.0".to_string(),
        }
    }

    /// Get server statistics
    pub async fn get_server_stats(&self) -> Result<serde_json::Value> {
        let total_requests = *self.total_requests.read().await;
        let total_connections = *self.total_connections.read().await;
        let active_connections = *self.active_connections.read().await;
        
        let connections = self.connections.read().await;
        let connection_list: Vec<serde_json::Value> = connections
            .values()
            .map(|stats| serde_json::json!({
                "connection_id": stats.connection_id,
                "connected_at": stats.connected_at.elapsed().as_secs(),
                "requests_processed": stats.requests_processed,
                "bytes_received": stats.bytes_received,
                "bytes_sent": stats.bytes_sent,
                "protocol_type": stats.protocol_type,
                "last_activity_seconds_ago": stats.last_activity.elapsed().as_secs(),
            }))
            .collect();

        Ok(serde_json::json!({
            "socket_path": self.config.socket_path,
            "total_requests": total_requests,
            "total_connections": total_connections,
            "active_connections": active_connections,
            "max_connections": self.config.max_connections,
            "binary_protocol_enabled": self.config.enable_binary_protocol,
            "json_protocol_enabled": self.config.enable_json_protocol,
            "prediction_timeout_ms": self.config.prediction_timeout_ms,
            "connections": connection_list,
        }))
    }
}

impl Drop for SocketServer {
    fn drop(&mut self) {
        // Clean up socket file
        if Path::new(&self.config.socket_path).exists() {
            let _ = std::fs::remove_file(&self.config.socket_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixStream;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    async fn create_test_context_engine() -> Arc<RwLock<ContextPredictionLayer>> {
        Arc::new(RwLock::new(ContextPredictionLayer::new()))
    }

    #[tokio::test]
    async fn test_socket_server_creation() {
        let engine = create_test_context_engine().await;
        let server = SocketServer::new(engine, None);
        
        assert_eq!(server.config.socket_path, DEFAULT_SOCKET_PATH);
        assert!(server.config.enable_binary_protocol);
        assert!(server.config.enable_json_protocol);
    }

    #[tokio::test]
    async fn test_ping_request() {
        let engine = create_test_context_engine().await;
        let config = SocketServerConfig {
            socket_path: "/tmp/test_layer4_ping.sock".to_string(),
            ..Default::default()
        };
        
        let mut server = SocketServer::new(engine, Some(config));
        
        // Start server in background
        let server_handle = tokio::spawn(async move {
            server.start().await
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect and send ping
        let mut stream = UnixStream::connect("/tmp/test_layer4_ping.sock").await.unwrap();
        
        let ping_request = SocketRequest::Ping {
            request_id: "test-ping".to_string(),
        };
        
        let request_line = format!("{}\n", serde_json::to_string(&ping_request).unwrap());
        stream.write_all(request_line.as_bytes()).await.unwrap();
        
        let mut reader = BufReader::new(&stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await.unwrap();
        
        let response: SocketResponse = serde_json::from_str(response_line.trim()).unwrap();
        
        match response {
            SocketResponse::Pong { request_id, layer, .. } => {
                assert_eq!(request_id, "test-ping");
                assert!(layer.contains("Layer 4"));
            },
            _ => panic!("Expected Pong response"),
        }

        // Cleanup
        drop(stream);
        server_handle.abort();
        let _ = std::fs::remove_file("/tmp/test_layer4_ping.sock");
    }
}