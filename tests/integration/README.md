# MFN Integration Tests

Comprehensive integration tests that validate all 4 MFN layers work together via Unix socket communication.

## Automated Testing (RECOMMENDED)

The test harness automatically manages server lifecycle - no manual setup required!

```bash
# One command to rule them all - builds binaries, runs tests, cleans up
./scripts/run_integration_tests.sh
```

Or run directly with cargo:
```bash
# Build binaries first
cargo build --release
cd layer1-zig-ifr && zig build -Doptimize=ReleaseFast && cd ..
cd layer3-go-alm && go build -o mfn-layer3-server && cd ..

# Run tests (harness handles server management)
cargo test --release --test full_system_test
```

The test harness automatically:
- ✅ Cleans up old socket files
- ✅ Starts all layer servers
- ✅ Waits for health checks
- ✅ Runs tests
- ✅ Stops servers and cleans up (even on test failure)

## Manual Testing (LEGACY)

If you need to manually manage servers:

```bash
# Start all layers at once
./scripts/start_all_layers.sh

# Verify sockets exist
ls -la /tmp/mfn_layer*.sock

# Run tests
cargo test --test full_system_test -- --nocapture

# Stop layers
./scripts/stop_all_layers.sh
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

### Build Failures
```bash
# Check all build dependencies are installed
rustc --version  # Rust compiler
zig version      # Zig compiler
go version       # Go compiler

# Build each layer individually
cargo build --release --bin layer2_socket_server
cargo build --release --bin layer4_socket_server
cd layer1-zig-ifr && zig build -Doptimize=ReleaseFast && cd ..
cd layer3-go-alm && go build -o mfn-layer3-server && cd ..
```

### Test Environment Setup Failed
```bash
# Check if binaries exist
ls -la target/release/layer*_socket_server
ls -la layer1-zig-ifr/zig-out/bin/layer1_socket_main
ls -la layer3-go-alm/mfn-layer3-server

# Clean and rebuild
cargo clean
./scripts/run_integration_tests.sh
```

### Socket Permission Issues
```bash
# Check socket directory permissions
ls -la /tmp/

# Remove old sockets manually
rm -f /tmp/mfn_layer*.sock
```

### Lingering Processes
```bash
# Kill all layer processes
pkill -f "layer1_socket_main"
pkill -f "layer2_socket_server"
pkill -f "mfn-layer3-server"
pkill -f "layer4_socket_server"

# Verify no processes remain
ps aux | grep -E "layer[1-4]_"
```

### Unrealistic Performance
If seeing 5-10 ns latencies:
1. Check layer implementations have real logic
2. Verify layers are processing queries, not just returning empty results
3. Check benchmark results match integration test results

## CI/CD Integration

For continuous integration - use the automated test script:

```yaml
# Example GitHub Actions workflow
- name: Build and Run Integration Tests
  run: ./scripts/run_integration_tests.sh

# Or for more control:
- name: Build Binaries
  run: |
    cargo build --release
    cd layer1-zig-ifr && zig build -Doptimize=ReleaseFast && cd ..
    cd layer3-go-alm && go build -o mfn-layer3-server && cd ..

- name: Run Integration Tests
  run: cargo test --release --test full_system_test

- name: Cleanup
  if: always()
  run: |
    pkill -f "layer.*_socket" || true
    rm -f /tmp/mfn_layer*.sock
```