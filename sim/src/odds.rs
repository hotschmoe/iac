// What a fight is expected to cost each side.
//
// There is no second model of the fight. `outlook` plays `combat::round`
// itself, `ODDS_RUNS` times or more with dice that are the same every time
// it is asked, from the two sides as they stand, damage and arms included,
// until one side is gone, a round takes no hull, or `ODDS_ROUNDS` have
// passed. Two large sides are scaled down together first, so that the larger
// has at most `ODDS_UNITS` units; two small ones are played more often for
// the same work. The result is the share of its worth each side keeps (a
// ship counts its cost at the score's weights times the share of its hull
// it holds) and how often the first side won and was destroyed.
// `scaling::outcome_ratio` turns that into the ratio the labels are cut from.

use iac_shared::constants::{ShipClass, ODDS_ROUNDS, ODDS_RUNS, ODDS_RUNS_MOST, ODDS_UNITS};
use iac_shared::scaling::{self, Arms, ShipyardItem};

use crate::combat::{self, Dice, Squad, Troop};

const DICE: u64 = 0x0DD5_0FA7_F1E3_7A11;
const RUN: u64 = 0xD6E8_FEB8_6659_FD93;

/// One side of a fight: troops, each with what it fights with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Force {
    troops: Vec<Troop>,
}

impl Force {
    pub fn of(arms: Arms, squads: Vec<Squad>) -> Force {
        let mut force = Force::default();
        if squads.iter().any(|s| s.count > 0) {
            force.troops.push(Troop { arms, squads });
        }
        force
    }

    /// `count` undamaged ships of `class` with every figure scaled by `multiplier`.
    pub fn of_group(class: ShipClass, count: u32, multiplier: f32) -> Force {
        Force::of(
            Arms::of_grade(multiplier),
            vec![Squad { unit: ShipyardItem::Ship(class), count, damaged: Vec::new() }],
        )
    }

    /// The group world generation puts in a sector `dist` rings out.
    pub fn of_ring(dist: u16) -> Force {
        let (class, count, multiplier) = scaling::npc_composition(dist);
        Force::of_group(class, u32::from(count), multiplier)
    }

    pub fn join(&mut self, other: &Force) {
        self.troops.extend(other.troops.iter().cloned());
    }

    pub fn is_empty(&self) -> bool {
        self.troops.is_empty()
    }
}

/// Share of its starting worth each side is expected to keep, and how the
/// fights played ended for the first side: the share it won (the other
/// side destroyed and itself not) and the share in which it was destroyed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outlook {
    pub ours: f32,
    pub theirs: f32,
    pub won: f32,
    pub lost: f32,
}

impl Outlook {
    pub fn ratio(self) -> f32 {
        scaling::outcome_ratio(self.ours, self.theirs, self.won, self.lost)
    }
}

fn units(troops: &[Troop]) -> u32 {
    troops.iter().map(Troop::units).sum()
}

fn squads_worth(arms: Arms, squads: &[Squad]) -> f64 {
    squads
        .iter()
        .map(|squad| {
            let each = f64::from(scaling::resource_weight(&squad.unit.unit_cost()));
            let full = f64::from(scaling::unit_figures(squad.unit, arms).hull.max(1));
            let whole = f64::from(squad.count) - squad.damaged.len() as f64;
            let held: f64 = squad.damaged.iter().map(|&hull| f64::from(hull) / full).sum();
            each * (whole + held)
        })
        .sum()
}

fn worth(troops: &[Troop]) -> f64 {
    troops.iter().map(|troop| squads_worth(troop.arms, &troop.squads)).sum()
}

/// `troops` with every squad at `units` in `of` of its count, and never
/// fewer than one unit: a kind that is there stays there. The damaged are
/// kept in proportion, picked evenly from the least hull to the most.
fn scaled(troops: &[Troop], units: u32, of: u32) -> Vec<Troop> {
    let part = |count: u64| ((count * u64::from(units) + u64::from(of) / 2) / u64::from(of)) as u32;
    troops
        .iter()
        .map(|troop| Troop {
            arms: troop.arms,
            squads: troop
                .squads
                .iter()
                .filter(|squad| squad.count > 0)
                .map(|squad| {
                    let count = part(u64::from(squad.count)).max(1);
                    let listed = squad.damaged.len();
                    let kept = (part(listed as u64) as usize).min(count as usize).min(listed);
                    let damaged = (0..kept).map(|i| squad.damaged[(2 * i + 1) * listed / (2 * kept)]).collect();
                    Squad { unit: squad.unit, count, damaged }
                })
                .collect(),
        })
        .collect()
}

fn after(before: &[Troop], squads: Vec<Vec<Squad>>) -> Vec<Troop> {
    before.iter().zip(squads).map(|(troop, squads)| Troop { arms: troop.arms, squads }).collect()
}

pub fn outlook(ours: &Force, theirs: &Force) -> Outlook {
    if theirs.is_empty() {
        return Outlook { ours: 1.0, theirs: 0.0, won: 1.0, lost: 0.0 };
    }
    if ours.is_empty() {
        return Outlook { ours: 0.0, theirs: 1.0, won: 0.0, lost: 1.0 };
    }
    let larger = units(&ours.troops).max(units(&theirs.troops));
    let (a, b) = if larger > ODDS_UNITS {
        (scaled(&ours.troops, ODDS_UNITS, larger), scaled(&theirs.troops, ODDS_UNITS, larger))
    } else {
        (ours.troops.clone(), theirs.troops.clone())
    };
    let runs = (ODDS_RUNS * ODDS_UNITS / larger.max(1)).clamp(ODDS_RUNS, ODDS_RUNS_MOST);
    let (a_worth, b_worth) = (worth(&a), worth(&b));
    let (mut a_kept, mut b_kept) = (0.0, 0.0);
    let (mut won, mut lost) = (0u32, 0u32);
    for run in 0..runs {
        let mut dice = Dice::new(DICE ^ u64::from(run).wrapping_mul(RUN));
        let (mut x, mut y) = (a.clone(), b.clone());
        for _ in 0..ODDS_ROUNDS {
            if units(&x) == 0 || units(&y) == 0 {
                break;
            }
            let fought = combat::round(&x, &y, &mut dice);
            let quiet = !fought.hull_lost();
            x = after(&x, fought.fleets.troops);
            y = after(&y, fought.hostiles.troops);
            if quiet {
                break;
            }
        }
        a_kept += worth(&x);
        b_kept += worth(&y);
        won += u32::from(units(&y) == 0 && units(&x) > 0);
        lost += u32::from(units(&x) == 0);
    }
    let share = |kept: f64, start: f64| if start > 0.0 { (kept / (start * f64::from(runs))) as f32 } else { 0.0 };
    Outlook {
        ours: share(a_kept, a_worth),
        theirs: share(b_kept, b_worth),
        won: won as f32 / runs as f32,
        lost: lost as f32 / runs as f32,
    }
}
