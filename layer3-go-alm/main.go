// Memory Flow Network - Layer 3: Associative Link Mesh (ALM)
// Implements graph-based associative memory with concurrent path finding
// Target Performance: <20ms multi-hop associative search

package main

import (
	"context"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"

	"github.com/mfn/layer3_alm/internal/alm"
	"github.com/mfn/layer3_alm/internal/config"
	"github.com/mfn/layer3_alm/internal/ffi"
	"github.com/mfn/layer3_alm/internal/persistence"
	"github.com/mfn/layer3_alm/internal/server"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

func main() {
	// Load configuration
	cfg := config.DefaultConfig()

	log.Printf("🐹 MFN Layer 3 (ALM) Starting...")
	log.Printf("Configuration: %+v", cfg)

	// Create persistence config
	persistConfig := persistence.DefaultConfig()
	dataDir := persistConfig.DataDir

	// Ensure data directory exists
	if err := os.MkdirAll(dataDir, 0755); err != nil {
		log.Fatalf("Failed to create persistence directory: %v", err)
	}

	log.Printf("💾 Persistence enabled: %s", dataDir)

	// Initialize the PoolManager
	poolManager := alm.NewPoolManager(dataDir, &cfg.ALMConfig)
	defer poolManager.Close()

	// Create default pool for backwards compatibility
	defaultPool, err := poolManager.GetOrCreatePool("crucible_training")
	if err != nil {
		log.Fatalf("Failed to initialize default pool: %v", err)
	}

	log.Printf("📝 Default pool AOF: %s", filepath.Join(dataDir, "pool_crucible_training.aof"))
	log.Printf("📸 Default pool snapshots: %s", filepath.Join(dataDir, "pool_crucible_training.snapshot"))

	// Initialize FFI interface for inter-layer communication
	ffiServer := ffi.NewFFIServer(defaultPool)
	if err := ffiServer.Start(); err != nil {
		log.Fatalf("Failed to start FFI server: %v", err)
	}
	defer ffiServer.Stop()

	// Initialize Unix socket server for inter-layer communication
	unixServer := server.NewUnixSocketServer(poolManager, "/tmp/mfn_test_layer3.sock")
	if err := unixServer.Start(); err != nil {
		log.Fatalf("Failed to start Unix socket server: %v", err)
	}
	defer unixServer.Stop()

	// Initialize optimized HTTP server for monitoring and API
	httpServer := server.NewOptimizedServer(defaultPool, &cfg.ServerConfig)

	// Start metrics endpoint
	http.Handle("/metrics", promhttp.Handler())
	go func() {
		log.Printf("Metrics server starting on :%d", cfg.ServerConfig.MetricsPort)
		if err := http.ListenAndServe(fmt.Sprintf(":%d", cfg.ServerConfig.MetricsPort), nil); err != nil {
			log.Printf("Metrics server error: %v", err)
		}
	}()

	// Start main HTTP server
	go func() {
		log.Printf("ALM server starting on :%d", cfg.ServerConfig.Port)
		if err := httpServer.Start(); err != nil {
			log.Printf("HTTP server error: %v", err)
		}
	}()

	// Populate with test data for demonstration
	if cfg.ALMConfig.PopulateTestData {
		log.Println("Populating with test data...")
		if err := populateTestData(defaultPool); err != nil {
			log.Printf("Warning: Failed to populate test data: %v", err)
		}
	}

	// Wait for shutdown signal
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, syscall.SIGINT, syscall.SIGTERM)
	
	log.Println("🚀 Layer 3 ALM is ready for associative queries!")
	<-sigChan
	
	log.Println("🛑 Shutting down Layer 3 ALM...")
	
	// Graceful shutdown
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	
	if err := httpServer.Shutdown(ctx); err != nil {
		log.Printf("HTTP server shutdown error: %v", err)
	}
	
	log.Println("✅ Layer 3 ALM shutdown complete")
}

// populateTestData adds sample memories and associations for testing
func populateTestData(almInstance *alm.ALM) error {
	testMemories := []struct {
		ID      uint64
		Content string
		Tags    []string
	}{
		{1, "The human brain contains approximately 86 billion neurons", []string{"neuroscience", "facts", "brain"}},
		{2, "Neurons communicate through electrical and chemical signals", []string{"neuroscience", "communication", "brain"}},
		{3, "Machine learning models are inspired by neural networks", []string{"ai", "machine_learning", "neural_networks"}},
		{4, "Deep learning uses multiple layers of artificial neurons", []string{"ai", "deep_learning", "neural_networks"}},
		{5, "The Internet connects billions of devices worldwide", []string{"technology", "internet", "connectivity"}},
		{6, "Network protocols enable communication between computers", []string{"technology", "networking", "protocols"}},
		{7, "Social networks connect people across the globe", []string{"social", "networks", "connectivity"}},
		{8, "Graph algorithms can find shortest paths between nodes", []string{"algorithms", "graphs", "computer_science"}},
		{9, "Memory systems store and retrieve information efficiently", []string{"computer_science", "memory", "algorithms"}},
		{10, "Associative memory links related concepts together", []string{"memory", "associations", "cognitive_science"}},
	}

	for _, mem := range testMemories {
		memory := &alm.Memory{
			ID:        mem.ID,
			Content:   mem.Content,
			Tags:      mem.Tags,
			CreatedAt: time.Now(),
		}

		if err := almInstance.AddMemory(memory); err != nil {
			return fmt.Errorf("failed to add memory %d: %w", mem.ID, err)
		}
	}

	// Create some explicit associations
	associations := []struct {
		FromID, ToID uint64
		Type         string
		Weight       float64
		Reason       string
	}{
		{1, 2, "semantic", 0.9, "Both about neurons and brain"},
		{2, 3, "conceptual", 0.7, "Neural networks inspired by neurons"},
		{3, 4, "hierarchical", 0.8, "Deep learning extends machine learning"},
		{5, 6, "functional", 0.85, "Protocols enable internet communication"},
		{6, 7, "conceptual", 0.6, "Both involve network connections"},
		{8, 9, "domain", 0.7, "Both computer science algorithms"},
		{9, 10, "semantic", 0.9, "Both about memory systems"},
		{1, 10, "cognitive", 0.6, "Brain memory and associative memory"},
	}

	for _, assoc := range associations {
		association := &alm.Association{
			FromMemoryID: assoc.FromID,
			ToMemoryID:   assoc.ToID,
			Type:         assoc.Type,
			Weight:       assoc.Weight,
			Reason:       assoc.Reason,
			CreatedAt:    time.Now(),
		}

		if err := almInstance.AddAssociation(association); err != nil {
			return fmt.Errorf("failed to add association %d->%d: %w", assoc.FromID, assoc.ToID, err)
		}
	}

	log.Printf("✅ Populated %d memories and %d associations", len(testMemories), len(associations))
	return nil
}