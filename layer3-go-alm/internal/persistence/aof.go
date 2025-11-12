package persistence

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// AofHandle provides a thread-safe interface for logging AOF entries
type AofHandle struct {
	entryChan chan *AofEntry
}

// NewAofHandle creates a new AOF handle
func NewAofHandle() (*AofHandle, chan *AofEntry) {
	ch := make(chan *AofEntry, 1000) // Buffered channel for non-blocking writes
	return &AofHandle{entryChan: ch}, ch
}

// Log logs an AOF entry (non-blocking)
func (h *AofHandle) Log(entry *AofEntry) error {
	select {
	case h.entryChan <- entry:
		return nil
	default:
		return fmt.Errorf("AOF channel full")
	}
}

// LogAddMemory logs an add memory operation
func (h *AofHandle) LogAddMemory(memoryId MemoryId, content string, connectionId *string) error {
	entry, err := NewAofEntry(AddMemory, &AddMemoryData{
		MemoryId:     memoryId,
		Content:      content,
		ConnectionId: connectionId,
	})
	if err != nil {
		return err
	}
	return h.Log(entry)
}

// LogUpdateMemory logs an update memory operation
func (h *AofHandle) LogUpdateMemory(memoryId MemoryId, activationCount uint64, strength float32) error {
	entry, err := NewAofEntry(UpdateMemory, &UpdateMemoryData{
		MemoryId:        memoryId,
		ActivationCount: activationCount,
		Strength:        strength,
	})
	if err != nil {
		return err
	}
	return h.Log(entry)
}

// LogRemoveMemory logs a remove memory operation
func (h *AofHandle) LogRemoveMemory(memoryId MemoryId, reason string) error {
	entry, err := NewAofEntry(RemoveMemory, &RemoveMemoryData{
		MemoryId: memoryId,
		Reason:   reason,
	})
	if err != nil {
		return err
	}
	return h.Log(entry)
}

// LogCleanupConnection logs a connection cleanup operation
func (h *AofHandle) LogCleanupConnection(connectionId string) error {
	entry, err := NewAofEntry(CleanupConnection, &CleanupConnectionData{
		ConnectionId: connectionId,
	})
	if err != nil {
		return err
	}
	return h.Log(entry)
}

// Close closes the AOF handle
func (h *AofHandle) Close() {
	close(h.entryChan)
}

// AofWriter is the background AOF writer
type AofWriter struct {
	file            *os.File
	writer          *bufio.Writer
	entryChan       chan *AofEntry
	fsyncInterval   time.Duration
	lastFsync       time.Time
	entriesWritten  uint64
	bytesWritten    uint64
	mu              sync.Mutex
	stopChan        chan struct{}
	doneChan        chan struct{}
}

// NewAofWriter creates a new AOF writer
func NewAofWriter(path string, entryChan chan *AofEntry, fsyncIntervalMs int64, bufferSize int) (*AofWriter, error) {
	// Ensure parent directory exists
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		return nil, fmt.Errorf("failed to create AOF directory: %w", err)
	}

	// Open file in append mode
	file, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		return nil, fmt.Errorf("failed to open AOF file: %w", err)
	}

	return &AofWriter{
		file:          file,
		writer:        bufio.NewWriterSize(file, bufferSize),
		entryChan:     entryChan,
		fsyncInterval: time.Duration(fsyncIntervalMs) * time.Millisecond,
		lastFsync:     time.Now(),
		stopChan:      make(chan struct{}),
		doneChan:      make(chan struct{}),
	}, nil
}

// Run starts the background AOF writer loop
func (w *AofWriter) Run() error {
	defer close(w.doneChan)
	defer w.file.Close()

	ticker := time.NewTicker(100 * time.Millisecond)
	defer ticker.Stop()

	for {
		select {
		case entry, ok := <-w.entryChan:
			if !ok {
				// Channel closed, flush and exit
				return w.flush()
			}
			if err := w.writeEntry(entry); err != nil {
				return err
			}

		case <-ticker.C:
			// Periodic fsync check
			if time.Since(w.lastFsync) >= w.fsyncInterval {
				if err := w.flush(); err != nil {
					return err
				}
			}

		case <-w.stopChan:
			// Graceful shutdown
			return w.flush()
		}

		// Check if we need to fsync
		if time.Since(w.lastFsync) >= w.fsyncInterval {
			if err := w.flush(); err != nil {
				return err
			}
		}
	}
}

// writeEntry writes a single entry to the buffer
func (w *AofWriter) writeEntry(entry *AofEntry) error {
	text, err := entry.ToText()
	if err != nil {
		return fmt.Errorf("failed to serialize entry: %w", err)
	}

	n, err := fmt.Fprintln(w.writer, text)
	if err != nil {
		return fmt.Errorf("failed to write entry: %w", err)
	}

	w.mu.Lock()
	w.entriesWritten++
	w.bytesWritten += uint64(n)
	w.mu.Unlock()

	return nil
}

// flush flushes the buffer and fsyncs to disk
func (w *AofWriter) flush() error {
	if err := w.writer.Flush(); err != nil {
		return fmt.Errorf("failed to flush: %w", err)
	}
	if err := w.file.Sync(); err != nil {
		return fmt.Errorf("failed to fsync: %w", err)
	}
	w.lastFsync = time.Now()
	return nil
}

// Stop gracefully stops the AOF writer
func (w *AofWriter) Stop() error {
	close(w.stopChan)
	<-w.doneChan
	return nil
}

// Stats returns current statistics
func (w *AofWriter) Stats() AofStats {
	w.mu.Lock()
	defer w.mu.Unlock()
	return AofStats{
		EntriesWritten: w.entriesWritten,
		BytesWritten:   w.bytesWritten,
	}
}
