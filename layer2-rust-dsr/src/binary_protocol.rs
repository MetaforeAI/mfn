//! Binary Protocol Support for Layer 2 DSR
//! 
//! High-performance binary protocol implementation for Layer 2 operations.
//! Provides significant performance improvements over JSON serialization.
//! 
//! Performance targets:
//! - Serialization: <50μs for typical operations
//! - Deserialization: <25μs for typical operations
//! - Memory overhead: <10% vs raw data

use std::collections::HashMap;
use std::io;
use std::mem;
use anyhow::{Result, anyhow};
use crc32fast::Hasher;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};

use crate::{Embedding, SimilarityResults, DSRPerformanceStats};

/// Binary protocol magic number for Layer 2 DSR
pub const DSR_BINARY_MAGIC: u32 = 0x44535202; // "DSR\x02"

/// Binary protocol version
pub const DSR_PROTOCOL_VERSION: u16 = 1;

/// Message types for binary protocol
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryMessageType {
    AddMemory = 0x0001,
    SimilaritySearch = 0x0002,
    GetStats = 0x0003,
    OptimizeReservoir = 0x0004,
    Ping = 0x0005,
    GetMemory = 0x0006,
    Response = 0x8000,
    Error = 0x8001,
}

impl BinaryMessageType {
    pub fn from_u16(value: u16) -> Result<Self> {
        match value {
            0x0001 => Ok(BinaryMessageType::AddMemory),
            0x0002 => Ok(BinaryMessageType::SimilaritySearch),
            0x0003 => Ok(BinaryMessageType::GetStats),
            0x0004 => Ok(BinaryMessageType::OptimizeReservoir),
            0x0005 => Ok(BinaryMessageType::Ping),
            0x0006 => Ok(BinaryMessageType::GetMemory),
            0x8000 => Ok(BinaryMessageType::Response),
            0x8001 => Ok(BinaryMessageType::Error),
            _ => Err(anyhow!("Invalid message type: {}", value)),
        }
    }
}

/// Message flags for binary protocol
#[repr(u16)]
pub enum BinaryMessageFlags {
    None = 0x0000,
    Compressed = 0x0001,
    HighPriority = 0x0002,
    Streaming = 0x0004,
}

/// Binary message header (16 bytes)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BinaryMessageHeader {
    pub magic: u32,           // 4 bytes - magic number
    pub version: u16,         // 2 bytes - protocol version
    pub message_type: u16,    // 2 bytes - message type
    pub flags: u16,           // 2 bytes - message flags
    pub payload_length: u32,  // 4 bytes - payload length
    pub sequence_id: u32,     // 4 bytes - sequence ID
}

impl BinaryMessageHeader {
    pub fn new(
        message_type: BinaryMessageType,
        payload_length: u32,
        sequence_id: u32,
        flags: u16,
    ) -> Self {
        Self {
            magic: DSR_BINARY_MAGIC,
            version: DSR_PROTOCOL_VERSION,
            message_type: message_type as u16,
            flags,
            payload_length,
            sequence_id,
        }
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0..4].copy_from_slice(&self.magic.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.version.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.message_type.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.flags.to_le_bytes());
        bytes[10..14].copy_from_slice(&self.payload_length.to_le_bytes());
        bytes[14..16].copy_from_slice(&self.sequence_id.to_le_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 16 {
            return Err(anyhow!("Header too short: {} bytes", bytes.len()));
        }

        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic != DSR_BINARY_MAGIC {
            return Err(anyhow!("Invalid magic number: {:x}", magic));
        }

        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != DSR_PROTOCOL_VERSION {
            return Err(anyhow!("Unsupported version: {}", version));
        }

        Ok(Self {
            magic,
            version,
            message_type: u16::from_le_bytes([bytes[6], bytes[7]]),
            flags: u16::from_le_bytes([bytes[8], bytes[9]]),
            payload_length: u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]),
            sequence_id: u32::from_le_bytes([bytes[14], bytes[15], 0, 0]),
        })
    }
}

/// Binary serializer for Layer 2 DSR operations
pub struct BinarySerializer {
    buffer: Vec<u8>,
    compression_threshold: usize,
}

impl BinarySerializer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            compression_threshold: 1024,
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    pub fn into_buffer(self) -> Vec<u8> {
        self.buffer
    }

    // Basic serialization primitives
    
    pub fn write_u8(&mut self, value: u8) -> Result<()> {
        self.buffer.push(value);
        Ok(())
    }

    pub fn write_u16(&mut self, value: u16) -> Result<()> {
        self.buffer.extend_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn write_u32(&mut self, value: u32) -> Result<()> {
        self.buffer.extend_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn write_u64(&mut self, value: u64) -> Result<()> {
        self.buffer.extend_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn write_f32(&mut self, value: f32) -> Result<()> {
        self.buffer.extend_from_slice(&value.to_le_bytes());
        Ok(())
    }

    pub fn write_bytes(&mut self, data: &[u8]) -> Result<()> {
        self.buffer.extend_from_slice(data);
        Ok(())
    }

    pub fn write_string(&mut self, s: &str) -> Result<()> {
        self.write_u32(s.len() as u32)?;
        self.write_bytes(s.as_bytes())?;
        Ok(())
    }

    pub fn write_string_array(&mut self, strings: &[String]) -> Result<()> {
        self.write_u32(strings.len() as u32)?;
        for s in strings {
            self.write_string(s)?;
        }
        Ok(())
    }

    pub fn write_embedding(&mut self, embedding: &Embedding) -> Result<()> {
        self.write_u32(embedding.len() as u32)?;
        // Use unsafe for performance - direct memory copy
        unsafe {
            let bytes = std::slice::from_raw_parts(
                embedding.as_ptr() as *const u8,
                embedding.len() * mem::size_of::<f32>(),
            );
            self.write_bytes(bytes)?;
        }
        Ok(())
    }

    pub fn write_optional_embedding(&mut self, embedding: Option<&Embedding>) -> Result<()> {
        match embedding {
            Some(emb) => {
                self.write_u8(1)?; // has embedding
                self.write_embedding(emb)?;
            },
            None => {
                self.write_u8(0)?; // no embedding
            }
        }
        Ok(())
    }

    pub fn write_metadata(&mut self, metadata: &HashMap<String, String>) -> Result<()> {
        self.write_u32(metadata.len() as u32)?;
        for (key, value) in metadata {
            self.write_string(key)?;
            self.write_string(value)?;
        }
        Ok(())
    }

    // High-level serialization methods

    pub fn serialize_add_memory_request(
        &mut self,
        memory_id: u64,
        embedding: &Embedding,
        content: &str,
        tags: &[String],
        metadata: &HashMap<String, String>,
    ) -> Result<()> {
        self.write_u64(memory_id)?;
        self.write_embedding(embedding)?;
        self.write_string(content)?;
        self.write_string_array(tags)?;
        self.write_metadata(metadata)?;
        Ok(())
    }

    pub fn serialize_similarity_search_request(
        &mut self,
        query_embedding: &Embedding,
        top_k: usize,
        min_confidence: Option<f32>,
        timeout_ms: Option<u64>,
    ) -> Result<()> {
        self.write_embedding(query_embedding)?;
        self.write_u32(top_k as u32)?;
        
        // Optional min confidence
        match min_confidence {
            Some(conf) => {
                self.write_u8(1)?;
                self.write_f32(conf)?;
            },
            None => self.write_u8(0)?,
        }

        // Optional timeout
        match timeout_ms {
            Some(timeout) => {
                self.write_u8(1)?;
                self.write_u64(timeout)?;
            },
            None => self.write_u8(0)?,
        }

        Ok(())
    }

    pub fn serialize_similarity_results(&mut self, results: &SimilarityResults) -> Result<()> {
        self.write_f32(results.processing_time_ms)?;
        self.write_u32(results.wells_evaluated as u32)?;
        self.write_u8(if results.has_confident_matches { 1 } else { 0 })?;
        
        // Matches
        self.write_u32(results.matches.len() as u32)?;
        for match_item in &results.matches {
            self.write_u64(match_item.memory_id.0)?;
            self.write_f32(match_item.confidence)?;
            self.write_f32(match_item.raw_activation)?;
            self.write_u32(match_item.rank as u32)?;
            self.write_string(&match_item.content)?;
        }
        
        Ok(())
    }

    pub fn serialize_performance_stats(&mut self, stats: &DSRPerformanceStats) -> Result<()> {
        self.write_u64(stats.total_queries)?;
        self.write_u64(stats.total_additions)?;
        self.write_u64(stats.cache_hits)?;
        self.write_u32(stats.similarity_wells_count as u32)?;
        self.write_u32(stats.reservoir_size as u32)?;
        self.write_f32(stats.average_well_activation)?;
        self.write_f32(stats.memory_usage_mb)?;
        Ok(())
    }

    pub fn serialize_error(&mut self, error_code: &str, error_message: &str) -> Result<()> {
        self.write_string(error_code)?;
        self.write_string(error_message)?;
        Ok(())
    }

    pub fn serialize_ping_response(&mut self, timestamp: u64, layer_info: &str, version: &str) -> Result<()> {
        self.write_u64(timestamp)?;
        self.write_string(layer_info)?;
        self.write_string(version)?;
        Ok(())
    }

    /// Create a complete binary message with header and optional compression
    pub fn create_message(
        &mut self,
        message_type: BinaryMessageType,
        sequence_id: u32,
    ) -> Result<Vec<u8>> {
        let payload_size = self.buffer.len();
        
        // Apply compression if payload is large enough
        let (final_payload, flags) = if payload_size >= self.compression_threshold {
            let compressed = compress_prepend_size(&self.buffer);
            if compressed.len() < payload_size {
                (compressed, BinaryMessageFlags::Compressed as u16)
            } else {
                (self.buffer.clone(), BinaryMessageFlags::None as u16)
            }
        } else {
            (self.buffer.clone(), BinaryMessageFlags::None as u16)
        };

        let header = BinaryMessageHeader::new(
            message_type,
            final_payload.len() as u32,
            sequence_id,
            flags,
        );

        // Build complete message
        let mut message = Vec::with_capacity(16 + final_payload.len() + 4);
        message.extend_from_slice(&header.to_bytes());
        message.extend_from_slice(&final_payload);

        // Add CRC32 checksum
        let mut hasher = Hasher::new();
        hasher.update(&message);
        let crc = hasher.finalize();
        message.extend_from_slice(&crc.to_le_bytes());

        Ok(message)
    }
}

/// Binary deserializer for Layer 2 DSR operations
pub struct BinaryDeserializer<'a> {
    buffer: &'a [u8],
    position: usize,
}

impl<'a> BinaryDeserializer<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self { buffer, position: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buffer.len() - self.position
    }

    pub fn position(&self) -> usize {
        self.position
    }

    // Basic deserialization primitives

    pub fn read_u8(&mut self) -> Result<u8> {
        if self.position >= self.buffer.len() {
            return Err(anyhow!("Buffer overflow reading u8"));
        }
        let value = self.buffer[self.position];
        self.position += 1;
        Ok(value)
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        if self.position + 2 > self.buffer.len() {
            return Err(anyhow!("Buffer overflow reading u16"));
        }
        let bytes = &self.buffer[self.position..self.position + 2];
        let value = u16::from_le_bytes([bytes[0], bytes[1]]);
        self.position += 2;
        Ok(value)
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        if self.position + 4 > self.buffer.len() {
            return Err(anyhow!("Buffer overflow reading u32"));
        }
        let bytes = &self.buffer[self.position..self.position + 4];
        let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        self.position += 4;
        Ok(value)
    }

    pub fn read_u64(&mut self) -> Result<u64> {
        if self.position + 8 > self.buffer.len() {
            return Err(anyhow!("Buffer overflow reading u64"));
        }
        let bytes = &self.buffer[self.position..self.position + 8];
        let value = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
        self.position += 8;
        Ok(value)
    }

    pub fn read_f32(&mut self) -> Result<f32> {
        let bits = self.read_u32()?;
        Ok(f32::from_bits(bits))
    }

    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.position + len > self.buffer.len() {
            return Err(anyhow!("Buffer overflow reading {} bytes", len));
        }
        let bytes = &self.buffer[self.position..self.position + len];
        self.position += len;
        Ok(bytes)
    }

    pub fn read_string(&mut self) -> Result<String> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| anyhow!("Invalid UTF-8 string: {}", e))
    }

    pub fn read_string_array(&mut self) -> Result<Vec<String>> {
        let count = self.read_u32()? as usize;
        let mut strings = Vec::with_capacity(count);
        for _ in 0..count {
            strings.push(self.read_string()?);
        }
        Ok(strings)
    }

    pub fn read_embedding(&mut self) -> Result<Embedding> {
        let dims = self.read_u32()? as usize;
        let byte_len = dims * mem::size_of::<f32>();
        let bytes = self.read_bytes(byte_len)?;
        
        // Use unsafe for performance - direct memory copy
        let mut embedding = Vec::with_capacity(dims);
        unsafe {
            let float_ptr = bytes.as_ptr() as *const f32;
            for i in 0..dims {
                embedding.push(*float_ptr.add(i));
            }
        }
        
        Ok(ndarray::Array1::from(embedding))
    }

    pub fn read_optional_embedding(&mut self) -> Result<Option<Embedding>> {
        let has_embedding = self.read_u8()? != 0;
        if has_embedding {
            Ok(Some(self.read_embedding()?))
        } else {
            Ok(None)
        }
    }

    pub fn read_metadata(&mut self) -> Result<HashMap<String, String>> {
        let count = self.read_u32()? as usize;
        let mut metadata = HashMap::with_capacity(count);
        for _ in 0..count {
            let key = self.read_string()?;
            let value = self.read_string()?;
            metadata.insert(key, value);
        }
        Ok(metadata)
    }

    /// Parse complete binary message with header validation
    pub fn parse_message(&mut self) -> Result<ParsedBinaryMessage> {
        // Read and validate header
        let header = BinaryMessageHeader::from_bytes(&self.buffer[self.position..])?;
        self.position += 16;

        // Read payload
        let payload_data = self.read_bytes(header.payload_length as usize)?;

        // Decompress if needed
        let final_payload = if header.flags & (BinaryMessageFlags::Compressed as u16) != 0 {
            decompress_size_prepended(payload_data)
                .map_err(|e| anyhow!("Decompression failed: {}", e))?
        } else {
            payload_data.to_vec()
        };

        // Verify CRC32
        let expected_crc = self.read_u32()?;
        let mut hasher = Hasher::new();
        hasher.update(&self.buffer[0..self.position - 4]);
        let actual_crc = hasher.finalize();
        
        if expected_crc != actual_crc {
            return Err(anyhow!(
                "CRC32 mismatch: expected {:x}, got {:x}",
                expected_crc,
                actual_crc
            ));
        }

        Ok(ParsedBinaryMessage {
            header,
            payload: final_payload,
        })
    }
}

/// Parsed binary message
pub struct ParsedBinaryMessage {
    pub header: BinaryMessageHeader,
    pub payload: Vec<u8>,
}

impl ParsedBinaryMessage {
    pub fn message_type(&self) -> Result<BinaryMessageType> {
        BinaryMessageType::from_u16(self.header.message_type)
    }

    pub fn payload_deserializer(&self) -> BinaryDeserializer {
        BinaryDeserializer::new(&self.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_header_serialization() {
        let header = BinaryMessageHeader::new(
            BinaryMessageType::AddMemory,
            1024,
            12345,
            BinaryMessageFlags::Compressed as u16,
        );

        let bytes = header.to_bytes();
        let parsed = BinaryMessageHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.magic, DSR_BINARY_MAGIC);
        assert_eq!(parsed.version, DSR_PROTOCOL_VERSION);
        assert_eq!(parsed.message_type, BinaryMessageType::AddMemory as u16);
        assert_eq!(parsed.payload_length, 1024);
        assert_eq!(parsed.sequence_id, 12345);
    }

    #[test]
    fn test_embedding_serialization_performance() {
        let embedding = ndarray::Array1::from(vec![0.1f32; 384]);
        let mut serializer = BinarySerializer::new(4096);

        let start = Instant::now();
        for _ in 0..1000 {
            serializer.reset();
            serializer.write_embedding(&embedding).unwrap();
        }
        let elapsed = start.elapsed();

        println!(
            "Embedding serialization: {:.2}μs per operation",
            elapsed.as_nanos() as f64 / 1000.0 / 1000.0
        );

        // Should be much faster than 100μs
        assert!(elapsed.as_nanos() / 1000 < 100_000);
    }

    #[test]
    fn test_round_trip_serialization() {
        let memory_id = 12345u64;
        let embedding = ndarray::Array1::from(vec![0.1, 0.2, 0.3, 0.4, 0.5]);
        let content = "Test memory content";
        let tags = vec!["test".to_string(), "performance".to_string()];
        let metadata = {
            let mut map = HashMap::new();
            map.insert("source".to_string(), "test".to_string());
            map
        };

        // Serialize
        let mut serializer = BinarySerializer::new(1024);
        serializer.serialize_add_memory_request(
            memory_id,
            &embedding,
            content,
            &tags,
            &metadata,
        ).unwrap();

        let message = serializer.create_message(
            BinaryMessageType::AddMemory,
            98765,
        ).unwrap();

        // Deserialize
        let mut deserializer = BinaryDeserializer::new(&message);
        let parsed = deserializer.parse_message().unwrap();
        
        assert_eq!(parsed.message_type().unwrap(), BinaryMessageType::AddMemory);
        assert_eq!(parsed.header.sequence_id, 98765);

        let mut payload_deserializer = parsed.payload_deserializer();
        let parsed_memory_id = payload_deserializer.read_u64().unwrap();
        let parsed_embedding = payload_deserializer.read_embedding().unwrap();
        let parsed_content = payload_deserializer.read_string().unwrap();
        let parsed_tags = payload_deserializer.read_string_array().unwrap();
        let parsed_metadata = payload_deserializer.read_metadata().unwrap();

        assert_eq!(parsed_memory_id, memory_id);
        assert_eq!(parsed_embedding.len(), embedding.len());
        assert_eq!(parsed_content, content);
        assert_eq!(parsed_tags, tags);
        assert_eq!(parsed_metadata, metadata);
    }
}