//! Layer 2 DSR Socket Interface Demo
//! 
//! Demonstrates the Unix socket interface for Layer 2 DSR operations.
//! Shows both JSON and binary protocol usage with performance comparisons.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use anyhow::{Result, anyhow};
use ndarray::Array1;
use serde_json;
use tokio::net::UnixStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::sleep;

use mfn_layer2_dsr::{
    DynamicSimilarityReservoir, DSRConfig, MemoryId, Embedding,
    SocketServer, SocketServerConfig, SocketRequest, SocketResponse,
    BinarySerializer, BinaryDeserializer, BinaryMessageType,
};

const DEMO_SOCKET_PATH: &str = "/tmp/mfn_layer2_demo.sock";

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::init();
    
    println!("🧠 Layer 2 DSR Socket Interface Demo");
    println!("====================================");
    
    // Create DSR instance
    let mut config = DSRConfig::default();
    config.reservoir_size = 1000;
    config.embedding_dim = 384; // Typical sentence transformer dimension
    
    let dsr = Arc::new(DynamicSimilarityReservoir::new(config)?);
    println!("✅ DSR instance created with {} neurons", dsr.get_performance_stats().reservoir_size);
    
    // Create and start socket server
    let socket_config = SocketServerConfig {
        socket_path: DEMO_SOCKET_PATH.to_string(),
        max_connections: 10,
        enable_binary_protocol: true,
        enable_json_protocol: true,
        ..Default::default()
    };
    
    let mut socket_server = SocketServer::new(dsr.clone(), Some(socket_config));
    
    // Start server in background
    let server_handle = {
        let mut server = socket_server;
        tokio::spawn(async move {
            if let Err(e) = server.start().await {
                eprintln!("Socket server error: {}", e);
            }
        })
    };
    
    // Give server time to start
    sleep(Duration::from_millis(200)).await;
    println!("✅ Socket server started on {}", DEMO_SOCKET_PATH);
    
    // Run demonstrations
    demo_json_protocol().await?;
    demo_performance_comparison().await?;
    demo_concurrent_operations().await?;
    
    // Cleanup
    server_handle.abort();
    let _ = std::fs::remove_file(DEMO_SOCKET_PATH);
    
    println!("🎯 Demo completed successfully!");
    
    Ok(())
}

/// Demonstrate JSON protocol usage
async fn demo_json_protocol() -> Result<()> {
    println!("\n📡 JSON Protocol Demonstration");
    println!("------------------------------");
    
    let mut stream = UnixStream::connect(DEMO_SOCKET_PATH).await?;
    let mut reader = BufReader::new(&stream);
    
    // Test 1: Ping
    println!("1️⃣  Testing ping...");
    let ping_request = SocketRequest::Ping {
        request_id: "ping-test".to_string(),
    };
    
    let request_line = format!("{}\n", serde_json::to_string(&ping_request)?);
    stream.write_all(request_line.as_bytes()).await?;
    
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;
    
    match serde_json::from_str::<SocketResponse>(response_line.trim())? {
        SocketResponse::Pong { layer, version, .. } => {
            println!("   ✅ Pong received: {} v{}", layer, version);
        },
        response => println!("   ❌ Unexpected response: {:?}", response),
    }
    
    // Test 2: Add memory
    println!("2️⃣  Adding test memories...");
    let test_memories = vec![
        (1001, "Neural networks learn from data patterns", vec!["AI", "machine learning", "neural networks"]),
        (1002, "Transformers revolutionized natural language processing", vec!["AI", "NLP", "transformers"]),
        (1003, "The human brain has billions of interconnected neurons", vec!["neuroscience", "brain", "neurons"]),
        (1004, "Memory systems store and retrieve information efficiently", vec!["memory", "storage", "retrieval"]),
        (1005, "Spiking neural networks mimic biological neural dynamics", vec!["SNN", "biology", "dynamics"]),
    ];
    
    for (memory_id, content, tags) in &test_memories {
        let embedding = generate_mock_embedding(content);
        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), "demo".to_string());
        metadata.insert("type".to_string(), "test_memory".to_string());
        
        let add_request = SocketRequest::AddMemory {
            request_id: format!("add-{}", memory_id),
            memory_id: *memory_id,
            embedding: embedding.to_vec(),
            content: content.to_string(),
            tags: Some(tags.clone()),
            metadata: Some(metadata),
        };
        
        let request_line = format!("{}\n", serde_json::to_string(&add_request)?);
        stream.write_all(request_line.as_bytes()).await?;
        
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;
        
        match serde_json::from_str::<SocketResponse>(response_line.trim())? {
            SocketResponse::Success { processing_time_ms, .. } => {
                println!("   ✅ Memory {} added in {:.2}ms", memory_id, processing_time_ms);
            },
            SocketResponse::Error { error, .. } => {
                println!("   ❌ Failed to add memory {}: {}", memory_id, error);
            },
            _ => println!("   ❓ Unexpected response for memory {}", memory_id),
        }
    }
    
    // Test 3: Similarity search
    println!("3️⃣  Testing similarity search...");
    let query_embedding = generate_mock_embedding("artificial neural network learning");
    let search_request = SocketRequest::SimilaritySearch {
        request_id: "search-test".to_string(),
        query_embedding: query_embedding.to_vec(),
        top_k: 3,
        min_confidence: Some(0.3),
        timeout_ms: Some(5000),
    };
    
    let request_line = format!("{}\n", serde_json::to_string(&search_request)?);
    stream.write_all(request_line.as_bytes()).await?;
    
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;
    
    match serde_json::from_str::<SocketResponse>(response_line.trim())? {
        SocketResponse::Success { data, processing_time_ms, .. } => {
            println!("   ✅ Search completed in {:.2}ms", processing_time_ms);
            if let Some(matches) = data.get("matches") {
                if let Some(matches_array) = matches.as_array() {
                    for (i, match_item) in matches_array.iter().enumerate() {
                        if let (Some(id), Some(conf), Some(content)) = (
                            match_item.get("memory_id"),
                            match_item.get("confidence"),
                            match_item.get("content")
                        ) {
                            println!("     {}. Memory {} (conf: {:.3}): {}",
                                i + 1, id, conf, content.as_str().unwrap_or(""));
                        }
                    }
                } else {
                    println!("   📊 No matches found");
                }
            }
        },
        SocketResponse::Error { error, .. } => {
            println!("   ❌ Search failed: {}", error);
        },
        _ => println!("   ❓ Unexpected search response"),
    }
    
    // Test 4: Get stats
    println!("4️⃣  Getting performance statistics...");
    let stats_request = SocketRequest::GetStats {
        request_id: "stats-test".to_string(),
    };
    
    let request_line = format!("{}\n", serde_json::to_string(&stats_request)?);
    stream.write_all(request_line.as_bytes()).await?;
    
    let mut response_line = String::new();
    reader.read_line(&mut response_line).await?;
    
    match serde_json::from_str::<SocketResponse>(response_line.trim())? {
        SocketResponse::Success { data, processing_time_ms, .. } => {
            println!("   ✅ Stats retrieved in {:.2}ms:", processing_time_ms);
            println!("     🔍 Total queries: {}", data.get("total_queries").unwrap_or(&serde_json::Value::Null));
            println!("     ➕ Total additions: {}", data.get("total_additions").unwrap_or(&serde_json::Value::Null));
            println!("     🎯 Wells count: {}", data.get("similarity_wells_count").unwrap_or(&serde_json::Value::Null));
            println!("     💾 Memory usage: {:.1}MB", data.get("memory_usage_mb").unwrap_or(&serde_json::Value::Null));
        },
        SocketResponse::Error { error, .. } => {
            println!("   ❌ Stats failed: {}", error);
        },
        _ => println!("   ❓ Unexpected stats response"),
    }
    
    Ok(())
}

/// Demonstrate performance comparison between JSON and binary protocols
async fn demo_performance_comparison() -> Result<()> {
    println!("\n⚡ Performance Comparison");
    println!("------------------------");
    
    // For this demo, we'll just show JSON performance
    // Binary protocol would require implementing the binary client side
    println!("🔬 JSON Protocol Performance Test");
    
    let mut stream = UnixStream::connect(DEMO_SOCKET_PATH).await?;
    let mut reader = BufReader::new(&stream);
    
    let iterations = 50;
    let mut total_time = Duration::ZERO;
    
    println!("   Running {} similarity searches...", iterations);
    
    for i in 0..iterations {
        let query_embedding = generate_mock_embedding(&format!("test query number {}", i));
        let search_request = SocketRequest::SimilaritySearch {
            request_id: format!("perf-test-{}", i),
            query_embedding: query_embedding.to_vec(),
            top_k: 5,
            min_confidence: None,
            timeout_ms: None,
        };
        
        let start = Instant::now();
        
        let request_line = format!("{}\n", serde_json::to_string(&search_request)?);
        stream.write_all(request_line.as_bytes()).await?;
        
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;
        
        let elapsed = start.elapsed();
        total_time += elapsed;
        
        // Verify we got a response
        let _: SocketResponse = serde_json::from_str(response_line.trim())?;
        
        if i % 10 == 0 {
            println!("     Completed {} searches...", i);
        }
    }
    
    let avg_time = total_time / iterations;
    println!("   ✅ Average search time: {:.2}ms", avg_time.as_secs_f32() * 1000.0);
    println!("   🎯 Target achieved: {} (target: <5ms)", 
        if avg_time.as_millis() < 5 { "YES" } else { "NO" });
    
    Ok(())
}

/// Demonstrate concurrent operations
async fn demo_concurrent_operations() -> Result<()> {
    println!("\n🚀 Concurrent Operations Test");
    println!("-----------------------------");
    
    let concurrent_clients = 5;
    let operations_per_client = 10;
    
    println!("   Spawning {} concurrent clients...", concurrent_clients);
    
    let mut handles = vec![];
    let start_time = Instant::now();
    
    for client_id in 0..concurrent_clients {
        let handle = tokio::spawn(async move {
            let mut client_results = vec![];
            
            match UnixStream::connect(DEMO_SOCKET_PATH).await {
                Ok(mut stream) => {
                    let mut reader = BufReader::new(&stream);
                    
                    for op_id in 0..operations_per_client {
                        let query_embedding = generate_mock_embedding(
                            &format!("concurrent query from client {} operation {}", client_id, op_id)
                        );
                        
                        let search_request = SocketRequest::SimilaritySearch {
                            request_id: format!("concurrent-{}-{}", client_id, op_id),
                            query_embedding: query_embedding.to_vec(),
                            top_k: 3,
                            min_confidence: None,
                            timeout_ms: Some(10000),
                        };
                        
                        let op_start = Instant::now();
                        
                        let request_line = format!("{}\n", serde_json::to_string(&search_request).unwrap());
                        if stream.write_all(request_line.as_bytes()).await.is_ok() {
                            let mut response_line = String::new();
                            if reader.read_line(&mut response_line).await.is_ok() {
                                if let Ok(response) = serde_json::from_str::<SocketResponse>(response_line.trim()) {
                                    match response {
                                        SocketResponse::Success { .. } => {
                                            client_results.push(op_start.elapsed());
                                        },
                                        _ => {},
                                    }
                                }
                            }
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Client {} failed to connect: {}", client_id, e);
                }
            }
            
            (client_id, client_results)
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut all_times = vec![];
    let mut successful_clients = 0;
    
    for handle in handles {
        if let Ok((client_id, times)) = handle.await {
            if !times.is_empty() {
                successful_clients += 1;
                all_times.extend(times);
                println!("   ✅ Client {} completed {} operations", client_id, times.len());
            } else {
                println!("   ❌ Client {} failed", client_id);
            }
        }
    }
    
    let total_time = start_time.elapsed();
    
    if !all_times.is_empty() {
        let avg_time = all_times.iter().sum::<Duration>() / all_times.len() as u32;
        let total_operations = all_times.len();
        let ops_per_second = total_operations as f64 / total_time.as_secs_f64();
        
        println!("   📊 Results:");
        println!("     • Successful clients: {}/{}", successful_clients, concurrent_clients);
        println!("     • Total operations: {}", total_operations);
        println!("     • Average operation time: {:.2}ms", avg_time.as_secs_f32() * 1000.0);
        println!("     • Throughput: {:.1} ops/second", ops_per_second);
        println!("     • Total test time: {:.2}s", total_time.as_secs_f32());
    } else {
        println!("   ❌ No successful operations completed");
    }
    
    Ok(())
}

/// Generate a mock embedding for demonstration purposes
/// In a real implementation, this would use a proper embedding model
fn generate_mock_embedding(text: &str) -> Array1<f32> {
    let dim = 384;
    let mut embedding = vec![0.0f32; dim];
    
    // Simple hash-based mock embedding
    let mut hash = 0u64;
    for byte in text.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    
    // Generate deterministic but varied values
    for i in 0..dim {
        let seed = hash.wrapping_add(i as u64);
        let normalized = (seed % 2000) as f32 / 2000.0 - 0.5; // [-0.5, 0.5]
        embedding[i] = normalized;
    }
    
    // Normalize to unit length
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut embedding {
            *value /= magnitude;
        }
    }
    
    Array1::from(embedding)
}

/// Example of how to use the socket interface from other languages
/// This would be implemented in Python, Go, etc.
fn print_integration_examples() {
    println!("\n🔧 Integration Examples");
    println!("======================");
    
    println!("Python client example:");
    println!(r#"
import socket
import json

# Connect to Layer 2 DSR
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect('/tmp/mfn_layer2.sock')

# Add memory
request = {{
    "type": "AddMemory",
    "request_id": "py-add-1",
    "memory_id": 2001,
    "embedding": [0.1, 0.2, 0.3, ...],  # 384-dim embedding
    "content": "Python integration test",
    "tags": ["python", "integration"],
    "metadata": {{"source": "python_client"}}
}}

sock.send((json.dumps(request) + '\n').encode())
response = sock.recv(4096).decode().strip()
result = json.loads(response)
print(f"Added memory: {{result}}")

# Search similar
search_request = {{
    "type": "SimilaritySearch", 
    "request_id": "py-search-1",
    "query_embedding": [0.15, 0.25, 0.35, ...],
    "top_k": 5
}}

sock.send((json.dumps(search_request) + '\n').encode())
response = sock.recv(4096).decode().strip()
results = json.loads(response)
print(f"Search results: {{results}}")

sock.close()
"#);
    
    println!("\nGo client example:");
    println!(r#"
package main

import (
    "encoding/json"
    "net"
    "fmt"
)

func main() {{
    conn, err := net.Dial("unix", "/tmp/mfn_layer2.sock")
    if err != nil {{
        panic(err)
    }}
    defer conn.Close()
    
    // Ping request
    request := map[string]interface{}{{
        "type": "Ping",
        "request_id": "go-ping-1",
    }}
    
    reqBytes, _ := json.Marshal(request)
    conn.Write(append(reqBytes, '\n'))
    
    buffer := make([]byte, 4096)
    n, _ := conn.Read(buffer)
    
    var response map[string]interface{{}}
    json.Unmarshal(buffer[:n], &response)
    fmt.Printf("Response: %+v\n", response)
}}
"#);
}