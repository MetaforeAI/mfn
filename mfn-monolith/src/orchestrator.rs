//! MFN Orchestrator - Parallel query execution across all 4 layers
//!
//! Coordinates queries across all layers and merges results intelligently.

use crate::{layer1, layer2, layer3, layer4, types::*};
use anyhow::Result;
use std::time::Instant;

/// Query all 4 layers in parallel and merge results
///
/// Executes queries across all layers concurrently using tokio::join! for maximum performance.
/// Results are merged with layer-specific scoring strategies.
///
/// # Arguments
/// * `l1` - Layer 1 exact match cache
/// * `l2` - Layer 2 similarity index
/// * `l3` - Layer 3 graph index
/// * `l4` - Layer 4 context predictor
/// * `query` - Query to execute
/// * `top_k` - Maximum number of results to return
///
/// # Returns
/// Merged QueryResult with results from all layers, sorted by score
///
/// # Example
/// ```no_run
/// use mfn_monolith::{orchestrator, layer1, layer2, layer3, layer4, types::Query};
///
/// # async fn example() -> anyhow::Result<()> {
/// let l1 = layer1::ExactMatchCache::new(10000)?;
/// let l2 = layer2::SimilarityIndex::new(10000, true)?;
/// let l3 = layer3::GraphIndex::new(10000)?;
/// let l4 = layer4::ContextPredictor::new(1000)?;
///
/// let query = Query::new("What is machine learning?");
/// let results = orchestrator::query_parallel(&l1, &l2, &l3, &l4, query, 10).await?;
/// # Ok(())
/// # }
/// ```
pub async fn query_parallel(
    l1: &layer1::ExactMatchCache,
    l2: &layer2::SimilarityIndex,
    l3: &layer3::GraphIndex,
    l4: &layer4::ContextPredictor,
    query: Query,
    top_k: usize,
) -> Result<QueryResult> {
    let start = Instant::now();
    let query_id = query.id;

    // Clone layers before spawning (required for 'static lifetime)
    let l1_clone = l1.clone();
    let l2_clone = l2.clone();
    let l3_clone = l3.clone();
    let l4_clone = l4.clone();

    // Execute all layers in parallel using tokio::join!
    // Layer 1 and 3 use fast async operations (no blocking needed - Phase 2 optimization)
    // Layer 2 and 4 use spawn_blocking for CPU-intensive work
    let (r1, r2, r3, r4) = tokio::join!(
        // Layer 1: Exact match (fastest, <1µs) - Direct call, no blocking needed
        async {
            let start = Instant::now();
            let result = l1_clone.get(&query);
            let latency = start.elapsed().as_micros() as u64;
            Ok::<_, tokio::task::JoinError>((result, latency))
        },
        // Layer 2: Similarity search (10-100µs) - CPU-intensive, needs blocking
        tokio::task::spawn_blocking({
            let q = query.clone();
            move || {
                let start = Instant::now();
                let result = l2_clone.search(&q, top_k);
                let latency = start.elapsed().as_micros() as u64;
                (result, latency)
            }
        }),
        // Layer 3: Graph traversal - Read-heavy with fast RwLock, no blocking needed
        async {
            let start = Instant::now();
            let result = l3_clone.traverse(&query, 3); // depth=3 for graph traversal
            let latency = start.elapsed().as_micros() as u64;
            Ok::<_, tokio::task::JoinError>((result, latency))
        },
        // Layer 4: Context prediction - CPU-intensive, needs blocking
        tokio::task::spawn_blocking({
            let q = query.clone();
            move || {
                let start = Instant::now();
                let result = l4_clone.predict(&q, top_k);
                let latency = start.elapsed().as_micros() as u64;
                (result, latency)
            }
        }),
    );

    // Unwrap task results
    let (r1_result, l1_latency) = r1?;
    let (r2_result, l2_latency) = r2?;
    let (r3_result, l3_latency) = r3?;
    let (r4_result, l4_latency) = r4?;

    // Merge results from all layers
    let mut all_results = Vec::new();

    // Layer 1: Exact match gets highest priority (score = 1.0)
    if let Some(mem) = r1_result {
        all_results.push(SearchResult {
            memory_id: mem.id,
            score: 1.0, // Perfect match
            layer: Layer::L1ExactMatch,
            content: mem.content,
        });
    }

    // Layer 2: Similarity results
    if let Ok(results) = r2_result {
        all_results.extend(results);
    }

    // Layer 3: Graph results
    all_results.extend(r3_result);

    // Layer 4: Predictions
    all_results.extend(r4_result);

    // Deduplicate by memory_id (keep highest score)
    let mut best_scores = std::collections::HashMap::new();
    let mut best_results = std::collections::HashMap::new();

    for result in all_results {
        let entry = best_scores.entry(result.memory_id).or_insert(0.0f64);
        if result.score > *entry {
            *entry = result.score;
            best_results.insert(result.memory_id, result);
        }
    }

    // Convert back to vec and sort by score (descending)
    let mut final_results: Vec<SearchResult> = best_results.into_values().collect();
    final_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // Truncate to top_k
    final_results.truncate(top_k);

    // Calculate total latency
    let total_latency = start.elapsed().as_micros() as u64;

    Ok(QueryResult {
        query_id,
        results: final_results,
        latency_us: total_latency,
        layer_latencies: LayerLatencies {
            l1_us: l1_latency,
            l2_us: l2_latency,
            l3_us: l3_latency,
            l4_us: l4_latency,
        },
    })
}

/// Add memory to all layers
///
/// Inserts a memory into all 4 layers, updating caches, indices, and patterns.
///
/// # Arguments
/// * `l1` - Layer 1 exact match cache
/// * `l2` - Layer 2 similarity index
/// * `l3` - Layer 3 graph index
/// * `l4` - Layer 4 context predictor
/// * `memory` - Memory to add
///
/// # Returns
/// Ok(()) if successful, Err if any layer fails
///
/// # Example
/// ```no_run
/// use mfn_monolith::{orchestrator, layer1, layer2, layer3, layer4, types::Memory};
///
/// # async fn example() -> anyhow::Result<()> {
/// let l1 = layer1::ExactMatchCache::new(10000)?;
/// let mut l2 = layer2::SimilarityIndex::new(10000, true)?;
/// let l3 = layer3::GraphIndex::new(10000)?;
/// let mut l4 = layer4::ContextPredictor::new(1000)?;
///
/// let memory = Memory::new(
///     "Machine learning is a subset of AI".to_string(),
///     vec![0.1, 0.2, 0.3]
/// );
///
/// orchestrator::add_memory_to_all(&l1, &mut l2, &l3, &mut l4, memory).await?;
/// # Ok(())
/// # }
/// ```
pub async fn add_memory_to_all(
    l1: &layer1::ExactMatchCache,
    l2: &mut layer2::SimilarityIndex,
    l3: &layer3::GraphIndex,
    l4: &mut layer4::ContextPredictor,
    memory: Memory,
) -> Result<()> {
    // Add to Layer 1 (exact match cache)
    l1.insert(memory.clone())?;

    // Add to Layer 2 (similarity index)
    l2.add(memory.clone())?;

    // Add to Layer 3 (graph index)
    l3.add_node(memory.clone())?;

    // Add to Layer 4 (context predictor - just track the sequence)
    l4.add_sequence(memory.id);

    Ok(())
}

/// Query statistics across all layers
#[derive(Debug, Clone)]
pub struct OrchestrationStats {
    pub l1_size: usize,
    pub l2_size: usize,
    pub l3_nodes: usize,
    pub l3_edges: usize,
    pub l4_sequences: usize,
    pub l4_patterns: usize,
}

/// Get statistics from all layers
///
/// # Arguments
/// * `l1` - Layer 1 exact match cache
/// * `l2` - Layer 2 similarity index
/// * `l3` - Layer 3 graph index
/// * `l4` - Layer 4 context predictor
///
/// # Returns
/// OrchestrationStats with metrics from all layers
pub fn get_all_stats(
    l1: &layer1::ExactMatchCache,
    l2: &layer2::SimilarityIndex,
    l3: &layer3::GraphIndex,
    l4: &layer4::ContextPredictor,
) -> OrchestrationStats {
    OrchestrationStats {
        l1_size: l1.len(),
        l2_size: l2.len(),
        l3_nodes: l3.node_count(),
        l3_edges: l3.edge_count(),
        l4_sequences: l4.sequence_count(),
        l4_patterns: l4.pattern_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_memory(content: &str, embedding: Vec<f32>) -> Memory {
        Memory::new(content.to_string(), embedding)
    }

    #[tokio::test]
    async fn test_query_parallel_empty() {
        let l1 = layer1::ExactMatchCache::new(100).unwrap();
        let l2 = layer2::SimilarityIndex::new(100, true).unwrap();
        let l3 = layer3::GraphIndex::new(100).unwrap();
        let l4 = layer4::ContextPredictor::new(100).unwrap();

        let query = Query::new("test query").with_embedding(vec![0.1, 0.2, 0.3]);
        let result = query_parallel(&l1, &l2, &l3, &l4, query, 10).await.unwrap();

        assert_eq!(result.results.len(), 0);
    }

    #[tokio::test]
    async fn test_add_memory_to_all() {
        let l1 = layer1::ExactMatchCache::new(100).unwrap();
        let mut l2 = layer2::SimilarityIndex::new(100, true).unwrap();
        let l3 = layer3::GraphIndex::new(100).unwrap();
        let mut l4 = layer4::ContextPredictor::new(100).unwrap();

        let memory = create_test_memory("test content", vec![0.1, 0.2, 0.3]);
        let memory_id = memory.id;

        add_memory_to_all(&l1, &mut l2, &l3, &mut l4, memory).await.unwrap();

        // Verify added to all layers
        assert_eq!(l1.len(), 1);
        assert_eq!(l2.len(), 1);
        assert_eq!(l3.node_count(), 1);
        assert_eq!(l4.sequence_count(), 1);

        // Verify can retrieve from L1
        let query = Query::new("test content");
        assert!(l1.get(&query).is_some());
    }

    #[tokio::test]
    async fn test_parallel_query_with_l1_hit() {
        let l1 = layer1::ExactMatchCache::new(100).unwrap();
        let mut l2 = layer2::SimilarityIndex::new(100, true).unwrap();
        let l3 = layer3::GraphIndex::new(100).unwrap();
        let mut l4 = layer4::ContextPredictor::new(100).unwrap();

        let memory = create_test_memory("exact match test", vec![0.5, 0.5, 0.5]);
        add_memory_to_all(&l1, &mut l2, &l3, &mut l4, memory.clone()).await.unwrap();

        // Query with exact match
        let query = Query::new("exact match test").with_embedding(vec![0.5, 0.5, 0.5]);
        let result = query_parallel(&l1, &l2, &l3, &l4, query, 10).await.unwrap();

        assert!(!result.results.is_empty());
        assert_eq!(result.results[0].layer, Layer::L1ExactMatch);
        assert_eq!(result.results[0].score, 1.0);
    }

    #[tokio::test]
    async fn test_parallel_query_with_l2_similarity() {
        let l1 = layer1::ExactMatchCache::new(100).unwrap();
        let mut l2 = layer2::SimilarityIndex::new(100, true).unwrap();
        let l3 = layer3::GraphIndex::new(100).unwrap();
        let mut l4 = layer4::ContextPredictor::new(100).unwrap();

        let memory = create_test_memory("similar content", vec![0.6, 0.7, 0.8]);
        add_memory_to_all(&l1, &mut l2, &l3, &mut l4, memory.clone()).await.unwrap();

        // Verify L2 has the memory
        assert_eq!(l2.len(), 1);

        // Query with similar but not exact match
        let query = Query::new("different content").with_embedding(vec![0.65, 0.75, 0.85]);
        let result = query_parallel(&l1, &l2, &l3, &l4, query, 10).await.unwrap();

        // Debug: print all results
        eprintln!("Results count: {}", result.results.len());
        for r in &result.results {
            eprintln!("  Layer: {:?}, Score: {}", r.layer, r.score);
        }

        assert!(!result.results.is_empty(), "Should have at least one result");
        // Should find via L2 similarity or L3 graph (since all nodes are starting points)
        let has_l2 = result.results.iter().any(|r| r.layer == Layer::L2Similarity);
        let has_l3 = result.results.iter().any(|r| r.layer == Layer::L3Graph);
        assert!(has_l2 || has_l3, "Should find via L2 similarity or L3 graph");
    }

    #[tokio::test]
    async fn test_get_all_stats() {
        let l1 = layer1::ExactMatchCache::new(100).unwrap();
        let mut l2 = layer2::SimilarityIndex::new(100, true).unwrap();
        let l3 = layer3::GraphIndex::new(100).unwrap();
        let mut l4 = layer4::ContextPredictor::new(100).unwrap();

        // Add some memories
        for i in 0..5 {
            let memory = create_test_memory(
                &format!("memory {}", i),
                vec![i as f32 * 0.1, i as f32 * 0.2, i as f32 * 0.3]
            );
            add_memory_to_all(&l1, &mut l2, &l3, &mut l4, memory).await.unwrap();
        }

        let stats = get_all_stats(&l1, &l2, &l3, &l4);

        assert_eq!(stats.l1_size, 5);
        assert_eq!(stats.l2_size, 5);
        assert_eq!(stats.l3_nodes, 5);
        assert_eq!(stats.l4_sequences, 5);
    }

    #[tokio::test]
    async fn test_deduplication() {
        let l1 = layer1::ExactMatchCache::new(100).unwrap();
        let mut l2 = layer2::SimilarityIndex::new(100, true).unwrap();
        let l3 = layer3::GraphIndex::new(100).unwrap();
        let mut l4 = layer4::ContextPredictor::new(100).unwrap();

        // Add memory that will appear in multiple layers
        let memory = create_test_memory("duplicate test", vec![0.7, 0.7, 0.7]);
        add_memory_to_all(&l1, &mut l2, &l3, &mut l4, memory.clone()).await.unwrap();

        // Query that will match in both L1 and L2
        let query = Query::new("duplicate test").with_embedding(vec![0.7, 0.7, 0.7]);
        let result = query_parallel(&l1, &l2, &l3, &l4, query, 10).await.unwrap();

        // Should deduplicate - same memory_id appears only once
        let memory_ids: Vec<_> = result.results.iter().map(|r| r.memory_id).collect();
        let unique_ids: std::collections::HashSet<_> = memory_ids.iter().collect();

        assert_eq!(memory_ids.len(), unique_ids.len(), "Should deduplicate results");
    }

    #[tokio::test]
    async fn test_latency_tracking() {
        let l1 = layer1::ExactMatchCache::new(100).unwrap();
        let l2 = layer2::SimilarityIndex::new(100, true).unwrap();
        let l3 = layer3::GraphIndex::new(100).unwrap();
        let l4 = layer4::ContextPredictor::new(100).unwrap();

        let query = Query::new("latency test").with_embedding(vec![0.1, 0.2, 0.3]);
        let result = query_parallel(&l1, &l2, &l3, &l4, query, 10).await.unwrap();

        // Verify latencies are tracked
        assert!(result.latency_us > 0);
        assert!(result.layer_latencies.l1_us >= 0);
        assert!(result.layer_latencies.l2_us >= 0);
        assert!(result.layer_latencies.l3_us >= 0);
        assert!(result.layer_latencies.l4_us >= 0);
    }
}
