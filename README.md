# MemFlow

**Stop Depending on LLMs to Remember.**

MemFlow is a 5-layer cognitive memory architecture for AI applications. Unlike vector databases that only do similarity search, MemFlow provides recall, association, prediction, and pattern recognition — the way human memory actually works.

## Why MemFlow?

| Capability | Vector DBs (Pinecone, Weaviate, Qdrant) | MemFlow |
|-----------|----------------------------------------|---------|
| Similarity Search | Yes | Yes (Layer 2 — DSR) |
| Sub-millisecond Recall | No | Yes (Layer 1 — IFR) |
| Associative Linking | No | Yes (Layer 3 — ALM) |
| Temporal Prediction | No | Yes (Layer 4 — CPE) |
| Pattern Synthesis | No | Yes (Layer 5 — PSR) |
| Multi-model Concurrent Access | No | Yes (non-blocking shared memory) |

Vector databases are similarity search engines. MemFlow is a **memory system** — it recalls, associates, predicts, and recognizes patterns across experiences.

## Quick Start

### Python SDK

```python
from mfn_client import MFNClient

client = MFNClient()

# Store a memory across layers
client.ifr_add_memory(
    pool_id="my_app",
    content="The user prefers dark mode and concise responses",
    tags=["preferences", "ui"]
)

# Search by association
results = client.alm_search(
    pool_id="my_app",
    query="user interface preferences",
    limit=5
)

# Predict what comes next in a sequence
predictions = client.cpe_predict(
    current_context=["login", "settings", "theme"],
    sequence_length=3
)

# Find similar patterns
matches = client.psr_similarity_search(
    embedding=[0.1, 0.2, ...],  # 256-dim vector
    top_k=5,
    min_confidence=0.3
)

client.close()
```

### REST API

```bash
# Store a memory
curl -X POST http://localhost:8080/v1/memory \
  -H "Content-Type: application/json" \
  -d '{"content": "User prefers dark mode", "tags": ["preferences"], "pool_id": "my_app"}'

# Search memories
curl -X POST http://localhost:8080/v1/memory/search \
  -H "Content-Type: application/json" \
  -d '{"query": "user preferences", "top_k": 5, "pool_id": "my_app"}'

# Predict next context
curl -X POST http://localhost:8080/v1/predict \
  -H "Content-Type: application/json" \
  -d '{"current_context": ["login", "settings", "theme"], "sequence_length": 3}'

# Health check
curl http://localhost:8080/health
```

### Self-Hosted (Docker)

```bash
git clone https://github.com/NeoTecDigital/MFN.git
cd MFN
docker-compose up -d
```

### Self-Hosted (Bare Metal)

```bash
# Build all layers
cargo build --release

# Start each layer (separate terminals or use the start script)
./scripts/start_all_layers.sh

# Start the API gateway
MFN_API_PORT=8080 ./target/release/mfn-gateway
```

## Architecture

MemFlow processes memories through five specialized layers, each implemented in the language best suited to its task:

| Layer | Name | Language | Purpose |
|-------|------|----------|---------|
| 1 | IFR | Zig | **Instant Flow Registry** — Sub-millisecond hot cache recall |
| 2 | DSR | Rust | **Dynamic Similarity Reservoir** — Vector similarity search |
| 3 | ALM | Go | **Associative Link Memory** — Graph-based memory association |
| 4 | CPE | Rust | **Context Prediction Engine** — Temporal sequence prediction |
| 5 | PSR | Rust | **Pattern Synthesis Registry** — Higher-order pattern recognition |

Layers communicate via Unix domain sockets with a lightweight binary protocol. Each layer can be scaled independently.

Multiple AI models can read and write to the same memory space concurrently without blocking — enabling shared cognitive workspaces across agents.

## Documentation

- [API Reference](API_REFERENCE.md) — Complete type and method documentation
- [Protocol Specification](PROTOCOL_SPECIFICATION.md) — Wire protocol for direct socket access
- [Architecture Deep Dive](docs/architecture/) — System design and layer interactions
- [User Guide](USER_GUIDE.md) — Detailed usage patterns and examples

## Configuration

All configuration is via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `MFN_API_PORT` | `8080` | REST API gateway port |
| `MFN_DATA_DIR` | `./data/mfn/memory` | Persistence data directory |
| `MFN_CORS_ORIGINS` | `http://localhost:3000` | Allowed CORS origins (comma-separated) |
| `MFN_SOCKET_PATH` | `/tmp/mfn_test_layer{N}.sock` | Override socket path per layer |

## License

[Business Source License 1.1](LICENSE) (BUSL-1.1)

Source-available. Free for development, testing, and internal production use. Commercial Memory Service offerings require a license. Each release becomes Apache 2.0 after four years.

For commercial licensing, contact licensing@metafore.io.

## Built by [MetaFore](https://metafore.io)
