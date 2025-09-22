#!/bin/bash
# MFN Test Orchestrator Validation Script
# Quick validation to ensure the orchestrator is properly configured

echo "========================================"
echo "MFN Test Orchestrator Validation"
echo "========================================"
echo ""

# Check Python version
echo "Checking Python version..."
python_version=$(python3 --version 2>&1)
echo "  $python_version"

# Check if orchestrator script exists
echo ""
echo "Checking test orchestrator..."
if [ -f "test_orchestrator.py" ]; then
    echo "  ✓ test_orchestrator.py found"
else
    echo "  ✗ test_orchestrator.py not found"
    exit 1
fi

# Check for key test files
echo ""
echo "Checking test suite files..."
test_files=(
    "stress_test_framework.py"
    "comprehensive_test_system.py"
    "performance_benchmark_suite.py"
    "test_accuracy.py"
    "test_memory_functionality.py"
)

for file in "${test_files[@]}"; do
    if [ -f "$file" ]; then
        echo "  ✓ $file"
    else
        echo "  ✗ $file missing"
    fi
done

# Create necessary directories
echo ""
echo "Creating output directories..."
mkdir -p results reports exports logs reports/charts
echo "  ✓ Directories created"

# Run environment validation
echo ""
echo "Running environment validation..."
python3 test_orchestrator.py --validate-only

echo ""
echo "========================================"
echo "Validation complete!"
echo ""
echo "To run tests:"
echo "  Quick test:        python3 test_orchestrator.py"
echo "  Full test suite:   python3 test_orchestrator.py --run-all"
echo "  Parallel tests:    python3 test_orchestrator.py --run-all --parallel"
echo "  Export results:    python3 test_orchestrator.py --run-all --export"
echo "  Docker setup:      python3 test_orchestrator.py --docker"
echo "========================================"