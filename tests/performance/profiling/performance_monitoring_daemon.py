#!/usr/bin/env python3
"""
Performance Monitoring Daemon
=============================

Continuous monitoring service for MFN Phase 2 performance validation.
Runs as a background daemon to track performance metrics, detect regressions,
and trigger alerts during the migration process.

Features:
- Real-time performance monitoring
- Automated regression detection
- Alert system with Discord notifications
- Performance trend analysis
- Automated rollback triggers
"""

import asyncio
import json
import time
import logging
import signal
import sys
from datetime import datetime, timedelta
from pathlib import Path
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, asdict
import sqlite3
import threading
import statistics
import psutil

# Import from the main framework
try:
    from mfn_phase2_validation_framework import (
        MFNPhase2ValidationFramework, PerformanceMetrics, TestConfiguration
    )
except ImportError:
    sys.path.append(str(Path(__file__).parent))
    from mfn_phase2_validation_framework import (
        MFNPhase2ValidationFramework, PerformanceMetrics, TestConfiguration
    )


logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler('/tmp/performance_monitoring_daemon.log'),
        logging.StreamHandler()
    ]
)
logger = logging.getLogger(__name__)


@dataclass
class AlertConfig:
    """Alert configuration settings"""
    latency_threshold_ms: float = 1.0  # Alert if > 1ms (vs 0.16ms target)
    qps_threshold: float = 4000  # Alert if < 4000 QPS
    error_rate_threshold: float = 0.05  # Alert if > 5% errors
    success_rate_threshold: float = 0.95  # Alert if < 95% success
    memory_threshold_mb: float = 1000  # Alert if > 1GB memory usage
    cpu_threshold_percent: float = 80  # Alert if > 80% CPU
    trend_window_minutes: int = 30  # Analyze trends over 30 minutes
    alert_cooldown_minutes: int = 5  # Min time between same alert type


@dataclass
class SystemHealth:
    """System health metrics"""
    timestamp: str
    cpu_percent: float
    memory_percent: float
    memory_available_mb: float
    disk_usage_percent: float
    active_connections: int
    unix_sockets_available: bool
    services_responding: Dict[str, bool]


class PerformanceMonitoringDaemon:
    """Main monitoring daemon"""
    
    def __init__(self, config_file: Optional[str] = None):
        self.framework = MFNPhase2ValidationFramework()
        self.alert_config = AlertConfig()
        self.running = False
        self.monitoring_thread = None
        self.health_thread = None
        self.last_alerts = {}  # Track last alert times for cooldown
        
        # Load configuration if provided
        if config_file and Path(config_file).exists():
            self.load_config(config_file)
    
    def load_config(self, config_file: str):
        """Load monitoring configuration from file"""
        try:
            with open(config_file, 'r') as f:
                config_data = json.load(f)
            
            # Update alert configuration
            for key, value in config_data.get('alerts', {}).items():
                if hasattr(self.alert_config, key):
                    setattr(self.alert_config, key, value)
            
            # Update framework thresholds
            thresholds = config_data.get('thresholds', {})
            self.framework.alert_thresholds.update(thresholds)
            
            logger.info(f"Configuration loaded from {config_file}")
            
        except Exception as e:
            logger.error(f"Failed to load configuration: {e}")
    
    def start(self, monitoring_interval: int = 30):
        """Start the monitoring daemon"""
        logger.info(f"🚀 Starting Performance Monitoring Daemon (interval: {monitoring_interval}s)")
        
        self.running = True
        
        # Start monitoring thread
        self.monitoring_thread = threading.Thread(
            target=self._monitoring_loop,
            args=(monitoring_interval,),
            daemon=False
        )
        self.monitoring_thread.start()
        
        # Start system health monitoring thread
        self.health_thread = threading.Thread(
            target=self._health_monitoring_loop,
            daemon=False
        )
        self.health_thread.start()
        
        # Set up signal handlers for graceful shutdown
        signal.signal(signal.SIGINT, self._signal_handler)
        signal.signal(signal.SIGTERM, self._signal_handler)
        
        logger.info("✅ Performance Monitoring Daemon started")
    
    def stop(self):
        """Stop the monitoring daemon"""
        logger.info("🛑 Stopping Performance Monitoring Daemon...")
        
        self.running = False
        
        # Wait for threads to complete
        if self.monitoring_thread and self.monitoring_thread.is_alive():
            self.monitoring_thread.join(timeout=30)
        
        if self.health_thread and self.health_thread.is_alive():
            self.health_thread.join(timeout=10)
        
        logger.info("✅ Performance Monitoring Daemon stopped")
    
    def _signal_handler(self, signum, frame):
        """Handle shutdown signals"""
        logger.info(f"Received signal {signum}, shutting down...")
        self.stop()
        sys.exit(0)
    
    def _monitoring_loop(self, interval: int):
        """Main performance monitoring loop"""
        logger.info("📊 Performance monitoring loop started")
        
        while self.running:
            try:
                # Run quick performance test
                config = TestConfiguration(
                    protocol="unix_socket",
                    target_qps=1000,  # Moderate load for continuous monitoring
                    test_duration_seconds=15,  # Quick test
                    ramp_up_seconds=2,
                    concurrent_connections=20,
                    request_timeout_ms=1000,
                    warmup_requests=5,
                    layer1_settings={},
                    layer2_settings={},
                    layer3_settings={},
                    layer4_settings={}
                )
                
                # Execute performance test
                metrics = self.framework._execute_load_test(config)
                metrics.test_name = "continuous_monitoring"
                
                # Store metrics
                self.framework.db.store_metrics(metrics)
                
                # Check for performance alerts
                self._check_performance_alerts(metrics)
                
                # Analyze trends
                self._analyze_performance_trends()
                
                # Log current status
                logger.info(
                    f"📈 Performance: {metrics.avg_latency_ms:.2f}ms avg, "
                    f"{metrics.requests_per_second:.0f} QPS, "
                    f"{metrics.success_rate:.1%} success"
                )
                
                # Sleep until next monitoring cycle
                time.sleep(interval)
                
            except Exception as e:
                logger.error(f"Monitoring loop error: {e}")
                time.sleep(interval)  # Continue monitoring despite errors
        
        logger.info("📊 Performance monitoring loop stopped")
    
    def _health_monitoring_loop(self):
        """System health monitoring loop"""
        logger.info("🏥 System health monitoring started")
        
        while self.running:
            try:
                # Collect system health metrics
                health = self._collect_system_health()
                
                # Check health alerts
                self._check_health_alerts(health)
                
                # Store health data (simplified storage)
                self._store_health_metrics(health)
                
                # Sleep for 60 seconds between health checks
                time.sleep(60)
                
            except Exception as e:
                logger.error(f"Health monitoring error: {e}")
                time.sleep(60)
        
        logger.info("🏥 System health monitoring stopped")
    
    def _collect_system_health(self) -> SystemHealth:
        """Collect current system health metrics"""
        # System metrics
        cpu_percent = psutil.cpu_percent(interval=1)
        memory = psutil.virtual_memory()
        disk = psutil.disk_usage('/')
        
        # Network connections
        connections = len(psutil.net_connections())
        
        # Check Unix socket availability
        unix_sockets_available = True
        for layer_id in [1, 2, 3, 4]:
            socket_path = self.framework.LAYER_ENDPOINTS[layer_id]['unix']
            if not Path(socket_path).exists():
                unix_sockets_available = False
                break
        
        # Check service responsiveness
        services_responding = {}
        for layer_id in [1, 2, 3, 4]:
            try:
                import requests
                url = f"{self.framework.LAYER_ENDPOINTS[layer_id]['http']}/health"
                response = requests.get(url, timeout=2.0)
                services_responding[f"layer_{layer_id}"] = response.status_code == 200
            except:
                services_responding[f"layer_{layer_id}"] = False
        
        return SystemHealth(
            timestamp=datetime.now().isoformat(),
            cpu_percent=cpu_percent,
            memory_percent=memory.percent,
            memory_available_mb=memory.available / 1024 / 1024,
            disk_usage_percent=disk.percent,
            active_connections=connections,
            unix_sockets_available=unix_sockets_available,
            services_responding=services_responding
        )
    
    def _store_health_metrics(self, health: SystemHealth):
        """Store system health metrics"""
        try:
            with sqlite3.connect(self.framework.db.db_path) as conn:
                conn.execute("""
                    CREATE TABLE IF NOT EXISTS system_health (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        timestamp TEXT NOT NULL,
                        cpu_percent REAL,
                        memory_percent REAL,
                        memory_available_mb REAL,
                        disk_usage_percent REAL,
                        active_connections INTEGER,
                        unix_sockets_available BOOLEAN,
                        services_responding TEXT
                    )
                """)
                
                conn.execute("""
                    INSERT INTO system_health VALUES (
                        NULL, ?, ?, ?, ?, ?, ?, ?, ?
                    )
                """, (
                    health.timestamp,
                    health.cpu_percent,
                    health.memory_percent,
                    health.memory_available_mb,
                    health.disk_usage_percent,
                    health.active_connections,
                    health.unix_sockets_available,
                    json.dumps(health.services_responding)
                ))
        except Exception as e:
            logger.error(f"Failed to store health metrics: {e}")
    
    def _check_performance_alerts(self, metrics: PerformanceMetrics):
        """Check performance metrics against alert thresholds"""
        current_time = datetime.now()
        alerts_to_send = []
        
        # Latency alert
        if metrics.avg_latency_ms > self.alert_config.latency_threshold_ms:
            alert_key = "high_latency"
            if self._should_send_alert(alert_key, current_time):
                alerts_to_send.append({
                    "type": "error",
                    "title": "🚨 High Latency Alert",
                    "message": f"Average latency: {metrics.avg_latency_ms:.2f}ms (threshold: {self.alert_config.latency_threshold_ms}ms)",
                    "details": f"P95: {metrics.p95_latency_ms:.2f}ms, P99: {metrics.p99_latency_ms:.2f}ms"
                })
        
        # QPS alert
        if metrics.requests_per_second < self.alert_config.qps_threshold:
            alert_key = "low_qps"
            if self._should_send_alert(alert_key, current_time):
                alerts_to_send.append({
                    "type": "warning",
                    "title": "⚠️ Low QPS Alert", 
                    "message": f"QPS: {metrics.requests_per_second:.0f} (threshold: {self.alert_config.qps_threshold})",
                    "details": f"Success rate: {metrics.success_rate:.1%}"
                })
        
        # Error rate alert
        if metrics.total_requests > 0:
            error_rate = metrics.failed_requests / metrics.total_requests
            if error_rate > self.alert_config.error_rate_threshold:
                alert_key = "high_error_rate"
                if self._should_send_alert(alert_key, current_time):
                    alerts_to_send.append({
                        "type": "error",
                        "title": "🚨 High Error Rate Alert",
                        "message": f"Error rate: {error_rate:.1%} (threshold: {self.alert_config.error_rate_threshold:.1%})",
                        "details": f"Errors: {metrics.failed_requests}/{metrics.total_requests}"
                    })
        
        # Success rate alert
        if metrics.success_rate < self.alert_config.success_rate_threshold:
            alert_key = "low_success_rate"
            if self._should_send_alert(alert_key, current_time):
                alerts_to_send.append({
                    "type": "error",
                    "title": "🚨 Low Success Rate Alert",
                    "message": f"Success rate: {metrics.success_rate:.1%} (threshold: {self.alert_config.success_rate_threshold:.1%})",
                    "details": f"Timeouts: {metrics.timeout_count}, Connection errors: {metrics.connection_errors}"
                })
        
        # Memory usage alert
        if metrics.memory_usage_mb > self.alert_config.memory_threshold_mb:
            alert_key = "high_memory"
            if self._should_send_alert(alert_key, current_time):
                alerts_to_send.append({
                    "type": "warning",
                    "title": "⚠️ High Memory Usage Alert",
                    "message": f"Memory usage: {metrics.memory_usage_mb:.0f}MB (threshold: {self.alert_config.memory_threshold_mb}MB)",
                    "details": f"Peak memory: {metrics.memory_peak_mb:.0f}MB"
                })
        
        # Send alerts
        for alert in alerts_to_send:
            self._send_alert(alert)
    
    def _check_health_alerts(self, health: SystemHealth):
        """Check system health metrics for alerts"""
        current_time = datetime.now()
        alerts_to_send = []
        
        # CPU usage alert
        if health.cpu_percent > self.alert_config.cpu_threshold_percent:
            alert_key = "high_cpu"
            if self._should_send_alert(alert_key, current_time):
                alerts_to_send.append({
                    "type": "warning",
                    "title": "⚠️ High CPU Usage Alert",
                    "message": f"CPU usage: {health.cpu_percent:.1f}% (threshold: {self.alert_config.cpu_threshold_percent}%)"
                })
        
        # Unix socket availability alert
        if not health.unix_sockets_available:
            alert_key = "unix_sockets_unavailable"
            if self._should_send_alert(alert_key, current_time):
                alerts_to_send.append({
                    "type": "error",
                    "title": "🚨 Unix Sockets Unavailable",
                    "message": "One or more Unix sockets are not available"
                })
        
        # Service responsiveness alerts
        for service, responding in health.services_responding.items():
            if not responding:
                alert_key = f"service_unresponsive_{service}"
                if self._should_send_alert(alert_key, current_time):
                    alerts_to_send.append({
                        "type": "error",
                        "title": f"🚨 Service Unresponsive: {service}",
                        "message": f"Service {service} is not responding to health checks"
                    })
        
        # Send alerts
        for alert in alerts_to_send:
            self._send_alert(alert)
    
    def _should_send_alert(self, alert_key: str, current_time: datetime) -> bool:
        """Check if alert should be sent based on cooldown period"""
        last_alert_time = self.last_alerts.get(alert_key)
        if last_alert_time is None:
            self.last_alerts[alert_key] = current_time
            return True
        
        time_since_last = current_time - last_alert_time
        cooldown = timedelta(minutes=self.alert_config.alert_cooldown_minutes)
        
        if time_since_last >= cooldown:
            self.last_alerts[alert_key] = current_time
            return True
        
        return False
    
    def _send_alert(self, alert: Dict[str, str]):
        """Send alert notification"""
        try:
            # Try to send Discord notification if available
            try:
                from mfn_phase2_validation_framework import mcp__nabu__mcp__nabu__discord_notify
                mcp__nabu__mcp__nabu__discord_notify(
                    title=alert["title"],
                    message=alert["message"],
                    type=alert["type"]
                )
            except:
                # Fallback to logging
                pass
            
            # Always log the alert
            logger.warning(f"{alert['title']}: {alert['message']}")
            if "details" in alert:
                logger.warning(f"Details: {alert['details']}")
        
        except Exception as e:
            logger.error(f"Failed to send alert: {e}")
    
    def _analyze_performance_trends(self):
        """Analyze performance trends and predict issues"""
        try:
            # Get recent performance data
            cutoff_time = (datetime.now() - timedelta(minutes=self.alert_config.trend_window_minutes)).isoformat()
            
            with sqlite3.connect(self.framework.db.db_path) as conn:
                cursor = conn.execute("""
                    SELECT avg_latency_ms, requests_per_second, success_rate, timestamp
                    FROM performance_metrics 
                    WHERE test_name = 'continuous_monitoring' AND timestamp > ?
                    ORDER BY timestamp
                """, (cutoff_time,))
                
                data = cursor.fetchall()
            
            if len(data) < 3:  # Need at least 3 points for trend analysis
                return
            
            latencies = [row[0] for row in data]
            qps_values = [row[1] for row in data]
            success_rates = [row[2] for row in data]
            
            # Calculate trends
            latency_trend = self._calculate_trend(latencies)
            qps_trend = self._calculate_trend(qps_values)
            success_trend = self._calculate_trend(success_rates)
            
            # Check for concerning trends
            current_time = datetime.now()
            
            # Latency increasing trend
            if latency_trend > 0.1:  # 10% increase
                alert_key = "latency_trend_increasing"
                if self._should_send_alert(alert_key, current_time):
                    self._send_alert({
                        "type": "warning",
                        "title": "📈 Latency Trend Alert",
                        "message": f"Latency trending upward: {latency_trend:.1%} increase over {self.alert_config.trend_window_minutes} minutes",
                        "details": f"Current: {latencies[-1]:.2f}ms, Previous: {latencies[0]:.2f}ms"
                    })
            
            # QPS decreasing trend
            if qps_trend < -0.1:  # 10% decrease
                alert_key = "qps_trend_decreasing"
                if self._should_send_alert(alert_key, current_time):
                    self._send_alert({
                        "type": "warning", 
                        "title": "📉 QPS Trend Alert",
                        "message": f"QPS trending downward: {abs(qps_trend):.1%} decrease over {self.alert_config.trend_window_minutes} minutes",
                        "details": f"Current: {qps_values[-1]:.0f}, Previous: {qps_values[0]:.0f}"
                    })
        
        except Exception as e:
            logger.error(f"Trend analysis error: {e}")
    
    def _calculate_trend(self, values: List[float]) -> float:
        """Calculate trend as percentage change from first to last value"""
        if len(values) < 2 or values[0] == 0:
            return 0.0
        
        return (values[-1] - values[0]) / values[0]
    
    def generate_status_report(self) -> Dict[str, Any]:
        """Generate current status report"""
        try:
            # Get latest performance metrics
            with sqlite3.connect(self.framework.db.db_path) as conn:
                conn.row_factory = sqlite3.Row
                cursor = conn.execute("""
                    SELECT * FROM performance_metrics 
                    WHERE test_name = 'continuous_monitoring' 
                    ORDER BY timestamp DESC LIMIT 1
                """)
                latest_perf = cursor.fetchone()
                
                cursor = conn.execute("""
                    SELECT * FROM system_health 
                    ORDER BY timestamp DESC LIMIT 1
                """)
                latest_health = cursor.fetchone()
            
            # Calculate status
            status = "healthy"
            issues = []
            
            if latest_perf:
                if latest_perf['avg_latency_ms'] > self.alert_config.latency_threshold_ms:
                    status = "degraded"
                    issues.append(f"High latency: {latest_perf['avg_latency_ms']:.2f}ms")
                
                if latest_perf['requests_per_second'] < self.alert_config.qps_threshold:
                    status = "degraded"
                    issues.append(f"Low QPS: {latest_perf['requests_per_second']:.0f}")
                
                if latest_perf['success_rate'] < self.alert_config.success_rate_threshold:
                    status = "critical"
                    issues.append(f"Low success rate: {latest_perf['success_rate']:.1%}")
            
            if latest_health:
                if latest_health['cpu_percent'] > self.alert_config.cpu_threshold_percent:
                    status = "degraded" if status == "healthy" else status
                    issues.append(f"High CPU: {latest_health['cpu_percent']:.1f}%")
                
                if not latest_health['unix_sockets_available']:
                    status = "critical"
                    issues.append("Unix sockets unavailable")
            
            report = {
                "timestamp": datetime.now().isoformat(),
                "status": status,
                "issues": issues,
                "latest_performance": dict(latest_perf) if latest_perf else None,
                "latest_health": dict(latest_health) if latest_health else None,
                "monitoring_active": self.running
            }
            
            return report
        
        except Exception as e:
            return {
                "timestamp": datetime.now().isoformat(),
                "status": "error",
                "error": str(e),
                "monitoring_active": self.running
            }


def main():
    """Main daemon execution"""
    import argparse
    
    parser = argparse.ArgumentParser(description="MFN Performance Monitoring Daemon")
    parser.add_argument("--config", type=str, help="Configuration file path")
    parser.add_argument("--interval", type=int, default=30, help="Monitoring interval in seconds")
    parser.add_argument("--daemon", action="store_true", help="Run as daemon (detached)")
    parser.add_argument("--status", action="store_true", help="Show current status and exit")
    parser.add_argument("--stop", action="store_true", help="Stop running daemon")
    
    args = parser.parse_args()
    
    daemon = PerformanceMonitoringDaemon(args.config)
    
    if args.status:
        # Show status
        report = daemon.generate_status_report()
        print(json.dumps(report, indent=2, default=str))
        return 0
    
    if args.stop:
        # Stop daemon (implementation would depend on process management)
        print("Stop functionality not implemented - send SIGTERM to daemon process")
        return 0
    
    if args.daemon:
        # TODO: Implement proper daemon mode with process forking
        logger.info("Daemon mode not fully implemented - running in foreground")
    
    try:
        # Start monitoring
        daemon.start(args.interval)
        
        # Keep main thread alive
        while daemon.running:
            time.sleep(1)
    
    except KeyboardInterrupt:
        logger.info("Received interrupt signal")
    
    finally:
        daemon.stop()
    
    return 0


if __name__ == "__main__":
    exit(main())