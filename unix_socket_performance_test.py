#!/usr/bin/env python3
"""
MFN Phase 2 Unix Socket Performance Analysis
============================================

Tests performance differences between HTTP API and Unix socket communication
for Layer 3 (Associative Link Mesh) service.

Target: Analyze current bottlenecks and measure improvements from Unix socket usage.
"""

import asyncio
import json
import socket
import time
import statistics
import requests
import sys
from typing import Dict, List, Tuple, Any
from dataclasses import dataclass
from concurrent.futures import ThreadPoolExecutor
import threading
import uuid


@dataclass
class PerformanceMetrics:
    """Performance metrics for a test run"""
    test_name: str
    min_time_ms: float
    max_time_ms: float
    avg_time_ms: float
    median_time_ms: float
    p95_time_ms: float
    p99_time_ms: float
    total_requests: int
    errors: int
    requests_per_second: float
    success_rate: float


class UnixSocketClient:
    """High-performance Unix socket client for Layer 3 ALM"""
    
    def __init__(self, socket_path: str = "/tmp/mfn_layer3.sock"):
        self.socket_path = socket_path
        self.connection = None
        
    def connect(self):
        """Establish Unix socket connection"""
        try:
            self.connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.connection.connect(self.socket_path)
            return True
        except Exception as e:
            print(f"Unix socket connection failed: {e}")
            return False
    
    def disconnect(self):
        """Close Unix socket connection"""
        if self.connection:
            try:
                self.connection.close()
            except:
                pass
            self.connection = None
    
    def send_request(self, request_data: Dict[str, Any]) -> Tuple[Dict[str, Any], float]:
        """Send request and measure response time"""
        if not self.connection:
            raise Exception("Not connected to Unix socket")
        
        request_json = json.dumps(request_data)
        start_time = time.perf_counter()
        
        try:
            # Send request
            self.connection.sendall(request_json.encode('utf-8'))
            
            # Receive response
            response_data = b""
            while True:
                chunk = self.connection.recv(4096)
                if not chunk:
                    break
                response_data += chunk
                # Try to decode as complete JSON
                try:
                    response = json.loads(response_data.decode('utf-8'))
                    break
                except json.JSONDecodeError:
                    continue
            
            elapsed_ms = (time.perf_counter() - start_time) * 1000
            return response, elapsed_ms
            
        except Exception as e:
            elapsed_ms = (time.perf_counter() - start_time) * 1000
            return {"error": str(e)}, elapsed_ms


class HTTPClient:
    """HTTP client for Layer 3 ALM"""
    
    def __init__(self, base_url: str = "http://localhost:8082"):
        self.base_url = base_url
        self.session = requests.Session()
        # Optimize HTTP session
        self.session.headers.update({
            'Connection': 'keep-alive',
            'Content-Type': 'application/json'
        })
    
    def send_request(self, endpoint: str, data: Dict[str, Any]) -> Tuple[Dict[str, Any], float]:
        """Send HTTP request and measure response time"""
        url = f"{self.base_url}{endpoint}"
        start_time = time.perf_counter()
        
        try:
            response = self.session.post(url, json=data, timeout=1.0)
            elapsed_ms = (time.perf_counter() - start_time) * 1000
            
            if response.status_code == 200:
                return response.json(), elapsed_ms
            else:
                return {"error": f"HTTP {response.status_code}: {response.text}"}, elapsed_ms
                
        except Exception as e:
            elapsed_ms = (time.perf_counter() - start_time) * 1000
            return {"error": str(e)}, elapsed_ms


class PerformanceAnalyzer:
    """Comprehensive performance analysis for MFN Layer 3"""
    
    def __init__(self):
        self.http_client = HTTPClient()
        self.results: List[PerformanceMetrics] = []
        
    def create_test_query(self, query_id: int) -> Dict[str, Any]:
        """Create associative search query"""
        return {
            "type": "associative_search",
            "request_id": f"test_{query_id}_{uuid.uuid4().hex[:8]}",
            "payload": {
                "start_memory_ids": [1, 2, 3],
                "max_depth": 3,
                "max_results": 10,
                "min_weight": 0.1,
                "timeout_ms": 50,
                "search_mode": "best_first"
            }
        }
    
    def test_http_performance(self, num_requests: int = 100) -> PerformanceMetrics:
        """Test HTTP API performance"""
        print(f"🔄 Testing HTTP performance ({num_requests} requests)...")
        
        times = []
        errors = 0
        
        for i in range(num_requests):
            query = {
                "start_memory_ids": [1, 2, 3],
                "max_depth": 3,
                "max_results": 10,
                "min_weight": 0.1,
                "timeout": "50ms",
                "search_mode": "best_first"
            }
            
            response, elapsed_ms = self.http_client.send_request("/search/associative", query)
            times.append(elapsed_ms)
            
            if "error" in response:
                errors += 1
                if errors <= 5:  # Show first 5 errors
                    print(f"  HTTP Error: {response['error']}")
        
        return self._calculate_metrics("HTTP API", times, errors)
    
    def test_unix_socket_performance(self, num_requests: int = 100) -> PerformanceMetrics:
        """Test Unix socket performance"""
        print(f"🔄 Testing Unix socket performance ({num_requests} requests)...")
        
        client = UnixSocketClient()
        if not client.connect():
            return PerformanceMetrics(
                "Unix Socket", 0, 0, 0, 0, 0, 0, 0, num_requests, 0, 0.0
            )
        
        try:
            times = []
            errors = 0
            
            for i in range(num_requests):
                query = self.create_test_query(i)
                response, elapsed_ms = client.send_request(query)
                times.append(elapsed_ms)
                
                if "error" in response:
                    errors += 1
                    if errors <= 5:  # Show first 5 errors
                        print(f"  Socket Error: {response['error']}")
            
            return self._calculate_metrics("Unix Socket", times, errors)
            
        finally:
            client.disconnect()
    
    def test_concurrent_performance(self, protocol: str, num_threads: int = 10, 
                                   requests_per_thread: int = 50) -> PerformanceMetrics:
        """Test concurrent performance"""
        print(f"🔄 Testing {protocol} concurrent performance ({num_threads} threads, "
              f"{requests_per_thread} requests each)...")
        
        times = []
        errors = 0
        times_lock = threading.Lock()
        errors_lock = threading.Lock()
        
        def worker_thread():
            nonlocal errors
            if protocol == "HTTP":
                client = HTTPClient()
                for i in range(requests_per_thread):
                    query = {
                        "start_memory_ids": [1, 2, 3],
                        "max_depth": 3,
                        "max_results": 10,
                        "min_weight": 0.1,
                        "timeout": "50ms",
                        "search_mode": "best_first"
                    }
                    response, elapsed_ms = client.send_request("/search/associative", query)
                    
                    with times_lock:
                        times.append(elapsed_ms)
                    
                    if "error" in response:
                        with errors_lock:
                            errors += 1
                            
            elif protocol == "Unix Socket":
                client = UnixSocketClient()
                if client.connect():
                    try:
                        for i in range(requests_per_thread):
                            query = self.create_test_query(i)
                            response, elapsed_ms = client.send_request(query)
                            
                            with times_lock:
                                times.append(elapsed_ms)
                            
                            if "error" in response:
                                with errors_lock:
                                    errors += 1
                    finally:
                        client.disconnect()
                else:
                    with errors_lock:
                        errors += requests_per_thread
        
        # Run concurrent threads
        with ThreadPoolExecutor(max_workers=num_threads) as executor:
            futures = [executor.submit(worker_thread) for _ in range(num_threads)]
            for future in futures:
                future.result()
        
        return self._calculate_metrics(f"{protocol} Concurrent", times, errors)
    
    def _calculate_metrics(self, test_name: str, times: List[float], errors: int) -> PerformanceMetrics:
        """Calculate performance metrics from timing data"""
        if not times:
            return PerformanceMetrics(test_name, 0, 0, 0, 0, 0, 0, 0, errors, 0, 0.0)
        
        sorted_times = sorted(times)
        total_requests = len(times)
        
        # Calculate percentiles
        def percentile(data, p):
            k = (len(data) - 1) * p / 100
            f = int(k)
            c = k - f
            if f == len(data) - 1:
                return data[f]
            return data[f] * (1 - c) + data[f + 1] * c
        
        total_time_seconds = sum(times) / 1000
        rps = total_requests / total_time_seconds if total_time_seconds > 0 else 0
        
        metrics = PerformanceMetrics(
            test_name=test_name,
            min_time_ms=min(times),
            max_time_ms=max(times),
            avg_time_ms=statistics.mean(times),
            median_time_ms=statistics.median(times),
            p95_time_ms=percentile(sorted_times, 95),
            p99_time_ms=percentile(sorted_times, 99),
            total_requests=total_requests,
            errors=errors,
            requests_per_second=rps,
            success_rate=(total_requests - errors) / total_requests * 100
        )
        
        self.results.append(metrics)
        return metrics
    
    def print_metrics(self, metrics: PerformanceMetrics):
        """Print performance metrics in a formatted table"""
        print(f"\n📊 {metrics.test_name} Results:")
        print(f"  Total Requests:     {metrics.total_requests}")
        print(f"  Errors:            {metrics.errors}")
        print(f"  Success Rate:      {metrics.success_rate:.1f}%")
        print(f"  Min Time:          {metrics.min_time_ms:.2f}ms")
        print(f"  Max Time:          {metrics.max_time_ms:.2f}ms")
        print(f"  Average Time:      {metrics.avg_time_ms:.2f}ms")
        print(f"  Median Time:       {metrics.median_time_ms:.2f}ms")
        print(f"  95th Percentile:   {metrics.p95_time_ms:.2f}ms")
        print(f"  99th Percentile:   {metrics.p99_time_ms:.2f}ms")
        print(f"  Requests/Second:   {metrics.requests_per_second:.1f}")
    
    def compare_protocols(self):
        """Generate comparison report"""
        if len(self.results) < 2:
            return
        
        http_metrics = next((r for r in self.results if "HTTP" in r.test_name and "Concurrent" not in r.test_name), None)
        socket_metrics = next((r for r in self.results if "Unix Socket" in r.test_name and "Concurrent" not in r.test_name), None)
        
        if not http_metrics or not socket_metrics:
            return
        
        print(f"\n🆚 Protocol Comparison:")
        print(f"{'Metric':<20} {'HTTP':<12} {'Unix Socket':<12} {'Improvement':<12}")
        print(f"{'-' * 60}")
        
        metrics_to_compare = [
            ("Avg Time (ms)", http_metrics.avg_time_ms, socket_metrics.avg_time_ms, "lower"),
            ("95th % (ms)", http_metrics.p95_time_ms, socket_metrics.p95_time_ms, "lower"),
            ("Requests/sec", http_metrics.requests_per_second, socket_metrics.requests_per_second, "higher"),
            ("Success Rate (%)", http_metrics.success_rate, socket_metrics.success_rate, "higher")
        ]
        
        for metric_name, http_val, socket_val, better in metrics_to_compare:
            if better == "lower":
                improvement = (http_val - socket_val) / http_val * 100 if http_val > 0 else 0
                symbol = "⬇️" if improvement > 0 else "⬆️"
            else:
                improvement = (socket_val - http_val) / http_val * 100 if http_val > 0 else 0
                symbol = "⬆️" if improvement > 0 else "⬇️"
            
            print(f"{metric_name:<20} {http_val:<12.2f} {socket_val:<12.2f} {symbol}{abs(improvement):<10.1f}%")
    
    def generate_infrastructure_report(self):
        """Generate infrastructure analysis report"""
        print(f"\n🏗️  MFN Phase 2 Infrastructure Analysis Report")
        print(f"=" * 60)
        
        # Analyze results
        http_avg = next((r.avg_time_ms for r in self.results if "HTTP" in r.test_name and "Concurrent" not in r.test_name), 0)
        socket_avg = next((r.avg_time_ms for r in self.results if "Unix Socket" in r.test_name and "Concurrent" not in r.test_name), 0)
        
        improvement = ((http_avg - socket_avg) / http_avg * 100) if http_avg > 0 else 0
        
        print(f"\n📈 Performance Findings:")
        print(f"  Current HTTP latency:     {http_avg:.2f}ms")
        print(f"  Unix Socket latency:      {socket_avg:.2f}ms")
        print(f"  Performance improvement:  {improvement:.1f}%")
        
        target_latency = 2.0  # Target <2ms
        print(f"  Target latency:           {target_latency:.1f}ms")
        print(f"  Gap to target:            {max(0, socket_avg - target_latency):.2f}ms")
        
        # Infrastructure recommendations
        print(f"\n🔧 Infrastructure Requirements:")
        
        if socket_avg > target_latency:
            print(f"  ⚠️  Additional optimizations needed:")
            print(f"     - Implement connection pooling")
            print(f"     - Add message framing protocol")
            print(f"     - Enable zero-copy transfers")
            print(f"     - Optimize serialization (MessagePack/Protocol Buffers)")
        else:
            print(f"  ✅ Target performance achieved with Unix sockets")
        
        print(f"\n🌐 Unified Socket Architecture Design:")
        print(f"  Layer 1 (IFR):     /tmp/mfn_layer1.sock")
        print(f"  Layer 2 (DSR):     /tmp/mfn_layer2.sock") 
        print(f"  Layer 3 (ALM):     /tmp/mfn_layer3.sock ✅ (implemented)")
        print(f"  Layer 4 (CPE):     /tmp/mfn_layer4.sock")
        
        print(f"\n⚡ Recommended Protocol Stack:")
        print(f"  Transport:        Unix Domain Sockets")
        print(f"  Framing:          Length-prefixed messages")
        print(f"  Serialization:    MessagePack or Protocol Buffers")
        print(f"  Connection Model: Connection pooling with keep-alive")
        print(f"  Error Handling:   Circuit breaker pattern")


def main():
    """Run comprehensive performance analysis"""
    print("🚀 MFN Phase 2 Unix Socket Performance Analysis")
    print("=" * 55)
    
    analyzer = PerformanceAnalyzer()
    
    # Test different scenarios
    scenarios = [
        ("HTTP API", "test_http_performance", 100),
        ("Unix Socket", "test_unix_socket_performance", 100),
        ("HTTP Concurrent", "test_concurrent_performance", "HTTP", 10, 20),
        ("Unix Socket Concurrent", "test_concurrent_performance", "Unix Socket", 10, 20)
    ]
    
    for scenario in scenarios:
        try:
            if scenario[0].endswith("Concurrent"):
                metrics = analyzer.test_concurrent_performance(scenario[2], scenario[3], scenario[4])
            else:
                method = getattr(analyzer, scenario[1])
                metrics = method(scenario[2])
            
            analyzer.print_metrics(metrics)
            time.sleep(0.5)  # Brief pause between tests
            
        except Exception as e:
            print(f"❌ {scenario[0]} test failed: {e}")
    
    # Generate analysis reports
    analyzer.compare_protocols()
    analyzer.generate_infrastructure_report()
    
    print(f"\n✅ Performance analysis complete!")
    print(f"📄 Results available for MFN Phase 2 architecture design")


if __name__ == "__main__":
    main()