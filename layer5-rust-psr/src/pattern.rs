//! Pattern data structures for Layer 5 PSR
//!
//! Defines the core Pattern type and related enums for structural pattern templates.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pattern category taxonomy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternCategory {
    /// Time-based patterns (sequences, cadence)
    Temporal,

    /// Space-based patterns (layout, proximity)
    Spatial,

    /// Transformation patterns (map, filter, fold)
    Transformational,

    /// Relationship patterns (hierarchy, network)
    Relational,
}

/// Type constraint for pattern slots
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternType {
    /// Sequential ordering
    Sequence,

    /// Unordered collection
    Set,

    /// Hierarchical structure
    Tree,

    /// Network structure
    Graph,

    /// Any type (no constraint)
    Any,
}

/// Type constraint with optional predicates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeConstraint {
    pub pattern_type: PatternType,
    pub predicates: Vec<Predicate>,
}

/// Predicate for pattern matching
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Predicate {
    pub name: String,
    pub expression: String,
}

/// Structural pattern template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    /// Unique pattern identifier
    pub id: String,

    /// Human-readable pattern name
    pub name: String,

    /// Pattern category
    pub category: PatternCategory,

    /// 256-dimensional embedding vector
    pub embedding: Vec<f32>,

    /// Source patterns that composed this pattern (P ∘ Q)
    pub source_patterns: Vec<String>,

    /// Patterns this can compose with
    pub composable_with: Vec<String>,

    /// Named slots with type constraints
    pub slots: HashMap<String, TypeConstraint>,

    /// Pattern matching predicates
    pub constraints: Vec<Predicate>,

    /// Input domain type
    pub domain: PatternType,

    /// Output codomain type
    pub codomain: PatternType,

    /// Text-based example
    pub text_example: String,

    /// Image-based example (base64 or URL)
    pub image_example: String,

    /// Audio-based example (base64 or URL)
    pub audio_example: String,

    /// Code-based example
    pub code_example: String,

    /// Number of times pattern was activated
    pub activation_count: u64,

    /// Confidence score (0.0-1.0)
    pub confidence: f32,

    /// First time pattern was observed (training step)
    pub first_seen_step: u64,

    /// Last time pattern was used (training step)
    pub last_used_step: u64,

    /// Unix timestamp (milliseconds) when created
    pub created_at: u64,
}

impl Pattern {
    /// Create a new pattern with default values
    pub fn new(id: String, name: String, category: PatternCategory, embedding: Vec<f32>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            id,
            name,
            category,
            embedding,
            source_patterns: vec![],
            composable_with: vec![],
            slots: HashMap::new(),
            constraints: vec![],
            domain: PatternType::Any,
            codomain: PatternType::Any,
            text_example: String::new(),
            image_example: String::new(),
            audio_example: String::new(),
            code_example: String::new(),
            activation_count: 0,
            confidence: 1.0,
            first_seen_step: 0,
            last_used_step: 0,
            created_at: now,
        }
    }

    /// Update pattern statistics
    pub fn update_stats(&mut self, activation_delta: u64, current_step: u64) {
        self.activation_count += activation_delta;
        self.last_used_step = current_step;
    }

    /// Check if pattern matches constraints
    pub fn matches_constraints(&self, context: &HashMap<String, String>) -> bool {
        for constraint in &self.constraints {
            // Simple string matching for now (can be extended with expression evaluation)
            if let Some(value) = context.get(&constraint.name) {
                if !value.contains(&constraint.expression) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_creation() {
        let pattern = Pattern::new(
            "test_pattern".to_string(),
            "Test Pattern".to_string(),
            PatternCategory::Transformational,
            vec![0.1; 256],
        );

        assert_eq!(pattern.id, "test_pattern");
        assert_eq!(pattern.name, "Test Pattern");
        assert_eq!(pattern.category, PatternCategory::Transformational);
        assert_eq!(pattern.embedding.len(), 256);
        assert_eq!(pattern.activation_count, 0);
        assert_eq!(pattern.confidence, 1.0);
    }

    #[test]
    fn test_update_stats() {
        let mut pattern = Pattern::new(
            "test".to_string(),
            "Test".to_string(),
            PatternCategory::Temporal,
            vec![0.5; 256],
        );

        pattern.update_stats(5, 100);
        assert_eq!(pattern.activation_count, 5);
        assert_eq!(pattern.last_used_step, 100);

        pattern.update_stats(3, 150);
        assert_eq!(pattern.activation_count, 8);
        assert_eq!(pattern.last_used_step, 150);
    }

    #[test]
    fn test_matches_constraints() {
        let mut pattern = Pattern::new(
            "test".to_string(),
            "Test".to_string(),
            PatternCategory::Relational,
            vec![0.1; 256],
        );

        pattern.constraints.push(Predicate {
            name: "type".to_string(),
            expression: "user_action".to_string(),
        });

        let mut context = HashMap::new();
        context.insert("type".to_string(), "user_action_click".to_string());

        assert!(pattern.matches_constraints(&context));

        context.insert("type".to_string(), "system_event".to_string());
        assert!(!pattern.matches_constraints(&context));
    }
}
