use async_trait::async_trait;
use mfn_core::{
    layer_interface::*,
    memory_types::*,
};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

pub mod socket_server;

/// Simple but functional Context Prediction Engine implementation
pub struct ContextPredictionLayer {
    config: LayerConfig,
    context_window: Arc<RwLock<VecDeque<MemoryAccess>>>,
    patterns: Arc<RwLock<HashMap<String, TemporalPattern>>>,
    prediction_cache: Arc<RwLock<HashMap<String, CachedPrediction>>>,
    memories: Arc<RwLock<HashMap<MemoryId, UniversalMemory>>>,
    performance: Arc<RwLock<LayerPerformance>>,
    uptime_start: SystemTime,
}

#[derive(Debug, Clone)]
struct CachedPrediction {
    results: Vec<PredictionResult>,
    timestamp: Timestamp,
    ttl_ms: u64,
}

impl ContextPredictionLayer {
    pub fn new() -> Self {
        let now = SystemTime::now();
        Self {
            config: LayerConfig {
                layer_id: LayerId::Layer4,
                max_memory_count: Some(100_000),
                max_association_count: Some(500_000),
                default_timeout_us: 50_000, // 50ms for predictions
                enable_caching: true,
                cache_size_limit: Some(1_000),
                performance_monitoring: true,
                custom_params: HashMap::new(),
            },
            context_window: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
            patterns: Arc::new(RwLock::new(HashMap::new())),
            prediction_cache: Arc::new(RwLock::new(HashMap::new())),
            memories: Arc::new(RwLock::new(HashMap::new())),
            performance: Arc::new(RwLock::new(LayerPerformance {
                layer_id: LayerId::Layer4,
                processing_time_us: 0,
                memory_usage_bytes: 0,
                operations_performed: 0,
                cache_hit_rate: Some(0.0),
                custom_metrics: HashMap::new(),
            })),
            uptime_start: now,
        }
    }

    /// Generate simple predictions based on recent access patterns
    async fn generate_predictions(&self, context: &ContextWindow) -> LayerResult<Vec<PredictionResult>> {
        let memories = self.memories.read();
        let mut predictions = Vec::new();
        
        // Simple prediction strategy: predict memories that are semantically similar or recently accessed
        if !context.recent_accesses.is_empty() {
            let recent_memory_ids: Vec<MemoryId> = context.recent_accesses
                .iter()
                .map(|access| access.memory_id)
                .collect();
            
            // Find memories with similar content or tags
            for (memory_id, memory) in memories.iter() {
                if recent_memory_ids.contains(memory_id) {
                    continue; // Skip recently accessed memories
                }
                
                // Simple prediction based on tag overlap with recent memories
                let mut confidence: f64 = 0.0;
                for recent_id in &recent_memory_ids {
                    if let Some(recent_memory) = memories.get(recent_id) {
                        let tag_overlap = calculate_tag_overlap(&memory.tags, &recent_memory.tags);
                        confidence = confidence.max(tag_overlap);
                    }
                }
                
                if confidence > 0.3 {
                    predictions.push(PredictionResult {
                        predicted_memory: memory.clone(),
                        confidence,
                        prediction_type: PredictionType::ContextualInference,
                        contributing_patterns: vec!["tag_similarity".to_string()],
                    });
                }
            }
        }
        
        // Sort by confidence and limit results
        predictions.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        predictions.truncate(10);
        
        Ok(predictions)
    }

    fn update_performance(&self, operation_time_us: u64) {
        let mut perf = self.performance.write();
        perf.processing_time_us = operation_time_us;
        perf.operations_performed += 1;
    }
}

fn calculate_tag_overlap(tags1: &[String], tags2: &[String]) -> f64 {
    if tags1.is_empty() || tags2.is_empty() {
        return 0.0;
    }
    
    let overlap_count = tags1.iter()
        .filter(|tag| tags2.contains(tag))
        .count();
    
    overlap_count as f64 / (tags1.len().max(tags2.len()) as f64)
}

#[async_trait]
impl MfnLayer for ContextPredictionLayer {
    fn layer_id(&self) -> LayerId {
        LayerId::Layer4
    }

    fn layer_name(&self) -> &str {
        "Context Prediction Engine"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    async fn add_memory(&mut self, memory: UniversalMemory) -> LayerResult<()> {
        let start_time = current_timestamp();
        
        {
            let mut memories = self.memories.write();
            memories.insert(memory.id, memory.clone());
        }
        
        // Update context with this memory access
        let access = MemoryAccess {
            memory_id: memory.id,
            access_type: AccessType::Write,
            timestamp: start_time,
            context_metadata: HashMap::new(),
        };
        
        self.update_context(access).await?;
        
        let processing_time = current_timestamp() - start_time;
        self.update_performance(processing_time);
        
        Ok(())
    }

    async fn add_association(&mut self, _association: UniversalAssociation) -> LayerResult<()> {
        // For this minimal implementation, we don't actively use associations
        // but we acknowledge them for API compliance
        let start_time = current_timestamp();
        let processing_time = current_timestamp() - start_time;
        self.update_performance(processing_time);
        Ok(())
    }

    async fn get_memory(&self, id: MemoryId) -> LayerResult<UniversalMemory> {
        let memories = self.memories.read();
        memories.get(&id)
            .cloned()
            .ok_or(LayerError::MemoryNotFound { id })
    }

    async fn remove_memory(&mut self, id: MemoryId) -> LayerResult<()> {
        let mut memories = self.memories.write();
        memories.remove(&id)
            .ok_or(LayerError::MemoryNotFound { id })?;
        Ok(())
    }

    async fn search(&self, query: &UniversalSearchQuery) -> LayerResult<RoutingDecision> {
        let start_time = current_timestamp();
        
        // Create context from query
        let context = ContextWindow {
            recent_accesses: self.context_window.read().iter().cloned().collect(),
            temporal_patterns: self.patterns.read().values().cloned().collect(),
            user_context: HashMap::new(),
            window_size_ms: 30_000, // 30 seconds
        };
        
        // Generate predictions
        let predictions = self.generate_predictions(&context).await?;
        
        // Convert predictions to search results
        let mut results = Vec::new();
        for prediction in predictions {
            results.push(UniversalSearchResult {
                memory: prediction.predicted_memory,
                confidence: prediction.confidence,
                path: Vec::new(), // No search path for predictions
                layer_origin: LayerId::Layer4,
                search_time_us: current_timestamp() - start_time,
            });
        }
        
        let processing_time = current_timestamp() - start_time;
        self.update_performance(processing_time);
        
        if results.is_empty() {
            Ok(RoutingDecision::RouteToLayers {
                suggested_layers: vec![LayerId::Layer1, LayerId::Layer2, LayerId::Layer3],
                routing_confidence: 0.5,
            })
        } else {
            Ok(RoutingDecision::FoundPartial {
                results,
                continue_search: true,
                suggested_layers: vec![LayerId::Layer1, LayerId::Layer2, LayerId::Layer3],
            })
        }
    }

    async fn get_performance(&self) -> LayerResult<LayerPerformance> {
        Ok(self.performance.read().clone())
    }

    async fn health_check(&self) -> LayerResult<LayerHealth> {
        let uptime = self.uptime_start.elapsed()
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        
        Ok(LayerHealth {
            layer_id: LayerId::Layer4,
            status: HealthStatus::Healthy,
            uptime_seconds: uptime,
            last_error: None,
            resource_usage: ResourceUsage {
                memory_bytes: 1024 * 1024, // Estimated 1MB
                cpu_percent: 5.0,
                active_connections: 0,
                pending_operations: 0,
            },
            diagnostics: {
                let mut diag = HashMap::new();
                diag.insert("context_window_size".to_string(), 
                          serde_json::Value::Number(serde_json::Number::from(self.context_window.read().len())));
                diag.insert("pattern_count".to_string(), 
                          serde_json::Value::Number(serde_json::Number::from(self.patterns.read().len())));
                diag.insert("memory_count".to_string(), 
                          serde_json::Value::Number(serde_json::Number::from(self.memories.read().len())));
                diag
            },
        })
    }

    async fn start(&mut self, config: LayerConfig) -> LayerResult<()> {
        self.config = config;
        Ok(())
    }

    async fn shutdown(&mut self) -> LayerResult<()> {
        // Clear caches and data structures
        self.context_window.write().clear();
        self.patterns.write().clear();
        self.prediction_cache.write().clear();
        Ok(())
    }

    fn get_config(&self) -> &LayerConfig {
        &self.config
    }
}

#[async_trait]
impl ContextPredictionEngine for ContextPredictionLayer {
    async fn predict_next(&self, context: &ContextWindow) -> LayerResult<Vec<PredictionResult>> {
        self.generate_predictions(context).await
    }

    async fn learn_pattern(&mut self, access_sequence: &[MemoryAccess]) -> LayerResult<()> {
        if access_sequence.len() < 2 {
            return Ok(());
        }
        
        // Extract simple sequential patterns
        let pattern_id = format!("seq_{}", uuid::Uuid::new_v4());
        let memory_sequence: Vec<MemoryId> = access_sequence.iter()
            .map(|access| access.memory_id)
            .collect();
        
        // Calculate average interval
        let intervals: Vec<u64> = access_sequence.windows(2)
            .map(|window| window[1].timestamp - window[0].timestamp)
            .collect();
        
        let average_interval_ms = if intervals.is_empty() {
            1000 // Default 1 second
        } else {
            (intervals.iter().sum::<u64>() / intervals.len() as u64) / 1000
        };
        
        let pattern = TemporalPattern {
            pattern_id: pattern_id.clone(),
            memory_sequence,
            average_interval_ms,
            confidence: 0.7, // Default confidence
            occurrences: 1,
        };
        
        self.patterns.write().insert(pattern_id, pattern);
        Ok(())
    }

    async fn get_context_state(&self) -> LayerResult<ContextState> {
        Ok(ContextState {
            active_patterns: self.patterns.read().len(),
            context_window_size: self.context_window.read().len(),
            prediction_accuracy: 0.75, // Estimated
            learning_rate: 0.1,
        })
    }

    async fn update_context(&mut self, access: MemoryAccess) -> LayerResult<()> {
        let mut window = self.context_window.write();
        
        // Add new access to context window
        window.push_back(access);
        
        // Maintain window size (keep last 100 accesses)
        const MAX_WINDOW_SIZE: usize = 100;
        while window.len() > MAX_WINDOW_SIZE {
            window.pop_front();
        }
        
        Ok(())
    }
}

impl Default for ContextPredictionLayer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_layer_basic_functionality() {
        let mut layer = ContextPredictionLayer::new();
        
        // Test layer identity
        assert_eq!(layer.layer_id(), LayerId::Layer4);
        assert_eq!(layer.layer_name(), "Context Prediction Engine");
        
        // Test health check
        let health = layer.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_memory_operations() {
        let mut layer = ContextPredictionLayer::new();
        
        let memory = UniversalMemory::new(1, "Test content".to_string());
        
        // Test adding memory
        layer.add_memory(memory.clone()).await.unwrap();
        
        // Test retrieving memory
        let retrieved = layer.get_memory(1).await.unwrap();
        assert_eq!(retrieved.content, "Test content");
        
        // Test removing memory
        layer.remove_memory(1).await.unwrap();
        
        // Should fail to retrieve removed memory
        assert!(layer.get_memory(1).await.is_err());
    }

    #[tokio::test]
    async fn test_prediction_functionality() {
        let mut layer = ContextPredictionLayer::new();
        
        // Add some memories
        let memory1 = UniversalMemory::new(1, "AI research".to_string())
            .with_tags(vec!["AI".to_string(), "research".to_string()]);
        let memory2 = UniversalMemory::new(2, "Machine learning".to_string())
            .with_tags(vec!["AI".to_string(), "ML".to_string()]);
        
        layer.add_memory(memory1).await.unwrap();
        layer.add_memory(memory2).await.unwrap();
        
        // Create context with recent access
        let access = MemoryAccess {
            memory_id: 1,
            access_type: AccessType::Read,
            timestamp: current_timestamp(),
            context_metadata: HashMap::new(),
        };
        
        let context = ContextWindow {
            recent_accesses: vec![access],
            temporal_patterns: vec![],
            user_context: HashMap::new(),
            window_size_ms: 30_000,
        };
        
        // Test prediction
        let predictions = layer.predict_next(&context).await.unwrap();
        assert!(!predictions.is_empty());
    }
}