//! Shared types across all MFN layers

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Universal memory ID
pub type MemoryId = Uuid;

/// Query input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub id: Uuid,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl Query {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            embedding: None,
            metadata: Default::default(),
        }
    }

    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Memory item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: MemoryId,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: std::collections::HashMap<String, String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Memory {
    pub fn new(content: String, embedding: Vec<f32>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content,
            embedding,
            metadata: Default::default(),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub query_id: Uuid,
    pub results: Vec<SearchResult>,
    pub latency_us: u64,
    pub layer_latencies: LayerLatencies,
}

/// Individual search result from a layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub memory_id: MemoryId,
    pub score: f64,
    pub layer: Layer,
    pub content: String,
}

/// Layer identification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Layer {
    L1ExactMatch,
    L2Similarity,
    L3Graph,
    L4Context,
}

/// Layer-specific latencies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerLatencies {
    pub l1_us: u64,
    pub l2_us: u64,
    pub l3_us: u64,
    pub l4_us: u64,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub l1_size: usize,
    pub l2_size: usize,
    pub l3_nodes: usize,
    pub l4_sequences: usize,
}
