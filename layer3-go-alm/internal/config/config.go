package config

import (
	"fmt"
	"time"
)

// Config holds all configuration for Layer 3 ALM
type Config struct {
	ALMConfig    ALMConfig    `json:"alm"`
	ServerConfig ServerConfig `json:"server"`
}

// ALMConfig configures the Associative Link Mesh
type ALMConfig struct {
	// Graph configuration
	MaxMemories         int           `json:"max_memories"`
	MaxAssociations     int           `json:"max_associations"`
	DefaultAssocWeight  float64       `json:"default_assoc_weight"`
	WeightDecayRate     float64       `json:"weight_decay_rate"`
	
	// Search configuration
	MaxSearchDepth      int           `json:"max_search_depth"`
	MaxSearchResults    int           `json:"max_search_results"`
	SearchTimeout       time.Duration `json:"search_timeout"`
	MinAssocThreshold   float64       `json:"min_assoc_threshold"`
	
	// Concurrency configuration
	MaxSearchWorkers    int `json:"max_search_workers"`
	MaxPathfindWorkers  int `json:"max_pathfind_workers"`
	
	// Memory management
	GCInterval          time.Duration `json:"gc_interval"`
	MaxIdleTime         time.Duration `json:"max_idle_time"`
	
	// Performance optimizations
	EnableObjectPooling bool `json:"enable_object_pooling"`
	EnableMemoryCache   bool `json:"enable_memory_cache"`
	EnableQueryCache    bool `json:"enable_query_cache"`
	CacheSize           int  `json:"cache_size"`
	EnableParallelSearch bool `json:"enable_parallel_search"`
	
	// Features
	EnableAutoDiscovery bool `json:"enable_auto_discovery"`
	EnableWeightDecay   bool `json:"enable_weight_decay"`
	PopulateTestData    bool `json:"populate_test_data"`
}

// ServerConfig configures the HTTP server
type ServerConfig struct {
	Port         int           `json:"port"`
	MetricsPort  int           `json:"metrics_port"`
	ReadTimeout  time.Duration `json:"read_timeout"`
	WriteTimeout time.Duration `json:"write_timeout"`
	IdleTimeout  time.Duration `json:"idle_timeout"`
	
	// HTTP optimizations
	EnableKeepAlive    bool `json:"enable_keep_alive"`
	MaxConnections     int  `json:"max_connections"`
	EnableCompression  bool `json:"enable_compression"`
	ReadBufferSize     int  `json:"read_buffer_size"`
	WriteBufferSize    int  `json:"write_buffer_size"`
}

// DefaultConfig returns the default configuration with performance optimizations
func DefaultConfig() *Config {
	return &Config{
		ALMConfig: ALMConfig{
			MaxMemories:         1000000,
			MaxAssociations:     5000000,
			DefaultAssocWeight:  0.5,
			WeightDecayRate:     0.01,
			MaxSearchDepth:      5,
			MaxSearchResults:    100,
			SearchTimeout:       15 * time.Millisecond, // Reduced from 20ms
			MinAssocThreshold:   0.1,
			MaxSearchWorkers:    20,  // Increased from 10
			MaxPathfindWorkers:  40,  // Increased from 20
			GCInterval:          5 * time.Minute,
			MaxIdleTime:         1 * time.Hour,
			
			// Performance optimizations enabled
			EnableObjectPooling:  true,
			EnableMemoryCache:    true,
			EnableQueryCache:     true,
			CacheSize:           10000,
			EnableParallelSearch: true,
			
			EnableAutoDiscovery: true,
			EnableWeightDecay:   true,
			PopulateTestData:    true,
		},
		ServerConfig: ServerConfig{
			Port:         8082,
			MetricsPort:  9092,
			ReadTimeout:  2 * time.Second,   // Reduced from 5s
			WriteTimeout: 5 * time.Second,   // Reduced from 10s
			IdleTimeout:  120 * time.Second, // Increased from 60s
			
			// HTTP optimizations enabled
			EnableKeepAlive:    true,
			MaxConnections:     1000,
			EnableCompression:  true,
			ReadBufferSize:     8192,  // 8KB
			WriteBufferSize:    8192,  // 8KB
		},
	}
}

// Validate checks if the configuration is valid
func (c *Config) Validate() error {
	if c.ALMConfig.MaxMemories <= 0 {
		return ErrInvalidMaxMemories
	}
	
	if c.ALMConfig.MaxAssociations <= 0 {
		return ErrInvalidMaxAssociations
	}
	
	if c.ALMConfig.MaxSearchDepth <= 0 || c.ALMConfig.MaxSearchDepth > 10 {
		return ErrInvalidSearchDepth
	}
	
	if c.ALMConfig.SearchTimeout <= 0 || c.ALMConfig.SearchTimeout > time.Second {
		return ErrInvalidSearchTimeout
	}
	
	if c.ServerConfig.Port <= 0 || c.ServerConfig.Port > 65535 {
		return ErrInvalidPort
	}
	
	return nil
}

// Configuration errors
var (
	ErrInvalidMaxMemories     = fmt.Errorf("max_memories must be positive")
	ErrInvalidMaxAssociations = fmt.Errorf("max_associations must be positive")  
	ErrInvalidSearchDepth     = fmt.Errorf("search_depth must be between 1 and 10")
	ErrInvalidSearchTimeout   = fmt.Errorf("search_timeout must be between 1ms and 1s")
	ErrInvalidPort           = fmt.Errorf("port must be between 1 and 65535")
)