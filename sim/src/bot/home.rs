// The homeworld planner every bot shares: what to queue next.
//
// One yardstick sizes everything: the cost of the next Metal Mine level. A
// support building or a tech is ordered once it costs no more than its
// appetite times that yardstick, so the ladder rises with the economy
// instead of following a fixed script. Mines are picked by payback, with a
// resource that sits low against its cap counting for more. Fleet value is
// held at a share of the empire's building and research points, and home
// defence at a margin over the next raid.
//
// Building and research orders are sent with `reserve`, so a queue runs in
// the order planned. A player who stays queues one order at a time; one
// about to leave fills every queue, gives each building place to an order
// worth its share of the income to come, and spends what no order has
// claimed.
//
// A shipyard batch is paid for whole when it is ordered, so the planner cuts
// a batch to the units the stockpile covers now and orders the rest at a
// later look.

use iac_shared::constants::{ResourceKind, Resources, ShipClass};
use iac_shared::pace::Pace;
use iac_shared::protocol::{Command, GameState, HomeworldState};
use iac_shared::scaling::{
    self, BuildingLevels, BuildingType, DefenceKind, ResearchLevels, ResearchPrereqKind, ResearchType, ShipyardItem,
};

use super::Personality;
use crate::script::Order;

/// What a ship is for when sizing the fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hull {
    Scout,
    Warship,
    Hauler,
}

impl Hull {
    pub(super) fn of(class: ShipClass) -> Hull {
        match class {
            ShipClass::Scout => Hull::Scout,
            ShipClass::Hauler => Hull::Hauler,
            ShipClass::Corvette | ShipClass::Frigate | ShipClass::Cruiser => Hull::Warship,
        }
    }

    /// The class to order: the heaviest warship the empire can build.
    fn class(self, research: &ResearchLevels) -> Option<ShipClass> {
        let options: &[ShipClass] = match self {
            Hull::Scout => &[ShipClass::Scout],
            Hull::Hauler => &[ShipClass::Hauler],
            Hull::Warship => &[ShipClass::Cruiser, ShipClass::Frigate, ShipClass::Corvette],
        };
        options.iter().copied().find(|&c| scaling::ship_class_unlocked(c, research))
    }
}

struct Profile {
    depot: f32,
    sensor: f32,
    grid: f32,
    tech: f32,
    research: &'static [(ResearchType, u8)],
    /// Fleet value held, as a share of building and research points.
    fleet_share: f32,
    hulls: &'static [(Hull, f32)],
    /// Structure power held, as a multiple of the next raid's expected power.
    defence_margin: f32,
}

const MODULAR_FABRICATION_APPETITE: f32 = 2.0;
/// Techs outside the personality's plan, bought as the economy outgrows them.
const SPARE_TECH_APPETITE: f32 = 0.25;
const VAULT_PRESSURE: f32 = 0.75;
/// Share of the income expected while away that the building queue should hold.
const ABSORB: f32 = 0.6;
/// How strongly a lopsided stockpile tilts the choice of mine.
const SCARCITY_PULL: f32 = 4.0;
const MAX_SHIP_BATCH: f32 = 12.0;
const MAX_DEFENCE_BATCH: f32 = 100.0;
const MAX_SURPLUS_BATCH: u16 = 500;

use ResearchType as T;

const TURTLE: Profile = Profile {
    depot: 0.0,
    sensor: 0.0,
    grid: 0.6,
    tech: 1.0,
    research: &[(T::ModularFabrication, 2)],
    fleet_share: 0.0,
    hulls: &[],
    defence_margin: 1.5,
};

const MINER: Profile = Profile {
    depot: 0.5,
    sensor: 0.5,
    grid: 0.3,
    tech: 1.0,
    research: &[
        (T::HaulerTech, 1),
        (T::HarvestingEfficiency, 1),
        (T::ModularFabrication, 1),
        (T::HarvestingEfficiency, 2),
        (T::FuelEfficiency, 1),
        (T::ModularFabrication, 2),
        (T::HarvestingEfficiency, 3),
        (T::ExtendedFuelTanks, 1),
        (T::Navigation, 1),
        (T::HarvestingEfficiency, 5),
        (T::FuelEfficiency, 3),
        (T::ExtendedFuelTanks, 3),
    ],
    fleet_share: 0.05,
    hulls: &[(Hull::Hauler, 1.0)],
    defence_margin: 1.15,
};

const RAIDER: Profile = Profile {
    depot: 1.5,
    sensor: 0.5,
    grid: 0.3,
    tech: 1.5,
    research: &[
        (T::CorvetteTech, 1),
        (T::WeaponsResearch, 1),
        (T::ModularFabrication, 1),
        (T::ReinforcedHulls, 1),
        (T::FuelEfficiency, 1),
        (T::FrigateTech, 1),
        (T::AdvancedShields, 1),
        (T::ModularFabrication, 2),
        (T::ExtendedFuelTanks, 1),
        (T::HaulerTech, 1),
        (T::WeaponsResearch, 2),
        (T::ReinforcedHulls, 2),
        (T::AdvancedShields, 2),
        (T::FuelEfficiency, 2),
        (T::CruiserTech, 1),
        (T::ExtendedFuelTanks, 2),
        (T::WeaponsResearch, 3),
        (T::FuelEfficiency, 3),
        (T::ExtendedFuelTanks, 3),
        (T::ReinforcedHulls, 3),
        (T::AdvancedShields, 3),
        (T::WeaponsResearch, 5),
        (T::ReinforcedHulls, 5),
        (T::AdvancedShields, 5),
        (T::FuelEfficiency, 5),
        (T::ExtendedFuelTanks, 5),
    ],
    fleet_share: 0.20,
    hulls: &[(Hull::Warship, 0.85), (Hull::Hauler, 0.15)],
    defence_margin: 1.15,
};

const EXPLORER: Profile = Profile {
    depot: 1.5,
    sensor: 1.0,
    grid: 0.3,
    tech: 1.5,
    research: &[
        (T::Navigation, 1),
        (T::CorvetteTech, 1),
        (T::FuelEfficiency, 1),
        (T::ModularFabrication, 1),
        (T::ExtendedFuelTanks, 1),
        (T::HaulerTech, 1),
        (T::FuelEfficiency, 2),
        (T::ExtendedFuelTanks, 2),
        (T::ModularFabrication, 2),
        (T::FrigateTech, 1),
        (T::ReinforcedHulls, 2),
        (T::AdvancedShields, 2),
        (T::FuelEfficiency, 3),
        (T::ExtendedFuelTanks, 3),
        (T::CruiserTech, 1),
        (T::WeaponsResearch, 2),
        (T::FuelEfficiency, 5),
        (T::ExtendedFuelTanks, 5),
        (T::Navigation, 3),
    ],
    fleet_share: 0.10,
    hulls: &[(Hull::Scout, 0.35), (Hull::Warship, 0.45), (Hull::Hauler, 0.20)],
    defence_margin: 1.15,
};

const MIXED: Profile = Profile {
    depot: 1.5,
    sensor: 0.5,
    grid: 0.3,
    tech: 1.5,
    research: &[
        (T::CorvetteTech, 1),
        (T::Navigation, 1),
        (T::ModularFabrication, 1),
        (T::FuelEfficiency, 1),
        (T::HaulerTech, 1),
        (T::WeaponsResearch, 1),
        (T::FrigateTech, 1),
        (T::ModularFabrication, 2),
        (T::ExtendedFuelTanks, 1),
        (T::ReinforcedHulls, 1),
        (T::AdvancedShields, 1),
        (T::FuelEfficiency, 2),
        (T::CruiserTech, 1),
        (T::ExtendedFuelTanks, 2),
        (T::WeaponsResearch, 2),
        (T::ReinforcedHulls, 2),
        (T::AdvancedShields, 2),
        (T::FuelEfficiency, 3),
        (T::ExtendedFuelTanks, 3),
        (T::WeaponsResearch, 3),
        (T::FuelEfficiency, 5),
        (T::ExtendedFuelTanks, 5),
    ],
    fleet_share: 0.15,
    hulls: &[(Hull::Scout, 0.15), (Hull::Warship, 0.65), (Hull::Hauler, 0.20)],
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
            BuildingType::Shipyard | BuildingType::ResearchLab => 1.0,
            BuildingType::Fabricator => 1.5,
            BuildingType::FuelDepot => self.depot,
            BuildingType::SensorArray => self.sensor,
            BuildingType::DefenseGrid => self.grid,
            BuildingType::MetalMine
            | BuildingType::CrystalMine
            | BuildingType::DeuteriumSynthesizer
            | BuildingType::StorageVault => 0.0,
        }
    }
}

const MINES: [(BuildingType, ResourceKind); 3] = [
    (BuildingType::MetalMine, ResourceKind::Metal),
    (BuildingType::CrystalMine, ResourceKind::Crystal),
    (BuildingType::DeuteriumSynthesizer, ResourceKind::Deuterium),
];

const SUPPORT: [BuildingType; 6] = [
    BuildingType::Shipyard,
    BuildingType::ResearchLab,
    BuildingType::Fabricator,
    BuildingType::FuelDepot,
    BuildingType::SensorArray,
    BuildingType::DefenseGrid,
];

fn weight_of(kind: ResourceKind) -> f32 {
    let mut one = Resources::default();
    *one.get_mut(kind) = 1000.0;
    scaling::resource_weight(&one)
}

fn fits(cost: Resources, cap: Resources) -> bool {
    ResourceKind::ALL.into_iter().all(|k| cost.get(k) <= cap.get(k))
}

/// The empire as it will stand once everything queued has finished.
struct Projection {
    buildings: BuildingLevels,
    research: ResearchLevels,
}

impl Projection {
    fn of(home: &HomeworldState) -> Projection {
        let mut buildings = BuildingLevels::default();
        for b in &home.buildings {
            buildings.set(b.building_type, b.level);
        }
        let queued = home.build_queue.iter().map(|q| (q.building_type, q.target_level));
        let waiting = home.build_pending.iter().map(|q| (q.building_type, q.target_level));
        for (b, level) in queued.chain(waiting) {
            buildings.set(b, buildings.get(b).max(level));
        }

        let mut research = ResearchLevels::default();
        for r in &home.research {
            research.set(r.tech, r.level);
        }
        let active = home.research_active.iter().map(|q| (q.tech, q.target_level));
        let waiting = home.research_pending.iter().map(|q| (q.tech, q.target_level));
        for (t, level) in active.chain(waiting) {
            research.set(t, research.get(t).max(level));
        }
        Projection { buildings, research }
    }
}

struct Planner<'a> {
    state: &'a GameState,
    home: &'a HomeworldState,
    profile: &'static Profile,
    pace: Pace,
    built: BuildingLevels,
    plan: Projection,
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
    let Ok(pace) = Pace::new(state.world.pace) else { return Resources::default() };
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
    let mut planner = Planner {
        state,
        home,
        profile: Profile::of(personality),
        pace,
        built,
        plan: Projection::of(home),
        stock: state.player.resources,
        spare: state.player.resources.sub(waiting),
        yard_room: 0,
        needs: Vec::new(),
        yard_saving: Resources::default(),
    };
    planner.research(away_for.is_some(), orders);
    planner.shipyard(orders);
    planner.buildings(away_for, orders);
    if away_for.is_some() {
        planner.surplus(orders);
    }
    planner.yard_saving
}

impl Planner<'_> {
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

    fn next_tech(&mut self) -> Option<ResearchType> {
        let yardstick = self.yardstick();
        let profile = self.profile;
        let planned = profile.research.iter().map(|&(tech, level)| (tech, level, profile.tech));
        let rest = T::ALL.into_iter().map(|tech| (tech, scaling::research_max_level(tech), SPARE_TECH_APPETITE));
        for (tech, goal, appetite) in planned.chain(rest) {
            let level = self.plan.research.get(tech) + 1;
            if level > goal.min(scaling::research_max_level(tech)) {
                continue;
            }
            let appetite = if tech == T::ModularFabrication { appetite.max(MODULAR_FABRICATION_APPETITE) } else { appetite };
            let cost = scaling::research_cost(tech, level);
            if scaling::resource_weight(&cost) > appetite * yardstick {
                continue;
            }
            if self.tech_ready(tech) && fits(cost, self.cap()) {
                return Some(tech);
            }
        }
        None
    }

    /// Whether `tech` can be queued now; what it lacks in buildings is noted.
    fn tech_ready(&mut self, tech: ResearchType) -> bool {
        let mut ready = self.plan.buildings.research_lab >= 1;
        self.need(BuildingType::ResearchLab, 1);
        for prereq in scaling::research_prerequisites(tech).into_iter().flatten() {
            match prereq {
                ResearchPrereqKind::Building(b) => {
                    ready &= self.plan.buildings.get(b.building) >= b.level;
                    self.need(b.building, b.level);
                }
                ResearchPrereqKind::Research { tech, level } => ready &= self.plan.research.get(tech) >= level,
            }
        }
        ready
    }

    fn shipyard(&mut self, orders: &mut Vec<Order>) {
        let held = usize::from(self.home.shipyard_queue.is_some());
        if self.profile.fleet_share > 0.0 {
            self.need(BuildingType::Shipyard, 1);
        }
        if self.plan.buildings.shipyard >= 2 {
            self.need(BuildingType::DefenseGrid, 1);
        }
        if self.plan.buildings.shipyard == 0 {
            return;
        }
        self.yard_room = self.room(held, self.home.shipyard_pending.len(), 1, true);
        // A batch waiting for the yard is already paid for: the plan orders
        // more only once the line behind the running batch is clear, so it
        // does not pay far ahead of what the yard can build.
        if !self.home.shipyard_pending.is_empty() {
            return;
        }
        if let Some(count) = self.turrets_wanted() {
            self.wanted(ShipyardItem::Defence(DefenceKind::PulseTurret), count, orders);
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
            .fold(f32::from(count), f32::min) as u16;
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
    /// Pulse Turrets, which cost no deuterium; one that does not, into the
    /// heaviest hull it can build. Only stock no order has claimed is spent,
    /// so no queued order is kept waiting by it.
    fn surplus(&mut self, orders: &mut Vec<Order>) {
        let turret = DefenceKind::PulseTurret;
        let item = if self.profile.fleet_share > 0.0 {
            if !scaling::defence_prerequisites_met(turret, &self.plan.buildings, &self.plan.research) {
                return;
            }
            ShipyardItem::Defence(turret)
        } else {
            match Hull::Warship.class(&self.plan.research).or(Hull::Scout.class(&self.plan.research)) {
                Some(class) => ShipyardItem::Ship(class),
                None => return,
            }
        };
        if self.plan.buildings.shipyard >= 1 && fits(item.unit_cost(), self.cap()) {
            self.batch(item, MAX_SURPLUS_BATCH, self.spare, orders);
        }
    }

    fn ordered(&self) -> impl Iterator<Item = (ShipyardItem, u16)> + '_ {
        let running = self.home.shipyard_queue.iter().map(|q| (q.item, q.count - q.built));
        let waiting = self.home.shipyard_pending.iter().map(|q| (q.item, q.count));
        running.chain(waiting)
    }

    /// Pulse Turrets give the most power for their cost of the three structures.
    fn turrets_wanted(&self) -> Option<u16> {
        let kind = DefenceKind::PulseTurret;
        if !scaling::defence_prerequisites_met(kind, &self.plan.buildings, &self.plan.research) {
            return None;
        }
        let raid = scaling::raid_power_base(scaling::econ_points(&self.plan.buildings, &self.plan.research));
        let grid = self.built.defense_grid;
        let standing: f32 = self.home.defences.iter().map(|d| (d.count + d.restoring) as f32 * d.power).sum();
        let coming: f32 = self
            .ordered()
            .filter_map(|(item, left)| match item {
                ShipyardItem::Defence(d) => Some(f32::from(left) * scaling::structure_power(d, grid)),
                ShipyardItem::Ship(_) => None,
            })
            .sum();
        let short = self.profile.defence_margin * raid - standing - coming;
        let count = (short / scaling::structure_power(kind, grid)).ceil().min(MAX_DEFENCE_BATCH);
        (count >= 1.0 && fits(kind.build_cost(), self.cap())).then_some(count as u16)
    }

    fn ships_wanted(&self) -> Option<(ShipClass, u16)> {
        let mut built_research = ResearchLevels::default();
        for r in &self.home.research {
            built_research.set(r.tech, r.level);
        }
        let target = self.profile.fleet_share * scaling::econ_points(&self.built, &built_research);
        let value = |class: ShipClass| scaling::resource_weight(&class.build_cost());

        let flying = self.state.fleets.iter().flat_map(|f| f.ships.iter()).map(|s| (s.ship_class, 1.0));
        let coming = self.ordered().filter_map(|(item, left)| match item {
            ShipyardItem::Ship(class) => Some((class, f32::from(left))),
            ShipyardItem::Defence(_) => None,
        });
        let mut have = [0.0f32; 3];
        for (class, count) in flying.chain(coming) {
            have[Hull::of(class) as usize] += count * value(class);
        }
        let budget = target - have.iter().sum::<f32>();

        let mut best: Option<(ShipClass, f32)> = None;
        for &(hull, share) in self.profile.hulls {
            let Some(class) = hull.class(&self.plan.research) else { continue };
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
        let stock = Resources {
            metal: self.spare.metal.max(0.0),
            crystal: self.spare.crystal.max(0.0),
            deuterium: self.spare.deuterium.max(0.0),
        };
        let income = |ticks: u64| scaling::resource_weight(&self.home.production) * ticks as f32;
        let mut budget = away_for.map_or(0.0, |ticks| scaling::resource_weight(&stock) + income(ticks));
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

    /// First of: a vault under pressure, what a planned tech or ship waits
    /// on, support buildings that are due, the best mine. `place_worth` is
    /// what one queue place should absorb (see `mines`).
    fn next_building(&self, place_worth: f32) -> Option<BuildingType> {
        let cap = self.cap();
        let vault_planned = self.plan.buildings.storage_vault > self.built.storage_vault;
        let stock = self.state.player.resources;
        let pressed = ResourceKind::ALL.into_iter().any(|k| stock.get(k) >= VAULT_PRESSURE * cap.get(k));

        let mut wanted: Vec<BuildingType> = Vec::new();
        if pressed && !vault_planned {
            wanted.push(BuildingType::StorageVault);
        }
        wanted.extend(self.needs.iter().filter(|&&(b, level)| self.plan.buildings.get(b) < level).map(|&(b, _)| self.unlocks(b)));
        wanted.extend(self.support());
        wanted.extend(self.mines(place_worth));

        for building in wanted {
            let level = self.plan.buildings.get(building) + 1;
            if level > scaling::MAX_BUILDING_LEVEL || !scaling::building_prerequisites_met(building, &self.plan.buildings) {
                continue;
            }
            let cost = scaling::building_cost(building, level);
            if fits(cost, cap) {
                return Some(self.bottleneck(cost).unwrap_or(building));
            }
            let vault = scaling::building_cost(BuildingType::StorageVault, self.plan.buildings.storage_vault + 1);
            if !vault_planned && fits(vault, cap) {
                return Some(BuildingType::StorageVault);
            }
        }
        None
    }

    /// Ticks until the planned mines have produced what `cost` lacks, per resource.
    fn wait(&self, cost: Resources, kind: ResourceKind) -> f32 {
        let short = cost.get(kind) - self.spare.get(kind);
        let mine = MINES.iter().find(|m| m.1 == kind).map_or(BuildingType::MetalMine, |m| m.0);
        let rate = scaling::production_per_tick(mine, self.plan.buildings.get(mine), &self.pace);
        if cost.get(kind) <= 0.0 || short <= 0.0 {
            0.0
        } else if rate <= 0.0 {
            f32::INFINITY
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
        let mine_wait = ResourceKind::ALL.into_iter().map(|k| self.wait(mine_cost, k)).fold(0.0, f32::max);
        let worth_it = level <= scaling::MAX_BUILDING_LEVEL && fits(mine_cost, self.cap()) && 2.0 * mine_wait < self.wait(cost, kind);
        worth_it.then_some(mine)
    }

    /// `building`, or what must be built first for it to be allowed.
    fn unlocks(&self, building: BuildingType) -> BuildingType {
        match scaling::building_prerequisites(building) {
            Some(p) if self.plan.buildings.get(p.building) < p.level => self.unlocks(p.building),
            _ => building,
        }
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

    /// The three mines, best return on cost first. A resource whose stockpile
    /// (less what is already promised) sits lower against its cap than the
    /// others counts for more. A level that costs less than `place_worth` is
    /// rated as if it cost that much: with queue places scarcer than
    /// resources, the bigger gain wins, not the cheaper level.
    fn mines(&self, place_worth: f32) -> Vec<BuildingType> {
        let cap = self.cap();
        let fill = |k: ResourceKind| self.spare.get(k) / cap.get(k);
        let mean_fill = ResourceKind::ALL.into_iter().map(fill).sum::<f32>() / 3.0;

        let mut ranked: Vec<(f32, BuildingType)> = MINES
            .into_iter()
            .map(|(mine, kind)| {
                let level = self.plan.buildings.get(mine);
                let gain = scaling::production_per_tick(mine, level + 1, &self.pace)
                    - scaling::production_per_tick(mine, level, &self.pace);
                let scarcity = (1.0 + SCARCITY_PULL * (mean_fill - fill(kind))).clamp(0.25, 3.0);
                let cost = scaling::resource_weight(&scaling::building_cost(mine, level + 1));
                (gain * weight_of(kind) * scarcity / cost.max(place_worth), mine)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        ranked.into_iter().map(|(_, mine)| mine).collect()
    }
}
