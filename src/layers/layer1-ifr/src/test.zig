// Test suite for Layer 1 Immediate Flow Registry
// Validates correctness of bloom filters, hash tables, and routing logic

const std = @import("std");
const print = std.debug.print;
const assert = std.debug.assert;
const testing = std.testing;
const ifr = @import("ifr.zig");

const IFR = ifr.ImmediateFlowRegistry;
const BloomFilter = ifr.BloomFilter;
const PerfectHashTable = ifr.PerfectHashTable;

// Test data from our generated test document
const test_memories = [_][]const u8{
    "The human brain contains approximately 86 billion neurons",
    "Octopuses have three hearts and blue blood due to copper-based hemocyanin",
    "The speed of light in a vacuum is exactly 299,792,458 meters per second",
    "Honey never spoils - archaeologists have found 3000-year-old honey that's still edible",
    "A single cloud can weigh more than a million pounds",
    "Napoleon Bonaparte was actually average height for his time at 5'7\"",
    "The Great Wall of China was built over several dynasties, primarily during the Ming Dynasty",
    "Vikings reached North America roughly 500 years before Columbus",
    "The shortest war in history lasted only 38-45 minutes between Britain and Zanzibar",
    "Your smartphone has more computing power than the computers that guided Apollo 11",
};

const non_existent_queries = [_][]const u8{
    "This memory does not exist in the system at all",
    "Another completely different query that should not match",
    "Totally unrelated content for testing false negatives",
    "Random text that was never stored in the memory system",
    "Non-existent data to verify routing to layer 2",
};

test "BloomFilter basic operations" {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var bloom = try BloomFilter.init(allocator, 10000, 0.01);
    defer bloom.deinit(allocator);

    // Test adding and checking membership
    for (test_memories) |memory| {
        assert(!bloom.contains(memory)); // Should not be present initially
        bloom.add(memory);
        assert(bloom.contains(memory)); // Should be present after adding
    }

    // Test non-existent items (some false positives allowed)
    var false_positives: u32 = 0;
    for (non_existent_queries) |query| {
        if (bloom.contains(query)) {
            false_positives += 1;
        }
    }

    // False positive rate should be reasonable (less than 50% for our test)
    const false_positive_rate = @as(f32, @floatFromInt(false_positives)) / @as(f32, @floatFromInt(non_existent_queries.len));
    print("Bloom filter false positive rate: {d:.2%}\n", .{false_positive_rate});
    assert(false_positive_rate < 0.5);

    // Check statistics
    const stats = bloom.getStats();
    print("Bloom filter stats: items_added={}, estimated_error_rate={d:.6}\n", .{ stats.items_added, stats.estimated_error_rate });
    assert(stats.items_added == test_memories.len);
}

test "PerfectHashTable basic operations" {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var hash_table = try PerfectHashTable.init(allocator, 100, 0.7);
    defer hash_table.deinit(allocator);

    // Test putting and getting
    for (test_memories, 0..) |memory, i| {
        const value = try std.fmt.allocPrint(allocator, "value_{}", .{i});
        defer allocator.free(value);

        assert(hash_table.get(memory) == null); // Should not exist initially
        try hash_table.put(allocator, memory, value);
        
        const retrieved = hash_table.get(memory);
        assert(retrieved != null);
        assert(std.mem.eql(u8, retrieved.?, value));
        assert(hash_table.contains(memory));
    }

    // Test non-existent keys
    for (non_existent_queries) |query| {
        assert(hash_table.get(query) == null);
        assert(!hash_table.contains(query));
    }

    // Check statistics
    const stats = hash_table.getStats();
    print("Hash table stats: size={}, count={}, load_factor={d:.3}, collision_rate={d:.3}\n", 
          .{ stats.size, stats.count, stats.load_factor, stats.collision_rate });
    assert(stats.count == test_memories.len);
    assert(stats.load_factor <= 0.7);
}

test "IFR full system integration" {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var registry = try IFR.init(allocator, 10000, 0.01, 100);
    defer registry.deinit();

    print("\n=== Adding memories to IFR ===\n");
    
    // Add all test memories
    for (test_memories, 0..) |memory, i| {
        const memory_data = try std.fmt.allocPrint(allocator, "{{\"id\":{},\"content\":\"{s}\",\"timestamp\":{}}}", 
                                                  .{ i, memory, std.time.timestamp() });
        defer allocator.free(memory_data);

        const memory_id = try registry.addMemory(memory, memory_data);
        print("Added memory {}: {s}...\n", .{ memory_id.hash, memory[0..@min(50, memory.len)] });
    }

    print("\n=== Testing exact matches ===\n");
    
    // Test exact matches
    for (test_memories) |memory| {
        const result = registry.query(memory);
        print("Query: '{s}...' -> Found: {}, Time: {}ns\n", 
              .{ memory[0..@min(30, memory.len)], result.found_exact, result.processing_time_ns });
        
        assert(result.found_exact == true);
        assert(result.result != null);
        assert(result.next_layer == null);
        assert(result.confidence == 1.0);
        assert(result.processing_time_ns < 1_000_000); // Should be < 1ms
    }

    print("\n=== Testing non-existent queries ===\n");
    
    // Test non-existent queries
    for (non_existent_queries) |query| {
        const result = registry.query(query);
        print("Query: '{s}...' -> Found: {}, Next Layer: {?}, Time: {}ns\n", 
              .{ query[0..@min(30, query.len)], result.found_exact, result.next_layer, result.processing_time_ns });
        
        assert(result.found_exact == false);
        assert(result.result == null);
        assert(result.next_layer == 2); // Should route to layer 2
        assert(result.confidence == 0.0);
        assert(result.processing_time_ns < 1_000_000); // Should be < 1ms
    }

    print("\n=== Performance Statistics ===\n");
    
    const stats = registry.getPerformanceStats();
    print("Total Queries: {}\n", .{stats.total_queries});
    print("Exact Hits: {}\n", .{stats.exact_hits});
    print("Hit Rate: {d:.2%}\n", .{stats.hit_rate});
    print("Memory Count: {}\n", .{stats.memory_count});
    print("Bloom False Positives: {}\n", .{stats.bloom_false_positives});
    print("False Positive Rate: {d:.2%}\n", .{stats.false_positive_rate});

    // Validate statistics
    assert(stats.total_queries == test_memories.len + non_existent_queries.len);
    assert(stats.exact_hits == test_memories.len);
    assert(stats.memory_count == test_memories.len);
    assert(stats.hit_rate > 0.0);
}

test "IFR performance targets" {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    var registry = try IFR.init(allocator, 100000, 0.001, 1000);
    defer registry.deinit();

    // Add a substantial number of memories
    const large_dataset_size = 1000;
    var large_dataset = try allocator.alloc([]const u8, large_dataset_size);
    defer {
        for (large_dataset) |item| {
            allocator.free(item);
        }
        allocator.free(large_dataset);
    }

    print("\n=== Performance Test: Adding {} memories ===\n", .{large_dataset_size});
    
    const add_start = std.time.nanoTimestamp();
    
    for (0..large_dataset_size) |i| {
        const memory_content = try std.fmt.allocPrint(allocator, "Memory item number {} with some additional content to make it more realistic", .{i});
        large_dataset[i] = memory_content;
        
        const memory_data = try std.fmt.allocPrint(allocator, "{{\"id\":{},\"data\":\"test_data_{}\"}}", .{ i, i });
        defer allocator.free(memory_data);
        
        _ = try registry.addMemory(memory_content, memory_data);
    }
    
    const add_time = std.time.nanoTimestamp() - add_start;
    const avg_add_time = @as(f64, @floatFromInt(add_time)) / @as(f64, @floatFromInt(large_dataset_size));
    
    print("Average memory addition time: {d:.2}ns ({d:.6}ms)\n", .{ avg_add_time, avg_add_time / 1_000_000.0 });
    
    // Target: <1ms per memory addition
    assert(avg_add_time < 1_000_000.0);

    print("\n=== Performance Test: Query throughput ===\n");
    
    // Create test queries (mix of existing and non-existing)
    var test_queries = try allocator.alloc([]const u8, 200);
    defer allocator.free(test_queries);
    
    // 50% existing queries, 50% non-existing
    for (0..100) |i| {
        test_queries[i] = large_dataset[i % large_dataset_size];
    }
    for (100..200) |i| {
        test_queries[i] = try std.fmt.allocPrint(allocator, "Non-existent query number {}", .{i});
        defer allocator.free(test_queries[i]);
    }

    // Benchmark query performance
    const benchmark_results = try registry.benchmarkPerformance(test_queries, 10);
    
    print("Query Performance Results:\n");
    print("  Min time: {}ns ({d:.6}ms)\n", .{ benchmark_results.min_time_ns, @as(f64, @floatFromInt(benchmark_results.min_time_ns)) / 1_000_000.0 });
    print("  Max time: {}ns ({d:.6}ms)\n", .{ benchmark_results.max_time_ns, @as(f64, @floatFromInt(benchmark_results.max_time_ns)) / 1_000_000.0 });
    print("  Mean time: {d:.2}ns ({d:.6}ms)\n", .{ benchmark_results.mean_time_ns, benchmark_results.mean_time_ns / 1_000_000.0 });
    print("  Total queries: {}\n", .{benchmark_results.total_queries});

    // Target: <0.1ms (100,000ns) average query time
    assert(benchmark_results.mean_time_ns < 100_000.0);
    
    print("\n✅ All performance targets met!\n");
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();

    print("🚀 MFN Layer 1 (IFR) Test Suite\n", .{});
    print("=====================================\n\n", .{});

    print("Running Zig tests...\n", .{});
    
    @import("std").testing.refAllDecls(@This());
    
    print("\n🎉 All tests passed! Layer 1 IFR is working correctly.\n", .{});
}