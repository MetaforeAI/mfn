//! Memory Flow Network - Layer 2: Dynamic Similarity Reservoir (DSR)
//!
//! Implements vector-based semantic similarity detection with:
//! - Zero-training memory addition via direct embedding storage
//! - Sub-1ms similarity search using SIMD cosine similarity
//! - LMDB snapshots and AOF for crash-safe persistence
//! - LRU eviction and per-connection cleanup
//!
//! Target Performance:
//! - Similarity search: <1ms with SIMD cosine similarity
//! - Memory addition: Zero-training, instant storage
//! - Scalability: 100k+ embeddings with graceful degradation
//! - Integration: FFI bindings with Zig Layer 1

use std::sync::Arc;
use tokio::sync::RwLock;
use ndarray::Array1;
use anyhow::Result;
use serde::{Deserialize, Serialize};

pub mod encoding;
pub mod reservoir;
pub mod similarity;
pub mod dynamics;
pub mod compression;
pub mod ffi;
pub mod socket_server;
pub mod binary_protocol;
pub mod persistence;
pub mod pool_manager;

// Re-exports for convenience
pub use encoding::{SpikeEncoder, EncodingStrategy, SpikePattern, EmbeddingPattern};
pub use reservoir::{SimilarityReservoir, VectorStore, StoredEntry, NeuronState, MemoryStats};
pub use similarity::{SimilarityResults, SimilarityMatcher, SimilarityMatch};
pub use dynamics::{SpikeDynamics, TemporalWindow};
pub use socket_server::{
    SocketServer, SocketServerConfig, SocketRequest, SocketResponse,
};
pub use binary_protocol::{
    BinarySerializer, BinaryDeserializer, BinaryMessageType, BinaryMessageHeader,
};
pub use persistence::{
    PersistenceConfig,
    AofWriter, AofEntry, AofEntryType, AofHandle,
    SnapshotCreator, WellSnapshot,
    RecoveryManager, RecoveryStats,
};
pub use pool_manager::PoolManager;

/// Core embedding type used throughout Layer 2
pub type Embedding = Array1<f32>;

/// Unique identifier for memory items
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(pub u64);

/// Configuration for the Dynamic Similarity Reservoir
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DSRConfig {
    /// Number of neurons in the reservoir (kept for backward compatibility)
    pub reservoir_size: usize,
    /// Dimensionality of input embeddings
    pub embedding_dim: usize,
    /// Spike encoding strategy to use (kept for config compatibility)
    pub encoding_strategy: EncodingStrategy,
    /// Similarity threshold for matching (0.0 to 1.0)
    pub similarity_threshold: f32,
    /// Competitive dynamics strength
    pub competition_strength: f32,
    /// Time window for spike integration (milliseconds)
    pub integration_window_ms: f32,
    /// Maximum number of similarity wells
    pub max_similarity_wells: usize,
}

impl Default for DSRConfig {
    fn default() -> Self {
        Self {
            reservoir_size: 2000,
            embedding_dim: 512,
            encoding_strategy: EncodingStrategy::RateCoding,
            similarity_threshold: 0.7,
            competition_strength: 0.9,
            integration_window_ms: 10.0,
            max_similarity_wells: 100_000,
        }
    }
}

/// Main Dynamic Similarity Reservoir implementation.
///
/// Wraps a VectorStore for direct embedding storage and SIMD cosine
/// similarity search, with optional AOF + LMDB snapshot persistence.
pub struct DynamicSimilarityReservoir {
    config: DSRConfig,
    reservoir: Arc<RwLock<VectorStore>>,
    matcher: Arc<SimilarityMatcher>,

    // Persistence (optional)
    aof_handle: Option<persistence::AofHandle>,
    snapshot_creator: Option<Arc<persistence::SnapshotCreator>>,
    snapshot_task: Option<tokio::task::JoinHandle<()>>,

    // Performance metrics
    total_queries: std::sync::atomic::AtomicU64,
    total_additions: std::sync::atomic::AtomicU64,
    cache_hits: std::sync::atomic::AtomicU64,
}

impl DynamicSimilarityReservoir {
    /// Create a new Dynamic Similarity Reservoir with the given configuration
    pub fn new(config: DSRConfig) -> Result<Self> {
        Self::new_with_persistence(config, None)
    }

    /// Create a new Dynamic Similarity Reservoir with optional persistence
    pub fn new_with_persistence(
        config: DSRConfig,
        persistence_config: Option<PersistenceConfig>,
    ) -> Result<Self> {
        let reservoir = Arc::new(RwLock::new(
            VectorStore::new(config.clone())?
        ));
        let matcher = Arc::new(SimilarityMatcher::new(config.clone()));

        let (aof_handle, snapshot_creator, snapshot_task) =
            Self::init_persistence(&config, &reservoir, persistence_config)?;

        Ok(Self {
            config,
            reservoir,
            matcher,
            aof_handle,
            snapshot_creator,
            snapshot_task,
            total_queries: std::sync::atomic::AtomicU64::new(0),
            total_additions: std::sync::atomic::AtomicU64::new(0),
            cache_hits: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Initialize persistence subsystem (AOF writer + snapshot task).
    /// Returns (aof_handle, snapshot_creator, snapshot_task) or None
    /// for each if persistence is disabled.
    fn init_persistence(
        _config: &DSRConfig,
        reservoir: &Arc<RwLock<VectorStore>>,
        persistence_config: Option<PersistenceConfig>,
    ) -> Result<(
        Option<persistence::AofHandle>,
        Option<Arc<persistence::SnapshotCreator>>,
        Option<tokio::task::JoinHandle<()>>,
    )> {
        let pconfig = match persistence_config {
            Some(pc) => pc,
            None => return Ok((None, None, None)),
        };

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
            reservoir.clone(),
            snapshot_creator.clone(),
            pconfig.snapshot_interval_secs,
        );

        tracing::info!(
            "Persistence enabled: AOF={}, snapshots every {}s",
            aof_path.display(),
            pconfig.snapshot_interval_secs
        );

        Ok((Some(handle), Some(snapshot_creator), Some(snapshot_task)))
    }

    /// Add a new memory with its embedding to the reservoir.
    /// Creates a stored entry directly from the embedding vector.
    pub async fn add_memory(
        &self,
        memory_id: MemoryId,
        embedding: &Embedding,
        content: String,
    ) -> Result<()> {
        self.add_memory_with_connection(memory_id, embedding, content, None).await
    }

    /// Add a new memory with optional connection tracking.
    pub async fn add_memory_with_connection(
        &self,
        memory_id: MemoryId,
        embedding: &Embedding,
        content: String,
        connection_id: Option<String>,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();

        // Create embedding pattern directly (no spike encoding)
        let pattern = EmbeddingPattern::from_embedding(embedding.to_vec());

        // Store in VectorStore
        {
            let mut reservoir = self.reservoir.write().await;
            reservoir.create_similarity_well_with_connection(
                memory_id,
                pattern,
                content.clone(),
                connection_id.clone(),
            )?;
        }

        // Log to AOF (non-blocking, ~250ns overhead)
        if let Some(ref aof) = self.aof_handle {
            aof.log_add_memory(memory_id, content, connection_id)?;
        }

        self.total_additions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        tracing::debug!(
            memory_id = memory_id.0,
            duration_ms = start_time.elapsed().as_secs_f32() * 1000.0,
            "Memory added to Layer 2"
        );

        Ok(())
    }

    /// Clean up all entries associated with a connection.
    pub async fn cleanup_connection(&self, connection_id: &str) -> Result<()> {
        let mut reservoir = self.reservoir.write().await;
        reservoir.cleanup_connection(connection_id);
        Ok(())
    }

    /// Search for similar memories using the query embedding.
    /// Returns top-k most similar memories with confidence scores.
    pub async fn similarity_search(
        &self,
        query_embedding: &Embedding,
        top_k: usize,
    ) -> Result<SimilarityResults> {
        let start_time = std::time::Instant::now();

        // Create embedding pattern directly (no spike encoding)
        let query_pattern = EmbeddingPattern::from_embedding(query_embedding.to_vec());

        // Run SIMD cosine similarity matching
        let results = {
            let mut reservoir = self.reservoir.write().await;
            self.matcher.find_similar(&mut *reservoir, &query_pattern, top_k).await?
        };

        self.total_queries.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let duration_ms = start_time.elapsed().as_secs_f32() * 1000.0;
        tracing::debug!(
            query_duration_ms = duration_ms,
            results_count = results.matches.len(),
            "Similarity search completed"
        );

        Ok(results)
    }

    /// Get performance statistics for monitoring and optimization.
    pub async fn get_performance_stats(&self) -> DSRPerformanceStats {
        let reservoir = self.reservoir.read().await;
        let memory_stats = reservoir.get_memory_stats();

        DSRPerformanceStats {
            total_queries: self.total_queries.load(std::sync::atomic::Ordering::Relaxed),
            total_additions: self.total_additions.load(std::sync::atomic::Ordering::Relaxed),
            cache_hits: self.cache_hits.load(std::sync::atomic::Ordering::Relaxed),
            similarity_wells_count: memory_stats.total_wells,
            reservoir_size: self.config.reservoir_size,
            average_well_activation: reservoir.get_average_activation(),
            memory_usage_mb: memory_stats.memory_usage_mb,
            max_wells: memory_stats.max_wells,
            wells_evicted: memory_stats.wells_evicted,
            connection_count: memory_stats.connection_count,
        }
    }

    /// Optimize reservoir performance by pruning inactive entries.
    pub async fn optimize_reservoir(&self) -> Result<()> {
        let mut reservoir = self.reservoir.write().await;
        reservoir.optimize_dynamics()?;
        Ok(())
    }

    /// Synchronous version of add_memory for FFI compatibility.
    pub fn add_memory_sync(
        &self,
        memory_id: MemoryId,
        embedding: &Embedding,
        content: String,
    ) -> Result<()> {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(self.add_memory(memory_id, embedding, content)),
            Err(_) => {
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(self.add_memory(memory_id, embedding, content))
            }
        }
    }

    /// Synchronous version of get_performance_stats for FFI compatibility.
    pub fn get_performance_stats_sync(&self) -> DSRPerformanceStats {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(self.get_performance_stats()),
            Err(_) => {
                // Fallback: return stats with current atomic counters only
                DSRPerformanceStats {
                    total_queries: self.total_queries.load(std::sync::atomic::Ordering::Relaxed),
                    total_additions: self.total_additions.load(std::sync::atomic::Ordering::Relaxed),
                    cache_hits: self.cache_hits.load(std::sync::atomic::Ordering::Relaxed),
                    similarity_wells_count: 0,
                    reservoir_size: self.config.reservoir_size,
                    average_well_activation: 0.0,
                    memory_usage_mb: 0.0,
                    max_wells: self.config.max_similarity_wells,
                    wells_evicted: 0,
                    connection_count: 0,
                }
            }
        }
    }

    /// Synchronous version of optimize_reservoir for FFI compatibility.
    pub fn optimize_reservoir_sync(&self) -> Result<()> {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => handle.block_on(self.optimize_reservoir()),
            Err(_) => {
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(self.optimize_reservoir())
            }
        }
    }

    /// Start background task for periodic snapshots.
    fn start_snapshot_task(
        reservoir: Arc<RwLock<VectorStore>>,
        snapshot_creator: Arc<persistence::SnapshotCreator>,
        interval_secs: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_secs(interval_secs)
            );

            loop {
                interval.tick().await;

                let wells = {
                    let reservoir = reservoir.read().await;
                    reservoir.get_wells_for_snapshot()
                };

                match snapshot_creator.create_snapshot(&wells) {
                    Ok(_) => {
                        tracing::info!("Snapshot created: {} wells", wells.len());
                    }
                    Err(e) => {
                        tracing::error!("Failed to create snapshot: {}", e);
                    }
                }
            }
        })
    }

    /// Recover DSR from persistence (snapshot + AOF replay).
    pub async fn recover_from_persistence(
        config: DSRConfig,
        persistence_config: PersistenceConfig,
    ) -> Result<Self> {
        tracing::info!(
            "Starting recovery from: {}",
            persistence_config.data_dir.display()
        );

        let recovery_manager = persistence::RecoveryManager::new(
            persistence_config.snapshot_path()
        )?;

        let (wells, stats) = recovery_manager.recover(
            persistence_config.aof_path()
        )?;

        tracing::info!(
            "Recovery complete: {} wells, {} AOF entries replayed, {}ms",
            wells.len(),
            stats.aof_entries_replayed,
            stats.recovery_time_ms
        );

        let dsr = Self::new_with_persistence(config, Some(persistence_config))?;

        // Populate reservoir with recovered wells
        {
            let mut reservoir = dsr.reservoir.write().await;
            reservoir.restore_from_snapshots(wells)?;
        }

        tracing::info!("DSR recovery complete and operational");

        Ok(dsr)
    }
}

/// Performance statistics for Layer 2 monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DSRPerformanceStats {
    pub total_queries: u64,
    pub total_additions: u64,
    pub cache_hits: u64,
    pub similarity_wells_count: usize,
    pub reservoir_size: usize,
    pub average_well_activation: f32,
    pub memory_usage_mb: f32,
    pub max_wells: usize,
    pub wells_evicted: u64,
    pub connection_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[tokio::test]
    async fn test_basic_similarity_search() {
        let mut config = DSRConfig::default();
        config.embedding_dim = 5;
        config.reservoir_size = 100;
        let dsr = DynamicSimilarityReservoir::new(config).unwrap();

        let embedding = array![0.8, 0.9, 0.7, 0.6, 0.5];
        let memory_id = MemoryId(1);
        dsr.add_memory(memory_id, &embedding, "test memory".to_string())
            .await
            .unwrap();

        let query_embedding = array![0.7, 0.8, 0.8, 0.7, 0.6];
        let results = dsr.similarity_search(&query_embedding, 5).await.unwrap();

        assert!(!results.matches.is_empty(), "Expected to find similarity matches");
        if !results.matches.is_empty() {
            let confidence = results.matches[0].confidence;
            assert!(!confidence.is_nan(), "Confidence should not be NaN");
            assert!(confidence > 0.0, "Confidence should be positive for matches");
        }
    }

    #[test]
    fn test_memory_addition_performance() {
        let mut config = DSRConfig::default();
        config.reservoir_size = 500;
        let dsr = DynamicSimilarityReservoir::new(config).unwrap();

        let start_time = std::time::Instant::now();

        for i in 0..100 {
            let embedding = Array1::from(vec![i as f32 / 100.0; 384]);
            let memory_id = MemoryId(i as u64);
            dsr.add_memory_sync(memory_id, &embedding, format!("memory {}", i))
                .unwrap();
        }

        let duration = start_time.elapsed();
        let avg_duration_ms = duration.as_secs_f32() * 1000.0 / 100.0;

        println!("Average memory addition time: {:.3}ms", avg_duration_ms);
        assert!(
            avg_duration_ms < 5.0,
            "Memory addition should be < 5ms on average for 384D embeddings"
        );
    }

    #[tokio::test]
    async fn test_real_similarity_matching() {
        let mut config = DSRConfig::default();
        config.embedding_dim = 10;
        config.reservoir_size = 200;
        config.similarity_threshold = 0.3;
        let dsr = DynamicSimilarityReservoir::new(config).unwrap();

        let memory1 = array![0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1, 0.0];
        let memory2 = array![0.85, 0.75, 0.65, 0.55, 0.45, 0.35, 0.25, 0.15, 0.05, 0.0];
        let memory3 = array![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];

        dsr.add_memory(MemoryId(1), &memory1, "Pattern A".to_string())
            .await.unwrap();
        dsr.add_memory(MemoryId(2), &memory2, "Similar to A".to_string())
            .await.unwrap();
        dsr.add_memory(MemoryId(3), &memory3, "Opposite pattern".to_string())
            .await.unwrap();

        let query = array![0.88, 0.78, 0.68, 0.58, 0.48, 0.38, 0.28, 0.18, 0.08, 0.0];
        let results = dsr.similarity_search(&query, 3).await.unwrap();

        println!("\n=== Similarity Search Results ===");
        println!("Processing time: {:.3}ms", results.processing_time_ms);
        println!("Wells evaluated: {}", results.wells_evaluated);
        for (i, match_result) in results.matches.iter().enumerate() {
            println!(
                "Match {}: memory_id={}, confidence={:.3}, content='{}'",
                i + 1,
                match_result.memory_id.0,
                match_result.confidence,
                match_result.content
            );
        }

        assert_eq!(results.matches.len(), 3, "Should return all 3 matches");
        assert!(results.processing_time_ms >= 0.0, "Should have non-negative processing time");
        assert!(results.processing_time_ms < 100.0, "Should be fast (<100ms)");

        let similar_confidences: Vec<f32> = results
            .matches
            .iter()
            .filter(|m| m.memory_id.0 == 1 || m.memory_id.0 == 2)
            .map(|m| m.confidence)
            .collect();

        let opposite_confidence = results
            .matches
            .iter()
            .find(|m| m.memory_id.0 == 3)
            .map(|m| m.confidence)
            .unwrap_or(0.0);

        println!("\nSimilar pattern confidences: {:?}", similar_confidences);
        println!("Opposite pattern confidence: {:.3}", opposite_confidence);

        let has_higher_similar = similar_confidences.iter().any(|&c| c > opposite_confidence);
        println!("Has higher similar confidence: {}", has_higher_similar);
        assert!(has_higher_similar, "Similar patterns should rank higher than opposite");
    }
}
