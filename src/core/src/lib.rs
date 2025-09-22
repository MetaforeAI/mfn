//! # MFN Core - Memory Flow Network Core Library
//!
//! This crate provides the foundational types, traits, and orchestration logic
//! for the Memory Flow Network (MFN) system. It defines universal interfaces
//! that all MFN layers must implement, enabling modular and pluggable architectures.
//!
//! ## Architecture Overview
//!
//! The MFN system consists of four layers:
//!
//! - **Layer 1**: Immediate Flow Registry (IFR) - Ultra-fast exact matching
//! - **Layer 2**: Dynamic Similarity Reservoir (DSR) - Neural similarity search  
//! - **Layer 3**: Associative Link Mesh (ALM) - Graph-based associative memory
//! - **Layer 4**: Context Prediction Engine (CPE) - Temporal pattern prediction
//!
//! ## Core Components
//!
//! ### Universal Types
//! All layers work with standardized memory and association types:
//! ```rust
//! use mfn_core::{UniversalMemory, UniversalAssociation, MemoryId};
//!
//! let memory = UniversalMemory::new(1, "Hello world".to_string())
//!     .with_tags(vec!["greeting".to_string()]);
//! ```
//!
//! ### Layer Interface  
//! Each layer implements the `MfnLayer` trait with all required methods.
//!
//! ### Orchestration
//! The orchestrator coordinates memory flow between layers:
//! ```no_run
//! use mfn_core::{MfnOrchestrator, UniversalSearchQuery};
//! 
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut orchestrator = MfnOrchestrator::new();
//! // Register layers...
//! 
//! let query = UniversalSearchQuery::default();
//! let results = orchestrator.search(query).await?;
//! # Ok(())
//! # }
//! ```

pub mod memory_types;
pub mod layer_interface;
pub mod orchestrator;

// Re-export commonly used types
pub use memory_types::{
    UniversalMemory, UniversalAssociation, UniversalSearchQuery, 
    UniversalSearchResult, UniversalSearchResults, MemoryId, Weight,
    AssociationType, LayerId, current_timestamp, timestamp_to_systemtime
};

pub use layer_interface::{
    MfnLayer, LayerError, LayerResult, RoutingDecision, LayerHealth,
    LayerConfig, HealthStatus, ResourceUsage, LayerPerformance,
    
    // Specialized interfaces
    ImmediateFlowRegistry, DynamicSimilarityReservoir, 
    AssociativeLinkMesh, ContextPredictionEngine,
    
    // Specialized types
    SimilarityInput, SpikePattern, SimilarityMatch, ReservoirState,
    AssociativeSearchQuery, AssociativeSearchMode, AssociativeSearchResults,
    GraphStatistics, ContextWindow, MemoryAccess, TemporalPattern,
    PredictionResult, PredictionType, ContextState
};

pub use orchestrator::{
    MfnOrchestrator, RoutingConfig, RoutingStrategy, PerformanceMonitor
};

/// Current version of the MFN Core library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default configuration values
pub mod defaults {
    
    /// Default search timeout (20ms)
    pub const DEFAULT_SEARCH_TIMEOUT_US: u64 = 20_000;
    
    /// Default maximum memories per layer
    pub const DEFAULT_MAX_MEMORIES: usize = 1_000_000;
    
    /// Default maximum associations per layer
    pub const DEFAULT_MAX_ASSOCIATIONS: usize = 10_000_000;
    
    /// Default confidence threshold for early search termination
    pub const DEFAULT_CONFIDENCE_THRESHOLD: f64 = 0.9;
    
    /// Default search depth for associative queries
    pub const DEFAULT_SEARCH_DEPTH: usize = 3;
    
    /// Default number of results to return
    pub const DEFAULT_MAX_RESULTS: usize = 10;
}

/// Utility functions for MFN operations
pub mod utils {
    use crate::{UniversalMemory, AssociationType, Weight};
    
    /// Calculate semantic similarity between two memories based on tags
    pub fn tag_similarity(memory1: &UniversalMemory, memory2: &UniversalMemory) -> Weight {
        if memory1.tags.is_empty() && memory2.tags.is_empty() {
            return 0.0;
        }
        
        let set1: std::collections::HashSet<_> = memory1.tags.iter().collect();
        let set2: std::collections::HashSet<_> = memory2.tags.iter().collect();
        
        let intersection = set1.intersection(&set2).count();
        let union = set1.union(&set2).count();
        
        if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
    }
    
    /// Calculate content similarity using simple string metrics
    pub fn content_similarity(content1: &str, content2: &str) -> Weight {
        // Simple Jaccard similarity on words
        let words1: std::collections::HashSet<_> = content1
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();
        let words2: std::collections::HashSet<_> = content2
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();
        
        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();
        
        if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
    }
    
    /// Suggest association type based on memory properties
    pub fn suggest_association_type(memory1: &UniversalMemory, memory2: &UniversalMemory) -> AssociationType {
        let tag_sim = tag_similarity(memory1, memory2);
        let content_sim = content_similarity(&memory1.content, &memory2.content);
        
        // Simple heuristic
        if tag_sim > 0.5 {
            AssociationType::Semantic
        } else if content_sim > 0.3 {
            AssociationType::Conceptual
        } else if memory1.created_at.abs_diff(memory2.created_at) < 60_000_000 { // 1 minute
            AssociationType::Temporal
        } else {
            AssociationType::Domain
        }
    }
    
    /// Generate a unique association ID
    pub fn generate_association_id(from_id: crate::MemoryId, to_id: crate::MemoryId) -> String {
        format!("assoc_{}_{}", from_id, to_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_memory_creation() {
        let memory = UniversalMemory::new(1, "Test content".to_string())
            .with_tags(vec!["test".to_string()]);
        
        assert_eq!(memory.id, 1);
        assert_eq!(memory.content, "Test content");
        assert_eq!(memory.tags, vec!["test"]);
    }

    #[test]
    fn test_tag_similarity() {
        let memory1 = UniversalMemory::new(1, "Test 1".to_string())
            .with_tags(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        
        let memory2 = UniversalMemory::new(2, "Test 2".to_string())
            .with_tags(vec!["b".to_string(), "c".to_string(), "d".to_string()]);
        
        let similarity = utils::tag_similarity(&memory1, &memory2);
        assert!((similarity - 0.5).abs() < f64::EPSILON); // 2/4 = 0.5
    }

    #[test]
    fn test_content_similarity() {
        let content1 = "the quick brown fox";
        let content2 = "the fast brown dog";
        
        let similarity = utils::content_similarity(content1, content2);
        assert!(similarity > 0.0 && similarity < 1.0);
    }

    #[test]
    fn test_association_id_generation() {
        let id = utils::generate_association_id(1, 2);
        assert_eq!(id, "assoc_1_2");
    }
}