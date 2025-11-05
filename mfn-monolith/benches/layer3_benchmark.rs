use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use mfn_monolith::{GraphIndex, Memory, Query, SearchMode};

fn create_test_memory(content: &str) -> Memory {
    Memory::new(
        content.to_string(),
        vec![0.1, 0.2, 0.3, 0.4, 0.5], // dummy embedding
    )
}

fn setup_graph(size: usize) -> GraphIndex {
    let index = GraphIndex::new(size * 2).unwrap();

    // Create memories
    let mut memories = Vec::new();
    for i in 0..size {
        let mem = create_test_memory(&format!("Memory {}", i));
        memories.push(mem.clone());
        index.add_node(mem).unwrap();
    }

    // Create edges (each node connected to 3 neighbors)
    for i in 0..size {
        for j in 1..=3 {
            let target = (i + j) % size;
            if i != target {
                let weight = 0.7 + (0.3 * (j as f64 / 3.0));
                index.add_edge(memories[i].id, memories[target].id, weight).ok();
            }
        }
    }

    index
}

fn bench_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer3_traversal");

    for size in [10, 50, 100, 500].iter() {
        let index = setup_graph(*size);
        let query = Query::new("test query");

        group.bench_with_input(
            BenchmarkId::new("bfs", size),
            size,
            |b, _| {
                b.iter(|| {
                    black_box(index.traverse(&query, 3))
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("dfs", size),
            size,
            |b, _| {
                b.iter(|| {
                    black_box(index.traverse_with_mode(&query, 3, SearchMode::DepthFirst))
                })
            },
        );

        group.bench_with_input(
            BenchmarkId::new("best_first", size),
            size,
            |b, _| {
                b.iter(|| {
                    black_box(index.traverse_with_mode(&query, 3, SearchMode::BestFirst))
                })
            },
        );
    }

    group.finish();
}

fn bench_graph_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("layer3_operations");

    let index = GraphIndex::new(1000).unwrap();

    group.bench_function("add_node", |b| {
        b.iter(|| {
            let mem = create_test_memory("test");
            black_box(index.add_node(mem))
        })
    });

    // Setup for edge operations
    let mem1 = create_test_memory("mem1");
    let mem2 = create_test_memory("mem2");
    index.add_node(mem1.clone()).unwrap();
    index.add_node(mem2.clone()).unwrap();

    group.bench_function("add_edge", |b| {
        b.iter(|| {
            black_box(index.add_edge(mem1.id, mem2.id, 0.8))
        })
    });

    group.bench_function("get_memory", |b| {
        b.iter(|| {
            black_box(index.get_memory(&mem1.id))
        })
    });

    group.bench_function("get_neighbors", |b| {
        b.iter(|| {
            black_box(index.get_neighbors(&mem1.id))
        })
    });

    group.finish();
}

criterion_group!(benches, bench_traversal, bench_graph_operations);
criterion_main!(benches);
