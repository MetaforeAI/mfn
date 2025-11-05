//! MFN Monolith - Single-process high-performance memory network
//!
//! Integrates all 4 layers into a unified, optimized architecture.

pub mod types;
pub mod layer1;
pub mod layer2;
pub mod layer3;
pub mod layer4;
pub mod orchestrator;

// Re-export key types
pub use types::{Memory, MemoryId, Query, QueryResult, SearchResult, Layer};
pub use layer1::ExactMatchCache;
pub use layer2::{SimilarityIndex, Layer2Stats};
pub use layer3::{GraphIndex, AssociationType, SearchMode};
pub use layer4::ContextPredictor;
pub use orchestrator::{query_parallel, add_memory_to_all, get_all_stats, OrchestrationStats};
