//! Pattern storage subsystem for Layer 5 PSR
//!
//! In-memory HashMap storage with CRUD operations and filtering.

use crate::pattern::Pattern;
use anyhow::{Result, anyhow};
use std::collections::HashMap;

/// In-memory pattern storage
pub struct PatternStorage {
    patterns: HashMap<String, Pattern>,
}

impl PatternStorage {
    /// Create new pattern storage
    pub fn new() -> Self {
        Self {
            patterns: HashMap::new(),
        }
    }

    /// Store a pattern (insert or update)
    pub fn store(&mut self, pattern: Pattern) -> Result<()> {
        self.patterns.insert(pattern.id.clone(), pattern);
        Ok(())
    }

    /// Get a pattern by ID
    pub fn get(&self, pattern_id: &str) -> Option<&Pattern> {
        self.patterns.get(pattern_id)
    }

    /// Get a mutable reference to a pattern
    pub fn get_mut(&mut self, pattern_id: &str) -> Option<&mut Pattern> {
        self.patterns.get_mut(pattern_id)
    }

    /// Delete a pattern by ID
    pub fn delete(&mut self, pattern_id: &str) -> Result<()> {
        self.patterns.remove(pattern_id)
            .ok_or_else(|| anyhow!("Pattern not found: {}", pattern_id))?;
        Ok(())
    }

    /// List patterns with optional filtering
    pub fn list(&self, min_activation_count: u64, limit: usize) -> Vec<Pattern> {
        let mut patterns: Vec<Pattern> = self.patterns
            .values()
            .filter(|p| p.activation_count >= min_activation_count)
            .cloned()
            .collect();

        // Sort by activation count (descending)
        patterns.sort_by(|a, b| b.activation_count.cmp(&a.activation_count));

        patterns.into_iter().take(limit).collect()
    }

    /// Update pattern statistics
    pub fn update_stats(&mut self, pattern_id: &str, activation_delta: u64, current_step: u64) -> Result<()> {
        let pattern = self.patterns.get_mut(pattern_id)
            .ok_or_else(|| anyhow!("Pattern not found: {}", pattern_id))?;

        pattern.update_stats(activation_delta, current_step);
        Ok(())
    }

    /// Get pattern count
    pub fn count(&self) -> usize {
        self.patterns.len()
    }

    /// Get all pattern IDs
    pub fn pattern_ids(&self) -> Vec<String> {
        self.patterns.keys().cloned().collect()
    }

    /// Check if pattern exists
    pub fn contains(&self, pattern_id: &str) -> bool {
        self.patterns.contains_key(pattern_id)
    }

    /// Clear all patterns
    pub fn clear(&mut self) {
        self.patterns.clear();
    }
}

impl Default for PatternStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{Pattern, PatternCategory};

    fn create_test_pattern(id: &str, name: &str, activation_count: u64) -> Pattern {
        let mut pattern = Pattern::new(
            id.to_string(),
            name.to_string(),
            PatternCategory::Transformational,
            vec![0.1; 256],
        );
        pattern.activation_count = activation_count;
        pattern
    }

    #[test]
    fn test_store_and_get() {
        let mut storage = PatternStorage::new();
        let pattern = create_test_pattern("p1", "Pattern 1", 0);

        storage.store(pattern.clone()).unwrap();
        assert_eq!(storage.count(), 1);

        let retrieved = storage.get("p1").unwrap();
        assert_eq!(retrieved.id, "p1");
        assert_eq!(retrieved.name, "Pattern 1");
    }

    #[test]
    fn test_delete() {
        let mut storage = PatternStorage::new();
        storage.store(create_test_pattern("p1", "Pattern 1", 0)).unwrap();

        assert_eq!(storage.count(), 1);
        storage.delete("p1").unwrap();
        assert_eq!(storage.count(), 0);
        assert!(storage.get("p1").is_none());
    }

    #[test]
    fn test_delete_nonexistent() {
        let mut storage = PatternStorage::new();
        let result = storage.delete("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_with_filter() {
        let mut storage = PatternStorage::new();

        storage.store(create_test_pattern("p1", "Pattern 1", 10)).unwrap();
        storage.store(create_test_pattern("p2", "Pattern 2", 5)).unwrap();
        storage.store(create_test_pattern("p3", "Pattern 3", 2)).unwrap();
        storage.store(create_test_pattern("p4", "Pattern 4", 15)).unwrap();

        let patterns = storage.list(5, 10);
        assert_eq!(patterns.len(), 3); // p1, p2, p4 (p3 has activation_count < 5)

        // Should be sorted by activation count descending
        assert_eq!(patterns[0].id, "p4"); // 15
        assert_eq!(patterns[1].id, "p1"); // 10
        assert_eq!(patterns[2].id, "p2"); // 5
    }

    #[test]
    fn test_list_with_limit() {
        let mut storage = PatternStorage::new();

        storage.store(create_test_pattern("p1", "Pattern 1", 10)).unwrap();
        storage.store(create_test_pattern("p2", "Pattern 2", 20)).unwrap();
        storage.store(create_test_pattern("p3", "Pattern 3", 30)).unwrap();

        let patterns = storage.list(0, 2);
        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0].id, "p3"); // Highest activation
        assert_eq!(patterns[1].id, "p2");
    }

    #[test]
    fn test_update_stats() {
        let mut storage = PatternStorage::new();
        storage.store(create_test_pattern("p1", "Pattern 1", 5)).unwrap();

        storage.update_stats("p1", 10, 100).unwrap();

        let pattern = storage.get("p1").unwrap();
        assert_eq!(pattern.activation_count, 15); // 5 + 10
        assert_eq!(pattern.last_used_step, 100);
    }

    #[test]
    fn test_update_stats_nonexistent() {
        let mut storage = PatternStorage::new();
        let result = storage.update_stats("nonexistent", 5, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_contains() {
        let mut storage = PatternStorage::new();
        storage.store(create_test_pattern("p1", "Pattern 1", 0)).unwrap();

        assert!(storage.contains("p1"));
        assert!(!storage.contains("p2"));
    }

    #[test]
    fn test_pattern_ids() {
        let mut storage = PatternStorage::new();
        storage.store(create_test_pattern("p1", "Pattern 1", 0)).unwrap();
        storage.store(create_test_pattern("p2", "Pattern 2", 0)).unwrap();

        let ids = storage.pattern_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"p1".to_string()));
        assert!(ids.contains(&"p2".to_string()));
    }

    #[test]
    fn test_clear() {
        let mut storage = PatternStorage::new();
        storage.store(create_test_pattern("p1", "Pattern 1", 0)).unwrap();
        storage.store(create_test_pattern("p2", "Pattern 2", 0)).unwrap();

        assert_eq!(storage.count(), 2);
        storage.clear();
        assert_eq!(storage.count(), 0);
    }
}
