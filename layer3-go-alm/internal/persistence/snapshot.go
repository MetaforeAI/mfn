package persistence

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"os"
	"time"

	"github.com/bmatsuo/lmdb-go/lmdb"
)

// SnapshotCreator creates and loads LMDB snapshots
type SnapshotCreator struct {
	env *lmdb.Env
	dbi lmdb.DBI
}

// NewSnapshotCreator creates a new snapshot creator
func NewSnapshotCreator(path string) (*SnapshotCreator, error) {
	// Ensure directory exists
	if err := os.MkdirAll(path, 0755); err != nil {
		return nil, fmt.Errorf("failed to create snapshot directory: %w", err)
	}

	// Open LMDB environment
	env, err := lmdb.NewEnv()
	if err != nil {
		return nil, fmt.Errorf("failed to create LMDB environment: %w", err)
	}

	if err := env.SetMaxDBs(2); err != nil {
		env.Close()
		return nil, fmt.Errorf("failed to set max DBs: %w", err)
	}

	if err := env.SetMapSize(100 * 1024 * 1024); err != nil { // 100MB max
		env.Close()
		return nil, fmt.Errorf("failed to set map size: %w", err)
	}

	if err := env.Open(path, 0, 0644); err != nil {
		env.Close()
		return nil, fmt.Errorf("failed to open LMDB environment: %w", err)
	}

	// Create/open database
	var dbi lmdb.DBI
	err = env.Update(func(txn *lmdb.Txn) error {
		var err error
		dbi, err = txn.OpenDBI("edges", lmdb.Create)
		return err
	})
	if err != nil {
		env.Close()
		return nil, fmt.Errorf("failed to create/open database: %w", err)
	}

	return &SnapshotCreator{
		env: env,
		dbi: dbi,
	}, nil
}

// Close closes the snapshot creator
func (s *SnapshotCreator) Close() error {
	s.env.Close()
	return nil
}

// CreateSnapshot creates a snapshot from in-memory edges
func (s *SnapshotCreator) CreateSnapshot(edges map[MemoryId]*EdgeSnapshot) error {
	return s.env.Update(func(txn *lmdb.Txn) error {
		// Clear existing data
		if err := txn.Drop(s.dbi, false); err != nil {
			return fmt.Errorf("failed to clear database: %w", err)
		}

		// Write all edges
		for memoryId, edge := range edges {
			key := make([]byte, 8)
			binary.BigEndian.PutUint64(key, uint64(memoryId))

			value, err := json.Marshal(edge)
			if err != nil {
				return fmt.Errorf("failed to serialize edge: %w", err)
			}

			if err := txn.Put(s.dbi, key, value, 0); err != nil {
				return fmt.Errorf("failed to write edge: %w", err)
			}
		}

		// Write metadata
		metadata := &SnapshotMetadata{
			SnapshotTimestampMs: time.Now().UnixMilli(),
			EdgeCount:           len(edges),
			FormatVersion:       1,
		}

		metaDBI, err := txn.OpenDBI("metadata", lmdb.Create)
		if err != nil {
			return fmt.Errorf("failed to open metadata DB: %w", err)
		}

		metaValue, err := json.Marshal(metadata)
		if err != nil {
			return fmt.Errorf("failed to serialize metadata: %w", err)
		}

		if err := txn.Put(metaDBI, []byte("metadata"), metaValue, 0); err != nil {
			return fmt.Errorf("failed to write metadata: %w", err)
		}

		return nil
	})
}

// LoadSnapshot loads a snapshot into memory
func (s *SnapshotCreator) LoadSnapshot() (map[MemoryId]*EdgeSnapshot, error) {
	edges := make(map[MemoryId]*EdgeSnapshot)

	err := s.env.View(func(txn *lmdb.Txn) error {
		cursor, err := txn.OpenCursor(s.dbi)
		if err != nil {
			return fmt.Errorf("failed to open cursor: %w", err)
		}
		defer cursor.Close()

		for {
			key, value, err := cursor.Get(nil, nil, lmdb.Next)
			if lmdb.IsNotFound(err) {
				break
			}
			if err != nil {
				return fmt.Errorf("failed to read cursor: %w", err)
			}

			// Skip if key is not 8 bytes
			if len(key) != 8 {
				continue
			}

			memoryId := MemoryId(binary.BigEndian.Uint64(key))

			var edge EdgeSnapshot
			if err := json.Unmarshal(value, &edge); err != nil {
				return fmt.Errorf("failed to deserialize edge: %w", err)
			}

			edges[memoryId] = &edge
		}

		return nil
	})

	return edges, err
}

// GetMetadata retrieves snapshot metadata
func (s *SnapshotCreator) GetMetadata() (*SnapshotMetadata, error) {
	var metadata *SnapshotMetadata

	err := s.env.View(func(txn *lmdb.Txn) error {
		metaDBI, err := txn.OpenDBI("metadata", 0)
		if lmdb.IsNotFound(err) {
			return nil
		}
		if err != nil {
			return fmt.Errorf("failed to open metadata DB: %w", err)
		}

		value, err := txn.Get(metaDBI, []byte("metadata"))
		if lmdb.IsNotFound(err) {
			return nil
		}
		if err != nil {
			return fmt.Errorf("failed to get metadata: %w", err)
		}

		metadata = &SnapshotMetadata{}
		if err := json.Unmarshal(value, metadata); err != nil {
			return fmt.Errorf("failed to deserialize metadata: %w", err)
		}

		return nil
	})

	return metadata, err
}

// SnapshotSize returns approximate size in bytes
func (s *SnapshotCreator) SnapshotSize() (int64, error) {
	var size int64
	err := s.env.View(func(txn *lmdb.Txn) error {
		stat, err := txn.Stat(s.dbi)
		if err != nil {
			return err
		}
		size = int64(stat.PSize) * int64(stat.Depth)
		return nil
	})
	return size, err
}
