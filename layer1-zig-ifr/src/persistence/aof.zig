/// Append-Only File (AOF) writer for Layer 1
///
/// Architecture:
/// - Buffered writes to in-memory queue (non-blocking)
/// - Background thread flushes to disk with fsync interval
/// - Text-based format for debugging and recovery
/// - Zero read overhead (all reads from memory)
/// - Minimal write overhead (~250ns including queue push)
const std = @import("std");
const types = @import("types.zig");

const AofEntry = types.AofEntry;
const MemoryId = types.MemoryId;
const AddMemoryData = types.AddMemoryData;
const UpdateMemoryData = types.UpdateMemoryData;
const RemoveMemoryData = types.RemoveMemoryData;
const CleanupConnectionData = types.CleanupConnectionData;
const EntryType = types.EntryType;

/// Thread-safe queue for AOF entries
const AofQueue = struct {
    entries: std.ArrayList(*AofEntry),
    mutex: std.Thread.Mutex,
    allocator: std.mem.Allocator,

    pub fn init(allocator: std.mem.Allocator) AofQueue {
        return .{
            .entries = std.ArrayList(*AofEntry).init(allocator),
            .mutex = std.Thread.Mutex{},
            .allocator = allocator,
        };
    }

    pub fn deinit(self: *AofQueue) void {
        self.mutex.lock();
        defer self.mutex.unlock();

        for (self.entries.items) |entry| {
            entry.deinit(self.allocator);
            self.allocator.destroy(entry);
        }
        self.entries.deinit();
    }

    pub fn push(self: *AofQueue, entry: *AofEntry) !void {
        self.mutex.lock();
        defer self.mutex.unlock();
        try self.entries.append(entry);
    }

    pub fn popAll(self: *AofQueue) ![]*AofEntry {
        self.mutex.lock();
        defer self.mutex.unlock();

        if (self.entries.items.len == 0) {
            return &[_]*AofEntry{};
        }

        const items = try self.allocator.alloc(*AofEntry, self.entries.items.len);
        @memcpy(items, self.entries.items);
        self.entries.clearRetainingCapacity();
        return items;
    }
};

/// AOF handle for non-blocking writes
pub const AofHandle = struct {
    queue: *AofQueue,
    allocator: std.mem.Allocator,

    pub fn init(allocator: std.mem.Allocator, queue: *AofQueue) AofHandle {
        return .{
            .queue = queue,
            .allocator = allocator,
        };
    }

    /// Log add memory operation
    pub fn logAddMemory(
        self: *AofHandle,
        memory_id: MemoryId,
        content: []const u8,
        connection_id: ?[]const u8,
    ) !void {
        const data = AddMemoryData{
            .memory_id = memory_id,
            .content = content,
            .connection_id = connection_id,
        };

        const json = try data.toJson(self.allocator);
        defer self.allocator.free(json);

        const entry = try self.allocator.create(AofEntry);
        entry.* = try AofEntry.init(self.allocator, .add_memory, json);

        try self.queue.push(entry);
    }

    /// Log update memory operation
    pub fn logUpdateMemory(
        self: *AofHandle,
        memory_id: MemoryId,
        activation_count: u64,
        strength: f32,
    ) !void {
        const data = UpdateMemoryData{
            .memory_id = memory_id,
            .activation_count = activation_count,
            .strength = strength,
        };

        const json = try data.toJson(self.allocator);
        defer self.allocator.free(json);

        const entry = try self.allocator.create(AofEntry);
        entry.* = try AofEntry.init(self.allocator, .update_memory, json);

        try self.queue.push(entry);
    }

    /// Log remove memory operation
    pub fn logRemoveMemory(
        self: *AofHandle,
        memory_id: MemoryId,
        reason: []const u8,
    ) !void {
        const data = RemoveMemoryData{
            .memory_id = memory_id,
            .reason = reason,
        };

        const json = try data.toJson(self.allocator);
        defer self.allocator.free(json);

        const entry = try self.allocator.create(AofEntry);
        entry.* = try AofEntry.init(self.allocator, .remove_memory, json);

        try self.queue.push(entry);
    }

    /// Log connection cleanup operation
    pub fn logCleanupConnection(
        self: *AofHandle,
        connection_id: []const u8,
    ) !void {
        const data = CleanupConnectionData{
            .connection_id = connection_id,
        };

        const json = try data.toJson(self.allocator);
        defer self.allocator.free(json);

        const entry = try self.allocator.create(AofEntry);
        entry.* = try AofEntry.init(self.allocator, .cleanup_connection, json);

        try self.queue.push(entry);
    }
};

/// Background AOF writer with buffered I/O and periodic fsync
pub const AofWriter = struct {
    file: std.fs.File,
    buffered_writer: std.io.BufferedWriter(4096, std.fs.File.Writer),
    queue: *AofQueue,
    fsync_interval_ns: u64,
    last_fsync_ns: i128,
    entries_written: u64,
    bytes_written: u64,
    allocator: std.mem.Allocator,
    should_stop: std.atomic.Value(bool),

    pub fn init(
        path: []const u8,
        queue: *AofQueue,
        fsync_interval_ms: u64,
        allocator: std.mem.Allocator,
    ) !AofWriter {
        // Ensure parent directory exists
        const dir_path = std.fs.path.dirname(path);
        if (dir_path) |dir| {
            try std.fs.cwd().makePath(dir);
        }

        // Open file in append mode
        const file = try std.fs.cwd().createFile(path, .{
            .truncate = false,
            .read = false,
        });

        // Seek to end for append mode
        try file.seekFromEnd(0);

        return .{
            .file = file,
            .buffered_writer = std.io.bufferedWriter(file.writer()),
            .queue = queue,
            .fsync_interval_ns = fsync_interval_ms * std.time.ns_per_ms,
            .last_fsync_ns = std.time.nanoTimestamp(),
            .entries_written = 0,
            .bytes_written = 0,
            .allocator = allocator,
            .should_stop = std.atomic.Value(bool).init(false),
        };
    }

    pub fn deinit(self: *AofWriter) void {
        _ = self.flush() catch {};
        self.file.close();
    }

    /// Run the background AOF writer loop
    pub fn run(self: *AofWriter) !void {
        while (!self.should_stop.load(.acquire)) {
            // Pop all pending entries
            const entries = try self.queue.popAll();
            defer self.allocator.free(entries);

            // Write all entries
            for (entries) |entry| {
                try self.writeEntry(entry);
                entry.deinit(self.allocator);
                self.allocator.destroy(entry);
            }

            // Check if we need to fsync
            const now_ns = std.time.nanoTimestamp();
            if (now_ns - self.last_fsync_ns >= self.fsync_interval_ns) {
                try self.flush();
            }

            // Sleep briefly to avoid busy-wait
            std.time.sleep(10 * std.time.ns_per_ms); // 10ms
        }

        // Final flush on exit
        try self.flush();
    }

    /// Write single entry to buffer
    fn writeEntry(self: *AofWriter, entry: *const AofEntry) !void {
        const text = try entry.toText(self.allocator);
        defer self.allocator.free(text);

        const writer = self.buffered_writer.writer();
        try writer.writeAll(text);
        try writer.writeByte('\n');

        self.entries_written += 1;
        self.bytes_written += text.len + 1; // +1 for newline
    }

    /// Flush buffer and fsync to disk
    fn flush(self: *AofWriter) !void {
        try self.buffered_writer.flush();
        try self.file.sync();
        self.last_fsync_ns = std.time.nanoTimestamp();
    }

    /// Stop the writer gracefully
    pub fn stop(self: *AofWriter) void {
        self.should_stop.store(true, .release);
    }

    /// Get statistics
    pub fn stats(self: *const AofWriter) types.AofStats {
        return .{
            .entries_written = self.entries_written,
            .bytes_written = self.bytes_written,
        };
    }
};
