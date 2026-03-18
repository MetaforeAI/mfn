#!/bin/bash

# MFN System 1000+ QPS Validation Test Runner
# ==========================================
# This script sets up and runs the comprehensive 1000+ QPS validation test

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
MFN_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
LAYER3_BINARY="${MFN_DIR}/layer3-go-alm/layer3_alm"
TEST_SCRIPT="${MFN_DIR}/comprehensive_1000qps_test.py"
OPTIMIZED_CLIENT="${MFN_DIR}/optimized_mfn_client.py"

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to check if a port is in use
check_port() {
    local port=$1
    if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null; then
        return 0
    else
        return 1
    fi
}

# Function to wait for service to be ready
wait_for_service() {
    local url=$1
    local timeout=${2:-30}
    local count=0
    
    print_status "Waiting for service at $url to be ready..."
    
    while [ $count -lt $timeout ]; do
        if curl -s -f "$url/health" >/dev/null 2>&1; then
            print_success "Service at $url is ready"
            return 0
        fi
        sleep 1
        count=$((count + 1))
        echo -n "."
    done
    
    echo ""
    print_error "Service at $url failed to start within $timeout seconds"
    return 1
}

# Function to start a single MFN instance
start_mfn_instance() {
    local instance_id=$1
    local port=$2
    local metrics_port=$3
    
    print_status "Starting MFN instance $instance_id on port $port..."
    
    # Check if port is already in use
    if check_port $port; then
        print_warning "Port $port is already in use. Checking if it's our service..."
        if curl -s -f "http://localhost:$port/health" >/dev/null 2>&1; then
            print_success "MFN instance already running on port $port"
            return 0
        else
            print_error "Port $port is occupied by another service"
            return 1
        fi
    fi
    
    # Build the Layer 3 binary if it doesn't exist
    if [ ! -f "$LAYER3_BINARY" ]; then
        print_status "Building Layer 3 ALM binary..."
        cd "${MFN_DIR}/layer3-go-alm"
        go build -o layer3_alm ./main.go
        if [ ! -f "$LAYER3_BINARY" ]; then
            print_error "Failed to build Layer 3 ALM binary"
            return 1
        fi
    fi
    
    # Start the instance in background
    cd "${MFN_DIR}/layer3-go-alm"
    export LAYER3_PORT=$port
    export METRICS_PORT=$metrics_port
    export INSTANCE_ID=$instance_id
    
    ./layer3_alm > "/tmp/mfn_${instance_id}.log" 2>&1 &
    local pid=$!
    echo $pid > "/tmp/mfn_${instance_id}.pid"
    
    # Wait for the instance to be ready
    if wait_for_service "http://localhost:$port" 30; then
        print_success "MFN instance $instance_id started successfully (PID: $pid)"
        return 0
    else
        print_error "Failed to start MFN instance $instance_id"
        kill $pid 2>/dev/null || true
        return 1
    fi
}

# Function to stop MFN instances
stop_mfn_instances() {
    print_status "Stopping MFN instances..."
    
    for instance_id in mfn-1 mfn-2 mfn-3 mfn-4; do
        local pid_file="/tmp/${instance_id}.pid"
        if [ -f "$pid_file" ]; then
            local pid=$(cat "$pid_file")
            if kill -0 $pid 2>/dev/null; then
                print_status "Stopping $instance_id (PID: $pid)..."
                kill $pid
                sleep 2
                if kill -0 $pid 2>/dev/null; then
                    print_warning "Force killing $instance_id..."
                    kill -9 $pid
                fi
            fi
            rm -f "$pid_file"
        fi
    done
    
    print_success "MFN instances stopped"
}

# Function to run the performance test
run_performance_test() {
    print_status "Running comprehensive 1000+ QPS performance test..."
    
    # Make sure we have the required Python packages
    if ! python3 -c "import aiohttp, numpy, matplotlib" 2>/dev/null; then
        print_status "Installing required Python packages..."
        pip3 install aiohttp numpy matplotlib psutil
    fi
    
    # Run the comprehensive test
    cd "$MFN_DIR"
    if python3 "$TEST_SCRIPT"; then
        print_success "🎉 Performance test completed successfully!"
        return 0
    else
        local exit_code=$?
        case $exit_code in
            1)
                print_warning "⚠️ Partial success: Some performance targets met but 1000 QPS not achieved"
                ;;
            2)
                print_error "❌ Performance test failed: Low throughput achieved"
                ;;
            *)
                print_error "❌ Performance test failed with exit code $exit_code"
                ;;
        esac
        return $exit_code
    fi
}

# Function to show system information
show_system_info() {
    print_status "System Information:"
    echo "  OS: $(uname -a)"
    echo "  CPU: $(nproc) cores"
    echo "  Memory: $(free -h | grep '^Mem:' | awk '{print $2}')"
    echo "  Go version: $(go version 2>/dev/null || echo 'Not installed')"
    echo "  Python version: $(python3 --version)"
    echo ""
}

# Function to validate prerequisites
validate_prerequisites() {
    print_status "Validating prerequisites..."
    
    # Check if Go is installed
    if ! command -v go &> /dev/null; then
        print_error "Go is not installed. Please install Go to build the MFN system."
        return 1
    fi
    
    # Check if Python 3 is installed
    if ! command -v python3 &> /dev/null; then
        print_error "Python 3 is not installed."
        return 1
    fi
    
    # Check if curl is installed
    if ! command -v curl &> /dev/null; then
        print_error "curl is not installed."
        return 1
    fi
    
    # Check if the MFN directory exists
    if [ ! -d "$MFN_DIR" ]; then
        print_error "MFN directory not found at $MFN_DIR"
        return 1
    fi
    
    print_success "Prerequisites validated"
    return 0
}

# Function to setup test environment
setup_test_environment() {
    print_status "Setting up test environment..."
    
    # Create necessary directories
    mkdir -p "$MFN_DIR/results"
    mkdir -p "$MFN_DIR/logs"
    
    # Set up file permissions
    chmod +x "$TEST_SCRIPT" 2>/dev/null || true
    chmod +x "$OPTIMIZED_CLIENT" 2>/dev/null || true
    
    print_success "Test environment setup complete"
}

# Function to clean up
cleanup() {
    print_status "Cleaning up..."
    stop_mfn_instances
    
    # Clean up temporary files
    rm -f /tmp/mfn_*.pid /tmp/mfn_*.log
    
    print_success "Cleanup complete"
}

# Trap to ensure cleanup on exit
trap cleanup EXIT

# Main execution
main() {
    echo "======================================================================"
    echo "                  MFN System 1000+ QPS Validation"
    echo "======================================================================"
    echo ""
    
    # Show system information
    show_system_info
    
    # Validate prerequisites
    if ! validate_prerequisites; then
        exit 1
    fi
    
    # Setup test environment
    setup_test_environment
    
    # Parse command line arguments
    local mode="full"
    if [ $# -gt 0 ]; then
        mode=$1
    fi
    
    case $mode in
        "single")
            print_status "Running single instance test only..."
            if start_mfn_instance "mfn-1" 8082 9092; then
                sleep 5  # Give it time to fully initialize
                run_performance_test
            else
                print_error "Failed to start single instance"
                exit 1
            fi
            ;;
        "full"|*)
            print_status "Running full horizontal scaling test..."
            
            # Start all MFN instances
            local all_started=true
            
            start_mfn_instance "mfn-1" 8082 9092 || all_started=false
            start_mfn_instance "mfn-2" 8083 9093 || all_started=false
            start_mfn_instance "mfn-3" 8084 9094 || all_started=false
            start_mfn_instance "mfn-4" 8085 9095 || all_started=false
            
            if [ "$all_started" = true ]; then
                print_success "All MFN instances started successfully"
                sleep 10  # Give time for full initialization
                run_performance_test
            else
                print_error "Failed to start all MFN instances"
                exit 1
            fi
            ;;
    esac
    
    print_status "Test run completed. Check the results files for detailed analysis."
}

# Show usage if --help is provided
if [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    echo "Usage: $0 [mode]"
    echo ""
    echo "Modes:"
    echo "  full (default) - Run full horizontal scaling test with 4 instances"
    echo "  single        - Run single instance test only"
    echo ""
    echo "This script will:"
    echo "1. Start MFN instances on ports 8082-8085"
    echo "2. Run comprehensive throughput tests"
    echo "3. Generate performance reports and charts"
    echo "4. Validate 1000+ QPS target achievement"
    echo ""
    exit 0
fi

# Run main function
main "$@"