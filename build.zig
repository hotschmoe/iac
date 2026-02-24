const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // ── Dependencies ───────────────────────────────────────────────
    const zqlite_dep = b.dependency("zqlite", .{ .target = target, .optimize = optimize });
    const zqlite_mod = zqlite_dep.module("zqlite");

    const webzocket_dep = b.dependency("webzocket", .{ .target = target, .optimize = optimize });
    const webzocket_mod = webzocket_dep.module("webzocket");

    const zithril_dep = b.dependency("zithril", .{ .target = target, .optimize = optimize });
    const zithril_mod = zithril_dep.module("zithril");

    // ── Shared module (protocol, hex math, constants) ──────────────
    const shared_mod = b.addModule("shared", .{
        .root_source_file = b.path("src/shared/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    // ── Server executable ──────────────────────────────────────────
    const server_exe = b.addExecutable(.{
        .name = "iac-server",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/server/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "shared", .module = shared_mod },
                .{ .name = "zqlite", .module = zqlite_mod },
                .{ .name = "webzocket", .module = webzocket_mod },
            },
        }),
    });
    b.installArtifact(server_exe);

    // ── Client executable ──────────────────────────────────────────
    const client_exe = b.addExecutable(.{
        .name = "iac-client",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/client/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "shared", .module = shared_mod },
                .{ .name = "zithril", .module = zithril_mod },
                .{ .name = "webzocket", .module = webzocket_mod },
            },
        }),
    });
    b.installArtifact(client_exe);

    // ── Run commands ───────────────────────────────────────────────
    const run_server = b.addRunArtifact(server_exe);
    run_server.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_server.addArgs(args);

    const run_server_step = b.step("server", "Run the IAC server");
    run_server_step.dependOn(&run_server.step);

    const run_client = b.addRunArtifact(client_exe);
    run_client.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_client.addArgs(args);

    const run_client_step = b.step("client", "Run the IAC TUI client");
    run_client_step.dependOn(&run_client.step);

    // ── Tests ──────────────────────────────────────────────────────
    const shared_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/shared/root.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });

    const server_tests = b.addTest(.{
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/server/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "shared", .module = shared_mod },
            },
        }),
    });

    const test_step = b.step("test", "Run all tests");
    test_step.dependOn(&b.addRunArtifact(shared_tests).step);
    test_step.dependOn(&b.addRunArtifact(server_tests).step);
}
