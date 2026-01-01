use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MFNConfig {
    pub project: String,
    pub socket_prefix: String,
    pub layers: HashMap<String, LayerConfig>,
    pub orchestrator: OrchestratorConfig,
    pub persistence: PersistenceConfig,
    pub common_protocol: ProtocolConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    pub name: String,
    pub language: String,
    pub socket_path: String,
    pub port: Option<u16>,
    pub protocol: String,
    pub operations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorConfig {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistenceConfig {
    pub base_dir: String,
    pub layer1_dir: String,
    pub layer2_dir: String,
    pub layer3_dir: String,
    pub layer4_dir: String,
    pub layer5_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub binary_format: bool,
    pub length_prefix_bytes: usize,
    pub byte_order: String,
    pub json_payload: bool,
    pub max_message_size: usize,
    pub connection_timeout_ms: u64,
    pub max_connections: usize,
}

impl MFNConfig {
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: MFNConfig = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn get_layer(&self, layer_id: &str) -> Option<&LayerConfig> {
        self.layers.get(layer_id)
    }

    pub fn get_socket_path(&self, layer_id: &str) -> Option<String> {
        self.get_layer(layer_id).map(|l| l.socket_path.clone())
    }
}
