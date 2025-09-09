//! Error types for the Context Prediction Engine

use thiserror::Error;

/// Result type alias for CPE operations
pub type CpeResult<T> = Result<T, CpeError>;

/// Comprehensive error types for the Context Prediction Engine
#[derive(Error, Debug)]
pub enum CpeError {
    #[error("Temporal analysis error: {message}")]
    TemporalAnalysis { message: String },
    
    #[error("Prediction generation error: {message}")]
    PredictionGeneration { message: String },
    
    #[error("Cache operation error: {message}")]
    CacheOperation { message: String },
    
    #[error("Configuration error: {message}")]
    Configuration { message: String },
    
    #[error("Pattern matching error: {message}")]
    PatternMatching { message: String },
    
    #[error("Statistical model error: {message}")]
    StatisticalModel { message: String },
    
    #[error("Memory access tracking error: {message}")]
    MemoryTracking { message: String },
    
    #[error("Session management error: {message}")]
    SessionManagement { message: String },
    
    #[error("Data serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("I/O operation error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("MFN Core error: {message}")]
    MfnCore { message: String },
    
    #[error("FFI interface error: {message}")]
    Ffi { message: String },
    
    #[error("Async task error: {0}")]
    Task(#[from] tokio::task::JoinError),
    
    #[error("Internal system error: {message}")]
    Internal { message: String },
}

impl CpeError {
    /// Create a new temporal analysis error
    pub fn temporal_analysis<S: Into<String>>(message: S) -> Self {
        Self::TemporalAnalysis {
            message: message.into(),
        }
    }
    
    /// Create a new prediction generation error
    pub fn prediction_generation<S: Into<String>>(message: S) -> Self {
        Self::PredictionGeneration {
            message: message.into(),
        }
    }
    
    /// Create a new cache operation error
    pub fn cache_operation<S: Into<String>>(message: S) -> Self {
        Self::CacheOperation {
            message: message.into(),
        }
    }
    
    /// Create a new configuration error
    pub fn configuration<S: Into<String>>(message: S) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }
    
    /// Create a new pattern matching error
    pub fn pattern_matching<S: Into<String>>(message: S) -> Self {
        Self::PatternMatching {
            message: message.into(),
        }
    }
    
    /// Create a new statistical model error
    pub fn statistical_model<S: Into<String>>(message: S) -> Self {
        Self::StatisticalModel {
            message: message.into(),
        }
    }
    
    /// Create a new memory tracking error
    pub fn memory_tracking<S: Into<String>>(message: S) -> Self {
        Self::MemoryTracking {
            message: message.into(),
        }
    }
    
    /// Create a new session management error
    pub fn session_management<S: Into<String>>(message: S) -> Self {
        Self::SessionManagement {
            message: message.into(),
        }
    }
    
    /// Create a new MFN Core error
    pub fn mfn_core<S: Into<String>>(message: S) -> Self {
        Self::MfnCore {
            message: message.into(),
        }
    }
    
    /// Create a new FFI interface error
    pub fn ffi<S: Into<String>>(message: S) -> Self {
        Self::Ffi {
            message: message.into(),
        }
    }
    
    /// Create a new internal system error
    pub fn internal<S: Into<String>>(message: S) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

/// Convert from MFN Core layer result errors
impl From<mfn_core::layer_interface::LayerError> for CpeError {
    fn from(err: mfn_core::layer_interface::LayerError) -> Self {
        Self::mfn_core(format!("MFN Layer error: {}", err))
    }
}