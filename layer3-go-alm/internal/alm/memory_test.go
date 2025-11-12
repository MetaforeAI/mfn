package alm

import (
	"fmt"
	"runtime"
	"sync"
	"testing"
	"time"

	"github.com/google/uuid"
)

// TestEdgeLimit tests that edges per node are properly limited
func TestEdgeLimit(t *testing.T) {
	// Create graph with max 10 edges per node for testing
	graph, err := NewMemoryGraphWithConfig(1000, 5000, 10, 24*time.Hour)
	if err != nil {
		t.Fatal(err)
	}

	// Add source node
	sourceNode := &Memory{
		ID:      1,
		Content: "Source",
	}
	if err := graph.AddMemory(sourceNode); err != nil {
		t.Fatal(err)
	}

	// Add 20 target nodes
	for i := 2; i <= 21; i++ {
		targetNode := &Memory{
			ID:      uint64(i),
			Content: fmt.Sprintf("Target_%d", i),
		}
		if err := graph.AddMemory(targetNode); err != nil {
			t.Fatal(err)
		}
	}

	// Add 20 edges from source - should evict after 10
	for i := 2; i <= 21; i++ {
		assoc := &Association{
			ID:           uuid.New().String(),
			FromMemoryID: 1,
			ToMemoryID:   uint64(i),
			Type:         "test",
			Weight:       float64(i) / 21.0, // Varying weights
			Reason:       "test",
			CreatedAt:    time.Now(),
			LastUsed:     time.Now(),
		}

		if err := graph.AddAssociation(assoc); err != nil {
			t.Fatal(err)
		}
	}

	// Check that source node has exactly 10 edges
	node := graph.GetNode(1)
	if node == nil {
		t.Fatal("Source node not found")
	}

	if len(node.OutEdges) != 10 {
		t.Errorf("Expected 10 edges, got %d", len(node.OutEdges))
	}

	// Verify that the weakest edges were evicted (should keep edges with higher weights)
	for toID := range node.OutEdges {
		if toID <= 11 {
			t.Errorf("Edge to node %d should have been evicted (weak weight)", toID)
		}
	}
}

// TestTTLEviction tests that edges are evicted after TTL expires
func TestTTLEviction(t *testing.T) {
	// Create graph with 1 second TTL for testing
	graph, err := NewMemoryGraphWithConfig(1000, 5000, 100, 1*time.Second)
	if err != nil {
		t.Fatal(err)
	}

	// Add nodes and edges
	for i := 1; i <= 5; i++ {
		node := &Memory{
			ID:      uint64(i),
			Content: fmt.Sprintf("Node_%d", i),
		}
		if err := graph.AddMemory(node); err != nil {
			t.Fatal(err)
		}
	}

	// Add old edges (will be evicted)
	oldTime := time.Now().Add(-2 * time.Second)
	for i := 1; i < 5; i++ {
		assoc := &Association{
			ID:           uuid.New().String(),
			FromMemoryID: uint64(i),
			ToMemoryID:   uint64(i + 1),
			Type:         "old",
			Weight:       0.5,
			Reason:       "test",
			CreatedAt:    oldTime,
			LastUsed:     oldTime,
		}
		if err := graph.AddAssociation(assoc); err != nil {
			t.Fatal(err)
		}
	}

	// Add new edges (should not be evicted)
	for i := 1; i < 3; i++ {
		assoc := &Association{
			ID:           uuid.New().String(),
			FromMemoryID: uint64(i),
			ToMemoryID:   uint64(i + 2),
			Type:         "new",
			Weight:       0.5,
			Reason:       "test",
			CreatedAt:    time.Now(),
			LastUsed:     time.Now(),
		}
		if err := graph.AddAssociation(assoc); err != nil {
			t.Fatal(err)
		}
	}

	// Check initial edge count
	stats := graph.GetStats()
	if stats.TotalAssociations != 6 {
		t.Errorf("Expected 6 associations, got %d", stats.TotalAssociations)
	}

	// Run TTL eviction
	evicted := graph.EvictExpiredEdges()
	if evicted != 4 {
		t.Errorf("Expected 4 edges to be evicted, got %d", evicted)
	}

	// Check remaining edges
	stats = graph.GetStats()
	if stats.TotalAssociations != 2 {
		t.Errorf("Expected 2 associations remaining, got %d", stats.TotalAssociations)
	}
}

// TestConnectionCleanup tests that connection data is properly cleaned up
func TestConnectionCleanup(t *testing.T) {
	graph, err := NewMemoryGraphWithConfig(1000, 5000, 100, 24*time.Hour)
	if err != nil {
		t.Fatal(err)
	}

	// Add nodes
	for i := 1; i <= 5; i++ {
		node := &Memory{
			ID:      uint64(i),
			Content: fmt.Sprintf("Node_%d", i),
		}
		if err := graph.AddMemory(node); err != nil {
			t.Fatal(err)
		}
	}

	// Add edges for connection1
	for i := 1; i < 3; i++ {
		assoc := &Association{
			ID:           uuid.New().String(),
			FromMemoryID: uint64(i),
			ToMemoryID:   uint64(i + 1),
			Type:         "conn1",
			Weight:       0.5,
			Reason:       "test",
			ConnectionID: "connection1",
			CreatedAt:    time.Now(),
			LastUsed:     time.Now(),
		}
		if err := graph.AddAssociation(assoc); err != nil {
			t.Fatal(err)
		}
	}

	// Add edges for connection2
	for i := 3; i < 5; i++ {
		assoc := &Association{
			ID:           uuid.New().String(),
			FromMemoryID: uint64(i),
			ToMemoryID:   uint64(i + 1),
			Type:         "conn2",
			Weight:       0.5,
			Reason:       "test",
			ConnectionID: "connection2",
			CreatedAt:    time.Now(),
			LastUsed:     time.Now(),
		}
		if err := graph.AddAssociation(assoc); err != nil {
			t.Fatal(err)
		}
	}

	// Check stats for connection1
	nodeCount, edgeCount := graph.GetConnectionStats("connection1")
	if nodeCount != 3 {
		t.Errorf("Expected 3 nodes for connection1, got %d", nodeCount)
	}
	if edgeCount != 2 {
		t.Errorf("Expected 2 edges for connection1, got %d", edgeCount)
	}

	// Close connection1
	nodesRemoved, edgesRemoved := graph.CloseConnection("connection1")
	if edgesRemoved != 2 {
		t.Errorf("Expected 2 edges removed, got %d", edgesRemoved)
	}
	// Nodes might not be removed if they have other connections
	_ = nodesRemoved

	// Check that connection2 data is still there
	nodeCount, edgeCount = graph.GetConnectionStats("connection2")
	if nodeCount != 3 {
		t.Errorf("Expected 3 nodes for connection2 after closing connection1, got %d", nodeCount)
	}
	if edgeCount != 2 {
		t.Errorf("Expected 2 edges for connection2 after closing connection1, got %d", edgeCount)
	}

	// Verify total edge count
	stats := graph.GetStats()
	if stats.TotalAssociations != 2 {
		t.Errorf("Expected 2 associations remaining, got %d", stats.TotalAssociations)
	}
}

// TestGoroutinePool tests that goroutine pool prevents exhaustion
func TestGoroutinePool(t *testing.T) {
	// Create pool with max 10 workers
	pool := NewGoroutinePool(10)
	defer pool.Close()

	// Track goroutines before test
	initialGoroutines := runtime.NumGoroutine()

	// Submit 100 tasks
	var wg sync.WaitGroup
	results := make([]int, 100)

	for i := 0; i < 100; i++ {
		wg.Add(1)
		idx := i
		err := pool.Submit(func() {
			defer wg.Done()
			time.Sleep(10 * time.Millisecond) // Simulate work
			results[idx] = idx * 2
		})
		if err != nil {
			wg.Done()
			t.Errorf("Failed to submit task %d: %v", i, err)
		}
	}

	// Wait for all tasks to complete
	wg.Wait()

	// Check pool stats
	stats := pool.GetStats()
	if stats.WorkerCount > 10 {
		t.Errorf("Worker count exceeded max: %d > 10", stats.WorkerCount)
	}
	if stats.TotalExecuted != 100 {
		t.Errorf("Expected 100 executed tasks, got %d", stats.TotalExecuted)
	}

	// Verify goroutine count didn't explode
	currentGoroutines := runtime.NumGoroutine()
	goroutineIncrease := currentGoroutines - initialGoroutines
	if goroutineIncrease > 15 { // Allow some buffer
		t.Errorf("Too many goroutines created: %d (increase of %d)", currentGoroutines, goroutineIncrease)
	}

	// Verify all tasks completed correctly
	for i := 0; i < 100; i++ {
		if results[i] != i*2 {
			t.Errorf("Task %d produced incorrect result: %d != %d", i, results[i], i*2)
		}
	}
}

// TestMemoryUsage tests that memory usage stays bounded under load
func TestMemoryUsage(t *testing.T) {
	graph, err := NewMemoryGraphWithConfig(1000, 5000, 50, 1*time.Hour)
	if err != nil {
		t.Fatal(err)
	}

	// Get initial memory stats
	var m1 runtime.MemStats
	runtime.ReadMemStats(&m1)
	runtime.GC()

	// Add 500 nodes
	for i := 1; i <= 500; i++ {
		node := &Memory{
			ID:      uint64(i),
			Content: fmt.Sprintf("Node_%d with some content to take up memory", i),
		}
		if err := graph.AddMemory(node); err != nil {
			t.Fatal(err)
		}
	}

	// Add many edges (but limited per node)
	for i := 1; i <= 500; i++ {
		// Add up to 100 edges per node (but limited to 50 by config)
		for j := 1; j <= 100 && j != i; j++ {
			targetID := uint64(((i + j) % 500) + 1)
			if targetID == uint64(i) {
				continue
			}

			assoc := &Association{
				ID:           uuid.New().String(),
				FromMemoryID: uint64(i),
				ToMemoryID:   targetID,
				Type:         "test",
				Weight:       0.5,
				Reason:       "test",
				CreatedAt:    time.Now(),
				LastUsed:     time.Now(),
			}
			graph.AddAssociation(assoc) // Ignore errors from hitting limits
		}
	}

	// Force GC and get memory stats
	runtime.GC()
	var m2 runtime.MemStats
	runtime.ReadMemStats(&m2)

	// Calculate memory increase
	memIncreaseMB := (m2.Alloc - m1.Alloc) / 1024 / 1024

	// Check that memory usage is reasonable (less than 100MB for this test)
	if memIncreaseMB > 100 {
		t.Errorf("Memory usage too high: %d MB", memIncreaseMB)
	}

	// Verify edge limits worked
	stats := graph.GetStats()
	maxEdgesPerNode := 0
	for i := 1; i <= 500; i++ {
		node := graph.GetNode(uint64(i))
		if node != nil {
			edges := len(node.OutEdges)
			if edges > maxEdgesPerNode {
				maxEdgesPerNode = edges
			}
			if edges > 50 {
				t.Errorf("Node %d has %d edges, exceeds limit of 50", i, edges)
			}
		}
	}

	t.Logf("Memory increase: %d MB, Max edges per node: %d, Total edges: %d",
		memIncreaseMB, maxEdgesPerNode, stats.TotalAssociations)
}