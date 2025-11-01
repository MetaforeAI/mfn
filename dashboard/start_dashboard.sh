#!/bin/bash

# MFN Dashboard Startup Script
# Zero external dependencies - self-contained monitoring

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

echo "========================================="
echo "  MFN System Dashboard"
echo "  Native monitoring with zero dependencies"
echo "========================================="
echo ""

# Check if Go is installed
if ! command -v go &> /dev/null; then
    echo "Error: Go is not installed. Please install Go to run the metrics server."
    echo "However, you can still open the dashboard directly in a browser:"
    echo "  file://$SCRIPT_DIR/index.html"
    exit 1
fi

# Install dependencies
echo "Installing Go dependencies..."
go mod download

# Build the metrics server
echo "Building metrics server..."
go build -o metrics_server metrics_server.go

# Start the server
echo "Starting dashboard server on http://localhost:8080"
echo "Press Ctrl+C to stop the server"
echo ""

./metrics_server