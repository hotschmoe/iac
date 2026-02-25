// src/shared/hex.zig
// Axial hex coordinates with cube distance calculations.
// Flat-top orientation. Stored as (q, r), cube s = -q - r derived.
//
// Hex layout (flat-top, axial):
//
//        -r
//    NW  /  NE
//      \/
//  W ──    ── E      +q →
//      /\
//    SW  \  SE
//        +r

const std = @import("std");

/// Hex coordinate in axial form. 4 bytes total, fits in a register.
/// Use `toKey()` for hashing/map lookups.
pub const Hex = struct {
    q: i16,
    r: i16,

    pub const ORIGIN = Hex{ .q = 0, .r = 0 };

    /// Cube coordinate s, derived from q and r.
    pub inline fn s(self: Hex) i16 {
        return -self.q - self.r;
    }

    /// Cube distance between two hexes.
    pub fn distance(a: Hex, b: Hex) u16 {
        const dq = @as(i32, a.q) - @as(i32, b.q);
        const dr = @as(i32, a.r) - @as(i32, b.r);
        const ds = @as(i32, a.s()) - @as(i32, b.s());
        return @intCast(@max(@max(absI32(dq), absI32(dr)), absI32(ds)));
    }

    /// Distance from the origin (0, 0).
    pub fn distFromOrigin(self: Hex) u16 {
        return distance(self, ORIGIN);
    }

    /// Add a direction vector to this hex.
    pub fn add(self: Hex, other: Hex) Hex {
        return .{
            .q = self.q + other.q,
            .r = self.r + other.r,
        };
    }

    /// Subtract a hex from this hex.
    pub fn sub(self: Hex, other: Hex) Hex {
        return .{
            .q = self.q - other.q,
            .r = self.r - other.r,
        };
    }

    /// Get neighbor in the given direction.
    pub fn neighbor(self: Hex, dir: HexDirection) Hex {
        return self.add(dir.toVec());
    }

    /// Get all 6 neighbor coordinates.
    pub fn neighbors(self: Hex) [6]Hex {
        var result: [6]Hex = undefined;
        for (HexDirection.ALL, 0..) |dir, i| {
            result[i] = self.neighbor(dir);
        }
        return result;
    }

    /// Pack into a u32 for use as hash map key.
    pub fn toKey(self: Hex) u32 {
        return @as(u32, @as(u16, @bitCast(self.q))) |
            (@as(u32, @as(u16, @bitCast(self.r))) << 16);
    }

    /// Unpack from a u32 key.
    pub fn fromKey(key: u32) Hex {
        return .{
            .q = @bitCast(@as(u16, @truncate(key))),
            .r = @bitCast(@as(u16, @truncate(key >> 16))),
        };
    }

    pub fn eql(a: Hex, b: Hex) bool {
        return a.q == b.q and a.r == b.r;
    }

    pub fn format(self: Hex, comptime _: []const u8, _: std.fmt.FormatOptions, writer: anytype) !void {
        try writer.print("[{d},{d}]", .{ self.q, self.r });
    }
};

/// The 6 hex directions (flat-top orientation).
pub const HexDirection = enum(u3) {
    east = 0,
    north_east = 1,
    north_west = 2,
    west = 3,
    south_west = 4,
    south_east = 5,

    pub const ALL = [_]HexDirection{ .east, .north_east, .north_west, .west, .south_west, .south_east };

    /// Direction vector in axial coordinates.
    pub fn toVec(self: HexDirection) Hex {
        return switch (self) {
            .east => .{ .q = 1, .r = 0 },
            .north_east => .{ .q = 1, .r = -1 },
            .north_west => .{ .q = 0, .r = -1 },
            .west => .{ .q = -1, .r = 0 },
            .south_west => .{ .q = -1, .r = 1 },
            .south_east => .{ .q = 0, .r = 1 },
        };
    }

    /// Opposite direction.
    pub fn opposite(self: HexDirection) HexDirection {
        return @enumFromInt(@as(u3, @intFromEnum(self)) +% 3);
    }

    /// Short label for display.
    pub fn label(self: HexDirection) []const u8 {
        return switch (self) {
            .east => "E",
            .north_east => "NE",
            .north_west => "NW",
            .west => "W",
            .south_west => "SW",
            .south_east => "SE",
        };
    }
};

/// Hex hash context for use with std.HashMap.
pub const HexContext = struct {
    pub fn hash(_: @This(), h: Hex) u64 {
        return @as(u64, h.toKey());
    }
    pub fn eql(_: @This(), a: Hex, b: Hex) bool {
        return a.eql(b);
    }
};

/// Iterate over all hexes in a ring at a given radius from center.
pub fn hexRing(center: Hex, radius: u16) HexRingIterator {
    if (radius == 0) {
        return .{
            .current = center,
            .radius = 0,
            .side = 0,
            .step = 0,
            .started = false,
        };
    }
    // Start at center + radius * direction[4] (SW), then walk each side
    var start = center;
    var i: u16 = 0;
    while (i < radius) : (i += 1) {
        start = start.neighbor(.south_west);
    }
    return .{
        .current = start,
        .radius = radius,
        .side = 0,
        .step = 0,
        .started = false,
    };
}

pub const HexRingIterator = struct {
    current: Hex,
    radius: u16,
    side: u3,
    step: u16,
    started: bool,

    pub fn next(self: *HexRingIterator) ?Hex {
        if (self.radius == 0) {
            if (!self.started) {
                self.started = true;
                return self.current;
            }
            return null;
        }

        if (!self.started) {
            self.started = true;
            return self.current;
        }

        // Walk directions: E, NE, NW, W, SW, SE
        const walk_dirs = [_]HexDirection{ .east, .north_east, .north_west, .west, .south_west, .south_east };

        self.step += 1;
        if (self.step >= self.radius) {
            self.step = 0;
            self.side += 1;
            if (self.side >= 6) return null;
        }

        self.current = self.current.neighbor(walk_dirs[self.side]);
        return self.current;
    }
};

/// Iterate over all hexes in a spiral from center out to max_radius.
pub fn hexSpiral(center: Hex, max_radius: u16) HexSpiralIterator {
    return .{
        .center = center,
        .max_radius = max_radius,
        .current_radius = 0,
        .ring_iter = hexRing(center, 0),
    };
}

pub const HexSpiralIterator = struct {
    center: Hex,
    max_radius: u16,
    current_radius: u16,
    ring_iter: HexRingIterator,

    pub fn next(self: *HexSpiralIterator) ?Hex {
        if (self.ring_iter.next()) |h| {
            return h;
        }
        self.current_radius += 1;
        if (self.current_radius > self.max_radius) return null;
        self.ring_iter = hexRing(self.center, self.current_radius);
        return self.ring_iter.next();
    }
};

fn absI32(x: i32) i32 {
    return if (x < 0) -x else x;
}

// ── Tests ──────────────────────────────────────────────────────────

test "hex distance" {
    const a = Hex{ .q = 0, .r = 0 };
    const b = Hex{ .q = 3, .r = -1 };
    try std.testing.expectEqual(@as(u16, 3), Hex.distance(a, b));
}

test "hex distance symmetric" {
    const a = Hex{ .q = 2, .r = -5 };
    const b = Hex{ .q = -1, .r = 3 };
    try std.testing.expectEqual(Hex.distance(a, b), Hex.distance(b, a));
}

test "hex neighbors count" {
    const center = Hex{ .q = 5, .r = -3 };
    const nbrs = center.neighbors();
    for (nbrs) |n| {
        try std.testing.expectEqual(@as(u16, 1), Hex.distance(center, n));
    }
}

test "hex key roundtrip" {
    const h = Hex{ .q = -42, .r = 127 };
    const key = h.toKey();
    const h2 = Hex.fromKey(key);
    try std.testing.expect(h.eql(h2));
}

test "hex direction opposite" {
    try std.testing.expectEqual(HexDirection.west, HexDirection.east.opposite());
    try std.testing.expectEqual(HexDirection.south_west, HexDirection.north_east.opposite());
}

test "hex ring radius 1" {
    var iter = hexRing(Hex.ORIGIN, 1);
    var count: u16 = 0;
    while (iter.next()) |_| {
        count += 1;
    }
    try std.testing.expectEqual(@as(u16, 6), count);
}

test "hex ring radius 2" {
    var iter = hexRing(Hex.ORIGIN, 2);
    var count: u16 = 0;
    while (iter.next()) |_| {
        count += 1;
    }
    try std.testing.expectEqual(@as(u16, 12), count);
}

test "hex spiral radius 2" {
    var iter = hexSpiral(Hex.ORIGIN, 2);
    var count: u16 = 0;
    while (iter.next()) |_| {
        count += 1;
    }
    // 1 + 6 + 12 = 19
    try std.testing.expectEqual(@as(u16, 19), count);
}
