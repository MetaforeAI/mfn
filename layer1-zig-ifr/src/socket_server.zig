// Unix Socket Server for Layer 1 Immediate Flow Registry (IFR)
// Ultra-fast exact matching with Unix socket interface
// Target: Maintain 0.013ms performance with socket access
//
// Socket path: /tmp/mfn_layer1.sock
// Protocols: JSON (compatibility) and binary (performance)
// Zig Features: Async I/O, C-compatible interface, zero-GC overhead

const std = @import("std");
const net = std.net;
const print = std.debug.print;
const assert = std.debug.assert;
const Allocator = std.mem.Allocator;
const ArrayList = std.ArrayList;
const ifr = @import("ifr.zig");

// Import the core IFR types
const ImmediateFlowRegistry = ifr.ImmediateFlowRegistry;
const RoutingDecision = ifr.RoutingDecision;
const MemoryID = ifr.MemoryID;

// ============================================================================
// Configuration and Constants
// ============================================================================

pub const DEFAULT_SOCKET_PATH = "/tmp/mfn_layer1.sock";
pub const MAX_CONNECTIONS: u32 = 50; // Reduced to limit thread count
pub const CONNECTION_TIMEOUT_MS: u64 = 30000;
pub const BUFFER_SIZE: usize = 8192;
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB max message
pub const MAX_THREAD_POOL_SIZE: u32 = 20; // Thread pool size limit
pub const DEFAULT_MAX_MEMORY_ENTRIES: u64 = 1_000_000; // 1M entries max
pub const MEMORY_WARNING_THRESHOLD_MB: u64 = 1536; // Warn at 1.5GB
pub const MEMORY_CRITICAL_THRESHOLD_MB: u64 = 2048; // Critical at 2GB

// Protocol markers
pub const PROTOCOL_JSON: u8 = 0x01;
pub const PROTOCOL_BINARY: u8 = 0x02;

// Message types for binary protocol
pub const MSG_ADD_MEMORY: u8 = 0x10;
pub const MSG_QUERY_MEMORY: u8 = 0x20;
pub const MSG_GET_STATS: u8 = 0x30;
pub const MSG_PING: u8 = 0x40;
pub const MSG_RESPONSE: u8 = 0x80;
pub const MSG_ERROR: u8 = 0x90;

// ============================================================================
// Server Configuration
// ============================================================================

pub const SocketServerConfig = struct {
    socket_path: []const u8,
    max_connections: u32,
    connection_timeout_ms: u64,
    enable_binary_protocol: bool,
    enable_json_protocol: bool,
    buffer_size: usize,

    // IFR-specific configuration
    bloom_capacity: u64,
    bloom_error_rate: f64,
    hash_initial_size: u64,
    max_memory_entries: u64, // Maximum number of memories allowed

    pub fn default() SocketServerConfig {
        return SocketServerConfig{
            .socket_path = DEFAULT_SOCKET_PATH,
            .max_connections = MAX_CONNECTIONS,
            .connection_timeout_ms = CONNECTION_TIMEOUT_MS,
            .enable_binary_protocol = true,
            .enable_json_protocol = true,
            .buffer_size = BUFFER_SIZE,
            .bloom_capacity = 100000,
            .bloom_error_rate = 0.001,
            .hash_initial_size = 10000,
            .max_memory_entries = DEFAULT_MAX_MEMORY_ENTRIES,
        };
    }
};

// ============================================================================
// Protocol Structures
// ============================================================================

// JSON Request/Response structures
pub const JsonRequest = struct {
    type: []const u8,
    request_id: []const u8,
    
    // AddMemory fields
    content: ?[]const u8 = null,
    memory_data: ?[]const u8 = null,
    
    // Query fields
    query_content: ?[]const u8 = null,
    
    // Ping (no additional fields needed)
};

pub const JsonResponse = struct {
    type: []const u8,
    request_id: []const u8,
    success: bool,
    
    // Response data
    found_exact: ?bool = null,
    result: ?[]const u8 = null,
    next_layer: ?u8 = null,
    confidence: ?f32 = null,
    processing_time_ns: ?u64 = null,
    memory_id_hash: ?u64 = null,
    
    // Statistics
    total_queries: ?u64 = null,
    exact_hits: ?u64 = null,
    hit_rate: ?f32 = null,
    memory_count: ?u64 = null,
    
    // Error information
    error_message: ?[]const u8 = null,
};

// Binary protocol header (16 bytes)
pub const BinaryHeader = packed struct {
    protocol_version: u8,   // Always PROTOCOL_BINARY
    message_type: u8,       // MSG_* constants
    request_id: u32,        // Unique request identifier
    payload_length: u32,    // Length of payload following header
    reserved: u64,          // Reserved for future use
};

// ============================================================================
// Connection Management
// ============================================================================

pub const Connection = struct {
    stream: net.Stream,
    buffer: [BUFFER_SIZE]u8,
    read_index: usize,
    write_index: usize,
    last_activity: i64,
    connection_id: u32,
    memory_keys: ArrayList([]const u8), // Track memory keys added by this connection
    allocator: Allocator,

    const Self = @This();

    pub fn init(allocator: Allocator, stream: net.Stream, connection_id: u32) Self {
        return Self{
            .stream = stream,
            .buffer = [_]u8{0} ** BUFFER_SIZE,
            .read_index = 0,
            .write_index = 0,
            .last_activity = std.time.timestamp(),
            .connection_id = connection_id,
            .memory_keys = ArrayList([]const u8){},
            .allocator = allocator,
        };
    }

    pub fn deinit(self: *Self) void {
        // Clean up tracked memory keys
        self.memory_keys.deinit(self.allocator);
    }
    
    pub fn updateActivity(self: *Self) void {
        self.last_activity = std.time.timestamp();
    }
    
    pub fn isTimedOut(self: *Self, timeout_ms: u64) bool {
        const current_time = std.time.timestamp();
        const timeout_seconds = @as(i64, @intCast(timeout_ms / 1000));
        return (current_time - self.last_activity) > timeout_seconds;
    }
};

// ============================================================================
// Main Socket Server
// ============================================================================

pub const SocketServer = struct {
    config: SocketServerConfig,
    ifr_registry: ImmediateFlowRegistry,
    allocator: Allocator,
    running: bool,
    connection_counter: u32,

    // Thread pool management
    thread_pool: ArrayList(std.Thread),
    available_threads: ArrayList(bool),
    thread_mutex: std.Thread.Mutex,

    // Connection memory tracking
    connection_memories: std.AutoHashMap(u32, ArrayList([]const u8)),
    memory_mutex: std.Thread.Mutex,

    // Memory monitoring
    last_memory_check: i64,
    memory_warnings_issued: u32,

    // Statistics
    total_connections: u64,
    active_connections: u32,
    total_requests: u64,
    total_responses: u64,
    total_errors: u64,
    total_evictions: u64,

    const Self = @This();
    
    pub fn init(allocator: Allocator, config: SocketServerConfig) !Self {
        const ifr_registry = try ImmediateFlowRegistry.initWithLimit(
            allocator,
            config.bloom_capacity,
            config.bloom_error_rate,
            config.hash_initial_size,
            config.max_memory_entries
        );

        return Self{
            .config = config,
            .ifr_registry = ifr_registry,
            .allocator = allocator,
            .running = false,
            .connection_counter = 0,
            .thread_pool = ArrayList(std.Thread){},
            .available_threads = ArrayList(bool){},
            .thread_mutex = std.Thread.Mutex{},
            .connection_memories = std.AutoHashMap(u32, ArrayList([]const u8)).init(allocator),
            .memory_mutex = std.Thread.Mutex{},
            .last_memory_check = std.time.timestamp(),
            .memory_warnings_issued = 0,
            .total_connections = 0,
            .active_connections = 0,
            .total_requests = 0,
            .total_responses = 0,
            .total_errors = 0,
            .total_evictions = 0,
        };
    }
    
    pub fn deinit(self: *Self) void {
        self.ifr_registry.deinit();

        // Clean up thread pool
        self.thread_pool.deinit(self.allocator);
        self.available_threads.deinit(self.allocator);

        // Clean up connection memories
        var iter = self.connection_memories.iterator();
        while (iter.next()) |entry| {
            entry.value_ptr.deinit(self.allocator);
        }
        self.connection_memories.deinit();

        // Clean up socket file if it exists
        std.fs.cwd().deleteFile(self.config.socket_path) catch |err| {
            if (err != error.FileNotFound) {
                print("Warning: Could not remove socket file: {}\n", .{err});
            }
        };
    }
    
    pub fn start(self: *Self) !void {
        print("🚀 Starting Layer 1 IFR Unix Socket Server\n", .{});
        print("Socket path: {s}\n", .{self.config.socket_path});
        print("Max connections: {}\n", .{self.config.max_connections});
        print("Binary protocol: {}\n", .{self.config.enable_binary_protocol});
        print("JSON protocol: {}\n", .{self.config.enable_json_protocol});
        
        // Remove existing socket file
        std.fs.cwd().deleteFile(self.config.socket_path) catch |err| {
            if (err != error.FileNotFound) {
                print("Warning: Could not remove existing socket file: {}\n", .{err});
            }
        };
        
        // Create Unix socket listener with increased backlog for stress testing
        const addr = try net.Address.initUnix(self.config.socket_path);
        var listener = try addr.listen(.{
            .reuse_address = true,
            .kernel_backlog = 512, // Increased from default 128 for high-concurrency stress tests
        });
        
        self.running = true;
        print("✅ Layer 1 IFR server listening on {s}\n", .{self.config.socket_path});
        
        // Main server loop
        while (self.running) {
            // Accept connection with timeout
            const connection = listener.accept() catch |err| {
                if (err == error.WouldBlock) {
                    // No connection available, continue
                    std.Thread.sleep(1000000); // 1ms sleep
                    continue;
                }
                print("Error accepting connection: {}\n", .{err});
                continue;
            };

            // Check connection limit
            if (self.active_connections >= self.config.max_connections) {
                print("⚠️  Connection limit reached ({}/{}), closing new connection\n",
                    .{self.active_connections, self.config.max_connections});
                connection.stream.close();
                continue;
            }

            self.connection_counter += 1;
            self.total_connections += 1;
            self.active_connections += 1;

            // Use thread pool instead of spawning new threads
            const thread_acquired = try self.acquireThread();
            if (!thread_acquired) {
                print("⚠️  Thread pool exhausted, handling in main thread\n", .{});
                // Handle synchronously if no threads available
                self.handleConnection(connection.stream, self.connection_counter) catch |err| {
                    print("Error handling connection {}: {}\n", .{ self.connection_counter, err });
                };
                self.active_connections -= 1;
            } else {
                // Spawn thread from pool to handle connection
                const thread_result = std.Thread.spawn(.{}, handleConnectionThread, .{
                    self,
                    connection.stream,
                    self.connection_counter,
                }) catch |err| {
                    print("Failed to spawn connection thread {}: {}\n", .{ self.connection_counter, err });
                    connection.stream.close();
                    self.active_connections -= 1;
                    self.releaseThread();
                    continue;
                };

                // Track the thread
                try self.thread_pool.append(self.allocator, thread_result);
            }
        }
    }
    
    pub fn stop(self: *Self) void {
        self.running = false;
        print("🛑 Layer 1 IFR server stopping...\n", .{});
    }

    /// Acquire a thread from the pool
    fn acquireThread(self: *Self) !bool {
        self.thread_mutex.lock();
        defer self.thread_mutex.unlock();

        // Check if we're under the thread pool limit
        if (self.thread_pool.items.len >= MAX_THREAD_POOL_SIZE) {
            return false;
        }

        return true;
    }

    /// Release a thread back to the pool
    fn releaseThread(self: *Self) void {
        self.thread_mutex.lock();
        defer self.thread_mutex.unlock();
        // Thread tracking handled by thread_pool list
    }

    /// Thread wrapper for concurrent connection handling
    fn handleConnectionThread(self: *Self, stream: net.Stream, connection_id: u32) void {
        defer {
            self.active_connections -= 1;
            self.releaseThread();
            // Clean up connection memories when connection closes
            self.cleanupConnectionMemories(connection_id);
        }

        self.handleConnection(stream, connection_id) catch |err| {
            print("Error handling connection {}: {}\n", .{ connection_id, err });
        };
    }

    /// Clean up memories associated with a connection
    fn cleanupConnectionMemories(self: *Self, connection_id: u32) void {
        self.memory_mutex.lock();
        defer self.memory_mutex.unlock();

        if (self.connection_memories.get(connection_id)) |memory_list| {
            // Remove all memories added by this connection from the IFR
            for (memory_list.items) |key| {
                self.ifr_registry.removeMemory(key) catch {};
                self.allocator.free(key);
            }
        }

        // Remove the connection from tracking
        _ = self.connection_memories.remove(connection_id);
    }

    fn handleConnection(self: *Self, stream: net.Stream, connection_id: u32) !void {
        defer stream.close();

        var conn = Connection.init(self.allocator, stream, connection_id);
        defer conn.deinit();
        print("🔗 Connection {} established\n", .{connection_id});

        // Initialize connection memory tracking
        self.memory_mutex.lock();
        try self.connection_memories.put(connection_id, ArrayList([]const u8){});
        self.memory_mutex.unlock();
        
        var message_buffer = ArrayList(u8){};
        defer message_buffer.deinit(self.allocator);
        
        // Connection handling loop
        while (true) {
            // Check for timeout
            if (conn.isTimedOut(self.config.connection_timeout_ms)) {
                print("⏰ Connection {} timed out\n", .{connection_id});
                break;
            }

            // Check if buffer is full
            if (conn.write_index >= BUFFER_SIZE) {
                print("❌ Connection {} buffer overflow: write_index={}, BUFFER_SIZE={}\n",
                    .{ connection_id, conn.write_index, BUFFER_SIZE });
                print("   This indicates a message larger than buffer size or a client not consuming data\n", .{});
                break;
            }

            // Try to read data
            const bytes_read = stream.read(conn.buffer[conn.write_index..]) catch |err| {
                if (err == error.WouldBlock) {
                    std.Thread.sleep(1000000); // 1ms sleep
                    continue;
                } else if (err == error.EndOfStream) {
                    print("🔌 Connection {} closed by client\n", .{connection_id});
                    break;
                } else {
                    print("❌ Error reading from connection {}: {}\n", .{ connection_id, err });
                    break;
                }
            };

            if (bytes_read == 0) {
                print("🔌 Connection {} closed (0 bytes read)\n", .{connection_id});
                break;
            }

            conn.write_index += bytes_read;
            conn.updateActivity();

            // Try to process complete messages
            while (try self.processMessage(&conn, &message_buffer)) {
                // Continue processing messages
            }
        }
        
        print("🚪 Connection {} closed\n", .{connection_id});
    }
    
    fn processMessage(self: *Self, conn: *Connection, message_buffer: *ArrayList(u8)) !bool {
        // Check if we have enough data for protocol detection
        if (conn.write_index < 1) {
            return false;
        }
        
        const protocol_byte = conn.buffer[conn.read_index];
        
        if (protocol_byte == PROTOCOL_BINARY) {
            return try self.processBinaryMessage(conn, message_buffer);
        } else if (protocol_byte == '{' or protocol_byte == ' ' or protocol_byte == '\n') {
            // Looks like JSON (starting with '{' or whitespace)
            return try self.processJsonMessage(conn, message_buffer);
        } else {
            // Unknown protocol
            try self.sendError(conn, "Unknown protocol");
            return false;
        }
    }
    
    fn processBinaryMessage(self: *Self, conn: *Connection, message_buffer: *ArrayList(u8)) !bool {
        _ = message_buffer;
        const available_data = conn.write_index - conn.read_index;
        
        // Need at least header size
        if (available_data < @sizeOf(BinaryHeader)) {
            return false;
        }
        
        // Parse header
        const header_bytes = conn.buffer[conn.read_index..conn.read_index + @sizeOf(BinaryHeader)];
        const header = std.mem.bytesToValue(BinaryHeader, header_bytes[0..@sizeOf(BinaryHeader)]);
        
        // Validate protocol version
        if (header.protocol_version != PROTOCOL_BINARY) {
            try self.sendError(conn, "Invalid binary protocol version");
            return false;
        }
        
        // Check if we have complete message
        const total_message_size = @sizeOf(BinaryHeader) + header.payload_length;
        if (available_data < total_message_size) {
            return false; // Need more data
        }
        
        // Extract payload
        const payload_start = conn.read_index + @sizeOf(BinaryHeader);
        const payload = conn.buffer[payload_start..payload_start + header.payload_length];
        
        // Process the binary message
        try self.handleBinaryMessage(conn, header, payload);
        
        // Update read index
        conn.read_index += total_message_size;
        
        // Compact buffer if needed
        if (conn.read_index > BUFFER_SIZE / 2) {
            const remaining = conn.write_index - conn.read_index;
            @memcpy(conn.buffer[0..remaining], conn.buffer[conn.read_index..conn.write_index]);
            conn.read_index = 0;
            conn.write_index = remaining;
        }
        
        return true;
    }
    
    fn processJsonMessage(self: *Self, conn: *Connection, message_buffer: *ArrayList(u8)) !bool {
        _ = message_buffer;
        // Look for complete JSON message (ending with \n)
        const available_data = conn.write_index - conn.read_index;
        const buffer_slice = conn.buffer[conn.read_index..conn.read_index + available_data];
        
        // Find newline
        const newline_pos = std.mem.indexOf(u8, buffer_slice, "\n");
        if (newline_pos == null) {
            return false; // No complete message yet
        }
        
        const message_end = newline_pos.?;
        const json_data = buffer_slice[0..message_end];
        
        // Process JSON message
        try self.handleJsonMessage(conn, json_data);

        // Update read index (skip the newline)
        conn.read_index += message_end + 1;

        // Compact buffer if needed (same as binary message processing)
        if (conn.read_index > BUFFER_SIZE / 2) {
            const remaining = conn.write_index - conn.read_index;
            @memcpy(conn.buffer[0..remaining], conn.buffer[conn.read_index..conn.write_index]);
            conn.read_index = 0;
            conn.write_index = remaining;
        }

        return true;
    }
    
    fn handleBinaryMessage(self: *Self, conn: *Connection, header: BinaryHeader, payload: []const u8) !void {
        self.total_requests += 1;
        
        switch (header.message_type) {
            MSG_ADD_MEMORY => try self.handleBinaryAddMemory(conn, header, payload),
            MSG_QUERY_MEMORY => try self.handleBinaryQuery(conn, header, payload),
            MSG_GET_STATS => try self.handleBinaryGetStats(conn, header),
            MSG_PING => try self.handleBinaryPing(conn, header),
            else => {
                try self.sendBinaryError(conn, header.request_id, "Unknown message type");
                self.total_errors += 1;
            },
        }
    }
    
    fn handleJsonMessage(self: *Self, conn: *Connection, json_data: []const u8) !void {
        self.total_requests += 1;
        
        // Parse JSON (simplified - would use a proper JSON parser in production)
        if (std.mem.indexOf(u8, json_data, "\"type\":\"add_memory\"") != null) {
            try self.handleJsonAddMemory(conn, json_data);
        } else if (std.mem.indexOf(u8, json_data, "\"type\":\"query\"") != null) {
            try self.handleJsonQuery(conn, json_data);
        } else if (std.mem.indexOf(u8, json_data, "\"type\":\"get_stats\"") != null) {
            try self.handleJsonGetStats(conn, json_data);
        } else if (std.mem.indexOf(u8, json_data, "\"type\":\"ping\"") != null) {
            try self.handleJsonPing(conn, json_data);
        } else {
            try self.sendJsonError(conn, "unknown", "Unknown request type");
            self.total_errors += 1;
        }
    }
    
    // Binary message handlers
    fn handleBinaryAddMemory(self: *Self, conn: *Connection, header: BinaryHeader, payload: []const u8) !void {
        // Check memory usage before adding
        try self.checkMemoryUsage();

        // Binary payload format: content_len(4) + content + memory_data_len(4) + memory_data
        if (payload.len < 8) {
            try self.sendBinaryError(conn, header.request_id, "Invalid add_memory payload");
            return;
        }

        const content_len = std.mem.readInt(u32, payload[0..4], .little);
        if (payload.len < 8 + content_len) {
            try self.sendBinaryError(conn, header.request_id, "Invalid add_memory payload length");
            return;
        }

        const content = payload[4..4 + content_len];
        const memory_data_len = std.mem.readInt(u32, payload[4 + content_len..8 + content_len][0..4], .little);

        if (payload.len != 8 + content_len + memory_data_len) {
            try self.sendBinaryError(conn, header.request_id, "Invalid add_memory payload structure");
            return;
        }

        const memory_data = payload[8 + content_len..8 + content_len + memory_data_len];

        // Add memory to IFR with LRU eviction if needed
        const result = self.ifr_registry.addMemoryWithEviction(content, memory_data) catch {
            try self.sendBinaryError(conn, header.request_id, "Failed to add memory");
            return;
        };

        // Track evictions
        if (result.evicted) {
            self.total_evictions += 1;
        }

        // Track which connection added this memory
        const content_copy = try self.allocator.dupe(u8, content);
        self.memory_mutex.lock();
        if (self.connection_memories.getPtr(conn.connection_id)) |list| {
            try list.append(self.allocator, content_copy);
        }
        self.memory_mutex.unlock();

        // Send success response
        try self.sendBinaryAddMemoryResponse(conn, header.request_id, result.memory_id);
    }
    
    fn handleBinaryQuery(self: *Self, conn: *Connection, header: BinaryHeader, payload: []const u8) !void {
        // Binary payload format: query_content
        const query_content = payload;
        
        // Query IFR
        const result = self.ifr_registry.query(query_content);
        
        // Send response
        try self.sendBinaryQueryResponse(conn, header.request_id, result);
    }
    
    fn handleBinaryGetStats(self: *Self, conn: *Connection, header: BinaryHeader) !void {
        const stats = self.ifr_registry.getPerformanceStats();
        try self.sendBinaryStatsResponse(conn, header.request_id, stats);
    }
    
    fn handleBinaryPing(self: *Self, conn: *Connection, header: BinaryHeader) !void {
        try self.sendBinaryPingResponse(conn, header.request_id);
    }
    
    // JSON message handlers (with basic JSON parsing)
    fn handleJsonAddMemory(self: *Self, conn: *Connection, json_data: []const u8) !void {
        // Check memory usage before adding
        try self.checkMemoryUsage();

        // Extract request_id from JSON
        var request_id: []const u8 = "unknown";
        if (std.mem.indexOf(u8, json_data, "\"request_id\":\"")) |id_pos| {
            const id_start = id_pos + 14; // Length of "\"request_id\":\""
            if (std.mem.indexOf(u8, json_data[id_start..], "\"")) |id_end| {
                request_id = json_data[id_start..id_start + id_end];
            }
        }

        // Extract content from JSON
        var content: []const u8 = "default_content";
        if (std.mem.indexOf(u8, json_data, "\"content\":\"")) |content_pos| {
            const content_start = content_pos + 11; // Length of "\"content\":\""
            if (std.mem.indexOf(u8, json_data[content_start..], "\"")) |content_end| {
                content = json_data[content_start..content_start + content_end];
            }
        }

        // Create memory data from content (Layer 1 doesn't need complex data)
        const memory_data = content; // Use content as memory data for simplicity

        const result = self.ifr_registry.addMemoryWithEviction(content, memory_data) catch {
            try self.sendJsonError(conn, request_id, "Failed to add memory");
            return;
        };

        // Track evictions
        if (result.evicted) {
            self.total_evictions += 1;
        }

        // Track which connection added this memory
        const content_copy = try self.allocator.dupe(u8, content);
        self.memory_mutex.lock();
        if (self.connection_memories.getPtr(conn.connection_id)) |list| {
            try list.append(self.allocator, content_copy);
        }
        self.memory_mutex.unlock();

        // Send proper JSON response with memory_id_hash
        const json_str = try std.fmt.allocPrint(self.allocator,
            "{{\"type\":\"add_memory_response\",\"request_id\":\"{s}\",\"success\":true,\"memory_id_hash\":{},\"evicted\":{}}}\n",
            .{ request_id, result.memory_id.hash, result.evicted }
        );
        defer self.allocator.free(json_str);

        _ = try conn.stream.writeAll(json_str);
        self.total_responses += 1;
    }
    
    fn handleJsonQuery(self: *Self, conn: *Connection, json_data: []const u8) !void {
        // Extract request_id from JSON
        var request_id: []const u8 = "unknown";
        if (std.mem.indexOf(u8, json_data, "\"request_id\":\"")) |id_pos| {
            const id_start = id_pos + 14;
            if (std.mem.indexOf(u8, json_data[id_start..], "\"")) |id_end| {
                request_id = json_data[id_start..id_start + id_end];
            }
        }
        
        // Extract content from JSON
        var query_content: []const u8 = "default_query";
        if (std.mem.indexOf(u8, json_data, "\"content\":\"")) |content_pos| {
            const content_start = content_pos + 11;
            if (std.mem.indexOf(u8, json_data[content_start..], "\"")) |content_end| {
                query_content = json_data[content_start..content_start + content_end];
            }
        }
        
        // Query IFR registry
        const result = self.ifr_registry.query(query_content);
        
        // Send detailed JSON response
        const next_layer_str = if (result.next_layer) |layer| switch (layer) {
            1 => "1",
            2 => "2", 
            3 => "3",
            4 => "4",
            else => "null"
        } else "null";
        
        const result_str = result.result orelse "null";
        
        const json_str = try std.fmt.allocPrint(self.allocator,
            "{{\"type\":\"query_response\",\"request_id\":\"{s}\",\"success\":true,\"found_exact\":{},\"next_layer\":{s},\"confidence\":{d:.3},\"processing_time_ns\":{},\"result\":\"{s}\"}}\n",
            .{ request_id, result.found_exact, next_layer_str, result.confidence, result.processing_time_ns, result_str }
        );
        defer self.allocator.free(json_str);
        
        _ = try conn.stream.writeAll(json_str);
        self.total_responses += 1;
    }
    
    fn handleJsonGetStats(self: *Self, conn: *Connection, json_data: []const u8) !void {
        // Extract request_id from JSON
        var request_id: []const u8 = "unknown";
        if (std.mem.indexOf(u8, json_data, "\"request_id\":\"")) |id_pos| {
            const id_start = id_pos + 14;
            if (std.mem.indexOf(u8, json_data[id_start..], "\"")) |id_end| {
                request_id = json_data[id_start..id_start + id_end];
            }
        }
        
        const stats = self.ifr_registry.getPerformanceStats();
        
        const json_str = try std.fmt.allocPrint(self.allocator,
            "{{\"type\":\"stats_response\",\"request_id\":\"{s}\",\"success\":true,\"total_queries\":{},\"exact_hits\":{},\"hit_rate\":{d:.3},\"memory_count\":{}}}\n",
            .{ request_id, stats.total_queries, stats.exact_hits, stats.hit_rate, stats.memory_count }
        );
        defer self.allocator.free(json_str);
        
        _ = try conn.stream.writeAll(json_str);
        self.total_responses += 1;
    }
    
    fn handleJsonPing(self: *Self, conn: *Connection, json_data: []const u8) !void {
        // Extract request_id from JSON
        var request_id: []const u8 = "unknown";
        if (std.mem.indexOf(u8, json_data, "\"request_id\":\"")) |id_pos| {
            const id_start = id_pos + 14;
            if (std.mem.indexOf(u8, json_data[id_start..], "\"")) |id_end| {
                request_id = json_data[id_start..id_start + id_end];
            }
        }
        
        const json_str = try std.fmt.allocPrint(self.allocator,
            "{{\"type\":\"pong\",\"request_id\":\"{s}\",\"success\":true}}\n",
            .{ request_id }
        );
        defer self.allocator.free(json_str);
        
        _ = try conn.stream.writeAll(json_str);
        self.total_responses += 1;
    }
    
    // Response senders
    fn sendBinaryAddMemoryResponse(self: *Self, conn: *Connection, request_id: u32, memory_id: MemoryID) !void {
        const payload = std.mem.asBytes(&memory_id.hash);
        try self.sendBinaryResponse(conn, request_id, payload);
        self.total_responses += 1;
    }
    
    fn sendBinaryQueryResponse(self: *Self, conn: *Connection, request_id: u32, result: RoutingDecision) !void {
        // Binary response format: found_exact(1) + next_layer(1) + confidence(4) + processing_time(8) + result_len(4) + result
        var response_buffer = ArrayList(u8){};
        defer response_buffer.deinit(self.allocator);
        
        try response_buffer.append(self.allocator, if (result.found_exact) 1 else 0);
        try response_buffer.append(self.allocator, result.next_layer orelse 0xFF);

        const confidence_bytes = std.mem.asBytes(&result.confidence);
        try response_buffer.appendSlice(self.allocator, confidence_bytes);

        const time_bytes = std.mem.asBytes(&result.processing_time_ns);
        try response_buffer.appendSlice(self.allocator, time_bytes);

        if (result.result) |result_data| {
            const result_len: u32 = @intCast(result_data.len);
            const len_bytes = std.mem.asBytes(&result_len);
            try response_buffer.appendSlice(self.allocator, len_bytes);
            try response_buffer.appendSlice(self.allocator, result_data);
        } else {
            const zero_len: u32 = 0;
            const len_bytes = std.mem.asBytes(&zero_len);
            try response_buffer.appendSlice(self.allocator, len_bytes);
        }
        
        try self.sendBinaryResponse(conn, request_id, response_buffer.items);
        self.total_responses += 1;
    }
    
    fn sendBinaryStatsResponse(self: *Self, conn: *Connection, request_id: u32, stats: anytype) !void {
        var response_buffer = ArrayList(u8){};
        defer response_buffer.deinit(self.allocator);
        
        const total_queries_bytes = std.mem.asBytes(&stats.total_queries);
        try response_buffer.appendSlice(self.allocator, total_queries_bytes);

        const exact_hits_bytes = std.mem.asBytes(&stats.exact_hits);
        try response_buffer.appendSlice(self.allocator, exact_hits_bytes);

        const hit_rate_bytes = std.mem.asBytes(&stats.hit_rate);
        try response_buffer.appendSlice(self.allocator, hit_rate_bytes);

        const memory_count_bytes = std.mem.asBytes(&stats.memory_count);
        try response_buffer.appendSlice(self.allocator, memory_count_bytes);
        
        try self.sendBinaryResponse(conn, request_id, response_buffer.items);
        self.total_responses += 1;
    }
    
    fn sendBinaryPingResponse(self: *Self, conn: *Connection, request_id: u32) !void {
        const payload = "pong";
        try self.sendBinaryResponse(conn, request_id, payload);
        self.total_responses += 1;
    }
    
    fn sendBinaryResponse(self: *Self, conn: *Connection, request_id: u32, payload: []const u8) !void {
        _ = self;
        const header = BinaryHeader{
            .protocol_version = PROTOCOL_BINARY,
            .message_type = MSG_RESPONSE,
            .request_id = request_id,
            .payload_length = @intCast(payload.len),
            .reserved = 0,
        };
        
        const header_bytes = std.mem.asBytes(&header);
        _ = try conn.stream.writeAll(header_bytes);
        _ = try conn.stream.writeAll(payload);
    }
    
    fn sendBinaryError(self: *Self, conn: *Connection, request_id: u32, error_message: []const u8) !void {
        const header = BinaryHeader{
            .protocol_version = PROTOCOL_BINARY,
            .message_type = MSG_ERROR,
            .request_id = request_id,
            .payload_length = @intCast(error_message.len),
            .reserved = 0,
        };
        
        const header_bytes = std.mem.asBytes(&header);
        _ = try conn.stream.writeAll(header_bytes);
        _ = try conn.stream.writeAll(error_message);
        
        self.total_errors += 1;
    }
    
    fn sendJsonResponse(self: *Self, conn: *Connection, response: JsonResponse) !void {
        // Simplified JSON serialization - in production would use proper JSON library
        const json_str = try std.fmt.allocPrint(self.allocator, 
            "{{\"type\":\"{s}\",\"request_id\":\"{s}\",\"success\":{}}}\n",
            .{ response.type, response.request_id, response.success }
        );
        defer self.allocator.free(json_str);
        
        _ = try conn.stream.writeAll(json_str);
        self.total_responses += 1;
    }
    
    fn sendJsonError(self: *Self, conn: *Connection, request_id: []const u8, error_message: []const u8) !void {
        const json_str = try std.fmt.allocPrint(self.allocator,
            "{{\"type\":\"error\",\"request_id\":\"{s}\",\"success\":false,\"error\":\"{s}\"}}\n",
            .{ request_id, error_message }
        );
        defer self.allocator.free(json_str);
        
        _ = try conn.stream.writeAll(json_str);
        self.total_errors += 1;
    }
    
    fn sendError(self: *Self, conn: *Connection, error_message: []const u8) !void {
        try self.sendJsonError(conn, "unknown", error_message);
    }
    
    // Memory monitoring
    fn checkMemoryUsage(self: *Self) !void {
        const current_time = std.time.timestamp();

        // Only check every 10 seconds to avoid overhead
        if (current_time - self.last_memory_check < 10) {
            return;
        }
        self.last_memory_check = current_time;

        // Get current process memory usage
        const memory_mb = try getProcessMemoryMB();

        if (memory_mb > MEMORY_CRITICAL_THRESHOLD_MB) {
            print("❌ CRITICAL: Memory usage {}MB exceeds critical threshold {}MB\n",
                .{ memory_mb, MEMORY_CRITICAL_THRESHOLD_MB });
            print("    Hash table entries: {}, Evictions: {}\n",
                .{ self.ifr_registry.getMemoryCount(), self.total_evictions });
        } else if (memory_mb > MEMORY_WARNING_THRESHOLD_MB) {
            if (self.memory_warnings_issued < 5) { // Limit warning spam
                print("⚠️  WARNING: Memory usage {}MB exceeds warning threshold {}MB\n",
                    .{ memory_mb, MEMORY_WARNING_THRESHOLD_MB });
                self.memory_warnings_issued += 1;
            }
        }
    }

    // Get process memory usage in MB
    fn getProcessMemoryMB() !u64 {
        // Read /proc/self/status for memory info on Linux
        const file = try std.fs.openFileAbsolute("/proc/self/status", .{});
        defer file.close();

        var buf: [4096]u8 = undefined;
        const bytes_read = try file.read(&buf);
        const content = buf[0..bytes_read];

        // Look for VmRSS: line (Resident Set Size)
        if (std.mem.indexOf(u8, content, "VmRSS:")) |rss_pos| {
            const line_start = rss_pos + 6; // Skip "VmRSS:"
            if (std.mem.indexOf(u8, content[line_start..], "\n")) |line_end| {
                const rss_str = std.mem.trim(u8, content[line_start..line_start + line_end], " \t");
                // Parse the number (it's in kB)
                var iter = std.mem.tokenizeAny(u8, rss_str, " \t");
                if (iter.next()) |num_str| {
                    const kb = try std.fmt.parseInt(u64, num_str, 10);
                    return kb / 1024; // Convert to MB
                }
            }
        }

        return 0; // Default if can't read
    }

    // Public statistics
    pub fn getServerStats(self: *Self) struct {
        total_connections: u64,
        active_connections: u32,
        total_requests: u64,
        total_responses: u64,
        total_errors: u64,
        total_evictions: u64,
        thread_pool_size: usize,
        ifr_stats: @TypeOf(self.ifr_registry.getPerformanceStats()),
    } {
        return .{
            .total_connections = self.total_connections,
            .active_connections = self.active_connections,
            .total_requests = self.total_requests,
            .total_responses = self.total_responses,
            .total_errors = self.total_errors,
            .total_evictions = self.total_evictions,
            .thread_pool_size = self.thread_pool.items.len,
            .ifr_stats = self.ifr_registry.getPerformanceStats(),
        };
    }
};

// ============================================================================
// C-Compatible API for FFI
// ============================================================================

// Opaque handle for C interface
const SocketServerHandle = *SocketServer;

export fn ifr_socket_server_create() ?SocketServerHandle {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    const allocator = gpa.allocator();
    
    const config = SocketServerConfig.default();
    const server = allocator.create(SocketServer) catch return null;
    server.* = SocketServer.init(allocator, config) catch {
        allocator.destroy(server);
        return null;
    };
    
    return server;
}

export fn ifr_socket_server_start(handle: SocketServerHandle) c_int {
    handle.start() catch return -1;
    return 0;
}

export fn ifr_socket_server_stop(handle: SocketServerHandle) void {
    handle.stop();
}

export fn ifr_socket_server_destroy(handle: SocketServerHandle) void {
    handle.deinit();
    // Note: In real implementation, need to properly manage the allocator
    // For now, this is a simplified version
}

export fn ifr_socket_server_add_memory(handle: SocketServerHandle, content: [*:0]const u8, memory_data: [*:0]const u8) u64 {
    const content_slice = std.mem.span(content);
    const memory_data_slice = std.mem.span(memory_data);
    
    const memory_id = handle.ifr_registry.addMemory(content_slice, memory_data_slice) catch return 0;
    return memory_id.hash;
}

export fn ifr_socket_server_query(handle: SocketServerHandle, content: [*:0]const u8) c_int {
    const content_slice = std.mem.span(content);
    const result = handle.ifr_registry.query(content_slice);
    return if (result.found_exact) 1 else 0;
}