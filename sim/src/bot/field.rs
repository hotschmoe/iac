// Fleets: which wing a ship belongs to, the standing order each wing flies
// under when its owner is away, and the pilot that flies it when present.
//
// Every fleet is kept to one wing. Ships built while a wing is out collect
// at home; at a look-in the wing is called home, merged and sent out again.
//
//   wing       doctrine            by hand
//   garrison   none                stays docked
//   mining     mine_and_return     nearest known ore, strip it, haul home
//   survey     prospect            nearest sector not yet charted; boards
//   recovery   salvage_and_sites   derelicts and scoops wreckage on the way
//   strike     patrol_home         nearest group it beats safely, then as survey
//
// The pilot knows only the chart: it never enters a sector it has not seen
// (it scans first), and it treats a cleared lair it cannot watch as occupied.
// Hull damage is never repaired, so it fights only with shields up and at
// odds that end the fight before the shields are through.
//
// Fuel is paid in deuterium the moment a fleet docks, ahead of anything
// waiting in a queue. A pilot therefore leaves only when the stockpile has a
// tank to spare beyond the next orders; a doctrine cannot be told that, so a
// fleet is handed to one only while the mines out-produce what it burns.

use std::collections::{BTreeMap, BTreeSet};

use iac_shared::constants::{
    EXPLORE_DURATION_TICKS, FUEL_DEUT_PER_UNIT, MAX_FLEETS_PER_PLAYER, MAX_FLEETS_TOTAL, SCAN_BASE_RANGE,
    SCAN_COOLDOWN, SCAN_SCOUT_RANGE, ShipClass,
};
use iac_shared::hex::Hex;
use iac_shared::protocol::{
    Command, FleetState, FleetStatus, GameState, HarvestResource, NpcBehavior, PolicyParams, PolicyPreset,
    PolicyUpdate, SectorState, SiteBrief, SiteRisk, ThreatBasis,
};
use iac_shared::scaling;

use super::home::Hull;
use super::{Attention, Personality};
use crate::engine::MAX_SHIPS_PER_FLEET;
use crate::script::Order;

/// Power ratio at which a pilot accepts a fight.
const SAFE_ODDS: f32 = 3.0;
/// The same against hostiles with rapid fire on some of the fleet's ships:
/// enough to finish the group in a volley.
const OUTCLASSED_ODDS: f32 = 8.0;
/// The ratio the prospect, mining and salvage doctrines are sent out with
/// (the game's default); also what a hybrid pilot assumes its prospectors
/// cover without help.
const DOCTRINE_RATIO_X10: u8 = 15;
const SHIELDS_UP: f32 = 0.95;
const RETURN_LOAD: f32 = 0.85;
/// Share of the tank kept spare on top of the way home.
const FUEL_MARGIN: f32 = 0.10;
const READY_FUEL: f32 = 0.90;
const REACH: u32 = 48;
const MIN_ORE: f32 = 3.0;
const WAIT: u64 = 30;
const REST: u64 = 3;
/// Share of deuterium production that fleets under doctrines may burn.
const DOCTRINE_FUEL_SHARE: f32 = 0.5;
/// A doctrine jumps about this often.
const DOCTRINE_HOP_TICKS: f32 = 4.0;

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

    fn doctrine(self) -> Option<PolicyUpdate> {
        let (preset, max_range, engage_ratio_x10) = match self {
            Wing::Garrison => return None,
            Wing::Mining => (PolicyPreset::MineAndReturn, 8, DOCTRINE_RATIO_X10),
            Wing::Survey => (PolicyPreset::Prospect, 30, DOCTRINE_RATIO_X10),
            Wing::Recovery => (PolicyPreset::SalvageAndSites, 30, DOCTRINE_RATIO_X10),
            Wing::Strike => (PolicyPreset::PatrolHome, 30, (SAFE_ODDS * 10.0) as u8),
        };
        Some(PolicyUpdate {
            fleet_id: 0,
            preset,
            params: Some(PolicyParams { max_range, engage_ratio_x10, ..PolicyParams::default() }),
        })
    }
}

struct Route {
    path: Vec<Hex>,
    next: usize,
    homeward: bool,
}

/// What a bot remembers between turns about its fleets.
#[derive(Default)]
pub(super) struct Logbook {
    /// Sectors whose chart a fleet has brought home.
    delivered: BTreeSet<u32>,
    /// Sectors each fleet has been seen in since it last docked.
    carried: BTreeMap<u64, BTreeSet<u32>>,
    routes: BTreeMap<u64, Route>,
}

impl Logbook {
    pub(super) fn observe(&mut self, state: &GameState) {
        let home = state.player.homeworld;
        self.carried.retain(|id, _| state.fleets.iter().any(|f| f.id == *id));
        self.routes.retain(|id, _| state.fleets.iter().any(|f| f.id == *id));
        for fleet in &state.fleets {
            let key = fleet.location.to_key();
            if fleet.location == home {
                if let Some(charts) = self.carried.remove(&fleet.id) {
                    self.delivered.extend(charts);
                }
            } else if !self.delivered.contains(&key) {
                self.carried.entry(fleet.id).or_default().insert(key);
            }
        }
    }

    fn charted(&self, sector: Hex) -> bool {
        let key = sector.to_key();
        self.delivered.contains(&key) || self.carried.values().any(|c| c.contains(&key))
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

    fn shields_up(&self) -> bool {
        let (now, full) = self.fleet.ships.iter().fold((0.0, 0.0), |(now, full), s| (now + s.shield, full + s.shield_max));
        now >= SHIELDS_UP * full
    }
}

/// What the admiral is told about the player it serves this turn.
#[derive(Clone, Copy)]
pub(super) struct Brief {
    pub personality: Personality,
    pub attention: Attention,
    /// Ticks until the next look-in once this one ends; None for a player who stays.
    pub away_for: Option<u64>,
    /// More combat or exploration points would still raise the score.
    pub points_wanted: bool,
}

/// One turn's worth of fleet orders for one empire.
pub(super) struct Admiral<'a> {
    state: &'a GameState,
    brief: Brief,
    log: &'a mut Logbook,
    orders: &'a mut Vec<Order>,
    chart: Chart<'a>,
    home: Hex,
    units: Vec<Unit<'a>>,
    busy: BTreeSet<u64>,
    /// Deuterium per tick that fleets under standing orders may still burn.
    fuel_allowance: f32,
}

impl<'a> Admiral<'a> {
    pub(super) fn new(
        state: &'a GameState,
        brief: Brief,
        log: &'a mut Logbook,
        orders: &'a mut Vec<Order>,
    ) -> Admiral<'a> {
        let units = state
            .fleets
            .iter()
            .filter_map(|fleet| {
                let mut count = [0usize; Wing::ALL.len()];
                for ship in &fleet.ships {
                    count[Wing::of(brief.personality, ship.ship_class) as usize] += 1;
                }
                let wing = Wing::ALL.into_iter().max_by_key(|&w| count[w as usize])?;
                (wing != Wing::Garrison).then_some(Unit { fleet, wing, pure: count[wing as usize] == fleet.ships.len() })
            })
            .collect();
        let mut admiral = Admiral {
            state,
            brief,
            log,
            orders,
            chart: Chart::new(&state.known_sectors),
            home: state.player.homeworld,
            units,
            busy: BTreeSet::new(),
            fuel_allowance: 0.0,
        };
        let from_stock = brief.away_for.map_or(0.0, |ticks| admiral.fuel_money().max(0.0) / ticks.max(1) as f32);
        admiral.fuel_allowance = DOCTRINE_FUEL_SHARE * state.homeworld.production.deuterium + from_stock;
        admiral
    }

    fn command(&mut self, command: Command) {
        self.orders.push(Order::Command(command));
    }

    /// Deuterium in stock beyond what the next order in each queue costs.
    fn fuel_money(&self) -> f32 {
        let home = &self.state.homeworld;
        let building = home.build_pending.first().map_or(0.0, |q| q.cost.deuterium);
        let research = home.research_pending.first().map_or(0.0, |q| q.cost.deuterium);
        let yard = home.shipyard_pending.first().map_or(0.0, |q| q.unit_cost.deuterium);
        self.state.player.resources.deuterium - building - research - yard
    }

    /// Whether the mines (and the spare stock, spread over the absence) can
    /// keep `fleet` flying under a doctrine, which never stops on its own
    /// to let the stockpile recover.
    fn fund(&mut self, fleet: &FleetState) -> bool {
        let burn = fleet.jump_fuel * FUEL_DEUT_PER_UNIT / DOCTRINE_HOP_TICKS;
        let funded = burn <= self.fuel_allowance;
        if funded {
            self.fuel_allowance -= burn;
        }
        funded
    }

    /// A fleet that would lose to the lair in its own home sector cannot
    /// come back: arriving home starts that fight.
    fn can_return(&self, unit: Unit) -> bool {
        self.chart.get(self.home).is_none_or(|home| enterable(home, unit.fleet))
    }

    /// A wing that flies for points has nothing to gain once the score cap
    /// is reached; the wings that bring resources home always do.
    fn has_work(&self, wing: Wing) -> bool {
        self.brief.points_wanted || matches!(wing, Wing::Mining | Wing::Recovery)
    }

    /// Put the fleet under its wing's doctrine if it has work, the empire
    /// can fuel it and it can come home; otherwise bring it home and keep
    /// it docked.
    fn delegate(&mut self, unit: Unit) {
        let fleet = unit.fleet;
        let Some(mut update) = unit.wing.doctrine() else { return };
        if self.has_work(unit.wing) && self.can_return(unit) && self.fund(fleet) {
            if fleet.policy != Some(update.preset) {
                update.fleet_id = fleet.id;
                self.orders.push(Order::Policy(update));
            }
        } else {
            self.dock(unit);
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
                    self.delegate(unit);
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
                self.delegate(unit);
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
            if here.site.is_some_and(|site| boardable(fleet, here, site)) {
                self.command(Command::ExploreSite { fleet_id: fleet.id });
                return u64::from(EXPLORE_DURATION_TICKS) + 1;
            }
        }
        if fleet.fuel < fleet.jump_fuel {
            return WAIT;
        }
        if !self.has_work(unit.wing) {
            return if at_home { WAIT } else { self.head_home(unit) };
        }
        if at_home {
            let tank_filling = self.state.player.resources.deuterium >= 1.0;
            if fleet.fuel < READY_FUEL * fleet.fuel_max && tank_filling {
                return 5;
            }
            let deployed = self.state.fleets.iter().filter(|f| f.location != self.home).count();
            let fuelled = self.fuel_money() >= FUEL_DEUT_PER_UNIT * fleet.fuel_max;
            if deployed >= MAX_FLEETS_PER_PLAYER || !self.can_return(unit) || !fuelled {
                return WAIT;
            }
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
        let route = self.log.routes.get(&fleet.id)?;
        let target = route.path.get(route.next).copied();
        let holds = target.is_some_and(|target| {
            let open = self.chart.get(target).is_some_and(|s| target == self.home || enterable(s, fleet));
            route.homeward == homeward && here.connections.contains(&target) && open
        });
        if !holds {
            self.log.routes.remove(&fleet.id);
            return None;
        }
        let Some(wait) = self.jump(unit, target?) else { return Some(REST) };
        self.log.routes.get_mut(&fleet.id)?.next += 1;
        Some(wait)
    }

    fn set_out(&mut self, unit: Unit, path: Vec<Hex>, homeward: bool) -> u64 {
        let Some(wait) = self.jump(unit, path[0]) else { return REST };
        self.log.routes.insert(unit.fleet.id, Route { path, next: 1, homeward });
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
        let home = self.home;
        let path = self
            .chart
            .nearest(fleet.location, |s| enterable(s, fleet), |s, _| s.location == home)
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
        if sector.site.is_some_and(|site| boardable(fleet, sector, site)) {
            return true;
        }
        let lasts = sector.salvage_despawn_tick.is_some_and(|t| t > self.state.tick + 3 * u64::from(hops));
        if sector.salvage.is_some() && lasts && fleet.cargo.total() < fleet.cargo_capacity {
            return true;
        }
        let prospected = self.brief.attention != Attention::HandsOn
            && unit.wing == Wing::Survey
            && fleet.power * 10.0 >= f32::from(DOCTRINE_RATIO_X10) * sector.threat.est_power;
        !prospected && !self.log.charted(sector.location)
    }

    /// A sector within scan range that the chart does not hold.
    fn blind_spot(&self, here: &SectorState, range: u8) -> bool {
        here.connections.iter().any(|&lane| match self.chart.get(lane) {
            None => true,
            Some(next) => range > 1 && next.connections.iter().any(|&far| self.chart.get(far).is_none()),
        })
    }

    fn scan(&mut self, fleet: &FleetState) -> u64 {
        self.log.routes.remove(&fleet.id);
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
        let open = |s: &SectorState| enterable(s, fleet);
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
        let fleet = unit.fleet;
        if let Some(group) = here.hostiles.as_ref().and_then(|list| list.first())
            && beats(fleet, here)
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
        let prey = self.chart.nearest(fleet.location, |s| enterable(s, fleet), |s, hops| {
            s.hostiles.is_some() && beats(fleet, s) && self.in_range(fleet, s, hops)
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
        let open = |s: &SectorState| enterable(s, fleet);
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

/// Whether the fleet would win a fight in `sector` without lasting damage.
/// What it cannot see it takes for the ring's usual class.
fn beats(fleet: &FleetState, sector: &SectorState) -> bool {
    let usual = [scaling::npc_class(sector.location.dist_from_origin())];
    let seen = sector.hostiles.iter().flatten().flat_map(|group| group.ships.iter().map(|s| s.ship_class));
    let outclassed = seen
        .chain(usual.into_iter().filter(|_| sector.hostiles.is_none()))
        .any(|hostile| fleet.ships.iter().any(|ship| hostile.rapid_fire_vs(ship.ship_class) > 0));
    let odds = if outclassed { OUTCLASSED_ODDS } else { SAFE_ODDS };
    fleet.power >= odds * sector.threat.est_power
}

fn enterable(sector: &SectorState, fleet: &FleetState) -> bool {
    !guarded(sector) || beats(fleet, sector)
}

/// A site is boarded when it reads quiet, or when the fleet would beat the
/// guard an ambush wakes.
fn boardable(fleet: &FleetState, sector: &SectorState, site: SiteBrief) -> bool {
    let guard = scaling::AMBUSH_GUARD_SHARE * scaling::npc_power(sector.location.dist_from_origin());
    site.risk == SiteRisk::Quiet
        || fleet.power >= scaling::RATIO_FAVOURABLE * guard * scaling::THREAT_AGGRESSIVE_BONUS
}
