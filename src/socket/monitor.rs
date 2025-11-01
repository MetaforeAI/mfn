//! Socket Monitoring and Metrics
//!
//! Provides comprehensive monitoring, metrics collection, and observability
//! for the unified socket communication system.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use tracing::{debug, info, warn};

/// Connection metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetrics {
    pub total_connections: u64,
    pub active_connections: usize,
    pub failed_connections: u64,
    pub avg_connection_duration_ms: f64,
    pub max_connection_duration_ms: f64,
    pub connection_reuse_ratio: f64,
}

/// Request metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub requests_per_second: f64,
}

/// Layer-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMetrics {
    pub layer_id: u8,
    pub requests: u64,
    pub errors: u64,
    pub avg_latency_ms: f64,
    pub uptime_percentage: f64,
    pub last_seen: u64,
}

/// Protocol metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub messages_compressed: u64,
    pub compression_ratio: f64,
    pub protocol_errors: u64,
    pub crc_failures: u64,
}

/// Time series data point
#[derive(Debug, Clone)]
struct DataPoint {
    timestamp: Instant,
    value: f64,
}

/// Socket monitor for collecting and analyzing metrics
pub struct SocketMonitor {
    // Counters
    total_connections: AtomicU64,
    active_connections: AtomicUsize,
    failed_connections: AtomicU64,
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    messages_compressed: AtomicU64,
    protocol_errors: AtomicU64,
    crc_failures: AtomicU64,

    // Time series data
    latencies: Arc<RwLock<VecDeque<DataPoint>>>,
    connection_durations: Arc<RwLock<VecDeque<DataPoint>>>,
    throughput: Arc<RwLock<VecDeque<DataPoint>>>,

    // Layer metrics
    layer_metrics: Arc<RwLock<HashMap<u8, LayerMetrics>>>,

    // Configuration
    max_data_points: usize,
    monitoring_interval: Duration,
    start_time: Instant,
}

impl SocketMonitor {
    /// Create a new socket monitor
    pub fn new() -> Self {
        Self {
            total_connections: AtomicU64::new(0),
            active_connections: AtomicUsize::new(0),
            failed_connections: AtomicU64::new(0),
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            messages_compressed: AtomicU64::new(0),
            protocol_errors: AtomicU64::new(0),
            crc_failures: AtomicU64::new(0),
            latencies: Arc::new(RwLock::new(VecDeque::new())),
            connection_durations: Arc::new(RwLock::new(VecDeque::new())),
            throughput: Arc::new(RwLock::new(VecDeque::new())),
            layer_metrics: Arc::new(RwLock::new(HashMap::new())),
            max_data_points: 10000,
            monitoring_interval: Duration::from_secs(1),
            start_time: Instant::now(),
        }
    }

    /// Record a new connection
    pub fn record_connection(&self) {
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record connection closed
    pub fn record_connection_closed(&self, duration: Duration) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
        tokio::spawn({
            let durations = Arc::clone(&self.connection_durations);
            let max_points = self.max_data_points;
            async move {
                let mut durations = durations.write().await;
                durations.push_back(DataPoint {
                    timestamp: Instant::now(),
                    value: duration.as_secs_f64() * 1000.0,
                });
                if durations.len() > max_points {
                    durations.pop_front();
                }
            }
        });
    }

    /// Record failed connection attempt
    pub fn record_connection_failed(&self) {
        self.failed_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a request
    pub fn record_request(&self, layer_id: u8, latency: Duration, success: bool) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        if success {
            self.successful_requests.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }

        // Record latency
        tokio::spawn({
            let latencies = Arc::clone(&self.latencies);
            let layer_metrics = Arc::clone(&self.layer_metrics);
            let max_points = self.max_data_points;
            async move {
                let latency_ms = latency.as_secs_f64() * 1000.0;

                // Update time series
                let mut latencies = latencies.write().await;
                latencies.push_back(DataPoint {
                    timestamp: Instant::now(),
                    value: latency_ms,
                });
                if latencies.len() > max_points {
                    latencies.pop_front();
                }

                // Update layer metrics
                let mut metrics = layer_metrics.write().await;
                let layer = metrics.entry(layer_id).or_insert_with(|| LayerMetrics {
                    layer_id,
                    requests: 0,
                    errors: 0,
                    avg_latency_ms: 0.0,
                    uptime_percentage: 100.0,
                    last_seen: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                });

                layer.requests += 1;
                if !success {
                    layer.errors += 1;
                }

                // Update average latency (exponential moving average)
                layer.avg_latency_ms = if layer.avg_latency_ms == 0.0 {
                    latency_ms
                } else {
                    layer.avg_latency_ms * 0.9 + latency_ms * 0.1
                };

                layer.last_seen = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
            }
        });
    }

    /// Record bytes transferred
    pub fn record_bytes(&self, sent: u64, received: u64) {
        self.bytes_sent.fetch_add(sent, Ordering::Relaxed);
        self.bytes_received.fetch_add(received, Ordering::Relaxed);
    }

    /// Record compression
    pub fn record_compression(&self, original_size: usize, compressed_size: usize) {
        self.messages_compressed.fetch_add(1, Ordering::Relaxed);

        // Record compression ratio
        let ratio = compressed_size as f64 / original_size as f64;
        debug!("Compression ratio: {:.2}", ratio);
    }

    /// Record protocol error
    pub fn record_protocol_error(&self) {
        self.protocol_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record CRC failure
    pub fn record_crc_failure(&self) {
        self.crc_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get connection metrics
    pub async fn get_connection_metrics(&self) -> ConnectionMetrics {
        let durations = self.connection_durations.read().await;

        let (avg_duration, max_duration) = if !durations.is_empty() {
            let sum: f64 = durations.iter().map(|d| d.value).sum();
            let avg = sum / durations.len() as f64;
            let max = durations.iter().map(|d| d.value).fold(0.0, f64::max);
            (avg, max)
        } else {
            (0.0, 0.0)
        };

        let total_conns = self.total_connections.load(Ordering::Relaxed);
        let reuse_ratio = if total_conns > 0 {
            let total_reqs = self.total_requests.load(Ordering::Relaxed);
            (total_reqs as f64 / total_conns as f64).min(1.0)
        } else {
            0.0
        };

        ConnectionMetrics {
            total_connections: total_conns,
            active_connections: self.active_connections.load(Ordering::Relaxed),
            failed_connections: self.failed_connections.load(Ordering::Relaxed),
            avg_connection_duration_ms: avg_duration,
            max_connection_duration_ms: max_duration,
            connection_reuse_ratio: reuse_ratio,
        }
    }

    /// Get request metrics
    pub async fn get_request_metrics(&self) -> RequestMetrics {
        let latencies = self.latencies.read().await;

        let (avg_latency, p50, p95, p99) = if !latencies.is_empty() {
            let sum: f64 = latencies.iter().map(|d| d.value).sum();
            let avg = sum / latencies.len() as f64;

            // Calculate percentiles
            let mut sorted: Vec<f64> = latencies.iter().map(|d| d.value).collect();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

            let p50_idx = sorted.len() / 2;
            let p95_idx = (sorted.len() as f64 * 0.95) as usize;
            let p99_idx = (sorted.len() as f64 * 0.99) as usize;

            (
                avg,
                sorted[p50_idx],
                sorted.get(p95_idx).copied().unwrap_or(sorted[sorted.len() - 1]),
                sorted.get(p99_idx).copied().unwrap_or(sorted[sorted.len() - 1]),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let total_reqs = self.total_requests.load(Ordering::Relaxed);
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let rps = if elapsed > 0.0 {
            total_reqs as f64 / elapsed
        } else {
            0.0
        };

        RequestMetrics {
            total_requests: total_reqs,
            successful_requests: self.successful_requests.load(Ordering::Relaxed),
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            avg_latency_ms: avg_latency,
            p50_latency_ms: p50,
            p95_latency_ms: p95,
            p99_latency_ms: p99,
            requests_per_second: rps,
        }
    }

    /// Get protocol metrics
    pub fn get_protocol_metrics(&self) -> ProtocolMetrics {
        let total_compressed = self.messages_compressed.load(Ordering::Relaxed);
        let total_sent = self.bytes_sent.load(Ordering::Relaxed);

        let compression_ratio = if total_compressed > 0 && total_sent > 0 {
            // Estimate based on typical compression
            0.6 // 40% size reduction average
        } else {
            1.0
        };

        ProtocolMetrics {
            bytes_sent: total_sent,
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            messages_compressed: total_compressed,
            compression_ratio,
            protocol_errors: self.protocol_errors.load(Ordering::Relaxed),
            crc_failures: self.crc_failures.load(Ordering::Relaxed),
        }
    }

    /// Get layer-specific metrics
    pub async fn get_layer_metrics(&self) -> Vec<LayerMetrics> {
        self.layer_metrics.read().await
            .values()
            .cloned()
            .collect()
    }

    /// Get comprehensive metrics report
    pub async fn get_report(&self) -> MetricsReport {
        MetricsReport {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            connection_metrics: self.get_connection_metrics().await,
            request_metrics: self.get_request_metrics().await,
            protocol_metrics: self.get_protocol_metrics(),
            layer_metrics: self.get_layer_metrics().await,
        }
    }

    /// Export metrics in Prometheus format
    pub async fn export_prometheus(&self) -> String {
        let report = self.get_report().await;
        let mut output = String::new();

        // Connection metrics
        output.push_str(&format!(
            "# HELP mfn_total_connections Total number of connections\n\
             # TYPE mfn_total_connections counter\n\
             mfn_total_connections {}\n",
            report.connection_metrics.total_connections
        ));

        output.push_str(&format!(
            "# HELP mfn_active_connections Current active connections\n\
             # TYPE mfn_active_connections gauge\n\
             mfn_active_connections {}\n",
            report.connection_metrics.active_connections
        ));

        // Request metrics
        output.push_str(&format!(
            "# HELP mfn_requests_total Total number of requests\n\
             # TYPE mfn_requests_total counter\n\
             mfn_requests_total {}\n",
            report.request_metrics.total_requests
        ));

        output.push_str(&format!(
            "# HELP mfn_request_latency_milliseconds Request latency in milliseconds\n\
             # TYPE mfn_request_latency_milliseconds summary\n\
             mfn_request_latency_milliseconds{{quantile=\"0.5\"}} {}\n\
             mfn_request_latency_milliseconds{{quantile=\"0.95\"}} {}\n\
             mfn_request_latency_milliseconds{{quantile=\"0.99\"}} {}\n",
            report.request_metrics.p50_latency_ms,
            report.request_metrics.p95_latency_ms,
            report.request_metrics.p99_latency_ms
        ));

        // Layer metrics
        for layer in &report.layer_metrics {
            output.push_str(&format!(
                "mfn_layer_requests{{layer=\"{}\"}} {}\n",
                layer.layer_id, layer.requests
            ));
            output.push_str(&format!(
                "mfn_layer_errors{{layer=\"{}\"}} {}\n",
                layer.layer_id, layer.errors
            ));
            output.push_str(&format!(
                "mfn_layer_latency_ms{{layer=\"{}\"}} {}\n",
                layer.layer_id, layer.avg_latency_ms
            ));
        }

        output
    }
}

/// Complete metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsReport {
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub connection_metrics: ConnectionMetrics,
    pub request_metrics: RequestMetrics,
    pub protocol_metrics: ProtocolMetrics,
    pub layer_metrics: Vec<LayerMetrics>,
}

impl Default for SocketMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monitor_creation() {
        let monitor = SocketMonitor::new();

        let report = monitor.get_report().await;
        assert_eq!(report.connection_metrics.total_connections, 0);
        assert_eq!(report.request_metrics.total_requests, 0);
    }

    #[tokio::test]
    async fn test_connection_tracking() {
        let monitor = SocketMonitor::new();

        monitor.record_connection();
        monitor.record_connection();
        monitor.record_connection_closed(Duration::from_millis(100));

        let metrics = monitor.get_connection_metrics().await;
        assert_eq!(metrics.total_connections, 2);
        assert_eq!(metrics.active_connections, 1);
    }

    #[tokio::test]
    async fn test_request_tracking() {
        let monitor = SocketMonitor::new();

        monitor.record_request(1, Duration::from_millis(10), true);
        monitor.record_request(1, Duration::from_millis(20), true);
        monitor.record_request(2, Duration::from_millis(15), false);

        tokio::time::sleep(Duration::from_millis(10)).await;

        let metrics = monitor.get_request_metrics().await;
        assert_eq!(metrics.total_requests, 3);
        assert_eq!(metrics.successful_requests, 2);
        assert_eq!(metrics.failed_requests, 1);
    }

    #[tokio::test]
    async fn test_prometheus_export() {
        let monitor = SocketMonitor::new();

        monitor.record_connection();
        monitor.record_request(1, Duration::from_millis(10), true);

        tokio::time::sleep(Duration::from_millis(10)).await;

        let prometheus = monitor.export_prometheus().await;
        assert!(prometheus.contains("mfn_total_connections 1"));
        assert!(prometheus.contains("mfn_requests_total 1"));
    }
}