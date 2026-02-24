const std = @import("std");
const shared = @import("shared");
const engine = @import("engine.zig");

const Ship = engine.Ship;
const Fleet = engine.Fleet;
const NpcFleet = engine.NpcFleet;
const ShipClass = shared.constants.ShipClass;
const GameEvent = shared.protocol.GameEvent;

pub const CombatRoundResult = struct {
    events: []GameEvent,
    concluded: bool,
    player_won: bool,
};

pub fn resolveCombatRound(
    allocator: std.mem.Allocator,
    active_combat: *engine.Combat,
    player_fleet: *Fleet,
    npc_fleet: *NpcFleet,
    tick: u64,
) !CombatRoundResult {
    active_combat.round += 1;

    var rng = std.Random.DefaultPrng.init(
        @as(u64, @truncate(@as(u128, @bitCast(std.time.nanoTimestamp())))) ^ @as(u64, active_combat.round),
    );
    const random = rng.random();

    var events: std.ArrayList(GameEvent) = .empty;

    // Each player ship fires
    for (player_fleet.ships[0..player_fleet.ship_count]) |*attacker| {
        if (attacker.hull <= 0) continue;
        try fireShip(attacker, npc_fleet.ships[0..npc_fleet.ship_count], true, npc_fleet.id, tick, random, &events, allocator);
    }

    // Each NPC ship fires
    for (npc_fleet.ships[0..npc_fleet.ship_count]) |*attacker| {
        if (attacker.hull <= 0) continue;
        try fireShip(attacker, player_fleet.ships[0..player_fleet.ship_count], false, player_fleet.id, tick, random, &events, allocator);
    }

    // Compact destroyed ships from player fleet
    var write_idx: usize = 0;
    for (player_fleet.ships[0..player_fleet.ship_count]) |ship| {
        if (ship.hull > 0) {
            player_fleet.ships[write_idx] = ship;
            write_idx += 1;
        }
    }
    player_fleet.ship_count = write_idx;

    // Compact destroyed ships from NPC fleet
    write_idx = 0;
    for (npc_fleet.ships[0..npc_fleet.ship_count]) |ship| {
        if (ship.hull > 0) {
            npc_fleet.ships[write_idx] = ship;
            write_idx += 1;
        }
    }
    npc_fleet.ship_count = @intCast(write_idx);

    // Check victory conditions
    const player_alive = player_fleet.ship_count > 0;
    const npc_alive = npc_fleet.ship_count > 0;
    const concluded = !player_alive or !npc_alive;

    if (concluded) {
        if (!npc_alive) {
            try events.append(allocator, .{
                .tick = tick,
                .kind = .{ .fleet_destroyed = .{
                    .fleet_id = npc_fleet.id,
                    .is_npc = true,
                    .salvage = active_combat.npc_value,
                } },
            });
        }
        if (!player_alive) {
            try events.append(allocator, .{
                .tick = tick,
                .kind = .{ .fleet_destroyed = .{
                    .fleet_id = player_fleet.id,
                    .is_npc = false,
                    .salvage = .{},
                } },
            });
        }
    }

    return .{
        .events = try events.toOwnedSlice(allocator),
        .concluded = concluded,
        .player_won = concluded and player_alive,
    };
}

fn fireShip(
    attacker: *Ship,
    targets: []Ship,
    target_is_npc: bool,
    target_fleet_id: u64,
    tick: u64,
    random: std.Random,
    events: *std.ArrayList(GameEvent),
    allocator: std.mem.Allocator,
) !void {
    while (true) {
        const target_idx = selectTarget(targets, targets.len, random) orelse return;
        const target = &targets[target_idx];

        const damage = rollDamage(attacker.weapon_power, random);
        const result = applyDamage(target, damage);
        const rapid = checkRapidFire(attacker.class, target.class, random);

        try events.append(allocator, .{
            .tick = tick,
            .kind = .{ .combat_round = .{
                .attacker_ship_id = attacker.id,
                .target_ship_id = target.id,
                .damage = damage,
                .shield_absorbed = result.shield_absorbed,
                .hull_damage = result.hull_damage,
                .rapid_fire = rapid,
            } },
        });

        if (target.hull <= 0) {
            try events.append(allocator, .{
                .tick = tick,
                .kind = .{ .ship_destroyed = .{
                    .ship_id = target.id,
                    .ship_class = target.class,
                    .owner_fleet_id = target_fleet_id,
                    .is_npc = target_is_npc,
                } },
            });
        }

        if (!rapid) break;
    }
}

fn selectTarget(ships: []Ship, count: usize, rng: std.Random) ?usize {
    if (count == 0) return null;

    var total_weight: f32 = 0;
    for (ships[0..count]) |ship| {
        if (ship.hull > 0) total_weight += ship.hull_max;
    }

    if (total_weight <= 0) return null;

    var roll = rng.float(f32) * total_weight;
    for (ships[0..count], 0..) |ship, i| {
        if (ship.hull <= 0) continue;
        roll -= ship.hull_max;
        if (roll <= 0) return i;
    }

    return count - 1;
}

fn applyDamage(target: *Ship, damage: f32) struct { shield_absorbed: f32, hull_damage: f32 } {
    const shield_absorbed = @min(damage, target.shield);
    target.shield -= shield_absorbed;

    const passthrough = damage - shield_absorbed;
    const hull_damage = @min(passthrough, target.hull);
    target.hull -= hull_damage;

    return .{
        .shield_absorbed = shield_absorbed,
        .hull_damage = hull_damage,
    };
}

fn rollDamage(weapon_power: f32, rng: std.Random) f32 {
    const variance = shared.constants.DAMAGE_VARIANCE_MIN +
        rng.float(f32) * (shared.constants.DAMAGE_VARIANCE_MAX - shared.constants.DAMAGE_VARIANCE_MIN);
    return weapon_power * variance;
}

fn checkRapidFire(attacker_class: ShipClass, target_class: ShipClass, rng: std.Random) bool {
    const rf = attacker_class.rapidFireVs(target_class);
    if (rf == 0) return false;
    return rng.float(f32) < (1.0 - 1.0 / @as(f32, @floatFromInt(rf)));
}
