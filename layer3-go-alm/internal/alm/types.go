package alm

import (
	"sync"
	"time"
)

// Memory represents a stored memory with metadata
type Memory struct {
	ID          uint64            `json:"id"`
	Content     string            `json:"content"`
	Tags        []string          `json:"tags,omitempty"`
	Metadata    map[string]string `json:"metadata,omitempty"`
	CreatedAt   time.Time         `json:"created_at"`
	LastAccessed time.Time        `json:"last_accessed"`
	AccessCount  int64             `json:"access_count"`
}

// Association represents a weighted link between memories
type Association struct {
	ID           string    `json:"id"`
	FromMemoryID uint64    `json:"from_memory_id"`
	ToMemoryID   uint64    `json:"to_memory_id"`
	Type         string    `json:"type"`         // semantic, temporal, causal, etc.
	Weight       float64   `json:"weight"`       // 0.0 to 1.0
	Reason       string    `json:"reason"`       // explanation for association
	CreatedAt    time.Time `json:"created_at"`
	LastUsed     time.Time `json:"last_used"`
	UsageCount   int64     `json:"usage_count"`
	ConnectionID string    `json:"connection_id,omitempty"` // Track which connection owns this
}

// AssociationType defines different types of memory associations
type AssociationType string

const (
	AssociationSemantic    AssociationType = "semantic"    // Related meaning
	AssociationTemporal    AssociationType = "temporal"    // Time-based connection
	AssociationCausal      AssociationType = "causal"      // Cause and effect
	AssociationSpatial     AssociationType = "spatial"     // Location-based
	AssociationConceptual  AssociationType = "conceptual"  // Abstract concepts
	AssociationHierarchical AssociationType = "hierarchical" // Parent-child
	AssociationFunctional  AssociationType = "functional"  // Function-based
	AssociationDomain      AssociationType = "domain"      // Same domain/field
	AssociationCognitive   AssociationType = "cognitive"   // Mental association
)

// SearchQuery represents a search request for associative memories
type SearchQuery struct {
	StartMemoryIDs []uint64          `json:"start_memory_ids"`
	QueryText      string            `json:"query_text,omitempty"`
	Tags           []string          `json:"tags,omitempty"`
	AssocTypes     []AssociationType `json:"association_types,omitempty"`
	MaxDepth       int               `json:"max_depth"`
	MaxResults     int               `json:"max_results"`
	MinWeight      float64           `json:"min_weight"`
	Timeout        time.Duration     `json:"timeout"`
	SearchMode     SearchMode        `json:"search_mode"`
}

// SearchMode defines how the search should be conducted
type SearchMode string

const (
	SearchModeDepthFirst   SearchMode = "depth_first"   // Explore deeply first
	SearchModeBreadthFirst SearchMode = "breadth_first" // Explore widely first  
	SearchModeBestFirst    SearchMode = "best_first"    // Follow highest weights
	SearchModeRandom       SearchMode = "random"        // Random exploration
)

// SearchResult represents the result of an associative search
type SearchResult struct {
	Memory       *Memory       `json:"memory"`
	Path         []*PathStep   `json:"path"`
	TotalWeight  float64       `json:"total_weight"`
	Depth        int           `json:"depth"`
	SearchTime   time.Duration `json:"search_time"`
}

// PathStep represents one step in the associative path
type PathStep struct {
	FromMemoryID uint64       `json:"from_memory_id"`
	ToMemoryID   uint64       `json:"to_memory_id"`
	Association  *Association `json:"association"`
	StepWeight   float64      `json:"step_weight"`
}

// SearchResults contains multiple search results with metadata
type SearchResults struct {
	Results       []*SearchResult `json:"results"`
	Query         *SearchQuery    `json:"query"`
	TotalFound    int             `json:"total_found"`
	SearchTime    time.Duration   `json:"search_time"`
	NodesExplored int             `json:"nodes_explored"`
	PathsFound    int             `json:"paths_found"`
}

// GraphStats provides statistics about the memory graph
type GraphStats struct {
	TotalMemories     int     `json:"total_memories"`
	TotalAssociations int     `json:"total_associations"`
	AverageConnections float64 `json:"average_connections"`
	MaxConnections    int     `json:"max_connections"`
	GraphDensity      float64 `json:"graph_density"`
	StronglyConnected int     `json:"strongly_connected_components"`
	LargestComponent  int     `json:"largest_component_size"`
}

// PerformanceMetrics tracks ALM performance
type PerformanceMetrics struct {
	// Search performance
	TotalSearches       int64         `json:"total_searches"`
	AverageSearchTime   time.Duration `json:"average_search_time"`
	FastestSearch       time.Duration `json:"fastest_search"`
	SlowestSearch       time.Duration `json:"slowest_search"`
	
	// Memory operations
	MemoriesAdded       int64 `json:"memories_added"`
	AssociationsAdded   int64 `json:"associations_added"`
	MemoriesAccessed    int64 `json:"memories_accessed"`
	
	// Resource usage
	MemoryUsageBytes    int64   `json:"memory_usage_bytes"`
	CPUUsagePercent     float64 `json:"cpu_usage_percent"`
	GoroutinesActive    int     `json:"goroutines_active"`
	
	// Cache performance
	CacheHitRate        float64 `json:"cache_hit_rate"`
	CacheMissRate       float64 `json:"cache_miss_rate"`
}

// Edge represents a directed edge in the memory graph
type Edge struct {
	To     uint64  `json:"to"`
	Weight float64 `json:"weight"`
	Assoc  *Association `json:"association"`
}

// Node represents a node in the memory graph  
type Node struct {
	Memory   *Memory          `json:"memory"`
	OutEdges map[uint64]*Edge `json:"out_edges"`
	InEdges  map[uint64]*Edge `json:"in_edges"`
}

// Priority queue item for search algorithms
type SearchItem struct {
	MemoryID    uint64
	Path        []*PathStep
	TotalWeight float64
	Depth       int
	Priority    float64
}

// SearchContext holds state during associative search
type SearchContext struct {
	Query         *SearchQuery
	Visited       map[uint64]bool // Not used in concurrent search
	Results       []*SearchResult
	StartTime     time.Time
	NodesExplored int
	mu            sync.Mutex // Protects NodesExplored counter
}

// WeightCalculationFunc defines how to calculate association weights
type WeightCalculationFunc func(assoc *Association, context *SearchContext) float64

// AssociationFilter filters associations during search
type AssociationFilter func(assoc *Association, from, to *Memory) bool