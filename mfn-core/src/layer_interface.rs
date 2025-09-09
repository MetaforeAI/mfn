// MFN Core - Layer Interface Definitions
// Defines the standard interfaces that all MFN layers must implement

use crate::memory_types::*;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during layer operations
#[derive(Error, Debug)]
pub enum LayerError {
    #[error("Memory not found: {id}")]
    MemoryNotFound { id: MemoryId },
    
    #[error("Association not found: {id}")]
    AssociationNotFound { id: String },
    
    #[error("Invalid operation: {message}")]
    InvalidOperation { message: String },
    
    #[error("Capacity exceeded: {message}")]
    CapacityExceeded { message: String },
    
    #[error("Timeout exceeded: {timeout_us}μs")]
    TimeoutExceeded { timeout_us: u64 },
    
    #[error("Layer communication error: {message}")]
    CommunicationError { message: String },
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Task timeout")]
    TaskTimeout(#[from] tokio::time::error::Elapsed),
}

pub type LayerResult<T> = Result<T, LayerError>;

/// Routing decision made by a layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingDecision {
    /// Found exact match, return result immediately
    FoundExact {
        results: Vec<UniversalSearchResult>,
    },
    /// Found partial match, may need additional processing
    FoundPartial {
        results: Vec<UniversalSearchResult>,
        continue_search: bool,
        suggested_layers: Vec<LayerId>,
    },
    /// No match found, route to next layer(s)
    RouteToLayers {
        suggested_layers: Vec<LayerId>,
        routing_confidence: Weight,
    },
    /// Search complete, no more layers needed
    SearchComplete {
        results: Vec<UniversalSearchResult>,
    },
}

/// Performance metrics returned by each layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerPerformance {
    pub layer_id: LayerId,
    pub processing_time_us: u64,
    pub memory_usage_bytes: u64,
    pub operations_performed: u64,
    pub cache_hit_rate: Option<f64>,
    pub custom_metrics: HashMap<String, serde_json::Value>,
}

/// Core interface that all MFN layers must implement
#[async_trait]
pub trait MfnLayer: Send + Sync {
    /// Get the unique identifier for this layer
    fn layer_id(&self) -> LayerId;
    
    /// Get a human-readable name for this layer
    fn layer_name(&self) -> &str;
    
    /// Get version information for this layer implementation
    fn version(&self) -> &str;

    /// Add a memory to this layer
    async fn add_memory(&mut self, memory: UniversalMemory) -> LayerResult<()>;
    
    /// Add an association between memories
    async fn add_association(&mut self, association: UniversalAssociation) -> LayerResult<()>;
    
    /// Retrieve a specific memory by ID
    async fn get_memory(&self, id: MemoryId) -> LayerResult<UniversalMemory>;
    
    /// Remove a memory and its associations
    async fn remove_memory(&mut self, id: MemoryId) -> LayerResult<()>;
    
    /// Perform a search and return routing decision
    async fn search(&self, query: &UniversalSearchQuery) -> LayerResult<RoutingDecision>;
    
    /// Get performance metrics for this layer
    async fn get_performance(&self) -> LayerResult<LayerPerformance>;
    
    /// Health check for this layer
    async fn health_check(&self) -> LayerResult<LayerHealth>;
    
    /// Initialize/start the layer with configuration
    async fn start(&mut self, config: LayerConfig) -> LayerResult<()>;
    
    /// Gracefully shutdown the layer
    async fn shutdown(&mut self) -> LayerResult<()>;
    
    /// Get current configuration
    fn get_config(&self) -> &LayerConfig;
}

/// Layer health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerHealth {
    pub layer_id: LayerId,
    pub status: HealthStatus,
    pub uptime_seconds: u64,
    pub last_error: Option<String>,
    pub resource_usage: ResourceUsage,
    pub diagnostics: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Starting,
    Stopping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub memory_bytes: u64,
    pub cpu_percent: f64,
    pub active_connections: u32,
    pub pending_operations: u32,
}

/// Configuration for a layer instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    pub layer_id: LayerId,
    pub max_memory_count: Option<usize>,
    pub max_association_count: Option<usize>,
    pub default_timeout_us: u64,
    pub enable_caching: bool,
    pub cache_size_limit: Option<usize>,
    pub performance_monitoring: bool,
    pub custom_params: HashMap<String, serde_json::Value>,
}

impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            layer_id: LayerId::Layer1,
            max_memory_count: Some(1_000_000),
            max_association_count: Some(10_000_000),
            default_timeout_us: 20_000, // 20ms
            enable_caching: true,
            cache_size_limit: Some(10_000),
            performance_monitoring: true,
            custom_params: HashMap::new(),
        }
    }
}

/// Specialized interface for Layer 1 (Immediate Flow Registry)
#[async_trait]
pub trait ImmediateFlowRegistry: MfnLayer {
    /// Check if content exists with bloom filter (fast negative check)
    async fn bloom_check(&self, content_hash: u64) -> bool;
    
    /// Get exact match if it exists
    async fn exact_match(&self, content_hash: u64) -> LayerResult<Option<UniversalMemory>>;
    
    /// Add content to bloom filter and hash table
    async fn index_content(&mut self, memory: &UniversalMemory) -> LayerResult<()>;
}

/// Specialized interface for Layer 2 (Dynamic Similarity Reservoir)
#[async_trait]
pub trait DynamicSimilarityReservoir: MfnLayer {
    /// Encode content/embedding into spike patterns
    async fn encode_to_spikes(&self, input: &SimilarityInput) -> LayerResult<SpikePattern>;
    
    /// Find similar memories using spiking neural network
    async fn find_similar(&self, input: &SimilarityInput) -> LayerResult<Vec<SimilarityMatch>>;
    
    /// Add memory with dynamic attractor formation
    async fn add_dynamic_attractor(&mut self, memory: &UniversalMemory) -> LayerResult<()>;
    
    /// Get current reservoir state
    async fn get_reservoir_state(&self) -> LayerResult<ReservoirState>;
}

/// Input for similarity operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimilarityInput {
    Content(String),
    Embedding(Vec<f32>),
    Memory(UniversalMemory),
}

/// Spike pattern representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpikePattern {
    pub spike_times: Vec<f64>,
    pub neuron_ids: Vec<usize>,
    pub duration_ms: f64,
    pub encoding_method: String,
}

/// Similarity match result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarityMatch {
    pub memory: UniversalMemory,
    pub similarity_score: Weight,
    pub spike_correlation: f64,
    pub network_activation: Vec<f64>,
}

/// Reservoir state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReservoirState {
    pub total_neurons: usize,
    pub active_neurons: usize,
    pub average_activity: f64,
    pub connectivity_density: f64,
    pub attractors_count: usize,
}

/// Specialized interface for Layer 3 (Associative Link Mesh)
#[async_trait]
pub trait AssociativeLinkMesh: MfnLayer {
    /// Perform multi-hop associative search
    async fn associative_search(&self, query: &AssociativeSearchQuery) -> LayerResult<AssociativeSearchResults>;
    
    /// Auto-discover associations between memories
    async fn discover_associations(&mut self, memory_id: MemoryId) -> LayerResult<Vec<UniversalAssociation>>;
    
    /// Get graph statistics
    async fn get_graph_stats(&self) -> LayerResult<GraphStatistics>;
    
    /// Find shortest path between memories
    async fn shortest_path(&self, from: MemoryId, to: MemoryId) -> LayerResult<Option<Vec<SearchStep>>>;
}

/// Associative search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociativeSearchQuery {
    pub start_memory_ids: Vec<MemoryId>,
    pub search_mode: AssociativeSearchMode,
    pub max_depth: usize,
    pub max_results: usize,
    pub min_weight: Weight,
    pub association_filters: Vec<AssociationType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssociativeSearchMode {
    DepthFirst,
    BreadthFirst,
    BestFirst,
    Random,
}

/// Associative search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociativeSearchResults {
    pub results: Vec<UniversalSearchResult>,
    pub paths_explored: usize,
    pub total_associations_traversed: usize,
    pub search_time_us: u64,
}

/// Graph statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStatistics {
    pub total_memories: usize,
    pub total_associations: usize,
    pub average_connections: f64,
    pub graph_density: f64,
    pub connected_components: usize,
    pub largest_component_size: usize,
}

/// Specialized interface for Layer 4 (Context Prediction Engine)
#[async_trait]
pub trait ContextPredictionEngine: MfnLayer {
    /// Predict next likely memories based on context
    async fn predict_next(&self, context: &ContextWindow) -> LayerResult<Vec<PredictionResult>>;
    
    /// Learn from memory access patterns
    async fn learn_pattern(&mut self, access_sequence: &[MemoryAccess]) -> LayerResult<()>;
    
    /// Get current context state
    async fn get_context_state(&self) -> LayerResult<ContextState>;
    
    /// Update context with new memory access
    async fn update_context(&mut self, access: MemoryAccess) -> LayerResult<()>;
}

/// Context window for predictions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub recent_accesses: Vec<MemoryAccess>,
    pub temporal_patterns: Vec<TemporalPattern>,
    pub user_context: HashMap<String, serde_json::Value>,
    pub window_size_ms: u64,
}

/// Memory access event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAccess {
    pub memory_id: MemoryId,
    pub access_type: AccessType,
    pub timestamp: Timestamp,
    pub context_metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccessType {
    Read,
    Write,
    Search,
    Association,
}

/// Temporal pattern detected in memory access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalPattern {
    pub pattern_id: String,
    pub memory_sequence: Vec<MemoryId>,
    pub average_interval_ms: u64,
    pub confidence: Weight,
    pub occurrences: u32,
}

/// Prediction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    pub predicted_memory: UniversalMemory,
    pub confidence: Weight,
    pub prediction_type: PredictionType,
    pub contributing_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredictionType {
    SequentialNext,
    AssociativeJump,
    ContextualInference,
    PatternBased,
}

/// Context engine state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextState {
    pub active_patterns: usize,
    pub context_window_size: usize,
    pub prediction_accuracy: f64,
    pub learning_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_decision_serialization() {
        let decision = RoutingDecision::RouteToLayers {
            suggested_layers: vec![LayerId::Layer2, LayerId::Layer3],
            routing_confidence: 0.8,
        };
        
        let serialized = serde_json::to_string(&decision).unwrap();
        let deserialized: RoutingDecision = serde_json::from_str(&serialized).unwrap();
        
        match deserialized {
            RoutingDecision::RouteToLayers { suggested_layers, routing_confidence } => {
                assert_eq!(suggested_layers.len(), 2);
                assert!((routing_confidence - 0.8).abs() < f64::EPSILON);
            }
            _ => panic!("Unexpected routing decision type"),
        }
    }

    #[test]
    fn test_layer_config_default() {
        let config = LayerConfig::default();
        assert_eq!(config.default_timeout_us, 20_000);
        assert!(config.enable_caching);
        assert!(config.performance_monitoring);
    }
}