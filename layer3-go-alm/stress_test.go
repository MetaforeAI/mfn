// Stress test for Layer 3 ALM memory management
package main

import (
	"context"
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
	"time"

	"github.com/google/uuid"
	"github.com/mfn/layer3_alm/internal/alm"
	"github.com/mfn/layer3_alm/internal/config"
)

func main() {
	fmt.Println("=== Layer 3 ALM Memory Management Stress Test ===")
	fmt.Println()

	// Get initial memory and goroutine baseline
	runtime.GC()
	var m0 runtime.MemStats
	runtime.ReadMemStats(&m0)
	initialGoroutines := runtime.NumGoroutine()

	fmt.Printf("Initial state:\n")
	fmt.Printf("  Memory: %d MB\n", m0.Alloc/1024/1024)
	fmt.Printf("  Goroutines: %d\n", initialGoroutines)
	fmt.Println()

	// Create ALM with memory limits
	cfg := config.DefaultConfig().ALMConfig
	cfg.MaxMemories = 10000
	cfg.MaxAssociations = 50000
	cfg.MaxEdgesPerNode = 100
	cfg.EdgeTTL = 30 * time.Second
	cfg.EvictionInterval = 5 * time.Second
	cfg.MaxGoroutines = 50
	cfg.EnableAutoDiscovery = false // Disable to control test better

	almInstance, err := alm.NewALM(&cfg)
	if err != nil {
		panic(fmt.Sprintf("Failed to create ALM: %v", err))
	}
	defer almInstance.Close()

	fmt.Println("Starting stress test with:")
	fmt.Printf("  Max memories: %d\n", cfg.MaxMemories)
	fmt.Printf("  Max associations: %d\n", cfg.MaxAssociations)
	fmt.Printf("  Max edges per node: %d\n", cfg.MaxEdgesPerNode)
	fmt.Printf("  Edge TTL: %v\n", cfg.EdgeTTL)
	fmt.Printf("  Max goroutines: %d\n", cfg.MaxGoroutines)
	fmt.Println()

	// Test parameters
	numClients := 20
	operationsPerClient := 500
	testDuration := 30 * time.Second

	// Metrics
	var totalMemories int64
	var totalAssociations int64
	var errors int64

	// Start test
	fmt.Printf("Running %d concurrent clients for %v...\n", numClients, testDuration)
	ctx, cancel := context.WithTimeout(context.Background(), testDuration)
	defer cancel()

	var wg sync.WaitGroup

	// Spawn client goroutines
	for i := 0; i < numClients; i++ {
		wg.Add(1)
		clientID := i

		go func(id int) {
			defer wg.Done()

			// Each client creates memories and associations
			for j := 0; j < operationsPerClient; j++ {
				select {
				case <-ctx.Done():
					return
				default:
				}

				// Add memory
				memory := &alm.Memory{
					ID:      uint64(id*10000 + j),
					Content: fmt.Sprintf("Client_%d_Memory_%d", id, j),
					Tags:    []string{"stress", fmt.Sprintf("client_%d", id)},
				}

				if err := almInstance.AddMemory(memory); err == nil {
					atomic.AddInt64(&totalMemories, 1)
				} else {
					atomic.AddInt64(&errors, 1)
				}

				// Add associations
				if j > 0 && j%5 == 0 {
					assoc := &alm.Association{
						ID:           uuid.New().String(),
						FromMemoryID: uint64(id*10000 + j),
						ToMemoryID:   uint64(id*10000 + j - 1),
						Type:         "sequential",
						Weight:       0.5,
						Reason:       "stress test",
						ConnectionID: fmt.Sprintf("client_%d", id),
					}

					if err := almInstance.AddAssociation(assoc); err == nil {
						atomic.AddInt64(&totalAssociations, 1)
					} else {
						atomic.AddInt64(&errors, 1)
					}
				}

				// Occasionally perform searches
				if j%10 == 0 {
					query := &alm.SearchQuery{
						StartMemoryIDs: []uint64{uint64(id*10000 + j)},
						MaxResults:     10,
						MaxDepth:       3,
						MinWeight:      0.1,
						Timeout:        100 * time.Millisecond,
					}
					almInstance.SearchAssociative(context.Background(), query)
				}

				// Small delay to simulate real usage
				time.Sleep(time.Millisecond)
			}
		}(clientID)
	}

	// Monitor goroutines and memory during test
	monitorDone := make(chan struct{})
	go func() {
		ticker := time.NewTicker(5 * time.Second)
		defer ticker.Stop()

		for {
			select {
			case <-ticker.C:
				runtime.GC()
				var m runtime.MemStats
				runtime.ReadMemStats(&m)

				stats := almInstance.GetGraphStats()
				metrics := almInstance.GetComprehensiveMetrics()

				fmt.Printf("\n[%s] Status:\n", time.Now().Format("15:04:05"))
				fmt.Printf("  Memory: %d MB (delta: %+d MB)\n",
					m.Alloc/1024/1024,
					(int64(m.Alloc)-int64(m0.Alloc))/1024/1024)
				fmt.Printf("  Goroutines: %d (delta: %+d)\n",
					runtime.NumGoroutine(),
					runtime.NumGoroutine()-initialGoroutines)
				fmt.Printf("  Graph: %d nodes, %d edges\n",
					stats.TotalMemories,
					stats.TotalAssociations)

				if poolStats, ok := metrics["goroutine_pool"].(map[string]interface{}); ok {
					fmt.Printf("  Pool: %v workers, %v active, %v queued\n",
						poolStats["worker_count"],
						poolStats["active_count"],
						poolStats["queue_length"])
				}

			case <-ctx.Done():
				close(monitorDone)
				return
			}
		}
	}()

	// Wait for clients to finish
	wg.Wait()
	<-monitorDone

	// Final cleanup and stats
	fmt.Println("\n=== Test Complete ===")

	// Get final stats
	runtime.GC()
	var mFinal runtime.MemStats
	runtime.ReadMemStats(&mFinal)
	finalGoroutines := runtime.NumGoroutine()

	stats := almInstance.GetGraphStats()
	metrics := almInstance.GetComprehensiveMetrics()

	fmt.Printf("\nFinal statistics:\n")
	fmt.Printf("  Operations attempted: %d memories, %d associations\n",
		totalMemories, totalAssociations)
	fmt.Printf("  Errors: %d\n", errors)
	fmt.Printf("  Final graph: %d nodes, %d edges\n",
		stats.TotalMemories, stats.TotalAssociations)
	fmt.Printf("  Memory usage: %d MB -> %d MB (delta: %+d MB)\n",
		m0.Alloc/1024/1024,
		mFinal.Alloc/1024/1024,
		(int64(mFinal.Alloc)-int64(m0.Alloc))/1024/1024)
	fmt.Printf("  Goroutines: %d -> %d (delta: %+d)\n",
		initialGoroutines,
		finalGoroutines,
		finalGoroutines-initialGoroutines)

	if poolStats, ok := metrics["goroutine_pool"].(map[string]interface{}); ok {
		fmt.Printf("  Goroutine pool total executed: %v\n", poolStats["total_executed"])
	}

	// Success criteria
	fmt.Println("\n=== Validation ===")
	success := true

	// Check memory growth (should be < 500MB for this test)
	memGrowthMB := (int64(mFinal.Alloc) - int64(m0.Alloc)) / 1024 / 1024
	if memGrowthMB > 500 {
		fmt.Printf("❌ Memory growth too high: %d MB\n", memGrowthMB)
		success = false
	} else {
		fmt.Printf("✅ Memory growth acceptable: %d MB\n", memGrowthMB)
	}

	// Check goroutine count (should not grow unbounded)
	goroutineGrowth := finalGoroutines - initialGoroutines
	if goroutineGrowth > cfg.MaxGoroutines+10 { // Allow some buffer
		fmt.Printf("❌ Too many goroutines: +%d\n", goroutineGrowth)
		success = false
	} else {
		fmt.Printf("✅ Goroutine count controlled: +%d\n", goroutineGrowth)
	}

	// Check edge limits
	if stats.TotalAssociations > cfg.MaxAssociations {
		fmt.Printf("❌ Associations exceed limit: %d > %d\n",
			stats.TotalAssociations, cfg.MaxAssociations)
		success = false
	} else {
		fmt.Printf("✅ Associations within limit: %d <= %d\n",
			stats.TotalAssociations, cfg.MaxAssociations)
	}

	if success {
		fmt.Println("\n🎉 All memory management checks PASSED!")
	} else {
		fmt.Println("\n❌ Some checks FAILED - memory management issues detected")
	}
}