// Socket-based integration module for MFN layers
// Replaces broken FFI and HTTP implementations with Unix socket communication

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use anyhow::{Result, anyhow};
use tracing::{info, debug, warn, error};
use serde_json;
use uuid;

use crate::socket_clients::{
    Layer1Client as SocketLayer1,
    Layer2Client as SocketLayer2,
    Layer3Client as SocketLayer3,
    Layer4Client as SocketLayer4,
    LayerConnectionPool,
    UniversalSearchQuery as SocketQuery,
    SearchType as SocketSearchType,
    LayerQueryResult as SocketQueryResult,
};

use mfn_core::{
    UniversalSearchQuery, UniversalSearchResult, UniversalMemory, MemoryId,
};

// Define missing types locally
#[derive(Debug, Clone)]
pub enum RoutingStrategy {
    Sequential,
    Parallel,
    Adaptive,
}

#[derive(Debug, Default)]
pub struct PerformanceStats {
    pub total_queries: u64,
    pub total_time_ms: f64,
    pub success_rate: f64,
}

#[derive(Debug)]
pub struct MfnQueryResult {
    pub results: Vec<UniversalSearchResult>,
    pub total_time_ms: f64,
    pub layer_times: Vec<(String, f64)>,
}

impl MfnQueryResult {
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            total_time_ms: 0.0,
            layer_times: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct LayerQueryResult {
    pub results: Vec<UniversalSearchResult>,
    pub processing_time_ms: f64,
    pub confidence: f64,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Default for LayerQueryResult {
    fn default() -> Self {
        Self {
            results: Vec::new(),
            processing_time_ms: 0.0,
            confidence: 0.0,
            metadata: HashMap::new(),
        }
    }
}

/// Socket-based MFN system integration
pub struct SocketMfnIntegration {
    connection_pool: Arc<Mutex<LayerConnectionPool>>,
    routing_strategy: RoutingStrategy,
    performance_stats: Arc<Mutex<PerformanceStats>>,
}

impl SocketMfnIntegration {
    pub async fn new() -> Result<Self> {
        let pool = LayerConnectionPool::new().await?;
        Ok(Self {
            connection_pool: Arc::new(Mutex::new(pool)),
            routing_strategy: RoutingStrategy::Sequential,
            performance_stats: Arc::new(Mutex::new(PerformanceStats::default())),
        })
    }

    /// Initialize and verify all layer connections
    pub async fn initialize_all_layers(&self) -> Result<()> {
        info!("Initializing all MFN layers via Unix sockets...");

        let mut pool = self.connection_pool.lock().await;

        // Try to connect to each layer
        let mut connected_layers = vec![];

        // Layer 1 (Zig IFR)
        match pool.get_layer1().await {
            Ok(_) => {
                info!("✅ Layer 1 (IFR) connected");
                connected_layers.push(1);
            },
            Err(e) => warn!("⚠️ Layer 1 (IFR) not available: {}", e),
        }

        // Layer 2 (Rust DSR)
        match pool.get_layer2().await {
            Ok(_) => {
                info!("✅ Layer 2 (DSR) connected");
                connected_layers.push(2);
            },
            Err(e) => warn!("⚠️ Layer 2 (DSR) not available: {}", e),
        }

        // Layer 3 (Go ALM)
        match pool.get_layer3().await {
            Ok(_) => {
                info!("✅ Layer 3 (ALM) connected");
                connected_layers.push(3);
            },
            Err(e) => warn!("⚠️ Layer 3 (ALM) not available: {}", e),
        }

        // Layer 4 (Rust CPE)
        match pool.get_layer4().await {
            Ok(_) => {
                info!("✅ Layer 4 (CPE) connected");
                connected_layers.push(4);
            },
            Err(e) => warn!("⚠️ Layer 4 (CPE) not available: {}", e),
        }

        if connected_layers.is_empty() {
            return Err(anyhow!("No layers available - please start layer socket servers"));
        }

        info!("Connected to {} layers: {:?}", connected_layers.len(), connected_layers);
        Ok(())
    }

    /// Add a memory to the MFN system
    pub async fn add_memory(&self, memory: UniversalMemory) -> Result<()> {
        let mut pool = self.connection_pool.lock().await;

        // Try to add to Layer 1 (primary storage)
        match pool.get_layer1().await {
            Ok(layer1) => {
                layer1.add_memory(&memory.content, memory.content.as_bytes()).await?;
                debug!("Memory {} added to Layer 1", memory.id);
            }
            Err(e) => {
                warn!("Failed to add memory to Layer 1: {}", e);
                return Err(anyhow!("Layer 1 not available for memory storage"));
            }
        }

        Ok(())
    }

    /// Search the MFN system (wrapper around query for compatibility)
    pub async fn search(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let result = self.query(query).await?;
        Ok(result.results)
    }

    /// Query the MFN system
    pub async fn query(&self, query: UniversalSearchQuery) -> Result<MfnQueryResult> {
        let start_time = Instant::now();

        let mut result = MfnQueryResult::new();
        let mut all_results = Vec::new();

        match self.routing_strategy {
            RoutingStrategy::Sequential => {
                all_results = self.query_sequential(query.clone()).await?;
            },
            RoutingStrategy::Parallel => {
                all_results = self.query_parallel(query.clone()).await?;
            },
            RoutingStrategy::Adaptive => {
                all_results = self.query_adaptive(query.clone()).await?;
            },
        }

        // Update statistics
        let mut stats = self.performance_stats.lock().await;
        stats.total_queries += 1;
        stats.total_time_ms += start_time.elapsed().as_millis() as f64;
        stats.success_rate = 1.0; // Update based on actual success/failure tracking

        // Sort by confidence and merge
        all_results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        if all_results.len() > query.max_results {
            all_results.truncate(query.max_results);
        }

        result.results = all_results;
        result.total_time_ms = start_time.elapsed().as_millis() as f64;

        Ok(result)
    }

    async fn query_sequential(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let mut all_results = Vec::new();
        let mut pool = self.connection_pool.lock().await;

        // Convert to socket query format
        let socket_query = convert_to_socket_query(&query);

        // Layer 1: Exact match
        if let Ok(layer1) = pool.get_layer1().await {
            match layer1.query(&socket_query).await {
                Ok(result) => {
                    debug!("Layer 1 returned {} results", result.results.len());
                    all_results.extend(convert_from_socket_results(result));

                    // If we found exact matches with high confidence, we can stop
                    if !all_results.is_empty() && all_results[0].confidence > 0.95 {
                        return Ok(all_results);
                    }
                },
                Err(e) => warn!("Layer 1 query failed: {}", e),
            }
        }

        // Layer 2: Similarity search
        if let Ok(layer2) = pool.get_layer2().await {
            match layer2.query(&socket_query).await {
                Ok(result) => {
                    debug!("Layer 2 returned {} results", result.results.len());
                    all_results.extend(convert_from_socket_results(result));
                },
                Err(e) => warn!("Layer 2 query failed: {}", e),
            }
        }

        // Layer 3: Associative search
        if let Ok(layer3) = pool.get_layer3().await {
            match layer3.query(&socket_query).await {
                Ok(result) => {
                    debug!("Layer 3 returned {} results", result.results.len());
                    all_results.extend(convert_from_socket_results(result));
                },
                Err(e) => warn!("Layer 3 query failed: {}", e),
            }
        }

        // Layer 4: Context prediction
        if let Ok(layer4) = pool.get_layer4().await {
            match layer4.query(&socket_query).await {
                Ok(result) => {
                    debug!("Layer 4 returned {} results", result.results.len());
                    all_results.extend(convert_from_socket_results(result));
                },
                Err(e) => warn!("Layer 4 query failed: {}", e),
            }
        }

        Ok(all_results)
    }

    async fn query_parallel(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        // For now, just use sequential query
        // TODO: Implement proper parallel execution with futures
        self.query_sequential(query).await
    }

    async fn query_adaptive(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        // Simple adaptive routing - use sequential for now
        // In future, could analyze query content to determine best routing
        self.query_sequential(query).await
    }

    async fn query_layer1_only(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let mut pool = self.connection_pool.lock().await;
        let socket_query = convert_to_socket_query(&query);

        if let Ok(layer1) = pool.get_layer1().await {
            match layer1.query(&socket_query).await {
                Ok(result) => Ok(convert_from_socket_results(result)),
                Err(e) => {
                    warn!("Layer 1 query failed: {}", e);
                    Ok(vec![])
                }
            }
        } else {
            Ok(vec![])
        }
    }

    async fn query_layer2_focused(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let mut pool = self.connection_pool.lock().await;
        let socket_query = convert_to_socket_query(&query);
        let mut all_results = Vec::new();

        // Primary: Layer 2
        if let Ok(layer2) = pool.get_layer2().await {
            match layer2.query(&socket_query).await {
                Ok(result) => all_results.extend(convert_from_socket_results(result)),
                Err(e) => warn!("Layer 2 query failed: {}", e),
            }
        }

        // Secondary: Layer 3 for associations
        if let Ok(layer3) = pool.get_layer3().await {
            match layer3.query(&socket_query).await {
                Ok(result) => all_results.extend(convert_from_socket_results(result)),
                Err(e) => warn!("Layer 3 query failed: {}", e),
            }
        }

        Ok(all_results)
    }

    async fn query_layers_2_and_3(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let mut pool = self.connection_pool.lock().await;
        let socket_query = convert_to_socket_query(&query);
        let mut all_results = Vec::new();

        // Query both Layer 2 and Layer 3
        if let Ok(layer2) = pool.get_layer2().await {
            match layer2.query(&socket_query).await {
                Ok(result) => all_results.extend(convert_from_socket_results(result)),
                Err(e) => warn!("Layer 2 query failed: {}", e),
            }
        }

        if let Ok(layer3) = pool.get_layer3().await {
            match layer3.query(&socket_query).await {
                Ok(result) => all_results.extend(convert_from_socket_results(result)),
                Err(e) => warn!("Layer 3 query failed: {}", e),
            }
        }

        Ok(all_results)
    }

    async fn query_all_layers_contextual(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        // Use parallel query but weight Layer 4 results higher
        let mut results = self.query_parallel(query).await?;

        // Boost confidence for Layer 4 results
        for result in &mut results {
            if result.layer_origin == mfn_core::LayerId::Layer4 {
                result.confidence *= 1.2;
                if result.confidence > 1.0 {
                    result.confidence = 1.0;
                }
            }
        }

        Ok(results)
    }

    pub async fn shutdown(self) -> Result<()> {
        let pool = Arc::try_unwrap(self.connection_pool)
            .map_err(|_| anyhow!("Failed to unwrap connection pool"))?
            .into_inner();

        pool.shutdown()?;
        info!("MFN socket integration shut down successfully");
        Ok(())
    }
}

// Helper functions for conversion
fn convert_to_socket_query(query: &UniversalSearchQuery) -> SocketQuery {
    SocketQuery {
        query_id: uuid::Uuid::new_v4().to_string(),
        content: query.content.clone().unwrap_or_default(),
        search_type: SocketSearchType::Similarity, // Default to similarity search
        max_results: query.max_results,
        min_confidence: query.min_weight as f32,
        timeout_ms: query.timeout_us / 1000,
        metadata: query.layer_params.iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect(),
    }
}

fn convert_from_socket_results(result: SocketQueryResult) -> Vec<UniversalSearchResult> {
    use mfn_core::LayerId;

    result.results.into_iter().map(|r| UniversalSearchResult {
        memory: UniversalMemory::new(
            r.memory_id,
            r.content,
        ),
        confidence: r.confidence as f64,
        path: vec![],  // No path information from socket results
        layer_origin: match r.layer_source {
            1 => LayerId::Layer1,
            2 => LayerId::Layer2,
            3 => LayerId::Layer3,
            4 => LayerId::Layer4,
            _ => LayerId::Layer1,
        },
        search_time_us: 1000, // Convert from ms to us if needed
    }).collect()
}

// Re-export for convenience
pub use SocketMfnIntegration as MfnIntegration;