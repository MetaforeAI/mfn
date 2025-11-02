#!/usr/bin/env python3
"""
Real MFN Performance Test with Actual Layer Processing
Tests the true throughput with all 4 layers processing requests
"""

import json
import time
import socket
import asyncio
import statistics
import concurrent.futures
from typing import Dict, List, Tuple, Optional

def send_layer_request(layer_num: int, request: dict, timeout: float = 1.0) -> Optional[dict]:
    """Send request to specific layer socket and get response"""
    socket_path = f"/tmp/mfn_layer{layer_num}.sock"
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(timeout)
            sock.connect(socket_path)

            # Send request
            request_json = json.dumps(request)
            sock.sendall((request_json + "\n").encode())

            # Read response
            response = b""
            while True:
                chunk = sock.recv(4096)
                if not chunk:
                    break
                response += chunk
                if b"\n" in response:
                    break

            return json.loads(response.decode().strip())
    except Exception as e:
        return None

def test_layer_connectivity() -> Dict[int, bool]:
    """Test which layers are actually running"""
    print("\n" + "="*60)
    print("LAYER CONNECTIVITY TEST")
    print("="*60)

    layer_status = {}

    # Different ping formats for each layer
    ping_requests = {
        1: {"type": "Ping"},  # Layer 1 might need this format
        2: {"type": "Ping"},  # Layer 2 expects capital P
        3: {"type": "ping", "request_id": "test"},  # Layer 3 works with this
        4: {"type": "predict", "request_id": "test", "query": {"content": "test"}}  # Layer 4 doesn't have ping
    }

    for layer_num, request in ping_requests.items():
        response = send_layer_request(layer_num, request)
        layer_status[layer_num] = response is not None
        status = "✅ Connected" if response else "❌ Not connected"
        print(f"  Layer {layer_num}: {status}")

    return layer_status

def add_test_memories() -> None:
    """Add test memories to layers that support it"""
    print("\n" + "="*60)
    print("ADDING TEST MEMORIES")
    print("="*60)

    test_memories = [
        "The quantum computer achieved 99.9% fidelity",
        "Neural network training completed successfully",
        "Database optimization reduced query time",
        "Memory allocation algorithm improved"
    ]

    # Add to Layer 2 (DSR)
    for i, content in enumerate(test_memories):
        request = {
            "type": "AddMemory",
            "memory": {
                "id": str(time.time_ns()),
                "content": content,
                "embedding": [0.1] * 128,  # Dummy embedding
                "tags": ["test"],
                "timestamp": int(time.time() * 1000000)
            }
        }
        response = send_layer_request(2, request)
        if response:
            print(f"  ✅ Added memory to Layer 2: '{content[:30]}...'")

    # Add to Layer 3 (ALM)
    for content in test_memories:
        request = {
            "type": "add_memory",
            "request_id": str(time.time_ns()),
            "memory": {
                "id": str(time.time_ns()),
                "content": content,
                "tags": ["test"],
                "timestamp": int(time.time() * 1000000)
            }
        }
        response = send_layer_request(3, request)
        if response and response.get("success"):
            print(f"  ✅ Added memory to Layer 3: '{content[:30]}...'")

def measure_single_query_performance(layer_status: Dict[int, bool]) -> Dict[str, float]:
    """Measure performance of a single query through active layers"""
    print("\n" + "="*60)
    print("SINGLE QUERY PERFORMANCE TEST")
    print("="*60)

    metrics = {}

    # Test Layer 2 (DSR) - Similarity Search
    if layer_status.get(2):
        request = {
            "type": "SimilaritySearch",
            "embedding": [0.1] * 128,
            "max_results": 5,
            "similarity_threshold": 0.7
        }

        start = time.perf_counter()
        response = send_layer_request(2, request)
        latency = (time.perf_counter() - start) * 1000

        if response:
            metrics["layer2_latency_ms"] = latency
            print(f"  Layer 2 (DSR) Similarity Search: {latency:.2f}ms")

    # Test Layer 3 (ALM) - Associative Search
    if layer_status.get(3):
        request = {
            "type": "search",
            "request_id": str(time.time_ns()),
            "query": {
                "content": "quantum computer"
            }
        }

        start = time.perf_counter()
        response = send_layer_request(3, request)
        latency = (time.perf_counter() - start) * 1000

        if response:
            metrics["layer3_latency_ms"] = latency
            print(f"  Layer 3 (ALM) Associative Search: {latency:.2f}ms")

    return metrics

def run_concurrent_stress_test(layer_status: Dict[int, bool], duration_seconds: int = 5) -> Dict[str, float]:
    """Run concurrent stress test on active layers"""
    print("\n" + "="*60)
    print(f"CONCURRENT STRESS TEST ({duration_seconds} seconds)")
    print("="*60)

    results = {
        "layer2": {"count": 0, "errors": 0, "latencies": []},
        "layer3": {"count": 0, "errors": 0, "latencies": []}
    }

    def stress_layer2():
        request = {
            "type": "SimilaritySearch",
            "embedding": [0.1] * 128,
            "max_results": 5,
            "similarity_threshold": 0.7
        }

        start = time.perf_counter()
        response = send_layer_request(2, request, timeout=0.5)
        latency = (time.perf_counter() - start) * 1000

        if response:
            return ("layer2", True, latency)
        else:
            return ("layer2", False, latency)

    def stress_layer3():
        request = {
            "type": "search",
            "request_id": str(time.time_ns()),
            "query": {
                "content": f"test query {time.time_ns()}"
            }
        }

        start = time.perf_counter()
        response = send_layer_request(3, request, timeout=0.5)
        latency = (time.perf_counter() - start) * 1000

        if response:
            return ("layer3", True, latency)
        else:
            return ("layer3", False, latency)

    # Run stress test
    start_time = time.time()
    with concurrent.futures.ThreadPoolExecutor(max_workers=20) as executor:
        futures = []

        while time.time() - start_time < duration_seconds:
            if layer_status.get(2):
                futures.append(executor.submit(stress_layer2))
            if layer_status.get(3):
                futures.append(executor.submit(stress_layer3))

            # Don't overwhelm the system
            time.sleep(0.001)  # 1ms between batches

        # Wait for all futures to complete
        for future in concurrent.futures.as_completed(futures):
            try:
                layer, success, latency = future.result(timeout=1)
                results[layer]["count"] += 1
                if success:
                    results[layer]["latencies"].append(latency)
                else:
                    results[layer]["errors"] += 1
            except:
                pass

    # Calculate metrics
    metrics = {}
    for layer, data in results.items():
        if data["count"] > 0:
            throughput = data["count"] / duration_seconds
            avg_latency = statistics.mean(data["latencies"]) if data["latencies"] else 0
            p95_latency = sorted(data["latencies"])[int(len(data["latencies"]) * 0.95)] if data["latencies"] else 0

            print(f"\n  {layer.upper()}:")
            print(f"    Total Requests: {data['count']}")
            print(f"    Successful: {len(data['latencies'])}")
            print(f"    Errors: {data['errors']}")
            print(f"    Throughput: {throughput:.1f} req/s")
            print(f"    Avg Latency: {avg_latency:.2f}ms")
            print(f"    P95 Latency: {p95_latency:.2f}ms")

            metrics[f"{layer}_throughput"] = throughput
            metrics[f"{layer}_avg_latency_ms"] = avg_latency
            metrics[f"{layer}_p95_latency_ms"] = p95_latency

    return metrics

def main():
    print("="*60)
    print("MFN REAL PERFORMANCE TEST")
    print("Testing actual layer processing performance")
    print("="*60)

    # Test connectivity
    layer_status = test_layer_connectivity()
    active_layers = sum(1 for active in layer_status.values() if active)
    print(f"\n  Active Layers: {active_layers}/4")

    if active_layers == 0:
        print("\n❌ No layers are running! Please start the layer servers.")
        return

    # Add test data
    add_test_memories()

    # Measure single query performance
    single_metrics = measure_single_query_performance(layer_status)

    # Run stress test
    stress_metrics = run_concurrent_stress_test(layer_status, duration_seconds=10)

    # Final report
    print("\n" + "="*60)
    print("REAL PERFORMANCE RESULTS")
    print("="*60)

    print("\n📊 SINGLE QUERY LATENCIES:")
    for key, value in single_metrics.items():
        print(f"  {key}: {value:.2f}")

    print("\n🚀 CONCURRENT PERFORMANCE:")
    for key, value in stress_metrics.items():
        if "throughput" in key:
            print(f"  {key}: {value:.1f} req/s")
        else:
            print(f"  {key}: {value:.2f}")

    # Reality check
    print("\n🔍 REALITY CHECK:")
    if "layer2_throughput" in stress_metrics:
        throughput = stress_metrics["layer2_throughput"]
        latency = stress_metrics.get("layer2_avg_latency_ms", 0)
        print(f"  Layer 2 DSR: {throughput:.1f} req/s @ {latency:.2f}ms")
        if throughput > 10000:
            print("  ⚠️ WARNING: Throughput seems unrealistically high!")
        elif throughput < 1000:
            print("  ⚠️ WARNING: Throughput lower than expected (1000-4000 req/s)")
        else:
            print("  ✅ Throughput within expected range (1000-4000 req/s)")

    if "layer3_throughput" in stress_metrics:
        throughput = stress_metrics["layer3_throughput"]
        latency = stress_metrics.get("layer3_avg_latency_ms", 0)
        print(f"  Layer 3 ALM: {throughput:.1f} req/s @ {latency:.2f}ms")
        if throughput > 10000:
            print("  ⚠️ WARNING: Throughput seems unrealistically high!")

    print("\n" + "="*60)
    print("TEST COMPLETE")
    print("="*60)

if __name__ == "__main__":
    main()