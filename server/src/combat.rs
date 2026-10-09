// Round-based combat resolution with RNG, target selection, damage calculation.
// Ported from Zig combat.zig.
//
// Works with copied ship data to avoid borrow checker issues with
// simultaneous mutable access to fleets.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use iac_shared::constants::{ShipClass, DAMAGE_VARIANCE_MIN, DAMAGE_VARIANCE_MAX};
use iac_shared::hex::Hex;
use iac_shared::protocol::{
    CombatRoundEvent, EventKind, FleetDestroyedEvent, GameEvent, ShipDestroyedEvent,
};
use iac_shared::Resources;

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

/// A single combat side (player fleet or NPC fleet).
pub struct CombatSide {
    pub fleet_id: u64,
    pub is_npc: bool,
    /// Owning empire's name; None for NPCs.
    pub owner: Option<String>,
    /// Wreckage this side leaves if it is destroyed by players.
    pub salvage: Resources,
    /// What an NPC group is called in messages; empty for players.
    pub label: String,
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
    sector: Hex,
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
            let shot = Shot { sector, tick, attacker_side: &player_sides[psi], target_sides: npc_sides };
            fire_ship(&attacker, &shot, &npc_targets, &mut npc_ships, &mut rng, &mut events);
        }
    }

    // Surviving NPC ships return fire; NPCs killed above do not shoot.
    // Indexing avoids holding borrows of the ship vecs across fire_ship's mutable access.
    #[allow(clippy::needless_range_loop)]
    for nsi in 0..npc_ships.len() {
        for ai in 0..npc_ships[nsi].len() {
            let attacker = npc_ships[nsi][ai];
            if attacker.hull <= 0.0 { continue; }
            let shot = Shot { sector, tick, attacker_side: &npc_sides[nsi], target_sides: player_sides };
            fire_ship(&attacker, &shot, &player_targets, &mut player_ships, &mut rng, &mut events);
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

    if concluded && !any_npc_alive {
        for side in npc_sides.iter() {
            events.push(fleet_destroyed(tick, sector, side, side.salvage));
        }
    }
    for (i, side) in player_sides.iter().enumerate() {
        if !side.ships.is_empty() && player_ships[i].is_empty() {
            events.push(fleet_destroyed(tick, sector, side, Resources::default()));
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

/// Who fires at whom, and where: the context every shot event needs.
struct Shot<'a> {
    sector: Hex,
    tick: u64,
    attacker_side: &'a CombatSide,
    target_sides: &'a [CombatSide],
}

/// Fire one ship, chaining extra shots while rapid-fire rolls succeed.
fn fire_ship(
    attacker: &CombatShip,
    shot: &Shot,
    targets: &[(usize, usize)],
    target_ships: &mut [Vec<CombatShip>],
    rng: &mut StdRng,
    events: &mut Vec<GameEvent>,
) {
    loop {
        let Some((si, ti)) = select_target(targets, target_ships, rng) else { return; };

        let damage = roll_damage(attacker.weapon_power, rng);
        let target = &mut target_ships[si][ti];
        let result = apply_damage(target, damage);
        let rapid = check_rapid_fire(attacker.ship_class, target.ship_class, rng);
        let target_side = &shot.target_sides[si];

        events.push(GameEvent {
            tick: shot.tick,
            kind: EventKind::CombatRound(CombatRoundEvent {
                sector: shot.sector,
                attacker_fleet_id: shot.attacker_side.fleet_id,
                attacker_owner: shot.attacker_side.owner.clone(),
                attacker_ship_id: attacker.id,
                target_fleet_id: target_side.fleet_id,
                target_owner: target_side.owner.clone(),
                target_ship_id: target.id,
                damage,
                shield_absorbed: result.shield_absorbed,
                hull_damage: result.hull_damage,
                rapid_fire: rapid,
                mine: false,
            }),
        });

        if target.hull <= 0.0 {
            events.push(GameEvent {
                tick: shot.tick,
                kind: EventKind::ShipDestroyed(ShipDestroyedEvent {
                    ship_id: target.id,
                    ship_class: target.ship_class,
                    owner_fleet_id: target_side.fleet_id,
                    is_npc: target_side.is_npc,
                    sector: shot.sector,
                    owner: target_side.owner.clone(),
                    mine: false,
                }),
            });
        }

        if !rapid { break; }
    }
}

fn fleet_destroyed(tick: u64, sector: Hex, side: &CombatSide, salvage: Resources) -> GameEvent {
    GameEvent {
        tick,
        kind: EventKind::FleetDestroyed(FleetDestroyedEvent {
            fleet_id: side.fleet_id,
            is_npc: side.is_npc,
            sector,
            owner: side.owner.clone(),
            mine: false,
            salvage,
            label: side.label.clone(),
        }),
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

#[cfg(test)]
mod tests {
    //! Win rates by power ratio: the evidence behind `scaling::ratio_label`.
    use super::*;
    use iac_shared::scaling::ship_power;

    fn group(class: ShipClass, mult: f32, power: f32, id0: u64) -> Vec<CombatShip> {
        let s = class.base_stats();
        let each = ship_power(class) * mult;
        let n = (power / each).round().max(1.0) as usize;
        (0..n)
            .map(|i| CombatShip {
                id: id0 + i as u64,
                ship_class: class,
                weapon_power: s.weapon * mult,
                hull: s.hull * mult,
                hull_max: s.hull * mult,
                shield: s.shield * mult,
                shield_max: s.shield * mult,
            })
            .collect()
    }

    fn side(ships: Vec<CombatShip>, npc: bool) -> CombatSide {
        CombatSide {
            fleet_id: u64::from(npc),
            is_npc: npc,
            owner: None,
            salvage: Resources::default(),
            label: String::new(),
            ships,
        }
    }

    /// (win rate, mean share of own hull left after a win) over `trials` fights.
    fn fight(player: &[CombatShip], npc: &[CombatShip], trials: u32) -> (f32, f32) {
        let (mut wins, mut kept) = (0u32, 0.0f32);
        for t in 0..trials {
            let p = vec![side(player.to_vec(), false)];
            let n = vec![side(npc.to_vec(), true)];
            let (mut ps, mut ns) = (p, n);
            for round in 0..200u32 {
                let r = resolve_combat_round(&ps, &ns, Hex::new(0, 0), 0, round.wrapping_add(t * 1000));
                for (s, ships) in ps.iter_mut().zip(r.player_ships) { s.ships = ships; }
                for (s, ships) in ns.iter_mut().zip(r.npc_ships) { s.ships = ships; }
                if r.concluded {
                    if r.player_won {
                        wins += 1;
                        let max: f32 = player.iter().map(|s| s.hull_max).sum();
                        kept += ps[0].ships.iter().map(|s| s.hull).sum::<f32>() / max;
                    }
                    break;
                }
            }
        }
        (wins as f32 / trials as f32, if wins > 0 { kept / wins as f32 } else { 0.0 })
    }

    fn power_of(ships: &[CombatShip]) -> f32 {
        ships.iter().map(|s| s.weapon_power + (s.hull + s.shield) / 10.0).sum()
    }

    /// Fights of `class` against a same-class group at a ratio near `want`.
    fn at_ratio(class: ShipClass, want: f32) -> (f32, f32, f32) {
        let npc = group(class, 1.0, 150.0, 1000);
        let me = group(class, 1.0, power_of(&npc) * want, 1);
        let (win, kept) = fight(&me, &npc, 60);
        (power_of(&me) / power_of(&npc), win, kept)
    }

    #[test]
    fn label_bands_follow_what_a_fight_costs() {
        use iac_shared::scaling::{RATIO_EVEN, RATIO_FAVOURABLE, RATIO_RISKY, RATIO_SAFE};
        for class in [ShipClass::Scout, ShipClass::Corvette] {
            let (r, win, _) = at_ratio(class, RATIO_RISKY - 0.15);
            assert!(r < RATIO_RISKY && win < 0.1, "{class:?} DEADLY side: ratio {r} wins {win}");
            let (r, win, _) = at_ratio(class, RATIO_EVEN + 0.05);
            assert!(r >= RATIO_EVEN && win > 0.95, "{class:?} EVEN floor: ratio {r} wins {win}");
            let (r, _, kept) = at_ratio(class, RATIO_EVEN + 0.05);
            assert!(kept < 0.7, "{class:?} an EVEN win must hurt: ratio {r} keeps {kept}");
            let (r, _, kept) = at_ratio(class, RATIO_FAVOURABLE + 0.05);
            assert!(kept > 0.75, "{class:?} FAVOURABLE floor: ratio {r} keeps {kept}");
            let (r, _, kept) = at_ratio(class, RATIO_SAFE + 0.05);
            assert!(kept > 0.97, "{class:?} SAFE floor: ratio {r} keeps {kept}");
        }
    }

    #[test]
    #[ignore = "prints the table behind the ratio bands"]
    fn print_win_rates_by_ratio() {
        let matchups: [(&str, ShipClass, f32, ShipClass, f32); 6] = [
            ("scouts v corvettes", ShipClass::Scout, 1.0, ShipClass::Corvette, 0.72),
            ("corvettes v frigates", ShipClass::Corvette, 1.0, ShipClass::Frigate, 0.9),
            ("scouts v scouts", ShipClass::Scout, 1.0, ShipClass::Scout, 0.8),
            ("corvettes v corvettes", ShipClass::Corvette, 1.0, ShipClass::Corvette, 1.0),
            ("corvettes v scouts", ShipClass::Corvette, 1.0, ShipClass::Scout, 0.8),
            ("frigates v corvettes", ShipClass::Frigate, 1.0, ShipClass::Corvette, 1.0),
        ];
        for (name, pc, pm, nc, nm) in matchups {
            for target in [60.0f32, 160.0] {
                let npc = group(nc, nm, target, 1000);
                let npc_power: f32 = npc.iter().map(|s| s.weapon_power + (s.hull + s.shield) / 10.0).sum();
                for ratio in [0.7f32, 0.9, 1.0, 1.1, 1.2, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0] {
                    let me = group(pc, pm, npc_power * ratio, 1);
                    let mine: f32 = me.iter().map(|s| s.weapon_power + (s.hull + s.shield) / 10.0).sum();
                    let (w, k) = fight(&me, &npc, 400);
                    println!("{name:24} npc {npc_power:6.0} ratio {:4.2} win {:5.1}% hull-kept {:4.0}%", mine / npc_power, w * 100.0, k * 100.0);
                }
            }
        }
    }
}
