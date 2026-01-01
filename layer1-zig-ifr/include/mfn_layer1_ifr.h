/**
 * MFN Layer 1: Immediate Flow Registry (IFR) - C Header File
 * 
 * Ultra-fast exact matching with Unix socket interface and FFI
 * Performance Target: <0.1ms query latency (achieved: 0.013ms)
 * 
 * This header provides C-compatible bindings for the Zig implementation
 * of Layer 1 IFR, enabling integration with other programming languages.
 * 
 * Features:
 * - Direct IFR operations (add_memory, query)
 * - Unix socket server control
 * - Performance monitoring
 * - Cross-language compatibility
 * 
 * Socket Path: /tmp/mfn_discord_layer1.sock
 * Protocols: JSON (compatibility) + Binary (performance)
 */

#ifndef MFN_LAYER1_IFR_H
#define MFN_LAYER1_IFR_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

// ============================================================================
// Version Information
// ============================================================================

#define MFN_LAYER1_VERSION_MAJOR 1
#define MFN_LAYER1_VERSION_MINOR 0
#define MFN_LAYER1_VERSION_PATCH 0
#define MFN_LAYER1_VERSION_STRING "1.0.0"

// ============================================================================
// Constants
// ============================================================================

#define MFN_LAYER1_SOCKET_PATH "/tmp/mfn_discord_layer1.sock"
#define MFN_LAYER1_MAX_CONTENT_SIZE (1024 * 1024)  // 1MB max content size
#define MFN_LAYER1_MAX_MEMORY_DATA_SIZE (1024 * 1024)  // 1MB max memory data size

// Protocol constants
#define MFN_PROTOCOL_JSON   0x01
#define MFN_PROTOCOL_BINARY 0x02

// Message types for binary protocol
#define MFN_MSG_ADD_MEMORY    0x10
#define MFN_MSG_QUERY_MEMORY  0x20
#define MFN_MSG_GET_STATS     0x30
#define MFN_MSG_PING          0x40
#define MFN_MSG_RESPONSE      0x80
#define MFN_MSG_ERROR         0x90

// Return codes
#define MFN_SUCCESS            0
#define MFN_ERROR_INVALID_PARAM -1
#define MFN_ERROR_OUT_OF_MEMORY -2
#define MFN_ERROR_SOCKET_ERROR  -3
#define MFN_ERROR_TIMEOUT      -4
#define MFN_ERROR_NOT_FOUND    -5
#define MFN_ERROR_INTERNAL     -10

// ============================================================================
// Data Structures
// ============================================================================

/**
 * Opaque handle for IFR socket server
 */
typedef void* mfn_ifr_server_t;

/**
 * Memory ID returned by add_memory operations
 */
typedef uint64_t mfn_memory_id_t;

/**
 * Query result structure
 */
typedef struct {
    bool found_exact;                    // True if exact match found
    uint8_t next_layer;                 // Next layer to route to (if no exact match)
    float confidence;                   // Confidence score (1.0 for exact match)
    uint64_t processing_time_ns;        // Processing time in nanoseconds
    char* result;                       // Result data (NULL if not found)
    size_t result_length;               // Length of result data
} mfn_query_result_t;

/**
 * Performance statistics structure
 */
typedef struct {
    uint64_t total_queries;             // Total queries processed
    uint64_t exact_hits;                // Number of exact matches found
    float hit_rate;                     // Hit rate (exact_hits / total_queries)
    uint64_t memory_count;              // Number of memories stored
    uint64_t bloom_false_positives;     // Bloom filter false positives
    float false_positive_rate;          // False positive rate
} mfn_performance_stats_t;

/**
 * Server statistics structure
 */
typedef struct {
    uint64_t total_connections;         // Total connections received
    uint32_t active_connections;        // Currently active connections
    uint64_t total_requests;            // Total requests processed
    uint64_t total_responses;           // Total responses sent
    uint64_t total_errors;              // Total errors encountered
    mfn_performance_stats_t ifr_stats;  // IFR performance statistics
} mfn_server_stats_t;

/**
 * Server configuration structure
 */
typedef struct {
    const char* socket_path;            // Unix socket path
    uint32_t max_connections;           // Maximum concurrent connections
    uint64_t connection_timeout_ms;     // Connection timeout in milliseconds
    bool enable_binary_protocol;       // Enable binary protocol
    bool enable_json_protocol;          // Enable JSON protocol
    size_t buffer_size;                 // Buffer size for connections
    uint64_t bloom_capacity;            // Bloom filter capacity
    double bloom_error_rate;            // Bloom filter error rate
    uint64_t hash_initial_size;         // Hash table initial size
} mfn_server_config_t;

// ============================================================================
// Direct IFR Operations (In-Process)
// ============================================================================

/**
 * Create a new IFR instance for direct (in-process) operations
 * 
 * @param bloom_capacity Bloom filter capacity
 * @param bloom_error_rate Bloom filter target error rate
 * @param hash_initial_size Initial hash table size
 * @return Handle to IFR instance, or NULL on failure
 */
mfn_ifr_server_t mfn_ifr_create(uint64_t bloom_capacity, 
                                double bloom_error_rate,
                                uint64_t hash_initial_size);

/**
 * Destroy an IFR instance
 * 
 * @param handle IFR handle to destroy
 */
void mfn_ifr_destroy(mfn_ifr_server_t handle);

/**
 * Add memory to IFR (direct operation)
 * 
 * @param handle IFR handle
 * @param content Memory content (null-terminated string)
 * @param memory_data Associated memory data (null-terminated string)
 * @return Memory ID on success, 0 on failure
 */
mfn_memory_id_t mfn_ifr_add_memory(mfn_ifr_server_t handle, 
                                   const char* content,
                                   const char* memory_data);

/**
 * Query IFR for exact match (direct operation)
 * 
 * @param handle IFR handle
 * @param content Query content (null-terminated string)
 * @param result Pointer to result structure (caller must free result.result)
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_query(mfn_ifr_server_t handle, 
                  const char* content,
                  mfn_query_result_t* result);

/**
 * Get IFR performance statistics (direct operation)
 * 
 * @param handle IFR handle
 * @param stats Pointer to statistics structure
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_get_stats(mfn_ifr_server_t handle, 
                      mfn_performance_stats_t* stats);

/**
 * Free memory allocated by mfn_ifr_query result
 * 
 * @param result Result structure to free
 */
void mfn_ifr_free_result(mfn_query_result_t* result);

// ============================================================================
// Socket Server Operations
// ============================================================================

/**
 * Create socket server with default configuration
 * 
 * @return Handle to socket server, or NULL on failure
 */
mfn_ifr_server_t mfn_ifr_socket_server_create(void);

/**
 * Create socket server with custom configuration
 * 
 * @param config Server configuration
 * @return Handle to socket server, or NULL on failure
 */
mfn_ifr_server_t mfn_ifr_socket_server_create_with_config(const mfn_server_config_t* config);

/**
 * Start socket server (blocking call)
 * 
 * @param handle Socket server handle
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_socket_server_start(mfn_ifr_server_t handle);

/**
 * Start socket server in background thread (non-blocking)
 * 
 * @param handle Socket server handle
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_socket_server_start_async(mfn_ifr_server_t handle);

/**
 * Stop socket server
 * 
 * @param handle Socket server handle
 */
void mfn_ifr_socket_server_stop(mfn_ifr_server_t handle);

/**
 * Destroy socket server
 * 
 * @param handle Socket server handle to destroy
 */
void mfn_ifr_socket_server_destroy(mfn_ifr_server_t handle);

/**
 * Get server statistics
 * 
 * @param handle Socket server handle
 * @param stats Pointer to statistics structure
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_socket_server_get_stats(mfn_ifr_server_t handle,
                                    mfn_server_stats_t* stats);

/**
 * Add memory via socket server (direct access to underlying IFR)
 * 
 * @param handle Socket server handle
 * @param content Memory content (null-terminated string)
 * @param memory_data Associated memory data (null-terminated string)
 * @return Memory ID on success, 0 on failure
 */
mfn_memory_id_t mfn_ifr_socket_server_add_memory(mfn_ifr_server_t handle,
                                                  const char* content,
                                                  const char* memory_data);

/**
 * Query via socket server (direct access to underlying IFR)
 * 
 * @param handle Socket server handle
 * @param content Query content (null-terminated string)
 * @return 1 if found, 0 if not found, -1 on error
 */
int mfn_ifr_socket_server_query(mfn_ifr_server_t handle,
                                const char* content);

// ============================================================================
// Socket Client Operations (for testing and integration)
// ============================================================================

/**
 * Connect to Layer 1 IFR socket server
 * 
 * @param socket_path Path to Unix socket (NULL for default)
 * @return Socket file descriptor on success, -1 on failure
 */
int mfn_ifr_client_connect(const char* socket_path);

/**
 * Disconnect from socket server
 * 
 * @param socket_fd Socket file descriptor
 */
void mfn_ifr_client_disconnect(int socket_fd);

/**
 * Send JSON ping to server
 * 
 * @param socket_fd Socket file descriptor
 * @param request_id Request identifier string
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_client_ping_json(int socket_fd, const char* request_id);

/**
 * Send binary ping to server
 * 
 * @param socket_fd Socket file descriptor
 * @param request_id Request identifier (numeric)
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_client_ping_binary(int socket_fd, uint32_t request_id);

/**
 * Send JSON add memory request
 * 
 * @param socket_fd Socket file descriptor
 * @param request_id Request identifier string
 * @param content Memory content
 * @param memory_data Memory data
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_client_add_memory_json(int socket_fd, 
                                   const char* request_id,
                                   const char* content,
                                   const char* memory_data);

/**
 * Send binary add memory request
 * 
 * @param socket_fd Socket file descriptor
 * @param request_id Request identifier (numeric)
 * @param content Memory content
 * @param memory_data Memory data
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_client_add_memory_binary(int socket_fd,
                                     uint32_t request_id,
                                     const char* content,
                                     const char* memory_data);

/**
 * Send JSON query request
 * 
 * @param socket_fd Socket file descriptor
 * @param request_id Request identifier string
 * @param content Query content
 * @param result Pointer to result structure (caller must free result.result)
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_client_query_json(int socket_fd,
                              const char* request_id,
                              const char* content,
                              mfn_query_result_t* result);

/**
 * Send binary query request
 * 
 * @param socket_fd Socket file descriptor
 * @param request_id Request identifier (numeric)
 * @param content Query content
 * @param result Pointer to result structure (caller must free result.result)
 * @return MFN_SUCCESS on success, error code on failure
 */
int mfn_ifr_client_query_binary(int socket_fd,
                                uint32_t request_id,
                                const char* content,
                                mfn_query_result_t* result);

// ============================================================================
// Utility Functions
// ============================================================================

/**
 * Get Layer 1 IFR version string
 * 
 * @return Version string (e.g., "1.0.0")
 */
const char* mfn_ifr_get_version(void);

/**
 * Get default server configuration
 * 
 * @param config Pointer to configuration structure to fill
 */
void mfn_ifr_get_default_config(mfn_server_config_t* config);

/**
 * Initialize performance statistics structure
 * 
 * @param stats Pointer to statistics structure to initialize
 */
void mfn_ifr_init_stats(mfn_performance_stats_t* stats);

/**
 * Initialize server statistics structure
 * 
 * @param stats Pointer to server statistics structure to initialize
 */
void mfn_ifr_init_server_stats(mfn_server_stats_t* stats);

/**
 * Get error message for error code
 * 
 * @param error_code Error code returned by other functions
 * @return Human-readable error message
 */
const char* mfn_ifr_get_error_message(int error_code);

/**
 * Check if socket server is running
 * 
 * @param socket_path Path to Unix socket (NULL for default)
 * @return true if server is responding, false otherwise
 */
bool mfn_ifr_is_server_running(const char* socket_path);

/**
 * Get current timestamp in nanoseconds (for benchmarking)
 * 
 * @return Current timestamp in nanoseconds
 */
uint64_t mfn_ifr_get_timestamp_ns(void);

// ============================================================================
// Performance Benchmarking
// ============================================================================

/**
 * Run direct IFR performance benchmark
 * 
 * @param handle IFR handle
 * @param iterations Number of iterations to run
 * @param content_array Array of test content strings
 * @param content_count Number of strings in content_array
 * @return Average latency in nanoseconds, 0 on error
 */
uint64_t mfn_ifr_benchmark_direct(mfn_ifr_server_t handle,
                                  uint32_t iterations,
                                  const char** content_array,
                                  size_t content_count);

/**
 * Run socket client performance benchmark
 * 
 * @param socket_path Path to Unix socket (NULL for default)
 * @param iterations Number of iterations to run
 * @param use_binary_protocol Use binary protocol (true) or JSON (false)
 * @return Average latency in nanoseconds, 0 on error
 */
uint64_t mfn_ifr_benchmark_socket(const char* socket_path,
                                  uint32_t iterations,
                                  bool use_binary_protocol);

#ifdef __cplusplus
}
#endif

#endif // MFN_LAYER1_IFR_H