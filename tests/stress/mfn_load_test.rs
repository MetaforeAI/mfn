//! MFN System Stress Test - No Stubs, Real Performance Testing
//!
//! This test spawns multiple concurrent clients hammering the actual MFN layers
//! and measures real performance under load.
//!
//! IMPORTANT: Requires all layer servers to be running!
//! Run: ./scripts/start_all_layers.sh before running these tests

use mfn_integration::socket_integration::SocketMfnIntegration;
use mfn_core::{
    UniversalMemory, MemoryId,
    UniversalSearchQuery, Weight, AssociationType,
};
use std::collections::HashMap;
use tokio::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use anyhow::Result;

/// Stress test configuration
struct StressConfig {
    /// Number of concurrent clients
    num_clients: usize,
    /// Requests per client
    requests_per_client: usize,
    /// Test duration (stops early if reached)
    duration: Duration,
    /// Memory size (for memory operations)
    memory_count: usize,
}

/// Initialize MFN system with all layers
async fn create_mfn_system() -> Result<Arc<SocketMfnIntegration>> {
    println!("🔧 Initializing MFN System with socket connections...");

    let system = SocketMfnIntegration::new().await
        .map_err(|e| {
            eprintln!("❌ Failed to create MFN system: {}", e);
            eprintln!("   Make sure all layer servers are running:");
            eprintln!("   Run: ./scripts/start_all_layers.sh");
            e
        })?;

    system.initialize_all_layers().await
        .map_err(|e| {
            eprintln!("❌ Failed to connect to layers: {}", e);
            eprintln!("   Some layers may not be running.");
            eprintln!("   Check that all socket servers are listening:");
            eprintln!("   - Layer 1 (Zig IFR): /tmp/mfn_layer1.sock");
            eprintln!("   - Layer 2 (Rust DSR): /tmp/mfn_layer2.sock");
            eprintln!("   - Layer 3 (Go ALM): /tmp/mfn_layer3.sock");
            eprintln!("   - Layer 4 (Rust CPE): /tmp/mfn_layer4.sock");
            e
        })?;

    println!("✅ MFN System initialized successfully");
    Ok(Arc::new(system))
}

/// Test results
struct StressResults {
    total_requests: u64,
    successful_requests: u64,
    failed_requests: u64,
    total_duration: Duration,
    min_latency: Duration,
    max_latency: Duration,
    avg_latency: Duration,
    p50_latency: Duration,
    p95_latency: Duration,
    p99_latency: Duration,
    requests_per_second: f64,
}

impl StressResults {
    fn print(&self) {
        println!("\n═══════════════════════════════════════════════════════");
        println!("                 STRESS TEST RESULTS");
        println!("═══════════════════════════════════════════════════════");
        println!("Total Requests:      {:>12}", self.total_requests);
        println!("Successful:          {:>12} ({:.1}%)",
            self.successful_requests,
            (self.successful_requests as f64 / self.total_requests as f64) * 100.0
        );
        println!("Failed:              {:>12} ({:.1}%)",
            self.failed_requests,
            (self.failed_requests as f64 / self.total_requests as f64) * 100.0
        );
        println!("───────────────────────────────────────────────────────");
        println!("Test Duration:       {:>12.2}s", self.total_duration.as_secs_f64());
        println!("Throughput:          {:>12.1} req/s", self.requests_per_second);
        println!("───────────────────────────────────────────────────────");
        println!("Min Latency:         {:>12.3}ms", self.min_latency.as_secs_f64() * 1000.0);
        println!("Avg Latency:         {:>12.3}ms", self.avg_latency.as_secs_f64() * 1000.0);
        println!("P50 Latency:         {:>12.3}ms", self.p50_latency.as_secs_f64() * 1000.0);
        println!("P95 Latency:         {:>12.3}ms", self.p95_latency.as_secs_f64() * 1000.0);
        println!("P99 Latency:         {:>12.3}ms", self.p99_latency.as_secs_f64() * 1000.0);
        println!("Max Latency:         {:>12.3}ms", self.max_latency.as_secs_f64() * 1000.0);
        println!("═══════════════════════════════════════════════════════\n");
    }
}

/// Run a memory addition stress test
async fn stress_test_memory_addition(config: StressConfig) -> Result<StressResults> {
    println!("\n🔥 MEMORY ADDITION STRESS TEST");
    println!("Clients: {}, Requests/Client: {}", config.num_clients, config.requests_per_client);

    let system = create_mfn_system().await?;

    let success_count = Arc::new(AtomicU64::new(0));
    let fail_count = Arc::new(AtomicU64::new(0));
    let mut latencies = Vec::new();

    let start = Instant::now();

    // Spawn concurrent clients
    let mut handles = vec![];
    for client_id in 0..config.num_clients {
        let sys = system.clone();
        let success = success_count.clone();
        let fail = fail_count.clone();
        let requests = config.requests_per_client;

        let handle = tokio::spawn(async move {
            let mut client_latencies = Vec::new();

            for i in 0..requests {
                let req_start = Instant::now();

                let memory_id = (client_id * requests + i) as u64;
                let memory = UniversalMemory::new(
                    memory_id,
                    format!("Client {} Memory {}", client_id, i),
                );

                match sys.add_memory(memory).await {
                    Ok(_) => {
                        success.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        fail.fetch_add(1, Ordering::Relaxed);
                    }
                }

                let latency = req_start.elapsed();
                client_latencies.push(latency);
            }

            client_latencies
        });

        handles.push(handle);
    }

    // Wait for all clients to finish
    for handle in handles {
        if let Ok(client_latencies) = handle.await {
            latencies.extend(client_latencies);
        }
    }

    let total_duration = start.elapsed();

    // Calculate statistics
    latencies.sort();
    let total_requests = config.num_clients * config.requests_per_client;
    let min_latency = *latencies.first().unwrap_or(&Duration::ZERO);
    let max_latency = *latencies.last().unwrap_or(&Duration::ZERO);
    let avg_latency = Duration::from_nanos(
        latencies.iter().map(|d| d.as_nanos()).sum::<u128>() as u64 / latencies.len() as u64
    );
    let p50_latency = latencies[latencies.len() / 2];
    let p95_latency = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99_latency = latencies[(latencies.len() as f64 * 0.99) as usize];

    Ok(StressResults {
        total_requests: total_requests as u64,
        successful_requests: success_count.load(Ordering::Relaxed),
        failed_requests: fail_count.load(Ordering::Relaxed),
        total_duration,
        min_latency,
        max_latency,
        avg_latency,
        p50_latency,
        p95_latency,
        p99_latency,
        requests_per_second: total_requests as f64 / total_duration.as_secs_f64(),
    })
}

/// Run a search stress test
async fn stress_test_search(config: StressConfig) -> Result<StressResults> {
    println!("\n🔍 SEARCH STRESS TEST");
    println!("Clients: {}, Requests/Client: {}", config.num_clients, config.requests_per_client);

    let system = create_mfn_system().await?;

    // Pre-populate with memories
    println!("Populating {} memories...", config.memory_count);
    for i in 0..config.memory_count {
        let memory = UniversalMemory::new(
            i as u64,
            format!("Test Memory {}", i),
        );
        system.add_memory(memory).await?;
    }
    println!("Population complete. Starting search stress test...");

    let success_count = Arc::new(AtomicU64::new(0));
    let fail_count = Arc::new(AtomicU64::new(0));
    let mut latencies = Vec::new();

    let start = Instant::now();

    // Spawn concurrent search clients
    let mut handles = vec![];
    for client_id in 0..config.num_clients {
        let sys = system.clone();
        let success = success_count.clone();
        let fail = fail_count.clone();
        let requests = config.requests_per_client;

        let handle = tokio::spawn(async move {
            let mut client_latencies = Vec::new();

            for i in 0..requests {
                let req_start = Instant::now();

                let query = UniversalSearchQuery {
                    content: Some(format!("Client {} Query {}", client_id, i)),
                    max_results: 10,
                    tags: vec![],
                    start_memory_ids: vec![],
                    embedding: None,
                    association_types: vec![],
                    max_depth: 3,
                    min_weight: 0.0,
                    timeout_us: 20_000,
                    layer_params: HashMap::new(),
                };

                match sys.search(query).await {
                    Ok(_) => {
                        success.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => {
                        fail.fetch_add(1, Ordering::Relaxed);
                    }
                }

                let latency = req_start.elapsed();
                client_latencies.push(latency);
            }

            client_latencies
        });

        handles.push(handle);
    }

    // Wait for all clients
    for handle in handles {
        if let Ok(client_latencies) = handle.await {
            latencies.extend(client_latencies);
        }
    }

    let total_duration = start.elapsed();

    // Calculate statistics
    latencies.sort();
    let total_requests = config.num_clients * config.requests_per_client;
    let min_latency = *latencies.first().unwrap_or(&Duration::ZERO);
    let max_latency = *latencies.last().unwrap_or(&Duration::ZERO);
    let avg_latency = Duration::from_nanos(
        latencies.iter().map(|d| d.as_nanos()).sum::<u128>() as u64 / latencies.len() as u64
    );
    let p50_latency = latencies[latencies.len() / 2];
    let p95_latency = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99_latency = latencies[(latencies.len() as f64 * 0.99) as usize];

    Ok(StressResults {
        total_requests: total_requests as u64,
        successful_requests: success_count.load(Ordering::Relaxed),
        failed_requests: fail_count.load(Ordering::Relaxed),
        total_duration,
        min_latency,
        max_latency,
        avg_latency,
        p50_latency,
        p95_latency,
        p99_latency,
        requests_per_second: total_requests as f64 / total_duration.as_secs_f64(),
    })
}

/// Run a mixed workload stress test
async fn stress_test_mixed_workload(config: StressConfig) -> Result<StressResults> {
    println!("\n⚡ MIXED WORKLOAD STRESS TEST");
    println!("Clients: {}, Requests/Client: {}", config.num_clients, config.requests_per_client);

    let system = create_mfn_system().await?;

    // Pre-populate
    println!("Populating {} memories...", config.memory_count);
    for i in 0..config.memory_count {
        let memory = UniversalMemory::new(
            i as u64,
            format!("Test Memory {}", i),
        );
        system.add_memory(memory).await?;
    }

    let success_count = Arc::new(AtomicU64::new(0));
    let fail_count = Arc::new(AtomicU64::new(0));
    let mut latencies = Vec::new();

    let start = Instant::now();

    // Spawn mixed workload clients (50% add, 50% search)
    let mut handles = vec![];
    for client_id in 0..config.num_clients {
        let sys = system.clone();
        let success = success_count.clone();
        let fail = fail_count.clone();
        let requests = config.requests_per_client;
        let memory_count = config.memory_count;

        let handle = tokio::spawn(async move {
            let mut client_latencies = Vec::new();

            for i in 0..requests {
                let req_start = Instant::now();

                // Alternate between add and search
                let result = if i % 2 == 0 {
                    // Add operation
                    let memory_id = memory_count as u64 + (client_id * requests + i) as u64;
                    let memory = UniversalMemory::new(
                        memory_id,
                        format!("Dynamic Memory {}", memory_id),
                    );
                    sys.add_memory(memory).await
                } else {
                    // Search operation
                    let query = UniversalSearchQuery {
                        content: Some(format!("Query {}", i)),
                        max_results: 5,
                        tags: vec![],
                        start_memory_ids: vec![],
                        embedding: None,
                        association_types: vec![],
                        max_depth: 3,
                        min_weight: 0.0,
                        timeout_us: 20_000,
                        layer_params: HashMap::new(),
                    };
                    sys.search(query).await.map(|_| ())
                };

                match result {
                    Ok(_) => success.fetch_add(1, Ordering::Relaxed),
                    Err(_) => fail.fetch_add(1, Ordering::Relaxed),
                };

                client_latencies.push(req_start.elapsed());
            }

            client_latencies
        });

        handles.push(handle);
    }

    // Wait for completion
    for handle in handles {
        if let Ok(client_latencies) = handle.await {
            latencies.extend(client_latencies);
        }
    }

    let total_duration = start.elapsed();

    // Statistics
    latencies.sort();
    let total_requests = config.num_clients * config.requests_per_client;

    Ok(StressResults {
        total_requests: total_requests as u64,
        successful_requests: success_count.load(Ordering::Relaxed),
        failed_requests: fail_count.load(Ordering::Relaxed),
        total_duration,
        min_latency: *latencies.first().unwrap_or(&Duration::ZERO),
        max_latency: *latencies.last().unwrap_or(&Duration::ZERO),
        avg_latency: Duration::from_nanos(
            latencies.iter().map(|d| d.as_nanos()).sum::<u128>() as u64 / latencies.len() as u64
        ),
        p50_latency: latencies[latencies.len() / 2],
        p95_latency: latencies[(latencies.len() as f64 * 0.95) as usize],
        p99_latency: latencies[(latencies.len() as f64 * 0.99) as usize],
        requests_per_second: total_requests as f64 / total_duration.as_secs_f64(),
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_test_light_load() {
    let config = StressConfig {
        num_clients: 10,
        requests_per_client: 100,
        duration: Duration::from_secs(30),
        memory_count: 100,
    };

    let results = match stress_test_memory_addition(config).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Test failed to initialize: {}", e);
            eprintln!("Make sure all layer servers are running:");
            eprintln!("Run: ./scripts/start_all_layers.sh");
            panic!("Failed to initialize MFN system");
        }
    };

    results.print();

    assert!(results.successful_requests > 0, "No successful requests!");
    assert!(results.requests_per_second > 100.0, "Throughput too low: {} req/s", results.requests_per_second);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_test_medium_load() {
    let config = StressConfig {
        num_clients: 50,
        requests_per_client: 100,
        duration: Duration::from_secs(60),
        memory_count: 500,
    };

    let results = match stress_test_search(config).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Test failed to initialize: {}", e);
            eprintln!("Make sure all layer servers are running:");
            eprintln!("Run: ./scripts/start_all_layers.sh");
            panic!("Failed to initialize MFN system");
        }
    };

    results.print();

    assert!(results.successful_requests > 0);
    assert!(results.avg_latency < Duration::from_millis(100), "Avg latency too high: {:?}", results.avg_latency);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_test_heavy_load() {
    let config = StressConfig {
        num_clients: 100,
        requests_per_client: 100,
        duration: Duration::from_secs(120),
        memory_count: 1000,
    };

    let results = match stress_test_mixed_workload(config).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Test failed to initialize: {}", e);
            eprintln!("Make sure all layer servers are running:");
            eprintln!("Run: ./scripts/start_all_layers.sh");
            panic!("Failed to initialize MFN system");
        }
    };

    results.print();

    assert!(results.successful_requests > 9000, "Too many failures: {}", results.failed_requests);
    assert!(results.p95_latency < Duration::from_millis(500), "P95 latency too high: {:?}", results.p95_latency);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
#[ignore] // Run with: cargo test --release stress_test_extreme -- --ignored --nocapture
async fn stress_test_extreme_load() {
    let config = StressConfig {
        num_clients: 500,
        requests_per_client: 200,
        duration: Duration::from_secs(300),
        memory_count: 5000,
    };

    println!("\n💥 EXTREME LOAD TEST - 100,000 REQUESTS");
    let results = match stress_test_mixed_workload(config).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Test failed to initialize: {}", e);
            eprintln!("Make sure all layer servers are running:");
            eprintln!("Run: ./scripts/start_all_layers.sh");
            panic!("Failed to initialize MFN system");
        }
    };

    results.print();

    // Just ensure system doesn't crash
    assert!(results.successful_requests > 0);
}
