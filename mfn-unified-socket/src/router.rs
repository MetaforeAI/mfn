use crate::{MFNConfig, UnifiedRequest, UnifiedResponse};
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

/// Routes requests to appropriate layer implementations
pub struct LayerRouter {
    config: Arc<MFNConfig>,
    layer2: Option<Arc<mfn_layer2_dsr::PoolManager>>,
    layer4: Option<()>,  // TODO: Implement Layer 4 integration
    layer5: Option<Arc<layer5_psr::PatternRegistry>>,
}

impl LayerRouter {
    pub fn new(config: Arc<MFNConfig>) -> Result<Self> {
        Ok(Self {
            config,
            layer2: None,
            layer4: None,
            layer5: None,
        })
    }

    /// Initialize Layer 2 (DSR)
    pub async fn init_layer2(&mut self) -> Result<()> {
        use std::path::PathBuf;

        let config = mfn_layer2_dsr::DSRConfig::default();
        let data_dir = PathBuf::from(&self.config.persistence.layer2_dir);
        std::fs::create_dir_all(&data_dir)?;

        let pool_manager = Arc::new(mfn_layer2_dsr::PoolManager::new(data_dir, config));
        self.layer2 = Some(pool_manager);

        tracing::info!("✅ Layer 2 (DSR) initialized");
        Ok(())
    }

    /// Initialize Layer 4 (CPE)
    pub async fn init_layer4(&mut self) -> Result<()> {
        // TODO: Initialize Layer 4
        tracing::warn!("⚠️  Layer 4 (CPE) initialization pending");
        Ok(())
    }

    /// Initialize Layer 5 (PSR)
    pub async fn init_layer5(&mut self) -> Result<()> {
        use std::path::PathBuf;

        let persistence_config = layer5_psr::PersistenceConfig {
            data_dir: PathBuf::from(&self.config.persistence.layer5_dir),
            pool_id: "discord_unified".to_string(),
            fsync_interval_ms: 1000,
            snapshot_interval_secs: 300,
            aof_buffer_size: 64 * 1024,
        };

        std::fs::create_dir_all(&persistence_config.data_dir)?;

        let psr = if persistence_config.aof_path().exists() || persistence_config.snapshot_path().exists() {
            Arc::new(layer5_psr::PatternRegistry::recover_from_persistence(persistence_config)?)
        } else {
            Arc::new(layer5_psr::PatternRegistry::new_with_persistence(Some(persistence_config))?)
        };

        self.layer5 = Some(psr);
        tracing::info!("✅ Layer 5 (PSR) initialized");
        Ok(())
    }

    /// Route request to appropriate layer
    pub async fn route_request(&self, req: UnifiedRequest) -> Result<UnifiedResponse> {
        let start = Instant::now();
        let target_layer = req.target_layer.clone();
        let request_id = req.request_id.clone();

        let result = match target_layer.as_str() {
            "layer1" => self.handle_layer1(req).await,
            "layer2" => self.handle_layer2(req).await,
            "layer3" => self.handle_layer3(req).await,
            "layer4" => self.handle_layer4(req).await,
            "layer5" => self.handle_layer5(req).await,
            _ => Err(anyhow!("Unknown target layer: {}", target_layer)),
        };

        let processing_time = start.elapsed().as_secs_f64() * 1000.0;

        match result {
            Ok(mut resp) => {
                resp.processing_time_ms = processing_time;
                Ok(resp)
            }
            Err(e) => Ok(UnifiedResponse {
                response_type: "error".to_string(),
                request_id,
                source_layer: target_layer,
                success: false,
                data: None,
                error: Some(e.to_string()),
                processing_time_ms: processing_time,
            }),
        }
    }

    async fn handle_layer1(&self, req: UnifiedRequest) -> Result<UnifiedResponse> {
        match req.request_type.as_str() {
            "ping" => Ok(UnifiedResponse {
                response_type: "pong".to_string(),
                request_id: req.request_id,
                source_layer: "layer1".to_string(),
                success: true,
                data: Some(serde_json::json!({"layer": "Layer 1: SSR", "version": "1.0.0"})),
                error: None,
                processing_time_ms: 0.0,
            }),
            _ => Err(anyhow!("Layer 1 operation '{}' not implemented in unified socket", req.request_type)),
        }
    }

    async fn handle_layer2(&self, req: UnifiedRequest) -> Result<UnifiedResponse> {
        let layer2 = self.layer2.as_ref()
            .ok_or_else(|| anyhow!("Layer 2 not initialized"))?;

        match req.request_type.as_str() {
            "ping" => Ok(UnifiedResponse {
                response_type: "pong".to_string(),
                request_id: req.request_id,
                source_layer: "layer2".to_string(),
                success: true,
                data: Some(serde_json::json!({"layer": "Layer 2: DSR", "version": "1.0.0"})),
                error: None,
                processing_time_ms: 0.0,
            }),
            "add_memory" => {
                let pool_id = req.pool_id.as_deref().unwrap_or("discord_unified");
                let pool = layer2.get_or_create_pool(pool_id).await?;

                // Extract memory data from payload
                let memory_id = req.payload.get("memory_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| anyhow!("Missing memory_id"))?;

                let embedding: Vec<f32> = req.payload.get("embedding")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| anyhow!("Missing embedding"))?
                    .iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect();

                let content = req.payload.get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let embedding_array = ndarray::Array1::from(embedding);

                pool.add_memory_with_connection(
                    mfn_layer2_dsr::MemoryId(memory_id),
                    &embedding_array,
                    content.to_string(),
                    None,
                ).await?;

                Ok(UnifiedResponse {
                    response_type: "add_memory_response".to_string(),
                    request_id: req.request_id,
                    source_layer: "layer2".to_string(),
                    success: true,
                    data: Some(serde_json::json!({"memory_id": memory_id, "added": true})),
                    error: None,
                    processing_time_ms: 0.0,
                })
            },
            _ => Err(anyhow!("Unknown Layer 2 operation: {}", req.request_type)),
        }
    }

    async fn handle_layer3(&self, req: UnifiedRequest) -> Result<UnifiedResponse> {
        match req.request_type.as_str() {
            "ping" => Ok(UnifiedResponse {
                response_type: "pong".to_string(),
                request_id: req.request_id,
                source_layer: "layer3".to_string(),
                success: true,
                data: Some(serde_json::json!({"layer": "Layer 3: TSR", "version": "1.0.0"})),
                error: None,
                processing_time_ms: 0.0,
            }),
            _ => Err(anyhow!("Layer 3 operation '{}' not implemented in unified socket", req.request_type)),
        }
    }

    async fn handle_layer4(&self, req: UnifiedRequest) -> Result<UnifiedResponse> {
        match req.request_type.as_str() {
            "ping" => Ok(UnifiedResponse {
                response_type: "pong".to_string(),
                request_id: req.request_id,
                source_layer: "layer4".to_string(),
                success: true,
                data: Some(serde_json::json!({"layer": "Layer 4: CPE", "version": "1.0.0"})),
                error: None,
                processing_time_ms: 0.0,
            }),
            _ => Err(anyhow!("Layer 4 operation '{}' not implemented in unified socket", req.request_type)),
        }
    }

    async fn handle_layer5(&self, req: UnifiedRequest) -> Result<UnifiedResponse> {
        let layer5 = self.layer5.as_ref()
            .ok_or_else(|| anyhow!("Layer 5 not initialized"))?;

        let req_id = req.request_id.clone();
        let req_type = req.request_type.clone();

        match req_type.as_str() {
            "ping" => Ok(UnifiedResponse {
                response_type: "pong".to_string(),
                request_id: req_id,
                source_layer: "layer5".to_string(),
                success: true,
                data: Some(serde_json::json!({"layer": "Layer 5: PSR", "version": "1.0.0"})),
                error: None,
                processing_time_ms: 0.0,
            }),
            "add_pattern" => {
                // Extract pattern data from payload
                let pattern_name = req.payload.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unnamed Pattern");

                let pattern_id = req.payload.get("pattern_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                let category_str = req.payload.get("category")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Transformational");

                let category = match category_str {
                    "Temporal" => layer5_psr::PatternCategory::Temporal,
                    "Spatial" => layer5_psr::PatternCategory::Spatial,
                    "Relational" => layer5_psr::PatternCategory::Relational,
                    _ => layer5_psr::PatternCategory::Transformational,
                };

                let embedding: Vec<f32> = req.payload.get("embedding")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect()
                    })
                    .unwrap_or_else(|| vec![0.0; 256]);

                // Ensure embedding is exactly 256 dimensions
                let mut embedding_256 = embedding;
                embedding_256.resize(256, 0.0);

                // Create pattern using Pattern::new constructor
                let pattern = layer5_psr::Pattern::new(
                    pattern_id.clone(),
                    pattern_name.to_string(),
                    category,
                    embedding_256,
                );

                layer5.store_pattern(pattern)?;

                Ok(UnifiedResponse {
                    response_type: "add_pattern_response".to_string(),
                    request_id: req_id,
                    source_layer: "layer5".to_string(),
                    success: true,
                    data: Some(serde_json::json!({"pattern_id": pattern_id, "added": true})),
                    error: None,
                    processing_time_ms: 0.0,
                })
            },
            "get_pattern" => {
                let pattern_id = req.payload.get("pattern_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow!("Missing pattern_id"))?;

                match layer5.get_pattern(pattern_id)? {
                    Some(pattern) => Ok(UnifiedResponse {
                        response_type: "get_pattern_response".to_string(),
                        request_id: req_id,
                        source_layer: "layer5".to_string(),
                        success: true,
                        data: Some(serde_json::json!({
                            "pattern": {
                                "id": pattern.id,
                                "name": pattern.name,
                                "category": format!("{:?}", pattern.category),
                                "embedding_dim": pattern.embedding.len(),
                                "activation_count": pattern.activation_count,
                                "confidence": pattern.confidence,
                                "created_at": pattern.created_at,
                            }
                        })),
                        error: None,
                        processing_time_ms: 0.0,
                    }),
                    None => Err(anyhow!("Pattern not found: {}", pattern_id)),
                }
            },
            _ => Err(anyhow!("Unknown Layer 5 operation: {}", req_type)),
        }
    }
}
