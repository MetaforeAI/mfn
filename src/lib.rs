//! MFN Telepathy - Unified Socket Communication Library
//!
//! This library provides a high-performance, unified socket communication system
//! for the Memory Flow Network (MFN). It eliminates HTTP dependencies between
//! internal components while maintaining a clean external HTTP API.
//!
//! # Architecture
//!
//! ```text
//! External Client
//!       ↓ (HTTP)
//! API Gateway
//!       ↓ (Binary Protocol)
//! Message Router
//!    ↙  ↓  ↓  ↘ (Unix Sockets)
//! Layer1 Layer2 Layer3 Layer4
//! ```
//!
//! # Features
//!
//! - **Binary Protocol**: High-performance binary serialization with LZ4 compression
//! - **Connection Pooling**: Efficient connection reuse and management
//! - **Load Balancing**: Intelligent routing with multiple strategies
//! - **Health Monitoring**: Automatic health checks and failover
//! - **Zero-Copy**: Optimized for minimal memory allocation
//! - **SIMD Support**: Hardware acceleration for large payloads
//!
//! # Performance Targets
//!
//! - Sub-millisecond socket communication overhead
//! - <100μs serialization/deserialization for typical payloads
//! - >100k requests/second throughput per layer
//! - <500ms API endpoint response time (p99)
//!
//! # Example Usage
//!
//! ```rust
//! use mfn_telepathy::socket::{SocketClient, SocketClientConfig, MessageType};
//! use bytes::Bytes;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a client to Layer 2
//!     let config = SocketClientConfig::default();
//!     let client = SocketClient::new("/tmp/mfn_layer2.sock", config);
//!
//!     // Send a similarity search request
//!     let payload = Bytes::from(vec![/* binary payload */]);
//!     let response = client.request(MessageType::SearchSimilarity, payload).await?;
//!
//!     println!("Response: {} bytes", response.payload.len());
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

/// Socket communication module
pub mod socket;

/// HTTP API Gateway module
pub mod api_gateway;

// Re-export commonly used types
pub use socket::{
    SocketClient, SocketClientConfig,
    SocketServer, SocketServerConfig,
    MessageType, SocketMessage,
    SocketPaths, UnifiedSocketConfig,
    SocketError, SocketResult,
};

pub use api_gateway::{
    ApiGatewayConfig,
    launch_gateway,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
        assert_eq!(NAME, "mfn-telepathy");
    }
}