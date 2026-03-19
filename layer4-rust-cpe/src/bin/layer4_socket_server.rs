#!/usr/bin/env cargo
/*
[dependencies]
layer4_cpe = { path = ".." }
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0"
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
*/

//! Layer 4 CPE Socket Server
//! High-performance Unix socket server for Context Prediction Engine

use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use layer4_cpe::{ContextPredictionLayer, ContextPredictionConfig, PoolManager};
use mfn_core::{MemoryId, current_timestamp};
use serde::{Deserialize, Serialize};
use anyhow::Result;

/// Maps content text to its memory_id (populated by AddMemoryContext, queried by PredictContext).
/// Keyed by pool_id -> content_string -> memory_id.
type ContentIdMap = Arc<RwLock<HashMap<String, HashMap<String, MemoryId>>>>;

#[derive(Debug, Deserialize)]
struct ContextRequest {
    #[serde(rename = "type")]
    request_type: String,
    request_id: String,
    #[serde(default = "default_pool_id")]
    pool_id: String,
    #[serde(flatten)]
    payload: serde_json::Value,
}

fn default_pool_id() -> String {
    "crucible_training".to_string()
}

#[derive(Debug, Serialize)]
struct ContextResponse {
    #[serde(rename = "type")]
    response_type: String,
    request_id: String,
    success: bool,
    #[serde(flatten)]
    data: serde_json::Value,
}

async fn handle_connection(
    stream: UnixStream,
    pool_manager: Arc<PoolManager>,
    server_start_time: std::time::Instant,
    content_id_map: ContentIdMap,
) -> Result<()> {
    use tokio::io::AsyncReadExt;

    let conn_id = format!("conn_{}", uuid::Uuid::new_v4());
    eprintln!("New connection: {}", conn_id);

    let (mut read_half, mut write_half) = stream.into_split();

    loop {
        // Read 4-byte length prefix (binary protocol)
        let mut len_buf = [0u8; 4];
        match read_half.read_exact(&mut len_buf).await {
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            },
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to read length: {}", e));
            }
        }

        let msg_len = u32::from_le_bytes(len_buf) as usize;

        if msg_len == 0 || msg_len > 10_000_000 {
            eprintln!("Invalid message length: {}", msg_len);
            break;
        }

        let mut msg_buf = vec![0u8; msg_len];
        if let Err(e) = read_half.read_exact(&mut msg_buf).await {
            eprintln!("Failed to read message: {}", e);
            break;
        }

        let msg_str = match std::str::from_utf8(&msg_buf) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Invalid UTF-8: {}", e);
                break;
            }
        };

        let request: ContextRequest = match serde_json::from_str(msg_str) {
            Ok(req) => req,
            Err(e) => {
                let resp = make_error_response("unknown", &format!("Invalid JSON: {}", e));
                send_response(&mut write_half, &resp).await?;
                continue;
            }
        };

        let layer = match pool_manager.get_or_create_pool(&request.pool_id).await {
            Ok(pool) => pool,
            Err(e) => {
                let resp = make_error_response(
                    &request.request_id,
                    &format!("Failed to get pool '{}': {}", request.pool_id, e),
                );
                send_response(&mut write_half, &resp).await?;
                continue;
            }
        };

        let response = match request.request_type.as_str() {
            "AddMemoryContext" => {
                handle_add_memory_context(
                    &layer, &request, &conn_id, &content_id_map, &request.pool_id,
                ).await
            }
            "PredictContext" => {
                handle_predict_context(
                    &layer, &request, &conn_id, &content_id_map, &request.pool_id,
                ).await
            }
            "GetContextHistory" => {
                handle_get_context_history(&layer, &request, &conn_id).await
            }
            "Ping" => handle_ping(&request).await,
            "HealthCheck" => {
                handle_health_check(&pool_manager, &layer, &request, server_start_time).await
            }
            "ListPools" => handle_list_pools(&pool_manager, &request).await,
            _ => make_error_response(
                &request.request_id,
                &format!("Unknown request type: {}", request.request_type),
            ),
        };

        send_response(&mut write_half, &response).await?;
    }

    eprintln!("Connection closed: {} - cleaning up resources", conn_id);
    Ok(())
}

/// Send a length-prefixed JSON response over the socket.
async fn send_response(
    write_half: &mut tokio::net::unix::OwnedWriteHalf,
    response: &ContextResponse,
) -> Result<()> {
    let response_json = serde_json::to_string(response)?;
    let response_bytes = response_json.as_bytes();
    let response_len = response_bytes.len() as u32;
    write_half.write_all(&response_len.to_le_bytes()).await?;
    write_half.write_all(response_bytes).await?;
    Ok(())
}

fn make_error_response(request_id: &str, error: &str) -> ContextResponse {
    ContextResponse {
        response_type: "error".to_string(),
        request_id: request_id.to_string(),
        success: false,
        data: serde_json::json!({ "error": error }),
    }
}

async fn handle_add_memory_context(
    layer: &Arc<ContextPredictionLayer>,
    request: &ContextRequest,
    conn_id: &str,
    content_id_map: &ContentIdMap,
    pool_id: &str,
) -> ContextResponse {
    let memory_id = request.payload.get("memory_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let content = request.payload.get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let context = request.payload.get("context")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<String>>())
        .unwrap_or_default();

    // Store content -> memory_id mapping for PredictContext lookups
    {
        let mut map = content_id_map.write().await;
        let pool_map = map.entry(pool_id.to_string()).or_insert_with(HashMap::new);
        pool_map.insert(content.to_string(), memory_id as u64);
    }

    // Add memory access to temporal analyzer with connection tracking
    layer.add_memory_access_with_connection(
        memory_id as u64,
        content,
        &context,
        Some(conn_id.to_string()),
    ).await;

    ContextResponse {
        response_type: "AddMemoryContext_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "memory_id": memory_id,
            "content": content,
            "context_added": context.len(),
            "timestamp": current_timestamp(),
            "connection_id": conn_id
        }),
    }
}

async fn handle_predict_context(
    layer: &Arc<ContextPredictionLayer>,
    request: &ContextRequest,
    _conn_id: &str,
    content_id_map: &ContentIdMap,
    pool_id: &str,
) -> ContextResponse {
    let current_context = request.payload.get("current_context")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str())
            .map(|s| s.to_string())
            .collect::<Vec<String>>())
        .unwrap_or_default();

    let sequence_length = request.payload.get("sequence_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    // Resolve current_context text strings to memory IDs via the content map
    let (start_memory_ids, id_to_content) = {
        let map = content_id_map.read().await;
        let mut ids = Vec::new();
        let mut reverse_map: HashMap<u64, String> = HashMap::new();

        if let Some(pool_map) = map.get(pool_id) {
            // Build reverse map: memory_id -> content text
            for (text, &id) in pool_map.iter() {
                reverse_map.insert(id, text.clone());
            }
            // Resolve each context string to its memory_id
            for text in &current_context {
                if let Some(&id) = pool_map.get(text.as_str()) {
                    ids.push(id);
                }
            }
        }
        (ids, reverse_map)
    };

    let resolved_count = start_memory_ids.len();
    let context_count = current_context.len();

    // Use the temporal analyzer directly for prediction
    let predictions = if !start_memory_ids.is_empty() {
        let context = layer4_cpe::temporal::PredictionContext {
            recent_sequence: Some(start_memory_ids.clone()),
            current_timestamp: current_timestamp(),
            user_context: Some(current_context.join(" ")),
            session_id: None,
            max_predictions: sequence_length,
            connection_id: None,
        };
        let analyzer = layer.get_analyzer().await;
        analyzer.predict_next(&context)
    } else {
        // Fallback: use predict_from_recent which reads from the analyzer's access window
        layer.predict_from_recent(sequence_length).await
    };

    // Convert predictions to JSON with actual content text
    let results: Vec<serde_json::Value> = predictions.iter().map(|pred| {
        let content = id_to_content.get(&pred.memory_id)
            .cloned()
            .unwrap_or_else(|| format!("memory_{}", pred.memory_id));
        serde_json::json!({
            "content": content,
            "predicted_content": content,
            "confidence": pred.confidence,
            "probability": pred.confidence,
            "memory_id": pred.memory_id,
            "prediction_type": format!("{:?}", pred.prediction_type),
            "estimated_time_us": pred.estimated_time_us,
            "contributing_evidence": pred.contributing_evidence,
        })
    }).collect();

    ContextResponse {
        response_type: "PredictContext_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "predictions": results,
            "context": current_context,
            "predicted_sequence_length": sequence_length,
            "total_predictions": results.len(),
            "resolved_memory_ids": resolved_count,
            "total_context_strings": context_count
        }),
    }
}

async fn handle_get_context_history(
    layer: &Arc<ContextPredictionLayer>,
    request: &ContextRequest,
    _conn_id: &str,
) -> ContextResponse {
    let memory_id = request.payload.get("memory_id")
        .and_then(|v| v.as_u64());

    let analyzer = layer.get_analyzer().await;

    // Filter by memory_id if provided (non-zero), otherwise return all history
    let filter = memory_id.filter(|&id| id != 0);
    let accesses = analyzer.get_access_history(filter);
    let total_accesses = accesses.len();

    // Build history entries from actual access records
    let history: Vec<serde_json::Value> = accesses.iter().map(|access| {
        serde_json::json!({
            "memory_id": access.memory_id,
            "timestamp": access.timestamp,
            "access_type": format!("{:?}", access.access_type),
            "context": access.user_context,
            "confidence": access.confidence,
        })
    }).collect();

    // Calculate pattern strength for the specific memory if requested
    let pattern_strength = match filter {
        Some(mid) => analyzer.get_pattern_strength(mid),
        None => {
            let stats = analyzer.get_statistics();
            stats.average_pattern_confidence
        }
    };

    ContextResponse {
        response_type: "GetContextHistory_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "memory_id": memory_id.unwrap_or(0),
            "history": history,
            "total_accesses": total_accesses,
            "pattern_strength": pattern_strength
        }),
    }
}

async fn handle_ping(request: &ContextRequest) -> ContextResponse {
    ContextResponse {
        response_type: "Pong".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "timestamp": current_timestamp(),
            "layer": "Layer4_CPE",
            "status": "operational"
        }),
    }
}

async fn handle_health_check(
    pool_manager: &Arc<PoolManager>,
    layer: &Arc<ContextPredictionLayer>,
    request: &ContextRequest,
    server_start_time: std::time::Instant,
) -> ContextResponse {
    let timestamp = current_timestamp();
    let uptime_seconds = server_start_time.elapsed().as_secs();
    let memory_stats = layer.get_memory_stats().await;
    let pool_count = pool_manager.pool_count().await;

    let metrics = serde_json::json!({
        "memory_stats": memory_stats,
        "uptime_seconds": uptime_seconds,
        "pool_count": pool_count,
    });

    ContextResponse {
        response_type: "HealthCheck_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "status": "healthy",
            "layer": "Layer4_CPE",
            "timestamp": timestamp,
            "uptime_seconds": uptime_seconds,
            "pool_id": request.pool_id,
            "pool_count": pool_count,
            "metrics": metrics,
            "memory_info": memory_stats,
        }),
    }
}

async fn handle_list_pools(
    pool_manager: &Arc<PoolManager>,
    request: &ContextRequest,
) -> ContextResponse {
    let pools = pool_manager.list_pools().await;
    let pool_count = pools.len();

    ContextResponse {
        response_type: "ListPools_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "pools": pools,
            "pool_count": pool_count,
        }),
    }
}

// Helper UUID generation
mod uuid {
    use rand::Rng;

    pub struct Uuid(String);

    impl Uuid {
        pub fn new_v4() -> Self {
            let mut rng = rand::thread_rng();
            let uuid = format!(
                "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                rng.gen::<u32>(),
                rng.gen::<u16>(),
                rng.gen::<u16>() & 0x0fff | 0x4000,
                rng.gen::<u16>() & 0x3fff | 0x8000,
                rng.gen::<u64>() & 0xffffffffffff
            );
            Uuid(uuid)
        }
    }

    impl std::fmt::Display for Uuid {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let socket_path_str = std::env::var("MFN_SOCKET_PATH")
        .unwrap_or_else(|_| "/tmp/mfn_test_layer4.sock".to_string());

    println!("Starting Layer 4 CPE Socket Server (Multi-Pool)");
    println!("Target: Context prediction and temporal analysis");
    println!("Socket: {}", socket_path_str);

    // Create data directory for all pools
    let data_dir = PathBuf::from(
        std::env::var("MFN_DATA_DIR")
            .map(|d| format!("{}/layer4_cpe", d))
            .unwrap_or_else(|_| "./data/mfn/memory/layer4_cpe".to_string()),
    );
    std::fs::create_dir_all(&data_dir)?;
    println!("Multi-pool persistence enabled: {}", data_dir.display());
    println!("Default pool: crucible_training");

    // Create PoolManager
    let config = ContextPredictionConfig::default();
    let pool_manager = Arc::new(PoolManager::new(data_dir, config));

    // Create content-to-memory-ID mapping shared across all connections
    let content_id_map: ContentIdMap = Arc::new(RwLock::new(HashMap::new()));

    // Remove existing socket file
    let socket_path = socket_path_str.as_str();
    if std::path::Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    println!("Layer 4 CPE socket server listening on {}", socket_path);
    println!("Operations: AddMemoryContext, PredictContext, GetContextHistory, Ping, HealthCheck, ListPools");

    let server_start_time = std::time::Instant::now();
    let socket_path_cleanup = socket_path_str.clone();

    tokio::select! {
        result = serve_connections(listener, pool_manager, server_start_time, content_id_map) => {
            if let Err(e) = result {
                eprintln!("Server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutdown signal received, stopping server gracefully...");
            if std::path::Path::new(&socket_path_cleanup).exists() {
                std::fs::remove_file(&socket_path_cleanup)?;
            }
            println!("Server stopped successfully");
        }
    }

    Ok(())
}

async fn serve_connections(
    listener: UnixListener,
    pool_manager: Arc<PoolManager>,
    server_start_time: std::time::Instant,
    content_id_map: ContentIdMap,
) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let pool_manager_clone = Arc::clone(&pool_manager);
        let content_id_map_clone = Arc::clone(&content_id_map);

        tokio::spawn(async move {
            if let Err(e) = handle_connection(
                stream, pool_manager_clone, server_start_time, content_id_map_clone,
            ).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}
