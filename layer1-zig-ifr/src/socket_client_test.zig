// Layer 1 IFR Socket Client Test
// Tests both JSON and binary protocol interfaces
// Validates performance and functionality

const std = @import("std");
const net = std.net;
const print = std.debug.print;
const ArrayList = std.ArrayList;
const socket_server = @import("socket_server.zig");

// Protocol constants (matching server)
const PROTOCOL_JSON: u8 = 0x01;
const PROTOCOL_BINARY: u8 = 0x02;
const MSG_ADD_MEMORY: u8 = 0x10;
const MSG_QUERY_MEMORY: u8 = 0x20;
const MSG_GET_STATS: u8 = 0x30;
const MSG_PING: u8 = 0x40;
const MSG_RESPONSE: u8 = 0x80;
const MSG_ERROR: u8 = 0x90;

const BinaryHeader = socket_server.BinaryHeader;
const DEFAULT_SOCKET_PATH = socket_server.DEFAULT_SOCKET_PATH;

// ============================================================================
// JSON Client Functions
// ============================================================================

fn connectToSocket(socket_path: []const u8) !net.Stream {
    const addr = try net.Address.initUnix(socket_path);
    const stream = try net.tcpConnectToAddress(addr);
    return stream;
}

fn sendJsonRequest(stream: net.Stream, json_request: []const u8) !void {
    _ = try stream.writeAll(json_request);
}

fn receiveJsonResponse(stream: net.Stream, allocator: std.mem.Allocator) ![]u8 {
    var buffer = ArrayList(u8){};
    defer buffer.deinit(allocator);
    
    var temp_buffer: [1024]u8 = undefined;
    
    while (true) {
        const bytes_read = try stream.read(temp_buffer[0..]);
        if (bytes_read == 0) break;
        
        try buffer.appendSlice(allocator, temp_buffer[0..bytes_read]);
        
        // Check for complete JSON response (ends with newline)
        if (std.mem.indexOf(u8, buffer.items, "\n")) |_| {
            break;
        }
    }
    
    return try allocator.dupe(u8, buffer.items);
}

fn testJsonPing(allocator: std.mem.Allocator, socket_path: []const u8) !void {
    print("📡 Testing JSON Ping...\n", .{});
    
    const stream = connectToSocket(socket_path) catch |err| {
        print("❌ Failed to connect to socket: {}\n", .{err});
        return err;
    };
    defer stream.close();
    
    const request = "{\"type\":\"ping\",\"request_id\":\"test_ping_001\"}\n";
    
    const start_time = std.time.nanoTimestamp();
    try sendJsonRequest(stream, request);
    const response = try receiveJsonResponse(stream, allocator);
    const end_time = std.time.nanoTimestamp();
    
    defer allocator.free(response);
    
    const latency_ns = end_time - start_time;
    print("✅ JSON Ping successful\n", .{});
    print("   Response: {s}\n", .{std.mem.trim(u8, response, "\n\r ")});
    print("   Latency: {}ns ({d:.6}ms)\n", .{ latency_ns, @as(f64, @floatFromInt(latency_ns)) / 1_000_000.0 });
}

fn testJsonAddMemory(allocator: std.mem.Allocator, socket_path: []const u8) !void {
    print("📝 Testing JSON Add Memory...\n", .{});
    
    const stream = connectToSocket(socket_path) catch |err| {
        print("❌ Failed to connect to socket: {}\n", .{err});
        return err;
    };
    defer stream.close();
    
    const request = "{\"type\":\"add_memory\",\"request_id\":\"test_add_001\",\"content\":\"Test memory content from client\",\"memory_data\":\"Additional memory data\"}\n";
    
    const start_time = std.time.nanoTimestamp();
    try sendJsonRequest(stream, request);
    const response = try receiveJsonResponse(stream, allocator);
    const end_time = std.time.nanoTimestamp();
    
    defer allocator.free(response);
    
    const latency_ns = end_time - start_time;
    print("✅ JSON Add Memory successful\n", .{});
    print("   Response: {s}\n", .{std.mem.trim(u8, response, "\n\r ")});
    print("   Latency: {}ns ({d:.6}ms)\n", .{ latency_ns, @as(f64, @floatFromInt(latency_ns)) / 1_000_000.0 });
}

fn testJsonQuery(allocator: std.mem.Allocator, socket_path: []const u8) !void {
    print("🔍 Testing JSON Query...\n", .{});
    
    const stream = connectToSocket(socket_path) catch |err| {
        print("❌ Failed to connect to socket: {}\n", .{err});
        return err;
    };
    defer stream.close();
    
    const request = "{\"type\":\"query\",\"request_id\":\"test_query_001\",\"query_content\":\"The human brain contains approximately 86 billion neurons\"}\n";
    
    const start_time = std.time.nanoTimestamp();
    try sendJsonRequest(stream, request);
    const response = try receiveJsonResponse(stream, allocator);
    const end_time = std.time.nanoTimestamp();
    
    defer allocator.free(response);
    
    const latency_ns = end_time - start_time;
    print("✅ JSON Query successful\n", .{});
    print("   Response: {s}\n", .{std.mem.trim(u8, response, "\n\r ")});
    print("   Latency: {}ns ({d:.6}ms)\n", .{ latency_ns, @as(f64, @floatFromInt(latency_ns)) / 1_000_000.0 });
}

fn testJsonGetStats(allocator: std.mem.Allocator, socket_path: []const u8) !void {
    print("📊 Testing JSON Get Stats...\n", .{});
    
    const stream = connectToSocket(socket_path) catch |err| {
        print("❌ Failed to connect to socket: {}\n", .{err});
        return err;
    };
    defer stream.close();
    
    const request = "{\"type\":\"get_stats\",\"request_id\":\"test_stats_001\"}\n";
    
    const start_time = std.time.nanoTimestamp();
    try sendJsonRequest(stream, request);
    const response = try receiveJsonResponse(stream, allocator);
    const end_time = std.time.nanoTimestamp();
    
    defer allocator.free(response);
    
    const latency_ns = end_time - start_time;
    print("✅ JSON Get Stats successful\n", .{});
    print("   Response: {s}\n", .{std.mem.trim(u8, response, "\n\r ")});
    print("   Latency: {}ns ({d:.6}ms)\n", .{ latency_ns, @as(f64, @floatFromInt(latency_ns)) / 1_000_000.0 });
}

// ============================================================================
// Binary Client Functions
// ============================================================================

fn sendBinaryMessage(stream: net.Stream, message_type: u8, request_id: u32, payload: []const u8) !void {
    const header = BinaryHeader{
        .protocol_version = PROTOCOL_BINARY,
        .message_type = message_type,
        .request_id = request_id,
        .payload_length = @intCast(payload.len),
        .reserved = 0,
    };
    
    const header_bytes = std.mem.asBytes(&header);
    _ = try stream.writeAll(header_bytes);
    _ = try stream.writeAll(payload);
}

fn receiveBinaryResponse(stream: net.Stream, allocator: std.mem.Allocator) !struct {
    header: BinaryHeader,
    payload: []u8,
} {
    // Read header
    var header_buffer: [@sizeOf(BinaryHeader)]u8 = undefined;
    _ = try stream.readAll(header_buffer[0..]);
    
    const header = std.mem.bytesToValue(BinaryHeader, header_buffer[0..]);
    
    // Read payload
    const payload = try allocator.alloc(u8, header.payload_length);
    _ = try stream.readAll(payload);
    
    return .{
        .header = header,
        .payload = payload,
    };
}

fn testBinaryPing(allocator: std.mem.Allocator, socket_path: []const u8) !void {
    print("📡 Testing Binary Ping...\n", .{});
    
    const stream = connectToSocket(socket_path) catch |err| {
        print("❌ Failed to connect to socket: {}\n", .{err});
        return err;
    };
    defer stream.close();
    
    const payload = "";
    const request_id: u32 = 1001;
    
    const start_time = std.time.nanoTimestamp();
    try sendBinaryMessage(stream, MSG_PING, request_id, payload);
    const response = try receiveBinaryResponse(stream, allocator);
    const end_time = std.time.nanoTimestamp();
    
    defer allocator.free(response.payload);
    
    const latency_ns = end_time - start_time;
    print("✅ Binary Ping successful\n", .{});
    print("   Request ID: {} -> {}\n", .{ request_id, response.header.request_id });
    print("   Message Type: {}\n", .{response.header.message_type});
    print("   Payload: {s}\n", .{response.payload});
    print("   Latency: {}ns ({d:.6}ms)\n", .{ latency_ns, @as(f64, @floatFromInt(latency_ns)) / 1_000_000.0 });
}

fn testBinaryAddMemory(allocator: std.mem.Allocator, socket_path: []const u8) !void {
    print("📝 Testing Binary Add Memory...\n", .{});
    
    const stream = connectToSocket(socket_path) catch |err| {
        print("❌ Failed to connect to socket: {}\n", .{err});
        return err;
    };
    defer stream.close();
    
    // Binary payload format: content_len(4) + content + memory_data_len(4) + memory_data
    const content = "Binary test memory content";
    const memory_data = "Binary test memory data";
    
    var payload_buffer = ArrayList(u8){};
    defer payload_buffer.deinit(allocator);
    
    const content_len: u32 = @intCast(content.len);
    const memory_data_len: u32 = @intCast(memory_data.len);
    
    const content_len_bytes = std.mem.asBytes(&content_len);
    try payload_buffer.appendSlice(allocator, content_len_bytes);
    try payload_buffer.appendSlice(allocator, content);
    
    const memory_data_len_bytes = std.mem.asBytes(&memory_data_len);
    try payload_buffer.appendSlice(allocator, memory_data_len_bytes);
    try payload_buffer.appendSlice(allocator, memory_data);
    
    const request_id: u32 = 1002;
    
    const start_time = std.time.nanoTimestamp();
    try sendBinaryMessage(stream, MSG_ADD_MEMORY, request_id, payload_buffer.items);
    const response = try receiveBinaryResponse(stream, allocator);
    const end_time = std.time.nanoTimestamp();
    
    defer allocator.free(response.payload);
    
    const latency_ns = end_time - start_time;
    print("✅ Binary Add Memory successful\n", .{});
    print("   Request ID: {} -> {}\n", .{ request_id, response.header.request_id });
    print("   Message Type: {}\n", .{response.header.message_type});
    print("   Memory ID Hash: {}\n", .{std.mem.readInt(u64, response.payload[0..8][0..8], .little)});
    print("   Latency: {}ns ({d:.6}ms)\n", .{ latency_ns, @as(f64, @floatFromInt(latency_ns)) / 1_000_000.0 });
}

fn testBinaryQuery(allocator: std.mem.Allocator, socket_path: []const u8) !void {
    print("🔍 Testing Binary Query...\n", .{});
    
    const stream = connectToSocket(socket_path) catch |err| {
        print("❌ Failed to connect to socket: {}\n", .{err});
        return err;
    };
    defer stream.close();
    
    // Query for one of the pre-populated memories
    const query_content = "The human brain contains approximately 86 billion neurons";
    const request_id: u32 = 1003;
    
    const start_time = std.time.nanoTimestamp();
    try sendBinaryMessage(stream, MSG_QUERY_MEMORY, request_id, query_content);
    const response = try receiveBinaryResponse(stream, allocator);
    const end_time = std.time.nanoTimestamp();
    
    defer allocator.free(response.payload);
    
    const latency_ns = end_time - start_time;
    
    // Parse binary query response
    if (response.payload.len >= 18) { // found_exact(1) + next_layer(1) + confidence(4) + processing_time(8) + result_len(4)
        const found_exact = response.payload[0] != 0;
        const next_layer = response.payload[1];
        const confidence: f32 = @bitCast(std.mem.readInt(u32, response.payload[2..6][0..4], .little));
        const processing_time_ns = std.mem.readInt(u64, response.payload[6..14][0..8], .little);
        const result_len = std.mem.readInt(u32, response.payload[14..18][0..4], .little);
        
        print("✅ Binary Query successful\n", .{});
        print("   Request ID: {} -> {}\n", .{ request_id, response.header.request_id });
        print("   Found Exact: {}\n", .{found_exact});
        print("   Next Layer: {}\n", .{next_layer});
        print("   Confidence: {d:.2}\n", .{confidence});
        print("   Processing Time (server): {}ns ({d:.6}ms)\n", .{ processing_time_ns, @as(f64, @floatFromInt(processing_time_ns)) / 1_000_000.0 });
        print("   Result Length: {}\n", .{result_len});
        print("   Total Latency: {}ns ({d:.6}ms)\n", .{ latency_ns, @as(f64, @floatFromInt(latency_ns)) / 1_000_000.0 });
        
        if (result_len > 0 and response.payload.len >= 18 + result_len) {
            const result_data = response.payload[18..18 + result_len];
            print("   Result Data: {s}\n", .{result_data});
        }
    } else {
        print("❌ Invalid binary query response format\n", .{});
    }
}

fn testBinaryGetStats(allocator: std.mem.Allocator, socket_path: []const u8) !void {
    print("📊 Testing Binary Get Stats...\n", .{});
    
    const stream = connectToSocket(socket_path) catch |err| {
        print("❌ Failed to connect to socket: {}\n", .{err});
        return err;
    };
    defer stream.close();
    
    const payload = "";
    const request_id: u32 = 1004;
    
    const start_time = std.time.nanoTimestamp();
    try sendBinaryMessage(stream, MSG_GET_STATS, request_id, payload);
    const response = try receiveBinaryResponse(stream, allocator);
    const end_time = std.time.nanoTimestamp();
    
    defer allocator.free(response.payload);
    
    const latency_ns = end_time - start_time;
    
    // Parse binary stats response
    if (response.payload.len >= 24) { // total_queries(8) + exact_hits(8) + hit_rate(4) + memory_count(8)
        const total_queries = std.mem.readInt(u64, response.payload[0..8][0..8], .little);
        const exact_hits = std.mem.readInt(u64, response.payload[8..16][0..8], .little);
        const hit_rate: f32 = @bitCast(std.mem.readInt(u32, response.payload[16..20][0..4], .little));
        const memory_count = std.mem.readInt(u64, response.payload[20..28][0..8], .little);
        
        print("✅ Binary Get Stats successful\n", .{});
        print("   Request ID: {} -> {}\n", .{ request_id, response.header.request_id });
        print("   Total Queries: {}\n", .{total_queries});
        print("   Exact Hits: {}\n", .{exact_hits});
        print("   Hit Rate: {d:.2}%\n", .{hit_rate * 100.0});
        print("   Memory Count: {}\n", .{memory_count});
        print("   Latency: {}ns ({d:.6}ms)\n", .{ latency_ns, @as(f64, @floatFromInt(latency_ns)) / 1_000_000.0 });
    } else {
        print("❌ Invalid binary stats response format\n", .{});
    }
}

// ============================================================================
// Performance Benchmarks
// ============================================================================

fn benchmarkJsonPerformance(allocator: std.mem.Allocator, socket_path: []const u8, iterations: u32) !void {
    print("🚀 Benchmarking JSON Performance ({} iterations)...\n", .{iterations});
    
    var total_latency: u64 = 0;
    var min_latency: u64 = std.math.maxInt(u64);
    var max_latency: u64 = 0;
    
    for (0..iterations) |_| {
        const stream = connectToSocket(socket_path) catch |err| {
            print("❌ Failed to connect to socket: {}\n", .{err});
            return err;
        };
        defer stream.close();
        
        const request = "{\"type\":\"ping\",\"request_id\":\"bench_ping\"}\n";
        
        const start_time = std.time.nanoTimestamp();
        try sendJsonRequest(stream, request);
        const response = try receiveJsonResponse(stream, allocator);
        const end_time = std.time.nanoTimestamp();
        
        allocator.free(response);
        
        const latency = @as(u64, @intCast(end_time - start_time));
        total_latency += latency;
        min_latency = @min(min_latency, latency);
        max_latency = @max(max_latency, latency);
    }
    
    const mean_latency = @as(f64, @floatFromInt(total_latency)) / @as(f64, @floatFromInt(iterations));
    
    print("📊 JSON Performance Results:\n", .{});
    print("   Iterations: {}\n", .{iterations});
    print("   Mean Latency: {d:.2}ns ({d:.6}ms)\n", .{ mean_latency, mean_latency / 1_000_000.0 });
    print("   Min Latency: {}ns ({d:.6}ms)\n", .{ min_latency, @as(f64, @floatFromInt(min_latency)) / 1_000_000.0 });
    print("   Max Latency: {}ns ({d:.6}ms)\n", .{ max_latency, @as(f64, @floatFromInt(max_latency)) / 1_000_000.0 });
    print("   Throughput: {d:.0} ops/sec\n", .{1_000_000_000.0 / mean_latency});
}

fn benchmarkBinaryPerformance(allocator: std.mem.Allocator, socket_path: []const u8, iterations: u32) !void {
    print("🚀 Benchmarking Binary Performance ({} iterations)...\n", .{iterations});
    
    var total_latency: u64 = 0;
    var min_latency: u64 = std.math.maxInt(u64);
    var max_latency: u64 = 0;
    
    for (0..iterations) |i| {
        const stream = connectToSocket(socket_path) catch |err| {
            print("❌ Failed to connect to socket: {}\n", .{err});
            return err;
        };
        defer stream.close();
        
        const payload = "";
        const request_id: u32 = @intCast(2000 + i);
        
        const start_time = std.time.nanoTimestamp();
        try sendBinaryMessage(stream, MSG_PING, request_id, payload);
        const response = try receiveBinaryResponse(stream, allocator);
        const end_time = std.time.nanoTimestamp();
        
        allocator.free(response.payload);
        
        const latency = @as(u64, @intCast(end_time - start_time));
        total_latency += latency;
        min_latency = @min(min_latency, latency);
        max_latency = @max(max_latency, latency);
    }
    
    const mean_latency = @as(f64, @floatFromInt(total_latency)) / @as(f64, @floatFromInt(iterations));
    
    print("📊 Binary Performance Results:\n", .{});
    print("   Iterations: {}\n", .{iterations});
    print("   Mean Latency: {d:.2}ns ({d:.6}ms)\n", .{ mean_latency, mean_latency / 1_000_000.0 });
    print("   Min Latency: {}ns ({d:.6}ms)\n", .{ min_latency, @as(f64, @floatFromInt(min_latency)) / 1_000_000.0 });
    print("   Max Latency: {}ns ({d:.6}ms)\n", .{ max_latency, @as(f64, @floatFromInt(max_latency)) / 1_000_000.0 });
    print("   Throughput: {d:.0} ops/sec\n", .{1_000_000_000.0 / mean_latency});
}

// ============================================================================
// Main Test Function
// ============================================================================

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const socket_path = DEFAULT_SOCKET_PATH;
    
    print("\n╔══════════════════════════════════════════════════════════════════════════════╗\n", .{});
    print("║                      Layer 1 IFR Socket Client Test Suite                  ║\n", .{});
    print("║                     Testing Unix Socket Interface Performance              ║\n", .{});
    print("╚══════════════════════════════════════════════════════════════════════════════╝\n\n", .{});
    
    print("🔗 Connecting to Layer 1 IFR Socket Server\n", .{});
    print("   Socket Path: {s}\n\n", .{socket_path});
    
    // Test JSON Protocol
    print("=" ** 80 ++ "\n", .{});
    print("                           JSON Protocol Tests\n", .{});
    print("=" ** 80 ++ "\n", .{});
    
    testJsonPing(allocator, socket_path) catch |err| {
        print("❌ JSON Ping test failed: {}\n", .{err});
    };
    print("\n", .{});
    
    testJsonAddMemory(allocator, socket_path) catch |err| {
        print("❌ JSON Add Memory test failed: {}\n", .{err});
    };
    print("\n", .{});
    
    testJsonQuery(allocator, socket_path) catch |err| {
        print("❌ JSON Query test failed: {}\n", .{err});
    };
    print("\n", .{});
    
    testJsonGetStats(allocator, socket_path) catch |err| {
        print("❌ JSON Get Stats test failed: {}\n", .{err});
    };
    print("\n", .{});
    
    // Test Binary Protocol
    print("=" ** 80 ++ "\n", .{});
    print("                          Binary Protocol Tests\n", .{});
    print("=" ** 80 ++ "\n", .{});
    
    testBinaryPing(allocator, socket_path) catch |err| {
        print("❌ Binary Ping test failed: {}\n", .{err});
    };
    print("\n", .{});
    
    testBinaryAddMemory(allocator, socket_path) catch |err| {
        print("❌ Binary Add Memory test failed: {}\n", .{err});
    };
    print("\n", .{});
    
    testBinaryQuery(allocator, socket_path) catch |err| {
        print("❌ Binary Query test failed: {}\n", .{err});
    };
    print("\n", .{});
    
    testBinaryGetStats(allocator, socket_path) catch |err| {
        print("❌ Binary Get Stats test failed: {}\n", .{err});
    };
    print("\n", .{});
    
    // Performance Benchmarks
    print("=" ** 80 ++ "\n", .{});
    print("                          Performance Benchmarks\n", .{});
    print("=" ** 80 ++ "\n", .{});
    
    benchmarkJsonPerformance(allocator, socket_path, 100) catch |err| {
        print("❌ JSON benchmark failed: {}\n", .{err});
    };
    print("\n", .{});
    
    benchmarkBinaryPerformance(allocator, socket_path, 100) catch |err| {
        print("❌ Binary benchmark failed: {}\n", .{err});
    };
    print("\n", .{});
    
    print("🎉 Layer 1 IFR Socket Client Test Suite completed!\n", .{});
}