// Integration test for MFN Orchestrator Memory Routing
// Tests end-to-end memory query flow through all 4 layers

use mfn_core::*;
use async_trait::async_trait;
use std::collections::HashMap;

/// Mock implementation of Layer 1 (Immediate Flow Registry)
struct MockLayer1 {
    layer_id: LayerId,
    memories: HashMap<MemoryId, UniversalMemory>,
    config: LayerConfig,
}

impl MockLayer1 {
    fn new() -> Self {
        Self {
            layer_id: LayerId::Layer1,
            memories: HashMap::new(),
            config: LayerConfig {
                layer_id: LayerId::Layer1,
                ..Default::default()
            },
        }
    }
}

#[async_trait]
impl MfnLayer for MockLayer1 {
    fn layer_id(&self) -> LayerId { self.layer_id }
    fn layer_name(&self) -> &str { "MockLayer1-IFR" }
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

    async fn search(&self, query: &UniversalSearchQuery) -> LayerResult<RoutingDecision> {
        // Layer 1: Exact match only
        if let Some(content) = &query.content {
            for memory in self.memories.values() {
                if memory.content == *content {
                    return Ok(RoutingDecision::FoundExact {
                        results: vec![UniversalSearchResult {
                            memory: memory.clone(),
                            confidence: 1.0,
                            search_time_us: 100,
                            layer_origin: LayerId::Layer1,
                            path: vec![],
                        }],
                    });
                }
            }
        }

        // No exact match, route to Layer 2 for similarity
        Ok(RoutingDecision::RouteToLayers {
            suggested_layers: vec![LayerId::Layer2],
            routing_confidence: 0.9,
        })
    }

    async fn get_performance(&self) -> LayerResult<LayerPerformance> {
        Ok(LayerPerformance {
            layer_id: self.layer_id,
            processing_time_us: 100,
            memory_usage_bytes: 1024,
            operations_performed: 1,
            cache_hit_rate: Some(0.9),
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

/// Mock implementation of Layer 2 (Dynamic Similarity Reservoir)
struct MockLayer2 {
    layer_id: LayerId,
    memories: HashMap<MemoryId, UniversalMemory>,
    config: LayerConfig,
}

impl MockLayer2 {
    fn new() -> Self {
        Self {
            layer_id: LayerId::Layer2,
            memories: HashMap::new(),
            config: LayerConfig {
                layer_id: LayerId::Layer2,
                ..Default::default()
            },
        }
    }

    fn similarity(&self, content1: &str, content2: &str) -> f64 {
        // Simple word-based similarity
        let words1: std::collections::HashSet<_> = content1
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();
        let words2: std::collections::HashSet<_> = content2
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();

        let intersection = words1.intersection(&words2).count();
        let union = words1.union(&words2).count();

        if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
    }
}

#[async_trait]
impl MfnLayer for MockLayer2 {
    fn layer_id(&self) -> LayerId { self.layer_id }
    fn layer_name(&self) -> &str { "MockLayer2-DSR" }
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

    async fn search(&self, query: &UniversalSearchQuery) -> LayerResult<RoutingDecision> {
        // Layer 2: Similarity-based search
        if let Some(content) = &query.content {
            let mut results = Vec::new();

            for memory in self.memories.values() {
                let sim = self.similarity(content, &memory.content);
                if sim >= query.min_weight {
                    results.push(UniversalSearchResult {
                        memory: memory.clone(),
                        confidence: sim,
                        search_time_us: 500,
                        layer_origin: LayerId::Layer2,
                        path: vec![],
                    });
                }
            }

            results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
            results.truncate(query.max_results);

            if !results.is_empty() {
                return Ok(RoutingDecision::FoundPartial {
                    results,
                    continue_search: true,
                    suggested_layers: vec![LayerId::Layer3],
                });
            }
        }

        // Route to Layer 3 for associative search
        Ok(RoutingDecision::RouteToLayers {
            suggested_layers: vec![LayerId::Layer3],
            routing_confidence: 0.7,
        })
    }

    async fn get_performance(&self) -> LayerResult<LayerPerformance> {
        Ok(LayerPerformance {
            layer_id: self.layer_id,
            processing_time_us: 500,
            memory_usage_bytes: 2048,
            operations_performed: 1,
            cache_hit_rate: Some(0.7),
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
                memory_bytes: 2048,
                cpu_percent: 10.0,
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

/// Mock implementation of Layer 3 (Associative Link Mesh)
struct MockLayer3 {
    layer_id: LayerId,
    memories: HashMap<MemoryId, UniversalMemory>,
    associations: Vec<UniversalAssociation>,
    config: LayerConfig,
}

impl MockLayer3 {
    fn new() -> Self {
        Self {
            layer_id: LayerId::Layer3,
            memories: HashMap::new(),
            associations: Vec::new(),
            config: LayerConfig {
                layer_id: LayerId::Layer3,
                ..Default::default()
            },
        }
    }
}

#[async_trait]
impl MfnLayer for MockLayer3 {
    fn layer_id(&self) -> LayerId { self.layer_id }
    fn layer_name(&self) -> &str { "MockLayer3-ALM" }
    fn version(&self) -> &str { "1.0.0" }

    async fn add_memory(&mut self, memory: UniversalMemory) -> LayerResult<()> {
        self.memories.insert(memory.id, memory);
        Ok(())
    }

    async fn add_association(&mut self, association: UniversalAssociation) -> LayerResult<()> {
        self.associations.push(association);
        Ok(())
    }

    async fn get_memory(&self, id: MemoryId) -> LayerResult<UniversalMemory> {
        self.memories.get(&id)
            .cloned()
            .ok_or(LayerError::MemoryNotFound { id })
    }

    async fn remove_memory(&mut self, id: MemoryId) -> LayerResult<()> {
        self.memories.remove(&id);
        self.associations.retain(|a| a.from_memory_id != id && a.to_memory_id != id);
        Ok(())
    }

    async fn search(&self, query: &UniversalSearchQuery) -> LayerResult<RoutingDecision> {
        // Layer 3: Associative search through graph
        let mut results = Vec::new();

        // Find memories with matching tags
        if !query.tags.is_empty() {
            for memory in self.memories.values() {
                let matching_tags = memory.tags.iter()
                    .filter(|t| query.tags.contains(t))
                    .count();

                if matching_tags > 0 {
                    let confidence = matching_tags as f64 / query.tags.len() as f64;
                    results.push(UniversalSearchResult {
                        memory: memory.clone(),
                        confidence,
                        search_time_us: 1000,
                        layer_origin: LayerId::Layer3,
                        path: vec![],
                    });
                }
            }
        }

        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        results.truncate(query.max_results);

        if !results.is_empty() {
            Ok(RoutingDecision::SearchComplete { results })
        } else {
            Ok(RoutingDecision::RouteToLayers {
                suggested_layers: vec![LayerId::Layer4],
                routing_confidence: 0.5,
            })
        }
    }

    async fn get_performance(&self) -> LayerResult<LayerPerformance> {
        Ok(LayerPerformance {
            layer_id: self.layer_id,
            processing_time_us: 1000,
            memory_usage_bytes: 4096,
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
                memory_bytes: 4096,
                cpu_percent: 15.0,
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

/// Mock implementation of Layer 4 (Context Prediction Engine)
struct MockLayer4 {
    layer_id: LayerId,
    memories: HashMap<MemoryId, UniversalMemory>,
    config: LayerConfig,
}

impl MockLayer4 {
    fn new() -> Self {
        Self {
            layer_id: LayerId::Layer4,
            memories: HashMap::new(),
            config: LayerConfig {
                layer_id: LayerId::Layer4,
                ..Default::default()
            },
        }
    }
}

#[async_trait]
impl MfnLayer for MockLayer4 {
    fn layer_id(&self) -> LayerId { self.layer_id }
    fn layer_name(&self) -> &str { "MockLayer4-CPE" }
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
        // Layer 4: Context-based predictions
        // For now, return empty results
        Ok(RoutingDecision::SearchComplete { results: vec![] })
    }

    async fn get_performance(&self) -> LayerResult<LayerPerformance> {
        Ok(LayerPerformance {
            layer_id: self.layer_id,
            processing_time_us: 2000,
            memory_usage_bytes: 8192,
            operations_performed: 1,
            cache_hit_rate: Some(0.3),
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
                memory_bytes: 8192,
                cpu_percent: 20.0,
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
async fn test_orchestrator_exact_match_layer1() {
    let mut orchestrator = MfnOrchestrator::new();

    // Register Layer 1
    let mut layer1 = MockLayer1::new();
    let memory = UniversalMemory::new(1, "exact match content".to_string());
    layer1.add_memory(memory.clone()).await.unwrap();

    orchestrator.register_layer(Box::new(layer1)).await.unwrap();

    // Search for exact match
    let query = UniversalSearchQuery {
        content: Some("exact match content".to_string()),
        ..Default::default()
    };

    let results = orchestrator.search(query).await.unwrap();

    assert_eq!(results.total_found, 1);
    assert_eq!(results.results[0].memory.id, 1);
    assert_eq!(results.results[0].confidence, 1.0);
    assert_eq!(results.results[0].layer_origin, LayerId::Layer1);
    assert!(results.layers_consulted.contains(&LayerId::Layer1));
}

#[tokio::test]
async fn test_orchestrator_similarity_layer2() {
    let mut orchestrator = MfnOrchestrator::new();

    // Register Layer 1 and 2
    orchestrator.register_layer(Box::new(MockLayer1::new())).await.unwrap();

    let mut layer2 = MockLayer2::new();
    let memory = UniversalMemory::new(2, "hello world test".to_string());
    layer2.add_memory(memory).await.unwrap();

    orchestrator.register_layer(Box::new(layer2)).await.unwrap();

    // Search for similar (not exact)
    let query = UniversalSearchQuery {
        content: Some("hello test".to_string()),
        min_weight: 0.3,
        ..Default::default()
    };

    let results = orchestrator.search(query).await.unwrap();

    assert!(results.total_found > 0);
    assert_eq!(results.results[0].layer_origin, LayerId::Layer2);
    assert!(results.layers_consulted.contains(&LayerId::Layer2));
}

#[tokio::test]
async fn test_orchestrator_associative_layer3() {
    let mut orchestrator = MfnOrchestrator::new();

    // Register all 3 layers
    orchestrator.register_layer(Box::new(MockLayer1::new())).await.unwrap();
    orchestrator.register_layer(Box::new(MockLayer2::new())).await.unwrap();

    let mut layer3 = MockLayer3::new();
    let memory = UniversalMemory::new(3, "test content".to_string())
        .with_tags(vec!["ai".to_string(), "memory".to_string()]);
    layer3.add_memory(memory).await.unwrap();

    orchestrator.register_layer(Box::new(layer3)).await.unwrap();

    // Search by tags (associative)
    let query = UniversalSearchQuery {
        tags: vec!["ai".to_string()],
        ..Default::default()
    };

    let results = orchestrator.search(query).await.unwrap();

    assert!(results.total_found > 0);
    assert_eq!(results.results[0].layer_origin, LayerId::Layer3);
    assert!(results.layers_consulted.contains(&LayerId::Layer3));
}

#[tokio::test]
async fn test_orchestrator_all_4_layers_sequential() {
    let mut orchestrator = MfnOrchestrator::new()
        .with_routing_config(RoutingConfig {
            default_strategy: RoutingStrategy::Sequential,
            ..Default::default()
        });

    // Register all 4 layers
    orchestrator.register_layer(Box::new(MockLayer1::new())).await.unwrap();
    orchestrator.register_layer(Box::new(MockLayer2::new())).await.unwrap();
    orchestrator.register_layer(Box::new(MockLayer3::new())).await.unwrap();
    orchestrator.register_layer(Box::new(MockLayer4::new())).await.unwrap();

    // Search with no results - should route through all layers
    let query = UniversalSearchQuery {
        content: Some("nonexistent content".to_string()),
        ..Default::default()
    };

    let results = orchestrator.search(query).await.unwrap();

    // All 4 layers should have been consulted
    assert!(results.layers_consulted.contains(&LayerId::Layer1));
    assert!(results.layers_consulted.contains(&LayerId::Layer2));
    assert!(results.layers_consulted.contains(&LayerId::Layer3));
    assert!(results.layers_consulted.contains(&LayerId::Layer4));
}

#[tokio::test]
async fn test_orchestrator_parallel_routing() {
    let mut orchestrator = MfnOrchestrator::new()
        .with_routing_config(RoutingConfig {
            default_strategy: RoutingStrategy::Parallel,
            ..Default::default()
        });

    // Register layers with data
    let mut layer1 = MockLayer1::new();
    layer1.add_memory(UniversalMemory::new(1, "layer1 data".to_string())).await.unwrap();
    orchestrator.register_layer(Box::new(layer1)).await.unwrap();

    let mut layer2 = MockLayer2::new();
    layer2.add_memory(UniversalMemory::new(2, "layer2 data".to_string())).await.unwrap();
    orchestrator.register_layer(Box::new(layer2)).await.unwrap();

    let query = UniversalSearchQuery {
        content: Some("layer1 data".to_string()),
        ..Default::default()
    };

    let results = orchestrator.search(query).await.unwrap();

    // Should find exact match from layer 1
    assert!(results.total_found > 0);
    // Both layers should be consulted in parallel
    assert_eq!(results.layers_consulted.len(), 2);
}

#[tokio::test]
async fn test_orchestrator_health_check() {
    let mut orchestrator = MfnOrchestrator::new();

    orchestrator.register_layer(Box::new(MockLayer1::new())).await.unwrap();
    orchestrator.register_layer(Box::new(MockLayer2::new())).await.unwrap();

    let health = orchestrator.health_check().await;

    assert_eq!(health.len(), 2);
    assert_eq!(health[&LayerId::Layer1].status, HealthStatus::Healthy);
    assert_eq!(health[&LayerId::Layer2].status, HealthStatus::Healthy);
}

#[tokio::test]
async fn test_orchestrator_add_memory_to_all_layers() {
    let mut orchestrator = MfnOrchestrator::new();

    orchestrator.register_layer(Box::new(MockLayer1::new())).await.unwrap();
    orchestrator.register_layer(Box::new(MockLayer2::new())).await.unwrap();
    orchestrator.register_layer(Box::new(MockLayer3::new())).await.unwrap();

    let memory = UniversalMemory::new(100, "test memory".to_string())
        .with_tags(vec!["test".to_string()]);

    orchestrator.add_memory(memory).await.unwrap();

    // Verify memory is searchable (meaning it was added to layers)
    let query = UniversalSearchQuery {
        content: Some("test memory".to_string()),
        ..Default::default()
    };

    let results = orchestrator.search(query).await.unwrap();
    assert!(results.total_found > 0);
}
