// Round-based combat resolution with RNG, target selection, damage calculation.
// Ported from Zig combat.zig.
//
// Works with copied ship data to avoid borrow checker issues with
// simultaneous mutable access to fleets.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use iac_shared::constants::{ShipClass, DAMAGE_VARIANCE_MIN, DAMAGE_VARIANCE_MAX};
use iac_shared::protocol::GameEvent;

use crate::engine::Ship;

/// Copy of ship data for combat resolution (avoids borrow issues).
#[derive(Clone, Copy)]
pub struct CombatShip {
    pub id: u64,
    pub ship_class: ShipClass,
    pub weapon_power: f32,
    pub hull: f32,
    pub hull_max: f32,
    pub shield: f32,
    pub shield_max: f32,
}

impl From<&Ship> for CombatShip {
    fn from(s: &Ship) -> Self {
        CombatShip {
            id: s.id,
            ship_class: s.ship_class,
            weapon_power: s.weapon_power,
            hull: s.hull,
            hull_max: s.hull_max,
            shield: s.shield,
            shield_max: s.shield_max,
        }
    }
}

#[allow(dead_code)]
/// A single combat side (player fleet or NPC fleet).
pub struct CombatSide {
    pub fleet_id: u64,
    pub is_npc: bool,
    pub ships: Vec<CombatShip>,
}

/// Result of resolving a single combat round.
pub struct CombatRoundResult {
    pub events: Vec<GameEvent>,
    pub player_ships: Vec<Vec<CombatShip>>,  // updated ship states per player fleet
    pub npc_ships: Vec<Vec<CombatShip>>,     // updated ship states per npc fleet
    pub concluded: bool,
    pub player_won: bool,
}

/// Resolve one combat round between player and NPC fleets.
/// Takes copied ship data, returns updated ship data + events.
///
/// Fire order matches the Zig original: all player ships fire first
/// (with rapid-fire chains), damage lands immediately, then surviving
/// NPC ships return fire against the updated player state.
pub fn resolve_combat_round(
    player_sides: &[CombatSide],
    npc_sides: &[CombatSide],
    npc_value: iac_shared::Resources,
    tick: u64,
    round: u32,
) -> CombatRoundResult {
    let seed = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64)
        ^ round as u64;
    let mut rng = StdRng::seed_from_u64(seed);

    let mut events: Vec<GameEvent> = Vec::new();

    // Clone ship data so we can mutate freely
    let mut player_ships: Vec<Vec<CombatShip>> = player_sides.iter()
        .map(|s| s.ships.clone()).collect();
    let mut npc_ships: Vec<Vec<CombatShip>> = npc_sides.iter()
        .map(|s| s.ships.clone()).collect();

    // Target index lists: (side_idx, ship_idx). Dead ships are skipped at
    // selection time, so fire redistributes to survivors as ships die.
    let npc_targets: Vec<(usize, usize)> = all_indices(&npc_ships);
    let player_targets: Vec<(usize, usize)> = all_indices(&player_ships);

    // Player ships fire at NPC ships, damage applied immediately.
    // Indexing avoids holding borrows of the ship vecs across fire_ship's mutable access.
    #[allow(clippy::needless_range_loop)]
    for psi in 0..player_ships.len() {
        for ai in 0..player_ships[psi].len() {
            let attacker = player_ships[psi][ai];
            if attacker.hull <= 0.0 { continue; }
            fire_ship(&attacker, &npc_targets, &mut npc_ships, npc_sides, tick, &mut rng, &mut events);
        }
    }

    // Surviving NPC ships return fire; NPCs killed above do not shoot.
    // Indexing avoids holding borrows of the ship vecs across fire_ship's mutable access.
    #[allow(clippy::needless_range_loop)]
    for nsi in 0..npc_ships.len() {
        for ai in 0..npc_ships[nsi].len() {
            let attacker = npc_ships[nsi][ai];
            if attacker.hull <= 0.0 { continue; }
            fire_ship(&attacker, &player_targets, &mut player_ships, player_sides, tick, &mut rng, &mut events);
        }
    }

    // Compact destroyed ships
    for ships in player_ships.iter_mut() {
        compact_ships(ships);
    }
    for ships in npc_ships.iter_mut() {
        compact_ships(ships);
    }

    let any_player_alive = player_ships.iter().any(|s| !s.is_empty());
    let any_npc_alive = npc_ships.iter().any(|s| !s.is_empty());
    let concluded = !any_player_alive || !any_npc_alive;

    if concluded {
        if !any_npc_alive {
            for side in npc_sides.iter() {
                events.push(GameEvent {
                    tick,
                    kind: iac_shared::protocol::EventKind::FleetDestroyed(
                        iac_shared::protocol::FleetDestroyedEvent {
                            fleet_id: side.fleet_id,
                            is_npc: true,
                            salvage: npc_value,
                        },
                    ),
                });
            }
        }
        for (i, side) in player_sides.iter().enumerate() {
            if player_ships[i].is_empty() {
                events.push(GameEvent {
                    tick,
                    kind: iac_shared::protocol::EventKind::FleetDestroyed(
                        iac_shared::protocol::FleetDestroyedEvent {
                            fleet_id: side.fleet_id,
                            is_npc: false,
                            salvage: iac_shared::Resources::default(),
                        },
                    ),
                });
            }
        }
    }

    CombatRoundResult {
        events,
        player_ships,
        npc_ships,
        concluded,
        player_won: concluded && any_player_alive,
    }
}

fn all_indices(ships: &[Vec<CombatShip>]) -> Vec<(usize, usize)> {
    ships.iter().enumerate()
        .flat_map(|(si, side)| (0..side.len()).map(move |ti| (si, ti)))
        .collect()
}

/// Fire one ship, chaining extra shots while rapid-fire rolls succeed.
fn fire_ship(
    attacker: &CombatShip,
    targets: &[(usize, usize)],
    target_ships: &mut [Vec<CombatShip>],
    target_sides: &[CombatSide],
    tick: u64,
    rng: &mut StdRng,
    events: &mut Vec<GameEvent>,
) {
    loop {
        let Some((si, ti)) = select_target(targets, target_ships, rng) else { return; };

        let damage = roll_damage(attacker.weapon_power, rng);
        let target = &mut target_ships[si][ti];
        let result = apply_damage(target, damage);
        let rapid = check_rapid_fire(attacker.ship_class, target.ship_class, rng);

        emit_combat_event(events, tick, attacker.id, target.id, damage, &result, rapid);

        if target.hull <= 0.0 {
            let target_id = target.id;
            let target_class = target.ship_class;
            emit_ship_destroyed(
                events, tick, target_id, target_class,
                target_sides[si].fleet_id, target_sides[si].is_npc,
            );
        }

        if !rapid { break; }
    }
}

struct DamageResult {
    shield_absorbed: f32,
    hull_damage: f32,
}

/// Weighted random target selection (weight = hull_max), skipping dead ships.
fn select_target(targets: &[(usize, usize)], all_ships: &[Vec<CombatShip>], rng: &mut StdRng) -> Option<(usize, usize)> {
    let total_weight: f32 = targets.iter()
        .map(|&(si, ti)| &all_ships[si][ti])
        .filter(|s| s.hull > 0.0)
        .map(|s| s.hull_max)
        .sum();
    if total_weight <= 0.0 { return None; }

    let mut roll = rng.random_range(0.0..1.0) * total_weight;
    for &(si, ti) in targets.iter() {
        if all_ships[si][ti].hull <= 0.0 { continue; }
        roll -= all_ships[si][ti].hull_max;
        if roll <= 0.0 { return Some((si, ti)); }
    }

    // Fallback: last alive
    targets.iter().rev()
        .find(|&&(si, ti)| all_ships[si][ti].hull > 0.0)
        .copied()
}

fn apply_damage(target: &mut CombatShip, damage: f32) -> DamageResult {
    let shield_absorbed = damage.min(target.shield);
    target.shield -= shield_absorbed;

    let passthrough = damage - shield_absorbed;
    let hull_damage = passthrough.min(target.hull);
    target.hull -= hull_damage;

    DamageResult {
        shield_absorbed,
        hull_damage,
    }
}

fn roll_damage(weapon_power: f32, rng: &mut StdRng) -> f32 {
    let variance = DAMAGE_VARIANCE_MIN
        + rng.random_range(0.0..1.0) * (DAMAGE_VARIANCE_MAX - DAMAGE_VARIANCE_MIN);
    weapon_power * variance
}

fn check_rapid_fire(attacker_class: ShipClass, target_class: ShipClass, rng: &mut StdRng) -> bool {
    let rf = attacker_class.rapid_fire_vs(target_class);
    if rf == 0 { return false; }
    rng.random_range(0.0..1.0) < (1.0 - 1.0 / rf as f32)
}

fn compact_ships(ships: &mut Vec<CombatShip>) {
    ships.retain(|s| s.hull > 0.0);
}

fn emit_combat_event(
    events: &mut Vec<GameEvent>,
    tick: u64,
    attacker_id: u64,
    target_id: u64,
    damage: f32,
    result: &DamageResult,
    rapid: bool,
) {
    events.push(GameEvent {
        tick,
        kind: iac_shared::protocol::EventKind::CombatRound(
            iac_shared::protocol::CombatRoundEvent {
                attacker_ship_id: attacker_id,
                target_ship_id: target_id,
                damage,
                shield_absorbed: result.shield_absorbed,
                hull_damage: result.hull_damage,
                rapid_fire: rapid,
            },
        ),
    });
}

fn emit_ship_destroyed(
    events: &mut Vec<GameEvent>,
    tick: u64,
    ship_id: u64,
    ship_class: ShipClass,
    fleet_id: u64,
    is_npc: bool,
) {
    events.push(GameEvent {
        tick,
        kind: iac_shared::protocol::EventKind::ShipDestroyed(
            iac_shared::protocol::ShipDestroyedEvent {
                ship_id,
                ship_class,
                owner_fleet_id: fleet_id,
                is_npc,
            },
        ),
    });
}
