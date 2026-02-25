// src/shared/scaling.zig
// Single source of truth for all balance formulas, costs, times, and modifiers.
// Pure data/formula file -- no side effects, no allocations.

const std = @import("std");
const constants = @import("constants.zig");

const Resources = constants.Resources;
const ShipClass = constants.ShipClass;
const ShipStats = constants.ShipStats;

pub const MAX_BUILDING_LEVEL: u8 = 20;

// ── Building System ──────────────────────────────────────────────

pub const BuildingType = enum(u8) {
    metal_mine,
    crystal_mine,
    deuterium_synthesizer,
    shipyard,
    research_lab,
    fuel_depot,
    sensor_array,
    defense_grid,

    pub const COUNT = @typeInfo(BuildingType).@"enum".fields.len;

    pub fn label(self: BuildingType) []const u8 {
        return switch (self) {
            .metal_mine => "Metal Mine",
            .crystal_mine => "Crystal Mine",
            .deuterium_synthesizer => "Deut Synthesizer",
            .shipyard => "Shipyard",
            .research_lab => "Research Lab",
            .fuel_depot => "Fuel Depot",
            .sensor_array => "Sensor Array",
            .defense_grid => "Defense Grid",
        };
    }

    pub fn shortLabel(self: BuildingType) []const u8 {
        return switch (self) {
            .metal_mine => "Metal",
            .crystal_mine => "Crystal",
            .deuterium_synthesizer => "Deut",
            .shipyard => "Shipyd",
            .research_lab => "Lab",
            .fuel_depot => "Depot",
            .sensor_array => "Sensor",
            .defense_grid => "DefGrd",
        };
    }
};

pub const BuildingLevels = struct {
    metal_mine: u8 = 1,
    crystal_mine: u8 = 1,
    deuterium_synthesizer: u8 = 0,
    shipyard: u8 = 0,
    research_lab: u8 = 0,
    fuel_depot: u8 = 0,
    sensor_array: u8 = 0,
    defense_grid: u8 = 0,

    pub fn get(self: BuildingLevels, building: BuildingType) u8 {
        return switch (building) {
            .metal_mine => self.metal_mine,
            .crystal_mine => self.crystal_mine,
            .deuterium_synthesizer => self.deuterium_synthesizer,
            .shipyard => self.shipyard,
            .research_lab => self.research_lab,
            .fuel_depot => self.fuel_depot,
            .sensor_array => self.sensor_array,
            .defense_grid => self.defense_grid,
        };
    }

    pub fn set(self: *BuildingLevels, building: BuildingType, level: u8) void {
        switch (building) {
            .metal_mine => self.metal_mine = level,
            .crystal_mine => self.crystal_mine = level,
            .deuterium_synthesizer => self.deuterium_synthesizer = level,
            .shipyard => self.shipyard = level,
            .research_lab => self.research_lab = level,
            .fuel_depot => self.fuel_depot = level,
            .sensor_array => self.sensor_array = level,
            .defense_grid => self.defense_grid = level,
        }
    }
};

// production_per_tick(level) = base_rate * level * 1.1^level
pub fn productionPerTick(building: BuildingType, level: u8) f32 {
    if (level == 0) return 0;
    const base: f32 = switch (building) {
        .metal_mine => 0.5,
        .crystal_mine => 0.3,
        .deuterium_synthesizer => 0.15,
        else => 0,
    };
    const l: f32 = @floatFromInt(level);
    return base * l * pow_f32(1.1, level);
}

// Cost to build level N (SPEC tables: linear in N)
pub fn buildingCost(building: BuildingType, level: u8) Resources {
    const n: f32 = @floatFromInt(level);
    return switch (building) {
        .metal_mine => .{ .metal = n * 60, .crystal = n * 15 },
        .crystal_mine => .{ .metal = n * 48, .crystal = n * 24 },
        .deuterium_synthesizer => .{ .metal = n * 225, .crystal = n * 75 },
        .shipyard => .{ .metal = n * 200, .crystal = n * 100, .deuterium = n * 50 },
        .research_lab => .{ .metal = n * 100, .crystal = n * 200, .deuterium = n * 50 },
        .fuel_depot => .{ .metal = n * 150, .crystal = n * 50, .deuterium = n * 100 },
        .sensor_array => .{ .metal = n * 100, .crystal = n * 150, .deuterium = n * 75 },
        .defense_grid => .{ .metal = n * 300, .crystal = n * 200, .deuterium = n * 100 },
    };
}

// build_ticks = base_ticks * level * 1.5^level
pub fn buildingTime(building: BuildingType, level: u8) u64 {
    if (level == 0) return 0;
    const base: u64 = switch (building) {
        .metal_mine, .crystal_mine, .deuterium_synthesizer => 30,
        .shipyard, .research_lab => 60,
        .fuel_depot => 40,
        .sensor_array => 50,
        .defense_grid => 90,
    };
    const l: f32 = @floatFromInt(level);
    const ticks = @as(f32, @floatFromInt(base)) * l * pow_f32(1.5, level);
    return @intFromFloat(@max(1, ticks));
}

pub const Prerequisite = struct {
    building: BuildingType,
    level: u8,
};

pub fn buildingPrerequisites(building: BuildingType) ?Prerequisite {
    return switch (building) {
        .metal_mine, .crystal_mine, .deuterium_synthesizer => null,
        .shipyard => .{ .building = .metal_mine, .level = 2 },
        .research_lab => .{ .building = .crystal_mine, .level = 2 },
        .fuel_depot => .{ .building = .deuterium_synthesizer, .level = 2 },
        .sensor_array => .{ .building = .research_lab, .level = 1 },
        .defense_grid => .{ .building = .shipyard, .level = 3 },
    };
}

pub fn buildingPrerequisitesMet(building: BuildingType, levels: BuildingLevels) bool {
    const prereq = buildingPrerequisites(building) orelse return true;
    return levels.get(prereq.building) >= prereq.level;
}

// ── Research System ──────────────────────────────────────────────

pub const ResearchType = enum(u8) {
    fuel_efficiency,
    extended_fuel_tanks,
    reinforced_hulls,
    advanced_shields,
    weapons_research,
    navigation,
    harvesting_efficiency,
    corvette_tech,
    frigate_tech,
    cruiser_tech,
    hauler_tech,
    emergency_jump,

    pub const COUNT = @typeInfo(ResearchType).@"enum".fields.len;

    pub fn label(self: ResearchType) []const u8 {
        return switch (self) {
            .fuel_efficiency => "Fuel Efficiency",
            .extended_fuel_tanks => "Extended Tanks",
            .reinforced_hulls => "Reinforced Hulls",
            .advanced_shields => "Advanced Shields",
            .weapons_research => "Weapons Research",
            .navigation => "Navigation",
            .harvesting_efficiency => "Harvesting Eff.",
            .corvette_tech => "Corvette Tech",
            .frigate_tech => "Frigate Tech",
            .cruiser_tech => "Cruiser Tech",
            .hauler_tech => "Hauler Tech",
            .emergency_jump => "Emergency Jump",
        };
    }
};

pub const ResearchLevels = struct {
    fuel_efficiency: u8 = 0,
    extended_fuel_tanks: u8 = 0,
    reinforced_hulls: u8 = 0,
    advanced_shields: u8 = 0,
    weapons_research: u8 = 0,
    navigation: u8 = 0,
    harvesting_efficiency: u8 = 0,
    corvette_tech: u8 = 0,
    frigate_tech: u8 = 0,
    cruiser_tech: u8 = 0,
    hauler_tech: u8 = 0,
    emergency_jump: u8 = 0,

    pub fn get(self: ResearchLevels, tech: ResearchType) u8 {
        return switch (tech) {
            .fuel_efficiency => self.fuel_efficiency,
            .extended_fuel_tanks => self.extended_fuel_tanks,
            .reinforced_hulls => self.reinforced_hulls,
            .advanced_shields => self.advanced_shields,
            .weapons_research => self.weapons_research,
            .navigation => self.navigation,
            .harvesting_efficiency => self.harvesting_efficiency,
            .corvette_tech => self.corvette_tech,
            .frigate_tech => self.frigate_tech,
            .cruiser_tech => self.cruiser_tech,
            .hauler_tech => self.hauler_tech,
            .emergency_jump => self.emergency_jump,
        };
    }

    pub fn set(self: *ResearchLevels, tech: ResearchType, level: u8) void {
        switch (tech) {
            .fuel_efficiency => self.fuel_efficiency = level,
            .extended_fuel_tanks => self.extended_fuel_tanks = level,
            .reinforced_hulls => self.reinforced_hulls = level,
            .advanced_shields => self.advanced_shields = level,
            .weapons_research => self.weapons_research = level,
            .navigation => self.navigation = level,
            .harvesting_efficiency => self.harvesting_efficiency = level,
            .corvette_tech => self.corvette_tech = level,
            .frigate_tech => self.frigate_tech = level,
            .cruiser_tech => self.cruiser_tech = level,
            .hauler_tech => self.hauler_tech = level,
            .emergency_jump => self.emergency_jump = level,
        }
    }
};

pub fn researchMaxLevel(tech: ResearchType) u8 {
    return switch (tech) {
        .corvette_tech, .frigate_tech, .cruiser_tech, .hauler_tech => 1,
        .emergency_jump => 3,
        else => 5,
    };
}

pub fn researchCost(tech: ResearchType, level: u8) Resources {
    const n: f32 = @floatFromInt(level);
    return switch (tech) {
        // Unlock techs: flat cost
        .corvette_tech => .{ .metal = 400, .crystal = 200, .deuterium = 100 },
        .frigate_tech => .{ .metal = 1000, .crystal = 600, .deuterium = 300 },
        .cruiser_tech => .{ .metal = 3000, .crystal = 2000, .deuterium = 1000 },
        .hauler_tech => .{ .metal = 600, .crystal = 300, .deuterium = 200 },
        // Scaling techs: cost scales with level
        .fuel_efficiency => .{ .metal = n * 200, .crystal = n * 100, .deuterium = n * 100 },
        .extended_fuel_tanks => .{ .metal = n * 150, .crystal = n * 75, .deuterium = n * 150 },
        .reinforced_hulls => .{ .metal = n * 300, .crystal = n * 100 },
        .advanced_shields => .{ .metal = n * 100, .crystal = n * 300, .deuterium = n * 50 },
        .weapons_research => .{ .metal = n * 200, .crystal = n * 200, .deuterium = n * 100 },
        .navigation => .{ .metal = n * 100, .crystal = n * 150, .deuterium = n * 50 },
        .harvesting_efficiency => .{ .metal = n * 150, .crystal = n * 100, .deuterium = n * 75 },
        .emergency_jump => .{ .metal = n * 500, .crystal = n * 400, .deuterium = n * 300 },
    };
}

pub fn researchTime(tech: ResearchType, level: u8) u64 {
    if (level == 0) return 0;
    return switch (tech) {
        // Unlock techs: flat time
        .corvette_tech => 60,
        .frigate_tech => 120,
        .cruiser_tech => 240,
        .hauler_tech => 90,
        // Scaling techs: same formula as buildings
        else => blk: {
            const base: u64 = 60;
            const l: f32 = @floatFromInt(level);
            const ticks = @as(f32, @floatFromInt(base)) * l * pow_f32(1.5, level);
            break :blk @intFromFloat(@max(1, ticks));
        },
    };
}

pub const ResearchPrereq = struct {
    kind: PrereqKind,

    pub const PrereqKind = union(enum) {
        building: Prerequisite,
        research: struct { tech: ResearchType, level: u8 },
    };
};

pub fn researchPrerequisites(tech: ResearchType) [2]?ResearchPrereq {
    return switch (tech) {
        .fuel_efficiency => .{
            .{ .kind = .{ .building = .{ .building = .deuterium_synthesizer, .level = 3 } } },
            null,
        },
        .extended_fuel_tanks => .{
            .{ .kind = .{ .building = .{ .building = .fuel_depot, .level = 2 } } },
            null,
        },
        .reinforced_hulls => .{
            .{ .kind = .{ .building = .{ .building = .shipyard, .level = 2 } } },
            null,
        },
        .advanced_shields => .{
            .{ .kind = .{ .building = .{ .building = .research_lab, .level = 3 } } },
            null,
        },
        .weapons_research => .{
            .{ .kind = .{ .building = .{ .building = .research_lab, .level = 3 } } },
            null,
        },
        .navigation => .{
            .{ .kind = .{ .building = .{ .building = .research_lab, .level = 2 } } },
            null,
        },
        .harvesting_efficiency => .{
            .{ .kind = .{ .building = .{ .building = .research_lab, .level = 2 } } },
            null,
        },
        .corvette_tech => .{
            .{ .kind = .{ .building = .{ .building = .shipyard, .level = 2 } } },
            null,
        },
        .frigate_tech => .{
            .{ .kind = .{ .research = .{ .tech = .corvette_tech, .level = 1 } } },
            .{ .kind = .{ .building = .{ .building = .shipyard, .level = 4 } } },
        },
        .cruiser_tech => .{
            .{ .kind = .{ .research = .{ .tech = .frigate_tech, .level = 1 } } },
            .{ .kind = .{ .building = .{ .building = .shipyard, .level = 6 } } },
        },
        .hauler_tech => .{
            .{ .kind = .{ .building = .{ .building = .shipyard, .level = 3 } } },
            null,
        },
        .emergency_jump => .{
            .{ .kind = .{ .research = .{ .tech = .navigation, .level = 1 } } },
            .{ .kind = .{ .building = .{ .building = .research_lab, .level = 4 } } },
        },
    };
}

pub fn researchPrerequisitesMet(
    tech: ResearchType,
    buildings: BuildingLevels,
    research: ResearchLevels,
) bool {
    const prereqs = researchPrerequisites(tech);
    for (prereqs) |maybe_prereq| {
        const prereq = maybe_prereq orelse continue;
        switch (prereq.kind) {
            .building => |b| {
                if (buildings.get(b.building) < b.level) return false;
            },
            .research => |r| {
                if (research.get(r.tech) < r.level) return false;
            },
        }
    }
    return true;
}

// ── Research Modifier Functions ──────────────────────────────────

pub fn applyResearchToStats(base: ShipStats, research: ResearchLevels) ShipStats {
    const hull_bonus = 1.0 + 0.10 * @as(f32, @floatFromInt(research.reinforced_hulls));
    const shield_bonus = 1.0 + 0.10 * @as(f32, @floatFromInt(research.advanced_shields));
    const weapon_bonus = 1.0 + 0.10 * @as(f32, @floatFromInt(research.weapons_research));
    const fuel_cap = fuelCapacityModifier(research.extended_fuel_tanks);
    return .{
        .hull = base.hull * hull_bonus,
        .shield = base.shield * shield_bonus,
        .weapon = base.weapon * weapon_bonus,
        .speed = base.speed,
        .cargo = base.cargo,
        .fuel = @intFromFloat(@as(f32, @floatFromInt(base.fuel)) * fuel_cap),
    };
}

// 1.0 - 0.10 * level (less fuel used)
pub fn fuelRateModifier(fuel_eff_level: u8) f32 {
    return 1.0 - 0.10 * @as(f32, @floatFromInt(fuel_eff_level));
}

// 1.0 + 0.15 * level (more fuel capacity)
pub fn fuelCapacityModifier(tanks_level: u8) f32 {
    return 1.0 + 0.15 * @as(f32, @floatFromInt(tanks_level));
}

// 1.0 + 0.10 * level (fuel depot bonus to fleet fuel_max)
pub fn fuelDepotModifier(depot_level: u8) f32 {
    return 1.0 + 0.10 * @as(f32, @floatFromInt(depot_level));
}

// Sensor array reveal range (BFS hops from homeworld)
pub fn sensorRange(sensor_level: u8) u8 {
    return sensor_level;
}

// Returns ticks to subtract from base movement cooldown
pub fn navigationCooldownReduction(nav_level: u8) u16 {
    return nav_level;
}

// 1.0 + 0.20 * level (more harvest per tick)
pub fn harvestRateModifier(harvest_eff_level: u8) f32 {
    return 1.0 + 0.20 * @as(f32, @floatFromInt(harvest_eff_level));
}

// 0.05 * level (reduce recall damage chance)
pub fn recallDamageReduction(ej_level: u8) f32 {
    return 0.05 * @as(f32, @floatFromInt(ej_level));
}

pub fn shipClassUnlocked(class: ShipClass, research: ResearchLevels) bool {
    return switch (class) {
        .scout => true,
        .corvette => research.corvette_tech >= 1,
        .frigate => research.frigate_tech >= 1,
        .cruiser => research.cruiser_tech >= 1,
        .hauler => research.hauler_tech >= 1,
    };
}

// base_ticks / (1 + 0.1 * shipyard_level)
pub fn shipBuildTime(class: ShipClass, shipyard_level: u8) u64 {
    const base: f32 = switch (class) {
        .scout => 30,
        .corvette => 60,
        .frigate => 120,
        .cruiser => 240,
        .hauler => 90,
    };
    const divisor = 1.0 + 0.1 * @as(f32, @floatFromInt(shipyard_level));
    return @intFromFloat(@max(1, base / divisor));
}

pub const CANCEL_REFUND_FRACTION: f32 = 0.50;

pub const QueueType = enum {
    building,
    ship,
    research,
};

// ── Utility ─────────────────────────────────────────────────────

fn pow_f32(base: f32, exp: u8) f32 {
    var result: f32 = 1.0;
    for (0..exp) |_| {
        result *= base;
    }
    return result;
}
