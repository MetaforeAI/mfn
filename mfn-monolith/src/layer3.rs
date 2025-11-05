//! Layer 3: Associative Learning Matrix (ALM)
//!
//! Graph-based associative memory search using petgraph.
//! Traverses memory associations to find related content.
//! Phase 2 optimization: parking_lot::RwLock for faster locking.

use crate::types::{Memory, MemoryId, Query, SearchResult, Layer};
use anyhow::{Context, Result};
use parking_lot::RwLock;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use std::collections::{HashMap, VecDeque, BinaryHeap};
use std::cmp::Ordering;
use std::sync::Arc;

/// Association type for memory connections
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssociationType {
    Semantic,
    Temporal,
    Causal,
    Spatial,
    Conceptual,
    Hierarchical,
    Functional,
    Domain,
    Cognitive,
}

/// Search mode for graph traversal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    BreadthFirst,
    DepthFirst,
    BestFirst,
}

/// Priority queue item for best-first search
#[derive(Debug, Clone)]
struct SearchItem {
    memory_id: MemoryId,
    score: f64,
    depth: usize,
}

impl PartialEq for SearchItem {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
    }
}

impl Eq for SearchItem {}

impl PartialOrd for SearchItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SearchItem {
    fn cmp(&self, other: &Self) -> Ordering {
        // Max-heap: higher scores first
        self.score.partial_cmp(&other.score)
            .unwrap_or(Ordering::Equal)
    }
}

/// Graph-based associative memory index
#[derive(Clone)]
pub struct GraphIndex {
    /// Petgraph directed graph with Memory nodes and f64 edge weights
    graph: Arc<RwLock<Graph<Memory, f64>>>,

    /// Fast lookup from MemoryId to NodeIndex
    id_to_node: Arc<RwLock<HashMap<MemoryId, NodeIndex>>>,

    /// Maximum number of nodes allowed
    max_nodes: usize,

    /// Default search mode
    default_search_mode: SearchMode,
}

impl GraphIndex {
    /// Create a new graph index with maximum capacity
    pub fn new(max_nodes: usize) -> Result<Self> {
        if max_nodes == 0 {
            anyhow::bail!("max_nodes must be positive");
        }

        Ok(Self {
            graph: Arc::new(RwLock::new(Graph::new())),
            id_to_node: Arc::new(RwLock::new(HashMap::with_capacity(max_nodes))),
            max_nodes,
            default_search_mode: SearchMode::BreadthFirst,
        })
    }

    /// Add a memory node to the graph
    pub fn add_node(&self, memory: Memory) -> Result<NodeIndex> {
        let mut graph = self.graph.write();
        let mut id_map = self.id_to_node.write();

        // Check capacity
        if graph.node_count() >= self.max_nodes {
            anyhow::bail!("Maximum node capacity reached ({})", self.max_nodes);
        }

        // Check for duplicate
        if id_map.contains_key(&memory.id) {
            anyhow::bail!("Memory {} already exists in graph", memory.id);
        }

        // Add node to graph
        let node_idx = graph.add_node(memory.clone());

        // Update lookup map
        id_map.insert(memory.id, node_idx);

        Ok(node_idx)
    }

    /// Add an association edge between two memories
    pub fn add_edge(&self, from: MemoryId, to: MemoryId, weight: f64) -> Result<()> {
        if weight < 0.0 || weight > 1.0 {
            anyhow::bail!("Weight must be between 0.0 and 1.0, got {}", weight);
        }

        let id_map = self.id_to_node.read();

        // Get node indices
        let from_idx = *id_map.get(&from)
            .with_context(|| format!("Source memory {} not found", from))?;
        let to_idx = *id_map.get(&to)
            .with_context(|| format!("Target memory {} not found", to))?;

        drop(id_map);

        // Add edge
        let mut graph = self.graph.write();
        graph.add_edge(from_idx, to_idx, weight);

        Ok(())
    }

    /// Get current node count
    pub fn node_count(&self) -> usize {
        self.graph.read().node_count()
    }

    /// Get current edge count
    pub fn edge_count(&self) -> usize {
        self.graph.read().edge_count()
    }

    /// Traverse the graph starting from nodes most similar to the query
    pub fn traverse(&self, query: &Query, depth: usize) -> Vec<SearchResult> {
        self.traverse_with_mode(query, depth, self.default_search_mode)
    }

    /// Traverse with specific search mode
    pub fn traverse_with_mode(
        &self,
        query: &Query,
        max_depth: usize,
        mode: SearchMode,
    ) -> Vec<SearchResult> {
        if max_depth == 0 {
            return Vec::new();
        }

        // Find starting nodes (nodes most relevant to query)
        let starting_nodes = self.find_starting_nodes(query);
        if starting_nodes.is_empty() {
            return Vec::new();
        }

        // Perform traversal based on mode
        match mode {
            SearchMode::BreadthFirst => self.bfs_traverse(&starting_nodes, max_depth),
            SearchMode::DepthFirst => self.dfs_traverse(&starting_nodes, max_depth),
            SearchMode::BestFirst => self.best_first_traverse(&starting_nodes, max_depth),
        }
    }

    /// Find starting nodes for traversal (simple heuristic: use all nodes for now)
    fn find_starting_nodes(&self, _query: &Query) -> Vec<NodeIndex> {
        let id_map = self.id_to_node.read();

        // Simple strategy: return all nodes as potential starting points
        // In production, this would use query embeddings to find most similar nodes
        id_map.values().copied().collect()
    }

    /// Breadth-first search traversal
    fn bfs_traverse(&self, starting_nodes: &[NodeIndex], max_depth: usize) -> Vec<SearchResult> {
        let graph = self.graph.read();
        let mut results = Vec::new();
        let mut visited = HashMap::new();

        for &start_node in starting_nodes {
            let mut queue = VecDeque::new();
            queue.push_back((start_node, 1.0, 0)); // (node, score, depth)

            while let Some((node_idx, score, depth)) = queue.pop_front() {
                // Skip if already visited with better score
                if let Some(&prev_score) = visited.get(&node_idx) {
                    if prev_score >= score {
                        continue;
                    }
                }
                visited.insert(node_idx, score);

                // Get memory at this node
                if let Some(memory) = graph.node_weight(node_idx) {
                    results.push(SearchResult {
                        memory_id: memory.id,
                        score,
                        layer: Layer::L3Graph,
                        content: memory.content.clone(),
                    });
                }

                // Continue traversal if not at max depth
                if depth < max_depth {
                    // Get outgoing neighbors
                    let mut neighbors: Vec<_> = graph
                        .neighbors_directed(node_idx, Direction::Outgoing)
                        .collect();

                    for neighbor in neighbors.drain(..) {
                        // Get edge weight
                        if let Some(edge) = graph.find_edge(node_idx, neighbor) {
                            if let Some(&weight) = graph.edge_weight(edge) {
                                let new_score = score * weight;

                                // Only explore if score is meaningful
                                if new_score > 0.01 {
                                    queue.push_back((neighbor, new_score, depth + 1));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by score (descending)
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        results
    }

    /// Depth-first search traversal
    fn dfs_traverse(&self, starting_nodes: &[NodeIndex], max_depth: usize) -> Vec<SearchResult> {
        let graph = self.graph.read();
        let mut results = Vec::new();
        let mut visited = HashMap::new();

        for &start_node in starting_nodes {
            let mut stack = Vec::new();
            stack.push((start_node, 1.0, 0)); // (node, score, depth)

            while let Some((node_idx, score, depth)) = stack.pop() {
                // Skip if already visited with better score
                if let Some(&prev_score) = visited.get(&node_idx) {
                    if prev_score >= score {
                        continue;
                    }
                }
                visited.insert(node_idx, score);

                // Get memory at this node
                if let Some(memory) = graph.node_weight(node_idx) {
                    results.push(SearchResult {
                        memory_id: memory.id,
                        score,
                        layer: Layer::L3Graph,
                        content: memory.content.clone(),
                    });
                }

                // Continue traversal if not at max depth
                if depth < max_depth {
                    // Get outgoing neighbors
                    let mut neighbors: Vec<_> = graph
                        .neighbors_directed(node_idx, Direction::Outgoing)
                        .collect();

                    for neighbor in neighbors.drain(..) {
                        // Get edge weight
                        if let Some(edge) = graph.find_edge(node_idx, neighbor) {
                            if let Some(&weight) = graph.edge_weight(edge) {
                                let new_score = score * weight;

                                // Only explore if score is meaningful
                                if new_score > 0.01 {
                                    stack.push((neighbor, new_score, depth + 1));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort by score (descending)
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        results
    }

    /// Best-first search traversal (follows highest-weight edges)
    fn best_first_traverse(&self, starting_nodes: &[NodeIndex], max_depth: usize) -> Vec<SearchResult> {
        let graph = self.graph.read();
        let mut results = Vec::new();
        let mut visited = HashMap::new();
        let mut pq = BinaryHeap::new();

        // Initialize priority queue with starting nodes
        for &start_node in starting_nodes {
            if let Some(memory) = graph.node_weight(start_node) {
                pq.push(SearchItem {
                    memory_id: memory.id,
                    score: 1.0,
                    depth: 0,
                });
            }
        }

        let id_map = self.id_to_node.read();

        while let Some(item) = pq.pop() {
            // Get node index
            let node_idx = match id_map.get(&item.memory_id) {
                Some(&idx) => idx,
                None => continue,
            };

            // Skip if already visited with better score
            if let Some(&prev_score) = visited.get(&node_idx) {
                if prev_score >= item.score {
                    continue;
                }
            }
            visited.insert(node_idx, item.score);

            // Get memory at this node
            if let Some(memory) = graph.node_weight(node_idx) {
                results.push(SearchResult {
                    memory_id: memory.id,
                    score: item.score,
                    layer: Layer::L3Graph,
                    content: memory.content.clone(),
                });
            }

            // Continue traversal if not at max depth
            if item.depth < max_depth {
                // Get outgoing neighbors
                let neighbors: Vec<_> = graph
                    .neighbors_directed(node_idx, Direction::Outgoing)
                    .collect();

                for neighbor in neighbors {
                    // Get edge weight
                    if let Some(edge) = graph.find_edge(node_idx, neighbor) {
                        if let Some(&weight) = graph.edge_weight(edge) {
                            let new_score = item.score * weight;

                            // Only explore if score is meaningful
                            if new_score > 0.01 {
                                if let Some(neighbor_memory) = graph.node_weight(neighbor) {
                                    pq.push(SearchItem {
                                        memory_id: neighbor_memory.id,
                                        score: new_score,
                                        depth: item.depth + 1,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Results are already sorted by priority queue (highest score first)
        results
    }

    /// Get memory by ID
    pub fn get_memory(&self, id: &MemoryId) -> Option<Memory> {
        let id_map = self.id_to_node.read();
        let node_idx = *id_map.get(id)?;
        drop(id_map);

        let graph = self.graph.read();
        graph.node_weight(node_idx).cloned()
    }

    /// Get neighbors of a memory (outgoing edges)
    pub fn get_neighbors(&self, id: &MemoryId) -> Vec<(Memory, f64)> {
        let id_map = self.id_to_node.read();
        let node_idx = match id_map.get(id) {
            Some(&idx) => idx,
            None => return Vec::new(),
        };
        drop(id_map);

        let graph = self.graph.read();
        let mut neighbors = Vec::new();

        for edge in graph.edges_directed(node_idx, Direction::Outgoing) {
            let target = edge.target();
            if let Some(memory) = graph.node_weight(target) {
                let weight = *edge.weight();
                neighbors.push((memory.clone(), weight));
            }
        }

        neighbors
    }

    /// Apply weight decay to all edges (for temporal decay of associations)
    pub fn apply_weight_decay(&self, decay_rate: f64) {
        if decay_rate <= 0.0 || decay_rate >= 1.0 {
            return;
        }

        let mut graph = self.graph.write();
        let edge_indices: Vec<_> = graph.edge_indices().collect();

        for edge_idx in edge_indices {
            if let Some(weight) = graph.edge_weight_mut(edge_idx) {
                *weight *= 1.0 - decay_rate;
            }
        }
    }

    /// Remove edges below minimum weight threshold
    pub fn prune_weak_edges(&self, min_weight: f64) -> usize {
        let mut graph = self.graph.write();
        let edges_to_remove: Vec<_> = graph
            .edge_indices()
            .filter_map(|idx| {
                graph.edge_weight(idx).and_then(|&weight| {
                    if weight < min_weight {
                        Some(idx)
                    } else {
                        None
                    }
                })
            })
            .collect();

        let count = edges_to_remove.len();
        for edge_idx in edges_to_remove {
            graph.remove_edge(edge_idx);
        }

        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_memory(content: &str) -> Memory {
        Memory::new(
            content.to_string(),
            vec![0.1, 0.2, 0.3], // dummy embedding
        )
    }

    #[test]
    fn test_graph_index_creation() {
        let index = GraphIndex::new(1000).unwrap();
        assert_eq!(index.node_count(), 0);
        assert_eq!(index.edge_count(), 0);
    }

    #[test]
    fn test_add_node() {
        let index = GraphIndex::new(10).unwrap();
        let memory = create_test_memory("test content");

        let result = index.add_node(memory);
        assert!(result.is_ok());
        assert_eq!(index.node_count(), 1);
    }

    #[test]
    fn test_add_duplicate_node() {
        let index = GraphIndex::new(10).unwrap();
        let memory = create_test_memory("test content");

        index.add_node(memory.clone()).unwrap();
        let result = index.add_node(memory);

        assert!(result.is_err());
    }

    #[test]
    fn test_add_edge() {
        let index = GraphIndex::new(10).unwrap();
        let mem1 = create_test_memory("memory 1");
        let mem2 = create_test_memory("memory 2");

        index.add_node(mem1.clone()).unwrap();
        index.add_node(mem2.clone()).unwrap();

        let result = index.add_edge(mem1.id, mem2.id, 0.8);
        assert!(result.is_ok());
        assert_eq!(index.edge_count(), 1);
    }

    #[test]
    fn test_traverse_empty_graph() {
        let index = GraphIndex::new(10).unwrap();
        let query = Query::new("test query");

        let results = index.traverse(&query, 3);
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_bfs_traverse() {
        let index = GraphIndex::new(100).unwrap();

        // Create a simple chain: A -> B -> C
        let mem_a = create_test_memory("A");
        let mem_b = create_test_memory("B");
        let mem_c = create_test_memory("C");

        index.add_node(mem_a.clone()).unwrap();
        index.add_node(mem_b.clone()).unwrap();
        index.add_node(mem_c.clone()).unwrap();

        index.add_edge(mem_a.id, mem_b.id, 0.9).unwrap();
        index.add_edge(mem_b.id, mem_c.id, 0.8).unwrap();

        let query = Query::new("test");
        let results = index.traverse(&query, 3);

        // Should find at least 3 nodes (may have duplicates from different starting points)
        assert!(results.len() >= 3);
        assert_eq!(results[0].layer, Layer::L3Graph);
    }

    #[test]
    fn test_dfs_traverse() {
        let index = GraphIndex::new(100).unwrap();

        // Create a graph with branches
        let mem_a = create_test_memory("A");
        let mem_b = create_test_memory("B");
        let mem_c = create_test_memory("C");
        let mem_d = create_test_memory("D");

        index.add_node(mem_a.clone()).unwrap();
        index.add_node(mem_b.clone()).unwrap();
        index.add_node(mem_c.clone()).unwrap();
        index.add_node(mem_d.clone()).unwrap();

        // A -> B, A -> C, B -> D
        index.add_edge(mem_a.id, mem_b.id, 0.9).unwrap();
        index.add_edge(mem_a.id, mem_c.id, 0.7).unwrap();
        index.add_edge(mem_b.id, mem_d.id, 0.8).unwrap();

        let query = Query::new("test");
        let results = index.traverse_with_mode(&query, 3, SearchMode::DepthFirst);

        assert!(results.len() >= 3); // Should find multiple nodes
    }

    #[test]
    fn test_best_first_traverse() {
        let index = GraphIndex::new(100).unwrap();

        // Create graph with different weights
        let mem_a = create_test_memory("A");
        let mem_b = create_test_memory("B");
        let mem_c = create_test_memory("C");

        index.add_node(mem_a.clone()).unwrap();
        index.add_node(mem_b.clone()).unwrap();
        index.add_node(mem_c.clone()).unwrap();

        // Higher weight path should be explored first
        index.add_edge(mem_a.id, mem_b.id, 0.9).unwrap();
        index.add_edge(mem_a.id, mem_c.id, 0.3).unwrap();

        let query = Query::new("test");
        let results = index.traverse_with_mode(&query, 2, SearchMode::BestFirst);

        assert!(results.len() >= 2);
        // Best-first should prioritize higher-weight paths
        assert!(results[0].score >= results[1].score);
    }

    #[test]
    fn test_get_memory() {
        let index = GraphIndex::new(10).unwrap();
        let memory = create_test_memory("test content");

        index.add_node(memory.clone()).unwrap();

        let retrieved = index.get_memory(&memory.id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().content, "test content");
    }

    #[test]
    fn test_get_neighbors() {
        let index = GraphIndex::new(10).unwrap();

        let mem1 = create_test_memory("memory 1");
        let mem2 = create_test_memory("memory 2");
        let mem3 = create_test_memory("memory 3");

        index.add_node(mem1.clone()).unwrap();
        index.add_node(mem2.clone()).unwrap();
        index.add_node(mem3.clone()).unwrap();

        index.add_edge(mem1.id, mem2.id, 0.8).unwrap();
        index.add_edge(mem1.id, mem3.id, 0.6).unwrap();

        let neighbors = index.get_neighbors(&mem1.id);
        assert_eq!(neighbors.len(), 2);
    }

    #[test]
    fn test_weight_decay() {
        let index = GraphIndex::new(10).unwrap();

        let mem1 = create_test_memory("memory 1");
        let mem2 = create_test_memory("memory 2");

        index.add_node(mem1.clone()).unwrap();
        index.add_node(mem2.clone()).unwrap();
        index.add_edge(mem1.id, mem2.id, 1.0).unwrap();

        // Apply 10% decay
        index.apply_weight_decay(0.1);

        let neighbors = index.get_neighbors(&mem1.id);
        assert!((neighbors[0].1 - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_prune_weak_edges() {
        let index = GraphIndex::new(10).unwrap();

        let mem1 = create_test_memory("memory 1");
        let mem2 = create_test_memory("memory 2");
        let mem3 = create_test_memory("memory 3");

        index.add_node(mem1.clone()).unwrap();
        index.add_node(mem2.clone()).unwrap();
        index.add_node(mem3.clone()).unwrap();

        index.add_edge(mem1.id, mem2.id, 0.9).unwrap(); // Strong
        index.add_edge(mem1.id, mem3.id, 0.1).unwrap(); // Weak

        let removed = index.prune_weak_edges(0.5);
        assert_eq!(removed, 1);
        assert_eq!(index.edge_count(), 1);
    }

    #[test]
    fn test_max_capacity() {
        let index = GraphIndex::new(2).unwrap();

        let mem1 = create_test_memory("memory 1");
        let mem2 = create_test_memory("memory 2");
        let mem3 = create_test_memory("memory 3");

        assert!(index.add_node(mem1).is_ok());
        assert!(index.add_node(mem2).is_ok());
        assert!(index.add_node(mem3).is_err()); // Should fail
    }

    #[test]
    fn test_edge_weight_validation() {
        let index = GraphIndex::new(10).unwrap();

        let mem1 = create_test_memory("memory 1");
        let mem2 = create_test_memory("memory 2");

        index.add_node(mem1.clone()).unwrap();
        index.add_node(mem2.clone()).unwrap();

        // Invalid weights
        assert!(index.add_edge(mem1.id, mem2.id, -0.1).is_err());
        assert!(index.add_edge(mem1.id, mem2.id, 1.5).is_err());

        // Valid weight
        assert!(index.add_edge(mem1.id, mem2.id, 0.5).is_ok());
    }
}
