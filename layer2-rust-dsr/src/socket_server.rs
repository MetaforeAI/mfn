//! Unix Socket Server for Layer 2 Dynamic Similarity Reservoir
//! 
//! Provides high-performance Unix socket interface following Layer 3's proven pattern.
//! Supports both JSON (backward compatibility) and binary protocol (high performance).
//! 
//! Target Performance:
//! - Binary protocol: <1ms operation latency
//! - Socket path: /tmp/mfn_layer2.sock
//! - Concurrent connection handling
//! - Zero-downtime migration compatibility

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{info, debug, warn, error};
use uuid::Uuid;

use crate::{
    DynamicSimilarityReservoir, MemoryId, PoolManager
};

/// Default socket path for Layer 2 DSR
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/mfn_layer2.sock";

/// Socket server configuration
#[derive(Debug, Clone)]
pub struct SocketServerConfig {
    pub socket_path: String,
    pub max_connections: usize,
    pub connection_timeout_ms: u64,
    pub enable_binary_protocol: bool,
    pub enable_json_protocol: bool,
    pub buffer_size: usize,
}

impl Default for SocketServerConfig {
    fn default() -> Self {
        Self {
            socket_path: DEFAULT_SOCKET_PATH.to_string(),
            max_connections: 100,
            connection_timeout_ms: 5000,  // Reduced from 30000ms to 5000ms
            enable_binary_protocol: true,
            enable_json_protocol: true,
            buffer_size: 8192,
        }
    }
}

/// Request types supported by the socket interface
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SocketRequest {
    /// Add memory to the reservoir
    AddMemory {
        request_id: String,
        pool_id: Option<String>,
        memory_id: u64,
        embedding: Vec<f32>,
        content: String,
        tags: Option<Vec<String>>,
        metadata: Option<HashMap<String, String>>,
    },

    /// Search for similar memories
    SimilaritySearch {
        request_id: String,
        pool_id: Option<String>,
        query_embedding: Vec<f32>,
        top_k: usize,
        min_confidence: Option<f32>,
        timeout_ms: Option<u64>,
    },

    /// Get performance statistics
    GetStats {
        request_id: String,
        pool_id: Option<String>,
    },

    /// Optimize reservoir performance
    OptimizeReservoir {
        request_id: String,
        pool_id: Option<String>,
    },

    /// Health check / ping
    Ping {
        request_id: String,
    },

    /// Comprehensive health check
    HealthCheck {
        request_id: String,
    },

    /// Get memory by ID (if supported by future versions)
    GetMemory {
        request_id: String,
        pool_id: Option<String>,
        memory_id: u64,
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

    /// Pong response to ping
    Pong {
        request_id: String,
        timestamp: u64,
        layer: String,
        version: String,
    },

    /// Health check response
    HealthCheckResponse {
        request_id: String,
        status: String,
        layer: String,
        timestamp: u64,
        uptime_seconds: u64,
        metrics: serde_json::Value,
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

/// Main Unix Socket Server for Layer 2 DSR
pub struct SocketServer {
    config: SocketServerConfig,
    dsr: Option<Arc<DynamicSimilarityReservoir>>,
    pool_manager: Option<Arc<PoolManager>>,
    listener: Option<UnixListener>,
    running: Arc<RwLock<bool>>,
    connections: Arc<RwLock<HashMap<String, ConnectionStats>>>,

    // Performance metrics
    total_requests: Arc<RwLock<u64>>,
    total_connections: Arc<RwLock<u64>>,
    active_connections: Arc<RwLock<u64>>,
    start_time: Instant,
}

impl SocketServer {
    /// Create a new socket server instance with single DSR (backwards compatibility)
    pub fn new(
        dsr: Arc<DynamicSimilarityReservoir>,
        config: Option<SocketServerConfig>,
    ) -> Self {
        Self {
            config: config.unwrap_or_default(),
            dsr: Some(dsr),
            pool_manager: None,
            listener: None,
            running: Arc::new(RwLock::new(false)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            total_requests: Arc::new(RwLock::new(0)),
            total_connections: Arc::new(RwLock::new(0)),
            active_connections: Arc::new(RwLock::new(0)),
            start_time: Instant::now(),
        }
    }

    /// Create a new socket server instance with pool manager (multi-pool support)
    pub fn new_with_pool_manager(
        pool_manager: Arc<PoolManager>,
        config: Option<SocketServerConfig>,
    ) -> Self {
        Self {
            config: config.unwrap_or_default(),
            dsr: None,
            pool_manager: Some(pool_manager),
            listener: None,
            running: Arc::new(RwLock::new(false)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            total_requests: Arc::new(RwLock::new(0)),
            total_connections: Arc::new(RwLock::new(0)),
            active_connections: Arc::new(RwLock::new(0)),
            start_time: Instant::now(),
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

        info!(
            "Layer 2 DSR socket server listening on {}",
            self.config.socket_path
        );
        
        info!(
            "Protocol support - Binary: {}, JSON: {}",
            self.config.enable_binary_protocol,
            self.config.enable_json_protocol
        );

        // Start accepting connections
        self.accept_connections().await?;

        Ok(())
    }

    /// Run the socket server (combines start logic)
    pub async fn run(mut self) -> Result<()> {
        self.start().await?;
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

        info!("Layer 2 DSR socket server stopped");
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
                            warn!("Connection limit reached, rejecting new connection");
                            continue;
                        }
                    }

                    // Spawn connection handler
                    let connection_id = Uuid::new_v4().to_string();
                    self.spawn_connection_handler(stream, connection_id).await;
                },
                Ok(Err(e)) => {
                    error!("Failed to accept connection: {}", e);
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
        let dsr = self.dsr.as_ref().map(Arc::clone);
        let pool_manager = self.pool_manager.as_ref().map(Arc::clone);
        let config = self.config.clone();
        let connections = Arc::clone(&self.connections);
        let total_requests = Arc::clone(&self.total_requests);
        let active_connections = Arc::clone(&self.active_connections);
        let start_time = self.start_time;

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
                dsr,
                pool_manager,
                config,
                connections.clone(),
                total_requests,
                start_time,
            ).await {
                error!("Connection handler error: {}", e);
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
        stream: UnixStream,
        connection_id: String,
        dsr: Option<Arc<DynamicSimilarityReservoir>>,
        pool_manager: Option<Arc<PoolManager>>,
        config: SocketServerConfig,
        connections: Arc<RwLock<HashMap<String, ConnectionStats>>>,
        total_requests: Arc<RwLock<u64>>,
        server_start_time: Instant,
    ) -> Result<()> {
        use tokio::io::AsyncReadExt;

        let (mut stream_read, mut stream_write) = stream.into_split();

        debug!("Connection {} established", connection_id);

        loop {
            // Try to detect protocol by reading first 4 bytes
            let mut peek_buf = [0u8; 4];

            let read_result = tokio::time::timeout(
                Duration::from_millis(config.connection_timeout_ms),
                stream_read.read_exact(&mut peek_buf)
            ).await;

            match read_result {
                Ok(Ok(_)) => {
                    // Check if this looks like a length prefix (binary protocol)
                    // Length-prefixed protocol: 4 bytes for length (u32 LE)
                    let potential_len = u32::from_le_bytes(peek_buf);

                    // If the length seems reasonable (< 10MB), treat as binary protocol
                    if potential_len > 0 && potential_len < 10_000_000 {
                        // Binary protocol detected
                        let msg_len = potential_len as usize;

                        // Read the message payload
                        let mut msg_buf = vec![0u8; msg_len];
                        if let Err(e) = stream_read.read_exact(&mut msg_buf).await {
                            error!("Connection {} failed to read binary message: {}", connection_id, e);
                            break;
                        }

                        // Update connection stats
                        {
                            let mut conns = connections.write().await;
                            if let Some(stats) = conns.get_mut(&connection_id) {
                                stats.bytes_received += (4 + msg_len) as u64;
                                stats.last_activity = Instant::now();
                                stats.requests_processed += 1;
                                stats.protocol_type = "Binary".to_string();
                            }
                        }

                        // Parse JSON directly from bytes into SocketRequest
                        let request: SocketRequest = match serde_json::from_slice(&msg_buf) {
                            Ok(req) => req,
                            Err(e) => {
                                error!("Connection {} invalid JSON in binary message: {}", connection_id, e);
                                break;
                            }
                        };

                        // Process request with connection ID
                        let response = Self::process_request(
                            &request,
                            &dsr,
                            &pool_manager,
                            &config,
                            server_start_time,
                            Some(connection_id.clone()),
                        ).await;

                        // Send binary response: length (4 bytes) + JSON
                        let response_bytes = response.as_bytes();
                        let response_len = response_bytes.len() as u32;

                        if let Err(e) = stream_write.write_all(&response_len.to_le_bytes()).await {
                            error!("Failed to write response length: {}", e);
                            break;
                        }
                        if let Err(e) = stream_write.write_all(response_bytes).await {
                            error!("Failed to write response: {}", e);
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
                                stats.bytes_sent += (4 + response_bytes.len()) as u64;
                            }
                        }
                    } else {
                        // Doesn't look like valid length prefix
                        error!("Connection {} invalid protocol - expected length prefix", connection_id);
                        break;
                    }
                },
                Ok(Err(e)) => {
                    // Connection closed or error
                    if e.kind() != std::io::ErrorKind::UnexpectedEof {
                        error!("Connection {} read error: {}", connection_id, e);
                    } else {
                        debug!("Connection {} closed by client", connection_id);
                    }
                    break;
                },
                Err(_) => {
                    // Timeout
                    debug!("Connection {} timed out", connection_id);
                    break;
                }
            }
        }

        // Clean up wells associated with this connection
        if let Some(ref dsr) = dsr {
            if let Err(e) = dsr.cleanup_connection(&connection_id).await {
                error!("Failed to cleanup connection {}: {}", connection_id, e);
            } else {
                debug!("Cleaned up wells for connection {}", connection_id);
            }
        }

        debug!("Connection {} handler finished", connection_id);
        Ok(())
    }

    /// Helper to get pool from either single DSR or pool manager
    async fn get_pool(
        dsr: &Option<Arc<DynamicSimilarityReservoir>>,
        pool_manager: &Option<Arc<PoolManager>>,
        pool_id: &str,
    ) -> Result<Arc<DynamicSimilarityReservoir>> {
        // If single DSR mode, return it regardless of pool_id
        if let Some(ref single_dsr) = dsr {
            return Ok(Arc::clone(single_dsr));
        }

        // If pool manager mode, get or create the pool
        if let Some(ref pm) = pool_manager {
            return pm.get_or_create_pool(pool_id).await;
        }

        Err(anyhow!("No DSR or pool manager configured"))
    }

    /// Process a request and route to appropriate handler with connection context
    async fn process_request(
        request: &SocketRequest,
        dsr: &Option<Arc<DynamicSimilarityReservoir>>,
        pool_manager: &Option<Arc<PoolManager>>,
        config: &SocketServerConfig,
        server_start_time: Instant,
        connection_id: Option<String>,
    ) -> String {
        let start_time = Instant::now();

        let response = match request {
            SocketRequest::AddMemory { request_id, pool_id, memory_id, embedding, content, tags, metadata } => {
                // Get pool from pool_id (default: "crucible_training")
                let pool_id_str = pool_id.as_deref().unwrap_or("crucible_training");
                match Self::get_pool(dsr, pool_manager, pool_id_str).await {
                    Ok(pool_dsr) => {
                        Self::handle_add_memory(
                            request_id.clone(),
                            *memory_id,
                            embedding.clone(),
                            content.clone(),
                            tags.clone().unwrap_or_default(),
                            metadata.clone().unwrap_or_default(),
                            &pool_dsr,
                            start_time,
                            connection_id,
                        ).await
                    },
                    Err(e) => {
                        SocketResponse::Error {
                            request_id: request_id.clone(),
                            error: format!("Failed to get pool '{}': {}", pool_id_str, e),
                            error_code: "POOL_ACCESS_FAILED".to_string(),
                        }
                    }
                }
            },
            SocketRequest::SimilaritySearch { request_id, pool_id, query_embedding, top_k, min_confidence, timeout_ms } => {
                let pool_id_str = pool_id.as_deref().unwrap_or("crucible_training");
                match Self::get_pool(dsr, pool_manager, pool_id_str).await {
                    Ok(pool_dsr) => {
                        Self::handle_similarity_search(
                            request_id.clone(),
                            query_embedding.clone(),
                            *top_k,
                            &pool_dsr,
                            start_time,
                        ).await
                    },
                    Err(e) => {
                        SocketResponse::Error {
                            request_id: request_id.clone(),
                            error: format!("Failed to get pool '{}': {}", pool_id_str, e),
                            error_code: "POOL_ACCESS_FAILED".to_string(),
                        }
                    }
                }
            },
            SocketRequest::GetStats { request_id, pool_id } => {
                let pool_id_str = pool_id.as_deref().unwrap_or("crucible_training");
                match Self::get_pool(dsr, pool_manager, pool_id_str).await {
                    Ok(pool_dsr) => {
                        Self::handle_get_stats(request_id.clone(), &pool_dsr, start_time).await
                    },
                    Err(e) => {
                        SocketResponse::Error {
                            request_id: request_id.clone(),
                            error: format!("Failed to get pool '{}': {}", pool_id_str, e),
                            error_code: "POOL_ACCESS_FAILED".to_string(),
                        }
                    }
                }
            },
            SocketRequest::OptimizeReservoir { request_id, pool_id } => {
                let pool_id_str = pool_id.as_deref().unwrap_or("crucible_training");
                match Self::get_pool(dsr, pool_manager, pool_id_str).await {
                    Ok(pool_dsr) => {
                        Self::handle_optimize_reservoir(
                            request_id.clone(),
                            &pool_dsr,
                            start_time,
                        ).await
                    },
                    Err(e) => {
                        SocketResponse::Error {
                            request_id: request_id.clone(),
                            error: format!("Failed to get pool '{}': {}", pool_id_str, e),
                            error_code: "POOL_ACCESS_FAILED".to_string(),
                        }
                    }
                }
            },
            SocketRequest::Ping { request_id } => {
                Self::handle_ping(request_id.clone(), start_time).await
            },
            SocketRequest::HealthCheck { request_id } => {
                // Health check uses default pool if available
                match Self::get_pool(dsr, pool_manager, "crucible_training").await {
                    Ok(pool_dsr) => {
                        Self::handle_health_check(request_id.clone(), &pool_dsr, config, start_time).await
                    },
                    Err(_) => {
                        // If no pool available, return basic health check
                        SocketResponse::HealthCheckResponse {
                            request_id: request_id.clone(),
                            status: "healthy".to_string(),
                            layer: "Layer2_DSR".to_string(),
                            timestamp: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64,
                            uptime_seconds: server_start_time.elapsed().as_secs(),
                            metrics: serde_json::json!({ "no_pools": true }),
                        }
                    }
                }
            },
            SocketRequest::GetMemory { request_id, pool_id, memory_id } => {
                // Not yet implemented, return error
                SocketResponse::Error {
                    request_id: request_id.clone(),
                    error: "GetMemory not yet implemented".to_string(),
                    error_code: "NOT_IMPLEMENTED".to_string(),
                }
            },
        };

        // Serialize response to JSON
        serde_json::to_string(&response).unwrap_or_else(|e| {
            error!("Failed to serialize response: {}", e);
            format!(r#"{{"error":"Failed to serialize response: {}"}}"#, e)
        })
    }

    /// Handle add memory request with connection tracking
    async fn handle_add_memory(
        request_id: String,
        memory_id: u64,
        embedding: Vec<f32>,
        content: String,
        tags: Vec<String>,
        metadata: HashMap<String, String>,
        dsr: &Arc<DynamicSimilarityReservoir>,
        start_time: Instant,
        connection_id: Option<String>,
    ) -> SocketResponse {
        let embedding_array = ndarray::Array1::from(embedding);

        match dsr.add_memory_with_connection(
            MemoryId(memory_id),
            &embedding_array,
            content,
            connection_id,
        ).await {
            Ok(_) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::Success {
                    request_id,
                    data: serde_json::json!({
                        "memory_id": memory_id,
                        "added": true,
                        "tags_count": tags.len(),
                        "metadata_count": metadata.len(),
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

    /// Handle similarity search request
    async fn handle_similarity_search(
        request_id: String,
        query_embedding: Vec<f32>,
        top_k: usize,
        dsr: &Arc<DynamicSimilarityReservoir>,
        start_time: Instant,
    ) -> SocketResponse {
        let query_array = ndarray::Array1::from(query_embedding);

        match dsr.similarity_search(&query_array, top_k).await {
            Ok(results) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                
                let matches: Vec<serde_json::Value> = results.matches
                    .into_iter()
                    .map(|m| serde_json::json!({
                        "memory_id": m.memory_id.0,
                        "confidence": m.confidence,
                        "raw_activation": m.raw_activation,
                        "rank": m.rank,
                        "content": m.content,
                    }))
                    .collect();

                SocketResponse::Success {
                    request_id,
                    data: serde_json::json!({
                        "matches": matches,
                        "processing_time_ms": results.processing_time_ms,
                        "wells_evaluated": results.wells_evaluated,
                        "has_confident_matches": results.has_confident_matches,
                    }),
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
        dsr: &Arc<DynamicSimilarityReservoir>,
        start_time: Instant,
    ) -> SocketResponse {
        let stats = dsr.get_performance_stats().await;
        let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;

        SocketResponse::Success {
            request_id,
            data: serde_json::to_value(stats).unwrap_or_default(),
            processing_time_ms: processing_time,
        }
    }

    /// Handle optimize reservoir request
    async fn handle_optimize_reservoir(
        request_id: String,
        dsr: &Arc<DynamicSimilarityReservoir>,
        start_time: Instant,
    ) -> SocketResponse {
        match dsr.optimize_reservoir().await {
            Ok(_) => {
                let processing_time = start_time.elapsed().as_secs_f32() * 1000.0;
                SocketResponse::Success {
                    request_id,
                    data: serde_json::json!({ "optimized": true }),
                    processing_time_ms: processing_time,
                }
            },
            Err(e) => {
                SocketResponse::Error {
                    request_id,
                    error: format!("Optimization failed: {}", e),
                    error_code: "OPTIMIZATION_FAILED".to_string(),
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
            layer: "Layer 2: Dynamic Similarity Reservoir".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    /// Handle health check request
    async fn handle_health_check(
        request_id: String,
        dsr: &Arc<DynamicSimilarityReservoir>,
        config: &SocketServerConfig,
        server_start_time: Instant,
    ) -> SocketResponse {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let uptime_seconds = server_start_time.elapsed().as_secs();

        // Get DSR statistics
        let stats = dsr.get_performance_stats().await;

        // Build metrics JSON with memory management stats
        let metrics = serde_json::json!({
            "total_queries": stats.total_queries,
            "total_additions": stats.total_additions,
            "cache_hits": stats.cache_hits,
            "similarity_wells_count": stats.similarity_wells_count,
            "max_wells": stats.max_wells,
            "wells_evicted": stats.wells_evicted,
            "reservoir_size": stats.reservoir_size,
            "average_well_activation": stats.average_well_activation,
            "memory_usage_mb": stats.memory_usage_mb,
            "connection_count": stats.connection_count,
        });

        SocketResponse::HealthCheckResponse {
            request_id,
            status: "healthy".to_string(),
            layer: "Layer2_DSR".to_string(),
            timestamp,
            uptime_seconds,
            metrics,
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
    use crate::DSRConfig;
    use tokio::net::UnixStream;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    async fn create_test_dsr() -> Arc<DynamicSimilarityReservoir> {
        let mut config = DSRConfig::default();
        config.reservoir_size = 100;
        config.embedding_dim = 5;
        Arc::new(DynamicSimilarityReservoir::new(config).unwrap())
    }

    #[tokio::test]
    async fn test_socket_server_creation() {
        let dsr = create_test_dsr().await;
        let server = SocketServer::new(dsr, None);
        
        assert_eq!(server.config.socket_path, DEFAULT_SOCKET_PATH);
        assert!(server.config.enable_binary_protocol);
        assert!(server.config.enable_json_protocol);
    }

    #[tokio::test]
    async fn test_ping_request() {
        use tokio::io::AsyncReadExt;

        let dsr = create_test_dsr().await;
        let config = SocketServerConfig {
            socket_path: "/tmp/test_layer2_ping.sock".to_string(),
            ..Default::default()
        };

        let mut server = SocketServer::new(dsr, Some(config));

        // Start server in background
        let server_handle = tokio::spawn(async move {
            server.start().await
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect and send ping using binary protocol
        let mut stream = UnixStream::connect("/tmp/test_layer2_ping.sock").await.unwrap();

        let ping_request = SocketRequest::Ping {
            request_id: "test-ping".to_string(),
        };

        // Send using binary protocol: length (4 bytes) + JSON
        let request_json = serde_json::to_string(&ping_request).unwrap();
        let request_bytes = request_json.as_bytes();
        let request_len = request_bytes.len() as u32;

        stream.write_all(&request_len.to_le_bytes()).await.unwrap();
        stream.write_all(request_bytes).await.unwrap();

        // Read response using binary protocol: length (4 bytes) + JSON
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.unwrap();
        let response_len = u32::from_le_bytes(len_buf) as usize;

        let mut response_buf = vec![0u8; response_len];
        stream.read_exact(&mut response_buf).await.unwrap();

        let response: SocketResponse = serde_json::from_slice(&response_buf).unwrap();

        match response {
            SocketResponse::Pong { request_id, layer, .. } => {
                assert_eq!(request_id, "test-ping");
                assert!(layer.contains("Layer 2"));
            },
            _ => panic!("Expected Pong response"),
        }

        // Cleanup
        drop(stream);
        server_handle.abort();
        let _ = std::fs::remove_file("/tmp/test_layer2_ping.sock");
    }
}