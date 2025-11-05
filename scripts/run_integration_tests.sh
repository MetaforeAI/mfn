#!/bin/bash
# Automated integration test runner
# Builds all binaries and runs tests with automatic server management

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}===== MFN Integration Test Suite =====${NC}"
echo ""

# Step 1: Build all binaries
echo -e "${GREEN}[1/3] Building binaries...${NC}"

# Build Rust binaries (workspace build includes all binaries)
echo -e "${YELLOW}  Building Rust workspace...${NC}"
cargo build --release || {
    echo -e "${RED}✗ Failed to build Rust workspace${NC}"
    exit 1
}
echo -e "${GREEN}  ✓ Rust workspace built${NC}"

# Build Zig binary (Layer 1)
echo -e "${YELLOW}  Building Zig binary...${NC}"
if [ -f "layer1-zig-ifr/zig-out/bin/ifr_socket_server" ]; then
    echo -e "${GREEN}  ✓ Zig binary already exists (using existing)${NC}"
else
    echo -e "${YELLOW}  ⚠️  Layer 1 binary not found${NC}"
    echo -e "${YELLOW}     Build manually: cd layer1-zig-ifr && zig build -Doptimize=ReleaseFast${NC}"
fi

# Build Go binary (Layer 3)
echo -e "${YELLOW}  Building Go binary...${NC}"
cd layer3-go-alm
go build -o layer3_alm || {
    echo -e "${RED}✗ Failed to build Go binary${NC}"
    exit 1
}
cd ..
echo -e "${GREEN}  ✓ Go binary built${NC}"

echo ""
echo -e "${GREEN}[2/3] Running integration tests...${NC}"
echo -e "${YELLOW}  (Test harness will automatically start/stop servers)${NC}"
echo ""

# Step 2: Run integration tests
# The test harness will automatically:
# - Clean up old sockets
# - Start all layer servers
# - Wait for health checks
# - Run tests
# - Stop servers and cleanup
cargo test --release --test full_system_test -- --nocapture || {
    echo ""
    echo -e "${RED}✗ Integration tests failed${NC}"
    exit 1
}

echo ""
echo -e "${GREEN}[3/3] Cleanup...${NC}"

# Step 3: Emergency cleanup (in case tests crashed)
# Kill any lingering processes
pkill -f "ifr_socket_server" 2>/dev/null || true
pkill -f "layer2_socket_server" 2>/dev/null || true
pkill -f "layer3_alm" 2>/dev/null || true
pkill -f "layer4_socket_server" 2>/dev/null || true

# Remove any lingering sockets
rm -f /tmp/mfn_layer*.sock 2>/dev/null || true

echo -e "${GREEN}  ✓ Cleanup complete${NC}"
echo ""
echo -e "${GREEN}===== Integration tests completed successfully! =====${NC}"
