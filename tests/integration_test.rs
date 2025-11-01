//! Integration tests for the unified socket communication system

use mfn_telepathy::socket::*;
use bytes::Bytes;
use std::time::Duration;
use tokio::time::sleep;

/// Test message handler for testing
struct TestHandler;

#[async_trait::async_trait]
impl MessageHandler for TestHandler {
    async fn handle_message(&self, message: SocketMessage) -> SocketResult<SocketMessage> {
        // Echo the message back with success
        Ok(SocketMessage::new(
            MessageType::Success,
            message.header.correlation_id,
            message.payload,
        ))
    }
}

#[tokio::test]
async fn test_socket_server_client_communication() {
    let socket_path = "/tmp/test_integration.sock";

    // Create and start server
    let server_config = SocketServerConfig {
        socket_path: socket_path.into(),
        max_connections: 10,
        ..Default::default()
    };

    let server = SocketServer::new(server_config.clone(), TestHandler);

    // Start server in background
    let server_handle = tokio::spawn(async move {
        server.start().await
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Create client
    let client_config = SocketClientConfig::default();
    let client = SocketClient::new(socket_path, client_config);

    // Test ping
    let ping_result = client.ping().await;
    assert!(ping_result.is_ok());
    let latency = ping_result.unwrap();
    assert!(latency < Duration::from_secs(1));

    // Test request/response
    let test_payload = Bytes::from("test data");
    let response = client.request(MessageType::MemoryAdd, test_payload.clone()).await;
    assert!(response.is_ok());

    let resp = response.unwrap();
    assert_eq!(resp.header.msg_type, MessageType::Success as u16);
    assert_eq!(resp.payload, test_payload);

    // Cleanup
    server_handle.abort();
    let _ = std::fs::remove_file(socket_path);
}

#[tokio::test]
async fn test_connection_pool() {
    let pool = ConnectionPool::new(
        "/tmp/test_pool.sock".into(),
        5,
        Duration::from_secs(5),
    );

    let stats = pool.stats().await;
    assert_eq!(stats.max_size, 5);
    assert_eq!(stats.available, 0); // No connections yet

    // Test pool lifecycle
    pool.close_all().await;
}

#[tokio::test]
async fn test_message_router() {
    let config = SocketClientConfig::default();
    let router = MessageRouter::new(config);

    // Add custom route
    let pattern = RoutePattern::new(MessageType::MemoryAdd, 1)
        .with_failover(vec![2, 3]);
    router.add_route(pattern).await;

    // Test health status
    let health = router.get_health_status().await;
    assert!(health.is_empty() || health.len() == 4);
}

#[tokio::test]
async fn test_binary_protocol() {
    // Test serialization/deserialization
    let payload = Bytes::from(vec![1, 2, 3, 4, 5]);
    let message = SocketMessage::new(MessageType::SearchSimilarity, 42, payload.clone());

    // Without compression
    let serialized = message.to_bytes(false).unwrap();
    let deserialized = SocketMessage::from_bytes(&serialized).unwrap();

    assert_eq!(message.header.msg_type, deserialized.header.msg_type);
    assert_eq!(message.header.correlation_id, deserialized.header.correlation_id);
    assert_eq!(message.payload, deserialized.payload);

    // With compression
    let compressed = message.to_bytes(true).unwrap();
    let decompressed = SocketMessage::from_bytes(&compressed).unwrap();

    assert_eq!(message.payload, decompressed.payload);
}

#[tokio::test]
async fn test_monitor_metrics() {
    let monitor = SocketMonitor::new();

    // Record some metrics
    monitor.record_connection();
    monitor.record_request(1, Duration::from_millis(10), true);
    monitor.record_request(2, Duration::from_millis(20), true);
    monitor.record_request(1, Duration::from_millis(15), false);

    sleep(Duration::from_millis(10)).await;

    // Get metrics
    let report = monitor.get_report().await;

    assert_eq!(report.connection_metrics.total_connections, 1);
    assert_eq!(report.request_metrics.total_requests, 3);
    assert_eq!(report.request_metrics.successful_requests, 2);
    assert_eq!(report.request_metrics.failed_requests, 1);

    // Check Prometheus export
    let prometheus = monitor.export_prometheus().await;
    assert!(prometheus.contains("mfn_requests_total 3"));
}

#[tokio::test]
async fn test_client_retry_logic() {
    let client_config = SocketClientConfig {
        max_retries: 3,
        retry_delay: Duration::from_millis(10),
        ..Default::default()
    };

    // Create client pointing to non-existent server
    let client = SocketClient::new("/tmp/nonexistent.sock", client_config);

    // Should fail after retries
    let result = client.request(MessageType::Ping, Bytes::from("test")).await;
    assert!(result.is_err());

    // Check metrics
    let stats = client.metrics().get_stats().await;
    assert!(stats.get("total_retries").unwrap_or(&0.0) > &0.0);
}

#[tokio::test]
async fn test_multi_client() {
    let mut multi = MultiClient::new(SocketClientConfig::default());

    multi.add_endpoint("layer1".to_string(), "/tmp/mfn_layer1.sock".into());
    multi.add_endpoint("layer2".to_string(), "/tmp/mfn_layer2.sock".into());

    assert!(multi.get("layer1").is_some());
    assert!(multi.get("layer2").is_some());
    assert!(multi.get("layer3").is_none());

    multi.close_all().await;
}

#[test]
fn test_socket_paths() {
    assert_eq!(SocketPaths::get_layer_socket(1).to_str().unwrap(), "/tmp/mfn_layer1.sock");
    assert_eq!(SocketPaths::get_layer_socket(2).to_str().unwrap(), "/tmp/mfn_layer2.sock");
    assert_eq!(SocketPaths::get_layer_socket(3).to_str().unwrap(), "/tmp/mfn_layer3.sock");
    assert_eq!(SocketPaths::get_layer_socket(4).to_str().unwrap(), "/tmp/mfn_layer4.sock");

    let health = SocketPaths::check_socket_health();
    assert_eq!(health.len(), 4);
}

#[test]
fn test_config_profiles() {
    let default = UnifiedSocketConfig::default();
    assert!(default.use_binary_protocol);
    assert!(default.enable_compression);

    let high_perf = UnifiedSocketConfig::high_performance();
    assert_eq!(high_perf.pool_size, 20);
    assert_eq!(high_perf.compression_threshold, 512);

    let low_latency = UnifiedSocketConfig::low_latency();
    assert!(!low_latency.enable_compression);
    assert_eq!(low_latency.request_timeout, Duration::from_millis(500));
}

#[tokio::test]
async fn test_large_payload_handling() {
    // Test with 1MB payload
    let large_payload = Bytes::from(vec![0u8; 1024 * 1024]);
    let message = SocketMessage::new(MessageType::BatchRequest, 9999, large_payload.clone());

    // Test serialization
    let serialized = message.to_bytes(true).unwrap();
    assert!(serialized.len() < large_payload.len()); // Should be compressed

    // Test deserialization
    let deserialized = SocketMessage::from_bytes(&serialized).unwrap();
    assert_eq!(deserialized.payload.len(), large_payload.len());
}