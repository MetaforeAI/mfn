//! MFN Integration Library
//! 
//! Provides unified access to all Memory Flow Network layers through a single interface.
//! Handles orchestration, communication, and data flow between heterogeneous layer implementations.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use anyhow::Result;

pub use mfn_core::*;

/// Unified MFN system that orchestrates all layers
pub struct MfnSystem {
    /// Layer 1: Zig IFR (via FFI)
    pub layer1: Option<Layer1Client>,
    
    /// Layer 2: Rust DSR (direct)  
    pub layer2: Option<Layer2Client>,
    
    /// Layer 3: Go ALM (via HTTP)
    pub layer3: Option<Layer3Client>,
    
    /// Layer 4: Rust CPE (direct)
    pub layer4: Option<Arc<RwLock<layer4_context_engine::ContextPredictionLayer>>>,
    
    /// System configuration
    config: MfnSystemConfig,
    
    /// Performance metrics
    metrics: Arc<RwLock<SystemMetrics>>,
}

/// Configuration for the complete MFN system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfnSystemConfig {
    pub layer1_enabled: bool,
    pub layer1_library_path: String,
    
    pub layer2_enabled: bool,
    
    pub layer3_enabled: bool,
    pub layer3_endpoint: String,
    
    pub layer4_enabled: bool,
    
    pub routing_strategy: RoutingStrategy,
    pub performance_monitoring: bool,
    pub cache_enabled: bool,
}

impl Default for MfnSystemConfig {
    fn default() -> Self {
        Self {
            layer1_enabled: true,
            layer1_library_path: "../layer1-zig-ifr/zig-out/lib/libifr.so".to_string(),
            
            layer2_enabled: true,
            
            layer3_enabled: true,
            layer3_endpoint: "http://localhost:8080".to_string(),
            
            layer4_enabled: true,
            
            routing_strategy: RoutingStrategy::Sequential,
            performance_monitoring: true,
            cache_enabled: true,
        }
    }
}

/// Memory flow routing strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingStrategy {
    /// Process layers sequentially: 1 → 2 → 3 → 4
    Sequential,
    /// Process layers in parallel and merge results
    Parallel,
    /// Adaptive routing based on query type and performance
    Adaptive,
    /// Custom routing logic
    Custom(String),
}

/// System-wide performance metrics
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub total_queries: u64,
    pub successful_queries: u64,
    pub average_response_time_ms: f64,
    pub layer_performance: HashMap<String, LayerMetrics>,
    pub routing_efficiency: f64,
    pub cache_hit_rate: f64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LayerMetrics {
    pub queries: u64,
    pub average_time_ms: f64,
    pub success_rate: f64,
    pub errors: u64,
}

/// Client for Layer 1 (Zig IFR) via FFI
pub struct Layer1Client {
    handle: *mut std::ffi::c_void,
}

unsafe impl Send for Layer1Client {}
unsafe impl Sync for Layer1Client {}

/// Client for Layer 2 (Rust DSR) - direct integration
pub struct Layer2Client {
    // Will contain direct Rust DSR integration
    phantom: std::marker::PhantomData<()>,
}

/// Client for Layer 3 (Go ALM) via HTTP API
pub struct Layer3Client {
    client: reqwest::Client,
    base_url: String,
}

impl MfnSystem {
    /// Create a new MFN system with default configuration
    pub async fn new() -> Result<Self> {
        Self::with_config(MfnSystemConfig::default()).await
    }
    
    /// Create a new MFN system with custom configuration
    pub async fn with_config(config: MfnSystemConfig) -> Result<Self> {
        log::info!("Initializing MFN System with config: {:?}", config);
        
        let mut system = Self {
            layer1: None,
            layer2: None,
            layer3: None,
            layer4: None,
            config,
            metrics: Arc::new(RwLock::new(SystemMetrics::default())),
        };
        
        system.initialize_layers().await?;
        
        Ok(system)
    }
    
    /// Initialize all enabled layers
    async fn initialize_layers(&mut self) -> Result<()> {
        if self.config.layer1_enabled {
            log::info!("Initializing Layer 1 (Zig IFR)...");
            self.layer1 = Some(Layer1Client::new(&self.config.layer1_library_path)?);
        }
        
        if self.config.layer2_enabled {
            log::info!("Initializing Layer 2 (Rust DSR)...");
            self.layer2 = Some(Layer2Client::new().await?);
        }
        
        if self.config.layer3_enabled {
            log::info!("Initializing Layer 3 (Go ALM)...");
            self.layer3 = Some(Layer3Client::new(&self.config.layer3_endpoint).await?);
        }
        
        if self.config.layer4_enabled {
            log::info!("Initializing Layer 4 (Context Engine)...");
            let layer4 = layer4_context_engine::ContextPredictionLayer::new();
            self.layer4 = Some(Arc::new(RwLock::new(layer4)));
        }
        
        log::info!("All layers initialized successfully");
        Ok(())
    }
    
    /// Process a memory query through the complete MFN pipeline
    pub async fn query(&self, query: UniversalSearchQuery) -> Result<MfnQueryResult> {
        let start_time = std::time::Instant::now();
        
        log::debug!("Processing query: {:?}", query.content);
        
        let result = match self.config.routing_strategy {
            RoutingStrategy::Sequential => self.query_sequential(query).await,
            RoutingStrategy::Parallel => self.query_parallel(query).await,
            RoutingStrategy::Adaptive => self.query_adaptive(query).await,
            RoutingStrategy::Custom(_) => self.query_custom(query).await,
        }?;
        
        // Update metrics
        let elapsed = start_time.elapsed();
        self.update_metrics(elapsed, &result).await;
        
        log::debug!("Query completed in {:?}", elapsed);
        Ok(result)
    }
    
    /// Sequential routing: Layer 1 → 2 → 3 → 4
    async fn query_sequential(&self, query: UniversalSearchQuery) -> Result<MfnQueryResult> {
        let mut result = MfnQueryResult::new();
        
        // Layer 1: Exact matching
        if let Some(layer1) = &self.layer1 {
            let layer1_result = layer1.query(&query).await?;
            result.layer_results.insert("layer1".to_string(), layer1_result.clone());
            
            if !layer1_result.results.is_empty() {
                // Found exact matches, can shortcut or continue for associations
                log::debug!("Layer 1 found {} exact matches", layer1_result.results.len());
            }
        }
        
        // Layer 2: Neural similarity
        if let Some(layer2) = &self.layer2 {
            let layer2_result = layer2.query(&query).await?;
            result.layer_results.insert("layer2".to_string(), layer2_result);
        }
        
        // Layer 3: Graph associations
        if let Some(layer3) = &self.layer3 {
            let layer3_result = layer3.query(&query).await?;
            result.layer_results.insert("layer3".to_string(), layer3_result);
        }
        
        // Layer 4: Context predictions
        if let Some(layer4) = &self.layer4 {
            let layer4_guard = layer4.read().await;
            let layer4_result = self.query_layer4(&*layer4_guard, &query).await?;
            result.layer_results.insert("layer4".to_string(), layer4_result);
        }
        
        // Merge and rank all results
        result.merged_results = self.merge_layer_results(&result.layer_results);
        
        Ok(result)
    }
    
    /// Parallel routing: All layers process simultaneously
    async fn query_parallel(&self, query: UniversalSearchQuery) -> Result<MfnQueryResult> {
        let mut handles = Vec::new();
        let mut result = MfnQueryResult::new();
        
        // Launch all layers concurrently
        if let Some(layer1) = &self.layer1 {
            let layer1 = layer1.clone();
            let query = query.clone();
            handles.push(tokio::spawn(async move {
                ("layer1", layer1.query(&query).await)
            }));
        }
        
        // Similar for other layers...
        // For brevity, showing the pattern
        
        // Wait for all results
        for handle in handles {
            match handle.await? {
                (layer_name, Ok(layer_result)) => {
                    result.layer_results.insert(layer_name.to_string(), layer_result);
                }
                (layer_name, Err(e)) => {
                    log::error!("Layer {} failed: {}", layer_name, e);
                }
            }
        }
        
        result.merged_results = self.merge_layer_results(&result.layer_results);
        Ok(result)
    }
    
    /// Adaptive routing: Choose strategy based on query characteristics
    async fn query_adaptive(&self, query: UniversalSearchQuery) -> Result<MfnQueryResult> {
        // Analyze query to determine optimal routing
        if query.content.as_ref().map(|c| c.len()).unwrap_or(0) < 50 {
            // Short queries: try exact matching first
            self.query_sequential(query).await
        } else if query.embedding.is_some() {
            // Embedding provided: parallel processing
            self.query_parallel(query).await
        } else {
            // Default to sequential
            self.query_sequential(query).await
        }
    }
    
    /// Custom routing logic
    async fn query_custom(&self, query: UniversalSearchQuery) -> Result<MfnQueryResult> {
        // Placeholder for custom routing implementation
        self.query_sequential(query).await
    }
    
    /// Query Layer 4 specifically
    async fn query_layer4(&self, layer4: &layer4_context_engine::ContextPredictionLayer, query: &UniversalSearchQuery) -> Result<LayerQueryResult> {
        use mfn_core::layer_interface::MfnLayer;
        
        let start_time = std::time::Instant::now();
        
        // Use the MfnLayer search interface
        let routing_decision = layer4.search(query).await
            .map_err(|e| anyhow::anyhow!("Layer 4 search failed: {}", e))?;
        
        let results = match routing_decision {
            RoutingDecision::FoundExact { results } => results,
            RoutingDecision::FoundPartial { results, .. } => results,
            RoutingDecision::SearchComplete { results } => results,
            _ => vec![],
        };
        
        let processing_time = start_time.elapsed();
        
        let confidence = if results.is_empty() { 0.0 } else { 0.8 };
        Ok(LayerQueryResult {
            results,
            processing_time_ms: processing_time.as_secs_f64() * 1000.0,
            confidence,
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("layer".to_string(), "layer4".to_string());
                meta.insert("type".to_string(), "context_prediction".to_string());
                meta
            },
        })
    }
    
    /// Merge results from all layers into a unified ranking
    fn merge_layer_results(&self, layer_results: &HashMap<String, LayerQueryResult>) -> Vec<UniversalSearchResult> {
        let mut all_results = Vec::new();
        
        for (layer_name, result) in layer_results {
            for mut search_result in result.results.clone() {
                // Add layer information to metadata
                search_result.memory.metadata.insert("source_layer".to_string(), layer_name.clone());
                all_results.push(search_result);
            }
        }
        
        // Sort by confidence score
        all_results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        
        all_results
    }
    
    /// Update system performance metrics
    async fn update_metrics(&self, elapsed: std::time::Duration, result: &MfnQueryResult) {
        let mut metrics = self.metrics.write().await;
        
        metrics.total_queries += 1;
        if !result.merged_results.is_empty() {
            metrics.successful_queries += 1;
        }
        
        let elapsed_ms = elapsed.as_millis() as f64;
        metrics.average_response_time_ms = 
            (metrics.average_response_time_ms * (metrics.total_queries - 1) as f64 + elapsed_ms) / 
            metrics.total_queries as f64;
    }
    
    /// Get current system metrics
    pub async fn get_metrics(&self) -> SystemMetrics {
        {
            let metrics = self.metrics.read().await;
            SystemMetrics {
                total_queries: metrics.total_queries,
                successful_queries: metrics.successful_queries,
                average_response_time_ms: metrics.average_response_time_ms,
                layer_performance: metrics.layer_performance.clone(),
                routing_efficiency: metrics.routing_efficiency,
                cache_hit_rate: metrics.cache_hit_rate,
            }
        }
    }
    
    /// Graceful shutdown of all layers
    pub async fn shutdown(&mut self) -> Result<()> {
        log::info!("Shutting down MFN System...");
        
        if let Some(layer1) = self.layer1.take() {
            layer1.shutdown()?;
        }
        
        if let Some(layer2) = self.layer2.take() {
            layer2.shutdown().await?;
        }
        
        if let Some(layer3) = self.layer3.take() {
            layer3.shutdown().await?;
        }
        
        // Layer 4 will be dropped automatically
        
        log::info!("MFN System shutdown complete");
        Ok(())
    }
}

/// Result from querying the complete MFN system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfnQueryResult {
    /// Results from each individual layer
    pub layer_results: HashMap<String, LayerQueryResult>,
    
    /// Merged and ranked results from all layers
    pub merged_results: Vec<UniversalSearchResult>,
    
    /// Overall processing metadata
    pub metadata: HashMap<String, String>,
}

impl MfnQueryResult {
    fn new() -> Self {
        Self {
            layer_results: HashMap::new(),
            merged_results: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Result from a single layer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerQueryResult {
    pub results: Vec<UniversalSearchResult>,
    pub processing_time_ms: f64,
    pub confidence: f64,
    pub metadata: HashMap<String, String>,
}

// Layer client implementations
impl Layer1Client {
    fn new(_library_path: &str) -> Result<Self> {
        // FFI initialization would go here
        Ok(Self {
            handle: std::ptr::null_mut(),
        })
    }
    
    async fn query(&self, _query: &UniversalSearchQuery) -> Result<LayerQueryResult> {
        // FFI call to Zig IFR would go here
        Ok(LayerQueryResult {
            results: vec![],
            processing_time_ms: 0.5,
            confidence: 1.0,
            metadata: HashMap::new(),
        })
    }
    
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
        }
    }
    
    fn shutdown(self) -> Result<()> {
        Ok(())
    }
}

impl Layer2Client {
    async fn new() -> Result<Self> {
        Ok(Self {
            phantom: std::marker::PhantomData,
        })
    }
    
    async fn query(&self, _query: &UniversalSearchQuery) -> Result<LayerQueryResult> {
        Ok(LayerQueryResult {
            results: vec![],
            processing_time_ms: 1.2,
            confidence: 0.8,
            metadata: HashMap::new(),
        })
    }
    
    async fn shutdown(self) -> Result<()> {
        Ok(())
    }
}

impl Layer3Client {
    async fn new(base_url: &str) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        })
    }
    
    async fn query(&self, query: &UniversalSearchQuery) -> Result<LayerQueryResult> {
        let response = self.client
            .post(&format!("{}/search", self.base_url))
            .json(query)
            .send()
            .await?;
            
        if response.status().is_success() {
            let result: LayerQueryResult = response.json().await?;
            Ok(result)
        } else {
            Err(anyhow::anyhow!("Layer 3 request failed: {}", response.status()))
        }
    }
    
    async fn shutdown(self) -> Result<()> {
        Ok(())
    }
}