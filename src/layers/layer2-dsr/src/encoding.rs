//! Embedding-to-Spike Encoding Strategies for Layer 2
//! 
//! Converts dense embeddings into spike patterns that can be processed
//! by the spiking neural reservoir. Multiple encoding strategies are supported
//! to optimize for different types of similarity matching.

use ndarray::{Array1, Array2, ArrayView1};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use rand::Rng;
use rand_distr::{Normal, Distribution};
use std::sync::Arc;

/// Available spike encoding strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncodingStrategy {
    /// Rate coding: Higher values = higher spike frequency
    RateCoding,
    /// Temporal coding: Values encoded as precise spike timing
    TemporalCoding,
    /// Population coding: Each dimension distributed across multiple neurons
    PopulationCoding,
    /// Delta modulation: Encode differences between dimensions
    DeltaModulation,
    /// Rank order coding: Encode dimensions as spike arrival order
    RankOrderCoding,
}

/// Represents a spike pattern with timing information
#[derive(Debug, Clone)]
pub struct SpikePattern {
    /// Spike times for each neuron (neuron_id -> spike_times)
    pub spike_times: Vec<Vec<f32>>,
    /// Total duration of the pattern in milliseconds
    pub duration_ms: f32,
    /// Number of neurons involved
    pub neuron_count: usize,
}

impl SpikePattern {
    pub fn new(neuron_count: usize, duration_ms: f32) -> Self {
        Self {
            spike_times: vec![Vec::new(); neuron_count],
            duration_ms,
            neuron_count,
        }
    }

    /// Add a spike at the given time for the specified neuron
    pub fn add_spike(&mut self, neuron_id: usize, time_ms: f32) {
        if neuron_id < self.neuron_count && time_ms <= self.duration_ms {
            self.spike_times[neuron_id].push(time_ms);
        }
    }

    /// Get total number of spikes across all neurons
    pub fn total_spike_count(&self) -> usize {
        self.spike_times.iter().map(|times| times.len()).sum()
    }

    /// Calculate spike rate for a specific neuron (spikes per second)
    pub fn neuron_spike_rate(&self, neuron_id: usize) -> f32 {
        if neuron_id >= self.neuron_count {
            return 0.0;
        }
        (self.spike_times[neuron_id].len() as f32) / (self.duration_ms / 1000.0)
    }
}

/// Trait for encoding embeddings into spike patterns
pub trait SpikeEncoder: Send + Sync {
    /// Encode an embedding into a spike pattern
    fn encode(&self, embedding: ArrayView1<f32>) -> Result<SpikePattern>;
    
    /// Get the number of neurons used by this encoder
    fn neuron_count(&self) -> usize;
    
    /// Get the encoding duration in milliseconds
    fn encoding_duration_ms(&self) -> f32;
}

/// Rate coding encoder: maps embedding values to spike frequencies
pub struct RateCodingEncoder {
    neuron_count: usize,
    duration_ms: f32,
    max_rate_hz: f32,
    min_rate_hz: f32,
}

impl RateCodingEncoder {
    pub fn new(embedding_dim: usize, duration_ms: f32) -> Self {
        Self {
            neuron_count: embedding_dim,
            duration_ms,
            max_rate_hz: 100.0, // Maximum 100 Hz
            min_rate_hz: 1.0,   // Minimum 1 Hz
        }
    }
}

impl SpikeEncoder for RateCodingEncoder {
    fn encode(&self, embedding: ArrayView1<f32>) -> Result<SpikePattern> {
        if embedding.len() != self.neuron_count {
            return Err(anyhow!(
                "Embedding dimension {} doesn't match neuron count {}",
                embedding.len(),
                self.neuron_count
            ));
        }

        let mut pattern = SpikePattern::new(self.neuron_count, self.duration_ms);
        let mut rng = rand::thread_rng();

        for (neuron_id, &value) in embedding.iter().enumerate() {
            // Normalize value to [0, 1] range assuming embeddings are roughly in [-1, 1]
            let normalized_value = ((value + 1.0) / 2.0).clamp(0.0, 1.0);
            
            // Map to spike rate
            let spike_rate = self.min_rate_hz + 
                (self.max_rate_hz - self.min_rate_hz) * normalized_value;
            
            // Generate Poisson-distributed spikes
            let expected_spikes = spike_rate * (self.duration_ms / 1000.0);
            let spike_count = if expected_spikes > 0.0 {
                rand_distr::Poisson::new(expected_spikes as f64)
                    .unwrap()
                    .sample(&mut rng) as usize
            } else {
                0
            };

            // Generate random spike times
            for _ in 0..spike_count {
                let spike_time = rng.gen::<f32>() * self.duration_ms;
                pattern.add_spike(neuron_id, spike_time);
            }

            // Sort spike times for this neuron
            pattern.spike_times[neuron_id].sort_by(|a, b| a.partial_cmp(b).unwrap());
        }

        Ok(pattern)
    }

    fn neuron_count(&self) -> usize {
        self.neuron_count
    }

    fn encoding_duration_ms(&self) -> f32 {
        self.duration_ms
    }
}

/// Temporal coding encoder: maps values to precise spike timings
pub struct TemporalCodingEncoder {
    neuron_count: usize,
    duration_ms: f32,
}

impl TemporalCodingEncoder {
    pub fn new(embedding_dim: usize, duration_ms: f32) -> Self {
        Self {
            neuron_count: embedding_dim,
            duration_ms,
        }
    }
}

impl SpikeEncoder for TemporalCodingEncoder {
    fn encode(&self, embedding: ArrayView1<f32>) -> Result<SpikePattern> {
        if embedding.len() != self.neuron_count {
            return Err(anyhow!(
                "Embedding dimension {} doesn't match neuron count {}",
                embedding.len(),
                self.neuron_count
            ));
        }

        let mut pattern = SpikePattern::new(self.neuron_count, self.duration_ms);

        for (neuron_id, &value) in embedding.iter().enumerate() {
            // Normalize value to [0, 1] range
            let normalized_value = ((value + 1.0) / 2.0).clamp(0.0, 1.0);
            
            // Map to spike time: higher values spike earlier
            let spike_time = (1.0 - normalized_value) * self.duration_ms;
            pattern.add_spike(neuron_id, spike_time);
        }

        Ok(pattern)
    }

    fn neuron_count(&self) -> usize {
        self.neuron_count
    }

    fn encoding_duration_ms(&self) -> f32 {
        self.duration_ms
    }
}

/// Population coding encoder: distributes each dimension across multiple neurons
pub struct PopulationCodingEncoder {
    embedding_dim: usize,
    neurons_per_dim: usize,
    duration_ms: f32,
    neuron_centers: Array2<f32>, // Centers of tuning curves
    neuron_widths: Array1<f32>,  // Widths of tuning curves
}

impl PopulationCodingEncoder {
    pub fn new(embedding_dim: usize, neurons_per_dim: usize, duration_ms: f32) -> Self {
        let total_neurons = embedding_dim * neurons_per_dim;
        
        // Create overlapping tuning curves for each dimension
        let mut neuron_centers = Array2::zeros((embedding_dim, neurons_per_dim));
        let neuron_widths = Array1::from_elem(total_neurons, 0.5); // Fixed width
        
        for dim in 0..embedding_dim {
            for i in 0..neurons_per_dim {
                // Distribute centers evenly across [-1, 1] range with overlap
                neuron_centers[[dim, i]] = -1.0 + (i as f32) * (2.0 / (neurons_per_dim - 1) as f32);
            }
        }

        Self {
            embedding_dim,
            neurons_per_dim,
            duration_ms,
            neuron_centers,
            neuron_widths,
        }
    }
}

impl SpikeEncoder for PopulationCodingEncoder {
    fn encode(&self, embedding: ArrayView1<f32>) -> Result<SpikePattern> {
        if embedding.len() != self.embedding_dim {
            return Err(anyhow!(
                "Embedding dimension {} doesn't match expected {}",
                embedding.len(),
                self.embedding_dim
            ));
        }

        let total_neurons = self.embedding_dim * self.neurons_per_dim;
        let mut pattern = SpikePattern::new(total_neurons, self.duration_ms);
        let mut rng = rand::thread_rng();

        for (dim, &value) in embedding.iter().enumerate() {
            for neuron_in_dim in 0..self.neurons_per_dim {
                let neuron_id = dim * self.neurons_per_dim + neuron_in_dim;
                let center = self.neuron_centers[[dim, neuron_in_dim]];
                let width = self.neuron_widths[neuron_id];

                // Calculate activation using Gaussian tuning curve
                let distance = (value - center).abs();
                let activation = (-distance.powi(2) / (2.0 * width.powi(2))).exp();

                // Convert activation to spike rate
                let max_rate = 50.0; // Max 50 Hz
                let spike_rate = activation * max_rate;
                let expected_spikes = spike_rate * (self.duration_ms / 1000.0);

                if expected_spikes > 0.0 {
                    let spike_count = rand_distr::Poisson::new(expected_spikes as f64)
                        .unwrap()
                        .sample(&mut rng) as usize;

                    for _ in 0..spike_count {
                        let spike_time = rng.gen::<f32>() * self.duration_ms;
                        pattern.add_spike(neuron_id, spike_time);
                    }

                    pattern.spike_times[neuron_id].sort_by(|a, b| a.partial_cmp(b).unwrap());
                }
            }
        }

        Ok(pattern)
    }

    fn neuron_count(&self) -> usize {
        self.embedding_dim * self.neurons_per_dim
    }

    fn encoding_duration_ms(&self) -> f32 {
        self.duration_ms
    }
}

/// Rank order coding encoder: encodes dimensions as spike arrival order
pub struct RankOrderCodingEncoder {
    neuron_count: usize,
    duration_ms: f32,
    time_precision_ms: f32,
}

impl RankOrderCodingEncoder {
    pub fn new(embedding_dim: usize, duration_ms: f32) -> Self {
        Self {
            neuron_count: embedding_dim,
            duration_ms,
            time_precision_ms: 0.1, // 0.1ms precision for ordering
        }
    }
}

impl SpikeEncoder for RankOrderCodingEncoder {
    fn encode(&self, embedding: ArrayView1<f32>) -> Result<SpikePattern> {
        if embedding.len() != self.neuron_count {
            return Err(anyhow!(
                "Embedding dimension {} doesn't match neuron count {}",
                embedding.len(),
                self.neuron_count
            ));
        }

        let mut pattern = SpikePattern::new(self.neuron_count, self.duration_ms);

        // Create index-value pairs and sort by value (descending)
        let mut indexed_values: Vec<(usize, f32)> = embedding
            .iter()
            .enumerate()
            .map(|(i, &val)| (i, val))
            .collect();
        
        indexed_values.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        // Assign spike times based on rank
        for (rank, &(neuron_id, _value)) in indexed_values.iter().enumerate() {
            let spike_time = rank as f32 * self.time_precision_ms;
            if spike_time < self.duration_ms {
                pattern.add_spike(neuron_id, spike_time);
            }
        }

        Ok(pattern)
    }

    fn neuron_count(&self) -> usize {
        self.neuron_count
    }

    fn encoding_duration_ms(&self) -> f32 {
        self.duration_ms
    }
}

/// Factory function to create encoders based on strategy
pub fn create_encoder(
    strategy: EncodingStrategy,
    embedding_dim: usize,
) -> Result<Arc<dyn SpikeEncoder>> {
    let duration_ms = 10.0; // Standard 10ms encoding window

    let encoder: Arc<dyn SpikeEncoder> = match strategy {
        EncodingStrategy::RateCoding => {
            Arc::new(RateCodingEncoder::new(embedding_dim, duration_ms))
        }
        EncodingStrategy::TemporalCoding => {
            Arc::new(TemporalCodingEncoder::new(embedding_dim, duration_ms))
        }
        EncodingStrategy::PopulationCoding => {
            Arc::new(PopulationCodingEncoder::new(embedding_dim, 5, duration_ms))
        }
        EncodingStrategy::RankOrderCoding => {
            Arc::new(RankOrderCodingEncoder::new(embedding_dim, duration_ms))
        }
        EncodingStrategy::DeltaModulation => {
            // For now, use rate coding as delta modulation placeholder
            Arc::new(RateCodingEncoder::new(embedding_dim, duration_ms))
        }
    };

    Ok(encoder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn test_rate_coding_encoder() {
        let encoder = RateCodingEncoder::new(3, 10.0);
        let embedding = array![0.5, -0.5, 1.0];
        
        let pattern = encoder.encode(embedding.view()).unwrap();
        
        assert_eq!(pattern.neuron_count, 3);
        assert_eq!(pattern.duration_ms, 10.0);
        assert!(pattern.total_spike_count() > 0);
        
        // Higher values should generally produce more spikes
        let rate_0 = pattern.neuron_spike_rate(0);
        let rate_1 = pattern.neuron_spike_rate(1);
        let rate_2 = pattern.neuron_spike_rate(2);
        
        println!("Rates: {} {} {}", rate_0, rate_1, rate_2);
        // Note: Due to randomness, we can't make strict assertions about ordering
    }

    #[test]
    fn test_temporal_coding_encoder() {
        let encoder = TemporalCodingEncoder::new(3, 10.0);
        let embedding = array![1.0, 0.0, -1.0];
        
        let pattern = encoder.encode(embedding.view()).unwrap();
        
        assert_eq!(pattern.neuron_count, 3);
        assert_eq!(pattern.spike_times[0].len(), 1); // Each neuron gets exactly one spike
        assert_eq!(pattern.spike_times[1].len(), 1);
        assert_eq!(pattern.spike_times[2].len(), 1);
        
        // Higher values should spike earlier
        let spike_time_0 = pattern.spike_times[0][0];
        let spike_time_1 = pattern.spike_times[1][0];
        let spike_time_2 = pattern.spike_times[2][0];
        
        assert!(spike_time_0 < spike_time_1); // 1.0 > 0.0, so earlier spike
        assert!(spike_time_1 < spike_time_2); // 0.0 > -1.0, so earlier spike
    }

    #[test]
    fn test_population_coding_encoder() {
        let encoder = PopulationCodingEncoder::new(2, 3, 50.0); // Longer duration for more spikes
        let embedding = array![0.0, 0.5]; // Values closer to neuron centers
        
        let pattern = encoder.encode(embedding.view()).unwrap();
        
        assert_eq!(pattern.neuron_count, 6); // 2 dimensions * 3 neurons per dimension
        assert!(pattern.total_spike_count() > 0, "Expected spikes but got {}", pattern.total_spike_count());
    }

    #[test]
    fn test_rank_order_coding_encoder() {
        let encoder = RankOrderCodingEncoder::new(4, 10.0);
        let embedding = array![0.2, 0.8, 0.1, 0.5]; // Order should be: 1, 3, 0, 2
        
        let pattern = encoder.encode(embedding.view()).unwrap();
        
        assert_eq!(pattern.neuron_count, 4);
        
        // Each neuron should have exactly one spike
        for neuron_spikes in &pattern.spike_times {
            assert_eq!(neuron_spikes.len(), 1);
        }
        
        // Check ordering: neuron 1 (value 0.8) should spike first
        let spike_time_1 = pattern.spike_times[1][0];
        let spike_time_3 = pattern.spike_times[3][0];
        let spike_time_0 = pattern.spike_times[0][0];
        let spike_time_2 = pattern.spike_times[2][0];
        
        assert!(spike_time_1 < spike_time_3);
        assert!(spike_time_3 < spike_time_0);
        assert!(spike_time_0 < spike_time_2);
    }
}