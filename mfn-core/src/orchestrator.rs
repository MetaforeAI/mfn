// MFN Core - Orchestrator
// Coordinates memory flow between layers and manages the overall MFN system

use crate::layer_interface::*;
use crate::memory_types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration};

/// Central orchestrator that manages memory flow through MFN layers
pub struct MfnOrchestrator {
    layers: HashMap<LayerId, Arc<RwLock<Box<dyn MfnLayer>>>>,
    routing_config: RoutingConfig,
    performance_monitor: PerformanceMonitor,
}

/// Configuration for how memory flows between layers
#[derive(Debug, Clone)]
pub struct RoutingConfig {
    /// Default routing strategy
    pub default_strategy: RoutingStrategy,
    /// Maximum time to spend on each layer (microseconds)
    pub layer_timeout_us: u64,
    /// Whether to enable parallel layer queries
    pub enable_parallel: bool,
    /// Maximum layers to consult per query
    pub max_layers: usize,
    /// Confidence threshold for stopping search early
    pub confidence_threshold: f64,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            default_strategy: RoutingStrategy::Sequential,
            layer_timeout_us: 10_000, // 10ms per layer
            enable_parallel: true,
            max_layers: 4,
            confidence_threshold: 0.9,
        }
    }
}

/// Strategy for routing queries through layers
#[derive(Debug, Clone)]
pub enum RoutingStrategy {
    /// Query layers in sequence: L1 → L2 → L3 → L4
    Sequential,
    /// Query all layers in parallel and merge results
    Parallel,
    /// Use smart routing based on query type and history
    Adaptive,
    /// Custom routing logic
    Custom(fn(&UniversalSearchQuery) -> Vec<LayerId>),
}

/// Performance monitoring for the orchestrator
#[derive(Debug)]
pub struct PerformanceMonitor {
    query_count: u64,
    total_query_time_us: u64,
    layer_performance: HashMap<LayerId, LayerPerformanceStats>,
}

#[derive(Debug, Default)]
struct LayerPerformanceStats {
    queries: u64,
    total_time_us: u64,
    success_rate: f64,
    average_results: f64,
}

impl MfnOrchestrator {
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
            routing_config: RoutingConfig::default(),
            performance_monitor: PerformanceMonitor {
                query_count: 0,
                total_query_time_us: 0,
                layer_performance: HashMap::new(),
            },
        }
    }

    pub fn with_routing_config(mut self, config: RoutingConfig) -> Self {
        self.routing_config = config;
        self
    }

    /// Register a layer with the orchestrator
    pub async fn register_layer(
        &mut self,
        layer: Box<dyn MfnLayer>,
    ) -> LayerResult<()> {
        let layer_id = layer.layer_id();
        let layer = Arc::new(RwLock::new(layer));
        self.layers.insert(layer_id, layer);
        self.performance_monitor.layer_performance.insert(layer_id, LayerPerformanceStats::default());
        Ok(())
    }

    /// Remove a layer from the orchestrator
    pub async fn unregister_layer(&mut self, layer_id: LayerId) -> LayerResult<()> {
        if let Some(layer_ref) = self.layers.remove(&layer_id) {
            let mut layer = layer_ref.write().await;
            layer.shutdown().await?;
        }
        self.performance_monitor.layer_performance.remove(&layer_id);
        Ok(())
    }

    /// Add a memory to the appropriate layers
    pub async fn add_memory(&mut self, memory: UniversalMemory) -> LayerResult<()> {
        // Add to all layers that can store memories
        for (layer_id, layer_ref) in &self.layers {
            let mut layer = layer_ref.write().await;
            
            match layer.add_memory(memory.clone()).await {
                Ok(()) => {
                    log::debug!("Added memory {} to {}", memory.id, layer_id.as_str());
                }
                Err(e) => {
                    log::warn!("Failed to add memory {} to {}: {}", memory.id, layer_id.as_str(), e);
                    // Continue with other layers
                }
            }
        }
        
        Ok(())
    }

    /// Add an association between memories
    pub async fn add_association(&mut self, association: UniversalAssociation) -> LayerResult<()> {
        // Add to layers that support associations (typically Layer 3+)
        let association_layers = vec![LayerId::Layer3, LayerId::Layer4];
        
        for layer_id in association_layers {
            if let Some(layer_ref) = self.layers.get(&layer_id) {
                let mut layer = layer_ref.write().await;
                
                match layer.add_association(association.clone()).await {
                    Ok(()) => {
                        log::debug!("Added association {} to {}", association.id, layer_id.as_str());
                    }
                    Err(e) => {
                        log::warn!("Failed to add association {} to {}: {}", association.id, layer_id.as_str(), e);
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Perform a search across the MFN system
    pub async fn search(&mut self, query: UniversalSearchQuery) -> LayerResult<UniversalSearchResults> {
        let start_time = current_timestamp();
        self.performance_monitor.query_count += 1;

        let results = match self.routing_config.default_strategy {
            RoutingStrategy::Sequential => self.search_sequential(&query).await?,
            RoutingStrategy::Parallel => self.search_parallel(&query).await?,
            RoutingStrategy::Adaptive => self.search_adaptive(&query).await?,
            RoutingStrategy::Custom(router) => self.search_custom(&query, router).await?,
        };

        let total_time = current_timestamp() - start_time;
        self.performance_monitor.total_query_time_us += total_time;

        let total_found = results.results.len();
        Ok(UniversalSearchResults {
            results: results.results,
            query,
            total_found,
            search_time_us: total_time,
            layers_consulted: results.layers_consulted,
            performance_stats: results.performance_stats,
        })
    }

    async fn search_sequential(&self, query: &UniversalSearchQuery) -> LayerResult<UniversalSearchResults> {
        let mut all_results = Vec::new();
        let mut layers_consulted = Vec::new();
        let mut performance_stats = HashMap::new();

        // Layer 1: Check for exact matches first
        if let Some(layer1_ref) = self.layers.get(&LayerId::Layer1) {
            let layer1 = layer1_ref.read().await;
            
            let layer_start = current_timestamp();
            let routing_decision = timeout(
                Duration::from_micros(self.routing_config.layer_timeout_us),
                layer1.search(query)
            ).await??;
            let layer_time = current_timestamp() - layer_start;

            layers_consulted.push(LayerId::Layer1);
            performance_stats.insert(
                "layer1_time_us".to_string(),
                serde_json::Value::Number(serde_json::Number::from(layer_time))
            );

            match routing_decision {
                RoutingDecision::FoundExact { results } => {
                    // Found exact match, return immediately
                    let total_found = results.len();
                    return Ok(UniversalSearchResults {
                        results,
                        query: query.clone(),
                        total_found,
                        search_time_us: layer_time,
                        layers_consulted,
                        performance_stats,
                    });
                }
                RoutingDecision::FoundPartial { results, continue_search, .. } => {
                    all_results.extend(results);
                    if !continue_search {
                        let total_found = all_results.len();
                        return Ok(UniversalSearchResults {
                            results: all_results,
                            query: query.clone(),
                            total_found,
                            search_time_us: layer_time,
                            layers_consulted,
                            performance_stats,
                        });
                    }
                }
                RoutingDecision::RouteToLayers { .. } => {
                    // Continue to next layer
                }
                RoutingDecision::SearchComplete { results } => {
                    all_results.extend(results);
                }
            }
        }

        // Layer 2: Similarity search if needed
        if let Some(layer2_ref) = self.layers.get(&LayerId::Layer2) {
            let layer2 = layer2_ref.read().await;
            
            let layer_start = current_timestamp();
            let routing_decision = timeout(
                Duration::from_micros(self.routing_config.layer_timeout_us),
                layer2.search(query)
            ).await??;
            let layer_time = current_timestamp() - layer_start;

            layers_consulted.push(LayerId::Layer2);
            performance_stats.insert(
                "layer2_time_us".to_string(),
                serde_json::Value::Number(serde_json::Number::from(layer_time))
            );

            match routing_decision {
                RoutingDecision::FoundExact { results } |
                RoutingDecision::SearchComplete { results } => {
                    all_results.extend(results);
                }
                RoutingDecision::FoundPartial { results, continue_search, .. } => {
                    all_results.extend(results);
                    if !continue_search {
                        let total_found = all_results.len();
                        return Ok(UniversalSearchResults {
                            results: all_results,
                            query: query.clone(),
                            total_found,
                            search_time_us: current_timestamp() - (current_timestamp() - self.performance_monitor.total_query_time_us),
                            layers_consulted,
                            performance_stats,
                        });
                    }
                }
                RoutingDecision::RouteToLayers { .. } => {
                    // Continue to next layer
                }
            }
        }

        // Layer 3: Associative search
        if let Some(layer3_ref) = self.layers.get(&LayerId::Layer3) {
            let layer3 = layer3_ref.read().await;
            
            let layer_start = current_timestamp();
            let routing_decision = timeout(
                Duration::from_micros(self.routing_config.layer_timeout_us),
                layer3.search(query)
            ).await??;
            let layer_time = current_timestamp() - layer_start;

            layers_consulted.push(LayerId::Layer3);
            performance_stats.insert(
                "layer3_time_us".to_string(),
                serde_json::Value::Number(serde_json::Number::from(layer_time))
            );

            match routing_decision {
                RoutingDecision::FoundExact { results } |
                RoutingDecision::SearchComplete { results } |
                RoutingDecision::FoundPartial { results, .. } => {
                    all_results.extend(results);
                }
                RoutingDecision::RouteToLayers { .. } => {
                    // Continue to Layer 4 if available
                }
            }
        }

        // Layer 4: Context prediction
        if let Some(layer4_ref) = self.layers.get(&LayerId::Layer4) {
            let layer4 = layer4_ref.read().await;
            
            let layer_start = current_timestamp();
            let routing_decision = timeout(
                Duration::from_micros(self.routing_config.layer_timeout_us),
                layer4.search(query)
            ).await??;
            let layer_time = current_timestamp() - layer_start;

            layers_consulted.push(LayerId::Layer4);
            performance_stats.insert(
                "layer4_time_us".to_string(),
                serde_json::Value::Number(serde_json::Number::from(layer_time))
            );

            match routing_decision {
                RoutingDecision::FoundExact { results } |
                RoutingDecision::SearchComplete { results } |
                RoutingDecision::FoundPartial { results, .. } => {
                    all_results.extend(results);
                }
                RoutingDecision::RouteToLayers { .. } => {
                    // No more layers available
                }
            }
        }

        // Sort results by confidence/relevance
        all_results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        // Limit results
        if all_results.len() > query.max_results {
            all_results.truncate(query.max_results);
        }

        let total_found = all_results.len();
        Ok(UniversalSearchResults {
            results: all_results,
            query: query.clone(),
            total_found,
            search_time_us: current_timestamp() - (current_timestamp() - self.performance_monitor.total_query_time_us),
            layers_consulted,
            performance_stats,
        })
    }

    async fn search_parallel(&self, query: &UniversalSearchQuery) -> LayerResult<UniversalSearchResults> {
        // TODO: Implement parallel search across all layers
        // For now, fall back to sequential
        self.search_sequential(query).await
    }

    async fn search_adaptive(&self, query: &UniversalSearchQuery) -> LayerResult<UniversalSearchResults> {
        // TODO: Implement adaptive routing based on query analysis
        // For now, fall back to sequential
        self.search_sequential(query).await
    }

    async fn search_custom(
        &self, 
        query: &UniversalSearchQuery, 
        router: fn(&UniversalSearchQuery) -> Vec<LayerId>
    ) -> LayerResult<UniversalSearchResults> {
        let _layer_order = router(query);
        // TODO: Implement custom routing
        self.search_sequential(query).await
    }

    /// Get health status of all layers
    pub async fn health_check(&self) -> HashMap<LayerId, LayerHealth> {
        let mut health_map = HashMap::new();
        
        for (&layer_id, layer_ref) in &self.layers {
            let layer = layer_ref.read().await;
            match layer.health_check().await {
                Ok(health) => {
                    health_map.insert(layer_id, health);
                }
                Err(e) => {
                    health_map.insert(layer_id, LayerHealth {
                        layer_id,
                        status: HealthStatus::Unhealthy,
                        uptime_seconds: 0,
                        last_error: Some(e.to_string()),
                        resource_usage: ResourceUsage {
                            memory_bytes: 0,
                            cpu_percent: 0.0,
                            active_connections: 0,
                            pending_operations: 0,
                        },
                        diagnostics: HashMap::new(),
                    });
                }
            }
        }
        
        health_map
    }

    /// Get performance statistics
    pub fn get_performance_stats(&self) -> &PerformanceMonitor {
        &self.performance_monitor
    }

    /// Shutdown all layers gracefully
    pub async fn shutdown(&mut self) -> LayerResult<()> {
        for (layer_id, layer_ref) in &self.layers {
            let mut layer = layer_ref.write().await;
            if let Err(e) = layer.shutdown().await {
                log::error!("Error shutting down {}: {}", layer_id.as_str(), e);
            }
        }
        self.layers.clear();
        Ok(())
    }
}

impl Default for MfnOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    // Mock layer for testing
    struct MockLayer {
        layer_id: LayerId,
        memories: HashMap<MemoryId, UniversalMemory>,
        config: LayerConfig,
    }

    impl MockLayer {
        fn new(layer_id: LayerId) -> Self {
            Self {
                layer_id,
                memories: HashMap::new(),
                config: LayerConfig {
                    layer_id,
                    ..Default::default()
                },
            }
        }
    }

    #[async_trait]
    impl MfnLayer for MockLayer {
        fn layer_id(&self) -> LayerId { self.layer_id }
        fn layer_name(&self) -> &str { "MockLayer" }
        fn version(&self) -> &str { "1.0.0" }

        async fn add_memory(&mut self, memory: UniversalMemory) -> LayerResult<()> {
            self.memories.insert(memory.id, memory);
            Ok(())
        }

        async fn add_association(&mut self, _association: UniversalAssociation) -> LayerResult<()> {
            Ok(())
        }

        async fn get_memory(&self, id: MemoryId) -> LayerResult<UniversalMemory> {
            self.memories.get(&id)
                .cloned()
                .ok_or(LayerError::MemoryNotFound { id })
        }

        async fn remove_memory(&mut self, id: MemoryId) -> LayerResult<()> {
            self.memories.remove(&id);
            Ok(())
        }

        async fn search(&self, _query: &UniversalSearchQuery) -> LayerResult<RoutingDecision> {
            Ok(RoutingDecision::SearchComplete { results: vec![] })
        }

        async fn get_performance(&self) -> LayerResult<LayerPerformance> {
            Ok(LayerPerformance {
                layer_id: self.layer_id,
                processing_time_us: 1000,
                memory_usage_bytes: 1024,
                operations_performed: 1,
                cache_hit_rate: Some(0.5),
                custom_metrics: HashMap::new(),
            })
        }

        async fn health_check(&self) -> LayerResult<LayerHealth> {
            Ok(LayerHealth {
                layer_id: self.layer_id,
                status: HealthStatus::Healthy,
                uptime_seconds: 3600,
                last_error: None,
                resource_usage: ResourceUsage {
                    memory_bytes: 1024,
                    cpu_percent: 5.0,
                    active_connections: 1,
                    pending_operations: 0,
                },
                diagnostics: HashMap::new(),
            })
        }

        async fn start(&mut self, config: LayerConfig) -> LayerResult<()> {
            self.config = config;
            Ok(())
        }

        async fn shutdown(&mut self) -> LayerResult<()> {
            Ok(())
        }

        fn get_config(&self) -> &LayerConfig {
            &self.config
        }
    }

    #[tokio::test]
    async fn test_orchestrator_registration() {
        let mut orchestrator = MfnOrchestrator::new();
        let layer = Box::new(MockLayer::new(LayerId::Layer1));
        
        orchestrator.register_layer(layer).await.unwrap();
        assert!(orchestrator.layers.contains_key(&LayerId::Layer1));
    }

    #[tokio::test]
    async fn test_orchestrator_health_check() {
        let mut orchestrator = MfnOrchestrator::new();
        let layer = Box::new(MockLayer::new(LayerId::Layer1));
        
        orchestrator.register_layer(layer).await.unwrap();
        
        let health = orchestrator.health_check().await;
        assert!(health.contains_key(&LayerId::Layer1));
        assert_eq!(health[&LayerId::Layer1].status, HealthStatus::Healthy);
    }
}