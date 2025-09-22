#!/usr/bin/env python3
"""
MFN Test Orchestration & Validation System
===========================================
Master test orchestrator that coordinates all test suites, analyzes results,
and produces comprehensive validation reports for external verification.

Features:
- Execute all test suites with different configurations
- Real-time progress tracking and monitoring
- Comprehensive results analysis and validation
- Performance claim verification
- Visual charts and graphs generation
- Export capabilities for external validation
- Docker container support for isolated testing
"""

import os
import sys
import json
import time
import subprocess
import argparse
import logging
import psutil
import numpy as np
import pandas as pd
import sqlite3
import threading
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Any, Optional, Tuple
from dataclasses import dataclass, asdict
from concurrent.futures import ThreadPoolExecutor, ProcessPoolExecutor
import matplotlib.pyplot as plt
import seaborn as sns
from tabulate import tabulate

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

@dataclass
class TestSuiteConfig:
    """Configuration for a test suite run"""
    name: str
    script_path: str
    parameters: Dict[str, Any]
    timeout_seconds: int
    priority: int
    dependencies: List[str]

@dataclass
class TestRunResult:
    """Result from a test suite execution"""
    suite_name: str
    start_time: datetime
    end_time: datetime
    duration_seconds: float
    status: str  # 'success', 'failed', 'timeout', 'skipped'
    exit_code: int
    stdout: str
    stderr: str
    metrics: Dict[str, Any]
    validation_status: Dict[str, bool]

@dataclass
class PerformanceClaim:
    """Performance claim to validate"""
    name: str
    description: str
    target_value: float
    unit: str
    test_suite: str
    validation_function: str

class SystemEnvironmentValidator:
    """Validates and prepares the test environment"""

    def __init__(self):
        self.validation_results = {}

    def validate_environment(self) -> Tuple[bool, Dict[str, Any]]:
        """Validate the test environment is ready"""
        logger.info("Validating test environment...")

        checks = {
            "python_version": self._check_python_version(),
            "required_packages": self._check_required_packages(),
            "system_resources": self._check_system_resources(),
            "port_availability": self._check_port_availability(),
            "file_permissions": self._check_file_permissions(),
            "database_access": self._check_database_access()
        }

        all_passed = all(checks.values())
        return all_passed, checks

    def _check_python_version(self) -> bool:
        """Check Python version is 3.8+"""
        version = sys.version_info
        return version.major == 3 and version.minor >= 8

    def _check_required_packages(self) -> bool:
        """Check all required packages are installed"""
        required = [
            'numpy', 'pandas', 'matplotlib', 'seaborn',
            'requests', 'aiohttp', 'psutil', 'tabulate'
        ]

        missing = []
        for package in required:
            try:
                __import__(package)
            except ImportError:
                missing.append(package)

        if missing:
            logger.warning(f"Missing packages: {missing}")
            return False
        return True

    def _check_system_resources(self) -> bool:
        """Check system has adequate resources"""
        memory = psutil.virtual_memory()
        disk = psutil.disk_usage('/')

        # Require at least 4GB RAM and 10GB disk
        has_memory = memory.available >= 4 * 1024 * 1024 * 1024
        has_disk = disk.free >= 10 * 1024 * 1024 * 1024

        return has_memory and has_disk

    def _check_port_availability(self) -> bool:
        """Check required ports are available"""
        required_ports = [8080, 8081, 8082]  # Layer 1, 2, 3

        for port in required_ports:
            sock = None
            try:
                import socket
                sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                result = sock.connect_ex(('localhost', port))
                if result != 0:
                    logger.warning(f"Port {port} is not available")
                    return False
            finally:
                if sock:
                    sock.close()

        return True

    def _check_file_permissions(self) -> bool:
        """Check we can write to necessary directories"""
        test_dirs = ['./results', './logs', './reports', './exports']

        for dir_path in test_dirs:
            Path(dir_path).mkdir(exist_ok=True)
            test_file = Path(dir_path) / '.test_write'
            try:
                test_file.write_text('test')
                test_file.unlink()
            except Exception as e:
                logger.error(f"Cannot write to {dir_path}: {e}")
                return False

        return True

    def _check_database_access(self) -> bool:
        """Check database connectivity"""
        try:
            conn = sqlite3.connect('./data/test_results.db')
            cursor = conn.cursor()
            cursor.execute("SELECT 1")
            conn.close()
            return True
        except Exception as e:
            logger.warning(f"Database access issue: {e}")
            return True  # Non-critical

class TestSuiteOrchestrator:
    """Orchestrates test suite execution"""

    def __init__(self):
        self.test_suites = self._initialize_test_suites()
        self.results = []
        self.start_time = None
        self.end_time = None

    def _initialize_test_suites(self) -> List[TestSuiteConfig]:
        """Initialize all test suite configurations"""
        return [
            TestSuiteConfig(
                name="Stress Test Framework",
                script_path="stress_test_framework.py",
                parameters={
                    "--test-type": "all",
                    "--duration": 60,
                    "--max-qps": 1000
                },
                timeout_seconds=300,
                priority=1,
                dependencies=[]
            ),
            TestSuiteConfig(
                name="Comprehensive Test System",
                script_path="comprehensive_test_system.py",
                parameters={
                    "--mode": "full",
                    "--memory-count": 10000,
                    "--iterations": 100
                },
                timeout_seconds=600,
                priority=2,
                dependencies=[]
            ),
            TestSuiteConfig(
                name="Performance Benchmark Suite",
                script_path="performance_benchmark_suite.py",
                parameters={
                    "--benchmark": "all",
                    "--compare": "true",
                    "--export": "true"
                },
                timeout_seconds=900,
                priority=3,
                dependencies=[]
            ),
            TestSuiteConfig(
                name="Layer 1 Zig Tests",
                script_path="layer1-zig/run_tests.sh",
                parameters={},
                timeout_seconds=120,
                priority=4,
                dependencies=["Stress Test Framework"]
            ),
            TestSuiteConfig(
                name="Layer 2 Rust Tests",
                script_path="layer2-rust/run_tests.sh",
                parameters={},
                timeout_seconds=180,
                priority=5,
                dependencies=["Layer 1 Zig Tests"]
            ),
            TestSuiteConfig(
                name="Layer 3 Go Tests",
                script_path="layer3-go-alm/run_tests.sh",
                parameters={},
                timeout_seconds=240,
                priority=6,
                dependencies=["Layer 2 Rust Tests"]
            ),
            TestSuiteConfig(
                name="Accuracy Validation",
                script_path="test_accuracy.py",
                parameters={
                    "--threshold": 0.94,
                    "--samples": 1000
                },
                timeout_seconds=300,
                priority=7,
                dependencies=[]
            ),
            TestSuiteConfig(
                name="Memory Functionality",
                script_path="test_memory_functionality.py",
                parameters={},
                timeout_seconds=120,
                priority=8,
                dependencies=[]
            ),
            TestSuiteConfig(
                name="Socket Performance",
                script_path="unix_socket_performance_test.py",
                parameters={
                    "--iterations": 1000,
                    "--payload-size": 1024
                },
                timeout_seconds=180,
                priority=9,
                dependencies=[]
            ),
            TestSuiteConfig(
                name="1000 QPS Validation",
                script_path="comprehensive_1000qps_test.py",
                parameters={
                    "--duration": 60,
                    "--target-qps": 1000
                },
                timeout_seconds=120,
                priority=10,
                dependencies=["Performance Benchmark Suite"]
            )
        ]

    def execute_suite(self, suite: TestSuiteConfig, progress_callback=None) -> TestRunResult:
        """Execute a single test suite"""
        logger.info(f"Executing test suite: {suite.name}")

        if progress_callback:
            progress_callback(f"Starting {suite.name}...")

        start_time = datetime.now()

        # Build command
        cmd = [sys.executable if suite.script_path.endswith('.py') else 'bash']
        cmd.append(suite.script_path)

        for param, value in suite.parameters.items():
            cmd.extend([param, str(value)])

        try:
            # Execute with timeout
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=suite.timeout_seconds
            )

            status = 'success' if result.returncode == 0 else 'failed'
            exit_code = result.returncode
            stdout = result.stdout
            stderr = result.stderr

        except subprocess.TimeoutExpired:
            status = 'timeout'
            exit_code = -1
            stdout = f"Test timed out after {suite.timeout_seconds} seconds"
            stderr = ""
        except Exception as e:
            status = 'failed'
            exit_code = -1
            stdout = ""
            stderr = str(e)

        end_time = datetime.now()
        duration = (end_time - start_time).total_seconds()

        # Parse metrics from output
        metrics = self._parse_metrics(stdout)

        # Validate results
        validation = self._validate_results(suite.name, metrics)

        return TestRunResult(
            suite_name=suite.name,
            start_time=start_time,
            end_time=end_time,
            duration_seconds=duration,
            status=status,
            exit_code=exit_code,
            stdout=stdout,
            stderr=stderr,
            metrics=metrics,
            validation_status=validation
        )

    def _parse_metrics(self, output: str) -> Dict[str, Any]:
        """Parse metrics from test output"""
        metrics = {}

        # Parse common metrics patterns
        patterns = {
            'response_time_ms': r'response[_\s]time[:\s]+([\d.]+)\s*ms',
            'throughput_qps': r'throughput[:\s]+([\d.]+)\s*(?:qps|ops/s)',
            'accuracy': r'accuracy[:\s]+([\d.]+)%?',
            'memory_usage_mb': r'memory[_\s]usage[:\s]+([\d.]+)\s*MB',
            'success_rate': r'success[_\s]rate[:\s]+([\d.]+)%?'
        }

        import re
        for metric_name, pattern in patterns.items():
            match = re.search(pattern, output, re.IGNORECASE)
            if match:
                try:
                    metrics[metric_name] = float(match.group(1))
                except:
                    pass

        return metrics

    def _validate_results(self, suite_name: str, metrics: Dict[str, Any]) -> Dict[str, bool]:
        """Validate test results against performance claims"""
        validation = {}

        # Define validation rules per suite
        rules = {
            "Stress Test Framework": {
                "high_throughput": metrics.get('throughput_qps', 0) >= 1000,
                "low_error_rate": metrics.get('error_rate', 1.0) <= 0.01
            },
            "Performance Benchmark Suite": {
                "layer1_performance": metrics.get('layer1_response_ms', 1.0) < 0.1,
                "layer2_performance": metrics.get('layer2_response_ms', 10.0) < 5.0,
                "layer3_performance": metrics.get('layer3_response_ms', 30.0) < 20.0
            },
            "Accuracy Validation": {
                "meets_accuracy": metrics.get('accuracy', 0) >= 94.0
            }
        }

        if suite_name in rules:
            validation = rules[suite_name]

        return validation

    def run_all_tests(self, parallel=False, progress_callback=None) -> List[TestRunResult]:
        """Run all test suites"""
        self.start_time = datetime.now()
        logger.info(f"Starting test orchestration at {self.start_time}")

        if parallel:
            with ThreadPoolExecutor(max_workers=3) as executor:
                futures = []
                for suite in self.test_suites:
                    future = executor.submit(self.execute_suite, suite, progress_callback)
                    futures.append(future)

                self.results = [f.result() for f in futures]
        else:
            for i, suite in enumerate(self.test_suites):
                if progress_callback:
                    progress_callback(f"Running test {i+1}/{len(self.test_suites)}: {suite.name}")

                result = self.execute_suite(suite, progress_callback)
                self.results.append(result)

        self.end_time = datetime.now()
        logger.info(f"Test orchestration completed at {self.end_time}")

        return self.results

class ResultsAnalyzer:
    """Analyzes test results and generates reports"""

    def __init__(self, results: List[TestRunResult]):
        self.results = results
        self.performance_claims = self._define_performance_claims()

    def _define_performance_claims(self) -> List[PerformanceClaim]:
        """Define performance claims to validate"""
        return [
            PerformanceClaim(
                name="Layer 1 Exact Match",
                description="Exact hash matching in <0.1ms",
                target_value=0.1,
                unit="ms",
                test_suite="Performance Benchmark Suite",
                validation_function="layer1_performance"
            ),
            PerformanceClaim(
                name="Layer 2 Similarity",
                description="Semantic similarity in <5ms",
                target_value=5.0,
                unit="ms",
                test_suite="Performance Benchmark Suite",
                validation_function="layer2_performance"
            ),
            PerformanceClaim(
                name="Layer 3 Association",
                description="Multi-hop associations in <20ms",
                target_value=20.0,
                unit="ms",
                test_suite="Performance Benchmark Suite",
                validation_function="layer3_performance"
            ),
            PerformanceClaim(
                name="Throughput",
                description="Sustained 1000+ queries per second",
                target_value=1000,
                unit="qps",
                test_suite="1000 QPS Validation",
                validation_function="throughput"
            ),
            PerformanceClaim(
                name="Accuracy",
                description="94%+ accuracy across configurations",
                target_value=94.0,
                unit="%",
                test_suite="Accuracy Validation",
                validation_function="accuracy"
            ),
            PerformanceClaim(
                name="Memory Capacity",
                description="Support for 50M+ memories",
                target_value=50000000,
                unit="memories",
                test_suite="Stress Test Framework",
                validation_function="memory_capacity"
            )
        ]

    def analyze_results(self) -> Dict[str, Any]:
        """Perform comprehensive analysis of test results"""
        analysis = {
            "summary": self._generate_summary(),
            "performance_validation": self._validate_performance_claims(),
            "detailed_metrics": self._extract_detailed_metrics(),
            "failure_analysis": self._analyze_failures(),
            "recommendations": self._generate_recommendations()
        }

        return analysis

    def _generate_summary(self) -> Dict[str, Any]:
        """Generate overall test summary"""
        total = len(self.results)
        successful = sum(1 for r in self.results if r.status == 'success')
        failed = sum(1 for r in self.results if r.status == 'failed')
        timeout = sum(1 for r in self.results if r.status == 'timeout')

        total_duration = sum(r.duration_seconds for r in self.results)

        return {
            "total_tests": total,
            "successful": successful,
            "failed": failed,
            "timeout": timeout,
            "success_rate": (successful / total * 100) if total > 0 else 0,
            "total_duration_seconds": total_duration,
            "average_duration_seconds": total_duration / total if total > 0 else 0
        }

    def _validate_performance_claims(self) -> Dict[str, Dict[str, Any]]:
        """Validate each performance claim"""
        validation_results = {}

        for claim in self.performance_claims:
            # Find relevant test result
            relevant_results = [r for r in self.results if r.suite_name == claim.test_suite]

            if relevant_results:
                result = relevant_results[0]

                # Extract actual value from metrics
                actual_value = None
                if claim.validation_function in result.metrics:
                    actual_value = result.metrics[claim.validation_function]

                passed = False
                if actual_value is not None:
                    if claim.unit in ['ms', 's']:
                        passed = actual_value <= claim.target_value
                    else:
                        passed = actual_value >= claim.target_value

                validation_results[claim.name] = {
                    "description": claim.description,
                    "target": f"{claim.target_value} {claim.unit}",
                    "actual": f"{actual_value} {claim.unit}" if actual_value else "Not measured",
                    "passed": passed,
                    "test_suite": claim.test_suite
                }
            else:
                validation_results[claim.name] = {
                    "description": claim.description,
                    "target": f"{claim.target_value} {claim.unit}",
                    "actual": "Test not run",
                    "passed": False,
                    "test_suite": claim.test_suite
                }

        return validation_results

    def _extract_detailed_metrics(self) -> Dict[str, Any]:
        """Extract detailed metrics from all tests"""
        metrics = {}

        for result in self.results:
            metrics[result.suite_name] = {
                "status": result.status,
                "duration": result.duration_seconds,
                "metrics": result.metrics,
                "validation": result.validation_status
            }

        return metrics

    def _analyze_failures(self) -> List[Dict[str, Any]]:
        """Analyze test failures"""
        failures = []

        for result in self.results:
            if result.status != 'success':
                failures.append({
                    "suite": result.suite_name,
                    "status": result.status,
                    "error": result.stderr[:500] if result.stderr else "No error message",
                    "duration": result.duration_seconds
                })

        return failures

    def _generate_recommendations(self) -> List[str]:
        """Generate recommendations based on results"""
        recommendations = []

        # Check success rate
        summary = self._generate_summary()
        if summary['success_rate'] < 90:
            recommendations.append("Success rate below 90% - investigate failing tests")

        # Check performance claims
        claims = self._validate_performance_claims()
        failed_claims = [k for k, v in claims.items() if not v['passed']]
        if failed_claims:
            recommendations.append(f"Performance claims not met: {', '.join(failed_claims)}")

        # Check for timeouts
        if summary['timeout'] > 0:
            recommendations.append(f"{summary['timeout']} tests timed out - consider increasing timeout or optimizing tests")

        return recommendations

class ReportGenerator:
    """Generates comprehensive test reports"""

    def __init__(self, analysis: Dict[str, Any], results: List[TestRunResult]):
        self.analysis = analysis
        self.results = results
        self.timestamp = datetime.now()

    def generate_text_report(self) -> str:
        """Generate detailed text report"""
        report = []
        report.append("=" * 80)
        report.append("MFN SYSTEM TEST VALIDATION REPORT")
        report.append(f"Generated: {self.timestamp.isoformat()}")
        report.append("=" * 80)
        report.append("")

        # Summary
        report.append("TEST EXECUTION SUMMARY")
        report.append("-" * 40)
        summary = self.analysis['summary']
        report.append(f"Total Tests: {summary['total_tests']}")
        report.append(f"Successful: {summary['successful']} ({summary['success_rate']:.1f}%)")
        report.append(f"Failed: {summary['failed']}")
        report.append(f"Timeout: {summary['timeout']}")
        report.append(f"Total Duration: {summary['total_duration_seconds']:.2f} seconds")
        report.append("")

        # Performance Claims Validation
        report.append("PERFORMANCE CLAIMS VALIDATION")
        report.append("-" * 40)

        claims_data = []
        for claim_name, claim_result in self.analysis['performance_validation'].items():
            status = "✓ PASS" if claim_result['passed'] else "✗ FAIL"
            claims_data.append([
                claim_name,
                claim_result['target'],
                claim_result['actual'],
                status
            ])

        report.append(tabulate(
            claims_data,
            headers=["Claim", "Target", "Actual", "Status"],
            tablefmt="grid"
        ))
        report.append("")

        # Detailed Test Results
        report.append("DETAILED TEST RESULTS")
        report.append("-" * 40)

        for result in self.results:
            report.append(f"\n{result.suite_name}")
            report.append(f"  Status: {result.status.upper()}")
            report.append(f"  Duration: {result.duration_seconds:.2f}s")

            if result.metrics:
                report.append("  Metrics:")
                for metric, value in result.metrics.items():
                    report.append(f"    - {metric}: {value}")

            if result.validation_status:
                report.append("  Validation:")
                for check, passed in result.validation_status.items():
                    status = "✓" if passed else "✗"
                    report.append(f"    {status} {check}")

        # Failures
        if self.analysis['failure_analysis']:
            report.append("\nFAILURE ANALYSIS")
            report.append("-" * 40)
            for failure in self.analysis['failure_analysis']:
                report.append(f"\n{failure['suite']}: {failure['status']}")
                report.append(f"  Error: {failure['error']}")

        # Recommendations
        if self.analysis['recommendations']:
            report.append("\nRECOMMENDATIONS")
            report.append("-" * 40)
            for rec in self.analysis['recommendations']:
                report.append(f"• {rec}")

        report.append("\n" + "=" * 80)
        report.append("END OF REPORT")
        report.append("=" * 80)

        return "\n".join(report)

    def generate_json_report(self) -> Dict[str, Any]:
        """Generate JSON report for external processing"""
        return {
            "metadata": {
                "timestamp": self.timestamp.isoformat(),
                "version": "1.0.0",
                "system": "MFN Test Orchestrator"
            },
            "summary": self.analysis['summary'],
            "performance_validation": self.analysis['performance_validation'],
            "test_results": [asdict(r) for r in self.results],
            "detailed_metrics": self.analysis['detailed_metrics'],
            "failures": self.analysis['failure_analysis'],
            "recommendations": self.analysis['recommendations']
        }

    def generate_html_report(self) -> str:
        """Generate HTML report with charts"""
        html = []
        html.append("""
        <!DOCTYPE html>
        <html>
        <head>
            <title>MFN Test Validation Report</title>
            <style>
                body { font-family: Arial, sans-serif; margin: 20px; }
                h1 { color: #333; border-bottom: 3px solid #007bff; }
                h2 { color: #555; margin-top: 30px; }
                table { border-collapse: collapse; width: 100%; margin: 20px 0; }
                th, td { border: 1px solid #ddd; padding: 12px; text-align: left; }
                th { background-color: #f2f2f2; }
                .pass { color: green; font-weight: bold; }
                .fail { color: red; font-weight: bold; }
                .summary-box { background: #f8f9fa; padding: 15px; border-radius: 5px; margin: 20px 0; }
                .metric { display: inline-block; margin: 10px 20px; }
                .chart-container { margin: 30px 0; }
            </style>
        </head>
        <body>
        """)

        html.append(f"<h1>MFN System Test Validation Report</h1>")
        html.append(f"<p>Generated: {self.timestamp.strftime('%Y-%m-%d %H:%M:%S')}</p>")

        # Summary box
        summary = self.analysis['summary']
        html.append('<div class="summary-box">')
        html.append('<h2>Executive Summary</h2>')
        html.append(f'<div class="metric">Total Tests: <strong>{summary["total_tests"]}</strong></div>')
        html.append(f'<div class="metric">Success Rate: <strong>{summary["success_rate"]:.1f}%</strong></div>')
        html.append(f'<div class="metric">Duration: <strong>{summary["total_duration_seconds"]:.1f}s</strong></div>')
        html.append('</div>')

        # Performance claims table
        html.append('<h2>Performance Claims Validation</h2>')
        html.append('<table>')
        html.append('<tr><th>Claim</th><th>Target</th><th>Actual</th><th>Status</th></tr>')

        for claim_name, result in self.analysis['performance_validation'].items():
            status_class = 'pass' if result['passed'] else 'fail'
            status_text = 'PASS' if result['passed'] else 'FAIL'
            html.append(f'''
                <tr>
                    <td>{claim_name}</td>
                    <td>{result["target"]}</td>
                    <td>{result["actual"]}</td>
                    <td class="{status_class}">{status_text}</td>
                </tr>
            ''')

        html.append('</table>')

        # Test results
        html.append('<h2>Test Suite Results</h2>')
        html.append('<table>')
        html.append('<tr><th>Test Suite</th><th>Status</th><th>Duration</th><th>Key Metrics</th></tr>')

        for result in self.results:
            status_class = 'pass' if result.status == 'success' else 'fail'
            metrics_str = ', '.join([f"{k}: {v}" for k, v in list(result.metrics.items())[:3]])
            html.append(f'''
                <tr>
                    <td>{result.suite_name}</td>
                    <td class="{status_class}">{result.status.upper()}</td>
                    <td>{result.duration_seconds:.2f}s</td>
                    <td>{metrics_str}</td>
                </tr>
            ''')

        html.append('</table>')

        # Recommendations
        if self.analysis['recommendations']:
            html.append('<h2>Recommendations</h2>')
            html.append('<ul>')
            for rec in self.analysis['recommendations']:
                html.append(f'<li>{rec}</li>')
            html.append('</ul>')

        html.append('</body></html>')

        return '\n'.join(html)

    def save_reports(self, output_dir: str = "./reports"):
        """Save all report formats"""
        output_path = Path(output_dir)
        output_path.mkdir(exist_ok=True)

        timestamp_str = self.timestamp.strftime('%Y%m%d_%H%M%S')

        # Save text report
        text_report = self.generate_text_report()
        text_path = output_path / f"test_report_{timestamp_str}.txt"
        text_path.write_text(text_report)
        logger.info(f"Text report saved to {text_path}")

        # Save JSON report
        json_report = self.generate_json_report()
        json_path = output_path / f"test_report_{timestamp_str}.json"
        with open(json_path, 'w') as f:
            json.dump(json_report, f, indent=2, default=str)
        logger.info(f"JSON report saved to {json_path}")

        # Save HTML report
        html_report = self.generate_html_report()
        html_path = output_path / f"test_report_{timestamp_str}.html"
        html_path.write_text(html_report)
        logger.info(f"HTML report saved to {html_path}")

        return {
            "text": str(text_path),
            "json": str(json_path),
            "html": str(html_path)
        }

class PerformanceVisualizer:
    """Creates performance visualization charts"""

    def __init__(self, analysis: Dict[str, Any], results: List[TestRunResult]):
        self.analysis = analysis
        self.results = results
        sns.set_style("whitegrid")

    def create_performance_charts(self, output_dir: str = "./reports/charts"):
        """Create all performance charts"""
        output_path = Path(output_dir)
        output_path.mkdir(parents=True, exist_ok=True)

        charts = []

        # Success rate pie chart
        chart_path = self._create_success_rate_chart(output_path)
        charts.append(chart_path)

        # Response time comparison
        chart_path = self._create_response_time_chart(output_path)
        charts.append(chart_path)

        # Performance claims validation
        chart_path = self._create_claims_validation_chart(output_path)
        charts.append(chart_path)

        # Test duration timeline
        chart_path = self._create_duration_timeline(output_path)
        charts.append(chart_path)

        return charts

    def _create_success_rate_chart(self, output_path: Path) -> str:
        """Create success rate pie chart"""
        summary = self.analysis['summary']

        fig, ax = plt.subplots(figsize=(8, 6))

        labels = ['Successful', 'Failed', 'Timeout']
        sizes = [summary['successful'], summary['failed'], summary['timeout']]
        colors = ['#28a745', '#dc3545', '#ffc107']

        ax.pie(sizes, labels=labels, colors=colors, autopct='%1.1f%%', startangle=90)
        ax.set_title('Test Execution Results')

        chart_path = output_path / 'success_rate.png'
        plt.savefig(chart_path, dpi=100, bbox_inches='tight')
        plt.close()

        return str(chart_path)

    def _create_response_time_chart(self, output_path: Path) -> str:
        """Create response time comparison chart"""
        # Extract response times from results
        response_times = {}
        for result in self.results:
            if 'response_time_ms' in result.metrics:
                response_times[result.suite_name] = result.metrics['response_time_ms']

        if not response_times:
            return ""

        fig, ax = plt.subplots(figsize=(10, 6))

        names = list(response_times.keys())
        times = list(response_times.values())

        bars = ax.bar(names, times)

        # Color bars based on performance
        for i, (name, time) in enumerate(zip(names, times)):
            if 'Layer 1' in name and time < 0.1:
                bars[i].set_color('#28a745')
            elif 'Layer 2' in name and time < 5:
                bars[i].set_color('#28a745')
            elif 'Layer 3' in name and time < 20:
                bars[i].set_color('#28a745')
            else:
                bars[i].set_color('#dc3545')

        ax.set_xlabel('Test Suite')
        ax.set_ylabel('Response Time (ms)')
        ax.set_title('Response Time Performance')
        plt.xticks(rotation=45, ha='right')

        # Add target lines
        ax.axhline(y=0.1, color='g', linestyle='--', alpha=0.5, label='Layer 1 Target (<0.1ms)')
        ax.axhline(y=5, color='b', linestyle='--', alpha=0.5, label='Layer 2 Target (<5ms)')
        ax.axhline(y=20, color='r', linestyle='--', alpha=0.5, label='Layer 3 Target (<20ms)')
        ax.legend()

        chart_path = output_path / 'response_times.png'
        plt.savefig(chart_path, dpi=100, bbox_inches='tight')
        plt.close()

        return str(chart_path)

    def _create_claims_validation_chart(self, output_path: Path) -> str:
        """Create performance claims validation chart"""
        claims = self.analysis['performance_validation']

        fig, ax = plt.subplots(figsize=(12, 6))

        claim_names = list(claims.keys())
        passed = [1 if claims[c]['passed'] else 0 for c in claim_names]

        colors = ['#28a745' if p else '#dc3545' for p in passed]
        bars = ax.bar(claim_names, passed, color=colors)

        ax.set_ylim([0, 1.2])
        ax.set_xlabel('Performance Claim')
        ax.set_ylabel('Validation Status')
        ax.set_title('Performance Claims Validation Results')
        ax.set_yticks([0, 1])
        ax.set_yticklabels(['Failed', 'Passed'])
        plt.xticks(rotation=45, ha='right')

        # Add actual vs target annotations
        for i, claim_name in enumerate(claim_names):
            claim = claims[claim_name]
            annotation = f"Target: {claim['target']}\nActual: {claim['actual']}"
            ax.annotate(annotation, xy=(i, passed[i]),
                       xytext=(0, 10), textcoords='offset points',
                       ha='center', fontsize=8)

        chart_path = output_path / 'claims_validation.png'
        plt.savefig(chart_path, dpi=100, bbox_inches='tight')
        plt.close()

        return str(chart_path)

    def _create_duration_timeline(self, output_path: Path) -> str:
        """Create test duration timeline"""
        fig, ax = plt.subplots(figsize=(12, 8))

        # Sort results by start time
        sorted_results = sorted(self.results, key=lambda x: x.start_time)

        for i, result in enumerate(sorted_results):
            color = '#28a745' if result.status == 'success' else '#dc3545'
            ax.barh(i, result.duration_seconds, left=0, height=0.8,
                   color=color, alpha=0.7, label=result.suite_name)

            # Add test name
            ax.text(-1, i, result.suite_name, va='center', ha='right', fontsize=9)

            # Add duration
            ax.text(result.duration_seconds + 1, i, f'{result.duration_seconds:.1f}s',
                   va='center', fontsize=8)

        ax.set_xlabel('Duration (seconds)')
        ax.set_title('Test Execution Timeline')
        ax.set_yticks([])
        ax.grid(axis='x', alpha=0.3)

        chart_path = output_path / 'duration_timeline.png'
        plt.savefig(chart_path, dpi=100, bbox_inches='tight')
        plt.close()

        return str(chart_path)

class DockerEnvironmentBuilder:
    """Builds Docker environment for isolated testing"""

    def create_dockerfile(self) -> str:
        """Create Dockerfile for test environment"""
        dockerfile = """
FROM python:3.9-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \\
    build-essential \\
    git \\
    curl \\
    wget \\
    && rm -rf /var/lib/apt/lists/*

# Install Zig
RUN wget https://ziglang.org/download/0.11.0/zig-linux-x86_64-0.11.0.tar.xz \\
    && tar -xf zig-linux-x86_64-0.11.0.tar.xz \\
    && mv zig-linux-x86_64-0.11.0 /opt/zig \\
    && ln -s /opt/zig/zig /usr/local/bin/zig

# Install Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install Go
RUN wget https://go.dev/dl/go1.21.0.linux-amd64.tar.gz \\
    && tar -C /usr/local -xzf go1.21.0.linux-amd64.tar.gz
ENV PATH="/usr/local/go/bin:${PATH}"

# Create app directory
WORKDIR /app

# Copy requirements
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy application code
COPY . .

# Build layers
RUN cd layer1-zig && zig build-exe main.zig
RUN cd layer2-rust && cargo build --release
RUN cd layer3-go-alm && go build

# Expose ports
EXPOSE 8080 8081 8082

# Run test orchestrator by default
CMD ["python", "test_orchestrator.py", "--mode", "docker"]
"""
        return dockerfile

    def create_docker_compose(self) -> str:
        """Create docker-compose.yml for multi-container testing"""
        compose = """
version: '3.8'

services:
  layer1:
    build:
      context: .
      dockerfile: Dockerfile.layer1
    ports:
      - "8080:8080"
    volumes:
      - ./data:/app/data
    command: ./layer1-zig/layer1

  layer2:
    build:
      context: .
      dockerfile: Dockerfile.layer2
    ports:
      - "8081:8081"
    volumes:
      - ./data:/app/data
    command: ./layer2-rust/target/release/layer2
    depends_on:
      - layer1

  layer3:
    build:
      context: .
      dockerfile: Dockerfile.layer3
    ports:
      - "8082:8082"
    volumes:
      - ./data:/app/data
    command: ./layer3-go-alm/layer3
    depends_on:
      - layer2

  orchestrator:
    build:
      context: .
      dockerfile: Dockerfile
    volumes:
      - ./results:/app/results
      - ./reports:/app/reports
    depends_on:
      - layer1
      - layer2
      - layer3
    command: python test_orchestrator.py --run-all --export

volumes:
  data:
  results:
  reports:
"""
        return compose

    def save_docker_files(self, output_dir: str = "."):
        """Save Docker configuration files"""
        output_path = Path(output_dir)

        # Save Dockerfile
        dockerfile_path = output_path / "Dockerfile.test"
        dockerfile_path.write_text(self.create_dockerfile())

        # Save docker-compose.yml
        compose_path = output_path / "docker-compose.test.yml"
        compose_path.write_text(self.create_docker_compose())

        logger.info(f"Docker files saved: {dockerfile_path}, {compose_path}")

        return {
            "dockerfile": str(dockerfile_path),
            "compose": str(compose_path)
        }

def main():
    """Main orchestrator entry point"""
    parser = argparse.ArgumentParser(description="MFN Test Orchestration System")

    parser.add_argument("--run-all", action="store_true",
                       help="Run all test suites")
    parser.add_argument("--suite", type=str,
                       help="Run specific test suite")
    parser.add_argument("--parallel", action="store_true",
                       help="Run tests in parallel")
    parser.add_argument("--validate-only", action="store_true",
                       help="Only validate environment")
    parser.add_argument("--export", action="store_true",
                       help="Export results for external validation")
    parser.add_argument("--docker", action="store_true",
                       help="Generate Docker test environment")
    parser.add_argument("--mode", choices=["quick", "standard", "comprehensive", "docker"],
                       default="standard", help="Test mode")
    parser.add_argument("--output-dir", type=str, default="./reports",
                       help="Output directory for reports")

    args = parser.parse_args()

    # Validate environment
    validator = SystemEnvironmentValidator()
    env_valid, env_checks = validator.validate_environment()

    if not env_valid and not args.validate_only:
        logger.error("Environment validation failed:")
        for check, passed in env_checks.items():
            status = "✓" if passed else "✗"
            logger.info(f"  {status} {check}")

        if not args.docker:
            return 1

    if args.validate_only:
        print("\nEnvironment Validation Results:")
        for check, passed in env_checks.items():
            status = "✓ PASS" if passed else "✗ FAIL"
            print(f"  {status}: {check}")
        return 0

    # Generate Docker environment if requested
    if args.docker:
        builder = DockerEnvironmentBuilder()
        docker_files = builder.save_docker_files()
        print("\nDocker environment created:")
        print(f"  Dockerfile: {docker_files['dockerfile']}")
        print(f"  Compose file: {docker_files['compose']}")
        print("\nTo run tests in Docker:")
        print("  docker-compose -f docker-compose.test.yml up")
        return 0

    # Initialize orchestrator
    orchestrator = TestSuiteOrchestrator()

    # Progress tracking
    def progress_callback(message):
        print(f"[{datetime.now().strftime('%H:%M:%S')}] {message}")

    # Run tests
    if args.run_all or args.mode in ["standard", "comprehensive"]:
        logger.info("Starting comprehensive test execution...")
        results = orchestrator.run_all_tests(
            parallel=args.parallel,
            progress_callback=progress_callback
        )
    elif args.suite:
        # Run specific suite
        suite_config = next((s for s in orchestrator.test_suites if s.name == args.suite), None)
        if suite_config:
            results = [orchestrator.execute_suite(suite_config, progress_callback)]
        else:
            logger.error(f"Suite '{args.suite}' not found")
            return 1
    else:
        logger.info("Running quick validation tests...")
        # Run quick subset
        quick_suites = orchestrator.test_suites[:3]
        results = []
        for suite in quick_suites:
            results.append(orchestrator.execute_suite(suite, progress_callback))

    # Analyze results
    analyzer = ResultsAnalyzer(results)
    analysis = analyzer.analyze_results()

    # Generate reports
    reporter = ReportGenerator(analysis, results)
    report_paths = reporter.save_reports(args.output_dir)

    # Create visualizations
    visualizer = PerformanceVisualizer(analysis, results)
    chart_paths = visualizer.create_performance_charts(f"{args.output_dir}/charts")

    # Print summary
    print("\n" + "=" * 80)
    print("TEST ORCHESTRATION COMPLETE")
    print("=" * 80)
    print(reporter.generate_text_report())

    # Export for external validation if requested
    if args.export:
        export_dir = Path(args.output_dir) / "export"
        export_dir.mkdir(exist_ok=True)

        # Create validation package
        validation_package = {
            "timestamp": datetime.now().isoformat(),
            "environment": env_checks,
            "results": analysis,
            "reports": report_paths,
            "charts": chart_paths,
            "validation_instructions": """
To validate these results independently:

1. Environment Setup:
   - Python 3.8+ with required packages
   - Zig 0.11+, Rust 1.70+, Go 1.21+
   - 4GB+ RAM, 10GB+ disk space

2. Run Validation:
   python test_orchestrator.py --run-all --export

3. Compare Results:
   - Check performance claims in reports/test_report_*.json
   - Verify all claims show "passed": true
   - Review charts in reports/charts/

4. Docker Validation:
   docker-compose -f docker-compose.test.yml up
            """
        }

        export_path = export_dir / f"validation_package_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
        with open(export_path, 'w') as f:
            json.dump(validation_package, f, indent=2, default=str)

        print(f"\nValidation package exported to: {export_path}")

    # Return exit code based on results
    if analysis['summary']['success_rate'] >= 90:
        return 0
    else:
        return 1

if __name__ == "__main__":
    sys.exit(main())