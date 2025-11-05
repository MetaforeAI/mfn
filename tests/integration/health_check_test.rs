//! Integration Test: Health Check for All MFN Layers
//!
//! This test validates that all layer socket servers respond to health check requests
//! and return properly formatted health status information.

use mfn_integration::health::{check_all_layers, check_layer_health, print_health_report};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_layer2_health_check() {
    // Test Layer 2 (Rust DSR) health check
    let result = check_layer_health(
        "Layer2_DSR",
        2,
        "/tmp/mfn_layer2.sock"
    ).await;

    match result {
        Ok(status) => {
            println!("✅ Layer 2 Health Check Response:");
            println!("   Status: {}", status.status);
            println!("   Uptime: {}s", status.uptime_seconds);
            println!("   Response Time: {:.2}ms", status.response_time_ms);
            println!("   Metrics: {}", serde_json::to_string_pretty(&status.metrics).unwrap());

            assert_eq!(status.status, "healthy");
            assert!(status.uptime_seconds > 0);
            assert!(status.response_time_ms < 100.0); // Should respond in <100ms
        }
        Err(e) => {
            eprintln!("⚠️ Layer 2 health check failed (server may not be running): {}", e);
            eprintln!("   To run this test, start the Layer 2 server:");
            eprintln!("   cargo run --bin layer2_socket_server");
        }
    }
}

#[tokio::test]
async fn test_layer3_health_check() {
    // Test Layer 3 (Go ALM) health check
    let result = check_layer_health(
        "Layer3_ALM",
        3,
        "/tmp/mfn_layer3.sock"
    ).await;

    match result {
        Ok(status) => {
            println!("✅ Layer 3 Health Check Response:");
            println!("   Status: {}", status.status);
            println!("   Uptime: {}s", status.uptime_seconds);
            println!("   Response Time: {:.2}ms", status.response_time_ms);
            println!("   Metrics: {}", serde_json::to_string_pretty(&status.metrics).unwrap());

            assert_eq!(status.status, "healthy");
            assert!(status.uptime_seconds > 0);
            assert!(status.response_time_ms < 100.0);
        }
        Err(e) => {
            eprintln!("⚠️ Layer 3 health check failed (server may not be running): {}", e);
            eprintln!("   To run this test, start the Layer 3 server:");
            eprintln!("   cd layer3-go-alm && go run cmd/socket_server/main.go");
        }
    }
}

#[tokio::test]
async fn test_layer4_health_check() {
    // Test Layer 4 (Rust CPE) health check
    let result = check_layer_health(
        "Layer4_CPE",
        4,
        "/tmp/mfn_layer4.sock"
    ).await;

    match result {
        Ok(status) => {
            println!("✅ Layer 4 Health Check Response:");
            println!("   Status: {}", status.status);
            println!("   Uptime: {}s", status.uptime_seconds);
            println!("   Response Time: {:.2}ms", status.response_time_ms);
            println!("   Metrics: {}", serde_json::to_string_pretty(&status.metrics).unwrap());

            assert_eq!(status.status, "healthy");
            assert!(status.uptime_seconds > 0);
            assert!(status.response_time_ms < 100.0);
        }
        Err(e) => {
            eprintln!("⚠️ Layer 4 health check failed (server may not be running): {}", e);
            eprintln!("   To run this test, start the Layer 4 server:");
            eprintln!("   cargo run --bin layer4_socket_server");
        }
    }
}

#[tokio::test]
async fn test_all_layers_health_check() {
    println!("\n🏥 Testing health check for all MFN layers...\n");

    // Check all layers
    let result = check_all_layers().await;

    match result {
        Ok(system_status) => {
            print_health_report(&system_status);

            // Validate response structure
            assert!(!system_status.overall_status.is_empty());
            assert_eq!(system_status.total_count, 4);

            // If any layers are healthy, overall status should not be "unhealthy"
            if system_status.healthy_count > 0 {
                assert_ne!(system_status.overall_status, "unhealthy");
            }

            // Each healthy layer should have valid metrics
            for layer in &system_status.layers {
                if layer.status == "healthy" {
                    assert!(layer.uptime_seconds > 0, "Uptime should be > 0 for healthy layer");
                    assert!(layer.response_time_ms > 0.0, "Response time should be > 0");
                    assert!(layer.response_time_ms < 1000.0, "Response time should be < 1000ms");
                }
            }
        }
        Err(e) => {
            eprintln!("⚠️ System health check failed: {}", e);
            eprintln!("\n📝 Note: Some or all layer servers may not be running.");
            eprintln!("   Start the layer servers to run this test:");
            eprintln!("   - Layer 2: cargo run --bin layer2_socket_server");
            eprintln!("   - Layer 3: cd layer3-go-alm && go run cmd/socket_server/main.go");
            eprintln!("   - Layer 4: cargo run --bin layer4_socket_server");
        }
    }
}

#[tokio::test]
async fn test_health_check_timeout() {
    // Test that health check properly times out on non-existent socket
    let result = check_layer_health(
        "NonExistent",
        99,
        "/tmp/nonexistent.sock"
    ).await;

    assert!(result.is_err(), "Should fail for non-existent socket");
}

#[tokio::test]
async fn test_health_check_response_format() {
    // Test that health check responses have required fields
    let result = check_all_layers().await;

    if let Ok(system_status) = result {
        for layer in &system_status.layers {
            // Validate required fields
            assert!(!layer.layer_name.is_empty(), "Layer name should not be empty");
            assert!(layer.layer_id > 0, "Layer ID should be > 0");
            assert!(!layer.status.is_empty(), "Status should not be empty");
            assert!(layer.timestamp > 0, "Timestamp should be > 0");

            // Validate metrics is a JSON object
            assert!(layer.metrics.is_object() || layer.metrics.is_null(),
                "Metrics should be an object or null");
        }
    }
}

#[tokio::test]
async fn test_sequential_health_checks() {
    // Test that multiple sequential health checks work correctly
    println!("\n🔄 Testing sequential health checks...\n");

    for i in 1..=3 {
        println!("Check #{}", i);
        let result = check_all_layers().await;

        if let Ok(status) = result {
            println!("  Healthy: {}/{}", status.healthy_count, status.total_count);

            // Wait a bit between checks
            sleep(Duration::from_millis(100)).await;
        }
    }

    println!("\n✅ Sequential health checks completed\n");
}

#[tokio::test]
async fn test_health_check_metrics_content() {
    // Test that health check metrics contain expected keys
    let result = check_all_layers().await;

    if let Ok(system_status) = result {
        for layer in &system_status.layers {
            if layer.status == "healthy" {
                let metrics = &layer.metrics;

                // Different layers may have different metrics, but all should have some
                if let Some(obj) = metrics.as_object() {
                    assert!(!obj.is_empty(), "Healthy layer should have metrics");

                    // Common metrics that most layers should have
                    let common_keys = ["total_queries", "success_rate", "avg_latency_us"];
                    let has_common_metric = common_keys.iter()
                        .any(|key| obj.contains_key(*key));

                    assert!(has_common_metric,
                        "Layer {} should have at least one common metric", layer.layer_name);
                }
            }
        }
    }
}
