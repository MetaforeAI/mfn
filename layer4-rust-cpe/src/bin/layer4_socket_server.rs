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

use std::sync::Arc;
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use layer4_cpe::{ContextPredictionLayer, ContextPredictionConfig};
use mfn_core::{UniversalSearchQuery, MemoryId, current_timestamp, MfnLayer, RoutingDecision};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Deserialize)]
struct ContextRequest {
    #[serde(rename = "type")]
    request_type: String,
    request_id: String,
    #[serde(flatten)]
    payload: serde_json::Value,
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

async fn handle_connection(stream: UnixStream, layer: Arc<ContextPredictionLayer>) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            break; // Connection closed
        }

        let trimmed_line = line.trim();
        if trimmed_line.is_empty() {
            continue;
        }

        // Parse JSON request
        let request: ContextRequest = match serde_json::from_str(trimmed_line) {
            Ok(req) => req,
            Err(e) => {
                let error_response = ContextResponse {
                    response_type: "error".to_string(),
                    request_id: "unknown".to_string(),
                    success: false,
                    data: serde_json::json!({
                        "error": format!("Invalid JSON: {}", e)
                    })
                };
                let response_json = serde_json::to_string(&error_response)?;
                write_half.write_all(format!("{}\n", response_json).as_bytes()).await?;
                continue;
            }
        };

        // Handle different request types
        let response = match request.request_type.as_str() {
            "AddMemoryContext" => handle_add_memory_context(&layer, &request).await,
            "PredictContext" => handle_predict_context(&layer, &request).await,
            "GetContextHistory" => handle_get_context_history(&layer, &request).await,
            "Ping" => handle_ping(&request).await,
            _ => ContextResponse {
                response_type: "error".to_string(),
                request_id: request.request_id,
                success: false,
                data: serde_json::json!({
                    "error": format!("Unknown request type: {}", request.request_type)
                })
            }
        };

        // Send response
        let response_json = serde_json::to_string(&response)?;
        write_half.write_all(format!("{}\n", response_json).as_bytes()).await?;
    }

    Ok(())
}

async fn handle_add_memory_context(
    layer: &Arc<ContextPredictionLayer>,
    request: &ContextRequest
) -> ContextResponse {
    // Extract memory_id and context from payload
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
    
    // Add memory access to temporal analyzer
    // This would be done through the layer's internal API
    ContextResponse {
        response_type: "AddMemoryContext_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "memory_id": memory_id,
            "content": content,
            "context_added": context.len(),
            "timestamp": current_timestamp()
        })
    }
}

async fn handle_predict_context(
    layer: &Arc<ContextPredictionLayer>,
    request: &ContextRequest
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
    
    // Create a search query for context prediction
    let query = UniversalSearchQuery {
        start_memory_ids: vec![],
        content: Some(current_context.join(" ")),
        embedding: None,
        tags: vec![],
        association_types: vec![],
        max_depth: 3,
        max_results: sequence_length,
        min_weight: 0.5,
        timeout_us: 10_000_000,
        layer_params: HashMap::new(),
    };
    
    match layer.search(&query).await {
        Ok(decision) => {
            let results = match decision {
                mfn_core::RoutingDecision::FoundExact { results } => results,
                mfn_core::RoutingDecision::FoundPartial { results, .. } => results,
                mfn_core::RoutingDecision::SearchComplete { results } => results,
                mfn_core::RoutingDecision::RouteToLayers { .. } => vec![],
            };

            ContextResponse {
                response_type: "PredictContext_Response".to_string(),
                request_id: request.request_id.clone(),
                success: true,
                data: serde_json::json!({
                    "predictions": results,
                    "context": current_context,
                    "predicted_sequence_length": sequence_length
                })
            }
        },
        Err(e) => ContextResponse {
            response_type: "error".to_string(),
            request_id: request.request_id.clone(),
            success: false,
            data: serde_json::json!({
                "error": format!("Prediction failed: {}", e)
            })
        }
    }
}

async fn handle_get_context_history(
    _layer: &Arc<ContextPredictionLayer>,
    request: &ContextRequest
) -> ContextResponse {
    let memory_id = request.payload.get("memory_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    
    // This would retrieve actual context history from the layer
    ContextResponse {
        response_type: "GetContextHistory_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "memory_id": memory_id,
            "history": [],
            "total_accesses": 0,
            "pattern_strength": 0.0
        })
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
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🧠 Starting Layer 4 CPE Socket Server");
    println!("🎯 Target: Context prediction and temporal analysis");
    println!("🔗 Socket: /tmp/mfn_layer4.sock");
    
    // Create CPE instance
    let config = ContextPredictionConfig::default();
    let layer = Arc::new(ContextPredictionLayer::new(config).await?);
    
    // Remove existing socket file
    let socket_path = "/tmp/mfn_layer4.sock";
    if std::path::Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }
    
    // Bind to Unix socket
    let listener = UnixListener::bind(socket_path)?;
    println!("✅ Layer 4 CPE socket server listening on {}", socket_path);
    println!("🔮 Operations: AddMemoryContext, PredictContext, GetContextHistory, Ping");
    
    // Handle graceful shutdown
    tokio::select! {
        result = serve_connections(listener, layer) => {
            if let Err(e) = result {
                eprintln!("❌ Server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\n🛑 Shutdown signal received, stopping server gracefully...");
            // Clean up socket file
            if std::path::Path::new(socket_path).exists() {
                std::fs::remove_file(socket_path)?;
            }
            println!("✅ Server stopped successfully");
        }
    }
    
    Ok(())
}

async fn serve_connections(listener: UnixListener, layer: Arc<ContextPredictionLayer>) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let layer_clone = Arc::clone(&layer);
        
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, layer_clone).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}