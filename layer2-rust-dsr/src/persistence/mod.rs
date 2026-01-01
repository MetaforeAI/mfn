/// Persistence subsystem for Layer 2 (Dynamic Similarity Reservoir)
///
/// Implements Redis-style AOF (Append-Only File) + Snapshots architecture
/// for zero-overhead persistence with crash recovery.
///
/// Architecture:
/// - Memory-first operations (40ns reads, 200ns writes)
/// - Asynchronous AOF writes (background worker)
/// - LMDB snapshots every 5 minutes
/// - Recovery: Load snapshot + replay AOF (<200ms)

pub mod aof;
pub mod snapshot;
pub mod recovery;

pub use aof::{AofWriter, AofEntry, AofEntryType, AofHandle, AofStats};
pub use snapshot::{SnapshotCreator, WellSnapshot, SnapshotMetadata};
pub use recovery::{RecoveryManager, RecoveryStats};

use std::path::PathBuf;
use anyhow::Result;

/// Configuration for persistence subsystem
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// Base directory for persistence files
    pub data_dir: PathBuf,

    /// Pool ID for multi-tenant isolation
    pub pool_id: String,

    /// Fsync interval in milliseconds (default: 1000)
    pub fsync_interval_ms: u64,

    /// Snapshot interval in seconds (default: 300)
    pub snapshot_interval_secs: u64,

    /// AOF buffer size in bytes (default: 64KB)
    pub aof_buffer_size: usize,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/usr/lib/neotec/telos/mfn/memory/layer2_dsr"),
            pool_id: "default".to_string(),
            fsync_interval_ms: 1000,
            snapshot_interval_secs: 300,
            aof_buffer_size: 64 * 1024,
        }
    }
}

impl PersistenceConfig {
    /// Get AOF file path for this pool
    pub fn aof_path(&self) -> PathBuf {
        self.data_dir.join(format!("pool_{}.aof", self.pool_id))
    }

    /// Get snapshot file path for this pool
    pub fn snapshot_path(&self) -> PathBuf {
        self.data_dir.join(format!("pool_{}.snapshot", self.pool_id))
    }

    /// Get metadata file path for this pool
    pub fn meta_path(&self) -> PathBuf {
        self.data_dir.join(format!("pool_{}.meta", self.pool_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persistence_config_paths() {
        let config = PersistenceConfig {
            data_dir: PathBuf::from("/tmp/test"),
            pool_id: "test123".to_string(),
            ..Default::default()
        };

        assert_eq!(config.aof_path(), PathBuf::from("/tmp/test/pool_test123.aof"));
        assert_eq!(config.snapshot_path(), PathBuf::from("/tmp/test/pool_test123.snapshot"));
        assert_eq!(config.meta_path(), PathBuf::from("/tmp/test/pool_test123.meta"));
    }
}
