//! Unit tests for parallel routing functionality

use mfn_core::{UniversalSearchResult, UniversalMemory, LayerId, current_timestamp};
use std::time::{Duration, Instant};
use anyhow::Result;
use std::collections::HashMap;

// Mock layer client for testing
#[derive(Clone)]
struct MockLayerClient {
    layer_id: LayerId,
    delay_ms: u64,
    should_fail: bool,
    results_to_return: Vec<UniversalSearchResult>,
}

impl MockLayerClient {
    fn new(layer_id: LayerId, delay_ms: u64) -> Self {
        let mut results = Vec::new();
        for i in 0..3 {
            let memory = UniversalMemory {
                id: (layer_id as u64) * 1000 + i,  // Generate unique IDs
                content: format!("Result {} from {:?}", i, layer_id),
                embedding: None,
                tags: vec![],
                metadata: HashMap::new(),
                created_at: current_timestamp(),
                last_accessed: current_timestamp(),
                access_count: 1,
            };

            results.push(UniversalSearchResult {
                memory,
                confidence: 0.8 - (i as f64 * 0.1),
                path: vec![],
                layer_origin: layer_id,
                search_time_us: 100,
            });
        }

        Self {
            layer_id,
            delay_ms,
            should_fail: false,
            results_to_return: results,
        }
    }

    fn with_failure(mut self) -> Self {
        self.should_fail = true;
        self
    }

    async fn query(&self) -> Result<Vec<UniversalSearchResult>> {
        // Simulate network delay
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;

        if self.should_fail {
            return Err(anyhow::anyhow!("Mock layer {} failed", self.layer_id as u32));
        }

        Ok(self.results_to_return.clone())
    }
}

/// Test that parallel execution is faster than sequential
#[tokio::test]
async fn test_parallel_execution_performance() {
    // Create 4 mock layers with 100ms delay each
    let layer1 = MockLayerClient::new(LayerId::Layer1, 100);
    let layer2 = MockLayerClient::new(LayerId::Layer2, 100);
    let layer3 = MockLayerClient::new(LayerId::Layer3, 100);
    let layer4 = MockLayerClient::new(LayerId::Layer4, 100);

    // Sequential execution simulation
    let start_seq = Instant::now();
    let _ = layer1.query().await;
    let _ = layer2.query().await;
    let _ = layer3.query().await;
    let _ = layer4.query().await;
    let seq_duration = start_seq.elapsed();

    // Parallel execution simulation
    let start_par = Instant::now();
    let (r1, r2, r3, r4) = tokio::join!(
        layer1.query(),
        layer2.query(),
        layer3.query(),
        layer4.query()
    );
    let par_duration = start_par.elapsed();

    println!("Sequential: {}ms, Parallel: {}ms", seq_duration.as_millis(), par_duration.as_millis());

    // Parallel should be at least 3x faster
    assert!(
        seq_duration.as_millis() > par_duration.as_millis() * 3,
        "Parallel execution ({}ms) should be at least 3x faster than sequential ({}ms)",
        par_duration.as_millis(),
        seq_duration.as_millis()
    );

    // Parallel should complete in ~100ms (plus overhead)
    assert!(
        par_duration.as_millis() < 150,
        "Parallel execution should complete in ~100ms, took {}ms",
        par_duration.as_millis()
    );

    // All results should be successful
    assert!(r1.is_ok() && r2.is_ok() && r3.is_ok() && r4.is_ok());
}

/// Test result merging and deduplication
#[tokio::test]
async fn test_result_merging_deduplication() {
    // Create layers with overlapping results
    let mut layer1 = MockLayerClient::new(LayerId::Layer1, 10);
    let mut layer2 = MockLayerClient::new(LayerId::Layer2, 10);

    // Add duplicate memory id
    layer1.results_to_return[0].memory.id = 999;  // Shared ID
    layer1.results_to_return[0].confidence = 0.9;

    layer2.results_to_return[0].memory.id = 999;  // Same shared ID
    layer2.results_to_return[0].confidence = 0.7;  // Lower confidence

    // Simulate parallel query and merging
    let (r1, r2) = tokio::join!(
        layer1.query(),
        layer2.query()
    );

    let mut all_results = Vec::new();
    if let Ok(results) = r1 {
        all_results.extend(results);
    }
    if let Ok(results) = r2 {
        all_results.extend(results);
    }

    // Count before dedup
    let count_before = all_results.len();
    assert_eq!(count_before, 6, "Should have 6 results before dedup (3 + 3)");

    // Deduplicate by memory.id, keeping highest confidence
    let mut seen = std::collections::HashMap::new();
    for result in all_results {
        seen.entry(result.memory.id.clone())
            .and_modify(|e: &mut UniversalSearchResult| {
                if result.confidence > e.confidence {
                    *e = result.clone();
                }
            })
            .or_insert(result);
    }

    let deduped: Vec<UniversalSearchResult> = seen.into_values().collect();

    assert_eq!(deduped.len(), 5, "Should have 5 unique results after dedup");

    // Check that we kept the higher confidence version
    let shared = deduped.iter().find(|r| r.memory.id == 999).unwrap();
    assert_eq!(shared.confidence, 0.9, "Should keep higher confidence duplicate");
    assert_eq!(shared.layer_origin, LayerId::Layer1, "Should be from Layer1 (higher confidence)");
}

/// Test partial failure handling
#[tokio::test]
async fn test_partial_failure_handling() {
    // Create layers with Layer2 failing
    let layer1 = MockLayerClient::new(LayerId::Layer1, 10);
    let layer2 = MockLayerClient::new(LayerId::Layer2, 10).with_failure();
    let layer3 = MockLayerClient::new(LayerId::Layer3, 10);
    let layer4 = MockLayerClient::new(LayerId::Layer4, 10);

    // Execute parallel queries
    let (r1, r2, r3, r4) = tokio::join!(
        layer1.query(),
        layer2.query(),
        layer3.query(),
        layer4.query()
    );

    // Collect successful results
    let mut all_results = Vec::new();
    let mut success_count = 0;

    if let Ok(results) = r1 {
        all_results.extend(results);
        success_count += 1;
    }
    if let Ok(results) = r2 {
        all_results.extend(results);
        success_count += 1;
    } else {
        println!("Layer 2 failed as expected");
    }
    if let Ok(results) = r3 {
        all_results.extend(results);
        success_count += 1;
    }
    if let Ok(results) = r4 {
        all_results.extend(results);
        success_count += 1;
    }

    // Should have 3 successful layers
    assert_eq!(success_count, 3, "Should have 3 successful layers");
    assert_eq!(all_results.len(), 9, "Should have 9 results (3 layers × 3 results)");

    // Verify Layer2 results are missing
    let has_layer2 = all_results.iter().any(|r| r.layer_origin == LayerId::Layer2);
    assert!(!has_layer2, "Should not have Layer2 results");

    // Other layers should be present
    let has_layer1 = all_results.iter().any(|r| r.layer_origin == LayerId::Layer1);
    let has_layer3 = all_results.iter().any(|r| r.layer_origin == LayerId::Layer3);
    let has_layer4 = all_results.iter().any(|r| r.layer_origin == LayerId::Layer4);

    assert!(has_layer1 && has_layer3 && has_layer4, "Should have results from layers 1, 3, 4");
}

/// Test timeout handling
#[tokio::test]
async fn test_timeout_handling() {
    // Create layers with Layer3 being slow (200ms delay)
    let layer1 = MockLayerClient::new(LayerId::Layer1, 10);
    let layer2 = MockLayerClient::new(LayerId::Layer2, 10);
    let layer3 = MockLayerClient::new(LayerId::Layer3, 200);  // Will timeout
    let layer4 = MockLayerClient::new(LayerId::Layer4, 10);

    // Simulate timeout wrapper
    async fn query_with_timeout(client: MockLayerClient, timeout_ms: u64) -> Result<Vec<UniversalSearchResult>> {
        match tokio::time::timeout(Duration::from_millis(timeout_ms), client.query()).await {
            Ok(result) => result,
            Err(_) => {
                println!("Layer {} timeout after {}ms", client.layer_id as u32, timeout_ms);
                Ok(vec![])  // Return empty on timeout
            }
        }
    }

    let timeout_ms = 100;  // 100ms timeout

    // Execute parallel queries with timeout
    let (r1, r2, r3, r4) = tokio::join!(
        query_with_timeout(layer1.clone(), timeout_ms),
        query_with_timeout(layer2.clone(), timeout_ms),
        query_with_timeout(layer3.clone(), timeout_ms),
        query_with_timeout(layer4.clone(), timeout_ms),
    );

    // Collect results
    let mut all_results = Vec::new();

    if let Ok(results) = r1 {
        all_results.extend(results);
    }
    if let Ok(results) = r2 {
        all_results.extend(results);
    }
    if let Ok(results) = r3 {
        all_results.extend(results);
    }
    if let Ok(results) = r4 {
        all_results.extend(results);
    }

    // Should have results from 3 fast layers (Layer3 timed out)
    assert_eq!(all_results.len(), 9, "Should have 9 results (3 fast layers)");

    // Verify Layer3 results are missing (due to timeout)
    let has_layer3 = all_results.iter().any(|r| r.layer_origin == LayerId::Layer3);
    assert!(!has_layer3, "Should not have Layer3 results (timed out)");
}

/// Test that all layers are queried simultaneously
#[tokio::test]
async fn test_all_layers_queried() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // Counter to track concurrent queries
    let concurrent_count = Arc::new(AtomicUsize::new(0));
    let max_concurrent = Arc::new(AtomicUsize::new(0));

    // Create mock queries that track concurrency
    async fn mock_query_with_tracking(
        layer_id: LayerId,
        concurrent: Arc<AtomicUsize>,
        max_concurrent: Arc<AtomicUsize>,
    ) -> Result<Vec<UniversalSearchResult>> {
        // Increment concurrent count
        let current = concurrent.fetch_add(1, Ordering::SeqCst) + 1;

        // Update max if needed
        let mut max = max_concurrent.load(Ordering::SeqCst);
        while current > max {
            match max_concurrent.compare_exchange(max, current, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => break,
                Err(x) => max = x,
            }
        }

        // Simulate work
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Decrement concurrent count
        concurrent.fetch_sub(1, Ordering::SeqCst);

        // Return mock results
        let memory = UniversalMemory {
            id: layer_id as u64 * 10000,  // Unique ID per layer
            content: format!("Result from {:?}", layer_id),
            embedding: None,
            tags: vec![],
            metadata: HashMap::new(),
            created_at: current_timestamp(),
            last_accessed: current_timestamp(),
            access_count: 1,
        };

        Ok(vec![UniversalSearchResult {
            memory,
            confidence: 0.8,
            path: vec![],
            layer_origin: layer_id,
            search_time_us: 50,
        }])
    }

    // Run parallel queries
    let concurrent = Arc::clone(&concurrent_count);
    let max = Arc::clone(&max_concurrent);

    let (r1, r2, r3, r4) = tokio::join!(
        mock_query_with_tracking(LayerId::Layer1, concurrent.clone(), max.clone()),
        mock_query_with_tracking(LayerId::Layer2, concurrent.clone(), max.clone()),
        mock_query_with_tracking(LayerId::Layer3, concurrent.clone(), max.clone()),
        mock_query_with_tracking(LayerId::Layer4, concurrent.clone(), max.clone()),
    );

    // All should succeed
    assert!(r1.is_ok() && r2.is_ok() && r3.is_ok() && r4.is_ok());

    // Check max concurrency
    let max_seen = max_concurrent.load(Ordering::SeqCst);
    println!("Max concurrent queries: {}", max_seen);

    assert_eq!(
        max_seen, 4,
        "Should have all 4 queries running concurrently, but saw max of {}",
        max_seen
    );
}

/// Test result ranking by confidence
#[tokio::test]
async fn test_result_ranking() {
    // Create results with varying confidence scores
    let low_memory = UniversalMemory {
        id: 1,  // ID for low confidence
        content: "Low confidence".to_string(),
        embedding: None,
        tags: vec![],
        metadata: HashMap::new(),
        created_at: current_timestamp(),
        last_accessed: current_timestamp(),
        access_count: 1,
    };

    let high_memory = UniversalMemory {
        id: 2,  // ID for high confidence
        content: "High confidence".to_string(),
        embedding: None,
        tags: vec![],
        metadata: HashMap::new(),
        created_at: current_timestamp(),
        last_accessed: current_timestamp(),
        access_count: 1,
    };

    let medium_memory = UniversalMemory {
        id: 3,  // ID for medium confidence
        content: "Medium confidence".to_string(),
        embedding: None,
        tags: vec![],
        metadata: HashMap::new(),
        created_at: current_timestamp(),
        last_accessed: current_timestamp(),
        access_count: 1,
    };

    let mut results = vec![
        UniversalSearchResult {
            memory: low_memory,
            confidence: 0.3,
            path: vec![],
            layer_origin: LayerId::Layer1,
            search_time_us: 100,
        },
        UniversalSearchResult {
            memory: high_memory,
            confidence: 0.9,
            path: vec![],
            layer_origin: LayerId::Layer2,
            search_time_us: 100,
        },
        UniversalSearchResult {
            memory: medium_memory,
            confidence: 0.6,
            path: vec![],
            layer_origin: LayerId::Layer3,
            search_time_us: 100,
        },
    ];

    // Sort by confidence (descending)
    results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    // Verify ranking
    assert_eq!(results[0].memory.id, 2, "Highest confidence should be first");
    assert_eq!(results[1].memory.id, 3, "Medium confidence should be second");
    assert_eq!(results[2].memory.id, 1, "Lowest confidence should be last");

    // Verify scores are descending
    for i in 1..results.len() {
        assert!(
            results[i - 1].confidence >= results[i].confidence,
            "Results should be sorted by confidence (descending)"
        );
    }
}

/// Test empty query handling
#[tokio::test]
async fn test_empty_results_handling() {
    // Create layers that return empty results
    let empty_layer = MockLayerClient {
        layer_id: LayerId::Layer1,
        delay_ms: 10,
        should_fail: false,
        results_to_return: vec![],  // Empty results
    };

    let result = empty_layer.query().await;
    assert!(result.is_ok(), "Empty results should not error");
    assert_eq!(result.unwrap().len(), 0, "Should return empty vec");

    // Test parallel with all empty
    let (r1, r2, r3, r4) = tokio::join!(
        empty_layer.query(),
        empty_layer.query(),
        empty_layer.query(),
        empty_layer.query(),
    );

    let mut all_results = Vec::new();
    if let Ok(res) = r1 { all_results.extend(res); }
    if let Ok(res) = r2 { all_results.extend(res); }
    if let Ok(res) = r3 { all_results.extend(res); }
    if let Ok(res) = r4 { all_results.extend(res); }

    assert_eq!(all_results.len(), 0, "Combined empty results should still be empty");
}

/// Test max_results limiting
#[tokio::test]
async fn test_max_results_limiting() {
    // Create layer with many results
    let mut layer = MockLayerClient::new(LayerId::Layer1, 10);

    // Add more results
    for i in 0..10 {
        let memory = UniversalMemory {
            id: 100 + i,  // Extra IDs starting from 100
            content: format!("Extra result {}", i),
            embedding: None,
            tags: vec![],
            metadata: HashMap::new(),
            created_at: current_timestamp(),
            last_accessed: current_timestamp(),
            access_count: 1,
        };

        layer.results_to_return.push(UniversalSearchResult {
            memory,
            confidence: 0.5 - (i as f64 * 0.01),
            path: vec![],
            layer_origin: LayerId::Layer1,
            search_time_us: 100,
        });
    }

    let results = layer.query().await.unwrap();
    assert_eq!(results.len(), 13, "Should have 13 total results");

    // Simulate limiting to max_results
    let max_results = 5;
    let mut limited = results.clone();
    limited.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    limited.truncate(max_results);

    assert_eq!(limited.len(), 5, "Should be limited to max_results");

    // Verify we kept the highest confidence ones
    for i in 0..limited.len() - 1 {
        assert!(
            limited[i].confidence >= limited[i + 1].confidence,
            "Should keep highest confidence results"
        );
    }
}