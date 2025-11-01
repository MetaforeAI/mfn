//! Unified Socket Server Implementation
//!
//! Provides a high-performance, async socket server that all MFN layers can use.
//! Supports concurrent connections, request routing, and monitoring.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{RwLock, mpsc, Semaphore};
use tokio::time::timeout;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bytes::Bytes;
use tracing::{info, debug, warn, error, instrument};

use crate::socket::{
    SocketError, SocketResult, SocketMessage, MessageType, MessageHeader,
    SocketProtocol, ConnectionMetrics,
};

/// Socket server configuration
#[derive(Debug, Clone)]
pub struct SocketServerConfig {
    /// Unix socket path
    pub socket_path: PathBuf,
    /// Maximum concurrent connections
    pub max_connections: usize,
    /// Connection timeout duration
    pub connection_timeout: Duration,
    /// Request timeout duration
    pub request_timeout: Duration,
    /// Buffer size for reading
    pub buffer_size: usize,
    /// Enable binary protocol
    pub use_binary_protocol: bool,
    /// Enable compression
    pub enable_compression: bool,
    /// Compression threshold
    pub compression_threshold: usize,
    /// Enable connection pooling
    pub enable_pooling: bool,
    /// Enable monitoring
    pub enable_monitoring: bool,
}

impl Default for SocketServerConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/mfn_server.sock"),
            max_connections: 100,
            connection_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
            buffer_size: 64 * 1024, // 64KB
            use_binary_protocol: true,
            enable_compression: true,
            compression_threshold: 1024,
            enable_pooling: true,
            enable_monitoring: true,
        }
    }
}

/// Message handler trait that layers must implement
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle an incoming message and return a response
    async fn handle_message(
        &self,
        message: SocketMessage,
    ) -> SocketResult<SocketMessage>;

    /// Get handler metrics (optional)
    async fn get_metrics(&self) -> Option<HashMap<String, f64>> {
        None
    }

    /// Perform periodic optimization (optional)
    async fn optimize(&self) -> SocketResult<()> {
        Ok(())
    }
}

/// Unified socket server
pub struct SocketServer {
    config: SocketServerConfig,
    handler: Arc<dyn MessageHandler>,
    protocol: Arc<SocketProtocol>,
    metrics: Arc<ServerMetrics>,
    connections: Arc<Semaphore>,
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Arc<RwLock<Option<mpsc::Receiver<()>>>>,
}

impl SocketServer {
    /// Create a new socket server
    pub fn new(config: SocketServerConfig, handler: impl MessageHandler + 'static) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let connections = Arc::new(Semaphore::new(config.max_connections));
        let protocol = Arc::new(
            SocketProtocol::new().with_compression(
                config.enable_compression,
                config.compression_threshold,
            )
        );

        Self {
            config,
            handler: Arc::new(handler),
            protocol,
            metrics: Arc::new(ServerMetrics::new()),
            connections,
            shutdown_tx,
            shutdown_rx: Arc::new(RwLock::new(Some(shutdown_rx))),
        }
    }

    /// Start the server
    #[instrument(skip(self))]
    pub async fn start(&self) -> SocketResult<()> {
        // Remove existing socket file if it exists
        if self.config.socket_path.exists() {
            std::fs::remove_file(&self.config.socket_path)?;
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = self.config.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Bind to socket
        let listener = UnixListener::bind(&self.config.socket_path)?;
        info!("Socket server listening on {:?}", self.config.socket_path);

        // Take ownership of shutdown receiver
        let mut shutdown_rx = self.shutdown_rx.write().await.take()
            .ok_or_else(|| SocketError::Connection("Server already running".to_string()))?;

        // Accept connections loop
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _)) => {
                            self.handle_connection(stream);
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Server shutdown signal received");
                    break;
                }
            }
        }

        // Cleanup socket file
        let _ = std::fs::remove_file(&self.config.socket_path);
        info!("Socket server stopped");

        Ok(())
    }

    /// Handle a new connection
    fn handle_connection(&self, stream: UnixStream) {
        let handler = Arc::clone(&self.handler);
        let protocol = Arc::clone(&self.protocol);
        let metrics = Arc::clone(&self.metrics);
        let connections = Arc::clone(&self.connections);
        let config = self.config.clone();

        tokio::spawn(async move {
            // Acquire connection permit
            let _permit = match connections.try_acquire() {
                Ok(permit) => permit,
                Err(_) => {
                    warn!("Connection limit reached, rejecting connection");
                    return;
                }
            };

            metrics.increment_connections();
            let conn_start = Instant::now();

            if let Err(e) = Self::handle_connection_inner(
                stream,
                handler,
                protocol,
                metrics.clone(),
                config,
            ).await {
                error!("Connection error: {}", e);
            }

            metrics.record_connection_duration(conn_start.elapsed());
            metrics.decrement_connections();
        });
    }

    /// Inner connection handler
    async fn handle_connection_inner(
        mut stream: UnixStream,
        handler: Arc<dyn MessageHandler>,
        protocol: Arc<SocketProtocol>,
        metrics: Arc<ServerMetrics>,
        config: SocketServerConfig,
    ) -> SocketResult<()> {
        debug!("New connection established");

        loop {
            // Set connection timeout
            let read_future = timeout(
                config.connection_timeout,
                protocol.read_message(&mut stream),
            );

            match read_future.await {
                Ok(Ok(request)) => {
                    let request_start = Instant::now();
                    metrics.increment_requests();

                    // Handle the message with request timeout
                    let handle_future = timeout(
                        config.request_timeout,
                        handler.handle_message(request),
                    );

                    match handle_future.await {
                        Ok(Ok(response)) => {
                            // Send response
                            if let Err(e) = protocol.write_message(&mut stream, &response).await {
                                error!("Failed to send response: {}", e);
                                break;
                            }
                            metrics.record_request_duration(request_start.elapsed());
                        }
                        Ok(Err(e)) => {
                            error!("Handler error: {}", e);
                            // Send error response
                            let error_msg = SocketMessage::new(
                                MessageType::Error,
                                0, // correlation_id should match request
                                Bytes::from(format!("Handler error: {}", e)),
                            );
                            let _ = protocol.write_message(&mut stream, &error_msg).await;
                            metrics.increment_errors();
                        }
                        Err(_) => {
                            warn!("Request timeout");
                            metrics.increment_timeouts();
                            break;
                        }
                    }
                }
                Ok(Err(e)) => {
                    if !matches!(e, SocketError::Io(_)) {
                        error!("Failed to read message: {}", e);
                    }
                    break;
                }
                Err(_) => {
                    debug!("Connection timeout");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Shutdown the server
    pub async fn shutdown(&self) -> SocketResult<()> {
        self.shutdown_tx.send(()).await
            .map_err(|_| SocketError::Connection("Failed to send shutdown signal".to_string()))?;
        Ok(())
    }

    /// Get server metrics
    pub fn metrics(&self) -> &ServerMetrics {
        &self.metrics
    }
}

/// Server metrics tracking
pub struct ServerMetrics {
    total_connections: AtomicU64,
    active_connections: AtomicUsize,
    total_requests: AtomicU64,
    total_errors: AtomicU64,
    total_timeouts: AtomicU64,
    total_bytes_sent: AtomicU64,
    total_bytes_received: AtomicU64,
    request_durations: RwLock<Vec<Duration>>,
    connection_durations: RwLock<Vec<Duration>>,
}

impl ServerMetrics {
    pub fn new() -> Self {
        Self {
            total_connections: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            total_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            total_timeouts: AtomicU64::new(0),
            total_bytes_sent: AtomicU64::new(0),
            total_bytes_received: AtomicU64::new(0),
            request_durations: RwLock::new(Vec::new()),
            connection_durations: RwLock::new(Vec::new()),
        }
    }

    pub fn increment_connections(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn increment_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_errors(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_timeouts(&self) {
        self.total_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn record_request_duration(&self, duration: Duration) {
        let mut durations = self.request_durations.write().await;
        durations.push(duration);
        if durations.len() > 1000 {
            durations.drain(0..500); // Keep last 500
        }
    }

    pub async fn record_connection_duration(&self, duration: Duration) {
        let mut durations = self.connection_durations.write().await;
        durations.push(duration);
        if durations.len() > 1000 {
            durations.drain(0..500);
        }
    }

    pub async fn get_stats(&self) -> HashMap<String, f64> {
        let mut stats = HashMap::new();

        stats.insert("total_connections".to_string(),
            self.total_connections.load(Ordering::Relaxed) as f64);
        stats.insert("active_connections".to_string(),
            self.active_connections.load(Ordering::Relaxed) as f64);
        stats.insert("total_requests".to_string(),
            self.total_requests.load(Ordering::Relaxed) as f64);
        stats.insert("total_errors".to_string(),
            self.total_errors.load(Ordering::Relaxed) as f64);
        stats.insert("total_timeouts".to_string(),
            self.total_timeouts.load(Ordering::Relaxed) as f64);

        // Calculate average request duration
        let request_durations = self.request_durations.read().await;
        if !request_durations.is_empty() {
            let avg_duration = request_durations.iter()
                .map(|d| d.as_secs_f64())
                .sum::<f64>() / request_durations.len() as f64;
            stats.insert("avg_request_duration_ms".to_string(), avg_duration * 1000.0);
        }

        // Calculate average connection duration
        let connection_durations = self.connection_durations.read().await;
        if !connection_durations.is_empty() {
            let avg_duration = connection_durations.iter()
                .map(|d| d.as_secs_f64())
                .sum::<f64>() / connection_durations.len() as f64;
            stats.insert("avg_connection_duration_s".to_string(), avg_duration);
        }

        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler;

    #[async_trait::async_trait]
    impl MessageHandler for TestHandler {
        async fn handle_message(&self, message: SocketMessage) -> SocketResult<SocketMessage> {
            // Echo the message back
            Ok(SocketMessage::new(
                MessageType::Success,
                message.header.correlation_id,
                message.payload,
            ))
        }
    }

    #[tokio::test]
    async fn test_server_lifecycle() {
        let config = SocketServerConfig {
            socket_path: PathBuf::from("/tmp/test_mfn_server.sock"),
            ..Default::default()
        };

        let server = SocketServer::new(config, TestHandler);

        // Start server in background
        let server_handle = tokio::spawn(async move {
            server.start().await
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Server should be running
        assert!(Path::new("/tmp/test_mfn_server.sock").exists());

        // Cleanup
        server_handle.abort();
        let _ = std::fs::remove_file("/tmp/test_mfn_server.sock");
    }
}