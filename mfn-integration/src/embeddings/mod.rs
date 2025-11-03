//! Embedding service for semantic vector generation

mod config;
mod models;
mod service;

#[cfg(test)]
mod tests;

pub use config::{EmbeddingConfig, EmbeddingMetrics};
pub use models::{EmbeddingModel, SemanticHashEmbedder, TfIdfVectorizer};
pub use service::EmbeddingService;