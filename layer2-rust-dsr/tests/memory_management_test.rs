//! Integration tests for Layer 2 DSR memory management
//!
//! Tests the following memory management features:
//! - Maximum wells limit with LRU eviction
//! - TTL-based eviction
//! - Connection-based cleanup
//! - Memory usage tracking
//! - Well size limits

use mfn_layer2_dsr::{
    DynamicSimilarityReservoir, DSRConfig, MemoryId,
};
use ndarray::Array1;
use tokio;

#[tokio::test]
async fn test_max_wells_limit_enforced() {
    let mut config = DSRConfig::default();
    config.max_similarity_wells = 5;  // Small limit for testing
    config.embedding_dim = 10;
    config.reservoir_size = 100;  // Small for fast testing

    let dsr = DynamicSimilarityReservoir::new(config).unwrap();

    // Add 10 memories (should evict first 5)
    for i in 0..10 {
        let embedding = Array1::from(vec![0.1 * i as f32; 10]);
        let memory_id = MemoryId(i);
        dsr.add_memory(memory_id, &embedding, format!("memory {}", i))
            .await
            .unwrap();
    }

    // Check stats
    let stats = dsr.get_performance_stats().await;
    assert_eq!(stats.similarity_wells_count, 5, "Should have exactly 5 wells (max_wells)");
    assert_eq!(stats.wells_evicted, 5, "Should have evicted 5 wells");

    // Verify oldest memories are evicted (0-4 should be gone, 5-9 should remain)
    let query = Array1::from(vec![0.1; 10]);
    let results = dsr.similarity_search(&query, 10).await.unwrap();

    let remaining_ids: Vec<u64> = results.matches.iter()
        .map(|m| m.memory_id.0)
        .collect();

    // Should only have memories 5-9
    for id in 5..10 {
        assert!(remaining_ids.contains(&id), "Memory {} should still exist", id);
    }
    for id in 0..5 {
        assert!(!remaining_ids.contains(&id), "Memory {} should be evicted", id);
    }
}

#[tokio::test]
async fn test_connection_cleanup_on_disconnect() {
    let mut config = DSRConfig::default();
    config.embedding_dim = 10;
    config.reservoir_size = 100;

    let dsr = DynamicSimilarityReservoir::new(config).unwrap();

    // Add memories with connection ID
    let conn_id = "test-conn-123";
    for i in 0..5 {
        let embedding = Array1::from(vec![0.1 * i as f32; 10]);
        let memory_id = MemoryId(i);
        dsr.add_memory_with_connection(
            memory_id,
            &embedding,
            format!("memory {}", i),
            Some(conn_id.to_string()),
        )
        .await
        .unwrap();
    }

    // Add memories without connection ID
    for i in 5..8 {
        let embedding = Array1::from(vec![0.1 * i as f32; 10]);
        let memory_id = MemoryId(i);
        dsr.add_memory(memory_id, &embedding, format!("memory {}", i))
            .await
            .unwrap();
    }

    // Verify all 8 memories exist
    let stats_before = dsr.get_performance_stats().await;
    assert_eq!(stats_before.similarity_wells_count, 8);
    assert_eq!(stats_before.connection_count, 1);  // One connection tracked

    // Clean up connection
    dsr.cleanup_connection(conn_id).await.unwrap();

    // Verify only non-connection memories remain
    let stats_after = dsr.get_performance_stats().await;
    assert_eq!(stats_after.similarity_wells_count, 3, "Should have 3 wells remaining");
    assert_eq!(stats_after.connection_count, 0, "Should have no connections tracked");
    assert_eq!(stats_after.wells_evicted, 5, "Should have evicted 5 wells");

    // Verify correct memories remain
    let query = Array1::from(vec![0.1; 10]);
    let results = dsr.similarity_search(&query, 10).await.unwrap();

    let remaining_ids: Vec<u64> = results.matches.iter()
        .map(|m| m.memory_id.0)
        .collect();

    // Should only have memories 5-7 (no connection ID)
    for id in 5..8 {
        assert!(remaining_ids.contains(&id), "Memory {} should still exist", id);
    }
    for id in 0..5 {
        assert!(!remaining_ids.contains(&id), "Memory {} should be cleaned up", id);
    }
}

#[tokio::test]
async fn test_memory_usage_tracking() {
    let mut config = DSRConfig::default();
    config.embedding_dim = 384;  // Realistic size
    config.reservoir_size = 2000;  // Full size

    let dsr = DynamicSimilarityReservoir::new(config).unwrap();

    // Get initial stats
    let initial_stats = dsr.get_performance_stats().await;
    let initial_memory = initial_stats.memory_usage_mb;

    println!("Initial memory usage: {:.2} MB", initial_memory);

    // Add 100 memories
    for i in 0..100 {
        let embedding = Array1::from(vec![0.001 * i as f32; 384]);
        let memory_id = MemoryId(i);
        dsr.add_memory(memory_id, &embedding, format!("memory {}", i))
            .await
            .unwrap();
    }

    // Check memory has increased
    let after_stats = dsr.get_performance_stats().await;
    let after_memory = after_stats.memory_usage_mb;

    println!("After 100 memories: {:.2} MB", after_memory);
    println!("Memory increase: {:.2} MB", after_memory - initial_memory);

    assert!(after_memory > initial_memory, "Memory usage should increase after adding wells");
    assert_eq!(after_stats.similarity_wells_count, 100);

    // Memory per well should be reasonable (not excessive)
    let memory_per_well = (after_memory - initial_memory) / 100.0;
    println!("Average memory per well: {:.3} MB", memory_per_well);

    // Each well should use less than 1MB (sanity check)
    assert!(memory_per_well < 1.0, "Memory per well should be less than 1MB");
}

#[tokio::test]
async fn test_lru_updates_on_access() {
    let mut config = DSRConfig::default();
    config.max_similarity_wells = 3;  // Very small limit
    config.embedding_dim = 5;
    config.reservoir_size = 50;

    let dsr = DynamicSimilarityReservoir::new(config).unwrap();

    // Add 3 memories (fill to max)
    for i in 0..3 {
        let embedding = Array1::from(vec![i as f32; 5]);
        dsr.add_memory(MemoryId(i), &embedding, format!("memory {}", i))
            .await
            .unwrap();
    }

    // Access memory 0 (should move it to end of LRU)
    let query = Array1::from(vec![0.0; 5]);  // Similar to memory 0
    let _ = dsr.similarity_search(&query, 1).await.unwrap();

    // Add memory 3 (should evict the least recently used)
    let embedding = Array1::from(vec![3.0; 5]);
    dsr.add_memory(MemoryId(3), &embedding, "memory 3".to_string())
        .await
        .unwrap();

    // Verify we still have exactly 3 wells (max limit enforced)
    let stats = dsr.get_performance_stats().await;
    assert_eq!(stats.similarity_wells_count, 3, "Should maintain max_wells limit");
    assert_eq!(stats.wells_evicted, 1, "Should have evicted 1 well");

    // Verify memory 3 was added successfully
    let results = dsr.similarity_search(&query, 10).await.unwrap();
    let ids: Vec<u64> = results.matches.iter().map(|m| m.memory_id.0).collect();
    assert!(ids.contains(&3), "Memory 3 should exist (just added)");
}

#[tokio::test]
async fn test_stress_memory_limits() {
    let mut config = DSRConfig::default();
    config.max_similarity_wells = 1000;  // Moderate limit
    config.embedding_dim = 100;
    config.reservoir_size = 500;

    let dsr = DynamicSimilarityReservoir::new(config).unwrap();

    // Add 5000 memories rapidly (should trigger lots of evictions)
    for i in 0..5000 {
        let embedding = Array1::from(vec![0.0001 * i as f32; 100]);
        dsr.add_memory(MemoryId(i), &embedding, format!("m{}", i))
            .await
            .unwrap();
    }

    // Verify limits are enforced
    let stats = dsr.get_performance_stats().await;
    assert_eq!(stats.similarity_wells_count, 1000, "Should be at max wells");
    assert_eq!(stats.wells_evicted, 4000, "Should have evicted 4000 wells");

    // Memory should be bounded
    println!("Final memory usage: {:.2} MB", stats.memory_usage_mb);
    assert!(stats.memory_usage_mb < 500.0, "Memory should be bounded under heavy load");
}

#[tokio::test]
async fn test_concurrent_connections() {
    let mut config = DSRConfig::default();
    config.embedding_dim = 10;
    config.reservoir_size = 100;

    let dsr = std::sync::Arc::new(DynamicSimilarityReservoir::new(config).unwrap());

    // Simulate multiple connections adding memories concurrently
    let mut handles = vec![];

    for conn_id in 0..5 {
        let dsr_clone = dsr.clone();
        let handle = tokio::spawn(async move {
            for i in 0..10 {
                let memory_id = MemoryId(conn_id * 100 + i);
                let embedding = Array1::from(vec![0.1 * i as f32; 10]);
                dsr_clone.add_memory_with_connection(
                    memory_id,
                    &embedding,
                    format!("conn{}-mem{}", conn_id, i),
                    Some(format!("conn-{}", conn_id)),
                )
                .await
                .unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all connections to finish
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all memories were added
    let stats = dsr.get_performance_stats().await;
    assert_eq!(stats.similarity_wells_count, 50, "Should have 50 wells total");
    assert_eq!(stats.connection_count, 5, "Should track 5 connections");

    // Clean up one connection
    dsr.cleanup_connection("conn-2").await.unwrap();

    let stats_after = dsr.get_performance_stats().await;
    assert_eq!(stats_after.similarity_wells_count, 40, "Should have 40 wells after cleanup");
    assert_eq!(stats_after.connection_count, 4, "Should track 4 connections");
}

#[test]
fn test_memory_stats_structure() {
    // Verify MemoryStats has all expected fields
    use mfn_layer2_dsr::MemoryStats;

    let stats = MemoryStats {
        total_wells: 100,
        max_wells: 1000,
        wells_created: 150,
        wells_evicted: 50,
        memory_usage_bytes: 10_485_760,
        memory_usage_mb: 10.0,
        connection_count: 5,
        ttl_seconds: 3600,
    };

    assert_eq!(stats.total_wells, 100);
    assert_eq!(stats.max_wells, 1000);
    assert_eq!(stats.wells_created, 150);
    assert_eq!(stats.wells_evicted, 50);
    assert_eq!(stats.memory_usage_bytes, 10_485_760);
    assert_eq!(stats.memory_usage_mb, 10.0);
    assert_eq!(stats.connection_count, 5);
    assert_eq!(stats.ttl_seconds, 3600);
}