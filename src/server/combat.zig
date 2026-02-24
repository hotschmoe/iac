// src/server/combat.zig
// Stochastic fleet combat resolution.
// Each tick during combat = one round.
// Ships fire at random targets, damage through shields then hull.
// Rapid-fire mechanics from OGame.

const std = @import("std");
const shared = @import("shared");
const engine = @import("engine.zig");

const Ship = engine.Ship;
const ShipClass = shared.constants.ShipClass;

pub const CombatRoundResult = struct {
    events: []shared.protocol.GameEvent,
    concluded: bool,
    player_won: bool,
};

/// Resolve one round of combat between a player fleet and an NPC fleet.
/// Each ship fires at a random enemy. Damage is stochastic.
/// Returns events generated this round and whether combat is over.
pub fn resolveCombatRound(
    allocator: std.mem.Allocator,
    active_combat: *engine.Combat,
    tick: u64,
) !CombatRoundResult {
    _ = allocator;
    _ = tick;

    active_combat.round += 1;

    // TODO: Full implementation. Scaffold:
    //
    // 1. Collect all living ships from both sides.
    // 2. Shuffle firing order (or iterate randomly).
    // 3. For each ship:
    //    a. Pick random enemy target (weighted by hull_max — bigger = easier to hit).
    //    b. Roll damage: weapon_power * random(0.8, 1.2)
    //    c. Apply to target shield first, overflow to hull.
    //    d. Check rapid-fire: if ship.class.rapidFireVs(target.class) > 0,
    //       probability (1 - 1/rf) to fire again at new random target.
    //    e. If target hull <= 0, mark destroyed.
    // 4. Remove destroyed ships.
    // 5. Check if either side has 0 ships — combat concludes.
    //
    // Events to generate:
    //   - CombatRoundEvent for each shot fired
    //   - ShipDestroyedEvent for each ship destroyed
    //   - FleetDestroyedEvent if entire fleet wiped

    return .{
        .events = &.{},
        .concluded = active_combat.round >= 20, // temp: auto-end after 20 rounds
        .player_won = true, // temp
    };
}

/// Calculate weighted random target selection.
/// Larger ships (higher hull_max) are more likely to be targeted.
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

    return count - 1; // fallback
}

/// Apply damage to a ship: shields absorb first, remainder hits hull.
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

/// Roll damage for an attack: base weapon power * variance.
fn rollDamage(weapon_power: f32, rng: std.Random) f32 {
    const variance = shared.constants.DAMAGE_VARIANCE_MIN +
        rng.float(f32) * (shared.constants.DAMAGE_VARIANCE_MAX - shared.constants.DAMAGE_VARIANCE_MIN);
    return weapon_power * variance;
}

/// Check rapid-fire: returns true if the ship fires again this round.
fn checkRapidFire(attacker_class: ShipClass, target_class: ShipClass, rng: std.Random) bool {
    const rf = attacker_class.rapidFireVs(target_class);
    if (rf == 0) return false;
    // Probability to fire again: (1 - 1/rf)
    // e.g., rf=3 → 66.7% chance to fire again
    return rng.float(f32) < (1.0 - 1.0 / @as(f32, @floatFromInt(rf)));
}
