//! Layer 2: SIMD-Accelerated Dynamic Similarity Reservoir
//!
//! High-performance similarity search using:
//! - SIMD-accelerated cosine similarity (4-8x speedup)
//! - Dense memory storage for cache efficiency
//! - Lock-free concurrent caching (DashMap, Phase 2 optimization)
//! - Target: 10-100µs search time (vs 7-10ms baseline)

use crate::types::{Memory, Query, SearchResult, Layer};
use anyhow::{Result, anyhow};
use dashmap::DashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

/// SIMD width - process this many f32s at once
/// Using 8 for AVX2 (8x f32), falls back to 4 for SSE (4x f32)
#[allow(dead_code)]
#[cfg(target_arch = "x86_64")]
const SIMD_WIDTH: usize = 8;

#[allow(dead_code)]
#[cfg(not(target_arch = "x86_64"))]
const SIMD_WIDTH: usize = 4;

/// Layer 2: Dynamic Similarity Reservoir with SIMD acceleration
pub struct SimilarityIndex {
    /// Dense memory storage
    memories: Vec<Memory>,

    /// Separate embedding array for SIMD-friendly access
    /// Stored as flattened array: [emb1_dim0, emb1_dim1, ..., emb2_dim0, emb2_dim1, ...]
    embeddings: Vec<f32>,

    /// Embedding dimension
    embedding_dim: usize,

    /// Lock-free concurrent cache: query_hash -> (results, timestamp)
    /// DashMap provides concurrent access without global locks (Phase 2 optimization)
    /// 10,000 entry capacity with approximate LRU eviction via timestamp
    cache: Arc<DashMap<u64, (Vec<SearchResult>, Instant)>>,

    /// Maximum cache size for eviction
    cache_max_size: usize,

    /// Feature flag: use SIMD acceleration
    use_simd: bool,

    /// Performance metrics
    total_queries: std::sync::atomic::AtomicU64,
    cache_hits: std::sync::atomic::AtomicU64,
    cache_misses: std::sync::atomic::AtomicU64,
}

impl Clone for SimilarityIndex {
    fn clone(&self) -> Self {
        Self {
            memories: self.memories.clone(),
            embeddings: self.embeddings.clone(),
            embedding_dim: self.embedding_dim,
            cache: Arc::clone(&self.cache),
            cache_max_size: self.cache_max_size,
            use_simd: self.use_simd,
            total_queries: std::sync::atomic::AtomicU64::new(
                self.total_queries.load(std::sync::atomic::Ordering::Relaxed)
            ),
            cache_hits: std::sync::atomic::AtomicU64::new(
                self.cache_hits.load(std::sync::atomic::Ordering::Relaxed)
            ),
            cache_misses: std::sync::atomic::AtomicU64::new(
                self.cache_misses.load(std::sync::atomic::Ordering::Relaxed)
            ),
        }
    }
}

impl SimilarityIndex {
    /// Create new similarity index
    ///
    /// # Arguments
    /// * `capacity` - Initial capacity for memories
    /// * `use_simd` - Enable SIMD acceleration (recommended)
    pub fn new(capacity: usize, use_simd: bool) -> Result<Self> {
        let cache_max_size = 10_000;
        let cache = Arc::new(DashMap::with_capacity(cache_max_size));

        Ok(Self {
            memories: Vec::with_capacity(capacity),
            embeddings: Vec::with_capacity(capacity * 384), // Assume 384D embeddings
            embedding_dim: 0, // Will be set on first add
            cache,
            cache_max_size,
            use_simd,
            total_queries: std::sync::atomic::AtomicU64::new(0),
            cache_hits: std::sync::atomic::AtomicU64::new(0),
            cache_misses: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Add memory to the index
    ///
    /// CRITICAL: Invalidates cache when adding new memories
    pub fn add(&mut self, memory: Memory) -> Result<()> {
        // Set embedding dimension on first add
        if self.embedding_dim == 0 {
            self.embedding_dim = memory.embedding.len();
        } else if memory.embedding.len() != self.embedding_dim {
            return Err(anyhow!(
                "Embedding dimension mismatch: expected {}, got {}",
                self.embedding_dim,
                memory.embedding.len()
            ));
        }

        // Append embedding to flattened array
        self.embeddings.extend_from_slice(&memory.embedding);

        // Store memory
        self.memories.push(memory);

        // CRITICAL: Invalidate cache when adding new memories
        // New memories change similarity landscape, making cached results stale
        self.cache.clear();

        Ok(())
    }

    /// Get number of memories in index
    pub fn len(&self) -> usize {
        self.memories.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.memories.is_empty()
    }

    /// Similarity search with lock-free caching
    ///
    /// Returns top-k most similar memories based on cosine similarity
    pub fn search(&self, query: &Query, top_k: usize) -> Result<Vec<SearchResult>> {
        let start = Instant::now();

        // Extract query embedding
        let query_embedding = query.embedding
            .as_ref()
            .ok_or_else(|| anyhow!("Query missing embedding"))?;

        if query_embedding.len() != self.embedding_dim {
            return Err(anyhow!(
                "Query embedding dimension mismatch: expected {}, got {}",
                self.embedding_dim,
                query_embedding.len()
            ));
        }

        // Compute query hash for cache key
        let query_hash = Self::compute_query_hash(query_embedding);

        // Check cache first (lock-free read)
        if let Some(entry) = self.cache.get(&query_hash) {
            // Clone results while holding read lock
            let cached_results = entry.value().0.clone();
            drop(entry); // Release read lock

            // Cache hit - update timestamp for approximate LRU (after releasing read lock)
            self.cache.insert(query_hash, (cached_results.clone(), Instant::now()));

            self.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.total_queries.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            let duration = start.elapsed();
            tracing::debug!(
                duration_us = duration.as_micros(),
                cache_hit = true,
                results_count = cached_results.len(),
                "Layer 2 search completed (cache hit)"
            );

            // Return truncated results if needed
            let mut results = cached_results;
            results.truncate(top_k);
            return Ok(results);
        }

        // Cache miss: perform similarity search
        self.cache_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Compute similarities for all memories
        let mut similarities = Vec::with_capacity(self.memories.len());

        for i in 0..self.memories.len() {
            let embedding_offset = i * self.embedding_dim;
            let memory_embedding = &self.embeddings[embedding_offset..embedding_offset + self.embedding_dim];

            let similarity = if self.use_simd {
                simd_cosine_similarity(query_embedding, memory_embedding)
            } else {
                scalar_cosine_similarity(query_embedding, memory_embedding)
            };

            similarities.push((i, similarity));
        }

        // Sort by similarity (descending)
        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k and convert to SearchResult
        let results: Vec<SearchResult> = similarities
            .into_iter()
            .take(top_k)
            .filter(|(_, score)| *score > 0.0) // Filter out zero/negative scores
            .map(|(idx, score)| {
                let memory = &self.memories[idx];
                SearchResult {
                    memory_id: memory.id,
                    score: score as f64,
                    layer: Layer::L2Similarity,
                    content: memory.content.clone(),
                }
            })
            .collect();

        // Cache the results with eviction if needed
        if self.cache.len() >= self.cache_max_size {
            // Simple eviction: remove oldest entry (approximate LRU)
            if let Some(oldest) = self.cache.iter()
                .min_by_key(|entry| entry.value().1)
                .map(|e| *e.key())
            {
                self.cache.remove(&oldest);
            }
        }

        self.cache.insert(query_hash, (results.clone(), Instant::now()));

        self.total_queries.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let duration = start.elapsed();
        tracing::debug!(
            duration_us = duration.as_micros(),
            cache_hit = false,
            results_count = results.len(),
            use_simd = self.use_simd,
            "Layer 2 search completed (cache miss)"
        );

        Ok(results)
    }

    /// Compute hash of query embedding for cache key
    fn compute_query_hash(embedding: &[f32]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for &value in embedding {
            value.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Get performance statistics
    pub fn get_stats(&self) -> Layer2Stats {
        let total_queries = self.total_queries.load(std::sync::atomic::Ordering::Relaxed);
        let cache_hits = self.cache_hits.load(std::sync::atomic::Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(std::sync::atomic::Ordering::Relaxed);

        let cache_hit_rate = if total_queries > 0 {
            (cache_hits as f64) / (total_queries as f64)
        } else {
            0.0
        };

        let cache_size = self.cache.len();

        Layer2Stats {
            total_queries,
            cache_hits,
            cache_misses,
            cache_hit_rate,
            cache_size,
            memory_count: self.memories.len(),
            use_simd: self.use_simd,
            embedding_dim: self.embedding_dim,
        }
    }
}

/// Layer 2 performance statistics
#[derive(Debug, Clone)]
pub struct Layer2Stats {
    pub total_queries: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub cache_size: usize,
    pub memory_count: usize,
    pub use_simd: bool,
    pub embedding_dim: usize,
}

/// SIMD-accelerated cosine similarity
///
/// Computes cosine similarity between two vectors using SIMD instructions.
/// Processes 4-8 floats at a time for significant speedup.
///
/// Formula: cosine_similarity(a, b) = dot(a, b) / (norm(a) * norm(b))
#[inline]
fn simd_cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same length");

    // Use SIMD on x86_64 with AVX2 support
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    {
        unsafe { simd_cosine_similarity_avx2(a, b) }
    }

    // Use SIMD on x86_64 with SSE support (fallback)
    #[cfg(all(target_arch = "x86_64", not(target_feature = "avx2"), target_feature = "sse"))]
    {
        unsafe { simd_cosine_similarity_sse(a, b) }
    }

    // Portable SIMD fallback (chunked scalar)
    #[cfg(not(all(target_arch = "x86_64", any(target_feature = "avx2", target_feature = "sse"))))]
    {
        simd_cosine_similarity_portable(a, b)
    }
}

/// AVX2 implementation (8x f32 per instruction)
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
unsafe fn simd_cosine_similarity_avx2(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;

    let len = a.len();
    let chunks = len / 8;
    let remainder = len % 8;

    // Accumulators
    let mut dot_acc = _mm256_setzero_ps();
    let mut norm_a_acc = _mm256_setzero_ps();
    let mut norm_b_acc = _mm256_setzero_ps();

    // Process 8 floats at a time
    for i in 0..chunks {
        let offset = i * 8;

        let va = _mm256_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm256_loadu_ps(b.as_ptr().add(offset));

        // Dot product: a * b
        dot_acc = _mm256_fmadd_ps(va, vb, dot_acc);

        // Norm a: a * a
        norm_a_acc = _mm256_fmadd_ps(va, va, norm_a_acc);

        // Norm b: b * b
        norm_b_acc = _mm256_fmadd_ps(vb, vb, norm_b_acc);
    }

    // Horizontal sum of accumulators
    let dot_product = horizontal_sum_avx2(dot_acc);
    let norm_a = horizontal_sum_avx2(norm_a_acc);
    let norm_b = horizontal_sum_avx2(norm_b_acc);

    // Process remainder with scalar
    let mut dot_rem = 0.0f32;
    let mut norm_a_rem = 0.0f32;
    let mut norm_b_rem = 0.0f32;

    for i in (chunks * 8)..len {
        dot_rem += a[i] * b[i];
        norm_a_rem += a[i] * a[i];
        norm_b_rem += b[i] * b[i];
    }

    let final_dot = dot_product + dot_rem;
    let final_norm_a = (norm_a + norm_a_rem).sqrt();
    let final_norm_b = (norm_b + norm_b_rem).sqrt();

    // Avoid division by zero
    if final_norm_a == 0.0 || final_norm_b == 0.0 {
        return 0.0;
    }

    final_dot / (final_norm_a * final_norm_b)
}

/// Horizontal sum for AVX2 (8x f32)
#[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
#[inline]
unsafe fn horizontal_sum_avx2(v: std::arch::x86_64::__m256) -> f32 {
    use std::arch::x86_64::*;

    // Sum high and low 128-bit lanes
    let sum_128 = _mm_add_ps(_mm256_castps256_ps128(v), _mm256_extractf128_ps(v, 1));

    // Horizontal add within 128-bit lane
    let sum_64 = _mm_add_ps(sum_128, _mm_movehl_ps(sum_128, sum_128));
    let sum_32 = _mm_add_ss(sum_64, _mm_shuffle_ps(sum_64, sum_64, 1));

    _mm_cvtss_f32(sum_32)
}

/// SSE implementation (4x f32 per instruction)
#[cfg(all(target_arch = "x86_64", target_feature = "sse"))]
#[inline]
unsafe fn simd_cosine_similarity_sse(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;

    let len = a.len();
    let chunks = len / 4;

    // Accumulators
    let mut dot_acc = _mm_setzero_ps();
    let mut norm_a_acc = _mm_setzero_ps();
    let mut norm_b_acc = _mm_setzero_ps();

    // Process 4 floats at a time
    for i in 0..chunks {
        let offset = i * 4;

        let va = _mm_loadu_ps(a.as_ptr().add(offset));
        let vb = _mm_loadu_ps(b.as_ptr().add(offset));

        // Dot product: a * b
        dot_acc = _mm_add_ps(dot_acc, _mm_mul_ps(va, vb));

        // Norm a: a * a
        norm_a_acc = _mm_add_ps(norm_a_acc, _mm_mul_ps(va, va));

        // Norm b: b * b
        norm_b_acc = _mm_add_ps(norm_b_acc, _mm_mul_ps(vb, vb));
    }

    // Horizontal sum of accumulators
    let dot_product = horizontal_sum_sse(dot_acc);
    let norm_a = horizontal_sum_sse(norm_a_acc);
    let norm_b = horizontal_sum_sse(norm_b_acc);

    // Process remainder with scalar
    let mut dot_rem = 0.0f32;
    let mut norm_a_rem = 0.0f32;
    let mut norm_b_rem = 0.0f32;

    for i in (chunks * 4)..len {
        dot_rem += a[i] * b[i];
        norm_a_rem += a[i] * a[i];
        norm_b_rem += b[i] * b[i];
    }

    let final_dot = dot_product + dot_rem;
    let final_norm_a = (norm_a + norm_a_rem).sqrt();
    let final_norm_b = (norm_b + norm_b_rem).sqrt();

    // Avoid division by zero
    if final_norm_a == 0.0 || final_norm_b == 0.0 {
        return 0.0;
    }

    final_dot / (final_norm_a * final_norm_b)
}

/// Horizontal sum for SSE (4x f32)
#[cfg(all(target_arch = "x86_64", target_feature = "sse"))]
#[inline]
unsafe fn horizontal_sum_sse(v: std::arch::x86_64::__m128) -> f32 {
    use std::arch::x86_64::*;

    let shuf = _mm_movehdup_ps(v);
    let sums = _mm_add_ps(v, shuf);
    let shuf = _mm_movehl_ps(shuf, sums);
    let sums = _mm_add_ss(sums, shuf);

    _mm_cvtss_f32(sums)
}

/// Portable SIMD fallback (chunked scalar processing)
#[allow(dead_code)]
#[inline]
fn simd_cosine_similarity_portable(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len();
    let chunks = len / 4;

    // Process 4 elements at a time (manual loop unrolling)
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..chunks {
        let offset = i * 4;

        // Unrolled loop for better instruction-level parallelism
        let a0 = a[offset];
        let a1 = a[offset + 1];
        let a2 = a[offset + 2];
        let a3 = a[offset + 3];

        let b0 = b[offset];
        let b1 = b[offset + 1];
        let b2 = b[offset + 2];
        let b3 = b[offset + 3];

        dot += a0 * b0 + a1 * b1 + a2 * b2 + a3 * b3;
        norm_a += a0 * a0 + a1 * a1 + a2 * a2 + a3 * a3;
        norm_b += b0 * b0 + b1 * b1 + b2 * b2 + b3 * b3;
    }

    // Process remainder
    for i in (chunks * 4)..len {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let norm_a = norm_a.sqrt();
    let norm_b = norm_b.sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Scalar cosine similarity (no SIMD)
///
/// Used as baseline for benchmarking and fallback when SIMD disabled
#[inline]
fn scalar_cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same length");

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let norm_a = norm_a.sqrt();
    let norm_b = norm_b.sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Memory, Query};

    fn create_test_memory(id_seed: u64, content: &str, embedding: Vec<f32>) -> Memory {
        Memory {
            id: uuid::Uuid::from_u128(id_seed as u128),
            content: content.to_string(),
            embedding,
            metadata: Default::default(),
            timestamp: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_simd_vs_scalar_correctness() {
        // Test that SIMD and scalar produce same results
        let a = vec![0.5, 0.8, 0.3, 0.9, 0.1, 0.7, 0.4, 0.6];
        let b = vec![0.4, 0.7, 0.5, 0.8, 0.2, 0.6, 0.3, 0.7];

        let simd_result = simd_cosine_similarity(&a, &b);
        let scalar_result = scalar_cosine_similarity(&a, &b);

        println!("SIMD result: {:.6}", simd_result);
        println!("Scalar result: {:.6}", scalar_result);

        // Allow small floating point error
        assert!((simd_result - scalar_result).abs() < 1e-5,
            "SIMD and scalar results differ: simd={}, scalar={}", simd_result, scalar_result);
    }

    #[test]
    fn test_cosine_similarity_known_values() {
        // Identical vectors should have similarity 1.0
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![1.0, 2.0, 3.0, 4.0];

        let sim = simd_cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-5, "Identical vectors should have similarity 1.0, got {}", sim);

        // Orthogonal vectors should have similarity 0.0
        let a = vec![1.0, 0.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0, 0.0];

        let sim = simd_cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5, "Orthogonal vectors should have similarity 0.0, got {}", sim);

        // Opposite vectors should have similarity -1.0
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![-1.0, -2.0, -3.0, -4.0];

        let sim = simd_cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-5, "Opposite vectors should have similarity -1.0, got {}", sim);
    }

    #[test]
    fn test_similarity_index_basic() {
        let mut index = SimilarityIndex::new(100, true).unwrap();

        // Add memories
        let mem1 = create_test_memory(1, "test memory 1", vec![0.8, 0.9, 0.7, 0.6]);
        let mem2 = create_test_memory(2, "test memory 2", vec![0.7, 0.8, 0.8, 0.7]);
        let mem3 = create_test_memory(3, "different memory", vec![0.1, 0.2, 0.3, 0.4]);

        index.add(mem1).unwrap();
        index.add(mem2).unwrap();
        index.add(mem3).unwrap();

        assert_eq!(index.len(), 3);

        // Search with query similar to mem1
        let query = Query::new("test query")
            .with_embedding(vec![0.75, 0.85, 0.75, 0.65]);

        let results = index.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].score > results[1].score, "Results should be sorted by score");
        println!("Top result: score={:.3}, content={}", results[0].score, results[0].content);
    }

    #[test]
    fn test_cache_hit() {
        let mut index = SimilarityIndex::new(100, true).unwrap();

        // Add memory
        let mem = create_test_memory(1, "test", vec![0.5, 0.5, 0.5, 0.5]);
        index.add(mem).unwrap();

        // First query (cache miss)
        let query = Query::new("test").with_embedding(vec![0.6, 0.6, 0.6, 0.6]);
        let results1 = index.search(&query, 1).unwrap();

        let stats1 = index.get_stats();
        assert_eq!(stats1.cache_misses, 1);
        assert_eq!(stats1.cache_hits, 0);

        // Second query with same embedding (cache hit)
        let results2 = index.search(&query, 1).unwrap();

        let stats2 = index.get_stats();
        assert_eq!(stats2.cache_misses, 1);
        assert_eq!(stats2.cache_hits, 1);

        // Results should be identical
        assert_eq!(results1.len(), results2.len());
        assert_eq!(results1[0].memory_id, results2[0].memory_id);
    }

    #[test]
    fn test_cache_invalidation() {
        let mut index = SimilarityIndex::new(100, true).unwrap();

        // Add memory and query
        let mem1 = create_test_memory(1, "test", vec![0.5, 0.5, 0.5, 0.5]);
        index.add(mem1).unwrap();

        let query = Query::new("test").with_embedding(vec![0.6, 0.6, 0.6, 0.6]);
        let _results1 = index.search(&query, 1).unwrap();

        // Add another memory (should invalidate cache)
        let mem2 = create_test_memory(2, "test2", vec![0.7, 0.7, 0.7, 0.7]);
        index.add(mem2).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.cache_size, 0, "Cache should be cleared after adding memory");
    }

    #[test]
    fn test_large_embedding_performance() {
        let mut index = SimilarityIndex::new(1000, true).unwrap();

        // Create 384D embeddings (standard sentence transformer size)
        let embedding_dim = 384;

        // Add 100 memories
        for i in 0..100 {
            let embedding: Vec<f32> = (0..embedding_dim)
                .map(|j| ((i + j) as f32) / 1000.0)
                .collect();
            let mem = create_test_memory(i, &format!("memory {}", i), embedding);
            index.add(mem).unwrap();
        }

        // Query
        let query_embedding: Vec<f32> = (0..embedding_dim)
            .map(|i| (i as f32) / 1000.0)
            .collect();
        let query = Query::new("test").with_embedding(query_embedding);

        // Measure search time
        let start = std::time::Instant::now();
        let results = index.search(&query, 10).unwrap();
        let duration = start.elapsed();

        println!("Search time for 100 memories (384D): {:?}", duration);
        println!("Results: {}", results.len());

        // Should be fast (<1ms for 100 memories)
        assert!(duration.as_millis() < 10, "Search too slow: {:?}", duration);
        assert!(!results.is_empty(), "Should find results");
    }
}

#[cfg(all(test, not(target_env = "msvc")))]
mod benches {
    use super::*;
    use std::time::Instant;

    /// Benchmark SIMD vs scalar cosine similarity
    #[test]
    #[ignore] // Run with: cargo test --release -- --ignored --nocapture
    fn bench_simd_vs_scalar() {
        let dims = [128, 384, 768, 1536];
        let iterations = 10_000;

        println!("\n=== SIMD vs Scalar Cosine Similarity Benchmark ===\n");

        for &dim in &dims {
            let a: Vec<f32> = (0..dim).map(|i| (i as f32) / (dim as f32)).collect();
            let b: Vec<f32> = (0..dim).map(|i| ((i + 1) as f32) / (dim as f32)).collect();

            // Benchmark SIMD
            let start = Instant::now();
            let mut simd_sum = 0.0f32;
            for _ in 0..iterations {
                simd_sum += simd_cosine_similarity(&a, &b);
            }
            let simd_duration = start.elapsed();

            // Benchmark scalar
            let start = Instant::now();
            let mut scalar_sum = 0.0f32;
            for _ in 0..iterations {
                scalar_sum += scalar_cosine_similarity(&a, &b);
            }
            let scalar_duration = start.elapsed();

            // Use the sums to prevent optimization
            std::hint::black_box(simd_sum);
            std::hint::black_box(scalar_sum);

            let simd_ns = simd_duration.as_nanos() / iterations;
            let scalar_ns = scalar_duration.as_nanos() / iterations;
            let speedup = (scalar_ns as f64) / (simd_ns as f64);

            println!("Dimension: {}", dim);
            println!("  SIMD:   {:>6} ns/op", simd_ns);
            println!("  Scalar: {:>6} ns/op", scalar_ns);
            println!("  Speedup: {:.2}x\n", speedup);
        }
    }

    /// Benchmark full similarity search
    #[test]
    #[ignore] // Run with: cargo test --release -- --ignored --nocapture
    fn bench_similarity_search() {
        let embedding_dim = 384;
        let memory_counts = [100, 1_000, 10_000];

        println!("\n=== Similarity Search Benchmark ===\n");

        for &count in &memory_counts {
            // Create index with SIMD
            let mut index_simd = SimilarityIndex::new(count, true).unwrap();

            // Add memories
            for i in 0..count {
                let embedding: Vec<f32> = (0..embedding_dim)
                    .map(|j| ((i + j) as f32) / 1000.0)
                    .collect();
                let mem = Memory {
                    id: uuid::Uuid::from_u128(i as u128),
                    content: format!("memory {}", i),
                    embedding,
                    metadata: Default::default(),
                    timestamp: chrono::Utc::now(),
                };
                index_simd.add(mem).unwrap();
            }

            // Create query
            let query_embedding: Vec<f32> = (0..embedding_dim)
                .map(|i| (i as f32) / 1000.0)
                .collect();
            let query = Query::new("test").with_embedding(query_embedding);

            // Benchmark search (cold cache)
            let iterations = 100;
            let start = Instant::now();
            for _ in 0..iterations {
                // Clear cache each time to measure cold performance
                index_simd.cache.clear();
                let _ = index_simd.search(&query, 10).unwrap();
            }
            let duration = start.elapsed();
            let avg_us = duration.as_micros() / iterations;

            println!("Memory count: {}", count);
            println!("  Avg search time: {} µs", avg_us);
            println!("  Throughput: {:.0} queries/sec\n", 1_000_000.0 / (avg_us as f64));
        }
    }
}
