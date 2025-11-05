const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // Test executable
    const test_exe = b.addExecutable(.{
        .name = "ifr_test",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/test.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });

    // Benchmark executable
    const benchmark_exe = b.addExecutable(.{
        .name = "ifr_benchmark",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/benchmark.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });

    // Socket server executable
    const socket_server_exe = b.addExecutable(.{
        .name = "ifr_socket_server",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/socket_main.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });
    socket_server_exe.linkLibC(); // For signal handling

    // Socket client test executable
    const socket_client_test_exe = b.addExecutable(.{
        .name = "ifr_socket_client_test",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/socket_client_test.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });

    // Install executables
    b.installArtifact(test_exe);
    b.installArtifact(benchmark_exe);
    b.installArtifact(socket_server_exe);
    b.installArtifact(socket_client_test_exe);

    // Install C header file
    const header_install = b.addInstallFile(b.path("include/mfn_layer1_ifr.h"), "include/mfn_layer1_ifr.h");
    b.getInstallStep().dependOn(&header_install.step);

    // Run steps
    const run_test = b.addRunArtifact(test_exe);
    const run_benchmark = b.addRunArtifact(benchmark_exe);
    const run_socket_server = b.addRunArtifact(socket_server_exe);
    const run_socket_client_test = b.addRunArtifact(socket_client_test_exe);

    const test_step = b.step("test", "Run Layer 1 tests");
    test_step.dependOn(&run_test.step);

    const benchmark_step = b.step("benchmark", "Run Layer 1 benchmarks");
    benchmark_step.dependOn(&run_benchmark.step);

    const server_step = b.step("server", "Run Layer 1 socket server");
    server_step.dependOn(&run_socket_server.step);

    const client_test_step = b.step("client-test", "Run Layer 1 socket client tests");
    client_test_step.dependOn(&run_socket_client_test.step);
}
