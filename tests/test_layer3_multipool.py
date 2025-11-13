#!/usr/bin/env python3
"""
Test Layer 3 ALM multi-pool architecture.
Tests that different pool_ids route to different ALM instances.
"""

import socket
import json
import struct
import time

SOCKET_PATH = "/tmp/mfn_layer3.sock"

def send_request(sock, request):
    """Send a request using the binary protocol."""
    # Serialize to JSON
    request_json = json.dumps(request)
    request_bytes = request_json.encode('utf-8')

    # Send length prefix (4 bytes, little-endian)
    length = len(request_bytes)
    sock.sendall(struct.pack('<I', length))

    # Send request data
    sock.sendall(request_bytes)

    # Read length prefix of response
    length_bytes = sock.recv(4)
    if len(length_bytes) < 4:
        raise Exception("Failed to read response length")

    response_length = struct.unpack('<I', length_bytes)[0]

    # Read response data
    response_bytes = b''
    while len(response_bytes) < response_length:
        chunk = sock.recv(response_length - len(response_bytes))
        if not chunk:
            raise Exception("Connection closed while reading response")
        response_bytes += chunk

    return json.loads(response_bytes.decode('utf-8'))

def test_multi_pool():
    """Test multi-pool functionality."""
    print("Testing Layer 3 ALM Multi-Pool Architecture")
    print("=" * 60)

    # Connect to the socket server
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect(SOCKET_PATH)
    print(f"✅ Connected to {SOCKET_PATH}")

    try:
        # Test 1: Add memory to default pool (crucible_training)
        print("\n[Test 1] Adding memory to default pool (crucible_training)...")
        req1 = {
            "type": "add_memory",
            "request_id": "test_1",
            "content": "Memory in default pool",
            "metadata": {}
        }
        resp1 = send_request(sock, req1)
        print(f"Response: {resp1}")
        assert resp1['success'], "Failed to add memory to default pool"
        memory_id_default = resp1['metadata']['memory_id']
        print(f"✅ Added memory {memory_id_default} to default pool")

        # Test 2: Add memory to pool_a
        print("\n[Test 2] Adding memory to pool_a...")
        req2 = {
            "type": "add_memory",
            "request_id": "test_2",
            "pool_id": "pool_a",
            "content": "Memory in pool A",
            "metadata": {}
        }
        resp2 = send_request(sock, req2)
        print(f"Response: {resp2}")
        assert resp2['success'], "Failed to add memory to pool_a"
        memory_id_a = resp2['metadata']['memory_id']
        print(f"✅ Added memory {memory_id_a} to pool_a")

        # Test 3: Add memory to pool_b
        print("\n[Test 3] Adding memory to pool_b...")
        req3 = {
            "type": "add_memory",
            "request_id": "test_3",
            "pool_id": "pool_b",
            "content": "Memory in pool B",
            "metadata": {}
        }
        resp3 = send_request(sock, req3)
        print(f"Response: {resp3}")
        assert resp3['success'], "Failed to add memory to pool_b"
        memory_id_b = resp3['metadata']['memory_id']
        print(f"✅ Added memory {memory_id_b} to pool_b")

        # Test 4: Get stats from default pool
        print("\n[Test 4] Getting stats from default pool...")
        req4 = {
            "type": "get_stats",
            "request_id": "test_4"
        }
        resp4 = send_request(sock, req4)
        print(f"Response: {resp4}")
        assert resp4['success'], "Failed to get stats from default pool"
        print(f"Default pool memories: {resp4['metadata']['total_memories']}")
        print(f"Total pools: {resp4['metadata']['total_pools']}")
        assert resp4['metadata']['total_pools'] >= 3, "Expected at least 3 pools"

        # Test 5: Get stats from pool_a
        print("\n[Test 5] Getting stats from pool_a...")
        req5 = {
            "type": "get_stats",
            "request_id": "test_5",
            "pool_id": "pool_a"
        }
        resp5 = send_request(sock, req5)
        print(f"Response: {resp5}")
        assert resp5['success'], "Failed to get stats from pool_a"
        print(f"Pool A memories: {resp5['metadata']['total_memories']}")

        # Test 6: Get stats from pool_b
        print("\n[Test 6] Getting stats from pool_b...")
        req6 = {
            "type": "get_stats",
            "request_id": "test_6",
            "pool_id": "pool_b"
        }
        resp6 = send_request(sock, req6)
        print(f"Response: {resp6}")
        assert resp6['success'], "Failed to get stats from pool_b"
        print(f"Pool B memories: {resp6['metadata']['total_memories']}")

        # Test 7: Health check with pool_id
        print("\n[Test 7] Health check for pool_a...")
        req7 = {
            "type": "HealthCheck",
            "request_id": "test_7",
            "pool_id": "pool_a"
        }
        resp7 = send_request(sock, req7)
        print(f"Response: {resp7}")
        assert resp7['status'] == 'healthy', "Health check failed"
        assert resp7['metrics']['pool_id'] == 'pool_a', "Wrong pool in health check"
        print(f"✅ Health check passed for pool_a")

        print("\n" + "=" * 60)
        print("✅ All tests passed!")
        print(f"Total pools created: {resp4['metadata']['total_pools']}")

    except Exception as e:
        print(f"\n❌ Test failed: {e}")
        import traceback
        traceback.print_exc()
    finally:
        sock.close()
        print("\n✅ Connection closed")

if __name__ == "__main__":
    test_multi_pool()
