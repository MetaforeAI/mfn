#!/bin/bash
set -e
IMAGE=${1:-memflow-engine:latest}
echo "Building MFN Engine..."
docker build -t "$IMAGE" .
echo "Built: $IMAGE"
