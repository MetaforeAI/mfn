#ifndef MFN_PROTOCOL_H
#define MFN_PROTOCOL_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Protocol Constants
// ============================================================================

#define MFN_MAGIC           0x4D464E01  // 'MFN' + version 1
#define MFN_MAX_CONTENT     (64 * 1024) // 64KB max content
#define MFN_MAX_TAGS        256         // Max tags per memory
#define MFN_MAX_METADATA    128         // Max metadata entries
#define MFN_MAX_EMBEDDING   4096        // Max embedding dimensions
#define MFN_MAX_BATCH       1000        // Max batch operations

// ============================================================================
// Core Enumerations
// ============================================================================

typedef enum {
    // Core operations
    MSG_MEMORY_ADD          = 0x0001,
    MSG_MEMORY_GET          = 0x0002,
    MSG_MEMORY_DELETE       = 0x0003,
    MSG_MEMORY_UPDATE       = 0x0004,
    
    // Association operations
    MSG_ASSOC_ADD           = 0x0010,
    MSG_ASSOC_GET           = 0x0011,
    MSG_ASSOC_DELETE        = 0x0012,
    
    // Search operations
    MSG_SEARCH_EXACT        = 0x0020,
    MSG_SEARCH_SIMILAR      = 0x0021,
    MSG_SEARCH_ASSOC        = 0x0022,
    MSG_SEARCH_BATCH        = 0x0023,
    
    // Control operations
    MSG_HEALTH_CHECK        = 0x0030,
    MSG_PERFORMANCE         = 0x0031,
    MSG_CONFIG              = 0x0032,
    
    // Response types
    MSG_RESPONSE            = 0x8000,  // OR with request type
    MSG_ERROR               = 0x8001,
    MSG_ACK                 = 0x8002,
} mfn_message_type_t;

typedef enum {
    OP_ADD                  = 0x01,
    OP_GET                  = 0x02,
    OP_DELETE               = 0x03,
    OP_UPDATE               = 0x04,
    OP_SEARCH               = 0x05,
    OP_BATCH                = 0x06,
    OP_STREAM               = 0x07,
    OP_HEALTH               = 0x08,
    OP_METRICS              = 0x09,
    OP_CONFIG               = 0x0A,
} mfn_operation_t;

typedef enum {
    LAYER_1_IFR             = 0x01,  // Immediate Flow Registry
    LAYER_2_DSR             = 0x02,  // Dynamic Similarity Reservoir
    LAYER_3_ALM             = 0x03,  // Associative Link Mesh
    LAYER_4_CPE             = 0x04,  // Context Prediction Engine
    LAYER_BROADCAST         = 0xFF,  // All layers
} mfn_layer_id_t;

typedef enum {
    ASSOC_SEMANTIC          = 0x01,
    ASSOC_TEMPORAL          = 0x02,
    ASSOC_CAUSAL            = 0x03,
    ASSOC_SPATIAL           = 0x04,
    ASSOC_CONCEPTUAL        = 0x05,
    ASSOC_HIERARCHICAL      = 0x06,
    ASSOC_FUNCTIONAL        = 0x07,
    ASSOC_DOMAIN            = 0x08,
    ASSOC_COGNITIVE         = 0x09,
    ASSOC_CUSTOM_BASE       = 0xF0,  // Custom types 0xF0-0xFF
} mfn_association_type_t;

typedef enum {
    FLAG_COMPRESSED         = 0x0001,  // Payload is compressed
    FLAG_ENCRYPTED          = 0x0002,  // Payload is encrypted
    FLAG_STREAMING          = 0x0004,  // Streaming/partial message
    FLAG_PRIORITY           = 0x0008,  // High priority message
    FLAG_ZERO_COPY          = 0x0010,  // Zero-copy shared memory
    FLAG_BATCH              = 0x0020,  // Batch operation
    FLAG_ASYNC              = 0x0040,  // Asynchronous operation
    FLAG_CACHED             = 0x0080,  // Cached response allowed
} mfn_message_flags_t;

typedef enum {
    SEARCH_DEPTH_FIRST      = 0x01,
    SEARCH_BREADTH_FIRST    = 0x02,
    SEARCH_BEST_FIRST       = 0x03,
    SEARCH_RANDOM           = 0x04,
} mfn_search_mode_t;

typedef enum {
    ERR_NONE                = 0x0000,
    ERR_INVALID_MESSAGE     = 0x0001,
    ERR_UNSUPPORTED_OP      = 0x0002,
    ERR_MEMORY_NOT_FOUND    = 0x0003,
    ERR_TIMEOUT             = 0x0004,
    ERR_CAPACITY_EXCEEDED   = 0x0005,
    ERR_SERIALIZATION       = 0x0006,
    ERR_LAYER_UNAVAILABLE   = 0x0007,
    ERR_INVALID_PARAMETER   = 0x0008,
    ERR_PERMISSION_DENIED   = 0x0009,
    ERR_RESOURCE_EXHAUSTED  = 0x000A,
} mfn_error_code_t;

// ============================================================================
// Core Structures
// ============================================================================

// Message header (16 bytes, aligned)
typedef struct __attribute__((packed)) {
    uint32_t magic;                 // Always MFN_MAGIC
    uint16_t message_type;          // mfn_message_type_t
    uint16_t flags;                 // mfn_message_flags_t bitfield
    uint32_t payload_size;          // Size of payload in bytes
    uint32_t sequence_id;           // For request/response matching
} mfn_message_header_t;

// Command header (4 bytes, aligned)
typedef struct __attribute__((packed)) {
    uint8_t operation;              // mfn_operation_t
    uint8_t layer_id;               // mfn_layer_id_t
    uint8_t priority;               // 0-255 priority
    uint8_t reserved;               // Reserved for future use
} mfn_command_t;

// Complete message structure
typedef struct {
    mfn_message_header_t header;
    mfn_command_t command;
    void* payload;                  // Variable length payload
    uint32_t crc32;                 // Message integrity check
} mfn_message_t;

// Memory structure for binary protocol
typedef struct __attribute__((packed)) {
    uint64_t id;                    // Memory ID
    uint64_t timestamp_created;     // Microseconds since epoch
    uint64_t timestamp_accessed;    // Last access time
    uint64_t access_count;          // Access counter
    
    uint32_t content_size;          // Content length in bytes
    uint16_t tag_count;             // Number of tags
    uint16_t metadata_count;        // Number of metadata entries
    
    uint32_t embedding_dims;        // Embedding dimensions (0 if none)
    uint32_t reserved;              // Reserved for alignment
    
    // Variable length data follows:
    // - Content string (content_size bytes)
    // - Tags (tag_count null-terminated strings)
    // - Metadata (metadata_count key=value pairs)
    // - Embedding data (float32 * embedding_dims)
} mfn_binary_memory_t;

// Association structure for binary protocol
typedef struct __attribute__((packed)) {
    uint64_t from_memory_id;
    uint64_t to_memory_id;
    uint64_t timestamp_created;
    uint64_t timestamp_used;
    uint64_t usage_count;
    
    float weight;                   // 0.0 to 1.0
    uint8_t association_type;       // mfn_association_type_t
    uint8_t reserved[3];            // Alignment padding
    
    uint32_t reason_size;           // Reason string length
    // Reason string follows (reason_size bytes)
} mfn_binary_association_t;

// Search query structure
typedef struct __attribute__((packed)) {
    uint64_t sequence_id;           // Query identifier
    uint64_t timeout_us;            // Timeout in microseconds
    
    uint32_t max_results;           // Maximum results to return
    uint32_t max_depth;             // Maximum search depth
    float min_weight;               // Minimum association weight
    
    uint16_t start_memory_count;    // Number of starting memory IDs
    uint16_t tag_count;             // Number of tags
    uint16_t assoc_type_count;      // Number of association types
    uint8_t search_mode;            // mfn_search_mode_t
    uint8_t reserved;               // Alignment
    
    uint32_t content_size;          // Search content size
    uint32_t embedding_dims;        // Embedding dimensions
    
    // Variable data follows in this order:
    // - Starting memory IDs (uint64_t * start_memory_count)
    // - Tags (tag_count null-terminated strings)
    // - Association types (uint8_t * assoc_type_count)
    // - Content string (content_size bytes)
    // - Embedding data (float32 * embedding_dims)
} mfn_binary_search_query_t;

// Path step in search results
typedef struct __attribute__((packed)) {
    uint64_t from_memory_id;
    uint64_t to_memory_id;
    mfn_binary_association_t association;
    float step_weight;
    uint32_t reserved;              // Alignment
} mfn_binary_path_step_t;

// Search result structure
typedef struct __attribute__((packed)) {
    mfn_binary_memory_t memory;     // Found memory
    
    uint64_t search_time_us;        // Search time in microseconds
    float confidence;               // Result confidence 0.0-1.0
    uint8_t layer_origin;           // Originating layer
    uint8_t path_length;            // Number of hops
    uint16_t reserved;              // Alignment
    
    // Path follows: mfn_binary_path_step_t * path_length
} mfn_binary_search_result_t;

// Error structure
typedef struct __attribute__((packed)) {
    uint32_t error_code;            // mfn_error_code_t
    uint32_t original_sequence;     // Original request sequence
    uint32_t message_size;          // Error message length
    // Error message follows (UTF-8 string, message_size bytes)
} mfn_binary_error_t;

// Batch operation header
typedef struct __attribute__((packed)) {
    uint32_t operation_count;       // Number of operations
    uint32_t total_size;            // Total batch size
    uint64_t batch_id;              // Batch identifier
    // Operations follow: mfn_binary_operation_t * operation_count
} mfn_batch_header_t;

// Individual operation in batch
typedef struct __attribute__((packed)) {
    uint16_t operation_type;        // mfn_message_type_t
    uint16_t flags;                 // mfn_message_flags_t
    uint32_t payload_size;          // Payload size
    // Payload follows
} mfn_binary_operation_t;

// Shared memory reference for zero-copy operations
typedef struct __attribute__((packed)) {
    uint32_t region_id;             // Shared memory region ID
    uint32_t offset;                // Offset within region
    uint32_t size;                  // Data size
    uint32_t checksum;              // Data integrity check
} mfn_shared_memory_ref_t;

// Compressed payload header
typedef struct __attribute__((packed)) {
    uint32_t original_size;         // Uncompressed size
    uint32_t compressed_size;       // Compressed size
    uint8_t algorithm;              // Compression algorithm (0=LZ4)
    uint8_t reserved[3];            // Alignment
    // Compressed data follows
} mfn_compressed_payload_t;

// Performance metrics structure
typedef struct __attribute__((packed)) {
    uint64_t total_requests;
    uint64_t total_errors;
    uint64_t avg_latency_ns;
    uint64_t min_latency_ns;
    uint64_t max_latency_ns;
    uint64_t memory_usage_bytes;
    uint32_t active_connections;
    float cpu_usage_percent;
    float cache_hit_rate;
    uint32_t reserved;              // Alignment
} mfn_performance_metrics_t;

// Health status structure
typedef struct __attribute__((packed)) {
    uint8_t status;                 // 0=healthy, 1=degraded, 2=unhealthy
    uint8_t layer_id;               // Reporting layer
    uint16_t reserved;              // Alignment
    uint64_t uptime_seconds;
    uint32_t error_count;
    uint32_t warning_count;
    mfn_performance_metrics_t metrics;
} mfn_health_status_t;

// ============================================================================
// Function Declarations
// ============================================================================

// Message creation and serialization
mfn_error_code_t mfn_create_message(
    mfn_message_type_t type,
    mfn_operation_t operation, 
    mfn_layer_id_t layer,
    const void* payload,
    size_t payload_size,
    uint32_t sequence_id,
    mfn_message_t* message
);

mfn_error_code_t mfn_serialize_message(
    const mfn_message_t* message,
    uint8_t* buffer,
    size_t buffer_size,
    size_t* bytes_written
);

mfn_error_code_t mfn_deserialize_message(
    const uint8_t* buffer,
    size_t buffer_size,
    mfn_message_t* message
);

// Memory operations
mfn_error_code_t mfn_serialize_memory(
    uint64_t id,
    const char* content,
    const char* const* tags,
    size_t tag_count,
    const char* const* metadata_keys,
    const char* const* metadata_values,
    size_t metadata_count,
    const float* embedding,
    size_t embedding_dims,
    uint8_t* buffer,
    size_t buffer_size,
    size_t* bytes_written
);

mfn_error_code_t mfn_deserialize_memory(
    const uint8_t* buffer,
    size_t buffer_size,
    mfn_binary_memory_t** memory,
    char** content,
    char*** tags,
    size_t* tag_count,
    char*** metadata_keys,
    char*** metadata_values,
    size_t* metadata_count,
    float** embedding,
    size_t* embedding_dims
);

// Association operations
mfn_error_code_t mfn_serialize_association(
    uint64_t from_id,
    uint64_t to_id,
    mfn_association_type_t type,
    float weight,
    const char* reason,
    uint8_t* buffer,
    size_t buffer_size,
    size_t* bytes_written
);

mfn_error_code_t mfn_deserialize_association(
    const uint8_t* buffer,
    size_t buffer_size,
    mfn_binary_association_t** association,
    char** reason
);

// Search operations
mfn_error_code_t mfn_serialize_search_query(
    const uint64_t* start_memory_ids,
    size_t start_count,
    const char* content,
    const float* embedding,
    size_t embedding_dims,
    const char* const* tags,
    size_t tag_count,
    const mfn_association_type_t* assoc_types,
    size_t assoc_count,
    uint32_t max_results,
    uint32_t max_depth,
    float min_weight,
    uint64_t timeout_us,
    mfn_search_mode_t search_mode,
    uint8_t* buffer,
    size_t buffer_size,
    size_t* bytes_written
);

mfn_error_code_t mfn_deserialize_search_query(
    const uint8_t* buffer,
    size_t buffer_size,
    mfn_binary_search_query_t** query,
    uint64_t** start_memory_ids,
    char** content,
    float** embedding,
    char*** tags,
    mfn_association_type_t** assoc_types
);

// Utility functions
uint32_t mfn_calculate_crc32(const void* data, size_t size);
size_t mfn_calculate_message_size(const mfn_message_t* message);
bool mfn_validate_message(const mfn_message_t* message);

// Memory management helpers
void mfn_free_memory(mfn_binary_memory_t* memory);
void mfn_free_association(mfn_binary_association_t* association);
void mfn_free_search_query(mfn_binary_search_query_t* query);
void mfn_free_search_result(mfn_binary_search_result_t* result);

// Error handling
const char* mfn_error_string(mfn_error_code_t error);

// Performance profiling
void mfn_start_timer(void);
uint64_t mfn_end_timer_ns(void);

#ifdef __cplusplus
}
#endif

#endif // MFN_PROTOCOL_H