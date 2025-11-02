use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use layer4_cpe::{
    ContextPredictionLayer, ContextPredictionConfig,
    TemporalAnalyzer, TemporalConfig, MemoryAccess,
    UniversalSearchQuery, MemoryId,
};

fn benchmark_temporal_pattern_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("temporal_pattern_analysis");

    // Test different sequence lengths
    for seq_length in [10, 50, 100, 500].iter() {
        let config = TemporalConfig {
            window_size: *seq_length,
            min_pattern_length: 2,
            max_pattern_length: 5,
            min_confidence: 0.3,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut analyzer = TemporalAnalyzer::new(config);

        // Pre-populate with access patterns
        rt.block_on(async {
            for i in 0..*seq_length {
                let access = MemoryAccess {
                    memory_id: MemoryId((i % 20) as u64), // Create repeating patterns
                    timestamp: i as u64,
                    session_id: 1,
                };
                analyzer.add_access(access).await.unwrap();
            }
        });

        group.bench_with_input(
            BenchmarkId::from_parameter(seq_length),
            seq_length,
            |b, &_len| {
                b.iter(|| {
                    rt.block_on(async {
                        let recent_ids = vec![MemoryId(5), MemoryId(10), MemoryId(15)];
                        let predictions = analyzer.predict_next(black_box(&recent_ids), 5).await.unwrap();
                        black_box(predictions);
                    });
                });
            },
        );
    }

    group.finish();
}

fn benchmark_markov_prediction(c: &mut Criterion) {
    let mut group = c.benchmark_group("markov_chain_prediction");

    // Test with different history sizes
    for history_size in [100, 500, 1000].iter() {
        let config = TemporalConfig::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut analyzer = TemporalAnalyzer::new(config);

        // Build Markov chain with clear transitions
        rt.block_on(async {
            for i in 0..*history_size {
                let mem_id = if i % 3 == 0 { 100 } else if i % 3 == 1 { 101 } else { 102 };
                let access = MemoryAccess {
                    memory_id: MemoryId(mem_id),
                    timestamp: i as u64,
                    session_id: 1,
                };
                analyzer.add_access(access).await.unwrap();
            }
        });

        group.bench_with_input(
            BenchmarkId::from_parameter(history_size),
            history_size,
            |b, &_size| {
                b.iter(|| {
                    rt.block_on(async {
                        let recent = vec![MemoryId(100), MemoryId(101)];
                        let predictions = analyzer.predict_next(black_box(&recent), 3).await.unwrap();
                        black_box(predictions);
                    });
                });
            },
        );
    }

    group.finish();
}

fn benchmark_ngram_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("ngram_frequency_analysis");

    // Test different n-gram sizes
    for ngram_size in [2, 3, 4, 5].iter() {
        let config = TemporalConfig {
            min_pattern_length: *ngram_size,
            max_pattern_length: *ngram_size,
            window_size: 500,
            min_confidence: 0.2,
        };

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut analyzer = TemporalAnalyzer::new(config);

        // Create repeating n-gram patterns
        rt.block_on(async {
            for i in 0..500u64 {
                let access = MemoryAccess {
                    memory_id: MemoryId(i % 10), // Creates repeating sequences
                    timestamp: i,
                    session_id: 1,
                };
                analyzer.add_access(access).await.unwrap();
            }
        });

        group.bench_with_input(
            BenchmarkId::from_parameter(ngram_size),
            ngram_size,
            |b, &n| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut recent = Vec::new();
                        for j in 0..n {
                            recent.push(MemoryId(j as u64));
                        }
                        let predictions = analyzer.predict_next(black_box(&recent), 5).await.unwrap();
                        black_box(predictions);
                    });
                });
            },
        );
    }

    group.finish();
}

fn benchmark_context_prediction_layer(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_prediction_layer");

    let config = ContextPredictionConfig::default();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let layer = rt.block_on(async {
        ContextPredictionLayer::new(config).await.unwrap()
    });

    // Pre-populate with some pattern history
    rt.block_on(async {
        for i in 0..100u64 {
            let access = MemoryAccess {
                memory_id: MemoryId(i % 20),
                timestamp: i,
                session_id: 1,
            };
            layer.record_access(access).await.unwrap();
        }
    });

    group.bench_function("search_prediction", |b| {
        b.iter(|| {
            rt.block_on(async {
                let query = UniversalSearchQuery {
                    content: "test query".to_string(),
                    max_results: 5,
                    min_score: 0.0,
                    context: Some(vec![
                        MemoryId(5).0.to_string(),
                        MemoryId(10).0.to_string(),
                    ]),
                    tags: vec![],
                    layers: vec![],
                };
                let decision = layer.search(black_box(&query)).await.unwrap();
                black_box(decision);
            });
        });
    });

    group.bench_function("record_access", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            rt.block_on(async {
                let access = MemoryAccess {
                    memory_id: MemoryId(counter % 20),
                    timestamp: counter,
                    session_id: 1,
                };
                layer.record_access(black_box(access)).await.unwrap();
                counter += 1;
            });
        });
    });

    group.finish();
}

fn benchmark_session_isolation(c: &mut Criterion) {
    let mut group = c.benchmark_group("session_isolation");

    // Test with multiple concurrent sessions
    for num_sessions in [1, 5, 10, 20].iter() {
        let config = TemporalConfig::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut analyzer = TemporalAnalyzer::new(config);

        // Create data for multiple sessions
        rt.block_on(async {
            for session in 0..*num_sessions {
                for i in 0..50 {
                    let access = MemoryAccess {
                        memory_id: MemoryId((session * 100 + i) as u64),
                        timestamp: i as u64,
                        session_id: session as u64,
                    };
                    analyzer.add_access(access).await.unwrap();
                }
            }
        });

        group.bench_with_input(
            BenchmarkId::from_parameter(num_sessions),
            num_sessions,
            |b, &_sessions| {
                b.iter(|| {
                    rt.block_on(async {
                        let recent = vec![MemoryId(105), MemoryId(110)];
                        let predictions = analyzer.predict_next(black_box(&recent), 3).await.unwrap();
                        black_box(predictions);
                    });
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_temporal_pattern_analysis,
    benchmark_markov_prediction,
    benchmark_ngram_analysis,
    benchmark_context_prediction_layer,
    benchmark_session_isolation
);
criterion_main!(benches);
