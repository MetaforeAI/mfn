// Pool Manager for Layer 3 ALM
// Manages multiple ALM instances (pools) within a single socket server,
// allowing concurrent access to different pools for multi-tenant/multi-experiment scenarios.

package alm

import (
	"fmt"
	"log"
	"sync"

	"github.com/mfn/layer3_alm/internal/config"
	"github.com/mfn/layer3_alm/internal/persistence"
)

// PoolManager manages multiple ALM instances
type PoolManager struct {
	pools   map[string]*ALM
	mu      sync.RWMutex
	dataDir string
	config  *config.ALMConfig
}

// NewPoolManager creates a new pool manager
func NewPoolManager(dataDir string, almConfig *config.ALMConfig) *PoolManager {
	return &PoolManager{
		pools:   make(map[string]*ALM),
		dataDir: dataDir,
		config:  almConfig,
	}
}

// GetOrCreatePool gets or creates a pool by ID
func (pm *PoolManager) GetOrCreatePool(poolID string) (*ALM, error) {
	// Fast path: check if pool already exists (read lock)
	pm.mu.RLock()
	if pool, exists := pm.pools[poolID]; exists {
		pm.mu.RUnlock()
		return pool, nil
	}
	pm.mu.RUnlock()

	// Slow path: create new pool (write lock)
	pm.mu.Lock()
	defer pm.mu.Unlock()

	// Double-check after acquiring write lock
	if pool, exists := pm.pools[poolID]; exists {
		return pool, nil
	}

	// Create persistence config for this pool
	persistConfig := &persistence.Config{
		DataDir:              pm.dataDir,
		PoolID:               poolID,
		FsyncIntervalMs:      1000,
		SnapshotIntervalSecs: 300,
		AofBufferSize:        64 * 1024,
	}

	log.Printf("Creating new ALM pool: %s", poolID)

	// Create the ALM instance with persistence
	almInstance, err := NewALMWithPersistence(pm.config, persistConfig)
	if err != nil {
		return nil, fmt.Errorf("failed to create pool %s: %w", poolID, err)
	}

	pm.pools[poolID] = almInstance
	log.Printf("Pool %s ready (total pools: %d)", poolID, len(pm.pools))

	return almInstance, nil
}

// GetPool retrieves a pool by ID (without creating)
func (pm *PoolManager) GetPool(poolID string) (*ALM, bool) {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	pool, exists := pm.pools[poolID]
	return pool, exists
}

// ListPools returns all active pool IDs
func (pm *PoolManager) ListPools() []string {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	poolIDs := make([]string, 0, len(pm.pools))
	for poolID := range pm.pools {
		poolIDs = append(poolIDs, poolID)
	}
	return poolIDs
}

// PoolCount returns the number of active pools
func (pm *PoolManager) PoolCount() int {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	return len(pm.pools)
}

// Close closes all pools and releases resources
func (pm *PoolManager) Close() error {
	pm.mu.Lock()
	defer pm.mu.Unlock()

	log.Printf("Closing PoolManager with %d pools...", len(pm.pools))

	for poolID, pool := range pm.pools {
		if err := pool.Close(); err != nil {
			log.Printf("Error closing pool %s: %v", poolID, err)
		}
	}

	pm.pools = make(map[string]*ALM)
	log.Println("PoolManager closed")

	return nil
}
