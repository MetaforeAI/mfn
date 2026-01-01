use mfn_unified_socket::{MFNConfig, LayerRouter, BinaryProtocol, UnifiedRequest};
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 MFN Unified Socket Server Starting...");

    // Load configuration
    let config_path = std::env::var("MFN_CONFIG")
        .unwrap_or_else(|_| "/home/persist/neotec/telos/MFN/mfn-config.yaml".to_string());

    println!("📋 Loading config from: {}", config_path);
    let config = Arc::new(MFNConfig::from_file(&config_path)?);

    println!("🎯 Project: {}", config.project);
    println!("🔧 Socket prefix: {}", config.socket_prefix);

    // Initialize layer router
    let mut router = LayerRouter::new(Arc::clone(&config))?;

    // Initialize all available layers
    println!("\n🔄 Initializing layers...");

    if let Err(e) = router.init_layer2().await {
        eprintln!("⚠️  Layer 2 init failed: {}", e);
    }

    if let Err(e) = router.init_layer4().await {
        eprintln!("⚠️  Layer 4 init failed: {}", e);
    }

    if let Err(e) = router.init_layer5().await {
        eprintln!("⚠️  Layer 5 init failed: {}", e);
    }

    let router = Arc::new(router);

    println!("\n🔗 Creating socket listeners...");

    // Start socket servers for each layer
    let mut handles = vec![];

    for (layer_id, layer_config) in &config.layers {
        let socket_path = layer_config.socket_path.clone();
        let socket_path_display = socket_path.clone();
        let router_clone = Arc::clone(&router);
        let layer_id_clone = layer_id.clone();

        let handle = tokio::spawn(async move {
            if let Err(e) = run_socket_server(&socket_path, router_clone, &layer_id_clone).await {
                eprintln!("❌ Socket server error for {}: {}", layer_id_clone, e);
            }
        });

        handles.push(handle);
        println!("  ✅ {} socket: {}", layer_config.name, socket_path_display);
    }

    println!("\n✨ All layers online!");
    println!("📡 Listening for connections...");
    println!("Press Ctrl+C to shutdown\n");

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("\n🛑 Shutdown signal received, stopping...");
        }
    }

    // Cleanup sockets
    for (_, layer_config) in &config.layers {
        let _ = std::fs::remove_file(&layer_config.socket_path);
    }

    println!("✅ Shutdown complete");
    Ok(())
}

async fn run_socket_server(
    socket_path: &str,
    router: Arc<LayerRouter>,
    layer_id: &str,
) -> Result<()> {
    // Remove existing socket
    let _ = std::fs::remove_file(socket_path);

    // Create listener
    let listener = UnixListener::bind(socket_path)?;

    // Set permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o666))?;
    }

    tracing::info!("Socket server listening: {}", socket_path);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let router_clone = Arc::clone(&router);
        let layer_id = layer_id.to_string();

        tokio::spawn(async move {
            let mut read_buf = vec![0u8; 8192];
            let mut message_buf = Vec::new();

            loop {
                // Read from socket
                let n = match stream.read(&mut read_buf).await {
                    Ok(0) => break, // Connection closed
                    Ok(n) => n,
                    Err(e) => {
                        tracing::error!("Read error: {}", e);
                        break;
                    }
                };

                // Append to message buffer
                message_buf.extend_from_slice(&read_buf[..n]);

                // Try to decode binary message
                let (message, consumed) = match BinaryProtocol::decode(&message_buf) {
                    Ok(Some(msg)) => msg,
                    Ok(None) => continue, // Need more data
                    Err(e) => {
                        tracing::error!("Decode error: {}", e);
                        break;
                    }
                };

                // Remove consumed bytes from buffer
                message_buf.drain(..consumed);

                // Parse request
                let mut request: UnifiedRequest = match serde_json::from_slice(&message) {
                    Ok(req) => req,
                    Err(e) => {
                        tracing::error!("Parse error: {}", e);
                        break;
                    }
                };

                // Set target layer if not specified
                if request.target_layer.is_empty() {
                    request.target_layer = layer_id.clone();
                }

                // Route request
                let response = router_clone.route_request(request).await.unwrap_or_else(|e| {
                    mfn_unified_socket::UnifiedResponse {
                        response_type: "error".to_string(),
                        request_id: "unknown".to_string(),
                        source_layer: layer_id.clone(),
                        success: false,
                        data: None,
                        error: Some(e.to_string()),
                        processing_time_ms: 0.0,
                    }
                });

                // Encode response
                let response_data = match BinaryProtocol::encode_response(&response) {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::error!("Encode error: {}", e);
                        break;
                    }
                };

                // Write response
                if let Err(e) = stream.write_all(&response_data).await {
                    tracing::error!("Write error: {}", e);
                    break;
                }
            }
        });
    }
}
