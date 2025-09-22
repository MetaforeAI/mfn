//! FFI (Foreign Function Interface) bindings for Layer 2
//! 
//! Provides C-compatible interface for integration with:
//! - Zig Layer 1 (Immediate Flow Registry) 
//! - Go Layer 3 (Associative Link Mesh)
//! - Other language components in the MFN system

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_float, c_int, c_uint, c_void};
use std::ptr;
use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap;

use anyhow::Result;
use ndarray::Array1;

use crate::{DynamicSimilarityReservoir, DSRConfig, MemoryId, SimilarityResults, EncodingStrategy};

/// Global registry of DSR instances for FFI access
static DSR_REGISTRY: OnceLock<Mutex<HashMap<u32, Arc<DynamicSimilarityReservoir>>>> = OnceLock::new();
static NEXT_DSR_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// FFI-compatible error codes
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum MFNError {
    Success = 0,
    InvalidHandle = 1,
    InvalidParameters = 2,
    OutOfMemory = 3,
    InitializationFailed = 4,
    ProcessingFailed = 5,
    NotFound = 6,
}

/// FFI-compatible configuration structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DSRConfigFFI {
    pub reservoir_size: c_uint,
    pub embedding_dim: c_uint,
    pub encoding_strategy: c_uint, // 0=Rate, 1=Temporal, 2=Population, 3=Delta, 4=RankOrder
    pub similarity_threshold: c_float,
    pub competition_strength: c_float,
    pub integration_window_ms: c_float,
    pub max_similarity_wells: c_uint,
}

impl Default for DSRConfigFFI {
    fn default() -> Self {
        let default_config = DSRConfig::default();
        Self {
            reservoir_size: default_config.reservoir_size as c_uint,
            embedding_dim: default_config.embedding_dim as c_uint,
            encoding_strategy: 0, // RateCoding
            similarity_threshold: default_config.similarity_threshold,
            competition_strength: default_config.competition_strength,
            integration_window_ms: default_config.integration_window_ms,
            max_similarity_wells: default_config.max_similarity_wells as c_uint,
        }
    }
}

/// FFI-compatible similarity match structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SimilarityMatchFFI {
    pub memory_id: u64,
    pub confidence: c_float,
    pub raw_activation: c_float,
    pub rank: c_uint,
    pub content_ptr: *const c_char, // Pointer to null-terminated string
}

/// FFI-compatible similarity results structure
#[repr(C)]
#[derive(Debug, Clone)]
pub struct SimilarityResultsFFI {
    pub matches: *mut SimilarityMatchFFI,
    pub match_count: c_uint,
    pub processing_time_ms: c_float,
    pub wells_evaluated: c_uint,
    pub has_confident_matches: c_int, // 0=false, 1=true
}

/// Convert internal config to FFI config
impl From<DSRConfig> for DSRConfigFFI {
    fn from(config: DSRConfig) -> Self {
        let encoding_strategy_num = match config.encoding_strategy {
            EncodingStrategy::RateCoding => 0,
            EncodingStrategy::TemporalCoding => 1,
            EncodingStrategy::PopulationCoding => 2,
            EncodingStrategy::DeltaModulation => 3,
            EncodingStrategy::RankOrderCoding => 4,
        };

        Self {
            reservoir_size: config.reservoir_size as c_uint,
            embedding_dim: config.embedding_dim as c_uint,
            encoding_strategy: encoding_strategy_num,
            similarity_threshold: config.similarity_threshold,
            competition_strength: config.competition_strength,
            integration_window_ms: config.integration_window_ms,
            max_similarity_wells: config.max_similarity_wells as c_uint,
        }
    }
}

/// Convert FFI config to internal config
impl TryFrom<DSRConfigFFI> for DSRConfig {
    type Error = anyhow::Error;

    fn try_from(ffi_config: DSRConfigFFI) -> Result<Self> {
        let encoding_strategy = match ffi_config.encoding_strategy {
            0 => EncodingStrategy::RateCoding,
            1 => EncodingStrategy::TemporalCoding,
            2 => EncodingStrategy::PopulationCoding,
            3 => EncodingStrategy::DeltaModulation,
            4 => EncodingStrategy::RankOrderCoding,
            _ => return Err(anyhow::anyhow!("Invalid encoding strategy: {}", ffi_config.encoding_strategy)),
        };

        Ok(DSRConfig {
            reservoir_size: ffi_config.reservoir_size as usize,
            embedding_dim: ffi_config.embedding_dim as usize,
            encoding_strategy,
            similarity_threshold: ffi_config.similarity_threshold,
            competition_strength: ffi_config.competition_strength,
            integration_window_ms: ffi_config.integration_window_ms,
            max_similarity_wells: ffi_config.max_similarity_wells as usize,
        })
    }
}

/// Initialize the DSR registry
fn get_dsr_registry() -> &'static Mutex<HashMap<u32, Arc<DynamicSimilarityReservoir>>> {
    DSR_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

// ============================================================================
// Public FFI Functions
// ============================================================================

/// Create a new Dynamic Similarity Reservoir instance
/// Returns handle ID on success, 0 on failure
#[no_mangle]
pub extern "C" fn mfn_dsr_create(config: *const DSRConfigFFI) -> u32 {
    if config.is_null() {
        return 0;
    }

    let ffi_config = unsafe { *config };
    
    let internal_config = match DSRConfig::try_from(ffi_config) {
        Ok(config) => config,
        Err(_) => return 0,
    };

    let dsr = match DynamicSimilarityReservoir::new(internal_config) {
        Ok(dsr) => Arc::new(dsr),
        Err(_) => return 0,
    };

    let handle = NEXT_DSR_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    
    if let Ok(mut registry) = get_dsr_registry().lock() {
        registry.insert(handle, dsr);
        handle
    } else {
        0
    }
}

/// Destroy a DSR instance and free resources
#[no_mangle]
pub extern "C" fn mfn_dsr_destroy(handle: u32) -> MFNError {
    if let Ok(mut registry) = get_dsr_registry().lock() {
        if registry.remove(&handle).is_some() {
            MFNError::Success
        } else {
            MFNError::InvalidHandle
        }
    } else {
        MFNError::ProcessingFailed
    }
}

/// Add a memory to the DSR
#[no_mangle]
pub extern "C" fn mfn_dsr_add_memory(
    handle: u32,
    memory_id: u64,
    embedding: *const c_float,
    embedding_size: c_uint,
    content: *const c_char,
) -> MFNError {
    if embedding.is_null() || content.is_null() {
        return MFNError::InvalidParameters;
    }

    let registry = match get_dsr_registry().lock() {
        Ok(registry) => registry,
        Err(_) => return MFNError::ProcessingFailed,
    };

    let dsr = match registry.get(&handle) {
        Some(dsr) => dsr.clone(),
        None => return MFNError::InvalidHandle,
    };

    // Convert C array to ndarray
    let embedding_slice = unsafe {
        std::slice::from_raw_parts(embedding, embedding_size as usize)
    };
    let embedding_array = Array1::from(embedding_slice.to_vec());

    // Convert C string to Rust string
    let content_str = match unsafe { CStr::from_ptr(content) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return MFNError::InvalidParameters,
    };

    // Add memory to DSR
    match dsr.add_memory_sync(MemoryId(memory_id), &embedding_array, content_str) {
        Ok(_) => MFNError::Success,
        Err(_) => MFNError::ProcessingFailed,
    }
}

/// Search for similar memories
/// Returns pointer to results structure, null on failure
/// Caller must free results using mfn_dsr_free_results
#[no_mangle]
pub extern "C" fn mfn_dsr_similarity_search(
    handle: u32,
    query_embedding: *const c_float,
    embedding_size: c_uint,
    top_k: c_uint,
) -> *mut SimilarityResultsFFI {
    if query_embedding.is_null() || top_k == 0 {
        return ptr::null_mut();
    }

    let registry = match get_dsr_registry().lock() {
        Ok(registry) => registry,
        Err(_) => return ptr::null_mut(),
    };

    let dsr = match registry.get(&handle) {
        Some(dsr) => dsr.clone(),
        None => return ptr::null_mut(),
    };

    drop(registry); // Release lock before async operation

    // Convert C array to ndarray
    let embedding_slice = unsafe {
        std::slice::from_raw_parts(query_embedding, embedding_size as usize)
    };
    let embedding_array = Array1::from(embedding_slice.to_vec());

    // Perform similarity search (blocking version of async function)
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return ptr::null_mut(),
    };

    let results = match runtime.block_on(dsr.similarity_search(&embedding_array, top_k as usize)) {
        Ok(results) => results,
        Err(_) => return ptr::null_mut(),
    };

    // Convert to FFI structure
    convert_results_to_ffi(results)
}

/// Get performance statistics
#[no_mangle]
pub extern "C" fn mfn_dsr_get_stats(
    handle: u32,
    total_queries: *mut u64,
    total_additions: *mut u64,
    wells_count: *mut c_uint,
    memory_usage_mb: *mut c_float,
) -> MFNError {
    if total_queries.is_null() || total_additions.is_null() || 
       wells_count.is_null() || memory_usage_mb.is_null() {
        return MFNError::InvalidParameters;
    }

    let registry = match get_dsr_registry().lock() {
        Ok(registry) => registry,
        Err(_) => return MFNError::ProcessingFailed,
    };

    let dsr = match registry.get(&handle) {
        Some(dsr) => dsr,
        None => return MFNError::InvalidHandle,
    };

    let stats = dsr.get_performance_stats_sync();

    unsafe {
        *total_queries = stats.total_queries;
        *total_additions = stats.total_additions;
        *wells_count = stats.similarity_wells_count as c_uint;
        *memory_usage_mb = stats.memory_usage_mb;
    }

    MFNError::Success
}

/// Free similarity results allocated by mfn_dsr_similarity_search
#[no_mangle]
pub extern "C" fn mfn_dsr_free_results(results: *mut SimilarityResultsFFI) {
    if results.is_null() {
        return;
    }

    unsafe {
        let results_ref = &*results;
        
        // Free match array and content strings
        if !results_ref.matches.is_null() {
            let matches_slice = std::slice::from_raw_parts(
                results_ref.matches,
                results_ref.match_count as usize,
            );

            // Free content strings
            for match_item in matches_slice {
                if !match_item.content_ptr.is_null() {
                    let _ = CString::from_raw(match_item.content_ptr as *mut c_char);
                }
            }

            // Free matches array
            let _ = Vec::from_raw_parts(
                results_ref.matches,
                results_ref.match_count as usize,
                results_ref.match_count as usize,
            );
        }

        // Free results structure
        let _ = Box::from_raw(results);
    }
}

/// Get the version of the Layer 2 FFI interface
#[no_mangle]
pub extern "C" fn mfn_dsr_get_version(
    major: *mut c_uint,
    minor: *mut c_uint,
    patch: *mut c_uint,
) -> MFNError {
    if major.is_null() || minor.is_null() || patch.is_null() {
        return MFNError::InvalidParameters;
    }

    unsafe {
        *major = 0;
        *minor = 1;
        *patch = 0;
    }

    MFNError::Success
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert internal SimilarityResults to FFI structure
fn convert_results_to_ffi(results: SimilarityResults) -> *mut SimilarityResultsFFI {
    let match_count = results.matches.len();
    
    // Allocate matches array
    let mut ffi_matches = Vec::with_capacity(match_count);
    
    for match_item in results.matches {
        // Convert content to C string
        let content_cstring = match CString::new(match_item.content) {
            Ok(s) => s,
            Err(_) => CString::new("").unwrap(),
        };
        
        let ffi_match = SimilarityMatchFFI {
            memory_id: match_item.memory_id.0,
            confidence: match_item.confidence,
            raw_activation: match_item.raw_activation,
            rank: match_item.rank as c_uint,
            content_ptr: content_cstring.into_raw(),
        };
        
        ffi_matches.push(ffi_match);
    }

    // Convert to raw pointer
    let matches_ptr = if ffi_matches.is_empty() {
        ptr::null_mut()
    } else {
        let boxed_slice = ffi_matches.into_boxed_slice();
        Box::into_raw(boxed_slice) as *mut SimilarityMatchFFI
    };

    // Create results structure
    let ffi_results = SimilarityResultsFFI {
        matches: matches_ptr,
        match_count: match_count as c_uint,
        processing_time_ms: results.processing_time_ms,
        wells_evaluated: results.wells_evaluated as c_uint,
        has_confident_matches: if results.has_confident_matches { 1 } else { 0 },
    };

    Box::into_raw(Box::new(ffi_results))
}

// ============================================================================
// Integration Functions for Layer 1 (Zig)
// ============================================================================

/// Callback function type for routing decisions from Layer 1
pub type Layer1RoutingCallback = extern "C" fn(
    found_exact: c_int,
    next_layer: c_uint,
    confidence: c_float,
    processing_time_ns: u64,
) -> c_int;

/// Register Layer 2 with Layer 1 for routing decisions
#[no_mangle]
pub extern "C" fn mfn_register_layer2_with_layer1(
    layer1_handle: u32,
    layer2_handle: u32,
    callback: Layer1RoutingCallback,
) -> MFNError {
    // In a real implementation, this would establish the routing connection
    // For now, we just validate that both handles exist
    
    let registry = match get_dsr_registry().lock() {
        Ok(registry) => registry,
        Err(_) => return MFNError::ProcessingFailed,
    };

    if !registry.contains_key(&layer2_handle) {
        return MFNError::InvalidHandle;
    }

    // TODO: Store callback and layer1_handle for routing
    // This would integrate with Layer 1's routing system
    
    MFNError::Success
}

// ============================================================================
// C Header Generation Helper
// ============================================================================

/// Generate C header file for FFI bindings (development helper)
#[cfg(feature = "generate-headers")]
pub fn generate_c_header() -> String {
    r#"
/* MFN Layer 2 (Dynamic Similarity Reservoir) FFI Header */
#ifndef MFN_LAYER2_FFI_H
#define MFN_LAYER2_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Error codes */
typedef enum {
    MFN_SUCCESS = 0,
    MFN_INVALID_HANDLE = 1,
    MFN_INVALID_PARAMETERS = 2,
    MFN_OUT_OF_MEMORY = 3,
    MFN_INITIALIZATION_FAILED = 4,
    MFN_PROCESSING_FAILED = 5,
    MFN_NOT_FOUND = 6
} MFNError;

/* Configuration structure */
typedef struct {
    uint32_t reservoir_size;
    uint32_t embedding_dim;
    uint32_t encoding_strategy;
    float similarity_threshold;
    float competition_strength;
    float integration_window_ms;
    uint32_t max_similarity_wells;
} DSRConfigFFI;

/* Similarity match structure */
typedef struct {
    uint64_t memory_id;
    float confidence;
    float raw_activation;
    uint32_t rank;
    const char* content_ptr;
} SimilarityMatchFFI;

/* Similarity results structure */
typedef struct {
    SimilarityMatchFFI* matches;
    uint32_t match_count;
    float processing_time_ms;
    uint32_t wells_evaluated;
    int has_confident_matches;
} SimilarityResultsFFI;

/* Core functions */
uint32_t mfn_dsr_create(const DSRConfigFFI* config);
MFNError mfn_dsr_destroy(uint32_t handle);
MFNError mfn_dsr_add_memory(uint32_t handle, uint64_t memory_id, 
                            const float* embedding, uint32_t embedding_size,
                            const char* content);
SimilarityResultsFFI* mfn_dsr_similarity_search(uint32_t handle,
                                               const float* query_embedding,
                                               uint32_t embedding_size,
                                               uint32_t top_k);
MFNError mfn_dsr_get_stats(uint32_t handle, uint64_t* total_queries,
                           uint64_t* total_additions, uint32_t* wells_count,
                           float* memory_usage_mb);
void mfn_dsr_free_results(SimilarityResultsFFI* results);
MFNError mfn_dsr_get_version(uint32_t* major, uint32_t* minor, uint32_t* patch);

/* Integration functions */
typedef int (*Layer1RoutingCallback)(int found_exact, uint32_t next_layer,
                                     float confidence, uint64_t processing_time_ns);
MFNError mfn_register_layer2_with_layer1(uint32_t layer1_handle,
                                         uint32_t layer2_handle,
                                         Layer1RoutingCallback callback);

#ifdef __cplusplus
}
#endif

#endif /* MFN_LAYER2_FFI_H */
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DSRConfig;

    #[test]
    fn test_config_conversion() {
        let internal_config = DSRConfig::default();
        let ffi_config = DSRConfigFFI::from(internal_config.clone());
        let converted_back = DSRConfig::try_from(ffi_config).unwrap();

        assert_eq!(internal_config.reservoir_size, converted_back.reservoir_size);
        assert_eq!(internal_config.embedding_dim, converted_back.embedding_dim);
        assert_eq!(internal_config.similarity_threshold, converted_back.similarity_threshold);
    }

    #[test]
    fn test_dsr_creation_and_destruction() {
        let config = DSRConfigFFI::default();
        let handle = mfn_dsr_create(&config);
        
        assert_ne!(handle, 0);
        
        let result = mfn_dsr_destroy(handle);
        assert!(matches!(result, MFNError::Success));
        
        // Double destroy should fail
        let result = mfn_dsr_destroy(handle);
        assert!(matches!(result, MFNError::InvalidHandle));
    }

    #[test]
    fn test_version_info() {
        let mut major = 0;
        let mut minor = 0;
        let mut patch = 0;
        
        let result = mfn_dsr_get_version(&mut major, &mut minor, &mut patch);
        
        assert!(matches!(result, MFNError::Success));
        assert_eq!(major, 0);
        assert_eq!(minor, 1);
        assert_eq!(patch, 0);
    }
}