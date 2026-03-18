# Telepathy - Memory Flow Network (MFN)

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Development](https://img.shields.io/badge/status-alpha-yellow.svg)]()

A multi-layer memory architecture that treats memories as network packets flowing through specialized processing layers.

## 🚀 Quick Start

See **[USER_GUIDE.md](USER_GUIDE.md)** for comprehensive usage instructions.

### Basic Usage

```rust
use mfn_core::{MfnOrchestrator, OrchestratorConfig, UniversalMemory, UniversalSearchQuery};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create orchestrator
    let config = OrchestratorConfig::default();
    let mut orchestrator = MfnOrchestrator::new(config);

    // Store a memory
    let memory = UniversalMemory {
        id: MemoryId(1),
        content: "Hello World".to_string(),
        embedding: None,
        tags: vec!["greeting".to_string()],
        metadata: Default::default(),
        timestamp: mfn_core::current_timestamp(),
    };
    orchestrator.add_memory(memory).await?;

    // Search for it
    let query = UniversalSearchQuery {
        content: Some("Hello".to_string()),
        max_results: 10,
        ..Default::default()
    };
    let results = orchestrator.search(&query).await?;

    Ok(())
}
```

## 📊 Implementation Status - Alpha Testing

**System Health: Active Development**

### Layer Status

| Layer | Status | Notes |
|-------|--------|-------|
| **Layer 1 (Zig IFR)** | ✅ Implemented | Hash-based exact matching, socket ready, persistence added |
| **Layer 2 (Rust DSR)** | ✅ Operational | Spiking neural network, fully integrated, persistence complete |
| **Layer 3 (Go ALM)** | ✅ Operational | Graph-based memory, fully integrated, persistence complete |
| **Layer 4 (Rust CPE)** | ✅ Operational | Temporal prediction, integrated, persistence complete |
| **Layer 5 (Rust PSR)** | ✅ Complete | Pattern structure registry, full implementation (39 tests passing) |

### Feature Status

| Feature | Status | Notes |
|---------|--------|-------|
| **Socket Communication** | ✅ Working | Binary protocol, Layers 1-4 connected |
| **Sequential Routing** | ✅ Working | Orchestrator routes queries successfully |
| **Parallel Routing** | ✅ Implemented | Concurrent queries to multiple layers (4x speedup) |
| **Adaptive Routing** | ✅ Implemented | Query analysis with 5 routing strategies |
| **Health Checks** | ❌ Sprint 4 | Not yet implemented |
| **Connection Pooling** | ❌ Sprint 5 | Creating new connections per query |
| **Monitoring** | ❌ Sprint 5 | No Prometheus/Grafana integration |

### What's Working
- ✅ 5-layer memory flow architecture (all layers complete)
- ✅ Socket-based inter-layer communication (binary protocol)
- ✅ Similarity search using liquid state machines (Layer 2)
- ✅ Graph-based associative memory (Layer 3)
- ✅ Temporal pattern prediction (Layer 4)
- ✅ Pattern structure registry with cosine similarity search (Layer 5)
- ✅ AOF + LMDB persistence across all 5 layers
- ✅ Sequential query routing
- ✅ Real performance measurement (~1,000 req/s)

### Known Limitations
See **[KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md)** for complete details on:
- Placeholder code and incomplete features
- Performance optimization opportunities
- Production readiness requirements
- Sprint 4-6 roadmap to 95% completion

**Current Status:**
This is an alpha-stage experimental system demonstrating multi-layer memory architecture. Core functionality is operational, but production features (monitoring, retry logic, health checks) are in progress.

**Production Timeline:** 4-6 weeks (Sprints 4-6)

### System Requirements

**Hardware**:
- CPU: x86_64 architecture (4+ cores recommended)
- RAM: 8GB minimum, 16GB recommended
- Disk: 20GB available space

**Software**:
- OS: Linux (kernel 5.10+)
- Docker: 20.10+ (for containerized deployment)
- Rust: 1.70+ (for building from source)

**Network**: Unix domain sockets for inter-layer communication (no external network required for core operation)

## 🏗️ Architecture

### Five Specialized Layers

1. **Layer 1 (IFR)** - Immediate Facility Registry
   - Zig implementation
   - Hash-based exact matching with bloom filters
   - Microsecond-level lookups
   - AOF persistence for crash recovery

2. **Layer 2 (DSR)** - Dynamic Similarity Reservoir
   - Rust implementation
   - Spiking neural network with liquid state machines
   - Victor-Purpura spike distance for similarity
   - Hebbian learning for pattern formation
   - AOF + LMDB persistence

3. **Layer 3 (ALM)** - Associative Link Matrix
   - Go implementation
   - Graph-based relationship traversal
   - 9 association types (Causal, Temporal, Spatial, etc.)
   - Concurrent depth-based search
   - AOF + LMDB persistence

4. **Layer 4 (CPE)** - Context Prediction Engine
   - Rust implementation
   - N-gram frequency analysis
   - Markov chain transition probabilities
   - Statistical temporal modeling
   - AOF + LMDB persistence

5. **Layer 5 (PSR)** - Pattern Structure Registry
   - Rust implementation
   - Pattern template storage with 256-dim embeddings
   - Cosine similarity search (linear scan, HNSW pending)
   - Pattern composition via Hadamard product
   - AOF + LMDB persistence
   - 39 tests passing (100% coverage)

### Orchestrator

Central coordinator that:
- Routes queries through appropriate layers
- Implements Sequential, Parallel, and Adaptive strategies
- Monitors performance and health
- Caches results for optimization

## 📚 Documentation

- **[USER_GUIDE.md](USER_GUIDE.md)** - **START HERE** - Complete usage guide
- **[API_REFERENCE.md](API_REFERENCE.md)** - Complete API documentation
- **[PROTOCOL_SPECIFICATION.md](PROTOCOL_SPECIFICATION.md)** - Protocol specification
- **[docs/architecture/](docs/architecture/)** - Architecture diagrams and design
- **[docs/guides/](docs/guides/)** - Implementation guides

## 🎯 Key Features

### Memory-as-Flow Paradigm
- Memories treated as packets flowing through specialized layers
- Each layer optimized for different retrieval types
- Intelligent routing based on query characteristics

### Neural-Graph Hybrid
- **Layer 2**: Spiking neural networks with reservoir computing
- **Layer 3**: Graph-based associative memory
- **Integration**: Neural similarity feeds graph associations

### Language-Optimized Layers
- **Zig**: Ultra-fast exact matching (Layer 1)
- **Rust**: Zero-cost neural computations (Layers 2, 4)
- **Go**: Concurrent graph processing (Layer 3)

## 📈 Performance

| Layer | Operation | Performance |
|-------|-----------|-------------|
| Layer 1 (IFR) | Exact Match | ~0.5μs |
| Layer 2 (DSR) | Encoding | ~159ns |
| Layer 2 (DSR) | Reservoir Update | ~109ns |
| Layer 2 (DSR) | Similarity Search | <2ms |
| Layer 3 (ALM) | Graph Search (socket) | ~0.13ms |
| Orchestrator | Routing | <200μs |

**Note**: Numbers shown are socket IPC latencies (validated via integration tests).
End-to-end system latency: 90-130µs for multi-layer queries.
System throughput: ~1,000 req/s (validated).

**Status**: Alpha testing

## 🛠️ Development

### Build

```bash
# Build all components
cargo build --release

# Run tests
cargo test --workspace --lib
cargo test --package layer4-rust-cpe --tests

# Run with Docker
docker-compose up -d
```

### Project Structure

```
telepathy/
├── mfn-core/              # Core orchestrator and types
├── layer1-zig-ifr/        # Zig exact matching layer
├── layer2-rust-dsr/       # Rust similarity layer (AOF + LMDB)
├── layer3-go-alm/         # Go associative layer (AOF + LMDB)
├── layer4-rust-cpe/       # Rust prediction layer (AOF + LMDB)
├── layer5-rust-psr/       # Rust pattern registry (AOF + LMDB)
├── mfn-integration/       # Integration utilities
└── USER_GUIDE.md          # Complete usage documentation
```

## 🤝 Contributing

1. Fork the repository
2. Create feature branch (`git checkout -b feature/amazing-feature`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing-feature`)
5. Open Pull Request

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🌟 Acknowledgments

- Biological neural network research for spiking network inspiration
- Graph theory and associative memory literature
- High-performance computing communities in Rust, Go, and Zig

---

**Memory Flow Network** - *Experimental Multi-Layer Memory Architecture*

*Built with ❤️ by NeoTec Digital*
