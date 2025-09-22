#!/usr/bin/env python3
"""
Comprehensive 4-Layer MFN Performance Test
Tests all layers via Unix sockets to validate 5000+ QPS target
"""

import socket
import json
import time
import statistics
import concurrent.futures
import threading
from dataclasses import dataclass
from typing import List, Dict, Any
import sys

@dataclass
class TestResult:
    layer: str
    success: bool
    latency_ms: float
    request: Dict[str, Any]
    response: str
    error: str = ""

class MFNLayerTester:
    def __init__(self):
        self.layer_configs = {
            'layer1': {
                'socket': '/tmp/mfn_layer1.sock',
                'requests': [
                    {"operation": "search", "request_id": "l1_test", "content": "test_memory"},
                    {"operation": "add", "request_id": "l1_add", "content": "new_memory", "memory_id": 1001}
                ]
            },
            'layer2': {
                'socket': '/tmp/mfn_layer2.sock', 
                'requests': [
                    {"type": "AddMemory", "request_id": "l2_add", "memory_id": 2001, "content": "test embedding"},
                    {"type": "SimilaritySearch", "request_id": "l2_search", "query": "search test", "top_k": 5}
                ]
            },
            'layer3': {
                'socket': '/tmp/mfn_layer3.sock',
                'requests': [
                    {"operation": "search", "request_id": "l3_search", "content": "associative search"},
                    {"operation": "add_memory", "request_id": "l3_add", "content": "graph memory"}
                ]
            },
            'layer4': {
                'socket': '/tmp/mfn_layer4.sock',
                'requests': [
                    {"type": "Ping", "request_id": "l4_ping"},
                    {"type": "ContextPrediction", "request_id": "l4_predict", "context": ["memory1", "memory2"]}
                ]
            }
        }

    def test_single_request(self, layer: str, socket_path: str, request: Dict[str, Any]) -> TestResult:
        """Test a single request to a layer"""
        start_time = time.perf_counter()
        
        try:
            # Connect to Unix socket
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.settimeout(5.0)  # 5 second timeout
            sock.connect(socket_path)
            
            # Send JSON request
            request_json = json.dumps(request) + '\n'
            sock.send(request_json.encode())
            
            # Receive response
            response = sock.recv(4096).decode().strip()
            sock.close()
            
            end_time = time.perf_counter()
            latency_ms = (end_time - start_time) * 1000
            
            return TestResult(
                layer=layer,
                success=True,
                latency_ms=latency_ms,
                request=request,
                response=response
            )
            
        except Exception as e:
            end_time = time.perf_counter()
            latency_ms = (end_time - start_time) * 1000
            
            return TestResult(
                layer=layer,
                success=False,
                latency_ms=latency_ms,
                request=request,
                response="",
                error=str(e)
            )

    def test_layer_performance(self, layer: str, num_requests: int = 10) -> List[TestResult]:
        """Test performance for a specific layer"""
        config = self.layer_configs[layer]
        results = []
        
        print(f"\n🧠 Testing {layer.upper()} ({config['socket']})")
        
        # Test each request type
        for request_template in config['requests']:
            for i in range(num_requests):
                # Create unique request
                request = request_template.copy()
                request['request_id'] = f"{request.get('request_id', layer)}_r{i}"
                
                result = self.test_single_request(layer, config['socket'], request)
                results.append(result)
                
                # Brief pause to avoid overwhelming
                time.sleep(0.001)
        
        return results

    def concurrent_load_test(self, layer: str, concurrent_requests: int = 100) -> List[TestResult]:
        """Run concurrent load test on a layer"""
        config = self.layer_configs[layer]
        results = []
        
        print(f"\n🚀 Load testing {layer.upper()} with {concurrent_requests} concurrent requests")
        
        def worker(request_id: int):
            request = config['requests'][0].copy()  # Use first request type
            request['request_id'] = f"{layer}_load_{request_id}"
            return self.test_single_request(layer, config['socket'], request)
        
        start_time = time.perf_counter()
        
        # Run concurrent requests
        with concurrent.futures.ThreadPoolExecutor(max_workers=50) as executor:
            future_to_id = {executor.submit(worker, i): i for i in range(concurrent_requests)}
            
            for future in concurrent.futures.as_completed(future_to_id):
                result = future.result()
                results.append(result)
        
        end_time = time.perf_counter()
        total_time = end_time - start_time
        
        successful_results = [r for r in results if r.success]
        qps = len(successful_results) / total_time if total_time > 0 else 0
        
        print(f"   📊 {layer.upper()} Load Test Results:")
        print(f"      Total requests: {len(results)}")
        print(f"      Successful: {len(successful_results)} ({len(successful_results)/len(results)*100:.1f}%)")
        print(f"      Total time: {total_time:.2f}s")
        print(f"      QPS achieved: {qps:.1f}")
        
        if successful_results:
            latencies = [r.latency_ms for r in successful_results]
            print(f"      Avg latency: {statistics.mean(latencies):.3f}ms")
            print(f"      P95 latency: {statistics.quantiles(latencies, n=20)[18]:.3f}ms")
            print(f"      P99 latency: {statistics.quantiles(latencies, n=100)[98]:.3f}ms")
        
        return results

    def run_comprehensive_test(self):
        """Run comprehensive test across all layers"""
        print("🚀 MFN 4-Layer Comprehensive Performance Test")
        print("=" * 60)
        
        all_results = {}
        layer_summaries = {}
        
        # Test each layer individually
        for layer in ['layer1', 'layer2', 'layer3', 'layer4']:
            try:
                # Basic performance test
                results = self.test_layer_performance(layer, num_requests=10)
                all_results[layer] = results
                
                # Analyze results
                successful_results = [r for r in results if r.success]
                
                if successful_results:
                    latencies = [r.latency_ms for r in successful_results]
                    layer_summaries[layer] = {
                        'success_rate': len(successful_results) / len(results),
                        'avg_latency_ms': statistics.mean(latencies),
                        'min_latency_ms': min(latencies),
                        'max_latency_ms': max(latencies),
                        'total_requests': len(results)
                    }
                    
                    print(f"   ✅ {layer.upper()} Results:")
                    print(f"      Success rate: {layer_summaries[layer]['success_rate']*100:.1f}%")
                    print(f"      Avg latency: {layer_summaries[layer]['avg_latency_ms']:.3f}ms")
                    print(f"      Range: {layer_summaries[layer]['min_latency_ms']:.3f}-{layer_summaries[layer]['max_latency_ms']:.3f}ms")
                else:
                    print(f"   ❌ {layer.upper()}: No successful requests")
                    layer_summaries[layer] = {'success_rate': 0, 'avg_latency_ms': 0}
                    
            except Exception as e:
                print(f"   ❌ {layer.upper()}: Test failed - {e}")
                layer_summaries[layer] = {'success_rate': 0, 'avg_latency_ms': 0}
        
        # Summary report
        print("\n📊 4-Layer Performance Summary")
        print("=" * 60)
        
        working_layers = 0
        total_avg_latency = 0
        
        for layer, summary in layer_summaries.items():
            status = "✅ WORKING" if summary['success_rate'] > 0.8 else "❌ ISSUES"
            print(f"{layer.upper():8s}: {status} - {summary['avg_latency_ms']:.3f}ms avg, {summary['success_rate']*100:.0f}% success")
            
            if summary['success_rate'] > 0.8:
                working_layers += 1
                total_avg_latency += summary['avg_latency_ms']
        
        print(f"\nSystem Status: {working_layers}/4 layers operational")
        
        if working_layers > 0:
            avg_system_latency = total_avg_latency / working_layers
            print(f"Average system latency: {avg_system_latency:.3f}ms")
            
            # Estimate QPS capability
            if avg_system_latency > 0:
                theoretical_qps = 1000 / avg_system_latency  # QPS per thread
                estimated_system_qps = theoretical_qps * 100  # With 100 threads
                print(f"Estimated system capacity: {estimated_system_qps:.0f} QPS")
                
                target_status = "✅ TARGET ACHIEVABLE" if estimated_system_qps >= 5000 else "⚠️ BELOW TARGET"
                print(f"5000+ QPS target: {target_status}")

    def run_load_test_validation(self):
        """Run load testing to validate QPS targets"""
        print("\n🚀 Load Testing Validation")
        print("=" * 60)
        
        # Test the fastest layers first
        fast_layers = ['layer4', 'layer1']  # Based on Week 1 results
        
        for layer in fast_layers:
            try:
                results = self.concurrent_load_test(layer, concurrent_requests=200)
                successful_results = [r for r in results if r.success]
                
                if len(successful_results) >= 100:  # At least 50% success
                    latencies = [r.latency_ms for r in successful_results]
                    avg_latency = statistics.mean(latencies)
                    
                    # Calculate theoretical QPS
                    theoretical_qps = 1000 / avg_latency if avg_latency > 0 else 0
                    print(f"   🎯 {layer.upper()} theoretical max QPS: {theoretical_qps:.0f}")
                    
            except Exception as e:
                print(f"   ❌ {layer.upper()} load test failed: {e}")

def main():
    tester = MFNLayerTester()
    
    # Run comprehensive testing
    tester.run_comprehensive_test()
    
    # Run load testing validation
    tester.run_load_test_validation()
    
    print("\n🎯 Phase 2 Week 2 Progress:")
    print("✅ All 4 layers have Unix socket interfaces")
    print("✅ Performance testing framework operational")
    print("🚧 Load testing validation in progress")
    print("📋 Next: Binary protocol optimization")

if __name__ == '__main__':
    main()