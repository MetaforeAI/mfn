//! Health Check Utility for MFN Layers
//!
//! Provides functions to check the health status of individual layers
//! and the entire MFN system.

use std::time::Duration;
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Health status for a single layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub layer_name: String,
    pub layer_id: u8,
    pub status: String,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub metrics: serde_json::Value,
    pub response_time_ms: f64,
}

/// Combined health status for all layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthStatus {
    pub overall_status: String,
    pub layers: Vec<HealthStatus>,
    pub healthy_count: usize,
    pub total_count: usize,
    pub timestamp: u64,
}

/// Layer socket paths
const LAYER_SOCKETS: [(&str, u8, &str); 5] = [
    ("Layer1_IFR", 1, "/tmp/mfn_test_layer1.sock"),
    ("Layer2_DSR", 2, "/tmp/mfn_test_layer2.sock"),
    ("Layer3_ALM", 3, "/tmp/mfn_test_layer3.sock"),
    ("Layer4_CPE", 4, "/tmp/mfn_test_layer4.sock"),
    ("Layer5_PSR", 5, "/tmp/mfn_test_layer5.sock"),
];

/// Check health of a single layer via Unix socket
pub async fn check_layer_health(
    layer_name: &str,
    layer_id: u8,
    socket_path: &str,
) -> Result<HealthStatus> {
    let start_time = std::time::Instant::now();

    // Connect to layer socket with timeout
    let stream = tokio::time::timeout(
        Duration::from_secs(2),
        UnixStream::connect(socket_path)
    ).await
        .map_err(|_| anyhow!("Connection timeout to {}", layer_name))?
        .map_err(|e| anyhow!("Failed to connect to {}: {}", layer_name, e))?;

    let (mut read_half, mut write_half) = stream.into_split();

    // Build health check request
    let request = serde_json::json!({
        "type": "HealthCheck",
        "request_id": uuid::Uuid::new_v4().to_string(),
    });

    let request_json = serde_json::to_string(&request)?;
    let request_bytes = request_json.as_bytes();
    let request_len = request_bytes.len() as u32;

    // Send request: 4-byte length + JSON payload
    write_half.write_all(&request_len.to_le_bytes()).await?;
    write_half.write_all(request_bytes).await?;

    // Read response: 4-byte length + JSON payload
    let mut len_buf = [0u8; 4];
    tokio::time::timeout(
        Duration::from_secs(2),
        read_half.read_exact(&mut len_buf)
    ).await
        .map_err(|_| anyhow!("Response timeout from {}", layer_name))??;

    let response_len = u32::from_le_bytes(len_buf) as usize;

    if response_len == 0 || response_len > 1_000_000 {
        return Err(anyhow!("Invalid response length from {}: {}", layer_name, response_len));
    }

    let mut response_buf = vec![0u8; response_len];
    tokio::time::timeout(
        Duration::from_secs(2),
        read_half.read_exact(&mut response_buf)
    ).await
        .map_err(|_| anyhow!("Response read timeout from {}", layer_name))??;

    // Parse response
    let response_str = std::str::from_utf8(&response_buf)?;
    let response: serde_json::Value = serde_json::from_str(response_str)?;

    let response_time_ms = start_time.elapsed().as_millis() as f64;

    // Extract health information from response
    let status = response.get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let timestamp = response.get("timestamp")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let uptime_seconds = response.get("uptime_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let metrics = response.get("metrics")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    Ok(HealthStatus {
        layer_name: layer_name.to_string(),
        layer_id,
        status,
        timestamp,
        uptime_seconds,
        metrics,
        response_time_ms,
    })
}

/// Check health of all MFN layers
pub async fn check_all_layers() -> Result<SystemHealthStatus> {
    debug!("Checking health of all MFN layers...");

    let mut health_statuses = Vec::new();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // Check each layer in parallel
    let mut handles = Vec::new();

    for (layer_name, layer_id, socket_path) in LAYER_SOCKETS.iter() {
        let name = layer_name.to_string();
        let id = *layer_id;
        let path = socket_path.to_string();

        let handle = tokio::spawn(async move {
            check_layer_health(&name, id, &path).await
        });

        handles.push(handle);
    }

    // Collect results
    for handle in handles {
        match handle.await {
            Ok(Ok(status)) => {
                debug!("✅ {} is {}", status.layer_name, status.status);
                health_statuses.push(status);
            }
            Ok(Err(e)) => {
                warn!("❌ Layer health check failed: {}", e);
            }
            Err(e) => {
                warn!("❌ Task failed: {}", e);
            }
        }
    }

    let healthy_count = health_statuses
        .iter()
        .filter(|s| s.status == "healthy")
        .count();

    let total_count = LAYER_SOCKETS.len();

    let overall_status = if healthy_count == total_count {
        "healthy".to_string()
    } else if healthy_count > 0 {
        "degraded".to_string()
    } else {
        "unhealthy".to_string()
    };

    Ok(SystemHealthStatus {
        overall_status,
        layers: health_statuses,
        healthy_count,
        total_count,
        timestamp,
    })
}

/// Print health status report to stdout
pub fn print_health_report(status: &SystemHealthStatus) {
    println!("\n🏥 MFN System Health Report");
    println!("═══════════════════════════════════════════════════");
    println!("Overall Status: {}", status.overall_status.to_uppercase());
    println!("Healthy Layers: {}/{}", status.healthy_count, status.total_count);
    println!("Timestamp: {}", status.timestamp);
    println!("\n📊 Layer Details:");
    println!("───────────────────────────────────────────────────");

    for layer in &status.layers {
        println!("\n{} (Layer {})", layer.layer_name, layer.layer_id);
        println!("  Status: {}", layer.status);
        println!("  Uptime: {}s", layer.uptime_seconds);
        println!("  Response Time: {:.2}ms", layer.response_time_ms);

        if let Some(metrics) = layer.metrics.as_object() {
            println!("  Metrics:");
            for (key, value) in metrics {
                println!("    {}: {}", key, value);
            }
        }
    }

    println!("\n═══════════════════════════════════════════════════\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_format() {
        let status = HealthStatus {
            layer_name: "TestLayer".to_string(),
            layer_id: 1,
            status: "healthy".to_string(),
            timestamp: 1234567890,
            uptime_seconds: 3600,
            metrics: serde_json::json!({"test": "value"}),
            response_time_ms: 10.5,
        };

        assert_eq!(status.layer_name, "TestLayer");
        assert_eq!(status.status, "healthy");
        assert_eq!(status.uptime_seconds, 3600);
    }

    #[tokio::test]
    async fn test_system_health_status() {
        let system_status = SystemHealthStatus {
            overall_status: "healthy".to_string(),
            layers: vec![],
            healthy_count: 0,
            total_count: 4,
            timestamp: 1234567890,
        };

        assert_eq!(system_status.overall_status, "healthy");
        assert_eq!(system_status.total_count, 4);
    }
}
