# MFN 5-Layer Socket Protocol Documentation

Complete protocol documentation for all 5 MFN layers, enabling full client implementations.

## Overview

All layers use Unix domain sockets with binary protocol support:
- **Layer 1 IFR**: `/tmp/mfn_layer1.sock` (Zig)
- **Layer 2 DSR**: `/tmp/mfn_layer2.sock` (Rust)
- **Layer 3 ALM**: `/tmp/mfn_layer3.sock` (Go)
- **Layer 4 CPE**: `/tmp/mfn_layer4.sock` (Rust)
- **Layer 5 PSR**: `/tmp/mfn_layer5.sock` (Rust)

## Common Protocol Features

### Binary Protocol (All Layers)
- **Length Prefix**: 4 bytes (u32, little-endian) indicating JSON payload length
- **JSON Payload**: UTF-8 encoded JSON message
- **Connection Handling**: Persistent connections with timeout
- **Pool Support**: Layers 2, 3, 4 support multi-pool via `pool_id` field (default: "crucible_training")

### Message Format
```
[4 bytes: message_length][N bytes: JSON payload]
```

---

## Layer 1: IFR (Immediate Flow Registry)

**Socket**: `/tmp/mfn_layer1.sock`  
**Language**: Zig  
**Protocol**: Binary (16-byte header) + JSON compatibility  
**Performance Target**: 0.013ms exact matching

### Binary Protocol Header (16 bytes)
```zig
BinaryHeader {
    protocol_version: u8,   // 0x02
    message_type: u8,       // MSG_* constants
    request_id: u32,        // Unique request ID
    payload_length: u32,    // Payload size
    reserved: u64           // Reserved
}
```

### Message Types
- `MSG_ADD_MEMORY = 0x10`
- `MSG_QUERY_MEMORY = 0x20`
- `MSG_GET_STATS = 0x30`
- `MSG_PING = 0x40`
- `MSG_RESPONSE = 0x80`
- `MSG_ERROR = 0x90`

### JSON Request Format

#### AddMemory
```json
{
  "type": "add_memory",
  "request_id": "req-123",
  "content": "exact match content",
  "memory_data": "associated data"
}
```

**Binary Payload**: `content_len(4) + content + memory_data_len(4) + memory_data`

**Response**:
```json
{
  "type": "add_memory_response",
  "request_id": "req-123",
  "success": true,
  "memory_id_hash": 12345678,
  "evicted": false
}
```

#### Query
```json
{
  "type": "query",
  "request_id": "req-124",
  "content": "query content"
}
```

**Response**:
```json
{
  "type": "query_response",
  "request_id": "req-124",
  "success": true,
  "found_exact": true,
  "next_layer": null,
  "confidence": 1.0,
  "processing_time_ns": 13000,
  "result": "matched data"
}
```

#### GetStats
```json
{
  "type": "get_stats",
  "request_id": "req-125"
}
```

**Response**:
```json
{
  "type": "stats_response",
  "request_id": "req-125",
  "success": true,
  "total_queries": 1000,
  "exact_hits": 850,
  "hit_rate": 0.85,
  "memory_count": 5000
}
```

#### Ping
```json
{
  "type": "ping",
  "request_id": "req-126"
}
```

**Response**:
```json
{
  "type": "pong",
  "request_id": "req-126",
  "success": true
}
```

### Key Features
- **Exact Matching**: Hash-based instant retrieval
- **Bloom Filter**: Fast negative lookups
- **LRU Eviction**: Memory management with configurable limits
- **Connection Tracking**: Memory cleanup on disconnect

---

## Layer 2: DSR (Dynamic Similarity Reservoir)

**Socket**: `/tmp/mfn_layer2.sock`  
**Language**: Rust  
**Protocol**: Binary (4-byte length + JSON)  
**Performance Target**: <2ms similarity search

### Message Format
```
[4 bytes: length (u32 LE)][JSON payload]
```

### Request Types

#### AddMemory
```json
{
  "type": "AddMemory",
  "request_id": "req-200",
  "pool_id": "crucible_training",
  "memory_id": 123456,
  "embedding": [0.1, 0.2, 0.3, ...],
  "content": "memory content",
  "tags": ["tag1", "tag2"],
  "metadata": {"key": "value"}
}
```

**Response**:
```json
{
  "type": "Success",
  "request_id": "req-200",
  "data": {
    "memory_id": 123456,
    "added": true,
    "tags_count": 2,
    "metadata_count": 1
  },
  "processing_time_ms": 0.5
}
```

#### SimilaritySearch
```json
{
  "type": "SimilaritySearch",
  "request_id": "req-201",
  "pool_id": "crucible_training",
  "query_embedding": [0.1, 0.2, 0.3, ...],
  "top_k": 10,
  "min_confidence": 0.7,
  "timeout_ms": 5000
}
```

**Response**:
```json
{
  "type": "Success",
  "request_id": "req-201",
  "data": {
    "matches": [
      {
        "memory_id": 123456,
        "confidence": 0.95,
        "raw_activation": 0.87,
        "rank": 0,
        "content": "matched content"
      }
    ],
    "processing_time_ms": 1.2,
    "wells_evaluated": 5,
    "has_confident_matches": true
  },
  "processing_time_ms": 1.2
}
```

#### GetStats
```json
{
  "type": "GetStats",
  "request_id": "req-202",
  "pool_id": "crucible_training"
}
```

**Response**:
```json
{
  "type": "Success",
  "request_id": "req-202",
  "data": {
    "total_queries": 1000,
    "total_additions": 5000,
    "cache_hits": 800,
    "similarity_wells_count": 50,
    "reservoir_size": 10000,
    "average_well_activation": 0.75,
    "memory_usage_mb": 256.5,
    "max_wells": 100,
    "wells_evicted": 10,
    "connection_count": 5
  },
  "processing_time_ms": 0.1
}
```

#### OptimizeReservoir
```json
{
  "type": "OptimizeReservoir",
  "request_id": "req-203",
  "pool_id": "crucible_training"
}
```

#### Ping
```json
{
  "type": "Ping",
  "request_id": "req-204"
}
```

**Response**:
```json
{
  "type": "Pong",
  "request_id": "req-204",
  "timestamp": 1234567890,
  "layer": "Layer 2: Dynamic Similarity Reservoir",
  "version": "0.1.0"
}
```

#### HealthCheck
```json
{
  "type": "HealthCheck",
  "request_id": "req-205"
}
```

**Response**:
```json
{
  "type": "HealthCheckResponse",
  "request_id": "req-205",
  "status": "healthy",
  "layer": "Layer2_DSR",
  "timestamp": 1234567890000,
  "uptime_seconds": 3600,
  "metrics": {
    "total_queries": 1000,
    "similarity_wells_count": 50,
    "memory_usage_mb": 256.5
  }
}
```

### Advanced Binary Protocol (Optional)

Layer 2 also supports a high-performance binary protocol:

**Header** (16 bytes):
```rust
BinaryMessageHeader {
    magic: u32,           // 0x44535202 ("DSR\x02")
    version: u16,         // Protocol version (1)
    message_type: u16,    // Message type enum
    flags: u16,           // Compression, etc.
    payload_length: u32,  // Payload size
    sequence_id: u16,     // Request sequence
    _padding: u16         // Reserved
}
```

**Message Types**:
- `AddMemory = 0x0001`
- `SimilaritySearch = 0x0002`
- `GetStats = 0x0003`
- `OptimizeReservoir = 0x0004`
- `Ping = 0x0005`
- `Response = 0x8000`
- `Error = 0x8001`

**CRC32**: Appended after payload for integrity checking

### Key Features
- **Similarity Wells**: Dynamic clustering of similar embeddings
- **Reservoir Sampling**: Memory-efficient storage
- **Connection-based Cleanup**: Wells cleaned on disconnect
- **Multi-pool Support**: Isolated memory pools per `pool_id`

---

## Layer 3: ALM (Associative Link Memory)

**Socket**: `/tmp/mfn_layer3.sock`  
**Language**: Go  
**Protocol**: Binary (4-byte length + JSON)  
**Performance Target**: Graph-based associative search

### Message Format
```
[4 bytes: length (u32 LE)][JSON payload]
```

### Request Types

#### AddMemory
```json
{
  "type": "add_memory",
  "request_id": "req-300",
  "pool_id": "crucible_training",
  "content": "memory content",
  "metadata": {"key": "value"}
}
```

**Response**:
```json
{
  "type": "add_memory_response",
  "request_id": "req-300",
  "success": true,
  "processing_time_ms": 0.5,
  "metadata": {
    "memory_id": 123456789
  }
}
```

#### AddAssociation
```json
{
  "type": "add_association",
  "request_id": "req-301",
  "pool_id": "crucible_training",
  "metadata": {
    "source_id": 123456789,
    "target_id": 987654321,
    "strength": 0.8
  }
}
```

**Response**:
```json
{
  "type": "add_association_response",
  "request_id": "req-301",
  "success": true,
  "processing_time_ms": 0.3
}
```

#### Search
```json
{
  "type": "search",
  "request_id": "req-302",
  "pool_id": "crucible_training",
  "query": "search query",
  "limit": 10,
  "min_confidence": 0.5
}
```

**Response**:
```json
{
  "type": "search_response",
  "request_id": "req-302",
  "success": true,
  "results": [
    {
      "id": 123456789,
      "content": "matched content",
      "score": 0.95,
      "distance": 1,
      "metadata": {"key": "value"}
    }
  ],
  "confidence": 0.85,
  "processing_time_ms": 2.5
}
```

#### GetStats
```json
{
  "type": "get_stats",
  "request_id": "req-303",
  "pool_id": "crucible_training"
}
```

**Response**:
```json
{
  "type": "stats_response",
  "request_id": "req-303",
  "success": true,
  "processing_time_ms": 0.2,
  "metadata": {
    "pool_id": "crucible_training",
    "total_pools": 3,
    "total_memories": 10000,
    "total_associations": 50000,
    "total_queries": 1000,
    "active_connections": 5
  }
}
```

#### Ping
```json
{
  "type": "ping",
  "request_id": "req-304"
}
```

**Response**:
```json
{
  "type": "pong",
  "request_id": "req-304",
  "success": true,
  "processing_time_ms": 0.1,
  "metadata": {
    "layer": "Layer3-ALM",
    "version": "1.0.0",
    "timestamp": 1234567890
  }
}
```

#### HealthCheck
```json
{
  "type": "HealthCheck",
  "request_id": "req-305"
}
```

**Response**:
```json
{
  "status": "healthy",
  "layer": "Layer3_ALM",
  "timestamp": 1234567890000,
  "uptime_seconds": 3600,
  "metrics": {
    "pool_id": "crucible_training",
    "total_pools": 3,
    "total_memories": 10000,
    "total_associations": 50000,
    "total_queries": 1000,
    "success_rate": 1.0,
    "avg_latency_us": 500,
    "graph_density": 5.0,
    "active_connections": 5
  }
}
```

### Key Features
- **Graph-based Memory**: Memories connected via associations
- **Associative Search**: Traverse graph to find related memories
- **Connection Tracking**: Associations cleaned on disconnect
- **Multi-pool Support**: Isolated graphs per `pool_id`

---

## Layer 4: CPE (Context Prediction Engine)

**Socket**: `/tmp/mfn_layer4.sock`  
**Language**: Rust  
**Protocol**: Binary (4-byte length + JSON)  
**Performance Target**: Context prediction and temporal analysis

### Message Format
```
[4 bytes: length (u32 LE)][JSON payload]
```

### Request Types

#### AddMemoryContext
```json
{
  "type": "AddMemoryContext",
  "request_id": "req-400",
  "pool_id": "crucible_training",
  "memory_id": 123456,
  "content": "memory content",
  "context": ["previous", "surrounding", "context"]
}
```

**Response**:
```json
{
  "type": "AddMemoryContext_Response",
  "request_id": "req-400",
  "success": true,
  "data": {
    "memory_id": 123456,
    "content": "memory content",
    "context_added": 3,
    "timestamp": 1234567890,
    "connection_id": "conn_uuid"
  }
}
```

#### PredictContext
```json
{
  "type": "PredictContext",
  "request_id": "req-401",
  "pool_id": "crucible_training",
  "current_context": ["current", "context", "words"],
  "sequence_length": 5
}
```

**Response**:
```json
{
  "type": "PredictContext_Response",
  "request_id": "req-401",
  "success": true,
  "data": {
    "predictions": [
      {"memory_id": 123, "score": 0.9, "content": "predicted"}
    ],
    "context": ["current", "context", "words"],
    "predicted_sequence_length": 5
  }
}
```

#### GetContextHistory
```json
{
  "type": "GetContextHistory",
  "request_id": "req-402",
  "pool_id": "crucible_training",
  "memory_id": 123456
}
```

**Response**:
```json
{
  "type": "GetContextHistory_Response",
  "request_id": "req-402",
  "success": true,
  "data": {
    "memory_id": 123456,
    "history": [],
    "total_accesses": 0,
    "pattern_strength": 0.0
  }
}
```

#### Ping
```json
{
  "type": "Ping",
  "request_id": "req-403"
}
```

**Response**:
```json
{
  "type": "Pong",
  "request_id": "req-403",
  "success": true,
  "data": {
    "timestamp": 1234567890,
    "layer": "Layer4_CPE",
    "status": "operational"
  }
}
```

#### HealthCheck
```json
{
  "type": "HealthCheck",
  "request_id": "req-404",
  "pool_id": "crucible_training"
}
```

**Response**:
```json
{
  "type": "HealthCheck_Response",
  "request_id": "req-404",
  "success": true,
  "data": {
    "status": "healthy",
    "layer": "Layer4_CPE",
    "timestamp": 1234567890,
    "uptime_seconds": 3600,
    "pool_id": "crucible_training",
    "pool_count": 3,
    "metrics": {
      "memory_stats": {},
      "uptime_seconds": 3600,
      "pool_count": 3
    },
    "memory_info": {}
  }
}
```

#### ListPools
```json
{
  "type": "ListPools",
  "request_id": "req-405"
}
```

**Response**:
```json
{
  "type": "ListPools_Response",
  "request_id": "req-405",
  "success": true,
  "data": {
    "pools": ["crucible_training", "production"],
    "pool_count": 2
  }
}
```

### Key Features
- **Temporal Analysis**: Track memory access patterns over time
- **Context Prediction**: Predict next memories based on context
- **Connection Tracking**: Context data cleaned on disconnect
- **Multi-pool Support**: Isolated context per `pool_id`

---

## Layer 5: PSR (Pattern Synthesis Reservoir)

**Socket**: `/tmp/mfn_layer5.sock`  
**Language**: Rust  
**Status**: Implemented
**Performance Target**: <1ms pattern storage, <5ms similarity search

### Protocol
Follows the Layer 2/4 pattern:
- Binary protocol: 4-byte length + JSON
- Multi-pool support via `pool_id`
- Pattern storage and similarity operations

---

## Connection Management

### All Layers Support:
1. **Persistent Connections**: Keep socket open for multiple requests
2. **Connection Timeout**: Automatic disconnect on inactivity
3. **Connection Cleanup**: Resources freed on disconnect
4. **Concurrent Connections**: Multiple clients supported

### Best Practices:
1. **Reuse Connections**: Keep socket open for multiple operations
2. **Handle Timeouts**: Reconnect on timeout errors
3. **Pool ID Consistency**: Use same `pool_id` for related operations
4. **Error Handling**: Check `success` field in all responses
5. **Binary Protocol**: Use 4-byte length prefix for all messages

---

## Error Responses

### Layer 1 (IFR)
```json
{
  "type": "error",
  "request_id": "req-123",
  "success": false,
  "error": "error message"
}
```

### Layer 2 (DSR)
```json
{
  "type": "Error",
  "request_id": "req-200",
  "error": "error message",
  "error_code": "ERROR_CODE"
}
```

### Layer 3 (ALM)
```json
{
  "type": "error",
  "request_id": "req-300",
  "success": false,
  "error": "error message"
}
```

### Layer 4 (CPE)
```json
{
  "type": "error",
  "request_id": "req-400",
  "success": false,
  "data": {
    "error": "error message"
  }
}
```

### Common Error Codes
- `POOL_ACCESS_FAILED`: Cannot access specified pool
- `ADD_MEMORY_FAILED`: Memory addition failed
- `SEARCH_FAILED`: Search operation failed
- `NOT_IMPLEMENTED`: Feature not yet implemented
- `INVALID_REQUEST`: Malformed request

---

## Testing Connections

### Test Layer 1
```bash
echo '{"type":"ping","request_id":"test-1"}' | nc -U /tmp/mfn_layer1.sock
```

### Test Layer 2
```python
import socket
import json
import struct

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect('/tmp/mfn_layer2.sock')

request = {"type": "Ping", "request_id": "test-2"}
msg = json.dumps(request).encode('utf-8')
sock.sendall(struct.pack('<I', len(msg)) + msg)

length = struct.unpack('<I', sock.recv(4))[0]
response = json.loads(sock.recv(length).decode('utf-8'))
print(response)
```

### Test Layer 3
```go
conn, _ := net.Dial("unix", "/tmp/mfn_layer3.sock")
request := map[string]string{"type": "ping", "request_id": "test-3"}
msg, _ := json.Marshal(request)
binary.Write(conn, binary.LittleEndian, uint32(len(msg)))
conn.Write(msg)
```

### Test Layer 4
```python
import socket
import json
import struct

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
sock.connect('/tmp/mfn_layer4.sock')

request = {"type": "Ping", "request_id": "test-4"}
msg = json.dumps(request).encode('utf-8')
sock.sendall(struct.pack('<I', len(msg)) + msg)

length = struct.unpack('<I', sock.recv(4))[0]
response = json.loads(sock.recv(length).decode('utf-8'))
print(response)
```

---

## Summary Table

| Layer | Socket Path | Language | Binary Protocol | Pool Support | Status |
|-------|-------------|----------|-----------------|--------------|--------|
| Layer 1 IFR | `/tmp/mfn_layer1.sock` | Zig | 16-byte header + payload | No | ✅ Implemented |
| Layer 2 DSR | `/tmp/mfn_layer2.sock` | Rust | 4-byte length + JSON | Yes | ✅ Implemented |
| Layer 3 ALM | `/tmp/mfn_layer3.sock` | Go | 4-byte length + JSON | Yes | ✅ Implemented |
| Layer 4 CPE | `/tmp/mfn_layer4.sock` | Rust | 4-byte length + JSON | Yes | ✅ Implemented |
| Layer 5 PSR | `/tmp/mfn_layer5.sock` | Rust | Not implemented | Expected | ⏳ Pending |

---

## Next Steps for Layer 5 Implementation

To complete Layer 5 PSR socket server:

1. Implement socket listener in `layer5_socket_server.rs`
2. Follow Layer 2/4 patterns: 4-byte length + JSON
3. Add request handlers for:
   - `AddPattern`
   - `SearchPattern`
   - `GetStats`
   - `Ping`
   - `HealthCheck`
4. Implement multi-pool support via `PoolManager`
5. Add connection cleanup for pattern data

Reference implementations:
- `mfn-layer2-rust/src/socket_server.rs`
- `mfn-layer4-rust/src/bin/layer4_socket_server.rs`
