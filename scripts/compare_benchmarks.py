#!/usr/bin/env python3
"""
Benchmark Comparison Script
===========================
Compares current benchmark results against a baseline to detect
performance regressions or improvements.
"""

import json
import argparse
from pathlib import Path
from typing import Dict, Any, List, Tuple
import sys

class BenchmarkComparator:
    """Compare benchmark results and detect regressions"""

    def __init__(self, baseline: Dict[str, Any], current: Dict[str, Any]):
        self.baseline = baseline
        self.current = current
        self.regressions = []
        self.improvements = []
        self.unchanged = []

    def compare_metrics(self) -> Dict[str, Any]:
        """Compare all metrics between baseline and current"""

        comparison = {
            "summary": {},
            "layer_performance": {},
            "throughput": {},
            "capacity": {},
            "regressions": [],
            "improvements": [],
            "analysis": {}
        }

        # Compare layer performance
        comparison["layer_performance"] = self._compare_layer_performance()

        # Compare throughput
        comparison["throughput"] = self._compare_throughput()

        # Compare capacity
        comparison["capacity"] = self._compare_capacity()

        # Determine regressions and improvements
        self._analyze_changes(comparison)

        # Generate summary
        comparison["summary"] = self._generate_summary()

        return comparison

    def _compare_layer_performance(self) -> Dict[str, Any]:
        """Compare layer latency metrics"""

        layer_comparison = {}

        # Layer 1
        baseline_l1 = self._extract_layer_metric(self.baseline, "layer1", "average_time_ms")
        current_l1 = self._extract_layer_metric(self.current, "layer1", "average_time_ms")

        if baseline_l1 is not None and current_l1 is not None:
            change_pct = ((current_l1 - baseline_l1) / baseline_l1) * 100
            layer_comparison["layer1"] = {
                "baseline_ms": baseline_l1,
                "current_ms": current_l1,
                "change_ms": current_l1 - baseline_l1,
                "change_percentage": change_pct,
                "status": self._get_status(change_pct, lower_is_better=True)
            }

        # Layer 2
        baseline_l2 = self._extract_layer_metric(self.baseline, "layer2", "average_time_ms")
        current_l2 = self._extract_layer_metric(self.current, "layer2", "average_time_ms")

        if baseline_l2 is not None and current_l2 is not None:
            change_pct = ((current_l2 - baseline_l2) / baseline_l2) * 100
            layer_comparison["layer2"] = {
                "baseline_ms": baseline_l2,
                "current_ms": current_l2,
                "change_ms": current_l2 - baseline_l2,
                "change_percentage": change_pct,
                "status": self._get_status(change_pct, lower_is_better=True)
            }

        # Layer 3
        baseline_l3 = self._extract_layer_metric(self.baseline, "layer3", "average_time_ms")
        current_l3 = self._extract_layer_metric(self.current, "layer3", "average_time_ms")

        if baseline_l3 is not None and current_l3 is not None:
            change_pct = ((current_l3 - baseline_l3) / baseline_l3) * 100
            layer_comparison["layer3"] = {
                "baseline_ms": baseline_l3,
                "current_ms": current_l3,
                "change_ms": current_l3 - baseline_l3,
                "change_percentage": change_pct,
                "status": self._get_status(change_pct, lower_is_better=True)
            }

        return layer_comparison

    def _compare_throughput(self) -> Dict[str, Any]:
        """Compare throughput metrics"""

        throughput_comparison = {}

        baseline_qps = self._extract_throughput_metric(self.baseline, "actual_qps")
        current_qps = self._extract_throughput_metric(self.current, "actual_qps")

        if baseline_qps is not None and current_qps is not None:
            change_pct = ((current_qps - baseline_qps) / baseline_qps) * 100
            throughput_comparison["sustained_qps"] = {
                "baseline": baseline_qps,
                "current": current_qps,
                "change": current_qps - baseline_qps,
                "change_percentage": change_pct,
                "status": self._get_status(change_pct, lower_is_better=False)
            }

        # Compare response times at different QPS levels
        for qps_level in [100, 500, 1000]:
            baseline_rt = self._extract_response_time_at_qps(self.baseline, qps_level)
            current_rt = self._extract_response_time_at_qps(self.current, qps_level)

            if baseline_rt is not None and current_rt is not None:
                change_pct = ((current_rt - baseline_rt) / baseline_rt) * 100
                throughput_comparison[f"response_time_{qps_level}qps"] = {
                    "baseline_ms": baseline_rt,
                    "current_ms": current_rt,
                    "change_ms": current_rt - baseline_rt,
                    "change_percentage": change_pct,
                    "status": self._get_status(change_pct, lower_is_better=True)
                }

        return throughput_comparison

    def _compare_capacity(self) -> Dict[str, Any]:
        """Compare capacity metrics"""

        capacity_comparison = {}

        baseline_cap = self._extract_capacity_metric(self.baseline, "successful_operations")
        current_cap = self._extract_capacity_metric(self.current, "successful_operations")

        if baseline_cap is not None and current_cap is not None:
            change_pct = ((current_cap - baseline_cap) / baseline_cap) * 100 if baseline_cap > 0 else 0
            capacity_comparison["max_capacity"] = {
                "baseline": baseline_cap,
                "current": current_cap,
                "change": current_cap - baseline_cap,
                "change_percentage": change_pct,
                "status": self._get_status(change_pct, lower_is_better=False)
            }

        return capacity_comparison

    def _extract_layer_metric(self, data: Dict, layer: str, metric: str) -> float:
        """Extract layer metric from benchmark data"""
        try:
            results = data.get("benchmark_results", {})
            layer_key = f"{layer}_exact_matching" if layer == "layer1" else f"{layer}_similarity_search" if layer == "layer2" else f"{layer}_associative_search"

            layer_results = results.get(layer_key, {}).get("results", {})

            # Average across all test configurations
            values = []
            for config_results in layer_results.values():
                if metric in config_results:
                    values.append(config_results[metric])

            return sum(values) / len(values) if values else None

        except:
            return None

    def _extract_throughput_metric(self, data: Dict, metric: str) -> float:
        """Extract throughput metric from benchmark data"""
        try:
            results = data.get("benchmark_results", {}).get("throughput_testing", {}).get("results", {})

            values = []
            for qps_results in results.values():
                if metric in qps_results:
                    values.append(qps_results[metric])

            return max(values) if values else None

        except:
            return None

    def _extract_response_time_at_qps(self, data: Dict, qps_level: int) -> float:
        """Extract response time at specific QPS level"""
        try:
            results = data.get("benchmark_results", {}).get("throughput_testing", {}).get("results", {})

            if str(qps_level) in results:
                return results[str(qps_level)].get("average_response_time_ms")

            return None

        except:
            return None

    def _extract_capacity_metric(self, data: Dict, metric: str) -> int:
        """Extract capacity metric from benchmark data"""
        try:
            results = data.get("benchmark_results", {}).get("capacity_testing", {}).get("results", {})

            values = []
            for cap_results in results.values():
                if metric in cap_results:
                    values.append(cap_results[metric])

            return max(values) if values else None

        except:
            return None

    def _get_status(self, change_percentage: float, lower_is_better: bool) -> str:
        """Determine status based on change percentage"""

        if lower_is_better:
            if change_percentage <= -10:
                return "improved"
            elif change_percentage >= 10:
                return "regressed"
            else:
                return "unchanged"
        else:
            if change_percentage >= 10:
                return "improved"
            elif change_percentage <= -10:
                return "regressed"
            else:
                return "unchanged"

    def _analyze_changes(self, comparison: Dict[str, Any]):
        """Analyze changes and categorize as regressions or improvements"""

        # Check layer performance
        for layer, metrics in comparison["layer_performance"].items():
            if metrics["status"] == "regressed":
                self.regressions.append({
                    "component": layer,
                    "metric": "latency",
                    "baseline": metrics["baseline_ms"],
                    "current": metrics["current_ms"],
                    "regression_pct": metrics["change_percentage"]
                })
                comparison["regressions"].append(f"{layer} latency increased by {metrics['change_percentage']:.1f}%")

            elif metrics["status"] == "improved":
                self.improvements.append({
                    "component": layer,
                    "metric": "latency",
                    "baseline": metrics["baseline_ms"],
                    "current": metrics["current_ms"],
                    "improvement_pct": abs(metrics["change_percentage"])
                })
                comparison["improvements"].append(f"{layer} latency improved by {abs(metrics['change_percentage']):.1f}%")

        # Check throughput
        if "sustained_qps" in comparison["throughput"]:
            metrics = comparison["throughput"]["sustained_qps"]
            if metrics["status"] == "regressed":
                self.regressions.append({
                    "component": "throughput",
                    "metric": "qps",
                    "baseline": metrics["baseline"],
                    "current": metrics["current"],
                    "regression_pct": abs(metrics["change_percentage"])
                })
                comparison["regressions"].append(f"Throughput decreased by {abs(metrics['change_percentage']):.1f}%")

            elif metrics["status"] == "improved":
                self.improvements.append({
                    "component": "throughput",
                    "metric": "qps",
                    "baseline": metrics["baseline"],
                    "current": metrics["current"],
                    "improvement_pct": metrics["change_percentage"]
                })
                comparison["improvements"].append(f"Throughput improved by {metrics['change_percentage']:.1f}%")

    def _generate_summary(self) -> Dict[str, Any]:
        """Generate comparison summary"""

        total_regressions = len(self.regressions)
        total_improvements = len(self.improvements)

        # Calculate overall health score
        health_score = 100

        for regression in self.regressions:
            # Penalize based on regression severity
            if regression["regression_pct"] > 50:
                health_score -= 20
            elif regression["regression_pct"] > 20:
                health_score -= 10
            else:
                health_score -= 5

        for improvement in self.improvements:
            # Reward improvements (but less than penalties)
            if improvement["improvement_pct"] > 20:
                health_score += 5
            else:
                health_score += 2

        health_score = max(0, min(100, health_score))

        return {
            "total_regressions": total_regressions,
            "total_improvements": total_improvements,
            "health_score": health_score,
            "recommendation": self._get_recommendation(health_score, total_regressions)
        }

    def _get_recommendation(self, health_score: int, regressions: int) -> str:
        """Generate recommendation based on comparison results"""

        if regressions == 0 and health_score >= 95:
            return "✅ Excellent! No regressions detected. Safe to deploy."
        elif regressions <= 1 and health_score >= 85:
            return "⚠️ Minor regression detected. Review before deployment."
        elif regressions <= 3 and health_score >= 70:
            return "⚠️ Multiple regressions detected. Investigation recommended."
        else:
            return "❌ Significant regressions detected. Do not deploy without fixes."

    def generate_report(self) -> str:
        """Generate human-readable comparison report"""

        comparison = self.compare_metrics()

        report = []
        report.append("="*60)
        report.append("BENCHMARK COMPARISON REPORT")
        report.append("="*60)
        report.append("")

        # Summary
        summary = comparison["summary"]
        report.append("SUMMARY")
        report.append("-"*30)
        report.append(f"Health Score: {summary['health_score']}/100")
        report.append(f"Regressions: {summary['total_regressions']}")
        report.append(f"Improvements: {summary['total_improvements']}")
        report.append(f"Recommendation: {summary['recommendation']}")
        report.append("")

        # Layer Performance
        if comparison["layer_performance"]:
            report.append("LAYER PERFORMANCE")
            report.append("-"*30)

            for layer, metrics in comparison["layer_performance"].items():
                status_emoji = "✅" if metrics["status"] == "improved" else "❌" if metrics["status"] == "regressed" else "➖"
                report.append(
                    f"{status_emoji} {layer.upper()}: "
                    f"{metrics['baseline_ms']:.3f}ms → {metrics['current_ms']:.3f}ms "
                    f"({metrics['change_percentage']:+.1f}%)"
                )

            report.append("")

        # Throughput
        if "sustained_qps" in comparison["throughput"]:
            report.append("THROUGHPUT")
            report.append("-"*30)

            metrics = comparison["throughput"]["sustained_qps"]
            status_emoji = "✅" if metrics["status"] == "improved" else "❌" if metrics["status"] == "regressed" else "➖"
            report.append(
                f"{status_emoji} Sustained QPS: "
                f"{metrics['baseline']:.0f} → {metrics['current']:.0f} "
                f"({metrics['change_percentage']:+.1f}%)"
            )

            report.append("")

        # Regressions
        if comparison["regressions"]:
            report.append("⚠️ REGRESSIONS DETECTED")
            report.append("-"*30)
            for regression in comparison["regressions"]:
                report.append(f"  • {regression}")
            report.append("")

        # Improvements
        if comparison["improvements"]:
            report.append("✅ IMPROVEMENTS")
            report.append("-"*30)
            for improvement in comparison["improvements"]:
                report.append(f"  • {improvement}")
            report.append("")

        report.append("="*60)

        return "\n".join(report)

def main():
    parser = argparse.ArgumentParser(description="Compare benchmark results")
    parser.add_argument(
        "--baseline",
        required=True,
        help="Path to baseline benchmark JSON"
    )
    parser.add_argument(
        "--current",
        required=True,
        help="Path to current benchmark JSON"
    )
    parser.add_argument(
        "--output",
        help="Path to save comparison report JSON"
    )
    parser.add_argument(
        "--fail-on-regression",
        action="store_true",
        help="Exit with error code if regressions detected"
    )
    parser.add_argument(
        "--regression-threshold",
        type=float,
        default=10.0,
        help="Percentage threshold for regression detection (default: 10%)"
    )

    args = parser.parse_args()

    # Load benchmark data
    with open(args.baseline, 'r') as f:
        baseline_data = json.load(f)

    with open(args.current, 'r') as f:
        current_data = json.load(f)

    # Compare benchmarks
    comparator = BenchmarkComparator(baseline_data, current_data)
    comparison = comparator.compare_metrics()

    # Generate and print report
    report = comparator.generate_report()
    print(report)

    # Save comparison if requested
    if args.output:
        with open(args.output, 'w') as f:
            json.dump(comparison, f, indent=2)
        print(f"\nComparison report saved to: {args.output}")

    # Check for regressions
    if args.fail_on_regression and comparison["summary"]["total_regressions"] > 0:
        print("\n❌ Build failed due to performance regressions")
        return 1

    # Check health score
    if comparison["summary"]["health_score"] < 70:
        print("\n⚠️ Warning: Performance health score below 70")
        if args.fail_on_regression:
            return 1

    return 0

if __name__ == "__main__":
    sys.exit(main())