//! MFN API Gateway Binary
//!
//! Launches the HTTP REST API gateway that provides external access
//! to the MFN layer sockets via RESTful endpoints.

use std::env;
use std::time::Duration;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod api_gateway {
    pub use mfn_telepathy::api_gateway::*;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mfn_gateway=info,mfn_telepathy=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let port: u16 = env::var("MFN_API_PORT")
        .or_else(|_| env::var("MFN_GATEWAY_PORT"))
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    info!("Starting MFN API Gateway on port {port}");

    let config = api_gateway::ApiGatewayConfig {
        port,
        enable_auth: env::var("MFN_ENABLE_AUTH")
            .ok()
            .map(|v| v == "true")
            .unwrap_or(false),
        enable_rate_limit: env::var("MFN_ENABLE_RATE_LIMIT")
            .ok()
            .map(|v| v == "true")
            .unwrap_or(true),
        rate_limit_rps: env::var("MFN_RATE_LIMIT_RPS")
            .ok()
            .and_then(|r| r.parse().ok())
            .unwrap_or(100),
        request_timeout: Duration::from_secs(
            env::var("MFN_REQUEST_TIMEOUT_SECS")
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(30),
        ),
        enable_cache: env::var("MFN_ENABLE_CACHE")
            .ok()
            .map(|v| v == "true")
            .unwrap_or(true),
        cache_ttl: Duration::from_secs(
            env::var("MFN_CACHE_TTL_SECS")
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(60),
        ),
        enable_websocket: env::var("MFN_ENABLE_WEBSOCKET")
            .ok()
            .map(|v| v == "true")
            .unwrap_or(true),
        enable_swagger: env::var("MFN_ENABLE_SWAGGER")
            .ok()
            .map(|v| v == "true")
            .unwrap_or(true),
    };

    info!("  Port: {}", config.port);
    info!("  Authentication: {}", config.enable_auth);
    info!("  Rate limiting: {} ({} RPS)", config.enable_rate_limit, config.rate_limit_rps);

    match api_gateway::launch_gateway(config).await {
        Ok(_) => {
            info!("API Gateway shut down gracefully");
            Ok(())
        }
        Err(e) => {
            error!("API Gateway error: {}", e);
            Err(e)
        }
    }
}
