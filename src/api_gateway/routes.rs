//! Route definitions for the MFN REST API Gateway.
//!
//! Each handler parses the request, builds a layer-specific JSON message,
//! forwards it to the correct Unix socket, and returns the response.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use super::layer_client::ApiError;
use super::AppState;

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StoreMemoryRequest {
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub pool_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchMemoryRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub pool_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SimilaritySearchRequest {
    pub embedding: Vec<f32>,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

#[derive(Debug, Deserialize)]
pub struct PredictRequest {
    pub current_context: Vec<f64>,
    #[serde(default = "default_sequence_length")]
    pub sequence_length: usize,
}

#[derive(Debug, Deserialize)]
pub struct StorePatternRequest {
    pub name: String,
    pub embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
pub struct SearchPatternRequest {
    pub embedding: Vec<f32>,
    #[serde(default = "default_pattern_top_k")]
    pub top_k: usize,
}

#[derive(Debug, Serialize)]
pub struct LayerStatus {
    pub layer: u8,
    pub name: &'static str,
    pub available: bool,
}

fn default_top_k() -> usize { 10 }
fn default_sequence_length() -> usize { 5 }
fn default_pattern_top_k() -> usize { 5 }

fn layer_name(id: u8) -> &'static str {
    match id {
        1 => "IFR (Instant Flash Recall)",
        2 => "DSR (Deep Similarity Recall)",
        3 => "ALM (Associative Linking Memory)",
        4 => "CPE (Contextual Prediction Engine)",
        5 => "PSR (Pattern Synthesis & Recognition)",
        _ => "Unknown",
    }
}

fn request_id() -> String {
    Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_routes() -> Router<AppState> {
    Router::new()
        // Universal
        .route("/health", get(health))
        .route("/layers", get(layers))
        // Memory
        .route("/v1/memory", post(store_memory))
        .route("/v1/memory/search", post(search_memory))
        .route("/v1/memory/similar", post(similarity_search))
        // Prediction
        .route("/v1/predict", post(predict))
        // Patterns
        .route("/v1/pattern", post(store_pattern))
        .route("/v1/pattern/search", post(search_pattern))
        // Layer-direct
        .route("/v1/layer/{layer_id}", post(layer_direct))
}

// ---------------------------------------------------------------------------
// Universal endpoints
// ---------------------------------------------------------------------------

async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let mut statuses = Vec::new();
    for id in 1..=5u8 {
        statuses.push(json!({
            "layer": id,
            "name": layer_name(id),
            "available": state.client.layer_available(id),
        }));
    }

    let all_available = statuses.iter().all(|s| s["available"].as_bool().unwrap_or(false));
    let any_available = statuses.iter().any(|s| s["available"].as_bool().unwrap_or(false));

    let status_str = if all_available {
        "healthy"
    } else if any_available {
        "degraded"
    } else {
        "unavailable"
    };

    let code = if all_available {
        StatusCode::OK
    } else if any_available {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(json!({
        "status": status_str,
        "layers": statuses,
    })))
}

async fn layers(State(state): State<AppState>) -> Json<Value> {
    let mut layer_list = Vec::new();
    for id in 1..=5u8 {
        layer_list.push(json!({
            "layer": id,
            "name": layer_name(id),
            "socket": crate::socket::SocketPaths::get_layer_socket(id).to_string_lossy(),
            "available": state.client.layer_available(id),
        }));
    }
    Json(json!({ "layers": layer_list }))
}

// ---------------------------------------------------------------------------
// Memory endpoints
// ---------------------------------------------------------------------------

async fn store_memory(
    State(state): State<AppState>,
    Json(req): Json<StoreMemoryRequest>,
) -> Result<Json<Value>, ApiError> {
    let payload = json!({
        "type": "store",
        "request_id": request_id(),
        "content": req.content,
        "tags": req.tags,
        "pool_id": req.pool_id.unwrap_or_else(|| "default".into()),
    });

    let response = state.client.send_to_layer(1, payload).await?;
    Ok(Json(response))
}

async fn search_memory(
    State(state): State<AppState>,
    Json(req): Json<SearchMemoryRequest>,
) -> Result<Json<Value>, ApiError> {
    let payload = json!({
        "type": "search",
        "request_id": request_id(),
        "query": req.query,
        "top_k": req.top_k,
        "pool_id": req.pool_id.unwrap_or_else(|| "default".into()),
    });

    let response = state.client.send_to_layer(3, payload).await?;
    Ok(Json(response))
}

async fn similarity_search(
    State(state): State<AppState>,
    Json(req): Json<SimilaritySearchRequest>,
) -> Result<Json<Value>, ApiError> {
    let payload = json!({
        "type": "similarity_search",
        "request_id": request_id(),
        "embedding": req.embedding,
        "top_k": req.top_k,
    });

    let response = state.client.send_to_layer(2, payload).await?;
    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// Prediction endpoint
// ---------------------------------------------------------------------------

async fn predict(
    State(state): State<AppState>,
    Json(req): Json<PredictRequest>,
) -> Result<Json<Value>, ApiError> {
    let payload = json!({
        "type": "predict",
        "request_id": request_id(),
        "current_context": req.current_context,
        "sequence_length": req.sequence_length,
    });

    let response = state.client.send_to_layer(4, payload).await?;
    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// Pattern endpoints
// ---------------------------------------------------------------------------

async fn store_pattern(
    State(state): State<AppState>,
    Json(req): Json<StorePatternRequest>,
) -> Result<Json<Value>, ApiError> {
    let payload = json!({
        "type": "store_pattern",
        "request_id": request_id(),
        "name": req.name,
        "embedding": req.embedding,
    });

    let response = state.client.send_to_layer(5, payload).await?;
    Ok(Json(response))
}

async fn search_pattern(
    State(state): State<AppState>,
    Json(req): Json<SearchPatternRequest>,
) -> Result<Json<Value>, ApiError> {
    let payload = json!({
        "type": "search_pattern",
        "request_id": request_id(),
        "embedding": req.embedding,
        "top_k": req.top_k,
    });

    let response = state.client.send_to_layer(5, payload).await?;
    Ok(Json(response))
}

// ---------------------------------------------------------------------------
// Layer-direct endpoint
// ---------------------------------------------------------------------------

async fn layer_direct(
    State(state): State<AppState>,
    Path(layer_id): Path<u8>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !(1..=5).contains(&layer_id) {
        return Err(ApiError::bad_request(format!(
            "Invalid layer_id: {layer_id}. Must be 1-5."
        )));
    }

    let response = state.client.send_to_layer(layer_id, payload).await?;
    Ok(Json(response))
}
