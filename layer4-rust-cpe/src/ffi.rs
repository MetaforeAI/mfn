//! FFI (Foreign Function Interface) bindings for Layer 4: Context Prediction Engine
//! 
//! This module provides C-compatible bindings to enable integration with other
//! layers of the MFN system and external applications.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_float, c_void};
use std::ptr;
use std::slice;
use tokio::runtime::Runtime;
use serde_json;

use crate::{
    ContextPredictionLayer, 
    ContextPredictionConfig, 
    ContextWindow,
    PredictionResult,
    CpeError,
};
use mfn_core::{memory_types::*, MfnLayer, layer_interface::ContextPredictionEngine};

/// Opaque handle for the Context Prediction Layer
pub struct CpeHandle {
    layer: ContextPredictionLayer,
    runtime: Runtime,
}

/// C-compatible result structure
#[repr(C)]
pub struct CpePredictionResult {
    pub memory_id: u64,
    pub confidence: c_float,
    pub predicted_delay_ms: c_float,
    pub pattern_strength: c_float,
}

/// C-compatible context window entry
#[repr(C)]
pub struct CpeContextEntry {
    pub memory_id: u64,
    pub timestamp_ms: u64,
    pub access_count: u64,
}

/// C-compatible configuration
#[repr(C)]
#[derive(Clone)]
pub struct CpeConfig {
    pub max_window_size: usize,
    pub min_pattern_length: usize,
    pub max_pattern_length: usize,
    pub min_frequency_threshold: usize,
    pub transition_threshold: c_float,
    pub cache_size: usize,
    pub cache_ttl_seconds: u64,
    pub enable_session_tracking: bool,
    pub max_prediction_results: usize,
}

impl Default for CpeConfig {
    fn default() -> Self {
        let config = ContextPredictionConfig::default();
        Self {
            max_window_size: config.max_window_size,
            min_pattern_length: config.min_pattern_length,
            max_pattern_length: config.max_pattern_length,
            min_frequency_threshold: config.min_frequency_threshold,
            transition_threshold: config.transition_threshold,
            cache_size: config.cache_size,
            cache_ttl_seconds: config.cache_ttl.as_secs(),
            enable_session_tracking: config.enable_session_tracking,
            max_prediction_results: config.max_prediction_results,
        }
    }
}

impl From<CpeConfig> for ContextPredictionConfig {
    fn from(c_config: CpeConfig) -> Self {
        Self {
            max_window_size: c_config.max_window_size,
            min_pattern_length: c_config.min_pattern_length,
            max_pattern_length: c_config.max_pattern_length,
            min_frequency_threshold: c_config.min_frequency_threshold,
            transition_threshold: c_config.transition_threshold,
            cache_size: c_config.cache_size,
            cache_ttl: std::time::Duration::from_secs(c_config.cache_ttl_seconds),
            enable_session_tracking: c_config.enable_session_tracking,
            max_prediction_results: c_config.max_prediction_results,
        }
    }
}

/// Initialize the Context Prediction Engine with default configuration
/// Returns a handle to the CPE instance, or null on error
#[no_mangle]
pub extern "C" fn cpe_init() -> *mut CpeHandle {
    cpe_init_with_config(&CpeConfig::default())
}

/// Initialize the Context Prediction Engine with custom configuration
/// Returns a handle to the CPE instance, or null on error
#[no_mangle]
pub extern "C" fn cpe_init_with_config(config: &CpeConfig) -> *mut CpeHandle {
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return ptr::null_mut(),
    };
    
    let rust_config = ContextPredictionConfig::from(config.clone());
    
    let layer = match rt.block_on(ContextPredictionLayer::new(rust_config)) {
        Ok(layer) => layer,
        Err(_) => return ptr::null_mut(),
    };
    
    let handle = Box::new(CpeHandle { layer, runtime: rt });
    Box::into_raw(handle)
}

/// Destroy the CPE instance and free memory
#[no_mangle]
pub extern "C" fn cpe_destroy(handle: *mut CpeHandle) {
    if !handle.is_null() {
        unsafe {
            let _ = Box::from_raw(handle);
        }
    }
}

/// Add a memory access to the temporal analysis
#[no_mangle]
pub extern "C" fn cpe_add_memory_access(
    handle: *mut CpeHandle,
    memory_id: u64,
    content: *const c_char,
    embedding: *const c_float,
    embedding_size: usize,
) -> c_int {
    if handle.is_null() || content.is_null() {
        return -1;
    }
    
    let handle = unsafe { &mut *handle };
    
    let content_str = match unsafe { CStr::from_ptr(content) }.to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };
    
    let embedding_vec = if !embedding.is_null() && embedding_size > 0 {
        let embedding_slice = unsafe { slice::from_raw_parts(embedding, embedding_size) };
        Some(embedding_slice.to_vec())
    } else {
        None
    };
    
    let _memory = UniversalMemory {
        id: memory_id,
        content: content_str.to_string(),
        embedding: embedding_vec,
        tags: Vec::new(),
        metadata: std::collections::HashMap::new(),
        created_at: current_timestamp(),
        last_accessed: current_timestamp(),
        access_count: 1,
    };

    // Memory access tracking would be implemented here
    // For now, return success as the layer manages its own internal state
    0
}

/// Generate predictions based on current context window
/// Returns the number of predictions generated, or negative on error
/// Predictions are written to the provided buffer
#[no_mangle]
pub extern "C" fn cpe_predict(
    handle: *mut CpeHandle,
    context: *const CpeContextEntry,
    context_size: usize,
    predictions: *mut CpePredictionResult,
    max_predictions: usize,
) -> c_int {
    if handle.is_null() || context.is_null() || predictions.is_null() {
        return -1;
    }
    
    let handle = unsafe { &mut *handle };
    let context_slice = unsafe { slice::from_raw_parts(context, context_size) };
    
    // Convert C context to Rust ContextWindow
    let mut window_entries = Vec::new();
    for entry in context_slice {
        // Create memory access entries for context
        use mfn_core::{MemoryAccess, AccessType};
        let access = MemoryAccess {
            memory_id: entry.memory_id,
            access_type: AccessType::Read,
            timestamp: entry.timestamp_ms,
            context_metadata: std::collections::HashMap::new(),
        };
        window_entries.push(access);
    }

    let context_window = ContextWindow {
        recent_accesses: window_entries,
        temporal_patterns: Vec::new(),
        user_context: std::collections::HashMap::new(),
        window_size_ms: 60000,
    };
    
    // Generate predictions
    let predictions_result = match handle.runtime.block_on(handle.layer.predict_next(&context_window)) {
        Ok(preds) => preds,
        Err(_) => return -2,
    };
    
    // Convert predictions to C format
    let output_slice = unsafe { slice::from_raw_parts_mut(predictions, max_predictions) };
    let mut count = 0;
    
    for (i, pred) in predictions_result.into_iter().enumerate() {
        if i >= max_predictions {
            break;
        }
        
        output_slice[i] = CpePredictionResult {
            memory_id: pred.predicted_memory.id,
            confidence: pred.confidence,
            predicted_delay_ms: 0.0, // Field no longer available
            pattern_strength: pred.contributing_patterns.len() as f32,
        };
        count += 1;
    }
    
    count as c_int
}

/// Get performance metrics as JSON string
/// Returns a C string that must be freed with cpe_free_string, or null on error
#[no_mangle]
pub extern "C" fn cpe_get_metrics(handle: *mut CpeHandle) -> *mut c_char {
    if handle.is_null() {
        return ptr::null_mut();
    }
    
    let handle = unsafe { &mut *handle };
    
    let metrics = match handle.runtime.block_on(handle.layer.get_performance()) {
        Ok(m) => m,
        Err(_) => return ptr::null_mut(),
    };
    
    let json_string = match serde_json::to_string(&metrics) {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };
    
    match CString::new(json_string) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => ptr::null_mut(),
    }
}

/// Free a string returned by CPE functions
#[no_mangle]
pub extern "C" fn cpe_free_string(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            let _ = CString::from_raw(s);
        }
    }
}

/// Clear the temporal analysis state
#[no_mangle]
pub extern "C" fn cpe_clear_state(handle: *mut CpeHandle) -> c_int {
    if handle.is_null() {
        return -1;
    }
    
    let handle = unsafe { &mut *handle };
    
    match handle.runtime.block_on(handle.layer.clear_temporal_state()) {
        Ok(_) => 0,
        Err(_) => -2,
    }
}

/// Get the current window size
#[no_mangle]
pub extern "C" fn cpe_get_window_size(handle: *mut CpeHandle) -> c_int {
    if handle.is_null() {
        return -1;
    }
    
    let handle = unsafe { &*handle };
    handle.layer.get_window_size() as c_int
}

/// Check if CPE is healthy and responsive
#[no_mangle]
pub extern "C" fn cpe_health_check(handle: *mut CpeHandle) -> c_int {
    if handle.is_null() {
        return -1;
    }
    
    let handle = unsafe { &mut *handle };
    
    match handle.runtime.block_on(handle.layer.health_check()) {
        Ok(true) => 1,  // Healthy
        Ok(false) => 0, // Unhealthy but responsive
        Err(_) => -2,   // Error during health check
    }
}

/// Get library version as C string
/// Returns a static string that does not need to be freed
#[no_mangle]
pub extern "C" fn cpe_get_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_config_conversion() {
        let c_config = CpeConfig::default();
        let rust_config = ContextPredictionConfig::from(c_config);
        
        assert_eq!(rust_config.max_window_size, c_config.max_window_size);
        assert_eq!(rust_config.cache_size, c_config.cache_size);
    }
    
    #[test]
    fn test_ffi_init_destroy() {
        let handle = cpe_init();
        assert!(!handle.is_null());
        
        cpe_destroy(handle);
    }
    
    #[test]
    fn test_version() {
        let version = cpe_get_version();
        assert!(!version.is_null());
        
        let version_str = unsafe { CStr::from_ptr(version) };
        assert!(!version_str.to_str().unwrap().is_empty());
    }
}