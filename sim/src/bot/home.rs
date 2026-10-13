// The homeworld planner every bot shares: what to queue next.
//
// One yardstick sizes everything: the cost of the next Metal Mine level. A
// support building or a tech is ordered once it costs no more than its
// appetite times that yardstick, so the ladder rises with the economy
// instead of following a fixed script. Mines are picked by payback, with a
// resource that sits low against its cap counting for more, and no mine is
// raised past the energy there is: the Solar Plant goes first. Fleet value
// is held at a share of the empire's building and research points, and home
// defence at a margin over the next raid.
//
// A personality names the research it wants and the ships it flies. What a
// research or a ship requires is not written here: the planner reads it from
// the catalogue and orders what is missing, a research before the one that
// needs it and the Shipyard level a class asks for once its research is in.
//
// Building and research orders are sent with `reserve`, so a queue runs in
// the order planned. A player who stays queues one order at a time; one
// about to leave fills every queue, gives each building place to an order
// worth its share of the income to come, and spends what no order has
// claimed.
//
// A shipyard batch is paid for whole when it is ordered, so the planner cuts
// a batch to the units the stockpile covers now and orders the rest at a
// later look, and orders none until the yard, building or tech it needs is
// built or under way.

use iac_shared::constants::{ResourceKind, Resources, ShipClass};
use iac_shared::pace::Pace;
use iac_shared::protocol::{Command, GameState, HomeworldState};
use iac_shared::scaling::{
    self, BuildingLevels, BuildingType, DefenceKind, PrereqRef, ResearchLevels, ResearchType, ShipyardItem,
};

use super::Personality;
use crate::script::{Order, Outlook, Want};

/// What a ship is for when sizing the fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hull {
    Scout,
    Warship,
    Hauler,
}

/// The line a role is filled from, heaviest first.
const SCOUTS: &[ShipClass] = &[ShipClass::EspionageProbe];
const WARSHIPS: &[ShipClass] =
    &[ShipClass::Battleship, ShipClass::Cruiser, ShipClass::HeavyFighter, ShipClass::LightFighter];
const HAULERS: &[ShipClass] = &[ShipClass::LargeCargo, ShipClass::SmallCargo];
const MINERS: &[ShipClass] = &[ShipClass::Recycler, ShipClass::LargeCargo, ShipClass::SmallCargo];

impl Hull {
    pub(super) fn of(class: ShipClass) -> Hull {
        match class {
            ShipClass::EspionageProbe => Hull::Scout,
            ShipClass::SmallCargo | ShipClass::LargeCargo | ShipClass::Recycler => Hull::Hauler,
            ShipClass::LightFighter
            | ShipClass::HeavyFighter
            | ShipClass::Cruiser
            | ShipClass::Battleship
            | ShipClass::Battlecruiser
            | ShipClass::Bomber
            | ShipClass::Destroyer
            | ShipClass::Deathstar
            | ShipClass::SolarSatellite => Hull::Warship,
        }
    }
}

struct Profile {
    tank: f32,
    tech: f32,
    research: &'static [(ResearchType, u8)],
    /// Fleet value held, as a share of building and research points.
    fleet_share: f32,
    hulls: &'static [(Hull, f32)],
    /// What its haulers are: a miner's line begins with the Recycler.
    haulers: &'static [ShipClass],
    /// Structure power held, as a multiple of the next raid's expected power.
    defence_margin: f32,
}

/// Computer Technology opens building slots, which every plan is short of.
const BUILD_SLOT_APPETITE: f32 = 6.0;
/// A heavier class is worked toward once one ship of it costs no more than
/// this many yardsticks.
const CLASS_APPETITE: f32 = 6.0;
/// Techs outside the personality's plan, bought as the economy outgrows them.
const SPARE_TECH_APPETITE: f32 = 0.25;
const STORAGE_PRESSURE: f64 = 0.75;
/// Share of the income expected while away that the building queue should hold.
const ABSORB: f32 = 0.6;
/// How strongly a lopsided stockpile tilts the choice of mine.
const SCARCITY_PULL: f64 = 2.0;
const MAX_SHIP_BATCH: f32 = 12.0;
const MAX_DEFENCE_BATCH: f32 = 100.0;
const MAX_SURPLUS_BATCH: u16 = 500;
const MAX_SATELLITE_BATCH: f64 = 50.0;
/// The one defence a bot builds.
const TURRET: DefenceKind = DefenceKind::RocketLauncher;

use ResearchType as T;

const TURTLE: Profile = Profile {
    tank: 0.0,
    tech: 2.0,
    research: &[
        (T::CombustionDrive, 1),
        (T::ComputerTechnology, 3),
        (T::ArmourTechnology, 2),
        (T::WeaponsTechnology, 2),
        (T::ComputerTechnology, 6),
        (T::ArmourTechnology, 4),
        (T::WeaponsTechnology, 4),
        (T::ShieldingTechnology, 3),
    ],
    fleet_share: 0.0,
    hulls: &[],
    haulers: HAULERS,
    defence_margin: 1.5,
};

const MINER: Profile = Profile {
    tank: 0.5,
    tech: 3.0,
    research: &[
        (T::CombustionDrive, 2),
        (T::LaserTechnology, 1),
        (T::ComputerTechnology, 3),
        (T::LaserTechnology, 3),
        (T::CombustionDrive, 6),
        (T::ShieldingTechnology, 2),
        (T::LaserTechnology, 5),
        (T::EnergyTechnology, 5),
        (T::ComputerTechnology, 6),
        (T::ImpulseDrive, 5),
    ],
    fleet_share: 0.10,
    hulls: &[(Hull::Hauler, 1.0)],
    haulers: MINERS,
    defence_margin: 1.15,
};

const RAIDER: Profile = Profile {
    tank: 1.5,
    tech: 4.0,
    research: &[
        (T::CombustionDrive, 1),
        (T::WeaponsTechnology, 1),
        (T::ArmourTechnology, 2),
        (T::ImpulseDrive, 2),
        (T::ComputerTechnology, 3),
        (T::CombustionDrive, 2),
        (T::WeaponsTechnology, 2),
        (T::ImpulseDrive, 4),
        (T::IonTechnology, 2),
        (T::ShieldingTechnology, 2),
        (T::WeaponsTechnology, 3),
        (T::ArmourTechnology, 3),
        (T::ComputerTechnology, 6),
        (T::EnergyTechnology, 5),
        (T::ShieldingTechnology, 5),
        (T::HyperspaceTechnology, 3),
        (T::HyperspaceDrive, 4),
        (T::WeaponsTechnology, 5),
        (T::ArmourTechnology, 5),
    ],
    fleet_share: 0.30,
    hulls: &[(Hull::Warship, 0.85), (Hull::Hauler, 0.15)],
    haulers: HAULERS,
    defence_margin: 1.15,
};

const EXPLORER: Profile = Profile {
    tank: 1.5,
    tech: 4.0,
    research: &[
        (T::CombustionDrive, 1),
        (T::EspionageTechnology, 2),
        (T::CombustionDrive, 3),
        (T::ComputerTechnology, 3),
        (T::EnergyTechnology, 3),
        (T::ArmourTechnology, 2),
        (T::ImpulseDrive, 2),
        (T::EspionageTechnology, 4),
        (T::CombustionDrive, 6),
        (T::ComputerTechnology, 6),
        (T::ImpulseDrive, 4),
        (T::IonTechnology, 2),
        (T::EnergyTechnology, 6),
        (T::WeaponsTechnology, 2),
        (T::ShieldingTechnology, 2),
    ],
    fleet_share: 0.20,
    hulls: &[(Hull::Scout, 0.10), (Hull::Warship, 0.60), (Hull::Hauler, 0.30)],
    haulers: HAULERS,
    defence_margin: 1.15,
};

const MIXED: Profile = Profile {
    tank: 1.5,
    tech: 4.0,
    research: &[
        (T::CombustionDrive, 1),
        (T::ComputerTechnology, 3),
        (T::CombustionDrive, 2),
        (T::WeaponsTechnology, 1),
        (T::ArmourTechnology, 2),
        (T::ImpulseDrive, 2),
        (T::EspionageTechnology, 2),
        (T::CombustionDrive, 3),
        (T::LaserTechnology, 2),
        (T::WeaponsTechnology, 2),
        (T::ImpulseDrive, 4),
        (T::IonTechnology, 2),
        (T::ComputerTechnology, 6),
        (T::ShieldingTechnology, 2),
        (T::EnergyTechnology, 5),
        (T::WeaponsTechnology, 3),
        (T::ArmourTechnology, 3),
        (T::CombustionDrive, 6),
    ],
    fleet_share: 0.25,
    hulls: &[(Hull::Scout, 0.05), (Hull::Warship, 0.70), (Hull::Hauler, 0.25)],
    haulers: HAULERS,
    defence_margin: 1.15,
};

impl Profile {
    fn of(personality: Personality) -> &'static Profile {
        match personality {
            Personality::Turtle => &TURTLE,
            Personality::Miner => &MINER,
            Personality::Raider => &RAIDER,
            Personality::Explorer => &EXPLORER,
            Personality::Mixed => &MIXED,
        }
    }

    fn appetite(&self, building: BuildingType) -> f32 {
        match building {
            BuildingType::Shipyard | BuildingType::ResearchLab => 2.0,
            BuildingType::RoboticsFactory => 2.5,
            BuildingType::NaniteFactory => 1.0,
            BuildingType::DeuteriumTank => self.tank,
            BuildingType::MetalMine
            | BuildingType::CrystalMine
            | BuildingType::DeuteriumSynthesizer
            | BuildingType::SolarPlant
            | BuildingType::FusionReactor
            | BuildingType::MetalStorage
            | BuildingType::CrystalStorage => 0.0,
        }
    }

    fn line(&self, hull: Hull) -> &'static [ShipClass] {
        match hull {
            Hull::Scout => SCOUTS,
            Hull::Warship => WARSHIPS,
            Hull::Hauler => self.haulers,
        }
    }
}

const MINES: [(BuildingType, ResourceKind); 3] = [
    (BuildingType::MetalMine, ResourceKind::Metal),
    (BuildingType::CrystalMine, ResourceKind::Crystal),
    (BuildingType::DeuteriumSynthesizer, ResourceKind::Deuterium),
];

const SUPPORT: [BuildingType; 5] = [
    BuildingType::RoboticsFactory,
    BuildingType::Shipyard,
    BuildingType::ResearchLab,
    BuildingType::DeuteriumTank,
    BuildingType::NaniteFactory,
];

fn fits(cost: Resources, cap: Resources) -> bool {
    ResourceKind::ALL.into_iter().all(|k| cost.get(k) <= cap.get(k))
}

/// The empire as it will stand once its orders have finished.
struct Projection {
    buildings: BuildingLevels,
    research: ResearchLevels,
}

impl Projection {
    /// Every order counts when `waiting` is set; otherwise only those under
    /// way, which finish on the clock whatever the stockpile does.
    fn of(home: &HomeworldState, waiting: bool) -> Projection {
        let mut buildings = BuildingLevels::default();
        for b in &home.buildings {
            buildings.set(b.building_type, b.level);
        }
        let running = home.build_queue.iter().map(|q| (q.building_type, q.target_level));
        let pending = home.build_pending.iter().filter(|_| waiting).map(|q| (q.building_type, q.target_level));
        for (b, level) in running.chain(pending) {
            buildings.set(b, buildings.get(b).max(level));
        }

        let mut research = ResearchLevels::default();
        for r in &home.research {
            research.set(r.tech, r.level);
        }
        let running = home.research_active.iter().map(|q| (q.tech, q.target_level));
        let pending = home.research_pending.iter().filter(|_| waiting).map(|q| (q.tech, q.target_level));
        for (t, level) in running.chain(pending) {
            research.set(t, research.get(t).max(level));
        }
        Projection { buildings, research }
    }

    fn unlocked(&self, item: ShipyardItem) -> bool {
        scaling::prerequisites_met(item.prerequisites(), &self.buildings, &self.research)
    }

    /// The heaviest class of `line` that can be built.
    fn heaviest(&self, line: &[ShipClass]) -> Option<ShipClass> {
        line.iter().copied().find(|&class| self.unlocked(ShipyardItem::Ship(class)))
    }
}

/// The next level of a tech the profile calls for.
struct Rung {
    tech: ResearchType,
    cost: Resources,
    /// The economy has grown into its cost.
    due: bool,
}

struct Planner<'a> {
    state: &'a GameState,
    home: &'a HomeworldState,
    profile: &'static Profile,
    pace: Pace,
    built: BuildingLevels,
    plan: Projection,
    /// What stands once the orders under way have finished. A batch pays
    /// when it is ordered, so the yard is judged by this and not by `plan`:
    /// a batch whose yard, building or tech still waits for resources would
    /// hold its price for nothing, and take it from the order it waits on.
    started: Projection,
    /// The stockpile less the batches ordered this turn: what a batch can
    /// be paid from.
    stock: Resources,
    /// `stock` less every building and research order already waiting or
    /// queued this turn: what no order has claimed.
    spare: Resources,
    /// Places left in the shipyard queue.
    yard_room: usize,
    /// Buildings that something the profile wants is waiting on.
    needs: Vec<(BuildingType, u8)>,
    /// The price of the unit the yard wants next when the stockpile does not
    /// cover even one.
    yard_saving: Resources,
}

/// Queue what the empire should build next. `away_for` is how long the
/// player will be gone after this look-in; None for a player who stays.
/// Returns what the shipyard is saving for: a batch cannot be ordered ahead
/// of the resources, so nothing in the queues says so.
pub(super) fn plan(state: &GameState, personality: Personality, away_for: Option<u64>, orders: &mut Vec<Order>) -> Resources {
    let Some(mut planner) = Planner::new(state, personality) else { return Resources::default() };
    planner.research(away_for.is_some(), orders);
    planner.shipyard(orders);
    planner.buildings(away_for, orders);
    if away_for.is_some() {
        planner.surplus(orders);
    }
    planner.yard_saving
}

/// What `plan` would do with one free place in each queue, asked in the
/// order `plan` works. Nothing is ordered.
pub(super) fn outlook(state: &GameState, personality: Personality, away_for: Option<u64>) -> Option<Outlook> {
    let mut planner = Planner::new(state, personality)?;
    let research = planner.research_want();
    let shipyard = planner.shipyard_want(away_for.is_some());
    let building = planner.building_want(away_for);
    Some(Outlook { building, research, shipyard })
}

impl<'a> Planner<'a> {
    fn new(state: &'a GameState, personality: Personality) -> Option<Planner<'a>> {
        let pace = Pace::new(state.world.pace).ok()?;
        let home = &state.homeworld;
        let mut built = BuildingLevels::default();
        for b in &home.buildings {
            built.set(b.building_type, b.level);
        }
        let waiting = home
            .build_pending
            .iter()
            .map(|q| q.cost)
            .chain(home.research_pending.iter().map(|q| q.cost))
            .fold(Resources::default(), Resources::add);
        Some(Planner {
            state,
            home,
            profile: Profile::of(personality),
            pace,
            built,
            plan: Projection::of(home, true),
            started: Projection::of(home, false),
            stock: state.player.resources,
            spare: state.player.resources.sub(waiting),
            yard_room: 0,
            needs: Vec::new(),
            yard_saving: Resources::default(),
        })
    }

    fn research_want(&mut self) -> Want {
        if let Some(tech) = self.next_tech() {
            let cost = scaling::research_cost(tech, self.plan.research.get(tech) + 1);
            return Want::Order { covered: fits(cost, self.spare) };
        }
        let ladder = self.tech_ladder();
        if ladder.is_empty() {
            Want::Nothing
        } else if ladder.iter().any(|rung| rung.due) {
            Want::Prerequisite
        } else {
            let covered = ladder.iter().any(|rung| self.tech_open(rung.tech) && fits(rung.cost, self.spare));
            Want::NotYet { covered }
        }
    }

    /// The yard wants what the plan calls for; a player about to leave also
    /// wants whatever the surplus buys.
    fn shipyard_want(&mut self, leaving: bool) -> Want {
        self.yard_needs();
        if self.started.buildings.shipyard == 0 {
            return Want::Prerequisite;
        }
        let satellites = self.satellites_wanted().map(|_| ShipyardItem::Ship(ShipClass::SolarSatellite));
        let turrets = self.turrets_wanted().map(|_| ShipyardItem::Defence(TURRET));
        let ships = self.ships_wanted().map(|(class, _)| ShipyardItem::Ship(class));
        let called_for: Vec<ShipyardItem> = satellites.into_iter().chain(turrets).chain(ships).collect();
        if !called_for.is_empty() {
            return Want::Order { covered: called_for.iter().any(|item| fits(item.unit_cost(), self.stock)) };
        }
        let covered = self.surplus_item().is_some_and(|item| fits(item.unit_cost(), self.spare));
        if leaving && covered { Want::Order { covered } } else { Want::NotYet { covered } }
    }

    fn building_want(&self, away_for: Option<u64>) -> Want {
        let slots = self.home.build_slots as usize;
        let room = self.room(self.home.build_queue.len(), self.home.build_pending.len(), slots, away_for.is_some());
        let place_worth = ABSORB * self.building_budget(away_for) / room.max(1) as f32;
        match self.next_building(place_worth) {
            Some(building) => {
                let cost = scaling::building_cost(building, self.plan.buildings.get(building) + 1);
                Want::Order { covered: fits(cost, self.spare) }
            }
            None if MINES.iter().all(|m| self.plan.buildings.get(m.0) >= scaling::MAX_BUILDING_LEVEL) => Want::Nothing,
            None => Want::Prerequisite,
        }
    }

    fn yardstick(&self) -> f32 {
        let level = (self.plan.buildings.metal_mine + 1).min(scaling::MAX_BUILDING_LEVEL);
        scaling::resource_weight(&scaling::building_cost(BuildingType::MetalMine, level))
    }

    fn cap(&self) -> Resources {
        self.home.storage.cap
    }

    /// Room in a queue: everything it holds when queueing ahead, otherwise
    /// one order and only while nothing waits.
    fn room(&self, held: usize, waiting: usize, slots: usize, queue_ahead: bool) -> usize {
        let free = (slots + self.home.queue_waiting_max as usize).saturating_sub(held + waiting);
        if queue_ahead { free } else { free.min(usize::from(waiting == 0)) }
    }

    fn need(&mut self, building: BuildingType, level: u8) {
        if self.plan.buildings.get(building) < level {
            self.needs.push((building, level));
        }
    }

    fn research(&mut self, queue_ahead: bool, orders: &mut Vec<Order>) {
        let held = usize::from(self.home.research_active.is_some());
        let room = self.room(held, self.home.research_pending.len(), 1, queue_ahead);
        for _ in 0..room {
            let Some(tech) = self.next_tech() else { break };
            let level = self.plan.research.get(tech) + 1;
            self.plan.research.set(tech, level);
            self.spare = self.spare.sub(scaling::research_cost(tech, level));
            orders.push(Order::Command(Command::Research { tech, reserve: true }));
        }
    }

    /// Every tech level the profile still calls for, in the order it is
    /// considered: the personality's plan, then the rest.
    fn tech_ladder(&self) -> Vec<Rung> {
        let yardstick = self.yardstick();
        let profile = self.profile;
        let planned = profile.research.iter().map(|&(tech, level)| (tech, level, profile.tech));
        let rest = T::ALL.into_iter().map(|tech| (tech, scaling::research_max_level(tech), SPARE_TECH_APPETITE));
        planned
            .chain(rest)
            .filter_map(|(tech, goal, appetite)| {
                let level = self.plan.research.get(tech) + 1;
                if level > goal.min(scaling::research_max_level(tech)) || scaling::research_energy(tech, level) > 0 {
                    return None;
                }
                let appetite = if tech == T::ComputerTechnology { appetite.max(BUILD_SLOT_APPETITE) } else { appetite };
                let cost = scaling::research_cost(tech, level);
                Some(Rung { tech, cost, due: scaling::resource_weight(&cost) <= appetite * yardstick })
            })
            .collect()
    }

    /// The first rung that is due and can be queued now. A rung that waits
    /// on another research gives way to that research.
    fn next_tech(&mut self) -> Option<ResearchType> {
        let cap = self.cap();
        let ladder = self.tech_ladder();
        for rung in ladder.into_iter().filter(|rung| rung.due) {
            if let Some(tech) = self.toward(rung.tech, 0)
                && fits(scaling::research_cost(tech, self.plan.research.get(tech) + 1), cap)
            {
                return Some(tech);
            }
        }
        None
    }

    /// Whether `tech` could be queued now, noting nothing.
    fn tech_open(&self, tech: ResearchType) -> bool {
        self.plan.buildings.research_lab >= 1
            && scaling::research_prerequisites_met(tech, &self.plan.buildings, &self.plan.research)
            && fits(scaling::research_cost(tech, self.plan.research.get(tech) + 1), self.cap())
    }

    /// What to research now to get the next level of `tech`: `tech` itself
    /// when everything it requires stands, the research it waits on when it
    /// does not, None while it waits on a building. The buildings missing
    /// are noted.
    fn toward(&mut self, tech: ResearchType, depth: u8) -> Option<ResearchType> {
        if depth > ResearchType::COUNT as u8 {
            return None;
        }
        let mut ready = self.plan.buildings.research_lab >= 1;
        self.need(BuildingType::ResearchLab, 1);
        let mut first = None;
        for &(prereq, level) in scaling::research_prerequisites(tech) {
            match prereq {
                PrereqRef::Building(building) => {
                    ready &= self.plan.buildings.get(building) >= level;
                    self.need(building, level);
                }
                PrereqRef::Research(other) if self.plan.research.get(other) < level => {
                    first = first.or(Some(other));
                }
                PrereqRef::Research(_) => {}
            }
        }
        match first {
            Some(other) => self.toward(other, depth + 1),
            None => ready.then_some(tech),
        }
    }

    /// The Shipyard levels the fleet is waiting on: the first, and the one
    /// the next class up of each line asks for once its research is in and
    /// the economy has grown into its price.
    fn yard_needs(&mut self) {
        if self.profile.fleet_share <= 0.0 {
            return;
        }
        self.need(BuildingType::Shipyard, 1);
        let yardstick = self.yardstick();
        for &(hull, _) in self.profile.hulls {
            let line = self.profile.line(hull);
            let have = line.iter().position(|&class| self.plan.unlocked(ShipyardItem::Ship(class))).unwrap_or(line.len());
            let Some(&next) = have.checked_sub(1).and_then(|i| line.get(i)) else { continue };
            if scaling::resource_weight(&next.build_cost()) > CLASS_APPETITE * yardstick {
                continue;
            }
            let prereqs = scaling::ship_prerequisites(next);
            let researched = prereqs.iter().all(|&(p, level)| match p {
                PrereqRef::Research(tech) => self.plan.research.get(tech) >= level,
                PrereqRef::Building(_) => true,
            });
            if !researched {
                continue;
            }
            for &(p, level) in prereqs {
                if let PrereqRef::Building(building) = p {
                    self.need(building, level);
                }
            }
        }
    }

    fn shipyard(&mut self, orders: &mut Vec<Order>) {
        let held = usize::from(self.home.shipyard_queue.is_some());
        self.yard_needs();
        if self.started.buildings.shipyard == 0 {
            return;
        }
        self.yard_room = self.room(held, self.home.shipyard_pending.len(), 1, true);
        // A batch waiting for the yard is already paid for: the plan orders
        // more only once the line behind the running batch is clear, so it
        // does not pay far ahead of what the yard can build.
        if !self.home.shipyard_pending.is_empty() {
            return;
        }
        if let Some(count) = self.satellites_wanted() {
            self.wanted(ShipyardItem::Ship(ShipClass::SolarSatellite), count, orders);
        }
        if let Some(count) = self.turrets_wanted() {
            self.wanted(ShipyardItem::Defence(TURRET), count, orders);
        }
        if let Some((class, count)) = self.ships_wanted() {
            self.wanted(ShipyardItem::Ship(class), count, orders);
        }
    }

    /// Order up to `count` of something the plan calls for, as many as the
    /// stockpile covers; when it covers none, the yard is saving for one.
    fn wanted(&mut self, item: ShipyardItem, count: u16, orders: &mut Vec<Order>) {
        if self.yard_room > 0 && !self.batch(item, count, self.stock, orders) {
            self.yard_saving = self.yard_saving.add(item.unit_cost());
        }
    }

    /// Order up to `count` of `item`. A batch is paid for whole when it is
    /// ordered, so it is cut to the units `purse` covers; a later look
    /// orders the rest. False when it covers none or the yard has no room.
    fn batch(&mut self, item: ShipyardItem, count: u16, purse: Resources, orders: &mut Vec<Order>) -> bool {
        let unit = item.unit_cost();
        let covered = ResourceKind::ALL
            .into_iter()
            .filter(|&k| unit.get(k) > 0.0)
            .map(|k| (purse.get(k) / unit.get(k)).floor().max(0.0))
            .fold(f64::from(count), f64::min) as u16;
        if self.yard_room == 0 || covered == 0 {
            return false;
        }
        self.yard_room -= 1;
        let cost = item.batch_cost(covered);
        self.stock = self.stock.sub(cost);
        self.spare = self.spare.sub(cost);
        orders.push(Order::Command(match item {
            ShipyardItem::Ship(ship_class) => Command::BuildShip { ship_class, count: covered },
            ShipyardItem::Defence(kind) => Command::BuildDefence { kind, count: covered },
        }));
        true
    }

    /// What is in stock beyond every order placed would sit there scoring
    /// nothing until the next look-in. An empire that flies puts it into
    /// Rocket Launchers, which cost no deuterium; one that does not, into
    /// the heaviest warship it can build, and into Rocket Launchers while it
    /// can build none. Only stock no order has claimed is spent, so no
    /// queued order is kept waiting by it.
    fn surplus(&mut self, orders: &mut Vec<Order>) {
        if let Some(item) = self.surplus_item() {
            self.batch(item, MAX_SURPLUS_BATCH, self.spare, orders);
        }
    }

    fn surplus_item(&self) -> Option<ShipyardItem> {
        let turret = ShipyardItem::Defence(TURRET);
        let hull = self.started.heaviest(WARSHIPS).map(ShipyardItem::Ship).filter(|_| self.profile.fleet_share <= 0.0);
        let item = hull.unwrap_or(turret);
        (self.started.unlocked(item) && fits(item.unit_cost(), self.cap())).then_some(item)
    }

    fn ordered(&self) -> impl Iterator<Item = (ShipyardItem, u16)> + '_ {
        let running = self.home.shipyard_queue.iter().map(|q| (q.item, q.count - q.built));
        let waiting = self.home.shipyard_pending.iter().map(|q| (q.item, q.count));
        running.chain(waiting)
    }

    /// Energy the planned buildings make and need, with the satellites that
    /// stand and those on order.
    fn planned_power(&self, buildings: &BuildingLevels) -> scaling::Power {
        let coming: u32 = self
            .ordered()
            .filter(|(item, _)| *item == ShipyardItem::Ship(ShipClass::SolarSatellite))
            .map(|(_, left)| u32::from(left))
            .sum();
        scaling::home_power(buildings, &self.plan.research, self.home.satellites + coming, true, &self.pace)
    }

    /// Solar Satellites, once the Solar Plant can rise no further and the
    /// mines planned are still short of energy.
    fn satellites_wanted(&self) -> Option<u16> {
        let item = ShipyardItem::Ship(ShipClass::SolarSatellite);
        if self.plan.buildings.solar_plant < scaling::MAX_BUILDING_LEVEL || !self.started.unlocked(item) {
            return None;
        }
        let power = self.planned_power(&self.plan.buildings);
        let short = f64::from(power.needed.saturating_sub(power.made));
        let count = (short / f64::from(scaling::satellite_energy())).ceil().min(MAX_SATELLITE_BATCH);
        (count >= 1.0 && fits(item.unit_cost(), self.cap())).then_some(count as u16)
    }

    /// Rocket Launchers give the most power for their cost of the defences
    /// that need no research.
    fn turrets_wanted(&self) -> Option<u16> {
        if !self.started.unlocked(ShipyardItem::Defence(TURRET)) {
            return None;
        }
        let raid = scaling::raid_power_base(scaling::econ_points(&self.plan.buildings, &self.plan.research));
        let power_of = |kind: DefenceKind| {
            self.home.defences.iter().find(|d| d.kind == kind).map_or(kind.power(), |d| d.power)
        };
        let standing: f32 = self.home.defences.iter().map(|d| d.count as f32 * d.power).sum();
        let coming: f32 = self
            .ordered()
            .filter_map(|(item, left)| match item {
                ShipyardItem::Defence(d) => Some(f32::from(left) * power_of(d)),
                ShipyardItem::Ship(_) => None,
            })
            .sum();
        let short = self.profile.defence_margin * raid - standing - coming;
        let count = (short / power_of(TURRET)).ceil().min(MAX_DEFENCE_BATCH);
        (count >= 1.0 && fits(TURRET.build_cost(), self.cap())).then_some(count as u16)
    }

    fn ships_wanted(&self) -> Option<(ShipClass, u16)> {
        let mut built_research = ResearchLevels::default();
        for r in &self.home.research {
            built_research.set(r.tech, r.level);
        }
        let target = self.profile.fleet_share * scaling::econ_points(&self.built, &built_research);
        let value = |class: ShipClass| scaling::resource_weight(&class.build_cost());

        let flying = self.state.fleets.iter().flat_map(|f| f.ships.iter()).map(|s| (s.ship_class, s.count as f32));
        let coming = self.ordered().filter_map(|(item, left)| match item {
            ShipyardItem::Ship(class) if class.flies() => Some((class, f32::from(left))),
            _ => None,
        });
        let mut have = [0.0f32; 3];
        for (class, count) in flying.chain(coming) {
            have[Hull::of(class) as usize] += count * value(class);
        }
        let budget = target - have.iter().sum::<f32>();

        let mut best: Option<(ShipClass, f32)> = None;
        for &(hull, share) in self.profile.hulls {
            let Some(class) = self.started.heaviest(self.profile.line(hull)) else { continue };
            let short = (share * target - have[hull as usize]).min(budget);
            if short >= value(class) && best.is_none_or(|(_, most)| short > most) && fits(class.build_cost(), self.cap()) {
                best = Some((class, short));
            }
        }
        best.map(|(class, short)| (class, (short / value(class)).floor().min(MAX_SHIP_BATCH) as u16))
    }

    /// A player about to leave has few queue places for hours of income:
    /// each place should absorb its share of what will come in.
    fn buildings(&mut self, away_for: Option<u64>, orders: &mut Vec<Order>) {
        let slots = self.home.build_slots as usize;
        let room = self.room(self.home.build_queue.len(), self.home.build_pending.len(), slots, away_for.is_some());
        let mut budget = self.building_budget(away_for);
        for left in (1..=room).rev() {
            let Some(building) = self.next_building(ABSORB * budget / left as f32) else { break };
            let level = self.plan.buildings.get(building) + 1;
            let cost = scaling::building_cost(building, level);
            self.plan.buildings.set(building, level);
            self.spare = self.spare.sub(cost);
            budget = (budget - scaling::resource_weight(&cost)).max(0.0);
            orders.push(Order::Command(Command::Build { building_type: building, reserve: true }));
        }
    }

    /// What the building queue has to absorb while the player is away: the
    /// stock no order has claimed and the income to come.
    fn building_budget(&self, away_for: Option<u64>) -> f32 {
        let stock = Resources {
            metal: self.spare.metal.max(0.0),
            crystal: self.spare.crystal.max(0.0),
            deuterium: self.spare.deuterium.max(0.0),
        };
        let production = self.home.production;
        let income = Resources { deuterium: production.deuterium.max(0.0), ..production };
        away_for.map_or(0.0, |ticks| scaling::resource_weight(&stock) + scaling::resource_weight(&income) * ticks as f32)
    }

    /// First of: a store under pressure, what a planned tech or ship waits
    /// on, support buildings that are due, the best mine. `place_worth` is
    /// what one queue place should absorb (see `mines`). Whatever is chosen,
    /// the Solar Plant goes before a mine that would run short of energy,
    /// the mine an order starves for before the order, and the store a cost
    /// does not fit before the cost.
    fn next_building(&self, place_worth: f32) -> Option<BuildingType> {
        let cap = self.cap();
        let stock = self.state.player.resources;
        let planned = |b: BuildingType| self.plan.buildings.get(b) > self.built.get(b);

        let mut wanted: Vec<BuildingType> = ResourceKind::ALL
            .into_iter()
            .filter(|&k| stock.get(k) >= STORAGE_PRESSURE * cap.get(k))
            .map(scaling::storage_building)
            .filter(|&store| !planned(store))
            .collect();
        if self.short_of_power(&self.plan.buildings) {
            wanted.push(BuildingType::SolarPlant);
        }
        wanted.extend(self.needs.iter().filter(|&&(b, level)| self.plan.buildings.get(b) < level).map(|&(b, _)| self.unlocks(b)));
        wanted.extend(self.support());
        wanted.extend(self.mines(place_worth));

        for building in wanted {
            let level = self.plan.buildings.get(building) + 1;
            if level > scaling::MAX_BUILDING_LEVEL
                || !scaling::building_prerequisites_met(building, &self.plan.buildings, &self.plan.research)
            {
                continue;
            }
            if let Some(plant) = self.power_for(building) {
                return Some(plant);
            }
            let cost = scaling::building_cost(building, level);
            if fits(cost, cap) {
                return Some(self.bottleneck(cost).unwrap_or(building));
            }
            let store = ResourceKind::ALL
                .into_iter()
                .filter(|&k| cost.get(k) > cap.get(k))
                .map(scaling::storage_building)
                .find(|&store| !planned(store));
            if let Some(store) = store
                && fits(scaling::building_cost(store, self.plan.buildings.get(store) + 1), cap)
            {
                return Some(store);
            }
        }
        None
    }

    fn short_of_power(&self, buildings: &BuildingLevels) -> bool {
        let power = self.planned_power(buildings);
        power.needed > power.made
    }

    /// The Solar Plant, when the next level of `building` would leave the
    /// mines short of energy and the plant can still rise and be paid for.
    fn power_for(&self, building: BuildingType) -> Option<BuildingType> {
        let plant = BuildingType::SolarPlant;
        let level = self.plan.buildings.get(plant) + 1;
        let mut raised = self.plan.buildings.clone();
        raised.set(building, raised.get(building) + 1);
        (building != plant
            && level <= scaling::MAX_BUILDING_LEVEL
            && self.short_of_power(&raised)
            && fits(scaling::building_cost(plant, level), self.cap()))
        .then_some(plant)
    }

    /// Ticks until the planned mines have produced what `cost` lacks, per resource.
    fn wait(&self, cost: Resources, kind: ResourceKind) -> f64 {
        let short = cost.get(kind) - self.spare.get(kind);
        let rate = scaling::production_per_tick(&self.plan.buildings, 1.0, &self.pace).get(kind);
        if cost.get(kind) <= 0.0 || short <= 0.0 {
            0.0
        } else if rate <= 0.0 {
            f64::INFINITY
        } else {
            short / rate
        }
    }

    /// The mine to raise first when an order would wait on one resource for
    /// more than twice as long as that mine takes to afford. Orders hold
    /// their place in the queue, so one that nothing produces would block it.
    fn bottleneck(&self, cost: Resources) -> Option<BuildingType> {
        let (mine, kind) = MINES.into_iter().max_by(|a, b| self.wait(cost, a.1).total_cmp(&self.wait(cost, b.1)))?;
        let level = self.plan.buildings.get(mine) + 1;
        let mine_cost = scaling::building_cost(mine, level);
        let mine_wait = ResourceKind::ALL.into_iter().map(|k| self.wait(mine_cost, k)).fold(0.0, f64::max);
        let worth_it = level <= scaling::MAX_BUILDING_LEVEL && fits(mine_cost, self.cap()) && 2.0 * mine_wait < self.wait(cost, kind);
        worth_it.then(|| self.power_for(mine).unwrap_or(mine))
    }

    /// `building`, or what must be built first for it to be allowed.
    fn unlocks(&self, building: BuildingType) -> BuildingType {
        let missing = scaling::building_prerequisites(building).iter().find_map(|&(p, level)| match p {
            PrereqRef::Building(other) if self.plan.buildings.get(other) < level => Some(other),
            _ => None,
        });
        missing.map_or(building, |other| self.unlocks(other))
    }

    /// Support buildings the economy has grown into, cheapest first.
    fn support(&self) -> Vec<BuildingType> {
        let yardstick = self.yardstick();
        let mut due: Vec<(f32, BuildingType)> = SUPPORT
            .into_iter()
            .filter_map(|b| {
                let cost = scaling::resource_weight(&scaling::building_cost(b, self.plan.buildings.get(b) + 1));
                (cost <= self.profile.appetite(b) * yardstick).then_some((cost, b))
            })
            .collect();
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        due.into_iter().map(|(_, b)| b).collect()
    }

    /// The three mines, best return on cost first. A resource that is a
    /// smaller share of the stockpile (less what is already promised, each
    /// resource at its weight) than the others counts for more. A level that
    /// costs less than `place_worth` is rated as if it cost that much: with
    /// queue places scarcer than resources, the bigger gain wins, not the
    /// cheaper level. The bigger gain is always the mine that is ahead, and
    /// only the shares keep it from running away from the other two. A
    /// level's cost counts the Solar Plant it takes to power it.
    fn mines(&self, place_worth: f32) -> Vec<BuildingType> {
        let held = |k: ResourceKind| self.spare.get(k) * k.weight();
        let all = ResourceKind::ALL.into_iter().map(|k| held(k).abs()).sum::<f64>().max(1.0);
        let fill = |k: ResourceKind| held(k) / all;
        let mean_fill = ResourceKind::ALL.into_iter().map(fill).sum::<f64>() / 3.0;
        let energy_price = self.energy_price();

        let mut ranked: Vec<(f64, BuildingType)> = MINES
            .into_iter()
            .map(|(mine, kind)| {
                let level = self.plan.buildings.get(mine);
                let gain = self.pace.rate(
                    (scaling::mine_output_per_hour(mine, level + 1) - scaling::mine_output_per_hour(mine, level)) / 3600.0,
                );
                let energy = scaling::energy_needed_by(mine, level + 1) - scaling::energy_needed_by(mine, level);
                let cost = scaling::resource_weight(&scaling::building_cost(mine, level + 1)) + energy as f32 * energy_price;
                let scarcity = (1.0 + SCARCITY_PULL * (mean_fill - fill(kind))).clamp(0.25, 3.0);
                (gain * kind.weight() * scarcity / f64::from(cost.max(place_worth)), mine)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        ranked.into_iter().map(|(_, mine)| mine).collect()
    }

    /// Points one unit of energy costs at the Solar Plant's next level.
    fn energy_price(&self) -> f32 {
        let level = self.plan.buildings.solar_plant.min(scaling::MAX_BUILDING_LEVEL - 1);
        let gain = scaling::solar_plant_energy(level + 1) - scaling::solar_plant_energy(level);
        scaling::resource_weight(&scaling::building_cost(BuildingType::SolarPlant, level + 1)) / gain.max(1) as f32
    }
}
