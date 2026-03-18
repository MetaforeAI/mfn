# MFN (Memory Flow Network) User Guide

**Version:** 1.0
**Date:** 2025-11-04
**Status:** Alpha Testing

## Table of Contents

1. [Introduction](#introduction)
2. [Quick Start](#quick-start)
3. [Core Concepts](#core-concepts)
4. [Installation](#installation)
5. [Basic Usage](#basic-usage)
6. [Advanced Features](#advanced-features)
7. [API Reference](#api-reference)
8. [Performance Tuning](#performance-tuning)
9. [Troubleshooting](#troubleshooting)

---

## Introduction

The Memory Flow Network (MFN) is a sophisticated multi-layer memory processing system that provides four types of memory retrieval:

1. **Exact Matching** (Layer 1 - IFR): Hash-based exact content lookup
2. **Similarity Search** (Layer 2 - DSR): Spiking neural network for similar memories
3. **Associative Memory** (Layer 3 - ALM): Graph-based relationship traversal
4. **Temporal Prediction** (Layer 4 - CPE): Pattern-based next-memory prediction

### What MFN Does

MFN routes memory queries through multiple layers to find the most relevant memories:

- **Fast exact lookups** when you know what you're looking for
- **Similarity detection** when you need "close enough" matches
- **Association discovery** when exploring related concepts
- **Future prediction** when anticipating next steps

---

## Quick Start

### 5-Minute Tutorial

```rust
use mfn_core::{MfnOrchestrator, OrchestratorConfig};
use mfn_core::{UniversalMemory, UniversalSearchQuery, MemoryId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create orchestrator with default config
    let config = OrchestratorConfig::default();
    let mut orchestrator = MfnOrchestrator::new(config);

    // 2. Store a memory
    let memory = UniversalMemory {
        id: MemoryId(1),
        content: "Hello World".to_string(),
        embedding: None,
        tags: vec!["greeting".to_string()],
        metadata: Default::default(),
        timestamp: mfn_core::current_timestamp(),
    };

    orchestrator.add_memory(memory).await?;

    // 3. Search for it
    let query = UniversalSearchQuery {
        start_memory_ids: vec![],
        content: Some("Hello".to_string()),
        embedding: None,
        tags: vec![],
        association_types: vec![],
        max_depth: 3,
        max_results: 10,
        min_weight: 0.5,
        timeout_us: 5_000_000, // 5 seconds
        layer_params: Default::default(),
    };

    let results = orchestrator.search(&query).await?;
    println!("Found {} results", results.len());

    Ok(())
}
```

---

## Core Concepts

### Memory Structure

Every memory in MFN has:

```rust
pub struct UniversalMemory {
    pub id: MemoryId,              // Unique identifier
    pub content: String,            // Text content
    pub embedding: Option<Vec<f32>>, // Optional vector embedding
    pub tags: Vec<String>,          // Classification tags
    pub metadata: HashMap<String, serde_json::Value>, // Custom fields
    pub timestamp: u64,             // Creation time (microseconds)
}
```

### Search Query

```rust
pub struct UniversalSearchQuery {
    pub start_memory_ids: Vec<MemoryId>,  // Starting points for graph traversal
    pub content: Option<String>,           // Text to search for
    pub embedding: Option<Vec<f32>>,       // Vector for similarity search
    pub tags: Vec<String>,                 // Filter by tags
    pub association_types: Vec<AssociationType>, // Relationship types to follow
    pub max_depth: usize,                  // Maximum traversal depth
    pub max_results: usize,                // Result limit
    pub min_weight: Weight,                // Minimum association strength (0.0-1.0)
    pub timeout_us: u64,                   // Query timeout in microseconds
    pub layer_params: HashMap<String, serde_json::Value>, // Layer-specific params
}
```

### Association Types

MFN supports 9 types of associations:

```rust
pub enum AssociationType {
    Causal,      // A causes B
    Temporal,    // A happens before/after B
    Spatial,     // A is near B
    Similarity,  // A is similar to B
    Category,    // A belongs to category B
    PartWhole,   // A is part of B
    Dependency,  // A depends on B
    Inference,   // A implies B
    Custom(String), // User-defined
}
```

---

## Installation

### Prerequisites

- Rust 1.70+ (for building from source)
- Docker (optional, for containerized deployment)
- 4GB+ RAM recommended

### From Source

```bash
git clone https://github.com/NeoTecDigital/telepathy.git
cd telepathy
cargo build --release
```

### Using Docker

```bash
docker-compose up -d
```

This starts all 4 layers as separate services.

---

## Basic Usage

### Creating an Orchestrator

```rust
use mfn_core::{MfnOrchestrator, OrchestratorConfig, RoutingStrategy};

let config = OrchestratorConfig {
    routing_strategy: RoutingStrategy::Adaptive, // or Sequential, Parallel
    layer_timeout_us: 5_000_000, // 5 seconds per layer
    enable_performance_monitoring: true,
    enable_caching: true,
    cache_size: 10000,
};

let orchestrator = MfnOrchestrator::new(config);
```

### Storing Memories

```rust
// Simple text memory
let memory = UniversalMemory {
    id: MemoryId(42),
    content: "Rust is a systems programming language".to_string(),
    embedding: None,
    tags: vec!["programming".to_string(), "rust".to_string()],
    metadata: Default::default(),
    timestamp: current_timestamp(),
};

orchestrator.add_memory(memory).await?;

// Memory with embedding (for similarity search)
let embedding = vec![0.1, 0.2, 0.3, /* ... */]; // 768-dim typical
let memory_with_embedding = UniversalMemory {
    id: MemoryId(43),
    content: "Python is a high-level language".to_string(),
    embedding: Some(embedding),
    tags: vec!["programming".to_string(), "python".to_string()],
    metadata: Default::default(),
    timestamp: current_timestamp(),
};

orchestrator.add_memory(memory_with_embedding).await?;
```

### Creating Associations

```rust
use mfn_core::{UniversalAssociation, AssociationType};

let association = UniversalAssociation {
    id: "rust-python-similarity".to_string(),
    from_memory_id: MemoryId(42),
    to_memory_id: MemoryId(43),
    association_type: AssociationType::Similarity,
    weight: 0.85, // 85% similar
    metadata: Default::default(),
    timestamp: current_timestamp(),
};

orchestrator.add_association(association).await?;
```

### Searching Memories

#### Exact Match

```rust
let query = UniversalSearchQuery {
    content: Some("Rust is a systems programming language".to_string()),
    max_results: 1,
    ..Default::default()
};

let results = orchestrator.search(&query).await?;
// Layer 1 (IFR) finds exact match in microseconds
```

#### Similarity Search

```rust
let query_embedding = vec![0.1, 0.2, 0.31, /* ... */]; // Similar to stored embedding

let query = UniversalSearchQuery {
    embedding: Some(query_embedding),
    max_results: 10,
    min_weight: 0.7, // 70% similarity threshold
    ..Default::default()
};

let results = orchestrator.search(&query).await?;
// Layer 2 (DSR) uses spiking neural network for similarity
```

#### Associative Search

```rust
let query = UniversalSearchQuery {
    start_memory_ids: vec![MemoryId(42)], // Start from "Rust" memory
    association_types: vec![AssociationType::Similarity],
    max_depth: 2, // Follow associations 2 levels deep
    max_results: 20,
    ..Default::default()
};

let results = orchestrator.search(&query).await?;
// Layer 3 (ALM) traverses relationship graph
```

#### Temporal Prediction

```rust
// First, access some memories in sequence to build pattern
orchestrator.get_memory(MemoryId(1)).await?;
orchestrator.get_memory(MemoryId(2)).await?;
orchestrator.get_memory(MemoryId(3)).await?;

// Now ask: what's likely next?
let query = UniversalSearchQuery {
    content: Some("predict_next".to_string()),
    layer_params: {
        let mut params = HashMap::new();
        params.insert("recent_sequence".to_string(),
                     json!([1, 2, 3]));
        params
    },
    max_results: 5,
    ..Default::default()
};

let predictions = orchestrator.search(&query).await?;
// Layer 4 (CPE) predicts likely next memories
```

---

## Advanced Features

### Routing Strategies

MFN offers three routing strategies:

#### Sequential (Default)
Queries each layer in order until match found.

```rust
config.routing_strategy = RoutingStrategy::Sequential;
```

**Pros:** Predictable, efficient for exact matches
**Cons:** Slower if exact match not available

#### Parallel
Queries all layers simultaneously.

```rust
config.routing_strategy = RoutingStrategy::Parallel;
```

**Pros:** Fastest total search, comprehensive results
**Cons:** Higher resource usage

#### Adaptive
Intelligently chooses layers based on query characteristics.

```rust
config.routing_strategy = RoutingStrategy::Adaptive;
```

**Pros:** Optimized for query type, balanced performance
**Cons:** Slightly more complex overhead

### Performance Monitoring

```rust
let performance = orchestrator.get_performance_metrics().await?;

println!("Total queries: {}", performance.total_queries);
println!("Average latency: {}μs", performance.avg_latency_us);
println!("Cache hit rate: {:.2}%", performance.cache_hit_rate * 100.0);

// Per-layer stats
for (layer_id, stats) in performance.layer_stats {
    println!("Layer {:?}: {} queries, {}μs avg",
             layer_id, stats.total_queries, stats.avg_time_us);
}
```

### Health Monitoring

```rust
let health = orchestrator.health_check().await?;

match health.status {
    HealthStatus::Healthy => println!("✓ All systems operational"),
    HealthStatus::Degraded => println!("⚠ Some layers degraded"),
    HealthStatus::Unhealthy => println!("✗ System unhealthy"),
    _ => {}
}

// Check individual layers
for layer_health in health.layer_health {
    println!("{:?}: {:?}", layer_health.layer_id, layer_health.status);
}
```

---

## API Reference

### Core Types

See [API_REFERENCE.md](API_REFERENCE.md) for complete API documentation.

**Key modules:**
- `mfn_core::memory_types` - Memory and association definitions
- `mfn_core::orchestrator` - Main orchestrator
- `mfn_core::layer_interface` - Layer trait and routing

---

## Performance Tuning

### Layer-Specific Optimization

#### Layer 1 (IFR) - Exact Matching
- **Bloom Filter Size:** Increase for lower false positive rate
- **Hash Functions:** 3-5 optimal for most use cases

```rust
layer_params.insert("bloom_filter_size", json!(1_000_000));
layer_params.insert("hash_functions", json!(4));
```

#### Layer 2 (DSR) - Similarity
- **Reservoir Size:** 500-2000 neurons (more = better accuracy, slower)
- **Leak Rate:** 0.1-0.3 (higher = faster forgetting)

```rust
layer_params.insert("reservoir_size", json!(1000));
layer_params.insert("leak_rate", json!(0.2));
```

#### Layer 3 (ALM) - Associations
- **Max Depth:** 2-4 (higher = more results, slower)
- **Min Weight:** 0.5-0.8 (higher = fewer, better results)

```rust
query.max_depth = 3;
query.min_weight = 0.7;
```

#### Layer 4 (CPE) - Predictions
- **Window Size:** 100-1000 recent accesses
- **N-gram Length:** 3-5 for best predictions

```rust
layer_params.insert("max_window_size", json!(500));
layer_params.insert("max_ngram_length", json!(4));
```

### Caching

```rust
config.enable_caching = true;
config.cache_size = 50000; // Number of cached results
config.cache_ttl_seconds = 300; // 5 minutes
```

### Concurrency

```rust
config.max_concurrent_queries = 100;
config.layer_timeout_us = 10_000_000; // 10 seconds
```

---

## Troubleshooting

### Common Issues

#### "Layer timeout exceeded"
**Cause:** Query took too long
**Solution:** Increase `layer_timeout_us` or reduce query complexity

```rust
config.layer_timeout_us = 15_000_000; // 15 seconds
query.max_depth = 2; // Reduce from 4
```

#### "Memory not found"
**Cause:** Memory ID doesn't exist
**Solution:** Verify memory was stored successfully

```rust
match orchestrator.get_memory(id).await {
    Ok(memory) => println!("Found: {}", memory.content),
    Err(e) => println!("Error: {}", e),
}
```

#### "Similarity search returns no results"
**Cause:** Embeddings not provided or threshold too high
**Solution:** Ensure embeddings are set and lower threshold

```rust
query.embedding = Some(compute_embedding(&text));
query.min_weight = 0.5; // Lower threshold
```

#### "High latency on parallel queries"
**Cause:** Resource contention
**Solution:** Switch to Adaptive routing or limit concurrency

```rust
config.routing_strategy = RoutingStrategy::Adaptive;
config.max_concurrent_queries = 50; // Reduce from 100
```

### Debug Mode

```rust
std::env::set_var("RUST_LOG", "mfn=debug");
env_logger::init();

// Now orchestrator logs detailed operations
let results = orchestrator.search(&query).await?;
```

### Getting Help

- **GitHub Issues:** https://github.com/NeoTecDigital/telepathy/issues
- **Documentation:** https://docs.rs/mfn-core
- **Examples:** `examples/` directory in repository

---

## Next Steps

1. **Read the [API Reference](API_REFERENCE.md)** for detailed function documentation
2. **Explore examples** in `/examples` directory
3. **Check performance benchmarks** in `/benches` directory
4. **Review architecture** in `docs/architecture/`

---

**MFN - Memory Flow Network**
*Intelligent Multi-Layer Memory Processing*

Version 1.0 | 2025-11-04 | Alpha Testing
