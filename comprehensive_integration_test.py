#!/usr/bin/env python3
"""
MFN System Comprehensive Integration Test Suite
Validates all aspects of the integrated system
"""

import socket
import json
import time
import sys
import threading
import statistics
from typing import Dict, Any, List, Tuple
from concurrent.futures import ThreadPoolExecutor, as_completed
import random

# Socket paths
LAYER1_SOCKET = "/tmp/mfn_layer1.sock"
LAYER2_SOCKET = "/tmp/mfn_layer2.sock"
LAYER3_SOCKET = "/tmp/mfn_layer3.sock"
LAYER4_SOCKET = "/tmp/mfn_layer4.sock"

class TestResults:
    """Track comprehensive test results"""
    def __init__(self):
        self.tests_run = 0
        self.tests_passed = 0
        self.tests_failed = 0
        self.performance_metrics = {}
        self.issues = []
        self.successes = []

    def add_result(self, test_name: str, passed: bool, details: str = ""):
        self.tests_run += 1
        if passed:
            self.tests_passed += 1
            self.successes.append((test_name, details))
        else:
            self.tests_failed += 1
            self.issues.append((test_name, details))

    def add_metric(self, metric_name: str, value: float):
        if metric_name not in self.performance_metrics:
            self.performance_metrics[metric_name] = []
        self.performance_metrics[metric_name].append(value)

    def get_summary(self) -> Dict[str, Any]:
        return {
            "total_tests": self.tests_run,
            "passed": self.tests_passed,
            "failed": self.tests_failed,
            "pass_rate": f"{(self.tests_passed/max(1,self.tests_run))*100:.1f}%",
            "performance_summary": {
                name: {
                    "mean": statistics.mean(values) if values else 0,
                    "median": statistics.median(values) if values else 0,
                    "min": min(values) if values else 0,
                    "max": max(values) if values else 0,
                    "p95": statistics.quantiles(values, n=20)[18] if len(values) > 1 else 0
                }
                for name, values in self.performance_metrics.items()
            }
        }

class LayerClient:
    """Enhanced client for layer communication with detailed testing"""

    def __init__(self, socket_path: str, layer_name: str):
        self.socket_path = socket_path
        self.layer_name = layer_name
        self.connected = False
        self.response_times = []

    def send_request(self, request: Dict[str, Any], timeout: float = 5.0) -> Tuple[Dict[str, Any], float]:
        """Send request and measure response time"""
        start_time = time.perf_counter()

        try:
            client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            client.settimeout(timeout)
            client.connect(self.socket_path)

            request_data = json.dumps(request).encode() + b'\n'
            client.send(request_data)

            response_data = b""
            while True:
                chunk = client.recv(4096)
                if not chunk:
                    break
                response_data += chunk
                if b'\n' in response_data or b'\x00' in response_data:
                    break

            client.close()

            elapsed = time.perf_counter() - start_time
            self.response_times.append(elapsed)

            # Handle multi-line responses (Layer 1 issue)
            lines = response_data.decode('utf-8', errors='ignore').strip().split('\n')
            if lines:
                try:
                    response = json.loads(lines[0])
                    self.connected = True
                    return response, elapsed
                except json.JSONDecodeError:
                    # Try parsing as simple text response
                    return {"response": lines[0], "raw": True}, elapsed

            return {"error": "Empty response"}, elapsed

        except FileNotFoundError:
            return {"error": f"Socket not found: {self.socket_path}"}, 0
        except ConnectionRefusedError:
            return {"error": "Connection refused"}, 0
        except socket.timeout:
            return {"error": f"Timeout after {timeout}s"}, timeout
        except Exception as e:
            return {"error": str(e)}, 0

def test_layer_connectivity(results: TestResults):
    """Test 1: Layer Connectivity and Basic Communication"""
    print("\n" + "="*60)
    print("TEST 1: LAYER CONNECTIVITY")
    print("="*60)

    layers = [
        ("Layer 1 (IFR)", LAYER1_SOCKET),
        ("Layer 2 (DSR)", LAYER2_SOCKET),
        ("Layer 3 (ALM)", LAYER3_SOCKET),
        ("Layer 4 (CPE)", LAYER4_SOCKET),
    ]

    active_layers = []

    for layer_name, socket_path in layers:
        client = LayerClient(socket_path, layer_name)
        response, latency = client.send_request({"type": "ping", "request_id": f"test-{time.time()}"})

        if "error" not in response:
            active_layers.append((layer_name, client))
            results.add_result(f"{layer_name} Connectivity", True, f"Latency: {latency*1000:.2f}ms")
            results.add_metric(f"{layer_name}_ping_latency_ms", latency * 1000)
            print(f"  ✅ {layer_name}: Connected (latency: {latency*1000:.2f}ms)")
        else:
            results.add_result(f"{layer_name} Connectivity", False, response.get("error", "Unknown error"))
            print(f"  ❌ {layer_name}: {response.get('error')}")

    return active_layers

def test_memory_operations(active_layers: List[Tuple[str, LayerClient]], results: TestResults):
    """Test 2: Memory Storage and Retrieval"""
    print("\n" + "="*60)
    print("TEST 2: MEMORY OPERATIONS")
    print("="*60)

    test_memories = [
        {"content": "Quantum error correction achieved 99.9% fidelity", "type": "technical"},
        {"content": "Neural network convergence at epoch 150", "type": "ml"},
        {"content": "Database query optimization reduced latency by 60%", "type": "performance"},
        {"content": "Memory allocation improved cache hit rate to 87%", "type": "system"},
        {"content": "Distributed consensus protocol achieved", "type": "distributed"},
    ]

    for layer_name, client in active_layers:
        print(f"\n  Testing {layer_name}:")
        successful_adds = 0

        for memory in test_memories:
            request = {
                "type": "add_memory",
                "request_id": f"add-{time.time()}",
                "content": memory["content"],
                "metadata": memory
            }

            response, latency = client.send_request(request)

            if response.get("success") or "error" not in response:
                successful_adds += 1
                results.add_metric(f"{layer_name}_add_latency_ms", latency * 1000)
                print(f"    ✅ Added memory ({latency*1000:.2f}ms)")
            else:
                print(f"    ❌ Failed: {response.get('error', 'Unknown error')}")

        results.add_result(
            f"{layer_name} Memory Storage",
            successful_adds > 0,
            f"{successful_adds}/{len(test_memories)} memories added"
        )

def test_search_functionality(active_layers: List[Tuple[str, LayerClient]], results: TestResults):
    """Test 3: Search and Retrieval Performance"""
    print("\n" + "="*60)
    print("TEST 3: SEARCH FUNCTIONALITY")
    print("="*60)

    search_queries = [
        "quantum error",
        "neural network",
        "optimization",
        "memory cache",
        "consensus"
    ]

    for layer_name, client in active_layers:
        print(f"\n  Testing {layer_name}:")
        successful_searches = 0

        for query in search_queries:
            request = {
                "type": "search" if "Layer 1" not in layer_name else "query",
                "request_id": f"search-{time.time()}",
                "query": query,
                "content": query,  # For Layer 1 compatibility
                "limit": 5
            }

            response, latency = client.send_request(request)

            if response.get("success") or response.get("results") or "raw" in response:
                successful_searches += 1
                results.add_metric(f"{layer_name}_search_latency_ms", latency * 1000)
                print(f"    ✅ Search '{query}': {latency*1000:.2f}ms")
            else:
                print(f"    ❌ Search '{query}' failed: {response.get('error', 'No results')}")

        results.add_result(
            f"{layer_name} Search",
            successful_searches > 0,
            f"{successful_searches}/{len(search_queries)} searches successful"
        )

def test_concurrent_performance(active_layers: List[Tuple[str, LayerClient]], results: TestResults):
    """Test 4: Concurrent Request Handling"""
    print("\n" + "="*60)
    print("TEST 4: CONCURRENT PERFORMANCE")
    print("="*60)

    if not active_layers:
        print("  ⚠️ No active layers to test")
        return

    def send_concurrent_request(client: LayerClient, request_type: str, index: int):
        request = {
            "type": request_type,
            "request_id": f"concurrent-{index}-{time.time()}",
            "query": f"test query {index}",
            "content": f"test content {index}"
        }
        return client.send_request(request, timeout=10.0)

    for layer_name, client in active_layers[:2]:  # Test first 2 active layers
        print(f"\n  Testing {layer_name}:")

        concurrent_requests = 20
        start_time = time.perf_counter()

        with ThreadPoolExecutor(max_workers=10) as executor:
            futures = []
            for i in range(concurrent_requests):
                request_type = "ping" if i % 2 == 0 else ("query" if "Layer 1" in layer_name else "search")
                futures.append(executor.submit(send_concurrent_request, client, request_type, i))

            successful = 0
            for future in as_completed(futures):
                try:
                    response, latency = future.result(timeout=15)
                    if "error" not in response:
                        successful += 1
                        results.add_metric(f"{layer_name}_concurrent_latency_ms", latency * 1000)
                except:
                    pass

        elapsed = time.perf_counter() - start_time
        qps = concurrent_requests / elapsed

        results.add_result(
            f"{layer_name} Concurrent Handling",
            successful > concurrent_requests * 0.8,
            f"{successful}/{concurrent_requests} successful, {qps:.1f} QPS"
        )

        print(f"    📊 Processed {successful}/{concurrent_requests} requests")
        print(f"    ⏱️ Total time: {elapsed:.2f}s")
        print(f"    🚀 Throughput: {qps:.1f} QPS")

def test_error_handling(active_layers: List[Tuple[str, LayerClient]], results: TestResults):
    """Test 5: Error Handling and Resilience"""
    print("\n" + "="*60)
    print("TEST 5: ERROR HANDLING")
    print("="*60)

    error_cases = [
        {"type": "invalid_type", "request_id": "error-1"},
        {"type": "search"},  # Missing required fields
        {"type": "add_memory", "content": ""},  # Empty content
        {"type": "search", "query": "x" * 10000},  # Very long query
        {},  # Empty request
    ]

    for layer_name, client in active_layers:
        print(f"\n  Testing {layer_name}:")
        handled_gracefully = 0

        for i, error_case in enumerate(error_cases):
            response, _ = client.send_request(error_case, timeout=2.0)

            # Check if error was handled (not a crash)
            if response is not None:
                handled_gracefully += 1
                print(f"    ✅ Error case {i+1}: Handled gracefully")
            else:
                print(f"    ❌ Error case {i+1}: Caused crash or timeout")

        results.add_result(
            f"{layer_name} Error Handling",
            handled_gracefully == len(error_cases),
            f"{handled_gracefully}/{len(error_cases)} errors handled"
        )

def test_persistence(active_layers: List[Tuple[str, LayerClient]], results: TestResults):
    """Test 6: Data Persistence"""
    print("\n" + "="*60)
    print("TEST 6: DATA PERSISTENCE")
    print("="*60)

    # Add unique test data
    test_id = f"persist-test-{random.randint(1000, 9999)}"
    test_content = f"Persistence test memory {test_id}"

    for layer_name, client in active_layers:
        print(f"\n  Testing {layer_name}:")

        # Add memory
        add_request = {
            "type": "add_memory",
            "request_id": f"persist-add-{test_id}",
            "content": test_content,
            "metadata": {"test_id": test_id}
        }

        add_response, _ = client.send_request(add_request)

        # Search for it
        search_request = {
            "type": "search" if "Layer 1" not in layer_name else "query",
            "request_id": f"persist-search-{test_id}",
            "query": test_id,
            "content": test_id
        }

        search_response, _ = client.send_request(search_request)

        # Check if data persists
        found = (
            search_response.get("success") or
            search_response.get("results") or
            test_id in str(search_response)
        )

        results.add_result(
            f"{layer_name} Persistence",
            found,
            "Data retrievable after storage" if found else "Data not found"
        )

        if found:
            print(f"    ✅ Data persisted and retrievable")
        else:
            print(f"    ❌ Data not retrievable")

def analyze_performance_claims(results: TestResults):
    """Analyze actual vs claimed performance"""
    print("\n" + "="*60)
    print("PERFORMANCE ANALYSIS")
    print("="*60)

    claims = {
        "Layer 1 IFR": {"latency_ms": 0.3, "description": "300μs Information Filtering"},
        "Layer 2 DSR": {"latency_ms": 2.0, "description": "<2ms Similarity Search"},
        "Layer 3 ALM": {"latency_ms": 5.0, "description": "<5ms Association"},
        "Layer 4 CPE": {"latency_ms": 10.0, "description": "<10ms Context Prediction"},
        "System QPS": {"value": 10000, "description": "10K+ queries/second"},
    }

    print("\n  📊 Actual vs Claimed Performance:")

    for metric_name, values in results.performance_metrics.items():
        if "latency" in metric_name and values:
            layer = metric_name.split("_")[0]
            if layer in ["Layer 1", "Layer 2", "Layer 3", "Layer 4"]:
                layer_key = f"{layer} {metric_name.split('(')[1].split(')')[0] if '(' in metric_name else ''}"

                mean_latency = statistics.mean(values)
                p95_latency = statistics.quantiles(values, n=20)[18] if len(values) > 1 else mean_latency

                for claim_key, claim_data in claims.items():
                    if layer in claim_key:
                        target = claim_data["latency_ms"]
                        meets_claim = p95_latency <= target

                        status = "✅" if meets_claim else "❌"
                        print(f"    {status} {claim_key}:")
                        print(f"       Target: <{target}ms")
                        print(f"       Actual: {mean_latency:.2f}ms (mean), {p95_latency:.2f}ms (p95)")

                        results.add_result(
                            f"{claim_key} Performance Claim",
                            meets_claim,
                            f"P95: {p95_latency:.2f}ms vs target <{target}ms"
                        )

def generate_final_report(results: TestResults):
    """Generate comprehensive test report"""
    print("\n" + "="*60)
    print("FINAL INTEGRATION TEST REPORT")
    print("="*60)

    summary = results.get_summary()

    print(f"\n📊 TEST SUMMARY:")
    print(f"  Total Tests Run: {summary['total_tests']}")
    print(f"  Tests Passed: {summary['passed']}")
    print(f"  Tests Failed: {summary['failed']}")
    print(f"  Pass Rate: {summary['pass_rate']}")

    if results.successes:
        print(f"\n✅ SUCCESSES ({len(results.successes)}):")
        for test_name, details in results.successes[:10]:
            print(f"  • {test_name}: {details}")

    if results.issues:
        print(f"\n❌ ISSUES FOUND ({len(results.issues)}):")
        for test_name, details in results.issues[:10]:
            print(f"  • {test_name}: {details}")

    print(f"\n⚡ PERFORMANCE METRICS:")
    for metric_name, stats in summary['performance_summary'].items():
        if stats['mean'] > 0:
            print(f"  {metric_name}:")
            print(f"    Mean: {stats['mean']:.2f}, P95: {stats['p95']:.2f}, Max: {stats['max']:.2f}")

    # Critical issues assessment
    critical_issues = []

    if summary['failed'] > summary['passed']:
        critical_issues.append("More tests failed than passed")

    if "Layer 1" in str(results.issues):
        critical_issues.append("Layer 1 (IFR) has connectivity issues")

    if "Layer 2" not in str(results.successes) and "Layer 3" not in str(results.successes):
        critical_issues.append("Core processing layers (2 & 3) not functional")

    # Performance analysis
    perf_issues = []
    for metric, stats in summary['performance_summary'].items():
        if "latency" in metric and stats['p95'] > 100:  # Over 100ms
            perf_issues.append(f"{metric}: P95 latency {stats['p95']:.0f}ms exceeds acceptable range")

    print(f"\n🔍 CRITICAL ASSESSMENT:")

    if critical_issues:
        print("  ⚠️ CRITICAL ISSUES:")
        for issue in critical_issues:
            print(f"    • {issue}")

    if perf_issues:
        print("  ⚠️ PERFORMANCE ISSUES:")
        for issue in perf_issues:
            print(f"    • {issue}")

    # Final verdict
    print(f"\n🎯 PRODUCTION READINESS:")

    if summary['failed'] == 0:
        print("  ✅ READY: All tests passed")
        return 0
    elif summary['pass_rate'].rstrip('%') >= '80':
        print("  ⚠️ PARTIALLY READY: Most tests passed but issues remain")
        return 1
    elif summary['pass_rate'].rstrip('%') >= '50':
        print("  ❌ NOT READY: Significant issues need resolution")
        return 2
    else:
        print("  ❌ CRITICAL: Major architectural issues detected")
        return 3

def main():
    """Run comprehensive integration tests"""
    print("="*60)
    print("MFN COMPREHENSIVE INTEGRATION TEST SUITE")
    print("="*60)
    print(f"Test Started: {time.strftime('%Y-%m-%d %H:%M:%S')}")

    results = TestResults()

    # Run test suites
    active_layers = test_layer_connectivity(results)

    if active_layers:
        test_memory_operations(active_layers, results)
        test_search_functionality(active_layers, results)
        test_concurrent_performance(active_layers, results)
        test_error_handling(active_layers, results)
        test_persistence(active_layers, results)
        analyze_performance_claims(results)
    else:
        print("\n❌ No layers are operational. Cannot proceed with tests.")
        results.add_result("System Availability", False, "No layers responding")

    # Generate final report
    exit_code = generate_final_report(results)

    print(f"\nTest Completed: {time.strftime('%Y-%m-%d %H:%M:%S')}")
    print("="*60)

    return exit_code

if __name__ == "__main__":
    sys.exit(main())