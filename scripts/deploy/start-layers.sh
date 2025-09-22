#!/bin/bash

# MFN System: Start All Layer Servers
# This script starts all 4 MFN layers with Unix socket interfaces

echo "🧠 Starting MFN System - All Layers"
echo "====================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to check if a socket exists and is accessible
check_socket() {
    local socket_path=$1
    local layer_name=$2
    
    if [ -S "$socket_path" ]; then
        echo -e "${GREEN}✅ $layer_name socket ready at $socket_path${NC}"
        return 0
    else
        echo -e "${RED}❌ $layer_name socket not found at $socket_path${NC}"
        return 1
    fi
}

# Function to start a layer server
start_layer() {
    local layer_num=$1
    local layer_name=$2
    local start_command=$3
    local socket_path=$4
    local working_dir=$5
    
    echo -e "${BLUE}🚀 Starting $layer_name...${NC}"
    
    # Change to layer directory
    if [ -d "$working_dir" ]; then
        cd "$working_dir"
    else
        echo -e "${RED}❌ Layer $layer_num directory not found: $working_dir${NC}"
        return 1
    fi
    
    # Remove existing socket
    if [ -S "$socket_path" ]; then
        rm -f "$socket_path"
        echo "   Removed existing socket: $socket_path"
    fi
    
    # Start the server in background
    eval "$start_command" &
    local pid=$!
    echo "   Process ID: $pid"
    echo $pid > "/tmp/mfn_layer${layer_num}.pid"
    
    # Wait for socket to be created (up to 10 seconds)
    local attempts=0
    while [ $attempts -lt 20 ]; do
        if [ -S "$socket_path" ]; then
            echo -e "${GREEN}   ✅ $layer_name server started successfully${NC}"
            return 0
        fi
        sleep 0.5
        attempts=$((attempts + 1))
    done
    
    echo -e "${RED}   ❌ $layer_name failed to start (socket not created)${NC}"
    return 1
}

# Function to stop all layers
stop_all_layers() {
    echo -e "${YELLOW}🛑 Stopping all MFN layers...${NC}"
    
    for i in {1..4}; do
        if [ -f "/tmp/mfn_layer${i}.pid" ]; then
            local pid=$(cat "/tmp/mfn_layer${i}.pid")
            if kill -0 "$pid" 2>/dev/null; then
                kill "$pid"
                echo "   Stopped Layer $i (PID: $pid)"
            fi
            rm -f "/tmp/mfn_layer${i}.pid"
        fi
        
        # Remove socket files
        rm -f "/tmp/mfn_layer${i}.sock"
    done
    
    echo -e "${GREEN}✅ All layers stopped${NC}"
}

# Check for stop command
if [ "$1" = "stop" ]; then
    stop_all_layers
    exit 0
fi

# Check for restart command
if [ "$1" = "restart" ]; then
    stop_all_layers
    sleep 2
    echo ""
fi

echo "Starting MFN layers in optimized order..."
echo ""

# Layer 1: Immediate Flow Registry (Zig) - Ultra-fast exact matching
echo "Layer 1: Immediate Flow Registry"
if start_layer 1 "Layer 1 (IFR)" "./zig-out/bin/ifr_socket_server" "/tmp/mfn_layer1.sock" "layer1-zig-ifr"; then
    sleep 1
else
    echo -e "${YELLOW}⚠️  Layer 1 failed to start, continuing with other layers${NC}"
fi
echo ""

# Layer 2: Dynamic Similarity Reservoir (Rust) - Neural similarity
echo "Layer 2: Dynamic Similarity Reservoir"
if start_layer 2 "Layer 2 (DSR)" "./target/release/layer2_socket_server" "/tmp/mfn_layer2.sock" "layer2-rust-dsr"; then
    sleep 1
else
    echo -e "${YELLOW}⚠️  Layer 2 failed to start, continuing with other layers${NC}"
fi
echo ""

# Layer 3: Associative Link Mesh (Go) - Already running optimized version
echo "Layer 3: Associative Link Mesh"
if check_socket "/tmp/mfn_layer3.sock" "Layer 3 (ALM)"; then
    echo -e "${BLUE}   Layer 3 already running (optimized version)${NC}"
else
    echo -e "${YELLOW}⚠️  Layer 3 not running, please start manually: ./layer3_alm_optimized${NC}"
fi
echo ""

# Layer 4: Context Prediction Engine (Rust) - Temporal patterns
echo "Layer 4: Context Prediction Engine"
if start_layer 4 "Layer 4 (CPE)" "./target/release/layer4-socket-server" "/tmp/mfn_layer4.sock" "layer4-context-engine"; then
    sleep 1
else
    echo -e "${YELLOW}⚠️  Layer 4 failed to start, continuing with other layers${NC}"
fi
echo ""

# Final system status check
echo "🔍 Final System Status Check:"
echo "=============================="

# Check all sockets
layers_running=0

if check_socket "/tmp/mfn_layer1.sock" "Layer 1 (IFR)"; then
    layers_running=$((layers_running + 1))
fi

if check_socket "/tmp/mfn_layer2.sock" "Layer 2 (DSR)"; then
    layers_running=$((layers_running + 1))
fi

if check_socket "/tmp/mfn_layer3.sock" "Layer 3 (ALM)"; then
    layers_running=$((layers_running + 1))
fi

if check_socket "/tmp/mfn_layer4.sock" "Layer 4 (CPE)"; then
    layers_running=$((layers_running + 1))
fi

echo ""

if [ $layers_running -eq 4 ]; then
    echo -e "${GREEN}🎉 ALL 4 MFN LAYERS RUNNING SUCCESSFULLY!${NC}"
    echo -e "${GREEN}   Ready for high-performance unified socket operations${NC}"
    
    # Display connection information
    echo ""
    echo "Socket Connections:"
    echo "  Layer 1 (IFR): /tmp/mfn_layer1.sock (Ultra-fast exact matching)"
    echo "  Layer 2 (DSR): /tmp/mfn_layer2.sock (Neural similarity search)"
    echo "  Layer 3 (ALM): /tmp/mfn_layer3.sock (Associative graph search)"
    echo "  Layer 4 (CPE): /tmp/mfn_layer4.sock (Context prediction)"
    
    echo ""
    echo "Test the system:"
    echo "  python3 unified_socket_client.py"
    echo ""
    echo "Stop all layers:"
    echo "  ./start_all_layers.sh stop"
    
elif [ $layers_running -gt 0 ]; then
    echo -e "${YELLOW}⚠️  PARTIAL SUCCESS: $layers_running/4 layers running${NC}"
    echo -e "${YELLOW}   System will work with reduced functionality${NC}"
else
    echo -e "${RED}❌ SYSTEM STARTUP FAILED: No layers are running${NC}"
    echo -e "${RED}   Check error messages above for troubleshooting${NC}"
fi

echo ""
echo "🧠 MFN System startup complete"

# Keep script running to show real-time status (optional)
if [ "$1" = "monitor" ]; then
    echo ""
    echo "📊 Monitoring mode - Press Ctrl+C to exit"
    echo "========================================"
    
    while true; do
        sleep 5
        echo -n "$(date): "
        running_count=0
        
        for i in {1..4}; do
            if [ -S "/tmp/mfn_layer${i}.sock" ]; then
                running_count=$((running_count + 1))
            fi
        done
        
        echo "$running_count/4 layers active"
        
        if [ $running_count -eq 0 ]; then
            echo -e "${RED}All layers down - exiting monitor mode${NC}"
            break
        fi
    done
fi