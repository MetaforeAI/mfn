#!/usr/bin/env python3
"""
Comprehensive 1000+ QPS Validation Test
=====================================
Complete validation framework to test and verify that the MFN system
achieves the target 1000+ queries per second throughput.

This test validates:
1. Single instance performance optimizations
2. Horizontal scaling with load balancer  
3. End-to-end system throughput
4. Performance under sustained load
5. System reliability and error rates
"""

import asyncio
import aiohttp
import time
import json
import logging
import statistics
import os
import sys
import subprocess
from typing import Dict, List, Any, Tuple
from dataclasses import dataclass, asdict
from concurrent.futures import ThreadPoolExecutor
import matplotlib.pyplot as plt
import numpy as np

# Add the MFN system path
_project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))
sys.path.append(_project_root)
sys.path.append(os.path.join(_project_root, 'horizontal_scaling'))

from optimized_mfn_client import OptimizedMFNClient, HighThroughputLoadTester
from load_balancer import MFNLoadBalancer, LoadBalancingStrategy, LoadBalancerConfig

logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

@dataclass
class TestResult:
    """Test result data structure"""
    test_name: str
    target_qps: int
    achieved_qps: float
    success_rate: float
    average_response_time_ms: float
    p95_response_time_ms: float
    p99_response_time_ms: float
    total_requests: int
    successful_requests: int
    failed_requests: int
    test_duration_seconds: float
    meets_target: bool
    error_details: List[str]
    additional_metrics: Dict[str, Any]

class ComprehensiveQPSValidator:
    """Comprehensive QPS validation framework"""
    
    def __init__(self):
        self.results = []
        self.test_configurations = [
            # Phase 1: Single instance optimizations
            {
                'name': 'single_instance_baseline',
                'description': 'Single MFN instance baseline performance',
                'target_qps': 100,
                'test_duration': 30,
                'warmup_duration': 5,
                'instances': ['http://localhost:8082'],
                'use_load_balancer': False
            },
            {
                'name': 'single_instance_optimized',
                'description': 'Single MFN instance with optimizations',
                'target_qps': 300,
                'test_duration': 60,
                'warmup_duration': 10,
                'instances': ['http://localhost:8082'],
                'use_load_balancer': False
            },
            # Phase 2: Horizontal scaling
            {
                'name': 'dual_instance_scaling',
                'description': 'Two MFN instances with load balancing',
                'target_qps': 600,
                'test_duration': 60,
                'warmup_duration': 10,
                'instances': ['http://localhost:8082', 'http://localhost:8083'],
                'use_load_balancer': True
            },
            {
                'name': 'quad_instance_scaling',
                'description': 'Four MFN instances with load balancing',
                'target_qps': 1000,
                'test_duration': 120,
                'warmup_duration': 15,
                'instances': [
                    'http://localhost:8082',
                    'http://localhost:8083', 
                    'http://localhost:8084',
                    'http://localhost:8085'
                ],
                'use_load_balancer': True
            },
            # Phase 3: Stress testing
            {
                'name': 'stress_test_1500qps',
                'description': 'Stress test at 1500 QPS',
                'target_qps': 1500,
                'test_duration': 180,
                'warmup_duration': 20,
                'instances': [
                    'http://localhost:8082',
                    'http://localhost:8083', 
                    'http://localhost:8084',
                    'http://localhost:8085'
                ],
                'use_load_balancer': True
            },
            {
                'name': 'sustained_1000qps_test',
                'description': 'Sustained 1000 QPS for 10 minutes',
                'target_qps': 1000,
                'test_duration': 600,  # 10 minutes
                'warmup_duration': 30,
                'instances': [
                    'http://localhost:8082',
                    'http://localhost:8083', 
                    'http://localhost:8084',
                    'http://localhost:8085'
                ],
                'use_load_balancer': True
            }
        ]
    
    async def validate_system_health(self, instances: List[str]) -> Tuple[bool, List[str]]:
        """Validate that all instances are healthy before testing"""
        logger.info("Validating system health...")
        
        health_issues = []
        all_healthy = True
        
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=10)) as session:
            for instance_url in instances:
                try:
                    async with session.get(f"{instance_url}/health") as response:
                        if response.status == 200:
                            health_data = await response.json()
                            logger.info(f"✅ {instance_url} is healthy")
                        else:
                            health_issues.append(f"{instance_url} returned status {response.status}")
                            all_healthy = False
                except Exception as e:
                    health_issues.append(f"{instance_url} health check failed: {e}")
                    all_healthy = False
        
        if all_healthy:
            logger.info("✅ All instances are healthy")
        else:
            logger.error("❌ Some instances are unhealthy:")
            for issue in health_issues:
                logger.error(f"  - {issue}")
        
        return all_healthy, health_issues
    
    async def setup_load_balancer(self, instances: List[str]) -> Optional[MFNLoadBalancer]:
        """Setup load balancer for multi-instance tests"""
        if len(instances) <= 1:
            return None
        
        logger.info(f"Setting up load balancer for {len(instances)} instances...")
        
        # Configure instances
        lb_instances = [
            {"id": f"mfn-{i+1}", "url": url, "weight": 1}
            for i, url in enumerate(instances)
        ]
        
        # Configure load balancer for optimal performance
        config = LoadBalancerConfig(
            strategy=LoadBalancingStrategy.LEAST_RESPONSE_TIME,
            health_check_interval=5,
            max_connections_per_instance=100,
            enable_sticky_sessions=False,  # Better for load distribution
            connection_timeout=5,
            request_timeout=15
        )
        
        load_balancer = MFNLoadBalancer(lb_instances, config)
        await load_balancer.start()
        
        logger.info("✅ Load balancer configured and started")
        return load_balancer
    
    async def run_throughput_test(self, config: Dict[str, Any]) -> TestResult:
        """Run a single throughput test configuration"""
        
        test_name = config['name']
        target_qps = config['target_qps']
        test_duration = config['test_duration']
        warmup_duration = config['warmup_duration']
        instances = config['instances']
        use_load_balancer = config['use_load_balancer']
        
        logger.info(f"🚀 Starting test: {test_name}")
        logger.info(f"   Target QPS: {target_qps}")
        logger.info(f"   Duration: {test_duration}s (warmup: {warmup_duration}s)")
        logger.info(f"   Instances: {len(instances)}")
        logger.info(f"   Load Balancer: {'Yes' if use_load_balancer else 'No'}")
        
        # Validate system health
        all_healthy, health_issues = await self.validate_system_health(instances)
        if not all_healthy:
            return TestResult(
                test_name=test_name,
                target_qps=target_qps,
                achieved_qps=0.0,
                success_rate=0.0,
                average_response_time_ms=0.0,
                p95_response_time_ms=0.0,
                p99_response_time_ms=0.0,
                total_requests=0,
                successful_requests=0,
                failed_requests=0,
                test_duration_seconds=0.0,
                meets_target=False,
                error_details=health_issues,
                additional_metrics={}
            )
        
        # Setup load balancer if needed
        load_balancer = None
        if use_load_balancer:
            load_balancer = await self.setup_load_balancer(instances)
        
        try:
            # Initialize client
            if use_load_balancer and load_balancer:
                # Use load balancer for testing
                test_url = "http://localhost:8080"  # Load balancer endpoint
                client = OptimizedMFNClient(
                    layer3_url=test_url,
                    max_connections=200,
                    enable_caching=True,
                    cache_size=10000
                )
            else:
                # Use single instance
                client = OptimizedMFNClient(
                    layer3_url=instances[0],
                    max_connections=100,
                    enable_caching=True,
                    cache_size=5000
                )
            
            # Run the test
            load_tester = HighThroughputLoadTester(client)
            
            start_time = time.time()
            result = await load_tester.throughput_test(
                target_qps=target_qps,
                test_duration_seconds=test_duration,
                warmup_seconds=warmup_duration
            )
            total_time = time.time() - start_time
            
            # Get additional metrics
            client_stats = client.get_performance_stats()
            additional_metrics = {
                'client_stats': client_stats,
                'test_configuration': config
            }
            
            if load_balancer:
                lb_stats = load_balancer.get_performance_stats()
                additional_metrics['load_balancer_stats'] = lb_stats
            
            # Create test result
            test_result = TestResult(
                test_name=test_name,
                target_qps=target_qps,
                achieved_qps=result['actual_qps'],
                success_rate=result['success_rate'],
                average_response_time_ms=result['average_response_time_ms'],
                p95_response_time_ms=result['p95_response_time_ms'],
                p99_response_time_ms=result['p99_response_time_ms'],
                total_requests=result['total_queries'],
                successful_requests=result['successful_queries'],
                failed_requests=result['failed_queries'],
                test_duration_seconds=total_time,
                meets_target=result['meets_target'],
                error_details=result.get('errors_sample', []),
                additional_metrics=additional_metrics
            )
            
            await client.close()
            
            # Log results
            self._log_test_result(test_result)
            
            return test_result
            
        finally:
            if load_balancer:
                await load_balancer.stop()
    
    def _log_test_result(self, result: TestResult):
        """Log test result in a formatted way"""
        status_emoji = "✅" if result.meets_target else "❌"
        
        logger.info(f"\n{status_emoji} TEST RESULT: {result.test_name}")
        logger.info(f"   Target QPS: {result.target_qps}")
        logger.info(f"   Achieved QPS: {result.achieved_qps:.2f}")
        logger.info(f"   Achievement Rate: {(result.achieved_qps / result.target_qps * 100):.1f}%")
        logger.info(f"   Success Rate: {result.success_rate:.1%}")
        logger.info(f"   Avg Response Time: {result.average_response_time_ms:.2f}ms")
        logger.info(f"   P95 Response Time: {result.p95_response_time_ms:.2f}ms")
        logger.info(f"   P99 Response Time: {result.p99_response_time_ms:.2f}ms")
        logger.info(f"   Total Requests: {result.total_requests}")
        logger.info(f"   Duration: {result.test_duration_seconds:.2f}s")
        logger.info(f"   Target Met: {'YES' if result.meets_target else 'NO'}")
        
        if result.error_details:
            logger.warning(f"   Errors: {result.error_details[:3]}")
    
    async def run_all_tests(self) -> List[TestResult]:
        """Run all test configurations"""
        logger.info("🔥 Starting Comprehensive 1000+ QPS Validation")
        logger.info("=" * 60)
        
        all_results = []
        
        for i, config in enumerate(self.test_configurations, 1):
            logger.info(f"\n📊 TEST {i}/{len(self.test_configurations)}: {config['description']}")
            logger.info("-" * 60)
            
            try:
                result = await self.run_throughput_test(config)
                all_results.append(result)
                
                # Stop testing if we fail to meet lower QPS targets
                if result.target_qps <= 300 and not result.meets_target:
                    logger.error("❌ Failed basic performance targets. Stopping test suite.")
                    break
                
                # Brief pause between tests
                if i < len(self.test_configurations):
                    logger.info("Pausing 30 seconds between tests...")
                    await asyncio.sleep(30)
                    
            except Exception as e:
                logger.error(f"❌ Test {config['name']} failed with error: {e}")
                error_result = TestResult(
                    test_name=config['name'],
                    target_qps=config['target_qps'],
                    achieved_qps=0.0,
                    success_rate=0.0,
                    average_response_time_ms=0.0,
                    p95_response_time_ms=0.0,
                    p99_response_time_ms=0.0,
                    total_requests=0,
                    successful_requests=0,
                    failed_requests=0,
                    test_duration_seconds=0.0,
                    meets_target=False,
                    error_details=[str(e)],
                    additional_metrics={}
                )
                all_results.append(error_result)
        
        self.results = all_results
        return all_results
    
    def generate_report(self, results: List[TestResult]) -> str:
        """Generate comprehensive test report"""
        
        report = [
            "=" * 80,
            "MFN SYSTEM 1000+ QPS VALIDATION REPORT",
            "=" * 80,
            f"Test Date: {time.strftime('%Y-%m-%d %H:%M:%S')}",
            f"Total Tests: {len(results)}",
            ""
        ]
        
        # Summary statistics
        successful_tests = [r for r in results if r.meets_target]
        max_qps_achieved = max([r.achieved_qps for r in results]) if results else 0
        
        report.extend([
            "SUMMARY:",
            f"  Tests Passed: {len(successful_tests)}/{len(results)}",
            f"  Maximum QPS Achieved: {max_qps_achieved:.2f}",
            f"  1000+ QPS Target Met: {'✅ YES' if max_qps_achieved >= 1000 else '❌ NO'}",
            ""
        ])
        
        # Detailed results
        report.append("DETAILED RESULTS:")
        report.append("-" * 40)
        
        for result in results:
            status = "PASS" if result.meets_target else "FAIL"
            report.extend([
                f"Test: {result.test_name}",
                f"  Status: {status}",
                f"  Target QPS: {result.target_qps}",
                f"  Achieved QPS: {result.achieved_qps:.2f}",
                f"  Success Rate: {result.success_rate:.1%}",
                f"  Avg Response Time: {result.average_response_time_ms:.2f}ms",
                f"  P95 Response Time: {result.p95_response_time_ms:.2f}ms",
                ""
            ])
        
        # Performance analysis
        if successful_tests:
            qps_values = [r.achieved_qps for r in successful_tests]
            response_times = [r.average_response_time_ms for r in successful_tests]
            
            report.extend([
                "PERFORMANCE ANALYSIS:",
                f"  QPS Range: {min(qps_values):.2f} - {max(qps_values):.2f}",
                f"  Average QPS: {statistics.mean(qps_values):.2f}",
                f"  Response Time Range: {min(response_times):.2f}ms - {max(response_times):.2f}ms",
                f"  Average Response Time: {statistics.mean(response_times):.2f}ms",
                ""
            ])
        
        # Recommendations
        report.extend([
            "RECOMMENDATIONS:",
            ""
        ])
        
        if max_qps_achieved >= 1000:
            report.extend([
                "✅ EXCELLENT: 1000+ QPS target achieved!",
                "  - System is production-ready for high-throughput workloads",
                "  - Consider implementing auto-scaling for peak loads",
                "  - Monitor performance metrics in production",
            ])
        elif max_qps_achieved >= 500:
            report.extend([
                "⚠️ GOOD: Significant throughput achieved, but target not met",
                "  - Add more instances or optimize existing ones",
                "  - Review connection pooling and caching strategies",
                "  - Consider upgrading hardware resources",
            ])
        else:
            report.extend([
                "❌ NEEDS IMPROVEMENT: Low throughput achieved",
                "  - Review system architecture and bottlenecks",
                "  - Optimize database and networking configurations", 
                "  - Consider redesigning for better scalability",
            ])
        
        report.extend([
            "",
            "=" * 80,
            "END OF REPORT",
            "=" * 80
        ])
        
        return "\n".join(report)
    
    def save_results(self, results: List[TestResult], filename: str = None):
        """Save results to JSON file"""
        if not filename:
            filename = os.path.join(_project_root, f"qps_validation_results_{int(time.time())}.json")
        
        # Convert results to serializable format
        serializable_results = [asdict(result) for result in results]
        
        with open(filename, 'w') as f:
            json.dump({
                'test_timestamp': time.time(),
                'test_date': time.strftime('%Y-%m-%d %H:%M:%S'),
                'summary': {
                    'total_tests': len(results),
                    'successful_tests': len([r for r in results if r.meets_target]),
                    'max_qps_achieved': max([r.achieved_qps for r in results]) if results else 0,
                    'target_1000qps_met': any(r.achieved_qps >= 1000 and r.meets_target for r in results)
                },
                'results': serializable_results
            }, f, indent=2, default=str)
        
        logger.info(f"📁 Results saved to: {filename}")
        return filename
    
    def create_performance_charts(self, results: List[TestResult]):
        """Create performance visualization charts"""
        if not results:
            return
        
        # QPS Achievement Chart
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(15, 6))
        
        test_names = [r.test_name for r in results]
        target_qps = [r.target_qps for r in results]
        achieved_qps = [r.achieved_qps for r in results]
        
        # QPS comparison
        x = np.arange(len(test_names))
        width = 0.35
        
        ax1.bar(x - width/2, target_qps, width, label='Target QPS', alpha=0.7)
        ax1.bar(x + width/2, achieved_qps, width, label='Achieved QPS', alpha=0.7)
        ax1.set_xlabel('Test Configuration')
        ax1.set_ylabel('Queries Per Second')
        ax1.set_title('QPS Performance Comparison')
        ax1.set_xticks(x)
        ax1.set_xticklabels([name.replace('_', '\n') for name in test_names], rotation=45, ha='right')
        ax1.legend()
        ax1.grid(True, alpha=0.3)
        
        # Response time chart
        response_times = [r.average_response_time_ms for r in results]
        p95_times = [r.p95_response_time_ms for r in results]
        
        ax2.bar(x, response_times, width, label='Average Response Time', alpha=0.7)
        ax2.bar(x, p95_times, width, bottom=response_times, label='P95 Response Time', alpha=0.7)
        ax2.set_xlabel('Test Configuration')
        ax2.set_ylabel('Response Time (ms)')
        ax2.set_title('Response Time Performance')
        ax2.set_xticks(x)
        ax2.set_xticklabels([name.replace('_', '\n') for name in test_names], rotation=45, ha='right')
        ax2.legend()
        ax2.grid(True, alpha=0.3)
        
        plt.tight_layout()
        chart_file = os.path.join(_project_root, f"qps_performance_charts_{int(time.time())}.png")
        plt.savefig(chart_file, dpi=300, bbox_inches='tight')
        logger.info(f"📊 Performance charts saved to: {chart_file}")
        plt.close()

async def main():
    """Main test execution"""
    
    # Initialize validator
    validator = ComprehensiveQPSValidator()
    
    try:
        # Run all validation tests
        logger.info("🚀 Starting comprehensive MFN 1000+ QPS validation...")
        results = await validator.run_all_tests()
        
        # Generate and display report
        report = validator.generate_report(results)
        print("\n" + report)
        
        # Save results
        results_file = validator.save_results(results)
        
        # Create performance charts
        try:
            validator.create_performance_charts(results)
        except Exception as e:
            logger.warning(f"Could not create charts: {e}")
        
        # Final assessment
        max_qps = max([r.achieved_qps for r in results]) if results else 0
        successful_1000qps = any(r.achieved_qps >= 1000 and r.meets_target for r in results)
        
        if successful_1000qps:
            logger.info("🎉 SUCCESS: MFN System achieved 1000+ QPS target!")
            sys.exit(0)
        elif max_qps >= 500:
            logger.warning("⚠️ PARTIAL: Significant performance achieved but 1000 QPS target not met")
            sys.exit(1)
        else:
            logger.error("❌ FAILURE: Low performance, significant optimization needed")
            sys.exit(2)
            
    except Exception as e:
        logger.error(f"❌ Test suite failed: {e}")
        sys.exit(3)

if __name__ == "__main__":
    asyncio.run(main())