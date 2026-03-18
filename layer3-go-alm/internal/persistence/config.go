// Package persistence implements AOF + LMDB snapshot architecture for Layer 3
//
// Architecture:
// - Memory-first operations (minimal overhead)
// - Asynchronous AOF writes (background goroutine)
// - LMDB snapshots every 5 minutes
// - Recovery: Load snapshot + replay AOF (<200ms)
package persistence

import (
	"fmt"
	"os"
	"path/filepath"
)

// Config holds persistence configuration
type Config struct {
	// Base directory for persistence files
	DataDir string

	// Pool ID for multi-tenant isolation
	PoolID string

	// Fsync interval in milliseconds (default: 1000)
	FsyncIntervalMs int64

	// Snapshot interval in seconds (default: 300)
	SnapshotIntervalSecs int64

	// AOF buffer size in bytes (default: 64KB)
	AofBufferSize int
}

// DefaultConfig returns default persistence configuration for Layer 3
func DefaultConfig() *Config {
	dataDir := os.Getenv("MFN_DATA_DIR")
	if dataDir == "" {
		dataDir = "./data/mfn/memory"
	}
	return &Config{
		DataDir:              filepath.Join(dataDir, "layer3_alm"),
		PoolID:               "default",
		FsyncIntervalMs:      1000,
		SnapshotIntervalSecs: 300,
		AofBufferSize:        64 * 1024,
	}
}

// AofPath returns the AOF file path for this pool
func (c *Config) AofPath() string {
	return filepath.Join(c.DataDir, fmt.Sprintf("pool_%s.aof", c.PoolID))
}

// SnapshotPath returns the snapshot directory path for this pool
func (c *Config) SnapshotPath() string {
	return filepath.Join(c.DataDir, fmt.Sprintf("pool_%s.snapshot", c.PoolID))
}

// MetaPath returns the metadata file path for this pool
func (c *Config) MetaPath() string {
	return filepath.Join(c.DataDir, fmt.Sprintf("pool_%s.meta", c.PoolID))
}
