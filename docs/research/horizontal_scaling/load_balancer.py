#!/usr/bin/env python3
"""
MFN System Horizontal Scaling Load Balancer
==========================================
High-performance load balancer for distributing queries across multiple
MFN instances to achieve 1000+ QPS throughput.

Features:
- Multiple load balancing algorithms
- Health checking and failover
- Connection pooling per instance
- Real-time performance monitoring
- Automatic scaling triggers
"""

import asyncio
import aiohttp
import json
import logging
import time
import threading
from typing import List, Dict, Any, Optional, Tuple
from dataclasses import dataclass, asdict
from enum import Enum
import hashlib
import random
from collections import deque, defaultdict
import statistics

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class LoadBalancingStrategy(Enum):
    ROUND_ROBIN = "round_robin"
    LEAST_CONNECTIONS = "least_connections"
    WEIGHTED_ROUND_ROBIN = "weighted_round_robin"
    LEAST_RESPONSE_TIME = "least_response_time"
    CONSISTENT_HASH = "consistent_hash"

class InstanceStatus(Enum):
    HEALTHY = "healthy"
    UNHEALTHY = "unhealthy"
    DRAINING = "draining"
    STARTING = "starting"

@dataclass
class MFNInstance:
    """Represents a single MFN instance"""
    id: str
    url: str
    weight: int = 1
    status: InstanceStatus = InstanceStatus.STARTING
    active_connections: int = 0
    total_requests: int = 0
    failed_requests: int = 0
    average_response_time_ms: float = 0.0
    last_health_check: float = 0.0
    health_check_failures: int = 0
    
    @property
    def success_rate(self) -> float:
        if self.total_requests == 0:
            return 1.0
        return (self.total_requests - self.failed_requests) / self.total_requests
    
    @property
    def is_healthy(self) -> bool:
        return self.status == InstanceStatus.HEALTHY

@dataclass
class LoadBalancerConfig:
    """Configuration for the load balancer"""
    strategy: LoadBalancingStrategy = LoadBalancingStrategy.LEAST_RESPONSE_TIME
    health_check_interval: int = 10  # seconds
    health_check_timeout: int = 5    # seconds
    max_health_check_failures: int = 3
    connection_timeout: int = 5      # seconds
    request_timeout: int = 10        # seconds
    max_connections_per_instance: int = 50
    enable_sticky_sessions: bool = False
    sticky_session_ttl: int = 300    # seconds
    circuit_breaker_threshold: float = 0.5  # 50% failure rate
    circuit_breaker_reset_timeout: int = 60  # seconds

class PerformanceMetrics:
    """Tracks performance metrics for the load balancer"""
    
    def __init__(self, window_size: int = 1000):
        self.window_size = window_size
        self.response_times = deque(maxlen=window_size)
        self.request_timestamps = deque(maxlen=window_size)
        self.successful_requests = 0
        self.failed_requests = 0
        self.total_requests = 0
        self.lock = threading.Lock()
    
    def record_request(self, response_time_ms: float, success: bool):
        """Record a request's performance metrics"""
        with self.lock:
            self.response_times.append(response_time_ms)
            self.request_timestamps.append(time.time())
            self.total_requests += 1
            
            if success:
                self.successful_requests += 1
            else:
                self.failed_requests += 1
    
    def get_current_qps(self, window_seconds: int = 10) -> float:
        """Calculate current QPS over the specified window"""
        with self.lock:
            current_time = time.time()
            cutoff_time = current_time - window_seconds
            
            recent_requests = [ts for ts in self.request_timestamps if ts >= cutoff_time]
            return len(recent_requests) / window_seconds if recent_requests else 0.0
    
    def get_average_response_time(self) -> float:
        """Get average response time from recent requests"""
        with self.lock:
            return statistics.mean(self.response_times) if self.response_times else 0.0
    
    def get_p95_response_time(self) -> float:
        """Get 95th percentile response time"""
        with self.lock:
            if not self.response_times:
                return 0.0
            return statistics.quantiles(sorted(self.response_times), n=20)[18]  # 95th percentile
    
    def get_success_rate(self) -> float:
        """Get success rate"""
        with self.lock:
            if self.total_requests == 0:
                return 1.0
            return self.successful_requests / self.total_requests

class SessionStore:
    """Manages sticky sessions for consistent routing"""
    
    def __init__(self, ttl_seconds: int = 300):
        self.sessions = {}
        self.ttl_seconds = ttl_seconds
        self.lock = threading.RLock()
    
    def get_instance(self, session_id: str) -> Optional[str]:
        """Get the assigned instance for a session"""
        with self.lock:
            if session_id in self.sessions:
                instance_id, timestamp = self.sessions[session_id]
                if time.time() - timestamp < self.ttl_seconds:
                    return instance_id
                else:
                    del self.sessions[session_id]
        return None
    
    def set_instance(self, session_id: str, instance_id: str):
        """Assign an instance to a session"""
        with self.lock:
            self.sessions[session_id] = (instance_id, time.time())
    
    def cleanup_expired_sessions(self):
        """Remove expired sessions"""
        with self.lock:
            current_time = time.time()
            expired_sessions = [
                session_id for session_id, (_, timestamp) in self.sessions.items()
                if current_time - timestamp >= self.ttl_seconds
            ]
            for session_id in expired_sessions:
                del self.sessions[session_id]

class MFNLoadBalancer:
    """High-performance load balancer for MFN instances"""
    
    def __init__(self, instances: List[Dict[str, Any]], config: LoadBalancerConfig = None):
        self.config = config or LoadBalancerConfig()
        self.instances = {}
        self.instance_order = []  # For round-robin
        self.current_index = 0
        
        # Performance tracking
        self.metrics = PerformanceMetrics()
        self.session_store = SessionStore(self.config.sticky_session_ttl)
        
        # Connection management
        self.sessions = {}  # Instance ID -> aiohttp.ClientSession
        self.instance_locks = defaultdict(asyncio.Lock)
        
        # Health checking
        self.health_check_task = None
        self.health_check_running = False
        
        # Initialize instances
        for instance_data in instances:
            instance = MFNInstance(
                id=instance_data['id'],
                url=instance_data['url'],
                weight=instance_data.get('weight', 1)
            )
            self.instances[instance.id] = instance
            self.instance_order.append(instance.id)
        
        logger.info(f"Initialized load balancer with {len(self.instances)} instances")
    
    async def start(self):
        """Start the load balancer services"""
        await self._initialize_sessions()
        await self._start_health_checking()
        logger.info("Load balancer started")
    
    async def stop(self):
        """Stop the load balancer services"""
        await self._stop_health_checking()
        await self._close_sessions()
        logger.info("Load balancer stopped")
    
    async def _initialize_sessions(self):
        """Initialize HTTP sessions for each instance"""
        connector_config = {
            'limit': self.config.max_connections_per_instance,
            'limit_per_host': self.config.max_connections_per_instance,
            'keepalive_timeout': 30,
            'enable_cleanup_closed': True,
            'use_dns_cache': True,
        }
        
        timeout_config = aiohttp.ClientTimeout(
            total=self.config.request_timeout,
            connect=self.config.connection_timeout
        )
        
        for instance_id in self.instances:
            connector = aiohttp.TCPConnector(**connector_config)
            session = aiohttp.ClientSession(
                connector=connector,
                timeout=timeout_config,
                headers={'Connection': 'keep-alive', 'Content-Type': 'application/json'}
            )
            self.sessions[instance_id] = session
    
    async def _close_sessions(self):
        """Close all HTTP sessions"""
        for session in self.sessions.values():
            await session.close()
        self.sessions.clear()
    
    async def _start_health_checking(self):
        """Start background health checking"""
        self.health_check_running = True
        self.health_check_task = asyncio.create_task(self._health_check_loop())
    
    async def _stop_health_checking(self):
        """Stop background health checking"""
        self.health_check_running = False
        if self.health_check_task:
            self.health_check_task.cancel()
            try:
                await self.health_check_task
            except asyncio.CancelledError:
                pass
    
    async def _health_check_loop(self):
        """Background health checking loop"""
        while self.health_check_running:
            try:
                await self._perform_health_checks()
                await asyncio.sleep(self.config.health_check_interval)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Health check loop error: {e}")
                await asyncio.sleep(1)
    
    async def _perform_health_checks(self):
        """Perform health checks on all instances"""
        health_tasks = [
            self._check_instance_health(instance_id, instance)
            for instance_id, instance in self.instances.items()
        ]
        
        await asyncio.gather(*health_tasks, return_exceptions=True)
        
        # Clean up expired sticky sessions
        self.session_store.cleanup_expired_sessions()
    
    async def _check_instance_health(self, instance_id: str, instance: MFNInstance):
        """Check health of a single instance"""
        try:
            session = self.sessions[instance_id]
            
            start_time = time.time()
            async with session.get(f"{instance.url}/health") as response:
                response_time = (time.time() - start_time) * 1000
                
                if response.status == 200:
                    # Instance is healthy
                    if instance.status == InstanceStatus.UNHEALTHY:
                        logger.info(f"Instance {instance_id} recovered")
                    
                    instance.status = InstanceStatus.HEALTHY
                    instance.health_check_failures = 0
                    instance.last_health_check = time.time()
                    
                    # Update response time (with exponential smoothing)
                    alpha = 0.3
                    instance.average_response_time_ms = (
                        alpha * response_time + 
                        (1 - alpha) * instance.average_response_time_ms
                    )
                else:
                    self._mark_instance_unhealthy(instance_id, instance, f"HTTP {response.status}")
                    
        except Exception as e:
            self._mark_instance_unhealthy(instance_id, instance, str(e))
    
    def _mark_instance_unhealthy(self, instance_id: str, instance: MFNInstance, reason: str):
        """Mark an instance as unhealthy"""
        instance.health_check_failures += 1
        
        if instance.health_check_failures >= self.config.max_health_check_failures:
            if instance.status == InstanceStatus.HEALTHY:
                logger.warning(f"Instance {instance_id} marked unhealthy: {reason}")
            
            instance.status = InstanceStatus.UNHEALTHY
            instance.last_health_check = time.time()
    
    def _select_instance(self, session_id: Optional[str] = None) -> Optional[MFNInstance]:
        """Select an instance based on the configured strategy"""
        
        # Check sticky sessions first
        if session_id and self.config.enable_sticky_sessions:
            sticky_instance_id = self.session_store.get_instance(session_id)
            if sticky_instance_id and sticky_instance_id in self.instances:
                instance = self.instances[sticky_instance_id]
                if instance.is_healthy:
                    return instance
        
        # Get healthy instances
        healthy_instances = [
            instance for instance in self.instances.values()
            if instance.is_healthy
        ]
        
        if not healthy_instances:
            logger.warning("No healthy instances available!")
            return None
        
        # Apply load balancing strategy
        if self.config.strategy == LoadBalancingStrategy.ROUND_ROBIN:
            selected = self._round_robin_selection(healthy_instances)
        elif self.config.strategy == LoadBalancingStrategy.LEAST_CONNECTIONS:
            selected = self._least_connections_selection(healthy_instances)
        elif self.config.strategy == LoadBalancingStrategy.WEIGHTED_ROUND_ROBIN:
            selected = self._weighted_round_robin_selection(healthy_instances)
        elif self.config.strategy == LoadBalancingStrategy.LEAST_RESPONSE_TIME:
            selected = self._least_response_time_selection(healthy_instances)
        elif self.config.strategy == LoadBalancingStrategy.CONSISTENT_HASH:
            selected = self._consistent_hash_selection(healthy_instances, session_id or "")
        else:
            selected = healthy_instances[0]  # Fallback
        
        # Update sticky session
        if session_id and self.config.enable_sticky_sessions and selected:
            self.session_store.set_instance(session_id, selected.id)
        
        return selected
    
    def _round_robin_selection(self, healthy_instances: List[MFNInstance]) -> MFNInstance:
        """Round-robin selection"""
        self.current_index = (self.current_index + 1) % len(healthy_instances)
        return healthy_instances[self.current_index]
    
    def _least_connections_selection(self, healthy_instances: List[MFNInstance]) -> MFNInstance:
        """Select instance with least active connections"""
        return min(healthy_instances, key=lambda x: x.active_connections)
    
    def _weighted_round_robin_selection(self, healthy_instances: List[MFNInstance]) -> MFNInstance:
        """Weighted round-robin selection"""
        total_weight = sum(instance.weight for instance in healthy_instances)
        if total_weight == 0:
            return healthy_instances[0]
        
        # Simple weighted selection
        weights = [instance.weight / total_weight for instance in healthy_instances]
        selected_index = random.choices(range(len(healthy_instances)), weights=weights)[0]
        return healthy_instances[selected_index]
    
    def _least_response_time_selection(self, healthy_instances: List[MFNInstance]) -> MFNInstance:
        """Select instance with lowest average response time"""
        return min(healthy_instances, key=lambda x: x.average_response_time_ms or float('inf'))
    
    def _consistent_hash_selection(self, healthy_instances: List[MFNInstance], key: str) -> MFNInstance:
        """Consistent hash selection for session affinity"""
        hash_value = int(hashlib.md5(key.encode()).hexdigest(), 16)
        instance_index = hash_value % len(healthy_instances)
        return healthy_instances[instance_index]
    
    async def route_request(self, method: str, path: str, **kwargs) -> Tuple[bool, Dict[str, Any]]:
        """Route a request to an appropriate instance"""
        session_id = kwargs.get('session_id')
        
        # Select instance
        instance = self._select_instance(session_id)
        if not instance:
            return False, {"error": "No healthy instances available"}
        
        # Track connection
        instance.active_connections += 1
        instance.total_requests += 1
        
        start_time = time.time()
        success = False
        response_data = {}
        
        try:
            session = self.sessions[instance.id]
            url = f"{instance.url}{path}"
            
            # Prepare request arguments
            request_kwargs = {k: v for k, v in kwargs.items() if k != 'session_id'}
            
            # Execute request
            async with session.request(method, url, **request_kwargs) as response:
                response_time = (time.time() - start_time) * 1000
                
                if response.status == 200:
                    response_data = await response.json()
                    success = True
                else:
                    response_data = {
                        "error": f"HTTP {response.status}",
                        "instance_id": instance.id
                    }
                    instance.failed_requests += 1
                
                # Update metrics
                self.metrics.record_request(response_time, success)
                
                # Update instance metrics
                alpha = 0.1  # Smoothing factor
                instance.average_response_time_ms = (
                    alpha * response_time + 
                    (1 - alpha) * instance.average_response_time_ms
                )
                
        except Exception as e:
            response_time = (time.time() - start_time) * 1000
            response_data = {
                "error": str(e),
                "instance_id": instance.id
            }
            instance.failed_requests += 1
            self.metrics.record_request(response_time, False)
            logger.error(f"Request to {instance.id} failed: {e}")
        
        finally:
            instance.active_connections -= 1
        
        return success, response_data
    
    async def add_memory(self, memory_data: Dict[str, Any], session_id: str = None) -> Tuple[bool, Dict[str, Any]]:
        """Add memory through load balancer"""
        return await self.route_request('POST', '/memories', json=memory_data, session_id=session_id)
    
    async def search_memories(self, query_data: Dict[str, Any], session_id: str = None) -> Tuple[bool, Dict[str, Any]]:
        """Search memories through load balancer"""
        return await self.route_request('POST', '/search', json=query_data, session_id=session_id)
    
    async def health_check(self) -> Dict[str, Any]:
        """Get aggregated health status"""
        healthy_instances = sum(1 for instance in self.instances.values() if instance.is_healthy)
        total_instances = len(self.instances)
        
        return {
            "status": "healthy" if healthy_instances > 0 else "unhealthy",
            "healthy_instances": healthy_instances,
            "total_instances": total_instances,
            "current_qps": self.metrics.get_current_qps(),
            "average_response_time_ms": self.metrics.get_average_response_time(),
            "p95_response_time_ms": self.metrics.get_p95_response_time(),
            "success_rate": self.metrics.get_success_rate(),
            "strategy": self.config.strategy.value,
            "instances": {
                instance_id: {
                    "url": instance.url,
                    "status": instance.status.value,
                    "active_connections": instance.active_connections,
                    "total_requests": instance.total_requests,
                    "success_rate": instance.success_rate,
                    "average_response_time_ms": instance.average_response_time_ms
                }
                for instance_id, instance in self.instances.items()
            }
        }
    
    def get_performance_stats(self) -> Dict[str, Any]:
        """Get detailed performance statistics"""
        return {
            "load_balancer": {
                "total_requests": self.metrics.total_requests,
                "successful_requests": self.metrics.successful_requests,
                "failed_requests": self.metrics.failed_requests,
                "success_rate": self.metrics.get_success_rate(),
                "current_qps": self.metrics.get_current_qps(),
                "average_response_time_ms": self.metrics.get_average_response_time(),
                "p95_response_time_ms": self.metrics.get_p95_response_time(),
            },
            "instances": {
                instance_id: asdict(instance) for instance_id, instance in self.instances.items()
            },
            "config": {
                "strategy": self.config.strategy.value,
                "health_check_interval": self.config.health_check_interval,
                "max_connections_per_instance": self.config.max_connections_per_instance,
                "enable_sticky_sessions": self.config.enable_sticky_sessions,
            }
        }


# Example usage and testing
async def main():
    """Example usage of the load balancer"""
    
    # Configure instances
    instances = [
        {"id": "mfn-1", "url": "http://localhost:8082", "weight": 1},
        {"id": "mfn-2", "url": "http://localhost:8083", "weight": 1},
        {"id": "mfn-3", "url": "http://localhost:8084", "weight": 1},
        {"id": "mfn-4", "url": "http://localhost:8085", "weight": 1},
    ]
    
    # Configure load balancer
    config = LoadBalancerConfig(
        strategy=LoadBalancingStrategy.LEAST_RESPONSE_TIME,
        max_connections_per_instance=100,
        health_check_interval=5
    )
    
    # Initialize load balancer
    lb = MFNLoadBalancer(instances, config)
    
    try:
        await lb.start()
        
        # Test load balancing
        print("Testing load balancer...")
        
        # Simulate multiple concurrent requests
        tasks = []
        for i in range(100):
            query_data = {
                "start_memory_ids": [1, 2, 3],
                "max_results": 10,
                "max_depth": 2,
                "min_weight": 0.1,
                "search_mode": "depth_first"
            }
            
            task = lb.search_memories(query_data, session_id=f"session_{i % 10}")
            tasks.append(task)
        
        # Execute concurrent requests
        start_time = time.time()
        results = await asyncio.gather(*tasks, return_exceptions=True)
        duration = time.time() - start_time
        
        # Analyze results
        successful_requests = sum(1 for success, _ in results if success)
        qps = len(results) / duration
        
        print(f"Completed {len(results)} requests in {duration:.2f}s")
        print(f"QPS: {qps:.2f}")
        print(f"Success rate: {successful_requests / len(results) * 100:.1f}%")
        
        # Print health status
        health_status = await lb.health_check()
        print(f"\nHealth Status:")
        print(f"  Healthy instances: {health_status['healthy_instances']}/{health_status['total_instances']}")
        print(f"  Current QPS: {health_status['current_qps']:.2f}")
        print(f"  Avg response time: {health_status['average_response_time_ms']:.2f}ms")
        
        # Print detailed stats
        stats = lb.get_performance_stats()
        print(f"\nPerformance Stats:")
        print(f"  Total requests: {stats['load_balancer']['total_requests']}")
        print(f"  Success rate: {stats['load_balancer']['success_rate']:.1%}")
        
    finally:
        await lb.stop()


if __name__ == "__main__":
    asyncio.run(main())