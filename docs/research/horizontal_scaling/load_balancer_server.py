#!/usr/bin/env python3
"""
MFN Load Balancer HTTP Server
============================
HTTP server that wraps the load balancer for external access.
"""

import asyncio
import json
import logging
import os
import time
from aiohttp import web, ClientSession
from load_balancer import MFNLoadBalancer, LoadBalancingStrategy, LoadBalancerConfig

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

class LoadBalancerServer:
    def __init__(self):
        self.app = web.Application()
        self.load_balancer = None
        self.setup_routes()
    
    def setup_routes(self):
        """Setup HTTP routes"""
        self.app.router.add_get('/', self.handle_root)
        self.app.router.add_get('/health', self.handle_health)
        self.app.router.add_get('/stats', self.handle_stats)
        self.app.router.add_post('/memories', self.handle_add_memory)
        self.app.router.add_get('/memories', self.handle_list_memories)
        self.app.router.add_get('/memories/{memory_id}', self.handle_get_memory)
        self.app.router.add_post('/search', self.handle_search)
        self.app.router.add_post('/associations', self.handle_add_association)
        
        # Administrative endpoints
        self.app.router.add_get('/admin/instances', self.handle_admin_instances)
        self.app.router.add_post('/admin/instances/{instance_id}/drain', self.handle_drain_instance)
        self.app.router.add_post('/admin/instances/{instance_id}/undrain', self.handle_undrain_instance)
    
    async def init_load_balancer(self):
        """Initialize the load balancer"""
        # Get configuration from environment
        instances = [
            {"id": "mfn-1", "url": "http://mfn-instance-1:8082", "weight": 1},
            {"id": "mfn-2", "url": "http://mfn-instance-2:8082", "weight": 1},
            {"id": "mfn-3", "url": "http://mfn-instance-3:8082", "weight": 1},
            {"id": "mfn-4", "url": "http://mfn-instance-4:8082", "weight": 1},
        ]
        
        strategy_name = os.getenv('LB_STRATEGY', 'least_response_time')
        strategy = LoadBalancingStrategy(strategy_name)
        
        config = LoadBalancerConfig(
            strategy=strategy,
            health_check_interval=int(os.getenv('LB_HEALTH_CHECK_INTERVAL', '5')),
            max_connections_per_instance=int(os.getenv('LB_MAX_CONNECTIONS_PER_INSTANCE', '100')),
            enable_sticky_sessions=os.getenv('LB_ENABLE_STICKY_SESSIONS', 'false').lower() == 'true'
        )
        
        self.load_balancer = MFNLoadBalancer(instances, config)
        await self.load_balancer.start()
        
        logger.info(f"Load balancer initialized with {len(instances)} instances")
    
    async def handle_root(self, request):
        """Handle root endpoint"""
        return web.json_response({
            "service": "MFN Load Balancer",
            "version": "1.0.0",
            "description": "High-performance load balancer for Memory Flow Network",
            "endpoints": {
                "GET /": "Service information",
                "GET /health": "Health check and status",
                "GET /stats": "Performance statistics",
                "POST /memories": "Add memory to system",
                "GET /memories": "List memories",
                "GET /memories/{id}": "Get specific memory",
                "POST /search": "Search memories",
                "POST /associations": "Add association",
                "GET /admin/instances": "Instance management",
            },
            "features": [
                "Multiple load balancing strategies",
                "Health checking and failover", 
                "Connection pooling",
                "Performance monitoring",
                "Sticky sessions support"
            ]
        })
    
    async def handle_health(self, request):
        """Handle health check"""
        if not self.load_balancer:
            return web.json_response(
                {"status": "unhealthy", "error": "Load balancer not initialized"}, 
                status=503
            )
        
        health_status = await self.load_balancer.health_check()
        status_code = 200 if health_status["status"] == "healthy" else 503
        
        return web.json_response(health_status, status=status_code)
    
    async def handle_stats(self, request):
        """Handle stats endpoint"""
        if not self.load_balancer:
            return web.json_response({"error": "Load balancer not initialized"}, status=503)
        
        stats = self.load_balancer.get_performance_stats()
        return web.json_response(stats)
    
    async def handle_add_memory(self, request):
        """Handle add memory request"""
        if not self.load_balancer:
            return web.json_response({"error": "Load balancer not initialized"}, status=503)
        
        try:
            memory_data = await request.json()
            session_id = request.headers.get('X-Session-ID')
            
            success, response = await self.load_balancer.add_memory(memory_data, session_id)
            status_code = 200 if success else 500
            
            return web.json_response(response, status=status_code)
            
        except Exception as e:
            logger.error(f"Error adding memory: {e}")
            return web.json_response({"error": str(e)}, status=400)
    
    async def handle_list_memories(self, request):
        """Handle list memories request"""
        if not self.load_balancer:
            return web.json_response({"error": "Load balancer not initialized"}, status=503)
        
        try:
            session_id = request.headers.get('X-Session-ID')
            success, response = await self.load_balancer.route_request(
                'GET', '/memories', session_id=session_id
            )
            
            status_code = 200 if success else 500
            return web.json_response(response, status=status_code)
            
        except Exception as e:
            logger.error(f"Error listing memories: {e}")
            return web.json_response({"error": str(e)}, status=500)
    
    async def handle_get_memory(self, request):
        """Handle get memory by ID request"""
        if not self.load_balancer:
            return web.json_response({"error": "Load balancer not initialized"}, status=503)
        
        try:
            memory_id = request.match_info['memory_id']
            session_id = request.headers.get('X-Session-ID')
            
            success, response = await self.load_balancer.route_request(
                'GET', f'/memories/{memory_id}', session_id=session_id
            )
            
            status_code = 200 if success else 404
            return web.json_response(response, status=status_code)
            
        except Exception as e:
            logger.error(f"Error getting memory: {e}")
            return web.json_response({"error": str(e)}, status=500)
    
    async def handle_search(self, request):
        """Handle search request"""
        if not self.load_balancer:
            return web.json_response({"error": "Load balancer not initialized"}, status=503)
        
        try:
            query_data = await request.json()
            session_id = request.headers.get('X-Session-ID')
            
            success, response = await self.load_balancer.search_memories(query_data, session_id)
            status_code = 200 if success else 500
            
            return web.json_response(response, status=status_code)
            
        except Exception as e:
            logger.error(f"Error searching memories: {e}")
            return web.json_response({"error": str(e)}, status=400)
    
    async def handle_add_association(self, request):
        """Handle add association request"""
        if not self.load_balancer:
            return web.json_response({"error": "Load balancer not initialized"}, status=503)
        
        try:
            association_data = await request.json()
            session_id = request.headers.get('X-Session-ID')
            
            success, response = await self.load_balancer.route_request(
                'POST', '/associations', json=association_data, session_id=session_id
            )
            
            status_code = 200 if success else 500
            return web.json_response(response, status=status_code)
            
        except Exception as e:
            logger.error(f"Error adding association: {e}")
            return web.json_response({"error": str(e)}, status=400)
    
    async def handle_admin_instances(self, request):
        """Handle admin instances endpoint"""
        if not self.load_balancer:
            return web.json_response({"error": "Load balancer not initialized"}, status=503)
        
        health_status = await self.load_balancer.health_check()
        return web.json_response({
            "instances": health_status.get("instances", {}),
            "summary": {
                "total": health_status.get("total_instances", 0),
                "healthy": health_status.get("healthy_instances", 0),
                "current_strategy": health_status.get("strategy", "unknown")
            }
        })
    
    async def handle_drain_instance(self, request):
        """Handle drain instance request"""
        instance_id = request.match_info['instance_id']
        
        if not self.load_balancer or instance_id not in self.load_balancer.instances:
            return web.json_response({"error": "Instance not found"}, status=404)
        
        # Mark instance as draining
        instance = self.load_balancer.instances[instance_id]
        from load_balancer import InstanceStatus
        instance.status = InstanceStatus.DRAINING
        
        logger.info(f"Instance {instance_id} marked for draining")
        
        return web.json_response({
            "message": f"Instance {instance_id} marked for draining",
            "instance_id": instance_id,
            "status": "draining"
        })
    
    async def handle_undrain_instance(self, request):
        """Handle undrain instance request"""
        instance_id = request.match_info['instance_id']
        
        if not self.load_balancer or instance_id not in self.load_balancer.instances:
            return web.json_response({"error": "Instance not found"}, status=404)
        
        # Mark instance as healthy (will be verified by health check)
        instance = self.load_balancer.instances[instance_id]
        from load_balancer import InstanceStatus
        instance.status = InstanceStatus.HEALTHY
        instance.health_check_failures = 0
        
        logger.info(f"Instance {instance_id} unmarked from draining")
        
        return web.json_response({
            "message": f"Instance {instance_id} unmarked from draining",
            "instance_id": instance_id,
            "status": "healthy"
        })
    
    async def init_app(self):
        """Initialize the application"""
        await self.init_load_balancer()
        return self.app
    
    async def cleanup(self):
        """Cleanup resources"""
        if self.load_balancer:
            await self.load_balancer.stop()


async def create_app():
    """Application factory"""
    server = LoadBalancerServer()
    app = await server.init_app()
    
    # Store server reference for cleanup
    app['server'] = server
    
    return app


async def cleanup_handler(app):
    """Cleanup handler for graceful shutdown"""
    server = app.get('server')
    if server:
        await server.cleanup()


def main():
    """Main entry point"""
    logger.info("Starting MFN Load Balancer Server...")
    
    # Get configuration
    host = os.getenv('HOST', '0.0.0.0')
    port = int(os.getenv('PORT', '8080'))
    
    # Create and run app
    async def init():
        app = await create_app()
        app.on_cleanup.append(cleanup_handler)
        return app
    
    web.run_app(init(), host=host, port=port)


if __name__ == '__main__':
    main()