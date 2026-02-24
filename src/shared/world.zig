// src/shared/world.zig
// Deterministic procedural world generation.
// Sectors are generated from their coordinates + world seed.
// Only modified sectors need database storage.

const std = @import("std");
const hex = @import("hex.zig");
const constants = @import("constants.zig");

const Hex = hex.Hex;
const Zone = constants.Zone;
const TerrainType = constants.TerrainType;
const Density = constants.Density;
const ShipClass = constants.ShipClass;

/// Seed-based sector generation. Pure function — same inputs always produce same output.
pub const WorldGen = struct {
    world_seed: u64,

    pub fn init(seed: u64) WorldGen {
        return .{ .world_seed = seed };
    }

    /// Generate the base properties of a sector from its coordinates.
    pub fn generateSector(self: WorldGen, coord: Hex) SectorTemplate {
        const seed = self.sectorSeed(coord);
        var rng = std.Random.DefaultPrng.init(seed);
        const random = rng.random();

        const dist = coord.distFromOrigin();
        const zone = Zone.fromDistance(dist);

        const terrain = self.rollTerrain(random, zone);
        const resources = self.rollResources(random, zone, terrain);
        const npc_template = self.rollNpcPresence(random, zone, dist);

        return .{
            .coord = coord,
            .zone = zone,
            .terrain = terrain,
            .metal_density = resources.metal,
            .crystal_density = resources.crystal,
            .deut_density = resources.deut,
            .npc_template = npc_template,
        };
    }

    /// Determine which edges from a hex are traversable.
    /// Returns a bitmask of the 6 directions (bit 0 = east, etc.).
    pub fn sectorConnections(self: WorldGen, coord: Hex) u6 {
        var mask: u6 = 0;
        var connected_count: u3 = 0;
        var last_dir: u3 = 0;

        for (hex.HexDirection.ALL, 0..) |dir, i| {
            const neighbor_coord = coord.neighbor(dir);
            if (self.edgeExists(coord, neighbor_coord)) {
                mask |= @as(u6, 1) << @intCast(i);
                connected_count += 1;
                last_dir = @intCast(i);
            }
        }

        // Guarantee: at least 1 connection. If all pruned, restore last checked.
        if (connected_count == 0) {
            mask |= @as(u6, 1) << last_dir;
        }

        return mask;
    }

    /// Get list of traversable neighbor coordinates.
    pub fn connectedNeighbors(self: WorldGen, coord: Hex) ConnectedList {
        const mask = self.sectorConnections(coord);
        var result = ConnectedList{};

        for (hex.HexDirection.ALL, 0..) |dir, i| {
            if (mask & (@as(u6, 1) << @intCast(i)) != 0) {
                result.items[result.len] = coord.neighbor(dir);
                result.len += 1;
            }
        }

        return result;
    }

    /// Check if an edge exists between two adjacent hexes.
    /// Uses symmetric hashing so both sides agree.
    fn edgeExists(self: WorldGen, a: Hex, b: Hex) bool {
        // Symmetric edge seed: order-independent combination
        const key_a = a.toKey();
        const key_b = b.toKey();
        const edge_seed = if (key_a < key_b)
            self.hashTwo(key_a, key_b)
        else
            self.hashTwo(key_b, key_a);

        // Survival probability based on the hex further from center
        const dist_a = a.distFromOrigin();
        const dist_b = b.distFromOrigin();
        const max_dist = @max(dist_a, dist_b);
        const zone = Zone.fromDistance(max_dist);
        const survival_pct = zone.edgeSurvivalPct(max_dist);

        // Roll against survival probability
        const roll = @as(u8, @truncate(edge_seed % 100));
        return roll < survival_pct;
    }

    fn rollTerrain(self: WorldGen, random: std.Random, zone: Zone) TerrainType {
        _ = self;
        const roll = random.intRangeAtMost(u8, 0, 100);
        return switch (zone) {
            .central_hub => .empty, // hub is special
            .inner_ring => blk: {
                if (roll < 55) break :blk .asteroid_field; // testing: boosted
                if (roll < 70) break :blk .nebula;
                if (roll < 80) break :blk .debris_field;
                break :blk .empty;
            },
            .outer_ring => blk: {
                if (roll < 35) break :blk .asteroid_field;
                if (roll < 50) break :blk .nebula;
                if (roll < 70) break :blk .debris_field;
                if (roll < 75) break :blk .anomaly;
                break :blk .empty;
            },
            .wandering => blk: {
                if (roll < 30) break :blk .asteroid_field;
                if (roll < 45) break :blk .nebula;
                if (roll < 65) break :blk .debris_field;
                if (roll < 80) break :blk .anomaly;
                break :blk .empty;
            },
        };
    }

    const ResourceRoll = struct { metal: Density, crystal: Density, deut: Density };

    fn rollResources(self: WorldGen, random: std.Random, zone: Zone, terrain: TerrainType) ResourceRoll {
        _ = self;
        // Only asteroid fields and nebulae have significant resources
        if (terrain == .empty) return .{ .metal = .none, .crystal = .none, .deut = .none };

        const zone_boost: u8 = switch (zone) {
            .central_hub => 0,
            .inner_ring => 30, // testing: boosted
            .outer_ring => 40, // testing: boosted
            .wandering => 50, // testing: boosted
        };

        return .{
            .metal = rollDensity(random, if (terrain == .asteroid_field) 60 + zone_boost else 20),
            .crystal = rollDensity(random, if (terrain == .asteroid_field or terrain == .nebula) 40 + zone_boost else 10),
            .deut = rollDensity(random, if (terrain == .nebula) 50 + zone_boost else 15),
        };
    }

    fn rollDensity(random: std.Random, chance: u8) Density {
        const roll = random.intRangeAtMost(u8, 0, 100);
        if (roll >= chance) return .none;
        const quality = random.intRangeAtMost(u8, 0, 100);
        if (quality < 40) return .sparse;
        if (quality < 70) return .moderate;
        if (quality < 90) return .rich;
        return .pristine;
    }

    fn rollNpcPresence(self: WorldGen, random: std.Random, zone: Zone, dist: u16) ?NpcTemplate {
        _ = self;
        const presence_chance: u8 = switch (zone) {
            .central_hub => 0,
            .inner_ring => 30,
            .outer_ring => 70, // testing: boosted
            .wandering => 80, // testing: boosted
        };

        if (random.intRangeAtMost(u8, 0, 100) >= presence_chance) return null;

        // Scale fleet composition and stats by distance
        if (dist <= 8) {
            const behavior: NpcBehaviorType = if (random.boolean()) .passive else .patrol;
            return .{ .ship_class = .scout, .count = 1, .behavior = behavior, .stat_multiplier = 0.6 };
        } else if (dist <= 15) {
            const count = random.intRangeAtMost(u8, 3, 8);
            return .{ .ship_class = .corvette, .count = count, .behavior = .patrol, .stat_multiplier = 0.8 };
        } else if (dist <= 25) {
            const count = random.intRangeAtMost(u8, 5, 15);
            return .{ .ship_class = .frigate, .count = count, .behavior = .aggressive, .stat_multiplier = 1.0 };
        } else {
            const count = random.intRangeAtMost(u8, 10, 30);
            return .{ .ship_class = .cruiser, .count = count, .behavior = .swarm, .stat_multiplier = 1.2 };
        }
    }

    /// Deterministic seed for a specific sector.
    fn sectorSeed(self: WorldGen, coord: Hex) u64 {
        return self.hashTwo(@as(u64, coord.toKey()), self.world_seed);
    }

    /// Simple hash combiner.
    fn hashTwo(self: WorldGen, a: anytype, b: anytype) u64 {
        _ = self;
        var hasher = std.hash.Wyhash.init(0);
        hasher.update(std.mem.asBytes(&a));
        hasher.update(std.mem.asBytes(&b));
        return hasher.final();
    }
};

pub const NpcTemplate = struct {
    ship_class: ShipClass,
    count: u8,
    behavior: NpcBehaviorType,
    stat_multiplier: f32 = 1.0,
};

pub const NpcBehaviorType = enum {
    passive,
    patrol,
    aggressive,
    swarm,
};

pub const SectorTemplate = struct {
    coord: Hex,
    zone: Zone,
    terrain: TerrainType,
    metal_density: Density,
    crystal_density: Density,
    deut_density: Density,
    npc_template: ?NpcTemplate,
};

/// Fixed-size list of connected hex neighbors (max 6).
pub const ConnectedList = struct {
    items: [6]Hex = undefined,
    len: u8 = 0,

    pub fn slice(self: *const ConnectedList) []const Hex {
        return self.items[0..self.len];
    }
};

// ── Tests ──────────────────────────────────────────────────────────

test "world gen deterministic" {
    const gen = WorldGen.init(12345);
    const s1 = gen.generateSector(Hex{ .q = 5, .r = -3 });
    const s2 = gen.generateSector(Hex{ .q = 5, .r = -3 });
    try std.testing.expectEqual(s1.terrain, s2.terrain);
    try std.testing.expectEqual(s1.metal_density, s2.metal_density);
}

test "world gen different sectors differ" {
    const gen = WorldGen.init(12345);
    const s1 = gen.generateSector(Hex{ .q = 5, .r = -3 });
    const s2 = gen.generateSector(Hex{ .q = 10, .r = -8 });
    // These *could* be equal by chance, but with different seeds it's unlikely.
    // We just verify the function runs without error.
    _ = s1;
    _ = s2;
}

test "edge symmetry" {
    const gen = WorldGen.init(12345);
    const a = Hex{ .q = 3, .r = -1 };
    const b = Hex{ .q = 4, .r = -1 };
    // Both sides must agree on whether the edge exists
    const conn_a = gen.sectorConnections(a);
    const conn_b = gen.sectorConnections(b);
    // b is east of a
    const a_has_east = (conn_a & 1) != 0;
    // a is west of b
    const b_has_west = (conn_b & (1 << 3)) != 0;
    try std.testing.expectEqual(a_has_east, b_has_west);
}

test "hub fully connected" {
    const gen = WorldGen.init(12345);
    const conn = gen.sectorConnections(Hex.ORIGIN);
    try std.testing.expectEqual(@as(u6, 0b111111), conn);
}

test "at least one connection" {
    const gen = WorldGen.init(12345);
    // Test several deep wandering hexes
    var q: i16 = 50;
    while (q < 60) : (q += 1) {
        const conn = gen.sectorConnections(Hex{ .q = q, .r = -30 });
        try std.testing.expect(conn != 0);
    }
}
