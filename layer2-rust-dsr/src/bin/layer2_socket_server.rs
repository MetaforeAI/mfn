///! Layer 2 DSR Socket Server with Multi-Pool Support
///! High-performance Unix socket server for Dynamic Similarity Reservoir

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

    println!("🧠 Starting Layer 2 DSR Socket Server (Multi-Pool)");
    println!("🎯 Target: <2ms similarity search latency");
    println!("🔗 Socket: /tmp/mfn_discord_layer2.sock");
    println!("🗂️  Multi-Pool: Enabled (client-side pool selection)");

    // Create pool manager
    let data_dir = PathBuf::from("/usr/lib/neotec/telos/mfn/memory/layer2_dsr");
    std::fs::create_dir_all(&data_dir)?;

    let config = DSRConfig::default();
    let pool_manager = Arc::new(PoolManager::new(data_dir.clone(), config));

    println!("💾 Pool directory: {}", data_dir.display());
    println!("📋 Pools will be created on-demand");

    // Create server configuration
    let server_config = SocketServerConfig {
        socket_path: "/tmp/mfn_discord_layer2.sock".to_string(),
        max_connections: 200,
        connection_timeout_ms: 30000,
        enable_binary_protocol: true,
        enable_json_protocol: true,
        buffer_size: 4096,
    };

    // Create socket server with pool manager
    let mut server = SocketServer::new_with_pool_manager(pool_manager, Some(server_config));

    println!("✅ Server ready! Press Ctrl+C to shutdown gracefully");
    println!("🧠 Layer 2 DSR socket server listening on /tmp/mfn_discord_layer2.sock");
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
