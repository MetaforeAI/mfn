//! Message Router for Inter-Layer Communication
//!
//! Provides intelligent message routing, load balancing, and service discovery
//! for MFN layers without requiring external service mesh tools.

use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use bytes::Bytes;
use tracing::{debug, info, warn, error};

use crate::socket::{
    SocketClient, SocketClientConfig, SocketMessage, MessageType,
    SocketError, SocketResult, SocketPaths,
};

/// Route pattern for matching message types to destinations
#[derive(Debug, Clone)]
pub struct RoutePattern {
    /// Message type to match
    pub message_type: MessageType,
    /// Target layer ID (1-4)
    pub target_layer: u8,
    /// Optional load balancing strategy
    pub load_balance: LoadBalanceStrategy,
    /// Optional failover layers
    pub failover_layers: Vec<u8>,
    /// Priority (higher = preferred)
    pub priority: u8,
}

impl RoutePattern {
    pub fn new(message_type: MessageType, target_layer: u8) -> Self {
        Self {
            message_type,
            target_layer,
            load_balance: LoadBalanceStrategy::RoundRobin,
            failover_layers: Vec::new(),
            priority: 100,
        }
    }

    pub fn with_failover(mut self, layers: Vec<u8>) -> Self {
        self.failover_layers = layers;
        self
    }

    pub fn with_load_balance(mut self, strategy: LoadBalanceStrategy) -> Self {
        self.load_balance = strategy;
        self
    }
}

/// Load balancing strategies
#[derive(Debug, Clone, Copy)]
pub enum LoadBalanceStrategy {
    /// Round-robin distribution
    RoundRobin,
    /// Random distribution
    Random,
    /// Least connections
    LeastConnections,
    /// Weighted round-robin
    WeightedRoundRobin,
    /// Sticky sessions (hash-based)
    Sticky,
}

/// Service health status
#[derive(Debug, Clone)]
pub struct ServiceHealth {
    pub layer_id: u8,
    pub is_healthy: bool,
    pub last_check: Instant,
    pub response_time_ms: f64,
    pub error_count: u32,
    pub success_count: u32,
}

impl ServiceHealth {
    pub fn new(layer_id: u8) -> Self {
        Self {
            layer_id,
            is_healthy: true,
            last_check: Instant::now(),
            response_time_ms: 0.0,
            error_count: 0,
            success_count: 0,
        }
    }

    pub fn update_success(&mut self, response_time: Duration) {
        self.is_healthy = true;
        self.last_check = Instant::now();
        self.success_count += 1;

        // Exponential moving average for response time
        let new_time = response_time.as_secs_f64() * 1000.0;
        self.response_time_ms = if self.response_time_ms == 0.0 {
            new_time
        } else {
            self.response_time_ms * 0.8 + new_time * 0.2
        };

        // Reset error count on success
        if self.error_count > 0 {
            self.error_count = self.error_count.saturating_sub(1);
        }
    }

    pub fn update_failure(&mut self) {
        self.last_check = Instant::now();
        self.error_count += 1;

        // Mark unhealthy after 3 consecutive errors
        if self.error_count >= 3 {
            self.is_healthy = false;
        }
    }

    pub fn score(&self) -> f64 {
        if !self.is_healthy {
            return 0.0;
        }

        // Score based on success rate and response time
        let success_rate = if self.success_count + self.error_count > 0 {
            self.success_count as f64 / (self.success_count + self.error_count) as f64
        } else {
            1.0
        };

        // Normalize response time (lower is better, max 1000ms)
        let response_score = 1.0 - (self.response_time_ms / 1000.0).min(1.0);

        success_rate * 0.7 + response_score * 0.3
    }
}

/// Message router for intelligent inter-layer communication
pub struct MessageRouter {
    /// Routing table mapping message types to routes
    routes: Arc<RwLock<HashMap<u16, Vec<RoutePattern>>>>,
    /// Layer clients
    clients: Arc<RwLock<HashMap<u8, Arc<SocketClient>>>>,
    /// Service health tracking
    health: Arc<RwLock<HashMap<u8, ServiceHealth>>>,
    /// Round-robin counters
    rr_counters: Arc<RwLock<HashMap<u8, usize>>>,
    /// Default client configuration
    default_config: SocketClientConfig,
    /// Health check interval
    health_check_interval: Duration,
}

impl MessageRouter {
    /// Create a new message router
    pub fn new(default_config: SocketClientConfig) -> Self {
        let router = Self {
            routes: Arc::new(RwLock::new(HashMap::new())),
            clients: Arc::new(RwLock::new(HashMap::new())),
            health: Arc::new(RwLock::new(HashMap::new())),
            rr_counters: Arc::new(RwLock::new(HashMap::new())),
            default_config,
            health_check_interval: Duration::from_secs(10),
        };

        // Initialize default routes
        router.setup_default_routes();

        // Start health monitoring
        router.start_health_monitoring();

        router
    }

    /// Setup default routing patterns for MFN layers
    fn setup_default_routes(&self) {
        let routes = vec![
            // Layer 1 (IFR) routes
            RoutePattern::new(MessageType::Layer1Store, 1)
                .with_failover(vec![2]),
            RoutePattern::new(MessageType::MemoryAdd, 1),
            RoutePattern::new(MessageType::MemoryGet, 1),

            // Layer 2 (DSR) routes
            RoutePattern::new(MessageType::Layer2Similarity, 2)
                .with_failover(vec![3]),
            RoutePattern::new(MessageType::SearchSimilarity, 2),

            // Layer 3 (ALM) routes
            RoutePattern::new(MessageType::Layer3Associate, 3)
                .with_failover(vec![2, 4]),
            RoutePattern::new(MessageType::SearchAssociative, 3),

            // Layer 4 (CPE) routes
            RoutePattern::new(MessageType::Layer4Context, 4)
                .with_failover(vec![3]),
            RoutePattern::new(MessageType::SearchTemporal, 4),
        ];

        let rt = self.routes.clone();
        tokio::spawn(async move {
            let mut route_table = rt.write().await;
            for pattern in routes {
                let msg_type = pattern.message_type as u16;
                route_table.entry(msg_type)
                    .or_insert_with(Vec::new)
                    .push(pattern);
            }
        });
    }

    /// Initialize layer clients
    pub async fn initialize_layers(&self) {
        let mut clients = self.clients.write().await;
        let mut health = self.health.write().await;

        for layer_id in 1..=4 {
            let socket_path = SocketPaths::get_layer_socket(layer_id);
            let client = Arc::new(SocketClient::new(socket_path, self.default_config.clone()));
            clients.insert(layer_id, client);
            health.insert(layer_id, ServiceHealth::new(layer_id));
        }

        info!("Initialized {} layer clients", clients.len());
    }

    /// Add a custom route
    pub async fn add_route(&self, pattern: RoutePattern) {
        let mut routes = self.routes.write().await;
        let msg_type = pattern.message_type as u16;
        routes.entry(msg_type)
            .or_insert_with(Vec::new)
            .push(pattern);
    }

    /// Route a message to the appropriate layer
    pub async fn route_message(
        &self,
        message: SocketMessage,
    ) -> SocketResult<SocketMessage> {
        let msg_type = message.header.msg_type;

        // Find matching routes
        let routes = self.routes.read().await;
        let patterns = routes.get(&msg_type)
            .ok_or_else(|| SocketError::Protocol(format!(
                "No route for message type: {:?}", msg_type
            )))?;

        // Sort by priority
        let mut sorted_patterns = patterns.clone();
        sorted_patterns.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Try primary route
        for pattern in &sorted_patterns {
            match self.send_to_layer(pattern.target_layer, &message).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    warn!("Failed to route to layer {}: {}", pattern.target_layer, e);

                    // Try failover layers
                    for &failover_layer in &pattern.failover_layers {
                        match self.send_to_layer(failover_layer, &message).await {
                            Ok(response) => return Ok(response),
                            Err(e) => {
                                warn!("Failover to layer {} failed: {}", failover_layer, e);
                            }
                        }
                    }
                }
            }
        }

        Err(SocketError::Protocol("All routes failed".to_string()))
    }

    /// Send message to specific layer
    async fn send_to_layer(
        &self,
        layer_id: u8,
        message: &SocketMessage,
    ) -> SocketResult<SocketMessage> {
        let clients = self.clients.read().await;
        let client = clients.get(&layer_id)
            .ok_or_else(|| SocketError::InvalidLayer(layer_id))?;

        let start = Instant::now();
        let result = client.request_with_id(
            MessageType::from(message.header.msg_type),
            message.header.correlation_id,
            message.payload.clone(),
        ).await;

        // Update health status
        let mut health = self.health.write().await;
        if let Some(h) = health.get_mut(&layer_id) {
            match &result {
                Ok(_) => h.update_success(start.elapsed()),
                Err(_) => h.update_failure(),
            }
        }

        result
    }

    /// Broadcast message to all layers
    pub async fn broadcast(
        &self,
        message: SocketMessage,
    ) -> Vec<(u8, SocketResult<SocketMessage>)> {
        let clients = self.clients.read().await;
        let mut results = Vec::new();

        for (&layer_id, client) in clients.iter() {
            let response = client.request_with_id(
                MessageType::from(message.header.msg_type),
                message.header.correlation_id,
                message.payload.clone(),
            ).await;
            results.push((layer_id, response));
        }

        results
    }

    /// Select best layer based on load balancing strategy
    pub async fn select_layer(
        &self,
        candidates: Vec<u8>,
        strategy: LoadBalanceStrategy,
    ) -> Option<u8> {
        if candidates.is_empty() {
            return None;
        }

        let health = self.health.read().await;
        let healthy_layers: Vec<u8> = candidates.into_iter()
            .filter(|&id| {
                health.get(&id).map(|h| h.is_healthy).unwrap_or(false)
            })
            .collect();

        if healthy_layers.is_empty() {
            return None;
        }

        match strategy {
            LoadBalanceStrategy::RoundRobin => {
                let mut counters = self.rr_counters.write().await;
                let counter = counters.entry(healthy_layers[0]).or_insert(0);
                let selected = healthy_layers[*counter % healthy_layers.len()];
                *counter += 1;
                Some(selected)
            }
            LoadBalanceStrategy::Random => {
                use rand::Rng;
                let idx = rand::thread_rng().gen_range(0..healthy_layers.len());
                Some(healthy_layers[idx])
            }
            LoadBalanceStrategy::LeastConnections => {
                // Would need connection tracking from pool
                Some(healthy_layers[0])
            }
            LoadBalanceStrategy::WeightedRoundRobin => {
                // Select based on health score
                let mut best_score = 0.0;
                let mut best_layer = healthy_layers[0];

                for &layer_id in &healthy_layers {
                    if let Some(h) = health.get(&layer_id) {
                        let score = h.score();
                        if score > best_score {
                            best_score = score;
                            best_layer = layer_id;
                        }
                    }
                }

                Some(best_layer)
            }
            LoadBalanceStrategy::Sticky => {
                // Use first healthy layer for simplicity
                Some(healthy_layers[0])
            }
        }
    }

    /// Start health monitoring background task
    fn start_health_monitoring(&self) {
        let router = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(router.health_check_interval);
            ticker.tick().await; // Skip first tick

            loop {
                ticker.tick().await;
                router.perform_health_checks().await;
            }
        });
    }

    /// Perform health checks on all layers
    async fn perform_health_checks(&self) {
        let clients = self.clients.read().await;

        for (&layer_id, client) in clients.iter() {
            let start = Instant::now();
            let result = client.ping().await;

            let mut health = self.health.write().await;
            if let Some(h) = health.get_mut(&layer_id) {
                match result {
                    Ok(duration) => {
                        h.update_success(duration);
                        debug!("Layer {} healthy, ping: {:?}", layer_id, duration);
                    }
                    Err(e) => {
                        h.update_failure();
                        warn!("Layer {} health check failed: {}", layer_id, e);
                    }
                }
            }
        }
    }

    /// Get current health status
    pub async fn get_health_status(&self) -> HashMap<u8, ServiceHealth> {
        self.health.read().await.clone()
    }
}

impl Clone for MessageRouter {
    fn clone(&self) -> Self {
        Self {
            routes: Arc::clone(&self.routes),
            clients: Arc::clone(&self.clients),
            health: Arc::clone(&self.health),
            rr_counters: Arc::clone(&self.rr_counters),
            default_config: self.default_config.clone(),
            health_check_interval: self.health_check_interval,
        }
    }
}

impl From<u16> for MessageType {
    fn from(value: u16) -> Self {
        match value {
            0x0001 => MessageType::MemoryAdd,
            0x0002 => MessageType::MemoryGet,
            0x0010 => MessageType::SearchSimilarity,
            0x0030 => MessageType::Ping,
            _ => MessageType::Error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_router_creation() {
        let config = SocketClientConfig::default();
        let router = MessageRouter::new(config);

        router.initialize_layers().await;

        let health = router.get_health_status().await;
        assert_eq!(health.len(), 4); // 4 layers
    }

    #[tokio::test]
    async fn test_health_scoring() {
        let mut health = ServiceHealth::new(1);

        health.update_success(Duration::from_millis(10));
        health.update_success(Duration::from_millis(20));

        let score = health.score();
        assert!(score > 0.9); // Should have high score

        health.update_failure();
        health.update_failure();
        health.update_failure();

        let score = health.score();
        assert_eq!(score, 0.0); // Should be unhealthy
    }
}