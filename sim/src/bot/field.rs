// Fleets: which wing a ship belongs to, the standing order each wing flies
// under when its owner is away, and the pilot that flies it when present.
//
// Every fleet is kept to one wing. Ships built while a wing is out collect
// at home; at a look-in the wing is called home, merged and sent out again.
//
//   wing       doctrine            with no work under it   by hand
//   garrison   none                                        stays docked
//   mining     mine_and_return     salvage_and_sites       nearest known ore that pays, strip it, haul home
//   survey     prospect            salvage_and_sites       nearest sector not yet charted; boards
//   recovery   salvage_and_sites   mine_and_return         derelicts and wreckage that pay
//   strike     patrol_home         salvage_and_sites       nearest group whose wreckage pays, else home
//
// A turtle's ships and, once it has a cargo ship, a miner's warships are
// the garrison: they are bought with what no order claimed, and stay home.
//
// A doctrine that finds nothing to do brings its fleet home and holds. A
// fleet found at the dock with a full tank as a look-in ends is therefore
// switched to its wing's other doctrine, and back at a later look-in if
// that one runs out of work too.
//
// The pilot knows only the chart: it never enters a sector it has not seen
// (it scans first), it treats a cleared lair it cannot watch as occupied,
// and it reads off each sector's chart mark whether a visit still pays.
// Hull lost stays lost until the home dock mends it, so it fights only at
// odds that cost it little, and takes a fleet whose hull has fallen below
// `HULL_READY` home before it does anything else. A wing with no warship
// in it picks no fight at any odds. In a fight it reads the server's odds
// of that fight as it stands (`FleetState::flight`) and jumps out toward
// home once they are under `LEAVE_ODDS`; with a group on its trail it
// runs for home. A raid on the homeworld is a fight too: when the
// server's forecast of one that is announced says it may leave no defender
// standing, the pilot takes its fleets to the next sector for it and
// brings them back when it is over; a fleet with no warship, which adds
// nothing to the defence and is what rapid fire eats, leaves for any raid
// announced. The odds are the
// server's, and they count the damage a fleet carries: before its orders
// the admiral asks (`odds`) how each fleet it may move would fare in every
// sector where that decides something, and it stays out of any sector it
// has no answer for.
//
// A trip is flown for what it brings home (`pays`): ore, wreckage or a
// derelict's hold, as much as the fleet can carry, against the deuterium
// of the detour, and for a fight also the worth the odds say the fleet
// will lose (`fight_pays`). Fuel is paid in deuterium the moment a fleet
// docks, ahead of anything waiting in a queue, so from home a trip is also
// held to the deuterium the stockpile has spare beyond the next orders.
// Away from home a fleet short of fuel refines what its hold carries
// before it turns home. A fleet under a doctrine is held to the doctrine's
// fuel budget by the server.

use std::collections::{BTreeMap, BTreeSet};

use iac_shared::constants::{
    Resources, ShipClass, EXPLORE_DURATION_TICKS, FUEL_DEUT_PER_UNIT, MAX_FLEETS_PER_PLAYER, MAX_FLEETS_TOTAL,
    MAX_FLEET_SHIPS, POLICY_DEFAULT_FUEL_BUDGET_PCT, SCAN_COOLDOWN,
};
use iac_shared::hex::Hex;
use iac_shared::pace::Pace;
use iac_shared::protocol::{
    Command, FleetState, FleetStatus, GameState, HarvestResource, MAX_ODDS_SECTORS, NpcBehavior, PolicyParams,
    PolicyPreset, PolicyUpdate, RaidLabel, SectorState, ShipCount, SiteBrief, SiteRisk, ThreatBasis,
};
use iac_shared::scaling;
use serde::{Deserialize, Serialize};

use super::home::Hull;
use super::Personality;
use crate::score;
use crate::script::Order;

/// Odds ratio (`SectorOdds::ratio`) at which a pilot accepts a fight: won
/// every time the odds played it, keeping about five sixths of the fleet's
/// worth. In OGame's round a shot takes a hull or nothing, so a fight that
/// costs no ship takes a fleet many times its enemy, and a pilot who waited
/// for one would not fight.
const SAFE_ODDS: f32 = 2.0;
/// Odds of a fight under way (`FlightQuote::ratio`) below which a pilot
/// leaves it: the edge of RISKY, where the doctrines leave too.
const LEAVE_ODDS: f32 = scaling::RATIO_RISKY;
/// How far around a fleet its pilot asks the server for odds. A guarded
/// sector farther off stays closed to it until it is nearer.
const ODDS_HORIZON: u16 = 12;
/// The same for a fleet in a fight, which only has to choose the lane it
/// leaves by.
const FLIGHT_HORIZON: u16 = 2;
/// The ratio the prospect, mining and salvage doctrines are sent out with
/// (the game's default).
const DOCTRINE_RATIO_X10: u8 = 15;
/// The ratio a wing of at least `CAREFUL_WING_SHIPS` ships flies at when its
/// doctrine or its hulls take it into fights: the pilot's own.
const CAREFUL_RATIO_X10: u8 = 20;
const CAREFUL_WING_SHIPS: u32 = 4;
/// Doctrines are leashed by odds and fuel, not by distance.
const DOCTRINE_RANGE: u8 = 30;
/// Fuel budget of the wings that haul, as a percent of deuterium income.
const HAUL_FUEL_BUDGET_PCT: u8 = 50;
/// Share of its full hull below which a pilot takes a fleet home to be mended.
const HULL_READY: f32 = 0.85;
const RETURN_LOAD: f64 = 0.85;
/// Share of the tank kept spare on top of the way home.
const FUEL_MARGIN: f64 = 0.10;
const READY_FUEL: f64 = 0.90;
const REACH: u32 = 48;
const MIN_ORE: f64 = 3.0;
const WAIT: u64 = 30;
const REST: u64 = 3;
/// A trip is flown for what it brings home when that is worth this many
/// times the deuterium it burns: the odds, the pile and the lanes home are
/// all estimates.
const PAYS: f32 = 1.5;
/// Heat below which a lair is remembered as fresh.
const COLD: f32 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Wing {
    Garrison,
    Mining,
    Survey,
    Recovery,
    Strike,
}

impl Wing {
    const ALL: [Wing; 5] = [Wing::Garrison, Wing::Mining, Wing::Survey, Wing::Recovery, Wing::Strike];
    const FIELD: [Wing; 4] = [Wing::Mining, Wing::Survey, Wing::Recovery, Wing::Strike];

    /// The wing a ship of `class` flies with. A miner mines with whatever it
    /// has until it has a cargo ship (`hauls`); after that its warships,
    /// which its surplus buys, stay home.
    fn of(personality: Personality, class: ShipClass, hauls: bool) -> Wing {
        match (personality, Hull::of(class)) {
            (Personality::Turtle, _) => Wing::Garrison,
            (Personality::Miner, Hull::Warship) if hauls => Wing::Garrison,
            (Personality::Miner, _) => Wing::Mining,
            (Personality::Explorer | Personality::Mixed, Hull::Scout) => Wing::Survey,
            (Personality::Raider | Personality::Mixed, Hull::Warship) => Wing::Strike,
            (Personality::Explorer, Hull::Warship | Hull::Hauler)
            | (Personality::Raider, Hull::Scout | Hull::Hauler)
            | (Personality::Mixed, Hull::Hauler) => Wing::Recovery,
        }
    }

    /// The doctrine a wing flies under, and the one it is tried on when the
    /// first finds nothing to do.
    fn doctrines(self) -> Option<[PolicyPreset; 2]> {
        match self {
            Wing::Garrison => None,
            Wing::Mining => Some([PolicyPreset::MineAndReturn, PolicyPreset::SalvageAndSites]),
            Wing::Survey => Some([PolicyPreset::Prospect, PolicyPreset::SalvageAndSites]),
            Wing::Recovery => Some([PolicyPreset::SalvageAndSites, PolicyPreset::MineAndReturn]),
            Wing::Strike => Some([PolicyPreset::PatrolHome, PolicyPreset::SalvageAndSites]),
        }
    }

    /// Orders for `preset` for `fleet`. The ratio is the odds at which a
    /// doctrine starts a fight where it has to. A patrol hunts, and fights
    /// only what it beats without lasting damage. `prospect` has to enter a
    /// lair to chart it, and a wing with warships is the one strong enough
    /// to go where every lane has a fight: both fly at the pilot's odds once
    /// they have the ships to leave home at them. The hauling doctrines pick no
    /// fight for a payday, so for probes and cargo ships under them the ratio
    /// gates quiet space, and stays at the game's default. Doctrines that
    /// bring resources home earn their fuel back and get the larger budget;
    /// the ones that fly for points get the game's default.
    fn orders(preset: PolicyPreset, fleet: &FleetState) -> PolicyUpdate {
        let fights = armed(fleet) || preset == PolicyPreset::Prospect;
        let careful = if fights && fleet.ship_count >= CAREFUL_WING_SHIPS { CAREFUL_RATIO_X10 } else { DOCTRINE_RATIO_X10 };
        let (engage_ratio_x10, fuel_budget_pct) = match preset {
            PolicyPreset::PatrolHome => ((SAFE_ODDS * 10.0) as u8, POLICY_DEFAULT_FUEL_BUDGET_PCT),
            PolicyPreset::MineAndReturn | PolicyPreset::SalvageAndSites => (careful, HAUL_FUEL_BUDGET_PCT),
            PolicyPreset::Prospect | PolicyPreset::Manual => (careful, POLICY_DEFAULT_FUEL_BUDGET_PCT),
        };
        PolicyUpdate {
            fleet_id: 0,
            preset,
            params: Some(PolicyParams {
                max_range: DOCTRINE_RANGE,
                engage_ratio_x10,
                fuel_budget_pct,
                ..PolicyParams::default()
            }),
        }
    }
}

/// The lanes a fleet set out on and how far along it is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Route {
    path: Vec<Hex>,
    next: usize,
    homeward: bool,
}

/// The kills a player remembers making in one lair: each one heats it, and
/// the heat cools as the game's farm heat does (`scaling::farm_factor`).
/// Other empires' kills there are not seen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(super) struct Heat {
    sector: Hex,
    heat: f32,
    tick: u64,
}

impl Heat {
    fn at(&self, tick: u64, pace: &Pace) -> f32 {
        let respawn = pace.econ_ticks(f64::from(scaling::npc_respawn_hours(self.sector.dist_from_origin())) * 3600.0);
        let half_life = (respawn as f32 * scaling::FARM_HALF_LIFE_RESPAWNS).ceil() as u64;
        scaling::farm_heat_after(self.heat, tick.saturating_sub(self.tick), half_life)
    }
}

struct Chart<'a> {
    sectors: &'a [SectorState],
    index: Vec<(u32, u32)>,
}

impl<'a> Chart<'a> {
    fn new(sectors: &'a [SectorState]) -> Chart<'a> {
        let mut index: Vec<(u32, u32)> =
            sectors.iter().enumerate().map(|(i, s)| (s.location.to_key(), i as u32)).collect();
        index.sort_unstable();
        Chart { sectors, index }
    }

    fn slot(&self, sector: Hex) -> Option<usize> {
        let at = self.index.binary_search_by_key(&sector.to_key(), |e| e.0).ok()?;
        Some(self.index[at].1 as usize)
    }

    fn get(&self, sector: Hex) -> Option<&'a SectorState> {
        self.slot(sector).map(|i| &self.sectors[i])
    }

    /// The lanes to the nearest known sector `goal` accepts (given its hop
    /// count), walking only through sectors `passable` accepts.
    fn nearest(
        &self,
        from: Hex,
        passable: impl Fn(&SectorState) -> bool,
        goal: impl Fn(&SectorState, u32) -> bool,
    ) -> Option<Vec<Hex>> {
        let start = self.slot(from)?;
        let mut prev = vec![u32::MAX; self.sectors.len()];
        prev[start] = start as u32;
        let mut frontier = vec![start];
        for hops in 1..=REACH {
            let mut next = Vec::new();
            for &at in &frontier {
                for &lane in &self.sectors[at].connections {
                    let Some(slot) = self.slot(lane) else { continue };
                    if prev[slot] != u32::MAX {
                        continue;
                    }
                    prev[slot] = at as u32;
                    let sector = &self.sectors[slot];
                    if goal(sector, hops) {
                        let mut path = vec![sector.location];
                        let mut step = at;
                        while step != start {
                            path.push(self.sectors[step].location);
                            step = prev[step] as usize;
                        }
                        path.reverse();
                        return Some(path);
                    }
                    if passable(sector) {
                        next.push(slot);
                    }
                }
            }
            if next.is_empty() {
                return None;
            }
            frontier = next;
        }
        None
    }
}

#[derive(Clone, Copy)]
struct Unit<'a> {
    fleet: &'a FleetState,
    wing: Wing,
    /// Every ship belongs to `wing`.
    pure: bool,
}

impl Unit<'_> {
    fn free(&self) -> bool {
        matches!(self.fleet.state, FleetStatus::Idle | FleetStatus::Docked) && self.fleet.cooldown_remaining == 0
    }

    fn mauled(&self) -> bool {
        (self.fleet.hull as f32) < HULL_READY * self.fleet.hull_max as f32
    }
}

/// The server's odds for the empire's fleets as asked this turn: the ratio
/// by fleet id and sector key.
pub(super) type Odds = BTreeMap<(u64, u32), f32>;

/// The empire has a cargo ship somewhere.
fn hauls(state: &GameState) -> bool {
    state.fleets.iter().flat_map(|f| f.ships.iter()).any(|s| Hull::of(s.ship_class) == Hull::Hauler)
}

fn units(state: &GameState, personality: Personality) -> Vec<Unit<'_>> {
    let hauls = hauls(state);
    state
        .fleets
        .iter()
        .filter_map(|fleet| {
            let mut count = [0u32; Wing::ALL.len()];
            for ship in &fleet.ships {
                count[Wing::of(personality, ship.ship_class, hauls) as usize] += ship.count;
            }
            let wing = Wing::ALL.into_iter().max_by_key(|&w| count[w as usize])?;
            let pure = count[wing as usize] == fleet.ship_count;
            (wing != Wing::Garrison || !pure).then_some(Unit { fleet, wing, pure })
        })
        .collect()
}

/// The `odds` requests behind this turn's orders. A fleet that will be
/// flown by hand is rated against the sectors around it where the answer
/// settles something; one that can only act where it stands, against that
/// sector; every fleet against home, if a fight waits there.
pub(super) fn questions(state: &GameState, brief: Brief, questions: &mut Vec<Command>) {
    let home = state.player.homeworld;
    for unit in units(state, brief.personality).into_iter().filter(|u| u.wing != Wing::Garrison) {
        let fleet = unit.fleet;
        let fighting = fleet.flight.is_some();
        let manual = (unit.free() || fighting) && fleet.policy.is_none();
        let held = fleet.location == home && held_at_home(state, brief, unit).is_some();
        let flying = manual && fleet.fuel >= fleet.jump_fuel && !held;
        let reach = match (flying, fighting) {
            (false, _) => 0,
            (true, true) => FLIGHT_HORIZON,
            (true, false) => ODDS_HORIZON,
        };
        let sectors: Vec<Hex> = state
            .known_sectors
            .iter()
            .filter(|s| s.location == home || (manual && Hex::distance(&s.location, &fleet.location) <= reach))
            .filter(|s| decides(s))
            .map(|s| s.location)
            .collect();
        for sectors in sectors.chunks(MAX_ODDS_SECTORS) {
            questions.push(Command::Odds { fleet_id: fleet.id, sectors: sectors.to_vec() });
        }
    }
}

/// Deuterium in stock beyond what the next order in each queue costs. A
/// waiting building or research order shows its cost; a shipyard batch can
/// only be ordered once it is covered, so the home planner says what the
/// yard is saving for.
fn fuel_money(state: &GameState, brief: Brief) -> f64 {
    let home = &state.homeworld;
    let building = home.build_pending.first().map_or(0.0, |q| q.cost.deuterium);
    let research = home.research_pending.first().map_or(0.0, |q| q.cost.deuterium);
    state.player.resources.deuterium - building - research - brief.yard_deuterium
}

/// How long a fleet docked at home stays there whatever the map looks like:
/// a tank still filling, no fleet slot away, no fuel to spare. A wing flown
/// for charts wants a tank to spare beyond the next orders; the others are
/// held to what each trip burns when they choose it.
fn held_at_home(state: &GameState, brief: Brief, unit: Unit) -> Option<u64> {
    let fleet = unit.fleet;
    let home = state.player.homeworld;
    let tank_filling = state.player.resources.deuterium >= 1.0;
    if fleet.fuel < READY_FUEL * fleet.fuel_max && tank_filling {
        return Some(5);
    }
    let deployed = state.fleets.iter().filter(|f| f.location != home).count();
    let spare = if unit.wing == Wing::Survey { FUEL_DEUT_PER_UNIT * fleet.fuel_max } else { 0.0 };
    (deployed >= MAX_FLEETS_PER_PLAYER || fuel_money(state, brief) < spare).then_some(WAIT)
}

/// What the admiral is told about the player it serves this turn.
#[derive(Clone, Copy)]
pub(super) struct Brief {
    pub personality: Personality,
    /// Ticks until the next look-in once this one ends; None for a player who stays.
    pub away_for: Option<u64>,
    /// More combat or exploration points would still raise the score.
    pub points_wanted: bool,
    /// Deuterium of the unit the shipyard wants next and cannot pay for yet.
    pub yard_deuterium: f64,
}

/// One turn's worth of fleet orders for one empire.
pub(super) struct Admiral<'a> {
    state: &'a GameState,
    brief: Brief,
    pace: Pace,
    routes: &'a mut BTreeMap<u64, Route>,
    heat: &'a mut BTreeMap<u32, Heat>,
    odds: &'a Odds,
    orders: &'a mut Vec<Order>,
    chart: Chart<'a>,
    home: Hex,
    units: Vec<Unit<'a>>,
    busy: BTreeSet<u64>,
    claimed: BTreeSet<(Claim, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Claim {
    Wreck,
    Site,
    Group,
}

impl<'a> Admiral<'a> {
    pub(super) fn new(
        state: &'a GameState,
        brief: Brief,
        routes: &'a mut BTreeMap<u64, Route>,
        heat: &'a mut BTreeMap<u32, Heat>,
        odds: &'a Odds,
        orders: &'a mut Vec<Order>,
    ) -> Admiral<'a> {
        let units = units(state, brief.personality);
        let pace = Pace::new(state.world.pace).unwrap_or_default();
        heat.retain(|_, lair| lair.at(state.tick, &pace) > COLD);
        Admiral {
            state,
            brief,
            pace,
            routes,
            heat,
            odds,
            orders,
            chart: Chart::new(&state.known_sectors),
            home: state.player.homeworld,
            units,
            busy: BTreeSet::new(),
            claimed: BTreeSet::new(),
        }
    }

    fn command(&mut self, command: Command) {
        self.orders.push(Order::Command(command));
    }

    /// A fleet that would lose to the lair in its own home sector cannot
    /// come back: arriving home starts that fight.
    fn can_return(&self, unit: Unit) -> bool {
        self.chart.get(self.home).is_none_or(|home| enterable(self.odds, home, unit.fleet))
    }

    /// Put the fleet under one of its wing's doctrines if it has work and
    /// can come home; otherwise bring it home and keep it docked. A fleet
    /// that sits at the dock with a full tank as its owner leaves found
    /// nothing to do under its orders and is given the wing's other doctrine.
    fn delegate(&mut self, unit: Unit, leaving: bool) {
        let fleet = unit.fleet;
        let Some(doctrines) = unit.wing.doctrines() else { return };
        if !self.can_return(unit) {
            self.dock(unit);
            return;
        }
        let resting = leaving && fleet.location == self.home && unit.free() && fleet.fuel >= fleet.fuel_max;
        let preset = match fleet.policy.and_then(|flown| doctrines.iter().position(|d| *d == flown.preset)) {
            Some(flown) if resting => doctrines[1 - flown],
            Some(flown) => doctrines[flown],
            None => doctrines[0],
        };
        if fleet.policy.map(|flown| flown.preset) != Some(preset) || leaving {
            let orders = Wing::orders(preset, fleet);
            self.orders.push(Order::Policy(PolicyUpdate { fleet_id: fleet.id, ..orders }));
        }
    }

    fn dock(&mut self, unit: Unit) {
        let fleet = unit.fleet;
        if fleet.policy.is_some() {
            self.orders.push(Order::Policy(PolicyUpdate { fleet_id: fleet.id, preset: PolicyPreset::Manual, params: None }));
        } else if fleet.location != self.home && unit.free() {
            self.head_home(unit);
        }
    }

    /// At home: split fleets that mix wings, then merge fleets of one wing.
    fn organize(&mut self) {
        let docked: Vec<Unit> = self.units.iter().copied().filter(|u| u.fleet.location == self.home && u.free()).collect();
        let hauls = hauls(self.state);
        for unit in docked.iter().filter(|u| !u.pure) {
            if self.state.fleets.len() >= MAX_FLEETS_TOTAL {
                break;
            }
            let strays: Vec<ShipCount> = unit
                .fleet
                .ships
                .iter()
                .filter(|s| Wing::of(self.brief.personality, s.ship_class, hauls) != unit.wing)
                .map(|s| ShipCount { ship_class: s.ship_class, count: s.count })
                .collect();
            self.busy.insert(unit.fleet.id);
            self.command(Command::Split { fleet_id: unit.fleet.id, ships: strays, damaged_first: false });
        }
        for wing in Wing::FIELD {
            let mut same: Vec<Unit> = docked.iter().copied().filter(|u| u.pure && u.wing == wing).collect();
            same.sort_by_key(|u| (u.fleet.policy.is_none(), u.fleet.id));
            let Some((keep, rest)) = same.split_first() else { continue };
            let mut size = keep.fleet.ship_count;
            for other in rest {
                if size + other.fleet.ship_count > MAX_FLEET_SHIPS {
                    continue;
                }
                size += other.fleet.ship_count;
                self.busy.extend([keep.fleet.id, other.fleet.id]);
                self.command(Command::Merge { fleet_id: keep.fleet.id, other_fleet_id: other.fleet.id });
            }
        }
    }

    /// A look-in: wings in several fleets are called home to merge; every
    /// other fleet flies under its wing's doctrine.
    pub(super) fn manage(&mut self, closing: bool) {
        self.organize();
        for wing in Wing::FIELD {
            let fleets: Vec<Unit> = self.units.iter().copied().filter(|u| u.wing == wing).collect();
            let mut sizes: Vec<u32> = fleets.iter().map(|u| u.fleet.ship_count).collect();
            sizes.sort_unstable();
            let regroup = !closing && sizes.len() > 1 && sizes[0] + sizes[1] <= MAX_FLEET_SHIPS;
            for unit in fleets {
                if self.busy.contains(&unit.fleet.id) {
                    continue;
                }
                if regroup {
                    self.dock(unit);
                } else {
                    self.delegate(unit, closing);
                }
            }
        }
    }

    /// Fly every field fleet by hand. Returns how many ticks can pass before
    /// any of them needs another order.
    pub(super) fn pilot(&mut self, closing: bool) -> u64 {
        self.organize();
        let mut idle_for = WAIT;
        for unit in self.units.clone() {
            if self.busy.contains(&unit.fleet.id) {
                idle_for = 1;
            } else if unit.wing == Wing::Garrison {
                continue;
            } else if closing {
                self.delegate(unit, true);
            } else if unit.fleet.policy.is_some() {
                self.dock(unit);
                idle_for = 1;
            } else {
                idle_for = idle_for.min(self.fly(unit));
            }
        }
        idle_for
    }

    fn fly(&mut self, unit: Unit) -> u64 {
        let fleet = unit.fleet;
        let cooldown = u64::from(fleet.cooldown_remaining);
        match fleet.state {
            // A jump lands the tick after the view shows its timer at 0.
            FleetStatus::Moving => return cooldown + 1,
            FleetStatus::Exploring => return cooldown,
            FleetStatus::InCombat => return self.fight(unit),
            FleetStatus::Returning => return 2,
            FleetStatus::Harvesting => return REST,
            FleetStatus::Idle | FleetStatus::Docked if cooldown > 0 => return cooldown,
            FleetStatus::Idle | FleetStatus::Docked => {}
        }
        let Some(here) = self.chart.get(fleet.location) else { return WAIT };
        let at_home = fleet.location == self.home;
        let load = fleet.cargo.total();

        if !at_home {
            if here.salvage.is_some() && load < fleet.cargo_capacity && self.claim(Claim::Wreck, here.location) {
                self.command(Command::CollectSalvage { fleet_id: fleet.id });
                return 1;
            }
            if here.site.is_some_and(|site| boardable(self.odds, fleet, here, site)) && self.claim(Claim::Site, here.location) {
                self.command(Command::ExploreSite { fleet_id: fleet.id });
                return u64::from(EXPLORE_DURATION_TICKS);
            }
        }
        let refinable = if at_home { 0.0 } else { fleet.refine.map_or(0.0, |r| r.fuel) };
        if fleet.fuel < fleet.jump_fuel {
            if refinable > 0.0 {
                return self.refine(fleet);
            }
            return WAIT;
        }
        if at_home {
            if self.raid_threatens() || (self.raid_announced() && !armed(fleet)) {
                return self.step_aside(unit);
            }
            if let Some(wait) = held_at_home(self.state, self.brief, unit) {
                return wait;
            }
            if !self.can_return(unit) {
                return WAIT;
            }
        } else if unit.mauled() || fleet.pursued_by.is_some() {
            return self.head_home(unit);
        } else {
            let hold_full = fleet.cargo_capacity > 0.0 && load >= RETURN_LOAD * fleet.cargo_capacity;
            let enough = fleet.home_fuel + 2.0 * fleet.jump_fuel + FUEL_MARGIN * fleet.fuel_max;
            if !hold_full && fleet.fuel < enough && fleet.fuel + refinable >= enough {
                return self.refine(fleet);
            }
            if hold_full || fleet.fuel < enough {
                return self.head_home(unit);
            }
        }
        match unit.wing {
            Wing::Strike => self.hunt(unit, here),
            Wing::Mining => self.mine(unit, here),
            Wing::Survey | Wing::Recovery => self.survey(unit),
            Wing::Garrison => WAIT,
        }
    }

    /// In a fight: stay while the server's odds of it, as it stands, are
    /// `LEAVE_ODDS` or better, and otherwise jump out toward home under
    /// fire, or out of home when the fight is a raid on it. A fleet that
    /// cannot leave fights on.
    fn fight(&mut self, unit: Unit) -> u64 {
        let leaves = unit.fleet.flight.as_ref().is_some_and(|flight| flight.ratio < LEAVE_ODDS && flight.refusal.is_none());
        match (leaves, unit.fleet.location == self.home) {
            (false, _) => 1,
            (true, true) => self.step_aside(unit),
            (true, false) => self.head_home(unit),
        }
    }

    /// A raid is announced that the server's forecast says may leave no
    /// defender standing: a fleet that stays at home for it is lost with
    /// the rest.
    fn raid_threatens(&self) -> bool {
        let raid = &self.state.homeworld.raid;
        raid.arrival_tick.is_some() && matches!(raid.odds.label, RaidLabel::AtRisk | RaidLabel::Lost)
    }

    /// A raid is on its way. A fleet with no warship adds nothing to the
    /// defence and is what the raiders' rapid fire eats first.
    fn raid_announced(&self) -> bool {
        self.state.homeworld.raid.arrival_tick.is_some()
    }

    /// Turn the deuterium in the hold into fuel.
    fn refine(&mut self, fleet: &FleetState) -> u64 {
        self.command(Command::Refine { fleet_id: fleet.id });
        1
    }

    /// A raid is being fought at home.
    fn raid_fought(&self) -> bool {
        self.state.battles.iter().any(|battle| battle.sector == self.home && battle.raid.is_some())
    }

    /// Jump to a sector beside the homeworld, where no raider follows.
    fn step_aside(&mut self, unit: Unit) -> u64 {
        let home = self.home;
        if unit.fleet.location == home && self.state.fleets.iter().filter(|f| f.location != home).count() >= MAX_FLEETS_PER_PLAYER {
            return REST;
        }
        let beside = self.chart.get(self.home).and_then(|home| home.connections.first().copied());
        beside.map_or(WAIT, |target| self.jump(unit, target))
    }

    /// After `hops` more jumps to `sector` the fleet still has the way home
    /// in the tank, counting what its hold would refine away from home.
    fn in_range(&self, fleet: &FleetState, sector: &SectorState, hops: u32) -> bool {
        let home_hops = u32::from(Hex::distance(&sector.location, &self.home));
        let refinable = if fleet.location == self.home { 0.0 } else { fleet.refine.map_or(0.0, |r| r.fuel) };
        fleet.fuel + refinable >= f64::from(hops + home_hops + 1) * fleet.jump_fuel + FUEL_MARGIN * fleet.fuel_max
    }

    /// Whether a trip of `hops` jumps to `sector` brings in `worth` points
    /// enough for the deuterium the detour burns, out and back home. From
    /// home the stockpile must also have that deuterium to spare beyond the
    /// next orders: the dock fills the tank again ahead of every queue.
    fn pays(&self, fleet: &FleetState, sector: &SectorState, hops: u32, worth: f32) -> bool {
        let back = |at: &Hex| f64::from(Hex::distance(at, &self.home));
        let jumps = (f64::from(hops) + back(&sector.location) - back(&fleet.location)).max(1.0);
        let deuterium = jumps * fleet.jump_fuel * FUEL_DEUT_PER_UNIT;
        let burned = scaling::resource_weight(&Resources { deuterium, ..Resources::default() });
        let spared = fleet.location != self.home || deuterium <= fuel_money(self.state, self.brief);
        spared && worth >= PAYS * burned
    }

    /// Points of `load` the fleet has room to carry.
    fn carried(fleet: &FleetState, load: Resources) -> f32 {
        let room = (fleet.cargo_capacity - fleet.cargo.total()).max(0.0);
        let units = load.total();
        if units <= 0.0 {
            return 0.0;
        }
        scaling::resource_weight(&load.scale((room / units).min(1.0)))
    }

    /// Whether killing what waits in `sector`, `hops` away, pays: the
    /// wreckage the fleet can carry home, as the lair's own kills have left
    /// it (`scaling::wreck_value`, the finds factor, the farm heat it
    /// remembers), against the deuterium of the detour and the worth the
    /// server's odds say the fleet will lose. No part of the score enters it:
    /// a fight is for what it brings home, and a ship lost is score lost.
    fn fight_pays(&self, fleet: &FleetState, sector: &SectorState, hops: u32) -> bool {
        let groups = sector.hostiles.as_ref().map_or(0, |list| list.len()).max(1);
        let power = scaling::npc_group_power(sector.location.dist_from_origin());
        let heat = self.heat.get(&sector.location.to_key()).map_or(0.0, |lair| lair.at(self.state.tick, &self.pace));
        let worth = self.pace.finds_mult() * f64::from(scaling::farm_factor(heat));
        let pile = scaling::split_loot(scaling::wreck_value(power) * groups as f64).scale(worth);
        let lost = ratio(self.odds, fleet, sector).map_or(f32::INFINITY, |ratio| worth_lost(ratio) * fleet_worth(fleet));
        self.pays(fleet, sector, hops, Self::carried(fleet, pile) - lost)
    }

    /// What the wreck in `sector`, and the derelict when it is to be
    /// boarded, would bring in.
    fn findings(&self, fleet: &FleetState, sector: &SectorState, board: bool) -> f32 {
        let mut found = sector.salvage.unwrap_or_default();
        if board {
            found = found.add(self.derelict(sector));
        }
        Self::carried(fleet, found)
    }

    /// What a derelict in `sector` holds on average.
    fn derelict(&self, sector: &SectorState) -> Resources {
        scaling::split_loot(scaling::derelict_value(sector.location.dist_from_origin()) * self.pace.finds_mult())
    }

    /// The first fleet to want what lies in `sector` this turn takes it; a
    /// second would be refused, the pile scooped or the group engaged.
    fn claim(&mut self, what: Claim, sector: Hex) -> bool {
        self.claimed.insert((what, sector.to_key()))
    }

    fn note_kill(&mut self, sector: Hex) {
        let (tick, pace) = (self.state.tick, self.pace);
        let lair = self.heat.entry(sector.to_key()).or_insert(Heat { sector, heat: 0.0, tick });
        *lair = Heat { sector, heat: lair.at(tick, &pace) + 1.0, tick };
    }

    fn jump(&mut self, unit: Unit, target: Hex) -> u64 {
        self.command(Command::Move { fleet_id: unit.fleet.id, target });
        2
    }

    /// Take the next hop of the fleet's remembered route, if it still holds.
    fn follow(&mut self, unit: Unit, homeward: bool) -> Option<u64> {
        let fleet = unit.fleet;
        let here = self.chart.get(fleet.location)?;
        let route = self.routes.get(&fleet.id)?;
        let target = route.path.get(route.next).copied();
        let holds = target.is_some_and(|target| {
            let open = self.chart.get(target).is_some_and(|s| target == self.home || enterable(self.odds, s, fleet));
            route.homeward == homeward && here.connections.contains(&target) && open
        });
        if !holds {
            self.routes.remove(&fleet.id);
            return None;
        }
        let wait = self.jump(unit, target?);
        self.routes.get_mut(&fleet.id)?.next += 1;
        Some(wait)
    }

    fn set_out(&mut self, unit: Unit, path: Vec<Hex>, homeward: bool) -> u64 {
        let wait = self.jump(unit, path[0]);
        self.routes.insert(unit.fleet.id, Route { path, next: 1, homeward });
        wait
    }

    fn head_home(&mut self, unit: Unit) -> u64 {
        let fleet = unit.fleet;
        if fleet.location != self.home && fleet.fuel < fleet.home_fuel && fleet.refine.is_some() {
            return self.refine(fleet);
        }
        if fleet.fuel < fleet.jump_fuel {
            return WAIT;
        }
        let beside_home = self.chart.get(fleet.location).is_some_and(|here| here.connections.contains(&self.home));
        let unwelcome = self.raid_threatens() || self.raid_fought() || (self.raid_announced() && !armed(fleet));
        if beside_home && unwelcome {
            return REST;
        }
        if let Some(wait) = self.follow(unit, true) {
            return wait;
        }
        let (home, odds) = (self.home, self.odds);
        let path = self
            .chart
            .nearest(fleet.location, |s| enterable(odds, s, fleet), |s, _| s.location == home)
            .or_else(|| self.chart.nearest(fleet.location, |_| true, |s, _| s.location == home));
        if let Some(path) = path {
            return self.set_out(unit, path, true);
        }
        let nearer = self
            .chart
            .get(fleet.location)
            .and_then(|here| here.connections.iter().copied().min_by_key(|n| Hex::distance(n, &home)));
        nearer.map_or(WAIT, |target| self.jump(unit, target))
    }

    fn worth_a_visit(&self, unit: Unit, sector: &SectorState, hops: u32) -> bool {
        let fleet = unit.fleet;
        if sector.location == self.home {
            return false;
        }
        let chart = if sector.chart.is_none() { self.chart_worth(unit, sector) } else { 0.0 };
        let board = sector.site.is_some_and(|site| boardable(self.odds, fleet, sector, site));
        let boarding = if board && self.brief.points_wanted {
            score::board_points(self.derelict(sector), sector.threat.rating)
        } else {
            0.0
        };
        let lasts = sector.salvage_despawn_tick.is_some_and(|t| t > self.state.tick + 3 * u64::from(hops));
        let scoop = sector.salvage.is_some() && lasts && fleet.cargo.total() < fleet.cargo_capacity;
        (chart > 0.0 || board || scoop) && self.pays(fleet, sector, hops, chart + boarding + self.findings(fleet, sector, board))
    }

    /// What charting `sector` is worth to `unit`. Charting is the survey
    /// wing's job, which finds what the other wings live on, so to it a
    /// chart is worth any trip its tank allows; to another wing it is worth
    /// its points while they still raise the score. This and the boarding
    /// points above are the only parts of the score a fleet's choice reads.
    fn chart_worth(&self, unit: Unit, sector: &SectorState) -> f32 {
        if unit.wing == Wing::Survey {
            f32::INFINITY
        } else if self.brief.points_wanted {
            score::chart_points(sector.threat.rating)
        } else {
            0.0
        }
    }

    /// A sector within scan range that the chart does not hold.
    fn blind_spot(&self, here: &SectorState, range: u8) -> bool {
        here.connections.iter().any(|&lane| match self.chart.get(lane) {
            None => true,
            Some(next) => range > 1 && next.connections.iter().any(|&far| self.chart.get(far).is_none()),
        })
    }

    fn scan(&mut self, fleet: &FleetState) -> u64 {
        self.routes.remove(&fleet.id);
        self.command(Command::Scan { fleet_id: fleet.id });
        u64::from(SCAN_COOLDOWN)
    }

    fn survey(&mut self, unit: Unit) -> u64 {
        let fleet = unit.fleet;
        let Some(here) = self.chart.get(fleet.location) else { return WAIT };
        let range = fleet.scan_range;
        if self.blind_spot(here, 1) {
            return self.scan(fleet);
        }
        if let Some(wait) = self.follow(unit, false) {
            return wait;
        }
        let odds = self.odds;
        let open = |s: &SectorState| enterable(odds, s, fleet);
        let unseen = self.chart.nearest(fleet.location, open, |s, hops| {
            open(s) && self.in_range(fleet, s, hops) && self.worth_a_visit(unit, s, hops)
        });
        if let Some(path) = unseen {
            return self.set_out(unit, path, false);
        }
        if self.blind_spot(here, range) {
            return self.scan(fleet);
        }
        let edge = self.chart.nearest(fleet.location, open, |s, hops| {
            open(s)
                && self.in_range(fleet, s, hops)
                && self.blind_spot(s, range)
                && self.pays(fleet, s, hops, self.chart_worth(unit, s))
        });
        match edge {
            Some(path) => self.set_out(unit, path, false),
            None if fleet.location != self.home => self.head_home(unit),
            None => WAIT,
        }
    }

    fn hunt(&mut self, unit: Unit, here: &SectorState) -> u64 {
        let (fleet, odds) = (unit.fleet, self.odds);
        let fought = self.state.battles.iter().any(|battle| battle.sector == here.location);
        if let Some(group) = here.hostiles.as_ref().and_then(|list| list.first())
            && !fought
            && beats(odds, fleet, here)
            && self.fight_pays(fleet, here, 0)
            && self.claim(Claim::Group, here.location)
        {
            self.note_kill(here.location);
            self.command(Command::Attack { fleet_id: fleet.id, target_fleet_id: group.id });
            return 2;
        }
        if let Some(wait) = self.follow(unit, false) {
            return wait;
        }
        let prey = self.chart.nearest(fleet.location, |s| enterable(odds, s, fleet), |s, hops| {
            s.hostiles.is_some()
                && beats(odds, fleet, s)
                && self.in_range(fleet, s, hops)
                && self.fight_pays(fleet, s, hops)
        });
        match prey {
            Some(path) => self.set_out(unit, path, false),
            None => self.nothing_pays(unit),
        }
    }

    /// No trip pays its fuel: a wing that may still earn points looks for
    /// them as the survey wing does; any other goes home and waits.
    fn nothing_pays(&mut self, unit: Unit) -> u64 {
        if self.brief.points_wanted {
            self.survey(unit)
        } else if unit.fleet.location != self.home {
            self.head_home(unit)
        } else {
            WAIT
        }
    }

    fn mine(&mut self, unit: Unit, here: &SectorState) -> u64 {
        let fleet = unit.fleet;
        if ore(here).total() > 0.5 && fleet.cargo.total() < fleet.cargo_capacity {
            self.command(Command::Harvest { fleet_id: fleet.id, resource: HarvestResource::Auto });
            return REST;
        }
        if let Some(wait) = self.follow(unit, false) {
            return wait;
        }
        let odds = self.odds;
        let open = |s: &SectorState| enterable(odds, s, fleet);
        let seam = self.chart.nearest(fleet.location, open, |s, hops| {
            let reserve = ore(s);
            reserve.total() >= MIN_ORE
                && open(s)
                && self.in_range(fleet, s, hops)
                && self.pays(fleet, s, hops, Self::carried(fleet, reserve))
        });
        match seam {
            Some(path) => self.set_out(unit, path, false),
            None => self.nothing_pays(unit),
        }
    }
}

/// What the ore tiles of `sector` hold, in raw units.
fn ore(sector: &SectorState) -> Resources {
    sector.ore_reserve.map_or_else(Resources::default, |o| Resources {
        metal: o.metal.units,
        crystal: o.crystal.units,
        deuterium: o.deuterium.units,
    })
}

/// Entering may start a fight: a group that is not passive, or a lair seen
/// empty that is not being watched and may have filled again.
fn guarded(sector: &SectorState) -> bool {
    match &sector.hostiles {
        Some(groups) => groups.iter().any(|g| g.behavior != NpcBehavior::Passive),
        None => !sector.live && sector.threat.basis == ThreatBasis::Template,
    }
}

/// Whether a fleet's odds in `sector` settle anything: a fight on entering,
/// prey, or a derelict that may be guarded.
fn decides(sector: &SectorState) -> bool {
    guarded(sector) || sector.hostiles.is_some() || sector.site.is_some_and(|site| site.risk != SiteRisk::Quiet)
}

/// What the fleet's ships cost to build, in points.
fn fleet_worth(fleet: &FleetState) -> f32 {
    fleet.ships.iter().map(|s| s.count as f32 * scaling::resource_weight(&s.ship_class.build_cost())).sum()
}

/// The share of its worth a fleet is expected to lose in a fight at odds
/// `ratio`, read off the table the odds are made from
/// (`scaling::RATIO_MARGINS`, what a fleet that wins keeps). The odds stop
/// at the table's last row, which any much stronger fleet reads, so that
/// row is taken as a fight that costs nothing.
fn worth_lost(ratio: f32) -> f32 {
    let table = scaling::RATIO_MARGINS;
    let last = table.len() - 1;
    let kept = |at: usize| if at == last { 1.0 } else { table[at].1 };
    let at = table.iter().position(|&(edge, _)| ratio < edge).unwrap_or(last + 1);
    let kept = match at {
        0 => kept(0),
        at if at > last => 1.0,
        at => {
            let (low, high) = (table[at - 1].0, table[at].0);
            kept(at - 1) + (kept(at) - kept(at - 1)) * (ratio - low) / (high - low)
        }
    };
    (1.0 - kept.max(0.0)).min(1.0)
}

fn ratio(odds: &Odds, fleet: &FleetState, sector: &SectorState) -> Option<f32> {
    odds.get(&(fleet.id, sector.location.to_key())).copied()
}

/// The fleet has a ship built to fight. One without picks no fight, whatever
/// its odds: a hundred cargo ships beat a Battlecruiser every time the odds
/// play it and lose twenty of themselves doing it.
fn armed(fleet: &FleetState) -> bool {
    fleet.ships.iter().any(|ships| Hull::of(ships.ship_class) == Hull::Warship)
}

/// Whether the server gives an armed fleet a fight in `sector` at the
/// pilot's odds. No answer is a no.
fn beats(odds: &Odds, fleet: &FleetState, sector: &SectorState) -> bool {
    armed(fleet) && ratio(odds, fleet, sector).is_some_and(|ratio| ratio >= SAFE_ODDS)
}

fn enterable(odds: &Odds, sector: &SectorState, fleet: &FleetState) -> bool {
    !guarded(sector) || beats(odds, fleet, sector)
}

/// A site is boarded when it reads quiet, or when the sector it lies in
/// reads favourable for an armed fleet: the guard an ambush wakes is a
/// share of the group the sector is rated by.
fn boardable(odds: &Odds, fleet: &FleetState, sector: &SectorState, site: SiteBrief) -> bool {
    site.risk == SiteRisk::Quiet
        || (armed(fleet) && ratio(odds, fleet, sector).is_some_and(|ratio| ratio >= scaling::RATIO_FAVOURABLE))
}
