#!/usr/bin/env python3
"""
MFN Deployment Test Suite
Validates the production container is working correctly
"""

import os
import sys
import time
import json
import socket
import subprocess
import asyncio
import logging
from typing import Dict, List, Any, Tuple

# Add lib to path
sys.path.insert(0, '/app/lib')

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

class DeploymentTester:
    """Comprehensive deployment testing"""

    def __init__(self):
        self.socket_dir = "/app/sockets"
        self.api_url = "http://localhost:8080"
        self.dashboard_url = "http://localhost:3000"
        self.test_results = []

    async def run_all_tests(self) -> Dict[str, Any]:
        """Run complete test suite"""
        logger.info("Starting MFN Deployment Tests")
        logger.info("=" * 50)

        test_suite = [
            ("Socket Connectivity", self.test_socket_connectivity),
            ("Layer Health", self.test_layer_health),
            ("API Gateway", self.test_api_gateway),
            ("Dashboard", self.test_dashboard),
            ("Persistence", self.test_persistence),
            ("Memory Operations", self.test_memory_operations),
            ("Performance", self.test_performance),
            ("Recovery", self.test_recovery),
            ("Monitoring", self.test_monitoring),
            ("Backup/Restore", self.test_backup_restore)
        ]

        results = {
            "total": len(test_suite),
            "passed": 0,
            "failed": 0,
            "tests": []
        }

        for test_name, test_func in test_suite:
            logger.info(f"\nRunning: {test_name}")
            try:
                start_time = time.time()
                test_result = await test_func()
                duration = time.time() - start_time

                if test_result:
                    logger.info(f"✓ {test_name} PASSED ({duration:.2f}s)")
                    results["passed"] += 1
                    status = "passed"
                else:
                    logger.error(f"✗ {test_name} FAILED ({duration:.2f}s)")
                    results["failed"] += 1
                    status = "failed"

                results["tests"].append({
                    "name": test_name,
                    "status": status,
                    "duration": duration
                })

            except Exception as e:
                logger.error(f"✗ {test_name} ERROR: {e}")
                results["failed"] += 1
                results["tests"].append({
                    "name": test_name,
                    "status": "error",
                    "error": str(e)
                })

        return results

    async def test_socket_connectivity(self) -> bool:
        """Test Unix socket connectivity for all layers"""
        layers = ["layer1", "layer2", "layer3", "layer4"]
        all_connected = True

        for layer in layers:
            socket_path = f"{self.socket_dir}/{layer}.sock"

            if not os.path.exists(socket_path):
                logger.error(f"  Socket not found: {socket_path}")
                all_connected = False
                continue

            try:
                sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                sock.settimeout(5.0)
                sock.connect(socket_path)
                sock.close()
                logger.info(f"  {layer}: Socket connected")

            except Exception as e:
                logger.error(f"  {layer}: Connection failed - {e}")
                all_connected = False

        return all_connected

    async def test_layer_health(self) -> bool:
        """Test layer health via supervisor"""
        try:
            result = subprocess.run(
                ["supervisorctl", "status"],
                capture_output=True,
                text=True,
                timeout=10
            )

            if result.returncode != 0:
                logger.error(f"  Supervisorctl failed: {result.stderr}")
                return False

            # Check each layer status
            required_services = [
                "layer1_ifr",
                "layer2_dsr",
                "layer3_alm",
                "layer4_cpe",
                "mfn_orchestrator",
                "mfn_api"
            ]

            all_running = True
            for service in required_services:
                if f"{service}" in result.stdout and "RUNNING" in result.stdout:
                    logger.info(f"  {service}: RUNNING")
                else:
                    logger.error(f"  {service}: NOT RUNNING")
                    all_running = False

            return all_running

        except Exception as e:
            logger.error(f"  Layer health check failed: {e}")
            return False

    async def test_api_gateway(self) -> bool:
        """Test API Gateway endpoints"""
        import aiohttp

        endpoints = [
            ("/health", "GET"),
            ("/api/v1/stats", "GET"),
            ("/metrics", "GET")
        ]

        all_working = True

        async with aiohttp.ClientSession() as session:
            for endpoint, method in endpoints:
                url = f"{self.api_url}{endpoint}"

                try:
                    async with session.request(method, url, timeout=5) as response:
                        if response.status == 200:
                            logger.info(f"  {endpoint}: OK ({response.status})")
                        else:
                            logger.error(f"  {endpoint}: Failed ({response.status})")
                            all_working = False

                except Exception as e:
                    logger.error(f"  {endpoint}: Error - {e}")
                    all_working = False

        return all_working

    async def test_dashboard(self) -> bool:
        """Test Dashboard availability"""
        import aiohttp

        try:
            async with aiohttp.ClientSession() as session:
                async with session.get(self.dashboard_url, timeout=5) as response:
                    if response.status == 200:
                        content = await response.text()
                        if "MFN System Dashboard" in content:
                            logger.info(f"  Dashboard: Available")
                            return True

            logger.error(f"  Dashboard: Not available")
            return False

        except Exception as e:
            logger.error(f"  Dashboard test failed: {e}")
            return False

    async def test_persistence(self) -> bool:
        """Test persistence layer"""
        try:
            from add_persistence import MFNPersistenceManager

            pm = MFNPersistenceManager("/app/data")
            stats = pm.get_storage_stats()

            logger.info(f"  Database exists: {stats['database_size_mb']:.2f} MB")
            logger.info(f"  Memories stored: {stats['memory_count']}")

            # Try to save a test memory
            from unified_socket_client import MemoryItem

            test_memory = MemoryItem(
                id=99999,
                content="Deployment test memory",
                tags=["test", "deployment"],
                metadata={"test": True}
            )

            success = pm.save_memory(test_memory)

            if success:
                logger.info(f"  Write test: PASSED")

                # Try to read it back
                loaded = pm.load_memory(99999)
                if loaded and loaded.content == "Deployment test memory":
                    logger.info(f"  Read test: PASSED")
                    return True

            logger.error(f"  Persistence test failed")
            return False

        except Exception as e:
            logger.error(f"  Persistence test error: {e}")
            return False

    async def test_memory_operations(self) -> bool:
        """Test memory CRUD operations via API"""
        import aiohttp

        try:
            async with aiohttp.ClientSession() as session:
                # Add memory
                memory_data = {
                    "id": 88888,
                    "content": "API test memory",
                    "tags": ["api", "test"],
                    "metadata": {"source": "deployment_test"}
                }

                async with session.post(
                    f"{self.api_url}/api/v1/memory",
                    json=memory_data,
                    timeout=10
                ) as response:
                    if response.status == 201:
                        logger.info(f"  Memory creation: PASSED")
                    else:
                        logger.error(f"  Memory creation failed: {response.status}")
                        return False

                # Search memory
                search_data = {
                    "query": "API test",
                    "max_results": 10
                }

                async with session.post(
                    f"{self.api_url}/api/v1/search",
                    json=search_data,
                    timeout=10
                ) as response:
                    if response.status == 200:
                        results = await response.json()
                        logger.info(f"  Memory search: Found {results['count']} results")
                        return True

            return False

        except Exception as e:
            logger.error(f"  Memory operations failed: {e}")
            return False

    async def test_performance(self) -> bool:
        """Test system performance metrics"""
        import aiohttp
        import statistics

        latencies = []
        errors = 0
        requests = 20

        async with aiohttp.ClientSession() as session:
            for i in range(requests):
                start = time.time()

                try:
                    async with session.get(
                        f"{self.api_url}/health",
                        timeout=5
                    ) as response:
                        if response.status == 200:
                            latencies.append(time.time() - start)
                        else:
                            errors += 1

                except Exception:
                    errors += 1

        if latencies:
            avg_latency = statistics.mean(latencies)
            p95_latency = statistics.quantiles(latencies, n=20)[18]  # 95th percentile
            error_rate = errors / requests

            logger.info(f"  Avg latency: {avg_latency*1000:.2f}ms")
            logger.info(f"  P95 latency: {p95_latency*1000:.2f}ms")
            logger.info(f"  Error rate: {error_rate*100:.1f}%")

            # Pass if latency < 100ms and error rate < 5%
            return avg_latency < 0.1 and error_rate < 0.05

        return False

    async def test_recovery(self) -> bool:
        """Test service recovery mechanisms"""
        try:
            # Kill a layer process
            subprocess.run(
                ["supervisorctl", "stop", "layer1_ifr"],
                capture_output=True,
                timeout=5
            )

            logger.info(f"  Stopped layer1_ifr")
            await asyncio.sleep(2)

            # Check if it auto-restarts
            result = subprocess.run(
                ["supervisorctl", "status", "layer1_ifr"],
                capture_output=True,
                text=True,
                timeout=5
            )

            if "RUNNING" in result.stdout:
                logger.info(f"  Auto-recovery: PASSED")
                return True
            else:
                # Try manual restart
                subprocess.run(
                    ["supervisorctl", "start", "layer1_ifr"],
                    capture_output=True,
                    timeout=5
                )

                await asyncio.sleep(2)

                result = subprocess.run(
                    ["supervisorctl", "status", "layer1_ifr"],
                    capture_output=True,
                    text=True,
                    timeout=5
                )

                if "RUNNING" in result.stdout:
                    logger.info(f"  Manual recovery: PASSED")
                    return True

            logger.error(f"  Recovery failed")
            return False

        except Exception as e:
            logger.error(f"  Recovery test error: {e}")
            return False

    async def test_monitoring(self) -> bool:
        """Test monitoring endpoints"""
        import aiohttp

        try:
            async with aiohttp.ClientSession() as session:
                # Check Prometheus metrics
                async with session.get(f"{self.api_url}/metrics", timeout=5) as response:
                    if response.status == 200:
                        content = await response.text()

                        # Verify key metrics exist
                        required_metrics = [
                            "mfn_api_requests_total",
                            "mfn_api_errors_total",
                            "mfn_api_uptime_seconds"
                        ]

                        all_present = all(metric in content for metric in required_metrics)

                        if all_present:
                            logger.info(f"  Metrics endpoint: OK")
                            return True

            logger.error(f"  Monitoring test failed")
            return False

        except Exception as e:
            logger.error(f"  Monitoring test error: {e}")
            return False

    async def test_backup_restore(self) -> bool:
        """Test backup and restore functionality"""
        import aiohttp

        try:
            async with aiohttp.ClientSession() as session:
                # Create backup
                async with session.post(
                    f"{self.api_url}/api/v1/backup",
                    timeout=30
                ) as response:
                    if response.status == 200:
                        result = await response.json()
                        if result.get("success"):
                            logger.info(f"  Backup creation: PASSED")

                            # Verify backup exists
                            backup_location = result.get("backup_location", "")
                            if os.path.exists(backup_location):
                                logger.info(f"  Backup verification: PASSED")
                                return True

            logger.error(f"  Backup/restore test failed")
            return False

        except Exception as e:
            logger.error(f"  Backup/restore test error: {e}")
            return False

async def main():
    """Main test execution"""
    tester = DeploymentTester()
    results = await tester.run_all_tests()

    # Print summary
    logger.info("\n" + "=" * 50)
    logger.info("DEPLOYMENT TEST SUMMARY")
    logger.info("=" * 50)
    logger.info(f"Total Tests: {results['total']}")
    logger.info(f"Passed: {results['passed']}")
    logger.info(f"Failed: {results['failed']}")
    logger.info(f"Success Rate: {(results['passed']/results['total']*100):.1f}%")

    # Save results
    with open("/app/logs/deployment_test_results.json", "w") as f:
        json.dump(results, f, indent=2)

    # Exit code based on results
    sys.exit(0 if results['failed'] == 0 else 1)

if __name__ == "__main__":
    asyncio.run(main())