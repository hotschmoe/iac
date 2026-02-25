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

const ShipRef = struct {
    ptr: *Ship,
    fleet_id: u64,
    is_npc: bool,
};

pub fn resolveCombatRound(
    allocator: std.mem.Allocator,
    active_combat: *engine.Combat,
    player_fleets: []const *Fleet,
    npc_fleets: []const *NpcFleet,
    tick: u64,
) !CombatRoundResult {
    active_combat.round += 1;

    var rng = std.Random.DefaultPrng.init(
        @as(u64, @truncate(@as(u128, @bitCast(std.time.nanoTimestamp())))) ^ @as(u64, active_combat.round),
    );
    const random = rng.random();

    var events: std.ArrayList(GameEvent) = .empty;

    var npc_targets: std.ArrayList(ShipRef) = .empty;
    defer npc_targets.deinit(allocator);
    for (npc_fleets) |npc| {
        for (npc.ships[0..npc.ship_count]) |*ship| {
            if (ship.hull > 0) {
                try npc_targets.append(allocator, .{ .ptr = ship, .fleet_id = npc.id, .is_npc = true });
            }
        }
    }

    var player_targets: std.ArrayList(ShipRef) = .empty;
    defer player_targets.deinit(allocator);
    for (player_fleets) |pf| {
        for (pf.ships[0..pf.ship_count]) |*ship| {
            if (ship.hull > 0) {
                try player_targets.append(allocator, .{ .ptr = ship, .fleet_id = pf.id, .is_npc = false });
            }
        }
    }

    for (player_fleets) |pf| {
        for (pf.ships[0..pf.ship_count]) |*attacker| {
            if (attacker.hull <= 0) continue;
            try fireShip(attacker, npc_targets.items, tick, random, &events, allocator);
        }
    }

    for (npc_fleets) |npc| {
        for (npc.ships[0..npc.ship_count]) |*attacker| {
            if (attacker.hull <= 0) continue;
            try fireShip(attacker, player_targets.items, tick, random, &events, allocator);
        }
    }

    for (player_fleets) |pf| {
        pf.ship_count = compactDestroyedShips(pf.ships[0..pf.ship_count]);
    }
    for (npc_fleets) |npc| {
        npc.ship_count = @intCast(compactDestroyedShips(npc.ships[0..npc.ship_count]));
    }

    const any_player_alive = for (player_fleets) |pf| {
        if (pf.ship_count > 0) break true;
    } else false;

    const any_npc_alive = for (npc_fleets) |npc| {
        if (npc.ship_count > 0) break true;
    } else false;

    const concluded = !any_player_alive or !any_npc_alive;

    if (concluded) {
        if (!any_npc_alive) {
            for (npc_fleets) |npc| {
                try events.append(allocator, .{
                    .tick = tick,
                    .kind = .{ .fleet_destroyed = .{
                        .fleet_id = npc.id,
                        .is_npc = true,
                        .salvage = active_combat.npc_value,
                    } },
                });
            }
        }
        for (player_fleets) |pf| {
            if (pf.ship_count == 0) {
                try events.append(allocator, .{
                    .tick = tick,
                    .kind = .{ .fleet_destroyed = .{
                        .fleet_id = pf.id,
                        .is_npc = false,
                        .salvage = .{},
                    } },
                });
            }
        }
    }

    return .{
        .events = try events.toOwnedSlice(allocator),
        .concluded = concluded,
        .player_won = concluded and any_player_alive,
    };
}

fn fireShip(
    attacker: *Ship,
    targets: []ShipRef,
    tick: u64,
    random: std.Random,
    events: *std.ArrayList(GameEvent),
    allocator: std.mem.Allocator,
) !void {
    while (true) {
        const target_ref = selectTarget(targets, random) orelse return;
        const target = target_ref.ptr;

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
                    .owner_fleet_id = target_ref.fleet_id,
                    .is_npc = target_ref.is_npc,
                } },
            });
        }

        if (!rapid) break;
    }
}

fn selectTarget(targets: []ShipRef, rng: std.Random) ?*ShipRef {
    var total_weight: f32 = 0;
    for (targets) |ref| {
        if (ref.ptr.hull > 0) total_weight += ref.ptr.hull_max;
    }

    if (total_weight <= 0) return null;

    var roll = rng.float(f32) * total_weight;
    for (targets) |*ref| {
        if (ref.ptr.hull <= 0) continue;
        roll -= ref.ptr.hull_max;
        if (roll <= 0) return ref;
    }

    // Fallback: last alive
    var i = targets.len;
    while (i > 0) {
        i -= 1;
        if (targets[i].ptr.hull > 0) return &targets[i];
    }
    return null;
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

fn compactDestroyedShips(ships: []Ship) usize {
    var write_idx: usize = 0;
    for (ships) |ship| {
        if (ship.hull > 0) {
            ships[write_idx] = ship;
            write_idx += 1;
        }
    }
    return write_idx;
}
