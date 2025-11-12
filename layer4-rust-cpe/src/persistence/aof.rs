/// Append-Only File (AOF) writer for Layer 4
///
/// Architecture:
/// - Buffered writes to in-memory channel (non-blocking)
/// - Background task flushes to disk with fsync interval
/// - Text-based format for debugging and recovery
/// - Zero read overhead (all reads from memory)
/// - Minimal write overhead (~250ns including channel send)

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

use mfn_core::MemoryId;

/// Type of AOF entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AofEntryType {
    /// Add new memory to similarity well
    AddMemory {
        memory_id: MemoryId,
        content: String,
        connection_id: Option<String>,
    },
    /// Update existing memory
    UpdateMemory {
        memory_id: MemoryId,
        activation_count: u64,
        strength: f32,
    },
    /// Remove memory (evicted or deleted)
    RemoveMemory {
        memory_id: MemoryId,
        reason: String,
    },
    /// Connection cleanup (remove all memories for connection)
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

        // Open file in append mode
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
            .context("Failed to open AOF file")?;

        let writer = BufWriter::with_capacity(64 * 1024, file); // 64KB buffer

        Ok(Self {
            writer,
            rx,
            fsync_interval: Duration::from_millis(fsync_interval_ms),
            last_fsync: Instant::now(),
            entries_written: 0,
            bytes_written: 0,
        })
    }

    /// Run the background AOF writer loop
    pub async fn run(&mut self) -> Result<()> {
        loop {
            // Wait for entries with timeout
            match tokio::time::timeout(Duration::from_millis(100), self.rx.recv()).await {
                Ok(Some(entry)) => {
                    // Write entry to buffer
                    self.write_entry(&entry)?;
                }
                Ok(None) => {
                    // Channel closed, flush and exit
                    self.flush()?;
                    break;
                }
                Err(_) => {
                    // Timeout, check if we need to fsync
                    if self.last_fsync.elapsed() >= self.fsync_interval {
                        self.flush()?;
                    }
                }
            }

            // Check if we need to fsync
            if self.last_fsync.elapsed() >= self.fsync_interval {
                self.flush()?;
            }
        }

        Ok(())
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
        self.writer.get_ref().sync_data()?;
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

/// Statistics for AOF writer
#[derive(Debug, Clone)]
pub struct AofStats {
    pub entries_written: u64,
    pub bytes_written: u64,
}

/// Handle for sending AOF entries from main thread
#[derive(Clone)]
pub struct AofHandle {
    tx: mpsc::UnboundedSender<AofEntry>,
}

impl AofHandle {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<AofEntry>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Log an AOF entry (non-blocking)
    pub fn log(&self, entry_type: AofEntryType) -> Result<()> {
        let entry = AofEntry::new(entry_type);
        self.tx.send(entry).context("AOF channel closed")?;
        Ok(())
    }

    /// Log add memory operation
    pub fn log_add_memory(
        &self,
        memory_id: MemoryId,
        content: String,
        connection_id: Option<String>,
    ) -> Result<()> {
        self.log(AofEntryType::AddMemory {
            memory_id,
            content,
            connection_id,
        })
    }

    /// Log update memory operation
    pub fn log_update_memory(
        &self,
        memory_id: MemoryId,
        activation_count: u64,
        strength: f32,
    ) -> Result<()> {
        self.log(AofEntryType::UpdateMemory {
            memory_id,
            activation_count,
            strength,
        })
    }

    /// Log remove memory operation
    pub fn log_remove_memory(&self, memory_id: MemoryId, reason: String) -> Result<()> {
        self.log(AofEntryType::RemoveMemory { memory_id, reason })
    }

    /// Log connection cleanup
    pub fn log_cleanup_connection(&self, connection_id: String) -> Result<()> {
        self.log(AofEntryType::CleanupConnection { connection_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    const MID_1: MemoryId = 1;
    const MID_2: MemoryId = 2;
    const MID_100: MemoryId = 100;

    #[test]
    fn test_aof_entry_serialization() {
        let entry = AofEntry::new(AofEntryType::AddMemory {
            memory_id: 12345,
            content: "test content".to_string(),
            connection_id: Some("conn123".to_string()),
        });

        let text = entry.to_text().unwrap();
        let parsed = AofEntry::from_text(&text).unwrap();

        assert_eq!(entry.timestamp_ms, parsed.timestamp_ms);

        match (entry.entry_type, parsed.entry_type) {
            (
                AofEntryType::AddMemory { memory_id: id1, content: c1, connection_id: conn1 },
                AofEntryType::AddMemory { memory_id: id2, content: c2, connection_id: conn2 },
            ) => {
                assert_eq!(id1, id2);
                assert_eq!(c1, c2);
                assert_eq!(conn1, conn2);
            }
            _ => panic!("Entry type mismatch"),
        }
    }

    #[tokio::test]
    async fn test_aof_writer_basic() {
        let temp_dir = tempfile::tempdir().unwrap();
        let aof_path = temp_dir.path().join("test.aof");

        let (handle, rx) = AofHandle::new();
        let mut writer = AofWriter::new(&aof_path, rx, 1000).unwrap();

        // Spawn writer task
        let writer_task = tokio::spawn(async move {
            writer.run().await
        });

        // Write some entries
        handle.log_add_memory(MID_1, "memory 1".to_string(), Some("conn1".to_string())).unwrap();
        handle.log_add_memory(MID_2, "memory 2".to_string(), Some("conn1".to_string())).unwrap();
        handle.log_update_memory(MID_1, 5, 0.8).unwrap();

        // Close channel
        drop(handle);

        // Wait for writer to finish
        writer_task.await.unwrap().unwrap();

        // Read and verify entries
        let file = File::open(&aof_path).unwrap();
        let reader = std::io::BufReader::new(file);
        let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();

        assert_eq!(lines.len(), 3);

        let entry1 = AofEntry::from_text(&lines[0]).unwrap();
        match entry1.entry_type {
            AofEntryType::AddMemory { memory_id, content, .. } => {
                assert_eq!(memory_id, MID_1);
                assert_eq!(content, "memory 1");
            }
            _ => panic!("Wrong entry type"),
        }
    }

    #[tokio::test]
    async fn test_aof_handle_operations() {
        let temp_dir = tempfile::tempdir().unwrap();
        let aof_path = temp_dir.path().join("test.aof");

        let (handle, rx) = AofHandle::new();
        let mut writer = AofWriter::new(&aof_path, rx, 100).unwrap();

        let writer_task = tokio::spawn(async move {
            writer.run().await
        });

        // Test all operation types
        handle.log_add_memory(MID_100, "test".to_string(), None).unwrap();
        handle.log_update_memory(MID_100, 10, 0.9).unwrap();
        handle.log_remove_memory(MID_100, "evicted".to_string()).unwrap();
        handle.log_cleanup_connection("conn123".to_string()).unwrap();

        drop(handle);
        writer_task.await.unwrap().unwrap();

        // Verify all entries were written
        let file = File::open(&aof_path).unwrap();
        let reader = std::io::BufReader::new(file);
        let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();

        assert_eq!(lines.len(), 4);
    }
}
