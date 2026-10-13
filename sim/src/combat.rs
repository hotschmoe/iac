// One round of a fight, as classic OGame fights it (docs/design/ogame.md,
// section 7.1).
//
//   1. every unit's shield is full
//   2. every unit of both sides fires, at once: a unit destroyed earlier in
//      the round still fires
//   3. a shot picks one unit of the other side, each as likely as another,
//      one already destroyed this round included (that shot is wasted)
//        the target has no shield left   the attack comes off its hull
//        otherwise                       n = attack * 100 / full shield, cut off
//          n is 0                        the shot bounces
//          shield left < n hundredths    the shield breaks, the rest comes off the hull
//          otherwise                     the shield loses n hundredths
//        no shield left, and the hull at or under EXPLODE_BELOW_PCT hundredths
//                                        it explodes with chance 1 - hull / full hull
//        rapid fire r against the target the unit fires again with chance (r - 1) / r
//   4. what exploded is gone when the round ends
//
// Whole numbers from end to end, with dice of its own, so a round is the same
// bytes on every target. It knows nothing of fleets, owners or sectors: it is
// handed two sides of troops and hands them back with what was fired.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use iac_shared::constants::EXPLODE_BELOW_PCT;
use iac_shared::hex::Hex;
use iac_shared::protocol::{BattleUnits, Volley};
use iac_shared::scaling::{self, Arms, ShipyardItem, UnitFigures};

const KINDS: usize = ShipyardItem::COUNT;

/// Units of one kind: how many, and the hull of each that is not whole,
/// ascending.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Squad {
    pub unit: ShipyardItem,
    pub count: u32,
    pub damaged: Vec<u32>,
}

/// One fleet, group or battery in a fight: its squads in catalogue order,
/// and what they fight with.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Troop {
    pub arms: Arms,
    pub squads: Vec<Squad>,
}

impl Troop {
    pub fn units(&self) -> u32 {
        self.squads.iter().map(|s| s.count).sum()
    }
}

/// The dice of a round: SplitMix64, which is the same sequence wherever it runs.
pub struct Dice(u64);

impl Dice {
    pub fn new(seed: u64) -> Dice {
        Dice(seed)
    }

    /// The dice of round `round` of the fight in `sector` on `tick`. Nothing
    /// of them is stored: a world stopped between two rounds fights the next
    /// as it would have.
    pub fn of_round(world_seed: u64, sector: Hex, tick: u64, round: u32) -> Dice {
        Dice(
            world_seed
                ^ tick.wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ u64::from(sector.to_key()).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
                ^ u64::from(round).wrapping_mul(0x1656_67B1_9E37_79F9),
        )
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A whole number below `n`, each as likely as another.
    fn below(&mut self, n: u32) -> u32 {
        (((self.next() >> 32) * u64::from(n)) >> 32) as u32
    }
}

/// One side after a round, and what it fired.
#[derive(Debug, Clone, Default)]
pub struct SideReport {
    /// Each troop as it stands, in the order given.
    pub troops: Vec<Vec<Squad>>,
    /// The same by kind, with what the round destroyed.
    pub standing: Vec<Vec<BattleUnits>>,
    pub volleys: Vec<Volley>,
    pub shots: u64,
    pub power: u64,
    pub absorbed: u64,
    pub hull_damage: u64,
    pub destroyed: u32,
}

impl SideReport {
    pub fn units(&self) -> u32 {
        self.troops.iter().flatten().map(|s| s.count).sum()
    }
}

pub struct RoundReport {
    pub fleets: SideReport,
    pub hostiles: SideReport,
}

impl RoundReport {
    /// A unit of either side lost hull.
    pub fn hull_lost(&self) -> bool {
        self.fleets.hull_damage + self.hostiles.hull_damage > 0
    }
}

#[derive(Clone, Copy)]
struct Unit {
    hull: u32,
    line: u16,
    /// Hundredths of its full shield still standing.
    shield: u8,
    dead: bool,
}

/// The units of one squad of one troop: a run of the side's unit list.
struct Line {
    kind: usize,
    figures: UnitFigures,
    troop: usize,
    start: usize,
    len: usize,
}

struct Side {
    lines: Vec<Line>,
    units: Vec<Unit>,
}

#[derive(Clone, Copy, Default)]
struct Fire {
    shots: u32,
    bounced: u32,
    wasted: u32,
    absorbed: u64,
    hull_damage: u64,
    destroyed: u32,
}

fn rapid_fire() -> &'static [[u16; KINDS]; KINDS] {
    static TABLE: OnceLock<[[u16; KINDS]; KINDS]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [[0u16; KINDS]; KINDS];
        for (from, row) in table.iter_mut().enumerate() {
            for (at, figure) in row.iter_mut().enumerate() {
                if let (Some(unit), Some(target)) = (ShipyardItem::from_index(from), ShipyardItem::from_index(at)) {
                    *figure = scaling::rapid_fire_of(unit, target);
                }
            }
        }
        table
    })
}

fn muster(troops: &[Troop]) -> Side {
    let mut side = Side { lines: Vec::new(), units: Vec::with_capacity(troops.iter().map(|t| t.units() as usize).sum()) };
    for (troop, t) in troops.iter().enumerate() {
        for squad in &t.squads {
            let figures = scaling::unit_figures(squad.unit, t.arms);
            let line = side.lines.len() as u16;
            let start = side.units.len();
            let whole = (squad.count as usize).saturating_sub(squad.damaged.len());
            let fresh = Unit { hull: figures.hull, line, shield: 100, dead: false };
            side.units.extend(std::iter::repeat_n(fresh, whole));
            side.units.extend(squad.damaged.iter().map(|&hull| Unit { hull: hull.min(figures.hull), ..fresh }));
            side.lines.push(Line { kind: squad.unit.index(), figures, troop, start, len: side.units.len() - start });
        }
    }
    side
}

/// Every unit of `from` fires at `at`. Returns the fire by kind of firing
/// unit and of target, and the attack of every shot summed.
fn fire(from: &Side, at: &mut Side, dice: &mut Dice) -> (Vec<Fire>, u64) {
    let rapid = rapid_fire();
    let mut tally = vec![Fire::default(); KINDS * KINDS];
    let mut power = 0u64;
    let targets = at.units.len() as u32;
    if targets == 0 {
        return (tally, power);
    }
    for line in &from.lines {
        let attack = line.figures.attack;
        if attack == 0 {
            continue;
        }
        for _ in 0..line.len {
            loop {
                let target = &mut at.units[dice.below(targets) as usize];
                let hit = &at.lines[usize::from(target.line)];
                let fire = &mut tally[line.kind * KINDS + hit.kind];
                fire.shots += 1;
                power += u64::from(attack);
                if target.dead {
                    fire.wasted += 1;
                } else {
                    let full = u64::from(hit.figures.shield);
                    let mut through = u64::from(attack);
                    let mut struck = true;
                    if full > 0 && target.shield > 0 {
                        let hundredths = u64::from(attack) * 100 / full;
                        if hundredths == 0 {
                            fire.bounced += 1;
                            through = 0;
                            struck = false;
                        } else if u64::from(target.shield) < hundredths {
                            let soaked = u64::from(target.shield) * full / 100;
                            fire.absorbed += soaked;
                            through -= soaked;
                            target.shield = 0;
                        } else {
                            target.shield -= hundredths as u8;
                            fire.absorbed += through;
                            through = 0;
                        }
                    }
                    let dealt = through.min(u64::from(target.hull)) as u32;
                    target.hull -= dealt;
                    fire.hull_damage += u64::from(dealt);
                    let bare = full == 0 || target.shield == 0;
                    let hull_max = u64::from(hit.figures.hull);
                    if struck && bare && u64::from(target.hull) * 100 <= hull_max * u64::from(EXPLODE_BELOW_PCT) {
                        let holds = u64::from(target.hull) * 100 / hull_max.max(1);
                        if target.hull == 0 || u64::from(dice.below(100)) >= holds {
                            target.dead = true;
                            fire.destroyed += 1;
                        }
                    }
                }
                let again = rapid[line.kind][hit.kind];
                if again == 0 || dice.below(u32::from(again)) == 0 {
                    break;
                }
            }
        }
    }
    (tally, power)
}

fn report(side: &Side, troops: usize, fired: &[Fire], power: u64) -> SideReport {
    let mut out = SideReport {
        troops: vec![Vec::new(); troops],
        standing: vec![Vec::new(); troops],
        power,
        ..SideReport::default()
    };
    for line in &side.lines {
        let Some(unit) = ShipyardItem::from_index(line.kind) else { continue };
        let full = line.figures.hull;
        let mut left = 0u32;
        let mut at_risk = 0u32;
        let mut hull = 0u64;
        let mut damaged: Vec<u32> = Vec::new();
        for standing in side.units[line.start..line.start + line.len].iter().filter(|u| !u.dead) {
            left += 1;
            hull += u64::from(standing.hull);
            if standing.hull < full {
                damaged.push(standing.hull);
                at_risk += u32::from(u64::from(standing.hull) * 100 <= u64::from(full) * u64::from(EXPLODE_BELOW_PCT));
            }
        }
        damaged.sort_unstable();
        out.standing[line.troop].push(BattleUnits {
            unit,
            count: left,
            damaged: damaged.len() as u32,
            at_risk,
            hull,
            hull_max: u64::from(left) * u64::from(full),
            lost: line.len as u32 - left,
        });
        if left > 0 {
            out.troops[line.troop].push(Squad { unit, count: left, damaged });
        }
    }
    for (index, fire) in fired.iter().enumerate().filter(|(_, fire)| fire.shots > 0) {
        let (Some(from), Some(at)) = (ShipyardItem::from_index(index / KINDS), ShipyardItem::from_index(index % KINDS)) else {
            continue;
        };
        out.volleys.push(Volley {
            from,
            at,
            shots: fire.shots,
            bounced: fire.bounced,
            wasted: fire.wasted,
            absorbed: fire.absorbed,
            hull_damage: fire.hull_damage,
            destroyed: fire.destroyed,
        });
        out.shots += u64::from(fire.shots);
        out.absorbed += fire.absorbed;
        out.hull_damage += fire.hull_damage;
        out.destroyed += fire.destroyed;
    }
    out
}

/// Fight one round between `fleets` and `hostiles`, each in the order its
/// units are to stand in: troops as given, squads in catalogue order, whole
/// units and then the damaged as listed.
pub fn round(fleets: &[Troop], hostiles: &[Troop], dice: &mut Dice) -> RoundReport {
    let mut ours = muster(fleets);
    let mut theirs = muster(hostiles);
    let (our_fire, our_power) = fire(&ours, &mut theirs, dice);
    let (their_fire, their_power) = fire(&theirs, &mut ours, dice);
    RoundReport {
        fleets: report(&ours, fleets.len(), &our_fire, our_power),
        hostiles: report(&theirs, hostiles.len(), &their_fire, their_power),
    }
}
