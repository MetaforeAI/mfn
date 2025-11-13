//! Layer 5: Pattern Structure Registry (PSR)
//!
//! Stores and retrieves structural pattern templates for pattern-aware learning.
//!
//! # Architecture
//!
//! - **Storage**: In-memory HashMap with persistence via AOF + LMDB snapshots
//! - **Search**: Linear scan similarity search (HNSW integration pending)
//! - **Composition**: Track pattern relationships (P ∘ Q)
//! - **Socket**: `/tmp/mfn_layer5.sock` with binary protocol
//!
//! # Performance Targets
//!
//! - Storage: <1ms per pattern
//! - Search: <5ms for 10K patterns (top-5)
//! - Composition: <0.5ms (Hadamard product + normalize)
//! - Throughput: >10K ops/sec

pub mod pattern;
pub mod storage;
pub mod search;
pub mod persistence;

pub use pattern::{Pattern, PatternCategory, PatternType, TypeConstraint, Predicate};
pub use storage::PatternStorage;
pub use search::SearchEngine;
pub use persistence::{PersistenceConfig, PatternSnapshot};

use std::sync::Arc;
use parking_lot::RwLock;
use anyhow::Result;

/// Pattern Structure Registry - Main API
pub struct PatternRegistry {
    storage: Arc<RwLock<PatternStorage>>,
    search: Arc<SearchEngine>,

    // Persistence (optional)
    aof_handle: Option<persistence::AofHandle>,
    snapshot_creator: Option<Arc<persistence::SnapshotCreator>>,
    snapshot_task: Option<tokio::task::JoinHandle<()>>,
}

impl PatternRegistry {
    /// Create new pattern registry without persistence
    pub fn new() -> Self {
        Self::new_with_persistence(None).expect("Failed to create PatternRegistry")
    }

    /// Create new pattern registry with optional persistence
    pub fn new_with_persistence(
        persistence_config: Option<PersistenceConfig>,
    ) -> Result<Self> {
        let storage = Arc::new(RwLock::new(PatternStorage::new()));
        let search = Arc::new(SearchEngine::new(storage.clone()));

        // Optional persistence initialization
        let (aof_handle, snapshot_creator, snapshot_task) = if let Some(pconfig) = persistence_config {
            tracing::info!(
                "Initializing persistence: data_dir={}, pool_id={}",
                pconfig.data_dir.display(),
                pconfig.pool_id
            );

            // Create AOF writer
            let (handle, rx) = persistence::AofHandle::new();
            let aof_path = pconfig.aof_path();
            let mut aof_writer = persistence::AofWriter::new(
                &aof_path,
                rx,
                pconfig.fsync_interval_ms,
            )?;

            // Start AOF background task
            tokio::spawn(async move {
                if let Err(e) = aof_writer.run().await {
                    tracing::error!("AOF writer error: {}", e);
                }
            });

            // Create snapshot creator
            let snapshot_creator = Arc::new(
                persistence::SnapshotCreator::new(pconfig.snapshot_path())?
            );

            // Start background snapshot task
            let snapshot_task = Self::start_snapshot_task(
                storage.clone(),
                snapshot_creator.clone(),
                pconfig.snapshot_interval_secs,
            );

            tracing::info!(
                "Persistence enabled: AOF={}, snapshots every {}s",
                aof_path.display(),
                pconfig.snapshot_interval_secs
            );

            (Some(handle), Some(snapshot_creator), Some(snapshot_task))
        } else {
            (None, None, None)
        };

        Ok(Self {
            storage,
            search,
            aof_handle,
            snapshot_creator,
            snapshot_task,
        })
    }

    /// Store a pattern
    pub fn store_pattern(&self, pattern: Pattern) -> Result<String> {
        let pattern_id = pattern.id.clone();

        // Log to AOF (non-blocking) BEFORE storing to ensure durability
        if let Some(ref aof) = self.aof_handle {
            aof.log_add_pattern(
                pattern_id.clone(),
                pattern.name.clone(),
                format!("{:?}", pattern.category),
                pattern.embedding.clone(),
                None,
            )?;
        }

        self.storage.write().store(pattern)?;
        self.search.index_pattern(&pattern_id)?;

        Ok(pattern_id)
    }

    /// Get a pattern by ID
    pub fn get_pattern(&self, pattern_id: &str) -> Result<Option<Pattern>> {
        Ok(self.storage.read().get(pattern_id).cloned())
    }

    /// Search for similar patterns
    pub fn search_patterns(
        &self,
        query_embedding: &[f32],
        top_k: usize,
        min_confidence: f32,
    ) -> Result<Vec<(String, f32, Pattern)>> {
        self.search.search(query_embedding, top_k, min_confidence)
    }

    /// List all patterns
    pub fn list_patterns(&self, min_activation_count: u64, limit: usize) -> Result<Vec<Pattern>> {
        Ok(self.storage.read().list(min_activation_count, limit))
    }

    /// Update pattern statistics
    pub fn update_stats(&self, pattern_id: &str, activation_count_delta: u64, last_used_step: u64) -> Result<()> {
        // Update storage first
        self.storage.write().update_stats(pattern_id, activation_count_delta, last_used_step)?;

        // Log to AOF (non-blocking)
        if let Some(ref aof) = self.aof_handle {
            let activation_count = {
                let storage = self.storage.read();
                let pattern = storage.get(pattern_id)
                    .ok_or_else(|| anyhow::anyhow!("Pattern not found: {}", pattern_id))?;
                pattern.activation_count
            };

            aof.log_update_pattern(
                pattern_id.to_string(),
                activation_count,
                last_used_step,
            )?;
        }

        Ok(())
    }

    /// Compose two patterns (P ∘ Q)
    pub fn compose_patterns(&self, p_id: &str, q_id: &str) -> Result<Vec<f32>> {
        let storage = self.storage.read();
        let p = storage.get(p_id).ok_or_else(|| anyhow::anyhow!("Pattern {} not found", p_id))?;
        let q = storage.get(q_id).ok_or_else(|| anyhow::anyhow!("Pattern {} not found", q_id))?;

        // Hadamard product (element-wise multiplication) + normalize
        let composed = p.embedding.iter()
            .zip(q.embedding.iter())
            .map(|(a, b)| a * b)
            .collect::<Vec<f32>>();

        // L2 normalize
        let norm = composed.iter().map(|x| x * x).sum::<f32>().sqrt();
        Ok(composed.iter().map(|x| x / norm).collect())
    }

    /// Delete a pattern
    pub fn delete_pattern(&self, pattern_id: &str) -> Result<()> {
        // Log to AOF (non-blocking) BEFORE deleting to ensure durability
        if let Some(ref aof) = self.aof_handle {
            aof.log_remove_pattern(pattern_id.to_string(), "explicit_delete".to_string())?;
        }

        self.storage.write().delete(pattern_id)?;
        self.search.remove_from_index(pattern_id)?;
        Ok(())
    }

    /// Get pattern count
    pub fn pattern_count(&self) -> usize {
        self.storage.read().count()
    }

    /// Start background task for periodic snapshots
    fn start_snapshot_task(
        storage: Arc<RwLock<PatternStorage>>,
        snapshot_creator: Arc<persistence::SnapshotCreator>,
        interval_secs: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(interval_secs)
            );

            loop {
                interval.tick().await;

                // Create snapshot from current storage state
                let pattern_snapshots = {
                    let storage = storage.read();
                    let mut snapshots = std::collections::HashMap::new();

                    for id in storage.pattern_ids() {
                        if let Some(pattern) = storage.get(&id) {
                            let snapshot = persistence::PatternSnapshot {
                                pattern_id: pattern.id.clone(),
                                name: pattern.name.clone(),
                                category: format!("{:?}", pattern.category),
                                embedding: pattern.embedding.clone(),
                                activation_count: pattern.activation_count,
                                connection_id: None,
                                created_timestamp_ms: pattern.created_at,
                                last_used_timestamp_ms: pattern.last_used_step,
                                composition_history: pattern.source_patterns.clone(),
                            };
                            snapshots.insert(id.clone(), snapshot);
                        }
                    }
                    snapshots
                };

                match snapshot_creator.create_snapshot(&pattern_snapshots) {
                    Ok(_) => {
                        tracing::info!("Snapshot created: {} patterns", pattern_snapshots.len());
                    }
                    Err(e) => {
                        tracing::error!("Failed to create snapshot: {}", e);
                    }
                }
            }
        })
    }

    /// Recover PatternRegistry from persistence (snapshot + AOF replay)
    pub fn recover_from_persistence(
        persistence_config: PersistenceConfig,
    ) -> Result<Self> {
        tracing::info!(
            "Starting recovery from: {}",
            persistence_config.data_dir.display()
        );

        // Create recovery manager
        let recovery_manager = persistence::RecoveryManager::new(
            persistence_config.snapshot_path()
        )?;

        // Recover pattern snapshots from snapshot + AOF
        let (pattern_snapshots, stats) = recovery_manager.recover(
            persistence_config.aof_path()
        )?;

        tracing::info!(
            "Recovery complete: {} patterns recovered ({} from snapshot, {} from AOF) in {:.2}ms",
            stats.final_pattern_count,
            stats.snapshot_patterns_loaded,
            stats.aof_entries_replayed,
            stats.recovery_duration_ms
        );

        // Create new registry with persistence
        let registry = Self::new_with_persistence(Some(persistence_config))?;

        // Load recovered patterns into storage
        {
            let mut storage = registry.storage.write();
            for (_, snapshot) in pattern_snapshots {
                let pattern = Self::pattern_from_snapshot(snapshot)?;
                storage.store(pattern)?;
            }
        }

        Ok(registry)
    }

    /// Convert PatternSnapshot to Pattern
    fn pattern_from_snapshot(snapshot: persistence::PatternSnapshot) -> Result<Pattern> {
        use crate::pattern::PatternCategory;

        let category = match snapshot.category.as_str() {
            "Temporal" => PatternCategory::Temporal,
            "Spatial" => PatternCategory::Spatial,
            "Transformational" => PatternCategory::Transformational,
            "Relational" => PatternCategory::Relational,
            _ => PatternCategory::Transformational,
        };

        Ok(Pattern {
            id: snapshot.pattern_id,
            name: snapshot.name,
            category,
            embedding: snapshot.embedding,
            source_patterns: snapshot.composition_history,
            composable_with: vec![],
            slots: Default::default(),
            constraints: vec![],
            domain: crate::pattern::PatternType::Any,
            codomain: crate::pattern::PatternType::Any,
            text_example: String::new(),
            image_example: String::new(),
            audio_example: String::new(),
            code_example: String::new(),
            activation_count: snapshot.activation_count,
            confidence: 1.0,
            first_seen_step: snapshot.created_timestamp_ms,
            last_used_step: snapshot.last_used_timestamp_ms,
            created_at: snapshot.created_timestamp_ms,
        })
    }
}

impl Default for PatternRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_pattern(id: &str, name: &str) -> Pattern {
        Pattern {
            id: id.to_string(),
            name: name.to_string(),
            category: PatternCategory::Transformational,
            embedding: vec![0.1; 256],
            source_patterns: vec![],
            composable_with: vec![],
            slots: Default::default(),
            constraints: vec![],
            domain: PatternType::Any,
            codomain: PatternType::Any,
            text_example: format!("Example for {}", name),
            image_example: String::new(),
            audio_example: String::new(),
            code_example: String::new(),
            activation_count: 0,
            confidence: 1.0,
            first_seen_step: 0,
            last_used_step: 0,
            created_at: 0,
        }
    }

    #[test]
    fn test_store_and_get_pattern() {
        let registry = PatternRegistry::new();
        let pattern = create_test_pattern("test1", "Test Pattern");

        let id = registry.store_pattern(pattern.clone()).unwrap();
        assert_eq!(id, "test1");

        let retrieved = registry.get_pattern("test1").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Pattern");
    }

    #[test]
    fn test_pattern_count() {
        let registry = PatternRegistry::new();
        assert_eq!(registry.pattern_count(), 0);

        registry.store_pattern(create_test_pattern("p1", "Pattern 1")).unwrap();
        assert_eq!(registry.pattern_count(), 1);

        registry.store_pattern(create_test_pattern("p2", "Pattern 2")).unwrap();
        assert_eq!(registry.pattern_count(), 2);
    }

    #[test]
    fn test_delete_pattern() {
        let registry = PatternRegistry::new();
        registry.store_pattern(create_test_pattern("p1", "Pattern 1")).unwrap();

        assert_eq!(registry.pattern_count(), 1);
        registry.delete_pattern("p1").unwrap();
        assert_eq!(registry.pattern_count(), 0);

        let retrieved = registry.get_pattern("p1").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_update_stats() {
        let registry = PatternRegistry::new();
        registry.store_pattern(create_test_pattern("p1", "Pattern 1")).unwrap();

        registry.update_stats("p1", 5, 100).unwrap();

        let pattern = registry.get_pattern("p1").unwrap().unwrap();
        assert_eq!(pattern.activation_count, 5);
        assert_eq!(pattern.last_used_step, 100);
    }

    #[test]
    fn test_compose_patterns() {
        let registry = PatternRegistry::new();

        let mut p1 = create_test_pattern("p1", "Pattern 1");
        p1.embedding = vec![2.0; 256];

        let mut p2 = create_test_pattern("p2", "Pattern 2");
        p2.embedding = vec![0.5; 256];

        registry.store_pattern(p1).unwrap();
        registry.store_pattern(p2).unwrap();

        let composed = registry.compose_patterns("p1", "p2").unwrap();
        assert_eq!(composed.len(), 256);

        // Hadamard: 2.0 * 0.5 = 1.0
        // Norm: sqrt(256 * 1.0^2) = 16.0
        // Normalized: 1.0 / 16.0 = 0.0625
        assert!((composed[0] - 0.0625).abs() < 0.001);
    }

    #[test]
    fn test_list_patterns() {
        let registry = PatternRegistry::new();

        let mut p1 = create_test_pattern("p1", "Pattern 1");
        p1.activation_count = 10;

        let mut p2 = create_test_pattern("p2", "Pattern 2");
        p2.activation_count = 5;

        let p3 = create_test_pattern("p3", "Pattern 3");

        registry.store_pattern(p1).unwrap();
        registry.store_pattern(p2).unwrap();
        registry.store_pattern(p3).unwrap();

        let patterns = registry.list_patterns(5, 10).unwrap();
        assert_eq!(patterns.len(), 2); // Only p1 and p2
    }
}
