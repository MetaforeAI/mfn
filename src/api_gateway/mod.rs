//! HTTP REST API Gateway for MFN
//!
//! Provides RESTful HTTP endpoints that forward requests to MFN layer
//! Unix sockets using the same wire protocol as mfn_client.py:
//! - Layer 1 (IFR): newline-delimited JSON
//! - Layers 2-5: 4-byte LE length-prefix + JSON

mod layer_client;
mod routes;

use std::sync::Arc;
use std::time::Duration;
use axum::Router;
use tower::ServiceBuilder;
use tower_http::{
    cors::{CorsLayer, AllowOrigin},
    compression::CompressionLayer,
    trace::TraceLayer,
    timeout::TimeoutLayer,
};
use tracing::info;

pub use layer_client::LayerClient;

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

/// Shared application state available to all route handlers.
#[derive(Clone)]
pub struct AppState {
    pub client: Arc<LayerClient>,
}

/// Launch the API Gateway HTTP server.
pub async fn launch_gateway(config: ApiGatewayConfig) -> Result<(), Box<dyn std::error::Error>> {
    let cors_origins = std::env::var("MFN_CORS_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:3000".to_string());

    let origins: Vec<_> = cors_origins
        .split(',')
        .filter_map(|o| o.trim().parse().ok())
        .collect();

    let cors = if origins.is_empty() {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any)
    };

    let state = AppState {
        client: Arc::new(LayerClient::new(Duration::from_secs(5))),
    };

    let app = Router::new()
        .merge(routes::build_routes())
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new())
                .layer(cors)
                .layer(TimeoutLayer::new(config.request_timeout)),
        );

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.port));
    info!("API Gateway listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
