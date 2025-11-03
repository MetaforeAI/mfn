//! Unit tests for the embedding service

#[cfg(test)]
mod tests {
    use super::super::*;
    use std::collections::HashSet;

    /// Test that different inputs produce unique embeddings
    #[test]
    fn test_embedding_generation_unique() {
        let embedder = SemanticHashEmbedder::new();

        let emb1 = embedder.encode("hello world");
        let emb2 = embedder.encode("goodbye world");
        let emb3 = embedder.encode("hello world"); // Same as emb1

        // Different inputs should produce different embeddings
        assert_ne!(emb1, emb2, "Different inputs should produce different embeddings");

        // Same input should produce same embedding (deterministic)
        assert_eq!(emb1, emb3, "Same input should produce same embedding");
    }

    /// Test that embeddings have the correct dimensions
    #[test]
    fn test_embedding_dimensions() {
        let embedder = SemanticHashEmbedder::new();

        // Test various input lengths
        let long_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(10);
        let test_inputs = vec![
            "",  // Empty input
            "a",  // Single char
            "hello",  // Short word
            "The quick brown fox jumps over the lazy dog",  // Medium sentence
            &long_text[..],  // Long text
        ];

        for input in test_inputs {
            let embedding = embedder.encode(input);
            assert_eq!(
                embedding.len(),
                384,
                "Embedding dimension should be 384 for input: '{}'",
                input.chars().take(50).collect::<String>()
            );
        }
    }

    /// Test L2 normalization of embeddings
    #[test]
    fn test_embedding_normalization() {
        let embedder = SemanticHashEmbedder::new();

        let test_inputs = vec![
            "test normalization",
            "another test case",
            "The quick brown fox",
            "machine learning embeddings",
            "natural language processing",
        ];

        for input in test_inputs {
            let embedding = embedder.encode(input);

            // Calculate L2 norm
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

            // Check if normalized (allow small tolerance for floating point)
            assert!(
                (norm - 1.0).abs() < 0.01,
                "L2 norm should be ~1.0 for '{}', got {}",
                input,
                norm
            );
        }
    }

    /// Test semantic similarity between related words
    #[test]
    fn test_semantic_similarity() {
        let embedder = SemanticHashEmbedder::new();

        // Helper function to calculate cosine similarity
        let cosine_similarity = |vec1: &[f32], vec2: &[f32]| -> f32 {
            let dot: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
            let norm1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm1 == 0.0 || norm2 == 0.0 {
                return 0.0;
            }
            dot / (norm1 * norm2)
        };

        // Test semantic relationships from the pre-defined clusters
        let cat_emb = embedder.encode("cat");
        let dog_emb = embedder.encode("dog");
        let car_emb = embedder.encode("car");
        let login_emb = embedder.encode("login");
        let password_emb = embedder.encode("password");
        let error_emb = embedder.encode("error");
        let bug_emb = embedder.encode("bug");

        // Animals should be similar to each other
        let cat_dog_sim = cosine_similarity(&cat_emb, &dog_emb);

        // Animals should be less similar to vehicles
        let cat_car_sim = cosine_similarity(&cat_emb, &car_emb);

        // Auth words should be similar
        let login_password_sim = cosine_similarity(&login_emb, &password_emb);

        // Error words should be similar
        let error_bug_sim = cosine_similarity(&error_emb, &bug_emb);

        // Cross-cluster should be less similar
        let cat_login_sim = cosine_similarity(&cat_emb, &login_emb);

        // Assert semantic relationships
        assert!(
            cat_dog_sim > cat_car_sim,
            "cat-dog similarity ({}) should be > cat-car similarity ({})",
            cat_dog_sim, cat_car_sim
        );

        assert!(
            login_password_sim > cat_login_sim,
            "login-password similarity ({}) should be > cat-login similarity ({})",
            login_password_sim, cat_login_sim
        );

        assert!(
            error_bug_sim > 0.3,
            "error-bug similarity ({}) should be > 0.3 (moderate similarity)",
            error_bug_sim
        );

        // Words in same cluster should have high similarity
        assert!(
            cat_dog_sim > 0.5,
            "cat-dog similarity ({}) should be > 0.5",
            cat_dog_sim
        );
    }

    /// Test empty input handling
    #[test]
    fn test_empty_input_handling() {
        let embedder = SemanticHashEmbedder::new();

        // Empty string
        let empty_emb = embedder.encode("");
        assert_eq!(empty_emb.len(), 384, "Empty input should return 384-dim vector");

        // Check it's the zero vector (or close to it)
        let sum: f32 = empty_emb.iter().map(|x| x.abs()).sum();
        assert!(sum < 0.01, "Empty input should produce near-zero vector");

        // Whitespace only
        let space_emb = embedder.encode("   ");
        assert_eq!(space_emb.len(), 384, "Whitespace input should return 384-dim vector");
    }

    /// Test that unknown words get encoded via character hashing
    #[test]
    fn test_unknown_word_encoding() {
        let embedder = SemanticHashEmbedder::new();

        // These words are not in the pre-defined clusters
        let unknown1 = embedder.encode("xyzabc123");
        let unknown2 = embedder.encode("qwerty999");
        let unknown3 = embedder.encode("xyzabc123"); // Same as unknown1

        // Should produce valid embeddings
        assert_eq!(unknown1.len(), 384);
        assert_eq!(unknown2.len(), 384);

        // Different unknown words should produce different embeddings
        assert_ne!(unknown1, unknown2);

        // Same unknown word should produce same embedding
        assert_eq!(unknown1, unknown3);

        // Should be normalized
        let norm: f32 = unknown1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }

    /// Test n-gram feature extraction
    #[test]
    fn test_ngram_features() {
        let embedder = SemanticHashEmbedder::new();

        // Words with common n-grams should be more similar
        let test1 = embedder.encode("testing");
        let test2 = embedder.encode("tested");
        let test3 = embedder.encode("completely different");

        // Calculate similarities
        let cosine_similarity = |vec1: &[f32], vec2: &[f32]| -> f32 {
            let dot: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();
            dot  // Already normalized so just dot product
        };

        let sim_12 = cosine_similarity(&test1, &test2);
        let sim_13 = cosine_similarity(&test1, &test3);

        // Words with common prefix should be more similar
        assert!(
            sim_12 > sim_13,
            "testing-tested similarity ({}) should be > testing-different similarity ({})",
            sim_12, sim_13
        );
    }

    /// Test positional features encoding
    #[test]
    fn test_positional_features() {
        let embedder = SemanticHashEmbedder::new();

        // Same words in different positions
        let emb1 = embedder.encode("first word last");
        let emb2 = embedder.encode("last word first");

        // Should produce different embeddings due to position
        assert_ne!(emb1, emb2, "Word position should affect embedding");

        // Test question detection
        let question_emb = embedder.encode("what is this");
        let statement_emb = embedder.encode("this is it");

        // Questions should have specific features in middle region
        let question_middle_sum: f32 = question_emb[192..202].iter().sum();
        let statement_middle_sum: f32 = statement_emb[192..202].iter().sum();

        assert!(
            question_middle_sum > statement_middle_sum,
            "Questions should have higher values in question-detection region"
        );
    }

    /// Test batch encoding consistency
    #[test]
    fn test_batch_encoding_consistency() {
        let embedder = SemanticHashEmbedder::new();

        let texts = vec![
            "first text",
            "second text",
            "third text",
        ];

        // Encode individually
        let individual: Vec<Vec<f32>> = texts.iter()
            .map(|t| embedder.encode(t))
            .collect();

        // Batch encoding would be done at service level, but test multiple calls
        let batch: Vec<Vec<f32>> = texts.iter()
            .map(|t| embedder.encode(t))
            .collect();

        // Should produce identical results
        for (ind, bat) in individual.iter().zip(batch.iter()) {
            assert_eq!(ind, bat, "Individual and batch encoding should match");
        }
    }

    /// Test TF-IDF fallback vectorizer
    #[test]
    fn test_tfidf_fallback() {
        let vectorizer = TfIdfVectorizer::new(384);

        let emb1 = vectorizer.encode("hello world");
        let emb2 = vectorizer.encode("hello world");
        let emb3 = vectorizer.encode("goodbye world");

        // Test dimensions
        assert_eq!(emb1.len(), 384);
        assert_eq!(emb2.len(), 384);
        assert_eq!(emb3.len(), 384);

        // Test deterministic
        assert_eq!(emb1, emb2, "Same input should produce same embedding");

        // Test different inputs
        assert_ne!(emb1, emb3, "Different inputs should produce different embeddings");

        // Test normalization
        let norm: f32 = emb1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01, "Should be L2 normalized");
    }

    /// Performance test - ensure embeddings are fast
    #[test]
    fn test_embedding_performance() {
        use std::time::Instant;

        let embedder = SemanticHashEmbedder::new();

        // Warm up
        for _ in 0..10 {
            let _ = embedder.encode("warmup text");
        }

        // Measure encoding time
        let iterations = 100;
        let start = Instant::now();

        for i in 0..iterations {
            let text = format!("test text number {}", i);
            let _ = embedder.encode(&text);
        }

        let elapsed = start.elapsed();
        let avg_time_ms = elapsed.as_millis() as f64 / iterations as f64;

        println!("Average embedding time: {}ms", avg_time_ms);

        // Should be very fast (< 1ms per embedding)
        assert!(
            avg_time_ms < 1.0,
            "Average embedding time ({}ms) should be < 1ms",
            avg_time_ms
        );
    }
}