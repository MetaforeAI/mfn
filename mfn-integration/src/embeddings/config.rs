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

#[derive(Debug, Default, Clone)]
pub struct EmbeddingMetrics {
    pub total_embeddings: u64,
    pub total_time_ms: f64,
    pub avg_latency_ms: f64,
    pub model_load_time_ms: f64,
}