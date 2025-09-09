//! Spike Dynamics and Temporal Processing for Layer 2
//! 
//! Handles temporal integration, spike timing analysis, and dynamic
//! pattern evolution within the reservoir network.

use ndarray::{Array1, Array2};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::SpikePattern;

/// Temporal window for spike integration and analysis
#[derive(Debug, Clone)]
pub struct TemporalWindow {
    /// Window duration in milliseconds
    pub duration_ms: f32,
    /// Time resolution (bin size) in milliseconds
    pub resolution_ms: f32,
    /// Number of time bins
    pub bin_count: usize,
    /// Current spike history within the window
    pub spike_history: VecDeque<Array1<f32>>,
    /// Current time position
    pub current_time: f32,
}

impl TemporalWindow {
    pub fn new(duration_ms: f32, resolution_ms: f32) -> Self {
        let bin_count = (duration_ms / resolution_ms) as usize;
        
        Self {
            duration_ms,
            resolution_ms,
            bin_count,
            spike_history: VecDeque::with_capacity(bin_count),
            current_time: 0.0,
        }
    }

    /// Add a new spike activity vector to the window
    pub fn add_activity(&mut self, activity: Array1<f32>) {
        self.spike_history.push_back(activity);
        
        // Remove old entries to maintain window size
        while self.spike_history.len() > self.bin_count {
            self.spike_history.pop_front();
        }
        
        self.current_time += self.resolution_ms;
    }

    /// Get the current activity pattern as a 2D array (time x neurons)
    pub fn get_activity_matrix(&self) -> Array2<f32> {
        if self.spike_history.is_empty() {
            return Array2::zeros((0, 0));
        }

        let neuron_count = self.spike_history[0].len();
        let time_steps = self.spike_history.len();
        
        let mut matrix = Array2::zeros((time_steps, neuron_count));
        
        for (t, activity) in self.spike_history.iter().enumerate() {
            for (n, &value) in activity.iter().enumerate() {
                matrix[[t, n]] = value;
            }
        }
        
        matrix
    }

    /// Calculate temporal correlation between different time lags
    pub fn calculate_autocorrelation(&self, max_lag: usize) -> Array1<f32> {
        let activity_matrix = self.get_activity_matrix();
        let mut correlations = Array1::zeros(max_lag + 1);
        
        if activity_matrix.nrows() < 2 {
            return correlations;
        }

        for lag in 0..=max_lag {
            if lag >= activity_matrix.nrows() {
                break;
            }

            let mut correlation_sum = 0.0;
            let mut count = 0;

            for t in lag..activity_matrix.nrows() {
                for n in 0..activity_matrix.ncols() {
                    let current = activity_matrix[[t, n]];
                    let lagged = activity_matrix[[t - lag, n]];
                    correlation_sum += current * lagged;
                    count += 1;
                }
            }

            correlations[lag] = if count > 0 {
                correlation_sum / count as f32
            } else {
                0.0
            };
        }

        correlations
    }

    /// Reset the temporal window
    pub fn reset(&mut self) {
        self.spike_history.clear();
        self.current_time = 0.0;
    }
}

/// Spike dynamics analyzer for pattern evolution
pub struct SpikeDynamics {
    /// Temporal window for integration
    pub window: TemporalWindow,
    /// Adaptation time constant
    pub adaptation_tau: f32,
    /// Facilitation time constant
    pub facilitation_tau: f32,
    /// Depression recovery time constant
    pub depression_tau: f32,
}

impl SpikeDynamics {
    pub fn new(window_duration_ms: f32, time_resolution_ms: f32) -> Self {
        Self {
            window: TemporalWindow::new(window_duration_ms, time_resolution_ms),
            adaptation_tau: 50.0,      // 50ms adaptation
            facilitation_tau: 100.0,   // 100ms facilitation
            depression_tau: 200.0,     // 200ms depression recovery
        }
    }

    /// Process a spike pattern through temporal dynamics
    pub fn process_pattern(&mut self, pattern: &SpikePattern) -> Result<TemporalDynamicsResult> {
        // Reset window for new pattern
        self.window.reset();

        // Convert spike pattern to time-binned activity
        let activity_sequence = self.convert_to_activity_sequence(pattern)?;

        // Process each time bin through the temporal window
        for activity in activity_sequence {
            self.window.add_activity(activity);
        }

        // Calculate temporal features
        let result = self.calculate_temporal_features()?;

        Ok(result)
    }

    /// Convert spike pattern to binned activity sequence
    fn convert_to_activity_sequence(&self, pattern: &SpikePattern) -> Result<Vec<Array1<f32>>> {
        let time_bins = (pattern.duration_ms / self.window.resolution_ms) as usize;
        let mut activity_sequence = Vec::with_capacity(time_bins);

        for bin in 0..time_bins {
            let bin_start = bin as f32 * self.window.resolution_ms;
            let bin_end = bin_start + self.window.resolution_ms;
            
            let mut activity = Array1::zeros(pattern.neuron_count);

            // Count spikes in this time bin for each neuron
            for (neuron_id, spike_times) in pattern.spike_times.iter().enumerate() {
                let spikes_in_bin = spike_times
                    .iter()
                    .filter(|&&spike_time| spike_time >= bin_start && spike_time < bin_end)
                    .count();

                activity[neuron_id] = spikes_in_bin as f32;
            }

            activity_sequence.push(activity);
        }

        Ok(activity_sequence)
    }

    /// Calculate comprehensive temporal features
    fn calculate_temporal_features(&self) -> Result<TemporalDynamicsResult> {
        let activity_matrix = self.window.get_activity_matrix();
        
        if activity_matrix.is_empty() {
            return Ok(TemporalDynamicsResult::empty());
        }

        // Calculate various temporal measures
        let synchrony = self.calculate_synchrony_index(&activity_matrix);
        let complexity = self.calculate_complexity_measure(&activity_matrix);
        let stability = self.calculate_stability_measure(&activity_matrix);
        let propagation_speed = self.calculate_propagation_speed(&activity_matrix);
        let oscillation_frequency = self.detect_oscillation_frequency(&activity_matrix);

        // Calculate autocorrelation with multiple lags
        let max_lag = std::cmp::min(10, activity_matrix.nrows() - 1);
        let autocorrelation = self.window.calculate_autocorrelation(max_lag);

        // Calculate burst detection
        let burst_statistics = self.detect_bursts(&activity_matrix);

        Ok(TemporalDynamicsResult {
            synchrony_index: synchrony,
            complexity_measure: complexity,
            stability_measure: stability,
            propagation_speed,
            oscillation_frequency,
            autocorrelation,
            burst_statistics,
            processing_time_ms: self.window.duration_ms,
        })
    }

    /// Calculate neural synchrony index
    fn calculate_synchrony_index(&self, activity_matrix: &Array2<f32>) -> f32 {
        if activity_matrix.nrows() < 2 || activity_matrix.ncols() < 2 {
            return 0.0;
        }

        let mut synchrony_sum = 0.0;
        let mut pair_count = 0;

        // Calculate pairwise correlations between neurons
        for n1 in 0..activity_matrix.ncols() {
            for n2 in (n1 + 1)..activity_matrix.ncols() {
                let mut correlation = 0.0;
                let mut norm1 = 0.0;
                let mut norm2 = 0.0;

                for t in 0..activity_matrix.nrows() {
                    let activity1 = activity_matrix[[t, n1]];
                    let activity2 = activity_matrix[[t, n2]];
                    
                    correlation += activity1 * activity2;
                    norm1 += activity1 * activity1;
                    norm2 += activity2 * activity2;
                }

                if norm1 > 0.0 && norm2 > 0.0 {
                    synchrony_sum += correlation / (norm1.sqrt() * norm2.sqrt());
                    pair_count += 1;
                }
            }
        }

        if pair_count > 0 {
            synchrony_sum / pair_count as f32
        } else {
            0.0
        }
    }

    /// Calculate complexity measure using entropy-like metric
    fn calculate_complexity_measure(&self, activity_matrix: &Array2<f32>) -> f32 {
        if activity_matrix.is_empty() {
            return 0.0;
        }

        // Calculate spatial complexity (entropy across neurons at each time step)
        let mut temporal_complexities = Vec::new();

        for t in 0..activity_matrix.nrows() {
            let mut entropy = 0.0;
            let mut total_activity = 0.0;

            // Calculate total activity for normalization
            for n in 0..activity_matrix.ncols() {
                total_activity += activity_matrix[[t, n]];
            }

            if total_activity > 0.0 {
                // Calculate entropy
                for n in 0..activity_matrix.ncols() {
                    let p = activity_matrix[[t, n]] / total_activity;
                    if p > 0.0 {
                        entropy -= p * p.log2();
                    }
                }
            }

            temporal_complexities.push(entropy);
        }

        // Return average complexity across time
        if temporal_complexities.is_empty() {
            0.0
        } else {
            temporal_complexities.iter().sum::<f32>() / temporal_complexities.len() as f32
        }
    }

    /// Calculate stability measure based on activity variance
    fn calculate_stability_measure(&self, activity_matrix: &Array2<f32>) -> f32 {
        if activity_matrix.nrows() < 2 {
            return 1.0; // Perfectly stable if only one time point
        }

        let mut total_variance = 0.0;
        let mut neuron_count = 0;

        for n in 0..activity_matrix.ncols() {
            // Calculate mean activity for this neuron
            let mut sum = 0.0;
            for t in 0..activity_matrix.nrows() {
                sum += activity_matrix[[t, n]];
            }
            let mean = sum / activity_matrix.nrows() as f32;

            // Calculate variance
            let mut variance = 0.0;
            for t in 0..activity_matrix.nrows() {
                let diff = activity_matrix[[t, n]] - mean;
                variance += diff * diff;
            }
            variance /= activity_matrix.nrows() as f32;

            total_variance += variance;
            neuron_count += 1;
        }

        if neuron_count > 0 {
            // Convert variance to stability (lower variance = higher stability)
            let avg_variance = total_variance / neuron_count as f32;
            1.0 / (1.0 + avg_variance)
        } else {
            0.0
        }
    }

    /// Calculate propagation speed across the neural population
    fn calculate_propagation_speed(&self, activity_matrix: &Array2<f32>) -> f32 {
        if activity_matrix.nrows() < 2 || activity_matrix.ncols() < 2 {
            return 0.0;
        }

        // Find peak activity time for each neuron
        let mut peak_times = Vec::new();
        
        for n in 0..activity_matrix.ncols() {
            let mut max_activity = 0.0;
            let mut peak_time = 0;

            for t in 0..activity_matrix.nrows() {
                if activity_matrix[[t, n]] > max_activity {
                    max_activity = activity_matrix[[t, n]];
                    peak_time = t;
                }
            }

            if max_activity > 0.0 {
                peak_times.push(peak_time as f32 * self.window.resolution_ms);
            }
        }

        if peak_times.len() < 2 {
            return 0.0;
        }

        // Calculate the spread of peak times
        let min_time = peak_times.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_time = peak_times.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        
        let time_spread = max_time - min_time;
        if time_spread > 0.0 {
            // Return propagation speed in arbitrary units (neurons per ms)
            (activity_matrix.ncols() as f32) / time_spread
        } else {
            f32::INFINITY // Synchronous activation
        }
    }

    /// Detect dominant oscillation frequency using simple peak detection
    fn detect_oscillation_frequency(&self, activity_matrix: &Array2<f32>) -> f32 {
        if activity_matrix.nrows() < 4 {
            return 0.0;
        }

        // Calculate population activity over time
        let mut population_activity = Vec::new();
        for t in 0..activity_matrix.nrows() {
            let mut total = 0.0;
            for n in 0..activity_matrix.ncols() {
                total += activity_matrix[[t, n]];
            }
            population_activity.push(total);
        }

        // Simple peak detection
        let mut peak_intervals = Vec::new();
        let mut last_peak_time = None;

        for (t, &activity) in population_activity.iter().enumerate() {
            // Look for local maxima
            let is_peak = (t == 0 || activity > population_activity[t - 1]) &&
                          (t == population_activity.len() - 1 || activity > population_activity[t + 1]);

            if is_peak && activity > 0.1 { // Minimum threshold for peak
                if let Some(last_peak) = last_peak_time {
                    peak_intervals.push(t - last_peak);
                }
                last_peak_time = Some(t);
            }
        }

        if peak_intervals.is_empty() {
            return 0.0;
        }

        // Calculate average interval and convert to frequency
        let avg_interval = peak_intervals.iter().sum::<usize>() as f32 / peak_intervals.len() as f32;
        let interval_ms = avg_interval * self.window.resolution_ms;
        
        if interval_ms > 0.0 {
            1000.0 / interval_ms // Convert to Hz
        } else {
            0.0
        }
    }

    /// Detect burst patterns in activity
    fn detect_bursts(&self, activity_matrix: &Array2<f32>) -> BurstStatistics {
        let mut bursts_detected = 0;
        let mut burst_durations = Vec::new();
        let mut interburst_intervals = Vec::new();

        // Calculate population activity
        let mut population_activity = Vec::new();
        for t in 0..activity_matrix.nrows() {
            let total: f32 = (0..activity_matrix.ncols())
                .map(|n| activity_matrix[[t, n]])
                .sum();
            population_activity.push(total);
        }

        if population_activity.is_empty() {
            return BurstStatistics::default();
        }

        // Calculate threshold as a fraction of maximum activity
        let max_activity = population_activity.iter().cloned().fold(0.0f32, f32::max);
        let burst_threshold = max_activity * 0.3; // 30% of maximum

        // Detect bursts
        let mut in_burst = false;
        let mut burst_start = 0;
        let mut last_burst_end = 0;

        for (t, &activity) in population_activity.iter().enumerate() {
            if !in_burst && activity > burst_threshold {
                // Start of burst
                in_burst = true;
                burst_start = t;
                
                if bursts_detected > 0 {
                    interburst_intervals.push(t - last_burst_end);
                }
            } else if in_burst && activity <= burst_threshold {
                // End of burst
                in_burst = false;
                last_burst_end = t;
                bursts_detected += 1;
                burst_durations.push(t - burst_start);
            }
        }

        let avg_duration = if burst_durations.is_empty() {
            0.0
        } else {
            burst_durations.iter().sum::<usize>() as f32 / burst_durations.len() as f32
        };

        let avg_interval = if interburst_intervals.is_empty() {
            0.0
        } else {
            interburst_intervals.iter().sum::<usize>() as f32 / interburst_intervals.len() as f32
        };

        BurstStatistics {
            burst_count: bursts_detected,
            average_duration_ms: avg_duration * self.window.resolution_ms,
            average_interval_ms: avg_interval * self.window.resolution_ms,
            burst_rate_hz: if self.window.duration_ms > 0.0 {
                (bursts_detected as f32) / (self.window.duration_ms / 1000.0)
            } else {
                0.0
            },
        }
    }
}

/// Results from temporal dynamics analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDynamicsResult {
    /// Neural synchrony index (0.0 to 1.0)
    pub synchrony_index: f32,
    /// Complexity measure based on entropy
    pub complexity_measure: f32,
    /// Stability measure (higher = more stable)
    pub stability_measure: f32,
    /// Propagation speed across population
    pub propagation_speed: f32,
    /// Dominant oscillation frequency in Hz
    pub oscillation_frequency: f32,
    /// Autocorrelation function (as Vec for serialization)
    #[serde(with = "autocorr_serde")]
    pub autocorrelation: Array1<f32>,
    /// Burst detection statistics
    pub burst_statistics: BurstStatistics,
    /// Total processing time
    pub processing_time_ms: f32,
}

impl TemporalDynamicsResult {
    pub fn empty() -> Self {
        Self {
            synchrony_index: 0.0,
            complexity_measure: 0.0,
            stability_measure: 0.0,
            propagation_speed: 0.0,
            oscillation_frequency: 0.0,
            autocorrelation: Array1::zeros(1),
            burst_statistics: BurstStatistics::default(),
            processing_time_ms: 0.0,
        }
    }
}

/// Statistics about burst patterns in neural activity
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BurstStatistics {
    /// Number of bursts detected
    pub burst_count: usize,
    /// Average burst duration in milliseconds
    pub average_duration_ms: f32,
    /// Average interval between bursts in milliseconds
    pub average_interval_ms: f32,
    /// Burst rate in Hz
    pub burst_rate_hz: f32,
}

/// Serde helper for Array1<f32> serialization
mod autocorr_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use ndarray::Array1;

    pub fn serialize<S>(array: &Array1<f32>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        array.to_vec().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Array1<f32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec = Vec::<f32>::deserialize(deserializer)?;
        Ok(Array1::from(vec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::{RateCodingEncoder, SpikeEncoder};
    use ndarray::array;

    #[test]
    fn test_temporal_window() {
        let mut window = TemporalWindow::new(10.0, 1.0);
        
        // Add some activity patterns
        window.add_activity(array![1.0, 0.0, 0.5]);
        window.add_activity(array![0.0, 1.0, 0.2]);
        window.add_activity(array![0.5, 0.5, 1.0]);

        let activity_matrix = window.get_activity_matrix();
        assert_eq!(activity_matrix.nrows(), 3);
        assert_eq!(activity_matrix.ncols(), 3);

        // Test autocorrelation
        let autocorr = window.calculate_autocorrelation(2);
        assert_eq!(autocorr.len(), 3); // lags 0, 1, 2
        assert!(autocorr[0] >= autocorr[1]); // Zero-lag should be highest
    }

    #[test]
    fn test_spike_dynamics_processing() {
        let mut dynamics = SpikeDynamics::new(20.0, 1.0);
        
        let encoder = RateCodingEncoder::new(5, 20.0);
        let embedding = array![0.8, 0.6, 0.4, 0.2, 0.0];
        let pattern = encoder.encode(embedding.view()).unwrap();

        let result = dynamics.process_pattern(&pattern).unwrap();
        
        assert!(result.synchrony_index >= 0.0 && result.synchrony_index <= 1.0);
        assert!(result.stability_measure >= 0.0 && result.stability_measure <= 1.0);
        assert!(result.complexity_measure >= 0.0);
        assert_eq!(result.processing_time_ms, 20.0);
    }

    #[test]
    fn test_burst_detection() {
        let dynamics = SpikeDynamics::new(50.0, 1.0);
        
        // Create a simple activity matrix with burst pattern
        let mut activity_matrix = Array2::zeros((10, 3));
        
        // First burst: high activity at times 1-3
        for t in 1..=3 {
            for n in 0..3 {
                activity_matrix[[t, n]] = 1.0;
            }
        }
        
        // Second burst: high activity at times 6-8
        for t in 6..=8 {
            for n in 0..3 {
                activity_matrix[[t, n]] = 0.8;
            }
        }

        let burst_stats = dynamics.detect_bursts(&activity_matrix);
        
        assert_eq!(burst_stats.burst_count, 2);
        assert!(burst_stats.average_duration_ms > 0.0);
        assert!(burst_stats.average_interval_ms > 0.0);
    }

    #[test]
    fn test_synchrony_calculation() {
        let dynamics = SpikeDynamics::new(10.0, 1.0);
        
        // Create perfectly synchronized activity
        let mut sync_matrix = Array2::zeros((5, 3));
        for t in 0..5 {
            for n in 0..3 {
                sync_matrix[[t, n]] = 1.0; // All neurons active together
            }
        }
        
        let sync_index = dynamics.calculate_synchrony_index(&sync_matrix);
        assert!(sync_index > 0.8); // Should be high synchrony

        // Create uncorrelated activity
        let mut async_matrix = Array2::zeros((5, 3));
        async_matrix[[0, 0]] = 1.0;
        async_matrix[[1, 1]] = 1.0;
        async_matrix[[2, 2]] = 1.0;
        
        let async_index = dynamics.calculate_synchrony_index(&async_matrix);
        assert!(async_index < sync_index); // Should be lower synchrony
    }
}