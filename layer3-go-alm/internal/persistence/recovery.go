package persistence

import (
	"bufio"
	"encoding/json"
	"fmt"
	"log"
	"os"
	"strings"
	"time"
)

// RecoveryManager handles crash recovery
type RecoveryManager struct {
	snapshotCreator *SnapshotCreator
}

// NewRecoveryManager creates a new recovery manager
func NewRecoveryManager(snapshotPath string) (*RecoveryManager, error) {
	creator, err := NewSnapshotCreator(snapshotPath)
	if err != nil {
		return nil, err
	}

	return &RecoveryManager{
		snapshotCreator: creator,
	}, nil
}

// Close closes the recovery manager
func (r *RecoveryManager) Close() error {
	return r.snapshotCreator.Close()
}

// Recover performs full recovery: load snapshot + replay AOF
func (r *RecoveryManager) Recover(aofPath string) (map[MemoryId]*EdgeSnapshot, *RecoveryStats, error) {
	start := time.Now()

	// Step 1: Load snapshot
	edges, err := r.snapshotCreator.LoadSnapshot()
	if err != nil {
		return nil, nil, fmt.Errorf("failed to load snapshot: %w", err)
	}

	snapshotEdgeCount := len(edges)

	// Get snapshot age
	var snapshotAgeSecs int64
	metadata, err := r.snapshotCreator.GetMetadata()
	if err == nil && metadata != nil {
		nowMs := time.Now().UnixMilli()
		snapshotAgeSecs = (nowMs - metadata.SnapshotTimestampMs) / 1000
	}

	// Step 2: Replay AOF if exists
	aofEntriesReplayed := 0
	aofEntriesSkipped := 0

	if _, err := os.Stat(aofPath); err == nil {
		replayed, skipped, err := r.replayAof(edges, aofPath)
		if err != nil {
			return nil, nil, fmt.Errorf("failed to replay AOF: %w", err)
		}
		aofEntriesReplayed = replayed
		aofEntriesSkipped = skipped
	}

	recoveryTimeMs := time.Since(start).Milliseconds()

	stats := &RecoveryStats{
		SnapshotEdgeCount:  snapshotEdgeCount,
		AofEntriesReplayed: aofEntriesReplayed,
		AofEntriesSkipped:  aofEntriesSkipped,
		RecoveryTimeMs:     recoveryTimeMs,
		SnapshotAgeSecs:    snapshotAgeSecs,
	}

	return edges, stats, nil
}

// replayAof replays AOF entries onto in-memory state
func (r *RecoveryManager) replayAof(edges map[MemoryId]*EdgeSnapshot, aofPath string) (int, int, error) {
	file, err := os.Open(aofPath)
	if err != nil {
		return 0, 0, fmt.Errorf("failed to open AOF file: %w", err)
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	replayed := 0
	skipped := 0
	lineNum := 0

	for scanner.Scan() {
		lineNum++
		line := scanner.Text()

		// Skip empty lines
		if strings.TrimSpace(line) == "" {
			continue
		}

		// Parse AOF entry
		entry, err := FromText(line)
		if err != nil {
			log.Printf("Failed to parse AOF line %d: %v", lineNum, err)
			skipped++
			continue
		}

		// Apply entry to state
		if err := r.applyEntry(edges, entry); err != nil {
			log.Printf("Failed to apply AOF entry %d: %v", lineNum, err)
			skipped++
			continue
		}

		replayed++
	}

	if err := scanner.Err(); err != nil {
		return replayed, skipped, fmt.Errorf("failed to read AOF: %w", err)
	}

	return replayed, skipped, nil
}

// applyEntry applies a single AOF entry to in-memory state
func (r *RecoveryManager) applyEntry(edges map[MemoryId]*EdgeSnapshot, entry *AofEntry) error {
	switch entry.EntryType {
	case AddMemory:
		var data AddMemoryData
		if err := json.Unmarshal(entry.Data, &data); err != nil {
			return err
		}

		// Add or update edge
		edges[data.MemoryId] = &EdgeSnapshot{
			MemoryId:                data.MemoryId,
			Content:                 data.Content,
			Strength:                1.0,
			ActivationCount:         0,
			ConnectionId:            data.ConnectionId,
			CreatedTimestampMs:      entry.TimestampMs,
			LastAccessedTimestampMs: entry.TimestampMs,
		}

	case UpdateMemory:
		var data UpdateMemoryData
		if err := json.Unmarshal(entry.Data, &data); err != nil {
			return err
		}

		// Update existing edge
		if edge, exists := edges[data.MemoryId]; exists {
			edge.ActivationCount = data.ActivationCount
			edge.Strength = data.Strength
			edge.LastAccessedTimestampMs = entry.TimestampMs
		}

	case RemoveMemory:
		var data RemoveMemoryData
		if err := json.Unmarshal(entry.Data, &data); err != nil {
			return err
		}

		// Remove edge
		delete(edges, data.MemoryId)

	case CleanupConnection:
		var data CleanupConnectionData
		if err := json.Unmarshal(entry.Data, &data); err != nil {
			return err
		}

		// Remove all edges for this connection
		for memoryId, edge := range edges {
			if edge.ConnectionId != nil && *edge.ConnectionId == data.ConnectionId {
				delete(edges, memoryId)
			}
		}
	}

	return nil
}

// CreateSnapshot creates an initial snapshot (for fresh start)
func (r *RecoveryManager) CreateSnapshot(edges map[MemoryId]*EdgeSnapshot) error {
	return r.snapshotCreator.CreateSnapshot(edges)
}
