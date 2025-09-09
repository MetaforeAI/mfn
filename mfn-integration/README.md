# MFN Integration Examples

This directory contains integration examples demonstrating the complete Memory Flow Network (MFN) system working across all 4 layers:

- **Layer 1**: Zig IFR (Immediate Flow Registry) - Ultra-fast exact matching
- **Layer 2**: Rust DSR (Dynamic Similarity Registry) - Neural similarity processing  
- **Layer 3**: Go ALM (Associative Link Mesh) - Graph-based associations
- **Layer 4**: Rust CPE (Context Prediction Engine) - Temporal pattern analysis

## Architecture Overview

```
Memory Query → Layer 1 → Layer 2 → Layer 3 → Layer 4 → Predictions
     ↓            ↓        ↓        ↓        ↓
   Exact       Neural   Graph    Temporal
  Matching   Similarity Search   Patterns
```

The MFN treats memories as flowing packets through optimized processing layers, with each layer using the best language for its computational requirements.

## Examples

1. **`basic_flow.rs`** - Simple memory flow through all layers
2. **`benchmarks.rs`** - Performance benchmarking across layers
3. **`integration_test.rs`** - End-to-end system validation
4. **`realworld_demo.rs`** - Realistic usage scenarios

## Performance Targets

- **Layer 1**: Sub-microsecond exact matching
- **Layer 2**: ~1ms neural similarity processing
- **Layer 3**: ~5ms graph traversal and association
- **Layer 4**: ~10ms context prediction generation

## Building & Running

```bash
# Build all layers
./build_all.sh

# Run integration tests
cargo test --bin integration_test

# Run benchmarks
cargo run --bin benchmarks --release

# Run demo
cargo run --bin realworld_demo
```

## Dependencies

Each layer runs as a separate process communicating via:
- FFI interfaces for direct calls
- HTTP APIs for network communication  
- Shared memory for high-performance data exchange

The integration layer orchestrates these communications seamlessly.