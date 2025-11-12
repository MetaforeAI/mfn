//! Layer 2 DSR Socket Server
//! High-performance Unix socket server for Dynamic Similarity Reservoir

use std::sync::Arc;
use mfn_layer2_dsr::{DynamicSimilarityReservoir, DSRConfig, socket_server::{SocketServer, SocketServerConfig}};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🧠 Starting Layer 2 DSR Socket Server");
    println!("🎯 Target: <2ms similarity search latency");
    println!("🔗 Socket: /tmp/mfn_layer2.sock");

    // Create DSR instance with config
    let config = DSRConfig::default();
    let dsr = Arc::new(DynamicSimilarityReservoir::new(config)?);

    // Create server configuration
    let config = SocketServerConfig {
        socket_path: "/tmp/mfn_layer2.sock".to_string(),
        max_connections: 200, // Increased for high-concurrency stress tests
        connection_timeout_ms: 30000,
        enable_binary_protocol: true,
        enable_json_protocol: true,
        buffer_size: 4096,
    };

    // Create socket server
    let mut server = SocketServer::new(dsr, Some(config));
    
    println!("✅ Server ready! Press Ctrl+C to shutdown gracefully");
    println!("🧠 Layer 2 DSR socket server listening on /tmp/mfn_layer2.sock");
    println!("🔮 Protocol support - Binary: true, JSON: true");

    // Handle graceful shutdown
    tokio::select! {
        result = server.start() => {
            if let Err(e) = result {
                eprintln!("❌ Server error: {}", e);
                return Err(e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n🛑 Shutdown signal received, stopping server gracefully...");
            server.stop().await?;
            println!("✅ Server stopped successfully");
        }
    }

    Ok(())
}