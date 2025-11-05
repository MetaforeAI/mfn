//! Basic usage example for MFN Monolith
//!
//! Demonstrates:
//! - Adding memories to all layers
//! - Parallel query execution
//! - Performance metrics

use mfn_monolith::{
    ExactMatchCache, SimilarityIndex, GraphIndex, ContextPredictor,
    Memory, Query, orchestrator,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== MFN Monolith Basic Usage ===\n");

    // Initialize all 4 layers
    println!("Initializing layers...");
    let l1 = ExactMatchCache::new(10_000)?;
    let mut l2 = SimilarityIndex::new(10_000, true)?;
    let l3 = GraphIndex::new(10_000)?;
    let mut l4 = ContextPredictor::new(1_000)?;
    println!("✓ All layers initialized\n");

    // Add some memories
    println!("Adding memories...");
    let memories = vec![
        Memory::new(
            "Rust is a systems programming language".to_string(),
            vec![0.8, 0.2, 0.1, 0.3, 0.5],
        ),
        Memory::new(
            "Python is a high-level programming language".to_string(),
            vec![0.7, 0.3, 0.2, 0.4, 0.5],
        ),
        Memory::new(
            "Machine learning uses neural networks".to_string(),
            vec![0.2, 0.8, 0.7, 0.3, 0.1],
        ),
        Memory::new(
            "Deep learning is a subset of machine learning".to_string(),
            vec![0.3, 0.9, 0.8, 0.2, 0.1],
        ),
    ];

    for (i, memory) in memories.iter().enumerate() {
        orchestrator::add_memory_to_all(&l1, &mut l2, &l3, &mut l4, memory.clone()).await?;
        println!("  Added memory {}: {}", i + 1, &memory.content[..50.min(memory.content.len())]);
    }
    println!("✓ {} memories added\n", memories.len());

    // Add some graph edges
    println!("Creating associations...");
    l3.add_edge(memories[2].id, memories[3].id, 0.9)?;
    println!("  Associated 'Machine learning' → 'Deep learning'\n");

    // Query the system
    println!("Querying the system...");
    let query = Query::new("What is machine learning?")
        .with_embedding(vec![0.25, 0.85, 0.75, 0.25, 0.15]);

    let result = orchestrator::query_parallel(&l1, &l2, &l3, &l4, query, 3).await?;

    println!("\nQuery Results:");
    println!("  Total latency: {} µs", result.latency_us);
    println!("  Layer latencies:");
    println!("    L1 (Exact Match):  {} µs", result.layer_latencies.l1_us);
    println!("    L2 (Similarity):   {} µs", result.layer_latencies.l2_us);
    println!("    L3 (Graph):        {} µs", result.layer_latencies.l3_us);
    println!("    L4 (Prediction):   {} µs", result.layer_latencies.l4_us);

    println!("\n  Top {} results:", result.results.len());
    for (i, r) in result.results.iter().enumerate() {
        println!("    {}. [{:?}] Score: {:.3}", i + 1, r.layer, r.score);
        println!("       {}", &r.content[..r.content.len().min(60)]);
    }

    // Show statistics
    println!("\nSystem Statistics:");
    let stats = orchestrator::get_all_stats(&l1, &l2, &l3, &l4);
    println!("  L1 cache size:     {}", stats.l1_size);
    println!("  L2 index size:     {}", stats.l2_size);
    println!("  L3 graph nodes:    {}", stats.l3_nodes);
    println!("  L3 graph edges:    {}", stats.l3_edges);
    println!("  L4 sequences:      {}", stats.l4_sequences);
    println!("  L4 patterns:       {}", stats.l4_patterns);

    let l2_stats = l2.get_stats();
    println!("\nL2 Performance:");
    println!("  Total queries:     {}", l2_stats.total_queries);
    println!("  Cache hit rate:    {:.1}%", l2_stats.cache_hit_rate * 100.0);
    println!("  SIMD enabled:      {}", l2_stats.use_simd);
    println!("  Embedding dim:     {}", l2_stats.embedding_dim);

    println!("\n✓ Demo complete!");

    Ok(())
}
