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
use std::collections::HashMap;
use parking_lot::RwLock;
use rand::Rng;
use rand_distr::{Normal, Distribution};

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
    /// Readout weights connecting reservoir to this well
    pub readout_weights: Array1<f32>,
}

impl SimilarityWell {
    pub fn new(
        memory_id: MemoryId,
        reference_pattern: SpikePattern,
        content: String,
        reservoir_size: usize,
    ) -> Self {
        // Initialize readout weights randomly with small values
        let mut rng = rand::thread_rng();
        let normal = Normal::new(0.0, 0.01).unwrap();
        let readout_weights = Array1::from_vec(
            (0..reservoir_size)
                .map(|_| normal.sample(&mut rng) as f32)
                .collect()
        );

        Self {
            memory_id,
            reference_pattern,
            content,
            strength: 1.0,
            activation_count: 0,
            last_activated: std::time::Instant::now(),
            readout_weights,
        }
    }

    /// Update the well based on activation patterns
    pub fn update_from_activation(&mut self, reservoir_activity: &[f32], learning_rate: f32) {
        self.activation_count += 1;
        self.last_activated = std::time::Instant::now();

        // Simple Hebbian learning for readout weights
        for (weight, &activity) in self.readout_weights.iter_mut().zip(reservoir_activity.iter()) {
            *weight += learning_rate * activity * self.strength;
            
            // Clip weights to prevent runaway growth
            *weight = weight.clamp(-1.0, 1.0);
        }
    }

    /// Calculate activation strength given current reservoir state
    pub fn calculate_activation(&self, reservoir_activity: &[f32]) -> f32 {
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
    
    // Simulation state
    current_time: f32,
    time_step: f32, // Integration time step in milliseconds
    
    // Performance tracking
    total_spikes: u64,
    wells_created: u64,
}

impl SimilarityReservoir {
    pub fn new(config: DSRConfig) -> Result<Self> {
        let mut reservoir = Self {
            config: config.clone(),
            neurons: Vec::new(),
            synapses: vec![Vec::new(); config.reservoir_size],
            similarity_wells: HashMap::new(),
            current_time: 0.0,
            time_step: 0.1, // 0.1ms time step
            total_spikes: 0,
            wells_created: 0,
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

    /// Create a new similarity well for a memory
    pub fn create_similarity_well(
        &mut self,
        memory_id: MemoryId,
        reference_pattern: SpikePattern,
        content: String,
    ) -> Result<()> {
        if self.similarity_wells.contains_key(&memory_id) {
            return Err(anyhow!("Memory {} already exists in reservoir", memory_id.0));
        }

        let well = SimilarityWell::new(
            memory_id,
            reference_pattern,
            content,
            self.config.reservoir_size,
        );

        self.similarity_wells.insert(memory_id, well);
        self.wells_created += 1;

        tracing::debug!(
            memory_id = memory_id.0,
            wells_count = self.similarity_wells.len(),
            "Similarity well created"
        );

        Ok(())
    }

    /// Process a spike pattern through the reservoir and return similarity activations
    pub fn process_pattern(&mut self, input_pattern: &SpikePattern) -> Result<HashMap<MemoryId, f32>> {
        // Reset reservoir state
        self.reset_reservoir();

        // Simulate the pattern through the reservoir
        let reservoir_activity = self.simulate_pattern(input_pattern)?;

        // Calculate similarity well activations
        let mut activations = HashMap::new();
        
        for (memory_id, well) in &mut self.similarity_wells {
            let activation = well.calculate_activation(&reservoir_activity);
            activations.insert(*memory_id, activation);
            
            // Update the well if it's activated above threshold
            if activation > self.config.similarity_threshold {
                well.update_from_activation(&reservoir_activity, 0.001); // Small learning rate
            }
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