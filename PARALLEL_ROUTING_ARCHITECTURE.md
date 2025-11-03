# Parallel Routing Architecture
**Phase 1, Step 2: Definition & Scoping**
**Bug**: BUG-002 - Stub Routing (Parallel Mode)
**Date**: 2025-11-02
**Author**: Integration Agent

---

## Executive Summary

This document specifies the architecture for true parallel query routing in the MFN system. The parallel routing strategy will query all four memory layers concurrently, merge results by relevance, and return a unified response. This replaces the current stub implementation that incorrectly calls `query_sequential()`.

**Key Requirements**:
- Query all 4 layers simultaneously using `tokio::join!`
- Handle partial failures gracefully (continue with available results)
- Merge results by confidence score (descending)
- Deduplicate by memory_id
- Target latency reduction: 4x vs sequential (5ms vs 20ms for equal layer times)

---

## 1. Current Implementation Analysis

### 1.1 Stub Code

**File**: `mfn-integration/src/socket_integration.rs:271-274`

```rust
async fn query_parallel(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    // For now, just use sequential query
    // TODO: Implement proper parallel execution with futures
    self.query_sequential(query).await
}
```

**Problems**:
1. **False advertising**: API claims parallel but executes sequential
2. **No performance benefit**: Same latency as sequential mode
3. **Wasted opportunity**: Multi-core CPUs not utilized
4. **Broken contract**: Callers expect 4x speedup, get 0x

### 1.2 Sequential Implementation (Baseline)

```rust
async fn query_sequential(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    let mut all_results = Vec::new();
    let mut pool = self.connection_pool.lock().await;

    // Query Layer 1 (exact match)
    if let Ok(layer1) = pool.get_layer1().await {
        match layer1.query(&query).await {
            Ok(result) => all_results.extend(convert_from_socket_results(result)),
            Err(e) => warn!("Layer 1 query failed: {}", e),
        }
    }

    // Query Layer 2 (similarity)
    if let Ok(layer2) = pool.get_layer2().await {
        match layer2.query(&query).await {
            Ok(result) => all_results.extend(convert_from_socket_results(result)),
            Err(e) => warn!("Layer 2 query failed: {}", e),
        }
    }

    // Query Layer 3 (associative)
    if let Ok(layer3) = pool.get_layer3().await {
        match layer3.query(&query).await {
            Ok(result) => all_results.extend(convert_from_socket_results(result)),
            Err(e) => warn!("Layer 3 query failed: {}", e),
        }
    }

    // Query Layer 4 (predictive)
    if let Ok(layer4) = pool.get_layer4().await {
        match layer4.query(&query).await {
            Ok(result) => all_results.extend(convert_from_socket_results(result)),
            Err(e) => warn!("Layer 4 query failed: {}", e),
        }
    }

    Ok(all_results)
}
```

**Latency**: `sum(layer1, layer2, layer3, layer4)` = ~20ms (if each layer is 5ms)

---

## 2. Parallel Execution Architecture

### 2.1 High-Level Flow

```
Query Request
    │
    ▼
┌─────────────────────────────────────────────┐
│  query_parallel()                           │
│  Clone query 4 times                        │
└─────────────────┬───────────────────────────┘
                  │
    ┌─────────────┼─────────────┬─────────────┐
    │             │             │             │
    ▼             ▼             ▼             ▼
┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐
│ Layer 1 │  │ Layer 2 │  │ Layer 3 │  │ Layer 4 │
│ (IFR)   │  │ (DSR)   │  │ (ALM)   │  │ (CPE)   │
│ 5ms     │  │ 5ms     │  │ 5ms     │  │ 5ms     │
└────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘
     │            │            │            │
     └────────────┼────────────┼────────────┘
                  │ tokio::join!
                  ▼
         ┌────────────────────┐
         │  Merge Results     │
         │  1. Flatten        │
         │  2. Deduplicate    │
         │  3. Sort by score  │
         │  4. Limit top-k    │
         └────────┬───────────┘
                  │
                  ▼
         ┌────────────────────┐
         │  Unified Results   │
         └────────────────────┘

Total Time: max(5, 5, 5, 5) = 5ms  ← 4x faster than 20ms sequential
```

### 2.2 Core Implementation

```rust
async fn query_parallel(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    let start = Instant::now();

    // Clone query for each layer
    let query1 = query.clone();
    let query2 = query.clone();
    let query3 = query.clone();
    let query4 = query.clone();

    // Get connection pool reference
    let pool = Arc::clone(&self.connection_pool);

    // Query all layers in parallel
    let (result1, result2, result3, result4) = tokio::join!(
        Self::query_layer1_safe(pool.clone(), query1),
        Self::query_layer2_safe(pool.clone(), query2),
        Self::query_layer3_safe(pool.clone(), query3),
        Self::query_layer4_safe(pool.clone(), query4),
    );

    // Collect all successful results
    let mut all_results = Vec::new();

    if let Ok(results) = result1 {
        all_results.extend(results);
    }
    if let Ok(results) = result2 {
        all_results.extend(results);
    }
    if let Ok(results) = result3 {
        all_results.extend(results);
    }
    if let Ok(results) = result4 {
        all_results.extend(results);
    }

    // Merge and rank results
    let merged = merge_and_rank_results(all_results, query.max_results);

    let elapsed = start.elapsed().as_millis() as f64;
    debug!("Parallel query completed in {}ms", elapsed);

    Ok(merged)
}
```

### 2.3 Safe Layer Query Wrappers

```rust
impl SocketMfnIntegration {
    /// Query Layer 1 with timeout and error handling
    async fn query_layer1_safe(
        pool: Arc<Mutex<LayerConnectionPool>>,
        query: UniversalSearchQuery,
    ) -> Result<Vec<UniversalSearchResult>> {
        let timeout = Duration::from_millis(query.timeout_ms as u64);

        match tokio::time::timeout(timeout, Self::query_layer1_impl(pool, query)).await {
            Ok(Ok(results)) => Ok(results),
            Ok(Err(e)) => {
                warn!("Layer 1 query failed: {}", e);
                Ok(vec![])  // Return empty, don't fail entire query
            }
            Err(_) => {
                warn!("Layer 1 query timeout after {}ms", timeout.as_millis());
                Ok(vec![])
            }
        }
    }

    async fn query_layer1_impl(
        pool: Arc<Mutex<LayerConnectionPool>>,
        query: UniversalSearchQuery,
    ) -> Result<Vec<UniversalSearchResult>> {
        let mut pool = pool.lock().await;
        let socket_query = convert_to_socket_query(&query);

        let layer1 = pool.get_layer1().await?;
        let result = layer1.query(&socket_query).await?;

        Ok(convert_from_socket_results(result))
    }

    // Similar implementations for Layer 2, 3, 4...
    async fn query_layer2_safe(
        pool: Arc<Mutex<LayerConnectionPool>>,
        query: UniversalSearchQuery,
    ) -> Result<Vec<UniversalSearchResult>> {
        // Same pattern as Layer 1
    }

    async fn query_layer3_safe(
        pool: Arc<Mutex<LayerConnectionPool>>,
        query: UniversalSearchQuery,
    ) -> Result<Vec<UniversalSearchResult>> {
        // Same pattern as Layer 1
    }

    async fn query_layer4_safe(
        pool: Arc<Mutex<LayerConnectionPool>>,
        query: UniversalSearchQuery,
    ) -> Result<Vec<UniversalSearchResult>> {
        // Same pattern as Layer 1
    }
}
```

---

## 3. Result Merging Algorithm

### 3.1 Merging Requirements

**Inputs**:
- `Vec<UniversalSearchResult>` from Layer 1 (exact matches)
- `Vec<UniversalSearchResult>` from Layer 2 (similarity)
- `Vec<UniversalSearchResult>` from Layer 3 (associative)
- `Vec<UniversalSearchResult>` from Layer 4 (predictive)

**Outputs**:
- Single `Vec<UniversalSearchResult>` with top-k results

**Operations**:
1. **Flatten**: Combine all layer results into single vector
2. **Deduplicate**: Remove duplicate memory_ids (keep highest confidence)
3. **Sort**: Order by confidence score (descending)
4. **Limit**: Take top-k results (respect `max_results`)

### 3.2 Merge Implementation

```rust
/// Merge results from multiple layers into unified ranked list
fn merge_and_rank_results(
    all_results: Vec<UniversalSearchResult>,
    max_results: usize,
) -> Vec<UniversalSearchResult> {
    if all_results.is_empty() {
        return vec![];
    }

    // Step 1: Deduplicate by memory_id (keep highest confidence)
    let mut deduped: HashMap<MemoryId, UniversalSearchResult> = HashMap::new();

    for result in all_results {
        let memory_id = result.memory_id.clone();

        match deduped.entry(memory_id) {
            Entry::Vacant(e) => {
                e.insert(result);
            }
            Entry::Occupied(mut e) => {
                // Keep result with higher confidence
                if result.confidence > e.get().confidence {
                    e.insert(result);
                } else {
                    // Merge metadata from both results
                    merge_metadata(e.get_mut(), &result);
                }
            }
        }
    }

    // Step 2: Convert to vector and sort by confidence
    let mut results: Vec<UniversalSearchResult> = deduped.into_values().collect();
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Step 3: Limit to max_results
    results.truncate(max_results);

    results
}

/// Merge metadata from duplicate results
fn merge_metadata(target: &mut UniversalSearchResult, source: &UniversalSearchResult) {
    // Add source layer to layers_found
    if let Some(source_layer) = source.metadata.get("layer") {
        target
            .metadata
            .entry("layers_found".to_string())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .unwrap()
            .push(source_layer.clone());
    }

    // Merge access_count (sum)
    if let (Some(target_count), Some(source_count)) = (
        target.metadata.get("access_count").and_then(|v| v.as_u64()),
        source.metadata.get("access_count").and_then(|v| v.as_u64()),
    ) {
        target
            .metadata
            .insert("access_count".to_string(), serde_json::json!(target_count + source_count));
    }

    // Keep latest timestamp
    if let (Some(target_ts), Some(source_ts)) = (
        target.metadata.get("timestamp").and_then(|v| v.as_i64()),
        source.metadata.get("timestamp").and_then(|v| v.as_i64()),
    ) {
        if source_ts > target_ts {
            target.metadata.insert("timestamp".to_string(), serde_json::json!(source_ts));
        }
    }
}
```

### 3.3 Merge Example

**Input Results**:
```
Layer 1: [
    { memory_id: "mem_1", confidence: 0.95, content: "exact match" },
    { memory_id: "mem_2", confidence: 0.80, content: "partial" },
]

Layer 2: [
    { memory_id: "mem_1", confidence: 0.85, content: "exact match" },  ← duplicate
    { memory_id: "mem_3", confidence: 0.75, content: "similar" },
]

Layer 3: [
    { memory_id: "mem_4", confidence: 0.70, content: "associated" },
]

Layer 4: [
    { memory_id: "mem_2", confidence: 0.90, content: "partial" },  ← duplicate (higher conf)
    { memory_id: "mem_5", confidence: 0.65, content: "predicted" },
]
```

**Step 1 - Deduplicate**:
```
{
    "mem_1": { confidence: 0.95, layers_found: ["Layer1", "Layer2"] },  ← kept higher
    "mem_2": { confidence: 0.90, layers_found: ["Layer1", "Layer4"] },  ← kept higher
    "mem_3": { confidence: 0.75, layers_found: ["Layer2"] },
    "mem_4": { confidence: 0.70, layers_found: ["Layer3"] },
    "mem_5": { confidence: 0.65, layers_found: ["Layer4"] },
}
```

**Step 2 - Sort by Confidence**:
```
[
    { memory_id: "mem_1", confidence: 0.95 },
    { memory_id: "mem_2", confidence: 0.90 },
    { memory_id: "mem_3", confidence: 0.75 },
    { memory_id: "mem_4", confidence: 0.70 },
    { memory_id: "mem_5", confidence: 0.65 },
]
```

**Step 3 - Limit (max_results=3)**:
```
[
    { memory_id: "mem_1", confidence: 0.95 },
    { memory_id: "mem_2", confidence: 0.90 },
    { memory_id: "mem_3", confidence: 0.75 },
]
```

---

## 4. Error Handling Strategy

### 4.1 Partial Failure Handling

**Philosophy**: Parallel routing should be resilient - if one layer fails, continue with remaining layers

**Failure Scenarios**:

| Scenario | Behavior | Rationale |
|----------|----------|-----------|
| 1 layer fails | Return results from 3 layers | Partial data better than no data |
| 2 layers fail | Return results from 2 layers | Still useful results |
| 3 layers fail | Return results from 1 layer | Better than total failure |
| All 4 fail | Return error | Nothing to return |

**Implementation**:
```rust
// In query_parallel()
let (result1, result2, result3, result4) = tokio::join!(...);

let success_count = [&result1, &result2, &result3, &result4]
    .iter()
    .filter(|r| r.is_ok())
    .count();

if success_count == 0 {
    return Err(anyhow!("All layers failed to respond"));
}

if success_count < 4 {
    warn!("Partial failure: only {}/4 layers responded", success_count);
}
```

### 4.2 Timeout Handling

**Per-Layer Timeout**:
- Default: 100ms per layer
- Configurable via `query.timeout_ms`
- Independent timeouts (one slow layer doesn't block others)

**Implementation**:
```rust
async fn query_layer1_safe(
    pool: Arc<Mutex<LayerConnectionPool>>,
    query: UniversalSearchQuery,
) -> Result<Vec<UniversalSearchResult>> {
    // Per-layer timeout (default 100ms)
    let timeout = Duration::from_millis(query.timeout_ms as u64);

    match tokio::time::timeout(timeout, Self::query_layer1_impl(pool, query)).await {
        Ok(Ok(results)) => Ok(results),
        Ok(Err(e)) => {
            warn!("Layer 1 query failed: {}", e);
            Ok(vec![])  // Return empty on failure
        }
        Err(_) => {
            warn!("Layer 1 query timeout after {}ms", timeout.as_millis());
            Ok(vec![])  // Return empty on timeout
        }
    }
}
```

**Total Query Timeout**:
- Max time = slowest layer timeout
- Example: If all layers timeout at 100ms, total = 100ms (not 400ms)

### 4.3 Connection Pool Contention

**Problem**: Parallel queries may exhaust connection pool

**Scenario**:
```
Connection pool size: 4 per layer
Concurrent queries: 10
Required connections: 10 queries × 4 layers = 40 connections
Available: 4 × 4 = 16 connections
Deficit: 24 connections
```

**Mitigation**:
1. **Increase pool size**: Scale pool with expected concurrency
   ```rust
   // In LayerConnectionPool::new()
   let pool_size = 8; // Support 2 concurrent parallel queries per layer
   ```

2. **Connection acquisition timeout**: Fail fast if pool exhausted
   ```rust
   async fn get_layer1_with_timeout(&mut self, timeout_ms: u64) -> Result<SocketLayer1> {
       tokio::time::timeout(
           Duration::from_millis(timeout_ms),
           self.get_layer1()
       ).await?
   }
   ```

3. **Backpressure**: Limit concurrent parallel queries
   ```rust
   pub struct SocketMfnIntegration {
       connection_pool: Arc<Mutex<LayerConnectionPool>>,
       parallel_query_limiter: Arc<Semaphore>,  // Cap at 2 concurrent
   }
   ```

### 4.4 Empty Results Handling

**Scenario**: All layers return empty results (no matches found)

**Behavior**:
```rust
fn merge_and_rank_results(
    all_results: Vec<UniversalSearchResult>,
    max_results: usize,
) -> Vec<UniversalSearchResult> {
    if all_results.is_empty() {
        return vec![];  // Valid response: no matches found
    }
    // ...
}
```

**Response**:
```json
{
  "results": [],
  "total_time_ms": 12.5,
  "layer_times": [
    ["Layer1", 3.2],
    ["Layer2", 12.5],
    ["Layer3", 8.1],
    ["Layer4", 5.3]
  ]
}
```

---

## 5. Performance Analysis

### 5.1 Latency Comparison

**Scenario**: Each layer takes 5ms to respond

**Sequential Routing**:
```
Layer 1: 0-5ms
Layer 2: 5-10ms
Layer 3: 10-15ms
Layer 4: 15-20ms
Total: 20ms
```

**Parallel Routing**:
```
Layer 1: 0-5ms  ┐
Layer 2: 0-5ms  ├─ Concurrent
Layer 3: 0-5ms  │
Layer 4: 0-5ms  ┘
Total: 5ms  ← 4x faster
```

### 5.2 Realistic Latency Breakdown

**Layer Performance** (from discovery report):
- Layer 1 (IFR): 0.1ms (in-memory index)
- Layer 2 (DSR): 1-5ms (spike encoding + reservoir)
- Layer 3 (ALM): 2-10ms (graph traversal)
- Layer 4 (CPE): 5-15ms (prediction engine)

**Sequential**:
```
Total = 0.1 + 5 + 10 + 15 = 30.1ms
```

**Parallel**:
```
Total = max(0.1, 5, 10, 15) = 15ms  ← 2x faster
```

**Note**: Speedup = 2x (not 4x) because layers have different latencies

### 5.3 Throughput Analysis

**Single-threaded Sequential**:
- Queries per second: 1000ms / 30ms = ~33 req/s

**Single-threaded Parallel**:
- Queries per second: 1000ms / 15ms = ~67 req/s
- Improvement: 2x

**Multi-threaded Parallel (4 cores)**:
- Queries per second: 67 req/s × 4 = ~268 req/s
- Improvement: 8x vs single-threaded sequential

### 5.4 Resource Utilization

**CPU Utilization**:
- Sequential: 1 core active at a time (serialized)
- Parallel: 4 cores active simultaneously (better utilization)

**Connection Pool Usage**:
- Sequential: 1 connection per layer (4 total over time)
- Parallel: 4 connections simultaneously (higher pressure)

**Memory**:
- Sequential: ~10MB per query (single layer at a time)
- Parallel: ~40MB per query (4 layers simultaneously)

---

## 6. Concurrency Architecture

### 6.1 Tokio Join vs Spawn

**Option A: tokio::join!** (RECOMMENDED)
```rust
let (r1, r2, r3, r4) = tokio::join!(
    query_layer1(...),
    query_layer2(...),
    query_layer3(...),
    query_layer4(...),
);
```

**Pros**:
- Simpler code (macro-based)
- Automatic error propagation
- No task overhead
- Easier to reason about

**Cons**:
- All queries must complete (can't cancel early)
- No individual task control

**Option B: tokio::spawn**
```rust
let task1 = tokio::spawn(query_layer1(...));
let task2 = tokio::spawn(query_layer2(...));
let task3 = tokio::spawn(query_layer3(...));
let task4 = tokio::spawn(query_layer4(...));

let r1 = task1.await?;
let r2 = task2.await?;
let r3 = task3.await?;
let r4 = task4.await?;
```

**Pros**:
- Individual task control (can cancel)
- Potential for task stealing across threads

**Cons**:
- More complex error handling
- Task spawn overhead (~1-2μs per task)
- Harder to debug

**Recommendation**: Use `tokio::join!` for simplicity and performance

### 6.2 Connection Pool Locking Strategy

**Problem**: Multiple parallel queries may deadlock on connection pool lock

**Current (Naive)**:
```rust
async fn query_layer1_impl(...) -> Result<...> {
    let mut pool = pool.lock().await;  ← Blocks other layers
    let layer1 = pool.get_layer1().await?;
    layer1.query(...).await
}
```

**Issue**: If Layer 1 holds lock while querying, Layer 2/3/4 must wait

**Solution: Lock Minimization**
```rust
async fn query_layer1_impl(...) -> Result<...> {
    // Acquire connection without holding lock
    let layer1 = {
        let mut pool = pool.lock().await;
        pool.get_layer1().await?
        // Lock released here
    };

    // Query without holding lock
    layer1.query(...).await
}
```

**Benefits**:
- Lock held for <0.1ms (just connection acquisition)
- No lock contention during query execution
- Parallel queries truly independent

---

## 7. Testing Strategy

### 7.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_faster_than_sequential() {
        let integration = SocketMfnIntegration::new().await.unwrap();
        let query = UniversalSearchQuery {
            query_text: "test query".to_string(),
            max_results: 10,
            ..Default::default()
        };

        // Measure sequential
        let start = Instant::now();
        let _ = integration.query_sequential(query.clone()).await.unwrap();
        let sequential_ms = start.elapsed().as_millis();

        // Measure parallel
        let start = Instant::now();
        let _ = integration.query_parallel(query.clone()).await.unwrap();
        let parallel_ms = start.elapsed().as_millis();

        // Parallel should be faster (allow 10% margin)
        assert!(
            parallel_ms < sequential_ms,
            "Parallel ({}ms) should be faster than sequential ({}ms)",
            parallel_ms,
            sequential_ms
        );
    }

    #[tokio::test]
    async fn test_parallel_handles_layer_failure() {
        // Mock Layer 2 to fail
        // Verify other 3 layers still return results
        // ...
    }

    #[tokio::test]
    async fn test_merge_deduplicates_correctly() {
        let results = vec![
            UniversalSearchResult {
                memory_id: "mem_1".into(),
                confidence: 0.9,
                ..Default::default()
            },
            UniversalSearchResult {
                memory_id: "mem_1".into(),  // duplicate
                confidence: 0.8,             // lower confidence
                ..Default::default()
            },
        ];

        let merged = merge_and_rank_results(results, 10);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].confidence, 0.9); // kept higher
    }

    #[tokio::test]
    async fn test_merge_sorts_by_confidence() {
        let results = vec![
            UniversalSearchResult { confidence: 0.7, ..Default::default() },
            UniversalSearchResult { confidence: 0.9, ..Default::default() },
            UniversalSearchResult { confidence: 0.5, ..Default::default() },
        ];

        let merged = merge_and_rank_results(results, 10);
        assert_eq!(merged[0].confidence, 0.9);
        assert_eq!(merged[1].confidence, 0.7);
        assert_eq!(merged[2].confidence, 0.5);
    }
}
```

### 7.2 Integration Tests

```rust
#[tokio::test]
async fn test_parallel_routing_end_to_end() {
    // Start all layer servers
    let _layer1 = start_layer1_server().await;
    let _layer2 = start_layer2_server().await;
    let _layer3 = start_layer3_server().await;
    let _layer4 = start_layer4_server().await;

    // Initialize integration
    let integration = SocketMfnIntegration::new().await.unwrap();
    integration.initialize_all_layers().await.unwrap();

    // Query with parallel routing
    let query = UniversalSearchQuery {
        query_text: "test integration".to_string(),
        max_results: 10,
        routing_strategy: RoutingStrategy::Parallel,
        ..Default::default()
    };

    let results = integration.query(&query).await.unwrap();

    // Verify results from multiple layers
    assert!(!results.results.is_empty());

    // Check metadata for layers_found
    for result in &results.results {
        assert!(result.metadata.contains_key("layer"));
    }
}
```

### 7.3 Performance Benchmarks

```rust
#[bench]
fn bench_parallel_vs_sequential(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let integration = runtime.block_on(async {
        SocketMfnIntegration::new().await.unwrap()
    });

    let query = UniversalSearchQuery {
        query_text: "benchmark query".to_string(),
        max_results: 10,
        ..Default::default()
    };

    b.iter(|| {
        runtime.block_on(async {
            integration.query_parallel(query.clone()).await.unwrap()
        })
    });
}
```

---

## 8. Deployment Considerations

### 8.1 Configuration

**Environment Variables**:
```bash
# Connection pool sizing
MFN_POOL_SIZE=8              # Connections per layer (default: 4)
MFN_MAX_PARALLEL_QUERIES=4   # Concurrent parallel queries (default: 2)

# Timeouts
MFN_LAYER_TIMEOUT_MS=100     # Per-layer timeout (default: 100ms)
MFN_QUERY_TIMEOUT_MS=500     # Total query timeout (default: 500ms)

# Performance tuning
MFN_ENABLE_BATCHING=true     # Enable request batching
MFN_BATCH_SIZE=16            # Requests per batch
```

### 8.2 Monitoring Metrics

**Key Metrics**:
```rust
pub struct ParallelRoutingMetrics {
    // Latency
    pub sequential_latency_ms: Histogram,
    pub parallel_latency_ms: Histogram,
    pub speedup_ratio: Gauge,  // parallel / sequential

    // Layer performance
    pub layer1_latency_ms: Histogram,
    pub layer2_latency_ms: Histogram,
    pub layer3_latency_ms: Histogram,
    pub layer4_latency_ms: Histogram,

    // Reliability
    pub layer1_success_rate: Counter,
    pub layer2_success_rate: Counter,
    pub layer3_success_rate: Counter,
    pub layer4_success_rate: Counter,
    pub partial_failure_count: Counter,

    // Resource usage
    pub concurrent_parallel_queries: Gauge,
    pub pool_connection_wait_ms: Histogram,
}
```

**Prometheus Exposition**:
```
# HELP mfn_parallel_latency_ms Parallel routing latency
# TYPE mfn_parallel_latency_ms histogram
mfn_parallel_latency_ms_bucket{le="5"} 120
mfn_parallel_latency_ms_bucket{le="10"} 450
mfn_parallel_latency_ms_bucket{le="20"} 890
mfn_parallel_latency_ms_bucket{le="50"} 980
mfn_parallel_latency_ms_bucket{le="+Inf"} 1000

# HELP mfn_speedup_ratio Parallel vs sequential speedup
# TYPE mfn_speedup_ratio gauge
mfn_speedup_ratio 2.3
```

### 8.3 Health Checks

```rust
impl SocketMfnIntegration {
    pub async fn health_check_parallel(&self) -> HealthStatus {
        let test_query = UniversalSearchQuery {
            query_text: "health check".to_string(),
            max_results: 1,
            timeout_ms: 1000,
            ..Default::default()
        };

        match self.query_parallel(test_query).await {
            Ok(results) => {
                // Check which layers responded
                let responding_layers = self.count_responding_layers(&results);

                match responding_layers {
                    4 => HealthStatus::Healthy,
                    3 => HealthStatus::Degraded {
                        reason: "1 layer unresponsive".to_string(),
                    },
                    2 => HealthStatus::Degraded {
                        reason: "2 layers unresponsive".to_string(),
                    },
                    _ => HealthStatus::Unhealthy {
                        reason: format!("Only {} layers responding", responding_layers),
                    },
                }
            }
            Err(e) => HealthStatus::Unhealthy {
                reason: format!("Parallel routing failed: {}", e),
            },
        }
    }
}
```

---

## 9. Migration Path

### 9.1 Backward Compatibility

**Requirement**: Ensure sequential routing still works during parallel rollout

**Strategy**:
```rust
pub enum RoutingStrategy {
    Sequential,      // Legacy: query layers one by one
    Parallel,        // New: query all layers concurrently
    Adaptive,        // Future: intelligent routing
}

impl SocketMfnIntegration {
    pub async fn query(&self, query: UniversalSearchQuery) -> Result<MfnQueryResult> {
        match self.routing_strategy {
            RoutingStrategy::Sequential => self.query_sequential(query).await,
            RoutingStrategy::Parallel => self.query_parallel(query).await,
            RoutingStrategy::Adaptive => self.query_adaptive(query).await,
        }
    }
}
```

**Migration Steps**:
1. **Deploy parallel implementation** (default: sequential)
2. **Enable parallel for 10% of traffic** (A/B test)
3. **Monitor metrics** (latency, error rates, resource usage)
4. **Gradually increase to 100%**
5. **Remove sequential code** (cleanup in future phase)

### 9.2 Feature Flag

```rust
pub struct MfnConfig {
    pub default_routing: RoutingStrategy,
    pub enable_parallel_routing: bool,
    pub parallel_rollout_percentage: u8,  // 0-100
}

impl SocketMfnIntegration {
    async fn select_routing_strategy(&self, query: &UniversalSearchQuery) -> RoutingStrategy {
        if !self.config.enable_parallel_routing {
            return RoutingStrategy::Sequential;
        }

        // Gradual rollout based on query_id hash
        let hash = query.query_id.as_bytes()[0] as u8;
        let use_parallel = hash < (255 * self.config.parallel_rollout_percentage / 100) as u8;

        if use_parallel {
            RoutingStrategy::Parallel
        } else {
            RoutingStrategy::Sequential
        }
    }
}
```

---

## 10. Success Criteria

### 10.1 Functional Requirements

- [ ] Query all 4 layers concurrently using `tokio::join!`
- [ ] Handle partial failures gracefully (return available results)
- [ ] Deduplicate results by memory_id correctly
- [ ] Sort merged results by confidence score
- [ ] Respect max_results limit after merging
- [ ] Timeout individual layers independently
- [ ] Sequential routing still functional (backward compatibility)

### 10.2 Performance Requirements

- [ ] Latency: max(layer1, layer2, layer3, layer4) not sum()
- [ ] Speedup: 1.5-2.5x faster than sequential (realistic)
- [ ] Throughput: 2x higher than sequential
- [ ] Resource usage: <2x memory vs sequential
- [ ] Connection pool: No deadlocks or exhaustion

### 10.3 Quality Requirements

- [ ] No stub or TODO comments remaining
- [ ] Comprehensive error handling for all failure modes
- [ ] Unit test coverage: >80%
- [ ] Integration tests with all 4 layers passing
- [ ] Performance benchmarks demonstrating speedup
- [ ] Documentation complete and accurate

### 10.4 Operational Requirements

- [ ] Health check endpoint functional
- [ ] Metrics for latency, throughput, error rates
- [ ] Graceful degradation on layer failures
- [ ] Logging at appropriate levels
- [ ] Feature flag for gradual rollout
- [ ] Backward compatibility maintained

---

## 11. Implementation Task Breakdown

**Task 1: Implement Safe Layer Query Wrappers** (1 hour)
- Create `query_layer1_safe()` through `query_layer4_safe()`
- Add timeout handling per layer
- Add error-to-empty-result conversion
- **File**: `mfn-integration/src/socket_integration.rs`

**Task 2: Implement Parallel Query Function** (1.5 hours)
- Replace stub with `tokio::join!` implementation
- Handle result collection from all layers
- Add logging for parallel execution
- **File**: `mfn-integration/src/socket_integration.rs`

**Task 3: Implement Result Merging** (1 hour)
- Create `merge_and_rank_results()` function
- Implement deduplication by memory_id
- Implement confidence-based sorting
- Handle metadata merging
- **File**: `mfn-integration/src/socket_integration.rs`

**Task 4: Connection Pool Lock Optimization** (0.5 hours)
- Minimize lock holding time
- Ensure lock released before query execution
- **File**: `mfn-integration/src/socket_integration.rs`

**Task 5: Add Unit Tests** (1 hour)
- Test merge deduplication
- Test merge sorting
- Test partial failure handling
- Test empty results handling
- **File**: `mfn-integration/tests/test_parallel_routing.rs`

**Total Estimated Effort**: 5 hours (within 4-6 hour target)

---

## Appendix: Flow Diagrams

### Sequential vs Parallel Timing Diagram

```
Sequential Routing:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Time →
0ms        5ms       10ms      15ms      20ms
│          │          │          │          │
├─Layer1─┤ │          │          │          │
│         ├─Layer2──┤ │          │          │
│         │         ├─Layer3───┤ │          │
│         │         │          ├─Layer4───┤│
│         │         │          │          │└─ Total: 20ms
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Parallel Routing:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Time →
0ms             5ms
│               │
├─Layer1──────┤ │
├─Layer2──────┤ │
├─Layer3──────┤ │
├─Layer4──────┤ │
│              └─ Total: 5ms (4x faster)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

---

**Document Status**: COMPLETE
**Ready for Step 4 Implementation**: YES
**Dependencies**: None (independent of BUG-001 embedding work)
