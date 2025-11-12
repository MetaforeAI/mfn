//! Append-Only File (AOF) writer for Layer 5 PSR
//!
//! Architecture:
//! - Buffered writes to in-memory channel (non-blocking)
//! - Background task flushes to disk with fsync interval
//! - Text-based JSON format for debugging and recovery
//! - Zero read overhead (all reads from memory)
//! - Minimal write overhead (~250ns including channel send)

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

use super::snapshot::PatternId;

/// Type of AOF entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AofEntryType {
    /// Add new pattern
    AddPattern {
        pattern_id: PatternId,
        name: String,
        category: String,
        embedding: Vec<f32>,
        connection_id: Option<String>,
    },
    /// Update pattern statistics
    UpdatePattern {
        pattern_id: PatternId,
        activation_count: u64,
        last_used_step: u64,
    },
    /// Remove pattern (deleted)
    RemovePattern {
        pattern_id: PatternId,
        reason: String,
    },
    /// Connection cleanup (remove all patterns for connection)
    CleanupConnection {
        connection_id: String,
    },
}

/// Single AOF log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AofEntry {
    /// Timestamp in milliseconds since epoch
    pub timestamp_ms: u64,
    /// Entry type and data
    pub entry_type: AofEntryType,
}

impl AofEntry {
    pub fn new(entry_type: AofEntryType) -> Self {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            timestamp_ms,
            entry_type,
        }
    }

    /// Serialize to text format (one line per entry)
    pub fn to_text(&self) -> Result<String> {
        let json = serde_json::to_string(self)?;
        Ok(json)
    }

    /// Deserialize from text format
    pub fn from_text(text: &str) -> Result<Self> {
        let entry = serde_json::from_str(text)?;
        Ok(entry)
    }
}

/// Background AOF writer with buffered I/O and periodic fsync
pub struct AofWriter {
    /// File writer with buffering
    writer: BufWriter<File>,
    /// Receive channel for AOF entries
    rx: mpsc::UnboundedReceiver<AofEntry>,
    /// Fsync interval
    fsync_interval: Duration,
    /// Last fsync time
    last_fsync: Instant,
    /// Total entries written
    entries_written: u64,
    /// Total bytes written
    bytes_written: u64,
}

impl AofWriter {
    /// Create new AOF writer
    pub fn new(
        path: impl AsRef<Path>,
        rx: mpsc::UnboundedReceiver<AofEntry>,
        fsync_interval_ms: u64,
    ) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create AOF directory")?;
        }

        // Open AOF file in append mode
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
            .context("Failed to open AOF file")?;

        let writer = BufWriter::with_capacity(64 * 1024, file);

        Ok(Self {
            writer,
            rx,
            fsync_interval: Duration::from_millis(fsync_interval_ms),
            last_fsync: Instant::now(),
            entries_written: 0,
            bytes_written: 0,
        })
    }

    /// Run the AOF writer (background task)
    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                // Receive AOF entries
                entry = self.rx.recv() => {
                    match entry {
                        Some(entry) => self.write_entry(&entry)?,
                        None => {
                            // Channel closed, flush and exit
                            self.flush()?;
                            return Ok(());
                        }
                    }
                }

                // Periodic fsync
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if self.last_fsync.elapsed() >= self.fsync_interval {
                        self.flush()?;
                    }
                }
            }
        }
    }

    /// Write single entry to buffer
    fn write_entry(&mut self, entry: &AofEntry) -> Result<()> {
        let text = entry.to_text()?;
        writeln!(self.writer, "{}", text)?;
        self.entries_written += 1;
        self.bytes_written += text.len() as u64 + 1; // +1 for newline
        Ok(())
    }

    /// Flush buffer and fsync to disk
    fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        self.writer.get_ref().sync_all()?;
        self.last_fsync = Instant::now();
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> AofStats {
        AofStats {
            entries_written: self.entries_written,
            bytes_written: self.bytes_written,
        }
    }
}

/// AOF statistics
#[derive(Debug, Clone)]
pub struct AofStats {
    pub entries_written: u64,
    pub bytes_written: u64,
}

/// Handle for sending AOF entries (non-blocking)
#[derive(Clone)]
pub struct AofHandle {
    tx: mpsc::UnboundedSender<AofEntry>,
}

impl AofHandle {
    /// Create new AOF handle and channel
    pub fn new() -> (Self, mpsc::UnboundedReceiver<AofEntry>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Log pattern addition
    pub fn log_add_pattern(
        &self,
        pattern_id: PatternId,
        name: String,
        category: String,
        embedding: Vec<f32>,
        connection_id: Option<String>,
    ) -> Result<()> {
        let entry = AofEntry::new(AofEntryType::AddPattern {
            pattern_id,
            name,
            category,
            embedding,
            connection_id,
        });
        self.tx.send(entry).context("Failed to send AOF entry")?;
        Ok(())
    }

    /// Log pattern update
    pub fn log_update_pattern(
        &self,
        pattern_id: PatternId,
        activation_count: u64,
        last_used_step: u64,
    ) -> Result<()> {
        let entry = AofEntry::new(AofEntryType::UpdatePattern {
            pattern_id,
            activation_count,
            last_used_step,
        });
        self.tx.send(entry).context("Failed to send AOF entry")?;
        Ok(())
    }

    /// Log pattern removal
    pub fn log_remove_pattern(&self, pattern_id: PatternId, reason: String) -> Result<()> {
        let entry = AofEntry::new(AofEntryType::RemovePattern {
            pattern_id,
            reason,
        });
        self.tx.send(entry).context("Failed to send AOF entry")?;
        Ok(())
    }

    /// Log connection cleanup
    pub fn log_cleanup_connection(&self, connection_id: String) -> Result<()> {
        let entry = AofEntry::new(AofEntryType::CleanupConnection { connection_id });
        self.tx.send(entry).context("Failed to send AOF entry")?;
        Ok(())
    }
}

impl Default for AofHandle {
    fn default() -> Self {
        Self::new().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_aof_entry_serialization() {
        let entry = AofEntry::new(AofEntryType::AddPattern {
            pattern_id: "p1".to_string(),
            name: "Test Pattern".to_string(),
            category: "Transformational".to_string(),
            embedding: vec![0.1, 0.2, 0.3],
            connection_id: Some("conn1".to_string()),
        });

        let text = entry.to_text().unwrap();
        let parsed = AofEntry::from_text(&text).unwrap();

        assert_eq!(entry.timestamp_ms, parsed.timestamp_ms);
        match (&entry.entry_type, &parsed.entry_type) {
            (
                AofEntryType::AddPattern { pattern_id: p1, .. },
                AofEntryType::AddPattern { pattern_id: p2, .. },
            ) => assert_eq!(p1, p2),
            _ => panic!("Entry type mismatch"),
        }
    }

    #[tokio::test]
    async fn test_aof_writer() {
        let temp_dir = TempDir::new().unwrap();
        let aof_path = temp_dir.path().join("test.aof");

        let (handle, rx) = AofHandle::new();
        let mut writer = AofWriter::new(&aof_path, rx, 100).unwrap();

        // Spawn writer task
        let writer_task = tokio::spawn(async move {
            writer.run().await
        });

        // Write entries
        handle.log_add_pattern(
            "p1".to_string(),
            "Pattern 1".to_string(),
            "Temporal".to_string(),
            vec![0.1; 256],
            Some("conn1".to_string()),
        ).unwrap();

        handle.log_update_pattern("p1".to_string(), 10, 100).unwrap();
        handle.log_remove_pattern("p1".to_string(), "test".to_string()).unwrap();

        // Close and wait
        drop(handle);
        writer_task.await.unwrap().unwrap();

        // Verify file exists and has content
        let content = std::fs::read_to_string(&aof_path).unwrap();
        assert!(content.contains("\"pattern_id\":\"p1\""));
        assert!(content.contains("AddPattern"));
        assert!(content.contains("UpdatePattern"));
        assert!(content.contains("RemovePattern"));
    }

    #[tokio::test]
    async fn test_aof_handle_operations() {
        let (handle, rx) = AofHandle::new();
        drop(rx); // Close receiver

        // Operations should fail with channel closed error
        let result = handle.log_add_pattern(
            "p1".to_string(),
            "Test".to_string(),
            "Spatial".to_string(),
            vec![0.5; 128],
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_aof_entry_types() {
        let add_entry = AofEntry::new(AofEntryType::AddPattern {
            pattern_id: "p1".to_string(),
            name: "Pattern".to_string(),
            category: "Relational".to_string(),
            embedding: vec![0.1; 64],
            connection_id: None,
        });
        assert!(matches!(add_entry.entry_type, AofEntryType::AddPattern { .. }));

        let update_entry = AofEntry::new(AofEntryType::UpdatePattern {
            pattern_id: "p1".to_string(),
            activation_count: 5,
            last_used_step: 100,
        });
        assert!(matches!(update_entry.entry_type, AofEntryType::UpdatePattern { .. }));

        let remove_entry = AofEntry::new(AofEntryType::RemovePattern {
            pattern_id: "p1".to_string(),
            reason: "evicted".to_string(),
        });
        assert!(matches!(remove_entry.entry_type, AofEntryType::RemovePattern { .. }));

        let cleanup_entry = AofEntry::new(AofEntryType::CleanupConnection {
            connection_id: "conn1".to_string(),
        });
        assert!(matches!(cleanup_entry.entry_type, AofEntryType::CleanupConnection { .. }));
    }
}
