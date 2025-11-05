//! Layer 4: Context Prediction Engine (CPE)
//!
//! Predicts next memory accesses based on access sequence patterns.
//! Simple n-gram based prediction using recent access history.

use crate::types::{MemoryId, Query, SearchResult, Layer};
use anyhow::{Result, anyhow};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};

/// Context prediction engine for sequence-based prediction
pub struct ContextPredictor {
    /// Recent access sequence (circular buffer)
    access_history: RwLock<VecDeque<MemoryId>>,

    /// Sequence patterns: sequence -> next memory ID -> frequency
    /// Maps [id1, id2, ...] -> (next_id -> count)
    patterns: RwLock<HashMap<Vec<MemoryId>, HashMap<MemoryId, u32>>>,

    /// Maximum window size for access history
    window_size: usize,

    /// Minimum pattern length (n-gram size)
    min_pattern_length: usize,

    /// Maximum pattern length
    max_pattern_length: usize,

    /// Total sequences tracked
    total_sequences: RwLock<u64>,
}

impl ContextPredictor {
    /// Create a new context predictor
    ///
    /// # Arguments
    /// * `window_size` - Maximum number of recent accesses to track
    ///
    /// # Example
    /// ```
    /// use mfn_monolith::layer4::ContextPredictor;
    ///
    /// let predictor = ContextPredictor::new(1000).unwrap();
    /// ```
    pub fn new(window_size: usize) -> Result<Self> {
        if window_size == 0 {
            return Err(anyhow!("window_size must be positive"));
        }

        Ok(Self {
            access_history: RwLock::new(VecDeque::with_capacity(window_size)),
            patterns: RwLock::new(HashMap::new()),
            window_size,
            min_pattern_length: 2,
            max_pattern_length: 5,
            total_sequences: RwLock::new(0),
        })
    }

    /// Predict next likely memories based on query context
    ///
    /// Uses recent access patterns to predict what memory will be accessed next.
    ///
    /// # Arguments
    /// * `query` - Query context (not heavily used yet)
    /// * `max_predictions` - Maximum number of predictions to return
    ///
    /// # Returns
    /// Vector of predicted SearchResults, sorted by confidence
    pub fn predict(&self, _query: &Query, max_predictions: usize) -> Vec<SearchResult> {
        let history = self.access_history.read();

        if history.is_empty() {
            return Vec::new();
        }

        // Get recent sequence (last N accesses)
        let recent_len = self.max_pattern_length.min(history.len());
        let recent_sequence: Vec<MemoryId> = history
            .iter()
            .rev()
            .take(recent_len)
            .rev()
            .copied()
            .collect();

        drop(history);

        if recent_sequence.len() < self.min_pattern_length {
            return Vec::new();
        }

        // Try to match patterns from longest to shortest
        let patterns = self.patterns.read();
        let mut predictions = Vec::new();

        for pattern_len in (self.min_pattern_length..=recent_sequence.len()).rev() {
            let pattern = &recent_sequence[recent_sequence.len() - pattern_len..];

            if let Some(next_map) = patterns.get(pattern) {
                // Found matching pattern, convert to predictions
                let total_count: u32 = next_map.values().sum();

                for (memory_id, &count) in next_map.iter() {
                    let confidence = (count as f64) / (total_count as f64);
                    predictions.push(SearchResult {
                        memory_id: *memory_id,
                        score: confidence,
                        layer: Layer::L4Context,
                        content: format!("Predicted memory (confidence: {:.2})", confidence),
                    });
                }

                // Stop after finding first matching pattern
                break;
            }
        }

        // Sort by confidence (descending)
        predictions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Return top predictions
        predictions.truncate(max_predictions);
        predictions
    }

    /// Add a memory access to the sequence
    ///
    /// Updates access history and learns patterns.
    ///
    /// # Arguments
    /// * `memory_id` - ID of the memory that was accessed
    ///
    /// # Example
    /// ```
    /// use mfn_monolith::layer4::ContextPredictor;
    /// use uuid::Uuid;
    ///
    /// let mut predictor = ContextPredictor::new(1000).unwrap();
    /// predictor.add_sequence(Uuid::new_v4());
    /// ```
    pub fn add_sequence(&mut self, memory_id: MemoryId) {
        let mut history = self.access_history.write();

        // If history is not empty, learn patterns before adding new access
        if !history.is_empty() {
            self.learn_patterns(&history, memory_id);
        }

        // Add to history
        history.push_back(memory_id);

        // Maintain window size
        if history.len() > self.window_size {
            history.pop_front();
        }

        drop(history);

        // Increment total sequences
        let mut total = self.total_sequences.write();
        *total += 1;
    }

    /// Learn patterns from current history and new access
    fn learn_patterns(&self, history: &VecDeque<MemoryId>, next_id: MemoryId) {
        let history_vec: Vec<MemoryId> = history.iter().copied().collect();

        if history_vec.is_empty() {
            return;
        }

        let mut patterns = self.patterns.write();

        // Extract patterns of different lengths
        for pattern_len in self.min_pattern_length..=self.max_pattern_length {
            if history_vec.len() >= pattern_len {
                // Get the last pattern_len items as the pattern
                let start_idx = history_vec.len() - pattern_len;
                let pattern: Vec<MemoryId> = history_vec[start_idx..].to_vec();

                // Update pattern -> next_id mapping
                let next_map = patterns.entry(pattern).or_insert_with(HashMap::new);
                *next_map.entry(next_id).or_insert(0) += 1;
            }
        }
    }

    /// Get the number of sequences tracked
    pub fn sequence_count(&self) -> usize {
        *self.total_sequences.read() as usize
    }

    /// Get the number of patterns learned
    pub fn pattern_count(&self) -> usize {
        self.patterns.read().len()
    }

    /// Get current window size
    pub fn current_window_size(&self) -> usize {
        self.access_history.read().len()
    }

    /// Clear all patterns and history
    pub fn clear(&mut self) {
        let mut history = self.access_history.write();
        history.clear();
        drop(history);

        let mut patterns = self.patterns.write();
        patterns.clear();
        drop(patterns);

        let mut total = self.total_sequences.write();
        *total = 0;
    }
}

// Implement Clone for use in parallel queries
impl Clone for ContextPredictor {
    fn clone(&self) -> Self {
        Self {
            access_history: RwLock::new(self.access_history.read().clone()),
            patterns: RwLock::new(self.patterns.read().clone()),
            window_size: self.window_size,
            min_pattern_length: self.min_pattern_length,
            max_pattern_length: self.max_pattern_length,
            total_sequences: RwLock::new(*self.total_sequences.read()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Query;
    use uuid::Uuid;

    #[test]
    fn test_new_predictor() {
        let predictor = ContextPredictor::new(1000).unwrap();
        assert_eq!(predictor.sequence_count(), 0);
        assert_eq!(predictor.pattern_count(), 0);
    }

    #[test]
    fn test_new_predictor_zero_window() {
        let result = ContextPredictor::new(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_sequence() {
        let mut predictor = ContextPredictor::new(100).unwrap();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        predictor.add_sequence(id1);
        assert_eq!(predictor.sequence_count(), 1);

        predictor.add_sequence(id2);
        assert_eq!(predictor.sequence_count(), 2);
    }

    #[test]
    fn test_pattern_learning() {
        let mut predictor = ContextPredictor::new(100).unwrap();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // Create sequence: id1 -> id2 -> id3
        predictor.add_sequence(id1);
        predictor.add_sequence(id2);
        predictor.add_sequence(id3);

        // Should have learned pattern [id1, id2] -> id3
        assert!(predictor.pattern_count() > 0);
    }

    #[test]
    fn test_prediction() {
        let mut predictor = ContextPredictor::new(100).unwrap();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // Teach pattern: id1 -> id2 -> id3 (multiple times)
        for _ in 0..5 {
            predictor.add_sequence(id1);
            predictor.add_sequence(id2);
            predictor.add_sequence(id3);
        }

        // Now access id1 -> id2, should predict id3
        let mut new_predictor = ContextPredictor::new(100).unwrap();
        new_predictor.patterns = RwLock::new(predictor.patterns.read().clone());

        let mut history = new_predictor.access_history.write();
        history.push_back(id1);
        history.push_back(id2);
        drop(history);

        let query = Query::new("test");
        let predictions = new_predictor.predict(&query, 5);

        assert!(!predictions.is_empty());
        assert_eq!(predictions[0].memory_id, id3);
        assert_eq!(predictions[0].layer, Layer::L4Context);
    }

    #[test]
    fn test_window_size_enforcement() {
        let mut predictor = ContextPredictor::new(3).unwrap();

        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();

        for &id in &ids {
            predictor.add_sequence(id);
        }

        // Window should be limited to 3
        assert_eq!(predictor.current_window_size(), 3);
    }

    #[test]
    fn test_clear() {
        let mut predictor = ContextPredictor::new(100).unwrap();

        predictor.add_sequence(Uuid::new_v4());
        predictor.add_sequence(Uuid::new_v4());
        predictor.add_sequence(Uuid::new_v4());

        assert!(predictor.sequence_count() > 0);

        predictor.clear();

        assert_eq!(predictor.sequence_count(), 0);
        assert_eq!(predictor.pattern_count(), 0);
        assert_eq!(predictor.current_window_size(), 0);
    }

    #[test]
    fn test_empty_prediction() {
        let predictor = ContextPredictor::new(100).unwrap();
        let query = Query::new("test");

        let predictions = predictor.predict(&query, 10);
        assert!(predictions.is_empty());
    }

    #[test]
    fn test_multiple_pattern_lengths() {
        let mut predictor = ContextPredictor::new(100).unwrap();

        let ids: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();

        // Create longer sequence
        for &id in &ids {
            predictor.add_sequence(id);
        }

        // Should learn patterns of different lengths (2-5)
        let pattern_count = predictor.pattern_count();
        assert!(pattern_count > 0, "Should have learned some patterns");
        println!("Learned {} patterns from 10 sequences", pattern_count);
    }

    #[test]
    fn test_prediction_confidence() {
        let mut predictor = ContextPredictor::new(100).unwrap();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let id4 = Uuid::new_v4();

        // Teach pattern with varying frequencies
        // id1 -> id2 -> id3 (3 times)
        for _ in 0..3 {
            predictor.add_sequence(id1);
            predictor.add_sequence(id2);
            predictor.add_sequence(id3);
        }

        // id1 -> id2 -> id4 (1 time)
        predictor.add_sequence(id1);
        predictor.add_sequence(id2);
        predictor.add_sequence(id4);

        // Simulate having id1 -> id2 in history
        let mut new_predictor = ContextPredictor::new(100).unwrap();
        new_predictor.patterns = RwLock::new(predictor.patterns.read().clone());

        let mut history = new_predictor.access_history.write();
        history.push_back(id1);
        history.push_back(id2);
        drop(history);

        let query = Query::new("test");
        let predictions = new_predictor.predict(&query, 5);

        assert_eq!(predictions.len(), 2);
        assert_eq!(predictions[0].memory_id, id3);
        assert!(predictions[0].score > predictions[1].score, "More frequent pattern should have higher confidence");
        assert!(predictions[0].score > 0.5, "id3 appeared 3/4 times, should be >0.5 confidence");
    }

    #[test]
    fn test_clone() {
        let mut predictor = ContextPredictor::new(100).unwrap();

        predictor.add_sequence(Uuid::new_v4());
        predictor.add_sequence(Uuid::new_v4());

        let cloned = predictor.clone();

        assert_eq!(cloned.sequence_count(), predictor.sequence_count());
        assert_eq!(cloned.pattern_count(), predictor.pattern_count());
        assert_eq!(cloned.current_window_size(), predictor.current_window_size());
    }
}
