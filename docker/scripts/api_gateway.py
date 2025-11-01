#!/usr/bin/env python3
"""
MFN API Gateway
Production-ready REST API with circuit breakers and monitoring
"""

import os
import sys
import json
import time
import asyncio
import logging
from datetime import datetime
from typing import Dict, List, Any, Optional
from collections import deque
from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException, Request, Response, status
from fastapi.responses import JSONResponse
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
import uvicorn

# Add lib to path
sys.path.insert(0, '/app/lib')

from unified_socket_client import UnifiedMFNClient, MemoryItem
from add_persistence import MFNPersistenceManager

# Configure logging
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# Pydantic models
class MemoryRequest(BaseModel):
    id: int
    content: str
    tags: List[str] = Field(default_factory=list)
    metadata: Dict[str, Any] = Field(default_factory=dict)
    embedding: Optional[List[float]] = None

class SearchRequest(BaseModel):
    query: str
    max_results: int = Field(default=10, ge=1, le=100)
    layers: Optional[List[str]] = None

class HealthResponse(BaseModel):
    status: str
    timestamp: str
    uptime_seconds: float
    layers: Dict[str, Dict[str, Any]]
    metrics: Dict[str, Any]

# Circuit Breaker implementation
class CircuitBreaker:
    def __init__(self, failure_threshold: int = 5, recovery_timeout: int = 60):
        self.failure_threshold = failure_threshold
        self.recovery_timeout = recovery_timeout
        self.failure_count = 0
        self.last_failure_time = None
        self.state = "closed"  # closed, open, half_open

    async def call(self, func, *args, **kwargs):
        if self.state == "open":
            if self.last_failure_time and \
               (time.time() - self.last_failure_time) > self.recovery_timeout:
                self.state = "half_open"
            else:
                raise HTTPException(
                    status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                    detail="Service temporarily unavailable (circuit open)"
                )

        try:
            result = await func(*args, **kwargs)
            if self.state == "half_open":
                self.state = "closed"
                self.failure_count = 0
            return result

        except Exception as e:
            self.failure_count += 1
            self.last_failure_time = time.time()

            if self.failure_count >= self.failure_threshold:
                self.state = "open"
                logger.error(f"Circuit breaker opened after {self.failure_count} failures")

            raise e

# Rate limiter
class RateLimiter:
    def __init__(self, max_requests: int = 100, window_seconds: int = 60):
        self.max_requests = max_requests
        self.window_seconds = window_seconds
        self.requests = deque()

    async def check_rate_limit(self, client_id: str) -> bool:
        now = time.time()

        # Remove old requests outside window
        while self.requests and self.requests[0][0] < now - self.window_seconds:
            self.requests.popleft()

        # Count requests from this client
        client_requests = sum(1 for _, cid in self.requests if cid == client_id)

        if client_requests >= self.max_requests:
            return False

        self.requests.append((now, client_id))
        return True

# Application state
class AppState:
    def __init__(self):
        self.mfn_client = None
        self.persistence = None
        self.circuit_breakers = {}
        self.rate_limiter = RateLimiter()
        self.start_time = time.time()
        self.request_count = 0
        self.error_count = 0

    async def initialize(self):
        """Initialize MFN connections"""
        try:
            self.mfn_client = UnifiedMFNClient()
            self.persistence = MFNPersistenceManager(
                data_dir=os.environ.get('MFN_DATA_DIR', '/app/data')
            )

            # Initialize circuit breakers for each layer
            for layer in ['layer1', 'layer2', 'layer3', 'layer4']:
                self.circuit_breakers[layer] = CircuitBreaker()

            logger.info("MFN API Gateway initialized successfully")

        except Exception as e:
            logger.error(f"Failed to initialize MFN client: {e}")
            raise

    async def cleanup(self):
        """Cleanup resources"""
        if self.mfn_client:
            # Cleanup client connections
            pass

# Create app with lifespan
@asynccontextmanager
async def lifespan(app: FastAPI):
    # Startup
    await app.state.app_state.initialize()
    yield
    # Shutdown
    await app.state.app_state.cleanup()

# Create FastAPI app
app = FastAPI(
    title="MFN API Gateway",
    description="Memory Flow Network Production API",
    version="1.0.0",
    lifespan=lifespan
)

# Add CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Initialize app state
app.state.app_state = AppState()

# Middleware for request tracking
@app.middleware("http")
async def track_requests(request: Request, call_next):
    start_time = time.time()

    # Rate limiting
    client_id = request.client.host if request.client else "unknown"
    if not await app.state.app_state.rate_limiter.check_rate_limit(client_id):
        return JSONResponse(
            status_code=status.HTTP_429_TOO_MANY_REQUESTS,
            content={"detail": "Rate limit exceeded"}
        )

    # Track request
    app.state.app_state.request_count += 1

    try:
        response = await call_next(request)

        # Add response headers
        process_time = time.time() - start_time
        response.headers["X-Process-Time"] = str(process_time)
        response.headers["X-Request-ID"] = str(app.state.app_state.request_count)

        return response

    except Exception as e:
        app.state.app_state.error_count += 1
        logger.error(f"Request failed: {e}")
        return JSONResponse(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            content={"detail": "Internal server error"}
        )

# API Routes
@app.get("/health", response_model=HealthResponse)
async def health_check():
    """System health check endpoint"""
    try:
        state = app.state.app_state
        uptime = time.time() - state.start_time

        # Check layer statuses
        layer_statuses = {}
        if state.mfn_client:
            stats = state.mfn_client.get_system_stats()
            for layer_name, layer_stats in stats.items():
                layer_statuses[layer_name] = {
                    "connected": "error" not in layer_stats,
                    "memory_count": layer_stats.get("memory_count", 0),
                    "circuit_state": state.circuit_breakers.get(
                        layer_name, CircuitBreaker()
                    ).state
                }

        metrics = {
            "requests_total": state.request_count,
            "errors_total": state.error_count,
            "error_rate": state.error_count / max(1, state.request_count),
            "uptime_seconds": uptime
        }

        return HealthResponse(
            status="healthy" if state.mfn_client else "unhealthy",
            timestamp=datetime.now().isoformat(),
            uptime_seconds=uptime,
            layers=layer_statuses,
            metrics=metrics
        )

    except Exception as e:
        logger.error(f"Health check failed: {e}")
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail=str(e)
        )

@app.post("/api/v1/memory", status_code=status.HTTP_201_CREATED)
async def add_memory(memory: MemoryRequest):
    """Add new memory to the system"""
    try:
        state = app.state.app_state

        if not state.mfn_client:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail="MFN client not initialized"
            )

        # Create memory item
        memory_item = MemoryItem(
            id=memory.id,
            content=memory.content,
            tags=memory.tags,
            metadata=memory.metadata
        )

        # Add to system with circuit breaker
        async def add_with_retry():
            return await asyncio.to_thread(
                state.mfn_client.add_memory,
                memory_item,
                memory.embedding
            )

        results = await state.circuit_breakers['layer1'].call(add_with_retry)

        # Save to persistence
        if state.persistence:
            await asyncio.to_thread(
                state.persistence.save_memory,
                memory_item,
                memory.embedding
            )

        return {
            "success": True,
            "memory_id": memory.id,
            "layer_results": results,
            "persisted": True
        }

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Failed to add memory: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to add memory: {str(e)}"
        )

@app.post("/api/v1/search")
async def search_memories(search: SearchRequest):
    """Search for memories across layers"""
    try:
        state = app.state.app_state

        if not state.mfn_client:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail="MFN client not initialized"
            )

        # Perform search
        async def search_with_retry():
            return await asyncio.to_thread(
                state.mfn_client.unified_search,
                search.query,
                search.max_results
            )

        results = await state.circuit_breakers['layer1'].call(search_with_retry)

        # Format results
        formatted_results = []
        for result in results:
            formatted_results.append({
                "memory_id": result.memory_id,
                "layer": result.layer,
                "confidence": result.confidence,
                "content": result.content,
                "metadata": result.metadata
            })

        return {
            "query": search.query,
            "results": formatted_results,
            "count": len(formatted_results)
        }

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Search failed: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Search failed: {str(e)}"
        )

@app.get("/api/v1/memory/{memory_id}")
async def get_memory(memory_id: int):
    """Retrieve specific memory"""
    try:
        state = app.state.app_state

        if not state.persistence:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail="Persistence not initialized"
            )

        memory = await asyncio.to_thread(
            state.persistence.load_memory,
            memory_id
        )

        if not memory:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail=f"Memory {memory_id} not found"
            )

        return {
            "id": memory.id,
            "content": memory.content,
            "tags": memory.tags,
            "metadata": memory.metadata,
            "created_at": memory.created_at,
            "updated_at": memory.updated_at
        }

    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Failed to retrieve memory: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to retrieve memory: {str(e)}"
        )

@app.get("/api/v1/stats")
async def get_statistics():
    """Get system statistics"""
    try:
        state = app.state.app_state

        stats = {
            "api_metrics": {
                "requests_total": state.request_count,
                "errors_total": state.error_count,
                "error_rate": state.error_count / max(1, state.request_count),
                "uptime_seconds": time.time() - state.start_time
            }
        }

        if state.mfn_client:
            stats["layer_stats"] = state.mfn_client.get_system_stats()

        if state.persistence:
            stats["storage_stats"] = await asyncio.to_thread(
                state.persistence.get_storage_stats
            )

        return stats

    except Exception as e:
        logger.error(f"Failed to get statistics: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Failed to get statistics: {str(e)}"
        )

@app.post("/api/v1/backup")
async def create_backup():
    """Create system backup"""
    try:
        state = app.state.app_state

        if not state.persistence:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail="Persistence not initialized"
            )

        backup_dir = await asyncio.to_thread(
            state.persistence.create_backup
        )

        return {
            "success": True,
            "backup_location": backup_dir,
            "timestamp": datetime.now().isoformat()
        }

    except Exception as e:
        logger.error(f"Backup failed: {e}")
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Backup failed: {str(e)}"
        )

# Metrics endpoint for Prometheus
@app.get("/metrics")
async def metrics():
    """Prometheus-compatible metrics endpoint"""
    state = app.state.app_state
    uptime = time.time() - state.start_time

    metrics_text = f"""# HELP mfn_api_requests_total Total number of API requests
# TYPE mfn_api_requests_total counter
mfn_api_requests_total {state.request_count}

# HELP mfn_api_errors_total Total number of API errors
# TYPE mfn_api_errors_total counter
mfn_api_errors_total {state.error_count}

# HELP mfn_api_uptime_seconds API uptime in seconds
# TYPE mfn_api_uptime_seconds gauge
mfn_api_uptime_seconds {uptime}
"""

    return Response(content=metrics_text, media_type="text/plain")

if __name__ == "__main__":
    # Run the API server
    uvicorn.run(
        app,
        host="0.0.0.0",
        port=int(os.environ.get("MFN_API_PORT", 8080)),
        log_level="info",
        access_log=True
    )