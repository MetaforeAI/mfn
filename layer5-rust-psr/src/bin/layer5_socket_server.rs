//! Layer 5 PSR Socket Server
//! High-performance Unix socket server for Pattern Structure Registry

use std::sync::Arc;
use std::path::PathBuf;
use layer5_psr::{
    PatternRegistry,
    PersistenceConfig,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🧠 Starting Layer 5 PSR Socket Server");
    println!("🎯 Target: <1ms pattern storage, <5ms similarity search");
    println!("🔗 Socket: /tmp/mfn_discord_layer5.sock");

    // Create persistence config
    let persistence_config = PersistenceConfig {
        data_dir: PathBuf::from("/usr/lib/neotec/telos/mfn/memory/layer5_psr"),
        pool_id: "crucible_training".to_string(),
        fsync_interval_ms: 1000,
        snapshot_interval_secs: 300,
        aof_buffer_size: 64 * 1024,
    };

    // Ensure data directory exists
    std::fs::create_dir_all(&persistence_config.data_dir)?;
    println!("💾 Persistence enabled: {}", persistence_config.data_dir.display());
    println!("📝 AOF: {}", persistence_config.aof_path().display());
    println!("📸 Snapshots: {}", persistence_config.snapshot_path().display());

    // Create PSR instance with persistence
    let psr = if persistence_config.aof_path().exists() || persistence_config.snapshot_path().exists() {
        println!("🔄 Recovering from persistence...");
        Arc::new(PatternRegistry::recover_from_persistence(
            persistence_config,
        )?)
    } else {
        println!("🆕 Fresh start with persistence enabled");
        Arc::new(PatternRegistry::new_with_persistence(
            Some(persistence_config),
        )?)
    };

    println!("✅ Layer 5 PSR ready! {} patterns loaded", psr.pattern_count());
    println!("🔮 Pattern storage and similarity search enabled");

    // TODO: Implement socket server for Layer 5
    // For now, just keep the process running
    println!("\n💡 Socket server implementation pending");
    println!("   Pattern registry is initialized with persistence");
    println!("   Press Ctrl+C to shutdown gracefully");

    // Handle graceful shutdown
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n🛑 Shutdown signal received, stopping server gracefully...");
            println!("✅ Server stopped successfully");
        }
    }

    Ok(())
}
