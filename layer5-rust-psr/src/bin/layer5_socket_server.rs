//! Layer 5 PSR Socket Server
//! High-performance Unix socket server for Pattern Structure Registry
//!
//! Protocol: 4-byte little-endian length prefix + JSON payload
//! Socket: configurable via SOCKET_PATH env var (default: /tmp/mfn_test_layer5.sock)

use std::sync::Arc;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use layer5_psr::{Pattern, PatternRegistry, PersistenceConfig};

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Request {
    #[serde(rename = "type")]
    request_type: String,
    #[serde(default = "default_request_id")]
    request_id: String,
    #[serde(flatten)]
    payload: serde_json::Value,
}

fn default_request_id() -> String {
    "unknown".to_string()
}

#[derive(Debug, Serialize)]
struct Response {
    #[serde(rename = "type")]
    response_type: String,
    request_id: String,
    success: bool,
    #[serde(flatten)]
    data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Connection handler
// ---------------------------------------------------------------------------

async fn handle_connection(
    stream: UnixStream,
    psr: Arc<PatternRegistry>,
    start_time: Instant,
) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();

    loop {
        // Read 4-byte length prefix
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                tracing::warn!("Read length error: {}", e);
                break;
            }
        }

        let msg_len = u32::from_le_bytes(len_buf) as usize;
        if msg_len == 0 || msg_len > 10_000_000 {
            tracing::warn!("Invalid message length: {}", msg_len);
            break;
        }

        // Read message body
        let mut msg_buf = vec![0u8; msg_len];
        if let Err(e) = reader.read_exact(&mut msg_buf).await {
            tracing::warn!("Read body error: {}", e);
            break;
        }

        let msg_str = match std::str::from_utf8(&msg_buf) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Invalid UTF-8: {}", e);
                break;
            }
        };

        // Parse JSON
        let request: Request = match serde_json::from_str(msg_str) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response {
                    response_type: "error".to_string(),
                    request_id: "unknown".to_string(),
                    success: false,
                    data: serde_json::json!({ "error": format!("Invalid JSON: {}", e) }),
                };
                send_response(&mut writer, &resp).await?;
                continue;
            }
        };

        tracing::info!("Received request: type={} id={}", request.request_type, request.request_id);

        // Dispatch
        let response = match request.request_type.as_str() {
            "AddPattern" => handle_add_pattern(&psr, &request),
            "SimilaritySearch" => handle_similarity_search(&psr, &request),
            "Synthesize" => handle_similarity_search(&psr, &request),
            "Ping" => handle_ping(&request),
            "HealthCheck" => handle_health_check(&psr, &request, start_time),
            "GetStats" => handle_get_stats(&psr, &request),
            other => Response {
                response_type: "error".to_string(),
                request_id: request.request_id.clone(),
                success: false,
                data: serde_json::json!({ "error": format!("Unknown request type: {}", other) }),
            },
        };

        send_response(&mut writer, &response).await?;
    }

    tracing::info!("Connection closed");
    Ok(())
}

async fn send_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: &Response,
) -> Result<()> {
    let json = serde_json::to_string(response)?;
    let bytes = json.as_bytes();
    let len = bytes.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(bytes).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Operation handlers
// ---------------------------------------------------------------------------

fn handle_add_pattern(psr: &PatternRegistry, request: &Request) -> Response {
    // The client sends a full Pattern object in the "pattern" field
    let pattern_value = match request.payload.get("pattern") {
        Some(v) => v,
        None => {
            return Response {
                response_type: "AddPattern_Response".to_string(),
                request_id: request.request_id.clone(),
                success: false,
                data: serde_json::json!({ "error": "Missing 'pattern' field" }),
            };
        }
    };

    let pattern: Pattern = match serde_json::from_value(pattern_value.clone()) {
        Ok(p) => p,
        Err(e) => {
            return Response {
                response_type: "AddPattern_Response".to_string(),
                request_id: request.request_id.clone(),
                success: false,
                data: serde_json::json!({ "error": format!("Invalid pattern: {}", e) }),
            };
        }
    };

    let pattern_id = pattern.id.clone();
    match psr.store_pattern(pattern) {
        Ok(id) => {
            tracing::info!("Stored pattern: {}", id);
            Response {
                response_type: "AddPattern_Response".to_string(),
                request_id: request.request_id.clone(),
                success: true,
                data: serde_json::json!({ "pattern_id": id }),
            }
        }
        Err(e) => {
            tracing::error!("Failed to store pattern {}: {}", pattern_id, e);
            Response {
                response_type: "AddPattern_Response".to_string(),
                request_id: request.request_id.clone(),
                success: false,
                data: serde_json::json!({ "error": format!("Store failed: {}", e) }),
            }
        }
    }
}

fn handle_similarity_search(psr: &PatternRegistry, request: &Request) -> Response {
    let response_type = format!("{}_Response", request.request_type);

    // Extract query embedding
    let embedding: Vec<f32> = match request.payload.get("embedding")
        .or_else(|| request.payload.get("query_pattern"))
    {
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(e) => e,
            Err(e) => {
                return Response {
                    response_type,
                    request_id: request.request_id.clone(),
                    success: false,
                    data: serde_json::json!({ "error": format!("Invalid embedding: {}", e) }),
                };
            }
        },
        None => {
            return Response {
                response_type,
                request_id: request.request_id.clone(),
                success: false,
                data: serde_json::json!({ "error": "Missing 'embedding' or 'query_pattern' field" }),
            };
        }
    };

    let top_k = request.payload.get("top_k")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    let min_confidence = request.payload.get("min_confidence")
        .or_else(|| request.payload.get("threshold"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;

    match psr.search_patterns(&embedding, top_k, min_confidence) {
        Ok(results) => {
            let count = results.len();
            let result_json: Vec<serde_json::Value> = results
                .into_iter()
                .map(|(id, similarity, pattern)| {
                    serde_json::json!({
                        "pattern_id": id,
                        "similarity": similarity,
                        "pattern": serde_json::to_value(&pattern).unwrap_or_default(),
                    })
                })
                .collect();

            tracing::info!("Similarity search returned {} results", count);
            Response {
                response_type,
                request_id: request.request_id.clone(),
                success: true,
                data: serde_json::json!({
                    "results": result_json,
                    "count": count,
                }),
            }
        }
        Err(e) => {
            tracing::error!("Similarity search failed: {}", e);
            Response {
                response_type,
                request_id: request.request_id.clone(),
                success: false,
                data: serde_json::json!({ "error": format!("Search failed: {}", e) }),
            }
        }
    }
}

fn handle_ping(request: &Request) -> Response {
    Response {
        response_type: "Pong".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "layer": "Layer5_PSR",
            "status": "operational",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }),
    }
}

fn handle_health_check(
    psr: &PatternRegistry,
    request: &Request,
    start_time: Instant,
) -> Response {
    let uptime_secs = start_time.elapsed().as_secs();
    let pattern_count = psr.pattern_count();

    Response {
        response_type: "HealthCheck_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "status": "healthy",
            "healthy": true,
            "layer": "Layer5_PSR",
            "pattern_count": pattern_count,
            "uptime_seconds": uptime_secs,
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }),
    }
}

fn handle_get_stats(psr: &PatternRegistry, request: &Request) -> Response {
    let pattern_count = psr.pattern_count();
    let pool_id = request.payload.get("pool_id")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    Response {
        response_type: "GetStats_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "pattern_count": pattern_count,
            "pool_id": pool_id,
        }),
    }
}

// ---------------------------------------------------------------------------
// Accept loop
// ---------------------------------------------------------------------------

async fn accept_connections(
    listener: UnixListener,
    psr: Arc<PatternRegistry>,
    start_time: Instant,
) -> Result<()> {
    loop {
        let (stream, _addr) = listener.accept().await?;
        let psr_clone = Arc::clone(&psr);
        tracing::info!("New connection accepted");

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, psr_clone, start_time).await {
                tracing::error!("Connection error: {}", e);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let socket_path = std::env::var("SOCKET_PATH")
        .unwrap_or_else(|_| "/tmp/mfn_test_layer5.sock".to_string());

    let data_dir = std::env::var("PSR_DATA_DIR")
        .unwrap_or_else(|_| "./data/mfn/memory/layer5_psr".to_string());

    println!("Starting Layer 5 PSR Socket Server");
    println!("Target: <1ms pattern storage, <5ms similarity search");
    println!("Socket: {}", socket_path);

    // Create persistence config
    let persistence_config = PersistenceConfig {
        data_dir: PathBuf::from(&data_dir),
        pool_id: "crucible_training".to_string(),
        fsync_interval_ms: 1000,
        snapshot_interval_secs: 300,
        aof_buffer_size: 64 * 1024,
    };

    // Ensure data directory exists
    std::fs::create_dir_all(&persistence_config.data_dir)?;
    println!("Persistence enabled: {}", persistence_config.data_dir.display());
    println!("AOF: {}", persistence_config.aof_path().display());
    println!("Snapshots: {}", persistence_config.snapshot_path().display());

    // Create PSR instance with persistence (recover if data exists)
    let psr = if persistence_config.aof_path().exists()
        || persistence_config.snapshot_path().exists()
    {
        println!("Recovering from persistence...");
        Arc::new(PatternRegistry::recover_from_persistence(persistence_config)?)
    } else {
        println!("Fresh start with persistence enabled");
        Arc::new(PatternRegistry::new_with_persistence(Some(persistence_config))?)
    };

    println!("Layer 5 PSR ready! {} patterns loaded", psr.pattern_count());

    // Clean up stale socket file
    if std::path::Path::new(&socket_path).exists() {
        std::fs::remove_file(&socket_path)?;
        tracing::info!("Removed stale socket file: {}", socket_path);
    }

    // Bind listener
    let listener = UnixListener::bind(&socket_path)?;
    println!("Layer 5 PSR socket server listening on {}", socket_path);
    println!("Operations: AddPattern, SimilaritySearch, Synthesize, Ping, HealthCheck, GetStats");

    let start_time = Instant::now();

    // Run until Ctrl+C
    tokio::select! {
        result = accept_connections(listener, psr, start_time) => {
            if let Err(e) = result {
                tracing::error!("Server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutdown signal received, stopping server gracefully...");
            // Clean up socket file
            if std::path::Path::new(&socket_path).exists() {
                let _ = std::fs::remove_file(&socket_path);
            }
            println!("Server stopped successfully");
        }
    }

    Ok(())
}
