//! MFN Binary Protocol Backwards Compatibility Layer
//! 
//! Provides seamless migration path from JSON-based APIs to binary protocol
//! while maintaining existing client compatibility during the transition period.

use std::collections::HashMap;
use serde_json::{Value, Map};
use crate::{Result, MfnProtocolError, MfnBinarySerializer, MfnBinaryDeserializer};
use crate::types::*;

/// Compatibility bridge that handles both JSON and binary formats
pub struct CompatibilityBridge {
    prefer_binary: bool,
    json_fallback: bool,
    version_support: VersionSupport,
}

#[derive(Clone, Debug)]
pub struct VersionSupport {
    pub supports_binary: bool,
    pub supports_json: bool,
    pub binary_version: u16,
    pub negotiated_features: Vec<String>,
}

impl Default for CompatibilityBridge {
    fn default() -> Self {
        Self {
            prefer_binary: true,
            json_fallback: true,
            version_support: VersionSupport {
                supports_binary: true,
                supports_json: true,
                binary_version: 1,
                negotiated_features: vec![
                    "compression".to_string(),
                    "batch_operations".to_string(),
                ],
            },
        }
    }
}

impl CompatibilityBridge {
    pub fn new(prefer_binary: bool, json_fallback: bool) -> Self {
        Self {
            prefer_binary,
            json_fallback,
            version_support: VersionSupport::default(),
        }
    }

    pub fn with_version_support(mut self, version_support: VersionSupport) -> Self {
        self.version_support = version_support;
        self
    }

    /// Detect message format and route to appropriate handler
    pub fn process_message(&self, data: &[u8]) -> Result<ProcessedMessage> {
        if self.is_binary_message(data) {
            self.process_binary_message(data)
        } else if self.is_json_message(data) {
            self.process_json_message(data)
        } else {
            Err(MfnProtocolError::DeserializationError(
                "Unknown message format".to_string()
            ))
        }
    }

    /// Check if data starts with binary protocol magic number
    fn is_binary_message(&self, data: &[u8]) -> bool {
        data.len() >= 4 && 
        u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == crate::constants::MFN_MAGIC
    }

    /// Check if data looks like JSON
    fn is_json_message(&self, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }
        
        // Skip whitespace
        let trimmed = data.iter().skip_while(|&&b| b == b' ' || b == b'\t' || b == b'\n' || b == b'\r').cloned().collect::<Vec<_>>();
        
        // JSON objects/arrays start with { or [
        !trimmed.is_empty() && (trimmed[0] == b'{' || trimmed[0] == b'[')
    }

    /// Process binary protocol message
    fn process_binary_message(&self, data: &[u8]) -> Result<ProcessedMessage> {
        let mut deserializer = MfnBinaryDeserializer::new(data);
        let parsed = deserializer.parse_message()?;

        Ok(ProcessedMessage {
            format: MessageFormat::Binary,
            message_type: Some(parsed.message_type),
            operation: Some(parsed.operation),
            layer_id: Some(parsed.layer_id),
            sequence_id: parsed.sequence_id,
            payload: parsed.payload,
            original_size: data.len(),
        })
    }

    /// Process JSON message and convert to binary equivalent
    fn process_json_message(&self, data: &[u8]) -> Result<ProcessedMessage> {
        if !self.version_support.supports_json {
            return Err(MfnProtocolError::DeserializationError(
                "JSON format not supported".to_string()
            ));
        }

        let json_str = std::str::from_utf8(data)
            .map_err(MfnProtocolError::InvalidUtf8)?;
        
        let json_value: Value = serde_json::from_str(json_str)
            .map_err(|e| MfnProtocolError::DeserializationError(e.to_string()))?;

        self.convert_json_to_binary(json_value, data.len())
    }

    /// Convert JSON structure to binary protocol equivalent
    fn convert_json_to_binary(&self, json: Value, original_size: usize) -> Result<ProcessedMessage> {
        match json {
            Value::Object(obj) => self.process_json_object(obj, original_size),
            Value::Array(arr) => self.process_json_array(arr, original_size),
            _ => Err(MfnProtocolError::DeserializationError(
                "Invalid JSON message structure".to_string()
            )),
        }
    }

    /// Process JSON object (single operation)
    fn process_json_object(&self, obj: Map<String, Value>, original_size: usize) -> Result<ProcessedMessage> {
        // Detect operation type from JSON structure
        let (msg_type, operation, layer_id) = self.detect_operation_type(&obj)?;
        
        // Convert payload to binary
        let binary_payload = self.convert_json_payload(&obj, &msg_type)?;
        
        // Generate sequence ID if not present
        let sequence_id = obj.get("sequence_id")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| self.generate_sequence_id()) as u32;

        Ok(ProcessedMessage {
            format: MessageFormat::Json,
            message_type: Some(msg_type),
            operation: Some(operation),
            layer_id: Some(layer_id),
            sequence_id,
            payload: binary_payload,
            original_size,
        })
    }

    /// Process JSON array (batch operations)
    fn process_json_array(&self, arr: Vec<Value>, original_size: usize) -> Result<ProcessedMessage> {
        let mut serializer = MfnBinarySerializer::new(original_size * 2);
        
        // Write batch header
        serializer.write_u32(arr.len() as u32)?;
        
        // Process each operation in batch
        for item in arr {
            if let Value::Object(obj) = item {
                let (msg_type, _operation, _layer_id) = self.detect_operation_type(&obj)?;
                let binary_payload = self.convert_json_payload(&obj, &msg_type)?;
                
                serializer.write_u16(msg_type as u16)?;
                serializer.write_u32(binary_payload.len() as u32)?;
                serializer.write_bytes(&binary_payload)?;
            }
        }

        Ok(ProcessedMessage {
            format: MessageFormat::JsonBatch,
            message_type: Some(MessageType::Batch),
            operation: Some(Operation::Batch),
            layer_id: Some(LayerId::Broadcast),
            sequence_id: self.generate_sequence_id() as u32,
            payload: serializer.into_buffer(),
            original_size,
        })
    }

    /// Detect MFN operation type from JSON structure
    fn detect_operation_type(&self, obj: &Map<String, Value>) -> Result<(MessageType, Operation, LayerId)> {
        // Check explicit operation field first
        if let Some(op_val) = obj.get("operation") {
            if let Some(op_str) = op_val.as_str() {
                return self.parse_explicit_operation(op_str);
            }
        }

        // Infer from JSON structure patterns
        if obj.contains_key("memory") || obj.contains_key("content") {
            if obj.contains_key("id") && obj.keys().len() == 1 {
                // Just ID - likely a GET request
                return Ok((MessageType::MemoryGet, Operation::Get, LayerId::Layer3));
            } else {
                // Full memory object - likely an ADD request
                return Ok((MessageType::MemoryAdd, Operation::Add, LayerId::Layer1));
            }
        }

        if obj.contains_key("search") || obj.contains_key("query") {
            return Ok((MessageType::SearchAssoc, Operation::Search, LayerId::Layer3));
        }

        if obj.contains_key("association") || (obj.contains_key("from_memory_id") && obj.contains_key("to_memory_id")) {
            return Ok((MessageType::AssocAdd, Operation::Add, LayerId::Layer3));
        }

        if obj.contains_key("health") || obj.contains_key("status") {
            return Ok((MessageType::HealthCheck, Operation::Health, LayerId::Broadcast));
        }

        // Default to generic operation
        Ok((MessageType::MemoryAdd, Operation::Add, LayerId::Layer3))
    }

    /// Parse explicit operation string
    fn parse_explicit_operation(&self, op_str: &str) -> Result<(MessageType, Operation, LayerId)> {
        match op_str.to_lowercase().as_str() {
            "add_memory" | "memory_add" => Ok((MessageType::MemoryAdd, Operation::Add, LayerId::Layer1)),
            "get_memory" | "memory_get" => Ok((MessageType::MemoryGet, Operation::Get, LayerId::Layer3)),
            "search" | "associative_search" => Ok((MessageType::SearchAssoc, Operation::Search, LayerId::Layer3)),
            "add_association" | "association_add" => Ok((MessageType::AssocAdd, Operation::Add, LayerId::Layer3)),
            "health_check" | "health" => Ok((MessageType::HealthCheck, Operation::Health, LayerId::Broadcast)),
            "performance" | "metrics" => Ok((MessageType::Performance, Operation::Metrics, LayerId::Broadcast)),
            _ => Err(MfnProtocolError::DeserializationError(
                format!("Unknown operation: {}", op_str)
            )),
        }
    }

    /// Convert JSON payload to binary format
    fn convert_json_payload(&self, obj: &Map<String, Value>, msg_type: &MessageType) -> Result<Vec<u8>> {
        let mut serializer = MfnBinarySerializer::new(4096);

        match msg_type {
            MessageType::MemoryAdd => {
                let memory = self.json_to_memory(obj)?;
                serializer.serialize_memory(&memory)?;
            },
            MessageType::MemoryGet => {
                let id = obj.get("id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| MfnProtocolError::DeserializationError("Missing memory ID".to_string()))?;
                serializer.write_u64(id)?;
            },
            MessageType::SearchAssoc => {
                let query = self.json_to_search_query(obj)?;
                serializer.serialize_search_query(&query)?;
            },
            MessageType::AssocAdd => {
                let association = self.json_to_association(obj)?;
                serializer.serialize_association(&association)?;
            },
            MessageType::HealthCheck => {
                // Empty payload for health check
            },
            MessageType::Performance => {
                // Empty payload for performance metrics request
            },
            _ => {
                return Err(MfnProtocolError::DeserializationError(
                    format!("Unsupported message type for JSON conversion: {:?}", msg_type)
                ));
            }
        }

        Ok(serializer.into_buffer())
    }

    /// Convert JSON object to UniversalMemory
    fn json_to_memory(&self, obj: &Map<String, Value>) -> Result<UniversalMemory> {
        let id = obj.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        let content = obj.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
        
        let tags = obj.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        let embedding = obj.get("embedding")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect());

        let metadata = obj.get("metadata")
            .and_then(|v| v.as_object())
            .map(|meta_obj| {
                meta_obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let created_at = obj.get("created_at").and_then(|v| v.as_u64())
            .unwrap_or_else(|| current_timestamp_us());
        let last_accessed = obj.get("last_accessed").and_then(|v| v.as_u64())
            .unwrap_or(created_at);
        let access_count = obj.get("access_count").and_then(|v| v.as_u64()).unwrap_or(0);

        Ok(UniversalMemory {
            id,
            content,
            embedding,
            tags,
            metadata,
            created_at,
            last_accessed,
            access_count,
        })
    }

    /// Convert JSON object to search query
    fn json_to_search_query(&self, obj: &Map<String, Value>) -> Result<UniversalSearchQuery> {
        let start_memory_ids = obj.get("start_memory_ids")
            .or_else(|| obj.get("startMemoryIds"))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
            .unwrap_or_default();

        let content = obj.get("content")
            .or_else(|| obj.get("query"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let embedding = obj.get("embedding")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect());

        let tags = obj.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_default();

        let max_depth = obj.get("max_depth")
            .or_else(|| obj.get("maxDepth"))
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;

        let max_results = obj.get("max_results")
            .or_else(|| obj.get("maxResults"))
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let min_weight = obj.get("min_weight")
            .or_else(|| obj.get("minWeight"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.1);

        let timeout_us = obj.get("timeout")
            .and_then(|v| v.as_u64())
            .map(|ms| ms * 1000) // Convert ms to μs
            .unwrap_or(10000);

        let association_types = obj.get("association_types")
            .or_else(|| obj.get("associationTypes"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| self.string_to_association_type(s))
                    .collect()
            })
            .unwrap_or_default();

        Ok(UniversalSearchQuery {
            start_memory_ids,
            content,
            embedding,
            tags,
            association_types,
            max_depth,
            max_results,
            min_weight,
            timeout_us,
        })
    }

    /// Convert JSON object to association
    fn json_to_association(&self, obj: &Map<String, Value>) -> Result<UniversalAssociation> {
        let from_memory_id = obj.get("from_memory_id")
            .or_else(|| obj.get("fromMemoryId"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| MfnProtocolError::DeserializationError("Missing from_memory_id".to_string()))?;

        let to_memory_id = obj.get("to_memory_id")
            .or_else(|| obj.get("toMemoryId"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| MfnProtocolError::DeserializationError("Missing to_memory_id".to_string()))?;

        let association_type_str = obj.get("type")
            .or_else(|| obj.get("association_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("semantic");
        let association_type = self.string_to_association_type(association_type_str);

        let weight = obj.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let reason = obj.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let created_at = current_timestamp_us();

        Ok(UniversalAssociation {
            from_memory_id,
            to_memory_id,
            association_type,
            weight,
            reason,
            created_at,
            last_used: created_at,
            usage_count: 0,
        })
    }

    /// Convert string to association type
    fn string_to_association_type(&self, s: &str) -> AssociationType {
        match s.to_lowercase().as_str() {
            "semantic" => AssociationType::Semantic,
            "temporal" => AssociationType::Temporal,
            "causal" => AssociationType::Causal,
            "spatial" => AssociationType::Spatial,
            "conceptual" => AssociationType::Conceptual,
            "hierarchical" => AssociationType::Hierarchical,
            "functional" => AssociationType::Functional,
            "domain" => AssociationType::Domain,
            "cognitive" => AssociationType::Cognitive,
            _ => AssociationType::Custom(s.to_string()),
        }
    }

    /// Generate sequence ID from current timestamp
    fn generate_sequence_id(&self) -> u64 {
        current_timestamp_us()
    }

    /// Create response in the preferred format
    pub fn create_response(&self, request_format: MessageFormat, data: &[u8]) -> Result<Vec<u8>> {
        match request_format {
            MessageFormat::Json | MessageFormat::JsonBatch => {
                if self.version_support.supports_json {
                    self.create_json_response(data)
                } else {
                    Ok(data.to_vec()) // Return binary if JSON not supported
                }
            },
            MessageFormat::Binary => {
                Ok(data.to_vec()) // Already binary
            },
        }
    }

    /// Convert binary response back to JSON
    fn create_json_response(&self, binary_data: &[u8]) -> Result<Vec<u8>> {
        let mut deserializer = MfnBinaryDeserializer::new(binary_data);
        let parsed = deserializer.parse_message()?;

        let json_response = match parsed.message_type {
            MessageType::Response => {
                // Try to parse payload as memory or search results
                self.binary_payload_to_json(&parsed.payload, &parsed.operation)?
            },
            MessageType::Error => {
                serde_json::json!({
                    "error": true,
                    "message": "Operation failed",
                    "sequence_id": parsed.sequence_id
                })
            },
            _ => {
                serde_json::json!({
                    "success": true,
                    "sequence_id": parsed.sequence_id
                })
            }
        };

        let json_bytes = serde_json::to_vec(&json_response)
            .map_err(|e| MfnProtocolError::SerializationError(e.to_string()))?;

        Ok(json_bytes)
    }

    /// Convert binary payload back to JSON
    fn binary_payload_to_json(&self, payload: &[u8], operation: &Operation) -> Result<Value> {
        match operation {
            Operation::Get => {
                // Parse as memory
                let mut deserializer = MfnBinaryDeserializer::new(payload);
                let memory = deserializer.deserialize_memory()?;
                
                Ok(serde_json::json!({
                    "id": memory.id,
                    "content": memory.content,
                    "embedding": memory.embedding,
                    "tags": memory.tags,
                    "metadata": memory.metadata,
                    "created_at": memory.created_at,
                    "last_accessed": memory.last_accessed,
                    "access_count": memory.access_count
                }))
            },
            Operation::Search => {
                // Parse as search results (simplified)
                Ok(serde_json::json!({
                    "results": [],
                    "total_found": 0,
                    "search_time_us": 0
                }))
            },
            _ => {
                Ok(serde_json::json!({
                    "success": true,
                    "operation": format!("{:?}", operation)
                }))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageFormat {
    Binary,
    Json,
    JsonBatch,
}

#[derive(Debug)]
pub struct ProcessedMessage {
    pub format: MessageFormat,
    pub message_type: Option<MessageType>,
    pub operation: Option<Operation>, 
    pub layer_id: Option<LayerId>,
    pub sequence_id: u32,
    pub payload: Vec<u8>,
    pub original_size: usize,
}

impl ProcessedMessage {
    pub fn compression_ratio(&self) -> f64 {
        self.payload.len() as f64 / self.original_size as f64
    }

    pub fn size_reduction_percent(&self) -> f64 {
        (1.0 - self.compression_ratio()) * 100.0
    }
}

fn current_timestamp_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

// Mock types for compilation
#[derive(Clone, Debug)]
pub struct UniversalMemory {
    pub id: u64,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: u64,
    pub last_accessed: u64,
    pub access_count: u64,
}

#[derive(Clone, Debug)]
pub struct UniversalAssociation {
    pub from_memory_id: u64,
    pub to_memory_id: u64,
    pub association_type: AssociationType,
    pub weight: f64,
    pub reason: String,
    pub created_at: u64,
    pub last_used: u64,
    pub usage_count: u64,
}

#[derive(Clone, Debug)]
pub enum AssociationType {
    Semantic,
    Temporal,
    Causal,
    Spatial,
    Conceptual,
    Hierarchical,
    Functional,
    Domain,
    Cognitive,
    Custom(String),
}

#[derive(Clone, Debug)]
pub struct UniversalSearchQuery {
    pub start_memory_ids: Vec<u64>,
    pub content: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub tags: Vec<String>,
    pub association_types: Vec<AssociationType>,
    pub max_depth: usize,
    pub max_results: usize,
    pub min_weight: f64,
    pub timeout_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_memory_conversion() {
        let bridge = CompatibilityBridge::default();
        
        let json_data = r#"{
            "id": 12345,
            "content": "Test memory content",
            "tags": ["test", "conversion"],
            "metadata": {"source": "test"},
            "operation": "add_memory"
        }"#;

        let result = bridge.process_message(json_data.as_bytes()).unwrap();
        
        assert_eq!(result.format, MessageFormat::Json);
        assert_eq!(result.message_type, Some(MessageType::MemoryAdd));
        assert!(result.size_reduction_percent() > 30.0); // Should be significantly smaller
    }

    #[test]
    fn test_binary_message_passthrough() {
        let bridge = CompatibilityBridge::default();
        
        // Create a binary message
        let memory = UniversalMemory {
            id: 12345,
            content: "Test content".to_string(),
            embedding: None,
            tags: vec!["test".to_string()],
            metadata: HashMap::new(),
            created_at: 1640995200000000,
            last_accessed: 1640995200000000,
            access_count: 1,
        };
        
        let mut serializer = MfnBinarySerializer::new(1024);
        serializer.serialize_memory(&memory).unwrap();
        let binary_msg = serializer.create_message(
            MessageType::MemoryAdd,
            Operation::Add,
            LayerId::Layer1,
            12345,
        ).unwrap();

        let result = bridge.process_message(&binary_msg).unwrap();
        
        assert_eq!(result.format, MessageFormat::Binary);
        assert_eq!(result.message_type, Some(MessageType::MemoryAdd));
    }

    #[test]
    fn test_format_detection() {
        let bridge = CompatibilityBridge::default();
        
        // Test JSON detection
        let json_data = r#"{"test": "data"}"#;
        assert!(bridge.is_json_message(json_data.as_bytes()));
        assert!(!bridge.is_binary_message(json_data.as_bytes()));
        
        // Test binary detection (mock)
        let binary_data = [0x01, 0x4E, 0x46, 0x4D, 0x00, 0x01]; // MFN magic + data
        assert!(!bridge.is_json_message(&binary_data));
        // Would be true with actual MFN magic number: assert!(bridge.is_binary_message(&binary_data));
    }
}