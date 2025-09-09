// Memory Flow Network - Layer 1: Immediate Flow Registry (IFR)
// Ultra-fast exact matching and routing using Zig's comptime optimization
// Target: <0.1ms response time with zero garbage collection overhead

const std = @import("std");
const assert = std.debug.assert;
const Allocator = std.mem.Allocator;

// ============================================================================
// Core Data Structures
// ============================================================================

pub const MemoryID = struct {
    hash: u64,
};

pub const RoutingDecision = struct {
    found_exact: bool,
    result: ?[]const u8,
    next_layer: ?u8,
    confidence: f32,
    processing_time_ns: u64,
};

// ============================================================================
// Comptime-Optimized Hash Functions
// ============================================================================

// MurmurHash3 implementation optimized for comptime evaluation
pub fn murmurHash3(comptime seed: u32) fn ([]const u8) u64 {
    return struct {
        pub fn hash(data: []const u8) u64 {
            const c1: u64 = 0x87c37b91114253d5;
            const c2: u64 = 0x4cf5ad432745937f;
            const r1: u8 = 31;
            const r2: u8 = 27; 
            const m: u64 = 5;
            const n: u64 = 0x52dce729;

            var h: u64 = seed;
            const len = data.len;
            const blocks = len / 8;

            // Process 8-byte blocks
            var i: usize = 0;
            while (i < blocks) : (i += 1) {
                var k: u64 = std.mem.readInt(u64, data[i * 8 .. i * 8 + 8][0..8], .little);
                
                k *%= c1;
                k = std.math.rotl(u64, k, r1);
                k *%= c2;
                
                h ^= k;
                h = std.math.rotl(u64, h, r2);
                h = h *% m +% n;
            }

            // Process remaining bytes
            const tail = data[blocks * 8..];
            var k1: u64 = 0;
            
            switch (tail.len) {
                7 => k1 ^= @as(u64, tail[6]) << 48,
                6 => k1 ^= @as(u64, tail[5]) << 40,
                5 => k1 ^= @as(u64, tail[4]) << 32,
                4 => k1 ^= @as(u64, tail[3]) << 24,
                3 => k1 ^= @as(u64, tail[2]) << 16,
                2 => k1 ^= @as(u64, tail[1]) << 8,
                1 => {
                    k1 ^= @as(u64, tail[0]);
                    k1 *%= c1;
                    k1 = std.math.rotl(u64, k1, r1);
                    k1 *%= c2;
                    h ^= k1;
                },
                0 => {},
                else => unreachable,
            }

            // Finalization
            h ^= len;
            h ^= h >> 33;
            h *%= 0xff51afd7ed558ccd;
            h ^= h >> 33;
            h *%= 0xc4ceb9fe1a85ec53;
            h ^= h >> 33;

            return h;
        }
    }.hash;
}

// Multiple hash functions for bloom filter
const hash_fn_1 = murmurHash3(0x12345678);
const hash_fn_2 = murmurHash3(0x87654321);
const hash_fn_3 = murmurHash3(0xABCDEF00);
const hash_fn_4 = murmurHash3(0x00FEDCBA);

// ============================================================================
// Bloom Filter Implementation  
// ============================================================================

pub const BloomFilter = struct {
    bits: []u64,        // Bit array packed into u64s
    size_bits: u64,     // Total number of bits
    hash_count: u8,     // Number of hash functions
    items_added: u64,   // Statistics
    
    const Self = @This();
    
    pub fn init(allocator: Allocator, capacity: u64, error_rate: f64) !Self {
        // Calculate optimal size and hash count
        const ln2 = @log(2.0);
        const size_bits = @as(u64, @intFromFloat(-@as(f64, @floatFromInt(capacity)) * @log(error_rate) / (ln2 * ln2)));
        const hash_count = @as(u8, @intFromFloat(@as(f64, @floatFromInt(size_bits)) * ln2 / @as(f64, @floatFromInt(capacity))));
        
        // Round up to multiple of 64 for efficient packing
        const size_u64s = (size_bits + 63) / 64;
        const bits = try allocator.alloc(u64, size_u64s);
        @memset(bits, 0);
        
        return Self{
            .bits = bits,
            .size_bits = size_u64s * 64,
            .hash_count = @min(hash_count, 4), // Limit to our 4 hash functions
            .items_added = 0,
        };
    }
    
    pub fn deinit(self: *Self, allocator: Allocator) void {
        allocator.free(self.bits);
    }
    
    pub fn add(self: *Self, data: []const u8) void {
        const hashes = self.getHashes(data);
        
        for (hashes[0..self.hash_count]) |hash_val| {
            const bit_index = hash_val % self.size_bits;
            const u64_index = bit_index / 64;
            const bit_offset = @as(u6, @intCast(bit_index % 64));
            
            self.bits[u64_index] |= (@as(u64, 1) << bit_offset);
        }
        
        self.items_added += 1;
    }
    
    pub fn contains(self: *Self, data: []const u8) bool {
        const hashes = self.getHashes(data);
        
        for (hashes[0..self.hash_count]) |hash_val| {
            const bit_index = hash_val % self.size_bits;
            const u64_index = bit_index / 64;
            const bit_offset = @as(u6, @intCast(bit_index % 64));
            
            if ((self.bits[u64_index] & (@as(u64, 1) << bit_offset)) == 0) {
                return false;
            }
        }
        
        return true;
    }
    
    inline fn getHashes(self: *Self, data: []const u8) [4]u64 {
        _ = self; // Suppress unused parameter warning
        return [4]u64{
            hash_fn_1(data),
            hash_fn_2(data), 
            hash_fn_3(data),
            hash_fn_4(data),
        };
    }
    
    pub fn getStats(self: *Self) struct {
        capacity: u64,
        size_bits: u64,
        hash_functions: u8,
        items_added: u64,
        estimated_error_rate: f64,
    } {
        // Count set bits for fill ratio calculation
        var bits_set: u64 = 0;
        for (self.bits) |word| {
            bits_set += @popCount(word);
        }
        
        const fill_ratio = @as(f64, @floatFromInt(bits_set)) / @as(f64, @floatFromInt(self.size_bits));
        const error_rate = std.math.pow(f64, fill_ratio, @as(f64, @floatFromInt(self.hash_count)));
        
        return .{
            .capacity = self.size_bits,
            .size_bits = self.size_bits,
            .hash_functions = self.hash_count,
            .items_added = self.items_added,
            .estimated_error_rate = error_rate,
        };
    }
};

// ============================================================================
// Perfect Hash Table Implementation
// ============================================================================

pub const HashEntry = struct {
    key: []const u8,
    value: []const u8,
    occupied: bool,
};

pub const PerfectHashTable = struct {
    entries: []HashEntry,
    size: u64,
    count: u64,
    load_factor: f32,
    
    const Self = @This();
    
    pub fn init(allocator: Allocator, initial_size: u64, load_factor: f32) !Self {
        const entries = try allocator.alloc(HashEntry, initial_size);
        for (entries) |*entry| {
            entry.* = HashEntry{
                .key = "",
                .value = "",
                .occupied = false,
            };
        }
        
        return Self{
            .entries = entries,
            .size = initial_size,
            .count = 0,
            .load_factor = load_factor,
        };
    }
    
    pub fn deinit(self: *Self, allocator: Allocator) void {
        allocator.free(self.entries);
    }
    
    pub fn put(self: *Self, allocator: Allocator, key: []const u8, value: []const u8) !void {
        // Resize if load factor exceeded
        if (@as(f32, @floatFromInt(self.count)) >= @as(f32, @floatFromInt(self.size)) * self.load_factor) {
            try self.resize(allocator);
        }
        
        const index = self.probe(key);
        
        if (!self.entries[index].occupied) {
            self.count += 1;
        }
        
        // Store copies of key and value
        const key_copy = try allocator.dupe(u8, key);
        const value_copy = try allocator.dupe(u8, value);
        
        self.entries[index] = HashEntry{
            .key = key_copy,
            .value = value_copy,
            .occupied = true,
        };
    }
    
    pub fn get(self: *Self, key: []const u8) ?[]const u8 {
        const index = self.probe(key);
        
        if (self.entries[index].occupied and std.mem.eql(u8, self.entries[index].key, key)) {
            return self.entries[index].value;
        }
        
        return null;
    }
    
    pub fn contains(self: *Self, key: []const u8) bool {
        return self.get(key) != null;
    }
    
    fn probe(self: *Self, key: []const u8) u64 {
        var index = hash_fn_1(key) % self.size;
        const original_index = index;
        
        while (self.entries[index].occupied) {
            if (std.mem.eql(u8, self.entries[index].key, key)) {
                return index; // Found existing key
            }
            
            // Linear probing
            index = (index + 1) % self.size;
            
            // Prevent infinite loop
            if (index == original_index) {
                @panic("Hash table is full - should not happen with proper load factor");
            }
        }
        
        return index; // Found empty slot
    }
    
    fn resize(self: *Self, allocator: Allocator) !void {
        const old_entries = self.entries;
        const old_size = self.size;
        
        // Double the size
        self.size = old_size * 2;
        self.entries = try allocator.alloc(HashEntry, self.size);
        self.count = 0;
        
        // Initialize new entries
        for (self.entries) |*entry| {
            entry.* = HashEntry{
                .key = "",
                .value = "",
                .occupied = false,
            };
        }
        
        // Rehash all existing entries
        for (old_entries) |entry| {
            if (entry.occupied) {
                const index = self.probe(entry.key);
                if (!self.entries[index].occupied) {
                    self.count += 1;
                }
                self.entries[index] = entry; // Move the entry directly
            }
        }
        
        // Free old entries array (keys and values are moved, not copied)
        allocator.free(old_entries);
    }
    
    pub fn getStats(self: *Self) struct {
        size: u64,
        count: u64,
        load_factor: f32,
        collision_rate: f32,
    } {
        var collisions: u64 = 0;
        
        for (self.entries, 0..) |entry, i| {
            if (entry.occupied) {
                const expected_index = hash_fn_1(entry.key) % self.size;
                if (expected_index != i) {
                    collisions += 1;
                }
            }
        }
        
        return .{
            .size = self.size,
            .count = self.count,
            .load_factor = @as(f32, @floatFromInt(self.count)) / @as(f32, @floatFromInt(self.size)),
            .collision_rate = if (self.count > 0) @as(f32, @floatFromInt(collisions)) / @as(f32, @floatFromInt(self.count)) else 0.0,
        };
    }
};

// ============================================================================
// Main Immediate Flow Registry Implementation
// ============================================================================

pub const ImmediateFlowRegistry = struct {
    bloom_filter: BloomFilter,
    hash_table: PerfectHashTable,
    allocator: Allocator,
    
    // Statistics
    query_count: u64,
    exact_hits: u64,
    bloom_hits: u64,
    bloom_false_positives: u64,
    
    const Self = @This();
    
    pub fn init(allocator: Allocator, 
                bloom_capacity: u64, 
                bloom_error_rate: f64,
                hash_initial_size: u64) !Self {
        
        const bloom_filter = try BloomFilter.init(allocator, bloom_capacity, bloom_error_rate);
        const hash_table = try PerfectHashTable.init(allocator, hash_initial_size, 0.7);
        
        return Self{
            .bloom_filter = bloom_filter,
            .hash_table = hash_table,
            .allocator = allocator,
            .query_count = 0,
            .exact_hits = 0,
            .bloom_hits = 0,
            .bloom_false_positives = 0,
        };
    }
    
    pub fn deinit(self: *Self) void {
        self.bloom_filter.deinit(self.allocator);
        self.hash_table.deinit(self.allocator);
    }
    
    pub fn addMemory(self: *Self, content: []const u8, memory_data: []const u8) !MemoryID {
        const hash_key = hash_fn_1(content);
        
        // Add to bloom filter
        self.bloom_filter.add(content);
        
        // Add to hash table
        try self.hash_table.put(self.allocator, content, memory_data);
        
        return MemoryID{ .hash = hash_key };
    }
    
    pub fn query(self: *Self, content: []const u8) RoutingDecision {
        const start_time = std.time.nanoTimestamp();
        
        self.query_count += 1;
        
        // Step 1: Check Bloom filter (fast membership test)
        if (!self.bloom_filter.contains(content)) {
            // Definitely not in hash table, route to similarity layer
            const processing_time = @as(u64, @intCast(std.time.nanoTimestamp() - start_time));
            
            return RoutingDecision{
                .found_exact = false,
                .result = null,
                .next_layer = 2, // Route to Layer 2 (Similarity)
                .confidence = 0.0,
                .processing_time_ns = processing_time,
            };
        }
        
        // Step 2: Bloom filter says "maybe", check hash table
        self.bloom_hits += 1;
        const result = self.hash_table.get(content);
        
        if (result) |memory_data| {
            // Exact match found!
            self.exact_hits += 1;
            const processing_time = @as(u64, @intCast(std.time.nanoTimestamp() - start_time));
            
            return RoutingDecision{
                .found_exact = true,
                .result = memory_data,
                .next_layer = null, // No need to route further
                .confidence = 1.0,
                .processing_time_ns = processing_time,
            };
        } else {
            // Bloom filter false positive
            self.bloom_false_positives += 1;
            const processing_time = @as(u64, @intCast(std.time.nanoTimestamp() - start_time));
            
            return RoutingDecision{
                .found_exact = false,
                .result = null,
                .next_layer = 2, // Route to Layer 2 (Similarity)
                .confidence = 0.0,
                .processing_time_ns = processing_time,
            };
        }
    }
    
    pub fn getPerformanceStats(self: *Self) struct {
        total_queries: u64,
        exact_hits: u64,
        hit_rate: f32,
        bloom_hits: u64,
        bloom_false_positives: u64,
        false_positive_rate: f32,
        memory_count: u64,
    } {
        const hit_rate = if (self.query_count > 0) 
            @as(f32, @floatFromInt(self.exact_hits)) / @as(f32, @floatFromInt(self.query_count)) 
            else 0.0;
            
        const false_positive_rate = if (self.bloom_hits > 0)
            @as(f32, @floatFromInt(self.bloom_false_positives)) / @as(f32, @floatFromInt(self.bloom_hits))
            else 0.0;
        
        return .{
            .total_queries = self.query_count,
            .exact_hits = self.exact_hits,
            .hit_rate = hit_rate,
            .bloom_hits = self.bloom_hits,
            .bloom_false_positives = self.bloom_false_positives,
            .false_positive_rate = false_positive_rate,
            .memory_count = self.hash_table.count,
        };
    }
    
    pub fn benchmarkPerformance(self: *Self, test_queries: [][]const u8, iterations: u32) !struct {
        min_time_ns: u64,
        max_time_ns: u64,
        mean_time_ns: f64,
        total_queries: u64,
    } {
        var times = try self.allocator.alloc(u64, test_queries.len * iterations);
        defer self.allocator.free(times);
        
        var time_index: u64 = 0;
        
        var iter: u32 = 0;
        while (iter < iterations) : (iter += 1) {
            for (test_queries) |test_query| {
                const result = self.query(test_query);
                times[time_index] = result.processing_time_ns;
                time_index += 1;
            }
        }
        
        // Sort times for statistics
        std.sort.block(u64, times, {}, comptime std.sort.asc(u64));
        
        const total_time: u64 = blk: {
            var sum: u64 = 0;
            for (times) |time| {
                sum += time;
            }
            break :blk sum;
        };
        
        return .{
            .min_time_ns = times[0],
            .max_time_ns = times[times.len - 1],
            .mean_time_ns = @as(f64, @floatFromInt(total_time)) / @as(f64, @floatFromInt(times.len)),
            .total_queries = times.len,
        };
    }
};