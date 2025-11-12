/// Memory limit tests for Layer 4 CPE
/// Validates that memory growth is properly bounded
use layer4_cpe::{
    TemporalAnalyzer,
    TemporalConfig,
    MemoryAccess,
    PredictionContext,
};
use layer4_cpe::temporal::AccessType;
use mfn_core::current_timestamp;

#[test]
fn test_ngram_limit_enforcement() {
    let config = TemporalConfig {
        max_window_size: 1000,
        min_pattern_occurrences: 3,
        max_ngram_length: 5, // Limited to prevent explosion
        min_prediction_confidence: 0.3,
        pattern_decay_rate: 0.1,
        max_sequence_gap_us: 60_000_000,
        enable_statistical_modeling: true,
    };

    let mut analyzer = TemporalAnalyzer::new(config);

    // Generate massive input to test bounded storage
    for i in 0..1_000_000 {
        let access = MemoryAccess {
            memory_id: (i % 10000) as u64, // Cycle through 10k unique IDs
            timestamp: current_timestamp() + i,
            access_type: AccessType::Read,
            user_context: Some(format!("context_{}", i % 100)),
            session_id: Some(format!("session_{}", i % 10)),
            confidence: 1.0,
            connection_id: Some(format!("conn_{}", i % 5)),
        };

        analyzer.add_access(access);

        // Check memory stats periodically
        if i % 10000 == 0 {
            let stats = analyzer.get_statistics();
            println!("After {} accesses - N-grams: {}, Patterns: {}, Memory: {:.2} MB",
                i, stats.ngram_count, stats.total_patterns,
                stats.memory_usage_estimate as f64 / 1_048_576.0);

            // Verify n-gram count is bounded
            assert!(stats.ngram_count <= 1_000_000, "N-gram count exceeded limit: {}", stats.ngram_count);

            // Verify pattern count is bounded
            assert!(stats.total_patterns <= 10_000, "Pattern count exceeded limit: {}", stats.total_patterns);

            // Verify memory usage is reasonable (under 2GB)
            assert!(stats.memory_usage_estimate < 2_147_483_648, "Memory usage exceeded 2GB: {}", stats.memory_usage_estimate);
        }
    }

    let final_stats = analyzer.get_statistics();
    println!("Final stats - N-grams: {}, Patterns: {}, Memory: {:.2} MB",
        final_stats.ngram_count, final_stats.total_patterns,
        final_stats.memory_usage_estimate as f64 / 1_048_576.0);

    // Final assertions
    assert!(final_stats.ngram_count <= 1_000_000, "Final n-gram count exceeded limit");
    assert!(final_stats.total_patterns <= 10_000, "Final pattern count exceeded limit");
    assert!(final_stats.memory_usage_estimate < 2_147_483_648, "Final memory usage exceeded 2GB");
}

#[test]
fn test_frequency_threshold_filtering() {
    let config = TemporalConfig::default();
    let mut analyzer = TemporalAnalyzer::new(config);

    // Add low-frequency n-grams
    for i in 0..1000 {
        let access = MemoryAccess {
            memory_id: i as u64, // Unique ID each time - low frequency
            timestamp: current_timestamp() + i,
            access_type: AccessType::Read,
            user_context: None,
            session_id: None,
            confidence: 1.0,
            connection_id: None,
        };
        analyzer.add_access(access);
    }

    // Add high-frequency n-grams
    for _ in 0..10 {
        for j in 0..5 {
            let access = MemoryAccess {
                memory_id: j as u64, // Same sequence repeated - high frequency
                timestamp: current_timestamp(),
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
                connection_id: None,
            };
            analyzer.add_access(access);
        }
    }

    let stats = analyzer.get_statistics();
    println!("Frequency test - N-grams: {}, Memory: {:.2} MB",
        stats.ngram_count, stats.memory_usage_estimate as f64 / 1_048_576.0);

    // Should have much fewer n-grams due to frequency filtering
    assert!(stats.ngram_count < 1000, "Too many low-frequency n-grams stored: {}", stats.ngram_count);
}

#[test]
fn test_pattern_lru_eviction() {
    let config = TemporalConfig {
        max_window_size: 100,
        min_pattern_occurrences: 2,
        max_ngram_length: 3,
        min_prediction_confidence: 0.1,
        pattern_decay_rate: 0.1,
        max_sequence_gap_us: 60_000_000,
        enable_statistical_modeling: false,
    };

    let mut analyzer = TemporalAnalyzer::new(config);

    // Create many patterns to trigger eviction
    for i in 0..20000 {
        // Create repeating pattern
        for j in 0..3 {
            let access = MemoryAccess {
                memory_id: ((i * 3 + j) % 50000) as u64,
                timestamp: current_timestamp() + i * 1000 + j,
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
                connection_id: None,
            };
            analyzer.add_access(access);
        }

        // Repeat pattern to register it
        for j in 0..3 {
            let access = MemoryAccess {
                memory_id: ((i * 3 + j) % 50000) as u64,
                timestamp: current_timestamp() + i * 1000 + 100 + j,
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
                connection_id: None,
            };
            analyzer.add_access(access);
        }
    }

    let stats = analyzer.get_statistics();
    println!("Pattern eviction test - Patterns: {}, Memory: {:.2} MB",
        stats.total_patterns, stats.memory_usage_estimate as f64 / 1_048_576.0);

    // Should be limited to 10k patterns
    assert!(stats.total_patterns <= 10_000, "Pattern count exceeded limit: {}", stats.total_patterns);
}

#[test]
fn test_connection_cleanup() {
    let config = TemporalConfig::default();
    let mut analyzer = TemporalAnalyzer::new(config);

    let conn_id = "test_connection_123".to_string();

    // Add accesses for specific connection
    for i in 0..100 {
        let access = MemoryAccess {
            memory_id: i as u64,
            timestamp: current_timestamp() + i,
            access_type: AccessType::Read,
            user_context: Some("test".to_string()),
            session_id: None,
            confidence: 1.0,
            connection_id: Some(conn_id.clone()),
        };
        analyzer.add_access(access);
    }

    let stats_before = analyzer.get_statistics();
    println!("Before cleanup - N-grams: {}, Connections: {}",
        stats_before.ngram_count, stats_before.connection_count);

    // Clean up the connection
    analyzer.cleanup_connection(&conn_id);

    let stats_after = analyzer.get_statistics();
    println!("After cleanup - N-grams: {}, Connections: {}",
        stats_after.ngram_count, stats_after.connection_count);

    // Should have fewer resources after cleanup
    assert!(stats_after.connection_count < stats_before.connection_count,
        "Connection not cleaned up properly");
}

#[test]
fn test_memory_stats_reporting() {
    let config = TemporalConfig::default();
    let mut analyzer = TemporalAnalyzer::new(config);

    // Add some data
    for i in 0..1000 {
        let access = MemoryAccess {
            memory_id: (i % 100) as u64,
            timestamp: current_timestamp() + i,
            access_type: AccessType::Read,
            user_context: None,
            session_id: None,
            confidence: 1.0,
            connection_id: None,
        };
        analyzer.add_access(access);
    }

    let memory_stats = analyzer.get_memory_stats();
    println!("Memory stats: {}", memory_stats);

    // Should contain expected information
    assert!(memory_stats.contains("N-grams:"), "Missing n-gram count");
    assert!(memory_stats.contains("Patterns:"), "Missing pattern count");
    assert!(memory_stats.contains("MB"), "Missing memory estimate");
}

#[test]
fn test_sustained_load_memory_stability() {
    let config = TemporalConfig::default();
    let mut analyzer = TemporalAnalyzer::new(config);

    let mut memory_samples = Vec::new();

    // Run sustained load for many iterations
    for iteration in 0..100 {
        for i in 0..10000 {
            let access = MemoryAccess {
                memory_id: (i % 1000) as u64,
                timestamp: current_timestamp() + (iteration * 10000 + i) as u64,
                access_type: AccessType::Read,
                user_context: Some(format!("ctx_{}", i % 50)),
                session_id: Some(format!("session_{}", iteration % 10)),
                confidence: 1.0,
                connection_id: Some(format!("conn_{}", iteration % 20)),
            };
            analyzer.add_access(access);
        }

        let stats = analyzer.get_statistics();
        memory_samples.push(stats.memory_usage_estimate);

        // Clean up old connections periodically
        if iteration % 10 == 0 {
            for old_conn in 0..5 {
                let old_conn_id = format!("conn_{}", old_conn);
                analyzer.cleanup_connection(&old_conn_id);
            }
        }
    }

    // Check that memory is stable (not continuously growing)
    let early_avg = memory_samples[..10].iter().sum::<usize>() / 10;
    let late_avg = memory_samples[90..].iter().sum::<usize>() / 10;

    println!("Memory stability - Early avg: {:.2} MB, Late avg: {:.2} MB",
        early_avg as f64 / 1_048_576.0, late_avg as f64 / 1_048_576.0);

    // Memory should not grow more than 2x from early to late
    assert!(late_avg < early_avg * 2,
        "Memory grew too much: {} -> {} bytes", early_avg, late_avg);

    // Final memory should be under 2GB
    assert!(memory_samples.last().unwrap() < &2_147_483_648,
        "Final memory usage exceeded 2GB");
}