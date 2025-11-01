/// Integration tests for temporal prediction engine with statistical models
use layer4_cpe::{
    TemporalAnalyzer, TemporalConfig, MemoryAccess, AccessType,
    PredictionContext, PredictionType,
};
use mfn_core::{MemoryId, current_timestamp};
use std::thread;
use std::time::Duration;

#[test]
fn test_statistical_predictions_basic() {
    let config = TemporalConfig {
        max_window_size: 1000,
        min_pattern_occurrences: 2,
        max_ngram_length: 5,
        min_prediction_confidence: 0.2,
        pattern_decay_rate: 0.1,
        max_sequence_gap_us: 10_000_000, // 10 seconds
        enable_statistical_modeling: true,
    };

    let mut analyzer = TemporalAnalyzer::new(config);

    // Simulate a repeating pattern with regular timing
    let pattern_sequence = vec![100, 101, 102, 103];
    let interval_us = 1_000_000; // 1 second

    // Add pattern 3 times to establish it
    for iteration in 0..3 {
        let base_time = current_timestamp() + (iteration * pattern_sequence.len() as u64 * interval_us);

        for (idx, &memory_id) in pattern_sequence.iter().enumerate() {
            let access = MemoryAccess {
                memory_id,
                timestamp: base_time + (idx as u64 * interval_us),
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
            };
            analyzer.add_access(access);
        }
    }

    // Small delay to simulate time passing
    thread::sleep(Duration::from_millis(10));

    // Now predict what comes next
    let context = PredictionContext {
        recent_sequence: Some(vec![100, 101, 102]), // We're at position 102
        current_timestamp: current_timestamp(),
        user_context: None,
        session_id: None,
        max_predictions: 10,
    };

    let predictions = analyzer.predict_next(&context);

    // Verify we got predictions
    assert!(!predictions.is_empty(), "Should have at least one prediction");

    // Check if memory 103 is predicted (next in pattern)
    let has_103 = predictions.iter().any(|p| p.memory_id == 103);
    assert!(has_103, "Should predict memory 103 as next in sequence");

    // Check for statistical model predictions
    let has_statistical = predictions.iter().any(|p| {
        matches!(p.prediction_type, PredictionType::StatisticalModel)
    });
    println!("Statistical predictions found: {}", has_statistical);

    // Print all predictions for debugging
    for pred in &predictions {
        println!(
            "Predicted: {} (confidence: {:.3}, type: {:?}, evidence: {:?})",
            pred.memory_id, pred.confidence, pred.prediction_type, pred.contributing_evidence
        );
    }
}

#[test]
fn test_statistical_temporal_probability() {
    let config = TemporalConfig {
        max_window_size: 500,
        min_pattern_occurrences: 2,
        max_ngram_length: 4,
        min_prediction_confidence: 0.25,
        pattern_decay_rate: 0.1,
        max_sequence_gap_us: 5_000_000,
        enable_statistical_modeling: true,
    };

    let mut analyzer = TemporalAnalyzer::new(config);

    // Create a pattern with consistent timing
    let sequence = vec![200, 201, 202];
    let interval = 500_000; // 0.5 seconds

    // Repeat pattern 5 times
    for iteration in 0..5 {
        let base_time = current_timestamp() + (iteration * 3 * interval);

        for (idx, &memory_id) in sequence.iter().enumerate() {
            let access = MemoryAccess {
                memory_id,
                timestamp: base_time + (idx as u64 * interval),
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
            };
            analyzer.add_access(access);
        }
    }

    thread::sleep(Duration::from_millis(10));

    // Predict from middle of pattern
    let context = PredictionContext {
        recent_sequence: Some(vec![200, 201]),
        current_timestamp: current_timestamp(),
        user_context: None,
        session_id: None,
        max_predictions: 5,
    };

    let predictions = analyzer.predict_next(&context);

    // Should predict 202
    assert!(!predictions.is_empty(), "Should generate predictions");

    let pred_202 = predictions.iter().find(|p| p.memory_id == 202);
    assert!(pred_202.is_some(), "Should predict memory 202");

    if let Some(pred) = pred_202 {
        println!("Prediction 202: confidence={:.3}, type={:?}", pred.confidence, pred.prediction_type);
        assert!(pred.confidence > 0.2, "Should have reasonable confidence");
    }
}

#[test]
fn test_markov_chain_predictions() {
    let config = TemporalConfig {
        max_window_size: 1000,
        min_pattern_occurrences: 2,
        max_ngram_length: 3,
        min_prediction_confidence: 0.3,
        pattern_decay_rate: 0.1,
        max_sequence_gap_us: 10_000_000,
        enable_statistical_modeling: true,
    };

    let mut analyzer = TemporalAnalyzer::new(config);

    // Create clear transition: 300 -> 301 (happens 80% of the time)
    //                          300 -> 302 (happens 20% of the time)
    let base_time = current_timestamp();

    // 300 -> 301 eight times
    for i in 0..8 {
        let t = base_time + (i * 2_000_000);
        analyzer.add_access(MemoryAccess {
            memory_id: 300,
            timestamp: t,
            access_type: AccessType::Read,
            user_context: None,
            session_id: None,
            confidence: 1.0,
        });
        analyzer.add_access(MemoryAccess {
            memory_id: 301,
            timestamp: t + 1_000_000,
            access_type: AccessType::Read,
            user_context: None,
            session_id: None,
            confidence: 1.0,
        });
    }

    // 300 -> 302 two times
    for i in 0..2 {
        let t = base_time + ((i + 8) * 2_000_000);
        analyzer.add_access(MemoryAccess {
            memory_id: 300,
            timestamp: t,
            access_type: AccessType::Read,
            user_context: None,
            session_id: None,
            confidence: 1.0,
        });
        analyzer.add_access(MemoryAccess {
            memory_id: 302,
            timestamp: t + 1_000_000,
            access_type: AccessType::Read,
            user_context: None,
            session_id: None,
            confidence: 1.0,
        });
    }

    thread::sleep(Duration::from_millis(10));

    // Now predict after seeing 300
    let context = PredictionContext {
        recent_sequence: Some(vec![300]),
        current_timestamp: current_timestamp(),
        user_context: None,
        session_id: None,
        max_predictions: 10,
    };

    let predictions = analyzer.predict_next(&context);

    // Should predict both 301 and 302, with 301 having higher confidence
    let pred_301 = predictions.iter().find(|p| p.memory_id == 301);
    let pred_302 = predictions.iter().find(|p| p.memory_id == 302);

    assert!(pred_301.is_some(), "Should predict 301");
    assert!(pred_302.is_some(), "Should predict 302");

    if let (Some(p301), Some(p302)) = (pred_301, pred_302) {
        println!("301: confidence={:.3}", p301.confidence);
        println!("302: confidence={:.3}", p302.confidence);
        assert!(p301.confidence > p302.confidence, "301 should have higher confidence than 302");
    }
}

#[test]
fn test_ngram_predictions() {
    let config = TemporalConfig {
        max_window_size: 1000,
        min_pattern_occurrences: 2,
        max_ngram_length: 5,
        min_prediction_confidence: 0.25,
        pattern_decay_rate: 0.1,
        max_sequence_gap_us: 10_000_000,
        enable_statistical_modeling: true,
    };

    let mut analyzer = TemporalAnalyzer::new(config);

    // Create a 4-gram pattern: 400 -> 401 -> 402 -> 403
    let sequence = vec![400, 401, 402, 403];
    let base_time = current_timestamp();

    // Repeat 4 times
    for iteration in 0..4 {
        let t = base_time + (iteration * 5_000_000);

        for (idx, &memory_id) in sequence.iter().enumerate() {
            analyzer.add_access(MemoryAccess {
                memory_id,
                timestamp: t + (idx as u64 * 1_000_000),
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
            });
        }
    }

    thread::sleep(Duration::from_millis(10));

    // Test prediction from 3-gram prefix
    let context = PredictionContext {
        recent_sequence: Some(vec![400, 401, 402]),
        current_timestamp: current_timestamp(),
        user_context: None,
        session_id: None,
        max_predictions: 10,
    };

    let predictions = analyzer.predict_next(&context);

    // Should predict 403
    let pred_403 = predictions.iter().find(|p| p.memory_id == 403);
    assert!(pred_403.is_some(), "Should predict 403 from n-gram");

    if let Some(pred) = pred_403 {
        println!("N-gram prediction: confidence={:.3}, type={:?}", pred.confidence, pred.prediction_type);
        assert!(pred.confidence > 0.25, "Should have reasonable confidence");
    }
}

#[test]
fn test_prediction_accuracy_tracking() {
    let config = TemporalConfig::default();
    let mut analyzer = TemporalAnalyzer::new(config);

    // Build a simple pattern
    let sequence = vec![500, 501, 502];
    let base_time = current_timestamp();

    for iteration in 0..3 {
        let t = base_time + (iteration * 3_000_000);
        for (idx, &memory_id) in sequence.iter().enumerate() {
            analyzer.add_access(MemoryAccess {
                memory_id,
                timestamp: t + (idx as u64 * 1_000_000),
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
            });
        }
    }

    thread::sleep(Duration::from_millis(10));

    // Get predictions
    let context = PredictionContext {
        recent_sequence: Some(vec![500, 501]),
        current_timestamp: current_timestamp(),
        user_context: None,
        session_id: None,
        max_predictions: 5,
    };

    let predictions = analyzer.predict_next(&context);

    // Calculate top-1 and top-5 accuracy potential
    let top_1_correct = predictions.first().map(|p| p.memory_id) == Some(502);
    let top_5_correct = predictions.iter().take(5).any(|p| p.memory_id == 502);

    println!("Top-1 correct: {}", top_1_correct);
    println!("Top-5 correct: {}", top_5_correct);

    // At least top-5 should be correct
    assert!(top_5_correct, "Expected memory should be in top 5 predictions");
}

#[test]
fn test_prediction_latency() {
    let config = TemporalConfig::default();
    let mut analyzer = TemporalAnalyzer::new(config);

    // Add significant amount of data
    let base_time = current_timestamp();
    for i in 0..100 {
        analyzer.add_access(MemoryAccess {
            memory_id: i,
            timestamp: base_time + (i * 100_000),
            access_type: AccessType::Read,
            user_context: None,
            session_id: None,
            confidence: 1.0,
        });
    }

    // Measure prediction latency
    let recent: Vec<MemoryId> = (90..100).collect();
    let context = PredictionContext {
        recent_sequence: Some(recent),
        current_timestamp: current_timestamp(),
        user_context: None,
        session_id: None,
        max_predictions: 10,
    };

    let start = std::time::Instant::now();
    let predictions = analyzer.predict_next(&context);
    let duration = start.elapsed();

    println!("Prediction latency: {:?}", duration);
    println!("Generated {} predictions", predictions.len());

    // Target: < 10ms for prediction
    assert!(duration.as_millis() < 10, "Prediction should take less than 10ms");
}

#[test]
fn test_pattern_detection_and_statistics() {
    let config = TemporalConfig {
        max_window_size: 500,
        min_pattern_occurrences: 2,
        max_ngram_length: 4,
        min_prediction_confidence: 0.3,
        pattern_decay_rate: 0.1,
        max_sequence_gap_us: 10_000_000,
        enable_statistical_modeling: true,
    };

    let mut analyzer = TemporalAnalyzer::new(config);

    // Create multiple patterns
    let pattern1 = vec![600, 601, 602];
    let pattern2 = vec![610, 611, 612];
    let base_time = current_timestamp();

    // Add pattern1 three times
    for iteration in 0..3 {
        let t = base_time + (iteration * 4_000_000);
        for (idx, &memory_id) in pattern1.iter().enumerate() {
            analyzer.add_access(MemoryAccess {
                memory_id,
                timestamp: t + (idx as u64 * 1_000_000),
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
            });
        }
    }

    // Add pattern2 twice
    for iteration in 0..2 {
        let t = base_time + ((iteration + 3) * 4_000_000);
        for (idx, &memory_id) in pattern2.iter().enumerate() {
            analyzer.add_access(MemoryAccess {
                memory_id,
                timestamp: t + (idx as u64 * 1_000_000),
                access_type: AccessType::Read,
                user_context: None,
                session_id: None,
                confidence: 1.0,
            });
        }
    }

    thread::sleep(Duration::from_millis(10));

    // Get statistics
    let stats = analyzer.get_statistics();

    println!("Total accesses: {}", stats.total_accesses);
    println!("Total patterns: {}", stats.total_patterns);
    println!("Average confidence: {:.3}", stats.average_pattern_confidence);
    println!("N-gram orders: {:?}", stats.ngram_orders);

    assert!(stats.total_patterns > 0, "Should have detected patterns");
    assert!(stats.total_accesses == 15, "Should have tracked all accesses");
}
