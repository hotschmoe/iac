// The homeworld planner every bot shares: what to queue next.
//
// One yardstick sizes everything: the cost of the next Metal Mine level. A
// support building or a tech is ordered once it costs no more than its
// appetite times that yardstick, so the ladder rises with the economy
// instead of following a fixed script. Mines are picked by payback, with a
// resource that sits low against its cap counting for more, and no mine is
// raised past the energy there is: the cheapest energy goes first, the
// Solar Plant, the Fusion Reactor with the deuterium it burns, or Solar
// Satellites at twice their price for what raids take of them. A store is
// raised before its resource would spill while the player is away. Fleet
// value is held at a share of the empire's building and research points,
// and home defence at a margin over the next raid.
//
// A personality names the research it wants and the ships it flies, from
// lines that run to the heaviest hulls; the next class up a line is worked
// toward once the economy has grown into its price. What a research or a
// ship requires is not written here: the planner reads it from the
// catalogue and orders what is missing, a research before the one that
// needs it and the Shipyard level a class asks for once its research is in.
//
// Building and research orders are sent with `reserve`, so a queue runs in
// the order planned. A player who stays queues one order at a time; one
// about to leave fills every queue and gives each building place to an
// order worth its share of the income to come. Either spends what no order
// has claimed on what scores, since the score counts what is built.
//
// A shipyard batch is paid for whole when it is ordered, so the planner cuts
// a batch to the units the stockpile covers now and orders the rest at a
// later look, and orders none until the yard, building or tech it needs is
// built or under way.

use iac_shared::constants::{ResourceKind, Resources, ShipClass};
use iac_shared::pace::Pace;
use iac_shared::protocol::{Command, GameState, HomeworldState, RaidLabel};
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
const WARSHIPS: &[ShipClass] = &[
    ShipClass::Deathstar,
    ShipClass::Destroyer,
    ShipClass::Battlecruiser,
    ShipClass::Bomber,
    ShipClass::Battleship,
    ShipClass::Cruiser,
    ShipClass::HeavyFighter,
    ShipClass::LightFighter,
];
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
    /// Espionage Probes kept: a scout is a count, not a share of the fleet.
    /// One aboard gives a fleet the long scan; more chart no faster.
    probes: u16,
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
/// The defences that fire, of which a bot builds the one that answers what
/// the next raid flies. The first needs no research.
const ARSENAL: [DefenceKind; 6] = [
    DefenceKind::RocketLauncher,
    DefenceKind::LightLaser,
    DefenceKind::HeavyLaser,
    DefenceKind::IonCannon,
    DefenceKind::GaussCannon,
    DefenceKind::PlasmaTurret,
];
/// The domes, of which a homeworld holds one each: built once one costs no
/// more than this many yardsticks.
const DOMES: [DefenceKind; 2] = [DefenceKind::SmallShieldDome, DefenceKind::LargeShieldDome];
const DOME_APPETITE: f32 = 4.0;
/// The research that opens the defences, wanted by every personality once
/// a level costs no more than this many yardsticks.
const DEFENCE_RESEARCH: &[(ResearchType, u8)] = &[
    (T::EnergyTechnology, 2),
    (T::LaserTechnology, 3),
    (T::EnergyTechnology, 3),
    (T::ShieldingTechnology, 2),
    (T::LaserTechnology, 6),
    (T::IonTechnology, 4),
    (T::WeaponsTechnology, 3),
    (T::EnergyTechnology, 6),
    (T::ShieldingTechnology, 6),
];
const DEFENCE_TECH_APPETITE: f32 = 1.5;
/// While the server's forecast of the next raid is worse than this, the
/// raiders destroyed every time and this share of what stands at home
/// kept, a bot orders more defence than its margin asks: so much of the
/// raid's power at a time, and never past so many times it in all.
const KEEP_AT_HOME: f32 = 0.85;
const TOP_UP: f32 = 0.5;
const MOST_DEFENCE: f32 = 6.0;
/// How much a defence's worth falls as it makes up more of what stands.
const MIX: f32 = 3.0;
/// Game hours at pace 1 of deuterium a Fusion Reactor level is charged for
/// when its energy is priced against the Solar Plant's.
const FUSION_HORIZON_HOURS: f64 = 72.0;
/// The Fusion Reactor stops when the stockpile has no deuterium, and the
/// mines with it: a level is built only while its burn stays under this
/// share of what the synthesizer makes.
const FUSION_SHARE: f64 = 0.2;
/// A Solar Satellite's energy is priced at this many times its cost: a
/// raid destroys satellites first, and not all come back.
const SATELLITE_RISK: f32 = 2.0;

/// Where the next energy comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Plant(BuildingType),
    Satellites,
}

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
    probes: 0,
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
    probes: 0,
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
    probes: 0,
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
    hulls: &[(Hull::Warship, 0.65), (Hull::Hauler, 0.35)],
    probes: 4,
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
    hulls: &[(Hull::Warship, 0.75), (Hull::Hauler, 0.25)],
    probes: 2,
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

    /// Every line the empire builds from: its hulls' and, when it keeps
    /// probes, the scouts'.
    fn lines(&self) -> impl Iterator<Item = &'static [ShipClass]> + '_ {
        let fleet = self.hulls.iter().filter(|_| self.fleet_share > 0.0).map(|&(hull, _)| self.line(hull));
        fleet.chain((self.probes > 0).then_some(SCOUTS))
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
    /// How long the player will be gone after this look; None for one who stays.
    away_for: Option<u64>,
    built: BuildingLevels,
    plan: Projection,
    /// What stands once the orders under way have finished. A batch pays
    /// when it is ordered, so the yard is judged by this and not by `plan`:
    /// a batch whose yard, building or tech still waits for resources would
    /// hold its price for nothing, and take it from the order it waits on.
    started: Projection,
    /// The stockpile less the batches and the research ordered this turn,
    /// which a free lab starts and pays for at once: what a batch can be
    /// paid from.
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
    let Some(mut planner) = Planner::new(state, personality, away_for) else { return Resources::default() };
    planner.research(away_for.is_some(), orders);
    planner.shipyard(orders);
    planner.buildings(away_for, orders);
    planner.surplus(orders);
    planner.yard_saving
}

/// What `plan` would do with one free place in each queue, asked in the
/// order `plan` works. Nothing is ordered.
pub(super) fn outlook(state: &GameState, personality: Personality, away_for: Option<u64>) -> Option<Outlook> {
    let mut planner = Planner::new(state, personality, away_for)?;
    let research = planner.research_want();
    let shipyard = planner.shipyard_want(away_for.is_some());
    let building = planner.building_want(away_for);
    Some(Outlook { building, research, shipyard })
}

impl<'a> Planner<'a> {
    fn new(state: &'a GameState, personality: Personality, away_for: Option<u64>) -> Option<Planner<'a>> {
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
            away_for,
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
        let dome = self.dome_wanted().map(ShipyardItem::Defence);
        let turrets = self.defences_wanted().map(|(kind, _)| ShipyardItem::Defence(kind));
        let ships = self.ships_wanted().map(|(class, _)| ShipyardItem::Ship(class));
        let called_for: Vec<ShipyardItem> = satellites.into_iter().chain(dome).chain(turrets).chain(ships).collect();
        if !called_for.is_empty() {
            return Want::Order { covered: called_for.iter().any(|item| fits(item.unit_cost(), self.stock)) };
        }
        let covered = self.surplus_item(self.spare).is_some_and(|item| fits(item.unit_cost(), self.spare));
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
            let Some(tech) = self.next_tech().or_else(|| self.surplus_tech()) else { break };
            let level = self.plan.research.get(tech) + 1;
            self.plan.research.set(tech, level);
            let cost = scaling::research_cost(tech, level);
            self.spare = self.spare.sub(cost);
            self.stock = self.stock.sub(cost);
            orders.push(Order::Command(Command::Research { tech, reserve: true }));
        }
    }

    /// Every tech level the profile still calls for, in the order it is
    /// considered: the personality's plan, what the next class up each of
    /// its lines needs, the defences' research, then the rest.
    fn tech_ladder(&self) -> Vec<Rung> {
        let yardstick = self.yardstick();
        let profile = self.profile;
        let planned = profile.research.iter().map(|&(tech, level)| (tech, level, profile.tech));
        let hulls: Vec<(ResearchType, u8, f32)> = self
            .next_classes()
            .into_iter()
            .flat_map(|class| scaling::ship_prerequisites(class).iter())
            .filter_map(|&(p, level)| match p {
                PrereqRef::Research(tech) => Some((tech, level, profile.tech)),
                PrereqRef::Building(_) => None,
            })
            .collect();
        let defence = DEFENCE_RESEARCH.iter().map(|&(tech, level)| (tech, level, DEFENCE_TECH_APPETITE));
        let rest = T::ALL.into_iter().map(|tech| (tech, scaling::research_max_level(tech), SPARE_TECH_APPETITE));
        planned
            .chain(hulls)
            .chain(defence)
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

    /// The Shipyard levels the yard's work is waiting on: the first; the
    /// one a defence asks for once its research is in and the economy has
    /// grown into its price; and the same for the next class up of each
    /// line the fleet flies.
    fn yard_needs(&mut self) {
        self.need(BuildingType::Shipyard, 1);
        let yardstick = self.yardstick();
        for kind in ARSENAL.into_iter().chain(DOMES) {
            let prereqs = kind.prerequisites();
            let researched = prereqs.iter().all(|&(p, level)| match p {
                PrereqRef::Research(tech) => self.plan.research.get(tech) >= level,
                PrereqRef::Building(_) => true,
            });
            if !researched || scaling::resource_weight(&kind.build_cost()) > CLASS_APPETITE * yardstick {
                continue;
            }
            for &(p, level) in prereqs {
                if let PrereqRef::Building(building) = p {
                    self.need(building, level);
                }
            }
        }
        for next in self.next_classes() {
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

    /// The next class up each line the empire builds from, once the economy
    /// has grown into its price: what its yard and its research work toward.
    fn next_classes(&self) -> Vec<ShipClass> {
        let yardstick = self.yardstick();
        self.profile
            .lines()
            .filter_map(|line| {
                let have = line.iter().position(|&class| self.plan.unlocked(ShipyardItem::Ship(class))).unwrap_or(line.len());
                have.checked_sub(1).and_then(|i| line.get(i)).copied()
            })
            .filter(|next| scaling::resource_weight(&next.build_cost()) <= CLASS_APPETITE * yardstick)
            .collect()
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
        if let Some(dome) = self.dome_wanted() {
            self.wanted(ShipyardItem::Defence(dome), 1, orders);
        }
        if let Some((kind, count)) = self.defences_wanted() {
            self.wanted(ShipyardItem::Defence(kind), count, orders);
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

    /// What is in stock beyond every order placed scores nothing while it
    /// sits there: the score counts what is built. It goes into the heaviest
    /// warship it pays for, which scores in full and fights, and into the
    /// defence that answers the next raid while it pays for none. Only stock
    /// no order has claimed is spent, so no queued order is kept waiting by
    /// it; a player who stays also keeps the price of the next Metal Mine
    /// level, which it orders one level at a time.
    fn surplus(&mut self, orders: &mut Vec<Order>) {
        while let Some(item) = self.surplus_item(self.purse()) {
            if !self.batch(item, MAX_SURPLUS_BATCH, self.purse(), orders) {
                break;
            }
        }
    }

    /// What no order has claimed, less, for a player who stays, the price of
    /// the next Metal Mine level while there is one.
    fn purse(&self) -> Resources {
        let level = self.plan.buildings.metal_mine + 1;
        match self.away_for {
            None if level <= scaling::MAX_BUILDING_LEVEL => {
                self.spare.sub(scaling::building_cost(BuildingType::MetalMine, level))
            }
            _ => self.spare,
        }
    }

    /// With nothing on the ladder due, the lab takes the cheapest level of
    /// any research it can start that what no order has claimed pays for:
    /// research scores as it is built, and late in a game the stock of one
    /// resource, most often deuterium, outruns everything the yard buys.
    fn surplus_tech(&mut self) -> Option<ResearchType> {
        let purse = self.purse();
        T::ALL
            .into_iter()
            .filter(|&tech| {
                let level = self.plan.research.get(tech) + 1;
                level <= scaling::research_max_level(tech)
                    && scaling::research_energy(tech, level) == 0
                    && self.tech_open(tech)
                    && fits(scaling::research_cost(tech, level), purse)
            })
            .min_by(|&a, &b| {
                let cost = |tech: ResearchType| scaling::resource_weight(&scaling::research_cost(tech, self.plan.research.get(tech) + 1));
                cost(a).total_cmp(&cost(b))
            })
    }

    fn surplus_item(&self, purse: Resources) -> Option<ShipyardItem> {
        let ship = WARSHIPS
            .iter()
            .map(|&class| ShipyardItem::Ship(class))
            .find(|&item| self.started.unlocked(item) && fits(item.unit_cost(), purse));
        let item = ship.or_else(|| self.answer().map(ShipyardItem::Defence))?;
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

    /// What one unit of energy costs from each source the empire can raise
    /// next, in points, cheapest first: the next level of the Solar Plant
    /// and of the Fusion Reactor, the reactor with the deuterium it burns
    /// over `FUSION_HORIZON_HOURS`, and a Solar Satellite, which raids eat,
    /// at `SATELLITE_RISK` times its price.
    fn energy_sources(&self) -> Vec<(f32, Source)> {
        let research = &self.plan.research;
        let mut sources = Vec::new();
        for plant in [BuildingType::SolarPlant, BuildingType::FusionReactor] {
            let level = self.plan.buildings.get(plant);
            if level >= scaling::MAX_BUILDING_LEVEL
                || !scaling::building_prerequisites_met(plant, &self.plan.buildings, research)
                || !fits(scaling::building_cost(plant, level + 1), self.cap())
            {
                continue;
            }
            if plant == BuildingType::FusionReactor {
                let burn = self.pace.rate(f64::from(scaling::fusion_burn_per_hour(level + 1)) / 3600.0);
                let made = scaling::production_per_tick(&self.plan.buildings, 1.0, &self.pace).deuterium;
                if burn > FUSION_SHARE * made {
                    continue;
                }
            }
            let cost = scaling::resource_weight(&scaling::building_cost(plant, level + 1));
            let (gain, burn) = match plant {
                BuildingType::FusionReactor => {
                    let energy = |l| scaling::fusion_energy(l, research.energy_technology);
                    let burn = scaling::fusion_burn_per_hour(level + 1) - scaling::fusion_burn_per_hour(level);
                    let deuterium = Resources { deuterium: f64::from(burn) * FUSION_HORIZON_HOURS, ..Resources::default() };
                    (energy(level + 1).saturating_sub(energy(level)), scaling::resource_weight(&deuterium))
                }
                _ => (scaling::solar_plant_energy(level + 1) - scaling::solar_plant_energy(level), 0.0),
            };
            if gain > 0 {
                sources.push(((cost + burn) / gain as f32, Source::Plant(plant)));
            }
        }
        let satellite = ShipyardItem::Ship(ShipClass::SolarSatellite);
        if self.started.unlocked(satellite) && fits(satellite.unit_cost(), self.cap()) {
            let price = scaling::resource_weight(&satellite.unit_cost()) / scaling::satellite_energy() as f32;
            sources.push((SATELLITE_RISK * price, Source::Satellites));
        }
        sources.sort_by(|a, b| a.0.total_cmp(&b.0));
        sources
    }

    /// Solar Satellites, once they are the cheapest energy there is and the
    /// mines planned are short of it.
    fn satellites_wanted(&self) -> Option<u16> {
        let item = ShipyardItem::Ship(ShipClass::SolarSatellite);
        if self.energy_sources().first().map(|s| s.1) != Some(Source::Satellites) {
            return None;
        }
        let power = self.planned_power(&self.plan.buildings);
        let short = f64::from(power.needed.saturating_sub(power.made));
        let count = (short / f64::from(scaling::satellite_energy())).ceil().min(MAX_SATELLITE_BATCH);
        (count >= 1.0 && fits(item.unit_cost(), self.cap())).then_some(count as u16)
    }

    /// Power of one `kind` standing at home, with the research it has.
    fn power_of(&self, kind: DefenceKind) -> f32 {
        self.home.defences.iter().find(|d| d.kind == kind).map_or(kind.power(), |d| d.power)
    }

    /// The defence to build against the raid the server forecasts: of those
    /// the yard can build, the one that buys the most power for its cost
    /// once the rapid fire of what the raid flies against it is counted.
    /// Against Light Fighters alone that is the cheapest; a wall of it
    /// brings the class that shreds it, and the answer moves on. A kind is
    /// worth less the more of the standing defence it already is (`MIX`):
    /// a wall of one kind is what one class's rapid fire eats.
    fn answer(&self) -> Option<DefenceKind> {
        let raid = &self.home.raid.ships;
        let flown: f32 = raid.iter().map(|s| s.count as f32 * scaling::ship_power(s.ship_class)).sum();
        let exposure = |kind: DefenceKind| {
            if flown <= 0.0 {
                return 1.0;
            }
            let shots: f32 = raid
                .iter()
                .map(|s| {
                    let again = scaling::rapid_fire_against(s.ship_class, ShipyardItem::Defence(kind)).max(1);
                    s.count as f32 * scaling::ship_power(s.ship_class) * f32::from(again)
                })
                .sum();
            shots / flown
        };
        let standing: f32 = self.home.defences.iter().map(|d| d.count as f32 * d.power).sum();
        let share = |kind: DefenceKind| {
            let of_kind = self.home.defences.iter().find(|d| d.kind == kind).map_or(0.0, |d| d.count as f32 * d.power);
            if standing > 0.0 { of_kind / standing } else { 0.0 }
        };
        let worth = |kind: DefenceKind| {
            self.power_of(kind) / scaling::resource_weight(&kind.build_cost()) / exposure(kind) / (1.0 + MIX * share(kind))
        };
        ARSENAL
            .into_iter()
            .filter(|&kind| self.started.unlocked(ShipyardItem::Defence(kind)) && fits(kind.build_cost(), self.cap()))
            .max_by(|&a, &b| worth(a).total_cmp(&worth(b)))
    }

    /// A dome the yard can build, the economy has grown into and the
    /// homeworld does not have, standing or on order.
    fn dome_wanted(&self) -> Option<DefenceKind> {
        let yardstick = self.yardstick();
        DOMES.into_iter().find(|&dome| {
            let item = ShipyardItem::Defence(dome);
            let have = self.home.defences.iter().find(|d| d.kind == dome).map_or(0, |d| d.count);
            let coming = self.ordered().any(|(ordered, _)| ordered == item);
            have == 0
                && !coming
                && self.started.unlocked(item)
                && fits(dome.build_cost(), self.cap())
                && scaling::resource_weight(&dome.build_cost()) <= DOME_APPETITE * yardstick
        })
    }

    /// The defence the next raid calls for and how many of it: enough to
    /// hold the profile's margin over the raid's power, and, while the
    /// server's forecast of the raid against what stands at home is worse
    /// than a raid destroyed with little lost, more of it at each look.
    /// The power of the next raid: the server's forecast, or what the
    /// planned empire draws when that is more.
    fn raid_power(&self) -> f32 {
        let econ = scaling::econ_points(&self.plan.buildings, &self.plan.research);
        self.home.raid.power.max(scaling::raid_power_base(econ))
    }

    fn standing_defence(&self) -> f32 {
        self.home.defences.iter().map(|d| d.count as f32 * d.power).sum()
    }

    fn defences_wanted(&self) -> Option<(DefenceKind, u16)> {
        let kind = self.answer()?;
        let forecast = &self.home.raid;
        let raid = self.raid_power();
        let standing = self.standing_defence();
        let coming: f32 = self
            .ordered()
            .filter_map(|(item, left)| match item {
                ShipyardItem::Defence(d) => Some(f32::from(left) * self.power_of(d)),
                ShipyardItem::Ship(_) => None,
            })
            .sum();
        let mut short = self.profile.defence_margin * raid - standing - coming;
        let holds = forecast.odds.label == RaidLabel::Repelled && forecast.odds.kept >= KEEP_AT_HOME;
        if short <= 0.0 && !holds && coming <= 0.0 && standing < MOST_DEFENCE * raid {
            short = TOP_UP * raid;
        }
        let count = (short / self.power_of(kind)).ceil().min(MAX_DEFENCE_BATCH);
        (count >= 1.0).then_some((kind, count as u16))
    }

    /// The Espionage Probes the profile keeps that are neither flying nor on
    /// order: probes lost are replaced, and no more.
    fn probes_wanted(&self) -> Option<(ShipClass, u16)> {
        let probe = ShipClass::EspionageProbe;
        if self.profile.probes == 0 || !self.started.unlocked(ShipyardItem::Ship(probe)) {
            return None;
        }
        let flying: u32 = self.state.fleets.iter().flat_map(|f| f.ships.iter()).filter(|s| s.ship_class == probe).map(|s| s.count).sum();
        let coming: u32 = self.ordered().filter(|(item, _)| *item == ShipyardItem::Ship(probe)).map(|(_, left)| u32::from(left)).sum();
        let missing = u32::from(self.profile.probes).saturating_sub(flying + coming);
        (missing > 0).then_some((probe, missing as u16))
    }

    fn ships_wanted(&self) -> Option<(ShipClass, u16)> {
        if let Some(probes) = self.probes_wanted() {
            return Some(probes);
        }
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

    /// First of: a store that will be under pressure before the player looks
    /// again, what a planned tech or ship waits on, support buildings that
    /// are due, the best mine. `place_worth` is what one queue place should
    /// absorb (see `mines`). Whatever is chosen, the cheapest plant goes
    /// before a mine that would run short of energy,
    /// the mine an order starves for before the order, and the store a cost
    /// does not fit before the cost.
    fn next_building(&self, place_worth: f32) -> Option<BuildingType> {
        let cap = self.cap();
        let stock = self.state.player.resources;
        let planned = |b: BuildingType| self.plan.buildings.get(b) > self.built.get(b);

        let coming = self.home.production.scale(self.away_for.unwrap_or(0) as f64);
        let mut wanted: Vec<BuildingType> = ResourceKind::ALL
            .into_iter()
            .filter(|&k| stock.get(k) + coming.get(k).max(0.0) >= STORAGE_PRESSURE * cap.get(k))
            .map(scaling::storage_building)
            .filter(|&store| !planned(store))
            .collect();
        if self.short_of_power(&self.plan.buildings) {
            wanted.extend(self.cheapest_plant());
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

    /// The plant that makes energy cheapest, while it is a building: the
    /// Solar Satellites the yard orders instead go beside the building.
    fn cheapest_plant(&self) -> Option<BuildingType> {
        match self.energy_sources().first()?.1 {
            Source::Plant(plant) => Some(plant),
            Source::Satellites => None,
        }
    }

    /// The cheapest plant, when the next level of `building` would leave the
    /// mines short of energy.
    fn power_for(&self, building: BuildingType) -> Option<BuildingType> {
        if matches!(building, BuildingType::SolarPlant | BuildingType::FusionReactor) {
            return None;
        }
        let mut raised = self.plan.buildings.clone();
        raised.set(building, raised.get(building) + 1);
        if self.short_of_power(&raised) { self.cheapest_plant() } else { None }
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

    /// Points one unit of energy costs from the cheapest source.
    fn energy_price(&self) -> f32 {
        self.energy_sources().first().map_or(0.0, |s| s.0)
    }
}
