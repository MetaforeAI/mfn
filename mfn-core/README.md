# MFN Core - Memory Flow Network Core Library

[![Crates.io](https://img.shields.io/crates/v/mfn-core.svg)](https://crates.io/crates/mfn-core)
[![Documentation](https://docs.rs/mfn-core/badge.svg)](https://docs.rs/mfn-core)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

The foundational library for the Memory Flow Network (MFN) system - a revolutionary multi-layer memory architecture that treats memories as network packets flowing through specialized processing layers.

## 🚀 Overview

MFN Core provides universal interfaces, types, and orchestration logic that enable modular, pluggable memory systems with sub-millisecond performance. Each layer specializes in different aspects of memory processing:

```
┌─────────────────────────────────────────────────────────────┐
│                    Memory Flow Network                      │
├─────────────────────────────────────────────────────────────┤
│ Layer 4: Context Prediction Engine (CPE) - Temporal        │
│ Layer 3: Associative Link Mesh (ALM) - Graph Search       │  
│ Layer 2: Dynamic Similarity Reservoir (DSR) - Neural      │
│ Layer 1: Immediate Flow Registry (IFR) - Exact Match      │
└─────────────────────────────────────────────────────────────┘
```

## ✨ Key Features

- **🔌 Pluggable Architecture** - Swap implementations without changing other layers
- **⚡ Sub-millisecond Performance** - Optimized routing and parallel processing
- **🌐 Universal Types** - Standardized memory and association representations
- **🤖 Neural Integration** - Built-in support for spiking neural networks
- **📊 Graph Processing** - Native associative memory and path finding
- **🔮 Context Prediction** - Temporal pattern analysis and prediction
- **📈 Performance Monitoring** - Real-time metrics and health checking

## 🏗️ Architecture

### Universal Memory Types

All layers work with standardized memory representations:

```rust
use mfn_core::{UniversalMemory, UniversalAssociation};

// Create a memory with metadata
let memory = UniversalMemory::new(1, "The human brain has 86 billion neurons".to_string())
    .with_tags(vec!["neuroscience".to_string(), "facts".to_string()])
    .with_embedding(vec![0.1, 0.5, -0.2, 0.8]);

// Create associations between memories
let association = UniversalAssociation {
    id: "assoc_1_2".to_string(),
    from_memory_id: 1,
    to_memory_id: 2,
    association_type: AssociationType::Semantic,
    weight: 0.85,
    reason: "Related neuroscience concepts".to_string(),
    // ... timestamps and usage tracking
};
```

### Layer Interface

Each layer implements the core `MfnLayer` trait:

```rust
use mfn_core::{MfnLayer, LayerId, LayerResult, RoutingDecision};
use async_trait::async_trait;

#[async_trait]
impl MfnLayer for MyCustomLayer {
    fn layer_id(&self) -> LayerId { LayerId::Layer1 }
    fn layer_name(&self) -> &str { "MyCustomLayer" }
    fn version(&self) -> &str { "1.0.0" }

    async fn search(&self, query: &UniversalSearchQuery) -> LayerResult<RoutingDecision> {
        // Custom search logic
        Ok(RoutingDecision::SearchComplete { results: vec![] })
    }
    
    // ... implement other required methods
}
```

### Orchestrated Memory Flow

The orchestrator coordinates memory flow between layers:

```rust
use mfn_core::{MfnOrchestrator, UniversalSearchQuery, RoutingStrategy};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut orchestrator = MfnOrchestrator::new()
        .with_routing_config(RoutingConfig {
            default_strategy: RoutingStrategy::Adaptive,
            enable_parallel: true,
            confidence_threshold: 0.9,
            ..Default::default()
        });

    // Register layers
    orchestrator.register_layer(Box::new(layer1_impl)).await?;
    orchestrator.register_layer(Box::new(layer2_impl)).await?;
    orchestrator.register_layer(Box::new(layer3_impl)).await?;

    // Perform search
    let query = UniversalSearchQuery {
        content: Some("neural networks".to_string()),
        max_results: 10,
        max_depth: 3,
        ..Default::default()
    };
    
    let results = orchestrator.search(query).await?;
    println!("Found {} results in {}μs", results.total_found, results.search_time_us);
    
    Ok(())
}
```

## 🔧 Layer Implementations

MFN Core defines specialized interfaces for each layer type:

### Layer 1: Immediate Flow Registry
```rust
use mfn_core::ImmediateFlowRegistry;

// Ultra-fast exact matching with bloom filters
impl ImmediateFlowRegistry for ZigIFRLayer {
    async fn bloom_check(&self, content_hash: u64) -> bool { ... }
    async fn exact_match(&self, content_hash: u64) -> LayerResult<Option<UniversalMemory>> { ... }
}
```

### Layer 2: Dynamic Similarity Reservoir  
```rust
use mfn_core::DynamicSimilarityReservoir;

// Spiking neural network similarity search
impl DynamicSimilarityReservoir for RustDSRLayer {
    async fn encode_to_spikes(&self, input: &SimilarityInput) -> LayerResult<SpikePattern> { ... }
    async fn find_similar(&self, input: &SimilarityInput) -> LayerResult<Vec<SimilarityMatch>> { ... }
}
```

### Layer 3: Associative Link Mesh
```rust
use mfn_core::AssociativeLinkMesh;

// Graph-based associative memory
impl AssociativeLinkMesh for GoALMLayer {
    async fn associative_search(&self, query: &AssociativeSearchQuery) -> LayerResult<AssociativeSearchResults> { ... }
    async fn discover_associations(&mut self, memory_id: MemoryId) -> LayerResult<Vec<UniversalAssociation>> { ... }
}
```

### Layer 4: Context Prediction Engine
```rust
use mfn_core::ContextPredictionEngine;

// Temporal pattern prediction
impl ContextPredictionEngine for RustCPELayer {
    async fn predict_next(&self, context: &ContextWindow) -> LayerResult<Vec<PredictionResult>> { ... }
    async fn learn_pattern(&mut self, sequence: &[MemoryAccess]) -> LayerResult<()> { ... }
}
```

## 📊 Performance Features

### Real-time Monitoring
```rust
let health = orchestrator.health_check().await;
for (layer_id, health_status) in health {
    println!("{}: {:?} ({}s uptime)", 
        layer_id.as_str(), 
        health_status.status,
        health_status.uptime_seconds
    );
}
```

### Performance Metrics
```rust
let performance = orchestrator.get_performance_stats();
println!("Average query time: {}μs", 
    performance.total_query_time_us / performance.query_count);
```

## 🛠️ Routing Strategies

### Sequential Routing (Default)
```rust
// L1 → L2 → L3 → L4 (stop on exact match)
RoutingStrategy::Sequential
```

### Parallel Routing
```rust
// Query all layers simultaneously, merge results
RoutingStrategy::Parallel  
```

### Adaptive Routing
```rust
// Smart routing based on query analysis and history
RoutingStrategy::Adaptive
```

### Custom Routing
```rust
// User-defined routing logic
RoutingStrategy::Custom(|query| {
    if query.embedding.is_some() {
        vec![LayerId::Layer2, LayerId::Layer3]
    } else {
        vec![LayerId::Layer1, LayerId::Layer3]
    }
})
```

## 🚀 Getting Started

Add to your `Cargo.toml`:

```toml
[dependencies]
mfn-core = "0.1.0"
tokio = { version = "1.35", features = ["full"] }
```

See the [examples](examples/) directory for complete implementations and integration patterns.

## 🏆 Performance Benchmarks

| Operation | Layer 1 (Exact) | Layer 2 (Neural) | Layer 3 (Graph) | Layer 4 (Context) |
|-----------|------------------|------------------|------------------|-------------------|
| Single Query | ~1μs | ~50μs | ~10μs | ~100μs |
| Batch (100) | ~10μs | ~200μs | ~50μs | ~500μs |
| Memory Adds | ~0.5μs | ~20μs | ~5μs | ~30μs |

*Benchmarks on consumer hardware (Intel i7, 16GB RAM)*

## 🔗 Related Projects

- **[mfn-layer1-zig](https://github.com/TheAgencyInstitute/mfn-layer1-zig)** - Zig implementation of IFR
- **[mfn-layer2-rust](https://github.com/TheAgencyInstitute/mfn-layer2-rust)** - Rust spiking neural network DSR  
- **[mfn-layer3-go](https://github.com/TheAgencyInstitute/mfn-layer3-go)** - Go graph-based ALM
- **[mfn-layer4-rust](https://github.com/TheAgencyInstitute/mfn-layer4-rust)** - Rust context prediction CPE
- **[mfn-integration](https://github.com/TheAgencyInstitute/mfn-integration)** - Full system integration and examples

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🤝 Contributing

Contributions are welcome! Please read our [Contributing Guide](CONTRIBUTING.md) for details on our code of conduct and the process for submitting pull requests.

## 🌟 Acknowledgments

- Inspired by biological neural networks and associative memory research
- Built with modern async Rust for maximum performance
- Designed for the next generation of AI memory systems

---

**Memory Flow Network** - *Treating memories as packets in a specialized processing network*