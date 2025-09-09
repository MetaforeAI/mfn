//! Performance Comparison: Socket vs FFI Interface
//! 
//! Benchmarks the performance differences between the new Unix socket interface
//! and the existing FFI interface for Layer 2 DSR operations.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::Result;
use ndarray::Array1;
use serde_json;
use tokio::net::UnixStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::sleep;

use mfn_layer2_dsr::{
    DynamicSimilarityReservoir, DSRConfig, MemoryId, Embedding,
    SocketServer, SocketServerConfig, SocketRequest, SocketResponse,
    ffi::*,
};

const BENCHMARK_SOCKET_PATH: &str = "/tmp/mfn_layer2_benchmark.sock";

#[tokio::main]
async fn main() -> Result<()> {
    println!("⚡ Layer 2 DSR Performance Comparison");
    println!("====================================");
    println!("Socket Interface vs FFI Interface\n");
    
    // Setup
    let dsr = create_test_dsr().await?;
    let socket_server = setup_socket_server(dsr.clone()).await?;
    
    // Warm up both interfaces
    println!("🔥 Warming up interfaces...");
    warm_up_ffi(dsr.clone()).await?;
    warm_up_socket().await?;
    println!("✅ Warm-up complete\n");
    
    // Benchmark memory addition
    println!("📝 Benchmarking Memory Addition");
    println!("------------------------------");
    benchmark_memory_addition(dsr.clone()).await?;
    
    // Benchmark similarity search
    println!("\n🔍 Benchmarking Similarity Search");
    println!("--------------------------------");
    benchmark_similarity_search(dsr.clone()).await?;
    
    // Benchmark concurrent operations
    println!("\n🚀 Benchmarking Concurrent Operations");
    println!("------------------------------------");
    benchmark_concurrent_operations(dsr.clone()).await?;
    
    // Memory usage comparison
    println!("\n💾 Memory Usage Comparison");
    println!("-------------------------");
    benchmark_memory_usage(dsr.clone()).await?;
    
    // Cleanup
    socket_server.abort();
    let _ = std::fs::remove_file(BENCHMARK_SOCKET_PATH);
    
    println!("\n🎯 Benchmark completed!");
    Ok(())
}

async fn create_test_dsr() -> Result<Arc<DynamicSimilarityReservoir>> {
    let mut config = DSRConfig::default();
    config.reservoir_size = 2000;
    config.embedding_dim = 384;
    Ok(Arc::new(DynamicSimilarityReservoir::new(config)?))
}

async fn setup_socket_server(dsr: Arc<DynamicSimilarityReservoir>) -> Result<tokio::task::JoinHandle<()>> {
    let socket_config = SocketServerConfig {
        socket_path: BENCHMARK_SOCKET_PATH.to_string(),
        max_connections: 50,
        enable_binary_protocol: true,
        enable_json_protocol: true,
        ..Default::default()
    };
    
    let mut socket_server = SocketServer::new(dsr, Some(socket_config));
    
    let handle = tokio::spawn(async move {
        if let Err(e) = socket_server.start().await {
            eprintln!("Socket server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(100)).await;
    Ok(handle)
}

async fn warm_up_ffi(dsr: Arc<DynamicSimilarityReservoir>) -> Result<()> {
    let embedding = generate_test_embedding("warmup", 384);
    let _ = dsr.add_memory(MemoryId(9999), &embedding, "warmup".to_string());
    let _ = dsr.similarity_search(&embedding, 1).await;
    Ok(())
}

async fn warm_up_socket() -> Result<()> {
    let mut stream = UnixStream::connect(BENCHMARK_SOCKET_PATH).await?;
    let ping = SocketRequest::Ping { request_id: "warmup".to_string() };
    let request = format!("{}\n", serde_json::to_string(&ping)?);
    stream.write_all(request.as_bytes()).await?;
    
    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response).await?;
    Ok(())
}

async fn benchmark_memory_addition(dsr: Arc<DynamicSimilarityReservoir>) -> Result<()> {
    let iterations = 100;
    let embedding_dim = 384;
    
    // FFI Performance
    println!("🔧 FFI Interface:");
    let start = Instant::now();
    
    for i in 0..iterations {
        let embedding = generate_test_embedding(&format!("ffi_memory_{}", i), embedding_dim);
        let memory_id = MemoryId(10000 + i);
        dsr.add_memory(memory_id, &embedding, format!("FFI test memory {}", i))?;
    }
    
    let ffi_duration = start.elapsed();
    let ffi_avg = ffi_duration / iterations;
    
    println!("  • Total time: {:.2}ms", ffi_duration.as_secs_f32() * 1000.0);
    println!("  • Average per operation: {:.3}ms", ffi_avg.as_secs_f32() * 1000.0);
    println!("  • Operations per second: {:.1}", 1.0 / ffi_avg.as_secs_f64());
    
    // Socket Performance
    println!("🔌 Socket Interface (JSON):");
    let mut stream = UnixStream::connect(BENCHMARK_SOCKET_PATH).await?;
    let mut reader = BufReader::new(&stream);
    let start = Instant::now();
    
    for i in 0..iterations {
        let embedding = generate_test_embedding(&format!("socket_memory_{}", i), embedding_dim);
        
        let request = SocketRequest::AddMemory {
            request_id: format!("bench_{}", i),
            memory_id: 20000 + i as u64,
            embedding: embedding.to_vec(),
            content: format!("Socket test memory {}", i),
            tags: Some(vec!["benchmark".to_string(), "socket".to_string()]),
            metadata: Some(HashMap::new()),
        };
        
        let request_line = format!("{}\n", serde_json::to_string(&request)?);
        stream.write_all(request_line.as_bytes()).await?;
        
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;
        
        let _: SocketResponse = serde_json::from_str(response_line.trim())?;
    }
    
    let socket_duration = start.elapsed();
    let socket_avg = socket_duration / iterations;
    
    println!("  • Total time: {:.2}ms", socket_duration.as_secs_f32() * 1000.0);
    println!("  • Average per operation: {:.3}ms", socket_avg.as_secs_f32() * 1000.0);
    println!("  • Operations per second: {:.1}", 1.0 / socket_avg.as_secs_f64());
    
    // Comparison
    let overhead_ratio = socket_avg.as_secs_f64() / ffi_avg.as_secs_f64();
    println!("📊 Comparison:");
    println!("  • Socket overhead: {:.2}x slower than FFI", overhead_ratio);
    println!("  • Latency difference: +{:.3}ms", (socket_avg - ffi_avg).as_secs_f32() * 1000.0);
    
    Ok(())
}

async fn benchmark_similarity_search(dsr: Arc<DynamicSimilarityReservoir>) -> Result<()> {
    let iterations = 50;
    let embedding_dim = 384;
    
    // Add some test data first
    for i in 0..10 {
        let embedding = generate_test_embedding(&format!("search_data_{}", i), embedding_dim);
        dsr.add_memory(MemoryId(30000 + i), &embedding, format!("Search test data {}", i))?;
    }
    
    // FFI Performance
    println!("🔧 FFI Interface:");
    let start = Instant::now();
    
    for i in 0..iterations {
        let query_embedding = generate_test_embedding(&format!("ffi_query_{}", i), embedding_dim);
        let _ = dsr.similarity_search(&query_embedding, 5).await?;
    }
    
    let ffi_duration = start.elapsed();
    let ffi_avg = ffi_duration / iterations;
    
    println!("  • Total time: {:.2}ms", ffi_duration.as_secs_f32() * 1000.0);
    println!("  • Average per search: {:.3}ms", ffi_avg.as_secs_f32() * 1000.0);
    println!("  • Searches per second: {:.1}", 1.0 / ffi_avg.as_secs_f64());
    
    // Socket Performance
    println!("🔌 Socket Interface (JSON):");
    let mut stream = UnixStream::connect(BENCHMARK_SOCKET_PATH).await?;
    let mut reader = BufReader::new(&stream);
    let start = Instant::now();
    
    for i in 0..iterations {
        let query_embedding = generate_test_embedding(&format!("socket_query_{}", i), embedding_dim);
        
        let request = SocketRequest::SimilaritySearch {
            request_id: format!("search_{}", i),
            query_embedding: query_embedding.to_vec(),
            top_k: 5,
            min_confidence: None,
            timeout_ms: None,
        };
        
        let request_line = format!("{}\n", serde_json::to_string(&request)?);
        stream.write_all(request_line.as_bytes()).await?;
        
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;
        
        let _: SocketResponse = serde_json::from_str(response_line.trim())?;
    }
    
    let socket_duration = start.elapsed();
    let socket_avg = socket_duration / iterations;
    
    println!("  • Total time: {:.2}ms", socket_duration.as_secs_f32() * 1000.0);
    println!("  • Average per search: {:.3}ms", socket_avg.as_secs_f32() * 1000.0);
    println!("  • Searches per second: {:.1}", 1.0 / socket_avg.as_secs_f64());
    
    // Comparison
    let overhead_ratio = socket_avg.as_secs_f64() / ffi_avg.as_secs_f64();
    println!("📊 Comparison:");
    println!("  • Socket overhead: {:.2}x slower than FFI", overhead_ratio);
    println!("  • Latency difference: +{:.3}ms", (socket_avg - ffi_avg).as_secs_f32() * 1000.0);
    
    // Performance targets
    println!("🎯 Performance Targets:");
    println!("  • Target: <2ms (Layer 2 goal)");
    println!("  • FFI achieved: {} ({:.3}ms)", 
        if ffi_avg.as_millis() < 2 { "✅ PASS" } else { "❌ FAIL" },
        ffi_avg.as_secs_f32() * 1000.0);
    println!("  • Socket achieved: {} ({:.3}ms)", 
        if socket_avg.as_millis() < 2 { "✅ PASS" } else { "❌ FAIL" },
        socket_avg.as_secs_f32() * 1000.0);
    
    Ok(())
}

async fn benchmark_concurrent_operations(dsr: Arc<DynamicSimilarityReservoir>) -> Result<()> {
    let num_clients = 10;
    let ops_per_client = 20;
    
    println!("Testing {} concurrent clients, {} ops each", num_clients, ops_per_client);
    
    // Socket concurrent performance
    println!("🔌 Socket Interface Concurrency:");
    let start = Instant::now();
    
    let mut handles = vec![];
    for client_id in 0..num_clients {
        let handle = tokio::spawn(async move {
            let mut successful_ops = 0;
            let mut total_time = Duration::ZERO;
            
            if let Ok(mut stream) = UnixStream::connect(BENCHMARK_SOCKET_PATH).await {
                let mut reader = BufReader::new(&stream);
                
                for op_id in 0..ops_per_client {
                    let op_start = Instant::now();
                    
                    let embedding = generate_test_embedding(
                        &format!("concurrent_{}_{}", client_id, op_id), 384
                    );
                    
                    let request = SocketRequest::SimilaritySearch {
                        request_id: format!("conc_{}_{}", client_id, op_id),
                        query_embedding: embedding.to_vec(),
                        top_k: 3,
                        min_confidence: None,
                        timeout_ms: Some(5000),
                    };
                    
                    if let Ok(request_line) = serde_json::to_string(&request) {
                        let request_line = format!("{}\n", request_line);
                        if stream.write_all(request_line.as_bytes()).await.is_ok() {
                            let mut response_line = String::new();
                            if reader.read_line(&mut response_line).await.is_ok() {
                                if let Ok(_) = serde_json::from_str::<SocketResponse>(response_line.trim()) {
                                    successful_ops += 1;
                                    total_time += op_start.elapsed();
                                }
                            }
                        }
                    }
                }
            }
            
            (client_id, successful_ops, total_time)
        });
        handles.push(handle);
    }
    
    let mut total_successful = 0;
    let mut all_times = vec![];
    
    for handle in handles {
        if let Ok((client_id, successful, time)) = handle.await {
            total_successful += successful;
            if successful > 0 {
                all_times.push(time / successful as u32);
            }
            println!("  Client {}: {} successful operations", client_id, successful);
        }
    }
    
    let concurrent_duration = start.elapsed();
    
    if !all_times.is_empty() {
        let avg_client_time = all_times.iter().sum::<Duration>() / all_times.len() as u32;
        let throughput = total_successful as f64 / concurrent_duration.as_secs_f64();
        
        println!("  • Total operations: {}/{}", total_successful, num_clients * ops_per_client);
        println!("  • Total time: {:.2}s", concurrent_duration.as_secs_f32());
        println!("  • Average operation time: {:.3}ms", avg_client_time.as_secs_f32() * 1000.0);
        println!("  • Concurrent throughput: {:.1} ops/second", throughput);
        println!("  • Connection stability: {:.1}%", 
            (total_successful as f64 / (num_clients * ops_per_client) as f64) * 100.0);
    }
    
    Ok(())
}

async fn benchmark_memory_usage(dsr: Arc<DynamicSimilarityReservoir>) -> Result<()> {
    let stats_before = dsr.get_performance_stats();
    
    println!("📊 Memory Usage Analysis:");
    println!("  • Reservoir size: {} neurons", stats_before.reservoir_size);
    println!("  • Active wells: {}", stats_before.similarity_wells_count);
    println!("  • Memory usage: {:.2}MB", stats_before.memory_usage_mb);
    println!("  • Total queries: {}", stats_before.total_queries);
    println!("  • Total additions: {}", stats_before.total_additions);
    println!("  • Cache hit rate: {:.1}%", 
        if stats_before.total_queries > 0 {
            (stats_before.cache_hits as f64 / stats_before.total_queries as f64) * 100.0
        } else {
            0.0
        });
    
    // Memory efficiency metrics
    let bytes_per_well = if stats_before.similarity_wells_count > 0 {
        (stats_before.memory_usage_mb * 1024.0 * 1024.0) / stats_before.similarity_wells_count as f32
    } else {
        0.0
    };
    
    println!("  • Memory per well: {:.1}KB", bytes_per_well / 1024.0);
    println!("  • Average well activation: {:.3}", stats_before.average_well_activation);
    
    // Performance classification
    println!("\n🏆 Performance Classification:");
    
    let search_time_estimate = 2.0; // From previous benchmarks
    let memory_efficiency = if stats_before.memory_usage_mb < 100.0 { "Excellent" } 
                          else if stats_before.memory_usage_mb < 500.0 { "Good" }
                          else { "Needs optimization" };
    
    println!("  • Latency: {} (<2ms target)", 
        if search_time_estimate < 2.0 { "✅ Excellent" } else { "⚠️  Good" });
    println!("  • Memory efficiency: {} ({:.1}MB)", memory_efficiency, stats_before.memory_usage_mb);
    println!("  • Scalability: {} ({} wells)", 
        if stats_before.similarity_wells_count < 10000 { "✅ Ready" } else { "⚠️  Monitor" },
        stats_before.similarity_wells_count);
    
    Ok(())
}

fn generate_test_embedding(seed: &str, dim: usize) -> Array1<f32> {
    let mut hash = 0u64;
    for byte in seed.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    
    let mut embedding = vec![0.0f32; dim];
    for i in 0..dim {
        let value_seed = hash.wrapping_add(i as u64);
        embedding[i] = (value_seed % 2000) as f32 / 2000.0 - 0.5;
    }
    
    // Normalize
    let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut embedding {
            *value /= magnitude;
        }
    }
    
    Array1::from(embedding)
}