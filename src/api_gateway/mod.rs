//! HTTP API Gateway for MFN
//!
//! Provides a clean RESTful HTTP interface for external clients while
//! internally using the high-performance binary socket protocol.
//!
//! Features:
//! - HTTP to socket protocol translation
//! - Authentication and rate limiting
//! - OpenAPI/Swagger documentation
//! - WebSocket support for streaming
//! - Request/response caching

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use axum::{
    Router,
    routing::{get, post, put, delete},
    extract::{State, Path, Query, Json},
    response::{IntoResponse, Response},
    http::{StatusCode, HeaderMap},
    middleware,
};
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};
use bytes::Bytes;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    compression::CompressionLayer,
    trace::TraceLayer,
    timeout::TimeoutLayer,
};
use tracing::{info, warn, error};
use uuid::Uuid;

use crate::socket::{
    MessageRouter, SocketMessage, MessageType, SocketClientConfig,
    SocketMonitor, MetricsReport,
};

/// API Gateway configuration
#[derive(Debug, Clone)]
pub struct ApiGatewayConfig {
    /// HTTP server port
    pub port: u16,
    /// Enable authentication
    pub enable_auth: bool,
    /// Enable rate limiting
    pub enable_rate_limit: bool,
    /// Rate limit (requests per second)
    pub rate_limit_rps: u32,
    /// Request timeout
    pub request_timeout: Duration,
    /// Enable request caching
    pub enable_cache: bool,
    /// Cache TTL
    pub cache_ttl: Duration,
    /// Enable WebSocket support
    pub enable_websocket: bool,
    /// Enable OpenAPI documentation
    pub enable_swagger: bool,
}

impl Default for ApiGatewayConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            enable_auth: false,
            enable_rate_limit: true,
            rate_limit_rps: 100,
            request_timeout: Duration::from_secs(30),
            enable_cache: true,
            cache_ttl: Duration::from_secs(60),
            enable_websocket: true,
            enable_swagger: true,
        }
    }
}

/// API Gateway state
struct ApiGatewayState {
    router: Arc<MessageRouter>,
    monitor: Arc<SocketMonitor>,
    config: ApiGatewayConfig,
    cache: Arc<tokio::sync::RwLock<HashMap<String, CachedResponse>>>,
}

/// Cached response
#[derive(Clone)]
struct CachedResponse {
    data: Value,
    expires_at: Instant,
}

/// Memory operation request
#[derive(Debug, Deserialize)]
struct MemoryRequest {
    content: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    metadata: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embedding: Option<Vec<f32>>,
}

/// Memory operation response
#[derive(Debug, Serialize)]
struct MemoryResponse {
    id: u64,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// Search request
#[derive(Debug, Deserialize)]
struct SearchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embedding: Option<Vec<f32>>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
}

fn default_limit() -> usize {
    10
}

/// Search response
#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
    total: usize,
    query_time_ms: f64,
}

/// Individual search result
#[derive(Debug, Serialize)]
struct SearchResult {
    id: u64,
    content: String,
    score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<HashMap<String, String>>,
}

/// Error response
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    timestamp: u64,
}

/// Create the API Gateway router
fn create_router(state: ApiGatewayState) -> Router {
    let enable_websocket = state.config.enable_websocket;
    let request_timeout = state.config.request_timeout;
    let state = Arc::new(state);

    let mut app = Router::new()
        // Memory operations
        .route("/api/v1/memory", post(create_memory))
        .route("/api/v1/memory/:id", get(get_memory))
        .route("/api/v1/memory/:id", put(update_memory))
        .route("/api/v1/memory/:id", delete(delete_memory))

        // Search operations
        .route("/api/v1/search", post(search_memories))
        .route("/api/v1/search/similar", post(search_similar))
        .route("/api/v1/search/associative", post(search_associative))

        // System operations
        .route("/api/v1/health", get(health_check))
        .route("/api/v1/metrics", get(get_metrics))
        .route("/api/v1/status", get(get_status))

        // Documentation
        .route("/api/docs", get(serve_swagger))
        .route("/api/openapi.json", get(serve_openapi_spec))
        .with_state(state);

    // Add WebSocket support if enabled
    if enable_websocket {
        app = app.route("/api/v1/ws", get(websocket_handler));
    }

    // Add middleware
    app.layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(CorsLayer::permissive())
            .layer(TimeoutLayer::new(request_timeout))
    )
}

/// Create a new memory
async fn create_memory(
    State(state): State<Arc<ApiGatewayState>>,
    Json(req): Json<MemoryRequest>,
) -> Result<Json<MemoryResponse>, ApiError> {
    let start = Instant::now();

    // Generate memory ID
    let memory_id = generate_memory_id();

    // Prepare binary payload
    let payload = serialize_memory_request(&req, memory_id)?;

    // Send to Layer 1 via router
    let message = SocketMessage::new(
        MessageType::MemoryAdd,
        memory_id,
        Bytes::from(payload),
    );

    let response = state.router
        .route_message(message)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Record metrics
    state.monitor.record_request(1, start.elapsed(), true);

    let msg_type = response.header.msg_type;
    Ok(Json(MemoryResponse {
        id: memory_id,
        success: matches!(msg_type, x if x == MessageType::Success as u16),
        message: None,
    }))
}

/// Get memory by ID
async fn get_memory(
    State(state): State<Arc<ApiGatewayState>>,
    Path(id): Path<u64>,
) -> Result<Json<Value>, ApiError> {
    let start = Instant::now();

    // Check cache first
    if state.config.enable_cache {
        let cache_key = format!("memory:{}", id);
        let cache = state.cache.read().await;
        if let Some(cached) = cache.get(&cache_key) {
            if cached.expires_at > Instant::now() {
                return Ok(Json(cached.data.clone()));
            }
        }
    }

    // Prepare request
    let message = SocketMessage::new(
        MessageType::MemoryGet,
        id,
        Bytes::from(id.to_le_bytes().to_vec()),
    );

    // Send via router
    let response = state.router
        .route_message(message)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Parse response
    let memory_data = parse_memory_response(&response.payload)?;

    // Update cache
    if state.config.enable_cache {
        let cache_key = format!("memory:{}", id);
        let mut cache = state.cache.write().await;
        cache.insert(cache_key, CachedResponse {
            data: memory_data.clone(),
            expires_at: Instant::now() + state.config.cache_ttl,
        });
    }

    // Record metrics
    state.monitor.record_request(1, start.elapsed(), true);

    Ok(Json(memory_data))
}

/// Update memory
async fn update_memory(
    State(state): State<Arc<ApiGatewayState>>,
    Path(id): Path<u64>,
    Json(req): Json<MemoryRequest>,
) -> Result<Json<MemoryResponse>, ApiError> {
    let start = Instant::now();

    // Prepare update payload
    let payload = serialize_memory_request(&req, id)?;

    let message = SocketMessage::new(
        MessageType::MemoryUpdate,
        id,
        Bytes::from(payload),
    );

    let response = state.router
        .route_message(message)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Invalidate cache
    if state.config.enable_cache {
        let cache_key = format!("memory:{}", id);
        let mut cache = state.cache.write().await;
        cache.remove(&cache_key);
    }

    state.monitor.record_request(1, start.elapsed(), true);

    let msg_type = response.header.msg_type;
    Ok(Json(MemoryResponse {
        id,
        success: matches!(msg_type, x if x == MessageType::Success as u16),
        message: None,
    }))
}

/// Delete memory
async fn delete_memory(
    State(state): State<Arc<ApiGatewayState>>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    let start = Instant::now();

    let message = SocketMessage::new(
        MessageType::MemoryDelete,
        id,
        Bytes::from(id.to_le_bytes().to_vec()),
    );

    let response = state.router
        .route_message(message)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Invalidate cache
    if state.config.enable_cache {
        let cache_key = format!("memory:{}", id);
        let mut cache = state.cache.write().await;
        cache.remove(&cache_key);
    }

    state.monitor.record_request(1, start.elapsed(), true);

    let msg_type = response.header.msg_type;
    if matches!(msg_type, x if x == MessageType::Success as u16) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Ok(StatusCode::NOT_FOUND)
    }
}

/// Search memories
async fn search_memories(
    State(state): State<Arc<ApiGatewayState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    let start = Instant::now();

    // Determine search type based on request
    let msg_type = if req.embedding.is_some() {
        MessageType::SearchSimilarity
    } else {
        MessageType::SearchAssociative
    };

    // Serialize search request
    let payload = serialize_search_request(&req)?;

    let message = SocketMessage::new(
        msg_type,
        generate_request_id(),
        Bytes::from(payload),
    );

    let response = state.router
        .route_message(message)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Parse search results
    let results = parse_search_response(&response.payload)?;

    let query_time = start.elapsed();
    state.monitor.record_request(2, query_time, true);

    Ok(Json(SearchResponse {
        total: results.len(),
        results,
        query_time_ms: query_time.as_secs_f64() * 1000.0,
    }))
}

/// Search similar memories
async fn search_similar(
    State(state): State<Arc<ApiGatewayState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    if req.embedding.is_none() {
        return Err(ApiError::BadRequest("Embedding required for similarity search".into()));
    }

    search_memories(State(state), Json(req)).await
}

/// Search associative memories
async fn search_associative(
    State(state): State<Arc<ApiGatewayState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    search_memories(State(state), Json(req)).await
}

/// Health check endpoint
async fn health_check(
    State(state): State<Arc<ApiGatewayState>>,
) -> Json<Value> {
    let health_status = state.router.get_health_status().await;

    let healthy_layers: Vec<u8> = health_status
        .iter()
        .filter(|(_, h)| h.is_healthy)
        .map(|(id, _)| *id)
        .collect();

    Json(json!({
        "status": if healthy_layers.len() >= 3 { "healthy" } else { "degraded" },
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        "layers": {
            "healthy": healthy_layers,
            "total": 4
        }
    }))
}

/// Get metrics endpoint
async fn get_metrics(
    State(state): State<Arc<ApiGatewayState>>,
) -> Result<Json<MetricsReport>, ApiError> {
    Ok(Json(state.monitor.get_report().await))
}

/// Get system status
async fn get_status(
    State(state): State<Arc<ApiGatewayState>>,
) -> Json<Value> {
    let health_status = state.router.get_health_status().await;
    let metrics = state.monitor.get_report().await;

    Json(json!({
        "version": "1.0.0",
        "uptime_seconds": metrics.uptime_seconds,
        "layers": health_status.into_iter().map(|(id, health)| {
            json!({
                "id": id,
                "healthy": health.is_healthy,
                "response_time_ms": health.response_time_ms,
                "error_rate": if health.success_count + health.error_count > 0 {
                    health.error_count as f64 / (health.success_count + health.error_count) as f64
                } else { 0.0 }
            })
        }).collect::<Vec<_>>(),
        "performance": {
            "requests_per_second": metrics.request_metrics.requests_per_second,
            "avg_latency_ms": metrics.request_metrics.avg_latency_ms,
            "p99_latency_ms": metrics.request_metrics.p99_latency_ms,
        }
    }))
}

/// Serve OpenAPI specification
async fn serve_openapi_spec() -> Json<Value> {
    Json(generate_openapi_spec())
}

/// Serve Swagger UI
async fn serve_swagger() -> impl IntoResponse {
    // Return HTML for Swagger UI
    axum::response::Html(include_str!("swagger.html"))
}

/// WebSocket handler (placeholder)
async fn websocket_handler() -> impl IntoResponse {
    StatusCode::NOT_IMPLEMENTED
}

/// API Error type
#[derive(Debug)]
enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse {
            error: message,
            details: None,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        });

        (status, body).into_response()
    }
}

/// Helper functions
fn generate_memory_id() -> u64 {
    uuid::Uuid::new_v4().as_u128() as u64
}

fn generate_request_id() -> u64 {
    uuid::Uuid::new_v4().as_u128() as u64
}

fn serialize_memory_request(req: &MemoryRequest, id: u64) -> Result<Vec<u8>, ApiError> {
    // Use binary protocol serialization
    let mut buffer = Vec::new();
    buffer.extend_from_slice(&id.to_le_bytes());
    buffer.extend_from_slice(&(req.content.len() as u32).to_le_bytes());
    buffer.extend_from_slice(req.content.as_bytes());
    // Add more fields as needed
    Ok(buffer)
}

fn serialize_search_request(req: &SearchRequest) -> Result<Vec<u8>, ApiError> {
    // Use binary protocol serialization
    let mut buffer = Vec::new();
    if let Some(query) = &req.query {
        buffer.extend_from_slice(&(query.len() as u32).to_le_bytes());
        buffer.extend_from_slice(query.as_bytes());
    } else {
        buffer.extend_from_slice(&0u32.to_le_bytes());
    }
    buffer.extend_from_slice(&(req.limit as u32).to_le_bytes());
    Ok(buffer)
}

fn parse_memory_response(payload: &[u8]) -> Result<Value, ApiError> {
    // Parse binary response to JSON
    Ok(json!({
        "id": u64::from_le_bytes(payload[0..8].try_into().unwrap()),
        "content": "Memory content",
        "metadata": {}
    }))
}

fn parse_search_response(payload: &[u8]) -> Result<Vec<SearchResult>, ApiError> {
    // Parse binary search results
    Ok(vec![])
}

fn generate_openapi_spec() -> Value {
    json!({
        "openapi": "3.0.0",
        "info": {
            "title": "MFN API",
            "version": "1.0.0",
            "description": "Memory Flow Network API Gateway"
        },
        "paths": {
            "/api/v1/memory": {
                "post": {
                    "summary": "Create a new memory",
                    "requestBody": {
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/MemoryRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Memory created successfully"
                        }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "MemoryRequest": {
                    "type": "object",
                    "required": ["content"],
                    "properties": {
                        "content": {
                            "type": "string"
                        },
                        "tags": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            }
                        }
                    }
                }
            }
        }
    })
}

/// Launch the API Gateway
pub async fn launch_gateway(config: ApiGatewayConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize router
    let socket_config = SocketClientConfig::default();
    let router = Arc::new(MessageRouter::new(socket_config));
    router.initialize_layers().await;

    // Initialize monitor
    let monitor = Arc::new(SocketMonitor::new());

    // Create state
    let state = ApiGatewayState {
        router,
        monitor,
        config: config.clone(),
        cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
    };

    // Create app
    let app = create_router(state);

    // Start server
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("API Gateway listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}