// Socket-based integration module for MFN layers
// Replaces broken FFI and HTTP implementations with Unix socket communication

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use anyhow::{Result, anyhow};
use tracing::{info, debug, warn, error};

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

use crate::{
    UniversalSearchQuery, UniversalSearchResult, SearchType,
    MfnQueryResult, LayerQueryResult, MemoryId,
    RoutingStrategy, PerformanceStats,
};

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
        stats.total_processing_time_ms += start_time.elapsed().as_millis() as f64;
        stats.average_response_time_ms = stats.total_processing_time_ms / stats.total_queries as f64;

        // Sort by confidence and merge
        all_results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        if all_results.len() > query.max_results {
            all_results.truncate(query.max_results);
        }

        result.merged_results = all_results;
        result.metadata.insert("total_time_ms".to_string(),
            start_time.elapsed().as_millis().to_string());

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
        let mut pool = self.connection_pool.lock().await;
        let socket_query = convert_to_socket_query(&query);

        // Create futures for all layer queries
        let mut futures = vec![];

        // Layer 1
        if let Ok(layer1) = pool.get_layer1().await {
            let q = socket_query.clone();
            futures.push(async move {
                layer1.query(&q).await.ok()
            });
        }

        // Layer 2
        if let Ok(layer2) = pool.get_layer2().await {
            let q = socket_query.clone();
            futures.push(async move {
                layer2.query(&q).await.ok()
            });
        }

        // Layer 3
        if let Ok(layer3) = pool.get_layer3().await {
            let q = socket_query.clone();
            futures.push(async move {
                layer3.query(&q).await.ok()
            });
        }

        // Layer 4
        if let Ok(layer4) = pool.get_layer4().await {
            let q = socket_query.clone();
            futures.push(async move {
                layer4.query(&q).await.ok()
            });
        }

        // Execute all queries in parallel
        let results = futures::future::join_all(futures).await;

        // Merge all results
        let mut all_results = Vec::new();
        for result_opt in results {
            if let Some(result) = result_opt {
                all_results.extend(convert_from_socket_results(result));
            }
        }

        Ok(all_results)
    }

    async fn query_adaptive(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        // Adaptive routing based on query type
        match query.search_type {
            SearchType::Exact => {
                // For exact queries, start with Layer 1
                let mut results = self.query_layer1_only(query.clone()).await?;
                if results.is_empty() {
                    // Fall back to other layers if no exact match
                    results = self.query_sequential(query).await?;
                }
                Ok(results)
            },
            SearchType::Similarity => {
                // For similarity, focus on Layer 2
                self.query_layer2_focused(query).await
            },
            SearchType::Associative => {
                // For associative, use Layers 2 and 3
                self.query_layers_2_and_3(query).await
            },
            SearchType::Contextual => {
                // For contextual, use all layers with emphasis on Layer 4
                self.query_all_layers_contextual(query).await
            },
        }
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
            if result.layer_source == 4 {
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
        query_id: query.query_id.clone(),
        content: query.query.clone(),
        search_type: match query.search_type {
            SearchType::Exact => SocketSearchType::Exact,
            SearchType::Similarity => SocketSearchType::Similarity,
            SearchType::Associative => SocketSearchType::Associative,
            SearchType::Contextual => SocketSearchType::Contextual,
        },
        max_results: query.max_results,
        min_confidence: query.min_confidence,
        timeout_ms: query.timeout_ms,
        metadata: query.metadata.clone(),
    }
}

fn convert_from_socket_results(result: SocketQueryResult) -> Vec<UniversalSearchResult> {
    result.results.into_iter().map(|r| UniversalSearchResult {
        memory_id: MemoryId(r.memory_id),
        content: r.content,
        confidence: r.confidence,
        layer_source: r.layer_source,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        associations: vec![],
        metadata: r.metadata,
    }).collect()
}

// Re-export for convenience
pub use SocketMfnIntegration as MfnIntegration;