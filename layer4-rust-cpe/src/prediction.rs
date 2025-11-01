// Layer 4 CPE - Context Prediction Engine Core
// Main prediction engine that coordinates temporal analysis with context awareness

use mfn_core::{
    MfnLayer, LayerId, LayerResult, LayerError, LayerConfig, LayerHealth, LayerPerformance,
    HealthStatus, ResourceUsage, RoutingDecision, UniversalMemory, UniversalAssociation,
    UniversalSearchQuery, UniversalSearchResult, MemoryId, Weight, current_timestamp,
    ContextPredictionEngine, ContextWindow, MemoryAccess as CoreMemoryAccess,
    PredictionResult as CorePredictionResult, PredictionType as CorePredictionType, ContextState,
};
use mfn_core::memory_types::Timestamp;
use mfn_core::layer_interface::AccessType as CoreAccessType;

use crate::temporal::{
    TemporalAnalyzer, TemporalConfig, MemoryAccess, AccessType, PredictionContext, 
    PredictionResult, PredictionType, AnalyzerStatistics,
};

use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

/// Configuration for the Context Prediction Engine
#[derive(Debug, Clone)]
pub struct ContextPredictionConfig {
    pub max_window_size: usize,
    pub min_pattern_length: usize,
    pub max_pattern_length: usize,
    pub min_frequency_threshold: usize,
    pub transition_threshold: f32,
    pub cache_size: usize,
    pub cache_ttl: std::time::Duration,
    pub enable_session_tracking: bool,
    pub max_prediction_results: usize,
}

impl Default for ContextPredictionConfig {
    fn default() -> Self {
        Self {
            max_window_size: 1000,
            min_pattern_length: 2,
            max_pattern_length: 10,
            min_frequency_threshold: 3,
            transition_threshold: 0.1,
            cache_size: 1000,
            cache_ttl: std::time::Duration::from_secs(300),
            enable_session_tracking: true,
            max_prediction_results: 50,
        }
    }
}

/// Layer 4: Context Prediction Engine
/// Analyzes temporal patterns in memory access and predicts future memory needs
pub struct ContextPredictionLayer {
    /// Core configuration
    config: LayerConfig,
    
    /// Temporal pattern analyzer
    analyzer: Arc<Mutex<TemporalAnalyzer>>,
    
    /// Context window for recent accesses
    context_window: Arc<RwLock<VecDeque<CoreMemoryAccess>>>,
    
    /// Performance metrics
    performance_metrics: Arc<RwLock<CPEPerformanceMetrics>>,
    
    /// Layer health status
    health_status: Arc<RwLock<LayerHealth>>,
    
    /// Prediction cache to avoid redundant computation
    prediction_cache: Arc<RwLock<HashMap<String, CachedPrediction>>>,
    
    /// Session tracking for context awareness
    session_tracker: Arc<RwLock<SessionTracker>>,
    
    /// Learning rate and adaptation parameters
    learning_params: LearningParameters,
}

/// Performance metrics specific to CPE
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CPEPerformanceMetrics {
    pub predictions_made: u64,
    pub patterns_detected: u64,
    pub accuracy_rate: f64,
    pub average_prediction_time_us: u64,
    pub cache_hit_rate: f64,
    pub context_window_size: usize,
    pub active_sessions: usize,
    pub memory_usage_mb: f64,
}

impl Default for CPEPerformanceMetrics {
    fn default() -> Self {
        Self {
            predictions_made: 0,
            patterns_detected: 0,
            accuracy_rate: 0.0,
            average_prediction_time_us: 0,
            cache_hit_rate: 0.0,
            context_window_size: 0,
            active_sessions: 0,
            memory_usage_mb: 0.0,
        }
    }
}

/// Cached prediction to avoid redundant computation
#[derive(Debug, Clone)]
struct CachedPrediction {
    pub predictions: Vec<CorePredictionResult>,
    pub timestamp: Timestamp,
    pub context_hash: u64,
    pub ttl_us: u64,
}

/// Session tracking for context-aware predictions
#[derive(Debug, Default)]
struct SessionTracker {
    pub active_sessions: HashMap<String, SessionContext>,
    pub global_patterns: HashMap<String, GlobalPattern>,
}

#[derive(Debug, Clone)]
struct SessionContext {
    pub session_id: String,
    pub start_time: Timestamp,
    pub last_activity: Timestamp,
    pub access_sequence: Vec<MemoryId>,
    pub user_metadata: HashMap<String, String>,
    pub prediction_accuracy: f64,
}

#[derive(Debug, Clone)]
struct GlobalPattern {
    pub pattern_id: String,
    pub frequency: u32,
    pub global_confidence: f64,
    pub contexts: Vec<String>,
}

/// Learning and adaptation parameters
#[derive(Debug, Clone)]
struct LearningParameters {
    pub learning_rate: f64,
    pub accuracy_decay_rate: f64,
    pub pattern_reinforcement_rate: f64,
    pub context_weight: f64,
    pub temporal_weight: f64,
    pub enable_online_learning: bool,
}

impl Default for LearningParameters {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            accuracy_decay_rate: 0.05,
            pattern_reinforcement_rate: 0.1,
            context_weight: 0.6,
            temporal_weight: 0.4,
            enable_online_learning: true,
        }
    }
}

impl ContextPredictionLayer {
    /// Create a new Context Prediction Layer with custom config
    pub async fn new(config: ContextPredictionConfig) -> LayerResult<Self> {
        let temporal_config = TemporalConfig {
            max_window_size: config.max_window_size,
            min_pattern_occurrences: config.min_frequency_threshold as u32,
            max_ngram_length: config.max_pattern_length,
            min_prediction_confidence: config.transition_threshold as f64,
            pattern_decay_rate: 0.95,
            max_sequence_gap_us: 60_000_000, // 60 seconds
            enable_statistical_modeling: true,
        };
        
        let analyzer = Arc::new(Mutex::new(TemporalAnalyzer::new(temporal_config)));
        
        let layer_config = LayerConfig {
            layer_id: LayerId::Layer4,
            max_memory_count: Some(100000),
            max_association_count: Some(50000),
            default_timeout_us: 5000000, // 5 seconds in microseconds
            enable_caching: true,
            cache_size_limit: Some(512 * 1024 * 1024), // 512MB
            performance_monitoring: true,
            custom_params: HashMap::new(),
        };

        let health_status = Arc::new(RwLock::new(LayerHealth {
            layer_id: LayerId::Layer4,
            status: HealthStatus::Healthy,
            uptime_seconds: 0,
            last_error: None,
            resource_usage: ResourceUsage {
                memory_bytes: 0,
                cpu_percent: 0.0,
                active_connections: 0,
                pending_operations: 0,
            },
            diagnostics: HashMap::new(),
        }));

        Ok(Self {
            config: layer_config,
            analyzer,
            context_window: Arc::new(RwLock::new(VecDeque::new())),
            performance_metrics: Arc::new(RwLock::new(CPEPerformanceMetrics::default())),
            health_status,
            prediction_cache: Arc::new(RwLock::new(HashMap::new())),
            learning_params: LearningParameters::default(),
            session_tracker: Arc::new(RwLock::new(SessionTracker::default())),
        })
    }

    /// Create a new Context Prediction Layer from core LayerConfig
    pub fn from_layer_config(config: LayerConfig) -> Self {
        let temporal_config = TemporalConfig::default();
        let analyzer = Arc::new(Mutex::new(TemporalAnalyzer::new(temporal_config)));
        
        let health_status = Arc::new(RwLock::new(LayerHealth {
            layer_id: LayerId::Layer4,
            status: HealthStatus::Starting,
            uptime_seconds: 0,
            last_error: None,
            resource_usage: ResourceUsage {
                memory_bytes: 0,
                cpu_percent: 0.0,
                active_connections: 0,
                pending_operations: 0,
            },
            diagnostics: HashMap::new(),
        }));

        Self {
            config,
            analyzer,
            context_window: Arc::new(RwLock::new(VecDeque::new())),
            performance_metrics: Arc::new(RwLock::new(CPEPerformanceMetrics::default())),
            health_status,
            prediction_cache: Arc::new(RwLock::new(HashMap::new())),
            session_tracker: Arc::new(RwLock::new(SessionTracker::default())),
            learning_params: LearningParameters::default(),
        }
    }


    /// Convert internal prediction results to core types
    fn convert_prediction_result(internal: PredictionResult) -> CorePredictionResult {
        let core_type = match internal.prediction_type {
            PredictionType::NGramBased => CorePredictionType::SequentialNext,
            PredictionType::MarkovChain => CorePredictionType::SequentialNext,
            PredictionType::PatternCompletion => CorePredictionType::PatternBased,
            PredictionType::StatisticalModel => CorePredictionType::ContextualInference,
            PredictionType::HybridEnsemble => CorePredictionType::AssociativeJump,
        };

        CorePredictionResult {
            predicted_memory: UniversalMemory {
                id: internal.memory_id,
                content: format!("Predicted memory {}", internal.memory_id),
                embedding: None,
                tags: vec!["predicted".to_string()],
                metadata: HashMap::new(),
                created_at: current_timestamp(),
                last_accessed: current_timestamp(),
                access_count: 0,
            },
            confidence: internal.confidence,
            prediction_type: core_type,
            contributing_patterns: internal.contributing_evidence,
        }
    }

    /// Calculate context hash for caching
    fn calculate_context_hash(context: &ContextWindow) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        
        // Hash recent accesses
        for access in &context.recent_accesses {
            access.memory_id.hash(&mut hasher);
            access.timestamp.hash(&mut hasher);
        }
        
        // Hash user context
        for (key, value) in &context.user_context {
            key.hash(&mut hasher);
            value.to_string().hash(&mut hasher);
        }
        
        hasher.finish()
    }

    /// Update prediction accuracy based on actual access
    async fn update_accuracy(&self, predicted: MemoryId, actual: MemoryId) {
        let mut metrics = self.performance_metrics.write();
        
        // Simple accuracy tracking
        let was_correct = predicted == actual;
        let new_accuracy = if metrics.predictions_made == 0 {
            if was_correct { 1.0 } else { 0.0 }
        } else {
            let current_weight = 1.0 / (metrics.predictions_made + 1) as f64;
            let historical_weight = 1.0 - current_weight;
            
            metrics.accuracy_rate * historical_weight + 
            if was_correct { current_weight } else { 0.0 }
        };
        
        metrics.accuracy_rate = new_accuracy;
        metrics.predictions_made += 1;
        
        // Apply decay to older predictions
        metrics.accuracy_rate *= 1.0 - self.learning_params.accuracy_decay_rate;
    }

    /// Clean expired cache entries
    async fn cleanup_cache(&self) {
        let mut cache = self.prediction_cache.write();
        let current_time = current_timestamp();
        
        cache.retain(|_, cached| {
            current_time.saturating_sub(cached.timestamp) < cached.ttl_us
        });
    }
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
        "0.1.0"
    }

    async fn add_memory(&mut self, memory: UniversalMemory) -> LayerResult<()> {
        // CPE doesn't store memories directly, but we can track access patterns
        let access = MemoryAccess {
            memory_id: memory.id,
            timestamp: current_timestamp(),
            access_type: AccessType::Write,
            user_context: None,
            session_id: None,
            confidence: 1.0,
        };

        let mut analyzer = self.analyzer.lock().await;
        analyzer.add_access(access);

        // Update context window
        let mut window = self.context_window.write();
        window.push_back(CoreMemoryAccess {
            memory_id: memory.id,
            access_type: CoreAccessType::Write,
            timestamp: current_timestamp(),
            context_metadata: HashMap::new(),
        });

        // Maintain window size (last 100 accesses)
        while window.len() > 100 {
            window.pop_front();
        }

        Ok(())
    }

    async fn add_association(&mut self, _association: UniversalAssociation) -> LayerResult<()> {
        // CPE tracks associations as part of access patterns
        // This could be enhanced to explicitly track association creation patterns
        Ok(())
    }

    async fn get_memory(&self, id: MemoryId) -> LayerResult<UniversalMemory> {
        // Record this as a read access
        let access = MemoryAccess {
            memory_id: id,
            timestamp: current_timestamp(),
            access_type: AccessType::Read,
            user_context: None,
            session_id: None,
            confidence: 1.0,
        };

        let mut analyzer = self.analyzer.lock().await;
        analyzer.add_access(access);

        // CPE doesn't store memories, so we return a placeholder
        Err(LayerError::MemoryNotFound { id })
    }

    async fn remove_memory(&mut self, _id: MemoryId) -> LayerResult<()> {
        // CPE doesn't store memories to remove
        Ok(())
    }

    async fn search(&self, query: &UniversalSearchQuery) -> LayerResult<RoutingDecision> {
        let start_time = current_timestamp();

        // Track this search as an access pattern
        if let Some(first_start_id) = query.start_memory_ids.first() {
            let access = MemoryAccess {
                memory_id: *first_start_id,
                timestamp: start_time,
                access_type: AccessType::Search,
                user_context: None,
                session_id: None,
                confidence: 1.0,
            };

            let mut analyzer = self.analyzer.lock().await;
            analyzer.add_access(access);
        }

        // CPE provides predictions rather than direct search results
        // Convert query into context for prediction
        let recent_sequence = if query.start_memory_ids.is_empty() {
            None
        } else {
            Some(query.start_memory_ids.clone())
        };

        let context = PredictionContext {
            recent_sequence,
            current_timestamp: current_timestamp(),
            user_context: None,
            session_id: None,
            max_predictions: query.max_results,
        };

        let analyzer = self.analyzer.lock().await;
        let predictions = analyzer.predict_next(&context);
        drop(analyzer);

        // Convert predictions to search results
        let results: Vec<UniversalSearchResult> = predictions
            .into_iter()
            .take(query.max_results)
            .map(|pred| UniversalSearchResult {
                memory: UniversalMemory {
                    id: pred.memory_id,
                    content: format!("Predicted memory {}", pred.memory_id),
                    embedding: None,
                    tags: vec!["predicted".to_string()],
                    metadata: HashMap::new(),
                    created_at: current_timestamp(),
                    last_accessed: current_timestamp(),
                    access_count: 0,
                },
                confidence: pred.confidence,
                path: Vec::new(),
                layer_origin: LayerId::Layer4,
                search_time_us: current_timestamp() - start_time,
            })
            .collect();

        // Update performance metrics
        {
            let mut metrics = self.performance_metrics.write();
            metrics.predictions_made += results.len() as u64;
            let search_time = current_timestamp() - start_time;
            metrics.average_prediction_time_us = 
                (metrics.average_prediction_time_us + search_time) / 2;
        }

        if results.is_empty() {
            Ok(RoutingDecision::SearchComplete { results })
        } else {
            Ok(RoutingDecision::FoundPartial {
                results,
                continue_search: false,
                suggested_layers: Vec::new(),
            })
        }
    }

    async fn get_performance(&self) -> LayerResult<LayerPerformance> {
        let metrics = self.performance_metrics.read();
        let analyzer = self.analyzer.lock().await;
        let stats = analyzer.get_statistics();
        drop(analyzer);

        let mut custom_metrics = HashMap::new();
        custom_metrics.insert("patterns_detected".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(metrics.patterns_detected)));
        custom_metrics.insert("accuracy_rate".to_string(), 
            serde_json::Value::Number(serde_json::Number::from_f64(metrics.accuracy_rate).unwrap()));
        custom_metrics.insert("total_patterns".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(stats.total_patterns)));

        Ok(LayerPerformance {
            layer_id: LayerId::Layer4,
            processing_time_us: metrics.average_prediction_time_us,
            memory_usage_bytes: stats.memory_usage_estimate as u64,
            operations_performed: metrics.predictions_made,
            cache_hit_rate: Some(metrics.cache_hit_rate),
            custom_metrics,
        })
    }

    async fn health_check(&self) -> LayerResult<LayerHealth> {
        let mut health = self.health_status.write();
        
        // Update diagnostics
        let analyzer = self.analyzer.lock().await;
        let stats = analyzer.get_statistics();
        drop(analyzer);
        
        health.diagnostics.insert("total_patterns".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(stats.total_patterns)));
        health.diagnostics.insert("active_matches".to_string(), 
            serde_json::Value::Number(serde_json::Number::from(stats.active_matches)));

        // Update resource usage
        let metrics = self.performance_metrics.read();
        health.resource_usage.memory_bytes = stats.memory_usage_estimate as u64;
        health.resource_usage.active_connections = metrics.active_sessions as u32;
        
        // Determine health status
        health.status = if stats.average_pattern_confidence > 0.3 {
            HealthStatus::Healthy
        } else if stats.total_patterns == 0 {
            HealthStatus::Starting
        } else {
            HealthStatus::Degraded
        };

        Ok(health.clone())
    }

    async fn start(&mut self, config: LayerConfig) -> LayerResult<()> {
        self.config = config;
        
        // Update health status
        {
            let mut health = self.health_status.write();
            health.status = HealthStatus::Healthy;
        }

        log::info!("Context Prediction Engine (Layer 4) started successfully");
        Ok(())
    }

    async fn shutdown(&mut self) -> LayerResult<()> {
        // Update health status
        {
            let mut health = self.health_status.write();
            health.status = HealthStatus::Stopping;
        }

        log::info!("Context Prediction Engine (Layer 4) shutting down");
        Ok(())
    }

    fn get_config(&self) -> &LayerConfig {
        &self.config
    }
}

#[async_trait]
impl ContextPredictionEngine for ContextPredictionLayer {
    async fn predict_next(&self, context: &ContextWindow) -> LayerResult<Vec<CorePredictionResult>> {
        let start_time = current_timestamp();
        
        // Check cache first
        let context_hash = Self::calculate_context_hash(context);
        let cache_key = format!("ctx_{}", context_hash);
        
        {
            let cache = self.prediction_cache.read();
            if let Some(cached) = cache.get(&cache_key) {
                let age = current_timestamp().saturating_sub(cached.timestamp);
                if age < cached.ttl_us {
                    // Update cache hit rate
                    let mut metrics = self.performance_metrics.write();
                    metrics.cache_hit_rate = (metrics.cache_hit_rate + 1.0) / 2.0;
                    
                    return Ok(cached.predictions.clone());
                }
            }
        }

        // Create prediction context
        let recent_sequence = if context.recent_accesses.is_empty() {
            None
        } else {
            Some(context.recent_accesses.iter().map(|a| a.memory_id).collect())
        };

        let pred_context = PredictionContext {
            recent_sequence,
            current_timestamp: current_timestamp(),
            user_context: context.user_context.get("user_id").map(|v| v.to_string()),
            session_id: context.user_context.get("session_id").map(|v| v.to_string()),
            max_predictions: 10,
        };

        // Get predictions from temporal analyzer
        let analyzer = self.analyzer.lock().await;
        let internal_predictions = analyzer.predict_next(&pred_context);
        drop(analyzer);

        // Convert to core types
        let core_predictions: Vec<CorePredictionResult> = internal_predictions
            .into_iter()
            .map(Self::convert_prediction_result)
            .collect();

        // Cache results
        {
            let mut cache = self.prediction_cache.write();
            cache.insert(cache_key, CachedPrediction {
                predictions: core_predictions.clone(),
                timestamp: current_timestamp(),
                context_hash,
                ttl_us: 30_000_000, // 30 seconds TTL
            });
        }

        // Update performance metrics
        {
            let mut metrics = self.performance_metrics.write();
            metrics.predictions_made += core_predictions.len() as u64;
            let prediction_time = current_timestamp() - start_time;
            metrics.average_prediction_time_us = 
                (metrics.average_prediction_time_us + prediction_time) / 2;
        }

        // Cleanup expired cache entries periodically
        if rand::random::<f64>() < 0.01 { // 1% chance
            self.cleanup_cache().await;
        }

        Ok(core_predictions)
    }

    async fn learn_pattern(&mut self, access_sequence: &[CoreMemoryAccess]) -> LayerResult<()> {
        if !self.learning_params.enable_online_learning {
            return Ok(());
        }

        // Convert access sequence to internal format
        for access in access_sequence {
            let internal_access = MemoryAccess {
                memory_id: access.memory_id,
                timestamp: access.timestamp,
                access_type: AccessType::Read, // Default access type
                user_context: access.context_metadata.get("user_context")
                    .and_then(|v| v.as_str().map(|s| s.to_string())),
                session_id: access.context_metadata.get("session_id")
                    .and_then(|v| v.as_str().map(|s| s.to_string())),
                confidence: 1.0,
            };

            let mut analyzer = self.analyzer.lock().await;
            analyzer.add_access(internal_access);
        }

        // Update performance metrics
        {
            let mut metrics = self.performance_metrics.write();
            let analyzer = self.analyzer.lock().await;
            let stats = analyzer.get_statistics();
            metrics.patterns_detected = stats.total_patterns as u64;
        }

        Ok(())
    }

    async fn get_context_state(&self) -> LayerResult<ContextState> {
        let analyzer = self.analyzer.lock().await;
        let stats = analyzer.get_statistics();
        let metrics = self.performance_metrics.read();

        Ok(ContextState {
            active_patterns: stats.total_patterns,
            context_window_size: stats.total_accesses,
            prediction_accuracy: metrics.accuracy_rate,
            learning_rate: self.learning_params.learning_rate,
        })
    }

    async fn update_context(&mut self, access: CoreMemoryAccess) -> LayerResult<()> {
        // Convert to internal format and add to analyzer
        let internal_access = MemoryAccess {
            memory_id: access.memory_id,
            timestamp: access.timestamp,
            access_type: AccessType::Read, // Default access type
            user_context: access.context_metadata.get("user_context")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            session_id: access.context_metadata.get("session_id")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            confidence: 1.0,
        };

        let mut analyzer = self.analyzer.lock().await;
        analyzer.add_access(internal_access);

        // Update context window
        let mut window = self.context_window.write();
        window.push_back(access);

        // Maintain window size
        while window.len() > 100 {
            window.pop_front();
        }

        Ok(())
    }
}

impl ContextPredictionLayer {
    /// Get performance metrics
    pub async fn get_performance(&self) -> LayerResult<ContextPredictionPerformance> {
        let metrics = self.performance_metrics.read();
        let analyzer = self.analyzer.lock().await;
        let stats = analyzer.get_statistics();

        Ok(ContextPredictionPerformance {
            predictions_generated: metrics.predictions_made,
            cache_hit_rate: metrics.cache_hit_rate as f32,
            accuracy_rate: metrics.accuracy_rate as f32,
            average_prediction_time_ms: metrics.average_prediction_time_us as f32 / 1000.0,
            patterns_detected: stats.total_patterns as u64,
            context_window_utilization: (stats.total_accesses as f32 / 1000.0).min(1.0),
        })
    }

    /// Clear temporal analysis state
    pub async fn clear_temporal_state(&mut self) -> LayerResult<()> {
        let mut analyzer = self.analyzer.lock().await;
        analyzer.clear_all_patterns();

        let mut cache = self.prediction_cache.write();
        cache.clear();

        let mut window = self.context_window.write();
        window.clear();

        Ok(())
    }

    /// Get current window size
    pub fn get_window_size(&self) -> usize {
        let window = self.context_window.read();
        window.len()
    }

    /// Perform health check
    pub async fn health_check_status(&self) -> LayerResult<bool> {
        // Check if analyzer is responsive
        if self.analyzer.try_lock().is_err() {
            return Err(LayerError::TimeoutExceeded { timeout_us: 0 });
        }

        // Check if cache is accessible
        if self.prediction_cache.try_read().is_err() {
            return Err(LayerError::TimeoutExceeded { timeout_us: 0 });
        }

        // Check window accessibility
        if self.context_window.try_read().is_err() {
            return Err(LayerError::TimeoutExceeded { timeout_us: 0 });
        }

        Ok(true)
    }
}

/// Performance metrics specific to context prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPredictionPerformance {
    pub predictions_generated: u64,
    pub cache_hit_rate: f32,
    pub accuracy_rate: f32,
    pub average_prediction_time_ms: f32,
    pub patterns_detected: u64,
    pub context_window_utilization: f32,
}