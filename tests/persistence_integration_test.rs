/// Integration test for AOF persistence across MFN layers
///
/// Tests the complete persistence flow:
/// 1. Write operations → AOF + in-memory state
/// 2. Create LMDB snapshot
/// 3. Simulate crash (drop in-memory state)
/// 4. Recover: Load snapshot + replay AOF
/// 5. Validate data integrity
///
/// This test validates Layers 2 and 4 (both Rust with full LMDB support)

use std::collections::HashMap;
use tempfile::TempDir;

// Layer 2 (DSR) imports
use layer2_dsr::persistence::{
    AofHandle, AofWriter, SnapshotCreator, RecoveryManager, AofEntryType,
    WellSnapshot, PersistenceConfig,
};
use layer2_dsr::MemoryId as Layer2MemoryId;

// Layer 4 (CPE) imports
use layer4_cpe::persistence::{
    AofHandle as L4AofHandle, AofWriter as L4AofWriter,
    SnapshotCreator as L4SnapshotCreator, RecoveryManager as L4RecoveryManager,
    PatternSnapshot as L4WellSnapshot, PersistenceConfig as L4PersistenceConfig,
};
use mfn_core::MemoryId as Layer4MemoryId;

/// Test Layer 2 (DSR) persistence integration
#[tokio::test]
async fn test_layer2_persistence_integration() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_path_buf();

    let config = PersistenceConfig {
        data_dir: base_path.clone(),
        pool_id: "test_pool".to_string(),
        fsync_interval_ms: 100,
        snapshot_interval_secs: 1,
        aof_buffer_size: 4096,
    };

    // Phase 1: Write operations
    println!("Phase 1: Writing data to AOF...");
    let (handle, rx) = AofHandle::new();
    let mut writer = AofWriter::new(
        config.aof_path(),
        rx,
        config.fsync_interval_ms,
    ).unwrap();

    // Spawn writer task
    let writer_task = tokio::spawn(async move {
        writer.run().await
    });

    // Write test data
    let test_data = vec![
        (Layer2MemoryId(1), "memory content 1", Some("conn1".to_string())),
        (Layer2MemoryId(2), "memory content 2", Some("conn1".to_string())),
        (Layer2MemoryId(3), "memory content 3", Some("conn2".to_string())),
        (Layer2MemoryId(4), "memory content 4", None),
        (Layer2MemoryId(5), "memory content 5", Some("conn1".to_string())),
    ];

    for (memory_id, content, connection_id) in &test_data {
        handle.log_add_memory(
            *memory_id,
            content.to_string(),
            connection_id.clone(),
        ).unwrap();
    }

    // Update some memories
    handle.log_update_memory(Layer2MemoryId(1), 10, 0.9).unwrap();
    handle.log_update_memory(Layer2MemoryId(2), 5, 0.7).unwrap();

    // Remove one memory
    handle.log_remove_memory(Layer2MemoryId(5), "evicted".to_string()).unwrap();

    // Close writer
    drop(handle);
    writer_task.await.unwrap().unwrap();

    // Phase 2: Create snapshot
    println!("Phase 2: Creating LMDB snapshot...");
    let snapshot_creator = SnapshotCreator::new(config.snapshot_path()).unwrap();

    // Build in-memory state (simulating what would be in Layer 2)
    let mut wells = HashMap::new();
    for (memory_id, content, connection_id) in &test_data[..4] { // Skip the removed one
        wells.insert(*memory_id, WellSnapshot {
            memory_id: *memory_id,
            content: content.to_string(),
            strength: if memory_id.0 == 1 { 0.9 } else if memory_id.0 == 2 { 0.7 } else { 0.8 },
            activation_count: if memory_id.0 == 1 { 10 } else if memory_id.0 == 2 { 5 } else { 0 },
            connection_id: connection_id.clone(),
            created_timestamp_ms: 1000,
            last_accessed_timestamp_ms: 2000,
        });
    }

    snapshot_creator.create_snapshot(&wells).unwrap();

    // Phase 3: Simulate crash (drop in-memory state)
    println!("Phase 3: Simulating crash - dropping in-memory state...");
    drop(wells);
    drop(snapshot_creator);

    // Phase 4: Recovery
    println!("Phase 4: Recovering from snapshot + AOF...");
    let recovery_mgr = RecoveryManager::new(config.snapshot_path()).unwrap();
    let (recovered_wells, stats) = recovery_mgr.recover(config.aof_path()).unwrap();

    // Phase 5: Validation
    println!("Phase 5: Validating recovered data...");
    println!("  Recovery stats: {:?}", stats);

    // Should have 4 wells (5 added - 1 removed)
    assert_eq!(recovered_wells.len(), 4, "Should have 4 wells after recovery");

    // Verify memory 1 (updated)
    let well1 = recovered_wells.get(&Layer2MemoryId(1)).expect("Memory 1 should exist");
    assert_eq!(well1.content, "memory content 1");
    assert_eq!(well1.activation_count, 10);
    assert_eq!(well1.strength, 0.9);

    // Verify memory 2 (updated)
    let well2 = recovered_wells.get(&Layer2MemoryId(2)).expect("Memory 2 should exist");
    assert_eq!(well2.activation_count, 5);
    assert_eq!(well2.strength, 0.7);

    // Verify memory 3 and 4 exist
    assert!(recovered_wells.contains_key(&Layer2MemoryId(3)));
    assert!(recovered_wells.contains_key(&Layer2MemoryId(4)));

    // Verify memory 5 was removed
    assert!(!recovered_wells.contains_key(&Layer2MemoryId(5)), "Memory 5 should be removed");

    println!("✅ Layer 2 persistence integration test PASSED");
}

/// Test Layer 4 (CPE) persistence integration
#[tokio::test]
async fn test_layer4_persistence_integration() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_path_buf();

    let config = L4PersistenceConfig {
        data_dir: base_path.clone(),
        pool_id: "test_pool".to_string(),
        fsync_interval_ms: 100,
        snapshot_interval_secs: 1,
        aof_buffer_size: 4096,
    };

    // Phase 1: Write operations
    println!("Phase 1: Writing data to AOF...");
    let (handle, rx) = L4AofHandle::new();
    let mut writer = L4AofWriter::new(
        config.aof_path(),
        rx,
        config.fsync_interval_ms,
    ).unwrap();

    let writer_task = tokio::spawn(async move {
        writer.run().await
    });

    // Write test data (using u64 directly for Layer 4)
    let test_data = vec![
        (1u64, "pattern 1", Some("conn1".to_string())),
        (2u64, "pattern 2", Some("conn1".to_string())),
        (3u64, "pattern 3", Some("conn2".to_string())),
        (4u64, "pattern 4", None),
        (5u64, "pattern 5", Some("conn1".to_string())),
    ];

    for (memory_id, content, connection_id) in &test_data {
        handle.log_add_memory(
            *memory_id,
            content.to_string(),
            connection_id.clone(),
        ).unwrap();
    }

    // Update some patterns
    handle.log_update_memory(1, 10, 0.9).unwrap();
    handle.log_update_memory(2, 5, 0.7).unwrap();

    // Remove one pattern
    handle.log_remove_memory(5, "evicted".to_string()).unwrap();

    drop(handle);
    writer_task.await.unwrap().unwrap();

    // Phase 2: Create snapshot
    println!("Phase 2: Creating LMDB snapshot...");
    let snapshot_creator = L4SnapshotCreator::new(config.snapshot_path()).unwrap();

    let mut patterns = HashMap::new();
    for (memory_id, content, connection_id) in &test_data[..4] {
        patterns.insert(*memory_id, L4WellSnapshot {
            memory_id: *memory_id,
            content: content.to_string(),
            strength: if *memory_id == 1 { 0.9 } else if *memory_id == 2 { 0.7 } else { 0.8 },
            activation_count: if *memory_id == 1 { 10 } else if *memory_id == 2 { 5 } else { 0 },
            connection_id: connection_id.clone(),
            created_timestamp_ms: 1000,
            last_accessed_timestamp_ms: 2000,
        });
    }

    snapshot_creator.create_snapshot(&patterns).unwrap();

    // Phase 3: Simulate crash
    println!("Phase 3: Simulating crash - dropping in-memory state...");
    drop(patterns);
    drop(snapshot_creator);

    // Phase 4: Recovery
    println!("Phase 4: Recovering from snapshot + AOF...");
    let recovery_mgr = L4RecoveryManager::new(config.snapshot_path()).unwrap();
    let (recovered_patterns, stats) = recovery_mgr.recover(config.aof_path()).unwrap();

    // Phase 5: Validation
    println!("Phase 5: Validating recovered data...");
    println!("  Recovery stats: {:?}", stats);

    assert_eq!(recovered_patterns.len(), 4, "Should have 4 patterns after recovery");

    // Verify pattern 1 (updated)
    let pattern1 = recovered_patterns.get(&1).expect("Pattern 1 should exist");
    assert_eq!(pattern1.content, "pattern 1");
    assert_eq!(pattern1.activation_count, 10);
    assert_eq!(pattern1.strength, 0.9);

    // Verify pattern 5 was removed
    assert!(!recovered_patterns.contains_key(&5), "Pattern 5 should be removed");

    println!("✅ Layer 4 persistence integration test PASSED");
}

/// Test recovery with corrupted AOF entries
#[tokio::test]
async fn test_recovery_with_corruption() {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_path_buf();

    let config = PersistenceConfig {
        data_dir: base_path.clone(),
        pool_id: "corruption_test".to_string(),
        fsync_interval_ms: 100,
        snapshot_interval_secs: 1,
        aof_buffer_size: 4096,
    };

    // Create snapshot with initial data
    let snapshot_creator = SnapshotCreator::new(config.snapshot_path()).unwrap();
    let mut wells = HashMap::new();
    wells.insert(Layer2MemoryId(1), WellSnapshot {
        memory_id: Layer2MemoryId(1),
        content: "initial".to_string(),
        strength: 0.5,
        activation_count: 1,
        connection_id: None,
        created_timestamp_ms: 1000,
        last_accessed_timestamp_ms: 1000,
    });
    snapshot_creator.create_snapshot(&wells).unwrap();
    drop(snapshot_creator);

    // Manually create AOF with corrupted entry
    use std::fs::OpenOptions;
    use std::io::Write;

    let mut aof_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(config.aof_path())
        .unwrap();

    // Valid entry
    writeln!(aof_file, r#"{{"timestamp_ms":2000,"entry_type":"add_memory","data":{{"memory_id":2,"content":"valid","connection_id":null}}}}"#).unwrap();

    // Corrupted entry
    writeln!(aof_file, "{{corrupted json entry}}").unwrap();

    // Another valid entry
    writeln!(aof_file, r#"{{"timestamp_ms":3000,"entry_type":"add_memory","data":{{"memory_id":3,"content":"also valid","connection_id":null}}}}"#).unwrap();

    drop(aof_file);

    // Attempt recovery - should skip corrupted entry
    let recovery_mgr = RecoveryManager::new(config.snapshot_path()).unwrap();
    let (recovered_wells, stats) = recovery_mgr.recover(config.aof_path()).unwrap();

    println!("Recovery with corruption stats: {:?}", stats);

    // Should have 3 wells (1 from snapshot + 2 valid from AOF)
    assert_eq!(recovered_wells.len(), 3);
    assert_eq!(stats.aof_entries_replayed, 2);
    assert_eq!(stats.aof_entries_skipped, 1);

    println!("✅ Corruption handling test PASSED");
}
