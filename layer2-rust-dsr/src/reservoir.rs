//! Vector store for DSR Layer 2.
//!
//! Replaces the spiking neural network reservoir with direct SIMD-accelerated
//! cosine similarity search. Uses LRU eviction with Markov chain access
//! pattern tracking for spatial/temporal locality.

use crate::encoding::SpikePattern;
use crate::persistence::WellSnapshot;
use crate::similarity::simd_cosine_similarity;
use crate::{DSRConfig, MemoryId};
use anyhow::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use tracing::{debug, info, warn};

// ============================================================
// Stored entry
// ============================================================

/// A stored memory entry with embedding and access metadata.
#[derive(Debug, Clone)]
pub struct StoredEntry {
    pub memory_id: MemoryId,
    pub embedding: Vec<f32>,
    pub l2_norm: f32,
    pub content: String,
    pub connection_id: Option<String>,
    pub created_at: Instant,
    pub last_accessed: Instant,
    pub access_count: u64,
    pub strength: f32,
}

/// Backward compatibility alias.
pub type SimilarityWell = StoredEntry;

/// Backward compatibility stub (referenced in lib.rs re-export).
pub type NeuronState = ();

// ============================================================
// Memory statistics
// ============================================================

/// Memory usage statistics (kept for compatibility with lib.rs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_wells: usize,
    pub max_wells: usize,
    pub wells_created: u64,
    pub wells_evicted: u64,
    pub memory_usage_bytes: usize,
    pub memory_usage_mb: f32,
    pub connection_count: usize,
    pub ttl_seconds: u64,
}

// ============================================================
// VectorStore
// ============================================================

/// Vector store with SIMD cosine similarity and Markov-aware LRU eviction.
pub struct VectorStore {
    config: DSRConfig,

    // Dense storage for SIMD-friendly iteration
    entries: Vec<StoredEntry>,
    id_to_index: HashMap<MemoryId, usize>,

    // LRU tracking
    lru_queue: VecDeque<MemoryId>,

    // Connection tracking
    connection_entries: HashMap<String, Vec<MemoryId>>,

    // Markov chain access pattern tracking
    markov_transitions: HashMap<u64, HashMap<u64, u32>>,
    recent_accesses: VecDeque<MemoryId>,
    markov_window_size: usize,

    // Limits
    max_entries: usize,
    embedding_dim: usize,
    ttl_seconds: u64,

    // Stats
    wells_created: u64,
    wells_evicted: u64,
    memory_usage_bytes: usize,
}

/// Backward compatibility alias.
pub type SimilarityReservoir = VectorStore;

impl VectorStore {
    pub fn new(config: DSRConfig) -> Result<Self> {
        let max_entries = if config.max_similarity_wells > 0 {
            config.max_similarity_wells
        } else {
            100_000
        };
        let embedding_dim = config.embedding_dim;

        info!(
            max_entries = max_entries,
            embedding_dim = embedding_dim,
            "VectorStore initialized with SIMD cosine similarity"
        );

        Ok(Self {
            config,
            entries: Vec::with_capacity(max_entries.min(10_000)),
            id_to_index: HashMap::new(),
            lru_queue: VecDeque::new(),
            connection_entries: HashMap::new(),
            markov_transitions: HashMap::new(),
            recent_accesses: VecDeque::new(),
            markov_window_size: 10,
            max_entries,
            embedding_dim,
            ttl_seconds: 3600,
            wells_created: 0,
            wells_evicted: 0,
            memory_usage_bytes: 0,
        })
    }

    // ========================================================
    // Add / create entries
    // ========================================================

    /// Add a memory entry with embedding (no connection tracking).
    pub fn create_similarity_well(
        &mut self,
        memory_id: MemoryId,
        pattern: SpikePattern,
        content: String,
    ) -> Result<()> {
        self.create_similarity_well_with_connection(memory_id, pattern, content, None)
    }

    /// Add a memory entry with embedding and optional connection tracking.
    pub fn create_similarity_well_with_connection(
        &mut self,
        memory_id: MemoryId,
        pattern: SpikePattern,
        content: String,
        connection_id: Option<String>,
    ) -> Result<()> {
        // Evict if at capacity
        while self.entries.len() >= self.max_entries {
            self.evict_lowest_score_entry();
        }

        // Remove existing entry with same ID (upsert semantics)
        if let Some(&idx) = self.id_to_index.get(&memory_id) {
            self.remove_entry_at(idx);
        }

        let embedding_bytes = pattern.embedding.len() * 4;
        let content_bytes = content.len();

        let entry = StoredEntry {
            memory_id,
            embedding: pattern.embedding,
            l2_norm: pattern.l2_norm,
            content,
            connection_id: connection_id.clone(),
            created_at: Instant::now(),
            last_accessed: Instant::now(),
            access_count: 0,
            strength: 1.0,
        };

        let idx = self.entries.len();
        self.memory_usage_bytes += embedding_bytes + content_bytes;
        self.entries.push(entry);
        self.id_to_index.insert(memory_id, idx);
        self.lru_queue.push_back(memory_id);

        if let Some(ref cid) = connection_id {
            self.connection_entries
                .entry(cid.clone())
                .or_default()
                .push(memory_id);
        }

        self.wells_created += 1;

        debug!(
            memory_id = memory_id.0,
            total = self.entries.len(),
            "Entry added to VectorStore"
        );
        Ok(())
    }

    // ========================================================
    // Query / process
    // ========================================================

    /// Compute cosine similarity of query against all stored entries.
    /// Uses rayon for parallel scan when > 1000 entries.
    pub fn process_pattern(
        &mut self,
        query: &SpikePattern,
    ) -> Result<HashMap<MemoryId, f32>> {
        if query.l2_norm < 1e-12 || self.entries.is_empty() {
            return Ok(HashMap::new());
        }

        let query_emb = &query.embedding;
        let query_norm = query.l2_norm;

        let activations: Vec<(MemoryId, f32)> = if self.entries.len() > 1000 {
            self.entries
                .par_iter()
                .map(|entry| {
                    let sim = simd_cosine_similarity(
                        query_emb,
                        &entry.embedding,
                        query_norm,
                        entry.l2_norm,
                    );
                    (entry.memory_id, sim)
                })
                .collect()
        } else {
            self.entries
                .iter()
                .map(|entry| {
                    let sim = simd_cosine_similarity(
                        query_emb,
                        &entry.embedding,
                        query_norm,
                        entry.l2_norm,
                    );
                    (entry.memory_id, sim)
                })
                .collect()
        };

        let result: HashMap<MemoryId, f32> = activations.into_iter().collect();

        // Update access stats for entries above threshold
        let threshold = self.config.similarity_threshold;
        for entry in &mut self.entries {
            if let Some(&sim) = result.get(&entry.memory_id) {
                if sim > threshold {
                    entry.last_accessed = Instant::now();
                    entry.access_count += 1;
                }
            }
        }

        self.update_markov_transitions(&result);

        Ok(result)
    }

    // ========================================================
    // Accessors
    // ========================================================

    /// Get a reference to a stored entry by memory ID.
    pub fn get_entry(&self, memory_id: &MemoryId) -> Option<&StoredEntry> {
        self.id_to_index
            .get(memory_id)
            .map(|&idx| &self.entries[idx])
    }

    /// Backward compatibility alias for get_entry.
    pub fn get_well(&self, memory_id: &MemoryId) -> Option<&StoredEntry> {
        self.get_entry(memory_id)
    }

    pub fn get_wells_count(&self) -> usize {
        self.entries.len()
    }

    pub fn get_average_activation(&self) -> f32 {
        if self.entries.is_empty() {
            return 0.0;
        }
        self.entries.iter().map(|e| e.access_count as f32).sum::<f32>()
            / self.entries.len() as f32
    }

    pub fn estimate_memory_usage(&self) -> f32 {
        self.memory_usage_bytes as f32 / (1024.0 * 1024.0)
    }

    pub fn get_memory_ids(&self) -> Vec<MemoryId> {
        self.entries.iter().map(|e| e.memory_id).collect()
    }

    pub fn get_memory_stats(&self) -> MemoryStats {
        MemoryStats {
            total_wells: self.entries.len(),
            max_wells: self.max_entries,
            wells_created: self.wells_created,
            wells_evicted: self.wells_evicted,
            memory_usage_bytes: self.memory_usage_bytes,
            memory_usage_mb: self.memory_usage_bytes as f32 / 1_048_576.0,
            connection_count: self.connection_entries.len(),
            ttl_seconds: self.ttl_seconds,
        }
    }

    // ========================================================
    // Connection management
    // ========================================================

    /// Clean up all entries for a connection.
    pub fn cleanup_connection(&mut self, connection_id: &str) {
        if let Some(ids) = self.connection_entries.remove(connection_id) {
            let count = ids.len();
            for id in ids {
                if let Some(&idx) = self.id_to_index.get(&id) {
                    self.remove_entry_at(idx);
                    self.wells_evicted += 1;
                }
            }
            info!(
                connection_id = connection_id,
                wells_cleaned = count,
                "Cleaned up entries for disconnected connection"
            );
        }
    }

    // ========================================================
    // Optimization / maintenance
    // ========================================================

    /// Optimize: prune expired entries and compact Markov state.
    pub fn optimize_dynamics(&mut self) -> Result<()> {
        self.cleanup_expired_entries();

        // Compact Markov transitions (remove low-count entries)
        self.markov_transitions.retain(|_, transitions| {
            transitions.retain(|_, count| *count > 1);
            !transitions.is_empty()
        });

        info!(
            remaining_entries = self.entries.len(),
            "VectorStore dynamics optimized"
        );
        Ok(())
    }

    /// Set maximum entries limit.
    pub fn set_max_wells(&mut self, max_wells: usize) {
        self.max_entries = max_wells;
        while self.entries.len() > self.max_entries {
            self.evict_lowest_score_entry();
        }
    }

    /// Set TTL for entries.
    pub fn set_ttl(&mut self, ttl_seconds: u64) {
        self.ttl_seconds = ttl_seconds;
    }

    // ========================================================
    // Persistence: snapshots
    // ========================================================

    /// Export entries as snapshots for persistence.
    pub fn get_wells_for_snapshot(&self) -> HashMap<MemoryId, WellSnapshot> {
        let now = std::time::SystemTime::now();
        let now_ms = now
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        self.entries
            .iter()
            .map(|entry| {
                let created_ms =
                    now_ms.saturating_sub(entry.created_at.elapsed().as_millis() as u64);
                let accessed_ms =
                    now_ms.saturating_sub(entry.last_accessed.elapsed().as_millis() as u64);

                (
                    entry.memory_id,
                    WellSnapshot {
                        memory_id: entry.memory_id,
                        content: entry.content.clone(),
                        strength: entry.strength,
                        activation_count: entry.access_count,
                        connection_id: entry.connection_id.clone(),
                        created_timestamp_ms: created_ms,
                        last_accessed_timestamp_ms: accessed_ms,
                    },
                )
            })
            .collect()
    }

    /// Restore entries from snapshots.
    ///
    /// Restored entries have zero embeddings and will not match queries
    /// until re-added with actual embeddings.
    pub fn restore_from_snapshots(
        &mut self,
        snapshots: HashMap<MemoryId, WellSnapshot>,
    ) -> Result<()> {
        for (memory_id, snap) in snapshots {
            let entry = StoredEntry {
                memory_id,
                embedding: vec![0.0; self.embedding_dim],
                l2_norm: 0.0,
                content: snap.content,
                connection_id: snap.connection_id.clone(),
                created_at: Instant::now(),
                last_accessed: Instant::now(),
                access_count: snap.activation_count,
                strength: snap.strength,
            };

            let idx = self.entries.len();
            self.memory_usage_bytes += entry.embedding.len() * 4 + entry.content.len();
            self.entries.push(entry);
            self.id_to_index.insert(memory_id, idx);
            self.lru_queue.push_back(memory_id);

            if let Some(ref cid) = snap.connection_id {
                self.connection_entries
                    .entry(cid.clone())
                    .or_default()
                    .push(memory_id);
            }
        }

        info!(
            total_entries = self.entries.len(),
            "Restored entries from snapshots"
        );
        Ok(())
    }

    // ========================================================
    // Markov chain access pattern tracking
    // ========================================================

    fn update_markov_transitions(&mut self, activations: &HashMap<MemoryId, f32>) {
        let threshold = self.config.similarity_threshold;

        let mut top: Vec<_> = activations
            .iter()
            .filter(|(_, &score)| score > threshold)
            .collect();
        top.sort_by(|a, b| {
            b.1.partial_cmp(a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        top.truncate(5);

        let accessed: Vec<MemoryId> = top.iter().map(|(&id, _)| id).collect();

        // Record transitions: recent -> current
        for recent in &self.recent_accesses {
            for current in &accessed {
                if recent != current {
                    *self
                        .markov_transitions
                        .entry(recent.0)
                        .or_default()
                        .entry(current.0)
                        .or_insert(0) += 1;
                }
            }
        }

        for id in &accessed {
            self.recent_accesses.push_back(*id);
        }
        while self.recent_accesses.len() > self.markov_window_size {
            self.recent_accesses.pop_front();
        }
    }

    /// How likely is this entry to be accessed next based on Markov chains?
    fn compute_markov_prediction(&self, memory_id: MemoryId) -> f32 {
        let mut score = 0.0f32;
        for recent in &self.recent_accesses {
            if let Some(transitions) = self.markov_transitions.get(&recent.0) {
                let total: u32 = transitions.values().sum();
                if total > 0 {
                    if let Some(&count) = transitions.get(&memory_id.0) {
                        score += count as f32 / total as f32;
                    }
                }
            }
        }
        score
    }

    // ========================================================
    // Eviction
    // ========================================================

    /// Evict the entry with lowest combined score (recency * frequency * markov).
    fn evict_lowest_score_entry(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        let now = Instant::now();
        let max_ac = self
            .entries
            .iter()
            .map(|e| e.access_count)
            .max()
            .unwrap_or(1)
            .max(1);

        let mut worst_idx = 0;
        let mut worst_score = f32::MAX;

        for (idx, entry) in self.entries.iter().enumerate() {
            let secs = now.duration_since(entry.last_accessed).as_secs_f32();
            let recency = 1.0 / (1.0 + secs / 3600.0);
            let frequency =
                (1.0 + entry.access_count as f32).ln() / (1.0 + max_ac as f32).ln();
            let markov = self.compute_markov_prediction(entry.memory_id);
            let score = recency * frequency * (1.0 + markov);

            if score < worst_score {
                worst_score = score;
                worst_idx = idx;
            }
        }

        let evicted_id = self.entries[worst_idx].memory_id;
        self.remove_entry_at(worst_idx);
        self.wells_evicted += 1;

        debug!(
            memory_id = evicted_id.0,
            score = worst_score,
            "Evicted lowest-score entry"
        );
    }

    fn cleanup_expired_entries(&mut self) {
        let now = Instant::now();
        let ttl = self.ttl_seconds;

        let expired: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| now.duration_since(e.last_accessed).as_secs() > ttl)
            .map(|(idx, _)| idx)
            .collect();

        // Remove in reverse order to preserve indices
        for &idx in expired.iter().rev() {
            self.remove_entry_at(idx);
            self.wells_evicted += 1;
        }

        if !expired.is_empty() {
            debug!(
                count = expired.len(),
                "Cleaned up expired entries"
            );
        }
    }

    /// Remove entry at index using swap_remove for O(1).
    fn remove_entry_at(&mut self, idx: usize) {
        if idx >= self.entries.len() {
            return;
        }

        let removed = self.entries.swap_remove(idx);
        self.id_to_index.remove(&removed.memory_id);
        self.memory_usage_bytes = self
            .memory_usage_bytes
            .saturating_sub(removed.embedding.len() * 4 + removed.content.len());

        // LRU queue cleanup
        self.lru_queue.retain(|id| *id != removed.memory_id);

        // Markov cleanup
        self.markov_transitions.remove(&removed.memory_id.0);

        // Fix swapped entry's index (swap_remove moves last element to idx)
        if idx < self.entries.len() {
            let swapped_id = self.entries[idx].memory_id;
            self.id_to_index.insert(swapped_id, idx);
        }
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::EmbeddingPattern;

    fn make_config(max_wells: usize) -> DSRConfig {
        DSRConfig {
            reservoir_size: 100,
            embedding_dim: 5,
            max_similarity_wells: max_wells,
            similarity_threshold: 0.7,
            ..DSRConfig::default()
        }
    }

    fn make_pattern(values: &[f32]) -> SpikePattern {
        EmbeddingPattern::from_embedding(values.to_vec())
    }

    #[test]
    fn test_store_creation() {
        let store = VectorStore::new(make_config(1000)).unwrap();
        assert_eq!(store.get_wells_count(), 0);
    }

    #[test]
    fn test_add_and_retrieve() {
        let mut store = VectorStore::new(make_config(1000)).unwrap();
        let pattern = make_pattern(&[0.1, 0.2, 0.3, 0.4, 0.5]);
        let id = MemoryId(1);

        store
            .create_similarity_well(id, pattern, "test".to_string())
            .unwrap();

        assert_eq!(store.get_wells_count(), 1);
        assert!(store.get_entry(&id).is_some());
        assert_eq!(store.get_entry(&id).unwrap().content, "test");
    }

    #[test]
    fn test_eviction_at_capacity() {
        let mut store = VectorStore::new(make_config(3)).unwrap();

        for i in 0..4u64 {
            let pattern = make_pattern(&[i as f32 * 0.1, 0.2, 0.3, 0.4, 0.5]);
            store
                .create_similarity_well(
                    MemoryId(i),
                    pattern,
                    format!("mem {}", i),
                )
                .unwrap();
        }

        assert_eq!(store.get_wells_count(), 3);
    }

    #[test]
    fn test_connection_cleanup() {
        let mut store = VectorStore::new(make_config(1000)).unwrap();
        let conn = "conn-1";

        for i in 0..3u64 {
            let pattern = make_pattern(&[i as f32 * 0.1, 0.2, 0.3, 0.4, 0.5]);
            store
                .create_similarity_well_with_connection(
                    MemoryId(i),
                    pattern,
                    format!("mem {}", i),
                    Some(conn.to_string()),
                )
                .unwrap();
        }

        assert_eq!(store.get_wells_count(), 3);
        store.cleanup_connection(conn);
        assert_eq!(store.get_wells_count(), 0);
    }

    #[test]
    fn test_process_pattern() {
        let mut store = VectorStore::new(make_config(1000)).unwrap();

        let p1 = make_pattern(&[1.0, 0.0, 0.0, 0.0, 0.0]);
        let p2 = make_pattern(&[0.0, 1.0, 0.0, 0.0, 0.0]);

        store
            .create_similarity_well(MemoryId(1), p1.clone(), "x-axis".to_string())
            .unwrap();
        store
            .create_similarity_well(MemoryId(2), p2, "y-axis".to_string())
            .unwrap();

        let results = store.process_pattern(&p1).unwrap();
        assert_eq!(results.len(), 2);

        // Self-similarity should be ~1.0
        let self_sim = results[&MemoryId(1)];
        assert!(
            (self_sim - 1.0).abs() < 1e-4,
            "Expected ~1.0, got {}",
            self_sim
        );

        // Orthogonal should be ~0.0
        let ortho_sim = results[&MemoryId(2)];
        assert!(
            ortho_sim.abs() < 1e-4,
            "Expected ~0.0, got {}",
            ortho_sim
        );
    }

    #[test]
    fn test_memory_stats() {
        let mut store = VectorStore::new(make_config(1000)).unwrap();
        let pattern = make_pattern(&[0.1, 0.2, 0.3, 0.4, 0.5]);
        store
            .create_similarity_well(MemoryId(1), pattern, "test".to_string())
            .unwrap();

        let stats = store.get_memory_stats();
        assert_eq!(stats.total_wells, 1);
        assert_eq!(stats.max_wells, 1000);
        assert!(stats.memory_usage_bytes > 0);
    }

    #[test]
    fn test_snapshot_roundtrip() {
        let mut store = VectorStore::new(make_config(1000)).unwrap();
        let pattern = make_pattern(&[0.1, 0.2, 0.3, 0.4, 0.5]);
        store
            .create_similarity_well(MemoryId(42), pattern, "hello".to_string())
            .unwrap();

        let snapshots = store.get_wells_for_snapshot();
        assert_eq!(snapshots.len(), 1);
        assert!(snapshots.contains_key(&MemoryId(42)));

        let mut store2 = VectorStore::new(make_config(1000)).unwrap();
        store2.restore_from_snapshots(snapshots).unwrap();
        assert_eq!(store2.get_wells_count(), 1);
        assert_eq!(
            store2.get_entry(&MemoryId(42)).unwrap().content,
            "hello"
        );
    }

    #[test]
    fn test_optimize_dynamics() {
        let mut store = VectorStore::new(make_config(1000)).unwrap();
        let pattern = make_pattern(&[0.1, 0.2, 0.3, 0.4, 0.5]);
        store
            .create_similarity_well(MemoryId(1), pattern, "test".to_string())
            .unwrap();

        // Should not error
        store.optimize_dynamics().unwrap();
    }
}
