#!/usr/bin/env python3
"""
Layer 3 Go ALM Optimization Benchmark Script
Tests performance improvements implemented for associative search operations
"""

import json
import requests
import time
import statistics
import subprocess
import threading
import signal
import os
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import List, Dict, Tuple

class Layer3OptimizationBenchmark:
    def __init__(self):
        self.base_url = "http://localhost:8082"
        self.process = None
        self.results = {
            "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
            "optimization_tests": {},
            "performance_comparison": {},
            "target_validation": {}
        }
        
    def start_layer3_server(self) -> bool:
        """Start the optimized Layer 3 server"""
        try:
            print("🚀 Starting optimized Layer 3 ALM server...")
            self.process = subprocess.Popen(
                ["./layer3_alm_optimized"],
                cwd="/home/persist/repos/mfn-system/layer3-go-alm",
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE
            )
            
            # Wait for server to start
            for i in range(30):  # 30 second timeout
                try:
                    response = requests.get(f"{self.base_url}/health", timeout=1)
                    if response.status_code == 200:
                        print("✅ Layer 3 ALM server started successfully")
                        return True
                except requests.exceptions.RequestException:
                    time.sleep(1)
                    
            print("❌ Failed to start Layer 3 ALM server")
            return False
            
        except Exception as e:
            print(f"❌ Error starting server: {e}")
            return False
    
    def stop_layer3_server(self):
        """Stop the Layer 3 server"""
        if self.process:
            print("🛑 Stopping Layer 3 ALM server...")
            self.process.terminate()
            self.process.wait()
            print("✅ Server stopped")
    
    def populate_test_data(self, memory_count: int = 100) -> bool:
        """Populate server with test data for benchmarking"""
        try:
            print(f"📊 Populating {memory_count} test memories...")
            
            # Add memories
            for i in range(1, memory_count + 1):
                memory = {
                    "id": i,
                    "content": f"Test memory {i}: Performance optimization benchmark data for associative search testing",
                    "tags": [f"tag_{i%10}", f"category_{i%5}", "benchmark", "test"]
                }
                
                response = requests.post(f"{self.base_url}/memories", json=memory, timeout=5)
                if response.status_code not in [200, 201]:
                    print(f"❌ Failed to add memory {i}: {response.status_code}")
                    return False
            
            # Add associations
            association_count = 0
            for i in range(1, min(memory_count, 50)):  # Limit associations for performance
                for j in range(i+1, min(i+4, memory_count+1)):  # 3 associations per memory
                    association = {
                        "from_memory_id": i,
                        "to_memory_id": j,
                        "type": "semantic",
                        "weight": 0.7 + (0.3 * ((i + j) % 10) / 10),  # Vary weights
                        "reason": f"Test association between memory {i} and {j}"
                    }
                    
                    response = requests.post(f"{self.base_url}/associations", json=association, timeout=5)
                    if response.status_code in [200, 201]:
                        association_count += 1
            
            print(f"✅ Added {memory_count} memories and {association_count} associations")
            return True
            
        except Exception as e:
            print(f"❌ Error populating test data: {e}")
            return False
    
    def benchmark_search_latency(self, iterations: int = 100) -> Dict:
        """Benchmark search operation latency"""
        print(f"⚡ Testing search latency ({iterations} iterations)...")
        
        latencies = []
        errors = 0
        
        for i in range(iterations):
            start_memory_id = (i % 50) + 1  # Cycle through memories
            query = {
                "start_memory_ids": [start_memory_id],
                "max_depth": 2,
                "max_results": 10,
                "min_weight": 0.1,
                "search_mode": "breadth_first"
            }
            
            try:
                start_time = time.perf_counter()
                response = requests.post(f"{self.base_url}/search/associative", json=query, timeout=1)
                end_time = time.perf_counter()
                
                if response.status_code == 200:
                    latency_ms = (end_time - start_time) * 1000
                    latencies.append(latency_ms)
                else:
                    errors += 1
                    
            except Exception as e:
                errors += 1
        
        if not latencies:
            return {"error": "No successful searches completed"}
        
        return {
            "iterations": iterations,
            "successful": len(latencies),
            "errors": errors,
            "avg_latency_ms": statistics.mean(latencies),
            "median_latency_ms": statistics.median(latencies),
            "p95_latency_ms": self.percentile(latencies, 95),
            "p99_latency_ms": self.percentile(latencies, 99),
            "min_latency_ms": min(latencies),
            "max_latency_ms": max(latencies),
            "target_met": statistics.mean(latencies) < 20.0  # Target: <20ms
        }
    
    def benchmark_throughput(self, duration_seconds: int = 30, concurrent_clients: int = 10) -> Dict:
        """Benchmark search throughput with concurrent requests"""
        print(f"🔥 Testing throughput ({duration_seconds}s, {concurrent_clients} clients)...")
        
        results = []
        start_time = time.time()
        end_time = start_time + duration_seconds
        
        def make_requests():
            client_results = []
            request_id = 0
            
            while time.time() < end_time:
                start_memory_id = (request_id % 50) + 1
                query = {
                    "start_memory_ids": [start_memory_id],
                    "max_depth": 2,
                    "max_results": 5,
                    "min_weight": 0.2,
                    "search_mode": "best_first"
                }
                
                try:
                    req_start = time.perf_counter()
                    response = requests.post(f"{self.base_url}/search/associative", json=query, timeout=2)
                    req_end = time.perf_counter()
                    
                    client_results.append({
                        "success": response.status_code == 200,
                        "latency_ms": (req_end - req_start) * 1000,
                        "timestamp": req_end
                    })
                    
                    request_id += 1
                    
                except Exception as e:
                    client_results.append({
                        "success": False,
                        "error": str(e),
                        "timestamp": time.time()
                    })
                    
                time.sleep(0.01)  # Small delay to prevent overwhelming
            
            return client_results
        
        # Run concurrent clients
        with ThreadPoolExecutor(max_workers=concurrent_clients) as executor:
            futures = [executor.submit(make_requests) for _ in range(concurrent_clients)]
            
            for future in as_completed(futures):
                try:
                    results.extend(future.result())
                except Exception as e:
                    print(f"Client error: {e}")
        
        # Calculate metrics
        successful_requests = [r for r in results if r.get("success", False)]
        failed_requests = [r for r in results if not r.get("success", False)]
        
        if not successful_requests:
            return {"error": "No successful requests completed"}
        
        actual_duration = max(r["timestamp"] for r in results) - min(r["timestamp"] for r in results)
        throughput_rps = len(successful_requests) / actual_duration if actual_duration > 0 else 0
        
        latencies = [r["latency_ms"] for r in successful_requests]
        
        return {
            "duration_seconds": actual_duration,
            "total_requests": len(results),
            "successful_requests": len(successful_requests),
            "failed_requests": len(failed_requests),
            "throughput_rps": throughput_rps,
            "avg_latency_ms": statistics.mean(latencies),
            "p95_latency_ms": self.percentile(latencies, 95),
            "error_rate": len(failed_requests) / len(results) * 100,
            "target_met": throughput_rps > 100  # Target: >100 RPS initially
        }
    
    def benchmark_batch_operations(self) -> Dict:
        """Test batch search operations"""
        print("📦 Testing batch operations...")
        
        # Create batch query
        batch_queries = []
        for i in range(5):  # 5 queries in batch
            batch_queries.append({
                "start_memory_ids": [(i * 10 + 1)],
                "max_depth": 2,
                "max_results": 5,
                "min_weight": 0.1,
                "search_mode": "breadth_first"
            })
        
        try:
            start_time = time.perf_counter()
            response = requests.post(f"{self.base_url}/search/batch", json=batch_queries, timeout=5)
            end_time = time.perf_counter()
            
            if response.status_code == 200:
                data = response.json()
                return {
                    "batch_size": len(batch_queries),
                    "total_latency_ms": (end_time - start_time) * 1000,
                    "avg_latency_per_query_ms": (end_time - start_time) * 1000 / len(batch_queries),
                    "results_returned": data.get("count", 0),
                    "success": True
                }
            else:
                return {"success": False, "status_code": response.status_code}
                
        except Exception as e:
            return {"success": False, "error": str(e)}
    
    def benchmark_cache_performance(self) -> Dict:
        """Test cache hit rates and performance"""
        print("🎯 Testing cache performance...")
        
        # First, make requests to populate cache
        populate_queries = []
        for i in range(1, 21):  # 20 unique queries
            query = {
                "start_memory_ids": [i],
                "max_depth": 2,
                "max_results": 10,
                "min_weight": 0.1,
                "search_mode": "breadth_first"
            }
            
            try:
                requests.post(f"{self.base_url}/search/associative", json=query, timeout=2)
            except:
                pass
        
        # Now repeat the same queries to test cache hits
        cache_test_latencies = []
        for _ in range(3):  # Repeat 3 times
            for i in range(1, 21):
                query = {
                    "start_memory_ids": [i],
                    "max_depth": 2,
                    "max_results": 10,
                    "min_weight": 0.1,
                    "search_mode": "breadth_first"
                }
                
                try:
                    start_time = time.perf_counter()
                    response = requests.post(f"{self.base_url}/search/associative", json=query, timeout=2)
                    end_time = time.perf_counter()
                    
                    if response.status_code == 200:
                        cache_test_latencies.append((end_time - start_time) * 1000)
                except:
                    pass
        
        # Get cache metrics from performance endpoint
        try:
            response = requests.get(f"{self.base_url}/performance", timeout=5)
            if response.status_code == 200:
                perf_data = response.json()
                cache_stats = perf_data.get("alm_metrics", {}).get("cache", {})
            else:
                cache_stats = {}
        except:
            cache_stats = {}
        
        return {
            "cache_test_queries": len(cache_test_latencies),
            "avg_cached_latency_ms": statistics.mean(cache_test_latencies) if cache_test_latencies else 0,
            "cache_stats": cache_stats,
            "performance_improvement": True if cache_test_latencies and statistics.mean(cache_test_latencies) < 10 else False
        }
    
    def get_server_metrics(self) -> Dict:
        """Get comprehensive server performance metrics"""
        try:
            response = requests.get(f"{self.base_url}/performance", timeout=5)
            if response.status_code == 200:
                return response.json()
            else:
                return {"error": f"Failed to get metrics: {response.status_code}"}
        except Exception as e:
            return {"error": f"Error getting metrics: {e}"}
    
    def percentile(self, data: List[float], p: float) -> float:
        """Calculate percentile"""
        if not data:
            return 0.0
        sorted_data = sorted(data)
        k = (len(sorted_data) - 1) * p / 100
        f = int(k)
        c = k - f
        if f == len(sorted_data) - 1:
            return sorted_data[f]
        return sorted_data[f] * (1 - c) + sorted_data[f + 1] * c
    
    def run_comprehensive_benchmark(self) -> Dict:
        """Run all optimization benchmarks"""
        print("🔬 Starting comprehensive Layer 3 optimization benchmark...")
        
        if not self.start_layer3_server():
            return {"error": "Failed to start server"}
        
        try:
            # Populate test data
            if not self.populate_test_data(100):
                return {"error": "Failed to populate test data"}
            
            time.sleep(2)  # Let server stabilize
            
            # Run benchmarks
            print("\n=== OPTIMIZATION BENCHMARKS ===")
            
            # 1. Search Latency Test
            self.results["optimization_tests"]["search_latency"] = self.benchmark_search_latency(100)
            print(f"✅ Search latency: {self.results['optimization_tests']['search_latency']['avg_latency_ms']:.2f}ms avg")
            
            # 2. Throughput Test
            self.results["optimization_tests"]["throughput"] = self.benchmark_throughput(30, 5)
            print(f"✅ Throughput: {self.results['optimization_tests']['throughput']['throughput_rps']:.1f} RPS")
            
            # 3. Batch Operations Test
            self.results["optimization_tests"]["batch_operations"] = self.benchmark_batch_operations()
            print(f"✅ Batch operations: {self.results['optimization_tests']['batch_operations'].get('success', False)}")
            
            # 4. Cache Performance Test
            self.results["optimization_tests"]["cache_performance"] = self.benchmark_cache_performance()
            print(f"✅ Cache performance: {self.results['optimization_tests']['cache_performance']['performance_improvement']}")
            
            # 5. Get final server metrics
            self.results["server_metrics"] = self.get_server_metrics()
            
            # Validate against targets
            self.validate_performance_targets()
            
            return self.results
            
        finally:
            self.stop_layer3_server()
    
    def validate_performance_targets(self):
        """Validate performance against target metrics"""
        targets = {
            "search_latency_target_ms": 20.0,
            "throughput_target_rps": 100.0,
            "error_rate_target_percent": 1.0
        }
        
        validation = {}
        
        # Search latency validation
        search_results = self.results["optimization_tests"].get("search_latency", {})
        if "avg_latency_ms" in search_results:
            validation["search_latency"] = {
                "target": targets["search_latency_target_ms"],
                "actual": search_results["avg_latency_ms"],
                "passed": search_results["avg_latency_ms"] < targets["search_latency_target_ms"],
                "improvement_factor": targets["search_latency_target_ms"] / search_results["avg_latency_ms"]
            }
        
        # Throughput validation
        throughput_results = self.results["optimization_tests"].get("throughput", {})
        if "throughput_rps" in throughput_results:
            validation["throughput"] = {
                "target": targets["throughput_target_rps"],
                "actual": throughput_results["throughput_rps"],
                "passed": throughput_results["throughput_rps"] > targets["throughput_target_rps"],
                "improvement_factor": throughput_results["throughput_rps"] / targets["throughput_target_rps"]
            }
        
        # Error rate validation
        if "error_rate" in throughput_results:
            validation["error_rate"] = {
                "target": targets["error_rate_target_percent"],
                "actual": throughput_results["error_rate"],
                "passed": throughput_results["error_rate"] < targets["error_rate_target_percent"]
            }
        
        self.results["target_validation"] = validation
        
        print("\n=== PERFORMANCE TARGET VALIDATION ===")
        for metric, result in validation.items():
            status = "✅ PASSED" if result["passed"] else "❌ FAILED"
            print(f"{metric}: {status} (Target: {result['target']}, Actual: {result['actual']:.2f})")

def main():
    benchmark = Layer3OptimizationBenchmark()
    
    try:
        results = benchmark.run_comprehensive_benchmark()
        
        # Save results
        output_file = "/home/persist/repos/mfn-system/layer3_optimization_benchmark.json"
        with open(output_file, "w") as f:
            json.dump(results, f, indent=2, default=str)
        
        print(f"\n📊 Results saved to: {output_file}")
        
        # Print summary
        print("\n=== OPTIMIZATION SUMMARY ===")
        if "optimization_tests" in results:
            search_latency = results["optimization_tests"].get("search_latency", {})
            throughput = results["optimization_tests"].get("throughput", {})
            
            print(f"🔍 Search Latency: {search_latency.get('avg_latency_ms', 'N/A')}ms average")
            print(f"⚡ Throughput: {throughput.get('throughput_rps', 'N/A')} requests/second")
            print(f"📈 Error Rate: {throughput.get('error_rate', 'N/A')}%")
            
            # Performance targets assessment
            validation = results.get("target_validation", {})
            targets_met = sum(1 for v in validation.values() if v.get("passed", False))
            total_targets = len(validation)
            
            print(f"🎯 Performance Targets: {targets_met}/{total_targets} met")
            
            if targets_met == total_targets:
                print("🏆 ALL PERFORMANCE TARGETS ACHIEVED!")
            elif targets_met > 0:
                print("✅ Some performance targets met - good progress!")
            else:
                print("⚠️ Performance targets need improvement")
        
    except Exception as e:
        print(f"❌ Benchmark failed: {e}")
        return 1
    
    return 0

if __name__ == "__main__":
    exit(main())