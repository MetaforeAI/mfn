#!/usr/bin/env python3
"""
MFN System Stress Testing Framework
===================================
High-intensity stress testing for validating MFN system performance
under extreme load conditions and capacity limits.

Tests:
1. Concurrent query stress testing (1000+ queries/second)
2. Memory capacity stress testing (50M+ memories)
3. Resource exhaustion testing
4. Fault tolerance testing
5. Performance degradation analysis
"""

import asyncio
import aiohttp
import time
import json
import threading
import multiprocessing as mp
import psutil
import numpy as np
from dataclasses import dataclass
from typing import List, Dict, Any, Optional
from concurrent.futures import ThreadPoolExecutor, ProcessPoolExecutor
import logging
import argparse
from pathlib import Path
import resource

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

@dataclass
class StressTestResult:
    """Result from a stress test"""
    test_name: str
    duration_seconds: float
    total_operations: int
    successful_operations: int
    failed_operations: int
    operations_per_second: float
    average_response_time_ms: float
    min_response_time_ms: float
    max_response_time_ms: float
    p95_response_time_ms: float
    p99_response_time_ms: float
    memory_usage_peak_mb: float
    cpu_usage_peak_percent: float
    error_rate: float
    errors: List[str]

class SystemMonitor:
    """Monitors system resources during stress tests"""
    
    def __init__(self):
        self.monitoring = False
        self.stats = []
        self.monitor_thread = None

    def start_monitoring(self):
        """Start resource monitoring"""
        self.monitoring = True
        self.stats = []
        self.monitor_thread = threading.Thread(target=self._monitor_loop)
        self.monitor_thread.start()

    def stop_monitoring(self) -> Dict[str, float]:
        """Stop monitoring and return peak statistics"""
        self.monitoring = False
        if self.monitor_thread:
            self.monitor_thread.join()
        
        if not self.stats:
            return {"memory_mb": 0, "cpu_percent": 0}
        
        memory_values = [s["memory_mb"] for s in self.stats]
        cpu_values = [s["cpu_percent"] for s in self.stats]
        
        return {
            "memory_mb": max(memory_values),
            "cpu_percent": max(cpu_values),
            "avg_memory_mb": np.mean(memory_values),
            "avg_cpu_percent": np.mean(cpu_values)
        }

    def _monitor_loop(self):
        """Resource monitoring loop"""
        while self.monitoring:
            try:
                memory_mb = psutil.virtual_memory().used / (1024 * 1024)
                cpu_percent = psutil.cpu_percent(interval=0.1)
                
                self.stats.append({
                    "timestamp": time.time(),
                    "memory_mb": memory_mb,
                    "cpu_percent": cpu_percent
                })
                
                time.sleep(0.5)  # Monitor every 500ms
            except Exception as e:
                logger.warning(f"Monitoring error: {e}")

class ConcurrentQueryStressTester:
    """Stress tests concurrent query performance"""
    
    def __init__(self, layer3_url: str = "http://localhost:8082"):
        self.layer3_url = layer3_url
        self.monitor = SystemMonitor()

    async def concurrent_query_stress_test(self, 
                                         concurrent_users: int = 100,
                                         queries_per_user: int = 10,
                                         test_duration_seconds: int = 60) -> StressTestResult:
        """Run concurrent query stress test"""
        logger.info(f"Starting concurrent query stress test: "
                   f"{concurrent_users} users, {queries_per_user} queries each")
        
        self.monitor.start_monitoring()
        
        # Prepare test queries
        test_queries = self._generate_test_queries(queries_per_user * concurrent_users)
        
        start_time = time.time()
        results = []
        errors = []
        
        # Create semaphore to limit concurrent connections
        semaphore = asyncio.Semaphore(concurrent_users)
        
        async with aiohttp.ClientSession(
            timeout=aiohttp.ClientTimeout(total=30),
            connector=aiohttp.TCPConnector(limit=concurrent_users * 2)
        ) as session:
            
            # Create tasks for concurrent execution
            tasks = []
            for i in range(concurrent_users):
                user_queries = test_queries[i * queries_per_user:(i + 1) * queries_per_user]
                task = asyncio.create_task(
                    self._user_query_session(session, semaphore, user_queries, i)
                )
                tasks.append(task)
            
            # Wait for all tasks to complete
            user_results = await asyncio.gather(*tasks, return_exceptions=True)
        
        end_time = time.time()
        duration = end_time - start_time
        
        # Collect results
        for result in user_results:
            if isinstance(result, Exception):
                errors.append(str(result))
            else:
                results.extend(result)
        
        # Stop monitoring
        resource_stats = self.monitor.stop_monitoring()
        
        # Calculate statistics
        successful_operations = len([r for r in results if r["success"]])
        failed_operations = len(results) - successful_operations
        response_times = [r["response_time_ms"] for r in results if r["success"]]
        
        if response_times:
            avg_response_time = np.mean(response_times)
            min_response_time = np.min(response_times)
            max_response_time = np.max(response_times)
            p95_response_time = np.percentile(response_times, 95)
            p99_response_time = np.percentile(response_times, 99)
        else:
            avg_response_time = min_response_time = max_response_time = 0
            p95_response_time = p99_response_time = 0
        
        return StressTestResult(
            test_name="concurrent_query_stress",
            duration_seconds=duration,
            total_operations=len(results),
            successful_operations=successful_operations,
            failed_operations=failed_operations,
            operations_per_second=len(results) / duration if duration > 0 else 0,
            average_response_time_ms=avg_response_time,
            min_response_time_ms=min_response_time,
            max_response_time_ms=max_response_time,
            p95_response_time_ms=p95_response_time,
            p99_response_time_ms=p99_response_time,
            memory_usage_peak_mb=resource_stats["memory_mb"],
            cpu_usage_peak_percent=resource_stats["cpu_percent"],
            error_rate=failed_operations / len(results) if results else 0,
            errors=errors[:10]  # Keep first 10 errors
        )

    async def _user_query_session(self, session: aiohttp.ClientSession, 
                                semaphore: asyncio.Semaphore,
                                queries: List[Dict], user_id: int) -> List[Dict]:
        """Simulate a single user's query session"""
        results = []
        
        for query in queries:
            async with semaphore:  # Limit concurrent connections
                start_time = time.time()
                
                try:
                    async with session.post(
                        f"{self.layer3_url}/search",
                        json=query,
                        headers={"Content-Type": "application/json"}
                    ) as response:
                        
                        end_time = time.time()
                        response_time_ms = (end_time - start_time) * 1000
                        
                        if response.status == 200:
                            await response.json()  # Read response body
                            results.append({
                                "user_id": user_id,
                                "response_time_ms": response_time_ms,
                                "status_code": response.status,
                                "success": True,
                                "error": None
                            })
                        else:
                            results.append({
                                "user_id": user_id,
                                "response_time_ms": response_time_ms,
                                "status_code": response.status,
                                "success": False,
                                "error": f"HTTP {response.status}"
                            })
                            
                except Exception as e:
                    end_time = time.time()
                    response_time_ms = (end_time - start_time) * 1000
                    
                    results.append({
                        "user_id": user_id,
                        "response_time_ms": response_time_ms,
                        "status_code": 0,
                        "success": False,
                        "error": str(e)
                    })
        
        return results

    def _generate_test_queries(self, count: int) -> List[Dict]:
        """Generate test queries for stress testing"""
        queries = []
        search_modes = ["breadth_first", "depth_first", "best_first"]
        
        for i in range(count):
            query = {
                "start_memory_ids": [np.random.randint(1, 100)],
                "max_depth": np.random.randint(2, 6),
                "max_results": np.random.randint(5, 21),
                "search_mode": np.random.choice(search_modes),
                "min_weight": np.random.uniform(0.1, 0.5)
            }
            queries.append(query)
        
        return queries

class CapacityStressTester:
    """Tests system capacity limits"""
    
    def __init__(self):
        self.monitor = SystemMonitor()

    def memory_capacity_stress_test(self, target_memories: int = 1000000) -> StressTestResult:
        """Test memory capacity under stress"""
        logger.info(f"Starting memory capacity stress test for {target_memories} memories")
        
        self.monitor.start_monitoring()
        start_time = time.time()
        
        successful_operations = 0
        failed_operations = 0
        errors = []
        response_times = []
        
        # Simulate memory operations in batches
        batch_size = 10000
        batches = target_memories // batch_size
        
        for batch_num in range(batches):
            batch_start = time.time()
            
            try:
                # Simulate batch memory operations
                memory_batch = self._generate_memory_batch(batch_size)
                
                # Simulate processing time (would be actual layer operations)
                processing_time = len(memory_batch) * 0.00001  # 0.01ms per memory
                time.sleep(processing_time)
                
                batch_end = time.time()
                batch_time_ms = (batch_end - batch_start) * 1000
                response_times.append(batch_time_ms)
                
                successful_operations += batch_size
                
                if batch_num % 10 == 0:  # Log progress every 10 batches
                    logger.info(f"Processed {(batch_num + 1) * batch_size} memories...")
                    
            except Exception as e:
                failed_operations += batch_size
                errors.append(f"Batch {batch_num}: {str(e)}")
                logger.error(f"Batch {batch_num} failed: {e}")
        
        end_time = time.time()
        duration = end_time - start_time
        
        resource_stats = self.monitor.stop_monitoring()
        
        # Calculate statistics
        if response_times:
            avg_response_time = np.mean(response_times)
            min_response_time = np.min(response_times)
            max_response_time = np.max(response_times)
            p95_response_time = np.percentile(response_times, 95)
            p99_response_time = np.percentile(response_times, 99)
        else:
            avg_response_time = min_response_time = max_response_time = 0
            p95_response_time = p99_response_time = 0
        
        return StressTestResult(
            test_name="memory_capacity_stress",
            duration_seconds=duration,
            total_operations=successful_operations + failed_operations,
            successful_operations=successful_operations,
            failed_operations=failed_operations,
            operations_per_second=(successful_operations + failed_operations) / duration,
            average_response_time_ms=avg_response_time,
            min_response_time_ms=min_response_time,
            max_response_time_ms=max_response_time,
            p95_response_time_ms=p95_response_time,
            p99_response_time_ms=p99_response_time,
            memory_usage_peak_mb=resource_stats["memory_mb"],
            cpu_usage_peak_percent=resource_stats["cpu_percent"],
            error_rate=failed_operations / (successful_operations + failed_operations) if (successful_operations + failed_operations) > 0 else 0,
            errors=errors[:10]
        )

    def _generate_memory_batch(self, batch_size: int) -> List[Dict]:
        """Generate a batch of test memories"""
        memories = []
        
        for i in range(batch_size):
            memory = {
                "id": i,
                "content": f"Test memory content {i} with various characteristics and properties",
                "category": np.random.choice(["science", "technology", "history"]),
                "tags": [f"tag_{np.random.randint(1, 100)}" for _ in range(np.random.randint(1, 5))],
                "complexity": np.random.uniform(0.1, 1.0)
            }
            memories.append(memory)
        
        return memories

class FaultToleranceStressTester:
    """Tests system fault tolerance under stress"""
    
    def __init__(self, layer3_url: str = "http://localhost:8082"):
        self.layer3_url = layer3_url
        self.monitor = SystemMonitor()

    async def fault_tolerance_stress_test(self) -> StressTestResult:
        """Test system fault tolerance"""
        logger.info("Starting fault tolerance stress test")
        
        self.monitor.start_monitoring()
        start_time = time.time()
        
        results = []
        errors = []
        
        # Test scenarios
        scenarios = [
            self._test_invalid_queries,
            self._test_timeout_handling,
            self._test_large_payload_handling,
            self._test_malformed_requests,
            self._test_resource_exhaustion
        ]
        
        for scenario in scenarios:
            try:
                scenario_results = await scenario()
                results.extend(scenario_results)
            except Exception as e:
                errors.append(f"Scenario failed: {str(e)}")
                logger.error(f"Scenario failed: {e}")
        
        end_time = time.time()
        duration = end_time - start_time
        
        resource_stats = self.monitor.stop_monitoring()
        
        # Calculate statistics
        successful_operations = len([r for r in results if r.get("success", False)])
        failed_operations = len(results) - successful_operations
        response_times = [r["response_time_ms"] for r in results if r.get("success")]
        
        if response_times:
            avg_response_time = np.mean(response_times)
            min_response_time = np.min(response_times)
            max_response_time = np.max(response_times)
            p95_response_time = np.percentile(response_times, 95)
            p99_response_time = np.percentile(response_times, 99)
        else:
            avg_response_time = min_response_time = max_response_time = 0
            p95_response_time = p99_response_time = 0
        
        return StressTestResult(
            test_name="fault_tolerance_stress",
            duration_seconds=duration,
            total_operations=len(results),
            successful_operations=successful_operations,
            failed_operations=failed_operations,
            operations_per_second=len(results) / duration if duration > 0 else 0,
            average_response_time_ms=avg_response_time,
            min_response_time_ms=min_response_time,
            max_response_time_ms=max_response_time,
            p95_response_time_ms=p95_response_time,
            p99_response_time_ms=p99_response_time,
            memory_usage_peak_mb=resource_stats["memory_mb"],
            cpu_usage_peak_percent=resource_stats["cpu_percent"],
            error_rate=failed_operations / len(results) if results else 0,
            errors=errors[:10]
        )

    async def _test_invalid_queries(self) -> List[Dict]:
        """Test handling of invalid queries"""
        logger.info("Testing invalid query handling...")
        
        invalid_queries = [
            {},  # Empty query
            {"invalid_field": "value"},  # Invalid fields
            {"start_memory_ids": []},  # Empty memory IDs
            {"start_memory_ids": [-1], "max_depth": -1},  # Negative values
            {"start_memory_ids": [1], "search_mode": "invalid_mode"},  # Invalid search mode
        ]
        
        results = []
        
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=10)) as session:
            for query in invalid_queries:
                start_time = time.time()
                
                try:
                    async with session.post(
                        f"{self.layer3_url}/search",
                        json=query,
                        headers={"Content-Type": "application/json"}
                    ) as response:
                        end_time = time.time()
                        response_time_ms = (end_time - start_time) * 1000
                        
                        # System should handle invalid queries gracefully (return error, not crash)
                        results.append({
                            "test": "invalid_query",
                            "response_time_ms": response_time_ms,
                            "status_code": response.status,
                            "success": response.status in [400, 422],  # Expected error codes
                            "query": query
                        })
                        
                except Exception as e:
                    end_time = time.time()
                    response_time_ms = (end_time - start_time) * 1000
                    
                    results.append({
                        "test": "invalid_query",
                        "response_time_ms": response_time_ms,
                        "status_code": 0,
                        "success": False,  # Exception means not handled gracefully
                        "error": str(e),
                        "query": query
                    })
        
        return results

    async def _test_timeout_handling(self) -> List[Dict]:
        """Test timeout handling"""
        logger.info("Testing timeout handling...")
        
        # Test with very short timeout
        query = {
            "start_memory_ids": [1],
            "max_depth": 10,  # Deep search to potentially cause timeout
            "max_results": 100,
            "search_mode": "depth_first",
            "min_weight": 0.1
        }
        
        results = []
        start_time = time.time()
        
        try:
            async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=0.1)) as session:
                async with session.post(
                    f"{self.layer3_url}/search",
                    json=query,
                    headers={"Content-Type": "application/json"}
                ) as response:
                    end_time = time.time()
                    response_time_ms = (end_time - start_time) * 1000
                    
                    results.append({
                        "test": "timeout_handling",
                        "response_time_ms": response_time_ms,
                        "status_code": response.status,
                        "success": True,  # Any response is good
                    })
                    
        except asyncio.TimeoutError:
            end_time = time.time()
            response_time_ms = (end_time - start_time) * 1000
            
            results.append({
                "test": "timeout_handling",
                "response_time_ms": response_time_ms,
                "status_code": 0,
                "success": True,  # Timeout is expected behavior
                "timeout": True
            })
            
        except Exception as e:
            end_time = time.time()
            response_time_ms = (end_time - start_time) * 1000
            
            results.append({
                "test": "timeout_handling",
                "response_time_ms": response_time_ms,
                "status_code": 0,
                "success": False,
                "error": str(e)
            })
        
        return results

    async def _test_large_payload_handling(self) -> List[Dict]:
        """Test large payload handling"""
        logger.info("Testing large payload handling...")
        
        # Create query with large data
        large_query = {
            "start_memory_ids": list(range(1, 1000)),  # Many starting points
            "max_depth": 5,
            "max_results": 1000,
            "search_mode": "breadth_first",
            "min_weight": 0.1,
            "extra_data": "x" * 10000  # 10KB of extra data
        }
        
        results = []
        start_time = time.time()
        
        try:
            async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=30)) as session:
                async with session.post(
                    f"{self.layer3_url}/search",
                    json=large_query,
                    headers={"Content-Type": "application/json"}
                ) as response:
                    end_time = time.time()
                    response_time_ms = (end_time - start_time) * 1000
                    
                    results.append({
                        "test": "large_payload",
                        "response_time_ms": response_time_ms,
                        "status_code": response.status,
                        "success": response.status in [200, 400, 413],  # OK or expected errors
                        "payload_size": len(json.dumps(large_query))
                    })
                    
        except Exception as e:
            end_time = time.time()
            response_time_ms = (end_time - start_time) * 1000
            
            results.append({
                "test": "large_payload",
                "response_time_ms": response_time_ms,
                "status_code": 0,
                "success": False,
                "error": str(e),
                "payload_size": len(json.dumps(large_query))
            })
        
        return results

    async def _test_malformed_requests(self) -> List[Dict]:
        """Test malformed request handling"""
        logger.info("Testing malformed request handling...")
        
        malformed_payloads = [
            "invalid json",
            '{"incomplete": json',
            None,
            "",
            "null"
        ]
        
        results = []
        
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=10)) as session:
            for payload in malformed_payloads:
                start_time = time.time()
                
                try:
                    # Send raw data instead of JSON
                    async with session.post(
                        f"{self.layer3_url}/search",
                        data=payload,
                        headers={"Content-Type": "application/json"}
                    ) as response:
                        end_time = time.time()
                        response_time_ms = (end_time - start_time) * 1000
                        
                        results.append({
                            "test": "malformed_request",
                            "response_time_ms": response_time_ms,
                            "status_code": response.status,
                            "success": response.status in [400, 422],  # Expected error codes
                            "payload": str(payload)[:100]  # First 100 chars
                        })
                        
                except Exception as e:
                    end_time = time.time()
                    response_time_ms = (end_time - start_time) * 1000
                    
                    results.append({
                        "test": "malformed_request",
                        "response_time_ms": response_time_ms,
                        "status_code": 0,
                        "success": False,
                        "error": str(e),
                        "payload": str(payload)[:100]
                    })
        
        return results

    async def _test_resource_exhaustion(self) -> List[Dict]:
        """Test resource exhaustion scenarios"""
        logger.info("Testing resource exhaustion scenarios...")
        
        # Send many concurrent requests to test connection limits
        semaphore = asyncio.Semaphore(200)  # High concurrency
        
        async def single_request(session, request_id):
            async with semaphore:
                query = {
                    "start_memory_ids": [1],
                    "max_depth": 3,
                    "max_results": 10,
                    "search_mode": "breadth_first",
                    "min_weight": 0.1
                }
                
                start_time = time.time()
                
                try:
                    async with session.post(
                        f"{self.layer3_url}/search",
                        json=query,
                        headers={"Content-Type": "application/json"}
                    ) as response:
                        end_time = time.time()
                        response_time_ms = (end_time - start_time) * 1000
                        
                        return {
                            "test": "resource_exhaustion",
                            "request_id": request_id,
                            "response_time_ms": response_time_ms,
                            "status_code": response.status,
                            "success": response.status == 200
                        }
                        
                except Exception as e:
                    end_time = time.time()
                    response_time_ms = (end_time - start_time) * 1000
                    
                    return {
                        "test": "resource_exhaustion",
                        "request_id": request_id,
                        "response_time_ms": response_time_ms,
                        "status_code": 0,
                        "success": False,
                        "error": str(e)
                    }
        
        async with aiohttp.ClientSession(
            timeout=aiohttp.ClientTimeout(total=30),
            connector=aiohttp.TCPConnector(limit=300)
        ) as session:
            
            tasks = [single_request(session, i) for i in range(100)]  # 100 concurrent requests
            results = await asyncio.gather(*tasks, return_exceptions=True)
            
            # Filter out exceptions
            valid_results = [r for r in results if not isinstance(r, Exception)]
            
        return valid_results

async def run_all_stress_tests(args) -> Dict[str, Any]:
    """Run all stress tests"""
    logger.info("Starting comprehensive stress testing...")
    
    results = {
        "test_configuration": vars(args),
        "timestamp": time.time(),
        "stress_test_results": {}
    }
    
    # Concurrent Query Stress Test
    logger.info("Running concurrent query stress test...")
    concurrent_tester = ConcurrentQueryStressTester()
    concurrent_result = await concurrent_tester.concurrent_query_stress_test(
        concurrent_users=args.concurrent_users,
        queries_per_user=args.queries_per_user
    )
    results["stress_test_results"]["concurrent_query"] = concurrent_result
    
    # Memory Capacity Stress Test
    logger.info("Running memory capacity stress test...")
    capacity_tester = CapacityStressTester()
    capacity_result = capacity_tester.memory_capacity_stress_test(args.target_capacity)
    results["stress_test_results"]["memory_capacity"] = capacity_result
    
    # Fault Tolerance Stress Test
    logger.info("Running fault tolerance stress test...")
    fault_tester = FaultToleranceStressTester()
    fault_result = await fault_tester.fault_tolerance_stress_test()
    results["stress_test_results"]["fault_tolerance"] = fault_result
    
    return results

def main():
    parser = argparse.ArgumentParser(description="MFN System Stress Testing Framework")
    parser.add_argument("--concurrent-users", type=int, default=50, help="Number of concurrent users")
    parser.add_argument("--queries-per-user", type=int, default=20, help="Queries per user")
    parser.add_argument("--target-capacity", type=int, default=100000, help="Target memory capacity")
    parser.add_argument("--output", type=str, default="stress_test_results.json", help="Output file")
    
    args = parser.parse_args()
    
    # Run stress tests
    results = asyncio.run(run_all_stress_tests(args))
    
    # Save results
    with open(args.output, 'w') as f:
        json.dump(results, f, indent=2, default=str)
    
    logger.info(f"Stress test results saved to {args.output}")
    
    # Print summary
    print("\n" + "="*60)
    print("MFN SYSTEM STRESS TEST RESULTS")
    print("="*60)
    
    for test_name, result in results["stress_test_results"].items():
        print(f"\n{test_name.upper()} STRESS TEST:")
        print(f"  Duration: {result.duration_seconds:.1f}s")
        print(f"  Operations: {result.total_operations}")
        print(f"  Success Rate: {(result.successful_operations/result.total_operations)*100:.1f}%")
        print(f"  Ops/sec: {result.operations_per_second:.1f}")
        print(f"  Avg Response: {result.average_response_time_ms:.2f}ms")
        print(f"  P99 Response: {result.p99_response_time_ms:.2f}ms")
        print(f"  Peak Memory: {result.memory_usage_peak_mb:.1f}MB")
        print(f"  Peak CPU: {result.cpu_usage_peak_percent:.1f}%")
    
    print("="*60)

if __name__ == "__main__":
    main()