# MFN Phase 2 Unified Unix Socket Architecture

## Executive Summary

Based on comprehensive performance analysis of existing Layer 3 Unix socket implementation, this document outlines the unified socket architecture for all 4 MFN layers. Performance testing shows **88.6% improvement** with Unix sockets (0.13ms vs 1.39ms HTTP average response time) and **777% increase** in requests per second (6,305 vs 719 RPS).

## Performance Analysis Results

### Current State Assessment

| Layer | Current Interface | Socket Path | Status |
|-------|------------------|-------------|---------|
| Layer 1 (IFR) | FFI Only | `/tmp/mfn_layer1.sock` | ❌ Not Implemented |
| Layer 2 (DSR) | FFI Only | `/tmp/mfn_layer2.sock` | ❌ Not Implemented |
| Layer 3 (ALM) | HTTP + Unix Socket | `/tmp/mfn_layer3.sock` | ✅ **Implemented** |
| Layer 4 (CPE) | FFI Only | `/tmp/mfn_layer4.sock` | ❌ Not Implemented |

### Performance Benchmarks (Layer 3)

```
Protocol Comparison:
Metric               HTTP         Unix Socket  Improvement
------------------------------------------------------------
Avg Time (ms)        1.39         0.13         ⬇️88.6%
95th % (ms)          0.61         0.18         ⬇️70.2%
Requests/sec         718.80       6304.90      ⬆️777.1%
Success Rate (%)     100.00       100.00       ✅
```

**Key Finding:** Unix socket implementation already achieves target <2ms latency with 0.13ms average response time.

## Unified Socket Architecture Design

### Socket Naming Convention

```
/tmp/mfn_layer{N}.sock
```

- `/tmp/mfn_layer1.sock` - Immediate Flow Registry (IFR)
- `/tmp/mfn_layer2.sock` - Dynamic Similarity Reservoir (DSR)
- `/tmp/mfn_layer3.sock` - Associative Link Mesh (ALM) ✅
- `/tmp/mfn_layer4.sock` - Context Prediction Engine (CPE)

### Protocol Stack Specification

#### Transport Layer
- **Protocol**: Unix Domain Sockets (`AF_UNIX`, `SOCK_STREAM`)
- **Benefits**: 
  - Zero network overhead
  - Kernel-level optimization
  - No TCP/IP stack traversal
  - Native Linux IPC performance

#### Framing Protocol
```
[4-byte length][JSON payload]
```
- Length-prefixed messages for reliable framing
- Network byte order (big-endian) for consistency
- Maximum message size: 1MB (configurable)

#### Message Format
```json
{
  "type": "request_type",
  "request_id": "unique_identifier",
  "layer": 1,
  "timestamp": 1631234567890,
  "payload": {
    // Layer-specific data
  }
}
```

#### Response Format
```json
{
  "type": "response_type", 
  "request_id": "matching_identifier",
  "success": true,
  "data": {
    // Response data
  },
  "error": null,
  "processing_time_ms": 0.16,
  "layer": 1
}
```

## Layer-Specific Socket Implementations

### Layer 1: Immediate Flow Registry (IFR)

**Current State**: Zig implementation with FFI only

**Required Implementation**:
```zig
// Unix socket server for Layer 1
pub const IFRSocketServer = struct {
    listener: std.net.StreamServer,
    ifr: *ImmediateFlowRegistry,
    running: bool,
    
    pub fn init(socket_path: []const u8, ifr: *ImmediateFlowRegistry) !Self {
        // Implementation
    }
    
    pub fn start(self: *Self) !void {
        // Accept connections and handle routing queries
    }
};
```

**Message Types**:
- `exact_match` - Query for exact content match
- `add_memory` - Add memory to registry
- `get_stats` - Retrieve performance statistics

**Target Performance**: <0.1ms response time

### Layer 2: Dynamic Similarity Reservoir (DSR) 

**Current State**: Rust implementation with extensive FFI

**Required Implementation**:
```rust
use tokio::net::UnixListener;
use serde::{Deserialize, Serialize};

pub struct DSRSocketServer {
    listener: UnixListener,
    dsr: Arc<DynamicSimilarityReservoir>,
    running: Arc<AtomicBool>,
}

impl DSRSocketServer {
    pub async fn new(socket_path: &str, dsr: Arc<DynamicSimilarityReservoir>) -> Result<Self> {
        // Implementation
    }
    
    pub async fn start(&self) -> Result<()> {
        // Handle similarity search requests
    }
}
```

**Message Types**:
- `similarity_search` - Find similar memories
- `add_memory` - Store new memory with embedding
- `get_performance` - Retrieve metrics

**Target Performance**: <1ms response time

### Layer 3: Associative Link Mesh (ALM) ✅

**Current State**: **COMPLETE** - Go implementation with optimized Unix socket

**Existing Implementation**: `layer3-go-alm/internal/ffi/ffi.go`

**Performance**: 0.13ms average, 6,305 RPS capacity

**Message Types**: ✅ Implemented
- `associative_search` - Multi-hop associative queries
- `add_memory` - Store memory with associations  
- `add_association` - Create memory links
- `get_memory` - Retrieve specific memory
- `get_stats` - Performance and graph statistics
- `ping` - Health check

### Layer 4: Context Prediction Engine (CPE)

**Current State**: Rust implementation with comprehensive FFI

**Required Implementation**:
```rust
use tokio::net::UnixListener;

pub struct CPESocketServer {
    listener: UnixListener,
    cpe: Arc<ContextPredictionLayer>,
    runtime: Arc<Runtime>,
}

impl CPESocketServer {
    pub async fn new(socket_path: &str, cpe: Arc<ContextPredictionLayer>) -> Result<Self> {
        // Implementation
    }
    
    pub async fn start(&self) -> Result<()> {
        // Handle prediction requests
    }
}
```

**Message Types**:
- `predict_next` - Generate context-based predictions
- `add_access` - Record memory access for temporal analysis
- `get_window` - Retrieve current context window
- `clear_state` - Reset temporal analysis

**Target Performance**: <2ms response time

## High-Performance Optimizations

### Connection Management

```rust
// Connection pool for high-throughput scenarios
pub struct SocketConnectionPool {
    connections: Vec<UnixStream>,
    available: Arc<Mutex<Vec<usize>>>,
    pool_size: usize,
}

impl SocketConnectionPool {
    pub async fn get_connection(&self) -> Result<UnixStream> {
        // Reuse existing connections or create new ones
    }
    
    pub fn return_connection(&self, connection: UnixStream) {
        // Return connection to pool
    }
}
```

### Zero-Copy Optimizations

- Use `sendfile()` for large data transfers
- Memory-mapped I/O for high-frequency operations
- Buffer pooling to reduce allocations

### Error Handling & Resilience

```rust
// Circuit breaker pattern for layer communication
pub struct CircuitBreaker {
    failure_count: Arc<AtomicUsize>,
    last_failure: Arc<Mutex<Option<Instant>>>,
    threshold: usize,
    timeout: Duration,
    state: Arc<Mutex<BreakerState>>,
}

enum BreakerState {
    Closed,    // Normal operation
    Open,      // Failing, reject requests
    HalfOpen,  // Testing recovery
}
```

## Infrastructure Requirements

### File System Permissions
```bash
# Create socket directory with proper permissions
sudo mkdir -p /tmp/mfn-sockets
sudo chown $(whoami):$(whoami) /tmp/mfn-sockets
sudo chmod 755 /tmp/mfn-sockets
```

### System Resource Configuration
```bash
# Increase Unix socket limits
echo 'net.core.somaxconn = 65535' >> /etc/sysctl.conf
echo 'kernel.threads-max = 2097152' >> /etc/sysctl.conf

# Apply settings
sudo sysctl -p
```

### Service Management

```systemd
# Example systemd service for layer management
[Unit]
Description=MFN Layer %i Socket Service
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/mfn-layer-%i --socket-mode
Restart=on-failure
RestartSec=5
User=mfn
Group=mfn

[Install]
WantedBy=multi-user.target
```

## Inter-Layer Communication Protocol

### Request Routing Flow

```
Client Request → Layer 1 (IFR)
                ↓ (if not exact match)
                Layer 2 (DSR) 
                ↓ (if similarity found)
                Layer 3 (ALM)
                ↓ (if context needed)
                Layer 4 (CPE)
```

### Routing Message Format

```json
{
  "type": "routing_decision",
  "found_exact": false,
  "next_layer": 2,
  "confidence": 0.85,
  "processing_time_ms": 0.05,
  "partial_results": {},
  "context": {
    "query_id": "uuid",
    "user_session": "session_id",
    "routing_path": [1, 2, 3]
  }
}
```

## Implementation Priority

### Phase 1: Foundation (Immediate - 1 week)
1. ✅ **Layer 3 Complete** - Already implemented and tested
2. Implement Layer 2 Unix socket server (Rust)
3. Create unified message protocol library

### Phase 2: Core Layers (Next 2 weeks)  
1. Implement Layer 1 Unix socket server (Zig)
2. Implement Layer 4 Unix socket server (Rust)
3. Add inter-layer routing protocol

### Phase 3: Optimization (Following 1 week)
1. Connection pooling implementation
2. Circuit breaker integration
3. Performance monitoring and metrics
4. Load testing and tuning

## Quality Assurance

### Performance Targets
- Layer 1 (IFR): <0.1ms response time
- Layer 2 (DSR): <1ms response time
- Layer 3 (ALM): <2ms response time ✅ **0.13ms achieved**
- Layer 4 (CPE): <2ms response time

### Load Testing Requirements
- 1000+ concurrent connections per layer
- 10,000+ requests per second sustained
- <1% error rate under load
- Graceful degradation under extreme load

### Monitoring & Alerting
- Response time percentiles (P95, P99, P99.9)
- Connection count and pool utilization
- Error rates by layer and message type
- Memory and CPU utilization per service

## Conclusion

The unified Unix socket architecture provides a high-performance, scalable foundation for MFN Phase 2. With Layer 3 already achieving exceptional performance (0.13ms average response time), extending this pattern to all layers will create a cohesive, ultra-fast memory system capable of handling enterprise-scale workloads while maintaining microsecond-level response times.

**Next Steps**: Begin implementation of Layer 2 Unix socket server, followed by Layer 1 and Layer 4 to complete the unified architecture.