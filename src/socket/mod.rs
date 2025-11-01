//! Unified Socket Communication System for MFN
//!
//! Provides a high-performance, standardized socket communication layer
//! that eliminates HTTP dependencies between internal components.
//!
//! Features:
//! - Binary protocol integration with LZ4 compression
//! - Connection pooling and management
//! - Request/response correlation with timeouts
//! - SIMD optimizations for large payloads
//! - Zero-copy operations where possible
//!
//! Performance Targets:
//! - Sub-millisecond socket communication overhead
//! - <100μs serialization/deserialization for typical payloads
//! - Connection reuse and pooling for high throughput
//! - Batch message support for bulk operations

pub mod server;
pub mod client;
pub mod protocol;
pub mod pool;
pub mod router;
pub mod monitor;

pub use server::{SocketServer, SocketServerConfig};
pub use client::{SocketClient, SocketClientConfig};
pub use protocol::{SocketMessage, SocketProtocol, MessageHeader, MessageType};
pub use pool::{ConnectionPool, PoolConfig};
pub use router::{MessageRouter, RoutePattern};
pub use monitor::{SocketMonitor, ConnectionMetrics, MetricsReport};

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Standard socket paths for MFN layers
pub struct SocketPaths;

impl SocketPaths {
    pub const LAYER1_IFR: &'static str = "/tmp/mfn_layer1.sock";
    pub const LAYER2_DSR: &'static str = "/tmp/mfn_layer2.sock";
    pub const LAYER3_ALM: &'static str = "/tmp/mfn_layer3.sock";
    pub const LAYER4_CPE: &'static str = "/tmp/mfn_layer4.sock";
    pub const API_GATEWAY: &'static str = "/tmp/mfn_gateway.sock";
    pub const ORCHESTRATOR: &'static str = "/tmp/mfn_orchestrator.sock";

    /// Get socket path for a specific layer
    pub fn get_layer_socket(layer_id: u8) -> PathBuf {
        match layer_id {
            1 => PathBuf::from(Self::LAYER1_IFR),
            2 => PathBuf::from(Self::LAYER2_DSR),
            3 => PathBuf::from(Self::LAYER3_ALM),
            4 => PathBuf::from(Self::LAYER4_CPE),
            0xFF => PathBuf::from(Self::ORCHESTRATOR),
            _ => PathBuf::from(format!("/tmp/mfn_layer{}.sock", layer_id)),
        }
    }

    /// Check if all layer sockets are available
    pub fn check_socket_health() -> Vec<(u8, bool)> {
        vec![
            (1, Path::new(Self::LAYER1_IFR).exists()),
            (2, Path::new(Self::LAYER2_DSR).exists()),
            (3, Path::new(Self::LAYER3_ALM).exists()),
            (4, Path::new(Self::LAYER4_CPE).exists()),
        ]
    }
}

/// Configuration for unified socket communication
#[derive(Debug, Clone)]
pub struct UnifiedSocketConfig {
    /// Enable binary protocol for internal communication
    pub use_binary_protocol: bool,
    /// Enable LZ4 compression for large payloads
    pub enable_compression: bool,
    /// Compression threshold in bytes
    pub compression_threshold: usize,
    /// Connection timeout duration
    pub connection_timeout: Duration,
    /// Request timeout duration
    pub request_timeout: Duration,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Enable connection pooling
    pub enable_pooling: bool,
    /// Pool size per destination
    pub pool_size: usize,
    /// Enable SIMD optimizations
    pub enable_simd: bool,
    /// Enable monitoring and metrics
    pub enable_monitoring: bool,
}

impl Default for UnifiedSocketConfig {
    fn default() -> Self {
        Self {
            use_binary_protocol: true,
            enable_compression: true,
            compression_threshold: 1024,
            connection_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            max_message_size: 10 * 1024 * 1024, // 10MB
            enable_pooling: true,
            pool_size: 10,
            enable_simd: true,
            enable_monitoring: true,
        }
    }
}

/// Performance-optimized configuration
impl UnifiedSocketConfig {
    pub fn high_performance() -> Self {
        Self {
            use_binary_protocol: true,
            enable_compression: true,
            compression_threshold: 512,
            connection_timeout: Duration::from_millis(100),
            request_timeout: Duration::from_secs(5),
            max_message_size: 50 * 1024 * 1024, // 50MB
            enable_pooling: true,
            pool_size: 20,
            enable_simd: true,
            enable_monitoring: false, // Disable for max performance
        }
    }

    pub fn low_latency() -> Self {
        Self {
            use_binary_protocol: true,
            enable_compression: false, // Skip compression for lowest latency
            compression_threshold: usize::MAX,
            connection_timeout: Duration::from_millis(50),
            request_timeout: Duration::from_millis(500),
            max_message_size: 1024 * 1024, // 1MB
            enable_pooling: true,
            pool_size: 5,
            enable_simd: true,
            enable_monitoring: false,
        }
    }
}

/// Result type for socket operations
pub type SocketResult<T> = Result<T, SocketError>;

/// Socket communication errors
#[derive(Debug, thiserror::Error)]
pub enum SocketError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Timeout after {0:?}")]
    Timeout(Duration),

    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("Pool exhausted: no connections available")]
    PoolExhausted,

    #[error("Invalid layer ID: {0}")]
    InvalidLayer(u8),

    #[error("Socket not found: {0}")]
    SocketNotFound(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_paths() {
        assert_eq!(
            SocketPaths::get_layer_socket(1),
            PathBuf::from("/tmp/mfn_layer1.sock")
        );
        assert_eq!(
            SocketPaths::get_layer_socket(2),
            PathBuf::from("/tmp/mfn_layer2.sock")
        );
    }

    #[test]
    fn test_config_profiles() {
        let high_perf = UnifiedSocketConfig::high_performance();
        assert!(high_perf.use_binary_protocol);
        assert!(high_perf.enable_pooling);
        assert_eq!(high_perf.pool_size, 20);

        let low_latency = UnifiedSocketConfig::low_latency();
        assert!(!low_latency.enable_compression);
        assert_eq!(low_latency.request_timeout, Duration::from_millis(500));
    }
}