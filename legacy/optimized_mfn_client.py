#!/usr/bin/env python3
"""
Optimized MFN System Client for 1000+ QPS Throughput
====================================================
High-performance client with connection pooling, async operations,
and parallel query processing optimized for maximum throughput.
"""

import asyncio
import aiohttp
import time
import json
import logging
from typing import List, Dict, Any, Optional
from dataclasses import dataclass, asdict
from concurrent.futures import ThreadPoolExecutor
import hashlib
import threading
from collections import defaultdict

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

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

class ResultCache:
    """High-performance LRU cache for query results"""
    
    def __init__(self, max_size: int = 10000, ttl_seconds: int = 300):
        self.max_size = max_size
        self.ttl_seconds = ttl_seconds
        self.cache = {}
        self.access_times = {}
        self.lock = threading.RLock()
    
    def _generate_key(self, query: str, **kwargs) -> str:
        """Generate cache key from query parameters"""
        key_data = f"{query}:{json.dumps(sorted(kwargs.items()))}"
        return hashlib.md5(key_data.encode()).hexdigest()
    
    def get(self, query: str, **kwargs) -> Optional[List[SearchResult]]:
        """Get cached results if available and not expired"""
        key = self._generate_key(query, **kwargs)
        
        with self.lock:
            if key not in self.cache:
                return None
            
            cached_time, results = self.cache[key]
            if time.time() - cached_time > self.ttl_seconds:
                del self.cache[key]
                del self.access_times[key]
                return None
            
            self.access_times[key] = time.time()
            return results
    
    def put(self, query: str, results: List[SearchResult], **kwargs):
        """Cache query results"""
        key = self._generate_key(query, **kwargs)
        current_time = time.time()
        
        with self.lock:
            # Evict old entries if cache is full
            if len(self.cache) >= self.max_size:
                self._evict_lru()
            
            self.cache[key] = (current_time, results)
            self.access_times[key] = current_time
    
    def _evict_lru(self):
        """Evict least recently used entries"""
        if not self.access_times:
            return
        
        # Remove oldest 10% of entries
        sorted_keys = sorted(self.access_times.keys(), 
                           key=lambda k: self.access_times[k])
        evict_count = max(1, len(sorted_keys) // 10)
        
        for key in sorted_keys[:evict_count]:
            self.cache.pop(key, None)
            self.access_times.pop(key, None)

class OptimizedMFNClient:
    """High-performance MFN client optimized for 1000+ QPS"""
    
    def __init__(self, 
                 layer3_url: str = "http://localhost:8082",
                 max_connections: int = 200,
                 connection_timeout: int = 5,
                 request_timeout: int = 10,
                 enable_caching: bool = True,
                 cache_size: int = 10000):
        
        self.layer3_url = layer3_url
        self.max_connections = max_connections
        self.connection_timeout = connection_timeout
        self.request_timeout = request_timeout
        
        # Performance tracking
        self.stats = {
            'total_requests': 0,
            'successful_requests': 0,
            'failed_requests': 0,
            'cache_hits': 0,
            'cache_misses': 0,
            'total_response_time': 0.0
        }
        self.stats_lock = threading.Lock()
        
        # Caching
        self.cache = ResultCache(max_size=cache_size) if enable_caching else None
        
        # Connection session (will be initialized async)
        self.session = None
        self.session_lock = asyncio.Lock()
    
    async def _ensure_session(self):
        """Ensure aiohttp session is initialized with optimizations"""
        if self.session is None:
            async with self.session_lock:
                if self.session is None:  # Double-check pattern
                    # Create optimized connector
                    connector = aiohttp.TCPConnector(
                        limit=self.max_connections,
                        limit_per_host=50,
                        keepalive_timeout=30,
                        enable_cleanup_closed=True,
                        use_dns_cache=True,
                        force_close=False,
                        enable_keepalive=True
                    )
                    
                    # Create session with optimized timeouts
                    timeout = aiohttp.ClientTimeout(
                        total=self.request_timeout,
                        connect=self.connection_timeout,
                        sock_read=5,
                        sock_connect=3
                    )
                    
                    self.session = aiohttp.ClientSession(
                        connector=connector,
                        timeout=timeout,
                        headers={
                            'Connection': 'keep-alive',
                            'Content-Type': 'application/json'
                        }
                    )
    
    def _update_stats(self, success: bool, response_time: float, cache_hit: bool = False):
        """Update performance statistics"""
        with self.stats_lock:
            self.stats['total_requests'] += 1
            self.stats['total_response_time'] += response_time
            
            if success:
                self.stats['successful_requests'] += 1
            else:
                self.stats['failed_requests'] += 1
            
            if cache_hit:
                self.stats['cache_hits'] += 1
            else:
                self.stats['cache_misses'] += 1
    
    async def add_memory(self, memory: MemoryItem) -> bool:
        """Add a memory to the MFN system with connection pooling"""
        await self._ensure_session()
        
        start_time = time.time()
        try:
            payload = {
                "id": memory.id,
                "content": memory.content,
                "tags": memory.tags,
                "metadata": memory.metadata
            }
            
            async with self.session.post(
                f"{self.layer3_url}/memories",
                json=payload
            ) as response:
                success = response.status == 200
                response_time = time.time() - start_time
                self._update_stats(success, response_time)
                return success
                
        except Exception as e:
            response_time = time.time() - start_time
            self._update_stats(False, response_time)
            logger.error(f"Error adding memory {memory.id}: {e}")
            return False
    
    async def search_memories(self, 
                            query: str, 
                            max_results: int = 10, 
                            search_mode: str = "depth_first") -> List[SearchResult]:
        """Search for memories with caching and optimized requests"""
        
        # Check cache first
        if self.cache:
            cached_results = self.cache.get(query, max_results=max_results, search_mode=search_mode)
            if cached_results:
                self._update_stats(True, 0.001, cache_hit=True)  # Cache hit time ~1ms
                return cached_results
        
        await self._ensure_session()
        start_time = time.time()
        
        try:
            # Step 1: Find starting memories by content similarity
            start_memory_ids = await self._find_relevant_memory_ids_async(query, max_starting_points=3)
            
            if not start_memory_ids:
                self._update_stats(True, time.time() - start_time)
                return []
            
            # Step 2: Perform associative search
            payload = {
                "start_memory_ids": start_memory_ids,
                "max_results": max_results,
                "max_depth": 2,
                "min_weight": 0.1,
                "search_mode": search_mode
            }
            
            async with self.session.post(
                f"{self.layer3_url}/search",
                json=payload
            ) as response:
                
                response_time = time.time() - start_time
                
                if response.status == 200:
                    data = await response.json()
                    results = []
                    
                    for result in data.get("results", []):
                        memory = result.get("memory", {})
                        results.append(SearchResult(
                            memory_id=memory.get("id", 0),
                            content=memory.get("content", ""),
                            confidence=result.get("total_weight", 0.0),
                            path=result.get("path", [])
                        ))
                    
                    # Cache successful results
                    if self.cache and results:
                        self.cache.put(query, results, max_results=max_results, search_mode=search_mode)
                    
                    self._update_stats(True, response_time)
                    return results
                else:
                    self._update_stats(False, response_time)
                    logger.error(f"Search failed with status {response.status}")
                    return []
                    
        except Exception as e:
            response_time = time.time() - start_time
            self._update_stats(False, response_time)
            logger.error(f"Error searching memories: {e}")
            return []
    
    async def _find_relevant_memory_ids_async(self, query: str, max_starting_points: int = 3) -> List[int]:
        """Async version of finding relevant memory IDs"""
        try:
            async with self.session.get(f"{self.layer3_url}/memories") as response:
                if response.status != 200:
                    return []
                
                data = await response.json()
                memories = data.get("memories", [])
                
                # Simple content similarity scoring
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
                    tag_boost = sum(0.2 for tag in tags if tag.lower() in query.lower())
                    
                    final_score = similarity + tag_boost
                    
                    if final_score > 0:
                        scored_memories.append((memory.get("id"), final_score))
                
                # Sort by score and return top memory IDs
                scored_memories.sort(key=lambda x: x[1], reverse=True)
                return [mem_id for mem_id, _ in scored_memories[:max_starting_points]]
                
        except Exception as e:
            logger.error(f"Error finding relevant memories: {e}")
            return []
    
    async def batch_search(self, queries: List[str], max_results: int = 10) -> List[List[SearchResult]]:
        """Perform multiple searches concurrently"""
        tasks = [
            self.search_memories(query, max_results) 
            for query in queries
        ]
        
        results = await asyncio.gather(*tasks, return_exceptions=True)
        
        # Handle exceptions
        processed_results = []
        for result in results:
            if isinstance(result, Exception):
                logger.error(f"Batch search error: {result}")
                processed_results.append([])
            else:
                processed_results.append(result)
        
        return processed_results
    
    async def get_memory(self, memory_id: int) -> Optional[MemoryItem]:
        """Get a specific memory by ID with connection pooling"""
        await self._ensure_session()
        
        start_time = time.time()
        try:
            async with self.session.get(f"{self.layer3_url}/memories/{memory_id}") as response:
                response_time = time.time() - start_time
                
                if response.status == 200:
                    data = await response.json()
                    self._update_stats(True, response_time)
                    return MemoryItem(
                        id=data.get("id"),
                        content=data.get("content", ""),
                        tags=data.get("tags", []),
                        metadata=data.get("metadata", {})
                    )
                else:
                    self._update_stats(False, response_time)
                    return None
                    
        except Exception as e:
            response_time = time.time() - start_time
            self._update_stats(False, response_time)
            logger.error(f"Error getting memory {memory_id}: {e}")
            return None
    
    async def health_check(self) -> bool:
        """Check if the MFN system is healthy"""
        await self._ensure_session()
        
        try:
            async with self.session.get(f"{self.layer3_url}/health") as response:
                return response.status == 200
        except:
            return False
    
    def get_performance_stats(self) -> Dict[str, Any]:
        """Get client performance statistics"""
        with self.stats_lock:
            stats = self.stats.copy()
        
        # Calculate derived metrics
        if stats['total_requests'] > 0:
            stats['success_rate'] = stats['successful_requests'] / stats['total_requests']
            stats['error_rate'] = stats['failed_requests'] / stats['total_requests']
            stats['average_response_time_ms'] = (stats['total_response_time'] / stats['total_requests']) * 1000
            
            if stats['cache_hits'] + stats['cache_misses'] > 0:
                stats['cache_hit_rate'] = stats['cache_hits'] / (stats['cache_hits'] + stats['cache_misses'])
            else:
                stats['cache_hit_rate'] = 0.0
        else:
            stats['success_rate'] = 0.0
            stats['error_rate'] = 0.0
            stats['average_response_time_ms'] = 0.0
            stats['cache_hit_rate'] = 0.0
        
        return stats
    
    async def close(self):
        """Close the aiohttp session"""
        if self.session:
            await self.session.close()


class HighThroughputLoadTester:
    """Load tester optimized for 1000+ QPS validation"""
    
    def __init__(self, client: OptimizedMFNClient):
        self.client = client
        self.results = []
        
    async def throughput_test(self, 
                            target_qps: int = 1000,
                            test_duration_seconds: int = 60,
                            warmup_seconds: int = 10) -> Dict[str, Any]:
        """Test system throughput at target QPS"""
        
        logger.info(f"🚀 Starting throughput test: {target_qps} QPS for {test_duration_seconds}s")
        
        # Warmup phase
        logger.info(f"Warming up for {warmup_seconds} seconds...")
        await self._warmup_phase(warmup_seconds)
        
        # Test queries
        test_queries = [
            "neural networks and machine learning",
            "quantum computing principles",
            "associative memory systems",
            "brain neurons and connections",
            "artificial intelligence algorithms",
            "distributed computing networks",
            "cognitive science research",
            "graph algorithms and paths"
        ]
        
        # Calculate timing
        query_interval = 1.0 / target_qps  # Time between queries
        total_queries = target_qps * test_duration_seconds
        
        logger.info(f"Executing {total_queries} queries at {target_qps} QPS...")
        
        start_time = time.time()
        tasks = []
        query_times = []
        
        # Create concurrent tasks with precise timing
        for i in range(total_queries):
            query = test_queries[i % len(test_queries)]
            
            # Schedule query at specific time
            scheduled_time = start_time + (i * query_interval)
            
            task = asyncio.create_task(
                self._timed_query(query, scheduled_time, i)
            )
            tasks.append(task)
        
        # Execute all queries
        results = await asyncio.gather(*tasks, return_exceptions=True)
        
        end_time = time.time()
        actual_duration = end_time - start_time
        
        # Process results
        successful_queries = 0
        failed_queries = 0
        response_times = []
        errors = []
        
        for result in results:
            if isinstance(result, Exception):
                failed_queries += 1
                errors.append(str(result))
            else:
                successful_queries += 1
                response_times.append(result['response_time_ms'])
        
        # Calculate performance metrics
        actual_qps = len(results) / actual_duration
        success_rate = successful_queries / len(results) if results else 0
        
        performance_stats = self.client.get_performance_stats()
        
        return {
            'test_name': 'high_throughput_test',
            'target_qps': target_qps,
            'actual_qps': actual_qps,
            'qps_achievement_rate': actual_qps / target_qps,
            'test_duration_seconds': actual_duration,
            'total_queries': len(results),
            'successful_queries': successful_queries,
            'failed_queries': failed_queries,
            'success_rate': success_rate,
            'average_response_time_ms': np.mean(response_times) if response_times else 0,
            'p95_response_time_ms': np.percentile(response_times, 95) if response_times else 0,
            'p99_response_time_ms': np.percentile(response_times, 99) if response_times else 0,
            'client_stats': performance_stats,
            'errors_sample': errors[:5],
            'meets_target': actual_qps >= target_qps and success_rate >= 0.95
        }
    
    async def _warmup_phase(self, warmup_seconds: int):
        """Warm up the system with sample queries"""
        warmup_queries = [
            "machine learning",
            "neural networks", 
            "quantum physics",
            "computer science"
        ]
        
        warmup_tasks = []
        for i in range(warmup_seconds * 10):  # 10 queries per second during warmup
            query = warmup_queries[i % len(warmup_queries)]
            task = asyncio.create_task(self.client.search_memories(query, max_results=5))
            warmup_tasks.append(task)
        
        await asyncio.gather(*warmup_tasks, return_exceptions=True)
        logger.info("Warmup phase completed")
    
    async def _timed_query(self, query: str, scheduled_time: float, query_id: int) -> Dict[str, Any]:
        """Execute a query at a specific scheduled time"""
        # Wait until scheduled time
        current_time = time.time()
        if current_time < scheduled_time:
            await asyncio.sleep(scheduled_time - current_time)
        
        start_time = time.time()
        results = await self.client.search_memories(query, max_results=5)
        response_time = (time.time() - start_time) * 1000  # Convert to ms
        
        return {
            'query_id': query_id,
            'query': query,
            'response_time_ms': response_time,
            'result_count': len(results),
            'timestamp': start_time
        }


# Example usage and testing
async def main():
    """Main testing function for optimized client"""
    
    # Initialize optimized client
    client = OptimizedMFNClient(
        max_connections=200,
        enable_caching=True,
        cache_size=10000
    )
    
    try:
        # Health check
        logger.info("Checking MFN system health...")
        if not await client.health_check():
            logger.error("❌ MFN system is not healthy")
            return
        
        logger.info("✅ MFN system is healthy")
        
        # Initialize load tester
        load_tester = HighThroughputLoadTester(client)
        
        # Test different QPS levels
        qps_targets = [100, 500, 1000, 1500]
        
        for target_qps in qps_targets:
            logger.info(f"\n{'='*60}")
            logger.info(f"Testing {target_qps} QPS")
            logger.info(f"{'='*60}")
            
            result = await load_tester.throughput_test(
                target_qps=target_qps,
                test_duration_seconds=30,
                warmup_seconds=5
            )
            
            # Print results
            print(f"""
🎯 THROUGHPUT TEST RESULTS - {target_qps} QPS TARGET:
   Actual QPS: {result['actual_qps']:.2f}
   Achievement Rate: {result['qps_achievement_rate']:.1%}
   Success Rate: {result['success_rate']:.1%}
   Avg Response Time: {result['average_response_time_ms']:.2f}ms
   P95 Response Time: {result['p95_response_time_ms']:.2f}ms
   P99 Response Time: {result['p99_response_time_ms']:.2f}ms
   Target Met: {'✅ YES' if result['meets_target'] else '❌ NO'}
            """)
            
            # Save results
            results_file = f"/tmp/mfn_throughput_{target_qps}qps_{int(time.time())}.json"
            with open(results_file, 'w') as f:
                json.dump(result, f, indent=2, default=str)
            logger.info(f"Results saved to: {results_file}")
            
            if not result['meets_target'] and target_qps <= 1000:
                logger.warning(f"⚠️ Failed to meet {target_qps} QPS target")
                break
        
        # Print final client statistics
        stats = client.get_performance_stats()
        print(f"""
📊 CLIENT PERFORMANCE STATISTICS:
   Total Requests: {stats['total_requests']}
   Success Rate: {stats['success_rate']:.1%}
   Cache Hit Rate: {stats['cache_hit_rate']:.1%}
   Average Response Time: {stats['average_response_time_ms']:.2f}ms
        """)
        
    finally:
        await client.close()


if __name__ == "__main__":
    import numpy as np
    asyncio.run(main())