///! Layer 2 DSR Socket Server with Multi-Pool Support
///! High-performance Unix socket server for Dynamic Similarity Reservoir

use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use mfn_layer2_dsr::{
    DSRConfig,
    PoolManager,
    socket_server::{SocketServer, SocketServerConfig},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let socket_path = env::var("MFN_SOCKET_PATH")
        .unwrap_or_else(|_| "/tmp/mfn_crucible_layer2.sock".to_string());
    let data_dir_str = env::var("MFN_DATA_DIR")
        .unwrap_or_else(|_| "./data/mfn/memory/layer2_dsr".to_string());

    println!("Starting Layer 2 DSR Socket Server (Multi-Pool)");
    println!("Target: <2ms similarity search latency");
    println!("Socket: {}", socket_path);
    println!("Multi-Pool: Enabled (client-side pool selection)");

    // Create pool manager
    let data_dir = PathBuf::from(&data_dir_str);
    std::fs::create_dir_all(&data_dir)?;

    let mut config = DSRConfig::default();
    if let Ok(dim) = env::var("MFN_EMBEDDING_DIM") {
        if let Ok(d) = dim.parse::<usize>() {
            config.embedding_dim = d;
            println!("Embedding dimension: {} (from MFN_EMBEDDING_DIM)", d);
        }
    }
    if let Ok(size) = env::var("MFN_RESERVOIR_SIZE") {
        if let Ok(s) = size.parse::<usize>() {
            config.reservoir_size = s;
            println!("Reservoir size: {} (from MFN_RESERVOIR_SIZE)", s);
        }
    }
    println!("Config: embedding_dim={}, reservoir_size={}", config.embedding_dim, config.reservoir_size);
    let pool_manager = Arc::new(PoolManager::new(data_dir.clone(), config));

    println!("Pool directory: {}", data_dir.display());
    println!("Pools will be created on-demand");

    // Create server configuration
    let server_config = SocketServerConfig {
        socket_path: socket_path.clone(),
        max_connections: 200,
        connection_timeout_ms: 30000,
        enable_binary_protocol: true,
        enable_json_protocol: true,
        buffer_size: 4096,
    };

    // Create socket server with pool manager
    let mut server = SocketServer::new_with_pool_manager(pool_manager, Some(server_config));

    println!("Server ready! Press Ctrl+C to shutdown gracefully");
    println!("Layer 2 DSR socket server listening on {}", socket_path);
    println!("Protocol support - Binary: true, JSON: true");

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
