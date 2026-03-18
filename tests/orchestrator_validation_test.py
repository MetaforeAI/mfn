#!/usr/bin/env python3
"""
Orchestrator Validation Test
Tests the MFN orchestrator coordinating all 4 layers
"""

import json
import time
import socket
import requests
import statistics
import concurrent.futures
from typing import Dict, List, Optional

def test_orchestrator_health() -> bool:
    """Test if orchestrator is running"""
    try:
        response = requests.get("http://localhost:8080/health", timeout=1)
        if response.status_code == 200:
            data = response.json()
            print(f"✅ Orchestrator healthy: {data}")
            return True
    except:
        pass
    print("❌ Orchestrator not running")
    return False

def start_orchestrator():
    """Start the orchestrator if not running"""
    print("\n" + "="*60)
    print("STARTING ORCHESTRATOR")
    print("="*60)

    import subprocess
    import os

    # Kill any existing gateway/orchestrator
    subprocess.run(["pkill", "-f", "mfn-gateway"], stderr=subprocess.DEVNULL)
    time.sleep(1)

    # Start new orchestrator (mfn-gateway)
    env = os.environ.copy()
    env["RUST_LOG"] = "info"

    proc = subprocess.Popen(
        ["./target/release/mfn-gateway"],
        cwd=".",
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE
    )

    # Wait for startup
    time.sleep(3)

    if proc.poll() is None:
        print("✅ Orchestrator started")
        return proc
    else:
        print("❌ Failed to start orchestrator")
        stderr = proc.stderr.read().decode() if proc.stderr else ""
        print(f"Error: {stderr[:500]}")
        return None

def test_orchestrator_operations():
    """Test orchestrator coordinating all layers"""
    print("\n" + "="*60)
    print("ORCHESTRATOR COORDINATION TEST")
    print("="*60)

    base_url = "http://localhost:8080"

    # Test 1: Add memories through orchestrator
    print("\n📝 Adding memories through orchestrator:")
    memories = [
        {"content": "Quantum computing breakthrough achieved", "tags": ["quantum", "computing"]},
        {"content": "Neural network optimization completed", "tags": ["ai", "neural"]},
        {"content": "Database performance improved significantly", "tags": ["database", "performance"]},
        {"content": "Memory allocation optimized for speed", "tags": ["memory", "optimization"]}
    ]

    for mem in memories:
        try:
            response = requests.post(
                f"{base_url}/memory",
                json={
                    "content": mem["content"],
                    "tags": mem["tags"],
                    "embedding": [0.1] * 128  # Dummy embedding
                },
                timeout=2
            )
            if response.status_code == 200:
                print(f"  ✅ Added: '{mem['content'][:30]}...'")
            else:
                print(f"  ❌ Failed to add: {response.status_code}")
        except Exception as e:
            print(f"  ❌ Error adding memory: {e}")

    # Test 2: Search through orchestrator
    print("\n🔍 Searching through orchestrator:")
    search_queries = [
        "quantum computing",
        "neural network",
        "database optimization",
        "memory allocation"
    ]

    for query in search_queries:
        try:
            response = requests.post(
                f"{base_url}/search",
                json={"content": query},
                timeout=2
            )
            if response.status_code == 200:
                results = response.json()
                count = len(results.get("results", []))
                print(f"  ✅ Query '{query}': {count} results")
            else:
                print(f"  ❌ Search failed: {response.status_code}")
        except Exception as e:
            print(f"  ❌ Error searching: {e}")

    # Test 3: Measure orchestrator latency
    print("\n⏱️ Measuring orchestrator latency:")
    latencies = []

    for i in range(20):
        try:
            start = time.perf_counter()
            response = requests.post(
                f"{base_url}/search",
                json={"content": f"test query {i}"},
                timeout=1
            )
            latency = (time.perf_counter() - start) * 1000
            if response.status_code == 200:
                latencies.append(latency)
        except:
            pass

    if latencies:
        avg_latency = statistics.mean(latencies)
        p95_latency = sorted(latencies)[int(len(latencies) * 0.95)]
        print(f"  Average: {avg_latency:.2f}ms")
        print(f"  P95: {p95_latency:.2f}ms")
        print(f"  Min: {min(latencies):.2f}ms")
        print(f"  Max: {max(latencies):.2f}ms")

    # Test 4: Concurrent load through orchestrator
    print("\n🚀 Concurrent load test (5 seconds):")

    def make_request():
        try:
            start = time.perf_counter()
            response = requests.post(
                f"{base_url}/search",
                json={"content": f"concurrent test {time.time_ns()}"},
                timeout=0.5
            )
            latency = (time.perf_counter() - start) * 1000
            return response.status_code == 200, latency
        except:
            return False, 0

    start_time = time.time()
    success_count = 0
    error_count = 0
    all_latencies = []

    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as executor:
        futures = []
        while time.time() - start_time < 5:
            futures.append(executor.submit(make_request))
            time.sleep(0.01)  # 10ms between requests

        for future in concurrent.futures.as_completed(futures):
            try:
                success, latency = future.result(timeout=1)
                if success:
                    success_count += 1
                    all_latencies.append(latency)
                else:
                    error_count += 1
            except:
                error_count += 1

    total = success_count + error_count
    if total > 0:
        throughput = total / 5.0
        success_rate = (success_count / total) * 100
        print(f"  Total Requests: {total}")
        print(f"  Successful: {success_count} ({success_rate:.1f}%)")
        print(f"  Failed: {error_count}")
        print(f"  Throughput: {throughput:.1f} req/s")

        if all_latencies:
            avg_lat = statistics.mean(all_latencies)
            p95_lat = sorted(all_latencies)[int(len(all_latencies) * 0.95)]
            print(f"  Avg Latency: {avg_lat:.2f}ms")
            print(f"  P95 Latency: {p95_lat:.2f}ms")

def main():
    print("="*60)
    print("MFN ORCHESTRATOR VALIDATION TEST")
    print("="*60)

    # Check if orchestrator is running
    if not test_orchestrator_health():
        print("\nAttempting to start orchestrator...")
        proc = start_orchestrator()
        if not proc:
            print("\n❌ Could not start orchestrator. Please check compilation.")
            return
        time.sleep(2)

    # Verify orchestrator is healthy
    if not test_orchestrator_health():
        print("\n❌ Orchestrator still not responding")
        return

    # Run tests
    test_orchestrator_operations()

    print("\n" + "="*60)
    print("ORCHESTRATOR VALIDATION COMPLETE")
    print("="*60)

if __name__ == "__main__":
    main()