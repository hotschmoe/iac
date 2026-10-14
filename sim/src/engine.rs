// Game engine: tick loop, movement, combat, harvesting, NPC behavior, homeworlds, build queues.
// Ported from Zig engine.zig.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

use log::info;
use serde::{Deserialize, Serialize};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use iac_shared::hex::Hex;
use iac_shared::keymap::{KeyMap, KeySet};
use iac_shared::constants::{
    Resources, ResourceKind, ShipClass, Density,
    STARTING_RESOURCES, STARTING_SHIPS, MAX_FLEETS_PER_PLAYER, MAX_FLEETS_TOTAL, MAX_FLEET_SHIPS,
    STRANDED_RECOVERY_TICKS, HARVEST_COOLDOWN, EXPLODE_BELOW_PCT, STANDOFF_ROUNDS,
    REPAIR_COST_SHARE_HOME, REPAIR_COST_SHARE_OUTPOST, REPAIR_TICKS,
    RECALL_FUEL_MULTIPLIER, RECALL_DAMAGE_CHANCE_PER_HEX, RECALL_DAMAGE_CHANCE_CAP,
    RECALL_HULL_DAMAGE_MIN, RECALL_HULL_DAMAGE_MAX,
    FUEL_DEUT_PER_UNIT, FLEE_MIN_ROUNDS, FLEE_FUEL_MULTIPLIER, CHASE_MIN_RATIO,
    SALVAGE_DESPAWN_TICKS, HOME_SALVAGE_WARN_TICKS, DERELICT_RESPAWN_TICKS, HARVEST_REPORT_TICKS,
    TEMPLATE_NPC_ID_BASE,
    MOVE_BASE_COOLDOWN,
    SCAN_COOLDOWN, SCAN_BASE_RANGE, SCAN_PROBE_RANGE, SCAN_REVEAL_TICKS,
    RAID_ROLL_INTERVAL, RAID_ROLL_CHANCE, RAID_PITY_STEP, RAID_MIN_INTERVAL,
    RAID_MIN_PLAYER_AGE, RAID_MIN_SHIPYARD, RAID_POWER_ROLL_MIN, RAID_POWER_ROLL_MAX,
    RAID_LOSS_CAP_METAL, RAID_LOSS_CAP_CRYSTAL, RAID_LOSS_CAP_DEUT,
    RAID_DEFENSE_SALVAGE_FRACTION, RAID_SUPPRESS_AFTER_LOSS, RAID_ROUNDS, ODDS_ROUNDS,
    DEFENCE_REBUILD_PCT, DEFENCE_REBUILD_SPREAD, DEFENCE_REBUILD_EACH_BELOW,
    EXPLORE_DURATION_TICKS, EXPLORE_PROBE_AMBUSH_REDUCTION, EXPLORE_RECYCLER_LOOT_MULTIPLIER,
    EXPLORE_RETRY_AMBUSH_BUMP, POLICY_EVAL_INTERVAL, POLICY_FUEL_PAYBACK, POLICY_FUEL_UNBOUNDED_PCT,
    POLICY_MIN_SEAM_UNITS, POLICY_SEARCH_HOPS, POLICY_HULL_READY,
    POLICY_MIN_FUEL_PCT_MAX, POLICY_CARGO_RETURN_PCT_MIN, POLICY_CARGO_RETURN_PCT_MAX,
    POLICY_RANGE_MIN, POLICY_RANGE_MAX, POLICY_ENGAGE_RATIO_X10_MIN, POLICY_ENGAGE_RATIO_X10_MAX,
    POLICY_DISENGAGE_RATIO_X10_MIN, POLICY_DISENGAGE_RATIO_X10_MAX,
    LANE_SEARCH_HOPS,
    OUTPOST_MIN_RING, OUTPOST_MAX_RING, OUTPOST_MAX_LEVEL, OUTPOST_RAID_LOSS, OUTPOST_SPACING,
};
use iac_shared::pace::Pace;
use iac_shared::world::{self, HubService, WorldGen};
use iac_shared::scaling::{
    self, Arms, BuildingType, ResearchType, BuildingLevels, ResearchLevels, Refit,
    MAX_BUILDING_LEVEL, CANCEL_REFUND_FRACTION, DefenceKind, RatioLabel, ShipyardItem,
};
use iac_shared::protocol::{
    AlertEvent, AlertLevel, ChartMark, Command, GameEvent, EventKind, ErrorCode, HarvestResource, PolicyPreset, PolicyParams,
    MovePreview, OreReserve, Refining, ResourceEta, SectorOdds, SectorState, ThreatBasis, ThreatInfo, TileReserve,
    StorageFullEvent, StorageNearCapEvent, StorageState,
    ExchangeMeter, ExchangeTerms, ExchangedEvent,
    OutpostCompletedEvent, OutpostSurvey, PowerRange, SectorResources, SurveySite,
    BattleGroup, BattleLosses, BattleOutcome, BattleRoundEvent, BattleSide, BattleState, BattleUnits,
    RaidBattle, RaidForecast, RaidLabel, RaidOdds,
    DockRepair, RepairQuote, ShipCount, ShipGroupState, ShipsLostEvent, UnitCount,
    ChaseForecast, Fleeing, FlightQuote, Pursuit, PursuitEvent, PursuitPhase,
};

use crate::auth::{self, TokenHash};
use crate::bot::BotRecord;
use crate::combat::{self, Squad, Troop};
use crate::score;
use crate::intel::KnownSectors;
use crate::odds::{self, Force};
use crate::snapshot::{ExploreCredit, Snapshot, WorldMeta};

// ── Core Data Types ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FleetStatus {
    Idle,
    Moving,
    Harvesting,
    InCombat,
    Returning,
    Docked,
    Exploring,
}

/// The ships of one class in a fleet or a group: how many, and the hull of
/// each that is not whole, ascending. The whole ones are not written down,
/// and no ship has an id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipGroup {
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    pub count: u32,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub damaged: Vec<u32>,
}

impl ShipGroup {
    pub fn whole(&self) -> u32 {
        self.count - self.damaged.len() as u32
    }
}

/// What a fleet or a pirate group is: its ships by class, in catalogue
/// order, with no class it has none of. This is all a fight needs between
/// two ticks, so it is what a stored world holds of one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ships(Vec<ShipGroup>);

impl Ships {
    pub fn of(counts: &[(ShipClass, u32)]) -> Ships {
        let mut ships = Ships::default();
        for &(class, count) in counts {
            ships.add(class, count);
        }
        ships
    }

    pub fn groups(&self) -> &[ShipGroup] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn total(&self) -> u32 {
        self.0.iter().map(|g| g.count).sum()
    }

    pub fn count(&self, class: ShipClass) -> u32 {
        self.0.iter().find(|g| g.ship_class == class).map_or(0, |g| g.count)
    }

    pub fn has(&self, class: ShipClass) -> bool {
        self.count(class) > 0
    }

    pub fn has_damage(&self) -> bool {
        self.0.iter().any(|g| !g.damaged.is_empty())
    }

    pub fn damaged(&self) -> u32 {
        self.0.iter().map(|g| g.damaged.len() as u32).sum()
    }

    pub fn counts(&self) -> impl Iterator<Item = (ShipClass, u32)> + '_ {
        self.0.iter().map(|g| (g.ship_class, g.count))
    }

    fn group_mut(&mut self, class: ShipClass) -> &mut ShipGroup {
        let at = match self.0.binary_search_by_key(&class, |g| g.ship_class) {
            Ok(at) => at,
            Err(at) => {
                self.0.insert(at, ShipGroup { ship_class: class, count: 0, damaged: Vec::new() });
                at
            }
        };
        &mut self.0[at]
    }

    fn settle(&mut self) {
        self.0.retain(|g| g.count > 0);
    }

    /// `count` whole ships of `class` join.
    pub fn add(&mut self, class: ShipClass, count: u32) {
        if count > 0 {
            self.group_mut(class).count += count;
        }
    }

    /// One ship of `class` holding `hull` joins: whole at `full` or more.
    pub fn add_holding(&mut self, class: ShipClass, hull: u32, full: u32) {
        let group = self.group_mut(class);
        group.count += 1;
        if hull < full {
            let at = group.damaged.partition_point(|&h| h <= hull);
            group.damaged.insert(at, hull.max(1));
        }
    }

    /// Every ship of `other` joins, each with the hull it holds.
    pub fn absorb(&mut self, other: Ships) {
        for theirs in other.0 {
            let group = self.group_mut(theirs.ship_class);
            group.count += theirs.count;
            group.damaged.extend(theirs.damaged);
            group.damaged.sort_unstable();
        }
    }

    /// Take `count` ships of `class` out, or None if there are not so many.
    /// The whole go first and then the least damaged; with `damaged_first`
    /// the most damaged go first.
    pub fn take(&mut self, class: ShipClass, count: u32, damaged_first: bool) -> Option<ShipGroup> {
        let group = self.0.iter_mut().find(|g| g.ship_class == class)?;
        if count > group.count {
            return None;
        }
        let listed = group.damaged.len() as u32;
        let damaged: Vec<u32> = if damaged_first {
            group.damaged.drain(..count.min(listed) as usize).collect()
        } else {
            let wounded = count.saturating_sub(group.whole());
            group.damaged.split_off((listed - wounded) as usize)
        };
        group.count -= count;
        self.settle();
        Some(ShipGroup { ship_class: class, count, damaged })
    }

    /// The hull every damaged ship holds, carried from one full hull to
    /// another: each keeps the share it had. What a change of Armour
    /// Technology does to a fleet.
    pub fn rearm(&mut self, from: Arms, to: Arms) {
        if from.hull == to.hull {
            return;
        }
        for group in &mut self.0 {
            let was = u64::from(scaling::ship_figures(group.ship_class, from).hull.max(1));
            let full = scaling::ship_figures(group.ship_class, to).hull;
            for hull in &mut group.damaged {
                *hull = ((u64::from(*hull) * u64::from(full) / was) as u32).max(1);
            }
            group.damaged.retain(|&hull| hull < full);
            group.damaged.sort_unstable();
        }
    }

    /// Make one ship of `class` that holds `hull` whole.
    fn mend_one(&mut self, class: ShipClass, hull: u32) {
        if let Some(group) = self.0.iter_mut().find(|g| g.ship_class == class)
            && let Ok(at) = group.damaged.binary_search(&hull)
        {
            group.damaged.remove(at);
        }
    }

    /// One step of a group's mending toward `muster`, the ships it was
    /// generated with: every damaged ship regains a part of its full hull,
    /// and a part of the ships of each class return, as far as it is short.
    pub fn heal(&mut self, muster: &Ships, arms: Arms) {
        for theirs in &muster.0 {
            let full = scaling::ship_figures(theirs.ship_class, arms).hull;
            let regained = scaling::npc_heal_hull(full);
            let group = self.group_mut(theirs.ship_class);
            for hull in &mut group.damaged {
                *hull = hull.saturating_add(regained);
            }
            group.damaged.retain(|&hull| hull < full);
            let short = theirs.count.saturating_sub(group.count);
            group.count += short.min(scaling::npc_heal_ships(theirs.count));
        }
        self.settle();
    }

    /// Steps of `heal` it takes until the ships are `muster` again.
    pub fn heal_steps(&self, muster: &Ships, arms: Arms) -> u32 {
        muster.0.iter().fold(0, |steps, theirs| {
            let full = scaling::ship_figures(theirs.ship_class, arms).hull;
            let here = self.0.iter().find(|g| g.ship_class == theirs.ship_class);
            let short = theirs.count.saturating_sub(here.map_or(0, |g| g.count));
            let lowest = here.and_then(|g| g.damaged.first().copied()).unwrap_or(full);
            let hull = full.saturating_sub(lowest).div_ceil(scaling::npc_heal_hull(full));
            steps.max(short.div_ceil(scaling::npc_heal_ships(theirs.count))).max(hull)
        })
    }

    /// Hull the ships hold between them, and at full, fighting with `arms`.
    pub fn hull(&self, arms: Arms) -> (u64, u64) {
        self.0.iter().fold((0, 0), |(held, full), group| {
            let each = u64::from(scaling::ship_figures(group.ship_class, arms).hull);
            let damaged: u64 = group.damaged.iter().map(|&h| u64::from(h)).sum();
            (held + each * u64::from(group.whole()) + damaged, full + each * u64::from(group.count))
        })
    }

    /// `weapon + (hull + shield) / 10` of every ship, each at the hull it holds.
    pub fn power(&self, arms: Arms) -> f32 {
        self.0
            .iter()
            .map(|group| {
                let figures = scaling::ship_figures(group.ship_class, arms);
                group.whole() as f32 * figures.power()
                    + group.damaged.iter().map(|&hull| figures.power_at(hull)).sum::<f32>()
            })
            .sum()
    }

    /// The ships as a round of a fight takes them.
    pub fn squads(&self) -> Vec<Squad> {
        self.0
            .iter()
            .map(|g| Squad { unit: ShipyardItem::Ship(g.ship_class), count: g.count, damaged: g.damaged.clone() })
            .collect()
    }

    /// The ships a round of a fight hands back.
    pub fn of_squads(squads: &[Squad]) -> Ships {
        let mut ships = Ships::default();
        for squad in squads {
            if let ShipyardItem::Ship(class) = squad.unit
                && squad.count > 0
            {
                let group = ships.group_mut(class);
                group.count = squad.count;
                group.damaged = squad.damaged.clone();
            }
        }
        ships
    }

    /// The ships by class as a client is told of them.
    pub fn states(&self, arms: Arms) -> Vec<ShipGroupState> {
        self.0
            .iter()
            .map(|group| {
                let figures = scaling::ship_figures(group.ship_class, arms);
                let full = u64::from(figures.hull);
                let held: u64 = group.damaged.iter().map(|&h| u64::from(h)).sum();
                ShipGroupState {
                    ship_class: group.ship_class,
                    count: group.count,
                    damaged: group.damaged.len() as u32,
                    at_risk: group.damaged.iter().filter(|&&hull| at_risk(hull, figures.hull)).count() as u32,
                    hull: full * u64::from(group.whole()) + held,
                    hull_max: full * u64::from(group.count),
                    unit_attack: figures.attack,
                    unit_shield: figures.shield,
                    unit_hull: figures.hull,
                }
            })
            .collect()
    }
}

/// A unit holding `hull` of `full` may explode when a shot next breaks its shield.
fn at_risk(hull: u32, full: u32) -> bool {
    u64::from(hull) * 100 <= u64::from(full) * u64::from(EXPLODE_BELOW_PCT)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fleet {
    pub id: u64,
    pub owner_id: u64,
    pub location: Hex,
    pub state: FleetStatus,
    pub ships: Ships,
    /// The Weapons, Shielding and Armour Technology the fleet fights with:
    /// what its owner had when it last docked at home.
    pub refit: Refit,
    pub cargo: Resources,
    pub fuel: f64,
    pub fuel_max: f64,
    pub move_cooldown: u16,
    pub action_cooldown: u16,
    pub move_target: Option<Hex>,
    /// What the current (or last) harvest order mines.
    pub harvest_target: HarvestResource,
    /// Sectors this fleet has charted and not yet delivered to a dock.
    pub charts: Vec<u32>,
    /// Loaded by hand at home: the dock leaves this hold alone. It ends with
    /// the fleet's next jump, so cargo it brings home unloads as any does.
    /// Never set on a fleet under standing orders.
    pub laden: bool,
}

impl Fleet {
    pub fn arms(&self) -> Arms {
        self.refit.arms()
    }

    /// The fleet as a round of a fight takes it. One that is charging a
    /// jump is leaving: it is fired on and does not fire.
    pub fn troop(&self) -> Troop {
        Troop { arms: self.arms(), squads: self.ships.squads(), silent: self.state == FleetStatus::Moving }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcFleet {
    pub id: u64,
    pub location: Hex,
    pub ships: Ships,
    /// What the group fights with: every figure of its class at the
    /// multiplier it was generated with.
    pub arms: Arms,
    pub behavior: iac_shared::world::NpcBehaviorType,
    pub home_sector: Hex,
    pub in_combat: bool,
    /// What the group cost to build, and its combined power, fixed at spawn.
    pub bounty: Resources,
    pub power: f32,
    /// "3x cruiser pack": how messages name the group.
    pub label: String,
    /// The ships it was generated with: what it mends back to.
    pub muster: Ships,
    /// Ticks rested in its own sector toward the next step of its mending.
    pub rested: u64,
    /// Jumps made after a fleet since it was last in its own sector.
    pub strayed: u8,
    /// The jump it is charging: after a fleet, or on its way back.
    pub errand: Option<Errand>,
}

/// A group's jump out of or back to its own sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Errand {
    /// The fleet it follows; None on the way back.
    pub quarry: Option<u64>,
    pub to: Hex,
    /// The tick it lands on.
    pub arrives: u64,
}

impl NpcFleet {
    pub fn troop(&self) -> Troop {
        Troop { arms: self.arms, squads: self.ships.squads(), silent: false }
    }

    pub fn whole(&self) -> bool {
        self.ships == self.muster
    }

    /// Jumps it may still make after a fleet before it turns back.
    pub fn leash(&self) -> u8 {
        self.behavior.chase_hexes().saturating_sub(self.strayed)
    }

    /// Ticks one jump takes it: its slowest class on the drive of its ring.
    pub fn jump_ticks(&self) -> u16 {
        let ring = self.home_sector.dist_from_origin();
        self.muster.counts().map(|(class, _)| scaling::npc_jump_ticks(class, ring)).max().unwrap_or(MOVE_BASE_COOLDOWN)
    }

    /// It is the group the template of its own sector spawns.
    pub fn of_lair(&self) -> bool {
        self.id == template_npc_id(self.home_sector)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: u64,
    pub name: String,
    pub resources: Resources,
    pub homeworld: Hex,
    pub buildings: BuildingLevels,
    pub research: ResearchLevels,
    /// Buildings under construction (at most `building_slots`).
    pub building_queue: Vec<BuildQueueEntry>,
    pub building_pending: Vec<PendingBuilding>,
    pub ship_queue: Option<ShipQueueEntry>,
    pub ship_pending: Vec<PendingShip>,
    pub defences: Defences,
    /// Solar Satellites at home: built as ships, never part of a fleet.
    pub satellites: u32,
    pub research_queue: Option<ResearchQueueEntry>,
    pub research_pending: Vec<PendingResearch>,
    /// SHA-256 of the account's token; None for accounts that predate
    /// tokens until their next login claims them.
    pub token_hash: Option<TokenHash>,
    /// Self-declared when the account was created; labels the leaderboard
    /// and nothing else.
    pub agent: bool,
    /// Score from kills and from exploration, before the 25 percent cap.
    pub combat_points: f32,
    pub explore_points: f32,
    /// Ancient relics found; reserved for the item system.
    pub relics: u32,
    /// The exchange meter as it read at `exchange_tick`, in weight units. It
    /// falls between trades: read it through `GameEngine::exchange_meter`.
    pub exchange_pressure: f64,
    pub exchange_tick: u64,
}

/// A queued building or research level holds no resources: it pays when it
/// starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingBuilding {
    pub id: u64,
    pub building_type: BuildingType,
    pub target_level: u8,
    pub reserve: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingResearch {
    pub id: u64,
    pub tech: ResearchType,
    pub target_level: u8,
    pub reserve: bool,
}

/// A shipyard batch waiting its turn. Unlike a building or research order
/// it paid for every unit when it was ordered, so it waits only for the yard
/// or for something queued ahead of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingShip {
    pub id: u64,
    pub item: ShipyardItem,
    pub count: u16,
}

/// Defence structures standing at home.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Defences {
    pub count: [u32; DefenceKind::COUNT],
}

impl Defences {
    /// Combined power of the structures with the owner's research.
    pub fn power(&self, research: &ResearchLevels) -> f32 {
        DefenceKind::ALL.iter()
            .map(|&k| self.count[k as usize] as f32 * scaling::structure_power(k, research))
            .sum()
    }

    pub fn owned(&self, kind: DefenceKind) -> u32 {
        self.count[kind as usize]
    }
}

impl Player {
    /// Building levels once everything queued has finished; what the next
    /// order is built on top of.
    pub fn projected_buildings(&self) -> BuildingLevels {
        let mut levels = self.buildings.clone();
        for q in &self.building_queue {
            levels.set(q.building_type, levels.get(q.building_type).max(q.target_level));
        }
        for q in &self.building_pending {
            levels.set(q.building_type, levels.get(q.building_type).max(q.target_level));
        }
        levels
    }

    pub fn projected_research(&self) -> ResearchLevels {
        let mut levels = self.research.clone();
        for q in self.research_queue.iter() {
            levels.set(q.tech, levels.get(q.tech).max(q.target_level));
        }
        for q in &self.research_pending {
            levels.set(q.tech, levels.get(q.tech).max(q.target_level));
        }
        levels
    }

    pub fn building_slots(&self) -> usize {
        scaling::building_slots(&self.research) as usize
    }

    /// The homeworld's energy this tick. The Fusion Reactor runs while the
    /// stockpile holds any deuterium for it.
    pub fn power(&self, pace: &Pace) -> scaling::Power {
        scaling::home_power(&self.buildings, &self.research, self.satellites, self.resources.deuterium > 0.0, pace)
    }

    /// What the mines add to the stockpile each tick with the energy there
    /// is, before the Fusion Reactor's burn.
    pub fn mined_per_tick(&self, pace: &Pace) -> Resources {
        scaling::production_per_tick(&self.buildings, self.power(pace).fraction(), pace)
    }

    /// What the stockpile gains each tick: the mines less the Fusion
    /// Reactor's burn, so deuterium may be negative.
    pub fn production_per_tick(&self, pace: &Pace) -> Resources {
        let power = self.power(pace);
        let mined = scaling::production_per_tick(&self.buildings, power.fraction(), pace);
        Resources { deuterium: mined.deuterium - power.fusion_burn, ..mined }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQueueEntry {
    pub id: u64,
    pub building_type: BuildingType,
    pub target_level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipQueueEntry {
    pub id: u64,
    pub item: ShipyardItem,
    pub count: u16,
    /// Units delivered; one more is under way from `start_tick` to `end_tick`.
    pub built: u16,
    pub start_tick: u64,
    pub end_tick: u64,
}

impl ShipQueueEntry {
    /// Units paid for and not yet delivered, the one under way included.
    pub fn undelivered(&self) -> u16 {
        self.count.saturating_sub(self.built)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchQueueEntry {
    pub id: u64,
    pub tech: ResearchType,
    pub target_level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Combat {
    pub id: u64,
    pub sector: Hex,
    pub player_fleet_ids: Vec<u64>,
    pub npc_fleet_ids: Vec<u64>,
    /// Rounds fought.
    pub round: u32,
    /// The most power the players' side has brought to bear, for scoring.
    pub peak_power: f32,
    /// Rounds in a row that took no hull from either side.
    pub quiet_rounds: u16,
    /// Fleets that jumped out of the fight, in the order they left: whom a
    /// group may follow once nobody is left to fight.
    pub fled: Vec<u64>,
    /// What every fleet and group that has fought here lost so far, fleets
    /// first, each side in the order it joined.
    pub losses: Vec<BattleLosses>,
    /// The last round as it was reported, for a client that arrives in the
    /// middle of the fight.
    pub last_round: Option<BattleRoundEvent>,
    /// The fight is a raid on a homeworld or an outpost: the raiders, and
    /// the defences that stand with the fleets. It has no group of
    /// `npc_fleets` in it.
    pub siege: Option<Siege>,
}

/// A raid being fought (`docs/design/ogame.md`, section 7.6). The raiders
/// are no group of the map: they come for one dock, stand in no sector's
/// view and are gone when the fight is over. The defender's fleets are the
/// fight's own, as in any fight, so they leave it as they leave any.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Siege {
    pub owner: u64,
    pub dock: Dock,
    /// The power the raid was announced with, and what stood in its way
    /// when it arrived, on the same scale.
    pub power: f32,
    pub met: f32,
    pub raiders: Ships,
    pub arms: Arms,
    /// The raiders as they arrived.
    pub muster: Ships,
    /// What defends that is not a fleet, as the fight has left it: at home
    /// the Solar Satellites and the defences, at the outpost its platform.
    pub battery: Vec<Squad>,
}

impl Siege {
    fn battery_stands(&self) -> bool {
        self.battery.iter().any(|squad| squad.count > 0)
    }

    /// How the raiders are named in a sentence: "raid of 12 Light Fighter, 2 Cruiser".
    pub fn label(&self) -> String {
        let ships: Vec<String> = self.muster.counts().map(|(class, count)| format!("{count} {}", class.label())).collect();
        format!("raid of {}", ships.join(", "))
    }
}

impl Combat {
    pub fn add_player_fleet(&mut self, fleet_id: u64) {
        if !self.player_fleet_ids.contains(&fleet_id) {
            self.player_fleet_ids.push(fleet_id);
        }
    }

    pub fn add_npc_fleet(&mut self, fleet_id: u64) {
        if !self.npc_fleet_ids.contains(&fleet_id) {
            self.npc_fleet_ids.push(fleet_id);
        }
    }

    pub fn has_player_fleet(&self, fleet_id: u64) -> bool {
        self.player_fleet_ids.contains(&fleet_id)
    }

    pub fn has_npc_fleet(&self, fleet_id: u64) -> bool {
        self.npc_fleet_ids.contains(&fleet_id)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectorOverride {
    pub metal_density: Option<Density>,
    pub crystal_density: Option<Density>,
    pub deut_density: Option<Density>,
    pub metal_harvested: f64,
    pub crystal_harvested: f64,
    pub deut_harvested: f64,
    pub salvage: Option<Resources>,
    pub salvage_despawn_tick: Option<u64>,
    pub npc_cleared_tick: Option<u64>,
    /// Tick a derelict site here was stripped (sites don't respawn).
    pub site_looted_tick: Option<u64>,
    /// Failed boardings so far — each makes the next attempt hotter.
    pub site_ambush_bumps: u8,
    /// Anti-farming heat: kills of this sector's group, decaying; see
    /// `scaling::farm_factor`. `kill_heat_tick` is when it was last updated.
    pub kill_heat: f32,
    pub kill_heat_tick: u64,
}

impl SectorOverride {
    /// Some tile here is below its generated level or part-emptied.
    pub fn has_ore_damage(&self) -> bool {
        self.metal_density.is_some() || self.crystal_density.is_some() || self.deut_density.is_some()
            || self.metal_harvested > 0.0 || self.crystal_harvested > 0.0 || self.deut_harvested > 0.0
    }

    pub fn effective_densities(
        override_opt: Option<&SectorOverride>,
        template: &iac_shared::world::SectorTemplate,
    ) -> (Density, Density, Density) {
        let ov = override_opt;
        (
            ov.and_then(|o| o.metal_density).unwrap_or(template.metal_density),
            ov.and_then(|o| o.crystal_density).unwrap_or(template.crystal_density),
            ov.and_then(|o| o.deut_density).unwrap_or(template.deut_density),
        )
    }
}

/// Per-player raid pacing state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidState {
    pub first_seen_tick: u64,
    pub next_roll_tick: u64,
    pub last_raid_tick: u64,
    pub suppress_until: u64,
    /// Eligible rolls in a row that missed; each raises the next one's odds.
    pub missed_rolls: u32,
    pub incoming: Option<IncomingRaid>,
}

/// Raid cadence at the world's pace (attention class, except the warning).
struct RaidTimers {
    roll_interval: u64,
    warning: u64,
    min_interval: u64,
    min_player_age: u64,
    suppress_after_loss: u64,
}

impl RaidTimers {
    fn new(pace: &Pace) -> Self {
        RaidTimers {
            roll_interval: pace.attention_ticks(RAID_ROLL_INTERVAL as f64),
            warning: pace.raid_warning_ticks(),
            min_interval: pace.attention_ticks(RAID_MIN_INTERVAL as f64),
            min_player_age: pace.attention_ticks(RAID_MIN_PLAYER_AGE as f64),
            suppress_after_loss: pace.attention_ticks(RAID_SUPPRESS_AFTER_LOSS as f64),
        }
    }
}

/// A player's outpost. `level` is 0 until its first level is finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outpost {
    pub sector: Hex,
    pub level: u8,
    pub store: Resources,
    pub job: Option<OutpostJob>,
    pub breached: bool,
    /// Its own raid clock; `first_seen_tick` is the tick level 1 was finished.
    pub raid: RaidState,
    /// Per resource: the owner has been told the store is full.
    pub full_marks: [bool; 3],
    /// The owner has been told the repair is waiting for materials.
    pub repair_warned: bool,
    /// Fleets standing here that were told the store could not fill their tank.
    pub dry_tanks: Vec<u64>,
    pub founded_tick: u64,
    /// Everything `load` has taken out of the store. No rule reads it: it is
    /// what the balance report counts as hauled.
    pub hauled: Resources,
}

impl Outpost {
    /// The extractor runs, tanks are filled and the platform defends.
    pub fn in_service(&self) -> bool {
        self.level >= 1 && !self.breached
    }

    /// The level whose repair the outpost would next have to pay: the one it
    /// stands at, or the one it is building, since a breach suffered during
    /// a build is repaired once the build is done.
    pub fn repair_level(&self) -> u8 {
        match self.job {
            Some(OutpostJob { task: OutpostTask::Build { level }, .. }) => level,
            _ => self.level,
        }
    }

    /// What a raid cannot take from the store: what its next repair would
    /// take from it as it stands.
    pub fn protected(&self) -> Resources {
        scaling::outpost_repair_payment(self.repair_level(), self.store).0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutpostJob {
    pub task: OutpostTask,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutpostTask {
    Build { level: u8 },
    Repair,
}

/// An empire's two docks: where a fleet's hold is filled and emptied by
/// hand, and where a raid comes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dock {
    Home,
    Outpost,
}

/// What a dock does for a fleet's damaged ships with what it has in stock.
struct Mending {
    /// The whole repair priced in metal.
    price: f64,
    /// What the stock pays for the ships it makes whole.
    payment: Resources,
    ships: u32,
    hull: u64,
    /// The fleet's ships once that is done.
    mended: Ships,
}

/// A dock that charges `share` of a ship's cost for a whole hull mends the
/// damaged ships of `fleet` from `stock`: each ship costs the par value of
/// that share times the share of its hull that is gone, the most damaged is
/// mended first, and a ship the stock does not cover is left as it is with
/// every ship after it.
fn mending(fleet: &Fleet, share: f64, stock: Resources) -> Mending {
    let arms = fleet.arms();
    let mut worn: Vec<(ShipClass, u32, u32)> = Vec::new();
    for group in fleet.ships.groups() {
        let full = scaling::ship_figures(group.ship_class, arms).hull;
        worn.extend(group.damaged.iter().map(|&hull| (group.ship_class, hull, full)));
    }
    worn.sort_by_key(|&(_, hull, full)| u64::from(hull) * 1_000_000 / u64::from(full.max(1)));
    let cost = |&(class, hull, full): &(ShipClass, u32, u32)| {
        scaling::par_value(&class.build_cost()) * share * f64::from(full - hull) / f64::from(full.max(1))
    };
    let price: f64 = worn.iter().map(cost).sum();
    let short = scaling::pay_by_weight(price, stock).1;
    let mut budget = price - short;
    let mut mended = fleet.ships.clone();
    let (mut ships, mut hull, mut spent) = (0u32, 0u64, 0.0f64);
    for ship in &worn {
        let due = cost(ship);
        if short > 0.0 && due > budget {
            break;
        }
        budget -= due;
        spent += due;
        ships += 1;
        hull += u64::from(ship.2 - ship.1);
        mended.mend_one(ship.0, ship.1);
    }
    let paid = if short > 0.0 { spent } else { price };
    Mending { price, payment: scaling::pay_by_weight(paid, stock).0, ships, hull, mended }
}

/// Harvest yield not yet reported to the owner.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HarvestReport {
    resources: Resources,
    ticks: u16,
    /// Tick the first unreported harvest happened.
    since: u64,
}

/// Which of a doctrine's two thresholds turns a fleet home, with its wording.
enum Homeward {
    Fuel(String),
    Cargo(String),
}

/// A fleet's standing orders plus the bit of memory the autopilot needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetPolicy {
    pub preset: PolicyPreset,
    pub params: PolicyParams,
    /// The sector this doctrine works: MineAndReturn's claim, staked where
    /// the orders are given and shuttled between it and home; the sector of
    /// PatrolHome's last fight, whose wreckage is its own kill's.
    pub work_sector: Option<Hex>,
    /// Next tick the autopilot will reconsider this fleet.
    pub next_eval_tick: u64,
    /// The last hold reason announced and when, so a fleet that keeps
    /// holding for the same reason does not flood the event stream.
    pub last_hold: Option<(String, u64)>,
    /// Fuel budget: deuterium the fleet could still put into its tank as of
    /// `fuel_since`, the tick of its last budgeted fill (or of its orders).
    pub fuel_credit: f64,
    pub fuel_since: u64,
}

/// The square of sectors within `radius` hexes of `centre`, one slot each:
/// a search confined to it indexes a table instead of hashing sector keys.
#[derive(Clone, Copy)]
struct Window {
    centre: Hex,
    radius: u8,
}

impl Window {
    fn side(&self) -> i32 {
        2 * i32::from(self.radius) + 1
    }

    fn slots(&self) -> usize {
        (self.side() * self.side()) as usize
    }

    fn slot(&self, coord: Hex) -> Option<usize> {
        let side = self.side();
        let q = i32::from(coord.q) - i32::from(self.centre.q) + i32::from(self.radius);
        let r = i32::from(coord.r) - i32::from(self.centre.r) + i32::from(self.radius);
        ((0..side).contains(&q) && (0..side).contains(&r)).then_some((q * side + r) as usize)
    }

    fn coord(&self, slot: usize) -> Hex {
        let side = self.side();
        let (q, r) = (slot as i32 / side, slot as i32 % side);
        Hex::new(
            (i32::from(self.centre.q) + q - i32::from(self.radius)) as i16,
            (i32::from(self.centre.r) + r - i32::from(self.radius)) as i16,
        )
    }
}

/// The ships a fight in a sector would meet: the group the ring spawns at
/// that distance from the hub, or ships that were counted.
enum Opposing {
    Ring(u16),
    Ships(Force),
}

impl Opposing {
    fn force(self) -> Force {
        match self {
            Opposing::Ring(dist) => Force::of_ring(dist),
            Opposing::Ships(force) => force,
        }
    }

    fn ring(&self) -> Option<u16> {
        match self {
            Opposing::Ring(dist) => Some(*dist),
            Opposing::Ships(_) => None,
        }
    }
}

/// One fleet's ratio against each ring's group. Most sectors a fleet is
/// rated in hold nothing its owner has counted, so the answer is the ring's
/// and is worked out once per ring for as long as the fleet is asked about.
#[derive(Default)]
struct RingOdds(RefCell<KeyMap<u16, f32>>);

/// Where a doctrine lets one fleet go, worked out once per evaluation: the
/// fleet's odds in every sector asked about, against what its owner knows.
/// An evaluation searches the same sectors several times over (a lane
/// without a fight, then any lane, then the reason nothing was found), so
/// what it learns of a sector is kept until it ends; the world cannot move
/// while the engine is borrowed.
struct Reach<'a> {
    engine: &'a GameEngine,
    fleet: &'a Fleet,
    params: PolicyParams,
    owner: u64,
    ours: Force,
    home: Hex,
    /// The doctrine's engage ratio.
    need: f32,
    /// Hexes from home the doctrine and the tank allow.
    range: u8,
    /// Score weight of the deuterium one jump burns.
    jump_weight: f32,
    /// Cargo units the hold takes when empty.
    hold: f64,
    /// Cargo units the hold can still take.
    room: f64,
    /// Sectors where a hostile group stands, and whether one of them
    /// attacks whoever enters.
    groups: KeyMap<u32, bool>,
    live: KeySet<u32>,
    /// The sectors a search inside `range` can ask about.
    window: Window,
    rings: RingOdds,
    rated: SectorMemo<(ThreatInfo, f32)>,
    fights: SectorMemo<bool>,
    seams: SectorMemo<Option<f32>>,
    boardable: SectorMemo<bool>,
}

/// One answer per sector for the length of an evaluation: a table over a
/// window, a map for the few sectors asked about beyond it.
struct SectorMemo<T> {
    near: RefCell<Vec<Option<T>>>,
    far: RefCell<KeyMap<u32, T>>,
}

impl<T> Default for SectorMemo<T> {
    fn default() -> SectorMemo<T> {
        SectorMemo { near: RefCell::default(), far: RefCell::default() }
    }
}

impl<T: Copy> SectorMemo<T> {
    fn recall(&self, window: Window, coord: Hex, work: impl FnOnce() -> T) -> T {
        let Some(slot) = window.slot(coord) else {
            let key = coord.to_key();
            if let Some(&known) = self.far.borrow().get(&key) {
                return known;
            }
            let found = work();
            self.far.borrow_mut().insert(key, found);
            return found;
        };
        if let Some(known) = self.near.borrow().get(slot).copied().flatten() {
            return known;
        }
        let found = work();
        let mut near = self.near.borrow_mut();
        if near.is_empty() {
            near.resize(window.slots(), None);
        }
        near[slot] = Some(found);
        found
    }
}

impl Reach<'_> {
    fn rating(&self, coord: Hex) -> (ThreatInfo, f32) {
        self.rated.recall(self.window, coord, || {
            let (threat, theirs) = self.engine.opposition(self.owner, coord, self.live.contains(&coord.to_key()));
            (threat, self.engine.ratio_against(&self.ours, theirs, &self.rings))
        })
    }

    fn ratio(&self, coord: Hex) -> f32 {
        self.rating(coord).1
    }

    /// The doctrine may take the fleet into `coord`: its odds there meet
    /// the engage ratio, whether or not hostiles sit there. A player who
    /// set the ratio below EVEN is honoured.
    fn open(&self, coord: Hex) -> bool {
        coord == self.home || self.ratio(coord) >= self.need
    }

    fn in_range(&self, coord: Hex) -> bool {
        Hex::distance(&coord, &self.home) <= u16::from(self.range)
    }

    /// Why the doctrine will not enter `coord`, if it will not.
    fn refusal(&self, coord: Hex) -> Option<String> {
        if self.open(coord) {
            return None;
        }
        let (threat, ratio) = self.rating(coord);
        let what = if self.engine.sector_has_hostiles(coord) { "holds hostiles" } else { "is unscouted or unsafe" };
        Some(format!(
            "{coord} {what} (T{} power {:.0}: your fleet rates {ratio:.2}x {} against it, the doctrine needs {:.1}x)",
            threat.rating, threat.est_power, scaling::ratio_label(ratio).label(), self.need,
        ))
    }

    /// The lanes from `from` to the nearest sector `goal` accepts, through
    /// sectors the doctrine may enter inside its range.
    fn path_to(&self, from: Hex, goal: &dyn Fn(Hex) -> bool) -> Option<Vec<Hex>> {
        self.path_worth(from, &|n, _| goal(n))
    }

    /// `path_to` with a goal that is told how many jumps out the sector
    /// lies. A lane that meets no fight is taken before a shorter one that
    /// does: hull is not repaired, so a fight the fleet would win still costs.
    fn path_worth(&self, from: Hex, goal: &dyn Fn(Hex, u32) -> bool) -> Option<Vec<Hex>> {
        let calm = |n: Hex| self.open(n) && self.in_range(n) && !self.starts_fight(n);
        self.engine.bfs_path_by_hops(from, POLICY_SEARCH_HOPS, &calm, goal).or_else(|| self.path_through(from, goal))
    }

    /// The shortest way to a goal, fights on the way or not.
    fn path_through(&self, from: Hex, goal: &dyn Fn(Hex, u32) -> bool) -> Option<Vec<Hex>> {
        self.engine.bfs_path_by_hops(from, POLICY_SEARCH_HOPS, &|n| self.open(n) && self.in_range(n), goal)
    }

    /// A hostile group stands in `coord`, or its lair has not woken yet.
    fn has_hostiles(&self, coord: Hex) -> bool {
        self.groups.contains_key(&coord.to_key()) || self.engine.pending_template_npc(coord).is_some()
    }

    /// Entering `coord` starts a fight: a group there is not passive.
    fn starts_fight(&self, coord: Hex) -> bool {
        let passive = iac_shared::world::NpcBehaviorType::Passive;
        self.fights.recall(self.window, coord, || {
            self.groups.get(&coord.to_key()).copied().unwrap_or(false)
                || self.engine.pending_template_npc(coord).is_some_and(|t| t.behavior != passive)
        })
    }

    /// Score weight of the seam in `coord` as it would land in the hold;
    /// None where the tiles hold too little to stake.
    fn seam(&self, coord: Hex) -> Option<f32> {
        self.seams.recall(self.window, coord, || {
            let ore = self.engine.ore_reserve_at(coord)?;
            (ore_units(&ore) >= POLICY_MIN_SEAM_UNITS).then(|| self.engine.seam_worth(self.owner, &ore, self.room))
        })
    }

    /// Whether the doctrine boards a derelict in `coord`.
    fn boards(&self, coord: Hex) -> bool {
        self.boardable.recall(self.window, coord, || self.engine.site_is_boardable(self.fleet, &self.params, coord))
    }

    /// Whether resources worth `weight` (`scaling::resource_weight`) repay
    /// the fuel of flying `hops` jumps out and as many back.
    fn repays(&self, weight: f32, hops: u32) -> bool {
        weight >= POLICY_FUEL_PAYBACK * self.jump_weight * 2.0 * hops as f32
    }

    /// Whether the wreckage in `coord`, as much of it as `room` units of
    /// hold take, repays the fuel of fetching it from `hops` jumps out.
    fn wreck_repays(&self, coord: Hex, room: f64, hops: u32) -> bool {
        self.repays(self.engine.wreck_worth(coord, room), hops)
    }

    /// The first jump of the way home and, when no lane meets the engage
    /// ratio, the band of the mildest lane there is. Err names what blocks
    /// every lane at RISKY odds or better.
    fn way_home(&self, from: Hex) -> Result<(Hex, Option<RatioLabel>), String> {
        let (engine, home) = (self.engine, self.home);
        let is_home = |n: Hex| n == home;
        let mut bars = vec![self.need];
        bars.extend(
            [scaling::RATIO_FAVOURABLE, scaling::RATIO_EVEN, scaling::RATIO_RISKY].into_iter().filter(|bar| *bar < self.need),
        );
        for (i, &bar) in bars.iter().enumerate() {
            let passable = |n: Hex| n == home || self.ratio(n) >= bar;
            let calm = |n: Hex| passable(n) && !self.starts_fight(n);
            let lane = engine.bfs_path(from, POLICY_SEARCH_HOPS, &calm, &is_home)
                .or_else(|| engine.bfs_path(from, POLICY_SEARCH_HOPS, &passable, &is_home));
            if let Some(path) = lane {
                return Ok((path[0], (i > 0).then(|| scaling::ratio_label(bar))));
            }
        }
        let floor = bars[bars.len() - 1];
        match engine.bfs_path(from, POLICY_SEARCH_HOPS, &|_| true, &is_home) {
            Some(path) => {
                let worst = path.iter().copied().min_by(|a, b| self.ratio(*a).total_cmp(&self.ratio(*b)));
                let why = worst.and_then(|n| self.refusal(n)).unwrap_or_else(|| format!("no lane toward {home}"));
                Err(format!("no lane home at {floor:.1}x or better; on the shortest, {why}"))
            }
            None => engine.world_gen.connected_neighbors(from).slice().iter().copied()
                .filter(|&n| n == home || self.ratio(n) >= floor)
                .min_by_key(|n| Hex::distance(n, &home))
                .map(|step| (step, None))
                .ok_or_else(|| format!("no lane toward {home} at {floor:.1}x or better")),
        }
    }

    /// Where a fleet leaving a fight at `from` jumps: the first jump of the
    /// way home if there is one, otherwise the lane it rates best, the
    /// nearer to home of two alike.
    fn way_out(&self, from: Hex) -> Result<Hex, String> {
        if let Ok((step, _)) = self.way_home(from) {
            return Ok(step);
        }
        let nearer = |n: &Hex| std::cmp::Reverse(Hex::distance(n, &self.home));
        self.engine.world_gen.connected_neighbors(from).slice().iter().copied()
            .max_by(|a, b| self.ratio(*a).total_cmp(&self.ratio(*b)).then(nearer(a).cmp(&nearer(b))))
            .ok_or_else(|| format!("no lane leaves {from}"))
    }
}

/// A raid on its way: its power and what it flies, fixed at forecast time
/// so the player's preparations after the warning actually help.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingRaid {
    pub arrival_tick: u64,
    pub power: f32,
    pub ships: Ships,
    pub arms: Arms,
}

// ── GameEngine ────────────────────────────────────────────────────

pub struct Authenticated {
    pub player_id: u64,
    /// Set only when a token was just issued; shown to the client once.
    pub new_token: Option<String>,
    pub registered: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    InvalidName(String),
    TokenRequired,
    InvalidToken,
    WorldFull,
}

/// See `GameEngine::home_route`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeRoute {
    pub charted: Option<u32>,
    pub direct: u32,
    pub unexplored: bool,
}

/// A trade worked out against the exchange; see `GameEngine::exchange_deal`.
#[derive(Debug)]
pub struct ExchangeDeal {
    pub terms: ExchangeTerms,
    /// The exact meter reading the trade leaves.
    pub pressure_after: f64,
    pub refusal: Option<Refusal>,
}

/// Why an order is turned down: the code, and the sentence its sender reads.
#[derive(Debug)]
pub struct Refusal {
    pub code: ErrorCode,
    pub reason: String,
}

impl Refusal {
    fn new(code: ErrorCode, reason: String) -> Refusal {
        Refusal { code, reason }
    }
}

/// Slack on a `load` amount, so a client that sends back the room or the
/// stock it was shown is not refused over a rounding.
const LOAD_SLACK: f64 = 0.01;

/// An outpost's store that was reported full is reported again only after
/// it has fallen below this share of its cap.
const STORE_REFILL_RATIO: f64 = 0.95;

pub struct GameEngine {
    pub world_gen: WorldGen,
    pub world: WorldMeta,
    pub current_tick: u64,

    pub players: BTreeMap<u64, Player>,
    pub fleets: BTreeMap<u64, Fleet>,
    pub npc_fleets: BTreeMap<u64, NpcFleet>,
    pub active_combats: BTreeMap<u64, Combat>,
    pub sector_overrides: BTreeMap<u32, SectorOverride>,

    /// Actively-scanned sectors per player: hex key → expiry tick.
    /// The entity maps are ordered: every pass walks them in id order.
    scan_reveals: BTreeMap<u64, BTreeMap<u32, u64>>,
    raid_states: BTreeMap<u64, RaidState>,
    /// Per player and resource: 0 below 80 percent of the cap, 1 once
    /// `StorageNearCap` fired, 2 once `StorageFull` fired.
    storage_marks: BTreeMap<u64, [u8; 3]>,
    /// Per player: the despawn tick of the home pile already warned about.
    home_salvage_warned: BTreeMap<u64, u64>,
    /// Harvest yield per fleet awaiting its next `ResourceHarvested` event.
    harvest_reports: BTreeMap<u64, HarvestReport>,
    /// The accounts that are built-in bots: what each is and remembers.
    /// The engine only keeps and ranks them; a host's `bot::Roster` plays them.
    bots: BTreeMap<u64, BotRecord>,
    /// Outposts by owner: one each.
    pub outposts: BTreeMap<u64, Outpost>,
    /// The owner of the outpost in each sector that holds one: `outposts`
    /// read the other way, for `outpost_at`, which every sector view asks.
    /// An outpost never moves, so founding and abandoning are all that change it.
    outpost_sites: KeyMap<u32, u64>,
    /// Standing orders per fleet.
    pub policies: BTreeMap<u64, FleetPolicy>,

    pending_events: Vec<GameEvent>,
    pending_arrivals: Vec<u64>,  // fleet IDs pending homeworld/NPC checks after movement

    next_id: u64,

    /// (player, sector key) pairs the player has entered.
    explored: KeySet<(u64, u32)>,
    /// Explore credits already paid: once per player and sector.
    credited: KeySet<(u64, u32, ExploreCredit)>,
    /// (player, sector key) whose chart mark may have changed since the
    /// last chart refresh: delivered, or lost with the fleet that held it.
    pub(crate) chart_news: Vec<(u64, u32)>,
    /// What each player has seen of the galaxy, live or remembered.
    pub known: KnownSectors,
    /// Fights already played forward, by the two sides and the rounds they
    /// were given; speed only.
    odds_memo: RefCell<OddsMemo>,
}

/// Two sides and the rounds they are given, and how that fight goes.
type OddsMemo = KeyMap<(Force, Force, u32), (odds::Outlook, f32)>;

/// Entries kept in the odds memo before it is emptied.
const ODDS_MEMO_LIMIT: usize = 4096;

/// The middle of the roll that sizes a raid: what a forecast with no raid
/// announced is made at.
const RAID_POWER_ROLL_MID: f32 = (RAID_POWER_ROLL_MIN + RAID_POWER_ROLL_MAX) / 2.0;

impl GameEngine {
    /// A fresh world: no players, tick 0.
    pub fn new(world_seed: u64, pace: Pace) -> GameEngine {
        GameEngine::restore(Snapshot::new(world_seed, pace))
    }

    /// Rebuild an engine from a `snapshot()` of a running one, as taken or
    /// as a host read it back from where it stores its world (`store`).
    pub fn restore(snapshot: Snapshot) -> GameEngine {
        let Snapshot {
            world_seed, meta, tick, next_id, players, fleets, npc_fleets, combats,
            sector_overrides, policies, scan_reveals, raid_states, storage_marks, home_salvage_warned,
            harvest_reports, bots, outposts, pending_events, explored, credited, known,
        } = snapshot;

        GameEngine {
            world_gen: WorldGen::init(world_seed),
            world: meta,
            current_tick: tick,
            players,
            fleets,
            npc_fleets,
            active_combats: combats,
            sector_overrides,
            scan_reveals,
            raid_states,
            storage_marks,
            home_salvage_warned,
            harvest_reports,
            bots,
            outpost_sites: outposts.iter().map(|(owner, outpost)| (outpost.sector.to_key(), *owner)).collect(),
            outposts,
            policies,
            pending_events,
            pending_arrivals: Vec::new(),
            next_id,
            explored: explored.into_iter().collect(),
            credited: credited.into_iter().collect(),
            chart_news: Vec::new(),
            known: KnownSectors::load(known),
            odds_memo: RefCell::default(),
        }
    }

    /// The complete world state in canonical order. Two engines that played
    /// the same game have equal snapshots (compare `Snapshot::hash`).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            world_seed: self.world_gen.world_seed,
            meta: self.world,
            tick: self.current_tick,
            next_id: self.next_id,
            players: self.players.clone(),
            fleets: self.fleets.clone(),
            npc_fleets: self.npc_fleets.clone(),
            combats: self.active_combats.clone(),
            sector_overrides: self.sector_overrides.clone(),
            policies: self.policies.clone(),
            scan_reveals: self.scan_reveals.clone(),
            raid_states: self.raid_states.clone(),
            storage_marks: self.storage_marks.clone(),
            home_salvage_warned: self.home_salvage_warned.clone(),
            harvest_reports: self.harvest_reports.clone(),
            bots: self.bots.clone(),
            outposts: self.outposts.clone(),
            pending_events: self.pending_events.clone(),
            explored: self.explored.iter().copied().collect(),
            credited: self.credited.iter().copied().collect(),
            known: self.known.rows(),
        }
    }

    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    pub fn pace(&self) -> Pace {
        self.world.pace
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // ── Main Tick ─────────────────────────────────────────────────

    pub fn tick(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // NOTE: pending_events must NOT be cleared here — command handlers
        // push events between ticks (a host answers its sessions before it
        // steps) and `host::step` drains the queue after every tick.
        self.current_tick += 1;

        self.reap_empty_fleets();
        self.process_policies()?;
        self.process_movement()?;
        self.process_pursuit()?;
        self.process_stranded();
        self.process_combat()?;
        self.process_exploration()?;
        self.process_harvesting()?;
        self.process_sector_regen()?;
        self.process_npc_behavior()?;
        self.process_homeworlds()?;
        self.process_outposts();
        self.process_build_queues()?;
        self.process_raids()?;
        self.process_home_salvage();
        self.process_salvage_despawn()?;
        self.process_derelict_respawn();
        self.process_cooldowns()?;
        self.prune_scan_reveals();

        let mut known = std::mem::take(&mut self.known);
        known.refresh(self);
        self.known = known;
        self.chart_news.clear();

        Ok(())
    }

    /// Emergency reserve: an idle fleet away from home with less fuel than one
    /// jump slowly regains exactly one jump's worth, taking
    /// `STRANDED_RECOVERY_TICKS` from empty. Progress is announced at each
    /// quarter. The reserve is ordinary fuel, so it buys one hop and then
    /// the wait starts over: a fleet five hops out is five waits from home.
    fn process_stranded(&mut self) {
        let mut notes: Vec<(AlertLevel, u64, Hex, String)> = Vec::new();
        let ids: Vec<u64> = self.fleets.keys().copied().collect();
        for fid in ids {
            let fleet = &self.fleets[&fid];
            if fleet.ships.is_empty() || fleet.state != FleetStatus::Idle { continue; }
            let Some(player) = self.players.get(&fleet.owner_id) else { continue; };
            if fleet.location == player.homeworld { continue; }
            let hop = fleet_fuel_cost(fleet, Some(&player.research)).min(fleet.fuel_max);
            if hop <= 0.0 || fleet.fuel >= hop { continue; }

            let old = fleet.fuel;
            let mut new = old + hop / STRANDED_RECOVERY_TICKS as f64;
            if new >= hop * 0.9999 { new = hop; }
            let quarters = |f: f64| (f / hop * 4.0 + 0.0001).floor() as u32;
            let location = fleet.location;
            let note = if new >= hop {
                Some((AlertLevel::Info, format!(
                    "fleet {fid} at {location}: emergency reserve ready, enough fuel for one jump toward home",
                )))
            } else if quarters(new) > quarters(old) {
                let left = ((hop - new) / (hop / STRANDED_RECOVERY_TICKS as f64)).ceil() as u64;
                Some((AlertLevel::Warning, format!(
                    "fleet {fid} stranded at {location}: emergency reserve {}% charged, about {left} ticks to one jump",
                    quarters(new) * 25,
                )))
            } else {
                None
            };
            if let Some((level, message)) = note { notes.push((level, fid, location, message)); }
            if let Some(f) = self.fleets.get_mut(&fid) { f.fuel = new; }
        }
        for (level, fid, sector, message) in notes {
            self.push_alert(level, fid, sector, message);
        }
    }

    /// Drop a fleet and its standing orders.
    fn remove_fleet(&mut self, fleet_id: u64) {
        if let Some(fleet) = self.fleets.remove(&fleet_id) {
            self.chart_news.extend(fleet.charts.iter().map(|&key| (fleet.owner_id, key)));
        }
        self.policies.remove(&fleet_id);
    }

    /// Fleets that lost every ship in combat. They linger until the next
    /// tick so the owner still receives the FleetDestroyed event (event
    /// routing looks the fleet up), but are never listed to clients.
    fn reap_empty_fleets(&mut self) {
        let empty: Vec<u64> = self.fleets.values()
            .filter(|f| f.ships.is_empty())
            .map(|f| f.id)
            .collect();
        for fid in empty {
            self.remove_fleet(fid);
        }
    }

    // ── Movement ──────────────────────────────────────────────────

    fn process_movement(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Phase 1: Advance fleets that are moving
        let mut arrived: Vec<(u64, Hex, u64)> = Vec::new(); // (fleet_id, target, owner_id)

        for (_, fleet) in self.fleets.iter_mut() {
            if fleet.state != FleetStatus::Moving { continue; }

            if fleet.move_cooldown > 0 {
                fleet.move_cooldown -= 1;
                continue;
            }

            if let Some(target) = fleet.move_target {
                fleet.location = target;
                fleet.state = FleetStatus::Idle;
                fleet.move_target = None;
                fleet.laden = false;
                arrived.push((fleet.id, target, fleet.owner_id));
            }
        }

        // Phase 2: Process arrivals (exploration, docking, NPC encounters)
        for (fleet_id, target, owner_id) in arrived {
            self.leave_combat(fleet_id, true);
            let first_visit = !self.explored.contains(&(owner_id, target.to_key()));
            self.explored.insert((owner_id, target.to_key()));
            self.note_chart(fleet_id, owner_id, target);

            self.pending_events.push(GameEvent {
                tick: self.current_tick,
                kind: EventKind::SectorEntered(iac_shared::protocol::SectorEnteredEvent {
                    fleet_id,
                    sector: target,
                    first_visit,
                }),
            });

            // Defer docking/encounter checks to avoid double mutable borrow
            self.pending_arrivals.push(fleet_id);
        }

        // Process deferred arrival checks
        let arrival_ids: Vec<u64> = self.pending_arrivals.drain(..).collect();
        for fleet_id in arrival_ids {
            self.check_homeworld_docking(fleet_id)?;
            self.check_outpost_docking(fleet_id);
            self.announce_if_stranded(fleet_id);
            self.check_npc_encounter(fleet_id)?;
        }

        Ok(())
    }

    fn announce_if_stranded(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let Some(player) = self.players.get(&fleet.owner_id) else { return; };
        if fleet.ships.is_empty() || fleet.location == player.homeworld { return; }
        let hop = fleet_fuel_cost(fleet, Some(&player.research));
        if fleet.fuel >= hop { return; }
        let (location, fuel) = (fleet.location, fleet.fuel);
        let message = format!(
            "fleet {fleet_id} is stranded at {location}: {fuel:.0} fuel, a jump costs {hop:.0}; the emergency reserve recovers one jump in about {} minutes",
            STRANDED_RECOVERY_TICKS / 60,
        );
        self.push_alert(AlertLevel::Critical, fleet_id, location, message);
    }

    // ── Combat ────────────────────────────────────────────────────

    /// One round of every fight, in fight order: every unit of both sides
    /// fires at once (`combat::round`), what is left is written back to the
    /// fleets and the groups, and everyone present is sent one report. A
    /// fleet stands in a round as its id orders it, a group likewise, so a
    /// world resumed from its document picks the same targets with the same
    /// dice. A fleet that is charging a jump is leaving: it stands in the
    /// round as a target and fires nothing. A fight in open space has no
    /// limit of rounds: it ends when a side has no unit left, when the last
    /// fleet has gone, or when `STANDOFF_ROUNDS` rounds in a row took no
    /// hull from anybody.
    fn process_combat(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let combat_ids: Vec<u64> = self.active_combats.keys().copied().collect();

        for combat_id in combat_ids {
            let Some(active) = self.active_combats.get(&combat_id) else { continue; };
            if active.siege.is_some() {
                self.siege_round(combat_id);
                continue;
            }
            let sector = active.sector;
            let round = active.round + 1;
            let fleet_ids = self.fleets_fighting(active);
            let mut group_ids: Vec<u64> = active.npc_fleet_ids.iter().copied()
                .filter(|id| self.npc_fleets.get(id).is_some_and(|n| !n.ships.is_empty()))
                .collect();
            group_ids.sort_unstable();

            if fleet_ids.is_empty() || group_ids.is_empty() {
                self.end_combat(combat_id, BattleOutcome::Withdrawn, None);
                continue;
            }

            let fleets: Vec<Troop> = fleet_ids.iter().map(|id| self.fleets[id].troop()).collect();
            let groups: Vec<Troop> = group_ids.iter().map(|id| self.npc_fleets[id].troop()).collect();
            let engaged: f32 = fleet_ids.iter().map(|id| fleet_power(&self.fleets[id])).sum();

            let mut dice = combat::Dice::of_round(self.world_gen.world_seed, sector, self.current_tick, round);
            let fought = combat::round(&fleets, &groups, &mut dice);
            let quiet = !fought.hull_lost();

            let wiped = self.write_back_fleets(&fleet_ids, &fought.fleets.troops);
            for (nid, squads) in group_ids.iter().zip(&fought.hostiles.troops) {
                if let Some(group) = self.npc_fleets.get_mut(nid) {
                    group.ships = Ships::of_squads(squads);
                }
            }

            let fleet_side = BattleSide {
                groups: self.fleet_groups(&fleet_ids, &fought.fleets.standing),
                ..battle_side(&fought.fleets)
            };
            let hostile_side = BattleSide {
                groups: group_ids.iter().zip(&fought.hostiles.standing).map(|(&fleet_id, units)| BattleGroup {
                    fleet_id,
                    owner: None,
                    label: self.npc_fleets.get(&fleet_id).map(|n| n.label.clone()).unwrap_or_default(),
                    mine: false,
                    fleeing: false,
                    battery: false,
                    units: units.clone(),
                }).collect(),
                ..battle_side(&fought.hostiles)
            };
            let report = BattleRoundEvent { sector, round, fleets: fleet_side, hostiles: hostile_side, mine: false, raid: false };
            let Some(quiet_rounds) = self.record_round(combat_id, report, engaged, quiet) else { continue; };

            let enemies = group_ids.iter()
                .map(|id| self.npc_fleets.get(id).map_or_else(|| "hostile group".to_string(), |n| n.label.clone()))
                .collect::<Vec<_>>()
                .join(", ");
            self.announce_wiped(&wiped, sector, &format!("hostile {enemies}"));

            let fleets_left = fought.fleets.units() > 0;
            let groups_left = fought.hostiles.units() > 0;
            let outcome = match (fleets_left, groups_left) {
                (true, false) => Some(BattleOutcome::HostilesDestroyed),
                (false, true) => Some(BattleOutcome::FleetsDestroyed),
                (false, false) => Some(BattleOutcome::BothDestroyed),
                (true, true) => (quiet_rounds >= STANDOFF_ROUNDS).then_some(BattleOutcome::Standoff),
            };
            if let Some(outcome) = outcome {
                self.end_combat(combat_id, outcome, None);
            }
        }

        Ok(())
    }

    /// The fleets of `combat` that still have ships, in the order they
    /// stand in a round.
    fn fleets_fighting(&self, combat: &Combat) -> Vec<u64> {
        let mut fleet_ids: Vec<u64> = combat.player_fleet_ids.iter().copied()
            .filter(|id| self.fleets.get(id).is_some_and(|f| !f.ships.is_empty()))
            .collect();
        fleet_ids.sort_unstable();
        fleet_ids
    }

    /// Write what a round left of each fleet back to it, and settle the
    /// tank and the hold of one that lost ships. Returns the fleets that
    /// have none left.
    fn write_back_fleets(&mut self, fleet_ids: &[u64], troops: &[Vec<Squad>]) -> Vec<u64> {
        let mut wiped: Vec<u64> = Vec::new();
        for (&fid, squads) in fleet_ids.iter().zip(troops) {
            let Some(fleet) = self.fleets.get_mut(&fid) else { continue; };
            let before = fleet.ships.total();
            fleet.ships = Ships::of_squads(squads);
            if fleet.ships.total() == before { continue; }
            if let Some(player) = self.players.get(&fleet.owner_id) {
                fleet.fuel_max = fleet_fuel_max(fleet, player);
                fleet.fuel = fleet.fuel.min(fleet.fuel_max);
            }
            if fleet.ships.is_empty() {
                wiped.push(fid);
            }
            self.settle_hold(fid);
        }
        wiped
    }

    /// The fleets of a round as its report lists them.
    fn fleet_groups(&self, fleet_ids: &[u64], standing: &[Vec<BattleUnits>]) -> Vec<BattleGroup> {
        fleet_ids.iter().zip(standing).map(|(&fleet_id, units)| BattleGroup {
            fleet_id,
            owner: self.fleets.get(&fleet_id).and_then(|f| self.players.get(&f.owner_id)).map(|p| p.name.clone()),
            label: String::new(),
            mine: false,
            fleeing: self.fleets.get(&fleet_id).is_some_and(|f| f.state == FleetStatus::Moving),
            battery: false,
            units: units.clone(),
        }).collect()
    }

    /// Count a round into its fight and send its report. Returns the rounds
    /// in a row that took no hull, this one included.
    fn record_round(&mut self, combat_id: u64, report: BattleRoundEvent, engaged: f32, quiet: bool) -> Option<u16> {
        let active = self.active_combats.get_mut(&combat_id)?;
        active.round = report.round;
        active.peak_power = active.peak_power.max(engaged);
        active.quiet_rounds = if quiet { active.quiet_rounds + 1 } else { 0 };
        for group in report.fleets.groups.iter().chain(&report.hostiles.groups) {
            tally_losses(&mut active.losses, group);
        }
        active.last_round = Some(report.clone());
        let quiet_rounds = active.quiet_rounds;
        self.pending_events.push(GameEvent { tick: self.current_tick, kind: EventKind::BattleRound(report) });
        Some(quiet_rounds)
    }

    /// Say that each of `wiped` lost its last ship in `sector`, to `by`.
    fn announce_wiped(&mut self, wiped: &[u64], sector: Hex, by: &str) {
        for &fid in wiped {
            let owner = self.fleets.get(&fid).and_then(|f| self.players.get(&f.owner_id)).map(|p| p.name.clone());
            self.pending_events.push(GameEvent {
                tick: self.current_tick,
                kind: EventKind::FleetDestroyed(iac_shared::protocol::FleetDestroyedEvent {
                    fleet_id: fid,
                    is_npc: false,
                    sector,
                    owner,
                    mine: false,
                    salvage: Resources::default(),
                    label: String::new(),
                }),
            });
            self.alert_fleet_lost(fid, sector, by);
        }
    }

    /// The sector of `owner`'s `dock`, if it has one.
    fn dock_sector(&self, owner: u64, dock: Dock) -> Option<Hex> {
        match dock {
            Dock::Home => self.players.get(&owner).map(|p| p.homeworld),
            Dock::Outpost => self.outposts.get(&owner).map(|o| o.sector),
        }
    }

    /// What defends `owner`'s `dock` that is not a fleet, whole, as a round
    /// takes it: at home the Solar Satellites and every defence, at the
    /// outpost the platform of its level, none while it is breached.
    fn battery_of(&self, owner: u64, dock: Dock) -> Vec<Squad> {
        let whole = |unit: ShipyardItem, count: u32| Squad { unit, count, damaged: Vec::new() };
        let squads: Vec<Squad> = match dock {
            Dock::Home => {
                let Some(player) = self.players.get(&owner) else { return Vec::new(); };
                std::iter::once(whole(ShipyardItem::Ship(ShipClass::SolarSatellite), player.satellites))
                    .chain(DefenceKind::ALL.into_iter().map(|kind| whole(ShipyardItem::Defence(kind), player.defences.count[kind as usize])))
                    .collect()
            }
            Dock::Outpost => {
                let Some(outpost) = self.outposts.get(&owner).filter(|o| !o.breached) else { return Vec::new(); };
                scaling::outpost_platform(outpost.level).iter().map(|&(kind, count)| whole(ShipyardItem::Defence(kind), count)).collect()
            }
        };
        squads.into_iter().filter(|squad| squad.count > 0).collect()
    }

    /// What a battery fights with: at home its owner's Weapons, Shielding
    /// and Armour Technology as they stand this tick; the outpost's
    /// platform at the catalogue's figures.
    fn battery_arms(&self, owner: u64, dock: Dock) -> Arms {
        match dock {
            Dock::Home => self.players.get(&owner).map_or(Arms::BASE, |p| Refit::of(&p.research).arms()),
            Dock::Outpost => Arms::BASE,
        }
    }

    /// The fleets of `owner` standing at `sector` that would fire in a raid
    /// arriving now: not one that is charging a jump out.
    fn garrison(&self, owner: u64, sector: Hex) -> Force {
        let mut force = Force::default();
        for fleet in self.fleets.values() {
            if fleet.owner_id == owner && fleet.location == sector && !fleet.ships.is_empty() && fleet.state != FleetStatus::Moving {
                force.join(&fleet_force(fleet));
            }
        }
        force
    }

    /// How a raid of `ships` fighting with `arms` would end against what
    /// stands at `owner`'s `dock` this tick: the fight itself played
    /// forward for `RAID_ROUNDS` rounds.
    pub fn raid_odds(&self, owner: u64, dock: Dock, ships: &Ships, arms: Arms) -> RaidOdds {
        let mut ours = self.dock_sector(owner, dock).map_or_else(Force::default, |sector| self.garrison(owner, sector));
        ours.join(&Force::of(self.battery_arms(owner, dock), self.battery_of(owner, dock)));
        self.odds_outlook(&ours, &Force::of(arms, ships.squads()), RAID_ROUNDS).raid()
    }

    /// The raid on `player`'s homeworld as a client is told of it: the one
    /// being fought, else the one announced, else one at the middle of its
    /// roll were it announced now.
    pub fn home_raid_forecast(&self, player: &Player) -> RaidForecast {
        if let Some(fought) = self.raid_fought(player.id, Dock::Home) {
            return fought;
        }
        let announced = self.raid_states.get(&player.id).and_then(|state| state.incoming.as_ref());
        match announced {
            Some(raid) => self.forecast(player.id, Dock::Home, Some(raid.arrival_tick), raid),
            None => {
                let base = scaling::raid_power_base(scaling::econ_points(&player.buildings, &player.research));
                self.forecast(player.id, Dock::Home, None, &Self::home_raid(player, base * RAID_POWER_ROLL_MID, 0))
            }
        }
    }

    /// The same for `owner`'s outpost; None before its first level is
    /// finished, when nothing raids it.
    pub fn outpost_raid_forecast(&self, owner: u64) -> Option<RaidForecast> {
        let outpost = self.outposts.get(&owner).filter(|outpost| outpost.level >= 1)?;
        if let Some(fought) = self.raid_fought(owner, Dock::Outpost) {
            return Some(fought);
        }
        Some(match outpost.raid.incoming.as_ref() {
            Some(raid) => self.forecast(owner, Dock::Outpost, Some(raid.arrival_tick), raid),
            None => {
                let ring = outpost.sector.dist_from_origin();
                let raid = outpost_raid(ring, scaling::outpost_raid_power(ring) * RAID_POWER_ROLL_MID, 0);
                self.forecast(owner, Dock::Outpost, None, &raid)
            }
        })
    }

    /// The raid being fought at `owner`'s `dock`, as its forecast: what it
    /// flew in with, the rounds left, and how the fight would end from
    /// where it stands.
    fn raid_fought(&self, owner: u64, dock: Dock) -> Option<RaidForecast> {
        let combat = self
            .active_combats
            .values()
            .find(|c| c.siege.as_ref().is_some_and(|siege| siege.owner == owner && siege.dock == dock))?;
        let siege = combat.siege.as_ref()?;
        Some(RaidForecast {
            arrival_tick: None,
            rounds_left: Some(RAID_ROUNDS.saturating_sub(combat.round)),
            power: siege.power,
            ships: siege.muster.counts().map(|(ship_class, count)| ShipCount { ship_class, count }).collect(),
            odds: self.siege_outlook(combat)?.raid(),
        })
    }

    fn forecast(&self, owner: u64, dock: Dock, arrival_tick: Option<u64>, raid: &IncomingRaid) -> RaidForecast {
        RaidForecast {
            arrival_tick,
            rounds_left: None,
            power: raid.power,
            ships: raid.ships.counts().map(|(ship_class, count)| ShipCount { ship_class, count }).collect(),
            odds: self.raid_odds(owner, dock, &raid.ships, raid.arms),
        }
    }

    /// A raid of `power` on `player`'s homeworld, arriving at `arrival_tick`:
    /// what it flies is decided by the defences that stand there now.
    fn home_raid(player: &Player, power: f32, arrival_tick: u64) -> IncomingRaid {
        let (ships, multiplier) = scaling::raid_composition(power, &player.defences.count);
        IncomingRaid { arrival_tick, power, ships: Ships::of(&ships), arms: Arms::of_grade(multiplier) }
    }

    /// A raid has arrived at `owner`'s `dock`. With nothing in its way it
    /// is lost on the spot. Otherwise it stands there as a fight of its
    /// own: every fleet of the owner at the dock is in it, with the
    /// battery, and the first round is fought on the next tick.
    fn begin_siege(&mut self, owner: u64, dock: Dock, raid: IncomingRaid) {
        let Some(sector) = self.dock_sector(owner, dock) else { return; };
        let met = match dock {
            Dock::Home => self.home_defense_power(owner),
            Dock::Outpost => self.outpost_defense_power(owner),
        };
        let siege = Siege {
            owner,
            dock,
            power: raid.power,
            met,
            raiders: raid.ships.clone(),
            arms: raid.arms,
            muster: raid.ships,
            battery: self.battery_of(owner, dock),
        };
        let fleets: Vec<u64> = self.fleets.values()
            .filter(|f| f.owner_id == owner && f.location == sector && !f.ships.is_empty())
            .map(|f| f.id)
            .collect();
        if fleets.is_empty() && !siege.battery_stands() {
            self.settle_raid(sector, &siege, None, 0, &[], None);
            return;
        }
        let against = {
            let ships: u32 = fleets.iter().filter_map(|id| self.fleets.get(id)).map(|f| f.ships.total()).sum();
            let (satellites, defences) = siege.battery.iter().fold((0, 0), |(satellites, defences), squad| match squad.unit {
                ShipyardItem::Ship(_) => (satellites + squad.count, defences),
                ShipyardItem::Defence(_) => (satellites, defences + squad.count),
            });
            format!("{ships} ship(s), {satellites} Solar Satellite(s) and {defences} defence(s)")
        };
        let place = match dock {
            Dock::Home => format!("your homeworld at {sector}"),
            Dock::Outpost => format!("your outpost at {sector}"),
        };
        let message = format!(
            "the {} has arrived at {place}: it meets {against} and fights for at most {RAID_ROUNDS} rounds, the first next tick",
            siege.label(),
        );
        let combat_id = self.next_id();
        self.active_combats.insert(combat_id, Combat {
            id: combat_id,
            sector,
            player_fleet_ids: Vec::new(),
            npc_fleet_ids: Vec::new(),
            round: 0,
            peak_power: 0.0,
            quiet_rounds: 0,
            fled: Vec::new(),
            losses: Vec::new(),
            last_round: None,
            siege: Some(siege),
        });
        self.enlist_garrison(combat_id);
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::Alert(AlertEvent {
                player_id: Some(owner),
                level: AlertLevel::Warning,
                message,
                sector: Some(sector),
                fleet_id: None,
            }),
        });
    }

    /// Every fleet of the raided empire that stands at the dock is in the
    /// raid's fight: those there when it arrived, and one that arrives or
    /// is built while it lasts, from the next round. A fleet of another
    /// empire standing there is not: the raid is not on it.
    fn enlist_garrison(&mut self, combat_id: u64) {
        let Some(combat) = self.active_combats.get(&combat_id) else { return; };
        let Some(owner) = combat.siege.as_ref().map(|siege| siege.owner) else { return; };
        let sector = combat.sector;
        let newcomers: Vec<u64> = self.fleets.values()
            .filter(|f| f.owner_id == owner && f.location == sector && !f.ships.is_empty() && !combat.has_player_fleet(f.id))
            .map(|f| f.id)
            .collect();
        for fid in newcomers {
            if let Some(combat) = self.active_combats.get_mut(&combat_id) {
                combat.add_player_fleet(fid);
            }
            self.engage_fleet(fid);
        }
    }

    /// The battery of a raid on a homeworld takes in what the yard has
    /// finished since the last round: a defence or a Solar Satellite built
    /// during the fight stands in it from the next round.
    fn reinforce_battery(&mut self, combat_id: u64) {
        let Some(siege) = self.active_combats.get_mut(&combat_id).and_then(|c| c.siege.as_mut()) else { return; };
        if siege.dock != Dock::Home { return; }
        let Some(player) = self.players.get(&siege.owner) else { return; };
        let standing = std::iter::once((ShipyardItem::Ship(ShipClass::SolarSatellite), player.satellites))
            .chain(DefenceKind::ALL.into_iter().map(|kind| (ShipyardItem::Defence(kind), player.defences.count[kind as usize])));
        for (unit, count) in standing {
            match siege.battery.iter_mut().find(|squad| squad.unit == unit) {
                Some(squad) => squad.count = squad.count.max(count),
                None if count > 0 => {
                    let at = siege.battery.partition_point(|squad| squad.unit.index() < unit.index());
                    siege.battery.insert(at, Squad { unit, count, damaged: Vec::new() });
                }
                None => {}
            }
        }
    }

    /// One round of a raid's fight: the owner's fleets at the dock and the
    /// battery against the raiders, by the round every fight is fought by.
    /// It is repelled when no raider is left, or when `RAID_ROUNDS` rounds
    /// are over or a round took no hull and a defender still stands; it is
    /// lost only when no defender is left.
    fn siege_round(&mut self, combat_id: u64) {
        self.enlist_garrison(combat_id);
        self.reinforce_battery(combat_id);
        let Some(active) = self.active_combats.get(&combat_id) else { return; };
        let Some(siege) = active.siege.as_ref() else { return; };
        let (sector, round, owner, dock) = (active.sector, active.round + 1, siege.owner, siege.dock);
        let fleet_ids = self.fleets_fighting(active);
        let battery: Vec<Squad> = siege.battery.iter().filter(|squad| squad.count > 0).cloned().collect();
        let has_battery = !battery.is_empty();
        if fleet_ids.is_empty() && !has_battery {
            self.end_siege(combat_id, BattleOutcome::FleetsDestroyed);
            return;
        }
        if siege.raiders.is_empty() {
            self.end_siege(combat_id, BattleOutcome::HostilesDestroyed);
            return;
        }

        let mut defenders: Vec<Troop> = fleet_ids.iter().map(|id| self.fleets[id].troop()).collect();
        if has_battery {
            defenders.push(Troop { arms: self.battery_arms(owner, dock), squads: battery, silent: false });
        }
        let raiders = [Troop { arms: siege.arms, squads: siege.raiders.squads(), silent: false }];
        let label = siege.label();
        let engaged: f32 = fleet_ids.iter().map(|id| fleet_power(&self.fleets[id])).sum();
        let owner_name = self.players.get(&owner).map(|p| p.name.clone());

        let mut dice = combat::Dice::of_round(self.world_gen.world_seed, sector, self.current_tick, round);
        let fought = combat::round(&defenders, &raiders, &mut dice);
        let quiet = !fought.hull_lost();

        let wiped = self.write_back_fleets(&fleet_ids, &fought.fleets.troops);
        let mut groups = self.fleet_groups(&fleet_ids, &fought.fleets.standing);
        if has_battery {
            let stands = fought.fleets.troops.last().cloned().unwrap_or_default();
            if dock == Dock::Home
                && let Some(player) = self.players.get_mut(&owner)
            {
                let left = |unit: ShipyardItem| stands.iter().find(|squad| squad.unit == unit).map_or(0, |squad| squad.count);
                player.satellites = left(ShipyardItem::Ship(ShipClass::SolarSatellite));
                for kind in DefenceKind::ALL {
                    player.defences.count[kind as usize] = left(ShipyardItem::Defence(kind));
                }
            }
            groups.push(BattleGroup {
                fleet_id: 0,
                owner: owner_name,
                label: match dock {
                    Dock::Home => "home defences".to_string(),
                    Dock::Outpost => "outpost platform".to_string(),
                },
                mine: false,
                fleeing: false,
                battery: true,
                units: fought.fleets.standing.last().cloned().unwrap_or_default(),
            });
            if let Some(siege) = self.active_combats.get_mut(&combat_id).and_then(|c| c.siege.as_mut()) {
                siege.battery = stands;
            }
        }
        if let Some(siege) = self.active_combats.get_mut(&combat_id).and_then(|c| c.siege.as_mut()) {
            siege.raiders = Ships::of_squads(&fought.hostiles.troops[0]);
        }

        let report = BattleRoundEvent {
            sector,
            round,
            fleets: BattleSide { groups, ..battle_side(&fought.fleets) },
            hostiles: BattleSide {
                groups: vec![BattleGroup {
                    fleet_id: combat_id,
                    owner: None,
                    label: label.clone(),
                    mine: false,
                    fleeing: false,
                    battery: false,
                    units: fought.hostiles.standing[0].clone(),
                }],
                ..battle_side(&fought.hostiles)
            },
            mine: false,
            raid: true,
        };
        let Some(quiet_rounds) = self.record_round(combat_id, report, engaged, quiet) else { return; };
        self.announce_wiped(&wiped, sector, &format!("the {label}"));

        let defenders_left = fought.fleets.units() > 0;
        let raiders_left = fought.hostiles.units() > 0;
        let outcome = match (defenders_left, raiders_left) {
            (true, false) => Some(BattleOutcome::HostilesDestroyed),
            (false, true) => Some(BattleOutcome::FleetsDestroyed),
            (false, false) => Some(BattleOutcome::BothDestroyed),
            (true, true) => (round >= RAID_ROUNDS || quiet_rounds >= STANDOFF_ROUNDS).then_some(BattleOutcome::Outlasted),
        };
        if let Some(outcome) = outcome {
            self.end_siege(combat_id, outcome);
        }
    }

    /// End a raid's fight. The fleets in it are free, and one charging its
    /// jump out goes on charging. Raiders destroyed to the last ship at a
    /// homeworld leave their wreckage there; at an outpost they leave
    /// none, as no raid on one ever did. Raiders that are not destroyed
    /// are gone with what they kept. What follows for the dock is
    /// `settle_raid`'s.
    fn end_siege(&mut self, combat_id: u64, outcome: BattleOutcome) {
        let Some(combat) = self.active_combats.remove(&combat_id) else { return; };
        let Some(siege) = combat.siege.as_ref() else { return; };
        let sector = combat.sector;
        for fid in &combat.player_fleet_ids {
            let Some(fleet) = self.fleets.get_mut(fid) else { continue; };
            if fleet.state == FleetStatus::InCombat {
                fleet.state = if fleet.ships.is_empty() { FleetStatus::Docked } else { FleetStatus::Idle };
            }
        }
        let dropped = (siege.raiders.is_empty() && siege.dock == Dock::Home).then(|| {
            let worth = siege.muster.counts()
                .fold(Resources::default(), |sum, (class, count)| sum.add(class.build_cost().scale(f64::from(count))));
            let salvage = worth.scale(RAID_DEFENSE_SALVAGE_FRACTION);
            let (pile, despawn) = self.drop_salvage(sector, salvage);
            (salvage, pile, despawn)
        });
        let (fleets, hostiles): (Vec<BattleLosses>, Vec<BattleLosses>) =
            combat.losses.iter().cloned().partition(|l| l.owner.is_some());
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::CombatEnded(iac_shared::protocol::CombatEndedEvent {
                sector,
                player_victory: outcome == BattleOutcome::HostilesDestroyed,
                outcome,
                raid: true,
                rounds: combat.round,
                losses: fleets.into_iter().chain(hostiles).collect(),
                owners: self.players.get(&siege.owner).map(|p| p.name.clone()).into_iter().collect(),
                mine: false,
                salvage: dropped.map(|(_, pile, _)| pile),
                salvage_despawn_tick: dropped.map(|(_, _, despawn)| despawn),
            }),
        });
        self.settle_raid(sector, siege, Some(outcome), combat.round, &combat.losses, dropped.map(|(salvage, _, despawn)| (salvage, despawn)));
    }

    /// End a fight. Fleets still in it are free, and one charging its jump
    /// out goes on charging. A group that was destroyed leaves its wreckage
    /// and pays its score, and its lair is cleared. A group that was not
    /// keeps its losses and its damage: when fleets jumped out it may follow
    /// the strongest of them (`begin_pursuit`), away from its own sector it
    /// turns back, and in it it mends (`process_npc_behavior`). `withdrew`
    /// names the owner of a fleet whose leaving ended the fight.
    fn end_combat(&mut self, combat_id: u64, outcome: BattleOutcome, withdrew: Option<String>) {
        let Some(combat) = self.active_combats.remove(&combat_id) else { return; };
        let sector = combat.sector;

        let mut owners: std::collections::BTreeSet<String> = withdrew.into_iter().collect();
        owners.extend(combat.losses.iter().filter_map(|l| l.owner.clone()));
        for fid in &combat.player_fleet_ids {
            let Some(fleet) = self.fleets.get_mut(fid) else { continue; };
            if fleet.state == FleetStatus::InCombat {
                fleet.state = if fleet.ships.is_empty() { FleetStatus::Docked } else { FleetStatus::Idle };
            }
            owners.extend(self.players.get(&fleet.owner_id).map(|p| p.name.clone()));
            if let Some(patrol) = self.policies.get_mut(fid).filter(|p| p.preset == PolicyPreset::PatrolHome) {
                patrol.work_sector = Some(sector);
            }
        }

        let (fallen, standing): (Vec<u64>, Vec<u64>) = combat.npc_fleet_ids.iter().copied()
            .filter(|nid| self.npc_fleets.contains_key(nid))
            .partition(|nid| self.npc_fleets[nid].ships.is_empty());

        let mut kills: Vec<(Hex, Resources, f32)> = Vec::new();
        let mut total = Resources::default();
        for nid in &fallen {
            let group = &self.npc_fleets[nid];
            let lair = if group.of_lair() { group.home_sector } else { sector };
            let salvage = npc_salvage(group, &self.world.pace).scale(f64::from(self.sector_farm_factor(lair)));
            total = total.add(salvage);
            match kills.iter_mut().find(|(at, ..)| *at == lair) {
                Some((_, pile, power)) => {
                    *pile = pile.add(salvage);
                    *power += group.power;
                }
                None => kills.push((lair, salvage, group.power)),
            }
            self.pending_events.push(GameEvent {
                tick: self.current_tick,
                kind: EventKind::FleetDestroyed(iac_shared::protocol::FleetDestroyedEvent {
                    fleet_id: *nid,
                    is_npc: true,
                    sector,
                    owner: None,
                    mine: false,
                    salvage,
                    label: group.label.clone(),
                }),
            });
        }
        let pile = (!fallen.is_empty()).then(|| self.drop_salvage(sector, total));

        let (fleets, hostiles): (Vec<BattleLosses>, Vec<BattleLosses>) =
            combat.losses.iter().cloned().partition(|l| l.owner.is_some());
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::CombatEnded(iac_shared::protocol::CombatEndedEvent {
                sector,
                player_victory: outcome == BattleOutcome::HostilesDestroyed,
                outcome,
                raid: false,
                rounds: combat.round,
                losses: fleets.into_iter().chain(hostiles).collect(),
                owners: owners.into_iter().collect(),
                mine: false,
                salvage: pile.map(|(heap, _)| heap),
                salvage_despawn_tick: pile.map(|(_, tick)| tick),
            }),
        });

        for (lair, spoils, power) in kills {
            self.credit_kill(combat.peak_power, &combat.player_fleet_ids, power, lair.to_key(), spoils);
            self.heat_sector(lair);
            self.ensure_override(lair.to_key()).npc_cleared_tick = Some(self.current_tick);
        }
        for nid in &fallen {
            self.npc_fleets.remove(nid);
        }
        for nid in standing {
            if let Some(group) = self.npc_fleets.get_mut(&nid) {
                group.in_combat = false;
                group.rested = 0;
            }
            if outcome != BattleOutcome::Standoff {
                self.begin_pursuit(nid, &combat.fled);
            }
            self.settle_group(nid);
        }
    }

    /// No group enters `sector`: home ground, the hub, or a sector an
    /// outpost stands in. A dock is a refuge.
    fn is_refuge(&self, sector: Hex) -> bool {
        world::is_home_ground(sector) || world::is_hub(sector) || self.outpost_at(sector).is_some()
    }

    /// Whether `group` goes after `fleet`, which has left it: Err says why
    /// not. A group follows next door and no further at a time, never into
    /// a refuge, and only as far as its leash.
    fn trail(&self, group: &NpcFleet, fleet_id: u64) -> Result<Hex, String> {
        let Some(fleet) = self.fleets.get(&fleet_id).filter(|f| !f.ships.is_empty()) else {
            return Err(format!("fleet {fleet_id} is gone"));
        };
        let at = fleet.location;
        if self.is_refuge(at) {
            return Err(format!("fleet {fleet_id} has reached {at}, where no group follows"));
        }
        if group.leash() == 0 {
            let far = group.strayed;
            return Err(format!(
                "it has followed {far} jump{} from its own sector at {} and goes no further",
                if far == 1 { "" } else { "s" },
                group.home_sector,
            ));
        }
        if !self.world_gen.connected_neighbors(group.location).slice().contains(&at) {
            return Err(format!("fleet {fleet_id} is out of its reach at {at}"));
        }
        Ok(at)
    }

    /// The fight a group stood in is over and fleets jumped out of it: it
    /// goes after the strongest of them it can reach, if that is a fleet it
    /// was beating (`CHASE_MIN_RATIO`).
    fn begin_pursuit(&mut self, group_id: u64, fled: &[u64]) {
        let Some(group) = self.npc_fleets.get(&group_id) else { return; };
        if group.ships.is_empty() { return; }
        let strongest = fled.iter()
            .filter(|id| self.trail(group, **id).is_ok())
            .filter_map(|id| self.fleets.get(id))
            .map(|fleet| (fleet_power(fleet), fleet.id))
            .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(&a.1)));
        if let Some((power, quarry)) = strongest
            && group.ships.power(group.arms) >= CHASE_MIN_RATIO * power
        {
            self.chase(group_id, quarry, PursuitPhase::Began);
        }
    }

    /// Start `group_id` on a jump to where `quarry` stands, which `trail`
    /// has allowed. A fleet under standing orders is woken to it.
    fn chase(&mut self, group_id: u64, quarry: u64, phase: PursuitPhase) {
        let Some(to) = self.fleets.get(&quarry).map(|f| f.location) else { return; };
        let tick = self.current_tick;
        let Some(group) = self.npc_fleets.get_mut(&group_id) else { return; };
        let ticks = group.jump_ticks();
        group.errand = Some(Errand { quarry: Some(quarry), to, arrives: tick + u64::from(ticks) });
        group.strayed += 1;
        group.rested = 0;
        if let Some(policy) = self.policies.get_mut(&quarry) {
            policy.next_eval_tick = policy.next_eval_tick.min(tick + 1);
        }
        self.push_pursuit(group_id, quarry, phase, None);
    }

    /// A group with no fight and no fleet to follow. Away from its own
    /// sector it starts the next jump of the way back; a way it cannot find
    /// puts it there. In its own sector and whole it stands down: the lair
    /// is its template again, an ambush party is gone. Wounded, it stays
    /// and mends.
    fn settle_group(&mut self, group_id: u64) {
        let Some(group) = self.npc_fleets.get(&group_id) else { return; };
        if group.in_combat || group.errand.is_some() { return; }
        let (from, home, ticks) = (group.location, group.home_sector, group.jump_ticks());
        if from != home {
            let reach = group.behavior.chase_hexes().saturating_add(2);
            let step = self.lane_path(from, home, reach, &|n| !self.is_refuge(n)).and_then(|path| path.first().copied());
            let tick = self.current_tick;
            let Some(group) = self.npc_fleets.get_mut(&group_id) else { return; };
            match step {
                Some(to) => {
                    group.errand = Some(Errand { quarry: None, to, arrives: tick + u64::from(ticks) });
                    return;
                }
                None => group.location = home,
            }
        }
        let Some(group) = self.npc_fleets.get_mut(&group_id) else { return; };
        group.strayed = 0;
        if group.whole() {
            self.npc_fleets.remove(&group_id);
        }
    }

    fn push_pursuit(&mut self, group_id: u64, quarry: u64, phase: PursuitPhase, why: Option<String>) {
        let Some(group) = self.npc_fleets.get(&group_id) else { return; };
        let owner = self.fleets.get(&quarry)
            .and_then(|f| self.players.get(&f.owner_id))
            .map(|p| p.name.clone())
            .unwrap_or_default();
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::Pursuit(PursuitEvent {
                group_id,
                label: group.label.clone(),
                fleet_id: quarry,
                owner,
                mine: false,
                sector: group.location,
                phase,
                to: group.errand.map(|e| e.to),
                arrives_in: group.errand.map(|e| e.arrives.saturating_sub(self.current_tick) as u16),
                hexes_left: group.leash(),
                why,
            }),
        });
    }

    /// The groups' jumps. One that lands where a fleet stands fires on it
    /// this tick: the fight is joined, or joined again, and a fleet still
    /// charging its jump there is fired on until it is gone. One that lands
    /// on an empty sector goes on after its fleet or turns back; one that
    /// lands in its own sector is home.
    fn process_pursuit(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tick = self.current_tick;
        let landing: Vec<u64> = self.npc_fleets.values()
            .filter(|n| !n.in_combat && n.errand.is_some_and(|e| e.arrives <= tick))
            .map(|n| n.id)
            .collect();
        for nid in landing {
            let Some(group) = self.npc_fleets.get_mut(&nid) else { continue; };
            let Some(errand) = group.errand.take() else { continue; };
            group.location = errand.to;
            let passive = group.behavior == iac_shared::world::NpcBehaviorType::Passive;
            let here = errand.to;
            let hunts = !passive && !self.is_refuge(here);
            let met = self.fleets.values()
                .filter(|f| hunts && f.location == here && !f.ships.is_empty())
                .map(|f| f.id)
                .min_by_key(|id| (Some(*id) != errand.quarry, *id));
            if let Some(fleet_id) = met {
                self.start_combat(fleet_id, nid)?;
                continue;
            }
            if let Some(quarry) = errand.quarry {
                let trail = self.npc_fleets.get(&nid).map(|group| self.trail(group, quarry));
                match trail {
                    Some(Ok(_)) => {
                        self.chase(nid, quarry, PursuitPhase::Continued);
                        continue;
                    }
                    Some(Err(why)) => {
                        self.settle_group(nid);
                        self.push_pursuit(nid, quarry, PursuitPhase::Abandoned, Some(why));
                        continue;
                    }
                    None => {}
                }
            }
            self.settle_group(nid);
        }
        Ok(())
    }

    /// The fights `player_id` has a fleet in or is present at, each with
    /// its last round when `with_round`.
    pub fn battles_of(&self, player_id: u64, with_round: bool) -> Vec<BattleState> {
        self.active_combats
            .values()
            .filter(|combat| {
                combat.player_fleet_ids.iter().any(|id| self.fleets.get(id).is_some_and(|f| f.owner_id == player_id))
                    || self.fleets.values().any(|f| f.owner_id == player_id && f.location == combat.sector && !f.ships.is_empty())
                    || self.players.get(&player_id).is_some_and(|p| p.homeworld == combat.sector)
                    || self.outposts.get(&player_id).is_some_and(|o| o.sector == combat.sector)
            })
            .map(|combat| BattleState {
                sector: combat.sector,
                rounds: combat.round,
                last_round: combat.last_round.as_ref().filter(|_| with_round).cloned(),
                losses: combat.losses.clone(),
                raid: combat.siege.as_ref().zip(self.siege_outlook(combat)).map(|(siege, outlook)| RaidBattle {
                    owner: self.players.get(&siege.owner).map(|p| p.name.clone()).unwrap_or_default(),
                    mine: siege.owner == player_id,
                    outpost: siege.dock == Dock::Outpost,
                    power: siege.power,
                    ships: siege.muster.counts().map(|(ship_class, count)| ShipCount { ship_class, count }).collect(),
                    rounds_left: RAID_ROUNDS.saturating_sub(combat.round),
                    odds: outlook.raid(),
                }),
            })
            .collect()
    }

    /// Score for destroying the groups of one lair, `power` between them as
    /// they were generated, once per respawn of the sector and to every
    /// player who fought (cooperation costs nothing). It follows the pile
    /// the kill left (finds factor and anti-farming heat included), so
    /// points and resources rise and fall together. `engaged` is the most
    /// power the players' side brought to bear.
    fn credit_kill(&mut self, engaged: f32, player_fleets: &[u64], power: f32, sector_key: u32, pile: Resources) {
        let fresh = self.sector_overrides.get(&sector_key).is_none_or(|o| o.npc_cleared_tick.is_none());
        if !fresh { return; }
        let points = score::kill_points(pile, engaged, power);
        if points <= 0.0 { return; }
        let owners: HashSet<u64> = player_fleets.iter()
            .filter_map(|f| self.fleets.get(f))
            .map(|f| f.owner_id)
            .collect();
        for pid in owners {
            if let Some(p) = self.players.get_mut(&pid) {
                p.combat_points += points;
            }
        }
    }

    /// Ticks for a sector's heat to halve.
    fn farm_half_life(&self, sector: Hex) -> u64 {
        let hours = scaling::npc_respawn_hours(sector.dist_from_origin());
        let respawn = self.world.pace.econ_ticks(f64::from(hours) * 3600.0);
        (respawn as f32 * scaling::FARM_HALF_LIFE_RESPAWNS).ceil() as u64
    }

    fn sector_heat(&self, sector: Hex) -> f32 {
        self.sector_overrides.get(&sector.to_key()).map_or(0.0, |o| {
            scaling::farm_heat_after(o.kill_heat, self.current_tick.saturating_sub(o.kill_heat_tick), self.farm_half_life(sector))
        })
    }

    /// What a kill here pays now, as a share of a fresh sector's.
    fn sector_farm_factor(&self, sector: Hex) -> f32 {
        scaling::farm_factor(self.sector_heat(sector))
    }

    /// Record a kill: the next one here pays less until the heat decays.
    fn heat_sector(&mut self, sector: Hex) {
        let heat = self.sector_heat(sector) + 1.0;
        let tick = self.current_tick;
        let key = sector.to_key();
        let ov = self.ensure_override(key);
        ov.kill_heat = heat;
        ov.kill_heat_tick = tick;
    }

    /// Turn the per-tick harvest tallies into events: one per fleet every
    /// HARVEST_REPORT_TICKS ticks, and right away once the fleet stops
    /// harvesting (or is gone), so the last stretch is never lost.
    fn flush_harvest_reports(&mut self) {
        let tick = self.current_tick;
        let due: Vec<u64> = self.harvest_reports.iter()
            .filter(|(fid, r)| {
                r.ticks > 0
                    && (tick - r.since >= HARVEST_REPORT_TICKS
                        || self.fleets.get(fid).is_none_or(|f| f.state != FleetStatus::Harvesting))
            })
            .map(|(fid, _)| *fid)
            .collect();
        for fid in due {
            let Some(r) = self.harvest_reports.remove(&fid) else { continue; };
            if self.fleets.contains_key(&fid) {
                self.pending_events.push(GameEvent {
                    tick,
                    kind: EventKind::ResourceHarvested(iac_shared::protocol::ResourceHarvestedEvent {
                        fleet_id: fid,
                        resources: r.resources,
                        ticks: r.ticks,
                    }),
                });
            }
        }
        let fleets = &self.fleets;
        self.harvest_reports.retain(|fid, _| fleets.contains_key(fid));
    }

    /// Tell a fleet's owner, loudly, that the fleet is gone, and `by` what.
    fn alert_fleet_lost(&mut self, fleet_id: u64, sector: Hex, by: &str) {
        let Some(owner) = self.fleets.get(&fleet_id).map(|f| f.owner_id) else { return; };
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::Alert(iac_shared::protocol::AlertEvent {
                player_id: Some(owner),
                level: iac_shared::protocol::AlertLevel::Critical,
                message: format!(
                    "FLEET LOST: fleet {fleet_id} destroyed at [{},{}] by {by}; every ship is gone",
                    sector.q, sector.r
                ),
                sector: Some(sector),
                fleet_id: Some(fleet_id),
            }),
        });
    }

    // ── Harvesting ────────────────────────────────────────────────

    fn process_harvesting(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Each harvesting fleet collects ALL available resources per tick
        // (metal, then crystal, then deuterium), capped by remaining cargo.
        let fleet_ids: Vec<u64> = self.fleets.keys().copied().collect();

        for fid in fleet_ids {
            // Phase 1: read fleet + sector, compute per-resource harvest amounts
            let Some(fleet) = self.fleets.get(&fid) else { continue; };
            if fleet.state != FleetStatus::Harvesting { continue; }

            let location = fleet.location;
            let sector_key = location.to_key();
            let template = self.world_gen.generate_sector(location);
            let ov = self.sector_overrides.get(&sector_key);
            let (metal_d, crystal_d, deut_d) = SectorOverride::effective_densities(ov, &template);

            let target = fleet.harvest_target;
            let yield_mult = self.players.get(&fleet.owner_id)
                .map_or(1.0, |p| scaling::harvest_yield(p.research.laser_technology));
            let harvest_power = fleet_harvest_power(fleet);
            let dist = location.dist_from_origin();
            let max_cargo = fleet_cargo_capacity(fleet, &self.world.pace);
            let used = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
            let mut remaining = max_cargo - used;

            if remaining <= 0.0 {
                if let Some(f) = self.fleets.get_mut(&fid) {
                    f.state = FleetStatus::Idle;
                }
                self.alert_harvest_ended(fid, "its hold is full");
                continue;
            }

            let targets = [
                (metal_d, HarvestResource::Metal, "metal", ov.map_or(0.0, |o| o.metal_harvested)),
                (crystal_d, HarvestResource::Crystal, "crystal", ov.map_or(0.0, |o| o.crystal_harvested)),
                (deut_d, HarvestResource::Deuterium, "deut", ov.map_or(0.0, |o| o.deut_harvested)),
            ];

            // Phase 2: apply, sequentially capped by remaining cargo
            let mut harvested_any = false;
            let mut took = Resources::default();
            for (density, res_type, accum_key, gone_in_level) in targets {
                if target != HarvestResource::Auto && target != res_type { continue; }
                let in_level = (scaling::ore_step_units(density, dist) - gone_in_level).max(0.0);
                let raw = (f64::from(density.harvest_multiplier()) * harvest_power).min(in_level).min(remaining / yield_mult);
                if raw > 0.0 {
                    let cargo = raw * yield_mult;
                    remaining -= cargo;
                    harvested_any = true;

                    if let Some(f) = self.fleets.get_mut(&fid) {
                        match res_type {
                            HarvestResource::Metal => f.cargo.metal += cargo,
                            HarvestResource::Crystal => f.cargo.crystal += cargo,
                            _ => f.cargo.deuterium += cargo,
                        }
                    }

                    self.accumulate_harvest(sector_key, accum_key, raw, density)?;
                    match res_type {
                        HarvestResource::Metal => took.metal += cargo,
                        HarvestResource::Crystal => took.crystal += cargo,
                        _ => took.deuterium += cargo,
                    }
                }
            }
            if harvested_any {
                let tick = self.current_tick;
                let report = self.harvest_reports.entry(fid)
                    .or_insert_with(|| HarvestReport { since: tick, ..Default::default() });
                report.resources = report.resources.add(took);
                report.ticks += 1;
            } else if let Some(f) = self.fleets.get_mut(&fid) {
                // Nothing harvestable here — stop instead of spinning forever.
                f.state = FleetStatus::Idle;
                self.alert_harvest_ended(fid, match target {
                    HarvestResource::Auto => "nothing is left to mine there",
                    HarvestResource::Metal => "no metal is left to mine there",
                    HarvestResource::Crystal => "no crystal is left to mine there",
                    HarvestResource::Deuterium => "no deuterium is left to mine there",
                });
            }
        }

        self.flush_harvest_reports();
        Ok(())
    }

    /// Tell the owner that a harvest ended by itself, and why. A fleet
    /// under standing orders is left to its doctrine, which reports what it
    /// does about it.
    fn alert_harvest_ended(&mut self, fleet_id: u64, why: &str) {
        if self.policies.contains_key(&fleet_id) { return; }
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let at = fleet.location;
        let message = format!(
            "fleet {fleet_id} stopped mining at {at}: {why}; it carries {:.0} of {:.0}",
            fleet.cargo.total(),
            fleet_cargo_capacity(fleet, &self.world.pace),
        );
        self.push_alert(AlertLevel::Warning, fleet_id, at, message);
    }

    fn accumulate_harvest(&mut self, sector_key: u32, resource: &str, amount: f64, current_density: Density) -> Result<(), Box<dyn std::error::Error>> {
        if current_density == Density::None { return Ok(()); }

        let step = scaling::ore_step_units(current_density, Hex::from_key(sector_key).dist_from_origin());
        let ov = self.ensure_override(sector_key);
        let harvested_ptr = match resource {
            "metal" => &mut ov.metal_harvested,
            "crystal" => &mut ov.crystal_harvested,
            "deut" => &mut ov.deut_harvested,
            _ => &mut ov.metal_harvested,
        };

        *harvested_ptr += amount;

        if *harvested_ptr >= step - scaling::ORE_EPSILON {
            *harvested_ptr = 0.0;
            let new_density = current_density.downgrade();
            match resource {
                "metal" => ov.metal_density = Some(new_density),
                "crystal" => ov.crystal_density = Some(new_density),
                "deut" => ov.deut_density = Some(new_density),
                _ => {}
            }
        }
        Ok(())
    }

    // ── Sector Regen ──────────────────────────────────────────────

    /// Stripped tiles refill over `scaling::ore_regen_hours`, paused while a
    /// fleet sits on the sector, and never past the generated density.
    fn process_sector_regen(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let occupied: KeySet<u32> = self.fleets.values().map(|f| f.location.to_key()).collect();
        let pace = self.world.pace;
        for (&sector_key, ov) in self.sector_overrides.iter_mut() {
            if !ov.has_ore_damage() || occupied.contains(&sector_key) { continue; }

            let coord = Hex::from_key(sector_key);
            let dist = coord.dist_from_origin();
            let template = self.world_gen.generate_sector(coord);
            regen_resource(&mut ov.metal_harvested, &mut ov.metal_density, template.metal_density, dist, &pace);
            regen_resource(&mut ov.crystal_harvested, &mut ov.crystal_density, template.crystal_density, dist, &pace);
            regen_resource(&mut ov.deut_harvested, &mut ov.deut_density, template.deut_density, dist, &pace);
        }
        Ok(())
    }

    // ── Salvage Despawn ───────────────────────────────────────────

    /// Wreckage on a player's own homeworld, mostly from a repelled raid. A
    /// fleet docked there scoops it into storage (what fits; the rest stays
    /// for another try). Otherwise, or when storage is full, an alert fires
    /// once when the pile has `HOME_SALVAGE_WARN_TICKS` left.
    fn process_home_salvage(&mut self) {
        let tick = self.current_tick;
        let mut pids: Vec<u64> = self.players.keys().copied().collect();
        pids.sort_unstable();
        for pid in pids {
            let home = self.players[&pid].homeworld;
            let key = home.to_key();
            let Some((pile, despawn)) = self.sector_overrides.get(&key)
                .and_then(|o| o.salvage.zip(o.salvage_despawn_tick))
            else {
                self.home_salvage_warned.remove(&pid);
                continue;
            };
            let docked = self.fleets.values()
                .filter(|f| {
                    f.owner_id == pid && f.location == home && !f.ships.is_empty()
                        && matches!(f.state, FleetStatus::Idle | FleetStatus::Docked)
                })
                .map(|f| f.id)
                .min();

            let mut left = pile;
            if let Some(fleet_id) = docked {
                let player = self.players.get_mut(&pid).unwrap();
                let cap = scaling::storage_cap(&player.buildings, &self.world.pace);
                let taken = player.resources.shortfall(cap).min(pile);
                if taken.total() > 0.0 {
                    player.resources = player.resources.add(taken);
                    let rest = pile.sub(taken);
                    let remaining = (rest.total() > 1.0).then_some(rest);
                    let ov = self.sector_overrides.get_mut(&key).unwrap();
                    ov.salvage = remaining;
                    if remaining.is_none() { ov.salvage_despawn_tick = None; }
                    self.pending_events.push(GameEvent {
                        tick,
                        kind: EventKind::SalvageCollected(iac_shared::protocol::SalvageCollectedEvent {
                            fleet_id,
                            sector: home,
                            resources: taken,
                            remaining,
                        }),
                    });
                    left = rest;
                }
            }

            let pending_loss = left.total() > 1.0;
            if pending_loss
                && despawn.saturating_sub(tick) <= u64::from(HOME_SALVAGE_WARN_TICKS)
                && self.home_salvage_warned.get(&pid) != Some(&despawn)
            {
                self.home_salvage_warned.insert(pid, despawn);
                let why = if docked.is_some() { "storage is full" } else { "no fleet is docked to scoop it" };
                self.pending_events.push(GameEvent {
                    tick,
                    kind: EventKind::Alert(AlertEvent {
                        player_id: Some(pid),
                        level: AlertLevel::Warning,
                        message: format!(
                            "salvage at home {home} despawns in {}s ({:.0} metal, {:.0} crystal, {:.0} deuterium): {why}",
                            despawn.saturating_sub(tick), left.metal, left.crystal, left.deuterium,
                        ),
                        sector: Some(home),
                        fleet_id: None,
                    }),
                });
            }
        }
    }

    fn process_salvage_despawn(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (&key, ov) in self.sector_overrides.iter_mut() {
            if let Some(despawn_tick) = ov.salvage_despawn_tick
                && self.current_tick >= despawn_tick
            {
                let lost = ov.salvage.take();
                ov.salvage_despawn_tick = None;
                if let Some(resources) = lost {
                    self.pending_events.push(GameEvent {
                        tick: self.current_tick,
                        kind: EventKind::SalvageDespawned(iac_shared::protocol::SalvageDespawnedEvent {
                            sector: Hex::from_key(key),
                            resources,
                        }),
                    });
                }
            }
        }
        Ok(())
    }

    // ── NPC Behavior ──────────────────────────────────────────────

    /// A cleared lair's group returns after its delay. A wounded group at
    /// rest in its own sector mends a step at a time, every
    /// `scaling::npc_heal_step_ticks`; whole again, it stands down.
    fn process_npc_behavior(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (&key, ov) in self.sector_overrides.iter_mut() {
            let Some(cleared_tick) = ov.npc_cleared_tick else { continue; };
            let coord = Hex::from_key(key);
            let hours = scaling::npc_respawn_hours(coord.dist_from_origin());
            let delay = self.world.pace.econ_ticks(f64::from(hours) * 3600.0);
            if self.current_tick >= cleared_tick + delay {
                ov.npc_cleared_tick = None;
            }
        }

        let pace = self.world.pace;
        let resting: Vec<u64> = self.npc_fleets.values()
            .filter(|n| !n.in_combat && n.errand.is_none() && n.location == n.home_sector && !n.whole())
            .map(|n| n.id)
            .collect();
        for nid in resting {
            let Some(group) = self.npc_fleets.get_mut(&nid) else { continue; };
            group.rested += 1;
            if group.rested < scaling::npc_heal_step_ticks(group.home_sector.dist_from_origin(), &pace) { continue; }
            group.rested = 0;
            let (muster, arms) = (group.muster.clone(), group.arms);
            group.ships.heal(&muster, arms);
            self.settle_group(nid);
        }
        Ok(())
    }

    /// The tick `group` will be whole again if it mends undisturbed from
    /// now; None for one that is whole. Away from its own sector or in a
    /// fight the clock is not running, and the tick is what it would be
    /// were it to start now.
    pub fn group_whole_at(&self, group: &NpcFleet) -> Option<u64> {
        if group.whole() { return None; }
        let step = scaling::npc_heal_step_ticks(group.home_sector.dist_from_origin(), &self.world.pace);
        let steps = u64::from(group.ships.heal_steps(&group.muster, group.arms));
        let resting = !group.in_combat && group.errand.is_none() && group.location == group.home_sector;
        Some(self.current_tick + (steps * step).saturating_sub(if resting { group.rested } else { 0 }))
    }

    // ── Homeworlds ────────────────────────────────────────────────

    fn process_homeworlds(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let pace = self.world.pace;
        for (_, player) in self.players.iter_mut() {
            let cap = scaling::storage_cap(&player.buildings, &pace);
            let burn = player.power(&pace).fusion_burn;
            player.resources = player.resources.add_capped(player.mined_per_tick(&pace), cap);
            player.resources.deuterium = (player.resources.deuterium - burn).max(0.0);
        }
        let docked: Vec<u64> = self.fleets.values()
            .filter(|f| f.cargo.total() > 0.0 && self.players.get(&f.owner_id).is_some_and(|p| p.homeworld == f.location))
            .map(|f| f.id)
            .collect();
        for fid in docked {
            self.unload_cargo(fid);
        }
        let thirsty: Vec<u64> = self.fleets.values()
            .filter(|f| f.fuel < f.fuel_max && self.players.get(&f.owner_id).is_some_and(|p| p.homeworld == f.location))
            .map(|f| f.id)
            .collect();
        for fid in thirsty {
            self.refuel_fleet(fid);
        }
        let worn: Vec<u64> = self.fleets.values()
            .filter(|f| self.players.get(&f.owner_id).is_some_and(|p| {
                p.homeworld == f.location && (f.ships.has_damage() || f.refit != Refit::of(&p.research))
            }))
            .map(|f| f.id)
            .collect();
        for fid in worn {
            self.tend_at_home(fid, false);
        }
        self.process_storage_alerts();
        Ok(())
    }

    /// The home dock's work on a fleet standing there: it is refitted to its
    /// owner's Weapons, Shielding and Armour Technology as they stand, and
    /// its damaged ships are mended. Not in a fight, and not while it is
    /// charging a jump out: it has left. `arriving` tells the owner of the
    /// refit; research that finishes over a docked fleet has said so itself.
    fn tend_at_home(&mut self, fleet_id: u64, arriving: bool) {
        let Some(fleet) = self.fleets.get_mut(&fleet_id) else { return; };
        let Some(player) = self.players.get(&fleet.owner_id) else { return; };
        if fleet.location != player.homeworld || fleet.ships.is_empty() { return; }
        if matches!(fleet.state, FleetStatus::InCombat | FleetStatus::Moving) { return; }
        let (was, now) = (fleet.refit, Refit::of(&player.research));
        if was != now {
            fleet.ships.rearm(was.arms(), now.arms());
            fleet.refit = now;
            if arriving {
                let message = format!(
                    "fleet {fleet_id} was refitted at the home dock: Weapons {}, Shielding {}, Armour {} (it flew with {}, {}, {})",
                    now.weapons, now.shielding, now.armour, was.weapons, was.shielding, was.armour,
                );
                let at = fleet.location;
                self.push_alert(AlertLevel::Info, fleet_id, at, message);
            }
        }
        self.mend_fleet(fleet_id, Dock::Home);
    }

    /// What the dock of `fleet`'s owner would do for its damaged ships with
    /// what it has in stock now, and why it would do nothing, if it would
    /// not. None when the owner has no such dock.
    fn dock_mending(&self, fleet: &Fleet, dock: Dock) -> Option<(Mending, Option<String>)> {
        let player = self.players.get(&fleet.owner_id)?;
        match dock {
            Dock::Home => Some((mending(fleet, REPAIR_COST_SHARE_HOME, player.resources), None)),
            Dock::Outpost => {
                let outpost = self.outposts.get(&fleet.owner_id)?;
                let refusal = if outpost.level == 0 {
                    Some(format!("the outpost at {} is not finished", outpost.sector))
                } else if outpost.breached {
                    Some(format!("the outpost at {} is breached and mends nothing until it is repaired", outpost.sector))
                } else if self.policies.contains_key(&fleet.id) {
                    Some("a fleet under standing orders does not use the outpost".to_string())
                } else {
                    None
                };
                Some((mending(fleet, REPAIR_COST_SHARE_OUTPOST, outpost.store), refusal))
            }
        }
    }

    /// What mending `fleet`'s damaged ships would take at each dock; None
    /// when none is damaged.
    pub fn repair_quote(&self, fleet: &Fleet) -> Option<RepairQuote> {
        if !fleet.ships.has_damage() { return None; }
        let (held, full) = fleet.ships.hull(fleet.arms());
        let quote = |(mending, refusal): (Mending, Option<String>)| DockRepair {
            price: mending.price,
            payment: if refusal.is_some() { Resources::default() } else { mending.payment },
            ships: if refusal.is_some() { 0 } else { mending.ships },
            hull: if refusal.is_some() { 0 } else { mending.hull },
            refusal,
        };
        Some(RepairQuote {
            ships: fleet.ships.damaged(),
            hull: full - held,
            home: quote(self.dock_mending(fleet, Dock::Home)?),
            outpost: self.dock_mending(fleet, Dock::Outpost).map(quote),
        })
    }

    /// Mend the damaged ships of a fleet standing at `dock`, as far as the
    /// dock's stock pays for whole ships, the most damaged first, and tell
    /// the owner what was done and what it took.
    fn mend_fleet(&mut self, fleet_id: u64, dock: Dock) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        if !fleet.ships.has_damage() { return; }
        let Some((mending, None)) = self.dock_mending(fleet, dock) else { return; };
        if mending.ships == 0 { return; }
        let (owner, at) = (fleet.owner_id, fleet.location);
        let left = fleet.ships.damaged() - mending.ships;
        let Mending { payment, ships, hull, mended, .. } = mending;
        match dock {
            Dock::Home => {
                if let Some(player) = self.players.get_mut(&owner) {
                    player.resources = player.resources.sub(payment);
                }
            }
            Dock::Outpost => {
                if let Some(outpost) = self.outposts.get_mut(&owner) {
                    outpost.store = outpost.store.sub(payment);
                }
            }
        }
        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.ships = mended;
        #[allow(clippy::unnecessary_min_or_max)]
        let cooldown = fleet.action_cooldown.max(REPAIR_TICKS);
        fleet.action_cooldown = cooldown;
        let place = match dock {
            Dock::Home => "the home dock",
            Dock::Outpost => "your outpost",
        };
        let price = if payment.total() > 0.0 {
            format!("for {}", crate::commands::amount_words(payment))
        } else {
            "for nothing".to_string()
        };
        let rest = if left > 0 { format!("; {left} more wait for the dock's stock to cover them") } else { String::new() };
        self.push_alert(AlertLevel::Info, fleet_id, at, format!(
            "fleet {fleet_id} was mended at {place}: {ships} ship(s) made whole, {hull} hull restored, {price}{rest}",
        ));
    }

    /// Move as much of the fleet's cargo into its owner's stockpile as the
    /// storage cap allows; the rest stays aboard. A laden hold is left alone.
    fn unload_cargo(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        if fleet.laden { return; }
        let Some(player) = self.players.get_mut(&fleet.owner_id) else { return; };
        let cap = scaling::storage_cap(&player.buildings, &self.world.pace);
        let room = player.resources.shortfall(cap).min(fleet.cargo);
        if room.total() <= 0.0 { return; }
        player.resources = player.resources.add(room);
        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.cargo = fleet.cargo.sub(room);
    }

    /// A fleet that has lost ships keeps what the holds it has left carry.
    /// The rest leaves, each resource in proportion: at the owner's
    /// homeworld into the stockpile as far as its caps allow, at the
    /// owner's outpost into its store as far as that has room, and the
    /// remainder is lost; anywhere else into the sector's wreckage. Nothing
    /// is made, and a fleet with no ships left sheds its whole cargo.
    fn settle_hold(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let capacity = fleet_cargo_capacity(fleet, &self.world.pace);
        let aboard = fleet.cargo.total();
        if aboard <= capacity { return; }
        let (owner, sector) = (fleet.owner_id, fleet.location);

        // Rounding can leave the scaled sum a unit in the last place over.
        let mut share = capacity / aboard;
        let mut kept = fleet.cargo.scale(share);
        while kept.total() > capacity {
            share = share.next_down();
            kept = fleet.cargo.scale(share);
        }
        let shed = fleet.cargo.sub(kept);

        let carried = crate::commands::amount_words(shed);
        let pace = self.world.pace;
        let home = self.players.get_mut(&owner).filter(|p| p.homeworld == sector);
        let outpost = self.outposts.get_mut(&owner).filter(|o| o.sector == sector);
        let docked = match (home, outpost) {
            (Some(player), _) => {
                let cap = scaling::storage_cap(&player.buildings, &pace);
                let stored = player.resources.shortfall(cap).min(shed);
                player.resources = player.resources.add(stored);
                Some(("the stockpile", stored))
            }
            (None, Some(outpost)) => {
                let cap = scaling::outpost_store_cap(outpost.level, &pace);
                let stored = outpost.store.shortfall(cap).min(shed);
                outpost.store = outpost.store.add(stored);
                Some(("the outpost's store", stored))
            }
            (None, None) => None,
        };
        let message = match docked {
            Some((dock, stored)) => {
                let lost = shed.sub(stored);
                match (stored.total() > LOAD_SLACK, lost.total() > LOAD_SLACK) {
                    (true, true) => format!(
                        "the ships fleet {fleet_id} lost carried {carried}: {} went to {dock} and {} was lost, {dock} is full",
                        crate::commands::amount_words(stored),
                        crate::commands::amount_words(lost),
                    ),
                    (false, _) => format!(
                        "the ships fleet {fleet_id} lost carried {carried}: it was lost, {dock} is full",
                    ),
                    (true, false) => format!("the ships fleet {fleet_id} lost carried {carried}: it went to {dock}"),
                }
            }
            None => {
                self.drop_salvage(sector, shed);
                format!(
                    "the ships fleet {fleet_id} lost carried {carried}: it drifts as wreckage at {sector} and can be collected for {SALVAGE_DESPAWN_TICKS} ticks",
                )
            }
        };
        if shed.total() > LOAD_SLACK {
            self.push_alert(AlertLevel::Warning, fleet_id, sector, message);
        }

        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.cargo = kept;
    }

    /// Fill the tank at the homeworld, paying `FUEL_DEUT_PER_UNIT` deuterium
    /// per unit. A dry stockpile fills it only as far as it can. A fleet
    /// whose standing orders set a fuel budget is filled in one go, and only
    /// once its allowance and the stockpile cover the whole fill; while the
    /// deuterium store is full the fill is free of the allowance.
    fn refuel_fleet(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let Some(player) = self.players.get(&fleet.owner_id) else { return; };
        let missing = fleet.fuel_max - fleet.fuel;
        let stock = player.resources.deuterium;
        let units = missing.min(stock / FUEL_DEUT_PER_UNIT);
        if units <= 0.0 { return; }

        let budget = self.policies.get(&fleet_id).filter(|p| p.params.fuel_budget_pct < POLICY_FUEL_UNBOUNDED_PCT);
        let store_full = stock >= scaling::storage_cap(&player.buildings, &self.world.pace).deuterium;
        let mut credit_left = None;
        if let Some(policy) = budget && !store_full {
            let cost = missing * FUEL_DEUT_PER_UNIT;
            let allowance = self.fuel_allowance(fleet, policy);
            if allowance < cost || stock < cost { return; }
            credit_left = Some(allowance - cost);
        }

        let owner = player.id;
        if let Some(player) = self.players.get_mut(&owner) {
            player.resources.deuterium = (stock - units * FUEL_DEUT_PER_UNIT).max(0.0);
        }
        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.fuel = if units >= missing { fleet.fuel_max } else { fleet.fuel + units };
        if let Some(credit) = credit_left
            && let Some(policy) = self.policies.get_mut(&fleet_id)
        {
            policy.fuel_credit = credit;
            policy.fuel_since = self.current_tick;
        }
    }

    pub fn storage_state(&self, player: &Player) -> StorageState {
        let pace = &self.world.pace;
        let cap = scaling::storage_cap(&player.buildings, pace);
        let rate = player.production_per_tick(pace);
        let eta = |kind: ResourceKind| {
            let (stock, cap, rate) = (player.resources.get(kind), cap.get(kind), rate.get(kind));
            if stock >= cap { Some(0) } else if rate > 0.0 { Some(((cap - stock) / rate).ceil() as u64) } else { None }
        };
        StorageState {
            cap,
            protected: scaling::storage_protected(&player.buildings, pace),
            full_in_s: ResourceEta {
                metal: eta(ResourceKind::Metal),
                crystal: eta(ResourceKind::Crystal),
                deuterium: eta(ResourceKind::Deuterium),
            },
            capped: ResourceKind::ALL.into_iter()
                .filter(|&k| player.resources.get(k) >= cap.get(k))
                .collect(),
        }
    }

    /// `StorageNearCap` at 85 percent of a cap and `StorageFull` at the cap,
    /// once per fill; hysteresis keeps a stockpile hovering at the cap from
    /// repeating itself.
    fn process_storage_alerts(&mut self) {
        let tick = self.current_tick;
        let mut events = Vec::new();
        let ids: Vec<u64> = self.players.keys().copied().collect();
        for pid in ids {
            let player = &self.players[&pid];
            let state = self.storage_state(player);
            let marks = self.storage_marks.entry(pid).or_insert([0; 3]);
            for (i, kind) in ResourceKind::ALL.into_iter().enumerate() {
                let ratio = player.resources.get(kind) / state.cap.get(kind);
                let full = ratio >= 1.0;
                let eta = match kind {
                    ResourceKind::Metal => state.full_in_s.metal,
                    ResourceKind::Crystal => state.full_in_s.crystal,
                    ResourceKind::Deuterium => state.full_in_s.deuterium,
                };
                let mut mark = marks[i];
                if mark == 2 && ratio < 0.95 { mark = 1; }
                if mark == 1 && ratio < 0.80 { mark = 0; }
                if mark < 2 && full {
                    mark = 2;
                    events.push(EventKind::StorageFull(StorageFullEvent { player_id: Some(pid), resource: kind }));
                } else if mark == 0 && ratio >= scaling::STORAGE_NEAR_CAP_RATIO {
                    mark = 1;
                    events.push(EventKind::StorageNearCap(StorageNearCapEvent {
                        player_id: Some(pid),
                        resource: kind,
                        ratio,
                        full_in_s: eta,
                    }));
                }
                marks[i] = mark;
            }
        }
        for kind in events {
            self.pending_events.push(GameEvent { tick, kind });
        }
    }

    // ── Build Queues ──────────────────────────────────────────────

    fn process_build_queues(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tick = self.current_tick;
        let pace = self.world.pace;
        let mut completed_ships: Vec<(u64, ShipyardItem)> = Vec::new();
        let mut fuel_recalc_players: Vec<u64> = Vec::new();
        let pids: Vec<u64> = self.players.keys().copied().collect();

        for &pid in &pids {
            let Some(player) = self.players.get_mut(&pid) else { continue; };

            let (done, running): (Vec<_>, Vec<_>) =
                std::mem::take(&mut player.building_queue).into_iter().partition(|q| tick >= q.end_tick);
            player.building_queue = running;
            for q in done {
                player.buildings.set(q.building_type, q.target_level);
                if q.building_type == BuildingType::DeuteriumTank {
                    fuel_recalc_players.push(pid);
                }
                self.pending_events.push(GameEvent {
                    tick,
                    kind: EventKind::BuildingCompleted(iac_shared::protocol::BuildingCompletedEvent {
                        building_type: q.building_type,
                        new_level: q.target_level,
                        player_id: Some(pid),
                    }),
                });
            }

            if let Some(q) = player.research_queue.clone()
                && tick >= q.end_tick
            {
                player.research.set(q.tech, q.target_level);
                player.research_queue = None;
                if q.tech == ResearchType::ImpulseDrive {
                    fuel_recalc_players.push(pid);
                }
                self.pending_events.push(GameEvent {
                    tick,
                    kind: EventKind::ResearchCompleted(iac_shared::protocol::ResearchCompletedEvent {
                        tech: q.tech,
                        new_level: q.target_level,
                        player_id: Some(pid),
                    }),
                });
            }

            let yard = player.buildings.clone();
            if let Some(q) = player.ship_queue.as_mut().filter(|q| tick >= q.end_tick) {
                completed_ships.push((pid, q.item));
                q.built += 1;
                if q.built < q.count {
                    q.start_tick = tick;
                    q.end_tick = tick + q.item.unit_ticks(&yard, &pace);
                } else {
                    player.ship_queue = None;
                }
            }
        }

        for (pid, item) in completed_ships {
            let kind = match item {
                ShipyardItem::Ship(ship_class) => {
                    if ship_class.flies() {
                        self.add_ship_to_homeworld(pid, ship_class)?;
                    } else if let Some(p) = self.players.get_mut(&pid) {
                        p.satellites += 1;
                    }
                    EventKind::ShipBuilt(iac_shared::protocol::ShipBuiltEvent {
                        ship_class,
                        count: 1,
                        player_id: Some(pid),
                    })
                }
                ShipyardItem::Defence(kind) => {
                    if let Some(p) = self.players.get_mut(&pid) {
                        p.defences.count[kind as usize] += 1;
                    }
                    EventKind::DefenceBuilt(iac_shared::protocol::DefenceBuiltEvent {
                        defence: kind,
                        count: 1,
                        player_id: Some(pid),
                    })
                }
            };
            self.pending_events.push(GameEvent { tick, kind });
        }

        for pid in fuel_recalc_players {
            self.recalculate_player_fleet_fuel(pid)?;
        }
        for pid in pids {
            self.advance_queues(pid);
        }

        Ok(())
    }

    /// Start waiting orders. A free building slot or lab starts the earliest
    /// waiting order that can start now (what it needs is built and the
    /// stockpile covers it). An order that cannot keeps its place and is
    /// checked again next tick; one with `reserve` set holds the line
    /// instead, so nothing behind it starts while it is ready and short. A
    /// free yard starts the earliest batch whose prerequisites are built: a
    /// batch is paid for when it is ordered, so the stockpile has no say.
    fn advance_queues(&mut self, pid: u64) {
        while let Some(i) = self.players.get(&pid).and_then(|p| {
            if p.building_queue.len() >= p.building_slots() {
                return None;
            }
            first_startable(
                &p.building_pending,
                |q| self.building_ready(p, q.building_type, q.target_level),
                |q| p.resources.can_afford(scaling::building_cost(q.building_type, q.target_level)),
                |q| q.reserve,
            )
        }) {
            let q = self.players.get_mut(&pid).unwrap().building_pending.remove(i);
            self.start_building(pid, q);
        }
        if let Some(i) = self.players.get(&pid).and_then(|p| {
            if p.research_queue.is_some() {
                return None;
            }
            first_startable(
                &p.research_pending,
                |q| self.research_ready(p, q.tech, q.target_level),
                |q| p.resources.can_afford(scaling::research_cost(q.tech, q.target_level)),
                |q| q.reserve,
            )
        }) {
            let q = self.players.get_mut(&pid).unwrap().research_pending.remove(i);
            self.start_research(pid, q);
        }
        if let Some(i) = self.players.get(&pid).and_then(|p| {
            if p.ship_queue.is_some() {
                return None;
            }
            p.ship_pending.iter().position(|q| self.ship_ready(p, q.item))
        }) {
            let q = self.players.get_mut(&pid).unwrap().ship_pending.remove(i);
            self.start_ship_batch(pid, q, Resources::default());
        }
    }

    /// Whether what a building order needs is in place; slots and money aside.
    fn building_ready(&self, p: &Player, b: BuildingType, target: u8) -> bool {
        p.building_queue.iter().all(|q| q.building_type != b)
            && p.buildings.get(b) + 1 == target
            && scaling::building_prerequisites_met(b, &p.buildings, &p.research)
    }

    fn research_ready(&self, p: &Player, tech: ResearchType, target: u8) -> bool {
        p.buildings.research_lab > 0
            && p.research.get(tech) + 1 == target
            && scaling::research_prerequisites_met(tech, &p.buildings, &p.research)
            && p.power(&self.world.pace).made >= scaling::research_energy(tech, target)
    }

    fn ship_ready(&self, p: &Player, item: ShipyardItem) -> bool {
        scaling::prerequisites_met(item.prerequisites(), &p.buildings, &p.research)
    }

    /// Tell the owner what happened to an order.
    fn queue_event(&mut self, pid: u64, queue_type: iac_shared::protocol::QueueType, id: u64, item: String, action: iac_shared::protocol::QueueAction) -> iac_shared::protocol::QueueEvent {
        iac_shared::protocol::QueueEvent {
            player_id: Some(pid),
            queue_type,
            id,
            item,
            action,
            paid: Resources::default(),
            refunded: Resources::default(),
            waiting_for: None,
            waiting_on: None,
            start_in: None,
        }
    }

    fn push_queue_event(&mut self, event: iac_shared::protocol::QueueEvent) {
        self.pending_events.push(GameEvent { tick: self.current_tick, kind: EventKind::Queue(event) });
    }

    /// Report a freshly accepted order that did not start: why, the exact
    /// shortfall and the estimated start. `paid` is what the order took from
    /// the stockpile when it was given: a shipyard batch's whole price,
    /// nothing for a building or research level.
    fn announce_waiting(&mut self, pid: u64, queue_type: iac_shared::protocol::QueueType, id: u64, item: String, paid: Resources) {
        use iac_shared::protocol::{QueueAction, QueueType};
        let Some(p) = self.players.get(&pid) else { return; };
        let waits = crate::queue::project(p, self.current_tick, &self.world.pace);
        let wait = match queue_type {
            QueueType::Building => p.building_pending.iter().position(|q| q.id == id).and_then(|i| waits.buildings.get(i)).cloned(),
            QueueType::Research => p.research_pending.iter().position(|q| q.id == id).and_then(|i| waits.research.get(i)).cloned(),
            QueueType::Ship => p.ship_pending.iter().position(|q| q.id == id).and_then(|i| waits.ships.get(i)).map(|batch| {
                crate::queue::Wait { reason: batch.reason.into(), start_in: batch.start_in, short: None }
            }),
        };
        let Some(wait) = wait else { return; };
        let mut event = self.queue_event(pid, queue_type, id, item, QueueAction::Waiting);
        event.paid = paid;
        event.waiting_for = wait.short;
        event.waiting_on = Some(wait.reason);
        event.start_in = wait.start_in;
        self.push_queue_event(event);
    }

    /// Pay for and begin a building. Callers have checked `building_ready`,
    /// the slot and affordability.
    fn start_building(&mut self, pid: u64, order: PendingBuilding) {
        use iac_shared::protocol::{QueueAction, QueueType};
        let tick = self.current_tick;
        let pace = self.world.pace;
        let Some(p) = self.players.get_mut(&pid) else { return; };
        let (b, target) = (order.building_type, order.target_level);
        let cost = scaling::building_cost(b, target);
        p.resources = p.resources.sub(cost);
        let ticks = scaling::building_time(b, target, &p.buildings, &pace);
        p.building_queue.push(BuildQueueEntry { id: order.id, building_type: b, target_level: target, start_tick: tick, end_tick: tick + ticks });
        let mut event = self.queue_event(pid, QueueType::Building, order.id, format!("{} Lv.{target}", b.label()), QueueAction::Started);
        event.paid = cost;
        self.push_queue_event(event);
    }

    fn start_research(&mut self, pid: u64, order: PendingResearch) {
        use iac_shared::protocol::{QueueAction, QueueType};
        let tick = self.current_tick;
        let pace = self.world.pace;
        let Some(p) = self.players.get_mut(&pid) else { return; };
        let (tech, target) = (order.tech, order.target_level);
        let cost = scaling::research_cost(tech, target);
        p.resources = p.resources.sub(cost);
        let ticks = scaling::research_time(tech, target, &p.buildings, &pace);
        p.research_queue = Some(ResearchQueueEntry { id: order.id, tech, target_level: target, start_tick: tick, end_tick: tick + ticks });
        let mut event = self.queue_event(pid, QueueType::Research, order.id, format!("{} Lv.{target}", tech.label()), QueueAction::Started);
        event.paid = cost;
        self.push_queue_event(event);
    }

    /// Begin the first unit of a shipyard batch, which was paid for when it
    /// was ordered: `paid` is that payment when the batch starts in the same
    /// call, so the event that answers the order can say so, and nothing
    /// when it starts later. The batch then holds the yard until its last
    /// unit is delivered (`process_build_queues`); each unit shows up as
    /// `ShipBuilt` / `DefenceBuilt`.
    fn start_ship_batch(&mut self, pid: u64, order: PendingShip, paid: Resources) {
        use iac_shared::protocol::{QueueAction, QueueType};
        let tick = self.current_tick;
        let pace = self.world.pace;
        let Some(p) = self.players.get_mut(&pid) else { return; };
        let item = order.item;
        let per_unit = item.unit_ticks(&p.buildings, &pace);
        p.ship_queue = Some(ShipQueueEntry {
            id: order.id, item, count: order.count, built: 0,
            start_tick: tick, end_tick: tick + per_unit,
        });
        let mut event = self.queue_event(pid, QueueType::Ship, order.id, format!("{} x{}", item.label(), order.count), QueueAction::Started);
        event.paid = paid;
        self.push_queue_event(event);
    }

    // ── Homeworld Raids ───────────────────────────────────────────
    //
    // Forecasted pressure, never a random tax: a raid is announced a
    // warning ahead with its power and what it flies, sized by what the
    // empire has built, and fought as a battle when it arrives
    // (`begin_siege`, `siege_round`, `settle_raid`). What a lost one takes
    // is capped. Defences exist so a player can leave home without
    // babysitting it.

    fn process_raids(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tick = self.current_tick;
        let timers = RaidTimers::new(&self.world.pace);
        let player_ids: Vec<u64> = self.players.keys().copied().collect();

        for pid in player_ids {
            let state = self.raid_states.entry(pid).or_insert_with(|| RaidState {
                first_seen_tick: tick,
                next_roll_tick: tick + timers.roll_interval,
                last_raid_tick: 0,
                suppress_until: 0,
                missed_rolls: 0,
                incoming: None,
            });

            if let Some(arrival) = state.incoming.as_ref().map(|raid| raid.arrival_tick) {
                if tick >= arrival
                    && let Some(raid) = state.incoming.take()
                {
                    self.begin_siege(pid, Dock::Home, raid);
                }
                continue;
            }
            if self.besieged(pid, Dock::Home) { continue; }
            let Some(state) = self.raid_states.get_mut(&pid) else { continue; };

            if tick < state.next_roll_tick { continue; }
            state.next_roll_tick = tick + timers.roll_interval;

            let age = tick.saturating_sub(state.first_seen_tick);
            if age < timers.min_player_age { continue; }
            if tick < state.suppress_until { continue; }
            if tick.saturating_sub(state.last_raid_tick) < timers.min_interval { continue; }

            let Some(player) = self.players.get(&pid) else { continue; };
            // Raids only threaten empires worth raiding — and defenseless
            // beginners are off the menu entirely.
            let eligible = player.buildings.shipyard >= RAID_MIN_SHIPYARD
                || DefenceKind::ALL.into_iter().any(|kind| player.defences.owned(kind) > 0);
            if !eligible { continue; }

            let base = scaling::raid_power_base(scaling::econ_points(&player.buildings, &player.research));

            let mut rng = StdRng::seed_from_u64(
                tick.wrapping_mul(0xA24BAED4963EE407).wrapping_add(pid),
            );
            let missed = self.raid_states.get(&pid).map_or(0, |s| s.missed_rolls);
            let chance = (RAID_ROLL_CHANCE + RAID_PITY_STEP * missed as f32).min(1.0);
            if rng.random_range(0.0..1.0) >= chance {
                if let Some(state) = self.raid_states.get_mut(&pid) {
                    state.missed_rolls += 1;
                }
                continue;
            }

            let power = base * rng.random_range(RAID_POWER_ROLL_MIN..RAID_POWER_ROLL_MAX);
            let raid = Self::home_raid(player, power, tick + timers.warning);
            self.announce_raid(pid, Dock::Home, &raid);
            if let Some(state) = self.raid_states.get_mut(&pid) {
                state.incoming = Some(raid);
                state.missed_rolls = 0;
            }
        }

        let outposts: Vec<u64> = self.outposts.iter().filter(|(_, o)| o.level >= 1).map(|(owner, _)| *owner).collect();
        for owner in outposts {
            self.process_outpost_raid(owner, &timers);
        }

        Ok(())
    }

    /// The homeworld's raid clock, run for a finished outpost: the same
    /// rolls, pity, intervals and warning, counted from the tick level 1 was
    /// finished. The raid is sized by the ring the outpost stands in, not by
    /// what its owner is worth, and the rolls are the outpost's own.
    fn process_outpost_raid(&mut self, owner: u64, timers: &RaidTimers) {
        let tick = self.current_tick;
        let Some(outpost) = self.outposts.get_mut(&owner) else { return; };
        let sector = outpost.sector;
        let state = &mut outpost.raid;

        if let Some(arrival) = state.incoming.as_ref().map(|raid| raid.arrival_tick) {
            if tick >= arrival
                && let Some(raid) = state.incoming.take()
            {
                self.begin_siege(owner, Dock::Outpost, raid);
            }
            return;
        }
        if self.besieged(owner, Dock::Outpost) { return; }
        let Some(state) = self.outposts.get_mut(&owner).map(|outpost| &mut outpost.raid) else { return; };

        if tick < state.next_roll_tick { return; }
        state.next_roll_tick = tick + timers.roll_interval;

        if tick.saturating_sub(state.first_seen_tick) < timers.min_player_age { return; }
        if tick < state.suppress_until { return; }
        if tick.saturating_sub(state.last_raid_tick) < timers.min_interval { return; }

        let mut rng = StdRng::seed_from_u64(
            self.world_gen.world_seed
                ^ tick.wrapping_mul(0x8CB92BA72F3D8DD7)
                ^ owner.wrapping_mul(0xC2B2AE3D27D4EB4F),
        );
        let chance = (RAID_ROLL_CHANCE + RAID_PITY_STEP * state.missed_rolls as f32).min(1.0);
        if rng.random_range(0.0..1.0) >= chance {
            state.missed_rolls += 1;
            return;
        }

        let ring = sector.dist_from_origin();
        let power = scaling::outpost_raid_power(ring) * rng.random_range(RAID_POWER_ROLL_MIN..RAID_POWER_ROLL_MAX);
        let raid = outpost_raid(ring, power, tick + timers.warning);
        state.incoming = Some(raid.clone());
        state.missed_rolls = 0;
        self.announce_raid(owner, Dock::Outpost, &raid);
    }

    /// A raid is being fought at `owner`'s `dock`.
    fn besieged(&self, owner: u64, dock: Dock) -> bool {
        self.active_combats.values().any(|c| c.siege.as_ref().is_some_and(|siege| siege.owner == owner && siege.dock == dock))
    }

    /// Warn `owner` of `raid` on its way to their `dock`: when it arrives,
    /// what it flies, and how it would end against what stands there now.
    fn announce_raid(&mut self, owner: u64, dock: Dock, raid: &IncomingRaid) {
        let odds = self.raid_odds(owner, dock, &raid.ships, raid.arms);
        let threat = match odds.label {
            RaidLabel::Repelled => "light",
            RaidLabel::Held => "moderate",
            RaidLabel::AtRisk | RaidLabel::Lost => "heavy",
        };
        let outpost = self.dock_sector(owner, dock).filter(|_| dock == Dock::Outpost);
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::RaidIncoming(iac_shared::protocol::RaidIncomingEvent {
                player_id: owner,
                arrival_tick: raid.arrival_tick,
                threat: threat.to_string(),
                est_power: raid.power,
                ships: raid.ships.counts().map(|(ship_class, count)| ShipCount { ship_class, count }).collect(),
                odds: Some(odds),
                outpost,
            }),
        });
        info!("Raid forecast for player {} at {:?} ({} threat, arrives tick {})", owner, dock, threat, raid.arrival_tick);
    }

    /// What follows a raid on `siege.dock` once its fight is over, or once
    /// it has found nothing in its way (`outcome` None). The defences and
    /// the Solar Satellites that fell at home are rebuilt at OGame's rate
    /// for defences, at once; ships lost are lost. A raid that left no
    /// defender takes its share of the dock's stock above the protected
    /// floor, and no raid follows for a while; at the outpost it also
    /// breaches it, and the outpost is never destroyed. `dropped` is the
    /// wreckage the raiders left and the tick it despawns.
    fn settle_raid(
        &mut self,
        sector: Hex,
        siege: &Siege,
        outcome: Option<BattleOutcome>,
        rounds: u32,
        losses: &[BattleLosses],
        dropped: Option<(Resources, u64)>,
    ) {
        let tick = self.current_tick;
        let (owner, dock) = (siege.owner, siege.dock);
        let defended = outcome.is_some_and(|outcome| {
            !matches!(outcome, BattleOutcome::FleetsDestroyed | BattleOutcome::Fled | BattleOutcome::Withdrawn)
        });

        let mut lost: Vec<UnitCount> = Vec::new();
        let mut fallen = [0u32; DefenceKind::COUNT];
        let (mut ships_lost, mut satellites_lost) = (0u32, 0u32);
        for entry in losses.iter().filter(|entry| entry.owner.is_some()) {
            for units in &entry.lost {
                match lost.iter_mut().find(|so_far| so_far.unit == units.unit) {
                    Some(so_far) => so_far.count += units.count,
                    None => {
                        let before = lost.partition_point(|so_far| so_far.unit.index() < units.unit.index());
                        lost.insert(before, *units);
                    }
                }
                match units.unit {
                    ShipyardItem::Defence(kind) => fallen[kind as usize] += units.count,
                    ShipyardItem::Ship(_) if entry.battery => satellites_lost += units.count,
                    ShipyardItem::Ship(_) => ships_lost += units.count,
                }
            }
        }
        let raiders_lost: Vec<ShipCount> = losses.iter()
            .filter(|entry| entry.owner.is_none())
            .flat_map(|entry| &entry.lost)
            .filter_map(|units| match units.unit {
                ShipyardItem::Ship(ship_class) => Some(ShipCount { ship_class, count: units.count }),
                ShipyardItem::Defence(_) => None,
            })
            .collect();
        let rebuilt = if dock == Dock::Home { self.rebuild_at_home(owner, &fallen, satellites_lost) } else { Vec::new() };

        let (mut resources_lost, mut protected_kept) = (Resources::default(), Resources::default());
        if !defended {
            let pace = self.world.pace;
            match dock {
                Dock::Home => {
                    if let Some(player) = self.players.get_mut(&owner) {
                        let protected = scaling::storage_protected(&player.buildings, &pace);
                        let rate = Resources { metal: RAID_LOSS_CAP_METAL, crystal: RAID_LOSS_CAP_CRYSTAL, deuterium: RAID_LOSS_CAP_DEUT };
                        (player.resources, resources_lost, protected_kept) = skim(player.resources, protected, rate);
                    }
                }
                Dock::Outpost => {
                    if let Some(outpost) = self.outposts.get_mut(&owner) {
                        let rate = Resources { metal: OUTPOST_RAID_LOSS, crystal: OUTPOST_RAID_LOSS, deuterium: OUTPOST_RAID_LOSS };
                        (outpost.store, resources_lost, protected_kept) = skim(outpost.store, outpost.protected(), rate);
                        outpost.breached = true;
                        outpost.repair_warned = false;
                    }
                }
            }
        }

        let suppress = RaidTimers::new(&self.world.pace).suppress_after_loss;
        let clock = match dock {
            Dock::Home => self.raid_states.get_mut(&owner),
            Dock::Outpost => self.outposts.get_mut(&owner).map(|outpost| &mut outpost.raid),
        };
        if let Some(state) = clock {
            state.last_raid_tick = tick;
            if !defended {
                state.suppress_until = tick + suppress;
            }
        }

        let ships_of = |ships: &Ships| ships.counts().map(|(ship_class, count)| ShipCount { ship_class, count }).collect();
        self.pending_events.push(GameEvent {
            tick,
            kind: EventKind::RaidResolved(iac_shared::protocol::RaidResolvedEvent {
                player_id: owner,
                outpost: (dock == Dock::Outpost).then_some(sector),
                defended,
                outcome,
                rounds,
                breached: dock == Dock::Outpost && !defended,
                resources_lost,
                salvage_dropped: dropped.map(|(salvage, _)| salvage),
                salvage_despawn_tick: dropped.map(|(_, despawn)| despawn),
                raid_power: siege.power,
                defense_power: siege.met,
                raiders: ships_of(&siege.muster),
                raiders_lost,
                structures_lost: fallen.iter().sum(),
                structures_restored: rebuilt.iter()
                    .filter(|units| matches!(units.unit, ShipyardItem::Defence(_)))
                    .map(|units| units.count)
                    .sum(),
                lost,
                rebuilt,
                satellites_lost,
                protected_kept,
                ships_lost,
            }),
        });
        info!(
            "Raid on player {} at {} ({:?}) resolved: {} after {} round(s) (raid {:.0} met {:.0})",
            owner, sector, dock, if defended { "repelled" } else { "lost" }, rounds, siege.power, siege.met,
        );
    }

    /// OGame's rebuild of defences, which here is the rule for everything
    /// at home that is not a ship: of the defences of each kind that fell,
    /// and then of the Solar Satellites, where fewer than
    /// `DEFENCE_REBUILD_EACH_BELOW` fell each comes back with
    /// `DEFENCE_REBUILD_PCT` chances in a hundred, and where more fell a
    /// share within `DEFENCE_REBUILD_SPREAD` of it is rolled for the kind.
    /// They stand again at once and cost nothing. A dome is not rebuilt
    /// beside one ordered since. The satellites are rolled for after the
    /// last defence, so their dice move no defence's. Returns what was
    /// rebuilt, by unit, in catalogue order.
    fn rebuild_at_home(&mut self, owner: u64, fallen: &[u32; DefenceKind::COUNT], satellites_fallen: u32) -> Vec<UnitCount> {
        let mut rng = StdRng::seed_from_u64(
            self.current_tick.wrapping_mul(0xD6E8FEB86659FD93).wrapping_add(owner),
        );
        let mut roll = |fell: u32| {
            if fell < DEFENCE_REBUILD_EACH_BELOW {
                (0..fell).filter(|_| rng.random_range(0..100u32) < DEFENCE_REBUILD_PCT).count() as u32
            } else {
                let pct = rng.random_range(DEFENCE_REBUILD_PCT - DEFENCE_REBUILD_SPREAD..=DEFENCE_REBUILD_PCT + DEFENCE_REBUILD_SPREAD);
                (u64::from(fell) * u64::from(pct) / 100) as u32
            }
        };
        let mut rebuilt = Vec::new();
        for kind in DefenceKind::ALL {
            let fell = fallen[kind as usize];
            if fell == 0 { continue; }
            let mut back = roll(fell);
            let Some(player) = self.players.get_mut(&owner) else { continue; };
            if let Some(most) = kind.max_count() {
                back = back.min(most.saturating_sub(Self::defences_owned_and_ordered(player, kind)));
            }
            if back == 0 { continue; }
            player.defences.count[kind as usize] += back;
            rebuilt.push(UnitCount { unit: ShipyardItem::Defence(kind), count: back });
        }
        let satellites = roll(satellites_fallen);
        if satellites > 0
            && let Some(player) = self.players.get_mut(&owner)
        {
            player.satellites += satellites;
            rebuilt.insert(0, UnitCount { unit: ShipyardItem::Ship(ShipClass::SolarSatellite), count: satellites });
        }
        rebuilt
    }


    /// Combat power defending the homeworld: docked ships, Solar Satellites
    /// and every defence.
    pub fn home_defense_power(&self, player_id: u64) -> f32 {
        let Some(player) = self.players.get(&player_id) else { return 0.0; };
        self.docked_power(player) + Self::satellite_power(player) + player.defences.power(&player.research)
    }

    /// What the ships standing at the homeworld add to its defence.
    pub fn docked_power(&self, player: &Player) -> f32 {
        self.fleets.values()
            .filter(|fleet| fleet.owner_id == player.id && fleet.location == player.homeworld)
            .map(fleet_power)
            .sum()
    }

    /// What the Solar Satellites at home add to its defence.
    pub fn satellite_power(player: &Player) -> f32 {
        player.satellites as f32 * scaling::satellite_power(&player.research)
    }

    // ── Outposts ──────────────────────────────────────────────────

    /// The outpost standing in `sector`, with its owner.
    pub fn outpost_at(&self, sector: Hex) -> Option<(u64, &Outpost)> {
        let owner = *self.outpost_sites.get(&sector.to_key())?;
        self.outposts.get(&owner).map(|outpost| (owner, outpost))
    }

    /// `fleet` is at its owner's finished outpost.
    pub fn docked_at_outpost(&self, fleet: &Fleet) -> bool {
        self.outposts.get(&fleet.owner_id).is_some_and(|o| o.level >= 1 && o.sector == fleet.location)
    }

    /// The ore `sector` was generated with: what an extractor there reads.
    pub fn site_densities(&self, sector: Hex) -> [Density; 3] {
        let template = self.world_gen.generate_sector(sector);
        [template.metal_density, template.crystal_density, template.deut_density]
    }

    /// What the extractor of `outpost` adds to its store each tick now.
    pub fn outpost_production(&self, outpost: &Outpost) -> Resources {
        if !outpost.in_service() { return Resources::default(); }
        scaling::outpost_yield_per_tick(
            self.site_densities(outpost.sector),
            outpost.sector.dist_from_origin(),
            outpost.level,
            &self.world.pace,
        )
    }

    /// What a raid on `owner`'s outpost meets: the platform, none while
    /// breached, and the owner's ships in the sector.
    pub fn outpost_defense_power(&self, owner: u64) -> f32 {
        let Some(outpost) = self.outposts.get(&owner) else { return 0.0; };
        let platform = if outpost.breached { 0.0 } else { scaling::outpost_platform_power(outpost.level) };
        platform + self.outpost_ships_power(owner)
    }

    /// What `owner`'s ships standing at their outpost add to its defence.
    pub fn outpost_ships_power(&self, owner: u64) -> f32 {
        let Some(outpost) = self.outposts.get(&owner) else { return 0.0; };
        self.fleets.values()
            .filter(|f| f.owner_id == owner && f.location == outpost.sector)
            .map(fleet_power)
            .sum()
    }

    fn outpost_alert(&mut self, owner: u64, level: AlertLevel, sector: Hex, message: String) {
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::Alert(AlertEvent { player_id: Some(owner), level, message, sector: Some(sector), fleet_id: None }),
        });
    }

    /// Whether `fleet` may found its owner's outpost in `sector` right now.
    /// Every founding rule is checked and worded here: the survey, the
    /// command and its refusal all read this. The rules a sector can fail
    /// from afar come first, so a survey of a distant sector that is refused
    /// only for the fleet not standing there names a site that qualifies.
    pub fn outpost_site(&self, player_id: u64, fleet: &Fleet, sector: Hex) -> Result<(), Refusal> {
        let refuse = |code: ErrorCode, reason: String| Err(Refusal::new(code, reason));
        let fleet_id = fleet.id;
        let here = fleet.location == sector;
        Self::outpost_ring(fleet, sector)?;
        let template = self.world_gen.generate_sector(sector);
        if template.terrain == iac_shared::constants::TerrainType::Anomaly {
            return refuse(ErrorCode::InvalidTarget, format!("{sector} is an Anomaly; no outpost stands in one"));
        }
        if template.has_derelict {
            return refuse(ErrorCode::InvalidTarget, format!(
                "a derelict drifts at {sector}; no outpost stands beside one, stripped or not",
            ));
        }
        let crowding = self.outposts.iter()
            .filter(|(owner, _)| **owner != player_id)
            .map(|(owner, other)| (Hex::distance(&sector, &other.sector), *owner, other.sector))
            .filter(|(hexes, ..)| *hexes < OUTPOST_SPACING)
            .min_by_key(|(hexes, owner, _)| (*hexes, *owner));
        if let Some((hexes, owner, at)) = crowding {
            let whose = self.players.get(&owner).map_or("another empire", |p| p.name.as_str());
            return refuse(ErrorCode::InvalidTarget, format!(
                "{sector} is {hexes} hex{} from the outpost of {whose} at {at}; outposts stand at least \
                 {OUTPOST_SPACING} hexes apart",
                if hexes == 1 { "" } else { "es" },
            ));
        }
        if let Some(mine) = self.outposts.get(&player_id) {
            return refuse(ErrorCode::InvalidCommand, format!(
                "you already have an outpost at {}; an empire has one, and abandon_outpost gives it up",
                mine.sector,
            ));
        }
        if !here {
            let hexes = Hex::distance(&fleet.location, &sector);
            return refuse(ErrorCode::NotInSector, format!(
                "fleet {fleet_id} is at {}, {hexes} hex{} from {sector}; an outpost is founded by a fleet standing in its sector",
                fleet.location, if hexes == 1 { "" } else { "es" },
            ));
        }
        match fleet.state {
            FleetStatus::Moving => {
                return refuse(ErrorCode::OnCooldown, format!(
                    "fleet {fleet_id} is in transit; it can found an outpost once it stands idle",
                ));
            }
            FleetStatus::InCombat => {
                return refuse(ErrorCode::OnCooldown, format!(
                    "fleet {fleet_id} is in combat; an outpost is founded on a sector its founder has won",
                ));
            }
            _ if !Self::fleet_is_free(fleet) => {
                return refuse(ErrorCode::OnCooldown, format!(
                    "fleet {fleet_id} is busy; it can found an outpost once it is idle",
                ));
            }
            _ => {}
        }
        let group = self.npc_fleets.values()
            .find(|n| n.location == sector && !n.ships.is_empty())
            .map(|n| n.label.clone())
            .or_else(|| self.pending_template_npc(sector).map(|t| t.label()));
        if let Some(group) = group {
            return refuse(ErrorCode::InvalidTarget, format!(
                "a {group} holds {sector}; an outpost is founded on a sector its founder has won",
            ));
        }
        if let Some(away) = self.npc_fleets.values().find(|n| n.home_sector == sector && !n.ships.is_empty()) {
            return refuse(ErrorCode::InvalidTarget, format!(
                "the {} that holds {sector} is away at {} and will be back; an outpost is founded on a sector its founder has won",
                away.label, away.location,
            ));
        }
        if let Some(policy) = self.policies.get(&fleet_id) {
            return refuse(ErrorCode::InvalidCommand, format!(
                "fleet {fleet_id} is under standing orders ({}); set it to manual before founding an outpost",
                policy.preset.label(),
            ));
        }
        let cost = scaling::outpost_cost(1);
        let short = fleet.cargo.shortfall(cost);
        if short.total() > 0.0 {
            return refuse(ErrorCode::NoResources, format!(
                "the hold of fleet {fleet_id} lacks {} of the {} an outpost costs; it is built from what the \
                 founding fleet carries, loaded at home",
                crate::commands::amount_words(short),
                crate::commands::amount_words(cost),
            ));
        }
        Ok(())
    }

    /// The founding rule a sector's place on the map alone decides.
    fn outpost_ring(fleet: &Fleet, sector: Hex) -> Result<(), Refusal> {
        let ring = sector.dist_from_origin();
        if (OUTPOST_MIN_RING..=OUTPOST_MAX_RING).contains(&ring) {
            return Ok(());
        }
        let subject = if fleet.location == sector { format!("fleet {}", fleet.id) } else { sector.to_string() };
        Err(Refusal::new(ErrorCode::InvalidTarget, format!(
            "{subject} is at ring {ring}; an outpost stands in rings {OUTPOST_MIN_RING} to {OUTPOST_MAX_RING}",
        )))
    }

    /// Answer to `outpost_survey`: what an outpost in `sector` (the fleet's
    /// own when None) would be worth, and why it cannot be founded now. Of
    /// a sector the player has never seen it tells what its ring decides
    /// and nothing of what is there.
    pub fn outpost_survey(&self, player_id: u64, fleet_id: u64, sector: Option<Hex>) -> Result<OutpostSurvey, Refusal> {
        let fleet = self.owned_fleet(player_id, fleet_id)
            .map_err(|code| Refusal::new(code, format!("fleet {fleet_id} not found (or it has no ships left)")))?;
        let sector = sector.unwrap_or(fleet.location);
        let pace = self.world.pace;
        let ring = sector.dist_from_origin();
        let seen = sector == fleet.location || self.known.remembered(player_id, sector).is_some();
        let site = seen.then(|| {
            let densities = self.site_densities(sector);
            let [metal, crystal, deuterium] = densities;
            SurveySite {
                densities: SectorResources { metal, crystal, deuterium },
                grade: scaling::site_grade(densities),
                production: scaling::outpost_yield_per_tick(densities, ring, 1, &pace),
                hops_home: self.players.get(&player_id)
                    .and_then(|player| self.world_gen.lane_hops(sector, player.homeworld, LANE_SEARCH_HOPS)),
            }
        });
        let verdict = if seen {
            self.outpost_site(player_id, fleet, sector)
        } else {
            Self::outpost_ring(fleet, sector).and_then(|()| Err(Refusal::new(
                ErrorCode::InvalidTarget,
                format!("{sector} is not on your chart; a survey reads the site of a sector you have seen"),
            )))
        };
        Ok(OutpostSurvey {
            fleet_id,
            sector,
            ring,
            site,
            cost: scaling::outpost_cost(1),
            build_ticks: scaling::outpost_build_ticks(1, &pace),
            raid_power: outpost_raid_range(ring),
            platform_power: scaling::outpost_platform_power(1),
            platform: platform_units(1),
            raid: Self::outpost_ring(fleet, sector).is_ok().then(|| {
                let power = scaling::outpost_raid_power(ring) * RAID_POWER_ROLL_MID;
                let raid = outpost_raid(ring, power, 0);
                let platform = scaling::outpost_platform(1)
                    .iter()
                    .map(|&(kind, count)| Squad { unit: ShipyardItem::Defence(kind), count, damaged: Vec::new() })
                    .collect();
                let ours = Force::of(Arms::BASE, platform);
                RaidForecast {
                    arrival_tick: None,
                    rounds_left: None,
                    power,
                    ships: raid.ships.counts().map(|(ship_class, count)| ShipCount { ship_class, count }).collect(),
                    odds: self.odds_outlook(&ours, &Force::of(raid.arms, raid.ships.squads()), RAID_ROUNDS).raid(),
                }
            }),
            refusal: verdict.err().map(|refusal| refusal.reason),
        })
    }

    /// Found `player_id`'s outpost where `fleet_id` stands: the hold pays
    /// level 1 and the build starts.
    pub fn handle_found_outpost(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        let sector = fleet.location;
        self.outpost_site(player_id, fleet, sector).map_err(|refusal| refusal.code)?;
        let cost = scaling::outpost_cost(1);
        let tick = self.current_tick;
        let ticks = scaling::outpost_build_ticks(1, &self.world.pace);
        let fleet = self.fleets.get_mut(&fleet_id).ok_or(ErrorCode::FleetNotFound)?;
        fleet.cargo = fleet.cargo.sub(cost);
        self.outposts.insert(player_id, Outpost {
            sector,
            level: 0,
            store: Resources::default(),
            job: Some(OutpostJob { task: OutpostTask::Build { level: 1 }, start_tick: tick, end_tick: tick + ticks }),
            breached: false,
            raid: RaidState {
                first_seen_tick: tick,
                next_roll_tick: tick,
                last_raid_tick: 0,
                suppress_until: 0,
                missed_rolls: 0,
                incoming: None,
            },
            full_marks: [false; 3],
            repair_warned: false,
            dry_tanks: Vec::new(),
            founded_tick: tick,
            hauled: Resources::default(),
        });
        self.outpost_sites.insert(sector.to_key(), player_id);
        self.push_alert(AlertLevel::Info, fleet_id, sector, format!(
            "fleet {fleet_id} founded your outpost at {sector} for {}: level 1 is finished at tick {}, in {ticks} ticks. \
             Until then it has a store and nothing else",
            crate::commands::amount_words(cost),
            tick + ticks,
        ));
        Ok(())
    }

    /// Give up `player_id`'s outpost: its store drifts in the sector and
    /// the lair it displaced returns after its usual delay.
    pub fn handle_abandon_outpost(&mut self, player_id: u64) -> Result<(), ErrorCode> {
        if self.besieged(player_id, Dock::Outpost) { return Err(ErrorCode::OnCooldown); }
        let outpost = self.outposts.remove(&player_id).ok_or(ErrorCode::InvalidCommand)?;
        self.outpost_sites.remove(&outpost.sector.to_key());
        let sector = outpost.sector;
        let left = if outpost.store.total() > 0.0 {
            self.drop_salvage(sector, outpost.store);
            format!(
                "{} from its store drifts there as wreckage for {SALVAGE_DESPAWN_TICKS} ticks",
                crate::commands::amount_words(outpost.store),
            )
        } else {
            "its store was empty".to_string()
        };
        if self.world_gen.generate_sector(sector).npc_template.is_some() {
            let tick = self.current_tick;
            self.ensure_override(sector.to_key()).npc_cleared_tick = Some(tick);
        }
        self.outpost_alert(player_id, AlertLevel::Info, sector, format!(
            "your outpost at {sector} is abandoned: {left}. Nothing is refunded, and you may found another",
        ));
        Ok(())
    }

    /// The level `upgrade_outpost` would build for `player_id` and what it
    /// would take from the store, or why it is refused right now.
    pub fn outpost_upgrade(&self, player_id: u64) -> Result<(u8, Resources), Refusal> {
        let refuse = |code: ErrorCode, reason: String| Err(Refusal::new(code, reason));
        let Some(outpost) = self.outposts.get(&player_id) else {
            return refuse(ErrorCode::InvalidCommand, "you have no outpost to upgrade".to_string());
        };
        let sector = outpost.sector;
        if let Some(job) = outpost.job {
            let doing = match job.task {
                OutpostTask::Build { level } => format!("building level {level}"),
                OutpostTask::Repair => "under repair".to_string(),
            };
            return refuse(ErrorCode::InvalidCommand, format!(
                "your outpost at {sector} is {doing} until tick {}; it does one job at a time",
                job.end_tick,
            ));
        }
        if outpost.breached {
            return refuse(ErrorCode::InvalidCommand, format!(
                "your outpost at {sector} is breached; it repairs itself first, once its store covers the repair: {}",
                repair_price_words(outpost.level),
            ));
        }
        if outpost.level >= OUTPOST_MAX_LEVEL {
            return refuse(ErrorCode::MaxLevelReached, format!(
                "your outpost at {sector} is at level {OUTPOST_MAX_LEVEL}, the highest",
            ));
        }
        let next = outpost.level + 1;
        let cost = scaling::outpost_cost(next);
        let short = outpost.store.shortfall(cost);
        if short.total() > 0.0 {
            return refuse(ErrorCode::InsufficientResources, format!(
                "outpost level {next} costs {}; the store at {sector} is short by {}. \
                 A level is paid in full from the outpost's own store when it is ordered",
                crate::commands::amount_words(cost),
                crate::commands::amount_words(short),
            ));
        }
        Ok((next, cost))
    }

    /// Build the next level of `player_id`'s outpost, paid from its store.
    pub fn handle_upgrade_outpost(&mut self, player_id: u64) -> Result<(), ErrorCode> {
        let (level, cost) = self.outpost_upgrade(player_id).map_err(|refusal| refusal.code)?;
        let tick = self.current_tick;
        let ticks = scaling::outpost_build_ticks(level, &self.world.pace);
        let outpost = self.outposts.get_mut(&player_id).ok_or(ErrorCode::ServerError)?;
        outpost.store = outpost.store.sub(cost);
        outpost.job = Some(OutpostJob { task: OutpostTask::Build { level }, start_tick: tick, end_tick: tick + ticks });
        let sector = outpost.sector;
        self.outpost_alert(player_id, AlertLevel::Info, sector, format!(
            "your outpost at {sector} began level {level}, paid {} from its store: finished at tick {}, in {ticks} ticks. \
             It works at level {} until then",
            crate::commands::amount_words(cost),
            tick + ticks,
            level - 1,
        ));
        Ok(())
    }

    /// A fleet that arrived at its owner's finished outpost hands over its
    /// charts and, flown by hand, has its tank filled.
    fn check_outpost_docking(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        if !self.docked_at_outpost(fleet) { return; }
        let Some(player) = self.players.get(&fleet.owner_id) else { return; };
        let fuel_max = fleet_fuel_max(fleet, player);
        self.deliver_charts(fleet_id);
        if let Some(fleet) = self.fleets.get_mut(&fleet_id) {
            fleet.fuel_max = fuel_max;
            fleet.fuel = fleet.fuel.min(fuel_max);
        }
        self.refuel_at_outpost(fleet_id);
    }

    /// Fill the tank of a fleet standing at its owner's outpost from the
    /// store's deuterium, at the homeworld's price and as far as the store
    /// pays. A fleet under standing orders is left alone: its orders do not
    /// use the outpost. So is one charging a jump: it has left. A tank the
    /// outpost cannot fill is reported once per stay.
    fn refuel_at_outpost(&mut self, fleet_id: u64) {
        if self.policies.contains_key(&fleet_id) { return; }
        let Some(fleet) = self.fleets.get_mut(&fleet_id) else { return; };
        if fleet.state == FleetStatus::Moving { return; }
        let owner = fleet.owner_id;
        let Some(outpost) = self.outposts.get_mut(&owner).filter(|o| o.level >= 1 && o.sector == fleet.location) else {
            return;
        };
        let missing = fleet.fuel_max - fleet.fuel;
        if missing <= 0.0 { return; }
        let units = if outpost.breached { 0.0 } else { missing.min(outpost.store.deuterium / FUEL_DEUT_PER_UNIT) };
        if units > 0.0 {
            outpost.store.deuterium = (outpost.store.deuterium - units * FUEL_DEUT_PER_UNIT).max(0.0);
            fleet.fuel = if units >= missing { fleet.fuel_max } else { fleet.fuel + units };
        }
        if units >= missing || outpost.dry_tanks.contains(&fleet_id) { return; }
        outpost.dry_tanks.push(fleet_id);
        let (sector, fuel, fuel_max) = (outpost.sector, fleet.fuel, fleet.fuel_max);
        let why = if outpost.breached {
            "is breached and fills no tank until it is repaired".to_string()
        } else {
            format!("has no deuterium left for the tank of fleet {fleet_id}; unload some into its store")
        };
        self.push_alert(AlertLevel::Warning, fleet_id, sector, format!(
            "your outpost at {sector} {why}: fleet {fleet_id} stands at {} of {} fuel",
            fuel_words(fuel), fuel_words(fuel_max),
        ));
    }

    /// Each outpost's tick, in owner order: a finished job, the extractor,
    /// the repair of a breach, then the tanks and the hulls of the fleets
    /// there, then what the owner is told about the store.
    fn process_outposts(&mut self) {
        let owners: Vec<u64> = self.outposts.keys().copied().collect();
        for owner in owners {
            self.finish_outpost_job(owner);
            self.run_extractor(owner);
            self.repair_outpost(owner);
            self.fill_tanks_at_outpost(owner);
            self.mend_at_outpost(owner);
            self.note_full_store(owner);
        }
    }

    /// A breached outpost with no job under way repairs itself as soon as
    /// its store covers the repair in whatever it holds: it pays and is
    /// whole when the job ends. Until the store covers it the repair waits,
    /// and says so once.
    fn repair_outpost(&mut self, owner: u64) {
        let tick = self.current_tick;
        let pace = self.world.pace;
        let Some(outpost) = self.outposts.get_mut(&owner).filter(|o| o.breached && o.job.is_none()) else { return; };
        let sector = outpost.sector;
        let (cost, short) = scaling::outpost_repair_payment(outpost.level, outpost.store);
        if short > 0.0 {
            if outpost.repair_warned { return; }
            outpost.repair_warned = true;
            let price = repair_price_words(outpost.level);
            self.outpost_alert(owner, AlertLevel::Warning, sector, format!(
                "your outpost at {sector} is breached and cannot repair itself: the repair costs {price}, and its store \
                 is short by {} metal. It makes nothing, fills no tank and has no platform until a fleet unloads the rest",
                short.ceil(),
            ));
            return;
        }
        let ticks = scaling::outpost_repair_ticks(outpost.level, &pace);
        outpost.store = outpost.store.sub(cost);
        outpost.job = Some(OutpostJob { task: OutpostTask::Repair, start_tick: tick, end_tick: tick + ticks });
        outpost.repair_warned = false;
        self.outpost_alert(owner, AlertLevel::Info, sector, format!(
            "your outpost at {sector} began its repair, paid {} from its store: it is whole at tick {}, in {ticks} ticks",
            crate::commands::amount_words(cost),
            tick + ticks,
        ));
    }

    /// The extractor of an outpost in service adds its yield to the store,
    /// up to the caps.
    fn run_extractor(&mut self, owner: u64) {
        let Some(outpost) = self.outposts.get(&owner) else { return; };
        let made = self.outpost_production(outpost);
        if made.total() <= 0.0 { return; }
        let cap = scaling::outpost_store_cap(outpost.level, &self.world.pace);
        let store = outpost.store.add_capped(made, cap);
        self.outposts.get_mut(&owner).unwrap().store = store;
    }

    /// End the job that is due. Level 1 opens the outpost with an empty
    /// store, which is no news to the fleets that stand there: they count as
    /// told that it fills no tank, so the first thing the founder hears is
    /// not a warning. A fleet that arrives later, or thirsts there again, is
    /// warned as ever.
    fn finish_outpost_job(&mut self, owner: u64) {
        let tick = self.current_tick;
        let roll_interval = RaidTimers::new(&self.world.pace).roll_interval;
        let Some(outpost) = self.outposts.get_mut(&owner) else { return; };
        let Some(job) = outpost.job.filter(|job| tick >= job.end_tick) else { return; };
        outpost.job = None;
        let sector = outpost.sector;
        match job.task {
            OutpostTask::Build { level } => {
                outpost.level = level;
                if level == 1 {
                    outpost.raid.first_seen_tick = tick;
                    outpost.raid.next_roll_tick = tick + roll_interval;
                    outpost.dry_tanks = self.fleets.values()
                        .filter(|f| f.owner_id == owner && f.location == sector && !f.ships.is_empty())
                        .map(|f| f.id)
                        .collect();
                }
                self.pending_events.push(GameEvent {
                    tick,
                    kind: EventKind::OutpostCompleted(OutpostCompletedEvent { player_id: owner, sector, level }),
                });
            }
            OutpostTask::Repair => {
                outpost.breached = false;
                outpost.repair_warned = false;
                self.outpost_alert(owner, AlertLevel::Info, sector, format!(
                    "your outpost at {sector} is repaired: its extractor, its tanks and its platform work again",
                ));
            }
        }
    }

    fn fill_tanks_at_outpost(&mut self, owner: u64) {
        let Some(outpost) = self.outposts.get(&owner).filter(|o| o.level >= 1) else { return; };
        let sector = outpost.sector;
        let stands = |f: &Fleet| {
            f.owner_id == owner && f.location == sector && !f.ships.is_empty() && f.state != FleetStatus::Moving
        };
        let thirsty: Vec<u64> = self.fleets.values().filter(|f| stands(f) && f.fuel < f.fuel_max).map(|f| f.id).collect();
        self.outposts.get_mut(&owner).unwrap().dry_tanks.retain(|id| thirsty.contains(id));
        for fleet_id in thirsty {
            self.refuel_at_outpost(fleet_id);
        }
    }

    /// The outpost's dock mends the fleets that stand at it, in fleet
    /// order, each as far as the store then pays. It does not refit: that
    /// is the home dock's. A fleet in a fight or charging a jump is left
    /// alone, and so is one under standing orders.
    fn mend_at_outpost(&mut self, owner: u64) {
        let Some(outpost) = self.outposts.get(&owner).filter(|o| o.in_service()) else { return; };
        let sector = outpost.sector;
        let worn: Vec<u64> = self.fleets.values()
            .filter(|f| {
                f.owner_id == owner && f.location == sector && f.ships.has_damage()
                    && !matches!(f.state, FleetStatus::Moving | FleetStatus::InCombat)
            })
            .map(|f| f.id)
            .collect();
        for fleet_id in worn {
            self.mend_fleet(fleet_id, Dock::Outpost);
        }
    }

    /// One warning per resource each time the store fills with something
    /// the extractor makes: from there on that yield is lost.
    fn note_full_store(&mut self, owner: u64) {
        let Some(outpost) = self.outposts.get(&owner) else { return; };
        let cap = scaling::outpost_store_cap(outpost.level, &self.world.pace);
        let makes = self.outpost_production(outpost);
        let sector = outpost.sector;
        let mut marks = outpost.full_marks;
        let mut filled = Vec::new();
        for (i, kind) in ResourceKind::ALL.into_iter().enumerate() {
            let (stock, cap) = (outpost.store.get(kind), cap.get(kind));
            if stock >= cap && !marks[i] {
                marks[i] = true;
                if makes.get(kind) > 0.0 {
                    filled.push((kind, cap));
                }
            } else if marks[i] && stock < cap * STORE_REFILL_RATIO {
                marks[i] = false;
            }
        }
        if marks == outpost.full_marks { return; }
        self.outposts.get_mut(&owner).unwrap().full_marks = marks;
        for (kind, cap) in filled {
            self.outpost_alert(owner, AlertLevel::Warning, sector, format!(
                "your outpost at {sector} holds all the {} its store takes ({cap:.0}): the extractor makes no more \
                 of it until a fleet loads some",
                kind.label(),
            ));
        }
    }

    // ── Cooldowns ─────────────────────────────────────────────────

    fn process_cooldowns(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (_, fleet) in self.fleets.iter_mut() {
            if fleet.action_cooldown > 0 {
                fleet.action_cooldown -= 1;
            }
        }
        Ok(())
    }

    // ── Player Registration ───────────────────────────────────────

    /// Resolve an auth request to a player (see `AuthRequest` for the rules),
    /// issuing a token when the account is new or still unclaimed.
    pub fn authenticate(
        &mut self,
        name: &str,
        presented: Option<&str>,
        agent: bool,
        new_token: impl FnOnce() -> String,
    ) -> Result<Authenticated, AuthError> {
        let Some(player_id) = self.find_player_by_name(name) else {
            auth::validate_name(name).map_err(AuthError::InvalidName)?;
            let player_id = self.register_player(name.to_string()).map_err(|_| AuthError::WorldFull)?;
            if let Some(p) = self.players.get_mut(&player_id) {
                p.agent = agent;
            }
            let token = self.issue_token(player_id, new_token());
            return Ok(Authenticated { player_id, new_token: Some(token), registered: true });
        };

        match self.players[&player_id].token_hash {
            None => {
                info!("Player '{}' (id={}) claimed a token", name, player_id);
                let token = self.issue_token(player_id, new_token());
                Ok(Authenticated { player_id, new_token: Some(token), registered: false })
            }
            Some(stored) => match presented {
                None => Err(AuthError::TokenRequired),
                Some(t) if auth::token_matches(&stored, t) => {
                    info!("Player '{}' logged in (id={})", name, player_id);
                    Ok(Authenticated { player_id, new_token: None, registered: false })
                }
                Some(_) => Err(AuthError::InvalidToken),
            },
        }
    }

    fn issue_token(&mut self, player_id: u64, token: String) -> String {
        if let Some(p) = self.players.get_mut(&player_id) {
            p.token_hash = Some(auth::hash_token(&token));
        }
        token
    }

    // ── Built-in Bots ─────────────────────────────────────────────

    /// Create the account of a built-in bot: an ordinary account, registered
    /// and placed like any other, whose name carries the reserved prefix
    /// and whose `record` marks it as a bot. `token` locks the account; the
    /// host has no use for it afterwards.
    pub fn enlist_bot(&mut self, name: &str, record: BotRecord, token: String) -> Result<u64, AuthError> {
        auth::validate_bot_name(name).map_err(AuthError::InvalidName)?;
        if self.find_player_by_name(name).is_some() {
            return Err(AuthError::InvalidName("that name is taken".to_string()));
        }
        let player_id = self.register_player(name.to_string()).map_err(|_| AuthError::WorldFull)?;
        self.issue_token(player_id, token);
        self.set_bot(player_id, record);
        Ok(player_id)
    }

    /// Home seats nobody has taken: how many more accounts the world admits.
    pub fn free_home_seats(&self) -> usize {
        world::home_seats().filter(|seat| !self.players.values().any(|p| p.homeworld == *seat)).count()
    }

    pub fn bots(&self) -> &BTreeMap<u64, BotRecord> {
        &self.bots
    }

    pub fn is_bot(&self, player_id: u64) -> bool {
        self.bots.contains_key(&player_id)
    }

    /// Keep what a bot is and remembers: it is part of the world.
    pub fn set_bot(&mut self, player_id: u64, record: BotRecord) {
        self.bots.insert(player_id, record);
    }

    // ── Chart Marks ───────────────────────────────────────────────

    /// The sectors whose chart some fleet of `player_id` still carries. A
    /// fleet that lost its last ship counts until it is reaped, which is
    /// when `chart_news` tells its owner's clients.
    pub fn charts_aboard(&self, player_id: u64) -> KeySet<u32> {
        self.fleets
            .values()
            .filter(|f| f.owner_id == player_id)
            .flat_map(|f| f.charts.iter().copied())
            .collect()
    }

    /// Where `player_id`'s chart of a sector stands, given `charts_aboard`.
    pub fn chart_mark(&self, player_id: u64, sector_key: u32, aboard: &KeySet<u32>) -> Option<ChartMark> {
        if self.credited.contains(&(player_id, sector_key, ExploreCredit::Charted)) {
            Some(ChartMark::Delivered)
        } else if aboard.contains(&sector_key) {
            Some(ChartMark::Aboard)
        } else {
            None
        }
    }

    /// Case-insensitive name lookup; an exact-case match wins (see `AuthRequest`).
    fn find_player_by_name(&self, name: &str) -> Option<u64> {
        let mut folded = None;
        for p in self.players.values() {
            if p.name == name {
                return Some(p.id);
            }
            if p.name.eq_ignore_ascii_case(name) && folded.is_none_or(|id| p.id < id) {
                folded = Some(p.id);
            }
        }
        folded
    }

    /// Creates the account. The name must be unused; `authenticate` is the
    /// entry point that decides between registering and logging in. A world
    /// whose home seats are all taken refuses with `WorldFull` and is left
    /// untouched.
    pub fn register_player(&mut self, name: String) -> Result<u64, ErrorCode> {
        if self.find_player_by_name(&name).is_some() {
            return Err(ErrorCode::InvalidCommand);
        }

        let homeworld = self.next_home_seat().ok_or(ErrorCode::WorldFull)?;
        let player_id = self.next_id();

        let player = Player {
            id: player_id,
            name: name.clone(),
            resources: STARTING_RESOURCES,
            homeworld,
            buildings: BuildingLevels::default(),
            research: ResearchLevels::default(),
            building_queue: Vec::new(),
            building_pending: Vec::new(),
            ship_queue: None,
            ship_pending: Vec::new(),
            defences: Defences::default(),
            satellites: 0,
            research_queue: None,
            research_pending: Vec::new(),
            token_hash: None,
            agent: false,
            combat_points: 0.0,
            explore_points: 0.0,
            relics: 0,
            exchange_pressure: 0.0,
            exchange_tick: 0,
        };

        self.players.insert(player_id, player);

        let fleet_id = self.next_id();

        let mut fleet = Fleet {
            id: fleet_id,
            owner_id: player_id,
            location: homeworld,
            state: FleetStatus::Idle,
            ships: Ships::of(STARTING_SHIPS),
            refit: Refit::of(&ResearchLevels::default()),
            cargo: Resources::default(),
            fuel: 0.0,
            fuel_max: 0.0,
            move_cooldown: 0,
            action_cooldown: 0,
            move_target: None,
            harvest_target: HarvestResource::Auto,
            charts: Vec::new(),
            laden: false,
        };

        if let Some(player) = self.players.get(&player_id) {
            fleet.fuel_max = fleet_fuel_max(&fleet, player);
            fleet.fuel = fleet.fuel_max;
        }

        self.fleets.insert(fleet_id, fleet);

        info!("Player '{}' registered (id={}) at homeworld {}", name, player_id, homeworld);
        Ok(player_id)
    }

    // ── Command Handlers ──────────────────────────────────────────

    /// Returns the fleet only if it exists, has ships, and belongs to the
    /// commanding player — fleet IDs arrive from the wire and must never
    /// let one player steer another's ships.
    fn owned_fleet(&self, player_id: u64, fleet_id: u64) -> Result<&Fleet, ErrorCode> {
        let fleet = self.fleets.get(&fleet_id).ok_or(ErrorCode::FleetNotFound)?;
        if fleet.owner_id != player_id { return Err(ErrorCode::FleetNotFound); }
        if fleet.ships.is_empty() { return Err(ErrorCode::FleetNotFound); }
        Ok(fleet)
    }

    /// Start a jump to the connected sector `target`. From inside a fight
    /// it is flight (`flight_order`): the fleet stops firing, stays a
    /// target while the jump charges, and is out of the fight when it
    /// lands. A fleet already charging a jump takes no other.
    pub fn handle_move(&mut self, player_id: u64, fleet_id: u64, target: Hex) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if fleet.state == FleetStatus::Moving { return Err(ErrorCode::OnCooldown); }
        if fleet.state == FleetStatus::InCombat {
            if !self.world_gen.connected_neighbors(fleet.location).slice().contains(&target) {
                return Err(ErrorCode::NoConnection);
            }
            let (fuel, rounds) = self.flight_order(fleet).map_err(|refusal| refusal.code)?;
            let hop = self.hop_fuel_cost(fleet);
            let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
            fleet_mut.fuel -= fuel;
            fleet_mut.state = FleetStatus::Moving;
            fleet_mut.move_target = Some(target);
            fleet_mut.move_cooldown = rounds;
            fleet_mut.action_cooldown = rounds;
            self.warn_if_cannot_return(fleet_id, target, hop);
            return Ok(());
        }
        if fleet.action_cooldown > 0 { return Err(ErrorCode::OnCooldown); }

        if let Some(player) = self.players.get(&fleet.owner_id)
            && (fleet.location == player.homeworld || self.docked_at_outpost(fleet))
            && self.count_deployed_fleets(fleet.owner_id, player.homeworld) >= MAX_FLEETS_PER_PLAYER
        {
            return Err(ErrorCode::FleetLimitReached);
        }

        let connections = self.world_gen.connected_neighbors(fleet.location);
        if !connections.slice().contains(&target) {
            return Err(ErrorCode::NoConnection);
        }

        let research = self.players.get(&fleet.owner_id).map(|p| &p.research);
        let fuel_cost = fleet_fuel_cost(fleet, research);
        if fleet.fuel < fuel_cost { return Err(ErrorCode::InsufficientFuel); }

        // The charge: every tick of the jump but the one it lands on.
        let cooldown = fleet_jump_ticks(fleet, research) - 1;

        let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
        fleet_mut.fuel -= fuel_cost;
        fleet_mut.state = FleetStatus::Moving;
        fleet_mut.move_target = Some(target);
        fleet_mut.move_cooldown = cooldown;
        fleet_mut.action_cooldown = cooldown;

        self.warn_if_cannot_return(fleet_id, target, fuel_cost);
        Ok(())
    }

    /// What moving to the adjacent sector `target` would cost and meet; nothing moves.
    pub fn preview_move(&self, player_id: u64, fleet_id: u64, target: Hex) -> Result<MovePreview, ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if !self.world_gen.connected_neighbors(fleet.location).slice().contains(&target) {
            return Err(ErrorCode::NoConnection);
        }
        let hop = self.hop_fuel_cost(fleet);
        let flight = self.flight_quote(fleet, Some(target));
        let fuel_cost = flight.as_ref().map_or(hop, |quote| quote.fuel);
        let can_jump = fleet.fuel >= fuel_cost;
        let fuel_after = if can_jump { fleet.fuel - fuel_cost } else { 0.0 };
        let route = self.home_route(player_id, target);
        let hops_home = route.direct;
        let fuel_to_return = f64::from(hops_home) * hop;
        let odds = self.fleet_odds(fleet, target);
        Ok(MovePreview {
            fleet_id,
            target,
            fuel_cost,
            fuel_after,
            can_jump,
            hops_home,
            charted_hops_home: route.charted,
            route_unexplored: route.unexplored,
            fuel_to_return,
            can_return: can_jump && fuel_after >= fuel_to_return,
            threat: odds.threat,
            fleet_power: fleet_power(fleet),
            ratio: odds.ratio,
            label: odds.label,
            flight,
        })
    }

    /// The fuel a jump out of the fight `fleet` is in burns and the rounds
    /// it charges for, under fire and silent: one fewer than its jump takes
    /// ticks and never fewer than `FLEE_MIN_ROUNDS`. Err says why the order
    /// is refused. Every flight goes through here: the order, its refusal,
    /// the quote in the state and the standing orders' disengage rule.
    pub fn flight_order(&self, fleet: &Fleet) -> Result<(f64, u16), Refusal> {
        let (fleet_id, at) = (fleet.id, fleet.location);
        match (fleet.state, fleet.move_target) {
            (FleetStatus::InCombat, _) => {}
            (FleetStatus::Moving, Some(target)) if self.combat_of(fleet_id).is_some() => {
                return Err(Refusal::new(ErrorCode::OnCooldown, format!(
                    "fleet {fleet_id} is already leaving the fight at {at} for {target}; stop calls the jump off, and its fuel is forfeit",
                )));
            }
            _ => {
                return Err(Refusal::new(ErrorCode::InvalidCommand, format!("fleet {fleet_id} is in no fight to leave")));
            }
        }
        if let Some(player) = self.players.get(&fleet.owner_id)
            && (fleet.location == player.homeworld || self.docked_at_outpost(fleet))
            && self.count_deployed_fleets(fleet.owner_id, player.homeworld) >= MAX_FLEETS_PER_PLAYER
        {
            return Err(Refusal::new(ErrorCode::FleetLimitReached, format!(
                "fleet {fleet_id} cannot leave the dock at {at}: {MAX_FLEETS_PER_PLAYER} of your fleets are away from \
                 your docks already, the most an empire may have there",
            )));
        }
        let fuel = self.hop_fuel_cost(fleet) * FLEE_FUEL_MULTIPLIER;
        if fleet.fuel < fuel {
            return Err(Refusal::new(ErrorCode::InsufficientFuel, format!(
                "fleet {fleet_id} has {} fuel and the jump out of the fight at {at} burns {}: it cannot leave, \
                 except by recall if it holds the fuel for that",
                fuel_words(fleet.fuel), fuel_words(fuel),
            )));
        }
        Ok((fuel, self.hop_ticks(fleet).saturating_sub(1).max(FLEE_MIN_ROUNDS)))
    }

    /// The odds of `combat` as it stands: its fleets that are firing against
    /// every group in it, each with the losses and the damage it has taken.
    fn fight_ratio(&self, combat: &Combat) -> f32 {
        if let Some(outlook) = self.siege_outlook(combat) {
            return outlook.ratio();
        }
        let mut fleet_ids = combat.player_fleet_ids.clone();
        fleet_ids.sort_unstable();
        let mut ours = Force::default();
        for fleet in fleet_ids.iter().filter_map(|id| self.fleets.get(id)).filter(|f| f.state != FleetStatus::Moving) {
            ours.join(&fleet_force(fleet));
        }
        let mut group_ids = combat.npc_fleet_ids.clone();
        group_ids.sort_unstable();
        let mut theirs = Force::default();
        for group in group_ids.iter().filter_map(|id| self.npc_fleets.get(id)) {
            theirs.join(&Force::of(group.arms, group.ships.squads()));
        }
        self.odds_ratio(&ours, &theirs)
    }

    /// How the raid fought in `combat` would end from where it stands: its
    /// fleets that are firing and its battery against the raiders, over the
    /// rounds that are left. None for a fight that is not a raid.
    fn siege_outlook(&self, combat: &Combat) -> Option<odds::Outlook> {
        let siege = combat.siege.as_ref()?;
        let mut ours = Force::default();
        for fleet in self.fleets_fighting(combat).iter().filter_map(|id| self.fleets.get(id)).filter(|f| f.state != FleetStatus::Moving) {
            ours.join(&fleet_force(fleet));
        }
        ours.join(&Force::of(self.battery_arms(siege.owner, siege.dock), siege.battery.clone()));
        let theirs = Force::of(siege.arms, siege.raiders.squads());
        Some(self.odds_outlook(&ours, &theirs, RAID_ROUNDS.saturating_sub(combat.round).max(1)))
    }

    /// What a `move` out of the fight `fleet` is in would cost right now,
    /// with the group that would follow it, to `target` if one is named.
    /// None for a fleet that is in no fight or is already leaving one.
    pub fn flight_quote(&self, fleet: &Fleet, target: Option<Hex>) -> Option<FlightQuote> {
        if fleet.state != FleetStatus::InCombat { return None; }
        let combat = self.combat_of(fleet.id)?;
        let rounds = self.hop_ticks(fleet).saturating_sub(1).max(FLEE_MIN_ROUNDS);
        let ratio = self.fight_ratio(combat);
        let ticks = self.hop_ticks(fleet);
        let power = fleet_power(fleet);
        let followed = !target.is_some_and(|to| self.is_refuge(to));
        let chase = combat.npc_fleet_ids.iter()
            .filter_map(|id| self.npc_fleets.get(id))
            .filter(|group| followed && !group.ships.is_empty() && group.leash() > 0)
            .filter(|group| group.ships.power(group.arms) >= CHASE_MIN_RATIO * power)
            .map(|group| ChaseForecast {
                group_id: group.id,
                label: group.label.clone(),
                hexes: group.leash(),
                jump_ticks: group.jump_ticks(),
                rounds_per_hex: ticks.saturating_sub(group.jump_ticks()),
            })
            .max_by_key(|chase| (chase.rounds_per_hex, chase.hexes, std::cmp::Reverse(chase.group_id)));
        Some(FlightQuote {
            fuel: self.hop_fuel_cost(fleet) * FLEE_FUEL_MULTIPLIER,
            ticks: rounds.saturating_add(1),
            rounds,
            ratio,
            label: scaling::ratio_label(ratio),
            refusal: self.flight_order(fleet).err().map(|refusal| refusal.reason),
            chase,
        })
    }

    /// The jump out of a fight `fleet` is charging, if it is.
    pub fn fleeing(&self, fleet: &Fleet) -> Option<Fleeing> {
        let to = fleet.move_target.filter(|_| fleet.state == FleetStatus::Moving)?;
        self.combat_of(fleet.id)?;
        Some(Fleeing { to, ticks: fleet.move_cooldown + 1, rounds: fleet.move_cooldown })
    }

    /// The group on the trail of `fleet`, if one is: the first to land.
    pub fn pursuit_of(&self, fleet: &Fleet) -> Option<Pursuit> {
        let tick = self.current_tick;
        let (group, errand) = self.npc_fleets.values()
            .filter(|group| !group.in_combat)
            .filter_map(|group| group.errand.filter(|e| e.quarry == Some(fleet.id)).map(|errand| (group, errand)))
            .min_by_key(|(group, errand)| (errand.arrives, group.id))?;
        let arrives_in = errand.arrives.saturating_sub(tick).min(u64::from(u16::MAX)) as u16;
        let gone_in = if fleet.state == FleetStatus::Moving { fleet.move_cooldown + 1 } else { self.hop_ticks(fleet) };
        Some(Pursuit {
            group_id: group.id,
            label: group.label.clone(),
            from: group.location,
            arrives_in,
            catches: errand.to == fleet.location && arrives_in < gone_in,
            hexes_left: group.leash(),
        })
    }

    /// How `fleet` would fare in `coord`, judged on what its owner knows of
    /// the sector: the one number previews, `odds` replies and standing
    /// orders share.
    pub fn fleet_odds(&self, fleet: &Fleet, coord: Hex) -> SectorOdds {
        let (threat, theirs) = self.known_opposition(fleet.owner_id, coord);
        let ratio = self.odds_ratio(&fleet_force(fleet), &theirs);
        SectorOdds { sector: coord, threat, ratio, label: scaling::ratio_label(ratio) }
    }

    /// Answer to an `odds` request.
    pub fn odds_for(&self, player_id: u64, fleet_id: u64, sectors: &[Hex]) -> Result<Vec<SectorOdds>, ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if sectors.len() > iac_shared::protocol::MAX_ODDS_SECTORS {
            return Err(ErrorCode::InvalidCommand);
        }
        let ours = fleet_force(fleet);
        let rings = RingOdds::default();
        let live: KeySet<u32> = crate::intel::live_coords(self, player_id).iter().map(|h| h.to_key()).collect();
        Ok(sectors
            .iter()
            .map(|&coord| {
                let (threat, theirs) = self.opposition(player_id, coord, live.contains(&coord.to_key()));
                let ratio = self.ratio_against(&ours, theirs, &rings);
                SectorOdds { sector: coord, threat, ratio, label: scaling::ratio_label(ratio) }
            })
            .collect())
    }

    /// `odds_ratio` of `ours` against `theirs`. `rings` must be `ours`'s own.
    fn ratio_against(&self, ours: &Force, theirs: Opposing, rings: &RingOdds) -> f32 {
        let ring = theirs.ring();
        if let Some(dist) = ring
            && let Some(&ratio) = rings.0.borrow().get(&dist)
        {
            return ratio;
        }
        let ratio = self.odds_ratio(ours, &theirs.force());
        if let Some(dist) = ring {
            rings.0.borrow_mut().insert(dist, ratio);
        }
        ratio
    }

    /// The ratio of `ours` against `theirs` in a fight in open space.
    pub fn odds_ratio(&self, ours: &Force, theirs: &Force) -> f32 {
        self.odds_of(ours, theirs, ODDS_ROUNDS).1
    }

    /// What a fight of `ours` against `theirs` over at most `rounds` rounds
    /// is expected to leave of each: a raid's reading of it.
    pub fn odds_outlook(&self, ours: &Force, theirs: &Force, rounds: u32) -> odds::Outlook {
        self.odds_of(ours, theirs, rounds).0
    }

    /// The fight itself, played forward (`odds::outlook_within`), with the
    /// ratio it comes to, and remembered by the two sides and the rounds.
    fn odds_of(&self, ours: &Force, theirs: &Force, rounds: u32) -> (odds::Outlook, f32) {
        let key = (ours.clone(), theirs.clone(), rounds);
        if let Some(&known) = self.odds_memo.borrow().get(&key) {
            return known;
        }
        let outlook = odds::outlook_within(ours, theirs, rounds);
        let known = (outlook, outlook.ratio());
        let mut memo = self.odds_memo.borrow_mut();
        if memo.len() >= ODDS_MEMO_LIMIT {
            memo.clear();
        }
        memo.insert(key, known);
        known
    }

    /// What the ore tiles of `coord` hold now; None when the sector has no ore.
    pub fn ore_reserve_at(&self, coord: Hex) -> Option<OreReserve> {
        let template = self.world_gen.generate_sector(coord);
        self.ore_reserve_in(coord, &template, self.sector_overrides.get(&coord.to_key()))
    }

    /// `ore_reserve_at` for a caller that holds the sector's template and override.
    pub(crate) fn ore_reserve_in(
        &self,
        coord: Hex,
        template: &world::SectorTemplate,
        ov: Option<&SectorOverride>,
    ) -> Option<OreReserve> {
        let dist = coord.dist_from_origin();
        let tile = |template_d: Density, max_units: f64, current: Option<Density>, harvested: f64| {
            let gone = scaling::ore_extracted(template_d, current.unwrap_or(template_d), harvested, dist);
            let refills_in_s = (gone > scaling::ORE_EPSILON)
                .then(|| scaling::ore_regen_per_tick(template_d, dist, &self.world.pace))
                .filter(|rate| *rate > 0.0)
                .map(|rate| (gone / rate).ceil() as u64);
            TileReserve { units: (max_units - gone).max(0.0), max_units, refills_in_s }
        };
        let [metal, crystal, deuterium] = template.ore_units;
        let metal = tile(template.metal_density, metal, ov.and_then(|o| o.metal_density), ov.map_or(0.0, |o| o.metal_harvested));
        let crystal = tile(template.crystal_density, crystal, ov.and_then(|o| o.crystal_density), ov.map_or(0.0, |o| o.crystal_harvested));
        let deuterium = tile(template.deut_density, deuterium, ov.and_then(|o| o.deut_density), ov.map_or(0.0, |o| o.deut_harvested));
        (metal.max_units + crystal.max_units + deuterium.max_units > 0.0)
            .then_some(OreReserve { metal, crystal, deuterium })
    }

    /// The threat of `coord` as this player knows it: the truth while the
    /// sector is live for them, the rating last seen if charted, otherwise
    /// the ring's estimate.
    pub fn known_threat(&self, player_id: u64, coord: Hex) -> ThreatInfo {
        self.known_opposition(player_id, coord).0
    }

    /// `known_threat` with the ships a fight there would meet: those in
    /// sight, those remembered at the strength last seen, otherwise the
    /// group the ring spawns.
    pub fn known_opposition(&self, player_id: u64, coord: Hex) -> (ThreatInfo, Force) {
        let live = crate::intel::live_coords(self, player_id).contains(&coord);
        let (threat, theirs) = self.opposition(player_id, coord, live);
        (threat, theirs.force())
    }

    fn opposition(&self, player_id: u64, coord: Hex, live: bool) -> (ThreatInfo, Opposing) {
        if live {
            return (self.sector_threat(coord), self.sector_opposing(coord));
        }
        match self.known.remembered(player_id, coord) {
            Some(seen) => (seen.threat, remembered_opposing(seen, self.current_tick)),
            None => (ring_threat(coord, ThreatBasis::Estimate), Opposing::Ring(coord.dist_from_origin())),
        }
    }

    /// The threat of `coord` right now. A group on the spot (or its template
    /// before it materialises) is rated by its own power; otherwise the ring's.
    pub fn sector_threat(&self, coord: Hex) -> ThreatInfo {
        let template = self.world_gen.generate_sector(coord);
        let pending = self.pending_template_in(coord, &template, self.sector_overrides.get(&coord.to_key()));
        let here = self.npc_fleets.values().filter(|n| n.location == coord);
        self.threat_among(coord, &template, pending.as_ref(), here)
    }

    /// The ships a fight in `coord` would meet right now: the groups on the
    /// spot, the lair's group before it materialises, otherwise the ring's.
    pub fn sector_force(&self, coord: Hex) -> Force {
        self.sector_opposing(coord).force()
    }

    fn sector_opposing(&self, coord: Hex) -> Opposing {
        let mut force = Force::default();
        for npc in self.npc_fleets.values().filter(|n| n.location == coord) {
            force.join(&Force::of(npc.arms, npc.ships.squads()));
        }
        if !force.is_empty() || self.outpost_at(coord).is_some() {
            return Opposing::Ships(force);
        }
        match self.pending_template_npc(coord) {
            Some(tmpl) => {
                Opposing::Ships(Force::of_group(tmpl.ship_class, u32::from(tmpl.count), tmpl.stat_multiplier))
            }
            None => Opposing::Ring(coord.dist_from_origin()),
        }
    }

    /// `sector_threat` for a caller that already knows which groups stand in
    /// `coord` and holds its template and what `pending_template_npc` says.
    pub(crate) fn threat_among<'a>(
        &self,
        coord: Hex,
        template: &world::SectorTemplate,
        pending: Option<&world::NpcTemplate>,
        npcs: impl Iterator<Item = &'a NpcFleet>,
    ) -> ThreatInfo {
        let mut observed: Option<f32> = None;
        for npc in npcs.filter(|n| !n.ships.is_empty()) {
            *observed.get_or_insert(0.0) += npc.ships.power(npc.arms) * behavior_threat_bonus(npc.behavior);
        }
        if observed.is_none()
            && let Some(tmpl) = pending
        {
            observed = Some(tmpl.power() * behavior_threat_bonus(tmpl.behavior));
        }
        if observed.is_none() && self.outpost_at(coord).is_some() {
            return ThreatInfo { rating: 0, est_power: 0.0, basis: ThreatBasis::Observed };
        }
        match observed {
            Some(est_power) => ThreatInfo { rating: scaling::threat_rating(est_power), est_power, basis: ThreatBasis::Observed },
            None => {
                let lair = template.npc_template.is_some();
                ring_threat(coord, if lair { ThreatBasis::Template } else { ThreatBasis::Estimate })
            }
        }
    }

    /// Fuel for one jump of this fleet.
    pub fn hop_fuel_cost(&self, fleet: &Fleet) -> f64 {
        let research = self.players.get(&fleet.owner_id).map(|p| &p.research);
        fleet_fuel_cost(fleet, research)
    }

    /// Ticks from a `move` order to the arrival: the charge `handle_move`
    /// sets, and the tick `process_movement` lands the fleet on.
    pub fn hop_ticks(&self, fleet: &Fleet) -> u16 {
        let research = self.players.get(&fleet.owner_id).map(|p| &p.research);
        fleet_jump_ticks(fleet, research)
    }

    /// Hops a scan by `fleet` reveals.
    pub fn scan_range(fleet: &Fleet) -> u8 {
        if fleet.ships.has(ShipClass::EspionageProbe) { SCAN_PROBE_RANGE } else { SCAN_BASE_RANGE }
    }

    /// The ways home from `from`. `direct` is the fewest hops over the real
    /// lanes, charted or not, and is what a fleet can actually fly; `charted`
    /// is the fewest over sectors the owner has entered, None when no such
    /// route exists. `unexplored` says whether the direct route crosses a
    /// sector the owner has never entered.
    pub fn home_route(&self, owner: u64, from: Hex) -> HomeRoute {
        let Some(player) = self.players.get(&owner) else {
            return HomeRoute { charted: None, direct: 0, unexplored: false };
        };
        let home = player.homeworld;
        if from == home {
            return HomeRoute { charted: Some(0), direct: 0, unexplored: false };
        }
        let entered = |n: Hex| self.explored.contains(&(owner, n.to_key()));
        let direct = self.lane_path(from, home, LANE_SEARCH_HOPS, &|_| true);
        let charted = self.lane_path(from, home, LANE_SEARCH_HOPS, &entered).map(|p| p.len() as u32);
        match direct {
            Some(path) => HomeRoute {
                charted,
                direct: path.len() as u32,
                unexplored: path.iter().take(path.len() - 1).any(|&n| !entered(n)),
            },
            None => HomeRoute { charted, direct: Hex::distance(&from, &home) as u32, unexplored: true },
        }
    }

    /// Shortest lane path from `from` to `to` (excluding `from`, including
    /// `to`), stepping only through sectors `passable` accepts. `to` itself
    /// need not pass.
    fn lane_path(&self, from: Hex, to: Hex, max_hops: u8, passable: &dyn Fn(Hex) -> bool) -> Option<Vec<Hex>> {
        self.bfs_path(from, max_hops, &|n| n == to || passable(n), &|n| n == to)
    }

    /// `home_route(owner, from).direct`, from the lane distances the world
    /// generator remembers: views and autopilots ask this every tick.
    pub(crate) fn direct_hops_home(&self, owner: u64, from: Hex) -> u32 {
        let Some(home) = self.players.get(&owner).map(|p| p.homeworld) else { return 0; };
        self.lane_hops(from, home)
    }

    /// Fewest jumps from `from` to `to` over the real lanes; the straight
    /// distance when no route of `LANE_SEARCH_HOPS` exists.
    fn lane_hops(&self, from: Hex, to: Hex) -> u32 {
        self.world_gen.lane_hops(from, to, LANE_SEARCH_HOPS).unwrap_or(Hex::distance(&from, &to) as u32)
    }

    /// Fuel the way from `from` to the fleet's owner's outpost costs, while
    /// that outpost is in service.
    pub fn outpost_route_cost(&self, fleet: &Fleet, from: Hex) -> Option<f64> {
        let outpost = self.outposts.get(&fleet.owner_id).filter(|o| o.in_service())?;
        Some(f64::from(self.lane_hops(from, outpost.sector)) * self.hop_fuel_cost(fleet))
    }

    /// Fuel the fleet would burn getting home from `from`, one jump per hop.
    pub fn route_home_cost(&self, fleet: &Fleet, from: Hex) -> f64 {
        f64::from(self.direct_hops_home(fleet.owner_id, from)) * self.hop_fuel_cost(fleet)
    }

    fn push_alert(&mut self, level: AlertLevel, fleet_id: u64, sector: Hex, message: String) {
        let player_id = self.fleets.get(&fleet_id).map(|f| f.owner_id);
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::Alert(AlertEvent {
                player_id,
                level,
                message,
                sector: Some(sector),
                fleet_id: Some(fleet_id),
            }),
        });
    }

    /// A jump that leaves the fleet unable to afford the way home is
    /// allowed, but never silent. "The way home" is the shortest real route,
    /// charted or not; a longer charted route is quoted alongside.
    fn warn_if_cannot_return(&mut self, fleet_id: u64, target: Hex, hop_cost: f64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let Some(player) = self.players.get(&fleet.owner_id) else { return; };
        if target == player.homeworld { return; }
        let have = fleet.fuel;
        if have + 0.001 >= f64::from(self.direct_hops_home(fleet.owner_id, target)) * hop_cost { return; }
        if self.outposts.get(&fleet.owner_id)
            .is_some_and(|o| o.in_service() && have + 0.001 >= f64::from(self.lane_hops(target, o.sector)) * hop_cost)
        {
            return;
        }
        let route = self.home_route(fleet.owner_id, target);
        let need = f64::from(route.direct) * hop_cost;
        let (level, outcome) = if have + 0.001 < hop_cost {
            (AlertLevel::Critical, "it will be stranded on arrival; the emergency reserve recovers one hop in about 5 minutes")
        } else {
            (AlertLevel::Warning, "it cannot reach home without help")
        };
        let across = if route.unexplored { ", across unexplored space" } else { "" };
        let way = match route.charted {
            Some(c) if c != route.direct => format!(
                "the charted route home is {c} hops, {:.0} fuel; the direct route is {} hops, {need:.0} fuel{across}",
                f64::from(c) * hop_cost, route.direct,
            ),
            None => format!("no charted route home; the direct route is {} hops, {need:.0} fuel{across}", route.direct),
            _ => format!("the way home is {} hops, {need:.0} fuel{across}", route.direct),
        };
        let message = format!("fleet {fleet_id} jumping to {target} with {have:.0} fuel after the jump; {way}: {outcome}");
        self.push_alert(level, fleet_id, target, message);
    }

    pub fn handle_harvest(&mut self, player_id: u64, fleet_id: u64, resource: HarvestResource) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if fleet.state == FleetStatus::InCombat { return Err(ErrorCode::OnCooldown); }
        if fleet.state == FleetStatus::Moving { return Err(ErrorCode::OnCooldown); }
        if fleet.action_cooldown > 0 { return Err(ErrorCode::OnCooldown); }

        let template = self.world_gen.generate_sector(fleet.location);
        let ov = self.sector_overrides.get(&fleet.location.to_key());
        let (metal_d, crystal_d, deut_d) = SectorOverride::effective_densities(ov, &template);
        if metal_d == Density::None && crystal_d == Density::None && deut_d == Density::None {
            return Err(ErrorCode::NoResources);
        }
        let present = match resource {
            HarvestResource::Metal => metal_d != Density::None,
            HarvestResource::Crystal => crystal_d != Density::None,
            HarvestResource::Deuterium => deut_d != Density::None,
            HarvestResource::Auto => true,
        };
        if !present { return Err(ErrorCode::ResourceNotPresent); }

        let max_cargo = fleet_cargo_capacity(fleet, &self.world.pace);
        let current_cargo = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
        if current_cargo >= max_cargo { return Err(ErrorCode::CargoFull); }

        let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
        fleet_mut.state = FleetStatus::Harvesting;
        fleet_mut.harvest_target = resource;
        fleet_mut.action_cooldown = HARVEST_COOLDOWN;

        Ok(())
    }

    pub fn handle_collect_salvage(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if fleet.state == FleetStatus::InCombat { return Err(ErrorCode::OnCooldown); }
        if fleet.state == FleetStatus::Moving { return Err(ErrorCode::OnCooldown); }

        let key = fleet.location.to_key();
        let ov = self.sector_overrides.get(&key).ok_or(ErrorCode::NoResources)?;
        if ov.salvage.is_none() { return Err(ErrorCode::NoResources); }
        let used = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
        if used >= fleet_cargo_capacity(fleet, &self.world.pace) { return Err(ErrorCode::CargoFull); }

        self.collect_salvage(fleet_id).map_err(|_| ErrorCode::ServerError)?;

        Ok(())
    }

    pub fn handle_attack(&mut self, player_id: u64, fleet_id: u64, target_fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if fleet.state == FleetStatus::InCombat { return Err(ErrorCode::OnCooldown); }
        if fleet.state == FleetStatus::Moving { return Err(ErrorCode::OnCooldown); }

        // Check if NPC exists at this location
        if let Some(npc) = self.npc_fleets.get(&target_fleet_id) {
            if npc.location != fleet.location { return Err(ErrorCode::InvalidTarget); }
            if npc.in_combat { return Err(ErrorCode::OnCooldown); }
            return self.start_combat(fleet_id, target_fleet_id).map_err(|_| ErrorCode::ServerError);
        }

        // The sector's own hostile, listed in the sector view before it materialises
        let location = fleet.location;
        if target_fleet_id == template_npc_id(location)
            && let Some(npc_tmpl) = self.pending_template_npc(location)
        {
            let spawned = self.spawn_npc_fleet(location, npc_tmpl, target_fleet_id)
                .map_err(|_| ErrorCode::ServerError)?;
            return self.start_combat(fleet_id, spawned.id).map_err(|_| ErrorCode::ServerError);
        }

        Err(ErrorCode::InvalidTarget)
    }

    /// The exact reading, span and window of `player`'s exchange meter now.
    fn exchange_standing(&self, player: &Player) -> (f64, f64, u64) {
        let pace = self.world.pace;
        let span = scaling::exchange_span(&player.mined_per_tick(&pace), &pace);
        let window = scaling::exchange_window(&pace);
        let elapsed = self.current_tick.saturating_sub(player.exchange_tick);
        (scaling::exchange_pressure_after(player.exchange_pressure, elapsed, span, window), span, window)
    }

    /// Where `player`'s meter at the hub's exchange stands this tick.
    pub fn exchange_meter(&self, player: &Player) -> ExchangeMeter {
        let (pressure, span, window) = self.exchange_standing(player);
        exchange_meter_at(pressure, span, window)
    }

    /// What the exchange would give `fleet_id` for `amount` of `give` (all it
    /// carries when None), paid in `want`; nothing changes. `Err` when there
    /// is no trade to work out; a trade that can be worked out but not made
    /// right now carries the reason in `refusal`.
    pub fn exchange_deal(
        &self,
        player_id: u64,
        fleet_id: u64,
        give: ResourceKind,
        want: ResourceKind,
        amount: Option<f64>,
    ) -> Result<ExchangeDeal, Refusal> {
        let refuse = Refusal::new;
        let fleet = self.owned_fleet(player_id, fleet_id)
            .map_err(|code| refuse(code, format!("fleet {fleet_id} not found (or it has no ships left)")))?;
        let player = self.players.get(&player_id)
            .ok_or_else(|| refuse(ErrorCode::ServerError, "internal server error".to_string()))?;
        if give == want {
            return Err(refuse(
                ErrorCode::InvalidCommand,
                format!("the exchange trades one resource for another; give and want are both {}", give.label()),
            ));
        }
        let held = fleet.cargo.get(give);
        let amount = match amount {
            Some(asked) if asked.is_finite() && asked > 0.0 => asked,
            Some(_) => {
                return Err(refuse(ErrorCode::InvalidCommand, "the amount to trade must be a positive number".to_string()));
            }
            None if held > 0.0 => held,
            None => {
                return Err(refuse(ErrorCode::NoResources, format!(
                    "fleet {fleet_id} holds no {} to trade (its hold: {:.0} metal, {:.0} crystal, {:.0} deuterium)",
                    give.label(), fleet.cargo.metal, fleet.cargo.crystal, fleet.cargo.deuterium,
                )));
            }
        };

        let (pressure, span, window) = self.exchange_standing(player);
        let value = amount * give.weight();
        let par = value / want.weight();
        let receive = (scaling::exchange_return(pressure, value, span) / want.weight()).floor().max(0.0);
        let terms = ExchangeTerms {
            fleet_id,
            give,
            want,
            amount,
            receive,
            par,
            fee: 1.0 - receive / par,
            meter: exchange_meter_at(pressure, span, window),
            meter_after: exchange_meter_at(pressure + value, span, window),
        };

        let capacity = fleet_cargo_capacity(fleet, &self.world.pace);
        let aboard = fleet.cargo.total();
        let after = aboard - amount + terms.receive;
        let refusal = if fleet.state == FleetStatus::Moving {
            Some(refuse(ErrorCode::OnCooldown, format!("fleet {fleet_id} is in transit; it can trade once it has arrived")))
        } else if !world::hub_services(fleet.location).contains(&HubService::Exchange) {
            Some(refuse(ErrorCode::NotInSector, match world::nearest_hub(fleet.location, HubService::Exchange) {
                Some(hub) => {
                    let hexes = Hex::distance(&fleet.location, &hub);
                    format!(
                        "fleet {fleet_id} is at {}, {hexes} hex{} from the exchange at the hub {hub}; \
                         a fleet trades what is in its hold, on the spot",
                        fleet.location, if hexes == 1 { "" } else { "es" },
                    )
                }
                None => "this world has no exchange".to_string(),
            }))
        } else if !Self::fleet_is_free(fleet) {
            Some(refuse(ErrorCode::OnCooldown, format!("fleet {fleet_id} is busy; it can trade once it is idle")))
        } else if amount > held {
            Some(refuse(ErrorCode::NoResources, format!(
                "fleet {fleet_id} holds {held:.1} {}; it cannot give {amount:.1}", give.label(),
            )))
        } else if receive < 1.0 {
            Some(refuse(ErrorCode::InvalidCommand, format!(
                "{amount:.1} {} is too little to trade: at a fee of {:.0}% it returns less than one {}",
                give.label(), terms.meter.fee * 100.0, want.label(),
            )))
        } else if terms.receive > amount && after > capacity + 0.01 {
            Some(refuse(ErrorCode::CargoFull, format!(
                "{amount:.0} {} returns {receive:.0} {}, and the hold of fleet {fleet_id} has no room for it: \
                 it carries {aboard:.0} of {capacity:.0} and would carry {after:.0}; trade a smaller amount",
                give.label(), want.label(),
            )))
        } else {
            None
        };
        Ok(ExchangeDeal { terms, pressure_after: pressure + value, refusal })
    }

    /// Trade at the hub's exchange: the fleet's hold and its owner's meter
    /// change, nothing else does.
    pub fn handle_exchange(
        &mut self,
        player_id: u64,
        fleet_id: u64,
        give: ResourceKind,
        want: ResourceKind,
        amount: Option<f64>,
    ) -> Result<(), ErrorCode> {
        let (terms, pressure_after) = match self.exchange_deal(player_id, fleet_id, give, want, amount) {
            Ok(ExchangeDeal { terms, pressure_after, refusal: None }) => (terms, pressure_after),
            Ok(ExchangeDeal { refusal: Some(refusal), .. }) | Err(refusal) => return Err(refusal.code),
        };
        let fleet = self.fleets.get_mut(&fleet_id).ok_or(ErrorCode::FleetNotFound)?;
        let left = if amount.is_some() { (fleet.cargo.get(give) - terms.amount).max(0.0) } else { 0.0 };
        *fleet.cargo.get_mut(give) = left;
        *fleet.cargo.get_mut(want) += terms.receive;
        let sector = fleet.location;
        let player = self.players.get_mut(&player_id).ok_or(ErrorCode::ServerError)?;
        player.exchange_pressure = pressure_after;
        player.exchange_tick = self.current_tick;
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::Exchanged(ExchangedEvent { sector, terms }),
        });
        Ok(())
    }

    /// `fleet_id` standing free at a dock of its owner's, the homeworld or
    /// the outpost: the places a hold is loaded or unloaded by hand. `verb`
    /// words the refusal.
    fn fleet_at_dock(&self, player_id: u64, fleet_id: u64, verb: &str) -> Result<(&Fleet, &Player, Dock), Refusal> {
        let fleet = self.owned_fleet(player_id, fleet_id)
            .map_err(|code| Refusal::new(code, format!("fleet {fleet_id} not found (or it has no ships left)")))?;
        let player = self.players.get(&player_id)
            .ok_or_else(|| Refusal::new(ErrorCode::ServerError, "internal server error".to_string()))?;
        let outpost = self.outposts.get(&player_id).map(|o| o.sector);
        if fleet.state == FleetStatus::Moving {
            let place = if outpost.is_some() { "at home or at your outpost" } else { "at home" };
            return Err(Refusal::new(
                ErrorCode::OnCooldown,
                format!("fleet {fleet_id} is in transit; it can {verb} once it stands idle {place}"),
            ));
        }
        let dock = if fleet.location == player.homeworld {
            Dock::Home
        } else if outpost == Some(fleet.location) {
            Dock::Outpost
        } else {
            let hexes_to = |dock: Hex| {
                let hexes = Hex::distance(&fleet.location, &dock);
                format!("{hexes} hex{}", if hexes == 1 { "" } else { "es" })
            };
            let home = player.homeworld;
            return Err(Refusal::new(ErrorCode::NotInSector, match outpost {
                None => format!(
                    "fleet {fleet_id} is at {}, {} from home {home}; a hold is filled from the stockpile, \
                     and emptied into it, at the homeworld only",
                    fleet.location, hexes_to(home),
                ),
                Some(outpost) => format!(
                    "fleet {fleet_id} is at {}, {} from home {home} and {} from your outpost {outpost}; \
                     a hold is filled and emptied by hand at those two docks only",
                    fleet.location, hexes_to(home), hexes_to(outpost),
                ),
            }));
        };
        if !Self::fleet_is_free(fleet) {
            return Err(Refusal::new(
                ErrorCode::OnCooldown,
                format!("fleet {fleet_id} is busy; it can {verb} once it is idle"),
            ));
        }
        Ok((fleet, player, dock))
    }

    /// How much of `resource` a `load` order would put aboard `fleet_id`:
    /// `amount`, or all the dock holds and the hold has room for when None.
    /// The dock's stock is the stockpile at home and the outpost's store at
    /// the outpost. Nothing changes.
    pub fn load_order(
        &self,
        player_id: u64,
        fleet_id: u64,
        resource: ResourceKind,
        amount: Option<f64>,
    ) -> Result<f64, Refusal> {
        self.load_at_dock(player_id, fleet_id, resource, amount).map(|(loaded, _)| loaded)
    }

    fn load_at_dock(
        &self,
        player_id: u64,
        fleet_id: u64,
        resource: ResourceKind,
        amount: Option<f64>,
    ) -> Result<(f64, Dock), Refusal> {
        let (fleet, player, dock) = self.fleet_at_dock(player_id, fleet_id, "load")?;
        if let Some(policy) = self.policies.get(&fleet_id) {
            let rule = match dock {
                Dock::Home => "which unload at the dock",
                Dock::Outpost => "which do not use the outpost",
            };
            return Err(Refusal::new(ErrorCode::InvalidCommand, format!(
                "fleet {fleet_id} is under standing orders ({}), {rule}; \
                 set it to manual before loading its hold by hand",
                policy.preset.label(),
            )));
        }
        let label = resource.label();
        let (source, stock) = match dock {
            Dock::Home => ("the stockpile", player.resources.get(resource)),
            Dock::Outpost => (
                "the outpost's store",
                self.outposts.get(&player_id).map_or(0.0, |o| o.store.get(resource)),
            ),
        };
        let capacity = fleet_cargo_capacity(fleet, &self.world.pace);
        let aboard = fleet.cargo.total();
        let room = (capacity - aboard).max(0.0);
        let full = || Refusal::new(ErrorCode::CargoFull, format!(
            "the hold of fleet {fleet_id} is full: it carries {aboard:.0} of {capacity:.0}",
        ));
        let loaded = match amount {
            Some(asked) if !(asked.is_finite() && asked > 0.0) => {
                Err(Refusal::new(ErrorCode::InvalidCommand, "the amount to load must be a positive number".to_string()))
            }
            Some(asked) if asked > stock + LOAD_SLACK => Err(Refusal::new(ErrorCode::NoResources, format!(
                "{source} holds {stock:.1} {label}; fleet {fleet_id} cannot load {asked:.1}",
            ))),
            Some(_) if room <= LOAD_SLACK => Err(full()),
            Some(asked) if asked > room + LOAD_SLACK => Err(Refusal::new(ErrorCode::CargoFull, format!(
                "the hold of fleet {fleet_id} has room for {room:.1} more (it carries {aboard:.0} of {capacity:.0}); \
                 it cannot load {asked:.1} {label}",
            ))),
            Some(asked) => Ok(asked.min(stock).min(room)),
            None if stock <= LOAD_SLACK => Err(Refusal::new(
                ErrorCode::NoResources,
                format!("{source} holds no {label} to load"),
            )),
            None if room <= LOAD_SLACK => Err(full()),
            None => Ok(stock.min(room)),
        }?;
        Ok((loaded, dock))
    }

    /// Move stock into the hold of a fleet at a dock. At home the fleet is
    /// marked laden, so the dock leaves the hold alone; at the outpost
    /// nothing unloads by itself.
    pub fn handle_load(
        &mut self,
        player_id: u64,
        fleet_id: u64,
        resource: ResourceKind,
        amount: Option<f64>,
    ) -> Result<(), ErrorCode> {
        let (loaded, dock) = self.load_at_dock(player_id, fleet_id, resource, amount).map_err(|refusal| refusal.code)?;
        match dock {
            Dock::Home => {
                let player = self.players.get_mut(&player_id).ok_or(ErrorCode::ServerError)?;
                let stock = player.resources.get_mut(resource);
                *stock = (*stock - loaded).max(0.0);
            }
            Dock::Outpost => {
                let outpost = self.outposts.get_mut(&player_id).ok_or(ErrorCode::ServerError)?;
                let stock = outpost.store.get_mut(resource);
                *stock = (*stock - loaded).max(0.0);
                *outpost.hauled.get_mut(resource) += loaded;
            }
        }
        let capacity = self.fleets.get(&fleet_id).map_or(0.0, |f| fleet_cargo_capacity(f, &self.world.pace));
        let fleet = self.fleets.get_mut(&fleet_id).ok_or(ErrorCode::FleetNotFound)?;
        *fleet.cargo.get_mut(resource) += loaded;
        fleet.laden = dock == Dock::Home;
        let (aboard, sector) = (fleet.cargo.total(), fleet.location);
        let label = resource.label();
        self.push_alert(AlertLevel::Info, fleet_id, sector, match dock {
            Dock::Home => format!(
                "fleet {fleet_id} loaded {loaded:.0} {label} from the stockpile: it carries {aboard:.0} of {capacity:.0} \
                 and keeps it aboard until it next returns home, or until unload",
            ),
            Dock::Outpost => format!(
                "fleet {fleet_id} loaded {loaded:.0} {label} from the outpost's store: it carries {aboard:.0} of {capacity:.0}",
            ),
        });
        Ok(())
    }

    /// Why `unload` would be refused for `fleet_id` right now, if it would.
    pub fn unload_refusal(&self, player_id: u64, fleet_id: u64) -> Option<Refusal> {
        match self.fleet_at_dock(player_id, fleet_id, "unload") {
            Err(refusal) => Some(refusal),
            Ok((fleet, _, Dock::Home)) if !fleet.laden => Some(Refusal::new(ErrorCode::InvalidCommand, format!(
                "fleet {fleet_id} was not loaded by hand; a docked fleet unloads by itself as the stockpile has room",
            ))),
            Ok((fleet, _, Dock::Outpost)) if fleet.cargo.total() <= LOAD_SLACK => Some(Refusal::new(
                ErrorCode::NoResources,
                format!("the hold of fleet {fleet_id} is empty; there is nothing to unload into the outpost's store"),
            )),
            Ok(_) => None,
        }
    }

    /// At home, end a hand-loaded hold: what fits goes back to the
    /// stockpile now and the dock takes the rest as room opens. At the
    /// outpost, put the hold into the store as far as the store has room.
    pub fn handle_unload(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        if let Some(refusal) = self.unload_refusal(player_id, fleet_id) {
            return Err(refusal.code);
        }
        let fleet = self.fleets.get_mut(&fleet_id).ok_or(ErrorCode::FleetNotFound)?;
        let (before, sector) = (fleet.cargo, fleet.location);
        let at_home = self.players.get(&player_id).is_some_and(|p| p.homeworld == sector);
        fleet.laden = false;
        if at_home {
            self.unload_cargo(fleet_id);
        } else {
            self.unload_into_store(fleet_id);
        }
        let left = self.fleets.get(&fleet_id).map_or(Resources::default(), |f| f.cargo);
        let (into, until) = if at_home {
            ("the stockpile", "stays aboard until the stockpile has room")
        } else {
            ("the outpost's store", "stays aboard, the store is full")
        };
        let rest = if left.total() > LOAD_SLACK {
            format!("; {} {until}", crate::commands::amount_words(left))
        } else {
            String::new()
        };
        self.push_alert(AlertLevel::Info, fleet_id, sector, format!(
            "fleet {fleet_id} unloaded {} into {into}{rest}",
            crate::commands::amount_words(before.sub(left)),
        ));
        Ok(())
    }

    /// Move as much of the fleet's cargo into its owner's outpost as the
    /// store's caps allow; the rest stays aboard.
    fn unload_into_store(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get_mut(&fleet_id) else { return; };
        let owner = fleet.owner_id;
        let Some(outpost) = self.outposts.get_mut(&owner) else { return; };
        let cap = scaling::outpost_store_cap(outpost.level, &self.world.pace);
        let moved = outpost.store.shortfall(cap).min(fleet.cargo);
        if moved.total() <= 0.0 { return; }
        outpost.store = outpost.store.add(moved);
        fleet.cargo = fleet.cargo.sub(moved);
    }

    /// What `refine` would turn into what for `fleet_id` right now, or why
    /// the order is refused.
    pub fn refine_order(&self, player_id: u64, fleet_id: u64) -> Result<Refining, Refusal> {
        let fleet = self.owned_fleet(player_id, fleet_id)
            .map_err(|code| Refusal::new(code, format!("fleet {fleet_id} not found (or it has no ships left)")))?;
        refining(fleet).ok_or_else(|| match (fleet.state, fleet.move_target) {
            (FleetStatus::InCombat, _) => Refusal::new(
                ErrorCode::OnCooldown,
                format!("fleet {fleet_id} is in combat; nothing is refined in a fight"),
            ),
            (FleetStatus::Moving, Some(target)) => Refusal::new(
                ErrorCode::OnCooldown,
                format!("fleet {fleet_id} is charging a jump to {target}; it can refine once it has arrived"),
            ),
            (FleetStatus::Moving, None) => Refusal::new(
                ErrorCode::OnCooldown,
                format!("fleet {fleet_id} is charging a jump; it can refine once it has arrived"),
            ),
            _ if fleet.cargo.deuterium <= 0.0 => Refusal::new(
                ErrorCode::NoResources,
                format!("the hold of fleet {fleet_id} carries no deuterium to refine"),
            ),
            _ => Refusal::new(ErrorCode::InvalidCommand, format!(
                "the tank of fleet {fleet_id} is full ({} of {} fuel); refining would add nothing",
                fuel_words(fleet.fuel), fuel_words(fleet.fuel_max),
            )),
        })
    }

    /// Turn deuterium in the hold into fuel, as `refine_order` quotes it, and
    /// tell the owner both figures. It takes no time and changes nothing
    /// else: a harvest or a boarding goes on.
    pub fn handle_refine(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        self.refine_order(player_id, fleet_id).map_err(|refusal| refusal.code)?;
        let Refining { deuterium, fuel } = self.refine_hold(fleet_id).ok_or(ErrorCode::ServerError)?;
        let fleet = self.fleets.get(&fleet_id).ok_or(ErrorCode::FleetNotFound)?;
        let (left, sector) = (fleet.cargo.deuterium, fleet.location);
        let hold = if left > 0.0 {
            format!("{} deuterium is left in the hold", fuel_words(left))
        } else {
            "the hold has no deuterium left".to_string()
        };
        let message = format!(
            "fleet {fleet_id} refined {} deuterium into {} fuel: the tank stands at {} of {}, and {hold}",
            fuel_words(deuterium), fuel_words(fuel), fuel_words(fleet.fuel), fuel_words(fleet.fuel_max),
        );
        self.push_alert(AlertLevel::Info, fleet_id, sector, message);
        Ok(())
    }

    /// Do what `refining` says for `fleet_id`: what left the hold and what
    /// the tank gained, or None when there was nothing to refine. A tank
    /// that is filled reads exactly its size.
    fn refine_hold(&mut self, fleet_id: u64) -> Option<Refining> {
        let fleet = self.fleets.get_mut(&fleet_id)?;
        let done = refining(fleet)?;
        let missing = fleet.fuel_max - fleet.fuel;
        fleet.cargo.deuterium = (fleet.cargo.deuterium - done.deuterium).max(0.0);
        fleet.fuel = if done.fuel >= missing { fleet.fuel_max } else { fleet.fuel + done.fuel };
        Some(done)
    }

    /// Fuel an emergency jump home would burn: the per-hop cost over the
    /// straight distance, doubled. All-or-nothing: a fleet with less than
    /// this does not move at all.
    pub fn recall_fuel_cost(&self, fleet: &Fleet) -> f64 {
        let dist = self.players.get(&fleet.owner_id)
            .map(|p| f64::from(Hex::distance(&fleet.location, &p.homeworld)))
            .unwrap_or(0.0);
        self.hop_fuel_cost(fleet) * dist * RECALL_FUEL_MULTIPLIER
    }

    /// Emergency jump home from another sector. A jump the fleet was
    /// charging is abandoned with its fuel, as `stop` would. At the
    /// homeworld there is nowhere to come back from and the order is
    /// refused: a fleet charging a jump there keeps the jump it paid for,
    /// and one harvesting there goes on. The owner is told what the jump
    /// cost: the fuel, a jump abandoned, hull damage and ships lost.
    pub fn handle_recall(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        let player = self.players.get(&fleet.owner_id).ok_or(ErrorCode::ServerError)?;
        if fleet.location == player.homeworld { return Err(ErrorCode::InvalidCommand); }

        let dist = Hex::distance(&fleet.location, &player.homeworld) as f32;
        let fuel_cost = self.recall_fuel_cost(fleet);
        if fleet.fuel < fuel_cost { return Err(ErrorCode::InsufficientFuel); }
        let from = fleet.location;
        let abandoned = fleet.move_target
            .filter(|_| fleet.state == FleetStatus::Moving)
            .map(|target| (target, self.hop_fuel_cost(fleet)));

        let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
        fleet_mut.fuel -= fuel_cost;

        let base_damage_chance = RECALL_DAMAGE_CHANCE_PER_HEX * dist;
        let ej_reduction = scaling::recall_damage_reduction(player.research.hyperspace_technology);
        let damage_chance = RECALL_DAMAGE_CHANCE_CAP.min(base_damage_chance.max(0.0) - ej_reduction.max(0.0));

        let mut rng = StdRng::seed_from_u64(
            self.world_gen.world_seed ^ self.current_tick.wrapping_mul(0x9E3779B97F4A7C15) ^ fleet_id.rotate_left(32)
        );

        let arms = fleet_mut.arms();
        let ships_before = fleet_mut.ships.total();
        let (mut damaged, mut hull_lost) = (0u32, 0u64);
        let mut broken: Vec<ShipCount> = Vec::new();
        let mut after = Ships::default();
        for group in fleet_mut.ships.groups() {
            let full = scaling::ship_figures(group.ship_class, arms).hull;
            let mut gone = 0u32;
            let holding = std::iter::repeat_n(full, group.whole() as usize).chain(group.damaged.iter().copied());
            for hull in holding {
                let mut left = hull;
                let roll = rng.random_range(0.0..1.0);
                if roll < damage_chance {
                    let share = RECALL_HULL_DAMAGE_MIN + rng.random_range(0.0..1.0) * (RECALL_HULL_DAMAGE_MAX - RECALL_HULL_DAMAGE_MIN);
                    let dealt = (full as f32 * share) as u32;
                    if dealt >= hull {
                        gone += 1;
                        continue;
                    }
                    left = hull - dealt;
                    damaged += 1;
                    hull_lost += u64::from(dealt);
                }
                after.add_holding(group.ship_class, left, full);
            }
            if gone > 0 {
                broken.push(ShipCount { ship_class: group.ship_class, count: gone });
            }
        }
        fleet_mut.ships = after;

        let home = player.homeworld;
        let lost = ships_before - fleet_mut.ships.total();
        if lost > 0 {
            self.pending_events.push(GameEvent {
                tick: self.current_tick,
                kind: EventKind::ShipsLost(ShipsLostEvent {
                    fleet_id,
                    sector: from,
                    owner: player.name.clone(),
                    mine: false,
                    ships: broken,
                }),
            });
            self.settle_hold(fleet_id);
        }
        let of_ships = |n: u32| match (n, ships_before) {
            (_, 1) => "its one ship".to_string(),
            (n, all) if n == all => format!("all {all} of its ships"),
            (n, all) => format!("{n} of its {all} ships"),
        };
        let harm = match (lost, damaged) {
            (0, 0) => "no ship was damaged".to_string(),
            (0, _) => format!("{} took {hull_lost} hull damage", of_ships(damaged)),
            (_, 0) => format!("{} broke up in the jump", of_ships(lost)),
            _ => format!(
                "{} broke up in the jump and {damaged} more took {hull_lost} hull damage",
                of_ships(lost),
            ),
        };
        let jump = abandoned.map_or_else(String::new, |(target, fuel)| {
            format!(", abandoning the jump to {target} it had burned {} fuel for", fuel_words(fuel))
        });
        self.push_alert(
            if lost + damaged > 0 { AlertLevel::Warning } else { AlertLevel::Info },
            fleet_id,
            from,
            format!(
                "fleet {fleet_id} made the emergency jump home from {from} for {} fuel{jump}: {harm}",
                fuel_words(fuel_cost),
            ),
        );
        let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
        fleet_mut.location = home;
        fleet_mut.state = if fleet_mut.ships.is_empty() { FleetStatus::Docked } else { FleetStatus::Idle };
        fleet_mut.move_target = None;
        fleet_mut.move_cooldown = 0;

        self.leave_combat(fleet_id, false);
        self.dock_fleet(fleet_id).map_err(|_| ErrorCode::ServerError)?;

        Ok(())
    }

    fn fleet_is_free(fleet: &Fleet) -> bool {
        fleet.state == FleetStatus::Idle || fleet.state == FleetStatus::Docked
    }

    /// Detach `take`, so many ships of each class named, from a fleet into
    /// a new fleet at the same sector and return its id. Of a class the
    /// whole ships go first and then the least damaged; with `damaged_first`
    /// the most damaged go first. Both halves fight with the fleet's refit.
    /// Fuel and cargo follow the ships in proportion to the tank and hold
    /// they take along; both halves of a laden fleet stay laden.
    pub fn handle_split(&mut self, player_id: u64, fleet_id: u64, take: &[ShipCount], damaged_first: bool) -> Result<u64, ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if !Self::fleet_is_free(fleet) { return Err(ErrorCode::OnCooldown); }

        let mut kept = fleet.ships.clone();
        let mut taken = Ships::default();
        for (i, order) in take.iter().enumerate() {
            let named_twice = take[..i].iter().any(|earlier| earlier.ship_class == order.ship_class);
            if order.count == 0 || named_twice { return Err(ErrorCode::InvalidTarget); }
            let group = kept.take(order.ship_class, order.count, damaged_first).ok_or(ErrorCode::InvalidTarget)?;
            taken.absorb(Ships(vec![group]));
        }
        if taken.is_empty() || kept.is_empty() { return Err(ErrorCode::InvalidTarget); }

        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        let owned = self.fleets.values().filter(|f| f.owner_id == player_id && !f.ships.is_empty()).count();
        if owned >= MAX_FLEETS_TOTAL { return Err(ErrorCode::FleetLimitReached); }
        if fleet.location != player.homeworld
            && !self.docked_at_outpost(fleet)
            && self.count_deployed_fleets(player_id, player.homeworld) >= MAX_FLEETS_PER_PLAYER
        {
            return Err(ErrorCode::FleetLimitReached);
        }

        let mut new_fleet = Fleet {
            id: 0,
            owner_id: player_id,
            location: fleet.location,
            state: FleetStatus::Idle,
            ships: taken,
            refit: fleet.refit,
            cargo: Resources::default(),
            fuel: 0.0,
            fuel_max: 0.0,
            move_cooldown: fleet.move_cooldown,
            action_cooldown: fleet.action_cooldown,
            move_target: None,
            harvest_target: fleet.harvest_target,
            charts: fleet.charts.clone(),
            laden: fleet.laden,
        };
        let mut rest = fleet.clone();
        rest.ships = kept;

        let old_tank = fleet_fuel_max(fleet, player);
        let old_hold = fleet_cargo_capacity(fleet, &self.world.pace);
        let tank_share = if old_tank > 0.0 { fleet_fuel_max(&new_fleet, player) / old_tank } else { 0.0 };
        let hold_share = if old_hold > 0.0 { fleet_cargo_capacity(&new_fleet, &self.world.pace) / old_hold } else { 0.0 };

        new_fleet.fuel_max = fleet_fuel_max(&new_fleet, player);
        new_fleet.fuel = (fleet.fuel * tank_share).min(new_fleet.fuel_max);
        new_fleet.cargo = fleet.cargo.scale(hold_share);
        rest.fuel_max = fleet_fuel_max(&rest, player);
        rest.fuel = (fleet.fuel - new_fleet.fuel).clamp(0.0, rest.fuel_max);
        rest.cargo = fleet.cargo.sub(new_fleet.cargo);

        let new_id = self.next_id();
        new_fleet.id = new_id;
        let location = new_fleet.location;
        let count = new_fleet.ships.total();
        self.fleets.insert(fleet_id, rest);
        self.fleets.insert(new_id, new_fleet);
        self.push_alert(
            AlertLevel::Info, new_id, location,
            format!("fleet {fleet_id} split: {count} ship(s) now fly as fleet {new_id}"),
        );
        Ok(new_id)
    }

    /// Fold `other_id` into `fleet_id`: ships, cargo and fuel pool and the
    /// absorbed fleet and its standing orders cease to exist. The merged
    /// fleet is laden if `fleet_id` was. It fights with the lower of the
    /// two fleets' Weapons, Shielding and Armour levels, each damaged ship
    /// keeping the share of its hull it had, until the home dock refits it.
    pub fn handle_merge(&mut self, player_id: u64, fleet_id: u64, other_id: u64) -> Result<(), ErrorCode> {
        if fleet_id == other_id { return Err(ErrorCode::InvalidTarget); }
        let a = self.owned_fleet(player_id, fleet_id)?;
        let b = self.owned_fleet(player_id, other_id)?;
        if a.location != b.location { return Err(ErrorCode::NotInSector); }
        if !Self::fleet_is_free(a) || !Self::fleet_is_free(b) { return Err(ErrorCode::OnCooldown); }
        if a.ships.total() + b.ships.total() > MAX_FLEET_SHIPS { return Err(ErrorCode::InvalidCommand); }
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;

        let mut merged = a.clone();
        merged.refit = a.refit.lower(b.refit);
        merged.ships.rearm(a.arms(), merged.arms());
        let mut joining = b.ships.clone();
        joining.rearm(b.arms(), merged.arms());
        merged.ships.absorb(joining);
        merged.cargo = a.cargo.add(b.cargo);
        merged.fuel_max = fleet_fuel_max(&merged, player);
        merged.fuel = (a.fuel + b.fuel).min(merged.fuel_max);
        merged.move_cooldown = a.move_cooldown.max(b.move_cooldown);
        merged.action_cooldown = a.action_cooldown.max(b.action_cooldown);
        for key in &b.charts {
            if !merged.charts.contains(key) {
                merged.charts.push(*key);
            }
        }
        let location = merged.location;
        let count = merged.ships.total();
        let refit = merged.refit;
        let lowered = (a.refit != refit || b.refit != refit) && location != player.homeworld;

        let orders_ended = self.policies.contains_key(&other_id);
        self.remove_fleet(other_id);
        self.fleets.insert(fleet_id, merged);
        self.tend_at_home(fleet_id, false);
        self.push_alert(
            AlertLevel::Info, fleet_id, location,
            format!(
                "fleet {other_id} merged into fleet {fleet_id}: {count} ship(s){}{}",
                if lowered {
                    format!(
                        ", fighting at the lower of the two fleets' levels (Weapons {}, Shielding {}, Armour {}) until the home dock refits them",
                        refit.weapons, refit.shielding, refit.armour,
                    )
                } else {
                    String::new()
                },
                if orders_ended { "; the absorbed fleet's standing orders ended" } else { "" },
            ),
        );
        Ok(())
    }

    /// A payment larger than a stockpile cap can never be made until that
    /// resource's storage building grows.
    fn ensure_fits_storage(&self, player: &Player, cost: Resources) -> Result<(), ErrorCode> {
        let cap = scaling::storage_cap(&player.buildings, &self.world.pace);
        if ResourceKind::ALL.into_iter().any(|k| cost.get(k) > cap.get(k)) {
            return Err(ErrorCode::StorageTooSmall);
        }
        Ok(())
    }

    /// A shipyard batch is paid in full when it is ordered. One larger than
    /// a stockpile cap is refused as a building or research level is, even
    /// when refunds paid past the cap would cover it; one the stockpile does
    /// not cover is refused for that.
    fn ensure_batch_paid_for(&self, player: &Player, cost: Resources) -> Result<(), ErrorCode> {
        self.ensure_fits_storage(player, cost)?;
        if !player.resources.can_afford(cost) {
            return Err(ErrorCode::InsufficientResources);
        }
        Ok(())
    }

    /// What `cmd` would cost, with a label for messages; None for commands
    /// that cost nothing.
    pub fn command_cost(&self, player_id: u64, cmd: &Command) -> Option<(String, Resources)> {
        let p = self.players.get(&player_id)?;
        match cmd {
            Command::Build { building_type, .. } => {
                let level = p.projected_buildings().get(*building_type) + 1;
                Some((format!("{} Lv.{level}", building_type.label()), scaling::building_cost(*building_type, level)))
            }
            Command::Research { tech, .. } => {
                let level = p.projected_research().get(*tech) + 1;
                Some((format!("{} Lv.{level}", tech.label()), scaling::research_cost(*tech, level)))
            }
            Command::BuildShip { ship_class, count } => Some((
                format!("{} x{count}", ship_class.label()),
                ShipyardItem::Ship(*ship_class).batch_cost(*count),
            )),
            Command::BuildDefence { kind, count } => Some((
                format!("{} x{count}", kind.label()),
                ShipyardItem::Defence(*kind).batch_cost(*count),
            )),
            _ => None,
        }
    }

    /// Queue a building level. It is accepted whenever it can ever start and
    /// the line has room, started now or not (see `advance_queues`).
    pub fn handle_build(&mut self, player_id: u64, building_type: BuildingType, reserve: bool) -> Result<(), ErrorCode> {
        use iac_shared::protocol::QueueType;
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        let projected = player.projected_buildings();
        let current_level = projected.get(building_type);
        if current_level >= MAX_BUILDING_LEVEL { return Err(ErrorCode::MaxLevelReached); }
        if !scaling::building_prerequisites_met(building_type, &projected, &player.projected_research()) {
            return Err(ErrorCode::PrerequisitesNotMet);
        }
        if player.building_queue.len() + player.building_pending.len() >= player.building_slots() + scaling::QUEUE_WAITING {
            return Err(ErrorCode::QueueFull);
        }

        let target_level = current_level + 1;
        self.ensure_fits_storage(player, scaling::building_cost(building_type, target_level))?;

        let id = self.next_id();
        let player_mut = self.players.get_mut(&player_id).unwrap();
        player_mut.building_pending.push(PendingBuilding { id, building_type, target_level, reserve });
        self.advance_queues(player_id);
        self.announce_waiting(player_id, QueueType::Building, id, format!("{} Lv.{target_level}", building_type.label()), Resources::default());
        Ok(())
    }

    pub fn handle_research(&mut self, player_id: u64, tech: ResearchType, reserve: bool) -> Result<(), ErrorCode> {
        use iac_shared::protocol::QueueType;
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        if player.projected_buildings().research_lab == 0 { return Err(ErrorCode::NoResearchLab); }

        let projected = player.projected_research();
        let current_level = projected.get(tech);
        if current_level >= scaling::research_max_level(tech) { return Err(ErrorCode::MaxLevelReached); }
        if !scaling::research_prerequisites_met(tech, &player.projected_buildings(), &projected)
            || player.power(&self.world.pace).made < scaling::research_energy(tech, current_level + 1)
        {
            return Err(ErrorCode::PrerequisitesNotMet);
        }
        if player.research_queue.iter().count() + player.research_pending.len() > scaling::QUEUE_WAITING {
            return Err(ErrorCode::QueueFull);
        }

        let target_level = current_level + 1;
        self.ensure_fits_storage(player, scaling::research_cost(tech, target_level))?;

        let id = self.next_id();
        let player_mut = self.players.get_mut(&player_id).unwrap();
        player_mut.research_pending.push(PendingResearch { id, tech, target_level, reserve });
        self.advance_queues(player_id);
        self.announce_waiting(player_id, QueueType::Research, id, format!("{} Lv.{target_level}", tech.label()), Resources::default());
        Ok(())
    }

    pub fn handle_build_ship(&mut self, player_id: u64, ship_class: ShipClass, count: u16) -> Result<(), ErrorCode> {
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        let buildings = player.projected_buildings();
        if buildings.shipyard == 0 { return Err(ErrorCode::NoShipyard); }
        if !scaling::ship_class_unlocked(ship_class, &buildings, &player.projected_research()) {
            return Err(ErrorCode::ShipLocked);
        }
        self.order_shipyard(player_id, ShipyardItem::Ship(ship_class), count)
    }

    pub fn handle_build_defence(&mut self, player_id: u64, kind: DefenceKind, count: u16) -> Result<(), ErrorCode> {
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        if player.projected_buildings().shipyard == 0 { return Err(ErrorCode::NoShipyard); }
        if !scaling::defence_prerequisites_met(kind, &player.projected_buildings(), &player.projected_research()) {
            return Err(ErrorCode::DefenceLocked);
        }
        if kind.max_count().is_some_and(|most| Self::defences_owned_and_ordered(player, kind) + u32::from(count) > most) {
            return Err(ErrorCode::MaxLevelReached);
        }
        self.order_shipyard(player_id, ShipyardItem::Defence(kind), count)
    }

    /// Defences of `kind` standing and paid for in the yard.
    pub fn defences_owned_and_ordered(player: &Player, kind: DefenceKind) -> u32 {
        let item = ShipyardItem::Defence(kind);
        let building = player.ship_queue.iter().filter(|q| q.item == item).map(|q| u32::from(q.undelivered()));
        let waiting = player.ship_pending.iter().filter(|q| q.item == item).map(|q| u32::from(q.count));
        player.defences.owned(kind) + building.chain(waiting).sum::<u32>()
    }

    /// Order a shipyard batch behind the ones already there. The whole batch
    /// is paid for now; it starts at once when the yard is free and what it
    /// needs is built, and the yard then builds it unit by unit.
    fn order_shipyard(&mut self, player_id: u64, item: ShipyardItem, count: u16) -> Result<(), ErrorCode> {
        use iac_shared::protocol::QueueType;
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        if count == 0 { return Err(ErrorCode::InvalidCommand); }
        if player.ship_queue.iter().count() + player.ship_pending.len() > scaling::QUEUE_WAITING {
            return Err(ErrorCode::QueueFull);
        }
        let cost = item.batch_cost(count);
        self.ensure_batch_paid_for(player, cost)?;
        let starts_now = player.ship_queue.is_none() && self.ship_ready(player, item);

        let id = self.next_id();
        let player_mut = self.players.get_mut(&player_id).unwrap();
        player_mut.resources = player_mut.resources.sub(cost);
        let order = PendingShip { id, item, count };
        if starts_now {
            self.start_ship_batch(player_id, order, cost);
        } else {
            self.players.get_mut(&player_id).unwrap().ship_pending.push(order);
            self.announce_waiting(player_id, QueueType::Ship, id, format!("{} x{count}", item.label()), cost);
        }
        Ok(())
    }

    /// Cancel an item that has started, by id. A building or research level
    /// keeps half its payment, and waiting items of the same kind move up a
    /// level. A shipyard batch keeps the units delivered and half the price
    /// of the rest.
    pub fn handle_cancel_build(&mut self, player_id: u64, queue_type: iac_shared::protocol::QueueType, id: u64) -> Result<(), ErrorCode> {
        use iac_shared::protocol::{QueueAction, QueueType};
        let player = self.players.get_mut(&player_id).ok_or(ErrorCode::ServerError)?;
        let (item, refund) = match queue_type {
            QueueType::Building => {
                let index = player.building_queue.iter().position(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let q = player.building_queue.remove(index);
                let cost = scaling::building_cost(q.building_type, q.target_level);
                for later in player.building_pending.iter_mut()
                    .filter(|p| p.building_type == q.building_type && p.target_level > q.target_level)
                {
                    later.target_level -= 1;
                }
                (format!("{} Lv.{}", q.building_type.label(), q.target_level), cost.scale(CANCEL_REFUND_FRACTION))
            }
            QueueType::Ship => {
                let q = player.ship_queue.take_if(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let label = if q.built > 0 {
                    format!("{} x{} ({} already built and kept)", q.item.label(), q.count, q.built)
                } else {
                    format!("{} x{}", q.item.label(), q.count)
                };
                (label, q.item.batch_cost(q.undelivered()).scale(CANCEL_REFUND_FRACTION))
            }
            QueueType::Research => {
                let q = player.research_queue.take_if(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let cost = scaling::research_cost(q.tech, q.target_level);
                for later in player.research_pending.iter_mut()
                    .filter(|p| p.tech == q.tech && p.target_level > q.target_level)
                {
                    later.target_level -= 1;
                }
                (format!("{} Lv.{}", q.tech.label(), q.target_level), cost.scale(CANCEL_REFUND_FRACTION))
            }
        };
        player.resources = player.resources.add(refund);
        let mut event = self.queue_event(player_id, queue_type, id, item, QueueAction::Cancelled);
        event.refunded = refund;
        self.push_queue_event(event);
        self.advance_queues(player_id);
        Ok(())
    }

    /// Remove a waiting item by id. A waiting building or research level
    /// paid nothing, so nothing comes back; a waiting shipyard batch paid
    /// when it was ordered and gets half back.
    pub fn handle_cancel_queued(&mut self, player_id: u64, queue_type: iac_shared::protocol::QueueType, id: u64) -> Result<(), ErrorCode> {
        use iac_shared::protocol::{QueueAction, QueueType};
        let player = self.players.get_mut(&player_id).ok_or(ErrorCode::ServerError)?;
        let (item, refund) = match queue_type {
            QueueType::Building => {
                let index = player.building_pending.iter().position(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let q = player.building_pending.remove(index);
                for later in player.building_pending.iter_mut()
                    .filter(|p| p.building_type == q.building_type && p.target_level > q.target_level)
                {
                    later.target_level -= 1;
                }
                (format!("{} Lv.{}", q.building_type.label(), q.target_level), Resources::default())
            }
            QueueType::Ship => {
                let index = player.ship_pending.iter().position(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let q = player.ship_pending.remove(index);
                (format!("{} x{}", q.item.label(), q.count), q.item.batch_cost(q.count).scale(CANCEL_REFUND_FRACTION))
            }
            QueueType::Research => {
                let index = player.research_pending.iter().position(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let q = player.research_pending.remove(index);
                for later in player.research_pending.iter_mut()
                    .filter(|p| p.tech == q.tech && p.target_level > q.target_level)
                {
                    later.target_level -= 1;
                }
                (format!("{} Lv.{}", q.tech.label(), q.target_level), Resources::default())
            }
        };
        player.resources = player.resources.add(refund);
        let mut event = self.queue_event(player_id, queue_type, id, item, QueueAction::Cancelled);
        event.refunded = refund;
        self.push_queue_event(event);
        self.advance_queues(player_id);
        Ok(())
    }

    /// Active scan: reveal connected sectors around the fleet and pick up
    /// faint signal contacts one hop beyond. An Espionage Probe aboard
    /// extends the range.
    pub fn handle_scan(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if fleet.state == FleetStatus::InCombat { return Err(ErrorCode::OnCooldown); }
        if fleet.state == FleetStatus::Moving { return Err(ErrorCode::OnCooldown); }
        if fleet.action_cooldown > 0 { return Err(ErrorCode::OnCooldown); }

        let origin = fleet.location;
        let range = Self::scan_range(fleet);

        let revealed = self.get_sensor_revealed_coords(origin, range);
        let fringe: Vec<Hex> = {
            let inner: std::collections::HashSet<u32> =
                revealed.iter().map(|h| h.to_key()).collect();
            self.get_sensor_revealed_coords(origin, range + 1)
                .into_iter()
                .filter(|h| !inner.contains(&h.to_key()))
                .collect()
        };

        let mut hostiles_detected: u16 = 0;
        let mut threats = Vec::with_capacity(revealed.len());
        for coord in &revealed {
            if self.sector_has_hostiles(*coord) { hostiles_detected += 1; }
            threats.push(iac_shared::protocol::SectorThreat { sector: *coord, threat: self.sector_threat(*coord) });
        }

        let mut signals: Vec<iac_shared::protocol::SignalContact> = Vec::new();
        for coord in &fringe {
            if let Some(signal) = self.faint_signal_at(*coord) {
                let threat_band = scaling::threat_band(self.sector_threat(*coord).rating);
                signals.push(iac_shared::protocol::SignalContact { sector: *coord, signal, threat_band });
            }
        }

        let expiry = self.current_tick + SCAN_REVEAL_TICKS;
        let player_reveals = self.scan_reveals.entry(player_id).or_default();
        // The scanning fleet's own sector counts as scanned — autopilots
        // use this to know a position's pings are still fresh.
        player_reveals.insert(origin.to_key(), expiry);
        for coord in &revealed {
            player_reveals.insert(coord.to_key(), expiry);
        }

        let sectors_revealed = revealed.len() as u16;
        let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
        fleet_mut.action_cooldown = SCAN_COOLDOWN;

        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::ScanCompleted(iac_shared::protocol::ScanCompletedEvent {
                fleet_id,
                sector: origin,
                sectors_revealed,
                hostiles_detected,
                threats,
                signals,
            }),
        });

        Ok(())
    }

    /// Stop whatever the fleet is doing. Aborting a jump forfeits the fuel
    /// already burned — that's the price of hesitation — and backing out of
    /// a boarding leaves the site on alert; the owner is told of either. A
    /// fleet that calls off its jump out of a fight fights on.
    pub fn handle_stop(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        match fleet.state {
            FleetStatus::Moving | FleetStatus::Harvesting | FleetStatus::Exploring => {
                let was_exploring = fleet.state == FleetStatus::Exploring;
                let location = fleet.location;
                let fighting = self.combat_of(fleet_id).is_some();
                let cost = match (fleet.state, fleet.move_target) {
                    (FleetStatus::Moving, Some(target)) if fighting => Some(format!(
                        "fleet {fleet_id} called off its jump to {target}: the {} fuel it burned for the jump is forfeit, \
                         and it fights on at {location}",
                        fuel_words(self.hop_fuel_cost(fleet) * FLEE_FUEL_MULTIPLIER),
                    )),
                    (FleetStatus::Moving, Some(target)) => Some(format!(
                        "fleet {fleet_id} called off its jump to {target}: the {} fuel it burned for the jump is forfeit",
                        fuel_words(self.hop_fuel_cost(fleet)),
                    )),
                    (FleetStatus::Exploring, _) => Some(format!(
                        "fleet {fleet_id} backed out of the derelict at {location}: nothing was recovered and the site is on alert, so the next boarding there is likelier to be ambushed",
                    )),
                    _ => None,
                };
                if was_exploring {
                    let ov = self.ensure_override(location.to_key());
                    ov.site_ambush_bumps = ov.site_ambush_bumps.saturating_add(1);
                }
                if let Some(message) = cost {
                    self.push_alert(AlertLevel::Warning, fleet_id, location, message);
                }
                let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
                fleet_mut.state = if fighting { FleetStatus::InCombat } else { FleetStatus::Idle };
                fleet_mut.move_target = None;
                fleet_mut.move_cooldown = 0;
                if was_exploring {
                    // action_cooldown was the boarding timer — release it.
                    fleet_mut.action_cooldown = 0;
                }
                Ok(())
            }
            FleetStatus::InCombat => Err(ErrorCode::OnCooldown),
            _ => Err(ErrorCode::InvalidCommand),
        }
    }

    // ── Derelict Sites ────────────────────────────────────────────

    /// The boardable derelict in a sector, if one exists and hasn't been
    /// stripped: (tier, failed-boarding bumps).
    pub fn derelict_site_at(&self, coord: Hex) -> Option<(u8, u8)> {
        let template = self.world_gen.generate_sector(coord);
        if !template.has_derelict { return None; }
        Self::derelict_site_in(coord, &template, self.sector_overrides.get(&coord.to_key()))
    }

    /// `derelict_site_at` for a caller that holds the sector's template and override.
    pub(crate) fn derelict_site_in(
        coord: Hex,
        template: &world::SectorTemplate,
        ov: Option<&SectorOverride>,
    ) -> Option<(u8, u8)> {
        if !template.has_derelict { return None; }
        if ov.map(|o| o.site_looted_tick.is_some()).unwrap_or(false) { return None; }
        let bumps = ov.map(|o| o.site_ambush_bumps).unwrap_or(0);
        Some((scaling::derelict_tier(coord.dist_from_origin()), bumps))
    }

    /// Ambush odds for a boarding attempt by this fleet at `site`.
    fn site_ambush_chance(&self, fleet: &Fleet, site: Hex, bumps: u8) -> f32 {
        let base = scaling::derelict_ambush(ring_rating(site));
        let read = if fleet.ships.has(ShipClass::EspionageProbe) { EXPLORE_PROBE_AMBUSH_REDUCTION } else { 0.0 };
        (base + bumps as f32 * EXPLORE_RETRY_AMBUSH_BUMP - read).clamp(0.02, 0.95)
    }

    /// Stripped derelicts are replaced after `DERELICT_RESPAWN_TICKS` (finds class).
    fn process_derelict_respawn(&mut self) {
        let delay = self.world.pace.finds_ticks(DERELICT_RESPAWN_TICKS as f64);
        let now = self.current_tick;
        let due: Vec<u32> = self.sector_overrides.iter()
            .filter(|(_, ov)| ov.site_looted_tick.is_some_and(|t| now >= t + delay))
            .map(|(key, _)| *key)
            .collect();
        for key in due {
            if let Some(ov) = self.sector_overrides.get_mut(&key) {
                ov.site_looted_tick = None;
                ov.site_ambush_bumps = 0;
            }
        }
    }

    /// Remember a sector the fleet has entered that its empire has never had
    /// credited; it pays when the fleet docks at the homeworld.
    fn note_chart(&mut self, fleet_id: u64, owner_id: u64, sector: Hex) {
        let key = sector.to_key();
        let at_home = self.players.get(&owner_id).is_none_or(|p| p.homeworld == sector);
        if at_home || self.credited.contains(&(owner_id, key, ExploreCredit::Charted)) { return; }
        if let Some(fleet) = self.fleets.get_mut(&fleet_id)
            && !fleet.charts.contains(&key)
        {
            fleet.charts.push(key);
        }
    }

    /// Pay explore points for the charts a docked fleet carries.
    fn deliver_charts(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get_mut(&fleet_id) else { return; };
        if fleet.charts.is_empty() { return; }
        let owner = fleet.owner_id;
        let charts = std::mem::take(&mut fleet.charts);

        let (mut sectors, mut points) = (0u32, 0.0f32);
        for key in charts {
            self.chart_news.push((owner, key));
            if self.credited.insert((owner, key, ExploreCredit::Charted)) {
                points += score::chart_points(ring_rating(Hex::from_key(key)));
                sectors += 1;
            }
        }
        if sectors == 0 { return; }
        if let Some(p) = self.players.get_mut(&owner) {
            p.explore_points += points;
        }
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::ChartDelivered(iac_shared::protocol::ChartDeliveredEvent { fleet_id, sectors, points }),
        });
    }

    /// Send a boarding party into the derelict in the fleet's sector.
    pub fn handle_explore_site(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if fleet.state == FleetStatus::InCombat { return Err(ErrorCode::OnCooldown); }
        if fleet.state == FleetStatus::Moving { return Err(ErrorCode::OnCooldown); }
        if fleet.state == FleetStatus::Exploring { return Err(ErrorCode::OnCooldown); }
        if fleet.action_cooldown > 0 { return Err(ErrorCode::OnCooldown); }

        let location = fleet.location;
        let (tier, _) = self.derelict_site_at(location).ok_or(ErrorCode::InvalidTarget)?;

        let end_tick = self.current_tick + EXPLORE_DURATION_TICKS as u64;
        let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
        fleet_mut.state = FleetStatus::Exploring;
        fleet_mut.action_cooldown = EXPLORE_DURATION_TICKS;

        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::SiteExplorationStarted(iac_shared::protocol::SiteExplorationStartedEvent {
                fleet_id,
                sector: location,
                tier,
                end_tick,
            }),
        });
        Ok(())
    }

    /// Resolve a boarding on the tick its start event named. The cooldown
    /// pass later this tick is what brings a timer of 1 down to 0, so a
    /// timer of 1 is due now.
    fn process_exploration(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let due: Vec<u64> = self.fleets.values()
            .filter(|f| f.state == FleetStatus::Exploring && f.action_cooldown <= 1)
            .map(|f| f.id)
            .collect();
        for fleet_id in due {
            if let Some(fleet) = self.fleets.get_mut(&fleet_id) {
                fleet.action_cooldown = 0;
            }
            self.resolve_exploration(fleet_id)?;
        }
        Ok(())
    }

    fn resolve_exploration(&mut self, fleet_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return Ok(()); };
        let location = fleet.location;
        let owner_id = fleet.owner_id;
        let key = location.to_key();

        // Site may have been looted out from under us (another fleet).
        let Some((tier, bumps)) = self.derelict_site_at(location) else {
            if let Some(f) = self.fleets.get_mut(&fleet_id) {
                f.state = FleetStatus::Idle;
            }
            return Ok(());
        };

        let fleet = self.fleets.get(&fleet_id).unwrap();
        let ambush_chance = self.site_ambush_chance(fleet, location, bumps);
        let has_rigging = fleet.ships.has(ShipClass::Recycler);

        let mut rng = StdRng::seed_from_u64(
            self.current_tick
                .wrapping_mul(0xC2B2AE3D27D4EB4F)
                .wrapping_add(u64::from(key))
                .wrapping_add(fleet_id),
        );

        if rng.random_range(0.0..1.0) < ambush_chance {
            // The wreck was bait. Site survives, gets hotter.
            let ov = self.ensure_override(key);
            ov.site_ambush_bumps = ov.site_ambush_bumps.saturating_add(1);

            let guardian = ambush_guardian_template(location, &mut rng);
            let guardian_id = self.next_id();
            let npc = self.spawn_npc_fleet(location, guardian, guardian_id)?;
            if let Some(f) = self.fleets.get_mut(&fleet_id) {
                f.state = FleetStatus::Idle;
            }
            self.pending_events.push(GameEvent {
                tick: self.current_tick,
                kind: EventKind::SiteAmbush(iac_shared::protocol::SiteAmbushEvent {
                    fleet_id,
                    sector: location,
                    npc_fleet_id: npc.id,
                }),
            });
            self.start_combat(fleet_id, npc.id)?;
            return Ok(());
        }

        // Clean breach: roll the loot.
        let dist = location.dist_from_origin();
        let rating = ring_rating(location);
        let base_value = scaling::derelict_value(dist)
            * f64::from(rng.random_range(scaling::DERELICT_ROLL_MIN..=scaling::DERELICT_ROLL_MAX));
        let loot_mult = self.world.pace.finds_mult() * if has_rigging { EXPLORE_RECYCLER_LOOT_MULTIPLIER } else { 1.0 };
        let rolled = scaling::split_loot(base_value).scale(loot_mult);

        // Cargo-cap the haul; anything the hold can't take drifts loose
        // as salvage for a follow-up trip.
        let fleet = self.fleets.get(&fleet_id).unwrap();
        let max_cargo = fleet_cargo_capacity(fleet, &self.world.pace);
        let used = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
        let mut remaining = (max_cargo - used).max(0.0);
        let mut taken = Resources {
            metal: rolled.metal.min(remaining),
            ..Default::default()
        };
        remaining -= taken.metal;
        taken.crystal = rolled.crystal.min(remaining);
        remaining -= taken.crystal;
        taken.deuterium = rolled.deuterium.min(remaining);
        let overflow = rolled.sub(taken);

        // Rare finds ride on top of the resources, by threat.
        let mut recovered_ship: Option<ShipClass> = None;
        if rng.random_range(0.0..1.0) < scaling::recovery_chance(rating) {
            recovered_ship = Some(recoverable_ship_class(tier, &mut rng));
        }
        let mut tech_cache: Option<ResearchType> = None;
        if rng.random_range(0.0..1.0) < scaling::data_core_chance(rating) {
            tech_cache = self.pick_tech_cache(owner_id, &mut rng);
        }
        let relic = rng.random_range(0.0..1.0) < scaling::relic_chance(rating);

        // Apply: cargo, site consumed, overflow salvage.
        if let Some(f) = self.fleets.get_mut(&fleet_id) {
            f.cargo = f.cargo.add(taken);
            f.state = FleetStatus::Idle;
        }

        let tick = self.current_tick;
        let ov = self.ensure_override(key);
        ov.site_looted_tick = Some(tick);
        if overflow.metal + overflow.crystal + overflow.deuterium > 1.0 {
            ov.salvage = Some(ov.salvage.unwrap_or_default().add(overflow));
            ov.salvage_despawn_tick = Some(tick + SALVAGE_DESPAWN_TICKS as u64);
        }

        if let Some(class) = recovered_ship {
            self.add_recovered_ship(fleet_id, class, &mut rng);
        }
        if let Some(tech) = tech_cache {
            self.grant_tech_level(owner_id, tech);
        }

        let mut points = 0.0;
        if self.credited.insert((owner_id, key, ExploreCredit::Boarded)) {
            points += score::board_points(scaling::split_loot(base_value).scale(self.world.pace.finds_mult()), rating);
        }
        if relic {
            points += scaling::RELIC_POINTS;
        }
        if let Some(p) = self.players.get_mut(&owner_id) {
            p.explore_points += points;
            p.relics += u32::from(relic);
        }

        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::SiteExplored(iac_shared::protocol::SiteExploredEvent {
                fleet_id,
                sector: location,
                tier,
                resources: taken,
                recovered_ship,
                tech_cache,
                relic,
                points,
            }),
        });
        info!(
            "Fleet {} stripped a tier-{} derelict at {} (+{:.0}M +{:.0}C +{:.0}D{}{}{}, +{:.2} explore)",
            fleet_id, tier, location, taken.metal, taken.crystal, taken.deuterium,
            recovered_ship.map(|c| format!(", recovered {}", c.label())).unwrap_or_default(),
            tech_cache.map(|t| format!(", data core: {:?}", t)).unwrap_or_default(),
            if relic { ", relic" } else { "" },
            points,
        );
        Ok(())
    }

    /// Wedge a battered-but-flyable ship into the fleet, if there's room. It
    /// fights at the levels of the fleet that found it.
    fn add_recovered_ship(&mut self, fleet_id: u64, class: ShipClass, rng: &mut StdRng) {
        let hull_frac: f32 = rng.random_range(0.35..0.70);
        let Some(fleet) = self.fleets.get_mut(&fleet_id) else { return; };
        if fleet.ships.total() >= MAX_FLEET_SHIPS { return; }
        let full = scaling::ship_figures(class, fleet.arms()).hull;
        fleet.ships.add_holding(class, (full as f32 * hull_frac) as u32, full);
        if let Some(player) = self.players.get(&fleet.owner_id) {
            fleet.fuel_max = fleet_fuel_max(fleet, player);
        }
    }

    /// A data core grants an instant level in a random low tech. Not in one
    /// that is bought with energy: Graviton is not to be found in a wreck.
    fn pick_tech_cache(&self, player_id: u64, rng: &mut StdRng) -> Option<ResearchType> {
        let player = self.players.get(&player_id)?;
        let candidates: Vec<ResearchType> = ResearchType::ALL.iter().copied()
            .filter(|&t| {
                let lvl = player.research.get(t);
                lvl < 3 && lvl < scaling::research_max_level(t) && scaling::research_energy(t, lvl + 1) == 0
            })
            .collect();
        if candidates.is_empty() { return None; }
        Some(candidates[rng.random_range(0..candidates.len())])
    }

    fn grant_tech_level(&mut self, player_id: u64, tech: ResearchType) {
        let Some(player) = self.players.get_mut(&player_id) else { return; };
        let new_level = player.research.get(tech) + 1;
        player.research.set(tech, new_level);
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::ResearchCompleted(iac_shared::protocol::ResearchCompletedEvent {
                tech,
                new_level,
                player_id: Some(player_id),
            }),
        });
    }

    // ── Standing Orders (Fleet Policies) ──────────────────────────

    /// Assign (or clear, with `manual`) a fleet's standing orders.
    pub fn handle_policy_update(
        &mut self,
        player_id: u64,
        fleet_id: u64,
        preset: PolicyPreset,
        params: Option<PolicyParams>,
    ) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        let location = fleet.location;

        if preset == PolicyPreset::Manual {
            self.policies.remove(&fleet_id);
            self.push_policy_event(fleet_id, PolicyPreset::Manual, "clear", "standing orders cleared");
            return Ok(());
        }

        // Clamp wire params to the same bounds the DB reload enforces,
        // or behavior would change across a restart.
        let mut params = params.unwrap_or_default();
        params.min_fuel_pct = params.min_fuel_pct.min(POLICY_MIN_FUEL_PCT_MAX);
        params.cargo_return_pct = params.cargo_return_pct.clamp(POLICY_CARGO_RETURN_PCT_MIN, POLICY_CARGO_RETURN_PCT_MAX);
        params.max_range = params.max_range.clamp(POLICY_RANGE_MIN, POLICY_RANGE_MAX);
        params.engage_ratio_x10 = params.engage_ratio_x10.clamp(POLICY_ENGAGE_RATIO_X10_MIN, POLICY_ENGAGE_RATIO_X10_MAX);
        params.disengage_ratio_x10 = params.disengage_ratio_x10
            .clamp(POLICY_DISENGAGE_RATIO_X10_MIN, POLICY_DISENGAGE_RATIO_X10_MAX)
            .min(params.engage_ratio_x10);
        params.fuel_budget_pct = params.fuel_budget_pct.min(POLICY_FUEL_UNBOUNDED_PCT);

        // The allowance starts empty: a fleet flies its first sortie on the
        // tank it has. New orders for a fleet already under orders keep what
        // its budget has earned, so re-issuing them neither resets nor
        // refills it.
        let (fuel_credit, fuel_since) = self.policies.get(&fleet_id)
            .map_or((0.0, self.current_tick), |p| (p.fuel_credit, p.fuel_since));
        let policy = FleetPolicy {
            preset,
            params,
            work_sector: (preset == PolicyPreset::MineAndReturn).then_some(location),
            next_eval_tick: self.current_tick + 1,
            last_hold: None,
            fuel_credit,
            fuel_since,
        };
        self.policies.insert(fleet_id, policy);
        let was_laden = self.fleets.get_mut(&fleet_id).is_some_and(|f| std::mem::take(&mut f.laden));
        if was_laden {
            self.unload_cargo(fleet_id);
        }
        let fuel = if params.fuel_budget_pct >= POLICY_FUEL_UNBOUNDED_PCT {
            "fuel unbounded".to_string()
        } else {
            format!("fuel budget {}% of deuterium income", params.fuel_budget_pct)
        };
        let reason = format!(
            "doctrine set: {} (range {}, engage {:.1}x, disengage {:.1}x, {fuel}{}){}",
            preset.label(), params.max_range, f32::from(params.engage_ratio_x10) / 10.0,
            f32::from(params.disengage_ratio_x10) / 10.0,
            if params.refine { ", refining from the hold when short of fuel" } else { "" },
            if was_laden { "; the hold loaded by hand goes back to the stockpile" } else { "" },
        );
        self.push_policy_event(fleet_id, preset, "engage", &reason);
        Ok(())
    }

    fn push_policy_event(&mut self, fleet_id: u64, preset: PolicyPreset, action: &str, reason: &str) {
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::PolicyAction(iac_shared::protocol::PolicyActionEvent {
                fleet_id,
                preset,
                action: action.to_string(),
                reason: reason.to_string(),
            }),
        });
    }

    /// The autopilot: fleets with standing orders act when free.
    fn process_policies(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tick = self.current_tick;
        let fleet_ids: Vec<u64> = self.policies.keys().copied().collect();

        for fid in fleet_ids {
            let Some(policy) = self.policies.get(&fid) else { continue; };
            let preset = policy.preset;
            if tick < policy.next_eval_tick { continue; }

            let Some(fleet) = self.fleets.get(&fid) else {
                // Fleet merged or died; orders die with it.
                self.policies.remove(&fid);
                continue;
            };
            if fleet.ships.is_empty() { continue; }
            if fleet.state == FleetStatus::InCombat {
                if let Some(p) = self.policies.get_mut(&fid) {
                    p.next_eval_tick = tick + POLICY_EVAL_INTERVAL as u64;
                }
                self.policy_disengage(fid, preset);
                continue;
            }
            // Only steer a fleet that is free to act.
            if fleet.state != FleetStatus::Idle && fleet.state != FleetStatus::Docked { continue; }
            if fleet.action_cooldown > 0 || fleet.move_cooldown > 0 { continue; }

            if let Some(p) = self.policies.get_mut(&fid) {
                p.next_eval_tick = tick + POLICY_EVAL_INTERVAL as u64;
            }

            let wait = self.policy_unload_wait(fid).or_else(|| self.policy_fuel_wait(fid));
            let acted = if let Some(why) = self.policy_pursued(fid) {
                self.policy_head_home(fid, preset, &why)
            } else if let Some((why, at_home)) = self.policy_hull_low(fid) {
                if at_home {
                    self.hold_policy(fid, preset, &format!("waiting at the dock to be mended: {why}"))
                } else {
                    self.policy_head_home(fid, preset, &format!("returning to be mended: {why}"))
                }
            } else if let Some(why) = wait {
                self.hold_policy(fid, preset, &why)
            } else {
                match preset {
                    PolicyPreset::Manual => {
                        self.policies.remove(&fid);
                        true
                    }
                    PolicyPreset::Prospect => self.policy_prospect(fid)?,
                    PolicyPreset::MineAndReturn => self.policy_mine_and_return(fid)?,
                    PolicyPreset::SalvageAndSites => self.policy_salvage_and_sites(fid)?,
                    PolicyPreset::PatrolHome => self.policy_patrol_home(fid)?,
                }
            };

            // Nothing to do — check back later instead of spinning.
            if !acted && let Some(p) = self.policies.get_mut(&fid) {
                p.next_eval_tick = tick + POLICY_EVAL_INTERVAL as u64 * 5;
            }
        }
        Ok(())
    }

    /// Deuterium a fleet under a bounded doctrine may put into its tank now:
    /// what was left after the last fill plus its share of the current
    /// deuterium production since, at most the price of a full tank.
    fn fuel_allowance(&self, fleet: &Fleet, policy: &FleetPolicy) -> f64 {
        let earned = self.fuel_income(fleet.owner_id, &policy.params)
            * self.current_tick.saturating_sub(policy.fuel_since) as f64;
        (policy.fuel_credit + earned).min(fleet.fuel_max * FUEL_DEUT_PER_UNIT)
    }

    /// Deuterium per tick a doctrine's fuel budget grows by.
    fn fuel_income(&self, owner: u64, params: &PolicyParams) -> f64 {
        let Some(player) = self.players.get(&owner) else { return 0.0; };
        player.production_per_tick(&self.world.pace).deuterium.max(0.0) * f64::from(params.fuel_budget_pct) / 100.0
    }

    /// Hull lost stays lost until a dock mends it, and a mauled fleet is
    /// brittle. A fleet under orders whose hull has fallen below
    /// `POLICY_HULL_READY` of full does nothing else until the home dock
    /// has mended it: the reason, and whether it already stands there.
    fn policy_hull_low(&self, fleet_id: u64) -> Option<(String, bool)> {
        let fleet = self.fleets.get(&fleet_id)?;
        let (held, full) = fleet.ships.hull(fleet.arms());
        if full == 0 || held as f32 >= POLICY_HULL_READY * full as f32 { return None; }
        let at_home = self.players.get(&fleet.owner_id).is_some_and(|p| p.homeworld == fleet.location);
        Some((
            format!(
                "hull at {:.0}% of full, and a fleet under orders works at {:.0}% or more",
                100.0 * held as f32 / full as f32,
                100.0 * POLICY_HULL_READY,
            ),
            at_home,
        ))
    }

    /// A fleet under orders with a group on its trail runs for home before
    /// it does anything else: the reason, for a fleet away from home.
    fn policy_pursued(&self, fleet_id: u64) -> Option<String> {
        let fleet = self.fleets.get(&fleet_id)?;
        if self.players.get(&fleet.owner_id).is_some_and(|p| p.homeworld == fleet.location) { return None; }
        let pursuit = self.pursuit_of(fleet)?;
        Some(format!(
            "running for home: the {} is on its trail and lands here in {} tick{}",
            pursuit.label, pursuit.arrives_in, if pursuit.arrives_in == 1 { "" } else { "s" },
        ))
    }

    /// The disengage rule, for a fleet under orders that stands in a fight:
    /// when its odds in the fight as it now stands are below the doctrine's
    /// `disengage_ratio_x10` it jumps out toward home, and says why. A fleet
    /// that cannot leave says that, once. True if it left.
    fn policy_disengage(&mut self, fleet_id: u64, preset: PolicyPreset) -> bool {
        let Some(fleet) = self.fleets.get(&fleet_id).cloned() else { return false; };
        let Some(params) = self.policies.get(&fleet_id).map(|p| p.params) else { return false; };
        let Some(home) = self.players.get(&fleet.owner_id).map(|p| p.homeworld) else { return false; };
        let Some(quote) = self.flight_quote(&fleet, None) else { return false; };
        if quote.ratio * 10.0 >= f32::from(params.disengage_ratio_x10) { return false; }
        let at = fleet.location;
        let odds = format!(
            "odds fell to {:.2}x, {}, below the doctrine's {:.1}x",
            quote.ratio, quote.label.label(), f32::from(params.disengage_ratio_x10) / 10.0,
        );
        let way = quote.refusal.clone().map_or_else(|| self.policy_reach(&fleet, &params, home).way_out(at), Err);
        let left = way.and_then(|step| {
            self.handle_move(fleet.owner_id, fleet_id, step)
                .map(|()| step)
                .map_err(|code| format!("the jump to {step} was refused ({code:?})"))
        });
        match left {
            Ok(step) => {
                let rounds = quote.rounds;
                self.push_policy_event(fleet_id, preset, "flee", &format!(
                    "left the fight at {at}: {odds}; jumping to {step}, under fire for {rounds} round{}",
                    if rounds == 1 { "" } else { "s" },
                ));
                true
            }
            Err(why) => self.hold_policy(fleet_id, preset, &format!("cannot leave the fight at {at} ({odds}): {why}")),
        }
    }

    /// A full store leaves cargo aboard a docked fleet, and a fleet that
    /// flew out again with it would turn straight back: the reason it stays
    /// docked until the cargo is off, if it must.
    fn policy_unload_wait(&self, fleet_id: u64) -> Option<String> {
        let fleet = self.fleets.get(&fleet_id)?;
        let policy = self.policies.get(&fleet_id)?;
        if fleet.location != self.players.get(&fleet.owner_id)?.homeworld { return None; }
        let hold = fleet_cargo_capacity(fleet, &self.world.pace);
        let aboard = fleet.cargo.total();
        (hold > 0.0 && aboard / hold * 100.0 >= f64::from(policy.params.cargo_return_pct)).then(|| {
            format!("the stockpile has no room for the cargo aboard ({:.0}% of the hold); waiting to unload", aboard / hold * 100.0)
        })
    }

    /// A fleet on a fuel budget leaves the dock with a full tank or not at
    /// all: the reason it is still waiting, if it is.
    fn policy_fuel_wait(&self, fleet_id: u64) -> Option<String> {
        let fleet = self.fleets.get(&fleet_id)?;
        let policy = self.policies.get(&fleet_id)?;
        let player = self.players.get(&fleet.owner_id)?;
        let pct = policy.params.fuel_budget_pct;
        if pct >= POLICY_FUEL_UNBOUNDED_PCT || fleet.location != player.homeworld || fleet.fuel >= fleet.fuel_max {
            return None;
        }
        let tank = format!("tank {:.0} of {:.0}", fleet.fuel, fleet.fuel_max);
        let cost = (fleet.fuel_max - fleet.fuel) * FUEL_DEUT_PER_UNIT;
        let short = cost - self.fuel_allowance(fleet, policy);
        if short <= 0.0 {
            return Some(format!("refuelling: the stockpile lacks the deuterium to fill the tank ({tank}, {cost:.0} deuterium)"));
        }
        let income = self.fuel_income(fleet.owner_id, &policy.params);
        if income <= 0.0 {
            return Some(format!(
                "refuelling: a fuel budget of {pct}% of deuterium income earns nothing ({tank}); raise fuel_budget_pct or wait for the store to fill",
            ));
        }
        Some(format!(
            "refuelling on a fuel budget of {pct}% of deuterium income ({tank}, full in about {} s)",
            (short / income).ceil() as u64,
        ))
    }

    /// Most hops from home the fleet can fly and still afford the way back
    /// with the doctrine's fuel reserve intact, capped by `max_range`.
    fn policy_effective_range(&self, fleet: &Fleet, params: &PolicyParams) -> u8 {
        let hop = self.hop_fuel_cost(fleet);
        if hop <= 0.0 { return params.max_range; }
        let reserve = fleet.fuel_max * f64::from(params.min_fuel_pct) / 100.0;
        let hops = ((fleet.fuel_max - reserve) / (2.0 * hop)).floor().clamp(0.0, 255.0) as u8;
        params.max_range.min(hops)
    }

    /// "within N hops of home", noting when the tank rather than max_range
    /// is the limit.
    fn range_phrase(effective: u8, params: &PolicyParams) -> String {
        if effective < params.max_range {
            format!("within {effective} hops of home (the fuel tank limits range; max_range is {})", params.max_range)
        } else {
            format!("within {} hops of home", params.max_range)
        }
    }

    /// Fuel/cargo doctrine thresholds: the reason to head home, if any. Fuel
    /// is judged against the cost of the direct route home, the fewest hops
    /// over the real lanes, charted or not, plus the doctrine's reserve.
    /// It is not a fraction of the tank.
    fn policy_return_reason(&self, fleet: &Fleet, params: &PolicyParams) -> Option<Homeward> {
        let need = self.route_home_cost(fleet, fleet.location);
        let reserve = fleet.fuel_max * f64::from(params.min_fuel_pct) / 100.0;
        if fleet.fuel < need + reserve {
            return Some(Homeward::Fuel(format!(
                "fuel {:.0} is below the route home {:.0} plus reserve {:.0}",
                fleet.fuel, need, reserve,
            )));
        }
        let max_cargo = fleet_cargo_capacity(fleet, &self.world.pace);
        if max_cargo > 0.0 {
            let used = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
            let cargo_pct = used / max_cargo * 100.0;
            if cargo_pct >= f64::from(params.cargo_return_pct) {
                return Some(Homeward::Cargo(format!("cargo {:.0}%", cargo_pct)));
            }
        }
        None
    }

    /// Act on those thresholds for a fleet away from home; None when
    /// neither sends it home. Short of fuel, a fleet whose orders say so
    /// refines first and is weighed again at its next evaluation; otherwise
    /// it turns home. True if the autopilot acted.
    fn policy_homeward(&mut self, fleet_id: u64, preset: PolicyPreset, fleet: &Fleet, params: &PolicyParams) -> Option<bool> {
        let reason = self.policy_return_reason(fleet, params)?;
        if let Homeward::Fuel(short) = &reason
            && self.policy_refine(fleet_id, preset, short)
        {
            return Some(true);
        }
        let why = match reason {
            Homeward::Cargo(full) if preset == PolicyPreset::PatrolHome => format!("returning to unload the salvage: {full}"),
            Homeward::Fuel(why) | Homeward::Cargo(why) => format!("returning: {why}"),
        };
        Some(self.policy_head_home(fleet_id, preset, &why))
    }

    /// A fleet whose orders have `refine` set turns the deuterium in its
    /// hold into fuel when it is `short`, and reports both figures. True if
    /// it refined anything: with no deuterium aboard or a full tank it
    /// did not, and the doctrine goes on to what it would have done.
    fn policy_refine(&mut self, fleet_id: u64, preset: PolicyPreset, short: &str) -> bool {
        if !self.policies.get(&fleet_id).is_some_and(|p| p.params.refine) { return false; }
        let Some(Refining { deuterium, fuel }) = self.refine_hold(fleet_id) else { return false; };
        let why = format!(
            "refined {} deuterium from the hold into {} fuel: {short}",
            fuel_words(deuterium), fuel_words(fuel),
        );
        self.push_policy_event(fleet_id, preset, "refine", &why);
        true
    }

    /// Whether a hold for `reason` is news: the same reason is announced at
    /// most once a minute.
    fn hold_is_news(&mut self, fleet_id: u64, reason: &str) -> bool {
        let tick = self.current_tick;
        match self.policies.get_mut(&fleet_id) {
            Some(p) => {
                let key = hold_key(reason);
                let repeat = matches!(&p.last_hold, Some((k, t)) if *k == key && tick < t + 60);
                if !repeat { p.last_hold = Some((key, tick)); }
                !repeat
            }
            None => true,
        }
    }

    /// Announce that the autopilot is standing still and why. Always
    /// returns false.
    fn hold_policy(&mut self, fleet_id: u64, preset: PolicyPreset, reason: &str) -> bool {
        if self.hold_is_news(fleet_id, reason) {
            self.push_policy_event(fleet_id, preset, "hold", reason);
        }
        false
    }

    /// The shortest path (excluding `from`, including the goal) from `from`
    /// to a sector satisfying `goal`, walking only through sectors satisfying
    /// `passable` (BFS with predecessors, bounded). The hex graph is sparse:
    /// pure greedy distance-chasing bounces off missing edges.
    fn bfs_path(
        &self,
        from: Hex,
        max_hops: u8,
        passable: &dyn Fn(Hex) -> bool,
        goal: &dyn Fn(Hex) -> bool,
    ) -> Option<Vec<Hex>> {
        self.bfs_path_by_hops(from, max_hops, passable, &|n, _| goal(n))
    }

    /// `bfs_path` with a goal that is told how many jumps out the sector lies.
    fn bfs_path_by_hops(
        &self,
        from: Hex,
        max_hops: u8,
        passable: &dyn Fn(Hex) -> bool,
        goal: &dyn Fn(Hex, u32) -> bool,
    ) -> Option<Vec<Hex>> {
        // A sector `max_hops` jumps out lies within as many hexes, so the
        // search cannot leave the window and indexes it: where each sector
        // was reached from, by slot.
        const UNSEEN: u32 = u32::MAX;
        let window = Window { centre: from, radius: max_hops };
        let start = window.slot(from)?;
        let mut prev: Vec<u32> = vec![UNSEEN; window.slots()];
        let mut frontier = vec![from];
        prev[start] = start as u32;
        let mut found: Option<usize> = None;

        'search: for hops in 1..=u32::from(max_hops) {
            let mut next = Vec::new();
            for coord in &frontier {
                let Some(at) = window.slot(*coord) else { continue; };
                for &n in self.world_gen.connected_neighbors(*coord).slice() {
                    let Some(slot) = window.slot(n) else { continue; };
                    if prev[slot] != UNSEEN || !passable(n) { continue; }
                    prev[slot] = at as u32;
                    if goal(n, hops) { found = Some(slot); break 'search; }
                    next.push(n);
                }
            }
            frontier = next;
        }

        let mut cur = found?;
        let mut path = vec![window.coord(cur)];
        while prev[cur] as usize != start {
            cur = prev[cur] as usize;
            path.push(window.coord(cur));
        }
        path.reverse();
        Some(path)
    }

    /// What the doctrine of `fleet` lets it do this evaluation.
    fn policy_reach<'a>(&'a self, fleet: &'a Fleet, params: &PolicyParams, home: Hex) -> Reach<'a> {
        let range = self.policy_effective_range(fleet, params);
        let hold = fleet_cargo_capacity(fleet, &self.world.pace);
        Reach {
            engine: self,
            fleet,
            params: *params,
            owner: fleet.owner_id,
            ours: fleet_force(fleet),
            home,
            need: f32::from(params.engage_ratio_x10) / 10.0,
            range,
            jump_weight: scaling::resource_weight(&Resources {
                deuterium: self.hop_fuel_cost(fleet) * FUEL_DEUT_PER_UNIT,
                ..Resources::default()
            }),
            hold,
            room: (hold - fleet.cargo.total()).max(0.0),
            groups: self.npc_fleets.values().filter(|n| !n.ships.is_empty()).fold(KeyMap::default(), |mut groups, n| {
                *groups.entry(n.location.to_key()).or_insert(false) |=
                    n.behavior != iac_shared::world::NpcBehaviorType::Passive;
                groups
            }),
            live: crate::intel::live_coords(self, fleet.owner_id).iter().map(|h| h.to_key()).collect(),
            window: Window { centre: home, radius: range.saturating_add(1) },
            rings: RingOdds::default(),
            rated: SectorMemo::default(),
            fights: SectorMemo::default(),
            seams: SectorMemo::default(),
            boardable: SectorMemo::default(),
        }
    }

    /// One jump toward home along the lane with the best odds: one the
    /// doctrine would take anywhere if there is one, otherwise the mildest
    /// the map offers, down to RISKY. Through DEADLY odds it does not go:
    /// the fleet holds where it is and its owner is alerted. True if the
    /// autopilot moved.
    fn policy_head_home(&mut self, fleet_id: u64, preset: PolicyPreset, reason: &str) -> bool {
        let Some(fleet) = self.fleets.get(&fleet_id).cloned() else { return false; };
        let Some(home) = self.players.get(&fleet.owner_id).map(|p| p.homeworld) else { return false; };
        let from = fleet.location;
        if from == home { return false; }
        let params = self.policies.get(&fleet_id).map(|p| p.params).unwrap_or_default();

        let way = self.policy_reach(&fleet, &params, home).way_home(from);
        match way {
            Ok((step, None)) => self.policy_move(fleet_id, step, true, preset, reason),
            Ok((step, Some(worst))) => {
                let why = format!(
                    "{reason}; no lane home meets the doctrine's {:.1}x, taking the mildest ({} at worst)",
                    f32::from(params.engage_ratio_x10) / 10.0, worst.label(),
                );
                self.policy_move(fleet_id, step, true, preset, &why)
            }
            Err(blocker) => {
                let why = format!("cut off from home: {blocker}; holding at {from}");
                if self.hold_is_news(fleet_id, &why) {
                    self.push_policy_event(fleet_id, preset, "hold", &why);
                    self.push_alert(AlertLevel::Warning, fleet_id, from, format!("fleet {fleet_id} is {why}"));
                }
                false
            }
        }
    }

    /// Issue one jump for the autopilot. A jump that is not heading home
    /// must leave enough fuel for the route back plus the reserve; if it
    /// would not, the fleet turns for home (or holds, if already there).
    fn policy_move(&mut self, fleet_id: u64, step: Hex, homeward: bool, preset: PolicyPreset, reason: &str) -> bool {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return false; };
        let (owner, from, fuel) = (fleet.owner_id, fleet.location, fleet.fuel);
        let hop = self.hop_fuel_cost(fleet);

        if !homeward {
            let min_pct = self.policies.get(&fleet_id).map(|p| p.params.min_fuel_pct).unwrap_or_default();
            let reserve = fleet.fuel_max * f64::from(min_pct) / 100.0;
            let need = f64::from(self.direct_hops_home(owner, step)) * hop + reserve;
            if fuel - hop < need {
                let why = format!(
                    "a jump to {step} would leave {:.0} fuel against {:.0} needed to return home",
                    (fuel - hop).max(0.0), need,
                );
                if self.policy_refine(fleet_id, preset, &why) { return true; }
                let home = self.players.get(&owner).map(|p| p.homeworld);
                if let Some(home) = home && from != home {
                    return self.policy_head_home(fleet_id, preset, &format!("returning: {why}"));
                }
                return self.hold_policy(fleet_id, preset, &why);
            }
        }

        match self.handle_move(owner, fleet_id, step) {
            Ok(()) => {
                if let Some(p) = self.policies.get_mut(&fleet_id) { p.last_hold = None; }
                self.push_policy_event(fleet_id, preset, "move", reason);
                true
            }
            Err(code) => {
                let why = if code == ErrorCode::InsufficientFuel {
                    format!(
                        "stranded: {fuel:.0} fuel, a jump costs {hop:.0}; the emergency reserve is recharging ({reason})",
                    )
                } else {
                    format!("cannot move ({code:?}): {reason}")
                };
                self.hold_policy(fleet_id, preset, &why);
                if let Some(p) = self.policies.get_mut(&fleet_id) {
                    p.next_eval_tick = self.current_tick + 30;
                }
                true
            }
        }
    }

    /// Head home saying `why`, or hold with it when already there.
    fn policy_stand_down(&mut self, fleet_id: u64, preset: PolicyPreset, at_home: bool, why: &str) -> bool {
        if at_home {
            self.hold_policy(fleet_id, preset, why)
        } else {
            self.policy_head_home(fleet_id, preset, &format!("returning: {why}"))
        }
    }

    /// Raw ore units the tiles of `coord` hold right now.
    fn seam_units(&self, coord: Hex) -> f64 {
        self.ore_reserve_at(coord).map_or(0.0, |ore| ore_units(&ore))
    }

    /// Score weight of `ore` as it would land in the hold of one of
    /// `owner`'s fleets with `room` units free.
    fn seam_worth(&self, owner: u64, ore: &OreReserve, room: f64) -> f32 {
        let yield_mult = self.players.get(&owner)
            .map_or(1.0, |p| scaling::harvest_yield(p.research.laser_technology));
        let load = Resources { metal: ore.metal.units, crystal: ore.crystal.units, deuterium: ore.deuterium.units }
            .scale(yield_mult);
        worth_aboard(load, room)
    }

    /// Score weight of the wreckage in `coord` that fits `room` units of hold.
    fn wreck_worth(&self, coord: Hex, room: f64) -> f32 {
        self.sector_overrides.get(&coord.to_key())
            .and_then(|o| o.salvage)
            .map_or(0.0, |pile| worth_aboard(pile, room))
    }

    /// Whether the doctrine boards the derelict at `coord`: a quiet one, or
    /// one whose guard the fleet rates at its engage ratio.
    fn site_is_boardable(&self, fleet: &Fleet, params: &PolicyParams, coord: Hex) -> bool {
        let Some((_, bumps)) = self.derelict_site_at(coord) else { return false; };
        if self.site_ambush_chance(fleet, coord, bumps) < 0.25 { return true; }
        let (class, count, multiplier) = scaling::ambush_guard(coord.dist_from_origin());
        let guard = Force::of_group(class, u32::from(count), multiplier);
        self.odds_ratio(&fleet_force(fleet), &guard) * 10.0 >= f32::from(params.engage_ratio_x10)
    }

    /// Prospect: scan when the pings have gone stale, then walk to the
    /// nearest sector inside the safe range of home whose chart the player
    /// has neither banked nor aboard a fleet (never entered, or entered by
    /// a fleet that did not come back). With nothing left to chart it goes
    /// home and holds.
    fn policy_prospect(&mut self, fleet_id: u64) -> Result<bool, Box<dyn std::error::Error>> {
        let preset = PolicyPreset::Prospect;
        let Some(fleet) = self.fleets.get(&fleet_id).cloned() else { return Ok(false); };
        let Some(params) = self.policies.get(&fleet_id).map(|p| p.params) else { return Ok(false); };
        let Some(home) = self.players.get(&fleet.owner_id).map(|p| p.homeworld) else { return Ok(false); };
        let (owner, location) = (fleet.owner_id, fleet.location);

        if location != home && let Some(acted) = self.policy_homeward(fleet_id, preset, &fleet, &params) {
            return Ok(acted);
        }

        // Scan if this position hasn't been pinged recently.
        let scanned_here = self.scan_reveals.get(&owner)
            .map(|m| m.contains_key(&location.to_key()))
            .unwrap_or(false);
        if !scanned_here && self.handle_scan(owner, fleet_id).is_ok() {
            self.push_policy_event(fleet_id, preset, "scan", "pinging the dark");
            return Ok(true);
        }

        let plan = {
            let reach = self.policy_reach(&fleet, &params, home);
            let aboard = self.charts_aboard(owner);
            let uncharted = |n: Hex| n != home && self.chart_mark(owner, n.to_key(), &aboard).is_none();
            match reach.path_to(location, &uncharted) {
                Some(path) => Ok(path[0]),
                None => Err(format!(
                    "{}; raise max_range or pick another doctrine",
                    self.prospect_blocker(&reach, location, &params, &uncharted),
                )),
            }
        };
        Ok(match plan {
            Ok(step) => self.policy_move(fleet_id, step, false, preset, "pushing frontier"),
            Err(why) => self.policy_stand_down(fleet_id, preset, location == home, &why),
        })
    }

    /// Which constraint stops prospect: the nearest uncharted sector is
    /// either behind a hazard, beyond the range limit, or there is none.
    fn prospect_blocker(
        &self,
        reach: &Reach,
        from: Hex,
        params: &PolicyParams,
        uncharted: &dyn Fn(Hex) -> bool,
    ) -> String {
        let home = reach.home;
        let within = Self::range_phrase(reach.range, params);

        if let Some(path) = self.bfs_path(from, POLICY_SEARCH_HOPS, &|n| reach.in_range(n), uncharted) {
            let near = *path.last().unwrap();
            if let Some(why) = path.iter().find_map(|&n| reach.refusal(n)) {
                return format!("no safe way to the nearest uncharted sector {near} {within}: {why}");
            }
        }
        if let Some(path) = self.bfs_path(from, POLICY_SEARCH_HOPS, &|_| true, uncharted) {
            let near = *path.last().unwrap();
            let d = Hex::distance(&near, &home);
            return format!("the nearest uncharted sector {near} is {d} hexes from home, outside {within}");
        }
        format!("every lane within {POLICY_SEARCH_HOPS} hops of {from} is charted {within}")
    }

    /// MineAndReturn: work a claim until the hold is full, bring it home,
    /// go back. A claim is the nearest seam worth a jump that the fleet may
    /// reach inside `max_range` of home; when it runs dry the next one is
    /// staked from where the fleet stands.
    fn policy_mine_and_return(&mut self, fleet_id: u64) -> Result<bool, Box<dyn std::error::Error>> {
        let preset = PolicyPreset::MineAndReturn;
        let Some(fleet) = self.fleets.get(&fleet_id).cloned() else { return Ok(false); };
        let Some(policy) = self.policies.get(&fleet_id) else { return Ok(false); };
        let (params, staked) = (policy.params, policy.work_sector);
        let Some(home) = self.players.get(&fleet.owner_id).map(|p| p.homeworld) else { return Ok(false); };
        let (owner, location) = (fleet.owner_id, fleet.location);
        let at_home = location == home;

        // Loaded or low on fuel for the way home: head home (docking
        // deposits + refuels).
        if !at_home && let Some(acted) = self.policy_homeward(fleet_id, preset, &fleet, &params) {
            return Ok(acted);
        }

        let leashed = !at_home && Hex::distance(&location, &home) <= u16::from(params.max_range);
        let on_claim = leashed && staked == Some(location);
        let on_seam = leashed && self.seam_units(location) >= POLICY_MIN_SEAM_UNITS;
        let mut ran_dry = false;
        if on_claim || on_seam {
            if self.seam_units(location) > scaling::ORE_EPSILON {
                match self.handle_harvest(owner, fleet_id, HarvestResource::Auto) {
                    Ok(()) => {
                        if !on_claim && let Some(p) = self.policies.get_mut(&fleet_id) {
                            p.work_sector = Some(location);
                        }
                        self.push_policy_event(fleet_id, preset, "harvest", "seam holds");
                        return Ok(true);
                    }
                    Err(ErrorCode::CargoFull) => {
                        return Ok(self.policy_head_home(fleet_id, preset, "returning: hold is full"));
                    }
                    Err(ErrorCode::NoResources) | Err(ErrorCode::ResourceNotPresent) => {}
                    Err(code) => {
                        let why = format!("cannot harvest ({code:?})");
                        return Ok(self.hold_policy(fleet_id, preset, &why));
                    }
                }
            }
            ran_dry = true;
        }

        let plan = {
            let reach = self.policy_reach(&fleet, &params, home);
            let ore = |n: Hex, hops: u32| n != home && reach.seam(n).is_some_and(|worth| reach.repays(worth, hops));
            let seam = |n: Hex, hops: u32| ore(n, hops) && !reach.starts_fight(n);
            let kept = staked
                .filter(|&claim| !ran_dry && claim != location)
                .and_then(|claim| reach.path_worth(location, &|n, hops| n == claim && seam(n, hops)));
            match kept.or_else(|| reach.path_worth(location, &seam)) {
                Some(path) => Ok(path),
                None => {
                    let within = Self::range_phrase(reach.range, &params);
                    Err(match self.bfs_path_by_hops(home, POLICY_SEARCH_HOPS, &|n| reach.in_range(n), &ore) {
                        None => format!("no seam {within} repays the fuel to fetch it (stripped tiles refill over hours)"),
                        Some(path) => {
                            let near = *path.last().unwrap();
                            match path.iter().find_map(|&n| reach.refusal(n)) {
                                Some(why) => format!("no way it may take to the ore at {near} {within}: {why}"),
                                None => format!("the ore at {near} {within} {NO_FIGHT_FOR_IT}"),
                            }
                        }
                    })
                }
            }
        };

        let claim = plan.as_ref().ok().and_then(|path| path.last().copied());
        if claim != staked
            && let Some(p) = self.policies.get_mut(&fleet_id)
        {
            p.work_sector = claim;
        }
        Ok(match plan {
            Ok(path) => {
                let reason = if ran_dry { "claim dry, moving to new seam" } else { "heading to claim" };
                self.policy_move(fleet_id, path[0], false, preset, reason)
            }
            Err(why) => self.policy_stand_down(fleet_id, preset, at_home, &why),
        })
    }

    /// SalvageAndSites: scoop wreckage that repays the fuel and board
    /// derelicts, nearest first, where no fight waits, by a lane without a
    /// fight where one exists.
    fn policy_salvage_and_sites(&mut self, fleet_id: u64) -> Result<bool, Box<dyn std::error::Error>> {
        let preset = PolicyPreset::SalvageAndSites;
        let Some(fleet) = self.fleets.get(&fleet_id).cloned() else { return Ok(false); };
        let Some(params) = self.policies.get(&fleet_id).map(|p| p.params) else { return Ok(false); };
        let Some(home) = self.players.get(&fleet.owner_id).map(|p| p.homeworld) else { return Ok(false); };
        let (owner, location) = (fleet.owner_id, fleet.location);
        let at_home = location == home;

        if !at_home && let Some(acted) = self.policy_homeward(fleet_id, preset, &fleet, &params) {
            return Ok(acted);
        }

        if !at_home {
            // Wreckage first: free money.
            if self.has_wreckage(location) && self.handle_collect_salvage(owner, fleet_id).is_ok() {
                self.push_policy_event(fleet_id, preset, "salvage", "scooping wreckage");
                return Ok(true);
            }
            // Then the hulk, if the odds look survivable.
            if let Some((tier, _)) = self.derelict_site_at(location)
                && self.site_is_boardable(&fleet, &params, location)
                && self.handle_explore_site(owner, fleet_id).is_ok()
            {
                let why = format!("boarding tier-{} derelict", tier);
                self.push_policy_event(fleet_id, preset, "explore", &why);
                return Ok(true);
            }
        }

        // Otherwise the nearest payday it may reach and work.
        let plan = {
            let reach = self.policy_reach(&fleet, &params, home);
            let wreck = |n: Hex, hops: u32| reach.wreck_repays(n, reach.room, hops);
            let contact = |n: Hex, hops: u32| {
                n != home && n != location && (wreck(n, hops) || self.derelict_site_at(n).is_some())
            };
            let payday = |n: Hex, hops: u32| {
                n != home && n != location && !reach.starts_fight(n)
                    && (wreck(n, hops) || reach.boards(n))
            };
            match reach.path_worth(location, &payday) {
                Some(path) => Ok(path[0]),
                None => {
                    let within = Self::range_phrase(reach.range, &params);
                    Err(match self.bfs_path_by_hops(home, POLICY_SEARCH_HOPS, &|n| reach.in_range(n), &contact) {
                        None => format!("board is clear {within}: no derelict, no wreckage that repays the fuel"),
                        Some(path) => {
                            let near = *path.last().unwrap();
                            match path.iter().find_map(|&n| reach.refusal(n)) {
                                Some(why) => format!("no way it may take to the contact at {near} {within}: {why}"),
                                None if reach.starts_fight(near) => {
                                    format!("the contact at {near} {within} {NO_FIGHT_FOR_IT}")
                                }
                                None => format!(
                                    "the derelict at {near} {within} is too hot to board: its guard outrates the doctrine's {:.1}x",
                                    reach.need,
                                ),
                            }
                        }
                    })
                }
            }
        };
        Ok(match plan {
            Ok(step) => self.policy_move(fleet_id, step, false, preset, "contact on the board"),
            Err(why) => self.policy_stand_down(fleet_id, preset, at_home, &why),
        })
    }

    fn has_wreckage(&self, coord: Hex) -> bool {
        self.sector_overrides.get(&coord.to_key()).is_some_and(|o| o.salvage.is_some())
    }

    /// PatrolHome: hunt the hostiles inside `max_range` of home that the
    /// fleet rates at its engage ratio, and between fights collect the
    /// wreckage in range that repays its fuel, the kill's or any other,
    /// nearest first. It takes the hold home when it is full, when the next
    /// pile would not fit what room is left, and when there is nothing left
    /// to fight or collect; there it unloads, is mended and goes out again.
    fn policy_patrol_home(&mut self, fleet_id: u64) -> Result<bool, Box<dyn std::error::Error>> {
        let preset = PolicyPreset::PatrolHome;
        let Some(fleet) = self.fleets.get(&fleet_id).cloned() else { return Ok(false); };
        let Some(params) = self.policies.get(&fleet_id).map(|p| p.params) else { return Ok(false); };
        let Some(home) = self.players.get(&fleet.owner_id).map(|p| p.homeworld) else { return Ok(false); };
        let (owner, location) = (fleet.owner_id, fleet.location);
        let at_home = location == home;

        if !at_home && let Some(acted) = self.policy_homeward(fleet_id, preset, &fleet, &params) {
            return Ok(acted);
        }

        // Engage what we can beat, where we stand.
        let hostiles_here = self.npc_fleets.values().any(|n| n.location == location && !n.ships.is_empty())
            || self.pending_template_npc(location).is_some();
        if hostiles_here {
            let ratio = self.odds_ratio(&fleet_force(&fleet), &self.sector_force(location));
            if ratio * 10.0 < f32::from(params.engage_ratio_x10) {
                let why = format!("hostiles at {location} too strong (the fleet rates {ratio:.2}x against them)");
                return Ok(self.policy_stand_down(fleet_id, preset, at_home, &why));
            }
            let target_id = self.npc_fleets.values()
                .find(|n| n.location == location && !n.ships.is_empty())
                .map(|n| n.id)
                .unwrap_or_else(|| template_npc_id(location));
            if self.handle_attack(owner, fleet_id, target_id).is_ok() {
                self.push_policy_event(fleet_id, preset, "attack", "engaging hostiles");
                return Ok(true);
            }
        }

        if !at_home && self.has_wreckage(location) && self.handle_collect_salvage(owner, fleet_id).is_ok() {
            let killed_here = self.policies.get(&fleet_id).is_some_and(|p| p.work_sector == Some(location));
            let why = if killed_here {
                "scooping the wreckage of the kill".to_string()
            } else {
                format!("scooping wreckage at {location} that it did not make")
            };
            self.push_policy_event(fleet_id, preset, "salvage", &why);
            return Ok(true);
        }

        enum Next {
            Go(Hex, String),
            Unload(String),
            Idle(String),
        }
        let next = {
            let reach = self.policy_reach(&fleet, &params, home);
            let target = |n: Hex, hops: u32| {
                n != home && (reach.has_hostiles(n) || reach.wreck_repays(n, reach.hold, hops))
            };
            match reach.path_through(location, &target) {
                Some(path) => {
                    let (at, hops) = (path[path.len() - 1], path.len() as u32);
                    if reach.has_hostiles(at) {
                        Next::Go(path[0], "intercepting contact".to_string())
                    } else if reach.wreck_repays(at, reach.room, hops) {
                        Next::Go(path[0], format!("collecting the wreckage at {at}"))
                    } else {
                        let pile = self.sector_overrides.get(&at.to_key()).and_then(|o| o.salvage).unwrap_or_default();
                        Next::Unload(format!(
                            "the hold has room for {:.0} of the {:.0} units of wreckage at {at}",
                            reach.room, pile.total(),
                        ))
                    }
                }
                None => Next::Idle(format!(
                    "no contact {} that the fleet rates {:.1}x against, and no wreckage that repays the fuel",
                    Self::range_phrase(reach.range, &params), reach.need,
                )),
            }
        };
        Ok(match next {
            Next::Go(step, why) => self.policy_move(fleet_id, step, false, preset, &why),
            Next::Unload(why) if at_home => {
                self.hold_policy(fleet_id, preset, &format!("waiting at the dock for the store to take the cargo aboard: {why}"))
            }
            Next::Unload(why) => self.policy_head_home(fleet_id, preset, &format!("returning to unload the salvage: {why}")),
            Next::Idle(why) => self.policy_stand_down(fleet_id, preset, at_home, &why),
        })
    }

    // ── Helper Methods ────────────────────────────────────────────

    /// True if the sector holds a live NPC fleet or an unspawned,
    /// uncleared NPC template.
    fn sector_has_hostiles(&self, coord: Hex) -> bool {
        if self.npc_fleets.values().any(|n| n.location == coord && !n.ships.is_empty()) {
            return true;
        }
        let cleared = self.sector_overrides.get(&coord.to_key())
            .map(|o| o.npc_cleared_tick.is_some())
            .unwrap_or(false);
        !cleared && self.outpost_at(coord).is_none() && self.world_gen.generate_sector(coord).npc_template.is_some()
    }

    /// What a scanner hears at the edge of its range: the strongest
    /// signature in the sector, or nothing.
    fn faint_signal_at(&self, coord: Hex) -> Option<iac_shared::protocol::SignalKind> {
        use iac_shared::protocol::SignalKind;
        if self.sector_has_hostiles(coord) {
            return Some(SignalKind::HostileMass);
        }
        if self.derelict_site_at(coord).is_some() {
            return Some(SignalKind::Derelict);
        }
        let ov = self.sector_overrides.get(&coord.to_key());
        if ov.and_then(|o| o.salvage).is_some() {
            return Some(SignalKind::Derelict);
        }
        let template = self.world_gen.generate_sector(coord);
        let (m, c, d) = SectorOverride::effective_densities(ov, &template);
        if [m, c, d].iter().any(|&den| den as u8 >= Density::Rich as u8) {
            return Some(SignalKind::RichOre);
        }
        if template.terrain == iac_shared::constants::TerrainType::Anomaly {
            return Some(SignalKind::Anomaly);
        }
        None
    }

    /// Sectors this player is actively scanning (unexpired reveals).
    pub fn scan_revealed_coords(&self, player_id: u64) -> Vec<Hex> {
        self.scan_reveals.get(&player_id)
            .map(|m| {
                m.iter()
                    .filter(|(_, expiry)| **expiry > self.current_tick)
                    .map(|(key, _)| Hex::from_key(*key))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn prune_scan_reveals(&mut self) {
        let tick = self.current_tick;
        for reveals in self.scan_reveals.values_mut() {
            reveals.retain(|_, &mut expiry| expiry > tick);
        }
        self.scan_reveals.retain(|_, m| !m.is_empty());
    }

    fn ensure_override(&mut self, sector_key: u32) -> &mut SectorOverride {
        self.sector_overrides
            .entry(sector_key)
            .or_default()
    }

    /// Fleets away from both docks: the homeworld and the owner's finished outpost.
    fn count_deployed_fleets(&self, player_id: u64, homeworld: Hex) -> usize {
        self.fleets.values().filter(|f| {
            f.owner_id == player_id && f.location != homeworld && !f.ships.is_empty() && !self.docked_at_outpost(f)
        }).count()
    }

    /// The free home seat farthest from every occupied one, so arrivals
    /// spread out and nobody picks. Equally far seats are split by a draw
    /// from the world seed: each world fills in its own order, and a world
    /// replays the same one. None when every seat is taken.
    fn next_home_seat(&self) -> Option<Hex> {
        let homes: Vec<Hex> = self.players.values().map(|p| p.homeworld).collect();
        world::home_seats()
            .filter(|seat| !homes.contains(seat))
            .max_by_key(|seat| {
                let nearest = homes.iter().map(|home| Hex::distance(home, seat)).min();
                (nearest.unwrap_or(u16::MAX), self.world_gen.sector_seed(*seat))
            })
    }

    fn check_homeworld_docking(&mut self, fleet_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let fleet = self.fleets.get(&fleet_id).ok_or("Fleet not found")?;
        let player = self.players.get(&fleet.owner_id).ok_or("Player not found")?;
        if fleet.location != player.homeworld { return Ok(()); }
        self.dock_fleet(fleet_id)
    }

    fn dock_fleet(&mut self, fleet_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let fleet = self.fleets.get(&fleet_id).ok_or("Fleet not found")?;
        let player_id = fleet.owner_id;

        // Deposit cargo and refuel; docked fleets stay separate until the
        // player merges them.
        let aboard = fleet.cargo;
        self.unload_cargo(fleet_id);
        self.report_unload(fleet_id, aboard);
        self.deliver_charts(fleet_id);

        let player = self.players.get(&player_id).ok_or("Player not found")?;
        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.fuel_max = fleet_fuel_max(fleet, player);
        fleet.fuel = fleet.fuel.min(fleet.fuel_max);
        self.refuel_fleet(fleet_id);
        self.tend_at_home(fleet_id, true);

        Ok(())
    }

    /// A fleet under orders that docks with a hold says what went into the
    /// stockpile, and what stays aboard for want of room.
    fn report_unload(&mut self, fleet_id: u64, aboard: Resources) {
        let Some(preset) = self.policies.get(&fleet_id).map(|p| p.preset) else { return; };
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let (left, stored) = (fleet.cargo, aboard.sub(fleet.cargo));
        if stored.total() <= 0.0 { return; }
        let why = if left.total() > 0.0 {
            format!(
                "unloaded {} into the stockpile; {} stay aboard until the store has room",
                crate::commands::amount_words(stored), crate::commands::amount_words(left),
            )
        } else {
            format!("unloaded {} into the stockpile", crate::commands::amount_words(stored))
        };
        self.push_policy_event(fleet_id, preset, "unload", &why);
    }

    fn check_npc_encounter(&mut self, fleet_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let fleet = self.fleets.get(&fleet_id).ok_or("Fleet not found")?;
        let location = fleet.location;

        // Check existing patrol NPCs
        let npc_ids: Vec<u64> = self.npc_fleets.keys().copied().collect();
        for npc_id in npc_ids {
            let npc = self.npc_fleets.get(&npc_id).unwrap();
            if npc.location == location && !npc.in_combat {
                if npc.behavior == iac_shared::world::NpcBehaviorType::Passive { continue; }
                return self.start_combat(fleet_id, npc_id);
            }
        }

        if let Some(npc_tmpl) = self.pending_template_npc(location) {
            if npc_tmpl.behavior == iac_shared::world::NpcBehaviorType::Passive { return Ok(()); }
            let spawned = self.spawn_npc_fleet(location, npc_tmpl, template_npc_id(location))?;
            self.start_combat(fleet_id, spawned.id)?;
        }
        Ok(())
    }

    /// The sector's template NPC while it has not materialised (or has since
    /// been cleared or has wandered off as a live fleet): the hostile a sector
    /// view lists under `template_npc_id(coord)`.
    pub fn pending_template_npc(&self, coord: Hex) -> Option<iac_shared::world::NpcTemplate> {
        let template = self.world_gen.generate_sector(coord);
        template.npc_template.as_ref()?;
        self.pending_template_in(coord, &template, self.sector_overrides.get(&coord.to_key()))
    }

    /// `pending_template_npc` for a caller that holds the sector's template and override.
    pub(crate) fn pending_template_in(
        &self,
        coord: Hex,
        template: &world::SectorTemplate,
        ov: Option<&SectorOverride>,
    ) -> Option<iac_shared::world::NpcTemplate> {
        let cleared = ov.is_some_and(|o| o.npc_cleared_tick.is_some());
        if template.npc_template.is_none()
            || cleared
            || self.npc_fleets.contains_key(&template_npc_id(coord))
            || self.outpost_at(coord).is_some()
        {
            return None;
        }
        template.npc_template.clone()
    }

    fn spawn_npc_fleet(&mut self, location: Hex, npc: iac_shared::world::NpcTemplate, npc_fleet_id: u64) -> Result<NpcFleet, Box<dyn std::error::Error>> {
        let count = u32::from(npc.count).min(scaling::NPC_MAX_SHIPS as u32);
        let ships = Ships::of(&[(npc.ship_class, count)]);
        let arms = Arms::of_grade(npc.stat_multiplier);

        let npc_fleet = NpcFleet {
            id: npc_fleet_id,
            location,
            power: ships.power(arms),
            muster: ships.clone(),
            ships,
            arms,
            behavior: npc.behavior,
            home_sector: location,
            in_combat: false,
            label: npc.label(),
            bounty: npc.ship_class.build_cost().scale(f64::from(count)),
            rested: 0,
            strayed: 0,
            errand: None,
        };

        self.npc_fleets.insert(npc_fleet_id, npc_fleet.clone());
        Ok(npc_fleet)
    }

    fn start_combat(&mut self, fleet_id: u64, npc_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let fleet = self.fleets.get(&fleet_id).ok_or("Fleet not found")?;
        if !self.npc_fleets.contains_key(&npc_id) { return Err("NPC not found".into()); }
        let sector = fleet.location;
        let owner = self.players.get(&fleet.owner_id).map(|p| p.name.clone()).unwrap_or_default();

        // A sector under raid holds that fight and no other.
        if self.active_combats.values().any(|c| c.sector == sector && c.siege.is_some()) {
            return Ok(());
        }

        // Check for existing combat in same sector
        for existing in self.active_combats.values() {
            if existing.sector == sector {
                let existing_id = existing.id;
                let add_player = !existing.has_player_fleet(fleet_id);
                let add_npc = !existing.has_npc_fleet(npc_id);

                if add_player {
                    if let Some(combat_mut) = self.active_combats.get_mut(&existing_id) {
                        combat_mut.add_player_fleet(fleet_id);
                    }
                    self.engage_fleet(fleet_id);
                }
                if add_npc {
                    if let Some(combat_mut) = self.active_combats.get_mut(&existing_id) {
                        combat_mut.add_npc_fleet(npc_id);
                    }
                    self.engage_group(npc_id);
                }

                self.pending_events.push(GameEvent {
                    tick: self.current_tick,
                    kind: EventKind::CombatStarted(iac_shared::protocol::CombatStartedEvent {
                        player_fleet_id: fleet_id,
                        owner,
                        enemy_fleet_id: npc_id,
                        enemy: self.npc_fleets.get(&npc_id).map(|n| n.label.clone()).unwrap_or_default(),
                        sector,
                        mine: false,
                    }),
                });
                self.enroll_all_player_fleets_in_sector_combat(&existing_id)?;
                return Ok(());
            }
        }

        // Create new combat
        let combat_id = self.next_id();
        let mut new_combat = Combat {
            id: combat_id,
            sector,
            player_fleet_ids: Vec::new(),
            npc_fleet_ids: Vec::new(),
            round: 0,
            peak_power: 0.0,
            quiet_rounds: 0,
            fled: Vec::new(),
            losses: Vec::new(),
            last_round: None,
            siege: None,
        };
        new_combat.add_player_fleet(fleet_id);
        new_combat.add_npc_fleet(npc_id);
        self.active_combats.insert(combat_id, new_combat);

        self.engage_fleet(fleet_id);
        self.engage_group(npc_id);

        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::CombatStarted(iac_shared::protocol::CombatStartedEvent {
                player_fleet_id: fleet_id,
                owner,
                enemy_fleet_id: npc_id,
                enemy: self.npc_fleets.get(&npc_id).map(|n| n.label.clone()).unwrap_or_default(),
                sector,
                mine: false,
            }),
        });

        self.enroll_all_player_fleets_in_sector_combat(&combat_id)?;
        Ok(())
    }

    fn enroll_all_player_fleets_in_sector_combat(&mut self, combat_id: &u64) -> Result<(), Box<dyn std::error::Error>> {
        let combat = self.active_combats.get(combat_id).unwrap();
        let sector = combat.sector;
        let existing_fleet_ids: Vec<u64> = combat.player_fleet_ids.clone();
        let _ = combat;

        let fleet_ids: Vec<u64> = self.fleets.keys().copied().collect();
        for fid in fleet_ids {
            let f = self.fleets.get(&fid).unwrap();
            if f.location != sector { continue; }
            if f.state == FleetStatus::InCombat { continue; }
            if f.ships.is_empty() { continue; }
            if existing_fleet_ids.contains(&fid) { continue; }

            if let Some(combat_mut) = self.active_combats.get_mut(combat_id) {
                combat_mut.add_player_fleet(fid);
            }
            self.engage_fleet(fid);
        }
        Ok(())
    }

    /// Put a group into a fight: the jump it was charging is abandoned, and
    /// the clock of its mending stops.
    fn engage_group(&mut self, group_id: u64) {
        if let Some(group) = self.npc_fleets.get_mut(&group_id) {
            group.in_combat = true;
            group.errand = None;
            group.rested = 0;
        }
    }

    /// The fight `fleet_id` stands in, firing or leaving.
    pub fn combat_of(&self, fleet_id: u64) -> Option<&Combat> {
        self.active_combats.values().find(|c| c.has_player_fleet(fleet_id))
    }

    /// Take a fleet that has left the sector out of the fight it was in:
    /// `jumped` out of it under fire, or gone home by the emergency jump.
    /// The fight goes on for the fleets that stay; with none left it is
    /// over, and a group may follow those that jumped. A raid's fight goes
    /// on while its battery stands, and with nothing left to defend the
    /// dock it is lost; raiders follow nobody.
    fn leave_combat(&mut self, fleet_id: u64, jumped: bool) {
        let Some(combat) = self.active_combats.values_mut().find(|c| c.has_player_fleet(fleet_id)) else { return; };
        let combat_id = combat.id;
        combat.player_fleet_ids.retain(|id| *id != fleet_id);
        if jumped {
            combat.fled.push(fleet_id);
        }
        let battery = combat.siege.as_ref().map(Siege::battery_stands);
        if battery.is_some() {
            self.enlist_garrison(combat_id);
        }
        let Some(combat) = self.active_combats.get(&combat_id) else { return; };
        if battery == Some(true) || !self.fleets_fighting(combat).is_empty() {
            return;
        }
        let outcome = if jumped { BattleOutcome::Fled } else { BattleOutcome::Withdrawn };
        if battery.is_some() {
            self.end_siege(combat_id, outcome);
        } else {
            let withdrew = self.fleets.get(&fleet_id).and_then(|f| self.players.get(&f.owner_id)).map(|p| p.name.clone());
            self.end_combat(combat_id, outcome, withdrew);
        }
    }

    /// Put a fleet into a fight. A fleet that is charging a jump keeps it:
    /// it is leaving, so it is fired on without answering until the jump
    /// lands, and its owner is told. A boarding is abandoned, and told of:
    /// the timer was the boarding's, and a fight is not a backing-out, so
    /// the site is not put on alert. Standing orders look at the fight at
    /// their next turn.
    fn engage_fleet(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let at = fleet.location;
        let boarding = fleet.state == FleetStatus::Exploring;
        let leaving = match (fleet.state, fleet.move_target) {
            (FleetStatus::Moving, Some(target)) => Some((target, fleet.move_cooldown + 1)),
            _ => None,
        };
        if let Some((target, ticks)) = leaving {
            self.push_alert(AlertLevel::Warning, fleet_id, at, format!(
                "fleet {fleet_id} was caught at {at} while charging its jump to {target}: \
                 it is fired on and does not answer until the jump lands in {ticks} tick{}",
                if ticks == 1 { "" } else { "s" },
            ));
        }
        if boarding {
            self.push_alert(AlertLevel::Warning, fleet_id, at, format!(
                "fleet {fleet_id} was caught in a fight at {at} while boarding the derelict: \
                 the boarding is abandoned and nothing was recovered",
            ));
        }
        let tick = self.current_tick;
        if let Some(policy) = self.policies.get_mut(&fleet_id) {
            policy.next_eval_tick = policy.next_eval_tick.min(tick + 1);
        }
        if let Some(fleet) = self.fleets.get_mut(&fleet_id) {
            if boarding {
                fleet.action_cooldown = 0;
            }
            if leaving.is_none() {
                fleet.state = FleetStatus::InCombat;
                fleet.move_target = None;
                fleet.move_cooldown = 0;
            }
        }
    }

    fn collect_salvage(&mut self, fleet_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let fleet = self.fleets.get(&fleet_id).ok_or("Fleet not found")?;
        let key = fleet.location.to_key();

        let ov = self.sector_overrides.get_mut(&key).ok_or("No override")?;
        let salvage = ov.salvage.ok_or("No salvage")?;

        let fleet = self.fleets.get(&fleet_id).unwrap();
        let max_cargo = fleet_cargo_capacity(fleet, &self.world.pace);
        let used_cargo = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
        let mut remaining = (max_cargo - used_cargo).max(0.0);
        if remaining <= 0.0 { return Ok(()); }

        let mut collected = Resources {
            metal: salvage.metal.min(remaining),
            ..Default::default()
        };
        remaining -= collected.metal;
        collected.crystal = salvage.crystal.min(remaining);
        remaining -= collected.crystal;
        collected.deuterium = salvage.deuterium.min(remaining);

        if let Some(fleet) = self.fleets.get_mut(&fleet_id) {
            fleet.cargo.metal += collected.metal;
            fleet.cargo.crystal += collected.crystal;
            fleet.cargo.deuterium += collected.deuterium;
        }

        // A full hold doesn't vaporize the rest of the wreckage — what
        // wasn't scooped stays for another trip.
        let remainder = salvage.sub(collected);
        let left_behind = (remainder.metal + remainder.crystal + remainder.deuterium > 1.0).then_some(remainder);
        if left_behind.is_some() {
            ov.salvage = left_behind;
        } else {
            ov.salvage = None;
            ov.salvage_despawn_tick = None;
        }

        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::SalvageCollected(iac_shared::protocol::SalvageCollectedEvent {
                fleet_id,
                sector: Hex::from_key(key),
                resources: collected,
                remaining: left_behind,
            }),
        });
        Ok(())
    }

    /// Add `pile` to the wreckage in `sector` and restart its despawn clock.
    /// Returns the pile as it now lies there (a fresh kill on top of an
    /// uncollected one stacks) together with its despawn tick.
    fn drop_salvage(&mut self, sector: Hex, pile: Resources) -> (Resources, u64) {
        let key = sector.to_key();
        let despawn = self.current_tick + SALVAGE_DESPAWN_TICKS as u64;
        let ov = self.ensure_override(key);
        let total = ov.salvage.unwrap_or_default().add(pile);
        ov.salvage = Some(total);
        ov.salvage_despawn_tick = Some(despawn);
        (total, despawn)
    }

    fn add_ship_to_homeworld(&mut self, player_id: u64, ship_class: ShipClass) -> Result<(), Box<dyn std::error::Error>> {
        let player = self.players.get(&player_id).ok_or("Player not found")?;
        let homeworld = player.homeworld;
        let refit = Refit::of(&player.research);

        // New hulls reinforce the lowest-numbered idle fleet at home that
        // has no standing orders and room; otherwise they form their own.
        let docked_fleet_id = self.fleets.values()
            .filter(|f| {
                f.owner_id == player_id
                    && f.location == homeworld
                    && f.state == FleetStatus::Idle
                    && f.ships.total() < MAX_FLEET_SHIPS
                    && !self.policies.contains_key(&f.id)
            })
            .map(|f| f.id)
            .min();

        let fleet_id = if let Some(fid) = docked_fleet_id {
            fid
        } else {
            let new_id = self.next_id();
            let new_fleet = Fleet {
                id: new_id,
                owner_id: player_id,
                location: homeworld,
                state: FleetStatus::Idle,
                ships: Ships::default(),
                refit,
                cargo: Resources::default(),
                fuel: 0.0,
                fuel_max: 0.0,
                move_cooldown: 0,
                action_cooldown: 0,
                move_target: None,
                harvest_target: HarvestResource::Auto,
                charts: Vec::new(),
                laden: false,
            };
            self.fleets.insert(new_id, new_fleet);
            new_id
        };

        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.ships.add(ship_class, 1);

        let player = self.players.get(&player_id).unwrap();
        let old_max = fleet.fuel_max;
        fleet.fuel_max = fleet_fuel_max(fleet, player);
        fleet.fuel = (fleet.fuel + (fleet.fuel_max - old_max)).min(fleet.fuel_max);

        Ok(())
    }

    fn recalculate_player_fleet_fuel(&mut self, player_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let player = self.players.get(&player_id).ok_or("Player not found")?;
        let fleet_ids: Vec<u64> = self.fleets.keys().copied().collect();
        for fid in fleet_ids {
            let fleet = self.fleets.get(&fid).unwrap();
            if fleet.owner_id != player.id { continue; }
            if fleet.ships.is_empty() { continue; }

            let new_max = fleet_fuel_max(fleet, player);
            if new_max > fleet.fuel_max {
                let fleet_mut = self.fleets.get_mut(&fid).unwrap();
                let bonus = new_max - fleet_mut.fuel_max;
                fleet_mut.fuel += bonus;
                fleet_mut.fuel_max = new_max;
            }
        }
        Ok(())
    }

    // ── Sensor / Map ──────────────────────────────────────────────

    pub fn get_sensor_revealed_coords(&self, origin: Hex, max_hops: u8) -> Vec<Hex> {
        self.world_gen.reachable_within(origin, max_hops).to_vec()
    }

    // ── Events ────────────────────────────────────────────────────

    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.pending_events)
    }
}

// ── Helper Functions ──────────────────────────────────────────────

/// A fuel amount in a sentence: whole when it is whole, else to a tenth.
fn fuel_words(fuel: f64) -> String {
    let tenths = (fuel * 10.0).round() / 10.0;
    if tenths.fract() == 0.0 { format!("{tenths:.0}") } else { format!("{tenths:.1}") }
}

/// A hold reason without its parenthesised figures (power, ratio), which drift
/// from tick to tick while the cause stays the same.
fn hold_key(reason: &str) -> String {
    let mut key = String::with_capacity(reason.len());
    let mut depth = 0u32;
    for c in reason.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => key.push(c),
            _ => {}
        }
    }
    key
}

/// Index of the earliest waiting order that can start now. An order that is
/// ready but unaffordable is passed over unless it reserves, in which case it
/// holds the line and nothing behind it starts.
fn first_startable<T>(
    orders: &[T],
    ready: impl Fn(&T) -> bool,
    affordable: impl Fn(&T) -> bool,
    reserve: impl Fn(&T) -> bool,
) -> Option<usize> {
    for (i, q) in orders.iter().enumerate() {
        if !ready(q) {
            continue;
        }
        if affordable(q) {
            return Some(i);
        }
        if reserve(q) {
            return None;
        }
    }
    None
}

/// Stable id of the hostile a sector's template spawns; see `TEMPLATE_NPC_ID_BASE`.
pub fn template_npc_id(coord: Hex) -> u64 {
    TEMPLATE_NPC_ID_BASE + u64::from(coord.to_key())
}

fn exchange_meter_at(pressure: f64, span: f64, window: u64) -> ExchangeMeter {
    let clear_in = if pressure <= 0.0 {
        Some(0)
    } else if span > 0.0 {
        Some((pressure / span * window as f64).ceil() as u64)
    } else {
        None
    };
    ExchangeMeter {
        pressure,
        span,
        fee: scaling::exchange_fee(pressure, span),
        window_ticks: window,
        clear_in,
    }
}

/// What a repair of `level` is priced at, in a sentence.
fn repair_price_words(level: u8) -> String {
    format!(
        "{} metal, or crystal at {} and deuterium at {} a unit in its place",
        scaling::outpost_repair_price(level).ceil(),
        ResourceKind::Crystal.weight(),
        ResourceKind::Deuterium.weight(),
    )
}

/// The bounds of the roll that sizes a raid on an outpost at `ring`.
pub fn outpost_raid_range(ring: u16) -> PowerRange {
    let base = scaling::outpost_raid_power(ring);
    PowerRange { least: base * RAID_POWER_ROLL_MIN, most: base * RAID_POWER_ROLL_MAX }
}

/// What a lost raid takes of a dock's `stock`: `rate` of what lies above
/// `protected`. Returns the stock that is left, what was taken, and what
/// the floor kept. The cargo of the ships that stood there is in the stock
/// by then, as far as it had room: they fell in the fight, and a hold that
/// loses its ships is emptied into the dock (`settle_hold`).
fn skim(stock: Resources, protected: Resources, rate: Resources) -> (Resources, Resources, Resources) {
    let exposed = stock.sub(protected);
    let lost = Resources {
        metal: exposed.metal.max(0.0) * rate.metal,
        crystal: exposed.crystal.max(0.0) * rate.crystal,
        deuterium: exposed.deuterium.max(0.0) * rate.deuterium,
    };
    (stock.sub(lost), lost, protected.min(stock))
}

/// A raid of `power` on an outpost at `ring`, arriving at `arrival_tick`:
/// ships of the class the ring's own groups fly.
fn outpost_raid(ring: u16, power: f32, arrival_tick: u64) -> IncomingRaid {
    let (class, count, multiplier) = scaling::outpost_raid_composition(ring, power);
    IncomingRaid { arrival_tick, power, ships: Ships::of(&[(class, count)]), arms: Arms::of_grade(multiplier) }
}

/// The defences an outpost's platform is at `level`, as a client is told.
pub fn platform_units(level: u8) -> Vec<UnitCount> {
    scaling::outpost_platform(level)
        .iter()
        .map(|&(kind, count)| UnitCount { unit: ShipyardItem::Defence(kind), count })
        .collect()
}

/// Wreckage a destroyed NPC group leaves behind: its whole worth, not one ship's.
fn npc_salvage(npc: &NpcFleet, pace: &Pace) -> Resources {
    scaling::split_loot(scaling::wreck_value(npc.power)).scale(pace.finds_mult())
}

/// The danger rating the ring alone gives a sector.
fn ring_rating(coord: Hex) -> u8 {
    scaling::threat_rating(scaling::npc_power(coord.dist_from_origin()))
}

fn behavior_threat_bonus(behavior: iac_shared::world::NpcBehaviorType) -> f32 {
    use iac_shared::world::NpcBehaviorType::{Aggressive, Swarm};
    if matches!(behavior, Aggressive | Swarm) { scaling::THREAT_AGGRESSIVE_BONUS } else { 1.0 }
}

/// The threat the ring alone implies, whoever lives in the sector.
fn ring_threat(coord: Hex, basis: ThreatBasis) -> ThreatInfo {
    let est_power = scaling::npc_group_power(coord.dist_from_origin());
    ThreatInfo { rating: scaling::threat_rating(est_power), est_power, basis }
}

/// Why a hauling doctrine leaves a seam, a wreck or a derelict where it is.
const NO_FIGHT_FOR_IT: &str = "sits among hostiles that attack on sight; this doctrine picks no fight for it (a patrol can clear them)";

/// Raw ore units the tiles hold between them.
fn ore_units(ore: &OreReserve) -> f64 {
    ore.metal.units + ore.crystal.units + ore.deuterium.units
}

/// Score weight of `load` once it sits in a hold with `room` units free:
/// all of it, or the share that fits.
fn worth_aboard(load: Resources, room: f64) -> f32 {
    let units = load.total();
    if units <= 0.0 { return 0.0; }
    scaling::resource_weight(&load.scale((room / units).min(1.0)))
}

/// The ships of a remembered sector: the groups last seen there, scaled to
/// the power they were rated at, or the ring's group if none was in sight.
/// A group seen wounded is counted whole once the tick it was to be whole by
/// is past at `now`; one seen passing through, after a fleet or on its way
/// back, is not counted there.
fn remembered_opposing(seen: &SectorState, now: u64) -> Opposing {
    use iac_shared::protocol::NpcBehavior::{Aggressive, Swarm};
    let ring = || Opposing::Ring(seen.location.dist_from_origin());
    if seen.outpost.is_some() && seen.hostiles.is_none() {
        return Opposing::Ships(Force::default());
    }
    let Some(groups) = seen.hostiles.as_ref().filter(|_| seen.threat.basis == ThreatBasis::Observed) else {
        return ring();
    };
    let rated: f32 = groups
        .iter()
        .map(|g| {
            let bonus = if matches!(g.behavior, Aggressive | Swarm) { scaling::THREAT_AGGRESSIVE_BONUS } else { 1.0 };
            bonus * g.ships.iter().map(|s| s.count as f32 * scaling::ship_power(s.ship_class)).sum::<f32>()
        })
        .sum();
    if rated <= 0.0 {
        return ring();
    }
    let multiplier = seen.threat.est_power / rated;
    let mut force = Force::default();
    for group in groups.iter().filter(|g| g.errand.is_none()) {
        let mended = group.whole_at.is_some_and(|tick| tick <= now) && group.ship_count > 0;
        for ship in &group.ships {
            let count = if mended {
                (u64::from(ship.count) * u64::from(group.muster)).div_ceil(u64::from(group.ship_count)) as u32
            } else {
                ship.count
            };
            force.join(&Force::of_group(ship.ship_class, count, multiplier));
        }
    }
    if force.is_empty() {
        return ring();
    }
    Opposing::Ships(force)
}

/// Ticks one jump takes `fleet`: its slowest ship sets them, with the
/// owner's drive research as it stands now.
fn fleet_jump_ticks(fleet: &Fleet, research: Option<&ResearchLevels>) -> u16 {
    let none = ResearchLevels::default();
    let research = research.unwrap_or(&none);
    fleet.ships.counts()
        .map(|(class, _)| scaling::jump_ticks(scaling::ship_speed(class, research)))
        .max()
        .unwrap_or(MOVE_BASE_COOLDOWN)
}

/// Fuel one jump burns: OGame's figure for each ship, less Energy
/// Technology's saving.
fn fleet_fuel_cost(fleet: &Fleet, research: Option<&ResearchLevels>) -> f64 {
    let none = ResearchLevels::default();
    let research = research.unwrap_or(&none);
    let burn = fleet.ships.counts()
        .fold(0.0, |burn, (class, count)| burn + f64::from(count) * scaling::ship_burn(class, research));
    burn * scaling::fuel_rate_modifier(research.energy_technology)
}

/// Raw units per tick the fleet can extract at density multiplier 1.
fn fleet_harvest_power(fleet: &Fleet) -> f64 {
    fleet.ships.counts().fold(0.0, |power, (class, count)| power + f64::from(count) * class.harvest_power())
}

fn fleet_fuel_max(fleet: &Fleet, player: &Player) -> f64 {
    // Folded from 0.0: the `sum()` of no floats is -0.0, and a fleet with no
    // ships left must not carry a zero whose sign a store may not keep.
    let total_fuel = fleet.ships.counts()
        .fold(0.0, |tank, (class, count)| tank + f64::from(count) * scaling::ship_tank(class, &player.research));
    total_fuel * scaling::tank_modifier(player.buildings.deuterium_tank)
}

/// What `refine` would turn into what for `fleet` right now: all the
/// deuterium in its hold, or as much of it as fills the tank, at the dock's
/// rate less `REFINE_LOSS`. None when the order would be refused: in a fight,
/// charging a jump, no deuterium aboard or no room in the tank. A hold's
/// deuterium becomes a tank's fuel here and nowhere else.
pub fn refining(fleet: &Fleet) -> Option<Refining> {
    if matches!(fleet.state, FleetStatus::InCombat | FleetStatus::Moving) { return None; }
    let aboard = fleet.cargo.deuterium;
    let room = fleet.fuel_max - fleet.fuel;
    if aboard <= 0.0 || room <= 0.0 { return None; }
    let rate = scaling::refined_fuel_per_deuterium();
    let fuel = aboard * rate;
    Some(if fuel <= room {
        Refining { deuterium: aboard, fuel }
    } else {
        Refining { deuterium: room / rate, fuel: room }
    })
}

/// The ships a fleet brings to a fight, as they stand and with its refit.
pub fn fleet_force(fleet: &Fleet) -> Force {
    Force::of(fleet.arms(), fleet.ships.squads())
}

/// Live combat power of a whole fleet, with its refit.
pub fn fleet_power(fleet: &Fleet) -> f32 {
    fleet.ships.power(fleet.arms())
}

/// What one round of fire from the whole fleet adds up to, and the shield
/// it raises at the start of each round.
pub fn fleet_broadside(fleet: &Fleet) -> (u64, u64) {
    let arms = fleet.arms();
    fleet.ships.counts().fold((0, 0), |(attack, shield), (class, count)| {
        let figures = scaling::ship_figures(class, arms);
        (attack + u64::from(count) * u64::from(figures.attack), shield + u64::from(count) * u64::from(figures.shield))
    })
}

/// One side of a round as a report carries it, without its groups.
fn battle_side(side: &combat::SideReport) -> BattleSide {
    BattleSide {
        groups: Vec::new(),
        entered: side.standing.iter().flatten().map(|units| units.entered).sum(),
        standing: side.units(),
        volleys: side.volleys.clone(),
        shots: side.shots,
        power: side.power,
        absorbed: side.absorbed,
        hull_damage: side.hull_damage,
        destroyed: side.destroyed,
    }
}

/// Add what `group` lost this round to the fight's running count, which
/// gains an entry for every fleet and group the first time it stands in a
/// round.
fn tally_losses(losses: &mut Vec<BattleLosses>, group: &BattleGroup) {
    let at = losses.iter().position(|l| l.fleet_id == group.fleet_id && l.owner == group.owner).unwrap_or_else(|| {
        losses.push(BattleLosses {
            fleet_id: group.fleet_id,
            owner: group.owner.clone(),
            label: group.label.clone(),
            mine: false,
            battery: group.battery,
            lost: Vec::new(),
            destroyed: false,
        });
        losses.len() - 1
    });
    let entry = &mut losses[at];
    for units in group.units.iter().filter(|u| u.lost > 0) {
        match entry.lost.iter_mut().find(|l| l.unit == units.unit) {
            Some(so_far) => so_far.count += units.lost,
            None => {
                let before = entry.lost.partition_point(|l| l.unit.index() < units.unit.index());
                entry.lost.insert(before, UnitCount { unit: units.unit, count: units.lost });
            }
        }
    }
    entry.destroyed = group.units.iter().all(|u| u.count == 0);
}

/// What jumps a careless boarding party: a share of the ring's group.
fn ambush_guardian_template(site: Hex, rng: &mut StdRng) -> iac_shared::world::NpcTemplate {
    let (ship_class, count, stat_multiplier) = scaling::ambush_guard(site.dist_from_origin());
    let swing = rng.random_range(0.8..=1.2);
    iac_shared::world::NpcTemplate {
        ship_class,
        count: ((f32::from(count) * swing).round() as usize).clamp(1, scaling::NPC_MAX_SHIPS) as u8,
        behavior: iac_shared::world::NpcBehaviorType::Aggressive,
        stat_multiplier,
    }
}

/// What can be coaxed back to life in a wreck of this tier.
fn recoverable_ship_class(tier: u8, rng: &mut StdRng) -> ShipClass {
    let pool: &[ShipClass] = match tier {
        1 => &[ShipClass::EspionageProbe, ShipClass::LightFighter],
        2 => &[ShipClass::LightFighter, ShipClass::SmallCargo, ShipClass::HeavyFighter],
        _ => &[ShipClass::HeavyFighter, ShipClass::LargeCargo, ShipClass::Cruiser, ShipClass::Battleship],
    };
    pool[rng.random_range(0..pool.len())]
}

/// The risk read a fleet on-site gets before boarding.
pub fn site_risk_label(site: Hex, bumps: u8) -> iac_shared::protocol::SiteRisk {
    use iac_shared::protocol::SiteRisk;
    let chance = scaling::derelict_ambush(ring_rating(site)) + bumps as f32 * EXPLORE_RETRY_AMBUSH_BUMP;
    if chance < 0.15 {
        SiteRisk::Quiet
    } else if chance < 0.30 {
        SiteRisk::Uneasy
    } else {
        SiteRisk::Hot
    }
}

/// The hold: each ship's base cargo times the pace's cargo factor (P^0.5,
/// like the loot it carries).
pub fn fleet_cargo_capacity(fleet: &Fleet, pace: &Pace) -> f64 {
    // Folded from 0.0 for the reason given in `fleet_fuel_max`: what is left
    // in the hold of a fleet with no ships is scaled by this.
    let base = fleet.ships.counts()
        .fold(0.0, |hold, (class, count)| hold + f64::from(count) * f64::from(class.base_stats().cargo));
    base * pace.cargo_mult()
}

fn regen_resource(
    harvested: &mut f64,
    override_density: &mut Option<Density>,
    template_density: Density,
    dist: u16,
    pace: &Pace,
) {
    let current = override_density.unwrap_or(template_density);
    let rate = scaling::ore_regen_per_tick(template_density, dist, pace);
    let (density, gone) = scaling::ore_regen_step(template_density, current, *harvested, rate, dist);
    *override_density = (density != template_density).then_some(density);
    *harvested = gone;
}
