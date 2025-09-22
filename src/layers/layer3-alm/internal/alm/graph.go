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
	
	// Concurrency control
	mu        sync.RWMutex
	
	// Configuration
	maxMemories     int
	maxAssociations int
	
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
		nodes:           make(map[uint64]*Node),
		edges:           make(map[string]*Edge),
		maxMemories:     maxMemories,
		maxAssociations: maxAssociations,
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
	
	edge := &Edge{
		To:     assoc.ToMemoryID,
		Weight: assoc.Weight,
		Assoc:  assoc,
	}
	
	// Add edge to graph structures
	g.edges[assoc.ID] = edge
	fromNode.OutEdges[assoc.ToMemoryID] = edge
	toNode.InEdges[assoc.FromMemoryID] = edge
	
	// Update statistics
	g.updateStats()
	
	return nil
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