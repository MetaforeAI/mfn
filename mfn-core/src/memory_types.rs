// MFN Core - Universal Memory Types and Interfaces
// Defines the fundamental memory structures used across all MFN layers

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Universal Memory ID type used across all layers
pub type MemoryId = u64;

/// Universal memory weight/confidence type
pub type Weight = f64;

/// Universal timestamp type (microseconds since Unix epoch)
pub type Timestamp = u64;

/// Core memory representation that all layers must understand
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UniversalMemory {
    pub id: MemoryId,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: Timestamp,
    pub last_accessed: Timestamp,
    pub access_count: u64,
}

impl UniversalMemory {
    pub fn new(id: MemoryId, content: String) -> Self {
        let now = current_timestamp();
        Self {
            id,
            content,
            embedding: None,
            tags: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            last_accessed: now,
            access_count: 0,
        }
    }

    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn touch(&mut self) {
        self.last_accessed = current_timestamp();
        self.access_count += 1;
    }

    /// Calculate content hash for exact matching
    pub fn content_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        self.content.hash(&mut hasher);
        hasher.finish()
    }
}

/// Association between memories with type and strength
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UniversalAssociation {
    pub id: String,
    pub from_memory_id: MemoryId,
    pub to_memory_id: MemoryId,
    pub association_type: AssociationType,
    pub weight: Weight,
    pub reason: String,
    pub created_at: Timestamp,
    pub last_used: Timestamp,
    pub usage_count: u64,
}

/// Types of associations between memories
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AssociationType {
    /// Semantic similarity in meaning
    Semantic,
    /// Temporal relationship (sequence, co-occurrence)
    Temporal,
    /// Causal relationship (cause and effect)
    Causal,
    /// Spatial/location-based relationship
    Spatial,
    /// Abstract conceptual relationship
    Conceptual,
    /// Hierarchical relationship (parent-child)
    Hierarchical,
    /// Functional relationship (tool-use, method-goal)
    Functional,
    /// Same domain or field
    Domain,
    /// Cognitive/mental association
    Cognitive,
    /// Custom user-defined association type
    Custom(String),
}

impl AssociationType {
    pub fn as_str(&self) -> &str {
        match self {
            AssociationType::Semantic => "semantic",
            AssociationType::Temporal => "temporal",
            AssociationType::Causal => "causal",
            AssociationType::Spatial => "spatial",
            AssociationType::Conceptual => "conceptual",
            AssociationType::Hierarchical => "hierarchical",
            AssociationType::Functional => "functional",
            AssociationType::Domain => "domain",
            AssociationType::Cognitive => "cognitive",
            AssociationType::Custom(name) => name,
        }
    }
}

/// Search query parameters used across layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalSearchQuery {
    /// Starting memory IDs for search
    pub start_memory_ids: Vec<MemoryId>,
    /// Optional content to search for
    pub content: Option<String>,
    /// Optional embedding for similarity search
    pub embedding: Option<Vec<f32>>,
    /// Filter by tags
    pub tags: Vec<String>,
    /// Filter by association types
    pub association_types: Vec<AssociationType>,
    /// Maximum search depth
    pub max_depth: usize,
    /// Maximum results to return
    pub max_results: usize,
    /// Minimum association weight threshold
    pub min_weight: Weight,
    /// Search timeout in microseconds
    pub timeout_us: u64,
    /// Layer-specific search parameters
    pub layer_params: HashMap<String, serde_json::Value>,
}

impl Default for UniversalSearchQuery {
    fn default() -> Self {
        Self {
            start_memory_ids: Vec::new(),
            content: None,
            embedding: None,
            tags: Vec::new(),
            association_types: Vec::new(),
            max_depth: 3,
            max_results: 10,
            min_weight: 0.1,
            timeout_us: 10_000, // 10ms default
            layer_params: HashMap::new(),
        }
    }
}

/// Search result with path and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalSearchResult {
    pub memory: UniversalMemory,
    pub confidence: Weight,
    pub path: Vec<SearchStep>,
    pub layer_origin: LayerId,
    pub search_time_us: u64,
}

/// Single step in a search path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStep {
    pub from_memory_id: MemoryId,
    pub to_memory_id: MemoryId,
    pub association: UniversalAssociation,
    pub step_weight: Weight,
}

/// Collection of search results with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalSearchResults {
    pub results: Vec<UniversalSearchResult>,
    pub query: UniversalSearchQuery,
    pub total_found: usize,
    pub search_time_us: u64,
    pub layers_consulted: Vec<LayerId>,
    pub performance_stats: HashMap<String, serde_json::Value>,
}

/// Layer identification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum LayerId {
    Layer1, // Immediate Flow Registry
    Layer2, // Dynamic Similarity Reservoir
    Layer3, // Associative Link Mesh
    Layer4, // Context Prediction Engine
}

impl LayerId {
    pub fn as_str(&self) -> &str {
        match self {
            LayerId::Layer1 => "layer1",
            LayerId::Layer2 => "layer2", 
            LayerId::Layer3 => "layer3",
            LayerId::Layer4 => "layer4",
        }
    }
}

/// Get current timestamp in microseconds since Unix epoch
pub fn current_timestamp() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_micros() as Timestamp
}

/// Convert microseconds timestamp to SystemTime
pub fn timestamp_to_systemtime(timestamp: Timestamp) -> SystemTime {
    UNIX_EPOCH + Duration::from_micros(timestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_memory_creation() {
        let memory = UniversalMemory::new(1, "Test content".to_string())
            .with_tags(vec!["test".to_string(), "memory".to_string()]);
        
        assert_eq!(memory.id, 1);
        assert_eq!(memory.content, "Test content");
        assert_eq!(memory.tags, vec!["test", "memory"]);
        assert_eq!(memory.access_count, 0);
    }

    #[test]
    fn test_memory_touch() {
        let mut memory = UniversalMemory::new(1, "Test".to_string());
        let initial_count = memory.access_count;
        let initial_time = memory.last_accessed;
        
        std::thread::sleep(Duration::from_millis(1));
        memory.touch();
        
        assert_eq!(memory.access_count, initial_count + 1);
        assert!(memory.last_accessed > initial_time);
    }

    #[test]
    fn test_association_type_serialization() {
        let assoc_type = AssociationType::Semantic;
        assert_eq!(assoc_type.as_str(), "semantic");

        let custom_type = AssociationType::Custom("domain_specific".to_string());
        assert_eq!(custom_type.as_str(), "domain_specific");
    }
}