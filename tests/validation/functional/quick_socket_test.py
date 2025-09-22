#!/usr/bin/env python3
"""Quick test with corrected HTTP timeout format"""
import json
import socket
import time
import requests

def test_http_fixed():
    """Test HTTP with correct timeout format"""
    query = {
        "start_memory_ids": [1, 2, 3],
        "max_depth": 3,
        "max_results": 10,
        "min_weight": 0.1,
        "timeout": 50000000,  # 50ms in nanoseconds
        "search_mode": "best_first"
    }
    
    times = []
    for i in range(10):
        start = time.perf_counter()
        try:
            response = requests.post("http://localhost:8082/search/associative", json=query, timeout=1.0)
            elapsed_ms = (time.perf_counter() - start) * 1000
            times.append(elapsed_ms)
            print(f"HTTP Request {i+1}: {elapsed_ms:.2f}ms - Status: {response.status_code}")
        except Exception as e:
            elapsed_ms = (time.perf_counter() - start) * 1000
            print(f"HTTP Request {i+1}: {elapsed_ms:.2f}ms - Error: {e}")
    
    if times:
        print(f"HTTP Average: {sum(times)/len(times):.2f}ms")

def test_socket_quick():
    """Quick Unix socket test"""
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect("/tmp/mfn_layer3.sock")
    
    times = []
    for i in range(10):
        query = {
            "type": "associative_search",
            "request_id": f"quick_test_{i}",
            "payload": {
                "start_memory_ids": [1, 2, 3],
                "max_depth": 3,
                "max_results": 10,
                "min_weight": 0.1,
                "timeout_ms": 50,
                "search_mode": "best_first"
            }
        }
        
        start = time.perf_counter()
        try:
            sock.sendall(json.dumps(query).encode())
            
            response_data = b""
            while True:
                chunk = sock.recv(4096)
                if not chunk:
                    break
                response_data += chunk
                try:
                    response = json.loads(response_data.decode('utf-8'))
                    break
                except json.JSONDecodeError:
                    continue
            
            elapsed_ms = (time.perf_counter() - start) * 1000
            times.append(elapsed_ms)
            print(f"Socket Request {i+1}: {elapsed_ms:.2f}ms - Success: {response.get('success', False)}")
        except Exception as e:
            elapsed_ms = (time.perf_counter() - start) * 1000
            print(f"Socket Request {i+1}: {elapsed_ms:.2f}ms - Error: {e}")
    
    sock.close()
    if times:
        print(f"Socket Average: {sum(times)/len(times):.2f}ms")

if __name__ == "__main__":
    print("🔧 Quick Socket vs HTTP Test")
    print("=" * 30)
    
    print("\n📡 Testing HTTP API...")
    test_http_fixed()
    
    print("\n🔌 Testing Unix Socket...")  
    test_socket_quick()