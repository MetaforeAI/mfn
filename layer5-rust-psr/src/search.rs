//! Pattern search engine for Layer 5 PSR
//!
//! Implements cosine similarity search over pattern embeddings.
//! Uses linear scan initially (HNSW integration pending).

use crate::pattern::Pattern;
use crate::storage::PatternStorage;
use anyhow::Result;
use std::sync::Arc;
use parking_lot::RwLock;

/// Search engine for pattern similarity queries
pub struct SearchEngine {
    storage: Arc<RwLock<PatternStorage>>,
}

impl SearchEngine {
    /// Create new search engine
    pub fn new(storage: Arc<RwLock<PatternStorage>>) -> Self {
        Self { storage }
    }

    /// Search for similar patterns using cosine similarity
    ///
    /// Returns: Vec<(pattern_id, similarity_score, pattern)>
    pub fn search(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        min_confidence: f32,
    ) -> Result<Vec<(String, f32, Pattern)>> {
        let storage = self.storage.read();

        // Normalize query embedding
        let query_norm = l2_norm(query_embedding);
        let query_normalized: Vec<f32> = query_embedding.iter()
            .map(|x| x / query_norm)
            .collect();

        // Compute cosine similarity for all patterns
        let mut results: Vec<(String, f32, Pattern)> = Vec::new();

        for pattern in storage.pattern_ids().iter() {
            if let Some(p) = storage.get(pattern) {
                let similarity = cosine_similarity(&query_normalized, &p.embedding);
                if similarity < min_confidence {
                    continue;
                }

                results.push((p.id.clone(), similarity, p.clone()));
            }
        }

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Take top-k
        Ok(results.into_iter().take(top_k).collect())
    }

    /// Index a pattern (no-op for linear scan, placeholder for HNSW)
    pub fn index_pattern(&self, _pattern_id: &str) -> Result<()> {
        // Linear scan doesn't require indexing
        // When HNSW is integrated, this will build the graph index
        Ok(())
    }

    /// Remove pattern from index (no-op for linear scan, placeholder for HNSW)
    pub fn remove_from_index(&self, _pattern_id: &str) -> Result<()> {
        // Linear scan doesn't require index removal
        // When HNSW is integrated, this will remove from graph
        Ok(())
    }
}

/// Compute L2 norm of a vector
fn l2_norm(vec: &[f32]) -> f32 {
    vec.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Compute cosine similarity between two vectors
///
/// Assumes vectors are already normalized (for efficiency)
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    // Normalize b
    let b_norm = l2_norm(b);
    let b_normalized: Vec<f32> = b.iter().map(|x| x / b_norm).collect();

    // Dot product (both vectors are normalized)
    a.iter().zip(b_normalized.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{Pattern, PatternCategory};

    fn create_test_pattern(id: &str, name: &str, embedding: Vec<f32>, confidence: f32) -> Pattern {
        let mut pattern = Pattern::new(
            id.to_string(),
            name.to_string(),
            PatternCategory::Transformational,
            embedding,
        );
        pattern.confidence = confidence;
        pattern
    }

    #[test]
    fn test_search_exact_match() {
        let storage = Arc::new(RwLock::new(PatternStorage::new()));
        let engine = SearchEngine::new(storage.clone());

        // Create patterns with different embeddings
        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.0, 1.0, 0.0];

        storage.write().store(create_test_pattern("p1", "Pattern 1", embedding1.clone(), 1.0)).unwrap();
        storage.write().store(create_test_pattern("p2", "Pattern 2", embedding2, 1.0)).unwrap();

        // Query with embedding1 should match p1 best
        let results = engine.search(&embedding1, 2, 0.0).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "p1"); // Best match
        assert!(results[0].1 > 0.99); // Nearly perfect similarity
    }

    #[test]
    fn test_search_similarity_filter() {
        let storage = Arc::new(RwLock::new(PatternStorage::new()));
        let engine = SearchEngine::new(storage.clone());

        let query = vec![1.0, 0.0, 0.0];

        // p1: identical to query (similarity ~1.0)
        storage.write().store(create_test_pattern("p1", "Pattern 1", vec![1.0, 0.0, 0.0], 0.5)).unwrap();
        // p2: orthogonal to query (similarity ~0.0)
        storage.write().store(create_test_pattern("p2", "Pattern 2", vec![0.0, 1.0, 0.0], 0.9)).unwrap();
        // p3: partially similar to query (similarity ~0.7)
        storage.write().store(create_test_pattern("p3", "Pattern 3", vec![1.0, 1.0, 0.0], 0.9)).unwrap();

        // Filter out patterns with similarity < 0.6
        let results = engine.search(&query, 10, 0.6).unwrap();

        assert_eq!(results.len(), 2); // Only p1 (~1.0) and p3 (~0.7) pass similarity threshold
        assert!(results.iter().all(|r| r.1 >= 0.6)); // Check similarity score, not confidence
    }

    #[test]
    fn test_search_top_k_limit() {
        let storage = Arc::new(RwLock::new(PatternStorage::new()));
        let engine = SearchEngine::new(storage.clone());

        let embedding = vec![1.0, 0.0, 0.0];

        for i in 0..10 {
            storage.write().store(create_test_pattern(
                &format!("p{}", i),
                &format!("Pattern {}", i),
                embedding.clone(),
                1.0,
            )).unwrap();
        }

        let results = engine.search(&embedding, 3, 0.0).unwrap();
        assert_eq!(results.len(), 3); // Limited to top 3
    }

    #[test]
    fn test_search_empty_storage() {
        let storage = Arc::new(RwLock::new(PatternStorage::new()));
        let engine = SearchEngine::new(storage);

        let results = engine.search(&[1.0, 0.0, 0.0], 5, 0.0).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_l2_norm() {
        assert!((l2_norm(&[3.0, 4.0]) - 5.0).abs() < 0.001);
        assert!((l2_norm(&[1.0, 0.0, 0.0]) - 1.0).abs() < 0.001);
        assert!((l2_norm(&[1.0, 1.0, 1.0]) - 1.732).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity() {
        // Same direction
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![2.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        // Orthogonal
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);

        // Opposite direction
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) + 1.0).abs() < 0.001);
    }

    #[test]
    fn test_index_and_remove() {
        let storage = Arc::new(RwLock::new(PatternStorage::new()));
        let engine = SearchEngine::new(storage);

        // These are no-ops for linear scan but should not error
        assert!(engine.index_pattern("p1").is_ok());
        assert!(engine.remove_from_index("p1").is_ok());
    }
}
