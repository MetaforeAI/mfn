/// Recovery manager for Layer 4 persistence
///
/// Implements fast crash recovery by:
/// 1. Loading LMDB snapshot (~100ms)
/// 2. Replaying AOF entries since snapshot (~10ms)
/// Total recovery time: <200ms

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use anyhow::{Result, Context, anyhow};

use mfn_core::MemoryId;
use super::{AofEntry, AofEntryType, SnapshotCreator};
use super::snapshot::WellSnapshot;

/// Statistics from recovery operation
#[derive(Debug, Clone)]
pub struct RecoveryStats {
    /// Number of wells loaded from snapshot
    pub snapshot_well_count: usize,

    /// Number of AOF entries replayed
    pub aof_entries_replayed: usize,

    /// Number of AOF entries skipped (corrupted or invalid)
    pub aof_entries_skipped: usize,

    /// Total recovery time in milliseconds
    pub recovery_time_ms: u64,

    /// Snapshot age in seconds
    pub snapshot_age_secs: u64,
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

    /// Perform full recovery: load snapshot + replay AOF
    pub fn recover(
        &self,
        aof_path: impl AsRef<Path>,
    ) -> Result<(HashMap<MemoryId, WellSnapshot>, RecoveryStats)> {
        let start = std::time::Instant::now();

        // Step 1: Load snapshot
        let mut wells = self.snapshot_creator.load_snapshot()
            .context("Failed to load snapshot")?;

        let snapshot_well_count = wells.len();

        // Get snapshot age
        let snapshot_age_secs = match self.snapshot_creator.get_metadata()? {
            Some(metadata) => {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                ((now_ms - metadata.snapshot_timestamp_ms) / 1000) as u64
            }
            None => 0,
        };

        // Step 2: Replay AOF if exists
        let (aof_entries_replayed, aof_entries_skipped) =
            if aof_path.as_ref().exists() {
                self.replay_aof(&mut wells, aof_path)?
            } else {
                (0, 0)
            };

        let recovery_time_ms = start.elapsed().as_millis() as u64;

        let stats = RecoveryStats {
            snapshot_well_count,
            aof_entries_replayed,
            aof_entries_skipped,
            recovery_time_ms,
            snapshot_age_secs,
        };

        Ok((wells, stats))
    }

    /// Replay AOF entries onto in-memory state
    fn replay_aof(
        &self,
        wells: &mut HashMap<MemoryId, WellSnapshot>,
        aof_path: impl AsRef<Path>,
    ) -> Result<(usize, usize)> {
        let file = File::open(aof_path).context("Failed to open AOF file")?;
        let reader = BufReader::new(file);

        let mut replayed = 0;
        let mut skipped = 0;

        for (line_num, line_result) in reader.lines().enumerate() {
            let line = match line_result {
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!("Failed to read AOF line {}: {}", line_num + 1, e);
                    skipped += 1;
                    continue;
                }
            };

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Parse AOF entry
            let entry = match AofEntry::from_text(&line) {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("Failed to parse AOF line {}: {}", line_num + 1, e);
                    skipped += 1;
                    continue;
                }
            };

            // Apply entry to state
            if let Err(e) = self.apply_entry(wells, entry) {
                tracing::warn!("Failed to apply AOF entry {}: {}", line_num + 1, e);
                skipped += 1;
                continue;
            }

            replayed += 1;
        }

        Ok((replayed, skipped))
    }

    /// Apply single AOF entry to in-memory state
    fn apply_entry(
        &self,
        wells: &mut HashMap<MemoryId, WellSnapshot>,
        entry: AofEntry,
    ) -> Result<()> {
        match entry.entry_type {
            AofEntryType::AddMemory { memory_id, content, connection_id } => {
                // Add or update well
                let well = WellSnapshot {
                    memory_id,
                    content,
                    strength: 1.0,
                    activation_count: 0,
                    connection_id,
                    created_timestamp_ms: entry.timestamp_ms,
                    last_accessed_timestamp_ms: entry.timestamp_ms,
                };
                wells.insert(memory_id, well);
            }

            AofEntryType::UpdateMemory { memory_id, activation_count, strength } => {
                // Update existing well
                if let Some(well) = wells.get_mut(&memory_id) {
                    well.activation_count = activation_count;
                    well.strength = strength;
                    well.last_accessed_timestamp_ms = entry.timestamp_ms;
                }
            }

            AofEntryType::RemoveMemory { memory_id, .. } => {
                // Remove well
                wells.remove(&memory_id);
            }

            AofEntryType::CleanupConnection { connection_id } => {
                // Remove all wells for this connection
                wells.retain(|_, well| {
                    well.connection_id.as_ref() != Some(&connection_id)
                });
            }
        }

        Ok(())
    }

    /// Create initial snapshot (for fresh start)
    pub fn create_snapshot(&self, wells: &HashMap<MemoryId, WellSnapshot>) -> Result<()> {
        self.snapshot_creator.create_snapshot(wells)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const MID_1: MemoryId = 1;
    const MID_2: MemoryId = 2;
    const MID_3: MemoryId = 3;

    fn create_test_well(memory_id: MemoryId, content: &str) -> WellSnapshot {
        WellSnapshot {
            memory_id,
            content: content.to_string(),
            strength: 0.8,
            activation_count: 5,
            connection_id: Some("conn123".to_string()),
            created_timestamp_ms: 1000,
            last_accessed_timestamp_ms: 2000,
        }
    }

    #[test]
    fn test_recovery_snapshot_only() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snapshot_path = temp_dir.path().join("snapshot");
        let aof_path = temp_dir.path().join("test.aof");

        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();

        // Create snapshot
        let mut wells = HashMap::new();
        wells.insert(MID_1, create_test_well(MID_1, "memory 1"));
        wells.insert(MID_2, create_test_well(MID_2, "memory 2"));
        recovery_mgr.create_snapshot(&wells).unwrap();

        // Recover (no AOF)
        let (recovered, stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 2);
        assert_eq!(stats.snapshot_well_count, 2);
        assert_eq!(stats.aof_entries_replayed, 0);
    }

    #[test]
    fn test_recovery_with_aof() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snapshot_path = temp_dir.path().join("snapshot");
        let aof_path = temp_dir.path().join("test.aof");

        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();

        // Create snapshot
        let mut wells = HashMap::new();
        wells.insert(MID_1, create_test_well(MID_1, "memory 1"));
        recovery_mgr.create_snapshot(&wells).unwrap();

        // Create AOF with additional entries
        let mut aof_file = File::create(&aof_path).unwrap();

        let add_entry = AofEntry::new(AofEntryType::AddMemory {
            memory_id: MID_2,
            content: "memory 2".to_string(),
            connection_id: Some("conn123".to_string()),
        });
        writeln!(aof_file, "{}", add_entry.to_text().unwrap()).unwrap();

        let update_entry = AofEntry::new(AofEntryType::UpdateMemory {
            memory_id: MID_1,
            activation_count: 10,
            strength: 0.9,
        });
        writeln!(aof_file, "{}", update_entry.to_text().unwrap()).unwrap();

        drop(aof_file);

        // Recover
        let (recovered, stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 2);
        assert_eq!(stats.snapshot_well_count, 1);
        assert_eq!(stats.aof_entries_replayed, 2);

        // Check memory 1 was updated
        assert_eq!(recovered.get(&MID_1).unwrap().activation_count, 10);
        assert_eq!(recovered.get(&MID_1).unwrap().strength, 0.9);

        // Check memory 2 was added
        assert_eq!(recovered.get(&MID_2).unwrap().content, "memory 2");
    }

    #[test]
    fn test_recovery_with_remove() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snapshot_path = temp_dir.path().join("snapshot");
        let aof_path = temp_dir.path().join("test.aof");

        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();

        // Create snapshot with 2 wells
        let mut wells = HashMap::new();
        wells.insert(MID_1, create_test_well(MID_1, "memory 1"));
        wells.insert(MID_2, create_test_well(MID_2, "memory 2"));
        recovery_mgr.create_snapshot(&wells).unwrap();

        // Create AOF that removes one
        let mut aof_file = File::create(&aof_path).unwrap();

        let remove_entry = AofEntry::new(AofEntryType::RemoveMemory {
            memory_id: MID_1,
            reason: "evicted".to_string(),
        });
        writeln!(aof_file, "{}", remove_entry.to_text().unwrap()).unwrap();

        drop(aof_file);

        // Recover
        let (recovered, stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 1);
        assert!(recovered.contains_key(&MID_2));
        assert!(!recovered.contains_key(&MID_1));
        assert_eq!(stats.aof_entries_replayed, 1);
    }

    #[test]
    fn test_recovery_connection_cleanup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snapshot_path = temp_dir.path().join("snapshot");
        let aof_path = temp_dir.path().join("test.aof");

        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();

        // Create snapshot with wells from different connections
        let mut wells = HashMap::new();

        let mut well1 = create_test_well(MID_1, "memory 1");
        well1.connection_id = Some("conn1".to_string());
        wells.insert(MID_1, well1);

        let mut well2 = create_test_well(MID_2, "memory 2");
        well2.connection_id = Some("conn2".to_string());
        wells.insert(MID_2, well2);

        recovery_mgr.create_snapshot(&wells).unwrap();

        // Create AOF that cleans up conn1
        let mut aof_file = File::create(&aof_path).unwrap();

        let cleanup_entry = AofEntry::new(AofEntryType::CleanupConnection {
            connection_id: "conn1".to_string(),
        });
        writeln!(aof_file, "{}", cleanup_entry.to_text().unwrap()).unwrap();

        drop(aof_file);

        // Recover
        let (recovered, _) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 1);
        assert!(recovered.contains_key(&MID_2));
        assert!(!recovered.contains_key(&MID_1));
    }

    #[test]
    fn test_recovery_corrupted_aof() {
        let temp_dir = tempfile::tempdir().unwrap();
        let snapshot_path = temp_dir.path().join("snapshot");
        let aof_path = temp_dir.path().join("test.aof");

        let recovery_mgr = RecoveryManager::new(&snapshot_path).unwrap();

        // Create snapshot
        let mut wells = HashMap::new();
        wells.insert(MID_1, create_test_well(MID_1, "memory 1"));
        recovery_mgr.create_snapshot(&wells).unwrap();

        // Create AOF with mix of valid and corrupted entries
        let mut aof_file = File::create(&aof_path).unwrap();

        // Valid entry
        let add_entry = AofEntry::new(AofEntryType::AddMemory {
            memory_id: MID_2,
            content: "memory 2".to_string(),
            connection_id: None,
        });
        writeln!(aof_file, "{}", add_entry.to_text().unwrap()).unwrap();

        // Corrupted entry
        writeln!(aof_file, "{{corrupted json}}").unwrap();

        // Another valid entry
        let add_entry2 = AofEntry::new(AofEntryType::AddMemory {
            memory_id: MID_3,
            content: "memory 3".to_string(),
            connection_id: None,
        });
        writeln!(aof_file, "{}", add_entry2.to_text().unwrap()).unwrap();

        drop(aof_file);

        // Recover
        let (recovered, stats) = recovery_mgr.recover(&aof_path).unwrap();

        assert_eq!(recovered.len(), 3); // snapshot + 2 valid AOF entries
        assert_eq!(stats.aof_entries_replayed, 2);
        assert_eq!(stats.aof_entries_skipped, 1); // corrupted entry skipped
    }
}
