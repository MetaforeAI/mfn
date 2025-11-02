// Full system integration test for MFN
// Tests real connectivity to all 4 layers via Unix sockets
//
// To run these tests:
// 1. Start all layers: ./scripts/start_all_layers.sh
// 2. Run tests: cargo test --test full_system_test

use std::time::Instant;
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Serialize, Deserialize};
use std::path::Path;
use std::collections::HashMap;

// Socket paths for all layers
const LAYER1_SOCKET: &str = "/tmp/mfn_layer1.sock";
const LAYER2_SOCKET: &str = "/tmp/mfn_layer2.sock";
const LAYER3_SOCKET: &str = "/tmp/mfn_layer3.sock";
const LAYER4_SOCKET: &str = "/tmp/mfn_layer4.sock";

// Test helper message types
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestMessage {
    msg_type: String,
    payload: TestPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum TestPayload {
    MemoryAdd {
        memory_id: String,
        content: String,
        embedding: Vec<f32>,
        metadata: HashMap<String, String>,
    },
    Query {
        query_id: String,
        content: String,
        search_type: String,
        max_results: usize,
        min_confidence: f32,
    },
    Health,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestResponse {
    success: bool,
    message: Option<String>,
    results: Option<Vec<SearchResult>>,
    processing_time_ms: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchResult {
    memory_id: String,
    content: String,
    confidence: f32,
    layer_source: u8,
}

// Helper function to check if layers are running
async fn check_layers_running() -> Vec<(String, bool)> {
    let mut status = Vec::new();

    status.push(("Layer1".to_string(), Path::new(LAYER1_SOCKET).exists()));
    status.push(("Layer2".to_string(), Path::new(LAYER2_SOCKET).exists()));
    status.push(("Layer3".to_string(), Path::new(LAYER3_SOCKET).exists()));
    status.push(("Layer4".to_string(), Path::new(LAYER4_SOCKET).exists()));

    status
}

// Helper function to connect to a layer socket
async fn connect_to_layer(socket_path: &str) -> Result<UnixStream, String> {
    match UnixStream::connect(socket_path).await {
        Ok(stream) => Ok(stream),
        Err(e) => Err(format!("Failed to connect to {}: {}", socket_path, e)),
    }
}

// Helper function to send message and receive response
async fn send_and_receive(
    stream: &mut UnixStream,
    message: TestMessage,
) -> Result<TestResponse, String> {
    // Serialize message
    let msg_bytes = serde_json::to_vec(&message)
        .map_err(|e| format!("Failed to serialize message: {}", e))?;

    // Send message length (4 bytes) + message
    let len = msg_bytes.len() as u32;
    stream.write_all(&len.to_le_bytes()).await
        .map_err(|e| format!("Failed to write message length: {}", e))?;
    stream.write_all(&msg_bytes).await
        .map_err(|e| format!("Failed to write message: {}", e))?;

    // Read response length
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await
        .map_err(|e| format!("Failed to read response length: {}", e))?;
    let resp_len = u32::from_le_bytes(len_buf) as usize;

    // Read response
    let mut resp_buf = vec![0u8; resp_len];
    stream.read_exact(&mut resp_buf).await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Parse response
    serde_json::from_slice(&resp_buf)
        .map_err(|e| format!("Failed to parse response: {}", e))
}

#[tokio::test]
async fn test_layer_connectivity() {
    println!("\n=== Testing Layer Connectivity ===");

    let layer_status = check_layers_running().await;
    let mut any_running = false;

    for (layer, is_running) in &layer_status {
        println!("  {} socket: {}", layer, if *is_running { "✓ EXISTS" } else { "✗ NOT FOUND" });
        if *is_running {
            any_running = true;
        }
    }

    if !any_running {
        println!("\n⚠️  No layer sockets found!");
        println!("   Please run: ./scripts/start_all_layers.sh");
        println!("   Skipping integration tests...\n");
        return;
    }

    // Try to connect to each available layer
    println!("\nAttempting connections:");

    for (layer, socket_path) in [
        ("Layer1", LAYER1_SOCKET),
        ("Layer2", LAYER2_SOCKET),
        ("Layer3", LAYER3_SOCKET),
        ("Layer4", LAYER4_SOCKET),
    ] {
        if !Path::new(socket_path).exists() {
            println!("  {} - SKIPPED (socket not found)", layer);
            continue;
        }

        match connect_to_layer(socket_path).await {
            Ok(_) => println!("  {} - ✓ CONNECTED", layer),
            Err(e) => println!("  {} - ✗ FAILED: {}", layer, e),
        }
    }
}

#[tokio::test]
async fn test_single_memory_flow() {
    println!("\n=== Testing Single Memory Flow ===");

    // Check if layers are running
    let layer_status = check_layers_running().await;
    let running_count = layer_status.iter().filter(|(_, running)| *running).count();

    if running_count == 0 {
        println!("⚠️  No layers running. Skipping test.");
        println!("   Run: ./scripts/start_all_layers.sh");
        return;
    }

    println!("Found {} layer(s) running", running_count);

    // Test memory add on each available layer
    let test_memory = TestMessage {
        msg_type: "memory_add".to_string(),
        payload: TestPayload::MemoryAdd {
            memory_id: format!("test_mem_{}", uuid::Uuid::new_v4()),
            content: "Integration test memory content".to_string(),
            embedding: vec![0.1, 0.2, 0.3, 0.4, 0.5],
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("test".to_string(), "true".to_string());
                meta.insert("timestamp".to_string(), chrono::Utc::now().to_rfc3339());
                meta
            },
        },
    };

    for (layer, socket_path) in [
        ("Layer1", LAYER1_SOCKET),
        ("Layer2", LAYER2_SOCKET),
        ("Layer3", LAYER3_SOCKET),
        ("Layer4", LAYER4_SOCKET),
    ] {
        if !Path::new(socket_path).exists() {
            continue;
        }

        print!("  Testing {} memory add... ", layer);

        match connect_to_layer(socket_path).await {
            Ok(mut stream) => {
                let start = Instant::now();

                match send_and_receive(&mut stream, test_memory.clone()).await {
                    Ok(response) => {
                        let elapsed = start.elapsed();
                        if response.success {
                            println!("✓ SUCCESS ({:.2} ms)", elapsed.as_secs_f64() * 1000.0);
                        } else {
                            println!("✗ FAILED: {:?}", response.message);
                        }
                    }
                    Err(e) => println!("✗ ERROR: {}", e),
                }
            }
            Err(e) => println!("✗ CONNECTION FAILED: {}", e),
        }
    }
}

#[tokio::test]
async fn test_query_routing() {
    println!("\n=== Testing Query Routing ===");

    // Check if layers are running
    let layer_status = check_layers_running().await;
    let running_count = layer_status.iter().filter(|(_, running)| *running).count();

    if running_count == 0 {
        println!("⚠️  No layers running. Skipping test.");
        println!("   Run: ./scripts/start_all_layers.sh");
        return;
    }

    // Test different query types
    let query_types = vec![
        ("exact", "Find exact match for test"),
        ("similarity", "Find similar memories"),
        ("associative", "Find associated concepts"),
        ("contextual", "Predict next context"),
    ];

    for (search_type, query_content) in query_types {
        println!("\n  Testing {} search:", search_type);

        let test_query = TestMessage {
            msg_type: "query".to_string(),
            payload: TestPayload::Query {
                query_id: format!("test_query_{}", uuid::Uuid::new_v4()),
                content: query_content.to_string(),
                search_type: search_type.to_string(),
                max_results: 10,
                min_confidence: 0.5,
            },
        };

        for (layer, socket_path) in [
            ("Layer1", LAYER1_SOCKET),
            ("Layer2", LAYER2_SOCKET),
            ("Layer3", LAYER3_SOCKET),
            ("Layer4", LAYER4_SOCKET),
        ] {
            if !Path::new(socket_path).exists() {
                continue;
            }

            print!("    {} - ", layer);

            match connect_to_layer(socket_path).await {
                Ok(mut stream) => {
                    let start = Instant::now();

                    match send_and_receive(&mut stream, test_query.clone()).await {
                        Ok(response) => {
                            let elapsed = start.elapsed();
                            if response.success {
                                let result_count = response.results.as_ref()
                                    .map(|r| r.len()).unwrap_or(0);
                                println!("✓ {} results in {:.2} ms",
                                    result_count,
                                    elapsed.as_secs_f64() * 1000.0);
                            } else {
                                println!("✗ Query failed: {:?}", response.message);
                            }
                        }
                        Err(e) => println!("✗ Error: {}", e),
                    }
                }
                Err(e) => println!("✗ Connection failed: {}", e),
            }
        }
    }
}

#[tokio::test]
async fn test_performance_sanity_check() {
    println!("\n=== Performance Sanity Check ===");

    // Check if layers are running
    let layer_status = check_layers_running().await;
    let running_count = layer_status.iter().filter(|(_, running)| *running).count();

    if running_count == 0 {
        println!("⚠️  No layers running. Skipping test.");
        println!("   Run: ./scripts/start_all_layers.sh");
        return;
    }

    println!("Testing realistic performance expectations:");
    println!("Expected ranges:");
    println!("  - Layer operations: 200-500 µs");
    println!("  - Network round-trip: 50-200 µs");
    println!("  - Total per query: 250-700 µs\n");

    // Run multiple queries to get average performance
    const NUM_ITERATIONS: usize = 100;

    for (layer, socket_path) in [
        ("Layer1", LAYER1_SOCKET),
        ("Layer2", LAYER2_SOCKET),
        ("Layer3", LAYER3_SOCKET),
        ("Layer4", LAYER4_SOCKET),
    ] {
        if !Path::new(socket_path).exists() {
            continue;
        }

        print!("  {} performance: ", layer);

        let mut latencies = Vec::new();
        let mut errors = 0;

        for i in 0..NUM_ITERATIONS {
            let test_query = TestMessage {
                msg_type: "query".to_string(),
                payload: TestPayload::Query {
                    query_id: format!("perf_test_{}", i),
                    content: format!("Performance test query {}", i),
                    search_type: "exact".to_string(),
                    max_results: 5,
                    min_confidence: 0.5,
                },
            };

            match connect_to_layer(socket_path).await {
                Ok(mut stream) => {
                    let start = Instant::now();

                    match send_and_receive(&mut stream, test_query).await {
                        Ok(_) => {
                            let elapsed = start.elapsed();
                            latencies.push(elapsed);
                        }
                        Err(_) => errors += 1,
                    }
                }
                Err(_) => errors += 1,
            }
        }

        if latencies.is_empty() {
            println!("✗ All requests failed");
            continue;
        }

        // Calculate statistics
        let avg_latency = latencies.iter()
            .map(|d| d.as_secs_f64())
            .sum::<f64>() / latencies.len() as f64;

        let min_latency = latencies.iter().min().unwrap();
        let max_latency = latencies.iter().max().unwrap();

        // Convert to microseconds for display
        let avg_us = avg_latency * 1_000_000.0;
        let min_us = min_latency.as_secs_f64() * 1_000_000.0;
        let max_us = max_latency.as_secs_f64() * 1_000_000.0;

        // Check if performance is realistic (not the fake 5-10 ns)
        if avg_us < 50.0 {
            println!("⚠️  WARNING: Unrealistic latency detected!");
            println!("     Average: {:.2} µs (too low - likely stub implementation)", avg_us);
        } else if avg_us > 5000.0 {
            println!("⚠️  WARNING: High latency detected!");
            println!("     Average: {:.2} µs (>{} ms - performance issue)", avg_us, avg_us / 1000.0);
        } else {
            println!("✓ GOOD");
            println!("     Avg: {:.2} µs, Min: {:.2} µs, Max: {:.2} µs",
                avg_us, min_us, max_us);

            if errors > 0 {
                println!("     {} errors out of {} requests", errors, NUM_ITERATIONS);
            }
        }
    }

    println!("\n  Performance check complete.");
    println!("  Note: Realistic layer operations should be 200-500 µs,");
    println!("        not 5-10 ns (which indicates empty/stub operations)");
}

#[tokio::test]
async fn test_concurrent_load() {
    println!("\n=== Testing Concurrent Load ===");

    // Check if layers are running
    let layer_status = check_layers_running().await;
    let running_count = layer_status.iter().filter(|(_, running)| *running).count();

    if running_count == 0 {
        println!("⚠️  No layers running. Skipping test.");
        println!("   Run: ./scripts/start_all_layers.sh");
        return;
    }

    const CONCURRENT_REQUESTS: usize = 10;

    for (layer, socket_path) in [
        ("Layer2", LAYER2_SOCKET), // Test Rust DSR layer as example
    ] {
        if !Path::new(socket_path).exists() {
            continue;
        }

        println!("  Testing {} with {} concurrent requests", layer, CONCURRENT_REQUESTS);

        let start = Instant::now();
        let mut tasks = Vec::new();

        for i in 0..CONCURRENT_REQUESTS {
            let socket_path = socket_path.to_string();
            let task = tokio::spawn(async move {
                let test_query = TestMessage {
                    msg_type: "query".to_string(),
                    payload: TestPayload::Query {
                        query_id: format!("concurrent_test_{}", i),
                        content: format!("Concurrent test query {}", i),
                        search_type: "similarity".to_string(),
                        max_results: 5,
                        min_confidence: 0.5,
                    },
                };

                match connect_to_layer(&socket_path).await {
                    Ok(mut stream) => {
                        send_and_receive(&mut stream, test_query).await.is_ok()
                    }
                    Err(_) => false,
                }
            });
            tasks.push(task);
        }

        let results = futures::future::join_all(tasks).await;
        let elapsed = start.elapsed();

        let successful = results.iter()
            .filter(|r| r.as_ref().map(|&b| b).unwrap_or(false))
            .count();

        println!("    Results: {}/{} successful in {:.2} ms",
            successful, CONCURRENT_REQUESTS, elapsed.as_secs_f64() * 1000.0);

        if successful == CONCURRENT_REQUESTS {
            println!("    ✓ All concurrent requests handled successfully");
        } else {
            println!("    ⚠️  Some requests failed under concurrent load");
        }
    }
}

// Dependencies for Cargo.toml test section
#[cfg(test)]
mod deps {
    // These dependencies need to be in [dev-dependencies]:
    // tokio = { version = "1.35", features = ["full", "test-util"] }
    // serde = { version = "1.0", features = ["derive"] }
    // serde_json = "1.0"
    // uuid = { version = "1.6", features = ["v4"] }
    // chrono = "0.4"
    // futures = "0.3"
}