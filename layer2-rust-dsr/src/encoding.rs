//! Embedding encoding for DSR Layer 2.
//!
//! Replaces the previous spike encoding pipeline with direct embedding storage.
//! Embeddings are stored as-is with a precomputed L2 norm for fast cosine similarity.

use anyhow::Result;
use ndarray::ArrayView1;
use std::sync::Arc;

/// Strategy for encoding embeddings (kept for config backward compatibility).
/// All strategies now use pass-through encoding.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EncodingStrategy {
    RateCoding,
    TemporalCoding,
    PopulationCoding,
    DeltaModulation,
    RankOrderCoding,
}

impl Default for EncodingStrategy {
    fn default() -> Self {
        Self::RateCoding
    }
}

/// Stored embedding with precomputed L2 norm for fast cosine similarity.
#[derive(Debug, Clone)]
pub struct EmbeddingPattern {
    /// Raw embedding vector stored directly
    pub embedding: Vec<f32>,
    /// Precomputed L2 norm: sqrt(sum(x_i^2))
    pub l2_norm: f32,
    /// Number of dimensions
    pub neuron_count: usize,
    /// Encoding duration (kept for trait compatibility)
    pub duration_ms: f32,
}

/// Backward compatibility alias
pub type SpikePattern = EmbeddingPattern;

impl EmbeddingPattern {
    pub fn from_embedding(embedding: Vec<f32>) -> Self {
        let l2_norm = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        let neuron_count = embedding.len();
        Self {
            embedding,
            l2_norm,
            neuron_count,
            duration_ms: 0.0,
        }
    }

    pub fn zeros(dim: usize) -> Self {
        Self {
            embedding: vec![0.0; dim],
            l2_norm: 0.0,
            neuron_count: dim,
            duration_ms: 0.0,
        }
    }
}

/// Trait for embedding encoders (kept for API compatibility).
pub trait SpikeEncoder: Send + Sync {
    fn encode(&self, embedding: ArrayView1<f32>) -> Result<SpikePattern>;
    fn neuron_count(&self) -> usize;
    fn encoding_duration_ms(&self) -> f32;
}

/// Pass-through encoder: stores embedding directly with precomputed norm.
pub struct PassThroughEncoder {
    embedding_dim: usize,
}

impl PassThroughEncoder {
    pub fn new(embedding_dim: usize) -> Self {
        Self { embedding_dim }
    }
}

impl SpikeEncoder for PassThroughEncoder {
    fn encode(&self, embedding: ArrayView1<f32>) -> Result<SpikePattern> {
        let vec: Vec<f32> = embedding.to_vec();
        Ok(EmbeddingPattern::from_embedding(vec))
    }

    fn neuron_count(&self) -> usize {
        self.embedding_dim
    }

    fn encoding_duration_ms(&self) -> f32 {
        0.0
    }
}

/// Create an encoder. All strategies return PassThroughEncoder.
pub fn create_encoder(
    _strategy: EncodingStrategy,
    embedding_dim: usize,
) -> Arc<dyn SpikeEncoder> {
    Arc::new(PassThroughEncoder::new(embedding_dim))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array1;

    #[test]
    fn test_embedding_pattern_norm() {
        let emb = vec![3.0, 4.0];
        let pattern = EmbeddingPattern::from_embedding(emb);
        assert!((pattern.l2_norm - 5.0).abs() < 1e-6);
    }

    #[test]
    fn test_pass_through_encoder() {
        let encoder = PassThroughEncoder::new(4);
        let arr = Array1::from_vec(vec![1.0, 2.0, 3.0, 4.0]);
        let pattern = encoder.encode(arr.view()).unwrap();
        assert_eq!(pattern.embedding, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(pattern.neuron_count, 4);
    }

    #[test]
    fn test_create_encoder_all_strategies() {
        for strategy in [
            EncodingStrategy::RateCoding,
            EncodingStrategy::TemporalCoding,
            EncodingStrategy::PopulationCoding,
        ] {
            let enc = create_encoder(strategy, 128);
            assert_eq!(enc.neuron_count(), 128);
        }
    }
}
