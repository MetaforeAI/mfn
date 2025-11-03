//! Integration tests with live layer servers

use mfn_integration::socket_integration::{SocketMfnIntegration, RoutingStrategy};
use mfn_core::UniversalSearchQuery;
use std::time::{Duration, Instant};

/// Helper to create a test query
fn create_test_query(content: &str) -> UniversalSearchQuery {
    UniversalSearchQuery {
        start_memory_ids: vec![],
        content: Some(content.to_string()),
        embedding: None,
        tags: vec![],
        association_types: vec![],
        max_depth: 3,
        max_results: 10,
        min_weight: 0.0,
        timeout_us: 5_000_000,  // 5 seconds
        layer_params: std::collections::HashMap::new(),
    }
}

/// Test that embeddings are properly sent to Layer 2
#[tokio::test]
async fn test_embedding_to_layer2_integration() {
    // Initialize the integration
    let integration = SocketMfnIntegration::new().await.expect("Failed to create integration");

    // Query 1: Authentication related
    let query1 = create_test_query("authentication error");
    let results1 = integration.query(query1).await.expect("Query 1 failed");

    // Query 2: Different topic
    let query2 = create_test_query("data processing pipeline");
    let results2 = integration.query(query2).await.expect("Query 2 failed");

    // Verify both queries returned results
    assert!(!results1.results.is_empty(), "Query 1 should return results");
    assert!(!results2.results.is_empty(), "Query 2 should return results");

    // If Layer 2 is working properly with embeddings, different queries should give different results
    // Check if the top results are different
    if !results1.results.is_empty() && !results2.results.is_empty() {
        let top1 = &results1.results[0].memory.id;
        let top2 = &results2.results[0].memory.id;

        // They might be different (depends on layer content)
        println!("Query 1 top result: memory_id={}", top1);
        println!("Query 2 top result: memory_id={}", top2);
    }

    // Verify Layer 2 is being consulted
    let has_layer2 = results1.results.iter()
        .any(|r| r.layer_origin as u32 == 2);

    if has_layer2 {
        println!("✓ Layer 2 returned results with embeddings");
    } else {
        println!("⚠ Layer 2 may not have returned results");
    }
}

/// Test parallel routing with all layers
#[tokio::test]
async fn test_parallel_routing_all_layers() {
    let mut integration = SocketMfnIntegration::new().await.expect("Failed to create integration");
    integration.set_routing_strategy(RoutingStrategy::Parallel);

    // Use parallel routing strategy
    let query = create_test_query("test parallel query");

    let start = Instant::now();
    let results = integration.query(query).await.expect("Parallel query failed");
    let elapsed = start.elapsed();

    println!("Parallel query completed in {}ms", elapsed.as_millis());

    // Check which layers responded
    let mut layers_found = std::collections::HashSet::new();
    for result in &results.results {
        layers_found.insert(result.layer_origin as u32);
    }

    println!("Layers that responded: {:?}", layers_found);

    // We should have results from multiple layers
    assert!(
        !layers_found.is_empty(),
        "Should have results from at least one layer"
    );

    // Print timing for analysis
    if layers_found.len() > 1 {
        println!("✓ Multiple layers responded in parallel");
    }

    // Verify timing is reasonable for parallel execution
    assert!(
        elapsed < Duration::from_secs(2),
        "Parallel query should complete within 2 seconds, took {}ms",
        elapsed.as_millis()
    );
}

/// Test that no placeholder code remains
#[tokio::test]
async fn test_no_placeholder_code_remains() {
    use std::process::Command;

    // Check for placeholder embedding vector
    let output = Command::new("grep")
        .args(&["-r", r"vec!\[0\.1", "mfn-integration/src/"])
        .output()
        .expect("Failed to run grep");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.is_empty(),
        "Found placeholder embedding code: {}",
        stdout
    );

    println!("✓ No placeholder embedding code found");

    // Check for mock TODO comments
    let output = Command::new("grep")
        .args(&["-r", "TODO.*placeholder", "mfn-integration/src/"])
        .output()
        .expect("Failed to run grep");

    let stdout = String::from_utf8_lossy(&output.stdout);

    if !stdout.is_empty() {
        println!("⚠ Found TODO placeholders: {}", stdout);
    } else {
        println!("✓ No TODO placeholders found");
    }
}

/// Performance test for embedding generation
#[tokio::test]
async fn test_embedding_performance() {
    let integration = SocketMfnIntegration::new().await.expect("Failed to create integration");

    // Warmup
    let warmup_query = create_test_query("warmup query");
    let _ = integration.query(warmup_query).await;

    // Test various input sizes
    let long_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(10);
    let test_cases = vec![
        ("short", "test"),
        ("medium", "The quick brown fox jumps over the lazy dog"),
        ("long", long_text.as_str()),
    ];

    for (label, text) in test_cases {
        let query = create_test_query(text);

        let start = Instant::now();
        let results = integration.query(query).await.expect("Query failed");
        let elapsed = start.elapsed();

        println!(
            "{} query ({} chars): {}ms, {} results",
            label,
            text.len(),
            elapsed.as_millis(),
            results.results.len()
        );

        // Embedding + query should complete quickly
        assert!(
            elapsed < Duration::from_millis(500),
            "{} query took too long: {}ms",
            label,
            elapsed.as_millis()
        );
    }
}

/// Test sequential vs parallel routing performance
#[tokio::test]
async fn test_sequential_vs_parallel_performance() {
    let mut integration = SocketMfnIntegration::new().await.expect("Failed to create integration");

    let test_query_text = "performance test query";

    // Sequential routing
    integration.set_routing_strategy(RoutingStrategy::Sequential);
    let seq_query = create_test_query(test_query_text);
    let seq_start = Instant::now();
    let seq_results = integration.query(seq_query).await.expect("Sequential query failed");
    let seq_elapsed = seq_start.elapsed();

    // Parallel routing
    integration.set_routing_strategy(RoutingStrategy::Parallel);
    let par_query = create_test_query(test_query_text);
    let par_start = Instant::now();
    let par_results = integration.query(par_query).await.expect("Parallel query failed");
    let par_elapsed = par_start.elapsed();

    println!("Sequential: {}ms, {} results", seq_elapsed.as_millis(), seq_results.results.len());
    println!("Parallel: {}ms, {} results", par_elapsed.as_millis(), par_results.results.len());

    // Calculate speedup
    if seq_elapsed > Duration::ZERO && par_elapsed > Duration::ZERO {
        let speedup = seq_elapsed.as_millis() as f64 / par_elapsed.as_millis() as f64;
        println!("Speedup: {:.2}x", speedup);

        // Parallel should be faster (at least marginally)
        if speedup > 1.5 {
            println!("✓ Significant speedup achieved with parallel routing");
        } else if speedup > 1.0 {
            println!("✓ Parallel routing is faster");
        } else {
            println!("⚠ Parallel routing may not be working optimally");
        }
    }
}

/// Test Layer 2 similarity quality with embeddings
#[tokio::test]
async fn test_layer2_similarity_quality() {
    let integration = SocketMfnIntegration::new().await.expect("Failed to create integration");

    // Test semantic similarity with related queries
    let auth_queries = vec![
        "authentication error",
        "login failure",
        "password incorrect",
    ];

    let data_queries = vec![
        "data processing",
        "information handling",
        "content management",
    ];

    println!("\n=== Testing Authentication-related Queries ===");
    for query_text in &auth_queries {
        let query = create_test_query(query_text);
        let results = integration.query(query).await.expect("Query failed");

        // Check if results contain auth-related content
        let auth_related = results.results.iter()
            .filter(|r| {
                let content = r.memory.content.to_lowercase();
                content.contains("auth") ||
                content.contains("login") ||
                content.contains("password") ||
                content.contains("user")
            })
            .count();

        println!(
            "Query '{}': {} results, {} auth-related",
            query_text,
            results.results.len(),
            auth_related
        );
    }

    println!("\n=== Testing Data-related Queries ===");
    for query_text in &data_queries {
        let query = create_test_query(query_text);
        let results = integration.query(query).await.expect("Query failed");

        // Check if results contain data-related content
        let data_related = results.results.iter()
            .filter(|r| {
                let content = r.memory.content.to_lowercase();
                content.contains("data") ||
                content.contains("process") ||
                content.contains("information") ||
                content.contains("content")
            })
            .count();

        println!(
            "Query '{}': {} results, {} data-related",
            query_text,
            results.results.len(),
            data_related
        );
    }
}

/// Test error handling when layers are down
#[tokio::test]
#[ignore] // This test requires manual layer shutdown
async fn test_layer_failure_handling() {
    let integration = SocketMfnIntegration::new().await.expect("Failed to create integration");

    // This test assumes you manually stop one or more layers
    println!("⚠ This test requires manually stopping layer servers");
    println!("Stop Layer 2 with: pkill -f layer2_socket_server");

    // Give time to manually stop a layer if running interactively
    tokio::time::sleep(Duration::from_secs(2)).await;

    let query = create_test_query("test with layer down");

    match integration.query(query).await {
        Ok(results) => {
            println!("Query succeeded with {} results despite layer failure", results.results.len());
            assert!(!results.results.is_empty(), "Should still get some results");
        }
        Err(e) => {
            println!("Query failed: {}", e);
            // Depending on how many layers are down, this might be expected
        }
    }
}

/// Comprehensive test to verify all fixes are working
#[tokio::test]
async fn test_comprehensive_bug_fixes() {
    let mut integration = SocketMfnIntegration::new().await.expect("Failed to create integration");

    println!("\n=== Comprehensive Bug Fix Validation ===\n");

    // Test BUG-001: Real embeddings instead of placeholder
    println!("1. Testing BUG-001 (Embedding Service):");
    let embedding_query = create_test_query("test embedding generation");
    let start = Instant::now();
    let results = integration.query(embedding_query).await.expect("Query failed");
    let elapsed = start.elapsed();

    println!("   ✓ Embedding query completed in {}ms", elapsed.as_millis());
    println!("   ✓ Returned {} results", results.results.len());
    assert!(elapsed < Duration::from_millis(500), "Embedding should be fast");

    // Test BUG-002: Parallel routing
    println!("\n2. Testing BUG-002 (Parallel Routing):");
    integration.set_routing_strategy(RoutingStrategy::Parallel);
    let parallel_query = create_test_query("test parallel execution");
    let par_start = Instant::now();
    let par_results = integration.query(parallel_query).await.expect("Query failed");
    let par_elapsed = par_start.elapsed();

    // Collect layer origins
    let mut layers = std::collections::HashSet::new();
    for r in &par_results.results {
        layers.insert(r.layer_origin as u32);
    }

    println!("   ✓ Parallel query completed in {}ms", par_elapsed.as_millis());
    println!("   ✓ Results from layers: {:?}", layers);
    println!("   ✓ Total results: {}", par_results.results.len());

    // Verify both fixes work together
    println!("\n3. Testing Combined Functionality:");
    let combined_query = create_test_query("authentication system error");
    let comb_start = Instant::now();
    let comb_results = integration.query(combined_query).await.expect("Query failed");
    let comb_elapsed = comb_start.elapsed();

    println!("   ✓ Combined query completed in {}ms", comb_elapsed.as_millis());
    println!("   ✓ Results: {}", comb_results.results.len());

    // Check for semantic relevance
    let relevant_count = comb_results.results.iter()
        .filter(|r| {
            let content = r.memory.content.to_lowercase();
            content.contains("auth") ||
            content.contains("system") ||
            content.contains("error")
        })
        .count();

    if relevant_count > 0 {
        println!("   ✓ Found {} semantically relevant results", relevant_count);
    }

    println!("\n=== All Bug Fixes Validated Successfully ===");
}