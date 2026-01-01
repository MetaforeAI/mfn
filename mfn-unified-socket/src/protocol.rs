use serde::{Deserialize, Serialize};
use serde_json::Value;
use anyhow::{Result, anyhow};
use bytes::{BytesMut, BufMut, Buf};

/// Universal request format for all MFN layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedRequest {
    #[serde(rename = "type")]
    pub request_type: String,
    pub request_id: String,
    pub target_layer: String,
    pub pool_id: Option<String>,
    pub payload: Value,
}

/// Universal response format for all MFN layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedResponse {
    #[serde(rename = "type")]
    pub response_type: String,
    pub request_id: String,
    pub source_layer: String,
    pub success: bool,
    pub data: Option<Value>,
    pub error: Option<String>,
    pub processing_time_ms: f64,
}

/// Binary protocol codec
pub struct BinaryProtocol;

impl BinaryProtocol {
    /// Encode message with 4-byte length prefix (little-endian)
    pub fn encode(data: &[u8]) -> Result<Vec<u8>> {
        if data.len() > 10_000_000 {
            return Err(anyhow!("Message too large: {} bytes", data.len()));
        }

        let mut buf = BytesMut::with_capacity(4 + data.len());
        buf.put_u32_le(data.len() as u32);
        buf.put_slice(data);

        Ok(buf.to_vec())
    }

    /// Decode message from buffer (returns message + remaining bytes)
    pub fn decode(buf: &[u8]) -> Result<Option<(Vec<u8>, usize)>> {
        if buf.len() < 4 {
            return Ok(None); // Need more data for length prefix
        }

        let msg_len = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;

        if msg_len > 10_000_000 {
            return Err(anyhow!("Invalid message length: {}", msg_len));
        }

        if buf.len() < 4 + msg_len {
            return Ok(None); // Need more data for full message
        }

        let message = buf[4..4 + msg_len].to_vec();
        Ok(Some((message, 4 + msg_len)))
    }

    /// Encode request to JSON then binary
    pub fn encode_request(req: &UnifiedRequest) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(req)?;
        Self::encode(&json)
    }

    /// Encode response to JSON then binary
    pub fn encode_response(resp: &UnifiedResponse) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(resp)?;
        Self::encode(&json)
    }

    /// Decode binary to request
    pub fn decode_request(data: &[u8]) -> Result<UnifiedRequest> {
        let req: UnifiedRequest = serde_json::from_slice(data)?;
        Ok(req)
    }

    /// Decode binary to response
    pub fn decode_response(data: &[u8]) -> Result<UnifiedResponse> {
        let resp: UnifiedResponse = serde_json::from_slice(data)?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_protocol() {
        let data = b"Hello, World!";
        let encoded = BinaryProtocol::encode(data).unwrap();

        assert_eq!(encoded.len(), 4 + data.len());
        assert_eq!(&encoded[0..4], &[13, 0, 0, 0]); // Little-endian length

        let (decoded, consumed) = BinaryProtocol::decode(&encoded).unwrap().unwrap();
        assert_eq!(decoded, data);
        assert_eq!(consumed, encoded.len());
    }
}
