use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use mfn_layer2_dsr::{DynamicSimilarityReservoir, DSRConfig, MemoryId};
use ndarray::Array1;

fn benchmark_similarity_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("similarity_search");

    // Test different embedding dimensions
    for embedding_dim in [10, 50, 100, 384].iter() {
        let mut config = DSRConfig::default();
        config.embedding_dim = *embedding_dim;
        config.reservoir_size = 500; // Reasonable size

        let rt = tokio::runtime::Runtime::new().unwrap();
        let dsr = DynamicSimilarityReservoir::new(config).unwrap();

        // Add 100 memories
        rt.block_on(async {
            for i in 0..100u64 {
                let mut values = vec![0.0; *embedding_dim];
                for (j, val) in values.iter_mut().enumerate() {
                    *val = ((i * 13 + j as u64 * 7) % 100) as f32 / 100.0;
                }
                let embedding = Array1::from(values);
                dsr.add_memory(MemoryId(i), &embedding, format!("memory {}", i))
                    .await
                    .unwrap();
            }
        });

        group.bench_with_input(
            BenchmarkId::from_parameter(embedding_dim),
            embedding_dim,
            |b, &dim| {
                let query = Array1::from(vec![0.5; dim]);
                b.iter(|| {
                    rt.block_on(async {
                        let results = dsr.similarity_search(black_box(&query), 10).await.unwrap();
                        black_box(results);
                    });
                });
            },
        );
    }

    group.finish();
}

fn benchmark_memory_addition(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_addition");

    for embedding_dim in [10, 50, 100, 384].iter() {
        let mut config = DSRConfig::default();
        config.embedding_dim = *embedding_dim;
        config.reservoir_size = 500;

        let rt = tokio::runtime::Runtime::new().unwrap();

        group.bench_with_input(
            BenchmarkId::from_parameter(embedding_dim),
            embedding_dim,
            |b, &dim| {
                let mut counter = 0u64;
                b.iter(|| {
                    let dsr = DynamicSimilarityReservoir::new(config.clone()).unwrap();
                    let embedding = Array1::from(vec![0.5; dim]);
                    rt.block_on(async {
                        dsr.add_memory(
                            MemoryId(counter),
                            black_box(&embedding),
                            "test memory".to_string(),
                        )
                        .await
                        .unwrap();
                    });
                    counter += 1;
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_similarity_search, benchmark_memory_addition);
criterion_main!(benches);
