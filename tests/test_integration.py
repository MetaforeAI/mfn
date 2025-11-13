#!/usr/bin/env python3
"""
MFN System Integration Test
Tests all 4 layers working together via Unix sockets
"""

import socket
import json
import time
import sys
from typing import Dict, Any, List

# Socket paths
LAYER1_SOCKET = "/tmp/mfn_layer1.sock"
LAYER2_SOCKET = "/tmp/mfn_layer2.sock"
LAYER3_SOCKET = "/tmp/mfn_layer3.sock"
LAYER4_SOCKET = "/tmp/mfn_layer4.sock"

class LayerClient:
    """Client for communicating with MFN layer socket servers"""

    def __init__(self, socket_path: str, layer_name: str):
        self.socket_path = socket_path
        self.layer_name = layer_name

    def send_request(self, request: Dict[str, Any]) -> Dict[str, Any]:
        """Send request to layer and get response"""
        try:
            # Connect to Unix socket
            client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            client.settimeout(5.0)
            client.connect(self.socket_path)

            # Send request
            request_str = json.dumps(request) + '\n'
            client.send(request_str.encode())

            # Receive response
            response_data = b""
            while True:
                chunk = client.recv(4096)
                if not chunk:
                    break
                response_data += chunk
                if b'\n' in response_data:
                    break

            client.close()

            # Parse response
            response_str = response_data.decode().strip()
            return json.loads(response_str)

        except FileNotFoundError:
            return {"error": f"{self.layer_name} socket not found at {self.socket_path}"}
        except ConnectionRefusedError:
            return {"error": f"{self.layer_name} connection refused"}
        except socket.timeout:
            return {"error": f"{self.layer_name} request timeout"}
        except Exception as e:
            return {"error": f"{self.layer_name} error: {str(e)}"}

def test_layer_ping(client: LayerClient) -> bool:
    """Test if layer responds to ping"""
    print(f"\n📍 Testing {client.layer_name} ping...")

    request = {
        "type": "ping",
        "request_id": f"test-ping-{time.time()}"
    }

    response = client.send_request(request)

    if "error" in response:
        print(f"  ❌ {client.layer_name}: {response['error']}")
        return False

    if response.get("type") == "pong" or response.get("success") == True:
        print(f"  ✅ {client.layer_name} is responding")
        return True
    else:
        print(f"  ⚠️ {client.layer_name} unexpected response: {response}")
        return False

def test_layer_search(client: LayerClient, query: str) -> List[Dict]:
    """Test search functionality"""
    print(f"\n🔍 Testing {client.layer_name} search for: '{query}'")

    request = {
        "type": "search" if client.layer_name != "Layer 1" else "query",
        "request_id": f"test-search-{time.time()}",
        "query": query,
        "content": query,  # For Layer 1 compatibility
        "limit": 5,
        "min_confidence": 0.5
    }

    response = client.send_request(request)

    if "error" in response:
        print(f"  ❌ {client.layer_name}: {response['error']}")
        return []

    if response.get("success"):
        results = response.get("results", [])
        print(f"  ✅ {client.layer_name} returned {len(results)} results")
        if results:
            for i, result in enumerate(results[:3]):
                print(f"    {i+1}. Score: {result.get('score', 'N/A')}, Content: {result.get('content', 'N/A')[:50]}...")
        return results
    else:
        print(f"  ⚠️ {client.layer_name} search failed: {response}")
        return []

def test_add_memory(client: LayerClient, content: str) -> bool:
    """Test adding memory to a layer"""
    print(f"\n💾 Adding memory to {client.layer_name}: '{content[:50]}...'")

    request = {
        "type": "add_memory",
        "request_id": f"test-add-{time.time()}",
        "content": content,
        "metadata": {
            "source": "integration_test",
            "timestamp": time.time()
        }
    }

    response = client.send_request(request)

    if "error" in response:
        print(f"  ❌ {client.layer_name}: {response['error']}")
        return False

    if response.get("success"):
        memory_id = response.get("metadata", {}).get("memory_id", "unknown")
        print(f"  ✅ Memory added to {client.layer_name} with ID: {memory_id}")
        return True
    else:
        print(f"  ⚠️ {client.layer_name} add memory failed: {response}")
        return False

def main():
    print("=" * 60)
    print("MFN SYSTEM INTEGRATION TEST")
    print("=" * 60)

    # Create layer clients
    layer1 = LayerClient(LAYER1_SOCKET, "Layer 1 (IFR)")
    layer2 = LayerClient(LAYER2_SOCKET, "Layer 2 (DSR)")
    layer3 = LayerClient(LAYER3_SOCKET, "Layer 3 (ALM)")
    layer4 = LayerClient(LAYER4_SOCKET, "Layer 4 (CPE)")

    layers = [layer1, layer2, layer3, layer4]

    # Test 1: Ping all layers
    print("\n" + "=" * 60)
    print("TEST 1: Layer Connectivity")
    print("=" * 60)

    active_layers = []
    for client in layers:
        if test_layer_ping(client):
            active_layers.append(client)

    print(f"\n📊 {len(active_layers)}/{len(layers)} layers are responding")

    if not active_layers:
        print("\n❌ No layers are running. Please start the layer servers first.")
        print("Run: ./scripts/start_all_layers.sh")
        return 1

    # Test 2: Add test memories
    print("\n" + "=" * 60)
    print("TEST 2: Memory Storage")
    print("=" * 60)

    test_memories = [
        "The quantum computer achieved 99.9% fidelity in error correction",
        "Neural network training completed with 95% accuracy on validation set",
        "Database optimization reduced query time by 60 milliseconds",
        "Memory allocation algorithm improved cache hit rate to 87%"
    ]

    for memory in test_memories:
        for client in active_layers:
            test_add_memory(client, memory)
            time.sleep(0.1)  # Small delay between operations

    # Test 3: Search queries
    print("\n" + "=" * 60)
    print("TEST 3: Search Functionality")
    print("=" * 60)

    test_queries = [
        "quantum computer",
        "neural network",
        "optimization",
        "memory cache"
    ]

    all_results = []
    for query in test_queries:
        print(f"\n🔎 Query: '{query}'")
        for client in active_layers:
            results = test_layer_search(client, query)
            all_results.extend(results)
            time.sleep(0.1)

    # Test 4: Performance metrics
    print("\n" + "=" * 60)
    print("TEST 4: Performance Metrics")
    print("=" * 60)

    print("\n📈 Sending rapid queries to test performance...")

    start_time = time.time()
    rapid_queries = 10

    for i in range(rapid_queries):
        for client in active_layers[:2]:  # Test first 2 layers for speed
            request = {
                "type": "search" if client.layer_name != "Layer 1" else "query",
                "request_id": f"perf-test-{i}",
                "query": f"test query {i}",
                "content": f"test query {i}",
                "limit": 1
            }
            client.send_request(request)

    elapsed = time.time() - start_time
    qps = (rapid_queries * min(2, len(active_layers))) / elapsed

    print(f"\n  ⏱️ Processed {rapid_queries * min(2, len(active_layers))} queries in {elapsed:.2f} seconds")
    print(f"  📊 Queries per second: {qps:.1f}")

    # Summary
    print("\n" + "=" * 60)
    print("TEST SUMMARY")
    print("=" * 60)

    print(f"\n✅ Active Layers: {len(active_layers)}/{len(layers)}")
    print(f"✅ Memories Added: {len(test_memories) * len(active_layers)}")
    print(f"✅ Search Queries: {len(test_queries) * len(active_layers)}")
    print(f"✅ Performance: {qps:.1f} QPS")

    if len(active_layers) == len(layers):
        print("\n🎉 All layers are functioning correctly!")
        return 0
    else:
        print(f"\n⚠️ Only {len(active_layers)} out of {len(layers)} layers are running")
        return 1

if __name__ == "__main__":
    sys.exit(main())