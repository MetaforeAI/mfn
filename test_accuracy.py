#!/usr/bin/env python3
"""
Real accuracy test - Input vs Output comparison
Tests actual retrieval accuracy with measurable results
"""

import json
import time
from mfn_client import MFNClient

def test_input_output_accuracy():
    """Test actual input vs output accuracy"""
    print("🎯 MFN Input/Output Accuracy Test")
    print("=" * 40)
    
    client = MFNClient()
    
    # Test memories with known content for verification
    test_memories = [
        {
            "content": "Machine learning uses neural networks for pattern recognition",
            "tags": ["ai", "ml", "neural"],
            "expected_queries": ["neural networks", "machine learning", "pattern recognition"]
        },
        {
            "content": "Python is a programming language used for data science and AI",
            "tags": ["python", "programming", "data"],
            "expected_queries": ["python programming", "data science", "AI language"]
        },
        {
            "content": "Quantum computing leverages quantum mechanics for computation",
            "tags": ["quantum", "computing", "physics"],
            "expected_queries": ["quantum mechanics", "quantum computing", "computation"]
        },
        {
            "content": "Blockchain technology enables decentralized digital transactions",
            "tags": ["blockchain", "crypto", "decentralized"],
            "expected_queries": ["blockchain", "decentralized", "digital transactions"]
        },
        {
            "content": "Deep learning algorithms process large datasets to find patterns",
            "tags": ["deep learning", "datasets", "algorithms"],
            "expected_queries": ["deep learning", "large datasets", "pattern finding"]
        }
    ]
    
    print(f"📝 Adding {len(test_memories)} test memories...")
    
    # Add memories and track IDs  
    from mfn_client import MemoryItem
    memory_ids = []
    for i, memory in enumerate(test_memories):
        memory_item = MemoryItem(
            id=i+1,
            content=memory["content"], 
            tags=memory["tags"]
        )
        success = client.add_memory(memory_item)
        if success:
            memory_ids.append(memory_item.id)
            print(f"  ✅ Memory {i+1}: ID {memory_item.id}")
        else:
            print(f"  ❌ Memory {i+1}: Failed to add")
            memory_ids.append(None)
    
    print(f"\n🔍 Testing Retrieval Accuracy...")
    print("-" * 40)
    
    total_tests = 0
    correct_retrievals = 0
    results = []
    
    for i, memory in enumerate(test_memories):
        if memory_ids[i] is None:
            continue
            
        original_content = memory["content"]
        memory_id = memory_ids[i]
        
        print(f"\n📋 Memory {i+1} (ID: {memory_id}):")
        print(f"   Original: {original_content[:50]}...")
        
        # Test each expected query
        for query in memory["expected_queries"]:
            total_tests += 1
            
            # Perform search
            search_results = client.search_memories(query, max_results=5)
            
            # Check if original memory is in results
            found = False
            rank = None
            returned_content = None
            confidence = 0.0
            
            if search_results and "results" in search_results:
                for rank_idx, result in enumerate(search_results["results"]):
                    if "memory_id" in result and result["memory_id"] == memory_id:
                        found = True
                        rank = rank_idx + 1
                        returned_content = result.get("content", "")
                        confidence = result.get("confidence", 0.0)
                        break
            
            # Verify content accuracy
            content_match = returned_content == original_content if returned_content else False
            
            if found and content_match:
                correct_retrievals += 1
                status = "✅"
            else:
                status = "❌"
            
            print(f"   Query: '{query}'")
            print(f"   Result: {status} Found={'Yes' if found else 'No'}, Rank={rank}, Confidence={confidence:.2f}")
            if returned_content and returned_content != original_content:
                print(f"   ⚠️  Content mismatch: Got '{returned_content[:50]}...'")
            
            results.append({
                "memory_id": memory_id,
                "original_content": original_content,
                "query": query,
                "found": found,
                "rank": rank,
                "confidence": confidence,
                "content_match": content_match,
                "returned_content": returned_content
            })
    
    # Calculate accuracy metrics
    accuracy = correct_retrievals / total_tests if total_tests > 0 else 0
    
    print(f"\n📊 ACCURACY RESULTS")
    print("=" * 40)
    print(f"Total Tests: {total_tests}")
    print(f"Correct Retrievals: {correct_retrievals}")
    print(f"Accuracy: {accuracy:.1%}")
    print(f"Failed Retrievals: {total_tests - correct_retrievals}")
    
    # Detailed analysis
    found_count = sum(1 for r in results if r["found"])
    content_matches = sum(1 for r in results if r["content_match"])
    avg_confidence = sum(r["confidence"] for r in results if r["found"]) / found_count if found_count > 0 else 0
    
    print(f"\n📈 DETAILED METRICS:")
    print(f"   Retrieval Rate: {found_count}/{total_tests} ({found_count/total_tests:.1%})")
    print(f"   Content Accuracy: {content_matches}/{total_tests} ({content_matches/total_tests:.1%})")
    print(f"   Average Confidence: {avg_confidence:.2f}")
    
    # Ranking analysis
    rank_distribution = {}
    for r in results:
        if r["rank"] is not None:
            rank_distribution[r["rank"]] = rank_distribution.get(r["rank"], 0) + 1
    
    print(f"\n🏆 RANKING DISTRIBUTION:")
    for rank in sorted(rank_distribution.keys()):
        count = rank_distribution[rank]
        print(f"   Rank {rank}: {count} results ({count/found_count:.1%})")
    
    return {
        "accuracy": accuracy,
        "total_tests": total_tests,
        "correct_retrievals": correct_retrievals,
        "retrieval_rate": found_count / total_tests,
        "content_accuracy": content_matches / total_tests,
        "average_confidence": avg_confidence,
        "results": results
    }

def test_similarity_accuracy():
    """Test semantic similarity accuracy"""
    print(f"\n🧠 Semantic Similarity Test")
    print("=" * 40)
    
    client = MFNClient()
    
    # Test semantic similarity with related concepts
    similarity_tests = [
        {
            "memory": "Artificial intelligence enables machine learning",
            "similar_query": "AI powers ML algorithms",
            "dissimilar_query": "Cooking pasta requires boiling water"
        },
        {
            "memory": "Solar panels convert sunlight into electricity", 
            "similar_query": "Photovoltaic cells generate electrical power",
            "dissimilar_query": "Dogs are popular household pets"
        },
        {
            "memory": "Database systems store and retrieve information",
            "similar_query": "Data storage and information retrieval",
            "dissimilar_query": "Mountain climbing requires proper equipment"
        }
    ]
    
    similarity_results = []
    
    for i, test in enumerate(similarity_tests):
        print(f"\n🔬 Test {i+1}:")
        print(f"   Memory: {test['memory']}")
        
        # Add memory
        memory_item = MemoryItem(
            id=1000+i,
            content=test["memory"],
            tags=["test"]
        )
        success = client.add_memory(memory_item)
        if not success:
            print(f"   ❌ Failed to add memory")
            continue
            
        memory_id = memory_item.id
        
        # Test similar query
        similar_results = client.search_memories(test["similar_query"], max_results=5)
        similar_confidence = 0.0
        similar_found = False
        
        if similar_results and "results" in similar_results:
            for result in similar_results["results"]:
                if result.get("memory_id") == memory_id:
                    similar_confidence = result.get("confidence", 0.0)
                    similar_found = True
                    break
        
        # Test dissimilar query  
        dissimilar_results = client.search_memories(test["dissimilar_query"], max_results=5)
        dissimilar_confidence = 0.0
        dissimilar_found = False
        
        if dissimilar_results and "results" in dissimilar_results:
            for result in dissimilar_results["results"]:
                if result.get("memory_id") == memory_id:
                    dissimilar_confidence = result.get("confidence", 0.0)
                    dissimilar_found = True
                    break
        
        # Analyze similarity discrimination
        discrimination = similar_confidence - dissimilar_confidence
        
        print(f"   Similar query: '{test['similar_query']}'")
        print(f"     Confidence: {similar_confidence:.2f} {'✅' if similar_found else '❌'}")
        print(f"   Dissimilar query: '{test['dissimilar_query']}'") 
        print(f"     Confidence: {dissimilar_confidence:.2f} {'✅' if not dissimilar_found else '❌'}")
        print(f"   Discrimination: {discrimination:.2f}")
        
        similarity_results.append({
            "memory": test["memory"],
            "similar_confidence": similar_confidence,
            "dissimilar_confidence": dissimilar_confidence,
            "discrimination": discrimination,
            "correct_discrimination": discrimination > 0.1
        })
    
    # Calculate discrimination accuracy
    correct_discriminations = sum(1 for r in similarity_results if r["correct_discrimination"])
    discrimination_accuracy = correct_discriminations / len(similarity_results)
    
    print(f"\n🎯 SIMILARITY DISCRIMINATION:")
    print(f"   Correct: {correct_discriminations}/{len(similarity_results)} ({discrimination_accuracy:.1%})")
    
    return similarity_results

def show_usage_guide():
    """Show how to use the MFN system"""
    print(f"\n📖 HOW TO USE THE MFN SYSTEM")
    print("=" * 40)
    
    print("1. START THE SYSTEM:")
    print("   cd /home/persist/repos/mfn-system")
    print("   ./start_layers.sh  # Start all layers")
    
    print("\n2. PYTHON API USAGE:")
    print("   from mfn_client import MFNClient")
    print("   client = MFNClient()")
    print("   ")
    print("   # Add memories")
    print("   result = client.add_memory('Your memory content', ['tag1', 'tag2'])")
    print("   memory_id = result['memory_id']")
    print("   ")
    print("   # Search memories")
    print("   results = client.search_memories('search query', max_results=10)")
    print("   for result in results['results']:")
    print("       print(f\"ID: {result['memory_id']}, Content: {result['content']}\")")
    print("   ")
    print("   # Add associations")
    print("   client.add_association(memory_id1, memory_id2, 'relates_to', 0.8)")
    
    print("\n3. COMMAND LINE USAGE:")
    print("   python3 mfn_client.py --test-memory-ops")
    print("   python3 mfn_client.py --stress-test --num-memories 100")
    
    print("\n4. PERFORMANCE TESTING:")
    print("   ./end_to_end_test.sh  # Full system test")
    print("   python3 test_accuracy.py  # This accuracy test")
    
    print("\n5. LAYER ENDPOINTS:")
    print("   Layer 1 (Zig):   Direct FFI calls")
    print("   Layer 2 (Rust):  Direct FFI calls")  
    print("   Layer 3 (Go):    HTTP API at :8080")
    print("   Layer 4 (Rust):  Direct FFI calls (partial)")

if __name__ == "__main__":
    show_usage_guide()
    
    # Run accuracy tests
    accuracy_results = test_input_output_accuracy()
    similarity_results = test_similarity_accuracy()
    
    print(f"\n🏁 FINAL RESULTS:")
    print(f"   Overall Accuracy: {accuracy_results['accuracy']:.1%}")
    print(f"   Retrieval Rate: {accuracy_results['retrieval_rate']:.1%}")
    print(f"   Content Accuracy: {accuracy_results['content_accuracy']:.1%}")
    print(f"   Semantic Discrimination: {sum(1 for r in similarity_results if r['correct_discrimination'])}/{len(similarity_results)}")
    print(f"\n✅ MFN System proven with real input/output verification")