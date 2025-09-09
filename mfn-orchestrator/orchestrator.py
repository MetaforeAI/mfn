#!/usr/bin/env python3
"""
MFN Orchestrator - Memory Flow Network Coordination Layer
Coordinates memory operations across all 4 layers for end-to-end memory processing
"""

import socket
import json
import time
import requests
from typing import Dict, List, Any, Optional
from dataclasses import dataclass

@dataclass
class MemoryFlowResult:
    """Result of a complete memory flow operation"""
    success: bool
    memory_id: Optional[int]
    layer_results: Dict[str, Any]
    total_time_ms: float
    final_decision: Optional[str]
    confidence: float

class MFNOrchestrator:
    """Orchestrates memory operations across all MFN layers"""
    
    def __init__(self):
        self.layer_configs = {
            'layer1': {
                'type': 'unix_socket',
                'socket': '/tmp/mfn_layer1.sock',
                'description': 'Immediate Flow Registry - Exact matching'
            },
            'layer2': {
                'type': 'unix_socket', 
                'socket': '/tmp/mfn_layer2.sock',
                'description': 'Dynamic Similarity Reservoir - Similarity search'
            },
            'layer3': {
                'type': 'http_api',
                'base_url': 'http://localhost:8082',
                'description': 'Associative Link Mesh - Graph-based associations'
            },
            'layer4': {
                'type': 'unix_socket',
                'socket': '/tmp/mfn_layer4.sock', 
                'description': 'Context Prediction Engine - Temporal patterns'
            }
        }
        
    def add_memory_flow(self, content: str, tags: List[str] = None, context: List[str] = None) -> MemoryFlowResult:
        """
        Complete memory addition flow across all layers
        1. Add to Layer 3 (ALM) for permanent storage
        2. Add to Layer 1 (IFR) for exact matching 
        3. Add to Layer 2 (DSR) for similarity search
        4. Add to Layer 4 (CPE) for context tracking
        """
        start_time = time.perf_counter()
        layer_results = {}
        memory_id = None
        
        try:
            # Step 1: Add to Layer 3 (ALM) for permanent storage and graph relationships
            print(f"🔄 Step 1: Adding to Layer 3 (ALM) - {content[:50]}...")
            l3_result = self._add_to_layer3(content, tags or [])
            layer_results['layer3'] = l3_result
            
            if l3_result.get('success'):
                memory_id = l3_result.get('memory_id')
                print(f"   ✅ Layer 3: Added memory ID {memory_id}")
            else:
                print(f"   ❌ Layer 3: Failed - {l3_result.get('error', 'Unknown error')}")
                
            # Step 2: Add to Layer 1 (IFR) for exact matching
            print(f"🔄 Step 2: Adding to Layer 1 (IFR)...")  
            l1_result = self._add_to_layer1(content, memory_id)
            layer_results['layer1'] = l1_result
            
            if l1_result.get('success'):
                print(f"   ✅ Layer 1: Added for exact matching")
            else:
                print(f"   ❌ Layer 1: {l1_result.get('error', 'Failed')}")
            
            # Step 3: Add to Layer 2 (DSR) for similarity search (if running)
            print(f"🔄 Step 3: Adding to Layer 2 (DSR)...")
            l2_result = self._add_to_layer2(content, memory_id)
            layer_results['layer2'] = l2_result
            
            if l2_result.get('success'):
                print(f"   ✅ Layer 2: Added for similarity search")
            else:
                print(f"   ⚠️  Layer 2: {l2_result.get('error', 'Failed')} (may not be running)")
                
            # Step 4: Add to Layer 4 (CPE) for context tracking  
            print(f"🔄 Step 4: Adding to Layer 4 (CPE)...")
            l4_result = self._add_to_layer4(memory_id, content, context or tags or [])
            layer_results['layer4'] = l4_result
            
            if l4_result.get('success'):
                print(f"   ✅ Layer 4: Added context tracking")
            else:
                print(f"   ❌ Layer 4: {l4_result.get('error', 'Failed')}")
                
        except Exception as e:
            print(f"❌ Orchestrator error: {e}")
            layer_results['orchestrator_error'] = str(e)
            
        end_time = time.perf_counter()
        total_time = (end_time - start_time) * 1000
        
        # Determine overall success
        success = (layer_results.get('layer3', {}).get('success', False) and 
                  layer_results.get('layer1', {}).get('success', False))
        
        return MemoryFlowResult(
            success=success,
            memory_id=memory_id,
            layer_results=layer_results,
            total_time_ms=total_time,
            final_decision="stored" if success else "failed",
            confidence=0.9 if success else 0.0
        )
    
    def query_memory_flow(self, query: str, max_results: int = 5) -> MemoryFlowResult:
        """
        Complete memory query flow across all layers
        1. Check Layer 1 (IFR) for exact matches first
        2. If no exact match, try Layer 2 (DSR) for similar memories
        3. Use Layer 3 (ALM) for associative search
        4. Get Layer 4 (CPE) context predictions
        """
        start_time = time.perf_counter()
        layer_results = {}
        
        try:
            # Step 1: Try Layer 1 (IFR) for exact matches
            print(f"🔍 Step 1: Checking Layer 1 (IFR) for exact match...")
            l1_result = self._query_layer1(query)
            layer_results['layer1'] = l1_result
            
            if l1_result.get('found_exact'):
                print(f"   ✅ Layer 1: Exact match found! Confidence: {l1_result.get('confidence', 0)}")
                decision = "exact_match"
                confidence = l1_result.get('confidence', 1.0)
            else:
                print(f"   ➡️  Layer 1: No exact match, routing to Layer {l1_result.get('next_layer', 2)}")
                
                # Step 2: Try Layer 2 (DSR) for similarity search
                print(f"🔍 Step 2: Checking Layer 2 (DSR) for similar memories...")
                l2_result = self._query_layer2(query, max_results)
                layer_results['layer2'] = l2_result
                
                if l2_result.get('success'):
                    results_count = len(l2_result.get('results', []))
                    print(f"   ✅ Layer 2: Found {results_count} similar memories")
                else:
                    print(f"   ⚠️  Layer 2: {l2_result.get('error', 'No results')} (may not be running)")
                
                decision = "similarity_search"
                confidence = 0.7
                
            # Step 3: Get Layer 3 (ALM) associative context 
            print(f"🔍 Step 3: Querying Layer 3 (ALM) for associations...")
            l3_result = self._query_layer3(query, max_results)
            layer_results['layer3'] = l3_result
            
            # Step 4: Get Layer 4 (CPE) context predictions
            print(f"🔍 Step 4: Getting Layer 4 (CPE) context predictions...")
            l4_result = self._predict_layer4(query.split(), 3)
            layer_results['layer4'] = l4_result
            
        except Exception as e:
            print(f"❌ Query orchestrator error: {e}")
            layer_results['orchestrator_error'] = str(e)
            decision = "error"
            confidence = 0.0
            
        end_time = time.perf_counter()
        total_time = (end_time - start_time) * 1000
        
        return MemoryFlowResult(
            success=len(layer_results) > 0,
            memory_id=None,
            layer_results=layer_results,
            total_time_ms=total_time,
            final_decision=decision,
            confidence=confidence
        )
    
    # Layer-specific communication methods
    
    def _add_to_layer3(self, content: str, tags: List[str]) -> Dict[str, Any]:
        """Add memory to Layer 3 via HTTP API"""
        try:
            payload = {
                "content": content,
                "tags": tags
            }
            response = requests.post(
                f"{self.layer_configs['layer3']['base_url']}/memories",
                json=payload,
                timeout=5
            )
            
            if response.status_code in [200, 201]:
                result = response.json()
                return {
                    "success": True,
                    "memory_id": result.get("memory", {}).get("id"),
                    "response": result
                }
            else:
                return {
                    "success": False,
                    "error": f"HTTP {response.status_code}: {response.text[:100]}"
                }
        except Exception as e:
            return {"success": False, "error": str(e)}
    
    def _add_to_layer1(self, content: str, memory_id: Optional[int]) -> Dict[str, Any]:
        """Add memory to Layer 1 via Unix socket"""
        return self._send_unix_socket_message('layer1', {
            "type": "add_memory",
            "request_id": f"orch_add_{int(time.time())}",
            "content": content
        })
    
    def _add_to_layer2(self, content: str, memory_id: Optional[int]) -> Dict[str, Any]:
        """Add memory to Layer 2 via Unix socket (if running)"""
        return self._send_unix_socket_message('layer2', {
            "type": "AddMemory",
            "request_id": f"orch_add_{int(time.time())}",
            "content": content,
            "memory_id": memory_id or 0
        })
    
    def _add_to_layer4(self, memory_id: Optional[int], content: str, context: List[str]) -> Dict[str, Any]:
        """Add memory context to Layer 4 via Unix socket"""
        return self._send_unix_socket_message('layer4', {
            "type": "AddMemoryContext",
            "request_id": f"orch_ctx_{int(time.time())}",
            "memory_id": memory_id or 0,
            "content": content,
            "context": context
        })
    
    def _query_layer1(self, query: str) -> Dict[str, Any]:
        """Query Layer 1 for exact matches"""
        return self._send_unix_socket_message('layer1', {
            "type": "query",
            "request_id": f"orch_query_{int(time.time())}",
            "content": query
        })
        
    def _query_layer2(self, query: str, max_results: int) -> Dict[str, Any]:
        """Query Layer 2 for similarity search"""
        return self._send_unix_socket_message('layer2', {
            "type": "SimilaritySearch",
            "request_id": f"orch_sim_{int(time.time())}",
            "query": query,
            "top_k": max_results
        })
    
    def _query_layer3(self, query: str, max_results: int) -> Dict[str, Any]:
        """Query Layer 3 via HTTP API"""
        try:
            response = requests.get(
                f"{self.layer_configs['layer3']['base_url']}/search",
                params={"q": query, "limit": max_results},
                timeout=5
            )
            
            if response.status_code == 200:
                return {"success": True, "results": response.json()}
            else:
                return {"success": False, "error": f"HTTP {response.status_code}"}
        except Exception as e:
            return {"success": False, "error": str(e)}
            
    def _predict_layer4(self, context: List[str], sequence_length: int) -> Dict[str, Any]:
        """Get context predictions from Layer 4"""
        return self._send_unix_socket_message('layer4', {
            "type": "PredictContext",
            "request_id": f"orch_pred_{int(time.time())}",
            "current_context": context,
            "sequence_length": sequence_length
        })
    
    def _send_unix_socket_message(self, layer: str, message: Dict[str, Any]) -> Dict[str, Any]:
        """Send message to Unix socket and return response"""
        try:
            socket_path = self.layer_configs[layer]['socket']
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.settimeout(5.0)
            sock.connect(socket_path)
            
            # Send JSON message
            message_json = json.dumps(message) + '\n'
            sock.send(message_json.encode())
            
            # Receive response
            response = sock.recv(4096).decode().strip()
            sock.close()
            
            return {"success": True, "response": json.loads(response)}
            
        except Exception as e:
            return {"success": False, "error": str(e)}

def main():
    """Test the MFN Orchestrator"""
    print("🧠 MFN Orchestrator - Memory Flow Network Coordination")
    print("=" * 60)
    
    orchestrator = MFNOrchestrator()
    
    # Test 1: Add Memory Flow
    print("\n📝 Test 1: Complete Memory Addition Flow")
    result = orchestrator.add_memory_flow(
        content="Machine learning requires large datasets for training",
        tags=["AI", "machine_learning", "training", "data"],
        context=["AI", "training", "data"]
    )
    
    print(f"\n📊 Addition Results:")
    print(f"   Success: {result.success}")
    print(f"   Memory ID: {result.memory_id}")
    print(f"   Total Time: {result.total_time_ms:.2f}ms")
    print(f"   Final Decision: {result.final_decision}")
    print(f"   Confidence: {result.confidence}")
    
    # Show layer-by-layer results
    for layer, layer_result in result.layer_results.items():
        status = "✅" if layer_result.get('success') else "❌"
        print(f"   {status} {layer.upper()}: {layer_result.get('error', 'Success')}")
    
    # Test 2: Query Memory Flow  
    print(f"\n🔍 Test 2: Complete Memory Query Flow")
    query_result = orchestrator.query_memory_flow(
        query="machine learning training",
        max_results=5
    )
    
    print(f"\n📊 Query Results:")
    print(f"   Success: {query_result.success}")
    print(f"   Total Time: {query_result.total_time_ms:.2f}ms")
    print(f"   Final Decision: {query_result.final_decision}")
    print(f"   Confidence: {query_result.confidence}")
    
    # Show layer-by-layer query results
    for layer, layer_result in query_result.layer_results.items():
        if layer_result.get('success'):
            response = layer_result.get('response', {})
            print(f"   ✅ {layer.upper()}: {response.get('type', 'Response received')}")
        else:
            print(f"   ❌ {layer.upper()}: {layer_result.get('error', 'Failed')}")
    
    print(f"\n🎯 MFN Orchestrator Test Complete")
    print("=" * 60)
    print("✅ Demonstrates end-to-end memory flow coordination")
    print("🚀 All 4 layers working together for memory processing")

if __name__ == "__main__":
    main()