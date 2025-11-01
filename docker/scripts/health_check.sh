#!/bin/bash
# MFN System Health Check Script

set -e

# Configuration
SOCKET_DIR="${MFN_SOCKET_DIR:-/app/sockets}"
API_PORT="${MFN_API_PORT:-8080}"
DASHBOARD_PORT="${MFN_DASHBOARD_PORT:-3000}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Health check results
HEALTH_STATUS=0

echo "MFN System Health Check"
echo "======================="

# Function to check socket
check_socket() {
    local socket_path=$1
    local service_name=$2

    if [ -S "$socket_path" ]; then
        echo -e "${GREEN}✓${NC} $service_name socket exists"
        # Try to connect to socket
        if timeout 1 bash -c "echo '' | nc -U '$socket_path'" 2>/dev/null; then
            echo -e "${GREEN}  → Socket responsive${NC}"
        else
            echo -e "${YELLOW}  → Socket exists but not responsive${NC}"
            HEALTH_STATUS=1
        fi
    else
        echo -e "${RED}✗${NC} $service_name socket missing: $socket_path"
        HEALTH_STATUS=2
    fi
}

# Function to check HTTP endpoint
check_http() {
    local port=$1
    local service_name=$2
    local endpoint="${3:-/health}"

    if curl -f -s -o /dev/null -w "%{http_code}" "http://localhost:$port$endpoint" | grep -q "200"; then
        echo -e "${GREEN}✓${NC} $service_name HTTP endpoint healthy (port $port)"
    else
        echo -e "${RED}✗${NC} $service_name HTTP endpoint unhealthy (port $port)"
        HEALTH_STATUS=2
    fi
}

# Function to check process
check_process() {
    local process_name=$1
    local display_name=$2

    if pgrep -f "$process_name" > /dev/null; then
        echo -e "${GREEN}✓${NC} $display_name process running"
        # Get memory usage
        local mem_usage=$(ps aux | grep -v grep | grep "$process_name" | awk '{print $4}' | head -1)
        if [ ! -z "$mem_usage" ]; then
            echo -e "  → Memory usage: ${mem_usage}%"
        fi
    else
        echo -e "${RED}✗${NC} $display_name process not found"
        HEALTH_STATUS=2
    fi
}

# Check Layer Sockets
echo ""
echo "Layer Services:"
echo "--------------"
check_socket "$SOCKET_DIR/layer1.sock" "Layer 1 (IFR)"
check_socket "$SOCKET_DIR/layer2.sock" "Layer 2 (DSR)"
check_socket "$SOCKET_DIR/layer3.sock" "Layer 3 (ALM)"
check_socket "$SOCKET_DIR/layer4.sock" "Layer 4 (CPE)"

# Check Core Services
echo ""
echo "Core Services:"
echo "-------------"
check_process "supervisord" "Supervisor"
check_process "start_orchestrator" "MFN Orchestrator"
check_process "api_gateway" "API Gateway"
check_process "dashboard_server" "Dashboard Server"
check_process "persistence_daemon" "Persistence Manager"

# Check HTTP Endpoints
echo ""
echo "HTTP Endpoints:"
echo "--------------"
check_http $API_PORT "API Gateway"
check_http $DASHBOARD_PORT "Dashboard"
check_http 9090 "Metrics" "/metrics"

# Check Data Persistence
echo ""
echo "Data Persistence:"
echo "----------------"
if [ -f "/app/data/mfn_memories.db" ]; then
    DB_SIZE=$(du -h "/app/data/mfn_memories.db" | cut -f1)
    echo -e "${GREEN}✓${NC} Database exists (size: $DB_SIZE)"

    # Check if database is accessible
    if sqlite3 "/app/data/mfn_memories.db" "SELECT COUNT(*) FROM memories;" 2>/dev/null; then
        MEMORY_COUNT=$(sqlite3 "/app/data/mfn_memories.db" "SELECT COUNT(*) FROM memories;" 2>/dev/null)
        echo -e "  → Memories stored: $MEMORY_COUNT"
    fi
else
    echo -e "${YELLOW}!${NC} Database not initialized yet"
fi

# Check Log Files
echo ""
echo "Log Files:"
echo "---------"
if [ -d "/app/logs" ]; then
    LOG_COUNT=$(ls -1 /app/logs/*.log 2>/dev/null | wc -l)
    if [ $LOG_COUNT -gt 0 ]; then
        echo -e "${GREEN}✓${NC} Log files present ($LOG_COUNT files)"

        # Check for recent errors
        ERROR_COUNT=$(grep -c ERROR /app/logs/*.log 2>/dev/null || echo "0")
        if [ "$ERROR_COUNT" -gt "0" ]; then
            echo -e "${YELLOW}  → Recent errors found: $ERROR_COUNT${NC}"
        fi
    else
        echo -e "${YELLOW}!${NC} No log files found"
    fi
else
    echo -e "${RED}✗${NC} Log directory missing"
    HEALTH_STATUS=2
fi

# System Resources
echo ""
echo "System Resources:"
echo "----------------"
# Memory usage
MEM_TOTAL=$(free -m | awk 'NR==2{print $2}')
MEM_USED=$(free -m | awk 'NR==2{print $3}')
MEM_PERCENT=$((MEM_USED * 100 / MEM_TOTAL))

if [ $MEM_PERCENT -lt 80 ]; then
    echo -e "${GREEN}✓${NC} Memory usage: ${MEM_USED}MB / ${MEM_TOTAL}MB (${MEM_PERCENT}%)"
else
    echo -e "${YELLOW}!${NC} High memory usage: ${MEM_USED}MB / ${MEM_TOTAL}MB (${MEM_PERCENT}%)"
    HEALTH_STATUS=1
fi

# CPU load
LOAD=$(uptime | awk -F'load average:' '{print $2}')
echo -e "  → CPU load average:$LOAD"

# Overall Health Status
echo ""
echo "======================="
if [ $HEALTH_STATUS -eq 0 ]; then
    echo -e "${GREEN}SYSTEM HEALTHY${NC}"
elif [ $HEALTH_STATUS -eq 1 ]; then
    echo -e "${YELLOW}SYSTEM DEGRADED${NC}"
else
    echo -e "${RED}SYSTEM UNHEALTHY${NC}"
fi

exit $HEALTH_STATUS