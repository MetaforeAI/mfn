# Adaptive Routing Algorithm
**Phase 1, Step 2: Definition & Scoping**
**Bug**: BUG-002 - Stub Routing (Adaptive Mode)
**Date**: 2025-11-02
**Author**: Integration Agent

---

## Executive Summary

This document specifies the architecture for intelligent adaptive query routing in the MFN system. The adaptive routing strategy will analyze query characteristics and route to appropriate memory layers based on query type, maximizing performance while maintaining accuracy. This replaces the current stub implementation that incorrectly calls `query_sequential()`.

**Key Requirements**:
- Analyze query content to determine optimal layer selection
- Route based on query type: Exact, Semantic, Contextual, Unknown
- Optimize for performance (skip irrelevant layers)
- Maintain accuracy (ensure correct layers queried)
- Fall back to Sequential for uncertain queries

---

## 1. Current Implementation Analysis

### 1.1 Stub Code

**File**: `mfn-integration/src/socket_integration.rs:277-280`

```rust
async fn query_adaptive(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    // Simple adaptive routing - use sequential for now
    // In future, could analyze query content to determine best routing
    self.query_sequential(query).await
}
```

**Problems**:
1. **No intelligence**: Always calls sequential (no adaptation)
2. **No performance benefit**: Same as sequential routing
3. **Wasted opportunity**: Doesn't leverage layer specialization
4. **False advertising**: Claims "adaptive" but has no query analysis

### 1.2 Layer Specialization

Each MFN layer has distinct capabilities:

| Layer | Name | Specialization | Best For | Performance |
|-------|------|----------------|----------|-------------|
| 1 | IFR | Exact matching | Short exact strings, IDs, keywords | 0.1ms |
| 2 | DSR | Similarity search | Natural language queries, semantic search | 1-5ms |
| 3 | ALM | Associative memory | Related concepts, graph traversal | 2-10ms |
| 4 | CPE | Context prediction | Conversation history, predictions | 5-15ms |

**Key Insight**: Not all layers are needed for all queries

---

## 2. Query Classification Algorithm

### 2.1 Query Types

#### Type 1: Exact Match

**Characteristics**:
- Short length (<20 characters)
- No spaces or single word
- Alphanumeric pattern (IDs, codes, keywords)
- Examples: "user_123", "ERROR", "login", "42"

**Optimal Routing**: Layer 1 only
**Rationale**: Exact match is instant (0.1ms), other layers add no value

#### Type 2: Semantic Search

**Characteristics**:
- Natural language questions
- Contains question words (what, how, why, when, where, who)
- Complete sentences with verbs
- Examples: "How do I reset my password?", "What causes login errors?"

**Optimal Routing**: Layer 2 (DSR) + Layer 3 (ALM)
**Rationale**: Semantic similarity + associative concepts, skip exact match and prediction

#### Type 3: Contextual/Conversational

**Characteristics**:
- Conversational context (pronouns: "it", "this", "that")
- Follow-up questions ("tell me more", "explain further")
- References to previous interactions
- Examples: "Tell me more about that", "What happens next?"

**Optimal Routing**: All layers (emphasize Layer 4)
**Rationale**: Requires context prediction, but also benefit from other layers

#### Type 4: Unknown/Ambiguous

**Characteristics**:
- Doesn't clearly fit other categories
- Mixed patterns
- Edge cases

**Optimal Routing**: Sequential (safe fallback)
**Rationale**: When uncertain, query all layers to avoid missing results

### 2.2 Classification Decision Tree

```
Query Text
    │
    ▼
┌─────────────────────┐
│ Length < 20 chars?  │
│ No spaces?          │
└──────┬──────────────┘
       │
   ┌───┴───┐
  Yes      No
   │        │
   ▼        ▼
[EXACT]  ┌────────────────────────┐
         │ Contains question      │
         │ words (what/how/why)?  │
         └──────┬─────────────────┘
                │
            ┌───┴───┐
           Yes      No
            │        │
            ▼        ▼
        [SEMANTIC] ┌──────────────────────┐
                   │ Contains pronouns    │
                   │ (it/this/that)?      │
                   │ Or context words?    │
                   └──────┬───────────────┘
                          │
                      ┌───┴───┐
                     Yes      No
                      │        │
                      ▼        ▼
                [CONTEXTUAL] [UNKNOWN]
```

### 2.3 Classification Implementation

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum QueryType {
    Exact,        // Short exact string
    Semantic,     // Natural language question
    Contextual,   // Conversational with context
    Unknown,      // Fallback
}

impl QueryType {
    pub fn classify(query_text: &str) -> Self {
        let text = query_text.trim().to_lowercase();
        let length = text.len();
        let word_count = text.split_whitespace().count();

        // Type 1: Exact match queries
        if Self::is_exact_match(&text, length, word_count) {
            return QueryType::Exact;
        }

        // Type 2: Semantic search queries
        if Self::is_semantic_query(&text) {
            return QueryType::Semantic;
        }

        // Type 3: Contextual queries
        if Self::is_contextual_query(&text) {
            return QueryType::Contextual;
        }

        // Type 4: Unknown (fallback)
        QueryType::Unknown
    }

    fn is_exact_match(text: &str, length: usize, word_count: usize) -> bool {
        // Short single-word queries
        if length < 20 && word_count == 1 {
            return true;
        }

        // ID patterns: user_123, error-404, etc.
        if length < 30 && text.contains(|c: char| c.is_numeric() || c == '_' || c == '-') {
            return true;
        }

        false
    }

    fn is_semantic_query(text: &str) -> bool {
        // Question words
        const QUESTION_WORDS: &[&str] = &[
            "what", "how", "why", "when", "where", "who", "which", "whose",
            "can", "could", "would", "should", "is", "are", "does", "do",
        ];

        // Check if starts with question word
        for word in QUESTION_WORDS {
            if text.starts_with(word) {
                return true;
            }
        }

        // Check for question marks
        if text.contains('?') {
            return true;
        }

        // Long natural language (>30 chars, >4 words)
        let word_count = text.split_whitespace().count();
        if text.len() > 30 && word_count > 4 {
            return true;
        }

        false
    }

    fn is_contextual_query(text: &str) -> bool {
        // Pronouns indicating context dependency
        const CONTEXT_INDICATORS: &[&str] = &[
            "it", "this", "that", "these", "those",
            "he", "she", "they", "them",
            "tell me more", "explain", "continue",
            "what about", "and then", "next",
        ];

        for indicator in CONTEXT_INDICATORS {
            if text.contains(indicator) {
                return true;
            }
        }

        // Very short conversational (2-4 words)
        let word_count = text.split_whitespace().count();
        if word_count >= 2 && word_count <= 4 && text.len() < 30 {
            return true;
        }

        false
    }
}
```

---

## 3. Layer Selection Strategy

### 3.1 Selection Matrix

| Query Type | Layer 1 (IFR) | Layer 2 (DSR) | Layer 3 (ALM) | Layer 4 (CPE) | Strategy |
|------------|---------------|---------------|---------------|---------------|----------|
| **Exact** | ✅ 100% | ❌ 0% | ❌ 0% | ❌ 0% | Layer 1 only |
| **Semantic** | ❌ 0% | ✅ 100% | ✅ 80% | ✅ 40% | Layer 2+3, optional 4 |
| **Contextual** | ✅ 50% | ✅ 60% | ✅ 60% | ✅ 100% | All layers, emphasize 4 |
| **Unknown** | ✅ Sequential | ✅ Sequential | ✅ Sequential | ✅ Sequential | Safe fallback |

**Note**: Percentages indicate relevance/weight, not probability

### 3.2 Routing Implementation

```rust
impl SocketMfnIntegration {
    async fn query_adaptive(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let start = Instant::now();

        // Classify query
        let query_type = QueryType::classify(&query.query_text);
        debug!("Query classified as {:?}: '{}'", query_type, query.query_text);

        // Route based on classification
        let results = match query_type {
            QueryType::Exact => {
                self.route_exact(query).await?
            }
            QueryType::Semantic => {
                self.route_semantic(query).await?
            }
            QueryType::Contextual => {
                self.route_contextual(query).await?
            }
            QueryType::Unknown => {
                warn!("Query type unknown, using sequential routing");
                self.query_sequential(query).await?
            }
        };

        let elapsed = start.elapsed().as_millis() as f64;
        debug!("Adaptive query completed in {}ms (type: {:?})", elapsed, query_type);

        Ok(results)
    }

    /// Route exact match queries (Layer 1 only)
    async fn route_exact(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        self.query_layer1_only(query).await
    }

    /// Route semantic queries (Layer 2 + Layer 3, optionally Layer 4)
    async fn route_semantic(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let query2 = query.clone();
        let query3 = query.clone();
        let pool = Arc::clone(&self.connection_pool);

        // Query Layer 2 and Layer 3 in parallel
        let (result2, result3) = tokio::join!(
            Self::query_layer2_safe(pool.clone(), query2),
            Self::query_layer3_safe(pool.clone(), query3),
        );

        // Collect results
        let mut all_results = Vec::new();
        if let Ok(results) = result2 {
            all_results.extend(results);
        }
        if let Ok(results) = result3 {
            all_results.extend(results);
        }

        // Merge and rank
        Ok(merge_and_rank_results(all_results, query.max_results))
    }

    /// Route contextual queries (all layers with Layer 4 emphasis)
    async fn route_contextual(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        // Use parallel routing for all layers
        self.query_parallel(query).await
    }
}
```

### 3.3 Advanced: Weighted Layer Selection

**Future Enhancement**: Use confidence weights to prioritize layers

```rust
struct LayerWeights {
    layer1_weight: f32,
    layer2_weight: f32,
    layer3_weight: f32,
    layer4_weight: f32,
}

impl QueryType {
    fn layer_weights(&self) -> LayerWeights {
        match self {
            QueryType::Exact => LayerWeights {
                layer1_weight: 1.0,
                layer2_weight: 0.0,
                layer3_weight: 0.0,
                layer4_weight: 0.0,
            },
            QueryType::Semantic => LayerWeights {
                layer1_weight: 0.0,
                layer2_weight: 1.0,
                layer3_weight: 0.8,
                layer4_weight: 0.4,
            },
            QueryType::Contextual => LayerWeights {
                layer1_weight: 0.5,
                layer2_weight: 0.6,
                layer3_weight: 0.6,
                layer4_weight: 1.0,
            },
            QueryType::Unknown => LayerWeights {
                layer1_weight: 1.0,
                layer2_weight: 1.0,
                layer3_weight: 1.0,
                layer4_weight: 1.0,
            },
        }
    }
}

fn merge_with_weights(
    results: Vec<UniversalSearchResult>,
    weights: LayerWeights,
) -> Vec<UniversalSearchResult> {
    let mut weighted_results = results;

    for result in &mut weighted_results {
        let layer = result.metadata.get("layer").and_then(|v| v.as_str()).unwrap_or("");
        let weight = match layer {
            "Layer1" => weights.layer1_weight,
            "Layer2" => weights.layer2_weight,
            "Layer3" => weights.layer3_weight,
            "Layer4" => weights.layer4_weight,
            _ => 1.0,
        };

        // Adjust confidence by layer weight
        result.confidence *= weight;
    }

    // Sort by adjusted confidence
    weighted_results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    weighted_results
}
```

---

## 4. Performance Analysis

### 4.1 Latency Comparison by Query Type

**Exact Match Queries** (e.g., "user_123"):
```
Sequential:  0.1 + 5 + 10 + 15 = 30.1ms
Parallel:    max(0.1, 5, 10, 15) = 15ms
Adaptive:    0.1ms (Layer 1 only)  ← 300x faster than sequential
```

**Semantic Queries** (e.g., "How do I reset my password?"):
```
Sequential:  0.1 + 5 + 10 + 15 = 30.1ms
Parallel:    max(0.1, 5, 10, 15) = 15ms
Adaptive:    max(5, 10) = 10ms (Layer 2+3 only)  ← 3x faster than sequential
```

**Contextual Queries** (e.g., "Tell me more about that"):
```
Sequential:  0.1 + 5 + 10 + 15 = 30.1ms
Parallel:    max(0.1, 5, 10, 15) = 15ms
Adaptive:    max(0.1, 5, 10, 15) = 15ms (all layers)  ← Same as parallel
```

**Unknown Queries**:
```
Sequential:  30.1ms
Parallel:    15ms
Adaptive:    30.1ms (fallback to sequential)  ← Same as sequential
```

### 4.2 Performance by Query Distribution

**Realistic Query Mix**:
- Exact: 30% of queries
- Semantic: 50% of queries
- Contextual: 15% of queries
- Unknown: 5% of queries

**Average Latency**:
```
Sequential: 30.1ms (all queries)

Parallel: 15ms (all queries)

Adaptive:
  = 0.30 × 0.1ms (exact)
  + 0.50 × 10ms (semantic)
  + 0.15 × 15ms (contextual)
  + 0.05 × 30.1ms (unknown)
  = 0.03 + 5.0 + 2.25 + 1.51
  = 8.79ms  ← Best overall performance
```

**Speedup vs Sequential**: 30.1 / 8.79 = **3.4x faster**

### 4.3 Resource Utilization

**Connection Pool Usage**:
```
Sequential: 1 connection at a time (4 total over time)
Parallel: 4 connections simultaneously
Adaptive:
  - Exact: 1 connection (Layer 1)
  - Semantic: 2 connections (Layer 2+3)
  - Contextual: 4 connections (all layers)
  - Average: ~2.1 connections per query
```

**CPU Utilization**:
```
Sequential: 1 core active per layer (serialized)
Parallel: 4 cores active (all layers)
Adaptive:
  - Exact: 1 core (Layer 1 only)
  - Semantic: 2 cores (Layer 2+3)
  - Contextual: 4 cores (all layers)
  - Average: ~2.2 cores per query
```

**Memory Usage**:
```
Sequential: ~10MB per query
Parallel: ~40MB per query
Adaptive:
  - Exact: ~10MB (1 layer)
  - Semantic: ~20MB (2 layers)
  - Contextual: ~40MB (4 layers)
  - Average: ~22MB per query
```

---

## 5. Accuracy Considerations

### 5.1 Risk: Missing Results

**Scenario**: Adaptive routing skips layers that might have relevant results

**Example**:
- Query: "login" (classified as Exact)
- Adaptive: Queries Layer 1 only
- Problem: Layer 2 has semantic variations ("sign in", "authenticate")
- Result: Misses relevant results from Layer 2

**Mitigation**:
1. **Conservative classification**: When uncertain, fallback to Sequential
2. **User feedback loop**: Track which queries have poor results
3. **Confidence threshold**: If Layer 1 confidence <0.5, expand to Layer 2
4. **Query expansion**: Automatic retry with broader routing if no results

```rust
impl SocketMfnIntegration {
    async fn route_exact_with_fallback(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        // Try Layer 1 only
        let results = self.query_layer1_only(query.clone()).await?;

        // If no results or low confidence, expand to semantic routing
        if results.is_empty() {
            warn!("No exact matches, expanding to semantic search");
            return self.route_semantic(query).await;
        }

        let max_confidence = results.iter().map(|r| r.confidence).fold(0.0, f32::max);
        if max_confidence < 0.5 {
            warn!("Low confidence exact match ({}), expanding search", max_confidence);
            let semantic_results = self.route_semantic(query).await?;

            // Merge both result sets
            let mut combined = results;
            combined.extend(semantic_results);
            return Ok(merge_and_rank_results(combined, query.max_results));
        }

        Ok(results)
    }
}
```

### 5.2 Classification Accuracy

**Goal**: >90% accurate query classification

**Validation Strategy**:
1. **Manual labeling**: Label 1000 test queries with expected type
2. **Classification test**: Run classifier, measure accuracy
3. **A/B testing**: Compare adaptive vs parallel in production
4. **User satisfaction**: Track result quality metrics

**Test Cases**:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_classification_exact() {
        assert_eq!(QueryType::classify("user_123"), QueryType::Exact);
        assert_eq!(QueryType::classify("ERROR"), QueryType::Exact);
        assert_eq!(QueryType::classify("42"), QueryType::Exact);
    }

    #[test]
    fn test_classification_semantic() {
        assert_eq!(
            QueryType::classify("How do I reset my password?"),
            QueryType::Semantic
        );
        assert_eq!(
            QueryType::classify("What causes login errors?"),
            QueryType::Semantic
        );
    }

    #[test]
    fn test_classification_contextual() {
        assert_eq!(
            QueryType::classify("Tell me more about that"),
            QueryType::Contextual
        );
        assert_eq!(
            QueryType::classify("What happens next?"),
            QueryType::Contextual
        );
    }
}
```

---

## 6. Future Enhancements

### 6.1 Machine Learning Classification

**Current**: Rule-based classification (pattern matching)
**Future**: ML model for query type prediction

**Architecture**:
```
Query Text
    │
    ▼
┌────────────────────────┐
│  Feature Extraction    │
│  - Length              │
│  - Word count          │
│  - POS tags            │
│  - TF-IDF features     │
└──────────┬─────────────┘
           │
           ▼
┌────────────────────────┐
│  ML Classifier         │
│  (Random Forest/NN)    │
│  Output: QueryType +   │
│          Confidence    │
└──────────┬─────────────┘
           │
           ▼
┌────────────────────────┐
│  Layer Selection       │
└────────────────────────┘
```

**Benefits**:
- Higher classification accuracy (>95%)
- Learns from user feedback
- Adapts to domain-specific patterns

**Implementation**: Phase 2+ (not Phase 1)

### 6.2 Performance-Based Routing

**Objective**: Route based on layer performance, not just query type

**Architecture**:
```rust
struct LayerPerformance {
    layer_id: String,
    avg_latency_ms: f64,
    success_rate: f64,
    avg_result_quality: f64,  // User feedback
}

impl SocketMfnIntegration {
    async fn route_performance_based(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let query_type = QueryType::classify(&query.query_text);

        // Get layer performance stats
        let perf = self.performance_tracker.get_stats().await;

        // Select fastest layers above quality threshold
        let selected_layers = perf
            .iter()
            .filter(|p| p.avg_result_quality > 0.7)  // Quality gate
            .filter(|p| p.success_rate > 0.95)       // Reliability gate
            .take(2)  // Top 2 fastest
            .map(|p| p.layer_id.clone())
            .collect::<Vec<_>>();

        // Query selected layers
        self.query_selected_layers(query, selected_layers).await
    }
}
```

### 6.3 Cost-Based Routing

**Objective**: Optimize for cost (compute, memory, latency)

**Cost Model**:
```rust
struct LayerCost {
    latency_cost: f64,      // ms × weight
    compute_cost: f64,      // CPU cycles × weight
    memory_cost: f64,       // MB × weight
}

fn calculate_routing_cost(layers: &[LayerId]) -> f64 {
    layers.iter().map(|layer| {
        let cost = layer.cost();
        cost.latency_cost * 0.5 +
        cost.compute_cost * 0.3 +
        cost.memory_cost * 0.2
    }).sum()
}
```

**Routing Strategy**: Select layers that minimize cost while meeting quality threshold

---

## 7. Testing Strategy

### 7.1 Classification Tests

```rust
#[cfg(test)]
mod classification_tests {
    use super::*;

    #[test]
    fn test_exact_queries() {
        let exact_queries = vec![
            "user_123",
            "ERROR",
            "404",
            "login",
            "api_key_abc123",
        ];

        for query in exact_queries {
            assert_eq!(
                QueryType::classify(query),
                QueryType::Exact,
                "Failed for query: {}",
                query
            );
        }
    }

    #[test]
    fn test_semantic_queries() {
        let semantic_queries = vec![
            "How do I reset my password?",
            "What is the meaning of life?",
            "Why does this error occur?",
            "When was the last login?",
            "Can you explain authentication?",
        ];

        for query in semantic_queries {
            assert_eq!(
                QueryType::classify(query),
                QueryType::Semantic,
                "Failed for query: {}",
                query
            );
        }
    }

    #[test]
    fn test_contextual_queries() {
        let contextual_queries = vec![
            "Tell me more",
            "What about this?",
            "Explain that",
            "Continue",
            "And then what?",
        ];

        for query in contextual_queries {
            assert_eq!(
                QueryType::classify(query),
                QueryType::Contextual,
                "Failed for query: {}",
                query
            );
        }
    }

    #[test]
    fn test_edge_cases() {
        // Empty query
        assert_eq!(QueryType::classify(""), QueryType::Unknown);

        // Very long query
        let long_query = "a".repeat(1000);
        assert!(matches!(
            QueryType::classify(&long_query),
            QueryType::Semantic | QueryType::Unknown
        ));

        // Special characters
        assert_eq!(QueryType::classify("@#$%^&*"), QueryType::Unknown);
    }
}
```

### 7.2 Routing Tests

```rust
#[tokio::test]
async fn test_adaptive_routes_exact_to_layer1() {
    let integration = SocketMfnIntegration::new().await.unwrap();

    let query = UniversalSearchQuery {
        query_text: "user_123".to_string(),
        max_results: 10,
        ..Default::default()
    };

    // Mock to verify only Layer 1 called
    let results = integration.query_adaptive(query).await.unwrap();

    // Verify results came from Layer 1 only
    assert!(results.iter().all(|r| {
        r.metadata.get("layer").and_then(|v| v.as_str()) == Some("Layer1")
    }));
}

#[tokio::test]
async fn test_adaptive_routes_semantic_to_layer2_layer3() {
    let integration = SocketMfnIntegration::new().await.unwrap();

    let query = UniversalSearchQuery {
        query_text: "How do I reset my password?".to_string(),
        max_results: 10,
        ..Default::default()
    };

    let results = integration.query_adaptive(query).await.unwrap();

    // Verify results came from Layer 2 and/or Layer 3
    let layers: HashSet<String> = results
        .iter()
        .filter_map(|r| r.metadata.get("layer").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
        .collect();

    assert!(
        layers.contains("Layer2") || layers.contains("Layer3"),
        "Expected Layer 2 or Layer 3 results"
    );
}
```

### 7.3 Performance Benchmarks

```rust
#[bench]
fn bench_adaptive_vs_sequential(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let integration = runtime.block_on(async {
        SocketMfnIntegration::new().await.unwrap()
    });

    // Mix of query types
    let queries = vec![
        ("user_123", QueryType::Exact),
        ("How do I login?", QueryType::Semantic),
        ("Tell me more", QueryType::Contextual),
    ];

    b.iter(|| {
        for (query_text, _) in &queries {
            runtime.block_on(async {
                let query = UniversalSearchQuery {
                    query_text: query_text.to_string(),
                    max_results: 10,
                    ..Default::default()
                };
                integration.query_adaptive(query).await.unwrap()
            });
        }
    });
}
```

### 7.4 Accuracy Tests

```rust
#[tokio::test]
async fn test_adaptive_finds_all_relevant_results() {
    let integration = SocketMfnIntegration::new().await.unwrap();

    // Populate test data across all layers
    populate_test_data(&integration).await;

    // Query that should match data in Layer 2
    let query = UniversalSearchQuery {
        query_text: "How do I authenticate?".to_string(),
        max_results: 10,
        ..Default::default()
    };

    let adaptive_results = integration.query_adaptive(query.clone()).await.unwrap();
    let sequential_results = integration.query_sequential(query).await.unwrap();

    // Adaptive should find same memory IDs as sequential (may differ in order)
    let adaptive_ids: HashSet<_> = adaptive_results
        .iter()
        .map(|r| &r.memory_id)
        .collect();
    let sequential_ids: HashSet<_> = sequential_results
        .iter()
        .map(|r| &r.memory_id)
        .collect();

    assert_eq!(
        adaptive_ids, sequential_ids,
        "Adaptive routing should find same results as sequential"
    );
}
```

---

## 8. Monitoring & Observability

### 8.1 Metrics

**Classification Metrics**:
```rust
pub struct AdaptiveMetrics {
    // Query type distribution
    pub exact_queries: Counter,
    pub semantic_queries: Counter,
    pub contextual_queries: Counter,
    pub unknown_queries: Counter,

    // Routing performance
    pub avg_latency_by_type: HashMap<QueryType, Histogram>,
    pub layer_selection_count: HashMap<String, Counter>,

    // Accuracy metrics
    pub empty_result_rate_by_type: HashMap<QueryType, Gauge>,
    pub avg_confidence_by_type: HashMap<QueryType, Gauge>,

    // Fallback metrics
    pub classification_fallback_count: Counter,
    pub empty_result_expansion_count: Counter,
}
```

**Dashboard Visualization**:
```
Query Type Distribution (Last 24h)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Exact:       ████████████ 30%
Semantic:    ████████████████████ 50%
Contextual:  ██████ 15%
Unknown:     ██ 5%

Latency by Query Type (p95)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Exact:       0.2ms  ███
Semantic:    12ms   ████████████████
Contextual:  18ms   ███████████████████
Unknown:     35ms   ████████████████████████████████

Layer Usage (Last 24h)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Layer 1:     45%  ████████████████
Layer 2:     85%  ███████████████████████████
Layer 3:     75%  ████████████████████████
Layer 4:     25%  ████████
```

### 8.2 Logging

```rust
impl SocketMfnIntegration {
    async fn query_adaptive_with_logging(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
        let start = Instant::now();
        let query_type = QueryType::classify(&query.query_text);

        info!(
            query_id = %query.query_id,
            query_type = ?query_type,
            query_text = %query.query_text,
            "Adaptive routing started"
        );

        let results = match query_type {
            QueryType::Exact => {
                debug!("Routing to Layer 1 only");
                self.route_exact(query).await?
            }
            QueryType::Semantic => {
                debug!("Routing to Layer 2 + Layer 3");
                self.route_semantic(query).await?
            }
            QueryType::Contextual => {
                debug!("Routing to all layers");
                self.route_contextual(query).await?
            }
            QueryType::Unknown => {
                warn!("Unknown query type, fallback to sequential");
                self.query_sequential(query).await?
            }
        };

        let elapsed = start.elapsed().as_millis() as f64;
        info!(
            query_id = %query.query_id,
            query_type = ?query_type,
            result_count = results.len(),
            latency_ms = elapsed,
            "Adaptive routing completed"
        );

        Ok(results)
    }
}
```

---

## 9. Success Criteria

### 9.1 Functional Requirements

- [ ] Query classification algorithm implemented (Exact, Semantic, Contextual, Unknown)
- [ ] Routing functions for each query type working
- [ ] Fallback to Sequential for Unknown queries
- [ ] Empty result expansion (Layer 1 → Layer 2+3 fallback)
- [ ] Integration with existing parallel/sequential routing

### 9.2 Performance Requirements

- [ ] Average latency: <10ms (realistic query mix)
- [ ] Speedup vs Sequential: 2-4x faster
- [ ] Resource usage: <50% of Parallel routing (average)
- [ ] Classification overhead: <0.5ms per query

### 9.3 Quality Requirements

- [ ] Classification accuracy: >85% (manual validation)
- [ ] Result completeness: Same memory IDs as Sequential (no missed results)
- [ ] No stub or TODO comments remaining
- [ ] Unit test coverage: >80%
- [ ] Integration tests with all query types passing

### 9.4 Operational Requirements

- [ ] Metrics for query type distribution
- [ ] Metrics for routing performance by type
- [ ] Logging for classification decisions
- [ ] Feature flag for gradual rollout
- [ ] Documentation complete and accurate

---

## 10. Implementation Task Breakdown

**Task 1: Implement Query Classification** (1.5 hours)
- Create `QueryType` enum
- Implement `QueryType::classify()` with pattern matching
- Add helper functions (is_exact_match, is_semantic_query, is_contextual_query)
- **File**: `mfn-integration/src/socket_integration.rs`

**Task 2: Implement Routing Functions** (1.5 hours)
- Create `route_exact()` (Layer 1 only)
- Create `route_semantic()` (Layer 2+3 parallel)
- Create `route_contextual()` (all layers, reuse parallel)
- **File**: `mfn-integration/src/socket_integration.rs`

**Task 3: Implement Adaptive Query Function** (1 hour)
- Replace stub with classification + routing dispatch
- Add logging for classification decisions
- **File**: `mfn-integration/src/socket_integration.rs`

**Task 4: Add Fallback Logic** (0.5 hours)
- Implement empty result expansion (Layer 1 → Layer 2+3)
- Add Unknown type fallback to Sequential
- **File**: `mfn-integration/src/socket_integration.rs`

**Task 5: Add Unit Tests** (1 hour)
- Test classification accuracy (exact, semantic, contextual)
- Test routing functions
- Test fallback logic
- **File**: `mfn-integration/tests/test_adaptive_routing.rs`

**Total Estimated Effort**: 5.5 hours (within 4-6 hour target)

---

## Appendix: Example Classifications

### Example 1: Exact Match
```
Query: "user_123"
Classification: QueryType::Exact
Reasoning: Short (8 chars), single word, contains ID pattern
Routing: Layer 1 only
Expected Latency: 0.1ms
```

### Example 2: Semantic Search
```
Query: "How do I reset my password?"
Classification: QueryType::Semantic
Reasoning: Starts with "How", question mark, natural language
Routing: Layer 2 + Layer 3
Expected Latency: max(5ms, 10ms) = 10ms
```

### Example 3: Contextual Query
```
Query: "Tell me more about that"
Classification: QueryType::Contextual
Reasoning: Contains "that" (pronoun), "tell me more" (context indicator)
Routing: All layers (parallel)
Expected Latency: max(0.1, 5, 10, 15) = 15ms
```

### Example 4: Edge Case (Unknown)
```
Query: "@#$%^&*"
Classification: QueryType::Unknown
Reasoning: No recognizable pattern, all special characters
Routing: Sequential (safe fallback)
Expected Latency: 30ms
```

---

**Document Status**: COMPLETE
**Ready for Step 4 Implementation**: YES
**Dependencies**: Requires parallel routing implementation (BUG-002 Part 1)
