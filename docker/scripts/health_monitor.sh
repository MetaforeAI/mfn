#!/bin/bash
# MFN Health Monitor - Continuous monitoring daemon

set -e

# Configuration
MONITOR_INTERVAL=${MFN_MONITOR_INTERVAL:-30}
ALERT_THRESHOLD_CPU=80
ALERT_THRESHOLD_MEM=85
ALERT_THRESHOLD_DISK=90
LOG_FILE="/app/logs/health_monitor.log"

# Initialize
echo "MFN Health Monitor Started - $(date)" >> $LOG_FILE

# Function to log
log_message() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" >> $LOG_FILE
}

# Function to send alert (could integrate with alerting system)
send_alert() {
    local severity=$1
    local message=$2

    log_message "ALERT [$severity]: $message"

    # In production, this would send to alerting system
    # Example: curl -X POST alerting-endpoint...
}

# Function to check service health
check_service() {
    local service_name=$1
    local check_command=$2

    if eval "$check_command" > /dev/null 2>&1; then
        return 0
    else
        send_alert "WARNING" "Service $service_name is unhealthy"
        return 1
    fi
}

# Function to check resource usage
check_resources() {
    # CPU usage
    cpu_usage=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1 | cut -d'.' -f1)
    if [ "$cpu_usage" -gt "$ALERT_THRESHOLD_CPU" ]; then
        send_alert "WARNING" "High CPU usage: ${cpu_usage}%"
    fi

    # Memory usage
    mem_usage=$(free | grep Mem | awk '{print int($3/$2 * 100)}')
    if [ "$mem_usage" -gt "$ALERT_THRESHOLD_MEM" ]; then
        send_alert "WARNING" "High memory usage: ${mem_usage}%"
    fi

    # Disk usage
    disk_usage=$(df /app/data | tail -1 | awk '{print int($5)}')
    if [ "$disk_usage" -gt "$ALERT_THRESHOLD_DISK" ]; then
        send_alert "CRITICAL" "High disk usage: ${disk_usage}%"
    fi
}

# Function to collect metrics
collect_metrics() {
    local metrics_file="/app/logs/metrics_$(date +%Y%m%d).json"

    # Gather metrics
    cpu_usage=$(top -bn1 | grep "Cpu(s)" | awk '{print $2}' | cut -d'%' -f1)
    mem_usage=$(free | grep Mem | awk '{print int($3/$2 * 100)}')
    disk_usage=$(df /app/data | tail -1 | awk '{print int($5)}')

    # Count processes
    layer1_count=$(pgrep -f layer1_socket_server | wc -l)
    layer2_count=$(pgrep -f layer2_socket_server | wc -l)
    layer3_count=$(pgrep -f layer3_server | wc -l)
    layer4_count=$(pgrep -f layer4_socket_server | wc -l)

    # Create metrics JSON
    cat > "$metrics_file.tmp" <<EOF
{
    "timestamp": "$(date -Iseconds)",
    "resources": {
        "cpu_percent": $cpu_usage,
        "memory_percent": $mem_usage,
        "disk_percent": $disk_usage
    },
    "processes": {
        "layer1": $layer1_count,
        "layer2": $layer2_count,
        "layer3": $layer3_count,
        "layer4": $layer4_count
    }
}
EOF

    # Append to metrics file
    if [ -f "$metrics_file" ]; then
        echo "," >> "$metrics_file"
        cat "$metrics_file.tmp" >> "$metrics_file"
    else
        echo "[" > "$metrics_file"
        cat "$metrics_file.tmp" >> "$metrics_file"
    fi

    rm "$metrics_file.tmp"
}

# Function to auto-recover services
auto_recover() {
    local service=$1

    log_message "Attempting to recover $service"

    case $service in
        "layer1")
            supervisorctl restart layer1_ifr
            ;;
        "layer2")
            supervisorctl restart layer2_dsr
            ;;
        "layer3")
            supervisorctl restart layer3_alm
            ;;
        "layer4")
            supervisorctl restart layer4_cpe
            ;;
        "api")
            supervisorctl restart mfn_api
            ;;
        *)
            log_message "Unknown service: $service"
            ;;
    esac

    sleep 5

    # Verify recovery
    if supervisorctl status $service | grep -q RUNNING; then
        log_message "Service $service recovered successfully"
        return 0
    else
        send_alert "CRITICAL" "Failed to recover service $service"
        return 1
    fi
}

# Main monitoring loop
log_message "Starting continuous monitoring (interval: ${MONITOR_INTERVAL}s)"

while true; do
    # Check critical services
    services_healthy=true

    # Check layer sockets
    for socket in layer1 layer2 layer3 layer4; do
        if [ ! -S "/app/sockets/${socket}.sock" ]; then
            log_message "Socket missing: ${socket}.sock"
            auto_recover "$socket"
            services_healthy=false
        fi
    done

    # Check API endpoint
    if ! curl -f -s http://localhost:8080/health > /dev/null 2>&1; then
        log_message "API health check failed"
        auto_recover "api"
        services_healthy=false
    fi

    # Check supervisor
    if ! supervisorctl status > /dev/null 2>&1; then
        send_alert "CRITICAL" "Supervisor is not responding"
        services_healthy=false
    fi

    # Check resources
    check_resources

    # Collect metrics
    collect_metrics

    # Log status
    if $services_healthy; then
        log_message "All services healthy"
    else
        log_message "Service issues detected, recovery attempted"
    fi

    # Check for stale processes
    for pid in $(find /app/logs -name "*.pid" -exec cat {} \;); do
        if ! kill -0 $pid 2>/dev/null; then
            log_message "Stale PID found: $pid"
            rm -f /app/logs/*.pid
        fi
    done

    # Rotate log if too large
    if [ -f "$LOG_FILE" ]; then
        log_size=$(du -m "$LOG_FILE" | cut -f1)
        if [ "$log_size" -gt 100 ]; then
            mv "$LOG_FILE" "${LOG_FILE}.$(date +%Y%m%d_%H%M%S)"
            echo "Log rotated - $(date)" > "$LOG_FILE"
        fi
    fi

    # Sleep until next check
    sleep $MONITOR_INTERVAL
done