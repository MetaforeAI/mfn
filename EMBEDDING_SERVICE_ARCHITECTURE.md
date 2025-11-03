# Embedding Service Architecture
**Phase 1, Step 2: Definition & Scoping**
**Bug**: BUG-001 - Placeholder Embeddings
**Date**: 2025-11-02
**Author**: Integration Agent

---

## Executive Summary

This document specifies the production-ready architecture for semantic embedding generation in the MFN system. The embedding service will replace the placeholder `vec![0.1; 128]` implementation with real sentence transformer embeddings, enabling Layer 2 DSR's neural similarity search to function correctly.

**Key Requirements**:
- Generate 384-dimensional semantic embeddings from query text
- Target latency: <50ms p95 per embedding
- Model: `all-MiniLM-L6-v2` (384-dim, 23M params, 90MB)
- Library: `rust-bert` with fallback to `fastembed-rs`
- Integration point: `mfn-integration/src/socket_clients.rs:217`

---

## 1. Model Selection

### Primary: all-MiniLM-L6-v2

**Specifications**:
- Model: `sentence-transformers/all-MiniLM-L6-v2`
- Architecture: BERT-based sentence transformer
- Output dimensions: 384
- Model parameters: 23M
- Model size: ~90MB download
- Pooling: Mean pooling across token embeddings
- Normalization: L2 normalization (unit vectors)

**Performance Characteristics**:
- Encoding speed: ~500 sentences/second on CPU
- Latency: 10-50ms per query (single sentence)
- Batch latency: 5-20ms per sentence (batches of 8-32)
- Memory footprint: ~150MB loaded (model + tokenizer + buffers)
- Quality: 0.78 average performance on STS benchmark

**Why MiniLM-L6-v2**:
1. **Optimal speed/quality tradeoff**: Fastest sentence transformer with acceptable quality
2. **384-dim output**: Matches Layer 2 DSR default configuration
3. **Industry standard**: Most widely deployed small sentence encoder
4. **Lightweight**: 90MB model fits in memory easily
5. **Well-supported**: Available in HuggingFace, ONNX, rust-bert

### Library Evaluation

#### Option A: rust-bert (RECOMMENDED)

**Repository**: `https://github.com/guillaume-be/rust-bert`
**Version**: 0.22.0+
**License**: Apache 2.0

**Pros**:
- Native Rust implementation using PyTorch C++ bindings
- Supports sentence transformers directly
- Model auto-download from HuggingFace Hub
- Good performance with CPU/GPU support
- Active maintenance and community

**Cons**:
- Heavy dependencies (~300MB compiled)
- Requires libtorch backend (~500MB download)
- Complex setup for cross-platform builds
- Slower compilation times

**Implementation Complexity**: Medium-High

#### Option B: fastembed-rs

**Repository**: `https://github.com/Anush008/fastembed-rs`
**Version**: 3.0.0+
**License**: Apache 2.0

**Pros**:
- ONNX Runtime backend (lighter than PyTorch)
- Pre-quantized models for faster inference
- Simpler dependency tree
- Faster compilation
- Better cross-platform support

**Cons**:
- Newer library with smaller community
- Limited model selection
- Less flexibility for custom models
- Documentation still developing

**Implementation Complexity**: Low-Medium

#### Option C: candle + sentence-transformers

**Repository**: `https://github.com/huggingface/candle`
**Version**: 0.3.0+
**License**: Apache 2.0/MIT

**Pros**:
- Pure Rust ML framework (no C++ deps)
- Lightweight and fast
- Growing ecosystem
- HuggingFace official support

**Cons**:
- Very new (requires manual BERT implementation)
- Sentence transformer support not mature
- Needs custom tokenizer integration
- More development effort required

**Implementation Complexity**: High

### Recommended Approach: Hybrid Strategy

**Phase 1 (MVP)**: Use `fastembed-rs`
- Fastest time to production (2-3 hours implementation)
- Simpler dependency management
- Good enough performance for initial deployment

**Phase 2 (Optimization)**: Migrate to `rust-bert` if needed
- Better long-term support and flexibility
- More model options
- GPU acceleration potential

---

## 2. Architecture Components

### 2.1 Module Structure

```
mfn-integration/
├── src/
│   ├── embeddings/
│   │   ├── mod.rs              # Public API
│   │   ├── service.rs          # EmbeddingService implementation
│   │   ├── models.rs           # Model loading and caching
│   │   ├── batch.rs            # Batch processing optimization
│   │   └── config.rs           # Configuration structs
│   ├── socket_clients.rs       # Modified to use embeddings
│   └── lib.rs                  # Export embeddings module
```

### 2.2 Core Components

#### EmbeddingService (Primary Interface)

```rust
pub struct EmbeddingService {
    model: Arc<EmbeddingModel>,
    config: EmbeddingConfig,
    metrics: Arc<Mutex<EmbeddingMetrics>>,
}

impl EmbeddingService {
    /// Initialize service with model download/cache
    pub async fn new(config: EmbeddingConfig) -> Result<Self>;

    /// Generate embedding for single text
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for batch (optimization)
    pub async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Warmup model (pre-load and test)
    pub async fn warmup(&self) -> Result<()>;

    /// Get performance metrics
    pub fn metrics(&self) -> EmbeddingMetrics;
}
```

#### EmbeddingModel (Model Abstraction)

```rust
enum EmbeddingModel {
    FastEmbed(TextEmbedding),      // fastembed-rs backend
    RustBert(SentenceEncoder),     // rust-bert backend (future)
    Fallback(TfIdfVectorizer),     // TF-IDF fallback
}

impl EmbeddingModel {
    /// Load model from cache or download
    fn load(model_name: &str) -> Result<Self>;

    /// Generate embeddings (internal)
    fn encode(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Get embedding dimension
    fn dimension(&self) -> usize;
}
```

#### EmbeddingConfig

```rust
pub struct EmbeddingConfig {
    /// Model identifier (e.g., "all-MiniLM-L6-v2")
    pub model_name: String,

    /// Model cache directory (default: ~/.cache/mfn/models)
    pub cache_dir: PathBuf,

    /// Enable batch processing optimization
    pub enable_batching: bool,

    /// Batch size for parallel queries
    pub batch_size: usize,

    /// Enable L2 normalization (required for Layer 2)
    pub normalize: bool,

    /// Fallback to TF-IDF on model load failure
    pub enable_fallback: bool,

    /// Model download timeout
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
            download_timeout_secs: 300, // 5 minutes
        }
    }
}
```

#### EmbeddingMetrics

```rust
pub struct EmbeddingMetrics {
    pub total_embeddings: u64,
    pub total_time_ms: f64,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub cache_hits: u64,
    pub model_load_time_ms: f64,
}
```

---

## 3. Integration Architecture

### 3.1 Current State (Placeholder)

```
┌─────────────────────┐
│  Query Request      │
│  text: "example"    │
└──────────┬──────────┘
           │
           ▼
┌─────────────────────────────┐
│  socket_clients.rs:217      │
│  vec![0.1f32; 128]          │  ❌ PLACEHOLDER
└──────────┬──────────────────┘
           │ Wrong dimension (128 vs 384)
           │ No semantic meaning
           ▼
┌─────────────────────────────┐
│  Layer 2 Socket Client      │
│  Send to Layer 2 DSR        │
└──────────┬──────────────────┘
           │
           ▼
┌─────────────────────────────┐
│  Layer 2 DSR Server         │
│  Similarity search BROKEN   │  ❌ All queries identical
└─────────────────────────────┘
```

### 3.2 Future State (Production)

```
┌─────────────────────┐
│  Query Request      │
│  text: "example"    │
└──────────┬──────────┘
           │
           ▼
┌──────────────────────────────┐
│  EmbeddingService::embed()   │
│  1. Tokenize text            │
│  2. BERT forward pass        │
│  3. Mean pooling             │
│  4. L2 normalize             │
└──────────┬───────────────────┘
           │ 384-dim semantic vector
           │ [0.0234, -0.1234, 0.0567, ...]
           ▼
┌──────────────────────────────┐
│  Layer 2 Socket Client       │
│  Send embedding to DSR       │
└──────────┬───────────────────┘
           │
           ▼
┌──────────────────────────────┐
│  Layer 2 DSR Server          │
│  1. Spike encode embedding   │
│  2. Reservoir dynamics       │
│  3. Similarity search        │
│  4. Return top-k results     │
└──────────────────────────────┘
           │
           ▼
┌──────────────────────────────┐
│  Differentiated Results      │  ✅ Real similarity scores
│  confidence: 0.85, 0.73...   │
└──────────────────────────────┘
```

### 3.3 Integration Point Modification

**File**: `mfn-integration/src/socket_clients.rs`
**Line**: 217
**Current**:
```rust
// Generate query embedding (simplified - real implementation would use actual encoding)
let query_embedding = vec![0.1f32; 128]; // Placeholder embedding
```

**Future**:
```rust
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

**Required Changes**:
1. Add `embedding_service: Arc<EmbeddingService>` field to `Layer2Client` struct
2. Pass embedding service during client initialization
3. Replace placeholder with real embedding generation
4. Add dimension validation
5. Add error handling for embedding failures

---

## 4. Model Loading & Caching Strategy

### 4.1 Model Download Flow

```
Application Startup
    │
    ▼
┌─────────────────────────────┐
│  Check Cache Directory      │
│  ~/.cache/mfn/models/       │
└──────────┬──────────────────┘
           │
    ┌──────┴──────┐
    │ Exists?     │
    └──────┬──────┘
           │
     ┌─────┴─────┐
    Yes          No
     │            │
     │            ▼
     │   ┌────────────────────┐
     │   │  Download Model    │
     │   │  from HuggingFace  │
     │   │  - config.json     │
     │   │  - tokenizer.json  │
     │   │  - model.onnx      │
     │   └─────────┬──────────┘
     │             │
     │             ▼
     │   ┌────────────────────┐
     │   │  Save to Cache     │
     │   └─────────┬──────────┘
     │             │
     └─────────────┘
                   │
                   ▼
          ┌────────────────────┐
          │  Load Model into   │
          │  Memory (~150MB)   │
          └─────────┬──────────┘
                    │
                    ▼
          ┌────────────────────┐
          │  Model Warmup      │
          │  (test embedding)  │
          └─────────┬──────────┘
                    │
                    ▼
          ┌────────────────────┐
          │  Service Ready     │
          └────────────────────┘
```

### 4.2 Caching Implementation

```rust
impl EmbeddingService {
    async fn load_model(config: &EmbeddingConfig) -> Result<EmbeddingModel> {
        let cache_path = config.cache_dir.join(&config.model_name);

        // Check if model exists in cache
        if cache_path.exists() {
            info!("Loading model from cache: {:?}", cache_path);
            match EmbeddingModel::from_cache(&cache_path) {
                Ok(model) => return Ok(model),
                Err(e) => {
                    warn!("Cache load failed: {}, re-downloading", e);
                    // Continue to download
                }
            }
        }

        // Download model from HuggingFace
        info!("Downloading model: {}", config.model_name);
        let model = EmbeddingModel::download(
            &config.model_name,
            &cache_path,
            config.download_timeout_secs
        ).await?;

        // Verify model works
        model.encode(&["test"])?;

        Ok(model)
    }
}
```

### 4.3 Model Warmup Strategy

**Purpose**: Pre-load model into memory and verify functionality before accepting queries

```rust
impl EmbeddingService {
    pub async fn warmup(&self) -> Result<()> {
        let start = Instant::now();

        // Test embedding generation
        let test_texts = vec![
            "Test embedding generation",
            "Quick brown fox",
            "The quick brown fox jumps over the lazy dog",
        ];

        for text in test_texts {
            let embedding = self.embed(text).await?;

            // Verify dimension
            if embedding.len() != 384 {
                return Err(anyhow!(
                    "Warmup failed: wrong dimension {}",
                    embedding.len()
                ));
            }

            // Verify normalization (L2 norm ≈ 1.0)
            let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            if (norm - 1.0).abs() > 0.01 {
                return Err(anyhow!(
                    "Warmup failed: not normalized (norm={})",
                    norm
                ));
            }
        }

        let elapsed = start.elapsed().as_millis();
        info!("Model warmup completed in {}ms", elapsed);

        Ok(())
    }
}
```

**When to Run Warmup**:
1. During `EmbeddingService::new()` initialization
2. After model download completes
3. Before starting query processing threads
4. In health check endpoints

---

## 5. Performance Optimization

### 5.1 Batching Strategy

**Motivation**: Processing multiple queries in a batch is 3-5x more efficient than sequential single queries

```rust
pub struct BatchProcessor {
    pending_requests: Vec<PendingEmbedding>,
    batch_size: usize,
    max_wait_ms: u64,
}

struct PendingEmbedding {
    text: String,
    sender: oneshot::Sender<Result<Vec<f32>>>,
    timestamp: Instant,
}

impl BatchProcessor {
    async fn process_loop(&mut self, service: Arc<EmbeddingService>) {
        loop {
            // Wait for batch to fill or timeout
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(self.max_wait_ms)) => {
                    // Timeout: process partial batch
                    if !self.pending_requests.is_empty() {
                        self.flush_batch(&service).await;
                    }
                }
                _ = self.wait_for_batch_full() => {
                    // Batch full: process immediately
                    self.flush_batch(&service).await;
                }
            }
        }
    }

    async fn flush_batch(&mut self, service: &EmbeddingService) {
        let batch = std::mem::take(&mut self.pending_requests);
        let texts: Vec<&str> = batch.iter().map(|p| p.text.as_str()).collect();

        match service.embed_batch(&texts).await {
            Ok(embeddings) => {
                // Send results back to requesters
                for (pending, embedding) in batch.into_iter().zip(embeddings) {
                    let _ = pending.sender.send(Ok(embedding));
                }
            }
            Err(e) => {
                // Send error to all requesters
                for pending in batch {
                    let _ = pending.sender.send(Err(anyhow!("Batch failed: {}", e)));
                }
            }
        }
    }
}
```

**Batching Configuration**:
- **Batch size**: 8-16 queries (optimal for MiniLM on CPU)
- **Max wait time**: 10-20ms (balance latency vs throughput)
- **Use case**: High-load scenarios (>100 req/s)
- **Disable for**: Low-load or strict latency requirements

### 5.2 Connection Pool Integration

The embedding service should be shared across all Layer2Client instances in the connection pool:

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

        // Create clients with shared service
        let layer2_clients = (0..4)
            .map(|_| SocketLayer2::new(Arc::clone(&embedding_service)))
            .collect::<Result<Vec<_>>>()?;

        // ... rest of initialization
    }
}
```

**Benefits**:
- Model loaded once (150MB memory)
- No contention on model access (Arc + internal locking)
- Consistent performance across all clients

### 5.3 Memory Management

**Total Memory Footprint**:
- Model weights: ~90MB
- Tokenizer: ~5MB
- Runtime buffers: ~30-50MB per concurrent request
- Total: ~150MB + (50MB × num_concurrent_requests)

**Memory Optimization Strategies**:
1. **Limit concurrent embeddings**: Use semaphore to cap concurrent requests
2. **Clear buffers**: Reuse buffers between requests
3. **Lazy loading**: Load model on first query (trade startup time for memory)

```rust
impl EmbeddingService {
    // Limit concurrent embeddings to prevent OOM
    const MAX_CONCURRENT: usize = 4;

    async fn embed_with_limit(&self, text: &str) -> Result<Vec<f32>> {
        let _permit = self.concurrency_limiter.acquire().await?;
        self.embed_internal(text).await
    }
}
```

---

## 6. Error Handling & Fallback

### 6.1 Error Scenarios

| Error | Cause | Mitigation |
|-------|-------|------------|
| **ModelDownloadFailed** | Network timeout, HuggingFace unavailable | Retry with backoff, use cached model, enable fallback |
| **ModelLoadFailed** | Corrupted cache, incompatible format | Clear cache, re-download, use fallback |
| **EncodingFailed** | Input too long, invalid UTF-8 | Truncate input, sanitize text, return error |
| **OutOfMemory** | Too many concurrent requests | Limit concurrency, reduce batch size, return 503 |
| **ModelNotFound** | Invalid model name | Use default model, return error |

### 6.2 Fallback Strategy

If model loading fails, fall back to TF-IDF vectorization:

```rust
impl EmbeddingModel {
    fn load_with_fallback(config: &EmbeddingConfig) -> Result<Self> {
        match Self::load_fastembed(&config.model_name) {
            Ok(model) => Ok(Self::FastEmbed(model)),
            Err(e) if config.enable_fallback => {
                warn!("Model load failed: {}, using TF-IDF fallback", e);
                Ok(Self::Fallback(TfIdfVectorizer::new(384)))
            }
            Err(e) => Err(e),
        }
    }
}

struct TfIdfVectorizer {
    dim: usize,
    hasher: ahash::AHasher,
}

impl TfIdfVectorizer {
    fn encode(&self, text: &str) -> Vec<f32> {
        // Simple hash-based TF-IDF approximation
        let mut vec = vec![0.0f32; self.dim];

        for token in text.split_whitespace() {
            let hash = self.hasher.hash_one(token) as usize % self.dim;
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

**Fallback Quality**:
- **Accuracy**: ~30-40% vs 78% for real embeddings
- **Speed**: <0.5ms vs 10-50ms
- **Use case**: Emergency fallback only, not production default

### 6.3 Graceful Degradation

```rust
impl Layer2Client {
    async fn query_with_fallback(&self, query: &SocketQuery) -> Result<SocketQueryResult> {
        match self.embedding_service.embed(&query.query_text).await {
            Ok(embedding) => {
                // Normal path: use real embedding
                self.query_dsr(query, embedding).await
            }
            Err(e) => {
                // Fallback: skip Layer 2, return empty results
                warn!("Embedding failed, skipping Layer 2: {}", e);
                Ok(SocketQueryResult {
                    results: vec![],
                    processing_time_ms: 0.0,
                    layer: "Layer2".to_string(),
                })
            }
        }
    }
}
```

---

## 7. Testing Strategy

### 7.1 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedding_dimension() {
        let service = EmbeddingService::new(EmbeddingConfig::default())
            .await.unwrap();

        let embedding = service.embed("test query").await.unwrap();
        assert_eq!(embedding.len(), 384);
    }

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

        // Cat and dog should be more similar than cat and car
        assert!(sim_cat_dog > sim_cat_car);
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

### 7.2 Integration Tests

```rust
#[tokio::test]
async fn test_layer2_with_real_embeddings() {
    // Initialize embedding service
    let embedding_service = Arc::new(
        EmbeddingService::new(EmbeddingConfig::default()).await.unwrap()
    );

    // Create Layer 2 client
    let client = Layer2Client::new(embedding_service).await.unwrap();

    // Query with different texts
    let query1 = SocketQuery {
        query_text: "search for cats".to_string(),
        max_results: 10,
        ..Default::default()
    };

    let query2 = SocketQuery {
        query_text: "search for dogs".to_string(),
        max_results: 10,
        ..Default::default()
    };

    let results1 = client.query(&query1).await.unwrap();
    let results2 = client.query(&query2).await.unwrap();

    // Results should be different (not all identical)
    assert_ne!(results1, results2, "Embeddings should differ");
}
```

### 7.3 Performance Benchmarks

```rust
#[bench]
fn bench_embedding_latency(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let service = runtime.block_on(async {
        EmbeddingService::new(EmbeddingConfig::default()).await.unwrap()
    });

    b.iter(|| {
        runtime.block_on(async {
            service.embed("test query for benchmarking").await.unwrap()
        })
    });
}

#[bench]
fn bench_batch_embedding(b: &mut Bencher) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let service = runtime.block_on(async {
        EmbeddingService::new(EmbeddingConfig::default()).await.unwrap()
    });

    let texts: Vec<&str> = (0..16).map(|i| "test query").collect();

    b.iter(|| {
        runtime.block_on(async {
            service.embed_batch(&texts).await.unwrap()
        })
    });
}
```

---

## 8. Deployment Considerations

### 8.1 Model Pre-download

**Production Deployment Checklist**:
1. Pre-download model during Docker image build
2. Verify model integrity in CI/CD pipeline
3. Monitor model cache directory size
4. Set up model CDN mirror for faster downloads

**Dockerfile Example**:
```dockerfile
# Pre-download embedding model
RUN mkdir -p /app/.cache/mfn/models && \
    python3 -c "from sentence_transformers import SentenceTransformer; \
    SentenceTransformer('sentence-transformers/all-MiniLM-L6-v2', \
    cache_folder='/app/.cache/mfn/models')"
```

### 8.2 Health Checks

```rust
impl EmbeddingService {
    pub async fn health_check(&self) -> Result<HealthStatus> {
        let start = Instant::now();

        // Test embedding generation
        match self.embed("health check").await {
            Ok(embedding) => {
                let latency = start.elapsed().as_millis();

                if latency > 100 {
                    Ok(HealthStatus::Degraded {
                        reason: format!("High latency: {}ms", latency),
                    })
                } else {
                    Ok(HealthStatus::Healthy)
                }
            }
            Err(e) => Ok(HealthStatus::Unhealthy {
                reason: format!("Embedding failed: {}", e),
            }),
        }
    }
}

pub enum HealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { reason: String },
}
```

### 8.3 Monitoring Metrics

**Key Metrics to Track**:
- `embedding_latency_ms` (p50, p95, p99)
- `embedding_throughput_rps`
- `embedding_errors_total`
- `model_load_time_ms`
- `model_memory_bytes`
- `batch_size_actual` (histogram)

```rust
impl EmbeddingService {
    async fn embed_with_metrics(&self, text: &str) -> Result<Vec<f32>> {
        let start = Instant::now();

        match self.embed_internal(text).await {
            Ok(embedding) => {
                let latency = start.elapsed().as_secs_f64() * 1000.0;

                // Update metrics
                let mut metrics = self.metrics.lock().await;
                metrics.total_embeddings += 1;
                metrics.total_time_ms += latency;
                metrics.latencies.push(latency);

                Ok(embedding)
            }
            Err(e) => {
                // Track errors
                self.error_counter.inc();
                Err(e)
            }
        }
    }
}
```

---

## 9. Implementation Dependencies

### 9.1 Cargo.toml Changes

**mfn-integration/Cargo.toml**:
```toml
[dependencies]
# ... existing deps ...

# Embedding libraries (choose one)
fastembed = "3.0"           # RECOMMENDED for Phase 1
# rust-bert = "0.22"        # Alternative for Phase 2

# For fallback TF-IDF
ahash = "0.8"               # Fast hashing
```

### 9.2 Required Crates

| Crate | Version | Purpose | Size |
|-------|---------|---------|------|
| `fastembed` | 3.0+ | ONNX-based sentence transformers | ~50MB |
| `ort` (via fastembed) | 1.16+ | ONNX Runtime bindings | ~30MB |
| `tokenizers` (via fastembed) | 0.15+ | HuggingFace tokenizers | ~5MB |
| `ahash` | 0.8+ | Fast hashing for fallback | <1MB |

**Total Dependency Size**: ~85MB compiled

---

## 10. Performance Analysis

### 10.1 Latency Breakdown

**Single Embedding Generation**:
```
Total: ~30ms (p95)
├── Tokenization:      5-10ms
├── Model forward:     15-20ms
├── Pooling:          1-2ms
└── Normalization:     <1ms
```

**Batch Embedding (16 queries)**:
```
Total: ~60ms (p95)
├── Tokenization:      10-15ms (parallel)
├── Model forward:     40-45ms (batched)
├── Pooling:          2-3ms
└── Normalization:     <1ms

Per-query: ~4ms (15x faster than sequential)
```

### 10.2 Throughput Estimation

**Single-threaded**:
- Sequential: ~33 req/s (30ms per embedding)
- Batched (16): ~267 req/s (4ms per embedding)

**4-core CPU**:
- Sequential: ~130 req/s
- Batched: ~1000 req/s

**Target**: 100-200 req/s with batching enabled

### 10.3 Comparison with Placeholder

| Metric | Placeholder | Real Embeddings | Delta |
|--------|-------------|-----------------|-------|
| Latency | <0.1ms | 30ms | +300x |
| Throughput | 10000 req/s | 130 req/s | -77x |
| Quality | 0% (broken) | 78% (STS) | ∞ |
| Memory | 0MB | 150MB | +150MB |

**Conclusion**: 300x latency increase is acceptable for functional similarity search (still <100ms total including Layer 2 DSR processing)

---

## 11. Risk Mitigation

### 11.1 Model Download Failures

**Risk**: Model download from HuggingFace fails (network, CDN outage)

**Mitigation**:
1. **Pre-download during deployment**: Include model in Docker image
2. **Retry with exponential backoff**: 3 retries with 1s, 2s, 4s delays
3. **Fallback to TF-IDF**: Degraded mode with 40% quality
4. **Health check failure**: Return 503 Service Unavailable
5. **Alert on failures**: Prometheus alert for >5 consecutive failures

**Implementation**:
```rust
impl EmbeddingService {
    async fn download_with_retry(
        model_name: &str,
        max_retries: usize
    ) -> Result<EmbeddingModel> {
        for attempt in 0..max_retries {
            match Self::download_model(model_name).await {
                Ok(model) => return Ok(model),
                Err(e) if attempt < max_retries - 1 => {
                    let delay = Duration::from_secs(2u64.pow(attempt as u32));
                    warn!("Download failed (attempt {}), retrying in {:?}", attempt + 1, delay);
                    tokio::time::sleep(delay).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }
}
```

### 11.2 Memory Exhaustion

**Risk**: High concurrent load causes OOM (50MB per request × 100 concurrent = 5GB)

**Mitigation**:
1. **Concurrency limiter**: Cap at 4-8 concurrent embeddings
2. **Request queuing**: Queue excess requests with timeout
3. **Backpressure**: Return 503 when queue full
4. **Memory monitoring**: Alert when usage >80%

**Implementation**:
```rust
pub struct EmbeddingService {
    model: Arc<EmbeddingModel>,
    limiter: Arc<Semaphore>,  // Cap concurrent requests
    queue: Arc<Mutex<VecDeque<PendingRequest>>>,
}

impl EmbeddingService {
    async fn embed_with_backpressure(&self, text: &str) -> Result<Vec<f32>> {
        // Try to acquire permit
        match self.limiter.try_acquire() {
            Ok(permit) => {
                let result = self.embed_internal(text).await;
                drop(permit);
                result
            }
            Err(_) => {
                // Queue full: return error
                Err(anyhow!("Service overloaded, try again later"))
            }
        }
    }
}
```

### 11.3 Model Corruption

**Risk**: Cached model file corrupted, causes crashes

**Mitigation**:
1. **Checksum verification**: Verify file hash after download
2. **Load validation**: Test embedding before accepting queries
3. **Auto-recovery**: Clear cache and re-download on load failure
4. **Atomic writes**: Use temp file + rename for cache writes

**Implementation**:
```rust
impl EmbeddingModel {
    fn load_from_cache(path: &Path) -> Result<Self> {
        // Verify checksum
        let expected_hash = Self::load_checksum(path)?;
        let actual_hash = Self::compute_file_hash(path)?;

        if expected_hash != actual_hash {
            warn!("Cache checksum mismatch, clearing cache");
            std::fs::remove_dir_all(path)?;
            return Err(anyhow!("Cache corrupted"));
        }

        // Load model
        let model = Self::load_internal(path)?;

        // Validate with test embedding
        let test = model.encode(&["test"])?;
        if test.is_empty() || test[0].len() != 384 {
            return Err(anyhow!("Model validation failed"));
        }

        Ok(model)
    }
}
```

---

## 12. Success Criteria

### 12.1 Functional Requirements

- [ ] Generate 384-dimensional embeddings for all queries
- [ ] L2 normalized vectors (norm = 1.0 ± 0.01)
- [ ] Semantic similarity preserved (cat/dog > cat/car)
- [ ] Integration with Layer 2 socket client working
- [ ] Model auto-download and caching functional
- [ ] Graceful fallback on model load failure

### 12.2 Performance Requirements

- [ ] p95 latency: <50ms per embedding
- [ ] p99 latency: <100ms per embedding
- [ ] Throughput: >100 req/s with batching
- [ ] Memory footprint: <200MB per service instance
- [ ] Model load time: <5 seconds (cold start)
- [ ] Warmup time: <2 seconds

### 12.3 Quality Requirements

- [ ] Embedding quality: >0.75 on STS benchmark (if tested)
- [ ] No placeholder or stub code remaining
- [ ] Comprehensive error handling for all failure modes
- [ ] Unit test coverage: >80%
- [ ] Integration tests passing with real Layer 2 DSR
- [ ] Documentation complete and accurate

### 12.4 Operational Requirements

- [ ] Health check endpoint functional
- [ ] Metrics collection for latency, throughput, errors
- [ ] Model pre-downloaded in Docker image
- [ ] Cache directory configurable via environment variable
- [ ] Logging at appropriate levels (info, warn, error)
- [ ] Graceful shutdown (finish pending requests)

---

## 13. Next Steps (Step 4 Implementation)

**Implementation Order**:

1. **Add Dependencies** (0.5 hours)
   - Update Cargo.toml with fastembed
   - Verify compilation

2. **Create Module Structure** (1 hour)
   - Create `mfn-integration/src/embeddings/` directory
   - Add mod.rs, service.rs, models.rs, config.rs

3. **Implement EmbeddingService** (4 hours)
   - Model loading and caching
   - Single embedding generation
   - Batch embedding generation
   - Model warmup

4. **Integrate with Layer2Client** (2 hours)
   - Add embedding_service field
   - Replace placeholder at line 217
   - Update client initialization

5. **Add Error Handling** (1 hour)
   - Implement fallback strategy
   - Add graceful degradation

6. **Write Tests** (2 hours)
   - Unit tests for embedding quality
   - Integration tests with Layer 2
   - Performance benchmarks

7. **Documentation** (0.5 hours)
   - Update inline comments
   - Add usage examples

**Total Estimated Effort**: 11 hours (within 10-12 hour target)

---

## Appendix A: Code References

**Files to Modify**:
- `mfn-integration/src/socket_clients.rs:217` (replace placeholder)
- `mfn-integration/Cargo.toml` (add dependencies)
- `mfn-integration/src/lib.rs` (export embeddings module)

**Files to Create**:
- `mfn-integration/src/embeddings/mod.rs`
- `mfn-integration/src/embeddings/service.rs`
- `mfn-integration/src/embeddings/models.rs`
- `mfn-integration/src/embeddings/config.rs`

**Tests to Create**:
- `mfn-integration/tests/test_embeddings.rs`
- `mfn-integration/benches/embedding_bench.rs`

---

## Appendix B: Alternative Architectures Considered

### Option 1: External Embedding Microservice

**Architecture**: Separate HTTP service for embeddings

**Pros**: Language-agnostic, scalable, reusable
**Cons**: Network latency, deployment complexity, added failure point

**Verdict**: REJECTED - adds latency and complexity

### Option 2: Python Embedding Service via FFI

**Architecture**: Python process with sentence-transformers, called via FFI

**Pros**: Best ecosystem, easier debugging
**Cons**: Python runtime dependency, FFI complexity, IPC overhead

**Verdict**: REJECTED - FFI already problematic in MFN

### Option 3: WASM Embedding Model

**Architecture**: Compile model to WebAssembly

**Pros**: Portable, sandboxed
**Cons**: Immature tooling, performance overhead, limited model support

**Verdict**: REJECTED - not production-ready

**Chosen**: Native Rust library (fastembed) - best performance and integration

---

**Document Status**: COMPLETE
**Ready for Step 4 Implementation**: YES
**Dependencies**: None (ready to proceed)
