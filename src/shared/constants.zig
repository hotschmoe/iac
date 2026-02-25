// src/shared/constants.zig
// All tunable game values. Single source of truth for balance.

/// Server tick rate.
pub const TICK_RATE_HZ: u32 = 1;
pub const TICK_DURATION_NS: u64 = 1_000_000_000 / TICK_RATE_HZ;

/// World zones defined by cube distance from origin.
pub const Zone = enum {
    central_hub,
    inner_ring,
    outer_ring,
    wandering,

    /// Maximum distance (inclusive) for each zone boundary.
    pub const inner_ring_radius: u16 = 8;
    pub const outer_ring_radius: u16 = 20;

    pub fn fromDistance(dist: u16) Zone {
        if (dist == 0) return .central_hub;
        if (dist <= inner_ring_radius) return .inner_ring;
        if (dist <= outer_ring_radius) return .outer_ring;
        return .wandering;
    }

    /// Edge survival probability for procedural connectivity.
    pub fn edgeSurvivalPct(self: Zone, dist: u16) u8 {
        return switch (self) {
            .central_hub => 100,
            .inner_ring => 95,
            .outer_ring => 80,
            .wandering => blk: {
                // Decreases from 60% at dist 21 to 40% at dist 60+
                const base: u16 = 60;
                const decay = @min(dist -| 20, 40) / 2; // lose 1% per 2 hexes
                break :blk @intCast(base -| decay);
            },
        };
    }
};

/// Resource density levels.
pub const Density = enum(u8) {
    none = 0,
    sparse = 1,
    moderate = 2,
    rich = 3,
    pristine = 4,

    pub fn harvestMultiplier(self: Density) f32 {
        return switch (self) {
            .none => 0.0,
            .sparse => 0.5,
            .moderate => 1.0,
            .rich => 2.0,
            .pristine => 4.0,
        };
    }

    pub fn depletionThreshold(self: Density) f32 {
        return switch (self) {
            .none => 0.0,
            .sparse => 10.0,
            .moderate => 20.0,
            .rich => 30.0,
            .pristine => 40.0,
        };
    }

    pub fn downgrade(self: Density) Density {
        return switch (self) {
            .pristine => .rich,
            .rich => .moderate,
            .moderate => .sparse,
            .sparse => .none,
            .none => .none,
        };
    }

    pub fn upgrade(self: Density) Density {
        return switch (self) {
            .none => .sparse,
            .sparse => .moderate,
            .moderate => .rich,
            .rich => .pristine,
            .pristine => .pristine,
        };
    }

    pub fn label(self: Density) []const u8 {
        return switch (self) {
            .none => "None",
            .sparse => "Sparse",
            .moderate => "Moderate",
            .rich => "Rich",
            .pristine => "Pristine",
        };
    }
};

/// Terrain types for sectors.
pub const TerrainType = enum(u8) {
    empty,
    asteroid_field,
    nebula,
    debris_field,
    anomaly,

    pub fn label(self: TerrainType) []const u8 {
        return switch (self) {
            .empty => "Empty Space",
            .asteroid_field => "Asteroid Field",
            .nebula => "Nebula",
            .debris_field => "Debris Field",
            .anomaly => "Anomaly",
        };
    }

    /// Map symbol for rendering.
    pub fn symbol(self: TerrainType) []const u8 {
        return switch (self) {
            .empty => "·",
            .asteroid_field => "*",
            .nebula => "≈",
            .debris_field => "×",
            .anomaly => "?",
        };
    }
};

/// Ship class definitions.
pub const ShipClass = enum(u8) {
    scout,
    corvette,
    frigate,
    cruiser,
    hauler,

    pub const COUNT = @typeInfo(ShipClass).@"enum".fields.len;
    pub const ALL = blk: {
        const fields = @typeInfo(ShipClass).@"enum".fields;
        var arr: [fields.len]ShipClass = undefined;
        for (fields, 0..) |f, i| arr[i] = @enumFromInt(f.value);
        break :blk arr;
    };

    pub fn baseStats(self: ShipClass) ShipStats {
        return switch (self) {
            .scout => .{ .hull = 30, .shield = 10, .weapon = 5, .speed = 10, .cargo = 20, .fuel = 60 },
            .corvette => .{ .hull = 50, .shield = 20, .weapon = 15, .speed = 8, .cargo = 10, .fuel = 50 },
            .frigate => .{ .hull = 120, .shield = 60, .weapon = 30, .speed = 5, .cargo = 30, .fuel = 80 },
            .cruiser => .{ .hull = 200, .shield = 100, .weapon = 80, .speed = 4, .cargo = 50, .fuel = 120 },
            .hauler => .{ .hull = 80, .shield = 20, .weapon = 5, .speed = 6, .cargo = 200, .fuel = 100 },
        };
    }

    pub fn label(self: ShipClass) []const u8 {
        return switch (self) {
            .scout => "Scout",
            .corvette => "Corvette",
            .frigate => "Frigate",
            .cruiser => "Cruiser",
            .hauler => "Hauler",
        };
    }

    /// Build cost in resources.
    pub fn buildCost(self: ShipClass) Resources {
        return switch (self) {
            .scout => .{ .metal = 200, .crystal = 50, .deuterium = 30 },
            .corvette => .{ .metal = 400, .crystal = 100, .deuterium = 60 },
            .frigate => .{ .metal = 1000, .crystal = 400, .deuterium = 200 },
            .cruiser => .{ .metal = 3000, .crystal = 1500, .deuterium = 800 },
            .hauler => .{ .metal = 600, .crystal = 200, .deuterium = 150 },
        };
    }

    /// Rapid-fire table: returns multiplier against target class (0 = no bonus).
    pub fn rapidFireVs(self: ShipClass, target: ShipClass) u8 {
        return switch (self) {
            .corvette => if (target == .scout) 3 else 0,
            .frigate => if (target == .corvette) 2 else 0,
            .cruiser => if (target == .frigate) 2 else 0,
            else => 0,
        };
    }
};

pub const ShipStats = struct {
    hull: f32,
    shield: f32,
    weapon: f32,
    speed: u8,
    cargo: u16,
    fuel: u16,
};

pub const Resources = struct {
    metal: f32 = 0,
    crystal: f32 = 0,
    deuterium: f32 = 0,

    pub fn add(self: Resources, other: Resources) Resources {
        return .{
            .metal = self.metal + other.metal,
            .crystal = self.crystal + other.crystal,
            .deuterium = self.deuterium + other.deuterium,
        };
    }

    pub fn sub(self: Resources, other: Resources) Resources {
        return .{
            .metal = self.metal - other.metal,
            .crystal = self.crystal - other.crystal,
            .deuterium = self.deuterium - other.deuterium,
        };
    }

    pub fn scale(self: Resources, factor: f32) Resources {
        return .{
            .metal = self.metal * factor,
            .crystal = self.crystal * factor,
            .deuterium = self.deuterium * factor,
        };
    }

    pub fn canAfford(self: Resources, cost: Resources) bool {
        return self.metal >= cost.metal and
            self.crystal >= cost.crystal and
            self.deuterium >= cost.deuterium;
    }
};

/// Starting resources for new players.
pub const STARTING_RESOURCES = Resources{
    .metal = 500,
    .crystal = 300,
    .deuterium = 100,
};

pub const STARTING_SCOUTS: usize = 2;

/// Homeworld spawn range (cube distance from origin).
pub const HOMEWORLD_MIN_DIST: u16 = 3;
pub const HOMEWORLD_MAX_DIST: u16 = 6;

/// Cooldowns (in ticks).
pub const MOVE_BASE_COOLDOWN: u16 = 1; // testing: fast movement
pub const HARVEST_COOLDOWN: u16 = 1;
pub const SCAN_COOLDOWN: u16 = 5;

/// Combat.
pub const DAMAGE_VARIANCE_MIN: f32 = 0.8;
pub const DAMAGE_VARIANCE_MAX: f32 = 1.2;
pub const SHIELD_REGEN_IDLE_TICKS: u16 = 10;

/// Emergency recall.
pub const RECALL_FUEL_MULTIPLIER: f32 = 2.0;
pub const RECALL_DAMAGE_CHANCE_PER_HEX: f32 = 0.02;
pub const RECALL_DAMAGE_CHANCE_CAP: f32 = 0.60;
pub const RECALL_HULL_DAMAGE_MIN: f32 = 0.20;
pub const RECALL_HULL_DAMAGE_MAX: f32 = 0.80;

/// Fuel consumption per hex: fleet_mass * this value.
pub const FUEL_RATE_PER_MASS: f32 = 0.1;

/// Resource regeneration: sectors regen this fraction per tick.
pub const SECTOR_REGEN_RATE: f32 = 0.0001; // ~full in ~3 hours at 1Hz

/// NPC respawn delays (in ticks at 1Hz).
pub const NPC_RESPAWN_INNER: u64 = 300; // 5 min
pub const NPC_RESPAWN_OUTER: u64 = 600; // 10 min
pub const NPC_RESPAWN_WANDERING: u64 = 1200; // 20 min
pub const NPC_PATROL_INTERVAL: u16 = 15; // ticks between patrol moves

pub fn npcRespawnDelay(zone: Zone) u64 {
    return switch (zone) {
        .central_hub => NPC_RESPAWN_INNER, // no NPCs spawn here, but fallback
        .inner_ring => NPC_RESPAWN_INNER,
        .outer_ring => NPC_RESPAWN_OUTER,
        .wandering => NPC_RESPAWN_WANDERING,
    };
}

/// Salvage: fraction of destroyed fleet's build cost that drops.
pub const SALVAGE_FRACTION: f32 = 0.30;
pub const SALVAGE_DESPAWN_TICKS: u32 = 60;

/// WebSocket server defaults.
pub const DEFAULT_PORT: u16 = 7777;
pub const DEFAULT_HOST = "127.0.0.1";

/// World seed (overridable at server start).
pub const DEFAULT_WORLD_SEED: u64 = 0xDEAD_BEEF_CAFE_BABE;
