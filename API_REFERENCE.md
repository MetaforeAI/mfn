# MFN API Reference

**Version:** 0.1.0
**Status:** Production Ready

Complete API reference for the Memory Flow Network (MFN) system.

---

## Table of Contents

1. [Core Types](#core-types)
2. [Memory Operations](#memory-operations)
3. [Search Operations](#search-operations)
4. [Orchestrator](#orchestrator)
5. [Layer Interface](#layer-interface)
6. [Layer-Specific APIs](#layer-specific-apis)
7. [Error Handling](#error-handling)
8. [Configuration](#configuration)

---

## Core Types

### UniversalMemory

The fundamental memory unit in MFN.

```rust
pub struct UniversalMemory {
    pub id: MemoryId,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub timestamp: Timestamp,
}
```

**Methods:**

```rust
// Create a new memory
pub fn new(id: MemoryId, content: String) -> Self

// Builder methods
pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self
pub fn with_tags(mut self, tags: Vec<String>) -> Self
pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> Self

// Update last access timestamp
pub fn touch(&mut self)
```

**Example:**

```rust
use mfn_core::{UniversalMemory, MemoryId};

let memory = UniversalMemory::new(
    MemoryId(1),
    "Hello World".to_string()
)
.with_tags(vec!["greeting".to_string()])
.with_metadata(
    [("author".to_string(), "Alice".to_string())]
    .iter().cloned().collect()
);
```

---

### MemoryId

Unique identifier for memories.

```rust
pub struct MemoryId(pub u64);
```

**Traits:** `Copy`, `Clone`, `Debug`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`

---

### UniversalAssociation

Represents relationships between memories.

```rust
pub struct UniversalAssociation {
    pub from_id: MemoryId,
    pub to_id: MemoryId,
    pub association_type: AssociationType,
    pub weight: Weight,
    pub created_at: Timestamp,
    pub last_accessed: Timestamp,
    pub metadata: HashMap<String, String>,
}
```

**Methods:**

```rust
pub fn new(
    from_id: MemoryId,
    to_id: MemoryId,
    association_type: AssociationType,
    weight: Weight
) -> Self

pub fn touch(&mut self)
pub fn id(&self) -> String  // Returns "from_id-to_id-type"
```

---

### AssociationType

Types of associations between memories.

```rust
pub enum AssociationType {
    Similarity,      // Memories are similar in content/meaning
    Causal,          // One memory caused/led to another
    Temporal,        // Sequential in time
    Spatial,         // Related by location/space
    Hierarchical,    // Parent-child relationship
    Correlational,   // Statistically correlated
    Semantic,        // Shared meaning/concepts
    Contextual,      // Share context/environment
    Predictive,      // One predicts the other
}
```

---

## Search Operations

### UniversalSearchQuery

Universal query structure for all layers.

```rust
pub struct UniversalSearchQuery {
    pub start_memory_ids: Vec<MemoryId>,
    pub content: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub association_types: Vec<AssociationType>,
    pub max_depth: usize,
    pub max_results: usize,
    pub min_weight: Weight,
    pub timeout_us: u64,
    pub layer_params: HashMap<String, serde_json::Value>,
}
```

**Default Values:**

```rust
impl Default for UniversalSearchQuery {
    fn default() -> Self {
        Self {
            start_memory_ids: vec![],
            content: None,
            embedding: None,
            tags: vec![],
            association_types: vec![],
            max_depth: 3,                  // DEFAULT_SEARCH_DEPTH
            max_results: 10,               // DEFAULT_MAX_RESULTS
            min_weight: 0.0,
            timeout_us: 20_000,            // 20ms
            layer_params: HashMap::new(),
        }
    }
}
```

**Example - Content Search:**

```rust
let query = UniversalSearchQuery {
    content: Some("machine learning".to_string()),
    max_results: 20,
    min_weight: 0.5,
    ..Default::default()
};
```

**Example - Associative Search:**

```rust
let query = UniversalSearchQuery {
    start_memory_ids: vec![MemoryId(100)],
    association_types: vec![
        AssociationType::Causal,
        AssociationType::Temporal,
    ],
    max_depth: 5,
    ..Default::default()
};
```

**Example - Similarity Search:**

```rust
let query = UniversalSearchQuery {
    embedding: Some(vec![0.1, 0.2, 0.3, /* ... */]),
    max_results: 10,
    min_weight: 0.7,
    ..Default::default()
};
```

---

### UniversalSearchResult

Single search result.

```rust
pub struct UniversalSearchResult {
    pub memory: UniversalMemory,
    pub confidence: Weight,
    pub match_type: String,
    pub path: Vec<MemoryId>,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

---

### UniversalSearchResults

Complete search response.

```rust
pub struct UniversalSearchResults {
    pub results: Vec<UniversalSearchResult>,
    pub query: UniversalSearchQuery,
    pub total_found: usize,
    pub search_time_us: u64,
    pub layers_consulted: Vec<LayerId>,
    pub performance_stats: HashMap<String, serde_json::Value>,
}
```

---

## Orchestrator

### MfnOrchestrator

Central coordinator for all MFN operations.

```rust
pub struct MfnOrchestrator {
    // Private fields
}
```

**Creation:**

```rust
impl MfnOrchestrator {
    pub fn new(config: OrchestratorConfig) -> Self
}

impl Default for MfnOrchestrator {
    fn default() -> Self
}
```

**Core Methods:**

```rust
// Memory operations
pub async fn add_memory(&mut self, memory: UniversalMemory)
    -> LayerResult<()>

pub async fn add_memories(&mut self, memories: Vec<UniversalMemory>)
    -> LayerResult<()>

pub async fn get_memory(&self, id: MemoryId)
    -> LayerResult<Option<UniversalMemory>>

pub async fn update_memory(&mut self, memory: UniversalMemory)
    -> LayerResult<()>

pub async fn delete_memory(&mut self, id: MemoryId)
    -> LayerResult<()>

// Association operations
pub async fn add_association(&mut self, association: UniversalAssociation)
    -> LayerResult<()>

pub async fn get_associations(&self, memory_id: MemoryId)
    -> LayerResult<Vec<UniversalAssociation>>

// Search operations
pub async fn search(&self, query: &UniversalSearchQuery)
    -> LayerResult<UniversalSearchResults>

// Health and monitoring
pub async fn health_check(&self) -> LayerResult<HashMap<LayerId, LayerHealth>>

pub fn get_performance_stats(&self) -> PerformanceStats
```

**Layer Management:**

```rust
pub async fn register_layer<L: MfnLayer + Send + Sync + 'static>(
    &mut self,
    layer_id: LayerId,
    layer: L
) -> LayerResult<()>

pub async fn unregister_layer(&mut self, layer_id: LayerId)
    -> LayerResult<()>
```

**Example - Basic Usage:**

```rust
use mfn_core::{MfnOrchestrator, OrchestratorConfig, UniversalMemory, MemoryId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create orchestrator
    let config = OrchestratorConfig::default();
    let mut orchestrator = MfnOrchestrator::new(config);

    // Add a memory
    let memory = UniversalMemory::new(
        MemoryId(1),
        "Neural networks are fascinating".to_string()
    ).with_tags(vec!["AI".to_string(), "ML".to_string()]);

    orchestrator.add_memory(memory).await?;

    // Search for it
    let query = UniversalSearchQuery {
        content: Some("neural".to_string()),
        max_results: 10,
        ..Default::default()
    };

    let results = orchestrator.search(&query).await?;

    println!("Found {} results", results.total_found);
    for result in results.results {
        println!("  - {} (confidence: {:.2})",
            result.memory.content, result.confidence);
    }

    Ok(())
}
```

---

### OrchestratorConfig

Configuration for the orchestrator.

```rust
pub struct OrchestratorConfig {
    pub routing_strategy: RoutingStrategy,
    pub layer_timeout_us: u64,
    pub enable_caching: bool,
    pub cache_size: usize,
    pub enable_parallel: bool,
}
```

**Default:**

```rust
impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            routing_strategy: RoutingStrategy::Sequential,
            layer_timeout_us: 100_000,  // 100ms per layer
            enable_caching: true,
            cache_size: 1000,
            enable_parallel: false,
        }
    }
}
```

---

### RoutingStrategy

Determines how queries flow through layers.

```rust
pub enum RoutingStrategy {
    Sequential,    // Layer 1 → 2 → 3 → 4 in order
    Parallel,      // All layers simultaneously
    Adaptive,      // Intelligent routing based on query
}
```

**Sequential**: Queries each layer in order, can short-circuit on exact match.
**Parallel**: Queries all layers concurrently, combines results.
**Adaptive**: Uses query characteristics to determine optimal routing.

---

## Layer Interface

### MfnLayer Trait

All layers must implement this trait.

```rust
#[async_trait]
pub trait MfnLayer: Send + Sync {
    // Core operations
    async fn add_memory(&mut self, memory: &UniversalMemory)
        -> LayerResult<()>;

    async fn get_memory(&self, id: MemoryId)
        -> LayerResult<Option<UniversalMemory>>;

    async fn search(&self, query: &UniversalSearchQuery)
        -> LayerResult<RoutingDecision>;

    async fn add_association(&mut self, association: &UniversalAssociation)
        -> LayerResult<()>;

    async fn get_associations(&self, memory_id: MemoryId)
        -> LayerResult<Vec<UniversalAssociation>>;

    async fn delete_memory(&mut self, id: MemoryId)
        -> LayerResult<()>;

    // Health and metadata
    async fn health_check(&self) -> LayerResult<LayerHealth>;

    fn layer_id(&self) -> LayerId;

    fn layer_type(&self) -> &'static str;
}
```

---

### RoutingDecision

Returned by layers to guide orchestrator.

```rust
pub enum RoutingDecision {
    FoundExact {
        results: Vec<UniversalSearchResult>,
    },
    FoundPartial {
        results: Vec<UniversalSearchResult>,
        continue_search: bool,
        suggested_layers: Vec<LayerId>,
    },
    RouteToLayers {
        suggested_layers: Vec<LayerId>,
        reason: String,
    },
    SearchComplete {
        results: Vec<UniversalSearchResult>,
    },
}
```

---

### LayerHealth

Health status information.

```rust
pub struct LayerHealth {
    pub status: HealthStatus,
    pub uptime_seconds: u64,
    pub total_queries: u64,
    pub error_count: u64,
    pub memory_count: usize,
    pub association_count: usize,
    pub resource_usage: ResourceUsage,
    pub last_error: Option<String>,
}

pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

pub struct ResourceUsage {
    pub memory_bytes: usize,
    pub cpu_percent: f64,
}
```

---

## Layer-Specific APIs

### Layer 1: Immediate Flow Registry (IFR)

**Fast exact matching with bloom filters.**

```rust
use layer1_zig_ifr::ImmediateFlowRegistry;

let mut ifr = ImmediateFlowRegistry::new();

// Operations are inherited from MfnLayer trait
// Specialized for exact hash-based lookups
```

**Performance:** ~0.5μs per exact match

---

### Layer 2: Dynamic Similarity Reservoir (DSR)

**Spiking neural network similarity search.**

```rust
use layer2_rust_dsr::DynamicSimilarityReservoir;

let config = DsrConfig {
    reservoir_size: 1000,
    spectral_radius: 1.5,
    input_scaling: 0.5,
    leak_rate: 0.3,
};

let mut dsr = DynamicSimilarityReservoir::new(config);
```

**Specialized Methods:**

```rust
// Encode memory into spike pattern
pub async fn encode_memory(&self, memory: &UniversalMemory)
    -> Result<SpikePattern, DsrError>

// Find similar memories using spike distance
pub async fn find_similar(&self, pattern: &SpikePattern, threshold: f64)
    -> Result<Vec<SimilarityMatch>, DsrError>
```

**Performance:**
- Encoding: ~159ns
- Reservoir update: ~109ns
- Similarity search: <2ms

---

### Layer 3: Associative Link Mesh (ALM)

**Graph-based relationship traversal.**

```rust
use layer3_go_alm::AssociativeLinkMesh;

let config = AlmConfig {
    max_depth: 5,
    traversal_strategy: TraversalStrategy::BreadthFirst,
};

let mut alm = AssociativeLinkMesh::new(config);
```

**Specialized Methods:**

```rust
// Find related memories by association type
pub async fn traverse_associations(
    &self,
    start_id: MemoryId,
    types: Vec<AssociationType>,
    max_depth: usize
) -> Result<AssociativeSearchResults, AlmError>

// Get graph statistics
pub async fn get_graph_stats(&self) -> Result<GraphStatistics, AlmError>
```

**Performance:** ~0.77ms per graph search

---

### Layer 4: Context Prediction Engine (CPE)

**Temporal pattern prediction with n-grams and Markov chains.**

```rust
use layer4_rust_cpe::ContextPredictionEngine;

let config = TemporalConfig {
    max_window_size: 1000,
    min_pattern_occurrences: 3,
    max_ngram_length: 8,
    min_prediction_confidence: 0.3,
    pattern_decay_rate: 0.1,
    max_sequence_gap_us: 60_000_000,  // 1 minute
    enable_statistical_modeling: true,
};

let mut cpe = ContextPredictionEngine::new(config);
```

**Specialized Methods:**

```rust
// Record memory access for temporal analysis
pub async fn record_access(&mut self, access: MemoryAccess)
    -> Result<(), CpeError>

// Predict next memories based on context
pub async fn predict_next(&self, context: &PredictionContext)
    -> Result<Vec<PredictionResult>, CpeError>

// Get detected patterns
pub async fn get_patterns(&self, pattern_type: Option<PatternType>)
    -> Result<Vec<TemporalPattern>, CpeError>
```

**Types:**

```rust
pub struct MemoryAccess {
    pub memory_id: MemoryId,
    pub timestamp: Timestamp,
    pub access_type: AccessType,
    pub user_context: Option<String>,
    pub session_id: Option<String>,
    pub confidence: Weight,
}

pub struct PredictionContext {
    pub recent_sequence: Option<Vec<MemoryId>>,
    pub current_timestamp: Timestamp,
    pub user_context: Option<String>,
    pub session_id: Option<String>,
    pub max_predictions: usize,
}

pub struct PredictionResult {
    pub memory_id: MemoryId,
    pub confidence: Weight,
    pub prediction_type: PredictionType,
    pub estimated_time_us: u64,
    pub contributing_evidence: Vec<String>,
}

pub enum PredictionType {
    NGramBased,
    MarkovChain,
    PatternCompletion,
    StatisticalModel,
    HybridEnsemble,
}
```

**Performance:** Routing <200μs

---

## Error Handling

### LayerError

Main error type for all layer operations.

```rust
pub enum LayerError {
    NotFound(String),
    InvalidInput(String),
    Timeout(Duration),
    InternalError(String),
    UnsupportedOperation(String),
    StorageFull,
    InvalidState(String),
}
```

**Result Type:**

```rust
pub type LayerResult<T> = Result<T, LayerError>;
```

**Error Handling Example:**

```rust
match orchestrator.search(&query).await {
    Ok(results) => {
        println!("Found {} results", results.total_found);
    }
    Err(LayerError::Timeout(duration)) => {
        eprintln!("Search timed out after {:?}", duration);
    }
    Err(LayerError::NotFound(msg)) => {
        eprintln!("Not found: {}", msg);
    }
    Err(e) => {
        eprintln!("Error: {:?}", e);
    }
}
```

---

## Configuration

### Default Constants

```rust
// From mfn_core::defaults

pub const DEFAULT_SEARCH_TIMEOUT_US: u64 = 20_000;         // 20ms
pub const DEFAULT_MAX_MEMORIES: usize = 1_000_000;         // 1M
pub const DEFAULT_MAX_ASSOCIATIONS: usize = 10_000_000;    // 10M
pub const DEFAULT_CONFIDENCE_THRESHOLD: f64 = 0.9;         // 90%
pub const DEFAULT_SEARCH_DEPTH: usize = 3;
pub const DEFAULT_MAX_RESULTS: usize = 10;
```

---

## Utility Functions

### Timestamp Operations

```rust
// Get current timestamp in microseconds
pub fn current_timestamp() -> u64

// Convert timestamp to SystemTime
pub fn timestamp_to_systemtime(timestamp: u64) -> SystemTime
```

**Example:**

```rust
use mfn_core::current_timestamp;

let now = current_timestamp();
println!("Current timestamp: {} μs", now);
```

---

## Complete Example

```rust
use mfn_core::{
    MfnOrchestrator, OrchestratorConfig, RoutingStrategy,
    UniversalMemory, UniversalAssociation, UniversalSearchQuery,
    MemoryId, AssociationType, current_timestamp,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create orchestrator with custom config
    let config = OrchestratorConfig {
        routing_strategy: RoutingStrategy::Sequential,
        layer_timeout_us: 50_000,  // 50ms
        enable_caching: true,
        cache_size: 5000,
        ..Default::default()
    };
    let mut orchestrator = MfnOrchestrator::new(config);

    // 2. Add memories
    let memory1 = UniversalMemory::new(
        MemoryId(1),
        "Neural networks revolutionized AI".to_string()
    ).with_tags(vec!["AI".to_string(), "ML".to_string()]);

    let memory2 = UniversalMemory::new(
        MemoryId(2),
        "Deep learning uses neural networks".to_string()
    ).with_tags(vec!["AI".to_string(), "DL".to_string()]);

    orchestrator.add_memories(vec![memory1, memory2]).await?;

    // 3. Create association
    let association = UniversalAssociation::new(
        MemoryId(1),
        MemoryId(2),
        AssociationType::Causal,
        0.9  // High confidence
    );
    orchestrator.add_association(association).await?;

    // 4. Search by content
    let query = UniversalSearchQuery {
        content: Some("neural networks".to_string()),
        max_results: 10,
        min_weight: 0.5,
        ..Default::default()
    };

    let results = orchestrator.search(&query).await?;

    println!("\n=== Search Results ===");
    println!("Found: {} results in {} μs",
        results.total_found, results.search_time_us);
    println!("Layers consulted: {:?}", results.layers_consulted);

    for (i, result) in results.results.iter().enumerate() {
        println!("\n{}. {} (confidence: {:.2})",
            i + 1, result.memory.content, result.confidence);
        println!("   Match type: {}", result.match_type);
        if !result.path.is_empty() {
            println!("   Path: {:?}", result.path);
        }
    }

    // 5. Check health
    let health = orchestrator.health_check().await?;
    println!("\n=== System Health ===");
    for (layer_id, status) in health {
        println!("{:?}: {:?} - {} memories, {} associations",
            layer_id, status.status,
            status.memory_count, status.association_count);
    }

    Ok(())
}
```

---

## Performance Guidelines

### Query Optimization

1. **Use appropriate timeout values:**
   ```rust
   query.timeout_us = 10_000;  // 10ms for interactive queries
   query.timeout_us = 100_000; // 100ms for background searches
   ```

2. **Limit result set:**
   ```rust
   query.max_results = 10;  // Only fetch what you need
   ```

3. **Use min_weight filtering:**
   ```rust
   query.min_weight = 0.7;  // Only high-confidence results
   ```

4. **Choose appropriate routing strategy:**
   ```rust
   // For exact matches
   config.routing_strategy = RoutingStrategy::Sequential;

   // For comprehensive search
   config.routing_strategy = RoutingStrategy::Parallel;

   // For optimal performance
   config.routing_strategy = RoutingStrategy::Adaptive;
   ```

### Memory Management

1. **Batch operations when possible:**
   ```rust
   orchestrator.add_memories(batch).await?;  // Better than loop
   ```

2. **Use appropriate cache size:**
   ```rust
   config.cache_size = 10_000;  // Based on working set
   ```

3. **Monitor resource usage:**
   ```rust
   let health = orchestrator.health_check().await?;
   for (_, status) in health {
       println!("Memory usage: {} bytes",
           status.resource_usage.memory_bytes);
   }
   ```

---

## See Also

- [USER_GUIDE.md](USER_GUIDE.md) - Complete usage guide
- [MFN_INTEGRATION_COMPLETE.md](MFN_INTEGRATION_COMPLETE.md) - Integration status
- [MFN_TECHNICAL_ANALYSIS_REPORT.md](MFN_TECHNICAL_ANALYSIS_REPORT.md) - Technical analysis

---

**Version:** 0.1.0
**Last Updated:** 2025-11-04
**Status:** Alpha Testing
