//! Layer 4 Context Prediction Engine - Socket Server Entry Point
//! 
//! High-performance Unix socket server for contextual memory prediction.
//! 
//! Performance Targets:
//! - Context predictions: <5.2ms latency
//! - Socket interface: /tmp/mfn_layer4.sock
//! - Concurrent connections: 100+
//! - Temporal pattern learning and prediction

use std::sync::Arc;
use std::env;
use tokio::sync::RwLock;
use tokio::signal;

mod socket_server;

use layer4_context_engine::ContextPredictionLayer;
use socket_server::{SocketServer, SocketServerConfig};
use mfn_core::MfnLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing/logging
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    
    println!("🧠 Starting Layer 4 Context Prediction Engine Socket Server");
    println!("🎯 Target: <5.2ms context prediction latency");
    println!("🔗 Socket: /tmp/mfn_layer4.sock");

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let socket_path = args.get(1)
        .map(|s| s.clone())
        .unwrap_or_else(|| "/tmp/mfn_layer4.sock".to_string());

    // Create context prediction engine
    let context_engine = Arc::new(RwLock::new(ContextPredictionLayer::new()));
    
    // Initialize and start the context engine
    {
        let mut engine = context_engine.write().await;
        let config = mfn_core::LayerConfig {
            layer_id: mfn_core::LayerId::Layer4,
            max_memory_count: Some(100_000),
            max_association_count: Some(500_000),
            default_timeout_us: 5_200, // 5.2ms target
            enable_caching: true,
            cache_size_limit: Some(10_000),
            performance_monitoring: true,
            custom_params: std::collections::HashMap::new(),
        };
        
        engine.start(config).await?;
    }

    // Create socket server configuration
    let server_config = SocketServerConfig {
        socket_path,
        max_connections: 100,
        connection_timeout_ms: 30000,
        enable_binary_protocol: true,
        enable_json_protocol: true,
        buffer_size: 8192,
        prediction_timeout_ms: 5200, // 5.2 second timeout
    };

    // Create and start socket server
    let mut server = SocketServer::new(context_engine.clone(), Some(server_config));
    
    println!("🚀 Starting context prediction socket server...");
    
    // Start server in background task
    let server_task = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("❌ Socket server error: {}", e);
        }
    });

    // Set up graceful shutdown
    println!("✅ Server ready! Press Ctrl+C to shutdown gracefully");
    
    // Wait for shutdown signal
    signal::ctrl_c().await?;
    
    println!("🛑 Received shutdown signal, stopping server...");
    
    // Shutdown the context engine
    {
        let mut engine = context_engine.write().await;
        engine.shutdown().await?;
    }
    
    // Cancel server task
    server_task.abort();
    
    println!("✅ Layer 4 Context Prediction Engine shutdown complete");
    Ok(())
}