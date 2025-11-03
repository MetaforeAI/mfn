use super::{EmbeddingConfig, EmbeddingMetrics, EmbeddingModel};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{info, debug};

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

        let service = Self {
            model: Arc::new(model),
            config,
            metrics: Arc::new(Mutex::new(metrics)),
        };

        // Run warmup
        service.warmup().await?;

        Ok(service)
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

        debug!("Generated embedding for '{}' in {}ms", text, latency);

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

        debug!("Generated {} embeddings in {}ms", texts.len(), latency);

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