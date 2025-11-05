// Socket Client implementations for MFN layers
// Provides unified interface for connecting to all 4 layers via Unix sockets

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::net::UnixStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::time::timeout;
use tokio::sync::Mutex;
use tracing::{debug, warn};
use uuid::Uuid;
use lru::LruCache;
use std::num::NonZeroUsize;

// Import embedding service
use crate::embeddings::EmbeddingService;

// Socket paths for all layers
pub const LAYER1_SOCKET_PATH: &str = "/tmp/mfn_layer1.sock";
pub const LAYER2_SOCKET_PATH: &str = "/tmp/mfn_layer2.sock";
pub const LAYER3_SOCKET_PATH: &str = "/tmp/mfn_layer3.sock";
pub const LAYER4_SOCKET_PATH: &str = "/tmp/mfn_layer4.sock";

// Binary protocol constants
pub const PROTOCOL_BINARY: u8 = 0x02;
pub const MSG_QUERY_MEMORY: u8 = 0x20;
pub const MSG_ADD_MEMORY: u8 = 0x10;
pub const MSG_RESPONSE: u8 = 0x80;
pub const MSG_ERROR: u8 = 0x90;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalSearchQuery {
    pub query_id: String,
    pub content: String,
    pub search_type: SearchType,
    pub max_results: usize,
    pub min_confidence: f32,
    pub timeout_ms: u64,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchType {
    Exact,
    Similarity,
    Associative,
    Contextual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniversalSearchResult {
    pub memory_id: u64,
    pub content: String,
    pub confidence: f32,
    pub layer_source: u8,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct LayerQueryResult {
    pub results: Vec<UniversalSearchResult>,
    pub processing_time_ms: f64,
    pub confidence: f64,
    pub metadata: HashMap<String, String>,
}

// ============================================================================
// Layer 1 IFR Client (Zig)
// ============================================================================

pub struct Layer1Client {
    socket_path: String,
    connection_timeout: Duration,
}

impl Layer1Client {
    pub fn new() -> Result<Self> {
        Ok(Self {
            socket_path: LAYER1_SOCKET_PATH.to_string(),
            connection_timeout: Duration::from_millis(5000),
        })
    }

    pub async fn query(&self, query: &UniversalSearchQuery) -> Result<LayerQueryResult> {
        let start = Instant::now();

        // Connect to Layer 1 socket
        let stream = timeout(
            self.connection_timeout,
            UnixStream::connect(&self.socket_path)
        ).await
            .map_err(|_| anyhow!("Layer 1 connection timeout"))?
            .map_err(|e| anyhow!("Layer 1 connection failed: {}", e))?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Send JSON query (Layer 1 supports JSON)
        let json_request = serde_json::json!({
            "type": "query",
            "request_id": &query.query_id,
            "content": &query.content,
        });

        let request_str = format!("{}\n", json_request);
        writer.write_all(request_str.as_bytes()).await?;

        // Read response
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;

        // Parse JSON response
        let response: serde_json::Value = serde_json::from_str(&response_line)?;

        let processing_time = start.elapsed().as_millis() as f64;

        // Convert response to LayerQueryResult
        if response["success"].as_bool().unwrap_or(false) {
            let found_exact = response["found_exact"].as_bool().unwrap_or(false);
            let confidence = response["confidence"].as_f64().unwrap_or(0.0);

            let mut results = Vec::new();
            if found_exact {
                if let Some(result_str) = response["result"].as_str() {
                    results.push(UniversalSearchResult {
                        memory_id: response["memory_id_hash"].as_u64().unwrap_or(0),
                        content: result_str.to_string(),
                        confidence: confidence as f32,
                        layer_source: 1,
                        metadata: HashMap::new(),
                    });
                }
            }

            Ok(LayerQueryResult {
                results,
                processing_time_ms: processing_time,
                confidence,
                metadata: HashMap::new(),
            })
        } else {
            Err(anyhow!("Layer 1 query failed"))
        }
    }

    pub async fn add_memory(&self, content: &str, memory_data: &[u8]) -> Result<u64> {
        // Connect to Layer 1 socket
        let stream = timeout(
            self.connection_timeout,
            UnixStream::connect(&self.socket_path)
        ).await
            .map_err(|_| anyhow!("Layer 1 connection timeout"))?
            .map_err(|e| anyhow!("Layer 1 connection failed: {}", e))?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Send JSON add memory request
        let json_request = serde_json::json!({
            "type": "add_memory",
            "request_id": Uuid::new_v4().to_string(),
            "content": content,
            "memory_data": base64::encode(memory_data),
        });

        let request_str = format!("{}\n", json_request);
        writer.write_all(request_str.as_bytes()).await?;

        // Read response
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;

        let response: serde_json::Value = serde_json::from_str(&response_line)?;

        if response["success"].as_bool().unwrap_or(false) {
            Ok(response["memory_id_hash"].as_u64().unwrap_or(0))
        } else {
            Err(anyhow!("Failed to add memory to Layer 1"))
        }
    }

    pub fn shutdown(self) -> Result<()> {
        // Clean shutdown
        Ok(())
    }
}

// ============================================================================
// Layer 2 DSR Client (Rust)
// ============================================================================

pub struct Layer2Client {
    socket_path: String,
    connection_timeout: Duration,
    embedding_service: Arc<EmbeddingService>,
    embedding_cache: Arc<Mutex<LruCache<String, Vec<f32>>>>,
}

impl Layer2Client {
    pub async fn new(embedding_service: Arc<EmbeddingService>) -> Result<Self> {
        Ok(Self {
            socket_path: LAYER2_SOCKET_PATH.to_string(),
            connection_timeout: Duration::from_millis(5000),
            embedding_service,
            embedding_cache: Arc::new(Mutex::new(
                LruCache::new(NonZeroUsize::new(100_000).unwrap())
            )),
        })
    }

    pub async fn query(&self, query: &UniversalSearchQuery) -> Result<LayerQueryResult> {
        let start = Instant::now();

        // Connect to Layer 2 socket
        let stream = timeout(
            self.connection_timeout,
            UnixStream::connect(&self.socket_path)
        ).await
            .map_err(|_| anyhow!("Layer 2 connection timeout"))?
            .map_err(|e| anyhow!("Layer 2 connection failed: {}", e))?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Check cache first, generate embedding if not cached
        let query_embedding = {
            let mut cache = self.embedding_cache.lock().await;
            if let Some(cached) = cache.get(&query.content) {
                debug!("Embedding cache HIT for query");
                cached.clone()
            } else {
                drop(cache); // Release lock before slow operation
                debug!("Embedding cache MISS, generating...");
                let embedding = self.embedding_service
                    .embed(&query.content)
                    .await
                    .map_err(|e| anyhow!("Embedding generation failed: {}", e))?;

                // Store in cache
                let mut cache = self.embedding_cache.lock().await;
                cache.put(query.content.clone(), embedding.clone());
                embedding
            }
        };

        // Validate embedding dimension
        if query_embedding.len() != 384 {
            return Err(anyhow!(
                "Invalid embedding dimension: expected 384, got {}",
                query_embedding.len()
            ));
        }

        // Send JSON similarity search request
        let json_request = serde_json::json!({
            "type": "SimilaritySearch",
            "request_id": &query.query_id,
            "query_embedding": query_embedding,
            "top_k": query.max_results,
            "min_confidence": query.min_confidence,
            "timeout_ms": query.timeout_ms,
        });

        let request_str = format!("{}\n", json_request);
        writer.write_all(request_str.as_bytes()).await?;

        // Read response
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;

        let response: serde_json::Value = serde_json::from_str(&response_line)?;

        let processing_time = start.elapsed().as_millis() as f64;

        // Convert response to LayerQueryResult
        if response["success"].as_bool().unwrap_or(false) {
            let mut results = Vec::new();

            if let Some(search_results) = response["results"].as_array() {
                for result in search_results {
                    results.push(UniversalSearchResult {
                        memory_id: result["memory_id"].as_u64().unwrap_or(0),
                        content: result["content"].as_str().unwrap_or("").to_string(),
                        confidence: result["similarity_score"].as_f64().unwrap_or(0.0) as f32,
                        layer_source: 2,
                        metadata: HashMap::new(),
                    });
                }
            }

            Ok(LayerQueryResult {
                results,
                processing_time_ms: processing_time,
                confidence: response["average_confidence"].as_f64().unwrap_or(0.0),
                metadata: HashMap::new(),
            })
        } else {
            Err(anyhow!("Layer 2 query failed"))
        }
    }

    pub fn shutdown(self) -> Result<()> {
        Ok(())
    }
}

// ============================================================================
// Layer 3 ALM Client (Go)
// ============================================================================

pub struct Layer3Client {
    socket_path: String,
    connection_timeout: Duration,
}

impl Layer3Client {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            socket_path: LAYER3_SOCKET_PATH.to_string(),
            connection_timeout: Duration::from_millis(5000),
        })
    }

    pub async fn query(&self, query: &UniversalSearchQuery) -> Result<LayerQueryResult> {
        let start = Instant::now();

        // Connect to Layer 3 socket
        let stream = timeout(
            self.connection_timeout,
            UnixStream::connect(&self.socket_path)
        ).await
            .map_err(|_| anyhow!("Layer 3 connection timeout"))?
            .map_err(|e| anyhow!("Layer 3 connection failed: {}", e))?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Send JSON search request
        let json_request = serde_json::json!({
            "type": "search",
            "request_id": &query.query_id,
            "query": &query.content,
            "limit": query.max_results,
            "min_confidence": query.min_confidence,
        });

        let request_str = format!("{}\n", json_request);
        writer.write_all(request_str.as_bytes()).await?;

        // Read response
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;

        let response: serde_json::Value = serde_json::from_str(&response_line)?;

        let processing_time = start.elapsed().as_millis() as f64;

        // Convert response to LayerQueryResult
        if response["success"].as_bool().unwrap_or(false) {
            let mut results = Vec::new();

            if let Some(search_results) = response["results"].as_array() {
                for result in search_results {
                    results.push(UniversalSearchResult {
                        memory_id: result["id"].as_u64().unwrap_or(0),
                        content: result["content"].as_str().unwrap_or("").to_string(),
                        confidence: result["score"].as_f64().unwrap_or(0.0) as f32,
                        layer_source: 3,
                        metadata: HashMap::new(),
                    });
                }
            }

            Ok(LayerQueryResult {
                results,
                processing_time_ms: processing_time,
                confidence: response["confidence"].as_f64().unwrap_or(0.0),
                metadata: HashMap::new(),
            })
        } else {
            Err(anyhow!("Layer 3 query failed"))
        }
    }

    pub fn shutdown(self) -> Result<()> {
        Ok(())
    }
}

// ============================================================================
// Layer 4 CPE Client (Rust)
// ============================================================================

pub struct Layer4Client {
    socket_path: String,
    connection_timeout: Duration,
}

impl Layer4Client {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            socket_path: LAYER4_SOCKET_PATH.to_string(),
            connection_timeout: Duration::from_millis(5000),
        })
    }

    pub async fn query(&self, query: &UniversalSearchQuery) -> Result<LayerQueryResult> {
        use tokio::io::AsyncReadExt;
        let start = Instant::now();

        // Connect to Layer 4 socket
        let stream = timeout(
            self.connection_timeout,
            UnixStream::connect(&self.socket_path)
        ).await
            .map_err(|_| anyhow!("Layer 4 connection timeout"))?
            .map_err(|e| anyhow!("Layer 4 connection failed: {}", e))?;

        let (mut reader, mut writer) = stream.into_split();

        // Send JSON context prediction request using BINARY PROTOCOL (4-byte length + JSON)
        let json_request = serde_json::json!({
            "type": "PredictContext",
            "request_id": &query.query_id,
            "payload": {
                "current_context": query.content.split_whitespace().collect::<Vec<&str>>(),
                "sequence_length": query.max_results,
            }
        });

        let request_json = serde_json::to_string(&json_request)?;
        let request_bytes = request_json.as_bytes();
        let request_len = request_bytes.len() as u32;

        // Write: 4-byte length (little-endian) + JSON payload
        writer.write_all(&request_len.to_le_bytes()).await?;
        writer.write_all(request_bytes).await?;

        // Read response: 4-byte length + JSON
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let response_len = u32::from_le_bytes(len_buf) as usize;

        let mut response_buf = vec![0u8; response_len];
        reader.read_exact(&mut response_buf).await?;

        let response: serde_json::Value = serde_json::from_slice(&response_buf)?;

        let processing_time = start.elapsed().as_millis() as f64;

        // Convert response to LayerQueryResult
        if response["success"].as_bool().unwrap_or(false) {
            let mut results = Vec::new();

            // Extract predictions from nested data structure
            if let Some(data) = response["data"].as_object() {
                if let Some(predictions) = data.get("predictions").and_then(|p| p.as_array()) {
                    for pred in predictions {
                        // Handle UniversalSearchResult structure from server
                        if let Some(memory) = pred.get("memory").and_then(|m| m.as_object()) {
                            results.push(UniversalSearchResult {
                                memory_id: memory.get("id").and_then(|id| id.as_u64()).unwrap_or(0),
                                content: memory.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string(),
                                confidence: pred.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.0) as f32,
                                layer_source: 4,
                                metadata: HashMap::new(),
                            });
                        }
                    }
                }
            }

            Ok(LayerQueryResult {
                results,
                processing_time_ms: processing_time,
                confidence: 0.8, // Layer 4 provides contextual predictions
                metadata: HashMap::new(),
            })
        } else {
            let error_msg = response["data"]["error"].as_str().unwrap_or("Unknown error");
            Err(anyhow!("Layer 4 query failed: {}", error_msg))
        }
    }

    pub fn shutdown(self) -> Result<()> {
        Ok(())
    }
}

// ============================================================================
// Connection Pool for efficient socket reuse
// ============================================================================

pub struct LayerConnectionPool {
    layer1: Option<Layer1Client>,
    layer2: Option<Layer2Client>,
    layer3: Option<Layer3Client>,
    layer4: Option<Layer4Client>,
    // Shared embedding service
    embedding_service: Arc<EmbeddingService>,
}

impl LayerConnectionPool {
    pub async fn new() -> Result<Self> {
        // Initialize embedding service ONCE
        use crate::embeddings::EmbeddingConfig;

        let embedding_service = Arc::new(
            EmbeddingService::new(EmbeddingConfig::default()).await?
        );

        Ok(Self {
            layer1: None,
            layer2: None,
            layer3: None,
            layer4: None,
            embedding_service,
        })
    }

    pub async fn get_layer1(&mut self) -> Result<&Layer1Client> {
        if self.layer1.is_none() {
            self.layer1 = Some(Layer1Client::new()?);
        }
        Ok(self.layer1.as_ref().unwrap())
    }

    pub async fn get_layer2(&mut self) -> Result<&Layer2Client> {
        if self.layer2.is_none() {
            self.layer2 = Some(Layer2Client::new(Arc::clone(&self.embedding_service)).await?);
        }
        Ok(self.layer2.as_ref().unwrap())
    }

    pub async fn get_layer3(&mut self) -> Result<&Layer3Client> {
        if self.layer3.is_none() {
            self.layer3 = Some(Layer3Client::new().await?);
        }
        Ok(self.layer3.as_ref().unwrap())
    }

    pub async fn get_layer4(&mut self) -> Result<&Layer4Client> {
        if self.layer4.is_none() {
            self.layer4 = Some(Layer4Client::new().await?);
        }
        Ok(self.layer4.as_ref().unwrap())
    }

    pub fn shutdown(self) -> Result<()> {
        if let Some(client) = self.layer1 {
            client.shutdown()?;
        }
        if let Some(client) = self.layer2 {
            client.shutdown()?;
        }
        if let Some(client) = self.layer3 {
            client.shutdown()?;
        }
        if let Some(client) = self.layer4 {
            client.shutdown()?;
        }
        Ok(())
    }
}

// Base64 encoding support for binary data
mod base64 {
    pub fn encode(input: &[u8]) -> String {
        use std::fmt::Write;
        let mut result = String::new();
        let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

        for chunk in input.chunks(3) {
            let mut buf = [0u8; 3];
            for (i, &byte) in chunk.iter().enumerate() {
                buf[i] = byte;
            }

            let _ = write!(result, "{}",
                alphabet.chars().nth((buf[0] >> 2) as usize).unwrap());
            let _ = write!(result, "{}",
                alphabet.chars().nth((((buf[0] & 0x03) << 4) | (buf[1] >> 4)) as usize).unwrap());

            if chunk.len() > 1 {
                let _ = write!(result, "{}",
                    alphabet.chars().nth((((buf[1] & 0x0f) << 2) | (buf[2] >> 6)) as usize).unwrap());
            } else {
                result.push('=');
            }

            if chunk.len() > 2 {
                let _ = write!(result, "{}",
                    alphabet.chars().nth((buf[2] & 0x3f) as usize).unwrap());
            } else {
                result.push('=');
            }
        }

        result
    }
}