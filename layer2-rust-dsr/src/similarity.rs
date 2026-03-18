//! SIMD-accelerated cosine similarity for DSR Layer 2.
//!
//! Replaces spike-based matching with direct vector cosine similarity
//! using AVX2/FMA intrinsics when available, with scalar fallback.

use crate::encoding::SpikePattern;
use crate::reservoir::VectorStore;
use crate::DSRConfig;
use crate::MemoryId;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Result of a similarity search across stored memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityResults {
    pub matches: Vec<SimilarityMatch>,
    pub processing_time_ms: f32,
    pub wells_evaluated: usize,
    pub has_confident_matches: bool,
}

/// A single similarity match with confidence score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityMatch {
    pub memory_id: MemoryId,
    pub confidence: f32,
    pub raw_activation: f32,
    pub content: String,
    pub rank: usize,
}

impl SimilarityResults {
    pub fn empty() -> Self {
        Self {
            matches: Vec::new(),
            processing_time_ms: 0.0,
            wells_evaluated: 0,
            has_confident_matches: false,
        }
    }

    pub fn best_match(&self) -> Option<&SimilarityMatch> {
        self.matches.first()
    }

    pub fn filter_by_confidence(&self, min_confidence: f32) -> Vec<&SimilarityMatch> {
        self.matches
            .iter()
            .filter(|m| m.confidence >= min_confidence)
            .collect()
    }

    pub fn average_confidence(&self) -> f32 {
        if self.matches.is_empty() {
            return 0.0;
        }
        self.matches.iter().map(|m| m.confidence).sum::<f32>() / self.matches.len() as f32
    }

    pub fn top_matches(&self, max_rank: usize) -> Vec<&SimilarityMatch> {
        self.matches
            .iter()
            .filter(|m| m.rank <= max_rank)
            .collect()
    }
}

// ============================================================
// SIMD Cosine Similarity
// ============================================================

/// Compute cosine similarity using AVX2 + FMA intrinsics.
/// Processes 8 floats per cycle for maximum throughput.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2", enable = "fma")]
unsafe fn cosine_similarity_avx2(a: &[f32], b: &[f32], a_norm: f32, b_norm: f32) -> f32 {
    use std::arch::x86_64::*;

    let len = a.len().min(b.len());
    let chunks = len / 8;
    let remainder = len - chunks * 8;

    let mut dot_acc = _mm256_setzero_ps();

    for i in 0..chunks {
        let va = _mm256_loadu_ps(a.as_ptr().add(i * 8));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i * 8));
        dot_acc = _mm256_fmadd_ps(va, vb, dot_acc);
    }

    // Horizontal sum of 8 floats in the accumulator
    let hi128 = _mm256_extractf128_ps(dot_acc, 1);
    let lo128 = _mm256_castps256_ps128(dot_acc);
    let sum128 = _mm_add_ps(lo128, hi128);
    let shuf = _mm_movehdup_ps(sum128);
    let sums = _mm_add_ps(sum128, shuf);
    let shuf2 = _mm_movehl_ps(sums, sums);
    let result = _mm_add_ss(sums, shuf2);
    let mut dot = _mm_cvtss_f32(result);

    // Handle remainder elements
    let base = chunks * 8;
    for i in 0..remainder {
        dot += a[base + i] * b[base + i];
    }

    let denom = a_norm * b_norm;
    if denom > 1e-12 {
        dot / denom
    } else {
        0.0
    }
}

/// Scalar fallback for non-x86 platforms.
fn cosine_similarity_scalar(a: &[f32], b: &[f32], a_norm: f32, b_norm: f32) -> f32 {
    let len = a.len().min(b.len());
    let dot: f32 = a[..len]
        .iter()
        .zip(b[..len].iter())
        .map(|(x, y)| x * y)
        .sum();
    let denom = a_norm * b_norm;
    if denom > 1e-12 {
        dot / denom
    } else {
        0.0
    }
}

/// Runtime-dispatched cosine similarity. Uses AVX2+FMA if available.
pub fn simd_cosine_similarity(a: &[f32], b: &[f32], a_norm: f32, b_norm: f32) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { cosine_similarity_avx2(a, b, a_norm, b_norm) };
        }
    }
    cosine_similarity_scalar(a, b, a_norm, b_norm)
}

/// Compute L2 norm of a vector.
pub fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

// ============================================================
// Similarity Matcher
// ============================================================

/// Finds similar memories using SIMD cosine similarity.
pub struct SimilarityMatcher {
    config: DSRConfig,
}

impl SimilarityMatcher {
    pub fn new(config: DSRConfig) -> Self {
        Self { config }
    }

    /// Find top-k similar memories for a query pattern.
    pub async fn find_similar(
        &self,
        store: &mut VectorStore,
        query_pattern: &SpikePattern,
        top_k: usize,
    ) -> Result<SimilarityResults> {
        let start = std::time::Instant::now();

        // Get raw cosine similarities from VectorStore
        let activations = store.process_pattern(query_pattern)?;
        let wells_evaluated = activations.len();

        // Build matches
        let mut matches: Vec<SimilarityMatch> = activations
            .into_iter()
            .filter_map(|(memory_id, similarity)| {
                let entry = store.get_entry(&memory_id)?;
                // Map cosine similarity [-1, 1] to confidence [0, 1]
                let confidence = (similarity + 1.0) / 2.0;
                Some(SimilarityMatch {
                    memory_id,
                    confidence,
                    raw_activation: similarity,
                    content: entry.content.clone(),
                    rank: 0,
                })
            })
            .collect();

        // Sort by confidence descending
        matches.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(top_k);

        // Assign ranks
        for (i, m) in matches.iter_mut().enumerate() {
            m.rank = i + 1;
        }

        let processing_time_ms = start.elapsed().as_secs_f32() * 1000.0;
        let threshold = self.config.similarity_threshold;
        let has_confident_matches = matches.iter().any(|m| m.raw_activation > threshold);

        Ok(SimilarityResults {
            matches,
            processing_time_ms,
            wells_evaluated,
            has_confident_matches,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let norm = l2_norm(&a);
        let sim = simd_cosine_similarity(&a, &a, norm, norm);
        assert!(
            (sim - 1.0).abs() < 1e-5,
            "Identical vectors should have similarity 1.0, got {}",
            sim
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0, 0.0];
        let sim = simd_cosine_similarity(&a, &b, l2_norm(&a), l2_norm(&b));
        assert!(
            sim.abs() < 1e-5,
            "Orthogonal vectors should have similarity 0.0, got {}",
            sim
        );
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = simd_cosine_similarity(&a, &b, l2_norm(&a), l2_norm(&b));
        assert!(
            (sim + 1.0).abs() < 1e-5,
            "Opposite vectors should have similarity -1.0, got {}",
            sim
        );
    }

    #[test]
    fn test_cosine_similarity_large_dim() {
        // Test with 2048-dim (actual model dimension)
        let a: Vec<f32> = (0..2048).map(|i| (i as f32).sin()).collect();
        let b: Vec<f32> = (0..2048).map(|i| (i as f32).cos()).collect();
        let sim = simd_cosine_similarity(&a, &b, l2_norm(&a), l2_norm(&b));
        assert!(
            sim > -1.0 && sim < 1.0,
            "Should be a valid cosine similarity, got {}",
            sim
        );
    }

    #[test]
    fn test_zero_vector() {
        let a = vec![0.0; 100];
        let b = vec![1.0; 100];
        let sim = simd_cosine_similarity(&a, &b, l2_norm(&a), l2_norm(&b));
        assert_eq!(sim, 0.0, "Zero vector should give 0.0 similarity");
    }
}
