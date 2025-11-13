///! Multi-Pool Manager for DSR
///!
///! Manages multiple DSR instances (pools) within a single socket server,
///! allowing concurrent access to different pools for multi-tenant/multi-experiment scenarios.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

use crate::{DynamicSimilarityReservoir, DSRConfig, PersistenceConfig};

/// Manager for multiple DSR pools
pub struct PoolManager {
    pools: RwLock<HashMap<String, Arc<DynamicSimilarityReservoir>>>,
    data_dir: PathBuf,
    config: DSRConfig,
}

impl PoolManager {
    /// Create a new pool manager
    pub fn new(data_dir: PathBuf, config: DSRConfig) -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            data_dir,
            config,
        }
    }

    /// Get or create a pool by ID
    pub async fn get_or_create_pool(&self, pool_id: &str) -> Result<Arc<DynamicSimilarityReservoir>> {
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

        tracing::info!("Creating new DSR pool: {}", pool_id);

        // Create pool with persistence
        let pool = if persistence_config.aof_path().exists() || persistence_config.snapshot_path().exists() {
            tracing::info!("Recovering pool {} from persistence", pool_id);
            Arc::new(DynamicSimilarityReservoir::recover_from_persistence(
                self.config.clone(),
                persistence_config,
            ).await?)
        } else {
            tracing::info!("Creating fresh pool {}", pool_id);
            Arc::new(DynamicSimilarityReservoir::new_with_persistence(
                self.config.clone(),
                Some(persistence_config),
            )?)
        };

        pools.insert(pool_id.to_string(), Arc::clone(&pool));
        tracing::info!("Pool {} ready (total pools: {})", pool_id, pools.len());

        Ok(pool)
    }

    /// Get pool if it exists (without creating)
    pub async fn get_pool(&self, pool_id: &str) -> Option<Arc<DynamicSimilarityReservoir>> {
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
