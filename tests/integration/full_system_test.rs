// Full system integration test for MFN
// Tests real connectivity to all 4 layers via Unix sockets
//
// To run these tests:
// cargo test --release --test full_system_test
//
// The test harness automatically:
// - Builds all layer binaries
// - Starts all layer servers
// - Waits for health checks
// - Runs tests
// - Stops all servers and cleans up

mod test_harness;

use std::time::{Duration, Instant};
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};
use std::path::Path;
use std::collections::HashMap;
use test_harness::TestEnvironment;

// Socket paths for all layers
const LAYER1_SOCKET: &str = "/tmp/mfn_layer1.sock";
const LAYER2_SOCKET: &str = "/tmp/mfn_layer2.sock";
const LAYER3_SOCKET: &str = "/tmp/mfn_layer3.sock";
const LAYER4_SOCKET: &str = "/tmp/mfn_layer4.sock";

// Timeout for socket operations
const SOCKET_TIMEOUT: Duration = Duration::from_secs(5);

// ============================================================================
// Layer-Specific Request Builders
// ============================================================================

/// Build Layer 1 (Zig IFR) Ping request
fn build_layer1_ping(request_id: &str) -> Value {
    json!({
        "type": "ping",
        "request_id": request_id
    })
}

/// Build Layer 1 (Zig IFR) AddMemory request
fn build_layer1_add_memory(request_id: &str, content: &str) -> Value {
    json!({
        "type": "add_memory",
        "request_id": request_id,
        "content": content,
        "memory_data": content
    })
}

/// Build Layer 2 (Rust DSR) Ping request
fn build_layer2_ping(request_id: &str) -> Value {
    json!({
        "Ping": {
            "request_id": request_id
        }
    })
}

/// Build Layer 2 (Rust DSR) SimilaritySearch request
fn build_layer2_similarity_search(
    request_id: &str,
    query_embedding: Vec<f32>,
    top_k: usize,
) -> Value {
    json!({
        "SimilaritySearch": {
            "request_id": request_id,
            "query_embedding": query_embedding,
            "top_k": top_k,
            "min_confidence": null,
            "timeout_ms": null
        }
    })
}

/// Build Layer 2 (Rust DSR) AddMemory request
fn build_layer2_add_memory(
    request_id: &str,
    memory_id: u64,
    embedding: Vec<f32>,
    content: String,
) -> Value {
    json!({
        "AddMemory": {
            "request_id": request_id,
            "memory_id": memory_id,
            "embedding": embedding,
            "content": content,
            "tags": null,
            "metadata": null
        }
    })
}

/// Build Layer 3 (Go ALM) Ping request
fn build_layer3_ping(request_id: &str) -> Value {
    json!({
        "type": "ping",
        "request_id": request_id
    })
}

/// Build Layer 3 (Go ALM) Search request
fn build_layer3_search(request_id: &str, query: &str, limit: usize) -> Value {
    json!({
        "type": "search",
        "request_id": request_id,
        "query": query,
        "limit": limit,
        "min_confidence": 0.5
    })
}

/// Build Layer 4 (Rust CPE) Ping request
fn build_layer4_ping(request_id: &str) -> Value {
    json!({
        "type": "Ping",
        "request_id": request_id
    })
}

/// Build Layer 4 (Rust CPE) PredictContext request
fn build_layer4_predict(request_id: &str, context_history: Vec<String>) -> Value {
    json!({
        "type": "predict_context",
        "request_id": request_id,
        "context_history": context_history,
        "max_predictions": 5
    })
}

// ============================================================================
// Generic Response Structure
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct GenericResponse {
    success: bool,
    #[serde(flatten)]
    data: HashMap<String, Value>,
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

// Helper function for Layer 1 (newline-delimited JSON)
async fn send_and_receive_layer1(
    stream: &mut UnixStream,
    message: Value,
) -> Result<Value, String> {
    // Serialize message and add newline
    let mut msg_bytes = serde_json::to_vec(&message)
        .map_err(|e| format!("Failed to serialize message: {}", e))?;
    msg_bytes.push(b'\n');

    // Wrap operations in timeout
    tokio::time::timeout(SOCKET_TIMEOUT, async {
        stream.write_all(&msg_bytes).await
            .map_err(|e| format!("Failed to write message: {}", e))?;

        // Read response until newline
        let mut resp_buf = Vec::new();
        let mut byte_buf = [0u8; 1];

        loop {
            stream.read_exact(&mut byte_buf).await
                .map_err(|e| format!("Failed to read response: {}", e))?;

            if byte_buf[0] == b'\n' {
                break;
            }
            resp_buf.push(byte_buf[0]);

            // Safety check to avoid infinite loop
            if resp_buf.len() > 10_000_000 {
                return Err("Response too large".to_string());
            }
        }

        // Parse response
        serde_json::from_slice(&resp_buf)
            .map_err(|e| format!("Failed to parse response: {} (raw: {:?})",
                e, String::from_utf8_lossy(&resp_buf[..resp_buf.len().min(200)])))
    })
    .await
    .map_err(|_| "Socket operation timed out after 5 seconds".to_string())?
}

// Helper function for Layers 2, 3, 4 (length-prefixed binary protocol)
async fn send_and_receive(
    stream: &mut UnixStream,
    message: Value,
) -> Result<Value, String> {
    // Serialize message
    let msg_bytes = serde_json::to_vec(&message)
        .map_err(|e| format!("Failed to serialize message: {}", e))?;

    // Send message length (4 bytes) + message
    let len = msg_bytes.len() as u32;

    // Wrap operations in timeout
    tokio::time::timeout(SOCKET_TIMEOUT, async {
        stream.write_all(&len.to_le_bytes()).await
            .map_err(|e| format!("Failed to write message length: {}", e))?;
        stream.write_all(&msg_bytes).await
            .map_err(|e| format!("Failed to write message: {}", e))?;

        // Read response length
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await
            .map_err(|e| format!("Failed to read response length: {}", e))?;
        let resp_len = u32::from_le_bytes(len_buf) as usize;

        // Validate response length
        if resp_len == 0 || resp_len > 10_000_000 {
            return Err(format!("Invalid response length: {}", resp_len));
        }

        // Read response
        let mut resp_buf = vec![0u8; resp_len];
        stream.read_exact(&mut resp_buf).await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Parse response
        serde_json::from_slice(&resp_buf)
            .map_err(|e| format!("Failed to parse response: {} (raw: {:?})",
                e, String::from_utf8_lossy(&resp_buf[..resp_buf.len().min(200)])))
    })
    .await
    .map_err(|_| "Socket operation timed out after 5 seconds".to_string())?
}

#[tokio::test]
async fn test_layer_connectivity() {
    println!("\n=== Testing Layer Connectivity ===");

    // Setup test environment (starts all layers automatically)
    let mut env = match TestEnvironment::setup().await {
        Ok(env) => env,
        Err(e) => {
            println!("✗ Failed to setup test environment: {}", e);
            println!("  Make sure all binaries are built:");
            println!("    cargo build --release");
            println!("    cd layer1-zig-ifr && zig build -Doptimize=ReleaseFast");
            println!("    cd layer3-go-alm && go build -o mfn-layer3-server");
            panic!("Test environment setup failed");
        }
    };

    let layer_status = check_layers_running().await;

    println!("\nLayer status:");
    for (layer, is_running) in &layer_status {
        println!("  {} socket: {}", layer, if *is_running { "✓ EXISTS" } else { "✗ NOT FOUND" });
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

    // Cleanup
    env.teardown();
}

#[tokio::test]
async fn test_single_memory_flow() {
    println!("\n=== Testing Single Memory Flow (Ping) ===");

    // Setup test environment (starts all layers automatically)
    let mut env = match TestEnvironment::setup().await {
        Ok(env) => env,
        Err(e) => {
            println!("✗ Failed to setup test environment: {}", e);
            panic!("Test environment setup failed");
        }
    };

    let layer_status = check_layers_running().await;
    let running_count = layer_status.iter().filter(|(_, running)| *running).count();
    println!("Found {} layer(s) running", running_count);

    // Test ping on each layer with correct message formats
    // Layer 1 uses newline-delimited JSON, others use length-prefixed
    for (layer_num, socket_path, builder, use_newline) in [
        (1, LAYER1_SOCKET, build_layer1_ping as fn(&str) -> Value, true),
        (2, LAYER2_SOCKET, build_layer2_ping as fn(&str) -> Value, false),
        (3, LAYER3_SOCKET, build_layer3_ping as fn(&str) -> Value, false),
        (4, LAYER4_SOCKET, build_layer4_ping as fn(&str) -> Value, false),
    ] {
        if !Path::new(socket_path).exists() {
            continue;
        }

        print!("  Testing Layer{} ping... ", layer_num);

        match connect_to_layer(socket_path).await {
            Ok(mut stream) => {
                let start = Instant::now();
                let request_id = format!("test_ping_layer{}_{}", layer_num, uuid::Uuid::new_v4());
                let ping_msg = builder(&request_id);

                let result = if use_newline {
                    send_and_receive_layer1(&mut stream, ping_msg).await
                } else {
                    send_and_receive(&mut stream, ping_msg).await
                };

                match result {
                    Ok(response) => {
                        let elapsed = start.elapsed();
                        // Check for success in various response formats
                        let success = response.get("success")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                            || response.get("type")
                                .and_then(|v| v.as_str())
                                .map(|t| t == "pong" || t == "Pong")
                                .unwrap_or(false);

                        if success {
                            println!("✓ SUCCESS ({:.2} ms)", elapsed.as_secs_f64() * 1000.0);
                        } else {
                            println!("✗ UNEXPECTED RESPONSE: {:?}", response);
                        }
                    }
                    Err(e) => println!("✗ ERROR: {}", e),
                }
            }
            Err(e) => println!("✗ CONNECTION FAILED: {}", e),
        }
    }

    // Cleanup
    env.teardown();
}

#[tokio::test]
async fn test_query_routing() {
    println!("\n=== Testing Query Routing (Ping All Layers) ===");

    // Setup test environment (starts all layers automatically)
    let mut env = match TestEnvironment::setup().await {
        Ok(env) => env,
        Err(e) => {
            println!("✗ Failed to setup test environment: {}", e);
            panic!("Test environment setup failed");
        }
    };

    let layer_status = check_layers_running().await;
    let running_count = layer_status.iter().filter(|(_, running)| *running).count();
    println!("Testing routing to all {} available layer(s):\n", running_count);

    // Test ping routing to each layer
    for (layer_num, socket_path, builder, use_newline) in [
        (1, LAYER1_SOCKET, build_layer1_ping as fn(&str) -> Value, true),
        (2, LAYER2_SOCKET, build_layer2_ping as fn(&str) -> Value, false),
        (3, LAYER3_SOCKET, build_layer3_ping as fn(&str) -> Value, false),
        (4, LAYER4_SOCKET, build_layer4_ping as fn(&str) -> Value, false),
    ] {
        if !Path::new(socket_path).exists() {
            continue;
        }

        print!("  Layer{} routing - ", layer_num);

        match connect_to_layer(socket_path).await {
            Ok(mut stream) => {
                let start = Instant::now();
                let request_id = format!("test_route_layer{}_{}", layer_num, uuid::Uuid::new_v4());
                let ping_msg = builder(&request_id);

                let result = if use_newline {
                    send_and_receive_layer1(&mut stream, ping_msg).await
                } else {
                    send_and_receive(&mut stream, ping_msg).await
                };

                match result {
                    Ok(response) => {
                        let elapsed = start.elapsed();
                        let success = response.get("success")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                            || response.get("type")
                                .and_then(|v| v.as_str())
                                .map(|t| t == "pong" || t == "Pong")
                                .unwrap_or(false);

                        if success {
                            println!("✓ SUCCESS ({:.2} ms)", elapsed.as_secs_f64() * 1000.0);
                        } else {
                            println!("✗ Unexpected response: {:?}", response);
                        }
                    }
                    Err(e) => println!("✗ Error: {}", e),
                }
            }
            Err(e) => println!("✗ Connection failed: {}", e),
        }
    }

    // Cleanup
    env.teardown();
}

#[tokio::test]
async fn test_performance_sanity_check() {
    println!("\n=== Performance Sanity Check ===");

    // Setup test environment (starts all layers automatically)
    let mut env = match TestEnvironment::setup().await {
        Ok(env) => env,
        Err(e) => {
            println!("✗ Failed to setup test environment: {}", e);
            panic!("Test environment setup failed");
        }
    };

    println!("Testing realistic performance expectations:");
    println!("Expected ranges (per ping):");
    println!("  - Layer operations: 200-500 µs");
    println!("  - Network round-trip: 50-200 µs");
    println!("  - Total per request: 250-700 µs\n");

    // Run multiple pings to get average performance
    const NUM_ITERATIONS: usize = 100;

    for (layer_num, socket_path, builder, use_newline) in [
        (1, LAYER1_SOCKET, build_layer1_ping as fn(&str) -> Value, true),
        (2, LAYER2_SOCKET, build_layer2_ping as fn(&str) -> Value, false),
        (3, LAYER3_SOCKET, build_layer3_ping as fn(&str) -> Value, false),
        (4, LAYER4_SOCKET, build_layer4_ping as fn(&str) -> Value, false),
    ] {
        if !Path::new(socket_path).exists() {
            continue;
        }

        print!("  Layer{} performance: ", layer_num);

        let mut latencies = Vec::new();
        let mut errors = 0;

        for i in 0..NUM_ITERATIONS {
            let request_id = format!("perf_test_l{}_i{}", layer_num, i);
            let ping_msg = builder(&request_id);

            match connect_to_layer(socket_path).await {
                Ok(mut stream) => {
                    let start = Instant::now();

                    let result = if use_newline {
                        send_and_receive_layer1(&mut stream, ping_msg).await
                    } else {
                        send_and_receive(&mut stream, ping_msg).await
                    };

                    match result {
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

        // Check if performance is realistic (not fake 5-10 ns)
        if avg_us < 50.0 {
            println!("⚠️  WARNING: Unrealistic latency detected!");
            println!("     Average: {:.2} µs (too low - likely stub implementation)", avg_us);
        } else if avg_us > 10000.0 {
            println!("⚠️  WARNING: High latency detected!");
            println!("     Average: {:.2} ms (>{:.2} ms - performance issue)", avg_us / 1000.0, avg_us / 1000.0);
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

    // Cleanup
    env.teardown();
}

#[tokio::test]
async fn test_concurrent_load() {
    println!("\n=== Testing Concurrent Load ===");

    // Setup test environment (starts all layers automatically)
    let mut env = match TestEnvironment::setup().await {
        Ok(env) => env,
        Err(e) => {
            println!("✗ Failed to setup test environment: {}", e);
            panic!("Test environment setup failed");
        }
    };

    const CONCURRENT_REQUESTS: usize = 10;

    // Test Layer 2 (Rust DSR) as example
    if !Path::new(LAYER2_SOCKET).exists() {
        println!("  Layer2 socket not found, skipping concurrent load test");
        env.teardown();
        return;
    }

    println!("  Testing Layer2 with {} concurrent ping requests", CONCURRENT_REQUESTS);

    let start = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..CONCURRENT_REQUESTS {
        let socket_path = LAYER2_SOCKET.to_string();
        let task = tokio::spawn(async move {
            let request_id = format!("concurrent_test_{}", i);
            let ping_msg = build_layer2_ping(&request_id);

            match connect_to_layer(&socket_path).await {
                Ok(mut stream) => {
                    send_and_receive(&mut stream, ping_msg).await.is_ok()
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

    // Cleanup
    env.teardown();
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