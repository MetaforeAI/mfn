///! Multi-Pool Manager for CPE
///!
///! Manages multiple CPE instances (pools) within a single socket server,
///! allowing concurrent access to different pools for multi-tenant/multi-experiment scenarios.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::{ContextPredictionLayer, ContextPredictionConfig, PersistenceConfig};

/// Manager for multiple CPE pools
pub struct PoolManager {
    pools: RwLock<HashMap<String, Arc<ContextPredictionLayer>>>,
    data_dir: PathBuf,
    config: ContextPredictionConfig,
}

impl PoolManager {
    /// Create a new pool manager
    pub fn new(data_dir: PathBuf, config: ContextPredictionConfig) -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            data_dir,
            config,
        }
    }

    /// Get or create a pool by ID
    pub async fn get_or_create_pool(&self, pool_id: &str) -> Result<Arc<ContextPredictionLayer>> {
        // Fast path: check if pool already exists
        {
            let pools = self.pools.read().await;
            if let Some(pool) = pools.get(pool_id) {
                return Ok(Arc::clone(pool));
            }
        }

        // Slow path: create new pool
        let mut pools = self.pools.write().await;

        // Double-check after acquiring write lock
        if let Some(pool) = pools.get(pool_id) {
            return Ok(Arc::clone(pool));
        }

        // Create persistence config for this pool
        let persistence_config = PersistenceConfig {
            data_dir: self.data_dir.clone(),
            pool_id: pool_id.to_string(),
            fsync_interval_ms: 1000,
            snapshot_interval_secs: 300,
            aof_buffer_size: 64 * 1024,
        };

        tracing::info!("Creating new CPE pool: {}", pool_id);

        // Check if persistence files exist
        let has_existing_data = persistence_config.aof_path().exists()
            || persistence_config.snapshot_path().exists();

        if has_existing_data {
            tracing::info!("Found existing data for pool {}, will recover", pool_id);
        } else {
            tracing::info!("Creating fresh pool {}", pool_id);
        }

        // Create pool with persistence (handles recovery automatically if data exists)
        let pool = Arc::new(ContextPredictionLayer::new_with_persistence(
            self.config.clone(),
            Some(persistence_config),
        ).await?);

        pools.insert(pool_id.to_string(), Arc::clone(&pool));
        tracing::info!("Pool {} ready (total pools: {})", pool_id, pools.len());

        Ok(pool)
    }

    /// Get pool if it exists (without creating)
    pub async fn get_pool(&self, pool_id: &str) -> Option<Arc<ContextPredictionLayer>> {
        let pools = self.pools.read().await;
        pools.get(pool_id).map(Arc::clone)
    }

    /// List all active pool IDs
    pub async fn list_pools(&self) -> Vec<String> {
        let pools = self.pools.read().await;
        pools.keys().cloned().collect()
    }

    /// Get pool count
    pub async fn pool_count(&self) -> usize {
        let pools = self.pools.read().await;
        pools.len()
    }
}
