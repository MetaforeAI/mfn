// Comprehensive benchmark suite for Layer 1 Immediate Flow Registry
// Tests performance against target metrics and compares with theoretical limits

const std = @import("std");
const print = std.debug.print;
const ifr = @import("ifr.zig");

const IFR = ifr.ImmediateFlowRegistry;

// Benchmark configuration
const BENCHMARK_CONFIG = struct {
    const small_dataset: u32 = 1_000;
    const medium_dataset: u32 = 10_000;  
    const large_dataset: u32 = 100_000;
    const benchmark_iterations: u32 = 1000;
    const query_batch_size: u32 = 1000;
};

// Performance targets from specification
const PERFORMANCE_TARGETS = struct {
    const max_query_time_ns: u64 = 100_000; // 0.1ms
    const max_add_time_ns: u64 = 1_000_000; // 1ms
    const min_throughput_qps: f64 = 10_000.0; // 10k queries per second
    const max_memory_overhead: f32 = 3.0; // 3x raw memory size
};

fn generateDataset(allocator: std.mem.Allocator, size: u32, prefix: []const u8) ![][]const u8 {
    var dataset = try allocator.alloc([]const u8, size);
    
    for (0..size) |i| {
        dataset[i] = try std.fmt.allocPrint(allocator, 
            "{s} item {} - additional content to simulate realistic memory sizes with various data patterns", 
            .{ prefix, i });
    }
    
    return dataset;
}

fn freeDataset(allocator: std.mem.Allocator, dataset: [][]const u8) void {
    for (dataset) |item| {
        allocator.free(item);
    }
    allocator.free(dataset);
}

fn benchmarkMemoryAddition(allocator: std.mem.Allocator, dataset_size: u32) !void {
    print("\n📊 Memory Addition Benchmark (Dataset Size: {})\n", .{dataset_size});
    print("=" ** 50 ++ "\n", .{});

    var registry = try IFR.init(allocator, dataset_size * 10, 0.001, dataset_size / 10);
    defer registry.deinit();

    const dataset = try generateDataset(allocator, dataset_size, "benchmark_memory");
    defer freeDataset(allocator, dataset);

    var add_times = try allocator.alloc(u64, dataset_size);
    defer allocator.free(add_times);

    print("Adding {} memories...\n", .{dataset_size});

    for (dataset, 0..) |memory_content, i| {
        const memory_data = try std.fmt.allocPrint(allocator, "{{\"id\":{},\"benchmark\":true}}", .{i});
        defer allocator.free(memory_data);

        const start_time = std.time.nanoTimestamp();
        _ = try registry.addMemory(memory_content, memory_data);
        const end_time = std.time.nanoTimestamp();

        add_times[i] = @as(u64, @intCast(end_time - start_time));
    }

    // Sort times for percentile calculation
    std.sort.block(u64, add_times, {}, comptime std.sort.asc(u64));

    const total_add_time: u64 = blk: {
        var sum: u64 = 0;
        for (add_times) |time| {
            sum += time;
        }
        break :blk sum;
    };

    const mean_add_time = @as(f64, @floatFromInt(total_add_time)) / @as(f64, @floatFromInt(dataset_size));
    const p50_add_time = add_times[dataset_size / 2];
    const p95_add_time = add_times[(dataset_size * 95) / 100];
    const p99_add_time = add_times[(dataset_size * 99) / 100];

    print("Memory Addition Results:\n", .{});
    print("  Mean time:    {d:.2}ns ({d:.6}ms)\n", .{ mean_add_time, mean_add_time / 1_000_000.0 });
    print("  P50 time:     {}ns ({d:.6}ms)\n", .{ p50_add_time, @as(f64, @floatFromInt(p50_add_time)) / 1_000_000.0 });
    print("  P95 time:     {}ns ({d:.6}ms)\n", .{ p95_add_time, @as(f64, @floatFromInt(p95_add_time)) / 1_000_000.0 });
    print("  P99 time:     {}ns ({d:.6}ms)\n", .{ p99_add_time, @as(f64, @floatFromInt(p99_add_time)) / 1_000_000.0 });
    print("  Min time:     {}ns ({d:.6}ms)\n", .{ add_times[0], @as(f64, @floatFromInt(add_times[0])) / 1_000_000.0 });
    print("  Max time:     {}ns ({d:.6}ms)\n", .{ add_times[dataset_size - 1], @as(f64, @floatFromInt(add_times[dataset_size - 1])) / 1_000_000.0 });

    // Check against targets
    const target_met = mean_add_time < @as(f64, @floatFromInt(PERFORMANCE_TARGETS.max_add_time_ns));
    print("  Target (<1ms): {} {s}\n", .{ target_met, if (target_met) "✅ PASS" else "❌ FAIL" });
}

fn benchmarkQueryPerformance(allocator: std.mem.Allocator, dataset_size: u32) !void {
    print("\n🔍 Query Performance Benchmark (Dataset Size: {})\n", .{dataset_size});
    print("=" ** 50 ++ "\n", .{});

    var registry = try IFR.init(allocator, dataset_size * 10, 0.001, dataset_size / 10);
    defer registry.deinit();

    // Populate registry
    const dataset = try generateDataset(allocator, dataset_size, "query_benchmark");
    defer freeDataset(allocator, dataset);

    print("Populating registry with {} memories...\n", .{dataset_size});
    for (dataset, 0..) |memory_content, i| {
        const memory_data = try std.fmt.allocPrint(allocator, "{{\"id\":{}}}", .{i});
        defer allocator.free(memory_data);
        _ = try registry.addMemory(memory_content, memory_data);
    }

    // Create mixed query set (80% hits, 20% misses)
    const query_count = BENCHMARK_CONFIG.query_batch_size;
    var queries = try allocator.alloc([]const u8, query_count);
    defer {
        for (queries[dataset_size..]) |query| {
            allocator.free(query);
        }
        allocator.free(queries);
    }

    // 80% existing queries
    const hit_count = (query_count * 8) / 10;
    for (0..hit_count) |i| {
        queries[i] = dataset[i % dataset_size];
    }

    // 20% non-existing queries  
    for (hit_count..query_count) |i| {
        queries[i] = try std.fmt.allocPrint(allocator, "non_existent_query_{}", .{i});
    }

    print("Running {} query iterations with {} queries each...\n", .{ BENCHMARK_CONFIG.benchmark_iterations, query_count });

    // Benchmark queries
    var all_query_times = try allocator.alloc(u64, query_count * BENCHMARK_CONFIG.benchmark_iterations);
    defer allocator.free(all_query_times);

    var time_index: u64 = 0;

    const total_start = std.time.nanoTimestamp();

    for (0..BENCHMARK_CONFIG.benchmark_iterations) |_| {
        for (queries) |query| {
            const result = registry.query(query);
            all_query_times[time_index] = result.processing_time_ns;
            time_index += 1;
        }
    }

    const total_end = std.time.nanoTimestamp();
    const total_benchmark_time = total_end - total_start;

    // Sort times for statistics
    std.sort.block(u64, all_query_times, {}, comptime std.sort.asc(u64));

    const total_queries = query_count * BENCHMARK_CONFIG.benchmark_iterations;
    const total_query_time: u64 = blk: {
        var sum: u64 = 0;
        for (all_query_times) |time| {
            sum += time;
        }
        break :blk sum;
    };

    const mean_query_time = @as(f64, @floatFromInt(total_query_time)) / @as(f64, @floatFromInt(total_queries));
    const p50_query_time = all_query_times[total_queries / 2];
    const p95_query_time = all_query_times[(total_queries * 95) / 100];
    const p99_query_time = all_query_times[(total_queries * 99) / 100];

    // Calculate throughput
    const throughput_qps = @as(f64, @floatFromInt(total_queries)) / (@as(f64, @floatFromInt(total_benchmark_time)) / 1_000_000_000.0);

    
    print("  Total queries: {}\n", .{total_queries});
    print("  Mean time:     {d:.2}ns ({d:.6}ms)\n", .{ mean_query_time, mean_query_time / 1_000_000.0 });
    print("  P50 time:      {}ns ({d:.6}ms)\n", .{ p50_query_time, @as(f64, @floatFromInt(p50_query_time)) / 1_000_000.0 });
    print("  P95 time:      {}ns ({d:.6}ms)\n", .{ p95_query_time, @as(f64, @floatFromInt(p95_query_time)) / 1_000_000.0 });
    print("  P99 time:      {}ns ({d:.6}ms)\n", .{ p99_query_time, @as(f64, @floatFromInt(p99_query_time)) / 1_000_000.0 });
    print("  Min time:      {}ns ({d:.6}ms)\n", .{ all_query_times[0], @as(f64, @floatFromInt(all_query_times[0])) / 1_000_000.0 });
    print("  Max time:      {}ns ({d:.6}ms)\n", .{ all_query_times[total_queries - 1], @as(f64, @floatFromInt(all_query_times[total_queries - 1])) / 1_000_000.0 });
    print("  Throughput:    {d:.0} queries/second\n", .{throughput_qps});

    // Check against targets
    const latency_target_met = mean_query_time < @as(f64, @floatFromInt(PERFORMANCE_TARGETS.max_query_time_ns));
    const throughput_target_met = throughput_qps >= PERFORMANCE_TARGETS.min_throughput_qps;

    print("  Latency (<0.1ms): {} {s}\n", .{ latency_target_met, if (latency_target_met) "✅ PASS" else "❌ FAIL" });
    print("  Throughput (>10k qps): {} {s}\n", .{ throughput_target_met, if (throughput_target_met) "✅ PASS" else "❌ FAIL" });

    // Print statistics
    const stats = registry.getPerformanceStats();
    
    print("  Hit rate: {d:.1}\n", .{stats.hit_rate * 100.0});
    print("  False positive rate: {d:.2}\n", .{stats.false_positive_rate * 100.0});
}

fn benchmarkMemoryEfficiency(allocator: std.mem.Allocator, dataset_size: u32) !void {
    print("\n💾 Memory Efficiency Benchmark (Dataset Size: {})\n", .{dataset_size});
    print("=" ** 50 ++ "\n", .{});

    var registry = try IFR.init(allocator, dataset_size * 10, 0.001, dataset_size / 10);
    defer registry.deinit();

    const dataset = try generateDataset(allocator, dataset_size, "memory_efficiency");
    defer freeDataset(allocator, dataset);

    // Calculate raw data size
    var raw_data_size: u64 = 0;
    for (dataset) |memory| {
        raw_data_size += memory.len;
    }

    // Populate registry
    for (dataset, 0..) |memory_content, i| {
        const memory_data = try std.fmt.allocPrint(allocator, "{{\"id\":{},\"timestamp\":{}}}", .{ i, std.time.timestamp() });
        defer allocator.free(memory_data);
        _ = try registry.addMemory(memory_content, memory_data);
        raw_data_size += memory_data.len;
    }

    // Estimate system memory usage (rough approximation)
    const bloom_stats = registry.bloom_filter.getStats();
    const hash_stats = registry.hash_table.getStats();

    const bloom_size = registry.bloom_filter.bits.len * @sizeOf(u64);
    const hash_table_size = hash_stats.size * @sizeOf(ifr.HashEntry);
    const estimated_total_size = bloom_size + hash_table_size;

    const memory_overhead = @as(f32, @floatFromInt(estimated_total_size)) / @as(f32, @floatFromInt(raw_data_size));

    
    print("  Raw data size:      {} bytes ({d:.2} MB)\n", .{ raw_data_size, @as(f64, @floatFromInt(raw_data_size)) / 1_048_576.0 });
    print("  Bloom filter size:  {} bytes ({d:.2} MB)\n", .{ bloom_size, @as(f64, @floatFromInt(bloom_size)) / 1_048_576.0 });
    print("  Hash table size:    {} bytes ({d:.2} MB)\n", .{ hash_table_size, @as(f64, @floatFromInt(hash_table_size)) / 1_048_576.0 });
    print("  Total system size:  {} bytes ({d:.2} MB)\n", .{ estimated_total_size, @as(f64, @floatFromInt(estimated_total_size)) / 1_048_576.0 });
    print("  Memory overhead:    {d:.2}x\n", .{memory_overhead});

    const efficiency_target_met = memory_overhead <= PERFORMANCE_TARGETS.max_memory_overhead;
    print("  Efficiency (<3x): {} {s}\n", .{ efficiency_target_met, if (efficiency_target_met) "✅ PASS" else "❌ FAIL" });

    
    print("  Estimated error rate: {d:.6}\n", .{bloom_stats.estimated_error_rate});
    print("  Hash functions: {}\n", .{bloom_stats.hash_functions});

    
    print("  Load factor: {d:.3}\n", .{hash_stats.load_factor});
    print("  Collision rate: {d:.3}\n", .{hash_stats.collision_rate});
}

fn runScalabilityTest(allocator: std.mem.Allocator) !void {
    
    print("=" ** 50 ++ "\n", .{});

    const dataset_sizes = [_]u32{ 1_000, 5_000, 10_000, 50_000, 100_000 };

    
    
    

    for (dataset_sizes) |size| {
        var registry = try IFR.init(allocator, size * 10, 0.001, size / 10);
        defer registry.deinit();

        // Quick population
        for (0..size) |i| {
            const content = try std.fmt.allocPrint(allocator, "scalability_test_item_{}", .{i});
            defer allocator.free(content);
            const data = try std.fmt.allocPrint(allocator, "{{\"id\":{}}}", .{i});
            defer allocator.free(data);
            _ = try registry.addMemory(content, data);
        }

        // Quick benchmark
        const test_queries = try allocator.alloc([]const u8, 100);
        defer {
            for (test_queries[size..]) |query| {
                allocator.free(query);
            }
            allocator.free(test_queries);
        }

        // Mix of existing and non-existing queries
        for (0..@min(size, 80)) |i| {
            test_queries[i] = try std.fmt.allocPrint(allocator, "scalability_test_item_{}", .{i});
        }
        for (@min(size, 80)..100) |i| {
            test_queries[i] = try std.fmt.allocPrint(allocator, "non_existent_{}", .{i});
        }

        const start_time = std.time.nanoTimestamp();
        var total_query_time: u64 = 0;

        for (0..10) |_| { // 10 iterations
            for (test_queries[0..@min(100, size)]) |query| {
                const result = registry.query(query);
                total_query_time += result.processing_time_ns;
            }
        }

        const end_time = std.time.nanoTimestamp();
        const benchmark_duration = end_time - start_time;

        const total_queries: u64 = @min(100, size) * 10;
        const mean_time = @as(f64, @floatFromInt(total_query_time)) / @as(f64, @floatFromInt(total_queries));
        const throughput = @as(f64, @floatFromInt(total_queries)) / (@as(f64, @floatFromInt(benchmark_duration)) / 1_000_000_000.0);

        // Rough memory estimate
        const hash_stats = registry.hash_table.getStats();
        const estimated_memory_mb = (@as(f64, @floatFromInt(hash_stats.size)) * @sizeOf(ifr.HashEntry)) / 1_048_576.0;

        print("{:>11} | {:>13.0}ns | {:>14.0} | {:>9.2}\n", .{ size, mean_time, throughput, estimated_memory_mb });

        // Clean up queries
        for (test_queries[@min(size, 80)..100]) |query| {
            allocator.free(query);
        }
        for (test_queries[0..@min(size, 80)]) |query| {
            allocator.free(query);
        }
    }
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    print("🚀 MFN Layer 1 (IFR) Comprehensive Benchmark Suite\n", .{});
    print("=" ** 60 ++ "\n", .{});
    print("Performance Targets:\n", .{});
    print("  Query Latency: <0.1ms\n", .{});
    print("  Memory Addition: <1ms\n", .{});
    print("  Throughput: >10,000 QPS\n", .{});
    print("  Memory Efficiency: <3x overhead\n\n", .{});

    // Run benchmarks for different dataset sizes
    const dataset_sizes = [_]u32{
        BENCHMARK_CONFIG.small_dataset,
        BENCHMARK_CONFIG.medium_dataset,
        BENCHMARK_CONFIG.large_dataset,
    };

    for (dataset_sizes) |size| {
        try benchmarkMemoryAddition(allocator, size);
        try benchmarkQueryPerformance(allocator, size);
        try benchmarkMemoryEfficiency(allocator, size);
        print("\n" ++ "=" ** 60 ++ "\n", .{});
    }

    try runScalabilityTest(allocator);

    print("\n🎉 Benchmark suite completed!\n", .{});
    print("Layer 1 IFR is optimized for ultra-fast exact matching with minimal overhead.\n", .{});
}