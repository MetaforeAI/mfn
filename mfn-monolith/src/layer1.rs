//! Layer 1: Immediate Factual Recall (IFR)
//!
//! Ultra-fast exact matching using concurrent hash map.
//! Target: <1µs lookup time (no IPC overhead)
//!
//! This layer provides O(1) exact match lookups for queries that have been seen before.
//! It uses DashMap for lock-free concurrent access and ahash for fast non-cryptographic hashing.

use crate::types::{Memory, Query};
use ahash::AHasher;
use dashmap::DashMap;
use std::hash::{Hash, Hasher};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Layer1Error {
    #[error("Cache capacity exceeded: {current} >= {max}")]
    CapacityExceeded { current: usize, max: usize },

    #[error("Invalid capacity: {0}")]
    InvalidCapacity(usize),
}

pub type Result<T> = std::result::Result<T, Layer1Error>;

/// ExactMatchCache provides O(1) exact match lookups for queries
///
/// Uses DashMap for lock-free concurrent access and ahash for fast hashing.
/// Thread-safe and designed for sub-microsecond lookups.
#[derive(Debug, Clone)]
pub struct ExactMatchCache {
    /// Concurrent hash map: query_hash -> Memory
    cache: DashMap<u64, Memory>,

    /// Maximum cache capacity
    capacity: usize,
}

impl ExactMatchCache {
    /// Create a new ExactMatchCache with the specified capacity
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of memories to store
    ///
    /// # Returns
    /// * `Ok(Self)` - New cache instance
    /// * `Err(Layer1Error::InvalidCapacity)` - If capacity is 0
    ///
    /// # Example
    /// ```
    /// use mfn_monolith::layer1::ExactMatchCache;
    ///
    /// let cache = ExactMatchCache::new(10000).unwrap();
    /// ```
    pub fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Layer1Error::InvalidCapacity(capacity));
        }

        Ok(Self {
            cache: DashMap::with_capacity(capacity),
            capacity,
        })
    }

    /// Get a memory by exact query match
    ///
    /// Hashes the query content and performs O(1) lookup.
    ///
    /// # Arguments
    /// * `query` - Query to look up
    ///
    /// # Returns
    /// * `Some(Memory)` - If exact match found
    /// * `None` - If no match found
    ///
    /// # Example
    /// ```
    /// use mfn_monolith::layer1::ExactMatchCache;
    /// use mfn_monolith::types::Query;
    ///
    /// let cache = ExactMatchCache::new(10000).unwrap();
    /// let query = Query::new("What is the capital of France?");
    /// let result = cache.get(&query);
    /// ```
    pub fn get(&self, query: &Query) -> Option<Memory> {
        let hash = Self::hash_query_content(&query.content);
        self.cache.get(&hash).map(|entry| entry.value().clone())
    }

    /// Insert a memory into the cache
    ///
    /// Hashes the memory content and stores it for exact match lookups.
    /// If the cache is at capacity, insertion will fail.
    ///
    /// # Arguments
    /// * `memory` - Memory to insert
    ///
    /// # Returns
    /// * `Ok(())` - If insertion successful
    /// * `Err(Layer1Error::CapacityExceeded)` - If cache is at capacity
    ///
    /// # Example
    /// ```
    /// use mfn_monolith::layer1::ExactMatchCache;
    /// use mfn_monolith::types::Memory;
    ///
    /// let cache = ExactMatchCache::new(10000).unwrap();
    /// let memory = Memory::new(
    ///     "Paris is the capital of France".to_string(),
    ///     vec![0.1, 0.2, 0.3]
    /// );
    /// cache.insert(memory).unwrap();
    /// ```
    pub fn insert(&self, memory: Memory) -> Result<()> {
        // Check capacity before insertion
        if self.cache.len() >= self.capacity {
            return Err(Layer1Error::CapacityExceeded {
                current: self.cache.len(),
                max: self.capacity,
            });
        }

        let hash = Self::hash_query_content(&memory.content);
        self.cache.insert(hash, memory);
        Ok(())
    }

    /// Get the current cache size
    ///
    /// # Returns
    /// Number of memories currently stored
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get the cache capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Hash query content using ahash
    ///
    /// Uses AHasher for fast non-cryptographic hashing.
    /// This is not suitable for cryptographic purposes but is extremely fast.
    fn hash_query_content(content: &str) -> u64 {
        let mut hasher = AHasher::default();
        content.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Memory, Query};

    #[test]
    fn test_new_cache() {
        let cache = ExactMatchCache::new(100).unwrap();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.capacity(), 100);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_new_cache_zero_capacity() {
        let result = ExactMatchCache::new(0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Layer1Error::InvalidCapacity(0)));
    }

    #[test]
    fn test_insert_and_get() {
        let cache = ExactMatchCache::new(100).unwrap();

        let memory = Memory::new(
            "Paris is the capital of France".to_string(),
            vec![0.1, 0.2, 0.3]
        );
        let content = memory.content.clone();

        cache.insert(memory).unwrap();
        assert_eq!(cache.len(), 1);

        let query = Query::new(&content);
        let result = cache.get(&query);

        assert!(result.is_some());
        let retrieved = result.unwrap();
        assert_eq!(retrieved.content, content);
    }

    #[test]
    fn test_exact_match_required() {
        let cache = ExactMatchCache::new(100).unwrap();

        let memory = Memory::new(
            "Paris is the capital of France".to_string(),
            vec![0.1, 0.2, 0.3]
        );
        cache.insert(memory).unwrap();

        // Exact match should work
        let query1 = Query::new("Paris is the capital of France");
        assert!(cache.get(&query1).is_some());

        // Different content should not match
        let query2 = Query::new("Paris is the capital of France.");
        assert!(cache.get(&query2).is_none());

        let query3 = Query::new("Paris is the capital");
        assert!(cache.get(&query3).is_none());

        let query4 = Query::new("paris is the capital of france");
        assert!(cache.get(&query4).is_none());
    }

    #[test]
    fn test_capacity_enforcement() {
        let cache = ExactMatchCache::new(2).unwrap();

        let memory1 = Memory::new("First memory".to_string(), vec![0.1]);
        let memory2 = Memory::new("Second memory".to_string(), vec![0.2]);
        let memory3 = Memory::new("Third memory".to_string(), vec![0.3]);

        assert!(cache.insert(memory1).is_ok());
        assert_eq!(cache.len(), 1);

        assert!(cache.insert(memory2).is_ok());
        assert_eq!(cache.len(), 2);

        // Third insert should fail
        let result = cache.insert(memory3);
        assert!(result.is_err());
        assert_eq!(cache.len(), 2);

        match result.unwrap_err() {
            Layer1Error::CapacityExceeded { current, max } => {
                assert_eq!(current, 2);
                assert_eq!(max, 2);
            }
            _ => panic!("Expected CapacityExceeded error"),
        }
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let cache = Arc::new(ExactMatchCache::new(1000).unwrap());
        let mut handles = vec![];

        // Spawn 10 threads that each insert and query
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    let content = format!("Memory {} from thread {}", j, i);
                    let memory = Memory::new(content.clone(), vec![i as f32, j as f32]);

                    // Insert might fail if capacity exceeded, that's ok for this test
                    let _ = cache_clone.insert(memory);

                    // Try to retrieve
                    let query = Query::new(&content);
                    let _ = cache_clone.get(&query);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Should have inserted up to 100 memories (10 threads * 10 each)
        assert!(cache.len() <= 100);
        assert!(cache.len() > 0);
    }

    #[test]
    fn test_hash_consistency() {
        let content = "Test content";
        let hash1 = ExactMatchCache::hash_query_content(content);
        let hash2 = ExactMatchCache::hash_query_content(content);
        assert_eq!(hash1, hash2, "Same content should produce same hash");

        let different_content = "Different content";
        let hash3 = ExactMatchCache::hash_query_content(different_content);
        assert_ne!(hash1, hash3, "Different content should produce different hash");
    }

    #[test]
    fn test_overwrite_same_content() {
        let cache = ExactMatchCache::new(100).unwrap();

        let memory1 = Memory::new("Same content".to_string(), vec![0.1, 0.2]);
        let memory2 = Memory::new("Same content".to_string(), vec![0.3, 0.4]);

        cache.insert(memory1).unwrap();
        assert_eq!(cache.len(), 1);

        // Inserting with same content should overwrite
        cache.insert(memory2).unwrap();
        assert_eq!(cache.len(), 1, "Should still be 1 as we overwrote");

        let query = Query::new("Same content");
        let result = cache.get(&query).unwrap();

        // Should have the second memory's embedding
        assert_eq!(result.embedding, vec![0.3, 0.4]);
    }

    #[test]
    fn test_lookup_performance() {
        use std::time::Instant;

        let cache = ExactMatchCache::new(10000).unwrap();

        // Insert 1000 memories
        for i in 0..1000 {
            let content = format!("Memory content number {}", i);
            let memory = Memory::new(content, vec![i as f32]);
            cache.insert(memory).unwrap();
        }

        // Warm up
        for i in 0..100 {
            let query = Query::new(&format!("Memory content number {}", i));
            let _ = cache.get(&query);
        }

        // Measure lookup time for 10,000 queries
        let start = Instant::now();
        let iterations = 10_000;

        for i in 0..iterations {
            let query = Query::new(&format!("Memory content number {}", i % 1000));
            let _ = cache.get(&query);
        }

        let elapsed = start.elapsed();
        let avg_lookup_us = elapsed.as_micros() as f64 / iterations as f64;

        println!("Layer 1 Performance:");
        println!("  Total queries: {}", iterations);
        println!("  Total time: {:?}", elapsed);
        println!("  Average lookup time: {:.3}µs", avg_lookup_us);
        println!("  Throughput: {:.0} queries/sec", 1_000_000.0 / avg_lookup_us);

        // Verify we meet the <1µs target
        assert!(avg_lookup_us < 1.0,
            "Average lookup time {:.3}µs exceeds 1µs target", avg_lookup_us);
    }
}
