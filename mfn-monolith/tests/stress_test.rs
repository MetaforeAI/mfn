use mfn_monolith::{layer1, layer2, layer3, layer4, orchestrator, types::*};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn create_test_memory(id: usize) -> Memory {
    let embedding: Vec<f32> = (0..384)
        .map(|i| ((i + id) as f32 * 0.1) % 1.0)
        .collect();

    Memory::new(
        format!("Memory content {}", id),
        embedding,
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn stress_test_light_load() {
    println!("\n🔥 LIGHT LOAD STRESS TEST");
    println!("Clients: 10, Requests/Client: 100");

    // Initialize MFN
    let l1 = Arc::new(layer1::ExactMatchCache::new(1000).unwrap());
    let mut l2 = layer2::SimilarityIndex::new(1000, true).unwrap();
    let l3 = Arc::new(layer3::GraphIndex::new(1000).unwrap());
    let mut l4 = layer4::ContextPredictor::new(10).unwrap();

    // Populate with 100 memories
    for i in 0..100 {
        let mem = create_test_memory(i);
        orchestrator::add_memory_to_all(&l1, &mut l2, &l3, &mut l4, mem)
            .await
            .unwrap();
    }

    let l2 = Arc::new(l2);
    let l4 = Arc::new(l4);

    // Run stress test
    let start = Instant::now();
    let mut handles = vec![];

    for client_id in 0..10 {
        let l1 = l1.clone();
        let l2 = l2.clone();
        let l3 = l3.clone();
        let l4 = l4.clone();

        let handle = tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(100);
            let mut successes = 0;
            let mut failures = 0;

            for req_id in 0..100 {
                let query = Query::new(format!("Query {} from client {}", req_id, client_id))
                    .with_embedding(vec![0.5; 384]);

                let req_start = Instant::now();
                match orchestrator::query_parallel(&l1, &l2, &l3, &l4, query, 10).await {
                    Ok(_) => {
                        successes += 1;
                        latencies.push(req_start.elapsed().as_micros() as f64 / 1000.0);
                    }
                    Err(_) => failures += 1,
                }
            }

            (successes, failures, latencies)
        });

        handles.push(handle);
    }

    // Collect results
    let mut all_latencies = Vec::new();
    let mut total_successes = 0;
    let mut total_failures = 0;

    for handle in handles {
        let (successes, failures, mut latencies) = handle.await.unwrap();
        total_successes += successes;
        total_failures += failures;
        all_latencies.append(&mut latencies);
    }

    let duration = start.elapsed();
    let total_ops = total_successes + total_failures;

    // Calculate statistics
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = all_latencies[0];
    let p50 = all_latencies[all_latencies.len() / 2];
    let p95 = all_latencies[(all_latencies.len() * 95) / 100];
    let p99 = all_latencies[(all_latencies.len() * 99) / 100];
    let max = all_latencies[all_latencies.len() - 1];
    let avg = all_latencies.iter().sum::<f64>() / all_latencies.len() as f64;

    // Print results
    println!("\n═══════════════════════════════════════════════════════");
    println!("                 STRESS TEST RESULTS");
    println!("═══════════════════════════════════════════════════════");
    println!("Total Requests:              {}", total_ops);
    println!("Successful:                  {} ({:.1}%)", total_successes, (total_successes as f64 / total_ops as f64) * 100.0);
    println!("Failed:                         {} ({:.1}%)", total_failures, (total_failures as f64 / total_ops as f64) * 100.0);
    println!("───────────────────────────────────────────────────────");
    println!("Test Duration:               {:.2}s", duration.as_secs_f64());
    println!("Throughput:               {:.1} req/s", total_successes as f64 / duration.as_secs_f64());
    println!("───────────────────────────────────────────────────────");
    println!("Min Latency:                {:.3}ms", min);
    println!("Avg Latency:                {:.3}ms", avg);
    println!("P50 Latency:                {:.3}ms", p50);
    println!("P95 Latency:                {:.3}ms", p95);
    println!("P99 Latency:                {:.3}ms", p99);
    println!("Max Latency:                {:.3}ms", max);
    println!("═══════════════════════════════════════════════════════\n");

    assert_eq!(total_failures, 0, "No requests should fail");
}

#[tokio::test(flavor = "multi_thread")]
async fn stress_test_medium_load() {
    println!("\n🔍 MEDIUM LOAD STRESS TEST");
    println!("Clients: 50, Requests/Client: 100");

    // Initialize MFN
    let l1 = Arc::new(layer1::ExactMatchCache::new(5000).unwrap());
    let mut l2 = layer2::SimilarityIndex::new(5000, true).unwrap();
    let l3 = Arc::new(layer3::GraphIndex::new(5000).unwrap());
    let mut l4 = layer4::ContextPredictor::new(10).unwrap();

    // Populate with 500 memories
    for i in 0..500 {
        let mem = create_test_memory(i);
        orchestrator::add_memory_to_all(&l1, &mut l2, &l3, &mut l4, mem)
            .await
            .unwrap();
    }

    let l2 = Arc::new(l2);
    let l4 = Arc::new(l4);

    // Run stress test
    let start = Instant::now();
    let mut handles = vec![];

    for client_id in 0..50 {
        let l1 = l1.clone();
        let l2 = l2.clone();
        let l3 = l3.clone();
        let l4 = l4.clone();

        let handle = tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(100);
            let mut successes = 0;
            let mut failures = 0;

            for req_id in 0..100 {
                let query = Query::new(format!("Query {} from client {}", req_id, client_id))
                    .with_embedding(vec![0.5; 384]);

                let req_start = Instant::now();
                match orchestrator::query_parallel(&l1, &l2, &l3, &l4, query, 10).await {
                    Ok(_) => {
                        successes += 1;
                        latencies.push(req_start.elapsed().as_micros() as f64 / 1000.0);
                    }
                    Err(_) => failures += 1,
                }
            }

            (successes, failures, latencies)
        });

        handles.push(handle);
    }

    // Collect results
    let mut all_latencies = Vec::new();
    let mut total_successes = 0;
    let mut total_failures = 0;

    for handle in handles {
        let (successes, failures, mut latencies) = handle.await.unwrap();
        total_successes += successes;
        total_failures += failures;
        all_latencies.append(&mut latencies);
    }

    let duration = start.elapsed();
    let total_ops = total_successes + total_failures;

    // Calculate statistics
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = all_latencies[0];
    let p50 = all_latencies[all_latencies.len() / 2];
    let p95 = all_latencies[(all_latencies.len() * 95) / 100];
    let p99 = all_latencies[(all_latencies.len() * 99) / 100];
    let max = all_latencies[all_latencies.len() - 1];
    let avg = all_latencies.iter().sum::<f64>() / all_latencies.len() as f64;

    // Print results
    println!("\n═══════════════════════════════════════════════════════");
    println!("                 STRESS TEST RESULTS");
    println!("═══════════════════════════════════════════════════════");
    println!("Total Requests:              {}", total_ops);
    println!("Successful:                  {} ({:.1}%)", total_successes, (total_successes as f64 / total_ops as f64) * 100.0);
    println!("Failed:                         {} ({:.1}%)", total_failures, (total_failures as f64 / total_ops as f64) * 100.0);
    println!("───────────────────────────────────────────────────────");
    println!("Test Duration:               {:.2}s", duration.as_secs_f64());
    println!("Throughput:               {:.1} req/s", total_successes as f64 / duration.as_secs_f64());
    println!("───────────────────────────────────────────────────────");
    println!("Min Latency:                {:.3}ms", min);
    println!("Avg Latency:                {:.3}ms", avg);
    println!("P50 Latency:                {:.3}ms", p50);
    println!("P95 Latency:                {:.3}ms", p95);
    println!("P99 Latency:                {:.3}ms", p99);
    println!("Max Latency:                {:.3}ms", max);
    println!("═══════════════════════════════════════════════════════\n");

    assert_eq!(total_failures, 0, "No requests should fail");
}

#[tokio::test(flavor = "multi_thread")]
async fn stress_test_heavy_load() {
    println!("\n⚡ HEAVY LOAD STRESS TEST");
    println!("Clients: 100, Requests/Client: 100");

    // Initialize MFN
    let l1 = Arc::new(layer1::ExactMatchCache::new(10000).unwrap());
    let mut l2 = layer2::SimilarityIndex::new(10000, true).unwrap();
    let l3 = Arc::new(layer3::GraphIndex::new(10000).unwrap());
    let mut l4 = layer4::ContextPredictor::new(10).unwrap();

    // Populate with 1000 memories
    for i in 0..1000 {
        let mem = create_test_memory(i);
        orchestrator::add_memory_to_all(&l1, &mut l2, &l3, &mut l4, mem)
            .await
            .unwrap();
    }

    let l2 = Arc::new(l2);
    let l4 = Arc::new(l4);

    // Run stress test
    let start = Instant::now();
    let mut handles = vec![];

    for client_id in 0..100 {
        let l1 = l1.clone();
        let l2 = l2.clone();
        let l3 = l3.clone();
        let l4 = l4.clone();

        let handle = tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(100);
            let mut successes = 0;
            let mut failures = 0;

            for req_id in 0..100 {
                let query = Query::new(format!("Query {} from client {}", req_id, client_id))
                    .with_embedding(vec![0.5; 384]);

                let req_start = Instant::now();
                match orchestrator::query_parallel(&l1, &l2, &l3, &l4, query, 10).await {
                    Ok(_) => {
                        successes += 1;
                        latencies.push(req_start.elapsed().as_micros() as f64 / 1000.0);
                    }
                    Err(_) => failures += 1,
                }
            }

            (successes, failures, latencies)
        });

        handles.push(handle);
    }

    // Collect results
    let mut all_latencies = Vec::new();
    let mut total_successes = 0;
    let mut total_failures = 0;

    for handle in handles {
        let (successes, failures, mut latencies) = handle.await.unwrap();
        total_successes += successes;
        total_failures += failures;
        all_latencies.append(&mut latencies);
    }

    let duration = start.elapsed();
    let total_ops = total_successes + total_failures;

    // Calculate statistics
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = all_latencies[0];
    let p50 = all_latencies[all_latencies.len() / 2];
    let p95 = all_latencies[(all_latencies.len() * 95) / 100];
    let p99 = all_latencies[(all_latencies.len() * 99) / 100];
    let max = all_latencies[all_latencies.len() - 1];
    let avg = all_latencies.iter().sum::<f64>() / all_latencies.len() as f64;

    // Print results
    println!("\n═══════════════════════════════════════════════════════");
    println!("                 STRESS TEST RESULTS");
    println!("═══════════════════════════════════════════════════════");
    println!("Total Requests:             {}", total_ops);
    println!("Successful:                 {} ({:.1}%)", total_successes, (total_successes as f64 / total_ops as f64) * 100.0);
    println!("Failed:                        {} ({:.1}%)", total_failures, (total_failures as f64 / total_ops as f64) * 100.0);
    println!("───────────────────────────────────────────────────────");
    println!("Test Duration:              {:.2}s", duration.as_secs_f64());
    println!("Throughput:              {:.1} req/s", total_successes as f64 / duration.as_secs_f64());
    println!("───────────────────────────────────────────────────────");
    println!("Min Latency:               {:.3}ms", min);
    println!("Avg Latency:               {:.3}ms", avg);
    println!("P50 Latency:               {:.3}ms", p50);
    println!("P95 Latency:               {:.3}ms", p95);
    println!("P99 Latency:               {:.3}ms", p99);
    println!("Max Latency:               {:.3}ms", max);
    println!("═══════════════════════════════════════════════════════\n");

    assert_eq!(total_failures, 0, "No requests should fail");
}
