// MFN Core - Parallel Orchestrator
// High-performance parallel orchestrator for 1000+ QPS throughput

use crate::layer_interface::*;
use crate::memory_types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{timeout, Duration, Instant};
use futures::future::join_all;
use rayon::prelude::*;

/// High-performance parallel orchestrator optimized for maximum throughput
pub struct ParallelMfnOrchestrator {
    layers: HashMap<LayerId, Arc<RwLock<Box<dyn MfnLayer>>>>,
    routing_config: ParallelRoutingConfig,
    performance_monitor: Arc<RwLock<PerformanceMonitor>>,
    query_cache: Arc<RwLock<QueryCache>>,
    layer_pool: Arc<LayerConnectionPool>,
}

#[derive(Debug, Clone)]
pub struct ParallelRoutingConfig {
    /// Enable parallel layer queries
    pub enable_parallel: bool,
    /// Maximum concurrent queries per layer
    pub max_concurrent_queries: usize,
    /// Layer timeout in microseconds
    pub layer_timeout_us: u64,
    /// Maximum layers to query in parallel
    pub max_parallel_layers: usize,
    /// Early termination confidence threshold
    pub confidence_threshold: f64,
    /// Enable result caching
    pub enable_caching: bool,
    /// Cache size limit
    pub cache_size: usize,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
}

impl Default for ParallelRoutingConfig {
    fn default() -> Self {
        Self {
            enable_parallel: true,
            max_concurrent_queries: 100,
            layer_timeout_us: 10_000, // 10ms
            max_parallel_layers: 4,
            confidence_threshold: 0.95,
            enable_caching: true,
            cache_size: 10000,
            cache_ttl_seconds: 300, // 5 minutes
        }
    }
}

/// Connection pool for managing layer connections
pub struct LayerConnectionPool {
    pools: HashMap<LayerId, tokio::sync::Semaphore>,
}

impl LayerConnectionPool {
    pub fn new(max_connections_per_layer: usize) -> Self {
        let mut pools = HashMap::new();
        
        for layer_id in [LayerId::Layer1, LayerId::Layer2, LayerId::Layer3, LayerId::Layer4] {
            pools.insert(layer_id, tokio::sync::Semaphore::new(max_connections_per_layer));
        }
        
        Self { pools }
    }
    
    pub async fn acquire(&self, layer_id: &LayerId) -> Option<tokio::sync::SemaphorePermit> {
        if let Some(semaphore) = self.pools.get(layer_id) {
            semaphore.acquire().await.ok()
        } else {
            None
        }
    }
}

/// Query result caching system
pub struct QueryCache {
    cache: HashMap<String, (Instant, UniversalSearchResults)>,
    max_size: usize,
    ttl: Duration,
}

impl QueryCache {
    pub fn new(max_size: usize, ttl_seconds: u64) -> Self {
        Self {
            cache: HashMap::new(),
            max_size,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }
    
    pub fn get(&mut self, query: &UniversalSearchQuery) -> Option<UniversalSearchResults> {
        let key = self.generate_cache_key(query);
        
        if let Some((timestamp, results)) = self.cache.get(&key) {
            if timestamp.elapsed() < self.ttl {
                return Some(results.clone());
            } else {
                self.cache.remove(&key);
            }
        }
        
        None
    }
    
    pub fn put(&mut self, query: &UniversalSearchQuery, results: UniversalSearchResults) {
        let key = self.generate_cache_key(query);
        
        // Evict old entries if cache is full
        if self.cache.len() >= self.max_size {
            self.evict_oldest_entries();
        }
        
        self.cache.insert(key, (Instant::now(), results));
    }
    
    fn generate_cache_key(&self, query: &UniversalSearchQuery) -> String {
        // Simple cache key generation - in production, use proper hashing
        format!("{}:{}:{}", 
                query.query_text, 
                query.max_results, 
                query.similarity_threshold)
    }
    
    fn evict_oldest_entries(&mut self) {
        // Remove oldest 10% of entries
        let evict_count = self.max_size / 10;
        let mut entries: Vec<_> = self.cache.iter().collect();
        entries.sort_by_key(|(_, (timestamp, _))| *timestamp);
        
        for (key, _) in entries.iter().take(evict_count) {
            self.cache.remove(*key);
        }
    }
}

/// Enhanced performance monitoring with thread-safe operations
#[derive(Debug)]
pub struct PerformanceMonitor {
    query_count: u64,
    successful_queries: u64,
    failed_queries: u64,
    total_query_time_us: u64,
    cache_hits: u64,
    cache_misses: u64,
    layer_performance: HashMap<LayerId, LayerPerformanceStats>,
    parallel_efficiency: f64,
    throughput_samples: Vec<ThroughputSample>,
}

#[derive(Debug, Clone)]
struct ThroughputSample {
    timestamp: Instant,
    queries_per_second: f64,
    average_latency_us: u64,
}

#[derive(Debug, Default, Clone)]
struct LayerPerformanceStats {
    queries: u64,
    total_time_us: u64,
    success_rate: f64,
    average_results: f64,
    parallel_utilization: f64,
}

impl ParallelMfnOrchestrator {
    pub fn new() -> Self {
        let config = ParallelRoutingConfig::default();
        
        Self {
            layers: HashMap::new(),
            routing_config: config.clone(),
            performance_monitor: Arc::new(RwLock::new(PerformanceMonitor {
                query_count: 0,
                successful_queries: 0,
                failed_queries: 0,
                total_query_time_us: 0,
                cache_hits: 0,
                cache_misses: 0,
                layer_performance: HashMap::new(),
                parallel_efficiency: 1.0,
                throughput_samples: Vec::new(),
            })),
            query_cache: Arc::new(RwLock::new(QueryCache::new(
                config.cache_size, 
                config.cache_ttl_seconds
            ))),
            layer_pool: Arc::new(LayerConnectionPool::new(config.max_concurrent_queries)),
        }
    }
    
    pub fn with_config(mut self, config: ParallelRoutingConfig) -> Self {
        self.routing_config = config;
        self
    }
    
    /// Register a layer with parallel processing support
    pub async fn register_layer(&mut self, layer: Box<dyn MfnLayer>) -> LayerResult<()> {
        let layer_id = layer.layer_id();
        let layer = Arc::new(RwLock::new(layer));
        self.layers.insert(layer_id, layer);
        
        let mut monitor = self.performance_monitor.write().await;
        monitor.layer_performance.insert(layer_id, LayerPerformanceStats::default());
        
        Ok(())
    }
    
    /// High-performance parallel search across all layers
    pub async fn search_parallel(&mut self, query: UniversalSearchQuery) -> LayerResult<UniversalSearchResults> {
        let query_start = Instant::now();
        
        // Check cache first
        if self.routing_config.enable_caching {
            let mut cache = self.query_cache.write().await;
            if let Some(cached_results) = cache.get(&query) {
                let mut monitor = self.performance_monitor.write().await;
                monitor.cache_hits += 1;
                monitor.query_count += 1;
                return Ok(cached_results);
            }
            monitor.cache_misses += 1;
        }
        
        // Execute parallel layer queries
        let results = if self.routing_config.enable_parallel {
            self.execute_parallel_queries(&query).await?
        } else {
            self.execute_sequential_queries(&query).await?
        };
        
        // Cache successful results
        if self.routing_config.enable_caching {
            let mut cache = self.query_cache.write().await;
            cache.put(&query, results.clone());
        }
        
        // Update performance metrics
        let query_time = query_start.elapsed().as_micros() as u64;
        self.update_performance_metrics(true, query_time).await;
        
        Ok(results)
    }
    
    /// Execute queries across layers in parallel with optimized coordination
    async fn execute_parallel_queries(&self, query: &UniversalSearchQuery) -> LayerResult<UniversalSearchResults> {
        let query_start = Instant::now();
        
        // Create tasks for all available layers
        let mut layer_tasks = Vec::new();
        
        for (&layer_id, layer_ref) in &self.layers {
            let layer_ref_clone = layer_ref.clone();
            let query_clone = query.clone();
            let pool_permit = self.layer_pool.acquire(&layer_id).await;
            let timeout_duration = Duration::from_micros(self.routing_config.layer_timeout_us);
            
            let task = tokio::spawn(async move {
                let _permit = pool_permit; // Hold connection permit
                let layer = layer_ref_clone.read().await;
                
                let layer_start = Instant::now();
                let result = timeout(timeout_duration, layer.search(&query_clone)).await;
                let layer_time = layer_start.elapsed().as_micros() as u64;
                
                match result {
                    Ok(Ok(routing_decision)) => Ok((layer_id, routing_decision, layer_time)),
                    Ok(Err(e)) => Err((layer_id, e, layer_time)),
                    Err(_) => Err((layer_id, LayerError::Timeout { timeout_us: timeout_duration.as_micros() as u64 }, layer_time)),
                }
            });
            
            layer_tasks.push(task);
        }
        
        // Wait for all layer tasks to complete
        let layer_results = join_all(layer_tasks).await;
        
        // Process results from all layers
        let mut all_results = Vec::new();
        let mut layers_consulted = Vec::new();
        let mut performance_stats = HashMap::new();
        let mut best_confidence = 0.0;
        
        for task_result in layer_results {
            match task_result {
                Ok(Ok((layer_id, routing_decision, layer_time))) => {
                    layers_consulted.push(layer_id);
                    performance_stats.insert(
                        format!("{}_time_us", layer_id.as_str()),
                        serde_json::Value::Number(serde_json::Number::from(layer_time))
                    );
                    
                    // Extract results based on routing decision
                    let layer_results = match routing_decision {
                        RoutingDecision::FoundExact { results } |
                        RoutingDecision::SearchComplete { results } => results,
                        RoutingDecision::FoundPartial { results, .. } => results,
                        RoutingDecision::RouteToLayers { .. } => Vec::new(),
                    };
                    
                    // Update best confidence
                    for result in &layer_results {
                        if result.confidence > best_confidence {
                            best_confidence = result.confidence;
                        }
                    }
                    
                    all_results.extend(layer_results);
                    
                    // Early termination if high confidence result found
                    if best_confidence >= self.routing_config.confidence_threshold {
                        break;
                    }
                }
                Ok(Err((layer_id, error, layer_time))) => {
                    // Log layer error but continue with other layers
                    log::warn!("Layer {} error: {} (time: {}us)", layer_id.as_str(), error, layer_time);
                    performance_stats.insert(
                        format!("{}_error", layer_id.as_str()),
                        serde_json::Value::String(error.to_string())
                    );
                }
                Err(join_error) => {
                    log::error!("Task join error: {}", join_error);
                }
            }
        }
        
        // Deduplicate and rank results using parallel processing
        all_results.par_sort_by(|a, b| {
            b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Remove duplicates based on memory ID
        all_results.dedup_by(|a, b| a.memory_id == b.memory_id);
        
        // Limit results
        if all_results.len() > query.max_results {
            all_results.truncate(query.max_results);
        }
        
        let search_time = query_start.elapsed().as_micros() as u64;
        
        Ok(UniversalSearchResults {
            results: all_results.clone(),
            query: query.clone(),
            total_found: all_results.len(),
            search_time_us: search_time,
            layers_consulted,
            performance_stats,
        })
    }
    
    /// Fallback sequential execution for comparison
    async fn execute_sequential_queries(&self, query: &UniversalSearchQuery) -> LayerResult<UniversalSearchResults> {
        // Implementation similar to original orchestrator but with optimizations
        // This is a fallback method - parallel execution is preferred
        
        let mut all_results = Vec::new();
        let mut layers_consulted = Vec::new();
        let mut performance_stats = HashMap::new();
        let query_start = Instant::now();
        
        // Execute layers sequentially with timeouts
        for layer_id in [LayerId::Layer1, LayerId::Layer2, LayerId::Layer3, LayerId::Layer4] {
            if let Some(layer_ref) = self.layers.get(&layer_id) {
                let _permit = self.layer_pool.acquire(&layer_id).await;
                let layer = layer_ref.read().await;
                
                let layer_start = Instant::now();
                let timeout_duration = Duration::from_micros(self.routing_config.layer_timeout_us);
                
                match timeout(timeout_duration, layer.search(query)).await {
                    Ok(Ok(routing_decision)) => {
                        let layer_time = layer_start.elapsed().as_micros() as u64;
                        layers_consulted.push(layer_id);
                        performance_stats.insert(
                            format!("{}_time_us", layer_id.as_str()),
                            serde_json::Value::Number(serde_json::Number::from(layer_time))
                        );
                        
                        match routing_decision {
                            RoutingDecision::FoundExact { results } => {
                                // Exact match found, return immediately
                                return Ok(UniversalSearchResults {
                                    results,
                                    query: query.clone(),
                                    total_found: results.len(),
                                    search_time_us: query_start.elapsed().as_micros() as u64,
                                    layers_consulted,
                                    performance_stats,
                                });
                            }
                            RoutingDecision::FoundPartial { results, continue_search, .. } => {
                                all_results.extend(results);
                                if !continue_search {
                                    break;
                                }
                            }
                            RoutingDecision::SearchComplete { results } => {
                                all_results.extend(results);
                            }
                            RoutingDecision::RouteToLayers { .. } => {
                                // Continue to next layer
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        log::warn!("Layer {} error: {}", layer_id.as_str(), e);
                    }
                    Err(_) => {
                        log::warn!("Layer {} timeout", layer_id.as_str());
                    }
                }
            }
        }
        
        // Sort and limit results
        all_results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        if all_results.len() > query.max_results {
            all_results.truncate(query.max_results);
        }
        
        Ok(UniversalSearchResults {
            results: all_results.clone(),
            query: query.clone(),
            total_found: all_results.len(),
            search_time_us: query_start.elapsed().as_micros() as u64,
            layers_consulted,
            performance_stats,
        })
    }
    
    /// Add memory to all layers with parallel processing
    pub async fn add_memory_parallel(&mut self, memory: UniversalMemory) -> LayerResult<()> {
        let tasks: Vec<_> = self.layers.iter().map(|(layer_id, layer_ref)| {
            let memory_clone = memory.clone();
            let layer_ref_clone = layer_ref.clone();
            let layer_id = *layer_id;
            
            tokio::spawn(async move {
                let mut layer = layer_ref_clone.write().await;
                let result = layer.add_memory(memory_clone.clone()).await;
                (layer_id, result)
            })
        }).collect();
        
        let results = join_all(tasks).await;
        
        for task_result in results {
            match task_result {
                Ok((layer_id, Ok(()))) => {
                    log::debug!("Added memory {} to {}", memory.id, layer_id.as_str());
                }
                Ok((layer_id, Err(e))) => {
                    log::warn!("Failed to add memory {} to {}: {}", memory.id, layer_id.as_str(), e);
                }
                Err(join_error) => {
                    log::error!("Task join error: {}", join_error);
                }
            }
        }
        
        Ok(())
    }
    
    /// Update performance metrics with thread safety
    async fn update_performance_metrics(&self, success: bool, query_time_us: u64) {
        let mut monitor = self.performance_monitor.write().await;
        
        monitor.query_count += 1;
        monitor.total_query_time_us += query_time_us;
        
        if success {
            monitor.successful_queries += 1;
        } else {
            monitor.failed_queries += 1;
        }
        
        // Update throughput sampling
        let current_time = Instant::now();
        let qps = if monitor.query_count > 0 && query_time_us > 0 {
            1_000_000.0 / (monitor.total_query_time_us as f64 / monitor.query_count as f64)
        } else {
            0.0
        };
        
        monitor.throughput_samples.push(ThroughputSample {
            timestamp: current_time,
            queries_per_second: qps,
            average_latency_us: query_time_us,
        });
        
        // Keep only recent samples (last 1000)
        if monitor.throughput_samples.len() > 1000 {
            monitor.throughput_samples.drain(..100);
        }
    }
    
    /// Get enhanced performance statistics
    pub async fn get_performance_stats(&self) -> PerformanceStats {
        let monitor = self.performance_monitor.read().await;
        
        let success_rate = if monitor.query_count > 0 {
            monitor.successful_queries as f64 / monitor.query_count as f64
        } else {
            0.0
        };
        
        let average_latency_us = if monitor.query_count > 0 {
            monitor.total_query_time_us / monitor.query_count
        } else {
            0
        };
        
        let current_qps = if !monitor.throughput_samples.is_empty() {
            let recent_samples = &monitor.throughput_samples[monitor.throughput_samples.len().saturating_sub(10)..];
            recent_samples.iter().map(|s| s.queries_per_second).sum::<f64>() / recent_samples.len() as f64
        } else {
            0.0
        };
        
        let cache_hit_rate = if monitor.cache_hits + monitor.cache_misses > 0 {
            monitor.cache_hits as f64 / (monitor.cache_hits + monitor.cache_misses) as f64
        } else {
            0.0
        };
        
        PerformanceStats {
            total_queries: monitor.query_count,
            successful_queries: monitor.successful_queries,
            failed_queries: monitor.failed_queries,
            success_rate,
            average_latency_us,
            current_qps,
            cache_hit_rate,
            parallel_efficiency: monitor.parallel_efficiency,
            layer_stats: monitor.layer_performance.clone(),
        }
    }
    
    /// Graceful shutdown with connection cleanup
    pub async fn shutdown(&mut self) -> LayerResult<()> {
        log::info!("Shutting down parallel orchestrator...");
        
        // Shutdown all layers in parallel
        let shutdown_tasks: Vec<_> = self.layers.iter().map(|(layer_id, layer_ref)| {
            let layer_ref_clone = layer_ref.clone();
            let layer_id = *layer_id;
            
            tokio::spawn(async move {
                let mut layer = layer_ref_clone.write().await;
                let result = layer.shutdown().await;
                (layer_id, result)
            })
        }).collect();
        
        let results = join_all(shutdown_tasks).await;
        
        for task_result in results {
            match task_result {
                Ok((layer_id, Ok(()))) => {
                    log::info!("Successfully shut down {}", layer_id.as_str());
                }
                Ok((layer_id, Err(e))) => {
                    log::error!("Error shutting down {}: {}", layer_id.as_str(), e);
                }
                Err(join_error) => {
                    log::error!("Shutdown task join error: {}", join_error);
                }
            }
        }
        
        self.layers.clear();
        log::info!("Parallel orchestrator shutdown complete");
        
        Ok(())
    }
}

/// Performance statistics structure
#[derive(Debug, Clone)]
pub struct PerformanceStats {
    pub total_queries: u64,
    pub successful_queries: u64,
    pub failed_queries: u64,
    pub success_rate: f64,
    pub average_latency_us: u64,
    pub current_qps: f64,
    pub cache_hit_rate: f64,
    pub parallel_efficiency: f64,
    pub layer_stats: HashMap<LayerId, LayerPerformanceStats>,
}

impl Default for ParallelMfnOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Mock high-performance layer for testing
    struct MockParallelLayer {
        layer_id: LayerId,
        memories: Arc<RwLock<HashMap<MemoryId, UniversalMemory>>>,
        query_count: Arc<AtomicU64>,
        config: LayerConfig,
    }

    impl MockParallelLayer {
        fn new(layer_id: LayerId) -> Self {
            Self {
                layer_id,
                memories: Arc::new(RwLock::new(HashMap::new())),
                query_count: Arc::new(AtomicU64::new(0)),
                config: LayerConfig {
                    layer_id,
                    ..Default::default()
                },
            }
        }
    }

    #[async_trait]
    impl MfnLayer for MockParallelLayer {
        fn layer_id(&self) -> LayerId { self.layer_id }
        fn layer_name(&self) -> &str { "MockParallelLayer" }
        fn version(&self) -> &str { "1.0.0" }

        async fn add_memory(&mut self, memory: UniversalMemory) -> LayerResult<()> {
            let mut memories = self.memories.write().await;
            memories.insert(memory.id, memory);
            Ok(())
        }

        async fn add_association(&mut self, _association: UniversalAssociation) -> LayerResult<()> {
            Ok(())
        }

        async fn get_memory(&self, id: MemoryId) -> LayerResult<UniversalMemory> {
            let memories = self.memories.read().await;
            memories.get(&id)
                .cloned()
                .ok_or(LayerError::MemoryNotFound { id })
        }

        async fn remove_memory(&mut self, id: MemoryId) -> LayerResult<()> {
            let mut memories = self.memories.write().await;
            memories.remove(&id);
            Ok(())
        }

        async fn search(&self, _query: &UniversalSearchQuery) -> LayerResult<RoutingDecision> {
            self.query_count.fetch_add(1, Ordering::Relaxed);
            
            // Simulate different response patterns based on layer
            match self.layer_id {
                LayerId::Layer1 => {
                    // Layer 1: Fast exact matches
                    tokio::time::sleep(Duration::from_micros(100)).await; // 0.1ms
                    Ok(RoutingDecision::RouteToLayers { next_layers: vec![LayerId::Layer2] })
                }
                LayerId::Layer2 => {
                    // Layer 2: Similarity search
                    tokio::time::sleep(Duration::from_millis(1)).await; // 1ms
                    Ok(RoutingDecision::FoundPartial { 
                        results: vec![], 
                        continue_search: true, 
                        confidence: 0.7 
                    })
                }
                LayerId::Layer3 => {
                    // Layer 3: Associative search
                    tokio::time::sleep(Duration::from_millis(2)).await; // 2ms
                    Ok(RoutingDecision::SearchComplete { results: vec![] })
                }
                LayerId::Layer4 => {
                    // Layer 4: Context prediction
                    tokio::time::sleep(Duration::from_millis(3)).await; // 3ms
                    Ok(RoutingDecision::SearchComplete { results: vec![] })
                }
            }
        }

        async fn get_performance(&self) -> LayerResult<LayerPerformance> {
            Ok(LayerPerformance {
                layer_id: self.layer_id,
                processing_time_us: 1000,
                memory_usage_bytes: 1024,
                operations_performed: self.query_count.load(Ordering::Relaxed),
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
    async fn test_parallel_orchestrator_throughput() {
        let mut orchestrator = ParallelMfnOrchestrator::new();
        
        // Register mock layers
        for layer_id in [LayerId::Layer1, LayerId::Layer2, LayerId::Layer3, LayerId::Layer4] {
            let layer = Box::new(MockParallelLayer::new(layer_id));
            orchestrator.register_layer(layer).await.unwrap();
        }
        
        // Test parallel query execution
        let query = UniversalSearchQuery {
            query_id: generate_query_id(),
            query_text: "test query".to_string(),
            max_results: 10,
            similarity_threshold: 0.7,
            ..Default::default()
        };
        
        let start_time = Instant::now();
        let result = orchestrator.search_parallel(query).await.unwrap();
        let elapsed_time = start_time.elapsed();
        
        // Parallel execution should be faster than sequential
        assert!(elapsed_time.as_millis() < 10); // Should complete in under 10ms
        assert!(result.layers_consulted.len() > 0);
        
        // Check performance stats
        let stats = orchestrator.get_performance_stats().await;
        assert_eq!(stats.total_queries, 1);
        assert_eq!(stats.successful_queries, 1);
        assert!(stats.success_rate > 0.9);
    }
    
    #[tokio::test]
    async fn test_concurrent_queries() {
        let mut orchestrator = Arc::new(RwLock::new(ParallelMfnOrchestrator::new()));
        
        // Register mock layers
        {
            let mut orch = orchestrator.write().await;
            for layer_id in [LayerId::Layer1, LayerId::Layer2, LayerId::Layer3, LayerId::Layer4] {
                let layer = Box::new(MockParallelLayer::new(layer_id));
                orch.register_layer(layer).await.unwrap();
            }
        }
        
        // Execute multiple concurrent queries
        let concurrent_queries = 50;
        let mut tasks = Vec::new();
        
        for i in 0..concurrent_queries {
            let orch_clone = orchestrator.clone();
            
            let task = tokio::spawn(async move {
                let query = UniversalSearchQuery {
                    query_id: generate_query_id(),
                    query_text: format!("test query {}", i),
                    max_results: 10,
                    similarity_threshold: 0.7,
                    ..Default::default()
                };
                
                let mut orch = orch_clone.write().await;
                orch.search_parallel(query).await
            });
            
            tasks.push(task);
        }
        
        // Wait for all queries to complete
        let start_time = Instant::now();
        let results = join_all(tasks).await;
        let total_time = start_time.elapsed();
        
        // Verify all queries succeeded
        let successful_queries = results.iter()
            .filter(|r| r.as_ref().unwrap().is_ok())
            .count();
        
        assert_eq!(successful_queries, concurrent_queries);
        
        // Calculate throughput
        let qps = concurrent_queries as f64 / total_time.as_secs_f64();
        println!("Achieved QPS: {:.2}", qps);
        
        // Should achieve high throughput with parallel processing
        assert!(qps > 100.0); // Should handle > 100 QPS easily
        
        // Check final performance stats
        let orch = orchestrator.read().await;
        let stats = orch.get_performance_stats().await;
        assert_eq!(stats.total_queries as usize, concurrent_queries);
        assert!(stats.success_rate > 0.95);
    }
}