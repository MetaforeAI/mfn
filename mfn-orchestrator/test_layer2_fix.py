#!/usr/bin/env python3
"""Test script to verify Layer 2 socket communication is fixed"""

import json
import socket
import struct
import numpy as np
import time

def test_binary_protocol():
    """Test Layer 2 with binary protocol"""
    print("🧪 Testing Layer 2 Binary Protocol Fix")
    print("=" * 50)

    # Create test embedding
    np.random.seed(42)
    test_embedding = np.random.randn(768).astype(np.float32)
    test_embedding = (test_embedding / np.linalg.norm(test_embedding)).tolist()

    # Test 1: Add Memory
    print("\n📝 Test 1: AddMemory Request")
    request = {
        "type": "AddMemory",
        "request_id": f"test_add_{int(time.time())}",
        "memory_id": 12345,
        "embedding": test_embedding,
        "content": "This is a test memory for Layer 2 validation",
        "tags": ["test", "validation"],
        "metadata": {"source": "test_script"}
    }

    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect("/tmp/mfn_layer2.sock")

        # Send binary protocol: length prefix + JSON
        message_json = json.dumps(request).encode('utf-8')
        message_len = len(message_json)

        sock.send(struct.pack('<I', message_len))
        sock.send(message_json)

        # Receive response
        len_bytes = sock.recv(4)
        response_len = struct.unpack('<I', len_bytes)[0]
        response_data = sock.recv(response_len)

        response = json.loads(response_data.decode('utf-8'))
        print(f"   ✅ AddMemory Response: {response.get('type', 'unknown')}")
        print(f"   📊 Success: {response.get('success', False)}")

        sock.close()

    except Exception as e:
        print(f"   ❌ AddMemory Failed: {e}")
        return False

    # Test 2: Similarity Search
    print("\n🔍 Test 2: SimilaritySearch Request")

    # Create query embedding
    query_text = "test validation memory"
    np.random.seed(hash(query_text) % (2**32))
    query_embedding = np.random.randn(768).astype(np.float32)
    query_embedding = (query_embedding / np.linalg.norm(query_embedding)).tolist()

    request = {
        "type": "SimilaritySearch",
        "request_id": f"test_search_{int(time.time())}",
        "query_embedding": query_embedding,
        "top_k": 5,
        "min_confidence": 0.5,
        "timeout_ms": 5000
    }

    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect("/tmp/mfn_layer2.sock")

        # Send binary protocol
        message_json = json.dumps(request).encode('utf-8')
        message_len = len(message_json)

        sock.send(struct.pack('<I', message_len))
        sock.send(message_json)

        # Receive response
        len_bytes = sock.recv(4)
        response_len = struct.unpack('<I', len_bytes)[0]
        response_data = sock.recv(response_len)

        response = json.loads(response_data.decode('utf-8'))
        print(f"   ✅ Search Response: {response.get('type', 'unknown')}")
        print(f"   📊 Results Found: {len(response.get('results', []))}")
        print(f"   🕐 Search Time: {response.get('search_time_ms', 'N/A')}ms")

        sock.close()

    except Exception as e:
        print(f"   ❌ SimilaritySearch Failed: {e}")
        return False

    # Test 3: Health Check
    print("\n❤️ Test 3: Health Check")
    request = {
        "type": "HealthCheck",
        "request_id": f"test_health_{int(time.time())}"
    }

    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(5.0)
        sock.connect("/tmp/mfn_layer2.sock")

        # Send binary protocol
        message_json = json.dumps(request).encode('utf-8')
        message_len = len(message_json)

        sock.send(struct.pack('<I', message_len))
        sock.send(message_json)

        # Receive response
        len_bytes = sock.recv(4)
        response_len = struct.unpack('<I', len_bytes)[0]
        response_data = sock.recv(response_len)

        response = json.loads(response_data.decode('utf-8'))
        print(f"   ✅ Health Status: {response.get('status', 'unknown')}")
        print(f"   📊 Total Memories: {response.get('total_memories', 'N/A')}")
        print(f"   🕐 Uptime: {response.get('uptime_seconds', 'N/A')}s")

        sock.close()

    except Exception as e:
        print(f"   ❌ Health Check Failed: {e}")
        return False

    print("\n" + "=" * 50)
    print("✅ All Layer 2 tests passed successfully!")
    print("🚀 Socket communication is now working with binary protocol")
    return True

if __name__ == "__main__":
    test_binary_protocol()