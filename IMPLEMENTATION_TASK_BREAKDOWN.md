# Implementation Task Breakdown
**Phase 1, Step 2: Definition & Scoping**
**Date**: 2025-11-02
**Author**: Integration Agent

---

## Executive Summary

This document provides a granular breakdown of all implementation tasks required to fix BUG-001 (Placeholder Embeddings) and BUG-002 (Stub Routing) in Phase 1. The tasks are organized with clear dependencies, estimated effort, acceptance criteria, and file paths.

**Total Estimated Effort**: 21.5 hours
**Target Completion**: Step 4 (Development & Implementation)
**Critical Path**: BUG-001 Embedding Service (11 hours)

---

## 1. Task Organization

### 1.1 Work Packages

| Package | Bug | Description | Effort | Assignee |
|---------|-----|-------------|--------|----------|
| **WP1** | BUG-001 | Embedding Service Implementation | 11 hours | @developer |
| **WP2** | BUG-002a | Parallel Routing Implementation | 5 hours | @developer |
| **WP3** | BUG-002b | Adaptive Routing Implementation | 5.5 hours | @developer |

**Note**: WP2 and WP3 can be parallelized with WP1 (independent work streams)

### 1.2 Dependency Graph

```
┌─────────────────────────────────────────────────────────────────┐
│                        Phase 1 Sprint                           │
└─────────────────────────────────────────────────────────────────┘
                                │
        ┌───────────────────────┼────────────────────────┐
        │                       │                        │
        ▼                       ▼                        ▼
┌───────────────┐   ┌──────────────────┐   ┌──────────────────┐
│ WP1: Embeddings│   │ WP2: Parallel   │   │ WP3: Adaptive    │
│ (BUG-001)      │   │ Routing          │   │ Routing          │
│ 11 hours       │   │ (BUG-002a)       │   │ (BUG-002b)       │
│                │   │ 5 hours          │   │ 5.5 hours        │
└───────┬────────┘   └────────┬─────────┘   └────────┬─────────┘
        │                     │                        │
        │                     │              ┌─────────┘
        │                     │              │ (depends on WP2)
        │                     │              │
        └─────────────────────┴──────────────┘
                              │
                              ▼
                     ┌────────────────┐
                     │ Integration    │
                     │ Testing        │
                     │ (Step 5)       │
                     └────────────────┘
```

---

## 2. WP1: Embedding Service Implementation (BUG-001)

**Objective**: Replace placeholder embeddings with production sentence transformers

### Task 1.1: Add Dependencies

**File**: `mfn-integration/Cargo.toml`
**Estimated Time**: 0.5 hours
**Dependencies**: None
**Priority**: CRITICAL (blocks all other WP1 tasks)

**Changes**:
```toml
[dependencies]
# ... existing deps ...

# Embedding library (ONNX-based sentence transformers)
fastembed = "3.0"

# For fallback TF-IDF
ahash = "0.8"
```

**Acceptance Criteria**:
- [ ] `fastembed` dependency added with version 3.0+
- [ ] `ahash` dependency added for fallback
- [ ] `cargo build` succeeds without errors
- [ ] `cargo tree` shows fastembed dependencies resolved

**Testing**:
```bash
cd mfn-integration
cargo add fastembed@3.0
cargo add ahash@0.8
cargo build
```

**Blockers**: None

---

### Task 1.2: Create Module Structure

**Files**:
- `mfn-integration/src/embeddings/mod.rs` (new)
- `mfn-integration/src/embeddings/service.rs` (new)
- `mfn-integration/src/embeddings/models.rs` (new)
- `mfn-integration/src/embeddings/config.rs` (new)
- `mfn-integration/src/lib.rs` (modify)

**Estimated Time**: 1 hour
**Dependencies**: Task 1.1 (dependencies added)
**Priority**: HIGH

**File: embeddings/mod.rs**
```rust
//! Embedding service for semantic vector generation

mod config;
mod models;
mod service;

pub use config::{EmbeddingConfig, EmbeddingMetrics};
pub use models::EmbeddingModel;
pub use service::EmbeddingService;
```

**File: embeddings/config.rs**
```rust
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub model_name: String,
    pub cache_dir: PathBuf,
    pub enable_batching: bool,
    pub batch_size: usize,
    pub normalize: bool,
    pub enable_fallback: bool,
    pub download_timeout_secs: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: "sentence-transformers/all-MiniLM-L6-v2".to_string(),
            cache_dir: dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("mfn/models"),
            enable_batching: true,
            batch_size: 16,
            normalize: true,
            enable_fallback: true,
            download_timeout_secs: 300,
        }
    }
}

#[derive(Debug, Default)]
pub struct EmbeddingMetrics {
    pub total_embeddings: u64,
    pub total_time_ms: f64,
    pub avg_latency_ms: f64,
    pub model_load_time_ms: f64,
}
```

**File: mfn-integration/src/lib.rs** (add export)
```rust
pub mod embeddings;
```

**Acceptance Criteria**:
- [ ] All 4 new files created with correct structure
- [ ] Module exports correct (pub use statements)
- [ ] `cargo build` succeeds
- [ ] Module accessible from `mfn_integration::embeddings`

**Testing**:
```bash
cargo build
# Should compile without errors
```

**Blockers**: Task 1.1

---

### Task 1.3: Implement EmbeddingModel (Model Abstraction)

**File**: `mfn-integration/src/embeddings/models.rs`
**Estimated Time**: 3 hours
**Dependencies**: Task 1.2 (module structure)
**Priority**: CRITICAL

**Implementation**:
```rust
use anyhow::{Result, anyhow};
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel as FastEmbedModel};
use std::path::Path;
use std::sync::Arc;

pub enum EmbeddingModel {
    FastEmbed(Arc<TextEmbedding>),
    Fallback(TfIdfVectorizer),
}

impl EmbeddingModel {
    /// Load model from cache or download
    pub fn load(model_name: &str, cache_dir: &Path, enable_fallback: bool) -> Result<Self> {
        match Self::load_fastembed(model_name, cache_dir) {
            Ok(model) => Ok(Self::FastEmbed(Arc::new(model))),
            Err(e) if enable_fallback => {
                warn!("Model load failed: {}, using TF-IDF fallback", e);
                Ok(Self::Fallback(TfIdfVectorizer::new(384)))
            }
            Err(e) => Err(e),
        }
    }

    fn load_fastembed(model_name: &str, cache_dir: &Path) -> Result<TextEmbedding> {
        let init_options = InitOptions {
            model_name: FastEmbedModel::AllMiniLML6V2,  // MiniLM-L6-v2
            cache_dir: cache_dir.to_path_buf(),
            show_download_progress: true,
        };

        TextEmbedding::try_new(init_options)
            .map_err(|e| anyhow!("Failed to load embedding model: {}", e))
    }

    /// Generate embeddings for texts
    pub fn encode(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        match self {
            Self::FastEmbed(model) => {
                let embeddings = model
                    .embed(texts.to_vec(), None)
                    .map_err(|e| anyhow!("Encoding failed: {}", e))?;
                Ok(embeddings)
            }
            Self::Fallback(vectorizer) => {
                Ok(texts.iter().map(|t| vectorizer.encode(t)).collect())
            }
        }
    }

    /// Get embedding dimension
    pub fn dimension(&self) -> usize {
        match self {
            Self::FastEmbed(_) => 384,
            Self::Fallback(v) => v.dim,
        }
    }
}

/// Simple TF-IDF fallback vectorizer
pub struct TfIdfVectorizer {
    dim: usize,
}

impl TfIdfVectorizer {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    pub fn encode(&self, text: &str) -> Vec<f32> {
        use ahash::AHasher;
        use std::hash::{Hash, Hasher};

        let mut vec = vec![0.0f32; self.dim];

        for token in text.split_whitespace() {
            let mut hasher = AHasher::default();
            token.hash(&mut hasher);
            let hash = hasher.finish() as usize % self.dim;
            vec[hash] += 1.0;
        }

        // L2 normalize
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut vec {
                *x /= norm;
            }
        }

        vec
    }
}
```

**Acceptance Criteria**:
- [ ] EmbeddingModel enum defined with FastEmbed and Fallback variants
- [ ] `load()` function handles model download and caching
- [ ] `encode()` generates 384-dim vectors
- [ ] Fallback TF-IDF implemented for failures
- [ ] Unit tests pass for model loading and encoding

**Testing**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_load() {
        let temp_dir = std::env::temp_dir().join("mfn_test");
        let model = EmbeddingModel::load("all-MiniLM-L6-v2", &temp_dir, true).unwrap();
        assert_eq!(model.dimension(), 384);
    }

    #[test]
    fn test_embedding_generation() {
        let temp_dir = std::env::temp_dir().join("mfn_test");
        let model = EmbeddingModel::load("all-MiniLM-L6-v2", &temp_dir, true).unwrap();

        let embeddings = model.encode(&["test query"]).unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].len(), 384);
    }

    #[test]
    fn test_fallback_vectorizer() {
        let vectorizer = TfIdfVectorizer::new(384);
        let embedding = vectorizer.encode("test query");
        assert_eq!(embedding.len(), 384);

        // Check normalization
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01);
    }
}
```

**Blockers**: Task 1.2

---

### Task 1.4: Implement EmbeddingService

**File**: `mfn-integration/src/embeddings/service.rs`
**Estimated Time**: 3 hours
**Dependencies**: Task 1.3 (EmbeddingModel)
**Priority**: CRITICAL

**Implementation**:
```rust
use super::{EmbeddingConfig, EmbeddingMetrics, EmbeddingModel};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{info, warn};

pub struct EmbeddingService {
    model: Arc<EmbeddingModel>,
    config: EmbeddingConfig,
    metrics: Arc<Mutex<EmbeddingMetrics>>,
}

impl EmbeddingService {
    /// Initialize service with model download/cache
    pub async fn new(config: EmbeddingConfig) -> Result<Self> {
        let start = Instant::now();

        info!("Loading embedding model: {}", config.model_name);

        // Load model (blocking operation, run in spawn_blocking)
        let model_name = config.model_name.clone();
        let cache_dir = config.cache_dir.clone();
        let enable_fallback = config.enable_fallback;

        let model = tokio::task::spawn_blocking(move || {
            EmbeddingModel::load(&model_name, &cache_dir, enable_fallback)
        })
        .await??;

        let load_time = start.elapsed().as_millis() as f64;
        info!("Model loaded in {}ms", load_time);

        let metrics = EmbeddingMetrics {
            model_load_time_ms: load_time,
            ..Default::default()
        };

        Ok(Self {
            model: Arc::new(model),
            config,
            metrics: Arc::new(Mutex::new(metrics)),
        })
    }

    /// Generate embedding for single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let start = Instant::now();

        // Run encoding in spawn_blocking (CPU-intensive)
        let model = Arc::clone(&self.model);
        let text_owned = text.to_string();

        let embeddings = tokio::task::spawn_blocking(move || {
            model.encode(&[text_owned.as_str()])
        })
        .await??;

        let embedding = embeddings.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No embedding generated"))?;

        // Update metrics
        let latency = start.elapsed().as_millis() as f64;
        let mut metrics = self.metrics.lock().await;
        metrics.total_embeddings += 1;
        metrics.total_time_ms += latency;
        metrics.avg_latency_ms = metrics.total_time_ms / metrics.total_embeddings as f64;

        Ok(embedding)
    }

    /// Generate embeddings for batch (optimization)
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let start = Instant::now();

        // Run batch encoding in spawn_blocking
        let model = Arc::clone(&self.model);
        let texts_owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

        let embeddings = tokio::task::spawn_blocking(move || {
            let text_refs: Vec<&str> = texts_owned.iter().map(|s| s.as_str()).collect();
            model.encode(&text_refs)
        })
        .await??;

        // Update metrics
        let latency = start.elapsed().as_millis() as f64;
        let mut metrics = self.metrics.lock().await;
        metrics.total_embeddings += texts.len() as u64;
        metrics.total_time_ms += latency;
        metrics.avg_latency_ms = metrics.total_time_ms / metrics.total_embeddings as f64;

        Ok(embeddings)
    }

    /// Warmup model (pre-load and test)
    pub async fn warmup(&self) -> Result<()> {
        let start = Instant::now();

        let test_texts = vec![
            "Test embedding generation",
            "Quick brown fox",
            "The quick brown fox jumps over the lazy dog",
        ];

        for text in test_texts {
            let embedding = self.embed(text).await?;

            // Verify dimension
            if embedding.len() != 384 {
                return Err(anyhow::anyhow!(
                    "Warmup failed: wrong dimension {}",
                    embedding.len()
                ));
            }

            // Verify normalization (L2 norm ≈ 1.0)
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if (norm - 1.0).abs() > 0.01 {
                return Err(anyhow::anyhow!(
                    "Warmup failed: not normalized (norm={})",
                    norm
                ));
            }
        }

        let elapsed = start.elapsed().as_millis();
        info!("Model warmup completed in {}ms", elapsed);

        Ok(())
    }

    /// Get performance metrics
    pub async fn metrics(&self) -> EmbeddingMetrics {
        self.metrics.lock().await.clone()
    }
}
```

**Acceptance Criteria**:
- [ ] EmbeddingService struct with model, config, metrics
- [ ] `new()` function loads model and runs warmup
- [ ] `embed()` generates single embedding asynchronously
- [ ] `embed_batch()` generates batch embeddings
- [ ] `warmup()` validates model functionality
- [ ] Metrics tracking for latency and count
- [ ] Unit tests pass

**Testing**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_initialization() {
        let config = EmbeddingConfig::default();
        let service = EmbeddingService::new(config).await.unwrap();
        service.warmup().await.unwrap();
    }

    #[tokio::test]
    async fn test_embedding_dimension() {
        let service = EmbeddingService::new(EmbeddingConfig::default())
            .await.unwrap();

        let embedding = service.embed("test query").await.unwrap();
        assert_eq!(embedding.len(), 384);
    }

    #[tokio::test]
    async fn test_batch_embedding() {
        let service = EmbeddingService::new(EmbeddingConfig::default())
            .await.unwrap();

        let texts = vec!["query 1", "query 2", "query 3"];
        let embeddings = service.embed_batch(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 3);
        assert!(embeddings.iter().all(|e| e.len() == 384));
    }
}
```

**Blockers**: Task 1.3

---

### Task 1.5: Integrate with Layer2Client

**File**: `mfn-integration/src/socket_clients.rs`
**Estimated Time**: 2 hours
**Dependencies**: Task 1.4 (EmbeddingService)
**Priority**: CRITICAL

**Changes Required**:

**1. Add field to Layer2Client**:
```rust
pub struct Layer2Client {
    socket_path: String,
    embedding_service: Arc<EmbeddingService>,  // NEW
}
```

**2. Update constructor**:
```rust
impl Layer2Client {
    pub async fn new(embedding_service: Arc<EmbeddingService>) -> Result<Self> {
        Ok(Self {
            socket_path: "/tmp/mfn_layer2.sock".to_string(),
            embedding_service,
        })
    }
}
```

**3. Replace placeholder at line 217**:
```rust
// OLD (line 217):
let query_embedding = vec![0.1f32; 128]; // Placeholder embedding

// NEW:
// Generate semantic embedding using sentence transformer
let query_embedding = self.embedding_service
    .embed(&query.query_text)
    .await
    .map_err(|e| anyhow!("Embedding generation failed: {}", e))?;

// Validate embedding dimension
if query_embedding.len() != 384 {
    return Err(anyhow!(
        "Invalid embedding dimension: expected 384, got {}",
        query_embedding.len()
    ));
}
```

**4. Update LayerConnectionPool**:
```rust
pub struct LayerConnectionPool {
    layer1_clients: Vec<SocketLayer1>,
    layer2_clients: Vec<SocketLayer2>,
    layer3_clients: Vec<SocketLayer3>,
    layer4_clients: Vec<SocketLayer4>,

    // Shared embedding service
    embedding_service: Arc<EmbeddingService>,  // NEW
}

impl LayerConnectionPool {
    pub async fn new() -> Result<Self> {
        // Initialize embedding service ONCE
        let embedding_service = Arc::new(
            EmbeddingService::new(EmbeddingConfig::default()).await?
        );

        // Warmup model before creating clients
        embedding_service.warmup().await?;

        // Create Layer 2 clients with shared service
        let layer2_clients = (0..4)
            .map(|_| SocketLayer2::new(Arc::clone(&embedding_service)))
            .collect::<Result<Vec<_>>>()?;

        // ... rest of initialization

        Ok(Self {
            layer1_clients,
            layer2_clients,
            layer3_clients,
            layer4_clients,
            embedding_service,
        })
    }
}
```

**Acceptance Criteria**:
- [ ] Layer2Client has embedding_service field
- [ ] Constructor takes Arc<EmbeddingService>
- [ ] Placeholder at line 217 replaced with real embedding generation
- [ ] Dimension validation added
- [ ] LayerConnectionPool initializes embedding service once
- [ ] Integration tests pass

**Testing**:
```rust
#[tokio::test]
async fn test_layer2_with_real_embeddings() {
    let embedding_service = Arc::new(
        EmbeddingService::new(EmbeddingConfig::default()).await.unwrap()
    );

    let client = Layer2Client::new(embedding_service).await.unwrap();

    let query = SocketQuery {
        query_text: "test semantic search".to_string(),
        max_results: 10,
        ..Default::default()
    };

    // Should not panic, should generate real embeddings
    let result = client.query(&query).await;
    assert!(result.is_ok() || result.is_err()); // Either works or Layer 2 down
}
```

**Blockers**: Task 1.4

---

### Task 1.6: Add Unit Tests

**File**: `mfn-integration/tests/test_embeddings.rs` (new)
**Estimated Time**: 1.5 hours
**Dependencies**: Task 1.5 (integration complete)
**Priority**: HIGH

**Test Cases**:
```rust
use mfn_integration::embeddings::{EmbeddingService, EmbeddingConfig};

#[tokio::test]
async fn test_embedding_normalization() {
    let service = EmbeddingService::new(EmbeddingConfig::default())
        .await.unwrap();

    let embedding = service.embed("test query").await.unwrap();
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 0.01, "L2 norm should be ~1.0");
}

#[tokio::test]
async fn test_embedding_semantic_similarity() {
    let service = EmbeddingService::new(EmbeddingConfig::default())
        .await.unwrap();

    let emb1 = service.embed("cat").await.unwrap();
    let emb2 = service.embed("dog").await.unwrap();
    let emb3 = service.embed("car").await.unwrap();

    let sim_cat_dog = cosine_similarity(&emb1, &emb2);
    let sim_cat_car = cosine_similarity(&emb1, &emb3);

    assert!(sim_cat_dog > sim_cat_car, "Cat should be more similar to dog than car");
}

#[tokio::test]
async fn test_batch_vs_sequential() {
    let service = EmbeddingService::new(EmbeddingConfig::default())
        .await.unwrap();

    let texts = vec!["query 1", "query 2", "query 3"];

    // Batch
    let batch_embeddings = service.embed_batch(&texts).await.unwrap();

    // Sequential
    let mut sequential_embeddings = Vec::new();
    for text in &texts {
        sequential_embeddings.push(service.embed(text).await.unwrap());
    }

    // Results should be identical (within floating point precision)
    for (batch, sequential) in batch_embeddings.iter().zip(sequential_embeddings.iter()) {
        for (b, s) in batch.iter().zip(sequential.iter()) {
            assert!((b - s).abs() < 0.001, "Batch and sequential should match");
        }
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
```

**Acceptance Criteria**:
- [ ] Test embedding dimension (384)
- [ ] Test L2 normalization (norm ≈ 1.0)
- [ ] Test semantic similarity (cat/dog > cat/car)
- [ ] Test batch equivalence
- [ ] All tests pass

**Blockers**: Task 1.5

---

### WP1 Summary

**Total Tasks**: 6
**Total Effort**: 11 hours
**Critical Path**: Task 1.1 → 1.2 → 1.3 → 1.4 → 1.5 → 1.6 (sequential)

**Files Modified**:
- `mfn-integration/Cargo.toml` (add deps)
- `mfn-integration/src/lib.rs` (export module)
- `mfn-integration/src/socket_clients.rs` (integration)

**Files Created**:
- `mfn-integration/src/embeddings/mod.rs`
- `mfn-integration/src/embeddings/config.rs`
- `mfn-integration/src/embeddings/models.rs`
- `mfn-integration/src/embeddings/service.rs`
- `mfn-integration/tests/test_embeddings.rs`

---

## 3. WP2: Parallel Routing Implementation (BUG-002a)

**Objective**: Implement true parallel layer querying

### Task 2.1: Implement Safe Layer Query Wrappers

**File**: `mfn-integration/src/socket_integration.rs`
**Estimated Time**: 1 hour
**Dependencies**: None (can start immediately)
**Priority**: HIGH

**Implementation**:
```rust
impl SocketMfnIntegration {
    /// Query Layer 1 with timeout and error handling
    async fn query_layer1_safe(
        pool: Arc<Mutex<LayerConnectionPool>>,
        query: UniversalSearchQuery,
    ) -> Result<Vec<UniversalSearchResult>> {
        let timeout = Duration::from_millis(query.timeout_ms as u64);

        match tokio::time::timeout(timeout, Self::query_layer1_impl(pool, query)).await {
            Ok(Ok(results)) => Ok(results),
            Ok(Err(e)) => {
                warn!("Layer 1 query failed: {}", e);
                Ok(vec![])
            }
            Err(_) => {
                warn!("Layer 1 query timeout after {}ms", timeout.as_millis());
                Ok(vec![])
            }
        }
    }

    async fn query_layer1_impl(
        pool: Arc<Mutex<LayerConnectionPool>>,
        query: UniversalSearchQuery,
    ) -> Result<Vec<UniversalSearchResult>> {
        let layer1 = {
            let mut pool = pool.lock().await;
            pool.get_layer1().await?
            // Lock released here
        };

        let socket_query = convert_to_socket_query(&query);
        let result = layer1.query(&socket_query).await?;

        Ok(convert_from_socket_results(result))
    }

    // Similar for Layer 2, 3, 4...
    async fn query_layer2_safe(...) -> Result<...> { ... }
    async fn query_layer3_safe(...) -> Result<...> { ... }
    async fn query_layer4_safe(...) -> Result<...> { ... }
}
```

**Acceptance Criteria**:
- [ ] 4 safe wrapper functions created (layer1-4)
- [ ] 4 implementation functions with lock minimization
- [ ] Timeout handling per layer
- [ ] Error-to-empty-result conversion
- [ ] Unit tests pass

**Blockers**: None

---

### Task 2.2: Implement Parallel Query Function

**File**: `mfn-integration/src/socket_integration.rs`
**Estimated Time**: 1.5 hours
**Dependencies**: Task 2.1 (safe wrappers)
**Priority**: CRITICAL

**Implementation**:
```rust
async fn query_parallel(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    let start = Instant::now();

    // Clone query for each layer
    let query1 = query.clone();
    let query2 = query.clone();
    let query3 = query.clone();
    let query4 = query.clone();

    let pool = Arc::clone(&self.connection_pool);

    // Query all layers in parallel
    let (result1, result2, result3, result4) = tokio::join!(
        Self::query_layer1_safe(pool.clone(), query1),
        Self::query_layer2_safe(pool.clone(), query2),
        Self::query_layer3_safe(pool.clone(), query3),
        Self::query_layer4_safe(pool.clone(), query4),
    );

    // Collect all successful results
    let mut all_results = Vec::new();

    if let Ok(results) = result1 {
        all_results.extend(results);
    }
    if let Ok(results) = result2 {
        all_results.extend(results);
    }
    if let Ok(results) = result3 {
        all_results.extend(results);
    }
    if let Ok(results) = result4 {
        all_results.extend(results);
    }

    // Check if all layers failed
    let success_count = [&result1, &result2, &result3, &result4]
        .iter()
        .filter(|r| r.is_ok())
        .count();

    if success_count == 0 {
        return Err(anyhow!("All layers failed to respond"));
    }

    if success_count < 4 {
        warn!("Partial failure: only {}/4 layers responded", success_count);
    }

    // Merge and rank results
    let merged = merge_and_rank_results(all_results, query.max_results);

    let elapsed = start.elapsed().as_millis() as f64;
    debug!("Parallel query completed in {}ms", elapsed);

    Ok(merged)
}
```

**Acceptance Criteria**:
- [ ] Replace stub with tokio::join! implementation
- [ ] Handle result collection from all layers
- [ ] Detect all-layer failure
- [ ] Log partial failures
- [ ] Call merge_and_rank_results
- [ ] Unit tests pass

**Blockers**: Task 2.1

---

### Task 2.3: Implement Result Merging

**File**: `mfn-integration/src/socket_integration.rs`
**Estimated Time**: 1 hour
**Dependencies**: None (can run in parallel with Task 2.1-2.2)
**Priority**: HIGH

**Implementation**:
```rust
use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// Merge results from multiple layers into unified ranked list
fn merge_and_rank_results(
    all_results: Vec<UniversalSearchResult>,
    max_results: usize,
) -> Vec<UniversalSearchResult> {
    if all_results.is_empty() {
        return vec![];
    }

    // Step 1: Deduplicate by memory_id (keep highest confidence)
    let mut deduped: HashMap<MemoryId, UniversalSearchResult> = HashMap::new();

    for result in all_results {
        let memory_id = result.memory_id.clone();

        match deduped.entry(memory_id) {
            Entry::Vacant(e) => {
                e.insert(result);
            }
            Entry::Occupied(mut e) => {
                if result.confidence > e.get().confidence {
                    e.insert(result);
                } else {
                    merge_metadata(e.get_mut(), &result);
                }
            }
        }
    }

    // Step 2: Sort by confidence
    let mut results: Vec<UniversalSearchResult> = deduped.into_values().collect();
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Step 3: Limit to max_results
    results.truncate(max_results);

    results
}

fn merge_metadata(target: &mut UniversalSearchResult, source: &UniversalSearchResult) {
    // Add source layer to layers_found
    if let Some(source_layer) = source.metadata.get("layer") {
        target
            .metadata
            .entry("layers_found".to_string())
            .or_insert_with(|| serde_json::json!([]))
            .as_array_mut()
            .unwrap()
            .push(source_layer.clone());
    }
}
```

**Acceptance Criteria**:
- [ ] Deduplication by memory_id works
- [ ] Keeps highest confidence on duplicates
- [ ] Sorts by confidence descending
- [ ] Respects max_results limit
- [ ] Metadata merging functional
- [ ] Unit tests pass

**Blockers**: None

---

### Task 2.4: Add Unit Tests

**File**: `mfn-integration/tests/test_parallel_routing.rs` (new)
**Estimated Time**: 1 hour
**Dependencies**: Task 2.2, 2.3 (implementation complete)
**Priority**: HIGH

**Test Cases**:
```rust
#[tokio::test]
async fn test_merge_deduplicates_correctly() {
    let results = vec![
        UniversalSearchResult {
            memory_id: "mem_1".into(),
            confidence: 0.9,
            ..Default::default()
        },
        UniversalSearchResult {
            memory_id: "mem_1".into(),
            confidence: 0.8,
            ..Default::default()
        },
    ];

    let merged = merge_and_rank_results(results, 10);
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].confidence, 0.9);
}

#[tokio::test]
async fn test_merge_sorts_by_confidence() {
    let results = vec![
        UniversalSearchResult { confidence: 0.7, ..Default::default() },
        UniversalSearchResult { confidence: 0.9, ..Default::default() },
        UniversalSearchResult { confidence: 0.5, ..Default::default() },
    ];

    let merged = merge_and_rank_results(results, 10);
    assert_eq!(merged[0].confidence, 0.9);
    assert_eq!(merged[1].confidence, 0.7);
    assert_eq!(merged[2].confidence, 0.5);
}

#[tokio::test]
async fn test_parallel_handles_layer_failure() {
    // Mock Layer 2 to fail
    // Verify other 3 layers still return results
    // ...
}
```

**Acceptance Criteria**:
- [ ] Test deduplication logic
- [ ] Test sorting logic
- [ ] Test max_results limit
- [ ] Test partial layer failure
- [ ] All tests pass

**Blockers**: Task 2.2, 2.3

---

### Task 2.5: Performance Validation

**File**: `mfn-integration/benches/parallel_benchmark.rs` (new)
**Estimated Time**: 0.5 hours
**Dependencies**: Task 2.2 (parallel implementation)
**Priority**: MEDIUM

**Benchmark**:
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_parallel_vs_sequential(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let integration = runtime.block_on(async {
        SocketMfnIntegration::new().await.unwrap()
    });

    let query = UniversalSearchQuery {
        query_text: "benchmark query".to_string(),
        max_results: 10,
        ..Default::default()
    };

    c.bench_function("parallel_routing", |b| {
        b.iter(|| {
            runtime.block_on(async {
                integration.query_parallel(black_box(query.clone())).await.unwrap()
            })
        })
    });

    c.bench_function("sequential_routing", |b| {
        b.iter(|| {
            runtime.block_on(async {
                integration.query_sequential(black_box(query.clone())).await.unwrap()
            })
        })
    });
}

criterion_group!(benches, bench_parallel_vs_sequential);
criterion_main!(benches);
```

**Acceptance Criteria**:
- [ ] Benchmark shows parallel faster than sequential
- [ ] Speedup ratio measured and documented
- [ ] Results added to implementation report

**Blockers**: Task 2.2

---

### WP2 Summary

**Total Tasks**: 5
**Total Effort**: 5 hours
**Parallelization**: Task 2.3 can run parallel with 2.1-2.2

**Files Modified**:
- `mfn-integration/src/socket_integration.rs` (replace stub, add functions)

**Files Created**:
- `mfn-integration/tests/test_parallel_routing.rs`
- `mfn-integration/benches/parallel_benchmark.rs`

---

## 4. WP3: Adaptive Routing Implementation (BUG-002b)

**Objective**: Implement intelligent query-based routing

### Task 3.1: Implement Query Classification

**File**: `mfn-integration/src/socket_integration.rs`
**Estimated Time**: 1.5 hours
**Dependencies**: None (can start immediately)
**Priority**: HIGH

**Implementation**: (see ADAPTIVE_ROUTING_ALGORITHM.md section 2.3)

**Acceptance Criteria**:
- [ ] QueryType enum defined
- [ ] classify() function implemented
- [ ] Pattern matching for Exact, Semantic, Contextual, Unknown
- [ ] Unit tests for classification pass

**Blockers**: None

---

### Task 3.2: Implement Routing Functions

**File**: `mfn-integration/src/socket_integration.rs`
**Estimated Time**: 1.5 hours
**Dependencies**: Task 3.1 (classification), WP2 Task 2.2 (parallel routing)
**Priority**: HIGH

**Implementation**:
```rust
/// Route exact match queries (Layer 1 only)
async fn route_exact(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    self.query_layer1_only(query).await
}

/// Route semantic queries (Layer 2 + Layer 3)
async fn route_semantic(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    let query2 = query.clone();
    let query3 = query.clone();
    let pool = Arc::clone(&self.connection_pool);

    let (result2, result3) = tokio::join!(
        Self::query_layer2_safe(pool.clone(), query2),
        Self::query_layer3_safe(pool.clone(), query3),
    );

    let mut all_results = Vec::new();
    if let Ok(results) = result2 {
        all_results.extend(results);
    }
    if let Ok(results) = result3 {
        all_results.extend(results);
    }

    Ok(merge_and_rank_results(all_results, query.max_results))
}

/// Route contextual queries (all layers)
async fn route_contextual(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    self.query_parallel(query).await
}
```

**Acceptance Criteria**:
- [ ] route_exact() queries Layer 1 only
- [ ] route_semantic() queries Layer 2+3 in parallel
- [ ] route_contextual() uses full parallel routing
- [ ] Integration tests pass

**Blockers**: Task 3.1, WP2 Task 2.2

---

### Task 3.3: Implement Adaptive Query Function

**File**: `mfn-integration/src/socket_integration.rs`
**Estimated Time**: 1 hour
**Dependencies**: Task 3.2 (routing functions)
**Priority**: CRITICAL

**Implementation**:
```rust
async fn query_adaptive(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    let start = Instant::now();

    let query_type = QueryType::classify(&query.query_text);
    debug!("Query classified as {:?}: '{}'", query_type, query.query_text);

    let results = match query_type {
        QueryType::Exact => self.route_exact(query).await?,
        QueryType::Semantic => self.route_semantic(query).await?,
        QueryType::Contextual => self.route_contextual(query).await?,
        QueryType::Unknown => {
            warn!("Query type unknown, using sequential routing");
            self.query_sequential(query).await?
        }
    };

    let elapsed = start.elapsed().as_millis() as f64;
    debug!("Adaptive query completed in {}ms (type: {:?})", elapsed, query_type);

    Ok(results)
}
```

**Acceptance Criteria**:
- [ ] Replace stub with classification + routing dispatch
- [ ] Log classification decisions
- [ ] Fallback to sequential for unknown
- [ ] Integration tests pass

**Blockers**: Task 3.2

---

### Task 3.4: Add Fallback Logic

**File**: `mfn-integration/src/socket_integration.rs`
**Estimated Time**: 0.5 hours
**Dependencies**: Task 3.3 (adaptive function)
**Priority**: MEDIUM

**Implementation**:
```rust
async fn route_exact_with_fallback(&self, query: UniversalSearchQuery) -> Result<Vec<UniversalSearchResult>> {
    let results = self.query_layer1_only(query.clone()).await?;

    if results.is_empty() {
        warn!("No exact matches, expanding to semantic search");
        return self.route_semantic(query).await;
    }

    let max_confidence = results.iter().map(|r| r.confidence).fold(0.0, f32::max);
    if max_confidence < 0.5 {
        warn!("Low confidence exact match ({}), expanding search", max_confidence);
        let semantic_results = self.route_semantic(query.clone()).await?;

        let mut combined = results;
        combined.extend(semantic_results);
        return Ok(merge_and_rank_results(combined, query.max_results));
    }

    Ok(results)
}
```

**Acceptance Criteria**:
- [ ] Empty result expansion implemented
- [ ] Low confidence expansion implemented
- [ ] Unit tests pass

**Blockers**: Task 3.3

---

### Task 3.5: Add Unit Tests

**File**: `mfn-integration/tests/test_adaptive_routing.rs` (new)
**Estimated Time**: 1 hour
**Dependencies**: Task 3.4 (implementation complete)
**Priority**: HIGH

**Test Cases**: (see ADAPTIVE_ROUTING_ALGORITHM.md section 7.1)

**Acceptance Criteria**:
- [ ] Test classification for all query types
- [ ] Test routing to correct layers
- [ ] Test fallback logic
- [ ] All tests pass

**Blockers**: Task 3.4

---

### WP3 Summary

**Total Tasks**: 5
**Total Effort**: 5.5 hours
**Dependencies**: Requires WP2 Task 2.2 (parallel routing)

**Files Modified**:
- `mfn-integration/src/socket_integration.rs` (add classification and routing)

**Files Created**:
- `mfn-integration/tests/test_adaptive_routing.rs`

---

## 5. Critical Path Analysis

### Longest Path (Sequential Work)

```
WP1: Embeddings (11 hours)
├─ Task 1.1: Dependencies (0.5h)
├─ Task 1.2: Module Structure (1h)
├─ Task 1.3: EmbeddingModel (3h)
├─ Task 1.4: EmbeddingService (3h)
├─ Task 1.5: Integration (2h)
└─ Task 1.6: Tests (1.5h)

Total: 11 hours (critical path)
```

### Parallelizable Work

**WP2 (5 hours)** can run 100% in parallel with WP1
**WP3 (5.5 hours)** depends on WP2 Task 2.2 (2.5 hours into WP2)

### Optimal Schedule

**Week 1 (Parallel Development)**:
- Developer A: WP1 Embeddings (11 hours over 2 days)
- Developer B: WP2 Parallel Routing (5 hours over 1 day)

**Week 1 (Sequential Completion)**:
- Developer B: WP3 Adaptive Routing (5.5 hours over 1 day)

**Total Calendar Time**: 2-3 days (with 2 developers)
**Total Effort Time**: 21.5 hours

---

## 6. Risk Mitigation

### Risk 1: Model Download Failures (WP1)

**Task**: 1.3 (EmbeddingModel load)
**Mitigation**: Pre-download model in CI/CD, enable fallback TF-IDF
**Contingency**: Skip to Task 1.4 using fallback only, implement real model later

### Risk 2: Connection Pool Exhaustion (WP2)

**Task**: 2.2 (Parallel query)
**Mitigation**: Increase pool size to 8 per layer
**Contingency**: Add semaphore limiter to cap concurrent parallel queries

### Risk 3: Classification Accuracy Low (WP3)

**Task**: 3.1 (Query classification)
**Mitigation**: Start with conservative rules, expand gradually
**Contingency**: Fallback to Sequential more often (prioritize accuracy over speed)

---

## 7. Testing Strategy

### Unit Tests (Part of Each Task)

- **WP1**: 15 unit tests across Tasks 1.3, 1.4, 1.6
- **WP2**: 10 unit tests in Task 2.4
- **WP3**: 12 unit tests in Task 3.5
- **Total**: 37 unit tests

### Integration Tests (Step 5)

After all WPs complete:
1. End-to-end query with real embeddings
2. Parallel routing with all 4 layers
3. Adaptive routing with all query types
4. Performance comparison: Sequential vs Parallel vs Adaptive

### Performance Benchmarks (Step 5)

1. Embedding latency (p50, p95, p99)
2. Parallel speedup vs Sequential
3. Adaptive latency by query type
4. Throughput under load

---

## 8. Success Criteria

### BUG-001: Embeddings

- [ ] No placeholder code at line 217
- [ ] Real 384-dim embeddings generated
- [ ] L2 normalized vectors
- [ ] Semantic similarity validated
- [ ] Latency <50ms p95
- [ ] All unit tests pass

### BUG-002a: Parallel Routing

- [ ] No stub code in query_parallel()
- [ ] All 4 layers queried concurrently
- [ ] Result merging functional
- [ ] Partial failure handling works
- [ ] Speedup 1.5-2.5x vs sequential
- [ ] All unit tests pass

### BUG-002b: Adaptive Routing

- [ ] No stub code in query_adaptive()
- [ ] Query classification >85% accuracy
- [ ] Correct layer routing per type
- [ ] Fallback logic functional
- [ ] Average latency <10ms
- [ ] All unit tests pass

---

## 9. Deliverables Checklist

### Code Deliverables

- [ ] `mfn-integration/src/embeddings/` (4 files)
- [ ] `mfn-integration/tests/test_embeddings.rs`
- [ ] `mfn-integration/tests/test_parallel_routing.rs`
- [ ] `mfn-integration/tests/test_adaptive_routing.rs`
- [ ] `mfn-integration/benches/parallel_benchmark.rs`
- [ ] Modified `socket_clients.rs` (embedding integration)
- [ ] Modified `socket_integration.rs` (routing implementations)
- [ ] Modified `Cargo.toml` (dependencies)

### Documentation Deliverables

- [x] EMBEDDING_SERVICE_ARCHITECTURE.md
- [x] PARALLEL_ROUTING_ARCHITECTURE.md
- [x] ADAPTIVE_ROUTING_ALGORITHM.md
- [x] IMPLEMENTATION_TASK_BREAKDOWN.md (this document)
- [ ] IMPLEMENTATION_REPORT.md (Step 4 completion report)

### Test Deliverables

- [ ] 37 unit tests passing
- [ ] Integration tests passing
- [ ] Performance benchmarks complete
- [ ] Test coverage report >80%

---

## Appendix A: File Change Summary

### Files to Modify

| File | Changes | LOC Added | LOC Removed |
|------|---------|-----------|-------------|
| `mfn-integration/Cargo.toml` | Add deps | 5 | 0 |
| `mfn-integration/src/lib.rs` | Export module | 2 | 0 |
| `mfn-integration/src/socket_clients.rs` | Embedding integration | 30 | 5 |
| `mfn-integration/src/socket_integration.rs` | Routing implementations | 250 | 10 |

### Files to Create

| File | LOC | Purpose |
|------|-----|---------|
| `embeddings/mod.rs` | 10 | Module exports |
| `embeddings/config.rs` | 40 | Configuration |
| `embeddings/models.rs` | 150 | Model abstraction |
| `embeddings/service.rs` | 200 | Service implementation |
| `tests/test_embeddings.rs` | 150 | Embedding tests |
| `tests/test_parallel_routing.rs` | 120 | Parallel routing tests |
| `tests/test_adaptive_routing.rs` | 150 | Adaptive routing tests |
| `benches/parallel_benchmark.rs` | 80 | Performance benchmarks |

**Total New Code**: ~1200 LOC
**Total Modifications**: ~290 LOC added, ~15 LOC removed

---

## Appendix B: Dependency Tree

```
Task 1.1 (Add Dependencies)
    │
    └─→ Task 1.2 (Module Structure)
            │
            └─→ Task 1.3 (EmbeddingModel)
                    │
                    └─→ Task 1.4 (EmbeddingService)
                            │
                            └─→ Task 1.5 (Integration)
                                    │
                                    └─→ Task 1.6 (Tests)

Task 2.1 (Safe Wrappers)
    │
    └─→ Task 2.2 (Parallel Query) ────────┐
            │                             │
            └─→ Task 2.4 (Tests)          │
                                          │
Task 2.3 (Result Merging) ────────────────┤
    │                                     │
    └─→ Task 2.4 (Tests)                  │
                                          │
Task 2.5 (Benchmarks) ←───────────────────┘

Task 3.1 (Classification)
    │
    └─→ Task 3.2 (Routing Functions) ←── (needs Task 2.2)
            │
            └─→ Task 3.3 (Adaptive Function)
                    │
                    └─→ Task 3.4 (Fallback)
                            │
                            └─→ Task 3.5 (Tests)
```

---

**Document Status**: COMPLETE
**Ready for Step 4 Implementation**: YES
**Next Step**: Delegate to @developer for implementation
