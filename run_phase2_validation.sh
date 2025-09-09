#!/bin/bash

# MFN Phase 2 Performance Validation Test Runner
# ==============================================
# 
# Comprehensive test runner for MFN Phase 2 performance validation framework.
# This script provides easy access to all validation features and test scenarios.
#
# Usage Examples:
#   ./run_phase2_validation.sh --comprehensive     # Full validation suite
#   ./run_phase2_validation.sh --monitoring        # Start continuous monitoring  
#   ./run_phase2_validation.sh --regression        # Quick regression test
#   ./run_phase2_validation.sh --migration         # Start migration process

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRAMEWORK_SCRIPT="$SCRIPT_DIR/mfn_phase2_validation_framework.py"
MONITORING_SCRIPT="$SCRIPT_DIR/performance_monitoring_daemon.py"
MIGRATION_SCRIPT="$SCRIPT_DIR/migration_orchestrator.py"
LOG_DIR="/tmp/mfn_validation_logs"
RESULTS_DIR="/tmp/mfn_validation_results"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Logging setup
mkdir -p "$LOG_DIR" "$RESULTS_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$LOG_DIR/validation_run_$TIMESTAMP.log"

log() {
    echo -e "$1" | tee -a "$LOG_FILE"
}

log_info() {
    log "${BLUE}[INFO]${NC} $1"
}

log_success() {
    log "${GREEN}[SUCCESS]${NC} $1"
}

log_warning() {
    log "${YELLOW}[WARNING]${NC} $1"
}

log_error() {
    log "${RED}[ERROR]${NC} $1"
}

log_header() {
    log "\n${CYAN}========================================${NC}"
    log "${CYAN}$1${NC}"
    log "${CYAN}========================================${NC}\n"
}

# Function to check prerequisites
check_prerequisites() {
    log_header "Checking Prerequisites"
    
    local all_good=true
    
    # Check Python dependencies
    log_info "Checking Python dependencies..."
    
    if ! python3 -c "import requests, psutil, numpy, sqlite3, msgpack" 2>/dev/null; then
        log_error "Missing Python dependencies. Install with:"
        log_error "pip install requests psutil numpy msgpack"
        all_good=false
    else
        log_success "Python dependencies OK"
    fi
    
    # Check if framework scripts exist
    if [[ ! -f "$FRAMEWORK_SCRIPT" ]]; then
        log_error "Framework script not found: $FRAMEWORK_SCRIPT"
        all_good=false
    else
        log_success "Validation framework found"
    fi
    
    if [[ ! -f "$MONITORING_SCRIPT" ]]; then
        log_error "Monitoring daemon script not found: $MONITORING_SCRIPT"
        all_good=false
    else
        log_success "Monitoring daemon found"
    fi
    
    if [[ ! -f "$MIGRATION_SCRIPT" ]]; then
        log_error "Migration orchestrator script not found: $MIGRATION_SCRIPT"
        all_good=false
    else
        log_success "Migration orchestrator found"
    fi
    
    # Check system resources
    log_info "Checking system resources..."
    
    local cpu_count=$(nproc)
    local mem_gb=$(free -g | awk 'NR==2{printf "%.1f", $7}')
    local disk_available=$(df . | tail -1 | awk '{print $4}')
    
    log_info "Available resources:"
    log_info "  CPU cores: $cpu_count"
    log_info "  Available memory: ${mem_gb}GB"
    log_info "  Available disk: ${disk_available}KB"
    
    if [[ $cpu_count -lt 4 ]]; then
        log_warning "Low CPU count: $cpu_count (recommended: 4+)"
    fi
    
    if [[ $(echo "$mem_gb < 8" | bc -l 2>/dev/null || echo "1") == "1" ]]; then
        log_warning "Low memory: ${mem_gb}GB (recommended: 8GB+)"
    fi
    
    # Check for running services
    log_info "Checking for running MFN services..."
    
    local services_found=0
    for port in 8080 8081 8082 8084; do
        if ss -tuln | grep -q ":$port "; then
            log_success "Service found on port $port"
            ((services_found++))
        else
            log_warning "No service found on port $port"
        fi
    done
    
    if [[ $services_found -eq 0 ]]; then
        log_warning "No MFN services detected. Some tests may fail."
        log_info "Start services manually or use --mock-services flag"
    else
        log_success "$services_found MFN services detected"
    fi
    
    if [[ "$all_good" == "true" ]]; then
        log_success "All prerequisites satisfied"
        return 0
    else
        log_error "Prerequisites check failed"
        return 1
    fi
}

# Function to run comprehensive validation
run_comprehensive_validation() {
    log_header "Running Comprehensive Performance Validation"
    
    local output_file="$RESULTS_DIR/comprehensive_validation_$TIMESTAMP.json"
    
    log_info "Starting comprehensive validation suite..."
    log_info "Results will be saved to: $output_file"
    
    if python3 "$FRAMEWORK_SCRIPT" --test comprehensive 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Comprehensive validation completed"
        
        # Check if results file exists and display summary
        if [[ -f "$output_file" ]]; then
            log_info "Parsing results..."
            python3 -c "
import json
import sys
try:
    with open('$output_file', 'r') as f:
        data = json.load(f)
    
    summary = data.get('summary', {})
    status = summary.get('validation_status', 'UNKNOWN')
    
    print(f'\\n🎯 VALIDATION SUMMARY')
    print(f'Status: {status}')
    
    achievements = summary.get('key_achievements', {})
    if achievements:
        print(f'\\n📊 Key Achievements:')
        for key, value in achievements.items():
            print(f'  • {key}: {value}')
    
    findings = summary.get('critical_findings', [])
    if findings:
        print(f'\\n🔍 Critical Findings:')
        for finding in findings:
            print(f'  • {finding}')
            
    next_steps = summary.get('next_steps', [])
    if next_steps:
        print(f'\\n📋 Next Steps:')
        for step in next_steps:
            print(f'  • {step}')
    
except Exception as e:
    print(f'Error parsing results: {e}')
    sys.exit(1)
" 2>/dev/null || log_warning "Could not parse results summary"
        fi
        
        return 0
    else
        log_error "Comprehensive validation failed"
        return 1
    fi
}

# Function to run regression testing
run_regression_test() {
    log_header "Running Performance Regression Test"
    
    log_info "Testing for performance regressions..."
    
    if python3 "$FRAMEWORK_SCRIPT" --test regression --duration 60 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Regression test completed"
        return 0
    else
        log_error "Regression test failed"
        return 1
    fi
}

# Function to run protocol comparison
run_protocol_comparison() {
    log_header "Running Protocol Comparison Test"
    
    log_info "Comparing HTTP vs Unix Socket vs Binary protocol performance..."
    
    if python3 "$FRAMEWORK_SCRIPT" --test comparison --duration 90 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Protocol comparison completed"
        return 0
    else
        log_error "Protocol comparison failed"
        return 1
    fi
}

# Function to run load testing
run_load_test() {
    local max_qps=${1:-5000}
    
    log_header "Running Load Test Suite (up to $max_qps QPS)"
    
    log_info "Testing system load capacity..."
    
    if python3 "$FRAMEWORK_SCRIPT" --test load --max-qps "$max_qps" --duration 120 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Load test completed"
        return 0
    else
        log_error "Load test failed"
        return 1
    fi
}

# Function to start monitoring daemon
start_monitoring() {
    local interval=${1:-30}
    
    log_header "Starting Performance Monitoring Daemon"
    
    log_info "Starting continuous monitoring (interval: ${interval}s)..."
    log_info "Press Ctrl+C to stop monitoring"
    
    if python3 "$MONITORING_SCRIPT" --interval "$interval" 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Monitoring daemon stopped"
        return 0
    else
        log_error "Monitoring daemon failed"
        return 1
    fi
}

# Function to show monitoring status
show_monitoring_status() {
    log_header "Performance Monitoring Status"
    
    if python3 "$MONITORING_SCRIPT" --status 2>&1 | tee -a "$LOG_FILE"; then
        return 0
    else
        log_error "Could not retrieve monitoring status"
        return 1
    fi
}

# Function to start migration process
start_migration() {
    log_header "Starting MFN Phase 2 Migration"
    
    log_warning "⚠️  This will start the actual migration process!"
    log_warning "⚠️  Ensure you have proper backups and are ready for potential downtime."
    
    read -p "Do you want to continue? (y/N): " -n 1 -r
    echo
    
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        log_info "Starting migration orchestrator..."
        
        if python3 "$MIGRATION_SCRIPT" 2>&1 | tee -a "$LOG_FILE"; then
            log_success "Migration completed successfully"
            return 0
        else
            log_error "Migration failed"
            return 1
        fi
    else
        log_info "Migration cancelled by user"
        return 0
    fi
}

# Function to run dry migration
run_migration_dry_run() {
    log_header "Running Migration Dry Run"
    
    log_info "Testing migration readiness (no actual changes)..."
    
    if python3 "$MIGRATION_SCRIPT" --dry-run 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Migration dry run completed"
        return 0
    else
        log_error "Migration dry run failed"
        return 1
    fi
}

# Function to show migration status
show_migration_status() {
    log_header "Migration Status"
    
    if python3 "$MIGRATION_SCRIPT" --status 2>&1 | tee -a "$LOG_FILE"; then
        return 0
    else
        log_error "Could not retrieve migration status"
        return 1
    fi
}

# Function to generate performance report
generate_report() {
    log_header "Generating Performance Report"
    
    log_info "Generating migration readiness report..."
    
    if python3 "$FRAMEWORK_SCRIPT" --report-only 2>&1 | tee -a "$LOG_FILE"; then
        log_success "Performance report generated"
        return 0
    else
        log_error "Performance report generation failed"
        return 1
    fi
}

# Function to cleanup old logs and results
cleanup() {
    log_header "Cleaning Up Old Files"
    
    log_info "Removing files older than 7 days..."
    
    find "$LOG_DIR" -type f -mtime +7 -delete 2>/dev/null || true
    find "$RESULTS_DIR" -type f -mtime +7 -delete 2>/dev/null || true
    
    # Clean up database files
    find /tmp -name "mfn_performance_validation*.db" -mtime +7 -delete 2>/dev/null || true
    
    log_success "Cleanup completed"
}

# Function to show usage information
show_usage() {
    echo "MFN Phase 2 Performance Validation Test Runner"
    echo "=============================================="
    echo
    echo "Usage: $0 [OPTION]"
    echo
    echo "Main Test Options:"
    echo "  --comprehensive          Run complete validation suite (recommended)"
    echo "  --regression            Run performance regression test"
    echo "  --comparison            Run protocol comparison test"
    echo "  --load [MAX_QPS]        Run load test suite (default: 5000 QPS)"
    echo "  --integration           Run end-to-end integration test"
    echo
    echo "Monitoring Options:"
    echo "  --monitoring [INTERVAL]  Start continuous monitoring (default: 30s)"
    echo "  --monitor-status        Show monitoring daemon status"
    echo
    echo "Migration Options:"
    echo "  --migration             Start actual migration process ⚠️"
    echo "  --migration-dry-run     Test migration readiness (no changes)"
    echo "  --migration-status      Show migration status"
    echo
    echo "Utility Options:"
    echo "  --report                Generate performance report"
    echo "  --check                 Check prerequisites only"
    echo "  --cleanup               Remove old logs and results"
    echo "  --help                  Show this help message"
    echo
    echo "Configuration:"
    echo "  --mock-services         Use mock services for testing"
    echo "  --debug                 Enable debug logging"
    echo "  --quiet                 Suppress non-essential output"
    echo
    echo "Examples:"
    echo "  $0 --comprehensive                    # Full validation"
    echo "  $0 --regression                       # Quick regression check"
    echo "  $0 --load 3000                        # Load test up to 3000 QPS"
    echo "  $0 --monitoring 60                    # Monitor every 60 seconds"
    echo "  $0 --migration-dry-run                # Test migration readiness"
    echo
    echo "Output:"
    echo "  Logs: $LOG_DIR/"
    echo "  Results: $RESULTS_DIR/"
}

# Main execution function
main() {
    local start_time=$(date +%s)
    
    log_header "MFN Phase 2 Performance Validation"
    log_info "Started at: $(date)"
    log_info "Log file: $LOG_FILE"
    
    # Parse command line arguments
    case "${1:-}" in
        --comprehensive)
            check_prerequisites && run_comprehensive_validation
            ;;
        --regression)
            check_prerequisites && run_regression_test
            ;;
        --comparison)
            check_prerequisites && run_protocol_comparison
            ;;
        --load)
            local max_qps=${2:-5000}
            check_prerequisites && run_load_test "$max_qps"
            ;;
        --integration)
            check_prerequisites && \
            python3 "$FRAMEWORK_SCRIPT" --test integration --duration 120
            ;;
        --monitoring)
            local interval=${2:-30}
            check_prerequisites && start_monitoring "$interval"
            ;;
        --monitor-status)
            show_monitoring_status
            ;;
        --migration)
            check_prerequisites && start_migration
            ;;
        --migration-dry-run)
            check_prerequisites && run_migration_dry_run
            ;;
        --migration-status)
            show_migration_status
            ;;
        --report)
            generate_report
            ;;
        --check)
            check_prerequisites
            ;;
        --cleanup)
            cleanup
            ;;
        --help|help|-h)
            show_usage
            exit 0
            ;;
        "")
            log_error "No option specified"
            echo
            show_usage
            exit 1
            ;;
        *)
            log_error "Unknown option: $1"
            echo
            show_usage
            exit 1
            ;;
    esac
    
    local exit_code=$?
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))
    
    log_info "Completed at: $(date)"
    log_info "Duration: ${duration}s"
    
    if [[ $exit_code -eq 0 ]]; then
        log_success "Operation completed successfully"
    else
        log_error "Operation failed (exit code: $exit_code)"
    fi
    
    return $exit_code
}

# Run main function with all arguments
main "$@"