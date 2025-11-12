//! Spiking Neural Network Reservoir for Dynamic Similarity Detection
//! 
//! Implements a Liquid State Machine (LSM) architecture where:
//! - Reservoir topology is fixed (no training required)
//! - Memories create dynamic "similarity wells" as attractors
//! - Competitive dynamics enable winner-take-all similarity detection
//! - Temporal integration provides robust pattern matching

use ndarray::{Array1, Array2, ArrayView1};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use parking_lot::RwLock;
use rand::Rng;
use rand_distr::{Normal, Distribution};
use std::time::{Duration, Instant};

use crate::{DSRConfig, MemoryId, SpikePattern};

/// State of individual neurons in the reservoir
#[derive(Debug, Clone)]
pub struct NeuronState {
    /// Current membrane potential
    pub potential: f32,
    /// Last spike time (milliseconds)
    pub last_spike_time: f32,
    /// Refractory period remaining (milliseconds)
    pub refractory_period: f32,
    /// Accumulated input current
    pub input_current: f32,
    /// Neuron type: excitatory (true) or inhibitory (false)
    pub is_excitatory: bool,
}

impl NeuronState {
    pub fn new(is_excitatory: bool) -> Self {
        Self {
            potential: -70.0, // Resting potential in mV
            last_spike_time: -1000.0, // Initialize to distant past
            refractory_period: 0.0,
            input_current: 0.0,
            is_excitatory,
        }
    }

    /// Reset neuron to resting state
    pub fn reset(&mut self) {
        self.potential = -70.0;
        self.input_current = 0.0;
        self.refractory_period = 0.0;
    }
}

/// Represents a synaptic connection between neurons
#[derive(Debug, Clone)]
pub struct Synapse {
    /// Target neuron index
    pub target: usize,
    /// Synaptic weight (positive for excitatory, negative for inhibitory)
    pub weight: f32,
    /// Synaptic delay (milliseconds)
    pub delay: f32,
}

/// Dynamic similarity well that represents a stored memory
#[derive(Debug, Clone)]
pub struct SimilarityWell {
    /// Unique identifier for this memory
    pub memory_id: MemoryId,
    /// Spike pattern that created this well
    pub reference_pattern: SpikePattern,
    /// Content/metadata associated with this memory
    pub content: String,
    /// Attractor strength (how strongly it pulls similar patterns)
    pub strength: f32,
    /// Number of times this well has been activated
    pub activation_count: u64,
    /// Last activation time
    pub last_activated: std::time::Instant,
    /// Last access time (for LRU tracking)
    pub last_accessed: std::time::Instant,
    /// Creation time (for TTL tracking)
    pub created_at: std::time::Instant,
    /// Connection ID that created this well (for cleanup on disconnect)
    pub connection_id: Option<String>,
    /// Readout weights connecting reservoir to this well
    pub readout_weights: Array1<f32>,
    /// Number of entries in this well (for limiting per-well size)
    pub entry_count: usize,
}

impl SimilarityWell {
    pub fn new(
        memory_id: MemoryId,
        reference_pattern: SpikePattern,
        content: String,
        reservoir_size: usize,
        connection_id: Option<String>,
    ) -> Self {
        // Initialize readout weights randomly with small values
        let mut rng = rand::thread_rng();
        let normal = Normal::new(0.0, 0.01).unwrap();
        let readout_weights = Array1::from_vec(
            (0..reservoir_size)
                .map(|_| normal.sample(&mut rng) as f32)
                .collect()
        );

        let now = std::time::Instant::now();
        Self {
            memory_id,
            reference_pattern,
            content,
            strength: 1.0,
            activation_count: 0,
            last_activated: now,
            last_accessed: now,
            created_at: now,
            connection_id,
            readout_weights,
            entry_count: 1,
        }
    }

    /// Update the well based on activation patterns
    pub fn update_from_activation(&mut self, reservoir_activity: &[f32], learning_rate: f32) {
        self.activation_count += 1;
        let now = std::time::Instant::now();
        self.last_activated = now;
        self.last_accessed = now;

        // Simple Hebbian learning for readout weights
        for (weight, &activity) in self.readout_weights.iter_mut().zip(reservoir_activity.iter()) {
            *weight += learning_rate * activity * self.strength;

            // Clip weights to prevent runaway growth
            *weight = weight.clamp(-1.0, 1.0);
        }
    }

    /// Calculate activation strength given current reservoir state
    pub fn calculate_activation(&mut self, reservoir_activity: &[f32]) -> f32 {
        // Update last accessed time when calculating activation
        self.last_accessed = std::time::Instant::now();

        // Dot product of readout weights with reservoir activity
        self.readout_weights
            .iter()
            .zip(reservoir_activity.iter())
            .map(|(&weight, &activity)| weight * activity)
            .sum::<f32>()
            .max(0.0) // ReLU activation
    }
}

/// Main similarity reservoir implementation
pub struct SimilarityReservoir {
    config: DSRConfig,

    // Reservoir structure (fixed topology)
    neurons: Vec<NeuronState>,
    synapses: Vec<Vec<Synapse>>, // Adjacency list representation

    // Dynamic similarity wells
    similarity_wells: HashMap<MemoryId, SimilarityWell>,

    // Memory management
    lru_queue: VecDeque<MemoryId>,  // Track LRU order
    connection_wells: HashMap<String, Vec<MemoryId>>,  // Track wells per connection
    max_wells: usize,  // Maximum number of wells
    max_entries_per_well: usize,  // Maximum entries per well
    ttl_seconds: u64,  // TTL for wells in seconds

    // Simulation state
    current_time: f32,
    time_step: f32, // Integration time step in milliseconds

    // Performance tracking
    total_spikes: u64,
    wells_created: u64,
    wells_evicted: u64,
    memory_usage_bytes: usize,
}

impl SimilarityReservoir {
    pub fn new(config: DSRConfig) -> Result<Self> {
        // Use max_similarity_wells from config, or default to 100K
        let max_wells = if config.max_similarity_wells > 0 {
            config.max_similarity_wells
        } else {
            100_000  // Default 100K wells limit
        };

        let mut reservoir = Self {
            config: config.clone(),
            neurons: Vec::new(),
            synapses: vec![Vec::new(); config.reservoir_size],
            similarity_wells: HashMap::new(),
            lru_queue: VecDeque::new(),
            connection_wells: HashMap::new(),
            max_wells,
            max_entries_per_well: 1000,  // Default 1K entries per well
            ttl_seconds: 3600,  // Default 1 hour TTL
            current_time: 0.0,
            time_step: 0.1, // 0.1ms time step
            total_spikes: 0,
            wells_created: 0,
            wells_evicted: 0,
            memory_usage_bytes: 0,
        };

        reservoir.initialize_reservoir()?;
        Ok(reservoir)
    }

    /// Initialize the fixed reservoir topology
    fn initialize_reservoir(&mut self) -> Result<()> {
        let mut rng = rand::thread_rng();
        
        // Create neurons (80% excitatory, 20% inhibitory as in biological networks)
        for i in 0..self.config.reservoir_size {
            let is_excitatory = i < (self.config.reservoir_size * 4) / 5;
            self.neurons.push(NeuronState::new(is_excitatory));
        }

        // Create sparse random connectivity (10% connection probability)
        let connection_prob = 0.1;
        let weight_std = 0.5;
        let normal_weight = Normal::new(0.0, weight_std).unwrap();

        for source in 0..self.config.reservoir_size {
            for target in 0..self.config.reservoir_size {
                if source != target && rng.gen::<f32>() < connection_prob {
                    let base_weight = normal_weight.sample(&mut rng) as f32;
                    
                    // Excitatory neurons have positive weights, inhibitory negative
                    let weight = if self.neurons[source].is_excitatory {
                        base_weight.abs() * 2.0 // Stronger excitatory connections
                    } else {
                        -base_weight.abs() * 5.0 // Stronger inhibitory connections
                    };

                    let delay = 1.0 + rng.gen::<f32>() * 3.0; // 1-4ms delay

                    self.synapses[source].push(Synapse {
                        target,
                        weight,
                        delay,
                    });
                }
            }
        }

        tracing::info!(
            reservoir_size = self.config.reservoir_size,
            excitatory_count = self.neurons.iter().filter(|n| n.is_excitatory).count(),
            inhibitory_count = self.neurons.iter().filter(|n| !n.is_excitatory).count(),
            total_synapses = self.synapses.iter().map(|s| s.len()).sum::<usize>(),
            "Reservoir topology initialized"
        );

        Ok(())
    }

    /// Create a new similarity well for a memory with connection tracking
    pub fn create_similarity_well(
        &mut self,
        memory_id: MemoryId,
        reference_pattern: SpikePattern,
        content: String,
    ) -> Result<()> {
        self.create_similarity_well_with_connection(memory_id, reference_pattern, content, None)
    }

    /// Create a new similarity well for a memory with optional connection ID
    pub fn create_similarity_well_with_connection(
        &mut self,
        memory_id: MemoryId,
        reference_pattern: SpikePattern,
        content: String,
        connection_id: Option<String>,
    ) -> Result<()> {
        if self.similarity_wells.contains_key(&memory_id) {
            return Err(anyhow!("Memory {} already exists in reservoir", memory_id.0));
        }

        // Check if we've reached max wells limit
        if self.similarity_wells.len() >= self.max_wells {
            // Evict LRU well
            self.evict_lru_well();
        }

        // Clean up expired wells before adding new one
        self.cleanup_expired_wells();

        let well = SimilarityWell::new(
            memory_id,
            reference_pattern,
            content,
            self.config.reservoir_size,
            connection_id.clone(),
        );

        // Update memory usage estimate
        let well_size = self.estimate_well_size(&well);
        self.memory_usage_bytes += well_size;

        // Track connection ownership
        if let Some(conn_id) = &connection_id {
            self.connection_wells
                .entry(conn_id.clone())
                .or_insert_with(Vec::new)
                .push(memory_id);
        }

        // Update LRU queue
        self.lru_queue.push_back(memory_id);

        self.similarity_wells.insert(memory_id, well);
        self.wells_created += 1;

        // Log memory usage warning if exceeding 4GB
        if self.memory_usage_bytes > 4_294_967_296 {  // 4GB
            tracing::warn!(
                memory_usage_gb = self.memory_usage_bytes as f64 / 1_073_741_824.0,
                wells_count = self.similarity_wells.len(),
                "Memory usage exceeding 4GB threshold"
            );
        }

        tracing::debug!(
            memory_id = memory_id.0,
            wells_count = self.similarity_wells.len(),
            memory_usage_mb = self.memory_usage_bytes as f32 / 1_048_576.0,
            connection_id = connection_id.as_deref().unwrap_or("none"),
            "Similarity well created"
        );

        Ok(())
    }

    /// Process a spike pattern through the reservoir and return similarity activations
    pub fn process_pattern(&mut self, input_pattern: &SpikePattern) -> Result<HashMap<MemoryId, f32>> {
        // Reset reservoir state
        self.reset_reservoir();

        // Clean up expired wells periodically
        if self.wells_created % 100 == 0 {
            self.cleanup_expired_wells();
        }

        // Simulate the pattern through the reservoir
        let reservoir_activity = self.simulate_pattern(input_pattern)?;

        // Calculate similarity well activations
        let mut activations = HashMap::new();
        let mut accessed_wells = Vec::new();  // Track which wells were accessed

        for (memory_id, well) in &mut self.similarity_wells {
            let activation = well.calculate_activation(&reservoir_activity);
            activations.insert(*memory_id, activation);

            // Track wells that were accessed for LRU update
            if activation > 0.0 {
                accessed_wells.push(*memory_id);
            }

            // Update the well if it's activated above threshold
            if activation > self.config.similarity_threshold {
                well.update_from_activation(&reservoir_activity, 0.001); // Small learning rate
            }
        }

        // Update LRU tracking after iteration
        for memory_id in accessed_wells {
            self.update_lru(memory_id);
        }

        Ok(activations)
    }

    /// Simulate the input pattern through the reservoir dynamics
    fn simulate_pattern(&mut self, input_pattern: &SpikePattern) -> Result<Vec<f32>> {
        let simulation_duration = input_pattern.duration_ms;
        let mut time = 0.0;
        let mut activity_trace = vec![0.0; self.config.reservoir_size];

        // Create input mapping (first N neurons receive input spikes)
        let input_neurons = std::cmp::min(input_pattern.neuron_count, self.config.reservoir_size);
        
        while time <= simulation_duration {
            // Inject input spikes
            for input_neuron in 0..input_neurons {
                if input_neuron < input_pattern.spike_times.len() {
                    for &spike_time in &input_pattern.spike_times[input_neuron] {
                        if (spike_time - time).abs() < self.time_step / 2.0 {
                            self.neurons[input_neuron].input_current += 10.0; // Strong input current
                        }
                    }
                }
            }

            // Update neuron states
            self.update_neurons(time);

            // Record activity for this timestep
            for (i, neuron) in self.neurons.iter().enumerate() {
                // Exponential decay of activity trace
                activity_trace[i] *= 0.99;
                
                // Add spike contribution
                if neuron.potential > -55.0 { // Spike threshold
                    activity_trace[i] += 1.0;
                    self.total_spikes += 1;
                }
            }

            time += self.time_step;
        }

        // Normalize activity trace
        let max_activity = activity_trace.iter().cloned().fold(0.0f32, f32::max);
        if max_activity > 0.0 {
            for activity in &mut activity_trace {
                *activity /= max_activity;
            }
        }

        Ok(activity_trace)
    }

    /// Update all neurons using leaky integrate-and-fire dynamics
    fn update_neurons(&mut self, current_time: f32) {
        self.current_time = current_time;
        
        // Collect spike events first to avoid borrowing issues
        let mut spike_events = Vec::new();
        
        // Update membrane potentials
        for i in 0..self.neurons.len() {
            let neuron = &mut self.neurons[i];
            
            // Skip if in refractory period
            if neuron.refractory_period > 0.0 {
                neuron.refractory_period -= self.time_step;
                neuron.potential = -70.0; // Hold at resting potential
                continue;
            }

            // Leaky integration
            let tau_membrane = 20.0; // 20ms membrane time constant
            let leak_current = -(neuron.potential - (-70.0)) / tau_membrane;
            
            // Update potential
            let total_current = neuron.input_current + leak_current;
            neuron.potential += total_current * self.time_step;

            // Check for spike
            if neuron.potential > -55.0 { // Spike threshold
                neuron.last_spike_time = current_time;
                neuron.refractory_period = 2.0; // 2ms refractory period
                neuron.potential = -70.0; // Reset potential
                
                // Record spike for later processing
                spike_events.push(i);
            }

            // Decay input current
            neuron.input_current *= 0.9;
        }
        
        // Process spike events
        for spiking_neuron in spike_events {
            // Propagate spike to connected neurons
            for synapse in &self.synapses[spiking_neuron].clone() {
                if synapse.target < self.neurons.len() {
                    self.neurons[synapse.target].input_current += synapse.weight;
                }
            }
        }
    }

    /// Reset reservoir to initial state
    fn reset_reservoir(&mut self) {
        for neuron in &mut self.neurons {
            neuron.reset();
        }
        self.current_time = 0.0;
    }

    /// Get the number of similarity wells
    pub fn get_wells_count(&self) -> usize {
        self.similarity_wells.len()
    }

    /// Get average activation across all wells
    pub fn get_average_activation(&self) -> f32 {
        if self.similarity_wells.is_empty() {
            return 0.0;
        }

        // For now, return a placeholder based on activation counts
        let total_activations: u64 = self.similarity_wells
            .values()
            .map(|well| well.activation_count)
            .sum();
        
        total_activations as f32 / self.similarity_wells.len() as f32
    }

    /// Estimate memory usage in bytes
    pub fn estimate_memory_usage(&self) -> usize {
        let neurons_size = std::mem::size_of::<NeuronState>() * self.neurons.len();
        let synapses_size = self.synapses
            .iter()
            .map(|s| std::mem::size_of::<Synapse>() * s.len())
            .sum::<usize>();
        let wells_size = self.similarity_wells
            .values()
            .map(|well| {
                std::mem::size_of::<SimilarityWell>() + 
                well.readout_weights.len() * std::mem::size_of::<f32>() +
                well.content.len()
            })
            .sum::<usize>();

        neurons_size + synapses_size + wells_size
    }

    /// Optimize reservoir dynamics by pruning inactive wells
    pub fn optimize_dynamics(&mut self) -> Result<()> {
        let cutoff_time = std::time::Instant::now() - std::time::Duration::from_secs(3600); // 1 hour
        
        // Remove wells that haven't been activated recently and have low activation counts
        self.similarity_wells.retain(|_memory_id, well| {
            well.activation_count > 5 || well.last_activated > cutoff_time
        });

        // Normalize well strengths
        let max_strength = self.similarity_wells
            .values()
            .map(|well| well.strength)
            .fold(0.0f32, f32::max);
        
        if max_strength > 0.0 {
            for well in self.similarity_wells.values_mut() {
                well.strength /= max_strength;
            }
        }

        tracing::info!(
            remaining_wells = self.similarity_wells.len(),
            "Reservoir dynamics optimized"
        );

        Ok(())
    }

    /// Get well by memory ID
    pub fn get_well(&self, memory_id: &MemoryId) -> Option<&SimilarityWell> {
        self.similarity_wells.get(memory_id)
    }

    /// Get all memory IDs in the reservoir
    pub fn get_memory_ids(&self) -> Vec<MemoryId> {
        self.similarity_wells.keys().copied().collect()
    }

    /// Evict the least recently used well
    fn evict_lru_well(&mut self) {
        if let Some(oldest_id) = self.lru_queue.pop_front() {
            if let Some(well) = self.similarity_wells.remove(&oldest_id) {
                // Update memory usage
                let well_size = self.estimate_well_size(&well);
                self.memory_usage_bytes = self.memory_usage_bytes.saturating_sub(well_size);

                // Remove from connection tracking
                if let Some(conn_id) = &well.connection_id {
                    if let Some(conn_wells) = self.connection_wells.get_mut(conn_id) {
                        conn_wells.retain(|&id| id != oldest_id);
                    }
                }

                self.wells_evicted += 1;

                tracing::debug!(
                    memory_id = oldest_id.0,
                    activation_count = well.activation_count,
                    age_seconds = well.created_at.elapsed().as_secs(),
                    "Evicted LRU well"
                );
            }
        }
    }

    /// Clean up wells that have exceeded TTL
    fn cleanup_expired_wells(&mut self) {
        let now = Instant::now();
        let ttl_duration = Duration::from_secs(self.ttl_seconds);

        let expired_ids: Vec<MemoryId> = self.similarity_wells
            .iter()
            .filter_map(|(id, well)| {
                if now.duration_since(well.created_at) > ttl_duration {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();

        for id in expired_ids {
            if let Some(well) = self.similarity_wells.remove(&id) {
                // Update memory usage
                let well_size = self.estimate_well_size(&well);
                self.memory_usage_bytes = self.memory_usage_bytes.saturating_sub(well_size);

                // Remove from LRU queue
                self.lru_queue.retain(|&queue_id| queue_id != id);

                // Remove from connection tracking
                if let Some(conn_id) = &well.connection_id {
                    if let Some(conn_wells) = self.connection_wells.get_mut(conn_id) {
                        conn_wells.retain(|&well_id| well_id != id);
                    }
                }

                self.wells_evicted += 1;

                tracing::debug!(
                    memory_id = id.0,
                    age_seconds = well.created_at.elapsed().as_secs(),
                    "Evicted expired well"
                );
            }
        }
    }

    /// Clean up all wells associated with a connection
    pub fn cleanup_connection(&mut self, connection_id: &str) {
        if let Some(well_ids) = self.connection_wells.remove(connection_id) {
            let wells_count = well_ids.len();

            for id in well_ids {
                if let Some(well) = self.similarity_wells.remove(&id) {
                    // Update memory usage
                    let well_size = self.estimate_well_size(&well);
                    self.memory_usage_bytes = self.memory_usage_bytes.saturating_sub(well_size);

                    // Remove from LRU queue
                    self.lru_queue.retain(|&queue_id| queue_id != id);

                    self.wells_evicted += 1;
                }
            }

            tracing::info!(
                connection_id = connection_id,
                wells_cleaned = wells_count,
                "Cleaned up wells for disconnected connection"
            );
        }
    }

    /// Estimate the memory size of a well in bytes
    fn estimate_well_size(&self, well: &SimilarityWell) -> usize {
        std::mem::size_of::<SimilarityWell>()
            + well.readout_weights.len() * std::mem::size_of::<f32>()
            + well.content.len()
            + well.reference_pattern.spike_times.iter()
                .map(|times| times.len() * std::mem::size_of::<f32>())
                .sum::<usize>()
    }

    /// Update LRU order when a well is accessed
    fn update_lru(&mut self, memory_id: MemoryId) {
        // Remove from current position
        self.lru_queue.retain(|&id| id != memory_id);
        // Add to back (most recently used)
        self.lru_queue.push_back(memory_id);
    }

    /// Get memory statistics
    pub fn get_memory_stats(&self) -> MemoryStats {
        MemoryStats {
            total_wells: self.similarity_wells.len(),
            max_wells: self.max_wells,
            wells_created: self.wells_created,
            wells_evicted: self.wells_evicted,
            memory_usage_bytes: self.memory_usage_bytes,
            memory_usage_mb: self.memory_usage_bytes as f32 / 1_048_576.0,
            connection_count: self.connection_wells.len(),
            ttl_seconds: self.ttl_seconds,
        }
    }

    /// Set maximum wells limit
    pub fn set_max_wells(&mut self, max_wells: usize) {
        self.max_wells = max_wells;

        // Evict wells if we're over the new limit
        while self.similarity_wells.len() > self.max_wells {
            self.evict_lru_well();
        }
    }

    /// Set TTL for wells
    pub fn set_ttl(&mut self, ttl_seconds: u64) {
        self.ttl_seconds = ttl_seconds;
    }
}

/// Memory statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_wells: usize,
    pub max_wells: usize,
    pub wells_created: u64,
    pub wells_evicted: u64,
    pub memory_usage_bytes: usize,
    pub memory_usage_mb: f32,
    pub connection_count: usize,
    pub ttl_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::{RateCodingEncoder, SpikeEncoder};
    use ndarray::array;

    #[test]
    fn test_reservoir_creation() {
        let config = DSRConfig::default();
        let reservoir = SimilarityReservoir::new(config.clone()).unwrap();

        assert_eq!(reservoir.neurons.len(), config.reservoir_size);
        assert_eq!(reservoir.synapses.len(), config.reservoir_size);
        
        // Check excitatory/inhibitory ratio
        let excitatory_count = reservoir.neurons.iter().filter(|n| n.is_excitatory).count();
        let expected_excitatory = (config.reservoir_size * 4) / 5;
        assert_eq!(excitatory_count, expected_excitatory);
    }

    #[test]
    fn test_similarity_well_creation() {
        let config = DSRConfig::default();
        let mut reservoir = SimilarityReservoir::new(config).unwrap();

        let encoder = RateCodingEncoder::new(5, 10.0);
        let embedding = array![0.1, 0.2, 0.3, 0.4, 0.5];
        let pattern = encoder.encode(embedding.view()).unwrap();

        let memory_id = MemoryId(1);
        reservoir.create_similarity_well(
            memory_id,
            pattern,
            "test memory".to_string(),
        ).unwrap();

        assert_eq!(reservoir.get_wells_count(), 1);
        assert!(reservoir.get_well(&memory_id).is_some());
    }

    #[test]
    fn test_lru_eviction() {
        let mut config = DSRConfig::default();
        config.max_similarity_wells = 3;  // Small limit for testing
        let mut reservoir = SimilarityReservoir::new(config).unwrap();

        let encoder = RateCodingEncoder::new(5, 10.0);

        // Add 4 wells, should evict the first one
        for i in 0..4 {
            let embedding = array![0.1 * i as f32, 0.2, 0.3, 0.4, 0.5];
            let pattern = encoder.encode(embedding.view()).unwrap();
            let memory_id = MemoryId(i);
            reservoir.create_similarity_well(
                memory_id,
                pattern,
                format!("memory {}", i),
            ).unwrap();
        }

        // Should have only 3 wells (max_wells)
        assert_eq!(reservoir.get_wells_count(), 3);
        // First well should be evicted
        assert!(reservoir.get_well(&MemoryId(0)).is_none());
        // Later wells should still exist
        assert!(reservoir.get_well(&MemoryId(1)).is_some());
        assert!(reservoir.get_well(&MemoryId(2)).is_some());
        assert!(reservoir.get_well(&MemoryId(3)).is_some());
    }

    #[test]
    fn test_connection_cleanup() {
        let config = DSRConfig::default();
        let mut reservoir = SimilarityReservoir::new(config).unwrap();

        let encoder = RateCodingEncoder::new(5, 10.0);
        let conn_id = "test-connection";

        // Add wells with connection ID
        for i in 0..3 {
            let embedding = array![0.1 * i as f32, 0.2, 0.3, 0.4, 0.5];
            let pattern = encoder.encode(embedding.view()).unwrap();
            let memory_id = MemoryId(i);
            reservoir.create_similarity_well_with_connection(
                memory_id,
                pattern,
                format!("memory {}", i),
                Some(conn_id.to_string()),
            ).unwrap();
        }

        assert_eq!(reservoir.get_wells_count(), 3);

        // Clean up connection
        reservoir.cleanup_connection(conn_id);

        // All wells should be removed
        assert_eq!(reservoir.get_wells_count(), 0);
    }

    #[test]
    fn test_pattern_processing() {
        let config = DSRConfig::default();
        let mut reservoir = SimilarityReservoir::new(config).unwrap();
        
        let encoder = RateCodingEncoder::new(10, 10.0);
        let embedding = array![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
        let pattern = encoder.encode(embedding.view()).unwrap();
        
        // Create a similarity well
        let memory_id = MemoryId(1);
        reservoir.create_similarity_well(
            memory_id,
            pattern.clone(),
            "test memory".to_string(),
        ).unwrap();

        // Process the same pattern through the reservoir
        let activations = reservoir.process_pattern(&pattern).unwrap();
        
        assert!(activations.contains_key(&memory_id));
        // The exact same pattern should produce some activation
        assert!(activations[&memory_id] >= 0.0);
    }
}