// Layer 4 CPE - Temporal Pattern Analysis
// Analyzes memory access sequences to detect patterns and predict future accesses

use mfn_core::{MemoryId, Weight, current_timestamp};
use mfn_core::memory_types::Timestamp;
use std::collections::{HashMap, VecDeque};
use serde::{Deserialize, Serialize};
use ndarray::{Array1, Array2};
use nalgebra::{DMatrix, DVector};

/// Temporal pattern analyzer that detects recurring sequences in memory access
#[derive(Debug)]
pub struct TemporalAnalyzer {
    /// Configuration parameters
    config: TemporalConfig,
    
    /// Sliding window of recent memory accesses
    access_window: VecDeque<MemoryAccess>,
    
    /// Detected patterns indexed by pattern signature
    patterns: HashMap<PatternSignature, TemporalPattern>,
    
    /// N-gram frequency analysis for different sequence lengths
    ngram_frequencies: HashMap<usize, HashMap<Vec<MemoryId>, FrequencyData>>,
    
    /// Markov chain transition probabilities
    transition_matrix: HashMap<MemoryId, HashMap<MemoryId, TransitionData>>,
    
    /// Statistical models for temporal intervals
    interval_models: HashMap<PatternType, IntervalModel>,
    
    /// Pattern matching state machine
    matcher: PatternMatcher,
}

/// Configuration for temporal analysis
#[derive(Debug, Clone)]
pub struct TemporalConfig {
    /// Maximum size of the access window
    pub max_window_size: usize,
    
    /// Minimum occurrences to consider a pattern significant
    pub min_pattern_occurrences: u32,
    
    /// Maximum N-gram length to analyze
    pub max_ngram_length: usize,
    
    /// Minimum confidence threshold for predictions
    pub min_prediction_confidence: f64,
    
    /// Time decay rate for pattern relevance (per hour)
    pub pattern_decay_rate: f64,
    
    /// Maximum time gap to consider accesses as related (microseconds)
    pub max_sequence_gap_us: u64,
    
    /// Enable advanced statistical analysis
    pub enable_statistical_modeling: bool,
}

impl Default for TemporalConfig {
    fn default() -> Self {
        Self {
            max_window_size: 10000,
            min_pattern_occurrences: 3,
            max_ngram_length: 8,
            min_prediction_confidence: 0.3,
            pattern_decay_rate: 0.1, // 10% decay per hour
            max_sequence_gap_us: 60_000_000, // 1 minute
            enable_statistical_modeling: true,
        }
    }
}

/// Memory access event with rich context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAccess {
    pub memory_id: MemoryId,
    pub timestamp: Timestamp,
    pub access_type: AccessType,
    pub user_context: Option<String>,
    pub session_id: Option<String>,
    pub confidence: Weight,
}

/// Type of memory access
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AccessType {
    Read,
    Write,
    Search,
    Association,
    Prediction,
}

/// Detected temporal pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalPattern {
    pub id: String,
    pub signature: PatternSignature,
    pub sequence: Vec<MemoryId>,
    pub pattern_type: PatternType,
    pub confidence: Weight,
    pub occurrences: u32,
    pub average_interval_us: u64,
    pub interval_variance: f64,
    pub last_occurrence: Timestamp,
    pub created_at: Timestamp,
    pub context_features: Vec<ContextFeature>,
}

/// Pattern signature for efficient matching
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatternSignature {
    pub sequence_hash: u64,
    pub length: usize,
    pub access_types: Vec<AccessType>,
}

/// Type of temporal pattern detected
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PatternType {
    /// Fixed sequence that repeats exactly
    ExactSequence,
    /// Sequence with minor variations
    ApproximateSequence,
    /// Periodic pattern with regular timing
    PeriodicPattern,
    /// Burst of related accesses
    BurstPattern,
    /// Session-based pattern
    SessionPattern,
    /// Context-dependent pattern
    ConditionalPattern,
}

/// Context feature extracted from access patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFeature {
    pub feature_type: String,
    pub value: f64,
    pub weight: Weight,
}

/// Frequency data for N-gram analysis
#[derive(Debug, Clone)]
struct FrequencyData {
    count: u32,
    last_seen: Timestamp,
    intervals: Vec<u64>,
    contexts: Vec<String>,
}

/// Transition data for Markov chain
#[derive(Debug, Clone)]
struct TransitionData {
    count: u32,
    probability: f64,
    average_interval: u64,
    confidence: Weight,
}

/// Statistical model for interval prediction
#[derive(Debug, Clone)]
struct IntervalModel {
    mean: f64,
    variance: f64,
    distribution_type: DistributionType,
    parameters: Vec<f64>,
}

#[derive(Debug, Clone)]
enum DistributionType {
    Normal,
    Exponential,
    Gamma,
    Weibull,
}

/// Pattern matching state machine
#[derive(Debug)]
struct PatternMatcher {
    active_matches: HashMap<PatternSignature, MatchState>,
    completed_matches: Vec<CompletedMatch>,
}

#[derive(Debug)]
struct MatchState {
    pattern_id: String,
    matched_positions: usize,
    start_timestamp: Timestamp,
    partial_sequence: Vec<MemoryId>,
}

#[derive(Debug)]
struct CompletedMatch {
    pattern_id: String,
    sequence: Vec<MemoryId>,
    start_time: Timestamp,
    end_time: Timestamp,
    confidence: Weight,
}

impl TemporalAnalyzer {
    /// Create a new temporal analyzer
    pub fn new(config: TemporalConfig) -> Self {
        Self {
            config,
            access_window: VecDeque::new(),
            patterns: HashMap::new(),
            ngram_frequencies: HashMap::new(),
            transition_matrix: HashMap::new(),
            interval_models: HashMap::new(),
            matcher: PatternMatcher {
                active_matches: HashMap::new(),
                completed_matches: Vec::new(),
            },
        }
    }

    /// Add a new memory access and update pattern analysis
    pub fn add_access(&mut self, access: MemoryAccess) {
        // Add to sliding window
        self.access_window.push_back(access.clone());
        
        // Maintain window size
        while self.access_window.len() > self.config.max_window_size {
            self.access_window.pop_front();
        }

        // Update N-gram analysis
        self.update_ngram_analysis(&access);
        
        // Update Markov chain
        self.update_transition_matrix(&access);
        
        // Update pattern matching
        self.update_pattern_matching(&access);
        
        // Detect new patterns
        if self.access_window.len() >= 3 {
            self.detect_patterns();
        }
        
        // Update statistical models
        if self.config.enable_statistical_modeling {
            self.update_statistical_models();
        }
    }

    /// Predict next likely memory accesses
    pub fn predict_next(&self, context: &PredictionContext) -> Vec<PredictionResult> {
        let mut predictions = Vec::new();
        
        // N-gram based predictions
        predictions.extend(self.ngram_predictions(context));
        
        // Markov chain predictions
        predictions.extend(self.markov_predictions(context));
        
        // Pattern completion predictions
        predictions.extend(self.pattern_completion_predictions(context));
        
        // Statistical model predictions
        if self.config.enable_statistical_modeling {
            predictions.extend(self.statistical_predictions(context));
        }
        
        // Merge and rank predictions
        self.merge_and_rank_predictions(predictions)
    }

    /// Get detected patterns matching criteria
    pub fn get_patterns(&self, pattern_type: Option<PatternType>) -> Vec<&TemporalPattern> {
        self.patterns
            .values()
            .filter(|p| pattern_type.map_or(true, |pt| p.pattern_type == pt))
            .filter(|p| p.confidence >= self.config.min_prediction_confidence)
            .collect()
    }

    /// Get current analyzer statistics
    pub fn get_statistics(&self) -> AnalyzerStatistics {
        AnalyzerStatistics {
            total_accesses: self.access_window.len(),
            total_patterns: self.patterns.len(),
            active_matches: self.matcher.active_matches.len(),
            ngram_orders: self.ngram_frequencies.keys().cloned().collect(),
            average_pattern_confidence: self.calculate_average_confidence(),
            memory_usage_estimate: self.estimate_memory_usage(),
        }
    }

    fn update_ngram_analysis(&mut self, access: &MemoryAccess) {
        if self.access_window.len() < 2 {
            return;
        }

        for n in 2..=self.config.max_ngram_length.min(self.access_window.len()) {
            let ngram: Vec<MemoryId> = self.access_window
                .iter()
                .rev()
                .take(n)
                .map(|a| a.memory_id)
                .collect();

            let frequencies = self.ngram_frequencies.entry(n).or_insert_with(HashMap::new);
            let freq_data = frequencies.entry(ngram).or_insert_with(|| FrequencyData {
                count: 0,
                last_seen: 0,
                intervals: Vec::new(),
                contexts: Vec::new(),
            });

            // Update frequency data
            if freq_data.count > 0 {
                let interval = access.timestamp.saturating_sub(freq_data.last_seen);
                freq_data.intervals.push(interval);
            }

            freq_data.count += 1;
            freq_data.last_seen = access.timestamp;

            if let Some(context) = &access.user_context {
                freq_data.contexts.push(context.clone());
            }
        }
    }

    fn update_transition_matrix(&mut self, access: &MemoryAccess) {
        if let Some(previous_access) = self.access_window.iter().rev().nth(1) {
            let transitions = self.transition_matrix
                .entry(previous_access.memory_id)
                .or_insert_with(HashMap::new);

            let transition_data = transitions
                .entry(access.memory_id)
                .or_insert_with(|| TransitionData {
                    count: 0,
                    probability: 0.0,
                    average_interval: 0,
                    confidence: 0.0,
                });

            transition_data.count += 1;
            
            let interval = access.timestamp.saturating_sub(previous_access.timestamp);
            transition_data.average_interval = 
                (transition_data.average_interval + interval) / 2;
        }

        // Recalculate probabilities
        self.recalculate_transition_probabilities();
    }

    fn recalculate_transition_probabilities(&mut self) {
        for (_, transitions) in &mut self.transition_matrix {
            let total_count: u32 = transitions.values().map(|t| t.count).sum();
            
            for transition in transitions.values_mut() {
                transition.probability = transition.count as f64 / total_count as f64;
                transition.confidence = (transition.count as f64 / total_count as f64).min(1.0);
            }
        }
    }

    fn update_pattern_matching(&mut self, access: &MemoryAccess) {
        // Update active matches
        let mut completed_matches = Vec::new();
        
        for pattern in self.patterns.values() {
            if let Some(match_state) = self.matcher.active_matches.get_mut(&pattern.signature) {
                if pattern.sequence[match_state.matched_positions] == access.memory_id {
                    match_state.matched_positions += 1;
                    match_state.partial_sequence.push(access.memory_id);
                    
                    // Check if pattern is complete
                    if match_state.matched_positions >= pattern.sequence.len() {
                        completed_matches.push(CompletedMatch {
                            pattern_id: pattern.id.clone(),
                            sequence: match_state.partial_sequence.clone(),
                            start_time: match_state.start_timestamp,
                            end_time: access.timestamp,
                            confidence: pattern.confidence,
                        });
                    }
                }
            }
        }

        // Handle completed matches
        for completed in completed_matches {
            self.matcher.active_matches.remove(&self.patterns[&completed.pattern_id].signature);
            self.matcher.completed_matches.push(completed);
            
            // Limit completed matches history
            if self.matcher.completed_matches.len() > 1000 {
                self.matcher.completed_matches.truncate(500);
            }
        }

        // Start new potential matches
        for pattern in self.patterns.values() {
            if pattern.sequence[0] == access.memory_id {
                self.matcher.active_matches.insert(
                    pattern.signature.clone(),
                    MatchState {
                        pattern_id: pattern.id.clone(),
                        matched_positions: 1,
                        start_timestamp: access.timestamp,
                        partial_sequence: vec![access.memory_id],
                    },
                );
            }
        }
    }

    fn detect_patterns(&mut self) {
        // Analyze recent access window for new patterns
        let window_size = self.access_window.len().min(50); // Analyze last 50 accesses
        let recent_accesses: Vec<MemoryAccess> = self.access_window
            .iter()
            .rev()
            .take(window_size)
            .cloned()
            .collect();

        // Look for repeating subsequences
        for length in 3..=8 {
            if length * 2 > recent_accesses.len() {
                continue;
            }

            for start in 0..=recent_accesses.len() - length * 2 {
                let sequence1: Vec<MemoryId> = recent_accesses[start..start + length]
                    .iter()
                    .map(|a| a.memory_id)
                    .collect();
                
                let sequence2: Vec<MemoryId> = recent_accesses[start + length..start + length * 2]
                    .iter()
                    .map(|a| a.memory_id)
                    .collect();

                if sequence1 == sequence2 {
                    let sequence_slice: Vec<&MemoryAccess> = recent_accesses[start..start + length]
                        .iter()
                        .collect();
                    self.register_new_pattern(sequence1, &sequence_slice);
                }
            }
        }
    }

    fn register_new_pattern(&mut self, sequence: Vec<MemoryId>, accesses: &[&MemoryAccess]) {
        let signature = PatternSignature {
            sequence_hash: self.calculate_sequence_hash(&sequence),
            length: sequence.len(),
            access_types: accesses.iter().map(|a| a.access_type).collect(),
        };

        if self.patterns.contains_key(&signature) {
            // Update existing pattern
            if let Some(pattern) = self.patterns.get_mut(&signature) {
                pattern.occurrences += 1;
                pattern.last_occurrence = current_timestamp();
                pattern.confidence = (pattern.occurrences as f64 / 
                    (pattern.occurrences + 1) as f64).min(1.0);
            }
        } else if accesses.len() >= 3 {
            // Create new pattern
            let intervals: Vec<u64> = accesses.windows(2)
                .map(|w| w[1].timestamp.saturating_sub(w[0].timestamp))
                .collect();

            let average_interval = if intervals.is_empty() {
                0
            } else {
                intervals.iter().sum::<u64>() / intervals.len() as u64
            };

            let pattern = TemporalPattern {
                id: format!("pattern_{}", uuid::Uuid::new_v4()),
                signature: signature.clone(),
                sequence: sequence.clone(),
                pattern_type: PatternType::ExactSequence,
                confidence: 0.5, // Initial confidence
                occurrences: 1,
                average_interval_us: average_interval,
                interval_variance: self.calculate_interval_variance(&intervals, average_interval),
                last_occurrence: current_timestamp(),
                created_at: current_timestamp(),
                context_features: self.extract_context_features(accesses),
            };

            self.patterns.insert(signature, pattern);
        }
    }

    fn calculate_sequence_hash(&self, sequence: &[MemoryId]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        sequence.hash(&mut hasher);
        hasher.finish()
    }

    fn calculate_interval_variance(&self, intervals: &[u64], mean: u64) -> f64 {
        if intervals.is_empty() {
            return 0.0;
        }

        let variance = intervals.iter()
            .map(|&interval| {
                let diff = interval as f64 - mean as f64;
                diff * diff
            })
            .sum::<f64>() / intervals.len() as f64;

        variance
    }

    fn extract_context_features(&self, accesses: &[&MemoryAccess]) -> Vec<ContextFeature> {
        let mut features = Vec::new();

        // Time-based features
        let timestamps: Vec<u64> = accesses.iter().map(|a| a.timestamp).collect();
        if let (Some(&first), Some(&last)) = (timestamps.first(), timestamps.last()) {
            features.push(ContextFeature {
                feature_type: "duration".to_string(),
                value: (last - first) as f64,
                weight: 0.7,
            });
        }

        // Access type distribution
        let mut access_type_counts = HashMap::new();
        for access in accesses {
            *access_type_counts.entry(access.access_type).or_insert(0u32) += 1;
        }

        for (access_type, count) in access_type_counts {
            features.push(ContextFeature {
                feature_type: format!("access_type_{:?}", access_type),
                value: count as f64 / accesses.len() as f64,
                weight: 0.5,
            });
        }

        features
    }

    fn update_statistical_models(&mut self) {
        // Update interval models for each pattern type
        for pattern_type in [
            PatternType::ExactSequence,
            PatternType::ApproximateSequence,
            PatternType::PeriodicPattern,
            PatternType::BurstPattern,
        ] {
            let intervals: Vec<u64> = self.patterns
                .values()
                .filter(|p| p.pattern_type == pattern_type)
                .map(|p| p.average_interval_us)
                .collect();

            if !intervals.is_empty() {
                let model = self.fit_interval_model(&intervals);
                self.interval_models.insert(pattern_type, model);
            }
        }
    }

    fn fit_interval_model(&self, intervals: &[u64]) -> IntervalModel {
        let mean = intervals.iter().sum::<u64>() as f64 / intervals.len() as f64;
        let variance = intervals.iter()
            .map(|&x| (x as f64 - mean).powi(2))
            .sum::<f64>() / intervals.len() as f64;

        IntervalModel {
            mean,
            variance,
            distribution_type: DistributionType::Normal, // Simplified for now
            parameters: vec![mean, variance.sqrt()],
        }
    }

    fn ngram_predictions(&self, context: &PredictionContext) -> Vec<PredictionResult> {
        let mut predictions = Vec::new();

        if let Some(recent_sequence) = context.recent_sequence.as_ref() {
            for n in (2..=self.config.max_ngram_length).rev() {
                if recent_sequence.len() >= n - 1 {
                    let prefix: Vec<MemoryId> = recent_sequence
                        .iter()
                        .rev()
                        .take(n - 1)
                        .cloned()
                        .collect();

                    if let Some(frequencies) = self.ngram_frequencies.get(&n) {
                        for (ngram, freq_data) in frequencies {
                            if ngram.starts_with(&prefix) && ngram.len() == prefix.len() + 1 {
                                let next_memory = ngram[prefix.len()];
                                let confidence = (freq_data.count as f64 / 
                                    self.calculate_total_ngram_count(n) as f64).min(1.0);

                                predictions.push(PredictionResult {
                                    memory_id: next_memory,
                                    confidence,
                                    prediction_type: PredictionType::NGramBased,
                                    estimated_time_us: freq_data.intervals
                                        .last()
                                        .copied()
                                        .unwrap_or(0),
                                    contributing_evidence: vec![
                                        format!("N-gram order: {}", n),
                                        format!("Occurrences: {}", freq_data.count),
                                    ],
                                });
                            }
                        }
                    }
                }
            }
        }

        predictions
    }

    fn markov_predictions(&self, context: &PredictionContext) -> Vec<PredictionResult> {
        let mut predictions = Vec::new();

        if let Some(last_memory) = context.recent_sequence.as_ref().and_then(|s| s.last()) {
            if let Some(transitions) = self.transition_matrix.get(last_memory) {
                for (next_memory, transition_data) in transitions {
                    if transition_data.confidence >= self.config.min_prediction_confidence {
                        predictions.push(PredictionResult {
                            memory_id: *next_memory,
                            confidence: transition_data.confidence,
                            prediction_type: PredictionType::MarkovChain,
                            estimated_time_us: transition_data.average_interval,
                            contributing_evidence: vec![
                                format!("Transition count: {}", transition_data.count),
                                format!("Probability: {:.3}", transition_data.probability),
                            ],
                        });
                    }
                }
            }
        }

        predictions
    }

    fn pattern_completion_predictions(&self, context: &PredictionContext) -> Vec<PredictionResult> {
        let mut predictions = Vec::new();

        if let Some(recent_sequence) = context.recent_sequence.as_ref() {
            for pattern in self.patterns.values() {
                // Check if recent sequence matches beginning of any pattern
                if recent_sequence.len() < pattern.sequence.len() {
                    let matches = recent_sequence.iter()
                        .zip(pattern.sequence.iter())
                        .all(|(a, b)| a == b);

                    if matches {
                        let next_memory = pattern.sequence[recent_sequence.len()];
                        predictions.push(PredictionResult {
                            memory_id: next_memory,
                            confidence: pattern.confidence * 0.8, // Slight penalty for incomplete match
                            prediction_type: PredictionType::PatternCompletion,
                            estimated_time_us: pattern.average_interval_us,
                            contributing_evidence: vec![
                                format!("Pattern: {}", pattern.id),
                                format!("Pattern occurrences: {}", pattern.occurrences),
                            ],
                        });
                    }
                }
            }
        }

        predictions
    }

    fn statistical_predictions(&self, _context: &PredictionContext) -> Vec<PredictionResult> {
        // TODO: Implement statistical model predictions
        Vec::new()
    }

    fn merge_and_rank_predictions(&self, predictions: Vec<PredictionResult>) -> Vec<PredictionResult> {
        let mut predictions = predictions;
        // Group by memory_id and merge confidences
        let mut merged = HashMap::new();
        
        for pred in predictions {
            let entry = merged.entry(pred.memory_id).or_insert_with(|| pred.clone());
            entry.confidence = (entry.confidence + pred.confidence) / 2.0; // Simple averaging
            entry.contributing_evidence.extend(pred.contributing_evidence);
        }

        // Convert back to vec and sort by confidence
        let mut results: Vec<PredictionResult> = merged.into_values().collect();
        results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Filter by minimum confidence
        results.retain(|p| p.confidence >= self.config.min_prediction_confidence);

        results
    }

    fn calculate_total_ngram_count(&self, n: usize) -> u32 {
        self.ngram_frequencies.get(&n)
            .map(|freq| freq.values().map(|f| f.count).sum())
            .unwrap_or(0)
    }

    fn calculate_average_confidence(&self) -> f64 {
        if self.patterns.is_empty() {
            return 0.0;
        }
        
        self.patterns.values().map(|p| p.confidence).sum::<f64>() / self.patterns.len() as f64
    }

    fn estimate_memory_usage(&self) -> usize {
        std::mem::size_of_val(self) +
        self.access_window.capacity() * std::mem::size_of::<MemoryAccess>() +
        self.patterns.len() * 256 + // Rough estimate
        self.ngram_frequencies.len() * 128
    }
}

/// Context for making predictions
#[derive(Debug, Clone)]
pub struct PredictionContext {
    pub recent_sequence: Option<Vec<MemoryId>>,
    pub current_timestamp: Timestamp,
    pub user_context: Option<String>,
    pub session_id: Option<String>,
    pub max_predictions: usize,
}

/// Result of a prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResult {
    pub memory_id: MemoryId,
    pub confidence: Weight,
    pub prediction_type: PredictionType,
    pub estimated_time_us: u64,
    pub contributing_evidence: Vec<String>,
}

/// Type of prediction method used
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PredictionType {
    NGramBased,
    MarkovChain,
    PatternCompletion,
    StatisticalModel,
    HybridEnsemble,
}

/// Statistics about the temporal analyzer
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzerStatistics {
    pub total_accesses: usize,
    pub total_patterns: usize,
    pub active_matches: usize,
    pub ngram_orders: Vec<usize>,
    pub average_pattern_confidence: f64,
    pub memory_usage_estimate: usize,
}

// Helper function to generate UUIDs (simple implementation)
mod uuid {
    use rand::Rng;
    
    pub struct Uuid(String);
    
    impl Uuid {
        pub fn new_v4() -> Self {
            let mut rng = rand::thread_rng();
            let uuid = format!(
                "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                rng.gen::<u32>(),
                rng.gen::<u16>(),
                rng.gen::<u16>() & 0x0fff | 0x4000,
                rng.gen::<u16>() & 0x3fff | 0x8000,
                rng.gen::<u64>() & 0xffffffffffff
            );
            Uuid(uuid)
        }
    }
    
    impl std::fmt::Display for Uuid {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.0)
        }
    }
}