#!/usr/bin/env python3
"""
MFN System Unified Client
Provides a simple Python interface to interact with the Memory Flow Network
"""

import json
import requests
import time
import random
import asyncio
from typing import List, Dict, Any, Optional
from dataclasses import dataclass
from concurrent.futures import ThreadPoolExecutor, as_completed

@dataclass
class MemoryItem:
    id: int
    content: str
    tags: List[str] = None
    metadata: Dict[str, str] = None
    
    def __post_init__(self):
        if self.tags is None:
            self.tags = []
        if self.metadata is None:
            self.metadata = {}

@dataclass
class SearchResult:
    memory_id: int
    content: str
    confidence: float
    path: List[Dict] = None
    
    def __post_init__(self):
        if self.path is None:
            self.path = []

class MFNClient:
    """Unified client for Memory Flow Network operations"""
    
    def __init__(self, layer3_url: str = "http://localhost:8082"):
        self.layer3_url = layer3_url
        self.session = requests.Session()
        self.session.timeout = 30
        
    def add_memory(self, memory: MemoryItem) -> bool:
        """Add a memory to the MFN system"""
        try:
            payload = {
                "id": memory.id,
                "content": memory.content,
                "tags": memory.tags,
                "metadata": memory.metadata
            }
            
            response = self.session.post(
                f"{self.layer3_url}/memories",
                json=payload,
                headers={"Content-Type": "application/json"}
            )
            
            return response.status_code == 200
            
        except Exception as e:
            print(f"Error adding memory {memory.id}: {e}")
            return False
    
    def search_memories(self, query: str, max_results: int = 10, search_mode: str = "depth_first") -> List[SearchResult]:
        """Search for memories in the MFN system using content-based associative search"""
        try:
            # Step 1: Find starting memories by content similarity
            start_memory_ids = self._find_relevant_memory_ids(query, max_starting_points=3)
            
            if not start_memory_ids:
                print(f"No relevant starting memories found for query: '{query}'")
                return []
            
            # Step 2: Perform associative search from those starting points
            payload = {
                "start_memory_ids": start_memory_ids,
                "max_results": max_results,
                "max_depth": 2,
                "min_weight": 0.1,
                "search_mode": search_mode
            }
            
            response = self.session.post(
                f"{self.layer3_url}/search",
                json=payload,
                headers={"Content-Type": "application/json"}
            )
            
            if response.status_code == 200:
                data = response.json()
                results = []
                
                for result in data.get("results", []):
                    memory = result.get("memory", {})
                    results.append(SearchResult(
                        memory_id=memory.get("id", 0),
                        content=memory.get("content", ""),
                        confidence=result.get("total_weight", 0.0),
                        path=result.get("path", [])
                    ))
                
                return results
            else:
                print(f"Search failed with status {response.status_code}: {response.text}")
                return []
                
        except Exception as e:
            print(f"Error searching memories: {e}")
            return []
    
    def _find_relevant_memory_ids(self, query: str, max_starting_points: int = 3) -> List[int]:
        """Find memory IDs that are relevant to the search query based on content similarity"""
        try:
            # Get all memories (in production, this would be optimized with an index)
            response = self.session.get(f"{self.layer3_url}/memories")
            
            if response.status_code != 200:
                return []
            
            data = response.json()
            memories = data.get("memories", [])
            
            # Simple content similarity scoring based on keyword overlap
            query_words = set(query.lower().split())
            scored_memories = []
            
            for memory in memories:
                content = memory.get("content", "").lower()
                content_words = set(content.split())
                
                # Calculate Jaccard similarity
                intersection = query_words.intersection(content_words)
                union = query_words.union(content_words)
                similarity = len(intersection) / len(union) if union else 0
                
                # Boost memories with matching tags
                tags = memory.get("tags", [])
                tag_boost = 0
                for tag in tags:
                    if tag.lower() in query.lower():
                        tag_boost += 0.2
                
                final_score = similarity + tag_boost
                
                if final_score > 0:
                    scored_memories.append((memory.get("id"), final_score))
            
            # Sort by score and return top memory IDs
            scored_memories.sort(key=lambda x: x[1], reverse=True)
            return [mem_id for mem_id, _ in scored_memories[:max_starting_points]]
            
        except Exception as e:
            print(f"Error finding relevant memories: {e}")
            return []
    
    def get_memory(self, memory_id: int) -> Optional[MemoryItem]:
        """Get a specific memory by ID"""
        try:
            response = self.session.get(f"{self.layer3_url}/memories/{memory_id}")
            
            if response.status_code == 200:
                data = response.json()
                return MemoryItem(
                    id=data.get("id"),
                    content=data.get("content", ""),
                    tags=data.get("tags", []),
                    metadata=data.get("metadata", {})
                )
            
            return None
            
        except Exception as e:
            print(f"Error getting memory {memory_id}: {e}")
            return None
    
    def list_memories(self) -> List[MemoryItem]:
        """List all memories (use with caution on large datasets)"""
        try:
            response = self.session.get(f"{self.layer3_url}/memories")
            
            if response.status_code == 200:
                data = response.json()
                memories = []
                
                for mem_data in data.get("memories", []):
                    memories.append(MemoryItem(
                        id=mem_data.get("id"),
                        content=mem_data.get("content", ""),
                        tags=mem_data.get("tags", []),
                        metadata=mem_data.get("metadata", {})
                    ))
                
                return memories
            
            return []
            
        except Exception as e:
            print(f"Error listing memories: {e}")
            return []
    
    def get_system_stats(self) -> Dict[str, Any]:
        """Get system performance statistics"""
        try:
            response = self.session.get(f"{self.layer3_url}/performance")
            
            if response.status_code == 200:
                return response.json()
            
            return {}
            
        except Exception as e:
            print(f"Error getting system stats: {e}")
            return {}
    
    def health_check(self) -> bool:
        """Check if the MFN system is healthy"""
        try:
            response = self.session.get(f"{self.layer3_url}/health")
            return response.status_code == 200
        except:
            return False

class MFNStressTester:
    """Stress testing framework for MFN system"""
    
    def __init__(self, client: MFNClient):
        self.client = client
        self.results = {
            "add_operations": [],
            "search_operations": [],
            "errors": [],
            "total_time": 0
        }
    
    def generate_test_memories(self, count: int) -> List[MemoryItem]:
        """Generate test memories with varied content"""
        
        # Template texts for testing - various domains
        templates = [
            "The quantum mechanics principle states that {concept} influences {property} through {mechanism}",
            "In machine learning, {algorithm} optimizes {objective} using {technique} methodology", 
            "The neural network architecture {model} processes {input} to generate {output}",
            "Economic theory suggests that {factor} impacts {market} via {channel}",
            "Historical analysis reveals {event} caused {consequence} through {process}",
            "Scientific research indicates {phenomenon} correlates with {variable} in {context}",
            "Software engineering practices recommend {pattern} for {problem} using {tool}",
            "Biological systems demonstrate {behavior} when {condition} triggers {response}",
            "Mathematical models predict {outcome} based on {parameter} within {domain}",
            "Psychological studies show {stimulus} affects {cognition} through {pathway}"
        ]
        
        concepts = ["superposition", "entanglement", "interference", "coherence", "decoherence"]
        algorithms = ["gradient descent", "backpropagation", "attention mechanism", "transformer", "CNN"]
        models = ["GPT", "BERT", "ResNet", "LSTM", "VAE"]
        factors = ["inflation", "interest rates", "supply chain", "demand fluctuation", "market sentiment"]
        events = ["industrial revolution", "digital transformation", "globalization", "automation", "AI revolution"]
        
        word_pools = {
            "concept": concepts, "algorithm": algorithms, "model": models, 
            "factor": factors, "event": events,
            "property": ["energy", "momentum", "frequency", "amplitude", "phase"],
            "mechanism": ["tunneling", "resonance", "feedback", "coupling", "interaction"],
            "objective": ["loss function", "accuracy", "precision", "recall", "F1-score"],
            "technique": ["supervised", "unsupervised", "reinforcement", "semi-supervised", "self-supervised"],
            "input": ["text", "image", "audio", "sensor data", "time series"],
            "output": ["classification", "prediction", "generation", "translation", "summarization"],
            "market": ["stock market", "commodity market", "real estate", "cryptocurrency", "forex"],
            "channel": ["credit expansion", "monetary policy", "fiscal policy", "trade relations", "technology"],
            "consequence": ["economic growth", "social change", "technological advancement", "cultural shift", "paradigm shift"],
            "process": ["innovation", "adaptation", "disruption", "evolution", "transformation"],
            "phenomenon": ["learning", "memory formation", "pattern recognition", "decision making", "creativity"],
            "variable": ["age", "experience", "training", "environment", "genetics"],
            "context": ["educational settings", "workplace", "clinical trials", "laboratory", "real-world"],
            "pattern": ["singleton", "observer", "factory", "strategy", "adapter"],
            "problem": ["concurrency", "scalability", "maintainability", "performance", "security"],
            "tool": ["frameworks", "libraries", "design patterns", "testing tools", "monitoring"],
            "behavior": ["adaptation", "cooperation", "competition", "symbiosis", "migration"],
            "condition": ["stress", "resource scarcity", "environmental change", "social pressure", "genetic mutation"],
            "response": ["fight or flight", "homeostasis", "reproduction", "growth", "defense"],
            "outcome": ["optimization", "convergence", "stability", "chaos", "equilibrium"],
            "parameter": ["temperature", "pressure", "concentration", "frequency", "amplitude"],
            "domain": ["physics", "chemistry", "biology", "economics", "psychology"],
            "stimulus": ["reward", "punishment", "novelty", "threat", "social cue"],
            "cognition": ["attention", "memory", "reasoning", "perception", "learning"],
            "pathway": ["neural circuits", "chemical signals", "behavioral conditioning", "cognitive processing", "emotional regulation"]
        }
        
        memories = []
        for i in range(count):
            template = random.choice(templates)
            
            # Fill template with appropriate words
            filled_content = template
            for placeholder, words in word_pools.items():
                if f"{{{placeholder}}}" in filled_content:
                    filled_content = filled_content.replace(f"{{{placeholder}}}", random.choice(words))
            
            # Generate tags based on content
            tags = []
            content_lower = filled_content.lower()
            if "quantum" in content_lower or "neural" in content_lower:
                tags.append("science")
            if "algorithm" in content_lower or "model" in content_lower:
                tags.append("technology") 
            if "economic" in content_lower or "market" in content_lower:
                tags.append("economics")
            if "biological" in content_lower or "psychological" in content_lower:
                tags.append("life-sciences")
            
            memories.append(MemoryItem(
                id=i + 1000,  # Start from 1000 to avoid conflicts
                content=filled_content,
                tags=tags,
                metadata={"generated": "true", "test_batch": str(time.time())}
            ))
        
        return memories
    
    def run_add_stress_test(self, memories: List[MemoryItem], parallel_threads: int = 10) -> Dict[str, Any]:
        """Stress test memory addition with parallel operations"""
        print(f"🔥 Running ADD stress test: {len(memories)} memories, {parallel_threads} threads")
        
        start_time = time.time()
        success_count = 0
        error_count = 0
        
        def add_memory_batch(memory_batch):
            batch_results = []
            for memory in memory_batch:
                operation_start = time.time()
                success = self.client.add_memory(memory)
                operation_time = time.time() - operation_start
                
                batch_results.append({
                    "memory_id": memory.id,
                    "success": success,
                    "duration_ms": operation_time * 1000
                })
            return batch_results
        
        # Split memories into batches for parallel processing
        batch_size = max(1, len(memories) // parallel_threads)
        batches = [memories[i:i + batch_size] for i in range(0, len(memories), batch_size)]
        
        with ThreadPoolExecutor(max_workers=parallel_threads) as executor:
            future_to_batch = {executor.submit(add_memory_batch, batch): batch for batch in batches}
            
            for future in as_completed(future_to_batch):
                try:
                    batch_results = future.result()
                    for result in batch_results:
                        self.results["add_operations"].append(result)
                        if result["success"]:
                            success_count += 1
                        else:
                            error_count += 1
                except Exception as e:
                    self.results["errors"].append(f"Batch processing error: {e}")
                    error_count += len(future_to_batch[future])
        
        total_time = time.time() - start_time
        
        add_times = [op["duration_ms"] for op in self.results["add_operations"] if "duration_ms" in op]
        
        return {
            "total_memories": len(memories),
            "successful_adds": success_count,
            "failed_adds": error_count,
            "success_rate": success_count / len(memories) if memories else 0,
            "total_time_seconds": total_time,
            "average_add_time_ms": sum(add_times) / len(add_times) if add_times else 0,
            "max_add_time_ms": max(add_times) if add_times else 0,
            "min_add_time_ms": min(add_times) if add_times else 0,
            "throughput_ops_per_second": len(memories) / total_time if total_time > 0 else 0
        }
    
    def run_search_stress_test(self, search_queries: List[str], iterations: int = 100, parallel_threads: int = 5) -> Dict[str, Any]:
        """Stress test memory search with parallel operations"""
        print(f"🔍 Running SEARCH stress test: {len(search_queries)} queries × {iterations} iterations, {parallel_threads} threads")
        
        start_time = time.time()
        total_searches = 0
        successful_searches = 0
        
        def search_batch(queries_batch):
            batch_results = []
            for query in queries_batch:
                for _ in range(iterations):
                    operation_start = time.time()
                    results = self.client.search_memories(query, max_results=5)
                    operation_time = time.time() - operation_start
                    
                    batch_results.append({
                        "query": query,
                        "success": len(results) > 0,
                        "result_count": len(results),
                        "duration_ms": operation_time * 1000,
                        "confidence_scores": [r.confidence for r in results]
                    })
            return batch_results
        
        # Prepare queries for parallel processing  
        queries_per_thread = max(1, len(search_queries) // parallel_threads)
        query_batches = [search_queries[i:i + queries_per_thread] for i in range(0, len(search_queries), queries_per_thread)]
        
        with ThreadPoolExecutor(max_workers=parallel_threads) as executor:
            future_to_batch = {executor.submit(search_batch, batch): batch for batch in query_batches}
            
            for future in as_completed(future_to_batch):
                try:
                    batch_results = future.result()
                    for result in batch_results:
                        self.results["search_operations"].append(result)
                        total_searches += 1
                        if result["success"]:
                            successful_searches += 1
                except Exception as e:
                    self.results["errors"].append(f"Search batch error: {e}")
        
        total_time = time.time() - start_time
        
        search_times = [op["duration_ms"] for op in self.results["search_operations"]]
        result_counts = [op["result_count"] for op in self.results["search_operations"]]
        
        return {
            "total_searches": total_searches,
            "successful_searches": successful_searches, 
            "success_rate": successful_searches / total_searches if total_searches > 0 else 0,
            "total_time_seconds": total_time,
            "average_search_time_ms": sum(search_times) / len(search_times) if search_times else 0,
            "max_search_time_ms": max(search_times) if search_times else 0,
            "min_search_time_ms": min(search_times) if search_times else 0,
            "average_results_per_query": sum(result_counts) / len(result_counts) if result_counts else 0,
            "throughput_searches_per_second": total_searches / total_time if total_time > 0 else 0
        }

def main():
    """Main testing function"""
    print("🚀 MFN System Memory Capabilities Test")
    print("=" * 50)
    
    # Initialize client
    client = MFNClient()
    
    # Health check
    if not client.health_check():
        print("❌ MFN system is not healthy. Make sure Layer 3 is running on localhost:8082")
        return
    
    print("✅ MFN system is healthy")
    
    # Initialize stress tester
    tester = MFNStressTester(client)
    
    # Generate test data
    print("\n📝 Generating test memories...")
    test_memories = tester.generate_test_memories(100)
    print(f"Generated {len(test_memories)} test memories")
    
    # Example of first few memories
    print("\nSample memories:")
    for i, memory in enumerate(test_memories[:3]):
        print(f"  {i+1}. [{', '.join(memory.tags)}] {memory.content}")
    
    # Run stress tests
    print("\n" + "="*50)
    print("STRESS TESTING")
    print("="*50)
    
    # Memory addition stress test
    add_results = tester.run_add_stress_test(test_memories, parallel_threads=5)
    
    print(f"""
📊 ADD STRESS TEST RESULTS:
   Total Memories: {add_results['total_memories']}
   Successful: {add_results['successful_adds']} ({add_results['success_rate']:.1%})
   Failed: {add_results['failed_adds']}
   Total Time: {add_results['total_time_seconds']:.2f}s
   Average Add Time: {add_results['average_add_time_ms']:.2f}ms
   Throughput: {add_results['throughput_ops_per_second']:.2f} ops/sec
   """)
    
    # Search stress test
    search_queries = [
        "quantum mechanics",
        "machine learning algorithm", 
        "neural network",
        "economic theory",
        "biological system",
        "software engineering",
        "mathematical model",
        "scientific research"
    ]
    
    search_results = tester.run_search_stress_test(search_queries, iterations=10, parallel_threads=3)
    
    print(f"""
🔍 SEARCH STRESS TEST RESULTS:
   Total Searches: {search_results['total_searches']}
   Successful: {search_results['successful_searches']} ({search_results['success_rate']:.1%})
   Total Time: {search_results['total_time_seconds']:.2f}s
   Average Search Time: {search_results['average_search_time_ms']:.2f}ms
   Average Results/Query: {search_results['average_results_per_query']:.1f}
   Throughput: {search_results['throughput_searches_per_second']:.2f} searches/sec
   """)
    
    # System performance stats
    stats = client.get_system_stats()
    if stats:
        print("\n📈 SYSTEM PERFORMANCE:")
        for key, value in stats.items():
            if isinstance(value, (int, float)):
                print(f"   {key}: {value}")
    
    print("\n✅ Memory capabilities test completed!")
    
    # Save detailed results
    results_file = f"/tmp/mfn_test_results_{int(time.time())}.json"
    with open(results_file, "w") as f:
        json.dump({
            "add_test": add_results,
            "search_test": search_results,
            "system_stats": stats,
            "test_timestamp": time.time()
        }, f, indent=2)
    
    print(f"📁 Detailed results saved to: {results_file}")

if __name__ == "__main__":
    main()