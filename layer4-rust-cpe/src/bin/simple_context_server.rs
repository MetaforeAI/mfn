#!/usr/bin/env cargo
/*
[dependencies]
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0"
serde = { version = "1.0", features = ["derive"] }
*/

//! Simple Layer 4 Context Prediction Socket Server
//! Minimal implementation for MFN testing without full CPE dependencies

use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

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

// Simple in-memory context store
struct SimpleContextStore {
    memory_contexts: HashMap<u32, Vec<String>>,
    context_history: HashMap<u32, Vec<u64>>, // timestamps of accesses
}

impl SimpleContextStore {
    fn new() -> Self {
        Self {
            memory_contexts: HashMap::new(),
            context_history: HashMap::new(),
        }
    }

    fn add_memory_context(&mut self, memory_id: u32, context: Vec<String>) {
        self.memory_contexts.insert(memory_id, context);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        self.context_history
            .entry(memory_id)
            .or_insert_with(Vec::new)
            .push(timestamp);
    }

    fn predict_context(&self, current_context: &[String]) -> Vec<String> {
        // Simple prediction: find memories with similar contexts
        let mut predictions = Vec::new();
        
        for (_, stored_context) in &self.memory_contexts {
            let overlap = current_context
                .iter()
                .filter(|ctx| stored_context.contains(ctx))
                .count();
            
            if overlap > 0 {
                // Add unique contexts from matched memories
                for ctx in stored_context {
                    if !current_context.contains(ctx) && !predictions.contains(ctx) {
                        predictions.push(ctx.clone());
                    }
                }
            }
        }
        
        predictions.truncate(5); // Limit to 5 predictions
        predictions
    }

    fn get_context_history(&self, memory_id: u32) -> (Vec<String>, Vec<u64>) {
        let context = self.memory_contexts
            .get(&memory_id)
            .cloned()
            .unwrap_or_default();
        let history = self.context_history
            .get(&memory_id)
            .cloned()
            .unwrap_or_default();
        
        (context, history)
    }
}

async fn handle_connection(
    stream: UnixStream,
    context_store: std::sync::Arc<tokio::sync::Mutex<SimpleContextStore>>
) -> Result<(), Box<dyn std::error::Error>> {
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
            "AddMemoryContext" => handle_add_memory_context(&context_store, &request).await,
            "PredictContext" => handle_predict_context(&context_store, &request).await,
            "GetContextHistory" => handle_get_context_history(&context_store, &request).await,
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
    context_store: &std::sync::Arc<tokio::sync::Mutex<SimpleContextStore>>,
    request: &ContextRequest
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
    
    // Add to context store
    let mut store = context_store.lock().await;
    store.add_memory_context(memory_id, context.clone());
    
    ContextResponse {
        response_type: "AddMemoryContext_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "memory_id": memory_id,
            "content": content,
            "context_added": context.len(),
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        })
    }
}

async fn handle_predict_context(
    context_store: &std::sync::Arc<tokio::sync::Mutex<SimpleContextStore>>,
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
    
    // Get predictions from context store
    let store = context_store.lock().await;
    let predictions = store.predict_context(&current_context);
    let confidence = if predictions.is_empty() { 0.0 } else { 0.7 };
    
    ContextResponse {
        response_type: "PredictContext_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "predictions": predictions,
            "confidence": confidence,
            "processing_time_ms": 0.5, // Simulated processing time
            "context": current_context,
            "predicted_sequence_length": sequence_length.min(predictions.len())
        })
    }
}

async fn handle_get_context_history(
    context_store: &std::sync::Arc<tokio::sync::Mutex<SimpleContextStore>>,
    request: &ContextRequest
) -> ContextResponse {
    let memory_id = request.payload.get("memory_id")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    
    let store = context_store.lock().await;
    let (context, history) = store.get_context_history(memory_id);
    
    ContextResponse {
        response_type: "GetContextHistory_Response".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "memory_id": memory_id,
            "context": context,
            "history": history,
            "total_accesses": history.len(),
            "pattern_strength": if history.len() > 1 { 0.8 } else { 0.0 }
        })
    }
}

async fn handle_ping(request: &ContextRequest) -> ContextResponse {
    ContextResponse {
        response_type: "Pong".to_string(),
        request_id: request.request_id.clone(),
        success: true,
        data: serde_json::json!({
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
            "layer": "Layer4_Simple_CPE",
            "status": "operational"
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧠 Starting Layer 4 Simple Context Prediction Server");
    println!("🎯 Target: Basic context prediction and temporal tracking");
    println!("🔗 Socket: /tmp/mfn_test_layer4.sock");

    // Create context store
    let context_store = std::sync::Arc::new(tokio::sync::Mutex::new(SimpleContextStore::new()));

    // Remove existing socket file
    let socket_path = "/tmp/mfn_test_layer4.sock";
    if std::path::Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }
    
    // Bind to Unix socket
    let listener = UnixListener::bind(socket_path)?;
    println!("✅ Layer 4 Context server listening on {}", socket_path);
    println!("🔮 Operations: AddMemoryContext, PredictContext, GetContextHistory, Ping");
    
    // Handle graceful shutdown
    tokio::select! {
        result = serve_connections(listener, context_store) => {
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

async fn serve_connections(
    listener: UnixListener, 
    context_store: std::sync::Arc<tokio::sync::Mutex<SimpleContextStore>>
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let (stream, _) = listener.accept().await?;
        let store_clone = std::sync::Arc::clone(&context_store);
        
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, store_clone).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}