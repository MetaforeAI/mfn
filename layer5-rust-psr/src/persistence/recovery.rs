//! Recovery manager for Layer 5 PSR
//!
//! Implements crash recovery by loading LMDB snapshot + replaying AOF.
//! Recovery process:
//! 1. Load LMDB snapshot (if exists)
//! 2. Replay AOF entries (if exists)
//! 3. Return reconstructed pattern state

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use anyhow::{Result, Context};

use super::aof::{AofEntry, AofEntryType};
use super::snapshot::{SnapshotCreator, PatternSnapshot, PatternId};

/// Recovery statistics
#[derive(Debug, Clone)]
pub struct RecoveryStats {
    /// Number of patterns loaded from snapshot
    pub snapshot_patterns_loaded: usize,
    /// Number of AOF entries replayed
    pub aof_entries_replayed: usize,
    /// Number of AOF entries skipped (corrupted)
    pub aof_entries_skipped: usize,
    /// Final pattern count
    pub final_pattern_count: usize,
    /// Recovery duration in milliseconds
    pub recovery_duration_ms: u64,
}

/// Recovery manager
pub struct RecoveryManager {
    snapshot_creator: SnapshotCreator,
}

impl RecoveryManager {
    /// Create new recovery manager
    pub fn new(snapshot_path: impl AsRef<Path>) -> Result<Self> {
        let snapshot_creator = SnapshotCreator::new(snapshot_path)?;
        Ok(Self { snapshot_creator })
    }

    /// Recover patterns from snapshot + AOF
    pub fn recover(
        &self,
        aof_path: impl AsRef<Path>,
    ) -> Result<(HashMap<PatternId, PatternSnapshot>, RecoveryStats)> {
        let start = std::time::Instant::now();

        // Step 1: Load snapshot (if exists)
        let mut patterns = match self.snapshot_creator.load_snapshot() {
            Ok(p) => p,
            Err(_) => HashMap::new(), // No snapshot, start empty
        };

        let snapshot_patterns_loaded = patterns.len();

        // Step 2: Replay AOF (if exists)
        let (aof_entries_replayed, aof_entries_skipped) = if aof_path.as_ref().exists() {
            self.replay_aof(&mut patterns, aof_path)?
        } else {
            (0, 0)
        };

        let recovery_duration_ms = start.elapsed().as_millis() as u64;

        let stats = RecoveryStats {
            snapshot_patterns_loaded,
            aof_entries_replayed,
            aof_entries_skipped,
            final_pattern_count: patterns.len(),
            recovery_duration_ms,
        };

        Ok((patterns, stats))
    }

    /// Replay AOF entries
    fn replay_aof(
        &self,
        patterns: &mut HashMap<PatternId, PatternSnapshot>,
        aof_path: impl AsRef<Path>,
    ) -> Result<(usize, usize)> {
        let file = File::open(aof_path.as_ref())
            .context("Failed to open AOF file")?;

        let reader = BufReader::new(file);
        let mut replayed = 0;
        let mut skipped = 0;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };

            if line.trim().is_empty() {
                continue;
            }

            let entry = match AofEntry::from_text(&line) {
                Ok(e) => e,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };

            self.apply_entry(patterns, &entry)?;
            replayed += 1;
        }

        Ok((replayed, skipped))
    }

    /// Apply single AOF entry
    fn apply_entry(
        &self,
        patterns: &mut HashMap<PatternId, PatternSnapshot>,
        entry: &AofEntry,
    ) -> Result<()> {
        match &entry.entry_type {
            AofEntryType::AddPattern {
                pattern_id,
                name,
                category,
                embedding,
                connection_id,
            } => {
                let snapshot = PatternSnapshot {
                    pattern_id: pattern_id.clone(),
                    name: name.clone(),
                    category: category.clone(),
                    embedding: embedding.clone(),
                    activation_count: 0,
                    connection_id: connection_id.clone(),
                    created_timestamp_ms: entry.timestamp_ms,
                    last_used_timestamp_ms: entry.timestamp_ms,
                    composition_history: vec![],
                };
                patterns.insert(pattern_id.clone(), snapshot);
            }

            AofEntryType::UpdatePattern {
                pattern_id,
                activation_count,
                last_used_step,
            } => {
                if let Some(pattern) = patterns.get_mut(pattern_id) {
                    pattern.activation_count = *activation_count;
                    pattern.last_used_timestamp_ms = *last_used_step;
                }
            }

            AofEntryType::RemovePattern { pattern_id, .. } => {
                patterns.remove(pattern_id);
            }

            AofEntryType::CleanupConnection { connection_id } => {
                patterns.retain(|_, p| {
                    p.connection_id.as_ref() != Some(connection_id)
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::aof::{AofWriter, AofHandle};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_recovery_from_aof_only() {
        let temp_dir = TempDir::new().unwrap();
        let aof_path = temp_dir.path().join("test.aof");
        let snapshot_path = temp_dir.path().join("snapshot");

        // Write AOF entries
        let (handle, rx) = AofHandle::new();
        let mut writer = AofWriter::new(&aof_path, rx, 100).unwrap();

        let writer_task = tokio::spawn(async move {
            writer.run().await
        });

        handle.log_add_pattern(
            "p1".to_string(),
            "Pattern 1".to_string(),
            "Temporal".to_string(),
            vec![0.1; 256],
            Some("conn1".to_string()),
        ).unwrap();

        handle.log_add_pattern(
            "p2".to_string(),
            "Pattern 2".to_string(),
            "Spatial".to_string(),
            vec![0.2; 256],
            None,
        ).unwrap();

        handle.log_update_pattern("p1".to_string(), 10, 100).unwrap();

        drop(handle);
        writer_task.await.unwrap().unwrap();

        // Recover
        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();
        let (patterns, stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(patterns.len(), 2);
        assert_eq!(stats.aof_entries_replayed, 3);
        assert_eq!(stats.snapshot_patterns_loaded, 0);

        let p1 = patterns.get("p1").unwrap();
        assert_eq!(p1.name, "Pattern 1");
        assert_eq!(p1.activation_count, 10);
    }

    #[tokio::test]
    async fn test_recovery_from_snapshot_and_aof() {
        let temp_dir = TempDir::new().unwrap();
        let aof_path = temp_dir.path().join("test.aof");
        let snapshot_path = temp_dir.path().join("snapshot");

        // Create snapshot
        let snapshot_creator = SnapshotCreator::new(&snapshot_path).unwrap();
        let mut patterns = HashMap::new();
        patterns.insert("p1".to_string(), PatternSnapshot {
            pattern_id: "p1".to_string(),
            name: "Initial Pattern".to_string(),
            category: "Transformational".to_string(),
            embedding: vec![0.1; 256],
            activation_count: 5,
            connection_id: None,
            created_timestamp_ms: 1000,
            last_used_timestamp_ms: 2000,
            composition_history: vec![],
        });
        snapshot_creator.create_snapshot(&patterns).unwrap();

        // Write AOF entries (after snapshot)
        let (handle, rx) = AofHandle::new();
        let mut writer = AofWriter::new(&aof_path, rx, 100).unwrap();

        let writer_task = tokio::spawn(async move {
            writer.run().await
        });

        handle.log_update_pattern("p1".to_string(), 15, 3000).unwrap();
        handle.log_add_pattern(
            "p2".to_string(),
            "New Pattern".to_string(),
            "Relational".to_string(),
            vec![0.2; 256],
            None,
        ).unwrap();

        drop(handle);
        writer_task.await.unwrap().unwrap();

        // Recover
        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();
        let (recovered, stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 2);
        assert_eq!(stats.snapshot_patterns_loaded, 1);
        assert_eq!(stats.aof_entries_replayed, 2);

        let p1 = recovered.get("p1").unwrap();
        assert_eq!(p1.activation_count, 15); // Updated from AOF
        assert_eq!(p1.last_used_timestamp_ms, 3000);

        let p2 = recovered.get("p2").unwrap();
        assert_eq!(p2.name, "New Pattern");
    }

    #[tokio::test]
    async fn test_recovery_remove_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let aof_path = temp_dir.path().join("test.aof");
        let snapshot_path = temp_dir.path().join("snapshot");

        // Create snapshot with patterns
        let snapshot_creator = SnapshotCreator::new(&snapshot_path).unwrap();
        let mut patterns = HashMap::new();
        patterns.insert("p1".to_string(), PatternSnapshot {
            pattern_id: "p1".to_string(),
            name: "Pattern 1".to_string(),
            category: "Temporal".to_string(),
            embedding: vec![0.1; 256],
            activation_count: 5,
            connection_id: None,
            created_timestamp_ms: 1000,
            last_used_timestamp_ms: 2000,
            composition_history: vec![],
        });
        snapshot_creator.create_snapshot(&patterns).unwrap();

        // Write AOF with removal
        let (handle, rx) = AofHandle::new();
        let mut writer = AofWriter::new(&aof_path, rx, 100).unwrap();

        let writer_task = tokio::spawn(async move {
            writer.run().await
        });

        handle.log_remove_pattern("p1".to_string(), "deleted".to_string()).unwrap();

        drop(handle);
        writer_task.await.unwrap().unwrap();

        // Recover
        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();
        let (recovered, _stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 0); // p1 was removed
    }

    #[tokio::test]
    async fn test_recovery_connection_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let aof_path = temp_dir.path().join("test.aof");
        let snapshot_path = temp_dir.path().join("snapshot");

        // Write AOF with multiple connections
        let (handle, rx) = AofHandle::new();
        let mut writer = AofWriter::new(&aof_path, rx, 100).unwrap();

        let writer_task = tokio::spawn(async move {
            writer.run().await
        });

        handle.log_add_pattern(
            "p1".to_string(),
            "Pattern 1".to_string(),
            "Temporal".to_string(),
            vec![0.1; 256],
            Some("conn1".to_string()),
        ).unwrap();

        handle.log_add_pattern(
            "p2".to_string(),
            "Pattern 2".to_string(),
            "Spatial".to_string(),
            vec![0.2; 256],
            Some("conn1".to_string()),
        ).unwrap();

        handle.log_add_pattern(
            "p3".to_string(),
            "Pattern 3".to_string(),
            "Relational".to_string(),
            vec![0.3; 256],
            Some("conn2".to_string()),
        ).unwrap();

        handle.log_cleanup_connection("conn1".to_string()).unwrap();

        drop(handle);
        writer_task.await.unwrap().unwrap();

        // Recover
        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();
        let (recovered, _stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 1); // Only p3 remains
        assert!(recovered.contains_key("p3"));
        assert!(!recovered.contains_key("p1"));
        assert!(!recovered.contains_key("p2"));
    }

    #[test]
    fn test_recovery_corrupted_aof() {
        let temp_dir = TempDir::new().unwrap();
        let aof_path = temp_dir.path().join("test.aof");
        let snapshot_path = temp_dir.path().join("snapshot");

        // Write corrupted AOF
        use std::io::Write;
        let mut file = File::create(&aof_path).unwrap();
        writeln!(file, r#"{{"timestamp_ms":1000,"entry_type":{{"AddPattern":{{"pattern_id":"p1","name":"Pattern 1","category":"Temporal","embedding":[0.1],"connection_id":null}}}}}}"#).unwrap();
        writeln!(file, "corrupted line here").unwrap();
        writeln!(file, r#"{{"timestamp_ms":2000,"entry_type":{{"AddPattern":{{"pattern_id":"p2","name":"Pattern 2","category":"Spatial","embedding":[0.2],"connection_id":null}}}}}}"#).unwrap();

        // Recover
        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();
        let (recovered, stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 2); // p1 and p2 recovered
        assert_eq!(stats.aof_entries_replayed, 2);
        assert_eq!(stats.aof_entries_skipped, 1); // Corrupted line skipped
    }
}
