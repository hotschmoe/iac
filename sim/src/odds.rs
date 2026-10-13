// What a fight is expected to cost each side.
//
// `outlook` plays `combat::resolve_combat_round` on averages instead of dice:
// the first side fires, then what is left of the second fires back, round
// after round. Ships of a class form one stack. Every shot picks a live ship
// by hull size, so a ship's hit count is a random number whose mean and
// spread follow from the shots fired at its stack; a ship dies when its hits
// outweigh hull and shield, and shields soak a hit before hull does. Rapid
// fire lengthens a ship's burst and makes it burstier. The result is the share
// of its starting hull each side is expected to keep; `scaling::outcome_ratio`
// turns that into the ratio the labels are cut from.

use std::hash::{Hash, Hasher};

use iac_shared::constants::{ShipClass, DAMAGE_VARIANCE_MAX, DAMAGE_VARIANCE_MIN};
use iac_shared::{maths, scaling};

use crate::engine::Ship;

const CLASSES: usize = ShipClass::ALL.len();
const MAX_ROUNDS: usize = 64;
/// A volley lands in this many parts, so ships that die early in it stop drawing fire.
const VOLLEY_STEPS: usize = 2;
/// Expected ships left below which a side is beaten.
const BEATEN: f64 = 0.02;
const MAX_TERMS: usize = 24;
const DAMAGE_SPREAD: f64 = (DAMAGE_VARIANCE_MAX - DAMAGE_VARIANCE_MIN) as f64 / 2.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Stack {
    count: f32,
    hull: f32,
    shield: f32,
    weight: f32,
    weapon: f32,
}

/// The ships of one side, summed by class.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Force {
    stacks: [Stack; CLASSES],
}

impl Eq for Force {}

impl Hash for Force {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for s in &self.stacks {
            for v in [s.count, s.hull, s.shield, s.weight, s.weapon] {
                state.write_u32(v.to_bits());
            }
        }
    }
}

impl Force {
    pub fn of_ships<'a>(ships: impl IntoIterator<Item = &'a Ship>) -> Force {
        let mut force = Force::default();
        for ship in ships {
            if ship.hull > 0.0 {
                force.add(ship.ship_class, 1.0, ship.hull, ship.shield, ship.hull_max, ship.weapon_power);
            }
        }
        force
    }

    /// `count` undamaged ships of `class` with every stat scaled by `multiplier`.
    pub fn of_group(class: ShipClass, count: f32, multiplier: f32) -> Force {
        let mut force = Force::default();
        force.add_group(class, count, multiplier);
        force
    }

    /// The group world generation puts in a sector `dist` rings out.
    pub fn of_ring(dist: u16) -> Force {
        let (class, count, multiplier) = scaling::npc_composition(dist);
        Force::of_group(class, f32::from(count), multiplier)
    }

    pub fn add_group(&mut self, class: ShipClass, count: f32, multiplier: f32) {
        let s = class.base_stats();
        let m = multiplier * count;
        self.add(class, count, s.hull * m, s.shield * m, s.hull * m, s.weapon * m);
    }

    fn add(&mut self, class: ShipClass, count: f32, hull: f32, shield: f32, weight: f32, weapon: f32) {
        let s = &mut self.stacks[class as usize];
        s.count += count;
        s.hull += hull;
        s.shield += shield;
        s.weight += weight;
        s.weapon += weapon;
    }

    pub fn join(&mut self, other: &Force) {
        for (class, s) in ShipClass::ALL.into_iter().zip(&other.stacks) {
            self.add(class, s.count, s.hull, s.shield, s.weight, s.weapon);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stacks.iter().all(|s| s.count <= 0.0 || s.hull <= 0.0)
    }
}

/// Share of its starting hull each side is expected to keep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outlook {
    pub ours: f32,
    pub theirs: f32,
}

impl Outlook {
    pub fn ratio(self) -> f32 {
        scaling::outcome_ratio(self.ours, self.theirs)
    }
}

/// One class on one side while the fight is played: per-ship means, and what
/// the average ship has taken so far.
#[derive(Clone, Copy, Default)]
struct Unit {
    count: f64,
    hull: f64,
    shield: f64,
    weight: f64,
    weapon: f64,
    hits: f64,
    damage: f64,
    spread: f64,
    alive: f64,
    kept: f64,
}

type Side = [Unit; CLASSES];

fn side(force: &Force) -> Side {
    let mut units = [Unit::default(); CLASSES];
    for (unit, s) in units.iter_mut().zip(&force.stacks) {
        if s.count <= 0.0 || s.hull <= 0.0 {
            continue;
        }
        let n = f64::from(s.count);
        *unit = Unit {
            count: n,
            hull: f64::from(s.hull) / n,
            shield: f64::from(s.shield) / n,
            weight: f64::from(s.weight) / n,
            weapon: f64::from(s.weapon) / n,
            alive: 1.0,
            kept: 1.0,
            ..Unit::default()
        };
    }
    units
}

fn ships_left(side: &Side) -> f64 {
    side.iter().map(|u| u.count * u.alive).sum()
}

fn hull_kept(side: &Side) -> f32 {
    let total: f64 = side.iter().map(|u| u.count * u.hull).sum();
    if total <= 0.0 {
        return 0.0;
    }
    (side.iter().map(|u| u.count * u.hull * u.kept).sum::<f64>() / total) as f32
}

/// `ours` fires first, as player fleets do.
pub fn outlook(ours: &Force, theirs: &Force) -> Outlook {
    let mut a = side(ours);
    let mut b = side(theirs);
    for _ in 0..MAX_ROUNDS {
        let fired = volley(&a, &mut b);
        if ships_left(&b) < BEATEN {
            break;
        }
        let answered = volley(&b, &mut a);
        if ships_left(&a) < BEATEN || !(fired || answered) {
            break;
        }
    }
    Outlook { ours: hull_kept(&a), theirs: hull_kept(&b) }
}

/// One side's fire for a round. False when nothing was fired or nothing is left to hit.
fn volley(from: &Side, at: &mut Side) -> bool {
    let weight = |at: &Side| at.iter().map(|t| t.count * t.alive * t.weight).sum::<f64>();
    let total = weight(at);
    if total <= 0.0 {
        return false;
    }
    let (mut shots, mut damage, mut burst) = (0.0, 0.0, 0.0);
    for (i, f) in from.iter().enumerate() {
        let firing = f.count * f.alive;
        if firing <= 0.0 || f.weapon <= 0.0 {
            continue;
        }
        let again: f64 = at
            .iter()
            .enumerate()
            .map(|(j, t)| {
                let rapid = scaling::rapid_fire_vs(ShipClass::ALL[i], ShipClass::ALL[j]);
                if rapid == 0 { 0.0 } else { t.count * t.alive * t.weight / total * (1.0 - 1.0 / f64::from(rapid)) }
            })
            .sum();
        let length = 1.0 / (1.0 - again);
        shots += firing * length;
        damage += firing * length * f.weapon;
        burst += firing * length * (length - 2.0);
    }
    if shots <= 0.0 {
        return false;
    }
    let part = 1.0 / VOLLEY_STEPS as f64;
    for _ in 0..VOLLEY_STEPS {
        let total = weight(at);
        if total <= 0.0 {
            break;
        }
        for t in at.iter_mut().filter(|t| t.count * t.alive > 0.0) {
            let share = t.weight / (total - t.alive * t.weight + t.weight);
            t.hits += shots * share * part;
            t.damage += damage * share * part;
            t.spread += (shots + burst * share) * share * part;
            settle(t);
        }
    }
    true
}

/// Recompute how many of a unit's ships live and how much hull they hold,
/// from the hits the average ship has taken.
fn settle(unit: &mut Unit) {
    if unit.hits <= 0.0 {
        return;
    }
    let shot = unit.damage / unit.hits;
    let dispersion = (unit.spread / unit.hits).max(0.0);
    let mut alive = 0.0;
    let mut kept = 0.0;
    let reach = (unit.hull + unit.shield) / shot;
    let mut tally = |hits: f64, chance: f64| -> bool {
        let taken = hits * shot;
        let over = taken - (unit.hull + unit.shield);
        let band_sq = DAMAGE_SPREAD * DAMAGE_SPREAD * shot * shot * hits;
        let survives = if over * over < band_sq {
            0.5 - over / (2.0 * band_sq.sqrt())
        } else if over < 0.0 || hits <= 0.0 {
            1.0
        } else {
            return false;
        };
        let left = (unit.hull - (taken - unit.shield).max(0.0)).clamp(0.0, unit.hull);
        alive += chance * survives;
        kept += chance * survives * left / unit.hull;
        true
    };
    if dispersion < 0.02 {
        tally(unit.hits, 1.0);
    } else if dispersion < 0.98 && reach <= (MAX_TERMS - 4) as f64 {
        // Few targets share the fire: a binomial count, less spread than Poisson.
        let p = 1.0 - dispersion;
        let trials = unit.hits / p;
        let odds = p / dispersion;
        let mut chance = maths::exp(trials * maths::ln(dispersion));
        let mut sum = 0.0;
        let mut k = 0.0;
        let mut counting = true;
        while chance > 0.0 {
            sum += chance;
            counting = counting && tally(k, chance);
            if !counting && trials > MAX_TERMS as f64 {
                sum = 1.0;
                break;
            }
            chance *= (trials - k) / (k + 1.0) * odds;
            k += 1.0;
        }
        if sum > 0.0 {
            alive /= sum;
            kept /= sum;
        }
    } else {
        // Many targets, or rapid fire: hits arrive Poisson, in clumps when
        // bursts land together or a ship takes too many hits to count singly;
        // the part of the count that does not vary comes first.
        let clump = dispersion.max(1.0).max(reach / (MAX_TERMS - 4) as f64);
        let mean = dispersion * unit.hits / (clump * clump);
        let fixed = unit.hits - clump * mean;
        let mut chance = maths::exp(-mean);
        for k in 0..MAX_TERMS + 8 {
            if !tally(fixed + k as f64 * clump, chance) {
                break;
            }
            chance *= mean / (k + 1) as f64;
        }
    }
    unit.alive = alive.clamp(0.0, 1.0);
    unit.kept = kept.clamp(0.0, 1.0);
}
