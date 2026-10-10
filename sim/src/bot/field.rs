// Fleets: which wing a ship belongs to, the standing order each wing flies
// under when its owner is away, and the pilot that flies it when present.
//
// Every fleet is kept to one wing. Ships built while a wing is out collect
// at home; at a look-in the wing is called home, merged and sent out again.
//
//   wing       doctrine            with no work under it   by hand
//   garrison   none                                        stays docked
//   mining     mine_and_return     salvage_and_sites       nearest known ore, strip it, haul home
//   survey     prospect            salvage_and_sites       nearest sector not yet charted; boards
//   recovery   salvage_and_sites   mine_and_return         derelicts and scoops wreckage on the way
//   strike     patrol_home         salvage_and_sites       nearest group it beats safely, then as survey
//
// A doctrine that finds nothing to do brings its fleet home and holds. A
// fleet found at the dock with a full tank as a look-in ends is therefore
// switched to its wing's other doctrine, and back at a later look-in if
// that one runs out of work too.
//
// The pilot knows only the chart: it never enters a sector it has not seen
// (it scans first), it treats a cleared lair it cannot watch as occupied,
// and it reads off each sector's chart mark whether a visit still pays.
// Hull damage is never repaired, so it fights only with shields up and at
// odds that end the fight before the shields are through. The odds are the
// server's: before its orders the admiral asks (`odds`) how each fleet it
// may move would fare in every sector where that decides something, and it
// stays out of any sector it has no answer for.
//
// Fuel is paid in deuterium the moment a fleet docks, ahead of anything
// waiting in a queue. A pilot therefore leaves only when the stockpile has a
// tank to spare beyond the next orders. A fleet under a doctrine is held to
// the doctrine's fuel budget by the server.

use std::collections::{BTreeMap, BTreeSet};

use iac_shared::constants::{
    EXPLORE_DURATION_TICKS, FUEL_DEUT_PER_UNIT, MAX_FLEETS_PER_PLAYER, MAX_FLEETS_TOTAL,
    POLICY_DEFAULT_FUEL_BUDGET_PCT, SCAN_BASE_RANGE, SCAN_COOLDOWN, SCAN_SCOUT_RANGE, ShipClass,
};
use iac_shared::hex::Hex;
use iac_shared::protocol::{
    Command, FleetState, FleetStatus, GameState, HarvestResource, MAX_ODDS_SECTORS, NpcBehavior, PolicyParams,
    PolicyPreset, PolicyUpdate, SectorState, SiteBrief, SiteRisk, ThreatBasis,
};
use iac_shared::scaling;
use serde::{Deserialize, Serialize};

use super::home::Hull;
use super::Personality;
use crate::engine::MAX_SHIPS_PER_FLEET;
use crate::script::Order;

/// Odds ratio (`SectorOdds::ratio`) at which a pilot accepts a fight.
const SAFE_ODDS: f32 = 3.0;
/// How far around a fleet its pilot asks the server for odds. A guarded
/// sector farther off stays closed to it until it is nearer.
const ODDS_HORIZON: u16 = 12;
/// The ratio the prospect, mining and salvage doctrines are sent out with
/// (the game's default).
const DOCTRINE_RATIO_X10: u8 = 15;
/// The ratio a wing of at least `CAREFUL_WING_SHIPS` flies its doctrine at:
/// SAFE, a fight that keeps 98 percent of the hull or more.
const CAREFUL_RATIO_X10: u8 = 25;
const CAREFUL_WING_SHIPS: usize = 4;
/// Doctrines are leashed by odds and fuel, not by distance.
const DOCTRINE_RANGE: u8 = 30;
/// Fuel budget of the wings that haul, as a percent of deuterium income.
const HAUL_FUEL_BUDGET_PCT: u8 = 50;
const SHIELDS_UP: f32 = 0.95;
const RETURN_LOAD: f32 = 0.85;
/// Share of the tank kept spare on top of the way home.
const FUEL_MARGIN: f32 = 0.10;
const READY_FUEL: f32 = 0.90;
const REACH: u32 = 48;
const MIN_ORE: f32 = 3.0;
const WAIT: u64 = 30;
const REST: u64 = 3;

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

    fn of(personality: Personality, class: ShipClass) -> Wing {
        match (personality, Hull::of(class)) {
            (Personality::Turtle, _) => Wing::Garrison,
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

    /// Orders for `preset` for a fleet of `ships`. A fleet that hunts fights
    /// only what it beats without lasting damage. Any other flies at SAFE
    /// odds once it is large enough to leave home at them; a smaller one
    /// could not, and flies at the game's default. Doctrines that bring
    /// resources home earn their fuel back and get the larger budget; the
    /// ones that fly for points get the game's default.
    fn orders(preset: PolicyPreset, ships: usize) -> PolicyUpdate {
        let careful = if ships >= CAREFUL_WING_SHIPS { CAREFUL_RATIO_X10 } else { DOCTRINE_RATIO_X10 };
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Route {
    path: Vec<Hex>,
    next: usize,
    homeward: bool,
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

    fn shields_up(&self) -> bool {
        let (now, full) = self.fleet.ships.iter().fold((0.0, 0.0), |(now, full), s| (now + s.shield, full + s.shield_max));
        now >= SHIELDS_UP * full
    }
}

/// The server's odds for the empire's fleets as asked this turn: the ratio
/// by fleet id and sector key.
pub(super) type Odds = BTreeMap<(u64, u32), f32>;

fn units(state: &GameState, personality: Personality) -> Vec<Unit<'_>> {
    state
        .fleets
        .iter()
        .filter_map(|fleet| {
            let mut count = [0usize; Wing::ALL.len()];
            for ship in &fleet.ships {
                count[Wing::of(personality, ship.ship_class) as usize] += 1;
            }
            let wing = Wing::ALL.into_iter().max_by_key(|&w| count[w as usize])?;
            (wing != Wing::Garrison).then_some(Unit { fleet, wing, pure: count[wing as usize] == fleet.ships.len() })
        })
        .collect()
}

/// The `odds` requests behind this turn's orders. A fleet that will be
/// flown by hand is rated against the sectors around it where the answer
/// settles something; one that can only act where it stands, against that
/// sector; every fleet against home, if a fight waits there.
pub(super) fn questions(state: &GameState, brief: Brief, questions: &mut Vec<Command>) {
    let home = state.player.homeworld;
    for unit in units(state, brief.personality) {
        let fleet = unit.fleet;
        let manual = unit.free() && fleet.policy.is_none();
        let held = fleet.location == home && held_at_home(state, brief, unit).is_some();
        let flying = manual && fleet.fuel >= fleet.jump_fuel && !held;
        let reach = if flying { ODDS_HORIZON } else { 0 };
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

/// Deuterium in stock beyond what the next order in each queue costs.
fn fuel_money(state: &GameState) -> f32 {
    let home = &state.homeworld;
    let building = home.build_pending.first().map_or(0.0, |q| q.cost.deuterium);
    let research = home.research_pending.first().map_or(0.0, |q| q.cost.deuterium);
    let yard = home.shipyard_pending.first().map_or(0.0, |q| q.unit_cost.deuterium);
    state.player.resources.deuterium - building - research - yard
}

/// A wing that flies for points has nothing to gain once the score cap is
/// reached; the wings that bring resources home always do.
fn has_work(brief: Brief, wing: Wing) -> bool {
    brief.points_wanted || matches!(wing, Wing::Mining | Wing::Recovery)
}

/// How long a fleet docked at home stays there whatever the map looks like:
/// no work, a tank still filling, no fleet slot away, no fuel to spare.
fn held_at_home(state: &GameState, brief: Brief, unit: Unit) -> Option<u64> {
    let fleet = unit.fleet;
    let home = state.player.homeworld;
    if !has_work(brief, unit.wing) {
        return Some(WAIT);
    }
    let tank_filling = state.player.resources.deuterium >= 1.0;
    if fleet.fuel < READY_FUEL * fleet.fuel_max && tank_filling {
        return Some(5);
    }
    let deployed = state.fleets.iter().filter(|f| f.location != home).count();
    let fuelled = fuel_money(state) >= FUEL_DEUT_PER_UNIT * fleet.fuel_max;
    (deployed >= MAX_FLEETS_PER_PLAYER || !fuelled).then_some(WAIT)
}

/// What the admiral is told about the player it serves this turn.
#[derive(Clone, Copy)]
pub(super) struct Brief {
    pub personality: Personality,
    /// Ticks until the next look-in once this one ends; None for a player who stays.
    pub away_for: Option<u64>,
    /// More combat or exploration points would still raise the score.
    pub points_wanted: bool,
}

/// One turn's worth of fleet orders for one empire.
pub(super) struct Admiral<'a> {
    state: &'a GameState,
    brief: Brief,
    routes: &'a mut BTreeMap<u64, Route>,
    odds: &'a Odds,
    orders: &'a mut Vec<Order>,
    chart: Chart<'a>,
    home: Hex,
    units: Vec<Unit<'a>>,
    busy: BTreeSet<u64>,
}

impl<'a> Admiral<'a> {
    pub(super) fn new(
        state: &'a GameState,
        brief: Brief,
        routes: &'a mut BTreeMap<u64, Route>,
        odds: &'a Odds,
        orders: &'a mut Vec<Order>,
    ) -> Admiral<'a> {
        let units = units(state, brief.personality);
        Admiral {
            state,
            brief,
            routes,
            odds,
            orders,
            chart: Chart::new(&state.known_sectors),
            home: state.player.homeworld,
            units,
            busy: BTreeSet::new(),
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
        if !(has_work(self.brief, unit.wing) && self.can_return(unit)) {
            self.dock(unit);
            return;
        }
        let resting = leaving && fleet.location == self.home && unit.free() && fleet.fuel >= fleet.fuel_max;
        let preset = match fleet.policy.and_then(|flown| doctrines.iter().position(|d| *d == flown)) {
            Some(flown) if resting => doctrines[1 - flown],
            Some(flown) => doctrines[flown],
            None => doctrines[0],
        };
        if fleet.policy != Some(preset) || leaving {
            let orders = Wing::orders(preset, fleet.ships.len());
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
        for unit in docked.iter().filter(|u| !u.pure) {
            if self.state.fleets.len() >= MAX_FLEETS_TOTAL {
                break;
            }
            let strays: Vec<u64> = unit
                .fleet
                .ships
                .iter()
                .filter(|s| Wing::of(self.brief.personality, s.ship_class) != unit.wing)
                .map(|s| s.id)
                .collect();
            self.busy.insert(unit.fleet.id);
            self.command(Command::Split { fleet_id: unit.fleet.id, ship_ids: strays });
        }
        for wing in Wing::FIELD {
            let mut same: Vec<Unit> = docked.iter().copied().filter(|u| u.pure && u.wing == wing).collect();
            same.sort_by_key(|u| (u.fleet.policy.is_none(), u.fleet.id));
            let Some((keep, rest)) = same.split_first() else { continue };
            let mut size = keep.fleet.ships.len();
            for other in rest {
                if size + other.fleet.ships.len() > MAX_SHIPS_PER_FLEET {
                    continue;
                }
                size += other.fleet.ships.len();
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
            let mut sizes: Vec<usize> = fleets.iter().map(|u| u.fleet.ships.len()).collect();
            sizes.sort_unstable();
            let regroup = !closing && sizes.len() > 1 && sizes[0] + sizes[1] <= MAX_SHIPS_PER_FLEET;
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
            FleetStatus::Moving | FleetStatus::Exploring => return cooldown + 1,
            FleetStatus::InCombat | FleetStatus::Returning => return 2,
            FleetStatus::Harvesting => return REST,
            FleetStatus::Idle | FleetStatus::Docked if cooldown > 0 => return cooldown,
            FleetStatus::Idle | FleetStatus::Docked => {}
        }
        let Some(here) = self.chart.get(fleet.location) else { return WAIT };
        let at_home = fleet.location == self.home;
        let load = fleet.cargo.total();

        if !at_home {
            if here.salvage.is_some() && load < fleet.cargo_capacity {
                self.command(Command::CollectSalvage { fleet_id: fleet.id });
                return 1;
            }
            if here.site.is_some_and(|site| boardable(self.odds, fleet, here, site)) {
                self.command(Command::ExploreSite { fleet_id: fleet.id });
                return u64::from(EXPLORE_DURATION_TICKS) + 1;
            }
        }
        if fleet.fuel < fleet.jump_fuel {
            return WAIT;
        }
        if at_home {
            if let Some(wait) = held_at_home(self.state, self.brief, unit) {
                return wait;
            }
            if !self.can_return(unit) {
                return WAIT;
            }
        } else if !has_work(self.brief, unit.wing) {
            return self.head_home(unit);
        } else {
            let hold_full = fleet.cargo_capacity > 0.0 && load >= RETURN_LOAD * fleet.cargo_capacity;
            let fuel_low = fleet.fuel < fleet.home_fuel + 2.0 * fleet.jump_fuel + FUEL_MARGIN * fleet.fuel_max;
            if hold_full || fuel_low {
                return self.head_home(unit);
            }
        }
        match unit.wing {
            Wing::Strike => self.hunt(unit, here),
            Wing::Mining => self.mine(unit, here),
            Wing::Garrison | Wing::Survey | Wing::Recovery => self.survey(unit),
        }
    }

    /// After `hops` more jumps to `sector` the fleet still has the way home in the tank.
    fn in_range(&self, fleet: &FleetState, sector: &SectorState, hops: u32) -> bool {
        let home_hops = u32::from(Hex::distance(&sector.location, &self.home));
        fleet.fuel >= (hops + home_hops + 1) as f32 * fleet.jump_fuel + FUEL_MARGIN * fleet.fuel_max
    }

    /// Jump to `target`, unless a fight waits there and the shields are not
    /// back up yet. None when the fleet holds.
    fn jump(&mut self, unit: Unit, target: Hex) -> Option<u64> {
        let fight = self.chart.get(target).is_some_and(guarded);
        if fight && !unit.shields_up() {
            return None;
        }
        self.command(Command::Move { fleet_id: unit.fleet.id, target });
        Some(2)
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
        let Some(wait) = self.jump(unit, target?) else { return Some(REST) };
        self.routes.get_mut(&fleet.id)?.next += 1;
        Some(wait)
    }

    fn set_out(&mut self, unit: Unit, path: Vec<Hex>, homeward: bool) -> u64 {
        let Some(wait) = self.jump(unit, path[0]) else { return REST };
        self.routes.insert(unit.fleet.id, Route { path, next: 1, homeward });
        wait
    }

    fn head_home(&mut self, unit: Unit) -> u64 {
        let fleet = unit.fleet;
        if fleet.fuel < fleet.jump_fuel {
            return WAIT;
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
        nearer.and_then(|target| self.jump(unit, target)).unwrap_or(WAIT)
    }

    fn worth_a_visit(&self, unit: Unit, sector: &SectorState, hops: u32) -> bool {
        let fleet = unit.fleet;
        if sector.location == self.home {
            return false;
        }
        if sector.site.is_some_and(|site| boardable(self.odds, fleet, sector, site)) {
            return true;
        }
        let lasts = sector.salvage_despawn_tick.is_some_and(|t| t > self.state.tick + 3 * u64::from(hops));
        if sector.salvage.is_some() && lasts && fleet.cargo.total() < fleet.cargo_capacity {
            return true;
        }
        sector.chart.is_none()
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
        let scout = fleet.ships.iter().any(|s| s.ship_class == ShipClass::Scout);
        let range = if scout { SCAN_SCOUT_RANGE } else { SCAN_BASE_RANGE };
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
            open(s) && self.in_range(fleet, s, hops) && self.blind_spot(s, range)
        });
        match edge {
            Some(path) => self.set_out(unit, path, false),
            None if fleet.location != self.home => self.head_home(unit),
            None => WAIT,
        }
    }

    fn hunt(&mut self, unit: Unit, here: &SectorState) -> u64 {
        let (fleet, odds) = (unit.fleet, self.odds);
        if let Some(group) = here.hostiles.as_ref().and_then(|list| list.first())
            && beats(odds, fleet, here)
        {
            if !unit.shields_up() {
                return REST;
            }
            self.command(Command::Attack { fleet_id: fleet.id, target_fleet_id: group.id });
            return 2;
        }
        if let Some(wait) = self.follow(unit, false) {
            return wait;
        }
        let prey = self.chart.nearest(fleet.location, |s| enterable(odds, s, fleet), |s, hops| {
            s.hostiles.is_some() && beats(odds, fleet, s) && self.in_range(fleet, s, hops)
        });
        match prey {
            Some(path) => self.set_out(unit, path, false),
            None => self.survey(unit),
        }
    }

    fn mine(&mut self, unit: Unit, here: &SectorState) -> u64 {
        let fleet = unit.fleet;
        if ore(here) > 0.5 && fleet.cargo.total() < fleet.cargo_capacity {
            self.command(Command::Harvest { fleet_id: fleet.id, resource: HarvestResource::Auto });
            return REST;
        }
        if let Some(wait) = self.follow(unit, false) {
            return wait;
        }
        let odds = self.odds;
        let open = |s: &SectorState| enterable(odds, s, fleet);
        let seam = self.chart.nearest(fleet.location, open, |s, hops| {
            ore(s) >= MIN_ORE && open(s) && self.in_range(fleet, s, hops)
        });
        match seam {
            Some(path) => self.set_out(unit, path, false),
            None => self.survey(unit),
        }
    }
}

fn ore(sector: &SectorState) -> f32 {
    sector.ore_reserve.map_or(0.0, |o| o.metal.units + o.crystal.units + o.deuterium.units)
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

fn ratio(odds: &Odds, fleet: &FleetState, sector: &SectorState) -> Option<f32> {
    odds.get(&(fleet.id, sector.location.to_key())).copied()
}

/// Whether the server gives the fleet a fight in `sector` without lasting
/// damage. No answer is a no.
fn beats(odds: &Odds, fleet: &FleetState, sector: &SectorState) -> bool {
    ratio(odds, fleet, sector).is_some_and(|ratio| ratio >= SAFE_ODDS)
}

fn enterable(odds: &Odds, sector: &SectorState, fleet: &FleetState) -> bool {
    !guarded(sector) || beats(odds, fleet, sector)
}

/// A site is boarded when it reads quiet, or when the sector it lies in
/// reads favourable for the fleet: the guard an ambush wakes is a share of
/// the group the sector is rated by.
fn boardable(odds: &Odds, fleet: &FleetState, sector: &SectorState, site: SiteBrief) -> bool {
    site.risk == SiteRisk::Quiet || ratio(odds, fleet, sector).is_some_and(|ratio| ratio >= scaling::RATIO_FAVOURABLE)
}
