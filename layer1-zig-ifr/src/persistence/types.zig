/// Type definitions for persistence layer
const std = @import("std");

pub const MemoryId = u64;

pub const EntryType = enum {
    add_memory,
    update_memory,
    remove_memory,
    cleanup_connection,

    pub fn toString(self: EntryType) []const u8 {
        return switch (self) {
            .add_memory => "add_memory",
            .update_memory => "update_memory",
            .remove_memory => "remove_memory",
            .cleanup_connection => "cleanup_connection",
        };
    }

    pub fn fromString(s: []const u8) !EntryType {
        if (std.mem.eql(u8, s, "add_memory")) return .add_memory;
        if (std.mem.eql(u8, s, "update_memory")) return .update_memory;
        if (std.mem.eql(u8, s, "remove_memory")) return .remove_memory;
        if (std.mem.eql(u8, s, "cleanup_connection")) return .cleanup_connection;
        return error.InvalidEntryType;
    }
};

pub const AofEntry = struct {
    timestamp_ms: i64,
    entry_type: EntryType,
    data: []const u8, // JSON string

    pub fn init(allocator: std.mem.Allocator, entry_type: EntryType, data: []const u8) !AofEntry {
        const timestamp_ms = std.time.milliTimestamp();
        return AofEntry{
            .timestamp_ms = timestamp_ms,
            .entry_type = entry_type,
            .data = try allocator.dupe(u8, data),
        };
    }

    pub fn deinit(self: *AofEntry, allocator: std.mem.Allocator) void {
        allocator.free(self.data);
    }

    /// Serialize to JSON text (one line)
    pub fn toText(self: *const AofEntry, allocator: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(
            allocator,
            "{{\"timestamp_ms\":{d},\"entry_type\":\"{s}\",\"data\":{s}}}",
            .{ self.timestamp_ms, self.entry_type.toString(), self.data },
        );
    }
};

pub const AddMemoryData = struct {
    memory_id: MemoryId,
    content: []const u8,
    connection_id: ?[]const u8,

    pub fn toJson(self: *const AddMemoryData, allocator: std.mem.Allocator) ![]u8 {
        if (self.connection_id) |conn_id| {
            return std.fmt.allocPrint(
                allocator,
                "{{\"memory_id\":{d},\"content\":\"{s}\",\"connection_id\":\"{s}\"}}",
                .{ self.memory_id, self.content, conn_id },
            );
        } else {
            return std.fmt.allocPrint(
                allocator,
                "{{\"memory_id\":{d},\"content\":\"{s}\"}}",
                .{ self.memory_id, self.content },
            );
        }
    }
};

pub const UpdateMemoryData = struct {
    memory_id: MemoryId,
    activation_count: u64,
    strength: f32,

    pub fn toJson(self: *const UpdateMemoryData, allocator: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(
            allocator,
            "{{\"memory_id\":{d},\"activation_count\":{d},\"strength\":{d}}}",
            .{ self.memory_id, self.activation_count, self.strength },
        );
    }
};

pub const RemoveMemoryData = struct {
    memory_id: MemoryId,
    reason: []const u8,

    pub fn toJson(self: *const RemoveMemoryData, allocator: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(
            allocator,
            "{{\"memory_id\":{d},\"reason\":\"{s}\"}}",
            .{ self.memory_id, self.reason },
        );
    }
};

pub const CleanupConnectionData = struct {
    connection_id: []const u8,

    pub fn toJson(self: *const CleanupConnectionData, allocator: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(
            allocator,
            "{{\"connection_id\":\"{s}\"}}",
            .{self.connection_id},
        );
    }
};

pub const EntrySnapshot = struct {
    memory_id: MemoryId,
    content: []const u8,
    strength: f32,
    activation_count: u64,
    connection_id: ?[]const u8,
    created_timestamp_ms: i64,
    last_accessed_timestamp_ms: i64,

    pub fn deinit(self: *EntrySnapshot, allocator: std.mem.Allocator) void {
        allocator.free(self.content);
        if (self.connection_id) |conn_id| {
            allocator.free(conn_id);
        }
    }
};

pub const SnapshotMetadata = struct {
    snapshot_timestamp_ms: i64,
    entry_count: usize,
    format_version: u32,
};

pub const RecoveryStats = struct {
    snapshot_entry_count: usize,
    aof_entries_replayed: usize,
    aof_entries_skipped: usize,
    recovery_time_ms: i64,
    snapshot_age_secs: i64,
};

pub const AofStats = struct {
    entries_written: u64,
    bytes_written: u64,
};
