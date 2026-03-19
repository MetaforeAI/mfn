//! Unix socket client for communicating with MFN layers.
//!
//! Implements two wire protocols matching mfn_client.py:
//! - Layer 1 (IFR): newline-delimited JSON
//! - Layers 2-5: 4-byte little-endian length prefix + JSON

use std::path::Path;
use std::time::Duration;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;
use tracing::{debug, error};
use uuid::Uuid;

use crate::socket::SocketPaths;

/// Maximum accepted response payload (10 MB).
const MAX_RESPONSE_SIZE: u32 = 10_000_000;

/// Error type returned by gateway endpoints.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
    pub layer: Option<u8>,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    layer: Option<u8>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            error: self.message,
            layer: self.layer,
        };
        (self.status, Json(body)).into_response()
    }
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, message: msg.into(), layer: None }
    }

    pub fn bad_gateway(layer: u8, msg: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_GATEWAY, message: msg.into(), layer: Some(layer) }
    }

    pub fn gateway_timeout(layer: u8) -> Self {
        Self {
            status: StatusCode::GATEWAY_TIMEOUT,
            message: "Layer request timed out".into(),
            layer: Some(layer),
        }
    }
}

/// Client that sends JSON payloads to layer Unix sockets.
pub struct LayerClient {
    timeout: Duration,
}

impl LayerClient {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Send a JSON payload to a layer and return the JSON response.
    pub async fn send_to_layer(
        &self,
        layer: u8,
        payload: Value,
    ) -> Result<Value, ApiError> {
        if !(1..=5).contains(&layer) {
            return Err(ApiError::bad_request(format!("Invalid layer: {layer}")));
        }

        let socket_path = SocketPaths::get_layer_socket(layer);
        if !Path::new(&socket_path).exists() {
            return Err(ApiError::bad_gateway(
                layer,
                format!("Layer {layer} socket not available"),
            ));
        }

        let result = timeout(self.timeout, self.exchange(layer, &socket_path, &payload)).await;

        match result {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ApiError::gateway_timeout(layer)),
        }
    }

    /// Check whether a layer socket file exists.
    pub fn layer_available(&self, layer: u8) -> bool {
        let socket_path = SocketPaths::get_layer_socket(layer);
        Path::new(&socket_path).exists()
    }

    /// Attempt to connect and ping a layer, returning true on success.
    pub async fn layer_healthy(&self, layer: u8) -> bool {
        let payload = serde_json::json!({
            "type": "ping",
            "request_id": Uuid::new_v4().to_string(),
        });
        self.send_to_layer(layer, payload).await.is_ok()
    }

    /// Connect, send, and receive on the appropriate socket.
    async fn exchange(
        &self,
        layer: u8,
        socket_path: &Path,
        payload: &Value,
    ) -> Result<Value, ApiError> {
        let mut stream = UnixStream::connect(socket_path)
            .await
            .map_err(|e| {
                error!("Failed to connect to layer {layer}: {e}");
                ApiError::bad_gateway(layer, format!("Connection failed: {e}"))
            })?;

        if layer == 1 {
            self.exchange_newline(&mut stream, layer, payload).await
        } else {
            self.exchange_length_prefix(&mut stream, layer, payload).await
        }
    }

    /// Layer 1 (IFR): newline-delimited JSON protocol.
    async fn exchange_newline(
        &self,
        stream: &mut UnixStream,
        layer: u8,
        payload: &Value,
    ) -> Result<Value, ApiError> {
        // Send: compact JSON + newline
        let mut data = serde_json::to_vec(payload).map_err(|e| {
            ApiError::bad_request(format!("Serialization error: {e}"))
        })?;
        data.push(b'\n');

        stream.write_all(&data).await.map_err(|e| {
            ApiError::bad_gateway(layer, format!("Write failed: {e}"))
        })?;
        stream.flush().await.ok();

        debug!("L1 sent {} bytes", data.len());

        // Read until newline
        let mut buf = Vec::with_capacity(4096);
        let mut byte = [0u8; 1];
        loop {
            let n = stream.read(&mut byte).await.map_err(|e| {
                ApiError::bad_gateway(layer, format!("Read failed: {e}"))
            })?;
            if n == 0 {
                break; // EOF
            }
            buf.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
            if buf.len() > MAX_RESPONSE_SIZE as usize {
                return Err(ApiError::bad_gateway(layer, "Response too large".to_string()));
            }
        }

        // Parse all newline-separated messages, skip spurious errors
        let response_str = String::from_utf8_lossy(&buf);
        for line in response_str.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(trimmed) {
                Ok(msg) => {
                    // Filter spurious errors from Zig buffer bug
                    if msg.get("type").and_then(|t| t.as_str()) == Some("error")
                        && msg.get("request_id").and_then(|r| r.as_str()) == Some("unknown")
                    {
                        debug!("L1: filtering spurious error");
                        continue;
                    }
                    return Ok(msg);
                }
                Err(_) => continue,
            }
        }

        Err(ApiError::bad_gateway(layer, "No valid response from layer".to_string()))
    }

    /// Layers 2-5: 4-byte LE length-prefix + JSON protocol.
    async fn exchange_length_prefix(
        &self,
        stream: &mut UnixStream,
        layer: u8,
        payload: &Value,
    ) -> Result<Value, ApiError> {
        // Send: 4-byte LE length + JSON bytes
        let json_bytes = serde_json::to_vec(payload).map_err(|e| {
            ApiError::bad_request(format!("Serialization error: {e}"))
        })?;
        let length = json_bytes.len() as u32;

        stream.write_all(&length.to_le_bytes()).await.map_err(|e| {
            ApiError::bad_gateway(layer, format!("Write length failed: {e}"))
        })?;
        stream.write_all(&json_bytes).await.map_err(|e| {
            ApiError::bad_gateway(layer, format!("Write payload failed: {e}"))
        })?;
        stream.flush().await.ok();

        debug!("L{layer} sent {length} bytes");

        // Receive: 4-byte LE length + JSON bytes
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.map_err(|e| {
            ApiError::bad_gateway(layer, format!("Read length failed: {e}"))
        })?;
        let resp_len = u32::from_le_bytes(len_buf);

        if resp_len == 0 || resp_len > MAX_RESPONSE_SIZE {
            return Err(ApiError::bad_gateway(
                layer,
                format!("Invalid response length: {resp_len}"),
            ));
        }

        let mut resp_buf = vec![0u8; resp_len as usize];
        stream.read_exact(&mut resp_buf).await.map_err(|e| {
            ApiError::bad_gateway(layer, format!("Read payload failed: {e}"))
        })?;

        serde_json::from_slice(&resp_buf).map_err(|e| {
            ApiError::bad_gateway(layer, format!("Invalid JSON response: {e}"))
        })
    }
}
