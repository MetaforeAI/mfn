use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use mfn_monolith::{layer1, layer2, layer3, layer4, orchestrator, types::*};
use std::sync::Arc;

fn create_test_memory(id: usize, embedding_dim: usize) -> Memory {
    let embedding: Vec<f32> = (0..embedding_dim)
        .map(|i| ((i + id) as f32 * 0.1) % 1.0)
        .collect();

    Memory::new(
        format!("Memory content {}", id),
        embedding,
    )
}

fn benchmark_layer1_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer1_exact_match");

    for size in [100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let cache = layer1::ExactMatchCache::new(size).unwrap();

            // Populate cache
            for i in 0..size {
                let mem = create_test_memory(i, 384);
                cache.insert(mem).unwrap();
            }

            // Query for a known item
            let query = Query::new("Memory content 42");

            b.iter(|| {
                black_box(cache.get(&query))
            });
        });
    }

    group.finish();
}

fn benchmark_layer2_simd_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer2_similarity_search");

    for size in [100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut index = layer2::SimilarityIndex::new(size, true).unwrap();

            // Populate index
            for i in 0..size {
                let mem = create_test_memory(i, 384);
                index.add(mem).unwrap();
            }

            let query = Query::new("Memory content")
                .with_embedding(vec![0.5; 384]);

            b.iter(|| {
                black_box(index.search(&query, 10))
            });
        });
    }

    group.finish();
}

fn benchmark_layer3_graph_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer3_graph_traversal");

    for size in [100, 1_000, 10_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let index = layer3::GraphIndex::new(size).unwrap();

            // Populate graph
            for i in 0..size {
                let mem = create_test_memory(i, 384);
                index.add_node(mem).unwrap();
            }

            // Add some associations
            for i in 0..size.min(100) {
                let mem1 = create_test_memory(i, 384);
                let mem2 = create_test_memory((i + 1) % size, 384);
                index.add_edge(
                    mem1.id,
                    mem2.id,
                    0.8
                ).ok();
            }

            let query = Query::new("Memory content");

            b.iter(|| {
                black_box(index.traverse(&query, 3))
            });
        });
    }

    group.finish();
}

fn benchmark_full_parallel_query(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("full_parallel_query");

    for size in [100, 1_000] {
        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let l1 = Arc::new(layer1::ExactMatchCache::new(size).unwrap());
            let mut l2 = layer2::SimilarityIndex::new(size, true).unwrap();
            let l3 = Arc::new(layer3::GraphIndex::new(size).unwrap());
            let mut l4 = layer4::ContextPredictor::new(10).unwrap();

            // Populate all layers
            rt.block_on(async {
                for i in 0..size {
                    let mem = create_test_memory(i, 384);
                    orchestrator::add_memory_to_all(&l1, &mut l2, &l3, &mut l4, mem)
                        .await
                        .unwrap();
                }
            });

            let l2 = Arc::new(l2);
            let l4 = Arc::new(l4);
            let query = Query::new("Memory content")
                .with_embedding(vec![0.5; 384]);

            b.iter(|| {
                rt.block_on(async {
                    black_box(
                        orchestrator::query_parallel(&l1, &l2, &l3, &l4, query.clone(), 10)
                            .await
                    )
                })
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_layer1_lookup,
    benchmark_layer2_simd_search,
    benchmark_layer3_graph_search,
    benchmark_full_parallel_query
);
criterion_main!(benches);
