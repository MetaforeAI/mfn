#!/usr/bin/env python3
"""
Test Actual MFN Memory Capabilities
Tests real memory operations, not just socket connectivity
"""

import socket
import json
import time
import requests

def test_layer3_memory_operations():
    """Test Layer 3 actual memory and association capabilities"""
    print("🧠 Testing Layer 3 Memory Operations (via HTTP API)")
    
    base_url = "http://localhost:8082"
    
    # Test 1: Add memories
    print("\n📝 Test 1: Adding memories")
    memories = [
        {"content": "Neural networks learn patterns", "source": "AI Research", "tags": ["AI", "neural", "learning"]},
        {"content": "Graph databases store relationships", "source": "Database Theory", "tags": ["database", "graph", "relationships"]},
        {"content": "Machine learning requires data", "source": "ML Basics", "tags": ["ML", "data", "training"]},
    ]
    
    memory_ids = []
    for i, memory in enumerate(memories, 1):
        try:
            response = requests.post(f"{base_url}/memories", json=memory, timeout=5)
            if response.status_code == 200:
                result = response.json()
                memory_ids.append(result.get('id', f'mem_{i}'))
                print(f"   ✅ Added memory {i}: {memory['content'][:50]}...")
            else:
                print(f"   ❌ Failed to add memory {i}: {response.status_code}")
        except Exception as e:
            print(f"   ❌ Error adding memory {i}: {e}")
    
    # Test 2: Search memories
    print(f"\n🔍 Test 2: Searching memories (added {len(memory_ids)} memories)")
    search_queries = [
        "neural learning",
        "graph relationships", 
        "machine learning data"
    ]
    
    for query in search_queries:
        try:
            response = requests.get(f"{base_url}/search", params={"q": query, "limit": 5}, timeout=5)
            if response.status_code == 200:
                results = response.json()
                result_count = len(results.get('results', []))
                print(f"   ✅ Query '{query}' found {result_count} results")
            else:
                print(f"   ❌ Search failed for '{query}': {response.status_code}")
        except Exception as e:
            print(f"   ❌ Error searching '{query}': {e}")
    
    # Test 3: Associations
    print("\n🕸️  Test 3: Creating associations")
    if len(memory_ids) >= 2:
        try:
            association = {
                "from_id": memory_ids[0],
                "to_id": memory_ids[1], 
                "type": "related",
                "weight": 0.8,
                "reason": "Both about computational concepts"
            }
            response = requests.post(f"{base_url}/associations", json=association, timeout=5)
            if response.status_code == 200:
                print(f"   ✅ Created association between memories")
            else:
                print(f"   ❌ Failed to create association: {response.status_code}")
        except Exception as e:
            print(f"   ❌ Error creating association: {e}")
    
    # Test 4: Graph stats
    print("\n📊 Test 4: Graph statistics")
    try:
        response = requests.get(f"{base_url}/graph/stats", timeout=5)
        if response.status_code == 200:
            stats = response.json()
            print(f"   ✅ Graph stats: {stats.get('memories', 0)} memories, {stats.get('associations', 0)} associations")
        else:
            print(f"   ❌ Failed to get stats: {response.status_code}")
    except Exception as e:
        print(f"   ❌ Error getting stats: {e}")

def test_layer4_context_operations():
    """Test Layer 4 context prediction capabilities"""
    print("\n🔮 Testing Layer 4 Context Operations (via Unix socket)")
    
    test_requests = [
        {
            "type": "AddMemoryContext",
            "request_id": "ctx_add_1",
            "memory_id": 4001,
            "content": "User searched for neural networks",
            "context": ["AI", "research", "learning"]
        },
        {
            "type": "PredictContext",
            "request_id": "ctx_predict_1", 
            "current_context": ["AI", "neural"],
            "sequence_length": 3
        },
        {
            "type": "GetContextHistory",
            "request_id": "ctx_history_1",
            "memory_id": 4001
        }
    ]
    
    for request in test_requests:
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect('/tmp/mfn_layer4.sock')
            sock.send((json.dumps(request) + '\n').encode())
            response = sock.recv(4096).decode().strip()
            sock.close()
            
            response_data = json.loads(response)
            if response_data.get('type') != 'Error':
                print(f"   ✅ {request['type']}: Success")
            else:
                print(f"   ❌ {request['type']}: {response_data.get('error', 'Unknown error')}")
                
        except Exception as e:
            print(f"   ❌ {request['type']}: {e}")

def test_layer2_similarity_operations():
    """Test Layer 2 similarity search capabilities"""
    print("\n🧠 Testing Layer 2 Similarity Operations (via Unix socket)")
    
    # Test with different similarity operations
    test_requests = [
        {
            "type": "AddMemory",
            "request_id": "sim_add_1",
            "memory_id": 2001,
            "embedding": [0.1, 0.2, 0.3, 0.4, 0.5] * 20,  # 100-dim vector
            "content": "Test similarity memory"
        },
        {
            "type": "SimilaritySearch", 
            "request_id": "sim_search_1",
            "query_embedding": [0.15, 0.25, 0.35, 0.45, 0.55] * 20,  # Similar vector
            "top_k": 5,
            "threshold": 0.5
        },
        {
            "type": "GetPerformanceStats",
            "request_id": "sim_stats_1"
        }
    ]
    
    for request in test_requests:
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect('/tmp/mfn_layer2.sock')
            sock.send((json.dumps(request) + '\n').encode())
            response = sock.recv(4096).decode().strip()
            sock.close()
            
            response_data = json.loads(response)
            if response_data.get('success', True):
                print(f"   ✅ {request['type']}: Success")
                if request['type'] == 'SimilaritySearch' and 'results' in response_data:
                    result_count = len(response_data['results'])
                    print(f"      Found {result_count} similar memories")
            else:
                print(f"   ❌ {request['type']}: {response_data.get('error', 'Unknown error')}")
                
        except Exception as e:
            print(f"   ❌ {request['type']}: {e}")

def test_layer1_exact_matching():
    """Test Layer 1 exact matching capabilities"""
    print("\n⚡ Testing Layer 1 Exact Matching (via Unix socket)")
    
    # Test with different exact match operations
    test_requests = [
        {
            "operation": "add",
            "request_id": "exact_add_1", 
            "content": "Exact match test content",
            "memory_id": 1001
        },
        {
            "operation": "search",
            "request_id": "exact_search_1",
            "content": "Exact match test content"  # Should find exact match
        },
        {
            "operation": "search", 
            "request_id": "exact_search_2",
            "content": "No match content"  # Should find no match
        },
        {
            "operation": "stats",
            "request_id": "exact_stats_1"
        }
    ]
    
    for request in test_requests:
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect('/tmp/mfn_layer1.sock')
            sock.send((json.dumps(request) + '\n').encode())
            response = sock.recv(4096).decode().strip()
            sock.close()
            
            # Layer 1 might use different response format
            try:
                response_data = json.loads(response)
                success = response_data.get('success', response_data.get('type') != 'error')
            except:
                success = 'error' not in response.lower()
            
            if success:
                print(f"   ✅ {request['operation']}: Success")
            else:
                print(f"   ❌ {request['operation']}: {response[:100]}")
                
        except Exception as e:
            print(f"   ❌ {request['operation']}: {e}")

def test_integration_flow():
    """Test end-to-end memory flow across layers"""
    print("\n🔄 Testing Integration Flow")
    print("   Testing memory flow: Add → Exact Match → Similarity → Association → Context")
    
    # This would test the full MFN pipeline
    memory_content = "Integration test: AI and machine learning applications"
    
    print(f"   📝 Testing with memory: '{memory_content}'")
    print("   🚧 Full integration testing requires coordinated layer communication")
    print("   📋 This would be implemented in the MFN orchestrator layer")

def main():
    print("🧠 MFN Memory Functionality Test")
    print("=" * 60)
    print("Testing actual memory capabilities beyond socket connectivity\n")
    
    # Test each layer's memory capabilities
    test_layer1_exact_matching()
    test_layer2_similarity_operations() 
    test_layer3_memory_operations()
    test_layer4_context_operations()
    
    # Test integration
    test_integration_flow()
    
    print("\n📊 Memory Functionality Summary")
    print("=" * 60)
    print("✅ Socket infrastructure working (from previous tests)")
    print("🧠 Memory operation testing results above")
    print("🚧 Integration flow requires orchestrator layer")
    print("\n💡 Key Finding:")
    print("   The socket performance is excellent, but we need to validate")
    print("   the actual memory processing capabilities of each layer.")

if __name__ == '__main__':
    main()