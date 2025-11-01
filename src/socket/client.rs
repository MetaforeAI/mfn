//! Unified Socket Client Implementation
//!
//! Provides a high-performance client for connecting to MFN socket servers.
//! Features connection pooling, automatic retries, and load balancing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::net::UnixStream;
use tokio::sync::{RwLock, Semaphore, mpsc};
use tokio::time::{timeout, sleep};
use bytes::Bytes;
use tracing::{debug, warn, error, instrument};

use crate::socket::{
    SocketError, SocketResult, SocketMessage, MessageType,
    SocketProtocol, ConnectionPool,
};

/// Socket client configuration
#[derive(Debug, Clone)]
pub struct SocketClientConfig {
    /// Connection timeout duration
    pub connection_timeout: Duration,
    /// Request timeout duration
    pub request_timeout: Duration,
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Retry delay duration
    pub retry_delay: Duration,
    /// Enable connection pooling
    pub enable_pooling: bool,
    /// Pool size (if pooling enabled)
    pub pool_size: usize,
    /// Enable compression
    pub enable_compression: bool,
    /// Compression threshold
    pub compression_threshold: usize,
    /// Enable automatic reconnection
    pub auto_reconnect: bool,
}

impl Default for SocketClientConfig {
    fn default() -> Self {
        Self {
            connection_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(10),
            max_retries: 3,
            retry_delay: Duration::from_millis(100),
            enable_pooling: true,
            pool_size: 5,
            enable_compression: true,
            compression_threshold: 1024,
            auto_reconnect: true,
        }
    }
}

/// Unified socket client
pub struct SocketClient {
    socket_path: PathBuf,
    config: SocketClientConfig,
    protocol: Arc<SocketProtocol>,
    pool: Option<Arc<ConnectionPool>>,
    metrics: Arc<ClientMetrics>,
    correlation_counter: AtomicU64,
}

impl SocketClient {
    /// Create a new socket client
    pub fn new<P: AsRef<Path>>(socket_path: P, config: SocketClientConfig) -> Self {
        let protocol = Arc::new(
            SocketProtocol::new().with_compression(
                config.enable_compression,
                config.compression_threshold,
            )
        );

        let pool = if config.enable_pooling {
            Some(Arc::new(ConnectionPool::new(
                socket_path.as_ref().to_path_buf(),
                config.pool_size,
                config.connection_timeout,
            )))
        } else {
            None
        };

        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
            config,
            protocol,
            pool,
            metrics: Arc::new(ClientMetrics::new()),
            correlation_counter: AtomicU64::new(1),
        }
    }

    /// Connect to server (for non-pooled connections)
    async fn connect(&self) -> SocketResult<UnixStream> {
        timeout(
            self.config.connection_timeout,
            UnixStream::connect(&self.socket_path),
        )
        .await
        .map_err(|_| SocketError::Timeout(self.config.connection_timeout))?
        .map_err(|e| e.into())
    }

    /// Send a request and wait for response
    #[instrument(skip(self, payload))]
    pub async fn request(
        &self,
        msg_type: MessageType,
        payload: Bytes,
    ) -> SocketResult<SocketMessage> {
        let correlation_id = self.correlation_counter.fetch_add(1, Ordering::Relaxed);
        let message = SocketMessage::new(msg_type, correlation_id, payload);

        self.request_with_retries(message, self.config.max_retries).await
    }

    /// Send a request with explicit correlation ID
    pub async fn request_with_id(
        &self,
        msg_type: MessageType,
        correlation_id: u64,
        payload: Bytes,
    ) -> SocketResult<SocketMessage> {
        let message = SocketMessage::new(msg_type, correlation_id, payload);
        self.request_with_retries(message, self.config.max_retries).await
    }

    /// Internal request with retry logic
    async fn request_with_retries(
        &self,
        message: SocketMessage,
        retries_left: u32,
    ) -> SocketResult<SocketMessage> {
        let start = Instant::now();
        self.metrics.increment_requests();

        for attempt in 0..=retries_left {
            if attempt > 0 {
                debug!("Retry attempt {} of {}", attempt, retries_left);
                sleep(self.config.retry_delay).await;
            }

            match self.send_request_internal(&message).await {
                Ok(response) => {
                    self.metrics.record_request_duration(start.elapsed());
                    return Ok(response);
                }
                Err(e) if attempt < retries_left => {
                    warn!("Request failed, will retry: {}", e);
                    self.metrics.increment_retries();
                    continue;
                }
                Err(e) => {
                    error!("Request failed after {} attempts: {}", attempt + 1, e);
                    self.metrics.increment_errors();
                    return Err(e);
                }
            }
        }

        unreachable!()
    }

    /// Internal request sending
    async fn send_request_internal(
        &self,
        message: &SocketMessage,
    ) -> SocketResult<SocketMessage> {
        // Get connection from pool or create new one
        let mut stream = if let Some(pool) = &self.pool {
            pool.get().await?
        } else {
            Box::new(self.connect().await?)
        };

        // Send request
        timeout(
            self.config.request_timeout,
            self.protocol.write_message(&mut *stream, message),
        )
        .await
        .map_err(|_| SocketError::Timeout(self.config.request_timeout))??;

        // Read response
        let response = timeout(
            self.config.request_timeout,
            self.protocol.read_message(&mut *stream),
        )
        .await
        .map_err(|_| SocketError::Timeout(self.config.request_timeout))??;

        // Validate correlation ID
        let response_corr_id = response.header.correlation_id;
        let message_corr_id = message.header.correlation_id;
        if response_corr_id != message_corr_id {
            return Err(SocketError::Protocol(format!(
                "Correlation ID mismatch: expected {}, got {}",
                message_corr_id,
                response_corr_id
            )));
        }

        // Return connection to pool if applicable
        if let Some(pool) = &self.pool {
            pool.return_connection(stream).await;
        }

        Ok(response)
    }

    /// Send a one-way message (no response expected)
    pub async fn send(&self, msg_type: MessageType, payload: Bytes) -> SocketResult<()> {
        let correlation_id = self.correlation_counter.fetch_add(1, Ordering::Relaxed);
        let message = SocketMessage::new(msg_type, correlation_id, payload)
            .with_flags(0x0008); // NoReply flag

        let mut stream = if let Some(pool) = &self.pool {
            pool.get().await?
        } else {
            Box::new(self.connect().await?)
        };

        self.protocol.write_message(&mut *stream, &message).await?;

        if let Some(pool) = &self.pool {
            pool.return_connection(stream).await;
        }

        Ok(())
    }

    /// Batch send multiple requests
    pub async fn batch_request(
        &self,
        requests: Vec<(MessageType, Bytes)>,
    ) -> SocketResult<Vec<SocketMessage>> {
        let mut responses = Vec::with_capacity(requests.len());
        let mut stream = if let Some(pool) = &self.pool {
            pool.get().await?
        } else {
            Box::new(self.connect().await?)
        };

        for (msg_type, payload) in requests {
            let correlation_id = self.correlation_counter.fetch_add(1, Ordering::Relaxed);
            let message = SocketMessage::new(msg_type, correlation_id, payload);

            self.protocol.write_message(&mut *stream, &message).await?;
            let response = self.protocol.read_message(&mut *stream).await?;

            let response_corr_id = response.header.correlation_id;
            if response_corr_id != correlation_id {
                return Err(SocketError::Protocol(
                    "Batch correlation ID mismatch".to_string()
                ));
            }

            responses.push(response);
        }

        if let Some(pool) = &self.pool {
            pool.return_connection(stream).await;
        }

        Ok(responses)
    }

    /// Check if server is reachable
    pub async fn ping(&self) -> SocketResult<Duration> {
        let start = Instant::now();
        let response = self.request(
            MessageType::Ping,
            Bytes::from("ping"),
        ).await?;

        let msg_type = response.header.msg_type;
        if !matches!(msg_type, x if x == MessageType::Success as u16) {
            return Err(SocketError::Protocol("Invalid ping response".to_string()));
        }

        Ok(start.elapsed())
    }

    /// Get client metrics
    pub fn metrics(&self) -> &ClientMetrics {
        &self.metrics
    }

    /// Close all connections (if pooled)
    pub async fn close(&self) {
        if let Some(pool) = &self.pool {
            pool.close_all().await;
        }
    }
}

/// Client metrics tracking
pub struct ClientMetrics {
    total_requests: AtomicU64,
    total_errors: AtomicU64,
    total_retries: AtomicU64,
    total_timeouts: AtomicU64,
    request_durations: RwLock<Vec<Duration>>,
}

impl ClientMetrics {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            total_retries: AtomicU64::new(0),
            total_timeouts: AtomicU64::new(0),
            request_durations: RwLock::new(Vec::new()),
        }
    }

    pub fn increment_requests(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_errors(&self) {
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_retries(&self) {
        self.total_retries.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_timeouts(&self) {
        self.total_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn record_request_duration(&self, duration: Duration) {
        let mut durations = self.request_durations.write().await;
        durations.push(duration);
        if durations.len() > 1000 {
            durations.drain(0..500);
        }
    }

    pub async fn get_stats(&self) -> HashMap<String, f64> {
        let mut stats = HashMap::new();

        stats.insert("total_requests".to_string(),
            self.total_requests.load(Ordering::Relaxed) as f64);
        stats.insert("total_errors".to_string(),
            self.total_errors.load(Ordering::Relaxed) as f64);
        stats.insert("total_retries".to_string(),
            self.total_retries.load(Ordering::Relaxed) as f64);
        stats.insert("total_timeouts".to_string(),
            self.total_timeouts.load(Ordering::Relaxed) as f64);

        let request_durations = self.request_durations.read().await;
        if !request_durations.is_empty() {
            let avg_duration = request_durations.iter()
                .map(|d| d.as_secs_f64())
                .sum::<f64>() / request_durations.len() as f64;
            stats.insert("avg_request_duration_ms".to_string(), avg_duration * 1000.0);

            // Calculate percentiles
            let mut sorted_durations: Vec<f64> = request_durations.iter()
                .map(|d| d.as_secs_f64() * 1000.0)
                .collect();
            sorted_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let p50_idx = sorted_durations.len() / 2;
            let p95_idx = (sorted_durations.len() as f64 * 0.95) as usize;
            let p99_idx = (sorted_durations.len() as f64 * 0.99) as usize;

            stats.insert("p50_duration_ms".to_string(), sorted_durations[p50_idx]);
            if p95_idx < sorted_durations.len() {
                stats.insert("p95_duration_ms".to_string(), sorted_durations[p95_idx]);
            }
            if p99_idx < sorted_durations.len() {
                stats.insert("p99_duration_ms".to_string(), sorted_durations[p99_idx]);
            }
        }

        stats
    }
}

/// Multi-endpoint client for connecting to multiple servers
pub struct MultiClient {
    clients: HashMap<String, Arc<SocketClient>>,
    default_config: SocketClientConfig,
}

impl MultiClient {
    pub fn new(default_config: SocketClientConfig) -> Self {
        Self {
            clients: HashMap::new(),
            default_config,
        }
    }

    /// Add a client for a specific endpoint
    pub fn add_endpoint(&mut self, name: String, socket_path: PathBuf) {
        let client = Arc::new(SocketClient::new(socket_path, self.default_config.clone()));
        self.clients.insert(name, client);
    }

    /// Get a client by name
    pub fn get(&self, name: &str) -> Option<&Arc<SocketClient>> {
        self.clients.get(name)
    }

    /// Send request to specific endpoint
    pub async fn request(
        &self,
        endpoint: &str,
        msg_type: MessageType,
        payload: Bytes,
    ) -> SocketResult<SocketMessage> {
        self.get(endpoint)
            .ok_or_else(|| SocketError::SocketNotFound(endpoint.to_string()))?
            .request(msg_type, payload)
            .await
    }

    /// Broadcast request to all endpoints
    pub async fn broadcast(
        &self,
        msg_type: MessageType,
        payload: Bytes,
    ) -> Vec<(String, SocketResult<SocketMessage>)> {
        let mut results = Vec::new();

        for (name, client) in &self.clients {
            let response = client.request(msg_type, payload.clone()).await;
            results.push((name.clone(), response));
        }

        results
    }

    /// Close all connections
    pub async fn close_all(&self) {
        for client in self.clients.values() {
            client.close().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let config = SocketClientConfig::default();
        let client = SocketClient::new("/tmp/test.sock", config);

        assert_eq!(client.socket_path, PathBuf::from("/tmp/test.sock"));
        assert!(client.pool.is_some());
    }

    #[tokio::test]
    async fn test_multi_client() {
        let mut multi = MultiClient::new(SocketClientConfig::default());

        multi.add_endpoint("layer1".to_string(), PathBuf::from("/tmp/mfn_layer1.sock"));
        multi.add_endpoint("layer2".to_string(), PathBuf::from("/tmp/mfn_layer2.sock"));

        assert!(multi.get("layer1").is_some());
        assert!(multi.get("layer2").is_some());
        assert!(multi.get("layer3").is_none());
    }
}