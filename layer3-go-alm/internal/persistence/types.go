package persistence

import (
	"encoding/json"
	"time"
)

// MemoryId is the unique identifier for memory items
type MemoryId uint64

// EntryType defines the type of AOF entry
type EntryType string

const (
	// AddMemory adds a new memory to the graph
	AddMemory EntryType = "add_memory"
	// UpdateMemory updates an existing memory's metadata
	UpdateMemory EntryType = "update_memory"
	// RemoveMemory removes a memory (evicted or deleted)
	RemoveMemory EntryType = "remove_memory"
	// CleanupConnection removes all memories for a connection
	CleanupConnection EntryType = "cleanup_connection"
)

// AofEntry represents a single AOF log entry
type AofEntry struct {
	TimestampMs int64           `json:"timestamp_ms"`
	EntryType   EntryType       `json:"entry_type"`
	Data        json.RawMessage `json:"data"`
}

// AddMemoryData holds data for AddMemory entry
type AddMemoryData struct {
	MemoryId     MemoryId `json:"memory_id"`
	Content      string   `json:"content"`
	ConnectionId *string  `json:"connection_id,omitempty"`
}

// UpdateMemoryData holds data for UpdateMemory entry
type UpdateMemoryData struct {
	MemoryId        MemoryId `json:"memory_id"`
	ActivationCount uint64   `json:"activation_count"`
	Strength        float32  `json:"strength"`
}

// RemoveMemoryData holds data for RemoveMemory entry
type RemoveMemoryData struct {
	MemoryId MemoryId `json:"memory_id"`
	Reason   string   `json:"reason"`
}

// CleanupConnectionData holds data for CleanupConnection entry
type CleanupConnectionData struct {
	ConnectionId string `json:"connection_id"`
}

// NewAofEntry creates a new AOF entry with current timestamp
func NewAofEntry(entryType EntryType, data interface{}) (*AofEntry, error) {
	dataBytes, err := json.Marshal(data)
	if err != nil {
		return nil, err
	}

	return &AofEntry{
		TimestampMs: time.Now().UnixMilli(),
		EntryType:   entryType,
		Data:        dataBytes,
	}, nil
}

// ToText serializes the entry to JSON text (one line)
func (e *AofEntry) ToText() (string, error) {
	bytes, err := json.Marshal(e)
	if err != nil {
		return "", err
	}
	return string(bytes), nil
}

// FromText deserializes an entry from JSON text
func FromText(text string) (*AofEntry, error) {
	var entry AofEntry
	err := json.Unmarshal([]byte(text), &entry)
	if err != nil {
		return nil, err
	}
	return &entry, nil
}

// EdgeSnapshot represents a serializable snapshot of an edge
type EdgeSnapshot struct {
	MemoryId               MemoryId `json:"memory_id"`
	Content                string   `json:"content"`
	Strength               float32  `json:"strength"`
	ActivationCount        uint64   `json:"activation_count"`
	ConnectionId           *string  `json:"connection_id,omitempty"`
	CreatedTimestampMs     int64    `json:"created_timestamp_ms"`
	LastAccessedTimestampMs int64    `json:"last_accessed_timestamp_ms"`
}

// SnapshotMetadata holds metadata about a snapshot
type SnapshotMetadata struct {
	SnapshotTimestampMs int64  `json:"snapshot_timestamp_ms"`
	EdgeCount           int    `json:"edge_count"`
	FormatVersion       uint32 `json:"format_version"`
}

// RecoveryStats holds statistics from a recovery operation
type RecoveryStats struct {
	SnapshotEdgeCount    int   `json:"snapshot_edge_count"`
	AofEntriesReplayed   int   `json:"aof_entries_replayed"`
	AofEntriesSkipped    int   `json:"aof_entries_skipped"`
	RecoveryTimeMs       int64 `json:"recovery_time_ms"`
	SnapshotAgeSecs      int64 `json:"snapshot_age_secs"`
}

// AofStats holds statistics for AOF writer
type AofStats struct {
	EntriesWritten uint64 `json:"entries_written"`
	BytesWritten   uint64 `json:"bytes_written"`
}
