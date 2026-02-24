// src/shared/root.zig
// Shared types and logic used by both server and client.

pub const hex = @import("hex.zig");
pub const protocol = @import("protocol.zig");
pub const constants = @import("constants.zig");
pub const world = @import("world.zig");

// Re-export most-used types at top level for convenience
pub const Hex = hex.Hex;
pub const HexDirection = hex.HexDirection;
pub const Resources = protocol.Resources;
pub const GameState = protocol.GameState;

test {
    @import("std").testing.refAllDecls(@This());
}
