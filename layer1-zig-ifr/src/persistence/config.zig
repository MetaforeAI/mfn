/// Persistence configuration for Layer 1 (Zig IFR)
///
/// Architecture:
/// - Memory-first operations (minimal overhead)
/// - Asynchronous AOF writes (background thread)
/// - LMDB snapshots every 5 minutes
/// - Recovery: Load snapshot + replay AOF (<200ms)
const std = @import("std");

pub const Config = struct {
    /// Base directory for persistence files
    data_dir: []const u8,

    /// Pool ID for multi-tenant isolation
    pool_id: []const u8,

    /// Fsync interval in milliseconds (default: 1000)
    fsync_interval_ms: u64,

    /// Snapshot interval in seconds (default: 300)
    snapshot_interval_secs: u64,

    /// AOF buffer size in bytes (default: 64KB)
    aof_buffer_size: usize,

    allocator: std.mem.Allocator,

    pub fn default(allocator: std.mem.Allocator) !Config {
        return Config{
            .data_dir = blk: {
                const env_val = std.posix.getenv("MFN_DATA_DIR");
                if (env_val) |base_dir| {
                    break :blk try std.fmt.allocPrint(allocator, "{s}/layer1_ifr", .{base_dir});
                } else {
                    break :blk try allocator.dupe(u8, "./data/mfn/memory/layer1_ifr");
                }
            },
            .pool_id = try allocator.dupe(u8, "default"),
            .fsync_interval_ms = 1000,
            .snapshot_interval_secs = 300,
            .aof_buffer_size = 64 * 1024,
            .allocator = allocator,
        };
    }

    pub fn deinit(self: *Config) void {
        self.allocator.free(self.data_dir);
        self.allocator.free(self.pool_id);
    }

    pub fn aofPath(self: *const Config, allocator: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(
            allocator,
            "{s}/pool_{s}.aof",
            .{ self.data_dir, self.pool_id },
        );
    }

    pub fn snapshotPath(self: *const Config, allocator: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(
            allocator,
            "{s}/pool_{s}.snapshot",
            .{ self.data_dir, self.pool_id },
        );
    }

    pub fn metaPath(self: *const Config, allocator: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(
            allocator,
            "{s}/pool_{s}.meta",
            .{ self.data_dir, self.pool_id },
        );
    }
};
