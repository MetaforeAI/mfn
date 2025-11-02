# MFN Integration Tests

Comprehensive integration tests that validate all 4 MFN layers work together via Unix socket communication.

## Prerequisites

All 4 layer socket servers must be running. The test will gracefully skip if layers are not available.

## Starting Layers

```bash
# Start all layers at once
./scripts/start_all_layers.sh

# Verify sockets exist
ls -la /tmp/mfn_layer*.sock
```

## Running Tests

```bash
# Run all integration tests
cargo test --test full_system_test

# Run with detailed output
cargo test --test full_system_test -- --nocapture

# Run specific test
cargo test --test full_system_test test_layer_connectivity
```

## Test Coverage

### 1. Layer Connectivity Test (`test_layer_connectivity`)
- Verifies socket files exist at `/tmp/mfn_layer{1,2,3,4}.sock`
- Attempts to connect to each available layer
- Reports connection status for each layer

### 2. Single Memory Flow Test (`test_single_memory_flow`)
- Adds a test memory to each available layer
- Measures response times for memory operations
- Validates successful memory storage

### 3. Query Routing Test (`test_query_routing`)
- Tests different query types:
  - Exact match queries
  - Similarity searches
  - Associative queries
  - Contextual predictions
- Measures query response times
- Reports number of results from each layer

### 4. Performance Sanity Check (`test_performance_sanity_check`)
- Runs 100 iterations of queries per layer
- Calculates average, min, max latencies
- **Important**: Validates realistic performance:
  - Expected: 200-500 µs per operation
  - Warning: <50 µs indicates stub/fake implementation
  - Warning: >5000 µs indicates performance issues
- Detects when layers return unrealistic 5-10 ns latencies (empty operations)

### 5. Concurrent Load Test (`test_concurrent_load`)
- Tests layers under concurrent request load
- Sends 10 parallel requests to each layer
- Measures total processing time
- Validates concurrent request handling

## Expected Behavior

### When Layers Are NOT Running:
```
=== Testing Layer Connectivity ===
  Layer1 socket: ✗ NOT FOUND
  Layer2 socket: ✗ NOT FOUND
  Layer3 socket: ✗ NOT FOUND
  Layer4 socket: ✗ NOT FOUND

⚠️  No layer sockets found!
   Please run: ./scripts/start_all_layers.sh
   Skipping integration tests...
```

### When Layers ARE Running:
```
=== Testing Layer Connectivity ===
  Layer1 socket: ✓ EXISTS
  Layer2 socket: ✓ EXISTS
  Layer3 socket: ✓ EXISTS
  Layer4 socket: ✓ EXISTS

Attempting connections:
  Layer1 - ✓ CONNECTED
  Layer2 - ✓ CONNECTED
  Layer3 - ✓ CONNECTED
  Layer4 - ✓ CONNECTED
```

## Performance Validation

The test specifically checks for realistic performance:

- **Good Performance**: 200-500 µs per operation
- **Network Overhead**: 50-200 µs round-trip
- **Total Expected**: 250-700 µs per query

### Red Flags:
- **5-10 ns latencies**: Indicates empty/stub operations (not real processing)
- **>5 ms latencies**: Indicates performance problems
- **All requests failing**: Indicates connectivity issues

## Troubleshooting

### Socket Files Not Found
```bash
# Check if layers are running
ps aux | grep -E "mfn_layer|layer[1-4]_"

# Check socket files
ls -la /tmp/mfn_layer*.sock

# Check layer logs
tail -f /tmp/layer*.log
```

### Connection Refused
```bash
# Restart layers
pkill -f "mfn_layer|layer[1-4]_"
./scripts/start_all_layers.sh
```

### Unrealistic Performance
If seeing 5-10 ns latencies:
1. Check layer implementations have real logic
2. Verify layers are processing queries, not just returning empty results
3. Check benchmark results match integration test results

## CI/CD Integration

For continuous integration:

```yaml
# Example GitHub Actions workflow
- name: Start MFN Layers
  run: ./scripts/start_all_layers.sh

- name: Wait for layers
  run: sleep 2

- name: Run Integration Tests
  run: cargo test --test full_system_test

- name: Stop Layers
  run: pkill -f "mfn_layer|layer[1-4]_" || true
```