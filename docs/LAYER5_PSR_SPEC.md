# MFN Layer 5: Pattern Structure Registry (PSR) Specification

## Overview

Layer 5 (PSR) is the Pattern Structure Registry for APEX. It stores and retrieves structural pattern templates, enabling pattern-aware learning across the MFN ecosystem.

## Architecture

### Socket
- **Path**: `/tmp/mfn_layer5.sock`
- **Protocol**: Binary (length-prefixed JSON)
- **Language**: Rust (recommended for performance + memory safety)

### Key Responsibilities
1. **Pattern Storage**: Store pattern templates with embeddings
2. **Pattern Retrieval**: Fast similarity search over patterns
3. **Pattern Composition**: Track pattern relationships (P ∘ Q)
4. **Pattern Validation**: Cross-validate patterns across data shards
5. **Pattern Evolution**: Track pattern usage and lifecycle

## Data Structures

###  Pattern
```rust
struct Pattern {
    id: String,                    // Unique pattern ID
    name: String,                  // Human-readable name
    category: PatternCategory,     // temporal, spatial, transformational, relational
    embedding: Vec<f32>,           // 256-dim embedding

    // Composition
    source_patterns: Vec<String>,  // Parent pattern IDs (for P∘Q)
    composable_with: Vec<String>,  // Compatible patterns

    // Structure
    slots: HashMap<String, TypeConstraint>,
    constraints: Vec<Predicate>,
    domain: PatternType,
    codomain: PatternType,

    // Examples
    text_example: String,
    image_example: String,
    audio_example: String,
    code_example: String,

    // Metadata
    activation_count: u64,         // Usage counter
    confidence: f32,               // Confidence score [0, 1]
    first_seen_step: u64,          // Training step discovered
    last_used_step: u64,           // Last usage
    created_at: u64,               // Timestamp
}

enum PatternCategory {
    Temporal,
    Spatial,
    Transformational,
    Relational,
}

enum PatternType {
    Sequence,
    Set,
    Tree,
    Graph,
    Any,
}

struct TypeConstraint {
    type_name: String,
    nullable: bool,
}

struct Predicate {
    condition: String,
}
```

## API Operations

### 1. Store Pattern
```json
{
    "type": "store_pattern",
    "request_id": "uuid",
    "pattern": {
        "id": "pattern_123",
        "name": "Recursion",
        "category": "transformational",
        "embedding": [0.1, 0.2, ...],
        "source_patterns": ["pattern_5", "pattern_12"],
        "slots": {
            "base_case": {"type_name": "predicate", "nullable": false},
            "recursive_step": {"type_name": "callable", "nullable": false}
        },
        "constraints": [
            {"condition": "base_case is reachable"},
            {"condition": "termination_measure decreases"}
        ],
        "composable_with": ["hierarchy", "sequence"],
        "text_example": "factorial(n) = 1 if n<=1 else n*factorial(n-1)",
        "activation_count": 0,
        "confidence": 1.0,
        "first_seen_step": 0
    }
}
```

**Response**:
```json
{
    "type": "store_response",
    "request_id": "uuid",
    "success": true,
    "pattern_id": "pattern_123"
}
```

### 2. Search Patterns
```json
{
    "type": "search_patterns",
    "request_id": "uuid",
    "query_embedding": [0.1, 0.2, ...],  // 256-dim
    "top_k": 5,
    "min_confidence": 0.5
}
```

**Response**:
```json
{
    "type": "search_response",
    "request_id": "uuid",
    "success": true,
    "patterns": [
        {
            "pattern_id": "pattern_123",
            "similarity": 0.95,
            "pattern": { /* full pattern object */ }
        },
        ...
    ]
}
```

### 3. Get Pattern
```json
{
    "type": "get_pattern",
    "request_id": "uuid",
    "pattern_id": "pattern_123"
}
```

**Response**:
```json
{
    "type": "get_response",
    "request_id": "uuid",
    "success": true,
    "pattern": { /* full pattern object */ }
}
```

### 4. Update Pattern Stats
```json
{
    "type": "update_stats",
    "request_id": "uuid",
    "pattern_id": "pattern_123",
    "activation_count_delta": 1,
    "last_used_step": 50000
}
```

**Response**:
```json
{
    "type": "update_response",
    "request_id": "uuid",
    "success": true
}
```

### 5. List Patterns
```json
{
    "type": "list_patterns",
    "request_id": "uuid",
    "category": "transformational",  // optional filter
    "min_activation_count": 100,     // optional filter
    "limit": 100,
    "offset": 0
}
```

**Response**:
```json
{
    "type": "list_response",
    "request_id": "uuid",
    "success": true,
    "patterns": [ /* array of pattern objects */ ],
    "total_count": 250
}
```

### 6. Compose Patterns
```json
{
    "type": "compose_patterns",
    "request_id": "uuid",
    "pattern1_id": "sequence",
    "pattern2_id": "recursion"
}
```

**Response**:
```json
{
    "type": "compose_response",
    "request_id": "uuid",
    "success": true,
    "composed_pattern": {
        "id": "sequence_recursion",
        "embedding": [0.3, 0.4, ...],  // Hadamard product
        "source_patterns": ["sequence", "recursion"]
    }
}
```

## Performance Requirements

| Metric | Target |
|--------|--------|
| Storage latency | <1ms |
| Search latency (10K patterns) | <5ms |
| Composition latency | <0.5ms |
| Throughput | >10K ops/sec |
| Memory footprint | <1GB (for 10K patterns) |
| Concurrency | Support 100+ concurrent connections |

## Storage Backend

### In-Memory Index
- Primary store: In-memory HashMap for O(1) access
- Search index: HNSW (Hierarchical Navigable Small World) for similarity search
- Persistence: Periodic snapshots to disk (every 5 minutes or 1000 ops)

### On-Disk Persistence
- Format: MessagePack or bincode for efficiency
- Location: `/usr/lib/alembic/mfn/psr.db`
- Backup: Incremental snapshots

## Integration with APEX

### Training Phase 1 (Foundation)
- PSR stores 20 seed patterns
- No pattern retrieval (patterns loaded directly into GPU cache)

### Training Phase 2 (Pattern-aware)
- Pattern discovery pipeline creates new patterns
- New patterns stored in PSR
- GPU cache periodically synced with PSR top patterns
- Pattern usage stats tracked

### Training Phase 3 (Multimodal)
- Cross-modal patterns stored
- Pattern composition tracked
- Pattern evolution monitored

## Implementation Roadmap

### Phase 1: Core Service (Week 1)
- [ ] Rust project setup (layer5-rust-psr)
- [ ] In-memory pattern storage (HashMap)
- [ ] Basic socket server
- [ ] Store/Get/List operations
- [ ] JSON protocol handling

### Phase 2: Search & Indexing (Week 2)
- [ ] HNSW similarity search index
- [ ] Search operation with top-k
- [ ] Batch operations
- [ ] Connection pooling

### Phase 3: Composition & Stats (Week 3)
- [ ] Pattern composition logic
- [ ] Usage statistics tracking
- [ ] Pattern lifecycle management
- [ ] Validation operations

### Phase 4: Persistence & Reliability (Week 4)
- [ ] On-disk persistence
- [ ] Snapshot/restore
- [ ] WAL (Write-Ahead Log)
- [ ] Error recovery

### Phase 5: Production Hardening
- [ ] Benchmarking
- [ ] Memory profiling
- [ ] Connection limits
- [ ] Monitoring/metrics
- [ ] Integration tests with APEX

## Python Client Interface

See `mfn_psr_client.py` for reference implementation.

## Testing

### Unit Tests
- Pattern CRUD operations
- Similarity search correctness
- Composition logic
- Serialization/deserialization

### Integration Tests
- APEX training integration
- Pattern discovery pipeline
- Multi-client concurrency
- Fault tolerance

### Performance Tests
- Search latency under load
- Throughput benchmarks
- Memory usage
- Connection pooling

## Security Considerations

1. **Input Validation**: Validate all pattern data (embedding dimensions, ID format, etc.)
2. **Resource Limits**: Max pattern size, max patterns per client, connection limits
3. **Socket Permissions**: Restrict Unix socket to alembic user/group
4. **DoS Protection**: Rate limiting, connection timeouts
5. **Data Integrity**: Checksums on persisted data

## Monitoring

### Metrics to Track
- Total patterns stored
- Search queries per second
- Average search latency (p50, p95, p99)
- Pattern composition rate
- Memory usage
- Active connections
- Error rate

### Logging
- All pattern storage operations
- Failed searches
- Composition failures
- Performance anomalies

## Compatibility

- **MFN Core**: Uses mfn-core types for consistency
- **Layers 1-4**: PSR is independent but follows same socket/protocol patterns
- **APEX**: Native integration via GPU cache + PSR sync
- **Future Models**: Generic pattern storage for any architecture

## Notes

- PSR is storage-focused, not compute-focused (learned from 108GB disaster)
- All pattern *search* compute happens in PSR service
- APEX only sends embeddings and receives results
- Pattern cache on GPU stays at 10K for performance
- PSR can store unlimited patterns (RAM/disk permitting)
