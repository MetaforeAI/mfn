//! Layer 4: Context Prediction Engine (CPE)
//! 
//! This layer provides temporal pattern analysis and context-aware memory prediction
//! for the Memory Flow Network (MFN) system. It analyzes memory access patterns
//! to predict likely next memories and optimize retrieval performance.
//!
//! # Features
//! 
//! - Temporal pattern analysis with sliding window
//! - N-gram frequency analysis
//! - Markov chain transition probabilities  
//! - Pattern matching state machine
//! - Context-aware predictions
//! - Session-based tracking
//! - Performance monitoring
//! - Async processing with caching
//! 
//! # Architecture
//! 
//! The CPE operates as the final layer in the MFN pipeline, receiving memory
//! access contexts and producing predictions about likely next accesses. It
//! maintains temporal models that learn from access patterns without requiring
//! explicit training phases.
//! 
//! # Usage
//! 
//! ```rust
//! use layer4_cpe::{ContextPredictionLayer, ContextPredictionConfig};
//! use mfn_core::memory_types::*;
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = ContextPredictionConfig::default();
//!     let mut layer = ContextPredictionLayer::new(config).await?;
//!     
//!     // Query for predictions
//!     let query = UniversalSearchQuery {
//!         content: "user behavior pattern".to_string(),
//!         max_results: 10,
//!         // ... other fields
//!     };
//!     
//!     let decision = layer.search(&query).await?;
//!     println!("Prediction confidence: {}", decision.confidence);
//!     
//!     Ok(())
//! }
//! ```

pub mod temporal;
pub mod prediction;
pub mod error;

// Re-export main types
pub use prediction::{
    ContextPredictionLayer,
    ContextPredictionConfig,
    ContextPredictionPerformance,
};

pub use temporal::{
    TemporalAnalyzer,
    TemporalConfig,
    MemoryAccess,
    TemporalPattern,
    PatternType,
};

pub use error::{CpeError, CpeResult};

// Re-export core MFN types for convenience
pub use mfn_core::{
    memory_types::*,
    layer_interface::*,
    layer_interface::RoutingDecision,
};

#[cfg(feature = "ffi")]
pub mod ffi;

/// Library version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Create a new Context Prediction Layer with default configuration
pub async fn create_layer() -> CpeResult<ContextPredictionLayer> {
    ContextPredictionLayer::new(ContextPredictionConfig::default()).await
        .map_err(|e| CpeError::internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_layer_creation() {
        let layer = create_layer().await;
        assert!(layer.is_ok());

        let layer = layer.unwrap();
        assert_eq!(layer.layer_id(), LayerId::Layer4);
        assert_eq!(layer.layer_name(), "Context Prediction Engine");
    }

    #[tokio::test]
    async fn test_basic_functionality() {
        let layer = create_layer().await;
        assert!(layer.is_ok());
        
        let layer = layer.unwrap();
        assert_eq!(layer.get_window_size(), 0);
    }

    #[tokio::test]
    async fn test_version() {
        assert!(!VERSION.is_empty());
    }
}