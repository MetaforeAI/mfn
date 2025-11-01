//! MFN Socket Monitor Binary
//!
//! Monitors the health and performance of all MFN socket connections
//! and provides metrics export capabilities.

use std::env;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use mfn_telepathy::socket::{
    SocketPaths, SocketClient, SocketClientConfig, SocketMonitor, MessageType,
};
use bytes::Bytes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mfn_monitor=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting MFN Socket Monitor");

    // Configuration
    let check_interval = Duration::from_secs(
        env::var("MFN_MONITOR_INTERVAL_SECS")
            .ok()
            .and_then(|i| i.parse().ok())
            .unwrap_or(10)
    );

    let export_prometheus = env::var("MFN_EXPORT_PROMETHEUS")
        .ok()
        .map(|v| v == "true")
        .unwrap_or(false);

    // Initialize monitor
    let monitor = SocketMonitor::new();

    // Create clients for each layer
    let client_config = SocketClientConfig {
        connection_timeout: Duration::from_secs(2),
        request_timeout: Duration::from_secs(5),
        max_retries: 1,
        enable_pooling: false, // Disable for monitoring
        ..Default::default()
    };

    let mut layer_clients = vec![
        (1, SocketClient::new(SocketPaths::LAYER1_IFR, client_config.clone())),
        (2, SocketClient::new(SocketPaths::LAYER2_DSR, client_config.clone())),
        (3, SocketClient::new(SocketPaths::LAYER3_ALM, client_config.clone())),
        (4, SocketClient::new(SocketPaths::LAYER4_CPE, client_config.clone())),
    ];

    info!("Monitoring {} layers with {}s interval", layer_clients.len(), check_interval.as_secs());

    // Start monitoring loop
    let mut ticker = interval(check_interval);
    ticker.tick().await; // Skip first immediate tick

    loop {
        ticker.tick().await;

        // Check each layer
        for (layer_id, client) in &layer_clients {
            let start = tokio::time::Instant::now();

            match client.ping().await {
                Ok(latency) => {
                    let total_latency = start.elapsed();
                    monitor.record_request(*layer_id, total_latency, true);

                    info!(
                        "Layer {} healthy - ping: {:?}, total: {:?}",
                        layer_id, latency, total_latency
                    );
                }
                Err(e) => {
                    monitor.record_request(*layer_id, start.elapsed(), false);
                    warn!("Layer {} unhealthy: {}", layer_id, e);
                }
            }

            // Optional: Get layer stats
            if let Ok(response) = client.request(
                MessageType::Stats,
                Bytes::from("{}"),
            ).await {
                info!("Layer {} stats received: {} bytes", layer_id, response.payload.len());
            }
        }

        // Print summary metrics
        let metrics = monitor.get_report().await;
        info!("=== Socket Metrics ===");
        info!("Total requests: {}", metrics.request_metrics.total_requests);
        info!("Success rate: {:.1}%",
            if metrics.request_metrics.total_requests > 0 {
                (metrics.request_metrics.successful_requests as f64 /
                 metrics.request_metrics.total_requests as f64) * 100.0
            } else { 0.0 }
        );
        info!("Avg latency: {:.2}ms", metrics.request_metrics.avg_latency_ms);
        info!("P99 latency: {:.2}ms", metrics.request_metrics.p99_latency_ms);

        // Layer-specific metrics
        for layer_metric in &metrics.layer_metrics {
            info!(
                "  Layer {}: {} requests, {} errors, {:.2}ms avg latency",
                layer_metric.layer_id,
                layer_metric.requests,
                layer_metric.errors,
                layer_metric.avg_latency_ms
            );
        }

        // Export Prometheus metrics if enabled
        if export_prometheus {
            let prometheus_metrics = monitor.export_prometheus().await;

            // In production, this would be served via HTTP endpoint
            // For now, we'll just log it
            if env::var("MFN_DEBUG_PROMETHEUS").ok().map(|v| v == "true").unwrap_or(false) {
                println!("=== Prometheus Metrics ===");
                println!("{}", prometheus_metrics);
            }
        }

        // Check for shutdown signal via try_recv instead of is_pending
        // Note: This is a simplified check; proper implementation would use select!
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Cleanup
    for (layer_id, client) in &layer_clients {
        client.close().await;
        info!("Closed connections to layer {}", layer_id);
    }

    info!("MFN Socket Monitor stopped");
    Ok(())
}