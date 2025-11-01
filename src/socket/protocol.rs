//! Binary Protocol Implementation for Socket Communication
//!
//! Integrates the MFN binary protocol throughout the socket layer,
//! providing high-performance serialization with LZ4 compression.

use std::io::{self, Write, Read, Cursor};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use bytes::{Bytes, BytesMut, BufMut};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::socket::SocketError;

/// Binary protocol constants
pub const PROTOCOL_MAGIC: u32 = 0x4D464E53; // "MFNS" - MFN Socket
pub const PROTOCOL_VERSION: u16 = 0x0001;
pub const MAX_PAYLOAD_SIZE: usize = 100 * 1024 * 1024; // 100MB

/// Message header structure (24 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MessageHeader {
    /// Protocol magic number (4 bytes)
    pub magic: u32,
    /// Protocol version (2 bytes)
    pub version: u16,
    /// Message type (2 bytes)
    pub msg_type: u16,
    /// Message flags (2 bytes)
    pub flags: u16,
    /// Payload size in bytes (4 bytes)
    pub payload_size: u32,
    /// Request/correlation ID (8 bytes)
    pub correlation_id: u64,
    /// Reserved for future use (2 bytes)
    pub reserved: u16,
}

impl MessageHeader {
    pub const SIZE: usize = 24;

    pub fn new(msg_type: MessageType, correlation_id: u64, payload_size: u32) -> Self {
        Self {
            magic: PROTOCOL_MAGIC,
            version: PROTOCOL_VERSION,
            msg_type: msg_type as u16,
            flags: 0,
            payload_size,
            correlation_id,
            reserved: 0,
        }
    }

    pub fn validate(&self) -> Result<(), SocketError> {
        let magic = self.magic;
        let version = self.version;

        if magic != PROTOCOL_MAGIC {
            return Err(SocketError::Protocol(format!(
                "Invalid magic: 0x{:08X}", magic
            )));
        }
        if version != PROTOCOL_VERSION {
            return Err(SocketError::Protocol(format!(
                "Unsupported version: {}", version
            )));
        }
        if self.payload_size as usize > MAX_PAYLOAD_SIZE {
            return Err(SocketError::MessageTooLarge {
                size: self.payload_size as usize,
                max: MAX_PAYLOAD_SIZE,
            });
        }
        Ok(())
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&self.msg_type.to_le_bytes());
        buf[8..10].copy_from_slice(&self.flags.to_le_bytes());
        buf[10..14].copy_from_slice(&self.payload_size.to_le_bytes());
        buf[14..22].copy_from_slice(&self.correlation_id.to_le_bytes());
        buf[22..24].copy_from_slice(&self.reserved.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8]) -> Result<Self, SocketError> {
        if buf.len() < Self::SIZE {
            return Err(SocketError::Protocol(format!(
                "Header too small: {} bytes", buf.len()
            )));
        }

        let header = Self {
            magic: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            version: u16::from_le_bytes([buf[4], buf[5]]),
            msg_type: u16::from_le_bytes([buf[6], buf[7]]),
            flags: u16::from_le_bytes([buf[8], buf[9]]),
            payload_size: u32::from_le_bytes([buf[10], buf[11], buf[12], buf[13]]),
            correlation_id: u64::from_le_bytes([
                buf[14], buf[15], buf[16], buf[17],
                buf[18], buf[19], buf[20], buf[21],
            ]),
            reserved: u16::from_le_bytes([buf[22], buf[23]]),
        };

        header.validate()?;
        Ok(header)
    }
}

/// Message types
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    // Memory operations
    MemoryAdd = 0x0001,
    MemoryGet = 0x0002,
    MemoryUpdate = 0x0003,
    MemoryDelete = 0x0004,

    // Search operations
    SearchSimilarity = 0x0010,
    SearchAssociative = 0x0011,
    SearchTemporal = 0x0012,

    // Layer-specific operations
    Layer1Store = 0x0020,
    Layer2Similarity = 0x0021,
    Layer3Associate = 0x0022,
    Layer4Context = 0x0023,

    // Control messages
    Ping = 0x0030,
    Stats = 0x0031,
    Configure = 0x0032,
    Optimize = 0x0033,

    // Batch operations
    BatchRequest = 0x0040,
    BatchResponse = 0x0041,

    // Response types
    Success = 0x8000,
    Error = 0x8001,
    Partial = 0x8002,
    Redirect = 0x8003,

    // Stream types
    StreamStart = 0x9000,
    StreamData = 0x9001,
    StreamEnd = 0x9002,
}

impl MessageType {
    pub fn is_response(&self) -> bool {
        (*self as u16) >= 0x8000
    }

    pub fn is_stream(&self) -> bool {
        (*self as u16) >= 0x9000
    }
}

/// Message flags
#[repr(u16)]
pub enum MessageFlags {
    None = 0x0000,
    Compressed = 0x0001,
    Encrypted = 0x0002,
    Priority = 0x0004,
    NoReply = 0x0008,
    Broadcast = 0x0010,
    Persistent = 0x0020,
}

/// Complete socket message with header and payload
pub struct SocketMessage {
    pub header: MessageHeader,
    pub payload: Bytes,
    pub timestamp: Instant,
}

impl SocketMessage {
    pub fn new(msg_type: MessageType, correlation_id: u64, payload: Bytes) -> Self {
        let header = MessageHeader::new(msg_type, correlation_id, payload.len() as u32);
        Self {
            header,
            payload,
            timestamp: Instant::now(),
        }
    }

    pub fn with_flags(mut self, flags: u16) -> Self {
        self.header.flags = flags;
        self
    }

    pub fn age(&self) -> Duration {
        self.timestamp.elapsed()
    }

    /// Serialize message to bytes with optional compression
    pub fn to_bytes(&self, compress: bool) -> Result<Bytes, SocketError> {
        let mut result = BytesMut::with_capacity(
            MessageHeader::SIZE + self.payload.len() + 4
        );

        // Apply compression if requested and beneficial
        let (final_payload, mut header) = if compress && self.payload.len() > 512 {
            match lz4_compress(&self.payload) {
                Ok(compressed) if compressed.len() < self.payload.len() => {
                    let mut h = self.header;
                    h.flags |= MessageFlags::Compressed as u16;
                    h.payload_size = compressed.len() as u32;
                    (compressed, h)
                }
                _ => (self.payload.clone(), self.header),
            }
        } else {
            (self.payload.clone(), self.header)
        };

        // Write header
        result.put_slice(&header.to_bytes());

        // Write payload
        result.put_slice(&final_payload);

        // Write CRC32
        let crc = calculate_crc32(&result);
        result.put_u32_le(crc);

        Ok(result.freeze())
    }

    /// Parse message from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, SocketError> {
        if data.len() < MessageHeader::SIZE + 4 {
            return Err(SocketError::Protocol(format!(
                "Message too small: {} bytes", data.len()
            )));
        }

        // Parse header
        let header = MessageHeader::from_bytes(&data[..MessageHeader::SIZE])?;

        // Verify message size
        let expected_size = MessageHeader::SIZE + header.payload_size as usize + 4;
        if data.len() != expected_size {
            return Err(SocketError::Protocol(format!(
                "Size mismatch: expected {}, got {}", expected_size, data.len()
            )));
        }

        // Verify CRC32
        let payload_end = MessageHeader::SIZE + header.payload_size as usize;
        let message_crc = u32::from_le_bytes([
            data[payload_end],
            data[payload_end + 1],
            data[payload_end + 2],
            data[payload_end + 3],
        ]);
        let calculated_crc = calculate_crc32(&data[..payload_end]);
        if message_crc != calculated_crc {
            return Err(SocketError::Protocol(format!(
                "CRC mismatch: expected {:08X}, got {:08X}", message_crc, calculated_crc
            )));
        }

        // Extract payload
        let payload_data = &data[MessageHeader::SIZE..payload_end];

        // Decompress if needed
        let final_payload = if header.flags & (MessageFlags::Compressed as u16) != 0 {
            lz4_decompress(payload_data)?
        } else {
            Bytes::copy_from_slice(payload_data)
        };

        Ok(Self {
            header,
            payload: final_payload,
            timestamp: Instant::now(),
        })
    }
}

/// Socket protocol handler
pub struct SocketProtocol {
    compression_threshold: usize,
    enable_compression: bool,
    enable_crc: bool,
}

impl SocketProtocol {
    pub fn new() -> Self {
        Self {
            compression_threshold: 1024,
            enable_compression: true,
            enable_crc: true,
        }
    }

    pub fn with_compression(mut self, enable: bool, threshold: usize) -> Self {
        self.enable_compression = enable;
        self.compression_threshold = threshold;
        self
    }

    /// Read a complete message from an async stream
    pub async fn read_message<R: AsyncReadExt + Unpin>(
        &self,
        reader: &mut R,
    ) -> Result<SocketMessage, SocketError> {
        // Read header
        let mut header_buf = [0u8; MessageHeader::SIZE];
        reader.read_exact(&mut header_buf).await?;
        let header = MessageHeader::from_bytes(&header_buf)?;

        // Read payload and CRC
        let mut payload_buf = vec![0u8; header.payload_size as usize + 4];
        reader.read_exact(&mut payload_buf).await?;

        // Combine and parse
        let mut full_message = Vec::with_capacity(header_buf.len() + payload_buf.len());
        full_message.extend_from_slice(&header_buf);
        full_message.extend_from_slice(&payload_buf);

        SocketMessage::from_bytes(&full_message)
    }

    /// Write a complete message to an async stream
    pub async fn write_message<W: AsyncWriteExt + Unpin>(
        &self,
        writer: &mut W,
        message: &SocketMessage,
    ) -> Result<(), SocketError> {
        let compress = self.enable_compression &&
                       message.payload.len() >= self.compression_threshold;
        let data = message.to_bytes(compress)?;
        writer.write_all(&data).await?;
        writer.flush().await?;
        Ok(())
    }
}

/// LZ4 compression wrapper
fn lz4_compress(data: &[u8]) -> Result<Bytes, SocketError> {
    // Use lz4_flex for actual compression
    // This is a placeholder that should be replaced with actual LZ4 implementation
    match lz4_flex::compress_prepend_size(data) {
        compressed => Ok(Bytes::from(compressed)),
    }
}

/// LZ4 decompression wrapper
fn lz4_decompress(data: &[u8]) -> Result<Bytes, SocketError> {
    // Use lz4_flex for actual decompression
    match lz4_flex::decompress_size_prepended(data) {
        Ok(decompressed) => Ok(Bytes::from(decompressed)),
        Err(e) => Err(SocketError::Protocol(format!("Decompression failed: {}", e))),
    }
}

/// CRC32 calculation for message integrity
fn calculate_crc32(data: &[u8]) -> u32 {
    const CRC32_POLY: u32 = 0xEDB88320;
    let mut crc = 0xFFFFFFFFu32;

    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ CRC32_POLY
            } else {
                crc >> 1
            };
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_serialization() {
        let header = MessageHeader::new(MessageType::MemoryAdd, 12345, 1024);
        let bytes = header.to_bytes();
        let parsed = MessageHeader::from_bytes(&bytes).unwrap();

        // Copy packed fields to local variables to avoid unaligned references
        let h_magic = header.magic;
        let h_msg_type = header.msg_type;
        let h_correlation_id = header.correlation_id;
        let h_payload_size = header.payload_size;
        let p_magic = parsed.magic;
        let p_msg_type = parsed.msg_type;
        let p_correlation_id = parsed.correlation_id;
        let p_payload_size = parsed.payload_size;

        assert_eq!(h_magic, p_magic);
        assert_eq!(h_msg_type, p_msg_type);
        assert_eq!(h_correlation_id, p_correlation_id);
        assert_eq!(h_payload_size, p_payload_size);
    }

    #[test]
    fn test_message_roundtrip() {
        let payload = Bytes::from(vec![1, 2, 3, 4, 5]);
        let message = SocketMessage::new(MessageType::Ping, 42, payload.clone());

        let serialized = message.to_bytes(false).unwrap();
        let deserialized = SocketMessage::from_bytes(&serialized).unwrap();

        // Copy packed fields to local variables to avoid unaligned references
        let m_msg_type = message.header.msg_type;
        let m_correlation_id = message.header.correlation_id;
        let d_msg_type = deserialized.header.msg_type;
        let d_correlation_id = deserialized.header.correlation_id;

        assert_eq!(m_msg_type, d_msg_type);
        assert_eq!(m_correlation_id, d_correlation_id);
        assert_eq!(message.payload, deserialized.payload);
    }

    #[test]
    fn test_compression() {
        let large_payload = Bytes::from(vec![42u8; 2048]);
        let message = SocketMessage::new(MessageType::MemoryAdd, 1, large_payload.clone());

        let compressed = message.to_bytes(true).unwrap();
        let uncompressed = message.to_bytes(false).unwrap();

        // Compressed should be smaller for repetitive data
        assert!(compressed.len() < uncompressed.len());

        // Both should deserialize correctly
        let msg1 = SocketMessage::from_bytes(&compressed).unwrap();
        let msg2 = SocketMessage::from_bytes(&uncompressed).unwrap();

        assert_eq!(msg1.payload, msg2.payload);
    }
}