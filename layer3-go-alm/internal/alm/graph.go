package alm

import (
	"fmt"
	"sync"
	"time"
)

// MemoryGraph implements a concurrent graph structure for memory associations
type MemoryGraph struct {
	// Graph structure
	nodes     map[uint64]*Node    // Memory ID -> Node
	edges     map[string]*Edge    // Association ID -> Edge

	// Connection tracking
	connectionNodes  map[string]map[uint64]bool  // Connection ID -> Node IDs
	connectionEdges  map[string]map[string]bool  // Connection ID -> Edge IDs

	// Concurrency control
	mu        sync.RWMutex

	// Configuration
	maxMemories      int
	maxAssociations  int
	maxEdgesPerNode  int           // Maximum edges allowed per node
	edgeTTL          time.Duration // Time-to-live for edges

	// Statistics
	stats     *GraphStats
	statsMu   sync.RWMutex
}

// NewMemoryGraph creates a new memory graph
func NewMemoryGraph(maxMemories, maxAssociations int) (*MemoryGraph, error) {
	if maxMemories <= 0 {
		return nil, fmt.Errorf("maxMemories must be positive")
	}

	if maxAssociations <= 0 {
		return nil, fmt.Errorf("maxAssociations must be positive")
	}

	return &MemoryGraph{
		nodes:            make(map[uint64]*Node),
		edges:            make(map[string]*Edge),
		connectionNodes:  make(map[string]map[uint64]bool),
		connectionEdges:  make(map[string]map[string]bool),
		maxMemories:      maxMemories,
		maxAssociations:  maxAssociations,
		maxEdgesPerNode:  1000,               // Default to 1000 edges per node
		edgeTTL:          24 * time.Hour,     // Default to 24 hour TTL
		stats: &GraphStats{
			TotalMemories:     0,
			TotalAssociations: 0,
		},
	}, nil
}

// NewMemoryGraphWithConfig creates a new memory graph with full configuration
func NewMemoryGraphWithConfig(maxMemories, maxAssociations, maxEdgesPerNode int, edgeTTL time.Duration) (*MemoryGraph, error) {
	if maxMemories <= 0 {
		return nil, fmt.Errorf("maxMemories must be positive")
	}

	if maxAssociations <= 0 {
		return nil, fmt.Errorf("maxAssociations must be positive")
	}

	if maxEdgesPerNode <= 0 {
		maxEdgesPerNode = 1000 // Default
	}

	if edgeTTL <= 0 {
		edgeTTL = 24 * time.Hour // Default
	}

	return &MemoryGraph{
		nodes:            make(map[uint64]*Node),
		edges:            make(map[string]*Edge),
		connectionNodes:  make(map[string]map[uint64]bool),
		connectionEdges:  make(map[string]map[string]bool),
		maxMemories:      maxMemories,
		maxAssociations:  maxAssociations,
		maxEdgesPerNode:  maxEdgesPerNode,
		edgeTTL:          edgeTTL,
		stats: &GraphStats{
			TotalMemories:     0,
			TotalAssociations: 0,
		},
	}, nil
}

// AddMemory adds a memory node to the graph
func (g *MemoryGraph) AddMemory(memory *Memory) error {
	g.mu.Lock()
	defer g.mu.Unlock()
	
	if len(g.nodes) >= g.maxMemories {
		return fmt.Errorf("maximum memory capacity reached (%d)", g.maxMemories)
	}
	
	if _, exists := g.nodes[memory.ID]; exists {
		return fmt.Errorf("memory %d already exists", memory.ID)
	}
	
	node := &Node{
		Memory:   memory,
		OutEdges: make(map[uint64]*Edge),
		InEdges:  make(map[uint64]*Edge),
	}
	
	g.nodes[memory.ID] = node
	
	// Update statistics
	g.updateStats()
	
	return nil
}

// AddAssociation adds an association edge to the graph
func (g *MemoryGraph) AddAssociation(assoc *Association) error {
	g.mu.Lock()
	defer g.mu.Unlock()

	if len(g.edges) >= g.maxAssociations {
		return fmt.Errorf("maximum association capacity reached (%d)", g.maxAssociations)
	}

	// Check that both memories exist
	fromNode, fromExists := g.nodes[assoc.FromMemoryID]
	toNode, toExists := g.nodes[assoc.ToMemoryID]

	if !fromExists {
		return fmt.Errorf("source memory %d not found", assoc.FromMemoryID)
	}

	if !toExists {
		return fmt.Errorf("target memory %d not found", assoc.ToMemoryID)
	}

	// Check for duplicate association
	if _, exists := g.edges[assoc.ID]; exists {
		return fmt.Errorf("association %s already exists", assoc.ID)
	}

	// Check edge limit per node and evict if necessary
	if len(fromNode.OutEdges) >= g.maxEdgesPerNode {
		// Evict weakest/oldest edge
		g.evictWeakestEdge(fromNode)
	}

	edge := &Edge{
		To:     assoc.ToMemoryID,
		Weight: assoc.Weight,
		Assoc:  assoc,
	}

	// Add edge to graph structures
	g.edges[assoc.ID] = edge
	fromNode.OutEdges[assoc.ToMemoryID] = edge
	toNode.InEdges[assoc.FromMemoryID] = edge

	// Track connection ownership if specified
	if assoc.ConnectionID != "" {
		if g.connectionEdges[assoc.ConnectionID] == nil {
			g.connectionEdges[assoc.ConnectionID] = make(map[string]bool)
		}
		g.connectionEdges[assoc.ConnectionID][assoc.ID] = true

		// Also track nodes for this connection
		if g.connectionNodes[assoc.ConnectionID] == nil {
			g.connectionNodes[assoc.ConnectionID] = make(map[uint64]bool)
		}
		g.connectionNodes[assoc.ConnectionID][assoc.FromMemoryID] = true
		g.connectionNodes[assoc.ConnectionID][assoc.ToMemoryID] = true
	}

	// Update statistics
	g.updateStats()

	return nil
}

// evictWeakestEdge removes the weakest or oldest edge from a node (must hold write lock)
func (g *MemoryGraph) evictWeakestEdge(node *Node) {
	if len(node.OutEdges) == 0 {
		return
	}

	var weakestEdge *Edge
	var weakestID uint64
	weakestWeight := 2.0 // Start with max possible weight

	// Find weakest edge based on weight and age
	for toID, edge := range node.OutEdges {
		// Calculate composite score (lower is weaker)
		age := time.Since(edge.Assoc.LastUsed).Hours()
		score := edge.Weight - (age / 24.0) * 0.1 // Penalize older edges

		if score < weakestWeight {
			weakestWeight = score
			weakestEdge = edge
			weakestID = toID
		}
	}

	if weakestEdge != nil {
		// Remove the weakest edge
		delete(node.OutEdges, weakestID)
		delete(g.edges, weakestEdge.Assoc.ID)

		// Remove from target node's incoming edges
		if targetNode, exists := g.nodes[weakestID]; exists {
			delete(targetNode.InEdges, node.Memory.ID)
		}

		// Remove from connection tracking
		if weakestEdge.Assoc.ConnectionID != "" {
			if edges, exists := g.connectionEdges[weakestEdge.Assoc.ConnectionID]; exists {
				delete(edges, weakestEdge.Assoc.ID)
			}
		}
	}
}

// GetMemory retrieves a memory by ID
func (g *MemoryGraph) GetMemory(id uint64) *Memory {
	g.mu.RLock()
	defer g.mu.RUnlock()
	
	if node, exists := g.nodes[id]; exists {
		return node.Memory
	}
	
	return nil
}

// GetNode retrieves a node by memory ID
func (g *MemoryGraph) GetNode(id uint64) *Node {
	g.mu.RLock()
	defer g.mu.RUnlock()
	
	return g.nodes[id]
}

// GetAllMemories returns all memories in the graph
func (g *MemoryGraph) GetAllMemories() []*Memory {
	g.mu.RLock()
	defer g.mu.RUnlock()
	
	memories := make([]*Memory, 0, len(g.nodes))
	for _, node := range g.nodes {
		memories = append(memories, node.Memory)
	}
	
	return memories
}

// GetNeighbors returns all neighboring memories for a given memory ID
func (g *MemoryGraph) GetNeighbors(memoryID uint64) ([]*Memory, []*Association) {
	g.mu.RLock()
	defer g.mu.RUnlock()
	
	node, exists := g.nodes[memoryID]
	if !exists {
		return nil, nil
	}
	
	neighbors := make([]*Memory, 0, len(node.OutEdges))
	associations := make([]*Association, 0, len(node.OutEdges))
	
	for neighborID, edge := range node.OutEdges {
		if neighborNode, exists := g.nodes[neighborID]; exists {
			neighbors = append(neighbors, neighborNode.Memory)
			associations = append(associations, edge.Assoc)
		}
	}
	
	return neighbors, associations
}

// GetIncomingEdges returns all incoming edges for a memory
func (g *MemoryGraph) GetIncomingEdges(memoryID uint64) []*Edge {
	g.mu.RLock()
	defer g.mu.RUnlock()
	
	node, exists := g.nodes[memoryID]
	if !exists {
		return nil
	}
	
	edges := make([]*Edge, 0, len(node.InEdges))
	for _, edge := range node.InEdges {
		edges = append(edges, edge)
	}
	
	return edges
}

// GetOutgoingEdges returns all outgoing edges for a memory
func (g *MemoryGraph) GetOutgoingEdges(memoryID uint64) []*Edge {
	g.mu.RLock()
	defer g.mu.RUnlock()
	
	node, exists := g.nodes[memoryID]
	if !exists {
		return nil
	}
	
	edges := make([]*Edge, 0, len(node.OutEdges))
	for _, edge := range node.OutEdges {
		edges = append(edges, edge)
	}
	
	return edges
}

// RemoveUnusedMemories removes memories that haven't been accessed recently
func (g *MemoryGraph) RemoveUnusedMemories(cutoffTime time.Time) int {
	g.mu.Lock()
	defer g.mu.Unlock()
	
	removed := 0
	toRemove := make([]uint64, 0)
	
	// Find memories to remove
	for id, node := range g.nodes {
		if node.Memory.LastAccessed.Before(cutoffTime) && node.Memory.AccessCount == 0 {
			toRemove = append(toRemove, id)
		}
	}
	
	// Remove memories and their associations
	for _, memoryID := range toRemove {
		if g.removeMemoryUnsafe(memoryID) {
			removed++
		}
	}
	
	g.updateStats()
	return removed
}

// RemoveWeakAssociations removes associations below threshold weight
func (g *MemoryGraph) RemoveWeakAssociations(minWeight float64) int {
	g.mu.Lock()
	defer g.mu.Unlock()
	
	removed := 0
	toRemove := make([]string, 0)
	
	// Find associations to remove
	for id, edge := range g.edges {
		if edge.Weight < minWeight {
			toRemove = append(toRemove, id)
		}
	}
	
	// Remove weak associations
	for _, assocID := range toRemove {
		if g.removeAssociationUnsafe(assocID) {
			removed++
		}
	}
	
	g.updateStats()
	return removed
}

// ApplyWeightDecay applies exponential decay to all association weights
func (g *MemoryGraph) ApplyWeightDecay(decayRate float64) {
	g.mu.Lock()
	defer g.mu.Unlock()
	
	for _, edge := range g.edges {
		edge.Weight *= (1.0 - decayRate)
		edge.Assoc.Weight = edge.Weight
	}
}

// GetStats returns current graph statistics
func (g *MemoryGraph) GetStats() *GraphStats {
	g.statsMu.RLock()
	defer g.statsMu.RUnlock()
	
	// Return a copy to avoid race conditions
	stats := *g.stats
	return &stats
}

// removeMemoryUnsafe removes a memory and all its associations (must hold write lock)
func (g *MemoryGraph) removeMemoryUnsafe(memoryID uint64) bool {
	node, exists := g.nodes[memoryID]
	if !exists {
		return false
	}
	
	// Remove all outgoing associations
	for _, edge := range node.OutEdges {
		delete(g.edges, edge.Assoc.ID)
		// Remove from target node's incoming edges
		if targetNode, exists := g.nodes[edge.To]; exists {
			delete(targetNode.InEdges, memoryID)
		}
	}
	
	// Remove all incoming associations
	for fromID, edge := range node.InEdges {
		delete(g.edges, edge.Assoc.ID)
		// Remove from source node's outgoing edges
		if sourceNode, exists := g.nodes[fromID]; exists {
			delete(sourceNode.OutEdges, memoryID)
		}
	}
	
	// Remove the node itself
	delete(g.nodes, memoryID)
	
	return true
}

// removeAssociationUnsafe removes an association (must hold write lock)
func (g *MemoryGraph) removeAssociationUnsafe(assocID string) bool {
	edge, exists := g.edges[assocID]
	if !exists {
		return false
	}
	
	assoc := edge.Assoc
	
	// Remove from nodes
	if fromNode, exists := g.nodes[assoc.FromMemoryID]; exists {
		delete(fromNode.OutEdges, assoc.ToMemoryID)
	}
	
	if toNode, exists := g.nodes[assoc.ToMemoryID]; exists {
		delete(toNode.InEdges, assoc.FromMemoryID)
	}
	
	// Remove from edges map
	delete(g.edges, assocID)
	
	return true
}

// updateStats recalculates graph statistics (must hold write lock)
func (g *MemoryGraph) updateStats() {
	g.statsMu.Lock()
	defer g.statsMu.Unlock()
	
	g.stats.TotalMemories = len(g.nodes)
	g.stats.TotalAssociations = len(g.edges)
	
	if g.stats.TotalMemories == 0 {
		g.stats.AverageConnections = 0
		g.stats.MaxConnections = 0
		g.stats.GraphDensity = 0
		return
	}
	
	// Calculate connection statistics
	totalConnections := 0
	maxConnections := 0
	
	for _, node := range g.nodes {
		connections := len(node.OutEdges) + len(node.InEdges)
		totalConnections += connections
		
		if connections > maxConnections {
			maxConnections = connections
		}
	}
	
	g.stats.AverageConnections = float64(totalConnections) / float64(g.stats.TotalMemories)
	g.stats.MaxConnections = maxConnections
	
	// Calculate graph density
	maxPossibleEdges := g.stats.TotalMemories * (g.stats.TotalMemories - 1)
	if maxPossibleEdges > 0 {
		g.stats.GraphDensity = float64(g.stats.TotalAssociations) / float64(maxPossibleEdges)
	}
	
	// TODO: Calculate strongly connected components and largest component
	// This would require more complex graph algorithms
	g.stats.StronglyConnected = 1 // Placeholder
	g.stats.LargestComponent = g.stats.TotalMemories // Placeholder
}

// EvictExpiredEdges removes edges that have exceeded their TTL
func (g *MemoryGraph) EvictExpiredEdges() int {
	g.mu.Lock()
	defer g.mu.Unlock()

	evicted := 0
	cutoffTime := time.Now().Add(-g.edgeTTL)
	toRemove := make([]string, 0)

	// Find expired edges
	for id, edge := range g.edges {
		if edge.Assoc.LastUsed.Before(cutoffTime) {
			toRemove = append(toRemove, id)
		}
	}

	// Remove expired edges
	for _, assocID := range toRemove {
		if g.removeAssociationUnsafe(assocID) {
			evicted++
		}
	}

	// Remove orphaned nodes (nodes with no edges)
	orphanedNodes := make([]uint64, 0)
	for id, node := range g.nodes {
		if len(node.OutEdges) == 0 && len(node.InEdges) == 0 {
			orphanedNodes = append(orphanedNodes, id)
		}
	}

	for _, nodeID := range orphanedNodes {
		delete(g.nodes, nodeID)
	}

	g.updateStats()
	return evicted
}

// CloseConnection removes all graph data associated with a connection
func (g *MemoryGraph) CloseConnection(connectionID string) (int, int) {
	g.mu.Lock()
	defer g.mu.Unlock()

	nodesRemoved := 0
	edgesRemoved := 0

	// Remove all edges associated with this connection
	if edges, exists := g.connectionEdges[connectionID]; exists {
		for edgeID := range edges {
			if g.removeAssociationUnsafe(edgeID) {
				edgesRemoved++
			}
		}
		delete(g.connectionEdges, connectionID)
	}

	// Remove orphaned nodes that were only associated with this connection
	if nodes, exists := g.connectionNodes[connectionID]; exists {
		for nodeID := range nodes {
			// Check if node is orphaned (no edges)
			if node, exists := g.nodes[nodeID]; exists {
				if len(node.OutEdges) == 0 && len(node.InEdges) == 0 {
					delete(g.nodes, nodeID)
					nodesRemoved++
				}
			}
		}
		delete(g.connectionNodes, connectionID)
	}

	g.updateStats()
	return nodesRemoved, edgesRemoved
}

// GetConnectionStats returns statistics for a specific connection
func (g *MemoryGraph) GetConnectionStats(connectionID string) (int, int) {
	g.mu.RLock()
	defer g.mu.RUnlock()

	nodeCount := 0
	edgeCount := 0

	if nodes, exists := g.connectionNodes[connectionID]; exists {
		nodeCount = len(nodes)
	}

	if edges, exists := g.connectionEdges[connectionID]; exists {
		edgeCount = len(edges)
	}

	return nodeCount, edgeCount
}

// GetConnectedComponents finds all connected components in the graph
func (g *MemoryGraph) GetConnectedComponents() [][]uint64 {
	g.mu.RLock()
	defer g.mu.RUnlock()

	visited := make(map[uint64]bool)
	components := make([][]uint64, 0)

	for nodeID := range g.nodes {
		if !visited[nodeID] {
			component := g.dfsComponent(nodeID, visited)
			if len(component) > 0 {
				components = append(components, component)
			}
		}
	}

	return components
}

// dfsComponent performs DFS to find connected component starting from nodeID
func (g *MemoryGraph) dfsComponent(startID uint64, visited map[uint64]bool) []uint64 {
	component := make([]uint64, 0)
	stack := []uint64{startID}
	
	for len(stack) > 0 {
		nodeID := stack[len(stack)-1]
		stack = stack[:len(stack)-1]
		
		if visited[nodeID] {
			continue
		}
		
		visited[nodeID] = true
		component = append(component, nodeID)
		
		// Add neighbors to stack (both incoming and outgoing)
		if node, exists := g.nodes[nodeID]; exists {
			for neighborID := range node.OutEdges {
				if !visited[neighborID] {
					stack = append(stack, neighborID)
				}
			}
			for neighborID := range node.InEdges {
				if !visited[neighborID] {
					stack = append(stack, neighborID)
				}
			}
		}
	}
	
	return component
}