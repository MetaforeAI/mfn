#!/bin/bash
# Start all MFN layer socket servers

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}===== Starting MFN Layer Socket Servers =====${NC}"

# Create temp directory for socket files
mkdir -p /tmp

# Kill any existing layer servers
echo -e "${YELLOW}Stopping any existing layer servers...${NC}"
pkill -f "mfn_layer1" || true
pkill -f "layer2_socket_server" || true
pkill -f "layer3_alm" || true
pkill -f "layer4_socket_server" || true
sleep 1

# Remove existing socket files
rm -f /tmp/mfn_layer*.sock

# Start Layer 1 (Zig IFR)
echo -e "${GREEN}Starting Layer 1 (Zig IFR)...${NC}"
cd layer1-zig-ifr || cd src/layers/layer1-ifr || { echo -e "${RED}Layer 1 directory not found${NC}"; exit 1; }
if [ -f "build.zig" ]; then
    zig build-exe src/socket_main.zig -O ReleaseFast 2>/dev/null || \
    zig build-exe src/socket_main.zig 2>/dev/null || \
    { echo -e "${RED}Failed to build Layer 1${NC}"; }

    if [ -f "socket_main" ]; then
        ./socket_main > /tmp/layer1.log 2>&1 &
        echo -e "${GREEN}✓ Layer 1 started (PID: $!)${NC}"
    else
        echo -e "${YELLOW}⚠ Layer 1 binary not found${NC}"
    fi
else
    echo -e "${YELLOW}⚠ Layer 1 build.zig not found${NC}"
fi
cd - > /dev/null

# Start Layer 2 (Rust DSR)
echo -e "${GREEN}Starting Layer 2 (Rust DSR)...${NC}"
cd layer2-rust-dsr || cd src/layers/layer2-dsr || { echo -e "${RED}Layer 2 directory not found${NC}"; exit 1; }
if cargo build --release --bin layer2_socket_server 2>/dev/null; then
    ./target/release/layer2_socket_server > /tmp/layer2.log 2>&1 &
    echo -e "${GREEN}✓ Layer 2 started (PID: $!)${NC}"
elif cargo build --bin layer2_socket_server 2>/dev/null; then
    ./target/debug/layer2_socket_server > /tmp/layer2.log 2>&1 &
    echo -e "${GREEN}✓ Layer 2 started in debug mode (PID: $!)${NC}"
else
    echo -e "${YELLOW}⚠ Failed to build Layer 2${NC}"
fi
cd - > /dev/null

# Start Layer 3 (Go ALM)
echo -e "${GREEN}Starting Layer 3 (Go ALM)...${NC}"
cd layer3-go-alm || cd src/layers/layer3-alm || { echo -e "${RED}Layer 3 directory not found${NC}"; exit 1; }
if [ -f "go.mod" ]; then
    go build -o layer3_alm main.go 2>/dev/null || { echo -e "${RED}Failed to build Layer 3${NC}"; }

    if [ -f "layer3_alm" ]; then
        ./layer3_alm > /tmp/layer3.log 2>&1 &
        echo -e "${GREEN}✓ Layer 3 started (PID: $!)${NC}"
    else
        echo -e "${YELLOW}⚠ Layer 3 binary not found${NC}"
    fi
else
    echo -e "${YELLOW}⚠ Layer 3 go.mod not found${NC}"
fi
cd - > /dev/null

# Start Layer 4 (Rust CPE)
echo -e "${GREEN}Starting Layer 4 (Rust CPE)...${NC}"
# Layer 4 is built at workspace level, so check workspace target directory
if [ -f "target/release/layer4_socket_server" ]; then
    ./target/release/layer4_socket_server > /tmp/layer4.log 2>&1 &
    echo -e "${GREEN}✓ Layer 4 started from workspace (PID: $!)${NC}"
elif [ -f "target/debug/layer4_socket_server" ]; then
    ./target/debug/layer4_socket_server > /tmp/layer4.log 2>&1 &
    echo -e "${GREEN}✓ Layer 4 started from workspace debug (PID: $!)${NC}"
else
    echo -e "${YELLOW}⚠ Layer 4 binary not found. Building...${NC}"
    if cargo build --release --bin layer4_socket_server 2>/dev/null; then
        ./target/release/layer4_socket_server > /tmp/layer4.log 2>&1 &
        echo -e "${GREEN}✓ Layer 4 started (PID: $!)${NC}"
    elif cargo build --bin layer4_socket_server 2>/dev/null; then
        ./target/debug/layer4_socket_server > /tmp/layer4.log 2>&1 &
        echo -e "${GREEN}✓ Layer 4 started in debug mode (PID: $!)${NC}"
    else
        echo -e "${YELLOW}⚠ Failed to build Layer 4${NC}"
    fi
fi

# Wait for sockets to be created
echo -e "${YELLOW}Waiting for socket files...${NC}"
sleep 2

# Check socket files
echo -e "${GREEN}Checking socket files:${NC}"
ls -la /tmp/mfn_layer*.sock 2>/dev/null || echo -e "${YELLOW}No socket files found yet${NC}"

# Show running processes
echo -e "${GREEN}Running layer processes:${NC}"
ps aux | grep -E "(mfn_layer|layer[1-4]_|socket_main)" | grep -v grep || echo -e "${YELLOW}No layer processes found${NC}"

echo -e "${GREEN}===== Layer startup complete =====${NC}"
echo -e "${YELLOW}Log files:${NC}"
echo "  Layer 1: /tmp/layer1.log"
echo "  Layer 2: /tmp/layer2.log"
echo "  Layer 3: /tmp/layer3.log"
echo "  Layer 4: /tmp/layer4.log"
echo ""
echo -e "${YELLOW}To stop all layers: pkill -f 'mfn_layer|layer[1-4]_'${NC}"