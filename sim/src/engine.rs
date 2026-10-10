// Game engine: tick loop, movement, combat, harvesting, NPC behavior, homeworlds, build queues.
// Ported from Zig engine.zig.

use std::collections::{BTreeMap, HashMap, HashSet};

use log::info;
use serde::{Deserialize, Serialize};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use iac_shared::hex::Hex;
use iac_shared::constants::{
    Resources, ResourceKind, ShipClass, Density,
    STARTING_RESOURCES, STARTING_SCOUTS, MAX_FLEETS_PER_PLAYER, MAX_FLEETS_TOTAL, STRANDED_RECOVERY_TICKS,
    HARVEST_COOLDOWN, SHIELD_REGEN_IDLE_TICKS,
    RECALL_FUEL_MULTIPLIER, RECALL_DAMAGE_CHANCE_PER_HEX, RECALL_DAMAGE_CHANCE_CAP,
    RECALL_HULL_DAMAGE_MIN, RECALL_HULL_DAMAGE_MAX,
    FUEL_RATE_PER_MASS, FUEL_DEUT_PER_UNIT,
    NPC_PATROL_INTERVAL, SALVAGE_DESPAWN_TICKS, HOME_SALVAGE_WARN_TICKS, DERELICT_RESPAWN_TICKS, HARVEST_REPORT_TICKS,
    TEMPLATE_NPC_ID_BASE,
    HOMEWORLD_MIN_DIST, HOMEWORLD_MAX_DIST, MOVE_BASE_COOLDOWN,
    SCAN_COOLDOWN, SCAN_BASE_RANGE, SCAN_SCOUT_RANGE, SCAN_REVEAL_TICKS,
    RAID_ROLL_INTERVAL, RAID_ROLL_CHANCE, RAID_PITY_STEP, RAID_LOST_SHIP_FRACTION, RAID_MIN_INTERVAL,
    RAID_MIN_PLAYER_AGE, RAID_POWER_ROLL_MIN, RAID_POWER_ROLL_MAX,
    RAID_STRUCTURE_DAMAGE, RAID_STRUCTURE_RESTORE, RAID_RESTORE_TICKS, RAID_LOST_STRUCTURE_FRACTION,
    RAID_LOSS_CAP_METAL, RAID_LOSS_CAP_CRYSTAL, RAID_LOSS_CAP_DEUT,
    RAID_DEFENSE_SALVAGE_FRACTION, RAID_SUPPRESS_AFTER_LOSS,
    EXPLORE_DURATION_TICKS, EXPLORE_SCOUT_AMBUSH_REDUCTION, EXPLORE_HAULER_LOOT_MULTIPLIER,
    EXPLORE_RETRY_AMBUSH_BUMP, POLICY_EVAL_INTERVAL, POLICY_PATROL_RADIUS, POLICY_PATROL_MIN_HULL,
    defense_grid_scout_units,
};
use iac_shared::pace::Pace;
use iac_shared::world::WorldGen;
use iac_shared::scaling::{
    self, BuildingType, ResearchType, BuildingLevels, ResearchLevels,
    MAX_BUILDING_LEVEL, CANCEL_REFUND_FRACTION, DefenceKind, ShipyardItem,
};
use iac_shared::protocol::{
    AlertEvent, AlertLevel, Command, GameEvent, EventKind, ErrorCode, HarvestResource, PolicyPreset, PolicyParams,
    MovePreview, OreReserve, ResourceEta, ThreatBasis, ThreatInfo, TileReserve, StorageFullEvent, StorageNearCapEvent, StorageState,
};

use crate::auth::{self, TokenHash};
use crate::combat;
use crate::score;
use crate::intel::KnownSectors;
use crate::persist::{ExploreCredit, ExploredEdge, Persist, PersistBatch, WorldMeta};
use crate::snapshot::Snapshot;

// ── Constants ─────────────────────────────────────────────────────

pub const MAX_SHIPS_PER_FLEET: usize = 64;
pub const MAX_NPC_SHIPS: usize = scaling::NPC_MAX_SHIPS;
pub const MAX_COMBAT_FLEETS: usize = 8;
/// Rings beyond `HOMEWORLD_MAX_DIST` a new homeworld may spill into once the home band is full.
const HOMEWORLD_SPILL_RINGS: u16 = 4;
/// Part-refilled tiles are written to disk this often (ticks).
const REGEN_PERSIST_TICKS: u64 = 60;

// ── Core Data Types ───────────────────────────────────────────────

/// Serde for a fixed ship array: written up to the last non-default slot,
/// padded back with defaults on read, so a snapshot stays small and exact.
mod ship_slots {
    use super::Ship;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer, const N: usize>(ships: &[Ship; N], out: S) -> Result<S::Ok, S::Error> {
        let used = ships.iter().rposition(|s| *s != Ship::default()).map_or(0, |i| i + 1);
        ships[..used].serialize(out)
    }

    pub fn deserialize<'de, D: Deserializer<'de>, const N: usize>(input: D) -> Result<[Ship; N], D::Error> {
        let list = Vec::<Ship>::deserialize(input)?;
        if list.len() > N {
            return Err(serde::de::Error::invalid_length(list.len(), &"at most the fleet's ship capacity"));
        }
        let mut ships = [Ship::default(); N];
        ships[..list.len()].copy_from_slice(&list);
        Ok(ships)
    }
}

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Ship {
    pub id: u64,
    pub ship_class: ShipClass,
    pub hull: f32,
    pub hull_max: f32,
    pub shield: f32,
    pub shield_max: f32,
    pub weapon_power: f32,
    pub speed: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fleet {
    pub id: u64,
    pub owner_id: u64,
    pub location: Hex,
    pub state: FleetStatus,
    #[serde(with = "ship_slots")]
    pub ships: [Ship; MAX_SHIPS_PER_FLEET],
    pub ship_count: usize,
    pub cargo: Resources,
    pub fuel: f32,
    pub fuel_max: f32,
    pub move_cooldown: u16,
    pub action_cooldown: u16,
    pub move_target: Option<Hex>,
    pub idle_ticks: u16,
    /// What the current (or last) harvest order mines.
    pub harvest_target: HarvestResource,
    /// Sectors this fleet has charted and not yet delivered to a dock.
    pub charts: Vec<u32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcFleet {
    pub id: u64,
    pub location: Hex,
    #[serde(with = "ship_slots")]
    pub ships: [Ship; MAX_NPC_SHIPS],
    pub ship_count: u8,
    pub behavior: iac_shared::world::NpcBehaviorType,
    pub home_sector: Hex,
    pub patrol_timer: u16,
    pub in_combat: bool,
    /// What the group cost to build, and its combined power, fixed at spawn.
    pub bounty: Resources,
    pub power: f32,
    /// "3x corvette pack": how messages name the group.
    pub label: String,
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
}

/// A queued item holds no resources: it pays when it starts.
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

/// A shipyard order: a batch that pays per unit. `built` units are done; a
/// batch between units sits here again until the next unit can be paid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingShip {
    pub id: u64,
    pub item: ShipyardItem,
    pub count: u16,
    pub built: u16,
    pub reserve: bool,
}

/// Defence structures standing at home, and those a repelled raid knocked
/// out that come back free at `restore_tick`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Defences {
    pub count: [u32; DefenceKind::COUNT],
    pub restoring: [u32; DefenceKind::COUNT],
    pub restore_tick: [u64; DefenceKind::COUNT],
}

impl Defences {
    /// Combined power of the structures at a Defense Grid level.
    pub fn power(&self, grid_level: u8) -> f32 {
        DefenceKind::ALL.iter()
            .map(|&k| self.count[k as usize] as f32 * scaling::structure_power(k, grid_level))
            .sum()
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

    /// What the homeworld mines add to the stockpile each tick.
    pub fn production_per_tick(&self, pace: &Pace) -> Resources {
        Resources {
            metal: scaling::production_per_tick(BuildingType::MetalMine, self.buildings.metal_mine, pace),
            crystal: scaling::production_per_tick(BuildingType::CrystalMine, self.buildings.crystal_mine, pace),
            deuterium: scaling::production_per_tick(
                BuildingType::DeuteriumSynthesizer,
                self.buildings.deuterium_synthesizer,
                pace,
            ),
        }
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
    /// Units finished before the one under way.
    pub built: u16,
    pub reserve: bool,
    pub start_tick: u64,
    pub end_tick: u64,
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
    pub round: u16,
    /// The most power the players' side has brought to bear, for scoring.
    pub peak_power: f32,
}

impl Combat {
    pub fn add_player_fleet(&mut self, fleet_id: u64) {
        if !self.player_fleet_ids.contains(&fleet_id) && self.player_fleet_ids.len() < MAX_COMBAT_FLEETS {
            self.player_fleet_ids.push(fleet_id);
        }
    }

    pub fn add_npc_fleet(&mut self, fleet_id: u64) {
        if !self.npc_fleet_ids.contains(&fleet_id) && self.npc_fleet_ids.len() < MAX_COMBAT_FLEETS {
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
    pub metal_harvested: f32,
    pub crystal_harvested: f32,
    pub deut_harvested: f32,
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

/// Per-player raid pacing state. Deliberately not persisted: a server
/// restart just resets the raid clock, which is always in the player's favor.
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

/// Harvest yield not yet reported to the owner.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HarvestReport {
    resources: Resources,
    ticks: u16,
    /// Tick the first unreported harvest happened.
    since: u64,
}

/// A fleet's standing orders plus the bit of memory the autopilot needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetPolicy {
    pub preset: PolicyPreset,
    pub params: PolicyParams,
    /// The sector this doctrine works (set when assigned; MineAndReturn
    /// shuttles between here and home).
    pub work_sector: Option<Hex>,
    /// Next tick the autopilot will reconsider this fleet.
    pub next_eval_tick: u64,
    /// The last hold reason announced and when, so a fleet that keeps
    /// holding for the same reason does not flood the event stream.
    pub last_hold: Option<(String, u64)>,
}

/// What a policy BFS is hunting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyTarget {
    Resources,
    SalvageOrSite,
    Hostiles,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IncomingRaid {
    pub arrival_tick: u64,
    /// Combat power of the raid fleet, fixed at forecast time so the
    /// player's preparations after the warning actually help.
    pub power: f32,
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
    Server,
}

/// See `GameEngine::home_route`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HomeRoute {
    pub charted: Option<u32>,
    pub direct: u32,
    pub unexplored: bool,
}

pub struct GameEngine {
    pub world_gen: WorldGen,
    pub world: WorldMeta,
    persister: Option<Box<dyn Persist>>,
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
    /// Standing orders per fleet.
    pub policies: BTreeMap<u64, FleetPolicy>,

    pending_events: Vec<GameEvent>,
    pending_arrivals: Vec<u64>,  // fleet IDs pending homeworld/NPC checks after movement

    next_id: u64,

    dirty_players: HashMap<u64, ()>,
    dirty_fleets: HashMap<u64, ()>,
    dirty_sectors: HashMap<u32, ()>,
    deleted_fleet_ids: HashMap<u64, ()>,
    dirty_policies: HashMap<u64, ()>,
    deleted_policy_ids: HashMap<u64, ()>,

    /// (player, sector key) pairs the player has entered; answers
    /// "first visit?" without touching the database.
    explored: HashSet<(u64, u32)>,
    /// Explored edges not yet handed to the persister.
    new_edges: Vec<ExploredEdge>,
    /// Explore credits already paid: once per player and sector.
    credited: HashSet<(u64, u32, ExploreCredit)>,
    new_credits: Vec<(u64, Hex, ExploreCredit)>,
    /// What each player has seen of the galaxy, live or remembered.
    pub known: KnownSectors,
}

impl GameEngine {
    /// A fresh world: no players, tick 0.
    pub fn new(world_seed: u64, pace: Pace) -> GameEngine {
        GameEngine::restore(Snapshot::new(world_seed, pace))
    }

    /// Rebuild an engine from saved state: a `snapshot()` of a running one,
    /// or a partial one a host assembled from its own storage (everything
    /// the engine can regenerate or that a restart resets may be left empty).
    /// No persistence is attached; see `attach_persist`.
    pub fn restore(snapshot: Snapshot) -> GameEngine {
        let Snapshot {
            world_seed, meta, tick, mut next_id, mut players, mut fleets, npc_fleets, combats,
            sector_overrides, mut policies, scan_reveals, raid_states, storage_marks, home_salvage_warned,
            harvest_reports, pending_events, explored, credited, known,
        } = snapshot;

        for player in players.values_mut() {
            assign_missing_queue_ids(player, &mut next_id);
        }
        // Fleets registered before the starting-fuel fix were stored with a
        // 50000 tank; hold them to the real one.
        for fleet in fleets.values_mut() {
            if let Some(player) = players.get(&fleet.owner_id) {
                let tank = fleet_fuel_max(fleet, player);
                if fleet.fuel_max > tank {
                    fleet.fuel_max = tank;
                    fleet.fuel = fleet.fuel.min(tank);
                }
            }
        }
        policies.retain(|fleet_id, _| fleets.contains_key(fleet_id));

        GameEngine {
            world_gen: WorldGen::init(world_seed),
            world: meta,
            persister: None,
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
            policies,
            pending_events,
            pending_arrivals: Vec::new(),
            next_id,
            dirty_players: HashMap::new(),
            dirty_fleets: HashMap::new(),
            dirty_sectors: HashMap::new(),
            deleted_fleet_ids: HashMap::new(),
            dirty_policies: HashMap::new(),
            deleted_policy_ids: HashMap::new(),
            explored: explored.into_iter().collect(),
            new_edges: Vec::new(),
            credited: credited.into_iter().collect(),
            new_credits: Vec::new(),
            known: KnownSectors::load(known),
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
            pending_events: self.pending_events.clone(),
            explored: self.explored.iter().copied().collect(),
            credited: self.credited.iter().copied().collect(),
            known: self.known.rows(),
        }
    }

    /// Send every later `persist_dirty_state` to `persist`. Without one the
    /// engine just forgets what changed.
    pub fn attach_persist(&mut self, persist: Box<dyn Persist>) {
        self.persister = Some(persist);
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
        // push events between ticks (commands are processed before tick()
        // in the server loop) and broadcast_updates() drains the queue
        // after every tick.
        self.current_tick += 1;

        self.reap_empty_fleets();
        self.process_policies()?;
        self.process_movement()?;
        self.process_stranded();
        self.process_combat()?;
        self.process_exploration()?;
        self.process_harvesting()?;
        self.process_sector_regen()?;
        self.process_npc_behavior()?;
        self.process_homeworlds()?;
        self.process_build_queues()?;
        self.process_raids()?;
        self.process_defence_restore();
        self.process_home_salvage();
        self.process_salvage_despawn()?;
        self.process_derelict_respawn();
        self.process_cooldowns()?;
        self.prune_scan_reveals();

        let mut known = std::mem::take(&mut self.known);
        known.refresh(self);
        self.known = known;

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
            if fleet.ship_count == 0 || fleet.state != FleetStatus::Idle { continue; }
            let Some(player) = self.players.get(&fleet.owner_id) else { continue; };
            if fleet.location == player.homeworld { continue; }
            let hop = fleet_fuel_cost(fleet, Some(&player.research)).min(fleet.fuel_max);
            if hop <= 0.0 || fleet.fuel >= hop { continue; }

            let old = fleet.fuel;
            let mut new = old + hop / STRANDED_RECOVERY_TICKS as f32;
            if new >= hop * 0.9999 { new = hop; }
            let quarters = |f: f32| (f / hop * 4.0 + 0.0001).floor() as u32;
            let location = fleet.location;
            let note = if new >= hop {
                Some((AlertLevel::Info, format!(
                    "fleet {fid} at {location}: emergency reserve ready, enough fuel for one jump toward home",
                )))
            } else if quarters(new) > quarters(old) {
                let left = ((hop - new) / (hop / STRANDED_RECOVERY_TICKS as f32)).ceil() as u64;
                Some((AlertLevel::Warning, format!(
                    "fleet {fid} stranded at {location}: emergency reserve {}% charged, about {left} ticks to one jump",
                    quarters(new) * 25,
                )))
            } else {
                None
            };
            if let Some((level, message)) = note { notes.push((level, fid, location, message)); }
            if let Some(f) = self.fleets.get_mut(&fid) { f.fuel = new; }
            self.dirty_fleets.insert(fid, ());
        }
        for (level, fid, sector, message) in notes {
            self.push_alert(level, fid, sector, message);
        }
    }

    /// Drop a fleet and its standing orders, and queue both for deletion.
    fn remove_fleet(&mut self, fleet_id: u64) {
        self.fleets.remove(&fleet_id);
        self.dirty_fleets.remove(&fleet_id);
        self.deleted_fleet_ids.insert(fleet_id, ());
        if self.policies.remove(&fleet_id).is_some() {
            self.dirty_policies.remove(&fleet_id);
            self.deleted_policy_ids.insert(fleet_id, ());
        }
    }

    /// Fleets that lost every ship in combat. They linger until the next
    /// tick so the owner still receives the FleetDestroyed event (event
    /// routing looks the fleet up), but are never listed to clients.
    fn reap_empty_fleets(&mut self) {
        let empty: Vec<u64> = self.fleets.values()
            .filter(|f| f.ship_count == 0)
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
                self.dirty_fleets.insert(fleet.id, ());
                arrived.push((fleet.id, target, fleet.owner_id));
            }
        }

        // Phase 2: Process arrivals (exploration, docking, NPC encounters)
        for (fleet_id, target, owner_id) in arrived {
            let first_visit = !self.explored.contains(&(owner_id, target.to_key()));
            self.record_explored(owner_id, target)?;
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
            self.announce_if_stranded(fleet_id);
            self.check_npc_encounter(fleet_id)?;
        }

        Ok(())
    }

    fn announce_if_stranded(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let Some(player) = self.players.get(&fleet.owner_id) else { return; };
        if fleet.ship_count == 0 || fleet.location == player.homeworld { return; }
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

    fn process_combat(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let combat_ids: Vec<u64> = self.active_combats.keys().copied().collect();
        let mut to_remove: Vec<u64> = Vec::new();

        for combat_id in combat_ids {
            let Some(active_combat) = self.active_combats.get(&combat_id) else { continue; };

            // Copy needed data before mutable borrows
            let combat_sector = active_combat.sector;
            let combat_round = active_combat.round;
            let pf_ids = active_combat.player_fleet_ids.clone();
            let npc_ids = active_combat.npc_fleet_ids.clone();
            let _ = active_combat;  // suppress unused warning

            // Collect player fleet ship data (read-only)
            let mut player_sides: Vec<combat::CombatSide> = Vec::new();
            for &fid in &pf_ids {
                if let Some(f) = self.fleets.get(&fid) {
                    let ships: Vec<combat::CombatShip> = f.ships[0..f.ship_count].iter()
                        .filter(|s| s.hull > 0.0)
                        .map(combat::CombatShip::from)
                        .collect();
                    player_sides.push(combat::CombatSide {
                        fleet_id: fid,
                        is_npc: false,
                        owner: self.players.get(&f.owner_id).map(|p| p.name.clone()),
                        salvage: Resources::default(),
                        label: String::new(),
                        ships,
                    });
                }
            }

            // Collect NPC fleet ship data (read-only)
            let farm = self.sector_farm_factor(combat_sector);
            let mut npc_sides: Vec<combat::CombatSide> = Vec::new();
            for &nid in &npc_ids {
                if let Some(n) = self.npc_fleets.get(&nid) {
                    let ships: Vec<combat::CombatShip> = n.ships[0..n.ship_count as usize].iter()
                        .filter(|s| s.hull > 0.0)
                        .map(combat::CombatShip::from)
                        .collect();
                    npc_sides.push(combat::CombatSide {
                        fleet_id: nid,
                        is_npc: true,
                        owner: None,
                        salvage: npc_salvage(n, &self.world.pace).scale(farm),
                        label: n.label.clone(),
                        ships,
                    });
                }
            }

            if player_sides.is_empty() || npc_sides.is_empty() {
                to_remove.push(combat_id);
                continue;
            }

            let engaged: f32 = player_sides.iter()
                .flat_map(|s| s.ships.iter())
                .map(|s| s.weapon_power + (s.hull + s.shield) / 10.0)
                .sum();
            if let Some(c) = self.active_combats.get_mut(&combat_id) {
                c.peak_power = c.peak_power.max(engaged);
            }

            // Resolve combat with copied data
            let result = combat::resolve_combat_round(
                &player_sides, &npc_sides,
                self.world_gen.world_seed,
                combat_sector,
                self.current_tick,
                combat_round as u32,
            );

            // Increment round counter
            if let Some(combat_mut) = self.active_combats.get_mut(&combat_id) {
                combat_mut.round = combat_round + 1;
            }

            for e in &result.events {
                if let EventKind::FleetDestroyed(d) = &e.kind
                    && !d.is_npc
                {
                    self.alert_fleet_lost(d.fleet_id, combat_sector, &npc_ids);
                }
            }
            self.pending_events.extend(result.events);

            // Write updated ship data back to player fleets
            for (i, side) in player_sides.iter().enumerate() {
                if let Some(f) = self.fleets.get_mut(&side.fleet_id) {
                    let new_ships: Vec<Ship> = result.player_ships[i].iter().map(|cs| Ship {
                        id: cs.id,
                        ship_class: cs.ship_class,
                        hull: cs.hull,
                        hull_max: cs.hull_max,
                        shield: cs.shield,
                        shield_max: cs.shield_max,
                        weapon_power: cs.weapon_power,
                        speed: cs.ship_class.base_stats().speed,
                    }).collect();
                    f.ships[..new_ships.len()].copy_from_slice(&new_ships);
                    for j in new_ships.len()..f.ships.len() {
                        f.ships[j] = Ship::default();
                    }
                    f.ship_count = new_ships.len();
                    self.dirty_fleets.insert(f.id, ());
                }
            }

            // Write updated ship data back to NPC fleets
            for (i, side) in npc_sides.iter().enumerate() {
                if let Some(n) = self.npc_fleets.get_mut(&side.fleet_id) {
                    let new_ships = &result.npc_ships[i];
                    let count = new_ships.len().min(n.ships.len());
                    for (j, ns) in new_ships.iter().enumerate().take(count) {
                        n.ships[j] = Ship {
                            id: ns.id,
                            ship_class: ns.ship_class,
                            hull: ns.hull,
                            hull_max: ns.hull_max,
                            shield: ns.shield,
                            shield_max: ns.shield_max,
                            weapon_power: ns.weapon_power,
                            speed: ns.ship_class.base_stats().speed,
                        };
                    }
                    for j in count..n.ships.len() {
                        n.ships[j] = Ship::default();
                    }
                    n.ship_count = count as u8;
                }
            }

            if result.concluded {
                to_remove.push(combat_id);

                // Update state for each player fleet
                for side in &player_sides {
                    if let Some(f) = self.fleets.get_mut(&side.fleet_id) {
                        f.state = if result.player_ships[player_sides.iter().position(|s| s.fleet_id == side.fleet_id).unwrap()].is_empty()
                            { FleetStatus::Docked } else { FleetStatus::Idle };
                        f.idle_ticks = 0;
                    }
                }

                let pile = if result.player_won {
                    let total = npc_sides.iter().fold(Resources::default(), |acc, s| acc.add(s.salvage));
                    Some(self.drop_salvage(combat_sector, total))
                } else {
                    None
                };
                self.pending_events.push(GameEvent {
                    tick: self.current_tick,
                    kind: EventKind::CombatEnded(iac_shared::protocol::CombatEndedEvent {
                        sector: combat_sector,
                        player_victory: result.player_won,
                        owners: player_sides.iter().filter_map(|s| s.owner.clone()).collect::<std::collections::BTreeSet<_>>().into_iter().collect(),
                        mine: false,
                        salvage: pile.map(|(r, _)| r),
                        salvage_despawn_tick: pile.map(|(_, t)| t),
                    }),
                });

                if result.player_won {
                    let cleared_key = combat_sector.to_key();
                    let pile = npc_sides.iter().fold(Resources::default(), |acc, s| acc.add(s.salvage));
                    self.credit_kill(combat_id, &pf_ids, &npc_ids, cleared_key, pile);
                    self.heat_sector(combat_sector);
                    self.ensure_override(cleared_key)
                        .npc_cleared_tick = Some(self.current_tick);
                    self.dirty_sectors.insert(cleared_key, ());
                }

                // Remove destroyed NPCs
                for &nid in &npc_ids {
                    self.npc_fleets.remove(&nid);
                }
            }
        }

        for id in to_remove {
            self.active_combats.remove(&id);
        }

        Ok(())
    }

    /// Score for destroying the NPC group, once per respawn of the sector and
    /// to every player who fought (cooperation costs nothing). It follows the
    /// pile the kill left (finds factor and anti-farming heat included), so
    /// points and resources rise and fall together.
    fn credit_kill(&mut self, combat_id: u64, player_fleets: &[u64], npcs: &[u64], sector_key: u32, pile: Resources) {
        let fresh = self.sector_overrides.get(&sector_key).is_none_or(|o| o.npc_cleared_tick.is_none());
        if !fresh { return; }
        let power: f32 = npcs.iter().filter_map(|id| self.npc_fleets.get(id)).map(|n| n.power).sum();
        let engaged = self.active_combats.get(&combat_id).map_or(0.0, |c| c.peak_power);
        let points = score::kill_points(pile, engaged, power);
        if points <= 0.0 { return; }
        let owners: HashSet<u64> = player_fleets.iter()
            .filter_map(|f| self.fleets.get(f))
            .map(|f| f.owner_id)
            .collect();
        for pid in owners {
            if let Some(p) = self.players.get_mut(&pid) {
                p.combat_points += points;
                self.dirty_players.insert(pid, ());
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
        self.dirty_sectors.insert(key, ());
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
        self.harvest_reports.retain(|fid, _| self.fleets.contains_key(fid));
    }

    /// Tell a fleet's owner, loudly, that the fleet is gone.
    fn alert_fleet_lost(&mut self, fleet_id: u64, sector: Hex, enemy_ids: &[u64]) {
        let Some(owner) = self.fleets.get(&fleet_id).map(|f| f.owner_id) else { return; };
        let enemies = enemy_ids.iter()
            .map(|id| self.npc_fleets.get(id).map_or_else(|| "hostile group".to_string(), |n| n.label.clone()))
            .collect::<Vec<_>>()
            .join(", ");
        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::Alert(iac_shared::protocol::AlertEvent {
                player_id: Some(owner),
                level: iac_shared::protocol::AlertLevel::Critical,
                message: format!(
                    "FLEET LOST: fleet {fleet_id} destroyed at [{},{}] by hostile {enemies}; every ship is gone",
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
                .map_or(1.0, |p| scaling::harvest_yield(p.research.harvesting_efficiency));
            let harvest_power = fleet_harvest_power(fleet);
            let dist = location.dist_from_origin();
            let max_cargo = fleet_cargo_capacity(fleet, &self.world.pace);
            let used = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
            let mut remaining = max_cargo - used;

            if remaining <= 0.0 {
                if let Some(f) = self.fleets.get_mut(&fid) {
                    f.state = FleetStatus::Idle;
                }
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
                let raw = (density.harvest_multiplier() * harvest_power).min(in_level).min(remaining / yield_mult);
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
            }

            if harvested_any {
                self.dirty_fleets.insert(fid, ());
            } else if let Some(f) = self.fleets.get_mut(&fid) {
                // Nothing harvestable here — stop instead of spinning forever.
                f.state = FleetStatus::Idle;
            }
        }

        self.flush_harvest_reports();
        Ok(())
    }

    fn accumulate_harvest(&mut self, sector_key: u32, resource: &str, amount: f32, current_density: Density) -> Result<(), Box<dyn std::error::Error>> {
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
            self.dirty_sectors.insert(sector_key, ());
        }
        Ok(())
    }

    // ── Sector Regen ──────────────────────────────────────────────

    /// Stripped tiles refill over `scaling::ore_regen_hours`, paused while a
    /// fleet sits on the sector, and never past the generated density.
    fn process_sector_regen(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let keys: Vec<u32> = self.sector_overrides.keys().copied().collect();
        for sector_key in keys {
            let Some(ov) = self.sector_overrides.get(&sector_key) else { continue; };
            if !ov.has_ore_damage() { continue; }

            let coord = Hex::from_key(sector_key);
            if self.fleets.values().any(|f| f.location == coord) { continue; }

            let dist = coord.dist_from_origin();
            let template = self.world_gen.generate_sector(coord);
            let pace = self.world.pace;
            let ov_mut = self.sector_overrides.get_mut(&sector_key).unwrap();
            let mut moved = regen_resource(&mut ov_mut.metal_harvested, &mut ov_mut.metal_density, template.metal_density, dist, &pace);
            moved |= regen_resource(&mut ov_mut.crystal_harvested, &mut ov_mut.crystal_density, template.crystal_density, dist, &pace);
            moved |= regen_resource(&mut ov_mut.deut_harvested, &mut ov_mut.deut_density, template.deut_density, dist, &pace);
            if moved || self.current_tick.is_multiple_of(REGEN_PERSIST_TICKS) {
                self.dirty_sectors.insert(sector_key, ());
            }
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
                    f.owner_id == pid && f.location == home && f.ship_count > 0
                        && matches!(f.state, FleetStatus::Idle | FleetStatus::Docked)
                })
                .map(|f| f.id)
                .min();

            let mut left = pile;
            if let Some(fleet_id) = docked {
                let player = self.players.get_mut(&pid).unwrap();
                let cap = scaling::storage_cap(player.buildings.storage_vault, &self.world.pace);
                let taken = player.resources.shortfall(cap).min(pile);
                if taken.total() > 0.0 {
                    player.resources = player.resources.add(taken);
                    self.dirty_players.insert(pid, ());
                    let rest = pile.sub(taken);
                    let remaining = (rest.total() > 1.0).then_some(rest);
                    let ov = self.sector_overrides.get_mut(&key).unwrap();
                    ov.salvage = remaining;
                    if remaining.is_none() { ov.salvage_despawn_tick = None; }
                    self.dirty_sectors.insert(key, ());
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
        let keys: Vec<u32> = self.sector_overrides.keys().copied().collect();
        for key in keys {
            let Some(ov) = self.sector_overrides.get_mut(&key) else { continue; };
            if let Some(despawn_tick) = ov.salvage_despawn_tick
                && self.current_tick >= despawn_tick
            {
                let lost = ov.salvage.take();
                ov.salvage_despawn_tick = None;
                self.dirty_sectors.insert(key, ());
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

    fn process_npc_behavior(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Respawn check
        let keys: Vec<u32> = self.sector_overrides.keys().copied().collect();
        for key in keys {
            let Some(ov) = self.sector_overrides.get(&key) else { continue; };
            let Some(cleared_tick) = ov.npc_cleared_tick else { continue; };
            let coord = Hex::from_key(key);
            let hours = scaling::npc_respawn_hours(coord.dist_from_origin());
            let delay = self.world.pace.econ_ticks(f64::from(hours) * 3600.0);
            if self.current_tick >= cleared_tick + delay
                && let Some(ov_mut) = self.sector_overrides.get_mut(&key)
            {
                ov_mut.npc_cleared_tick = None;
                self.dirty_sectors.insert(key, ());
            }
        }

        // Patrol movement
        let mut rng = StdRng::seed_from_u64(self.current_tick.wrapping_mul(0x9E3779B97F4A7C15));

        // Sorted so the per-tick rng draws land on the same patrols on every run:
        // HashMap order is randomised per process.
        let mut npc_ids: Vec<u64> = self.npc_fleets.keys().copied().collect();
        npc_ids.sort_unstable();
        for npc_id in npc_ids {
            let Some(npc) = self.npc_fleets.get(&npc_id) else { continue; };
            if npc.in_combat { continue; }
            if npc.behavior != iac_shared::world::NpcBehaviorType::Patrol
                && npc.behavior != iac_shared::world::NpcBehaviorType::Aggressive
            {
                continue;
            }

            let npc_mut = self.npc_fleets.get(&npc_id).unwrap();
            if npc_mut.patrol_timer > 0 {
                if let Some(npc_m) = self.npc_fleets.get_mut(&npc_id) {
                    npc_m.patrol_timer -= 1;
                }
                continue;
            }

            let connections = self.world_gen.connected_neighbors(npc.location);
            let conns = connections.slice();
            if conns.is_empty() { continue; }

            let idx = rng.random_range(0..conns.len());
            let new_location = conns[idx];

            if let Some(npc_m) = self.npc_fleets.get_mut(&npc_id) {
                npc_m.location = new_location;
                npc_m.patrol_timer = NPC_PATROL_INTERVAL;
            }

            // Check if player fleet at new location -> trigger combat
            for fleet in self.fleets.values() {
                if fleet.location == new_location && fleet.state != FleetStatus::InCombat && fleet.ship_count > 0 {
                    self.start_combat(fleet.id, npc_id)?;
                    break;
                }
            }
        }
        Ok(())
    }

    // ── Homeworlds ────────────────────────────────────────────────

    fn process_homeworlds(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let pace = self.world.pace;
        for (_, player) in self.players.iter_mut() {
            let cap = scaling::storage_cap(player.buildings.storage_vault, &pace);
            player.resources = player.resources.add_capped(player.production_per_tick(&pace), cap);
            self.dirty_players.insert(player.id, ());
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
        self.process_storage_alerts();
        Ok(())
    }

    /// Move as much of the fleet's cargo into its owner's stockpile as the
    /// storage cap allows; the rest stays aboard.
    fn unload_cargo(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let Some(player) = self.players.get_mut(&fleet.owner_id) else { return; };
        let cap = scaling::storage_cap(player.buildings.storage_vault, &self.world.pace);
        let room = player.resources.shortfall(cap).min(fleet.cargo);
        if room.total() <= 0.0 { return; }
        player.resources = player.resources.add(room);
        let owner = player.id;
        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.cargo = fleet.cargo.sub(room);
        self.dirty_fleets.insert(fleet_id, ());
        self.dirty_players.insert(owner, ());
    }

    /// Fill the tank at the homeworld, paying `FUEL_DEUT_PER_UNIT` deuterium
    /// per unit. A dry stockpile fills it only as far as it can.
    fn refuel_fleet(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let Some(player) = self.players.get_mut(&fleet.owner_id) else { return; };
        let missing = fleet.fuel_max - fleet.fuel;
        let units = missing.min(player.resources.deuterium / FUEL_DEUT_PER_UNIT);
        if units <= 0.0 { return; }
        player.resources.deuterium = (player.resources.deuterium - units * FUEL_DEUT_PER_UNIT).max(0.0);
        let owner = player.id;
        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.fuel = if units >= missing { fleet.fuel_max } else { fleet.fuel + units };
        self.dirty_fleets.insert(fleet_id, ());
        self.dirty_players.insert(owner, ());
    }

    pub fn storage_state(&self, player: &Player) -> StorageState {
        let pace = &self.world.pace;
        let cap = scaling::storage_cap(player.buildings.storage_vault, pace);
        let rate = player.production_per_tick(pace);
        let eta = |kind: ResourceKind| {
            let (stock, cap, rate) = (player.resources.get(kind), cap.get(kind), rate.get(kind));
            if stock >= cap { Some(0) } else if rate > 0.0 { Some(((cap - stock) / rate).ceil() as u64) } else { None }
        };
        StorageState {
            cap,
            protected: scaling::storage_protected(player.buildings.storage_vault, pace),
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
                if q.building_type == BuildingType::FuelDepot {
                    fuel_recalc_players.push(pid);
                }
                self.dirty_players.insert(pid, ());
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
                if q.tech == ResearchType::ExtendedFuelTanks {
                    fuel_recalc_players.push(pid);
                }
                self.dirty_players.insert(pid, ());
                self.pending_events.push(GameEvent {
                    tick,
                    kind: EventKind::ResearchCompleted(iac_shared::protocol::ResearchCompletedEvent {
                        tech: q.tech,
                        new_level: q.target_level,
                        player_id: Some(pid),
                    }),
                });
            }

            if player.ship_queue.as_ref().is_some_and(|q| tick >= q.end_tick) {
                let q = player.ship_queue.take().unwrap();
                completed_ships.push((pid, q.item));
                if q.built + 1 < q.count {
                    player.ship_pending.insert(0, PendingShip {
                        id: q.id, item: q.item, count: q.count, built: q.built + 1, reserve: q.reserve,
                    });
                }
                self.dirty_players.insert(pid, ());
            }
        }

        for (pid, item) in completed_ships {
            let kind = match item {
                ShipyardItem::Ship(ship_class) => {
                    self.add_ship_to_homeworld(pid, ship_class)?;
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

    /// Start waiting orders. A free slot starts the earliest waiting order
    /// that can start now (what it needs is built and the stockpile covers
    /// it). An order that cannot keeps its place and is checked again next
    /// tick; one with `reserve` set holds the line instead, so nothing behind
    /// it starts while it is ready and short.
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
            first_startable(
                &p.ship_pending,
                |q| self.ship_ready(p, q.item),
                |q| p.resources.can_afford(q.item.unit_cost()),
                |q| q.reserve,
            )
        }) {
            let q = self.players.get_mut(&pid).unwrap().ship_pending.remove(i);
            self.start_ship_unit(pid, q);
        }
    }

    /// Whether what a building order needs is in place; slots and money aside.
    fn building_ready(&self, p: &Player, b: BuildingType, target: u8) -> bool {
        p.building_queue.iter().all(|q| q.building_type != b)
            && p.buildings.get(b) + 1 == target
            && scaling::building_prerequisites_met(b, &p.buildings)
    }

    fn research_ready(&self, p: &Player, tech: ResearchType, target: u8) -> bool {
        p.buildings.research_lab > 0
            && p.research.get(tech) + 1 == target
            && scaling::research_prerequisites_met(tech, &p.buildings, &p.research)
    }

    fn ship_ready(&self, p: &Player, item: ShipyardItem) -> bool {
        p.buildings.shipyard > 0
            && match item {
                ShipyardItem::Ship(class) => scaling::ship_class_unlocked(class, &p.research),
                ShipyardItem::Defence(kind) => scaling::defence_prerequisites_met(kind, &p.buildings, &p.research),
            }
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
    /// shortfall and the estimated start.
    fn announce_waiting(&mut self, pid: u64, queue_type: iac_shared::protocol::QueueType, id: u64, item: String) {
        use iac_shared::protocol::{QueueAction, QueueType};
        let Some(p) = self.players.get(&pid) else { return; };
        let waits = crate::queue::project(p, self.current_tick, &self.world.pace);
        let wait = match queue_type {
            QueueType::Building => p.building_pending.iter().position(|q| q.id == id).and_then(|i| waits.buildings.get(i)),
            QueueType::Research => p.research_pending.iter().position(|q| q.id == id).and_then(|i| waits.research.get(i)),
            QueueType::Ship => p.ship_pending.iter().position(|q| q.id == id).and_then(|i| waits.ships.get(i)),
        };
        let Some(wait) = wait.cloned() else { return; };
        let mut event = self.queue_event(pid, queue_type, id, item, QueueAction::Waiting);
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
        let ticks = scaling::building_time(b, target, p.buildings.fabricator, &pace);
        p.building_queue.push(BuildQueueEntry { id: order.id, building_type: b, target_level: target, start_tick: tick, end_tick: tick + ticks });
        self.dirty_players.insert(pid, ());
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
        let ticks = scaling::research_time(tech, target, p.buildings.research_lab, &pace);
        p.research_queue = Some(ResearchQueueEntry { id: order.id, tech, target_level: target, start_tick: tick, end_tick: tick + ticks });
        self.dirty_players.insert(pid, ());
        let mut event = self.queue_event(pid, QueueType::Research, order.id, format!("{} Lv.{target}", tech.label()), QueueAction::Started);
        event.paid = cost;
        self.push_queue_event(event);
    }

    /// Pay for and begin the next unit of a shipyard batch. Only the first
    /// unit announces itself; later units start quietly and show up as
    /// `ShipBuilt` / `DefenceBuilt`.
    fn start_ship_unit(&mut self, pid: u64, order: PendingShip) {
        use iac_shared::protocol::{QueueAction, QueueType};
        let tick = self.current_tick;
        let pace = self.world.pace;
        let Some(p) = self.players.get_mut(&pid) else { return; };
        let item = order.item;
        let cost = item.unit_cost();
        p.resources = p.resources.sub(cost);
        let per_unit = item.unit_ticks(p.buildings.shipyard, &pace);
        p.ship_queue = Some(ShipQueueEntry {
            id: order.id, item, count: order.count, built: order.built, reserve: order.reserve,
            start_tick: tick, end_tick: tick + per_unit,
        });
        self.dirty_players.insert(pid, ());
        if order.built == 0 {
            let mut event = self.queue_event(pid, QueueType::Ship, order.id, format!("{} x{}", item.label(), order.count), QueueAction::Started);
            event.paid = cost;
            self.push_queue_event(event);
        }
    }

    // ── Homeworld Raids ───────────────────────────────────────────
    //
    // Forecasted pressure, never a random tax: raids are announced
    // RAID_WARNING_TICKS in advance, sized against the defense the player
    // already has, hard-capped on losses, and pay out salvage when repelled.
    // DefenseGrid exists so a player can leave home without babysitting it.

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

            if let Some(incoming) = state.incoming {
                if tick >= incoming.arrival_tick {
                    state.incoming = None;
                    self.resolve_raid(pid, incoming.power)?;
                }
                continue;
            }

            if tick < state.next_roll_tick { continue; }
            state.next_roll_tick = tick + timers.roll_interval;

            let age = tick.saturating_sub(state.first_seen_tick);
            if age < timers.min_player_age { continue; }
            if tick < state.suppress_until { continue; }
            if tick.saturating_sub(state.last_raid_tick) < timers.min_interval { continue; }

            let Some(player) = self.players.get(&pid) else { continue; };
            // Raids only threaten empires worth raiding — and defenseless
            // beginners are off the menu entirely.
            let eligible = player.buildings.shipyard >= 2 || player.buildings.defense_grid >= 1;
            if !eligible { continue; }

            let base = scaling::raid_power_base(scaling::econ_points(&player.buildings, &player.research));
            let defense = self.home_defense_power(pid);

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
            let arrival_tick = tick + timers.warning;

            if let Some(state) = self.raid_states.get_mut(&pid) {
                state.incoming = Some(IncomingRaid { arrival_tick, power });
                state.missed_rolls = 0;
            }

            let threat = if power < defense * 0.5 {
                "light"
            } else if power < defense {
                "moderate"
            } else {
                "heavy"
            };

            self.pending_events.push(GameEvent {
                tick,
                kind: EventKind::RaidIncoming(iac_shared::protocol::RaidIncomingEvent {
                    player_id: pid,
                    arrival_tick,
                    threat: threat.to_string(),
                    est_power: power,
                }),
            });
            info!("Raid forecast for player {} ({} threat, arrives tick {})", pid, threat, arrival_tick);
        }

        Ok(())
    }

    fn resolve_raid(&mut self, player_id: u64, raid_power: f32) -> Result<(), Box<dyn std::error::Error>> {
        let Some(player) = self.players.get(&player_id) else { return Ok(()); };
        let homeworld = player.homeworld;
        let grid_level = player.buildings.defense_grid;

        // Defense is measured at arrival, not forecast — ships built or
        // recalled during the warning window count.
        let defense_power = self.home_defense_power(player_id);

        let mut rng = StdRng::seed_from_u64(
            self.current_tick.wrapping_mul(0xD6E8FEB86659FD93).wrapping_add(player_id),
        );
        // ±10% fog of war on the outcome so a near-run thing stays tense.
        let effective_defense = defense_power * rng.random_range(0.9..1.1);
        let defended = effective_defense >= raid_power;

        // Raid fleet value, for salvage: treat it as corvette-equivalents.
        let corvette_power = scaling::ship_power(ShipClass::Corvette);
        let corvettes = (raid_power / corvette_power).ceil().max(1.0);
        let raid_value = ShipClass::Corvette.build_cost().scale(corvettes);

        let mut resources_lost = Resources::default();
        let mut ships_lost = 0;
        let mut salvage_dropped: Option<Resources> = None;
        let mut salvage_despawn_tick: Option<u64> = None;
        let mut protected_kept = Resources::default();
        let (structures_lost, structures_restored);

        if defended {
            (structures_lost, structures_restored) =
                self.damage_structures(player_id, (raid_power / defense_power).min(1.0) * RAID_STRUCTURE_DAMAGE, true);
            let salvage = raid_value.scale(RAID_DEFENSE_SALVAGE_FRACTION);
            let key = homeworld.to_key();
            let tick = self.current_tick;
            let ov = self.ensure_override(key);
            ov.salvage = Some(ov.salvage.unwrap_or_default().add(salvage));
            salvage_despawn_tick = Some(tick + SALVAGE_DESPAWN_TICKS as u64);
            ov.salvage_despawn_tick = salvage_despawn_tick;
            self.dirty_sectors.insert(key, ());
            salvage_dropped = Some(salvage);
            // Defenders take shield damage but the grid keeps hulls intact.
            for fleet in self.fleets.values_mut() {
                if fleet.owner_id != player_id || fleet.location != homeworld { continue; }
                for ship in &mut fleet.ships[0..fleet.ship_count] {
                    ship.shield = (ship.shield * 0.5).max(0.0);
                }
                fleet.idle_ticks = 0;
                self.dirty_fleets.insert(fleet.id, ());
            }
        } else {
            (structures_lost, structures_restored) =
                self.damage_structures(player_id, RAID_LOST_STRUCTURE_FRACTION, false);
            ships_lost = self.destroy_docked_ships(player_id, RAID_LOST_SHIP_FRACTION, &mut rng);
            let player_mut = self.players.get_mut(&player_id).unwrap();
            let protected = scaling::storage_protected(player_mut.buildings.storage_vault, &self.world.pace);
            protected_kept = protected.min(player_mut.resources);
            let exposed = player_mut.resources.sub(protected);
            resources_lost = Resources {
                metal: exposed.metal.max(0.0) * RAID_LOSS_CAP_METAL,
                crystal: exposed.crystal.max(0.0) * RAID_LOSS_CAP_CRYSTAL,
                deuterium: exposed.deuterium.max(0.0) * RAID_LOSS_CAP_DEUT,
            };
            player_mut.resources = player_mut.resources.sub(resources_lost);
            self.dirty_players.insert(player_id, ());
        }

        let tick = self.current_tick;
        let timers = RaidTimers::new(&self.world.pace);
        let state = self.raid_states.entry(player_id).or_insert_with(|| RaidState {
            first_seen_tick: tick,
            next_roll_tick: tick + timers.roll_interval,
            last_raid_tick: 0,
            suppress_until: 0,
            missed_rolls: 0,
            incoming: None,
        });
        state.last_raid_tick = tick;
        if !defended {
            state.suppress_until = tick + timers.suppress_after_loss;
        }

        self.pending_events.push(GameEvent {
            tick: self.current_tick,
            kind: EventKind::RaidResolved(iac_shared::protocol::RaidResolvedEvent {
                player_id,
                defended,
                resources_lost,
                salvage_dropped,
                salvage_despawn_tick,
                raid_power,
                defense_power,
                structures_lost,
                structures_restored,
                protected_kept,
                ships_lost,
            }),
        });
        info!(
            "Raid on player {} resolved: {} (raid {:.0} vs defense {:.0}, grid L{})",
            player_id, if defended { "repelled" } else { "lost" },
            raid_power, defense_power, grid_level,
        );

        Ok(())
    }

    /// A raid wears down the structures at home: `fraction` of each kind is
    /// destroyed. With `restore`, 70 percent of what fell comes back free
    /// after a delay. Returns (destroyed, restoring).
    fn damage_structures(&mut self, player_id: u64, fraction: f32, restore: bool) -> (u32, u32) {
        let due = self.current_tick + self.world.pace.finds_ticks(RAID_RESTORE_TICKS as f64);
        let Some(player) = self.players.get_mut(&player_id) else { return (0, 0); };
        let (mut lost, mut coming) = (0, 0);
        for kind in DefenceKind::ALL {
            let i = kind as usize;
            let destroyed = ((player.defences.count[i] as f32 * fraction).round() as u32).min(player.defences.count[i]);
            if destroyed == 0 { continue; }
            player.defences.count[i] -= destroyed;
            lost += destroyed;
            if restore {
                let back = (destroyed as f32 * RAID_STRUCTURE_RESTORE).round() as u32;
                player.defences.restoring[i] += back;
                player.defences.restore_tick[i] = due;
                coming += back;
            }
        }
        if lost > 0 {
            self.dirty_players.insert(player_id, ());
        }
        (lost, coming)
    }

    /// A lost raid reaches the ships docked at home: `fraction` of them (at
    /// least one) are destroyed, chosen at random. Returns how many.
    fn destroy_docked_ships(&mut self, player_id: u64, fraction: f32, rng: &mut StdRng) -> u32 {
        let Some(home) = self.players.get(&player_id).map(|p| p.homeworld) else { return 0; };
        let mut docked: Vec<(u64, usize)> = Vec::new();
        let mut fleet_ids: Vec<u64> = self.fleets.values()
            .filter(|f| f.owner_id == player_id && f.location == home && f.ship_count > 0)
            .map(|f| f.id)
            .collect();
        fleet_ids.sort_unstable();
        for fid in &fleet_ids {
            docked.extend((0..self.fleets[fid].ship_count).map(|i| (*fid, i)));
        }
        if docked.is_empty() { return 0; }
        let lose = ((docked.len() as f32 * fraction).ceil() as usize).clamp(1, docked.len());
        let mut doomed: Vec<(u64, usize)> = Vec::with_capacity(lose);
        for _ in 0..lose {
            doomed.push(docked.swap_remove(rng.random_range(0..docked.len())));
        }
        // Highest index first so removals do not shift the rest.
        doomed.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        for (fid, i) in doomed {
            if let Some(f) = self.fleets.get_mut(&fid) {
                let n = f.ship_count;
                f.ships.copy_within(i + 1..n, i);
                f.ships[n - 1] = Ship::default();
                f.ship_count = n - 1;
                self.dirty_fleets.insert(fid, ());
            }
        }
        for fid in fleet_ids {
            if let Some(f) = self.fleets.get(&fid) {
                if f.ship_count == 0 {
                    self.remove_fleet(fid);
                } else {
                    let fuel_max = fleet_fuel_max(f, &self.players[&player_id]);
                    let f = self.fleets.get_mut(&fid).unwrap();
                    f.fuel_max = fuel_max;
                    f.fuel = f.fuel.min(fuel_max);
                }
            }
        }
        lose as u32
    }

    /// Structures a repelled raid knocked out return when their time is up.
    fn process_defence_restore(&mut self) {
        let tick = self.current_tick;
        let mut notes = Vec::new();
        for player in self.players.values_mut() {
            let mut back = 0;
            for kind in DefenceKind::ALL {
                let i = kind as usize;
                if player.defences.restoring[i] > 0 && tick >= player.defences.restore_tick[i] {
                    player.defences.count[i] += player.defences.restoring[i];
                    back += player.defences.restoring[i];
                    player.defences.restoring[i] = 0;
                }
            }
            if back > 0 {
                notes.push((player.id, back, player.homeworld));
            }
        }
        for (pid, back, home) in notes {
            self.dirty_players.insert(pid, ());
            self.pending_events.push(GameEvent {
                tick,
                kind: EventKind::Alert(AlertEvent {
                    player_id: Some(pid),
                    level: AlertLevel::Info,
                    message: format!("{back} defence structure(s) rebuilt after the raid"),
                    sector: Some(home),
                    fleet_id: None,
                }),
            });
        }
    }

    /// Combat power defending the homeworld: docked ships, the DefenseGrid's
    /// virtual platforms and every defence structure.
    pub fn home_defense_power(&self, player_id: u64) -> f32 {
        let Some(player) = self.players.get(&player_id) else { return 0.0; };
        let mut power = 0.0;
        for fleet in self.fleets.values() {
            if fleet.owner_id != player_id || fleet.location != player.homeworld { continue; }
            for ship in &fleet.ships[0..fleet.ship_count] {
                if ship.hull <= 0.0 { continue; }
                power += ship.weapon_power + (ship.hull + ship.shield) / 10.0;
            }
        }
        power += defense_grid_scout_units(player.buildings.defense_grid)
            * scaling::ship_power(ShipClass::Scout);
        power + player.defences.power(player.buildings.defense_grid)
    }

    // ── Cooldowns ─────────────────────────────────────────────────

    fn process_cooldowns(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (_, fleet) in self.fleets.iter_mut() {
            if fleet.action_cooldown > 0 {
                fleet.action_cooldown -= 1;
            }
            if fleet.state == FleetStatus::Idle {
                fleet.idle_ticks = fleet.idle_ticks.saturating_add(1);
                if fleet.idle_ticks >= SHIELD_REGEN_IDLE_TICKS {
                    for ship in &mut fleet.ships[0..fleet.ship_count] {
                        if ship.shield < ship.shield_max {
                            ship.shield = (ship.shield + ship.shield_max * 0.1).min(ship.shield_max);
                            self.dirty_fleets.insert(fleet.id, ());
                        }
                    }
                }
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
            let player_id = self.register_player(name.to_string()).map_err(|_| AuthError::Server)?;
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
            self.dirty_players.insert(player_id, ());
        }
        token
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
    /// entry point that decides between registering and logging in.
    pub fn register_player(&mut self, name: String) -> Result<u64, ErrorCode> {
        if self.find_player_by_name(&name).is_some() {
            return Err(ErrorCode::InvalidCommand);
        }

        let player_id = self.next_id();
        let homeworld = self.find_homeworld_location(player_id).ok_or(ErrorCode::ServerError)?;

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
            research_queue: None,
            research_pending: Vec::new(),
            token_hash: None,
            agent: false,
            combat_points: 0.0,
            explore_points: 0.0,
            relics: 0,
        };

        self.players.insert(player_id, player);

        let fleet_id = self.next_id();
        let scout_base = ShipClass::Scout.base_stats();
        let scout_stats = scaling::apply_research_to_stats(scout_base, &ResearchLevels::default());

        let mut fleet = Fleet {
            id: fleet_id,
            owner_id: player_id,
            location: homeworld,
            state: FleetStatus::Idle,
            ships: [Ship::default(); MAX_SHIPS_PER_FLEET],
            ship_count: STARTING_SCOUTS,
            cargo: Resources::default(),
            fuel: 0.0,
            fuel_max: 0.0,
            move_cooldown: 0,
            action_cooldown: 0,
            move_target: None,
            idle_ticks: 0,
            harvest_target: HarvestResource::Auto,
            charts: Vec::new(),
        };

        for i in 0..STARTING_SCOUTS {
            fleet.ships[i] = Ship {
                id: self.next_id(),
                ship_class: ShipClass::Scout,
                hull: scout_stats.hull,
                hull_max: scout_stats.hull,
                shield: scout_stats.shield,
                shield_max: scout_stats.shield,
                weapon_power: scout_stats.weapon,
                speed: scout_stats.speed,
            };
        }

        if let Some(player) = self.players.get(&player_id) {
            fleet.fuel_max = fleet_fuel_max(&fleet, player);
            fleet.fuel = fleet.fuel_max;
        }

        self.fleets.insert(fleet_id, fleet);
        self.dirty_players.insert(player_id, ());
        self.dirty_fleets.insert(fleet_id, ());

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
        if fleet.ship_count == 0 { return Err(ErrorCode::FleetNotFound); }
        Ok(fleet)
    }

    pub fn handle_move(&mut self, player_id: u64, fleet_id: u64, target: Hex) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if fleet.state == FleetStatus::InCombat { return Err(ErrorCode::OnCooldown); }
        if fleet.action_cooldown > 0 { return Err(ErrorCode::OnCooldown); }

        // Fleet cap check
        if let Some(player) = self.players.get(&fleet.owner_id)
            && fleet.location == player.homeworld
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

        // Compute cooldown before mutable borrow (needs immutable fleet data)
        let cooldown = fleet_move_cooldown(fleet, research);

        let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
        fleet_mut.fuel -= fuel_cost;
        fleet_mut.state = FleetStatus::Moving;
        fleet_mut.move_target = Some(target);
        fleet_mut.move_cooldown = cooldown;
        fleet_mut.action_cooldown = cooldown;
        self.dirty_fleets.insert(fleet_id, ());

        self.warn_if_cannot_return(fleet_id, target, fuel_cost);
        Ok(())
    }

    /// What moving to the adjacent sector `target` would cost and meet; nothing moves.
    pub fn preview_move(&self, player_id: u64, fleet_id: u64, target: Hex) -> Result<MovePreview, ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if !self.world_gen.connected_neighbors(fleet.location).slice().contains(&target) {
            return Err(ErrorCode::NoConnection);
        }
        let fuel_cost = self.hop_fuel_cost(fleet);
        let can_jump = fleet.fuel >= fuel_cost;
        let fuel_after = if can_jump { fleet.fuel - fuel_cost } else { 0.0 };
        let route = self.home_route(player_id, target);
        let hops_home = route.direct;
        let fuel_to_return = hops_home as f32 * fuel_cost;
        let threat = self.known_threat(player_id, target);
        let fleet_power = fleet_power(fleet);
        let ratio = fleet_power / threat.est_power.max(f32::EPSILON);
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
            threat,
            fleet_power,
            ratio,
            label: scaling::ratio_label(ratio),
        })
    }

    /// What the ore tiles of `coord` hold now; None when the sector has no ore.
    pub fn ore_reserve_at(&self, coord: Hex) -> Option<OreReserve> {
        let template = self.world_gen.generate_sector(coord);
        let ov = self.sector_overrides.get(&coord.to_key());
        let dist = coord.dist_from_origin();
        let tile = |template_d: Density, current: Option<Density>, harvested: f32| {
            let max_units = scaling::ore_reserve_units(template_d, dist);
            let gone = scaling::ore_extracted(template_d, current.unwrap_or(template_d), harvested, dist);
            let rate = scaling::ore_regen_per_tick(template_d, dist, &self.world.pace);
            TileReserve {
                units: (max_units - gone).max(0.0),
                max_units,
                refills_in_s: (gone > scaling::ORE_EPSILON && rate > 0.0).then(|| (gone / rate).ceil() as u64),
            }
        };
        let metal = tile(template.metal_density, ov.and_then(|o| o.metal_density), ov.map_or(0.0, |o| o.metal_harvested));
        let crystal = tile(template.crystal_density, ov.and_then(|o| o.crystal_density), ov.map_or(0.0, |o| o.crystal_harvested));
        let deuterium = tile(template.deut_density, ov.and_then(|o| o.deut_density), ov.map_or(0.0, |o| o.deut_harvested));
        (metal.max_units + crystal.max_units + deuterium.max_units > 0.0)
            .then_some(OreReserve { metal, crystal, deuterium })
    }

    /// The threat of `coord` as this player knows it: the truth while the
    /// sector is live for them, the rating last seen if charted, otherwise
    /// the ring's estimate.
    pub fn known_threat(&self, player_id: u64, coord: Hex) -> ThreatInfo {
        if crate::intel::live_coords(self, player_id).contains(&coord) {
            return self.sector_threat(coord);
        }
        self.known
            .threat(player_id, coord)
            .unwrap_or_else(|| ring_threat(coord, ThreatBasis::Estimate))
    }

    /// The threat of `coord` right now. A group on the spot (or its template
    /// before it materialises) is rated by its own power; otherwise the ring's.
    pub fn sector_threat(&self, coord: Hex) -> ThreatInfo {
        let mut observed: Option<f32> = None;
        for npc in self.npc_fleets.values().filter(|n| n.location == coord && n.ship_count > 0) {
            let power: f32 = npc.ships[..npc.ship_count as usize].iter()
                .filter(|s| s.hull > 0.0)
                .map(|s| s.weapon_power + (s.hull + s.shield) / 10.0)
                .sum();
            *observed.get_or_insert(0.0) += power * behavior_threat_bonus(npc.behavior);
        }
        if observed.is_none()
            && let Some(tmpl) = self.pending_template_npc(coord)
        {
            observed = Some(tmpl.power() * behavior_threat_bonus(tmpl.behavior));
        }
        match observed {
            Some(est_power) => ThreatInfo { rating: scaling::threat_rating(est_power), est_power, basis: ThreatBasis::Observed },
            None => {
                let lair = self.world_gen.generate_sector(coord).npc_template.is_some();
                ring_threat(coord, if lair { ThreatBasis::Template } else { ThreatBasis::Estimate })
            }
        }
    }

    /// Fuel for one jump of this fleet.
    pub fn hop_fuel_cost(&self, fleet: &Fleet) -> f32 {
        let research = self.players.get(&fleet.owner_id).map(|p| &p.research);
        fleet_fuel_cost(fleet, research)
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
        let direct = self.lane_path(from, home, 64, &|_| true);
        let charted = self.lane_path(from, home, 64, &entered).map(|p| p.len() as u32);
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

    /// Fuel the fleet would burn getting home from `from`, one jump per hop.
    pub fn route_home_cost(&self, fleet: &Fleet, from: Hex) -> f32 {
        self.home_route(fleet.owner_id, from).direct as f32 * self.hop_fuel_cost(fleet)
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
    fn warn_if_cannot_return(&mut self, fleet_id: u64, target: Hex, hop_cost: f32) {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return; };
        let Some(player) = self.players.get(&fleet.owner_id) else { return; };
        if target == player.homeworld { return; }
        let route = self.home_route(fleet.owner_id, target);
        let need = route.direct as f32 * hop_cost;
        let have = fleet.fuel;
        if have + 0.001 >= need { return; }
        let (level, outcome) = if have + 0.001 < hop_cost {
            (AlertLevel::Critical, "it will be stranded on arrival; the emergency reserve recovers one hop in about 5 minutes")
        } else {
            (AlertLevel::Warning, "it cannot reach home without help")
        };
        let across = if route.unexplored { ", across unexplored space" } else { "" };
        let way = match route.charted {
            Some(c) if c != route.direct => format!(
                "the charted route home is {c} hops, {:.0} fuel; the direct route is {} hops, {need:.0} fuel{across}",
                c as f32 * hop_cost, route.direct,
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
        self.dirty_fleets.insert(fleet_id, ());

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

    /// Fuel an emergency jump home would burn: the per-hop cost over the
    /// straight distance, doubled. All-or-nothing: a fleet with less than
    /// this does not move at all.
    pub fn recall_fuel_cost(&self, fleet: &Fleet) -> f32 {
        let dist = self.players.get(&fleet.owner_id)
            .map(|p| Hex::distance(&fleet.location, &p.homeworld) as f32)
            .unwrap_or(0.0);
        self.hop_fuel_cost(fleet) * dist * RECALL_FUEL_MULTIPLIER
    }

    pub fn handle_recall(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        let player = self.players.get(&fleet.owner_id).ok_or(ErrorCode::ServerError)?;

        let dist = Hex::distance(&fleet.location, &player.homeworld) as f32;
        let fuel_cost = self.recall_fuel_cost(fleet);
        if fleet.fuel < fuel_cost { return Err(ErrorCode::InsufficientFuel); }

        let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
        fleet_mut.fuel -= fuel_cost;

        let base_damage_chance = RECALL_DAMAGE_CHANCE_PER_HEX * dist;
        let ej_reduction = scaling::recall_damage_reduction(player.research.emergency_jump);
        let damage_chance = RECALL_DAMAGE_CHANCE_CAP.min(base_damage_chance.max(0.0) - ej_reduction.max(0.0));

        let mut rng = StdRng::seed_from_u64(
            self.world_gen.world_seed ^ self.current_tick.wrapping_mul(0x9E3779B97F4A7C15) ^ fleet_id.rotate_left(32)
        );

        let mut i = 0;
        while i < fleet_mut.ship_count {
            let roll = rng.random_range(0.0..1.0);
            if roll < damage_chance {
                let damage_pct = RECALL_HULL_DAMAGE_MIN + rng.random_range(0.0..1.0) * (RECALL_HULL_DAMAGE_MAX - RECALL_HULL_DAMAGE_MIN);
                fleet_mut.ships[i].hull -= fleet_mut.ships[i].hull_max * damage_pct;

                if fleet_mut.ships[i].hull <= 0.0 {
                    self.pending_events.push(GameEvent {
                        tick: self.current_tick,
                        kind: EventKind::ShipDestroyed(iac_shared::protocol::ShipDestroyedEvent {
                            ship_id: fleet_mut.ships[i].id,
                            ship_class: fleet_mut.ships[i].ship_class,
                            owner_fleet_id: fleet_id,
                            is_npc: false,
                            sector: fleet_mut.location,
                            owner: Some(player.name.clone()),
                            mine: false,
                        }),
                    });
                    // Swap-remove
                    fleet_mut.ship_count -= 1;
                    fleet_mut.ships[i] = fleet_mut.ships[fleet_mut.ship_count];
                    continue;
                }
            }
            i += 1;
        }

        fleet_mut.location = player.homeworld;
        fleet_mut.state = if fleet_mut.ship_count == 0 { FleetStatus::Docked } else { FleetStatus::Idle };
        fleet_mut.move_target = None;

        self.dock_fleet(fleet_id).map_err(|_| ErrorCode::ServerError)?;

        Ok(())
    }

    fn fleet_is_free(fleet: &Fleet) -> bool {
        fleet.state == FleetStatus::Idle || fleet.state == FleetStatus::Docked
    }

    /// Detach `ship_ids` from a fleet into a new fleet at the same sector and
    /// return its id. Fuel and cargo follow the ships in proportion to the
    /// tank and hold they take along.
    pub fn handle_split(&mut self, player_id: u64, fleet_id: u64, ship_ids: &[u64]) -> Result<u64, ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if !Self::fleet_is_free(fleet) { return Err(ErrorCode::OnCooldown); }

        let mut take: Vec<u64> = ship_ids.to_vec();
        take.sort_unstable();
        take.dedup();
        let aboard = &fleet.ships[0..fleet.ship_count];
        if take.is_empty()
            || take.len() >= fleet.ship_count
            || take.iter().any(|id| !aboard.iter().any(|s| s.id == *id))
        {
            return Err(ErrorCode::InvalidTarget);
        }

        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        let owned = self.fleets.values().filter(|f| f.owner_id == player_id && f.ship_count > 0).count();
        if owned >= MAX_FLEETS_TOTAL { return Err(ErrorCode::FleetLimitReached); }
        if fleet.location != player.homeworld
            && self.count_deployed_fleets(player_id, player.homeworld) >= MAX_FLEETS_PER_PLAYER
        {
            return Err(ErrorCode::FleetLimitReached);
        }

        let (kept, taken): (Vec<Ship>, Vec<Ship>) = aboard.iter().copied().partition(|s| !take.contains(&s.id));
        let mut new_fleet = Fleet {
            id: 0,
            owner_id: player_id,
            location: fleet.location,
            state: FleetStatus::Idle,
            ships: [Ship::default(); MAX_SHIPS_PER_FLEET],
            ship_count: taken.len(),
            cargo: Resources::default(),
            fuel: 0.0,
            fuel_max: 0.0,
            move_cooldown: fleet.move_cooldown,
            action_cooldown: fleet.action_cooldown,
            move_target: None,
            idle_ticks: fleet.idle_ticks,
            harvest_target: fleet.harvest_target,
            charts: fleet.charts.clone(),
        };
        new_fleet.ships[..taken.len()].copy_from_slice(&taken);
        let mut rest = fleet.clone();
        rest.ships = [Ship::default(); MAX_SHIPS_PER_FLEET];
        rest.ships[..kept.len()].copy_from_slice(&kept);
        rest.ship_count = kept.len();

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
        let count = new_fleet.ship_count;
        self.fleets.insert(fleet_id, rest);
        self.fleets.insert(new_id, new_fleet);
        self.dirty_fleets.insert(fleet_id, ());
        self.dirty_fleets.insert(new_id, ());
        self.push_alert(
            AlertLevel::Info, new_id, location,
            format!("fleet {fleet_id} split: {count} ship(s) now fly as fleet {new_id}"),
        );
        Ok(new_id)
    }

    /// Fold `other_id` into `fleet_id`: ships, cargo and fuel pool and the
    /// absorbed fleet and its standing orders cease to exist.
    pub fn handle_merge(&mut self, player_id: u64, fleet_id: u64, other_id: u64) -> Result<(), ErrorCode> {
        if fleet_id == other_id { return Err(ErrorCode::InvalidTarget); }
        let a = self.owned_fleet(player_id, fleet_id)?;
        let b = self.owned_fleet(player_id, other_id)?;
        if a.location != b.location { return Err(ErrorCode::NotInSector); }
        if !Self::fleet_is_free(a) || !Self::fleet_is_free(b) { return Err(ErrorCode::OnCooldown); }
        if a.ship_count + b.ship_count > MAX_SHIPS_PER_FLEET { return Err(ErrorCode::InvalidCommand); }
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;

        let mut merged = a.clone();
        merged.ships[a.ship_count..a.ship_count + b.ship_count].copy_from_slice(&b.ships[..b.ship_count]);
        merged.ship_count = a.ship_count + b.ship_count;
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
        let count = merged.ship_count;

        let orders_ended = self.policies.contains_key(&other_id);
        self.remove_fleet(other_id);
        self.fleets.insert(fleet_id, merged);
        self.dirty_fleets.insert(fleet_id, ());
        self.push_alert(
            AlertLevel::Info, fleet_id, location,
            format!(
                "fleet {other_id} merged into fleet {fleet_id}: {count} ship(s){}",
                if orders_ended { "; the absorbed fleet's standing orders ended" } else { "" },
            ),
        );
        Ok(())
    }

    /// A payment larger than a stockpile cap can never be made until the
    /// Storage Vault grows.
    fn ensure_fits_storage(&self, player: &Player, cost: Resources) -> Result<(), ErrorCode> {
        let cap = scaling::storage_cap(player.buildings.storage_vault, &self.world.pace);
        if ResourceKind::ALL.into_iter().any(|k| cost.get(k) > cap.get(k)) {
            return Err(ErrorCode::StorageTooSmall);
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
            Command::BuildShip { ship_class, .. } => Some((
                format!("one {}", ship_class.label()),
                ship_class.build_cost(),
            )),
            Command::BuildDefence { kind, .. } => Some((
                format!("one {}", kind.label()),
                kind.build_cost(),
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
        if !scaling::building_prerequisites_met(building_type, &projected) {
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
        self.dirty_players.insert(player_id, ());
        self.advance_queues(player_id);
        self.announce_waiting(player_id, QueueType::Building, id, format!("{} Lv.{target_level}", building_type.label()));
        Ok(())
    }

    pub fn handle_research(&mut self, player_id: u64, tech: ResearchType, reserve: bool) -> Result<(), ErrorCode> {
        use iac_shared::protocol::QueueType;
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        if player.projected_buildings().research_lab == 0 { return Err(ErrorCode::NoResearchLab); }

        let projected = player.projected_research();
        let current_level = projected.get(tech);
        if current_level >= scaling::research_max_level(tech) { return Err(ErrorCode::MaxLevelReached); }
        if !scaling::research_prerequisites_met(tech, &player.projected_buildings(), &projected) {
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
        self.dirty_players.insert(player_id, ());
        self.advance_queues(player_id);
        self.announce_waiting(player_id, QueueType::Research, id, format!("{} Lv.{target_level}", tech.label()));
        Ok(())
    }

    pub fn handle_build_ship(&mut self, player_id: u64, ship_class: ShipClass, count: u16, reserve: bool) -> Result<(), ErrorCode> {
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        if player.projected_buildings().shipyard == 0 { return Err(ErrorCode::NoShipyard); }
        if !scaling::ship_class_unlocked(ship_class, &player.projected_research()) { return Err(ErrorCode::ShipLocked); }
        self.queue_shipyard(player_id, ShipyardItem::Ship(ship_class), count, reserve)
    }

    pub fn handle_build_defence(&mut self, player_id: u64, kind: DefenceKind, count: u16, reserve: bool) -> Result<(), ErrorCode> {
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        if player.projected_buildings().shipyard == 0 { return Err(ErrorCode::NoShipyard); }
        if !scaling::defence_prerequisites_met(kind, &player.projected_buildings(), &player.projected_research()) {
            return Err(ErrorCode::DefenceLocked);
        }
        self.queue_shipyard(player_id, ShipyardItem::Defence(kind), count, reserve)
    }

    /// Queue a shipyard batch behind the ones already there. It pays and
    /// starts one unit at a time (see `advance_queues`).
    fn queue_shipyard(&mut self, player_id: u64, item: ShipyardItem, count: u16, reserve: bool) -> Result<(), ErrorCode> {
        use iac_shared::protocol::QueueType;
        let player = self.players.get(&player_id).ok_or(ErrorCode::ServerError)?;
        if count == 0 { return Err(ErrorCode::InvalidCommand); }
        if player.ship_queue.iter().count() + player.ship_pending.len() > scaling::QUEUE_WAITING {
            return Err(ErrorCode::QueueFull);
        }
        self.ensure_fits_storage(player, item.unit_cost())?;

        let id = self.next_id();
        let player_mut = self.players.get_mut(&player_id).unwrap();
        player_mut.ship_pending.push(PendingShip { id, item, count, built: 0, reserve });
        self.dirty_players.insert(player_id, ());
        self.advance_queues(player_id);
        self.announce_waiting(player_id, QueueType::Ship, id, format!("{} x{count}", item.label()));
        Ok(())
    }

    /// Cancel an item that has started, by id. It keeps half its payment
    /// (one unit for a ship batch), and waiting items of the same kind move
    /// up a level.
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
                (label, q.item.unit_cost().scale(CANCEL_REFUND_FRACTION))
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
        self.dirty_players.insert(player_id, ());
        let mut event = self.queue_event(player_id, queue_type, id, item, QueueAction::Cancelled);
        event.refunded = refund;
        self.push_queue_event(event);
        self.advance_queues(player_id);
        Ok(())
    }

    /// Remove a waiting item by id. Nothing was paid for it, so nothing comes
    /// back (units of a part-built batch are kept).
    pub fn handle_cancel_queued(&mut self, player_id: u64, queue_type: iac_shared::protocol::QueueType, id: u64) -> Result<(), ErrorCode> {
        use iac_shared::protocol::{QueueAction, QueueType};
        let player = self.players.get_mut(&player_id).ok_or(ErrorCode::ServerError)?;
        let item = match queue_type {
            QueueType::Building => {
                let index = player.building_pending.iter().position(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let q = player.building_pending.remove(index);
                for later in player.building_pending.iter_mut()
                    .filter(|p| p.building_type == q.building_type && p.target_level > q.target_level)
                {
                    later.target_level -= 1;
                }
                format!("{} Lv.{}", q.building_type.label(), q.target_level)
            }
            QueueType::Ship => {
                let index = player.ship_pending.iter().position(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let q = player.ship_pending.remove(index);
                if q.built > 0 {
                    format!("{} x{} ({} already built and kept)", q.item.label(), q.count, q.built)
                } else {
                    format!("{} x{}", q.item.label(), q.count)
                }
            }
            QueueType::Research => {
                let index = player.research_pending.iter().position(|q| q.id == id).ok_or(ErrorCode::InvalidTarget)?;
                let q = player.research_pending.remove(index);
                for later in player.research_pending.iter_mut()
                    .filter(|p| p.tech == q.tech && p.target_level > q.target_level)
                {
                    later.target_level -= 1;
                }
                format!("{} Lv.{}", q.tech.label(), q.target_level)
            }
        };
        self.dirty_players.insert(player_id, ());
        let event = self.queue_event(player_id, queue_type, id, item, QueueAction::Cancelled);
        self.push_queue_event(event);
        self.advance_queues(player_id);
        Ok(())
    }

    /// Active scan: reveal connected sectors around the fleet and pick up
    /// faint signal contacts one hop beyond. Scouts extend the range —
    /// that's their job.
    pub fn handle_scan(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        if fleet.state == FleetStatus::InCombat { return Err(ErrorCode::OnCooldown); }
        if fleet.state == FleetStatus::Moving { return Err(ErrorCode::OnCooldown); }
        if fleet.action_cooldown > 0 { return Err(ErrorCode::OnCooldown); }

        let origin = fleet.location;
        let has_scout = fleet.ships[0..fleet.ship_count].iter()
            .any(|s| s.ship_class == ShipClass::Scout);
        let range = if has_scout { SCAN_SCOUT_RANGE } else { SCAN_BASE_RANGE };

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
        fleet_mut.idle_ticks = 0;

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
    /// already burned — that's the price of hesitation.
    pub fn handle_stop(&mut self, player_id: u64, fleet_id: u64) -> Result<(), ErrorCode> {
        let fleet = self.owned_fleet(player_id, fleet_id)?;
        match fleet.state {
            FleetStatus::Moving | FleetStatus::Harvesting | FleetStatus::Exploring => {
                // Backing out of a boarding leaves the site on alert.
                let was_exploring = fleet.state == FleetStatus::Exploring;
                if was_exploring {
                    let key = fleet.location.to_key();
                    let ov = self.ensure_override(key);
                    ov.site_ambush_bumps = ov.site_ambush_bumps.saturating_add(1);
                    self.dirty_sectors.insert(key, ());
                }
                let fleet_mut = self.fleets.get_mut(&fleet_id).unwrap();
                fleet_mut.state = FleetStatus::Idle;
                fleet_mut.move_target = None;
                fleet_mut.move_cooldown = 0;
                if was_exploring {
                    // action_cooldown was the boarding timer — release it.
                    fleet_mut.action_cooldown = 0;
                }
                fleet_mut.idle_ticks = 0;
                self.dirty_fleets.insert(fleet_id, ());
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
        let ov = self.sector_overrides.get(&coord.to_key());
        if ov.map(|o| o.site_looted_tick.is_some()).unwrap_or(false) { return None; }
        let bumps = ov.map(|o| o.site_ambush_bumps).unwrap_or(0);
        Some((scaling::derelict_tier(coord.dist_from_origin()), bumps))
    }

    /// Ambush odds for a boarding attempt by this fleet at `site`.
    fn site_ambush_chance(&self, fleet: &Fleet, site: Hex, bumps: u8) -> f32 {
        let base = scaling::derelict_ambush(ring_rating(site));
        let has_scout = fleet.ships[0..fleet.ship_count].iter()
            .any(|s| s.ship_class == ShipClass::Scout);
        let scout_bonus = if has_scout { EXPLORE_SCOUT_AMBUSH_REDUCTION } else { 0.0 };
        (base + bumps as f32 * EXPLORE_RETRY_AMBUSH_BUMP - scout_bonus).clamp(0.02, 0.95)
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
                self.dirty_sectors.insert(key, ());
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
            self.dirty_fleets.insert(fleet_id, ());
        }
    }

    /// Pay explore points for the charts a docked fleet carries.
    fn deliver_charts(&mut self, fleet_id: u64) {
        let Some(fleet) = self.fleets.get_mut(&fleet_id) else { return; };
        if fleet.charts.is_empty() { return; }
        let owner = fleet.owner_id;
        let charts = std::mem::take(&mut fleet.charts);
        self.dirty_fleets.insert(fleet_id, ());

        let (mut sectors, mut points) = (0u32, 0.0f32);
        for key in charts {
            if self.credited.insert((owner, key, ExploreCredit::Charted)) {
                let sector = Hex::from_key(key);
                self.new_credits.push((owner, sector, ExploreCredit::Charted));
                points += score::chart_points(ring_rating(sector));
                sectors += 1;
            }
        }
        if sectors == 0 { return; }
        if let Some(p) = self.players.get_mut(&owner) {
            p.explore_points += points;
            self.dirty_players.insert(owner, ());
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
        fleet_mut.idle_ticks = 0;
        self.dirty_fleets.insert(fleet_id, ());

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

    /// Resolve boardings whose timer has run out.
    fn process_exploration(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let due: Vec<u64> = self.fleets.values()
            .filter(|f| f.state == FleetStatus::Exploring && f.action_cooldown == 0)
            .map(|f| f.id)
            .collect();
        for fleet_id in due {
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
            self.dirty_fleets.insert(fleet_id, ());
            return Ok(());
        };

        let fleet = self.fleets.get(&fleet_id).unwrap();
        let ambush_chance = self.site_ambush_chance(fleet, location, bumps);
        let has_hauler = fleet.ships[0..fleet.ship_count].iter()
            .any(|s| s.ship_class == ShipClass::Hauler);

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
            self.dirty_sectors.insert(key, ());

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
            * rng.random_range(scaling::DERELICT_ROLL_MIN..=scaling::DERELICT_ROLL_MAX);
        let loot_mult = self.world.pace.finds_mult() * if has_hauler { EXPLORE_HAULER_LOOT_MULTIPLIER } else { 1.0 };
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
        self.dirty_fleets.insert(fleet_id, ());

        let tick = self.current_tick;
        let ov = self.ensure_override(key);
        ov.site_looted_tick = Some(tick);
        if overflow.metal + overflow.crystal + overflow.deuterium > 1.0 {
            ov.salvage = Some(ov.salvage.unwrap_or_default().add(overflow));
            ov.salvage_despawn_tick = Some(tick + SALVAGE_DESPAWN_TICKS as u64);
        }
        self.dirty_sectors.insert(key, ());

        if let Some(class) = recovered_ship {
            self.add_recovered_ship(fleet_id, class, &mut rng);
        }
        if let Some(tech) = tech_cache {
            self.grant_tech_level(owner_id, tech);
        }

        let mut points = 0.0;
        if self.credited.insert((owner_id, key, ExploreCredit::Boarded)) {
            self.new_credits.push((owner_id, location, ExploreCredit::Boarded));
            points += score::board_points(scaling::split_loot(base_value).scale(self.world.pace.finds_mult()), rating);
        }
        if relic {
            points += scaling::RELIC_POINTS;
        }
        if let Some(p) = self.players.get_mut(&owner_id) {
            p.explore_points += points;
            p.relics += u32::from(relic);
            self.dirty_players.insert(owner_id, ());
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

    /// Wedge a battered-but-flyable ship into the fleet, if there's room.
    fn add_recovered_ship(&mut self, fleet_id: u64, class: ShipClass, rng: &mut StdRng) {
        let research = self.fleets.get(&fleet_id)
            .and_then(|f| self.players.get(&f.owner_id))
            .map(|p| p.research.clone())
            .unwrap_or_default();
        let stats = scaling::apply_research_to_stats(class.base_stats(), &research);
        let hull_frac: f32 = rng.random_range(0.35..0.70);
        let ship_id = self.next_id();
        if let Some(f) = self.fleets.get_mut(&fleet_id) {
            if f.ship_count >= MAX_SHIPS_PER_FLEET { return; }
            f.ships[f.ship_count] = Ship {
                id: ship_id,
                ship_class: class,
                hull: stats.hull * hull_frac,
                hull_max: stats.hull,
                shield: 0.0,
                shield_max: stats.shield,
                weapon_power: stats.weapon,
                speed: stats.speed,
            };
            f.ship_count += 1;
            self.dirty_fleets.insert(fleet_id, ());
        }
    }

    /// A data core grants an instant level in a random low tech.
    fn pick_tech_cache(&self, player_id: u64, rng: &mut StdRng) -> Option<ResearchType> {
        let player = self.players.get(&player_id)?;
        let candidates: Vec<ResearchType> = ResearchType::ALL.iter().copied()
            .filter(|&t| {
                let lvl = player.research.get(t);
                lvl < 3 && lvl < scaling::research_max_level(t)
            })
            .collect();
        if candidates.is_empty() { return None; }
        Some(candidates[rng.random_range(0..candidates.len())])
    }

    fn grant_tech_level(&mut self, player_id: u64, tech: ResearchType) {
        let Some(player) = self.players.get_mut(&player_id) else { return; };
        let new_level = player.research.get(tech) + 1;
        player.research.set(tech, new_level);
        self.dirty_players.insert(player_id, ());
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
            self.dirty_policies.remove(&fleet_id);
            self.deleted_policy_ids.insert(fleet_id, ());
            self.push_policy_event(fleet_id, PolicyPreset::Manual, "clear", "standing orders cleared");
            return Ok(());
        }

        // Clamp wire params to the same bounds the DB reload enforces,
        // or behavior would change across a restart.
        let mut params = params.unwrap_or_default();
        params.min_fuel_pct = params.min_fuel_pct.min(90);
        params.cargo_return_pct = params.cargo_return_pct.clamp(10, 100);
        params.max_range = params.max_range.clamp(1, 30);
        params.engage_ratio_x10 = params.engage_ratio_x10.clamp(1, 100);

        let policy = FleetPolicy {
            preset,
            params,
            work_sector: Some(location),
            next_eval_tick: self.current_tick + 1,
            last_hold: None,
        };
        self.policies.insert(fleet_id, policy);
        self.deleted_policy_ids.remove(&fleet_id);
        self.dirty_policies.insert(fleet_id, ());
        // Policies persist alongside their fleet, so make sure the fleet
        // row lands in the same transaction (FK).
        self.dirty_fleets.insert(fleet_id, ());
        let reason = format!("doctrine set: {}", preset.label());
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
                self.dirty_policies.remove(&fid);
                self.deleted_policy_ids.insert(fid, ());
                continue;
            };
            if fleet.ship_count == 0 { continue; }
            // Only steer a fleet that is free to act.
            if fleet.state != FleetStatus::Idle && fleet.state != FleetStatus::Docked { continue; }
            if fleet.action_cooldown > 0 || fleet.move_cooldown > 0 { continue; }

            if let Some(p) = self.policies.get_mut(&fid) {
                p.next_eval_tick = tick + POLICY_EVAL_INTERVAL as u64;
            }

            let acted = match preset {
                PolicyPreset::Manual => { self.policies.remove(&fid); true }
                PolicyPreset::Prospect => self.policy_prospect(fid)?,
                PolicyPreset::MineAndReturn => self.policy_mine_and_return(fid)?,
                PolicyPreset::SalvageAndSites => self.policy_salvage_and_sites(fid)?,
                PolicyPreset::PatrolHome => self.policy_patrol_home(fid)?,
            };

            // Nothing to do — check back later instead of spinning.
            if !acted && let Some(p) = self.policies.get_mut(&fid) {
                p.next_eval_tick = tick + POLICY_EVAL_INTERVAL as u64 * 5;
            }
        }
        Ok(())
    }

    /// Most hops from home the fleet can fly and still afford the way back
    /// with the doctrine's fuel reserve intact, capped by `max_range`.
    fn policy_effective_range(&self, fleet: &Fleet, params: &PolicyParams) -> u8 {
        let hop = self.hop_fuel_cost(fleet);
        if hop <= 0.0 { return params.max_range; }
        let reserve = fleet.fuel_max * params.min_fuel_pct as f32 / 100.0;
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
    /// is judged against the actual cost of the charted route home plus the
    /// doctrine's reserve, not a fraction of the tank.
    fn policy_return_reason(&self, fleet: &Fleet, params: &PolicyParams) -> Option<String> {
        let need = self.route_home_cost(fleet, fleet.location);
        let reserve = fleet.fuel_max * params.min_fuel_pct as f32 / 100.0;
        if fleet.fuel < need + reserve {
            return Some(format!(
                "fuel {:.0} is below the route home {:.0} plus reserve {:.0}",
                fleet.fuel, need, reserve,
            ));
        }
        let max_cargo = fleet_cargo_capacity(fleet, &self.world.pace);
        if max_cargo > 0.0 {
            let used = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
            let cargo_pct = used / max_cargo * 100.0;
            if cargo_pct >= params.cargo_return_pct as f32 {
                return Some(format!("cargo {:.0}%", cargo_pct));
            }
        }
        None
    }

    /// Announce that the autopilot is standing still and why; the same
    /// reason is repeated at most once a minute. Always returns false.
    fn hold_policy(&mut self, fleet_id: u64, preset: PolicyPreset, reason: &str) -> bool {
        let tick = self.current_tick;
        let announce = match self.policies.get_mut(&fleet_id) {
            Some(p) => {
                let key = hold_key(reason);
                let repeat = matches!(&p.last_hold, Some((k, t)) if *k == key && tick < t + 60);
                if !repeat { p.last_hold = Some((key, tick)); }
                !repeat
            }
            None => true,
        };
        if announce { self.push_policy_event(fleet_id, preset, "hold", reason); }
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
        let mut prev: HashMap<u32, u32> = HashMap::new();
        let mut frontier = vec![from];
        prev.insert(from.to_key(), from.to_key());
        let mut found: Option<Hex> = None;

        'search: for _ in 0..max_hops {
            let mut next = Vec::new();
            for coord in &frontier {
                for &n in self.world_gen.connected_neighbors(*coord).slice() {
                    let key = n.to_key();
                    if prev.contains_key(&key) || !passable(n) { continue; }
                    prev.insert(key, coord.to_key());
                    if goal(n) { found = Some(n); break 'search; }
                    next.push(n);
                }
            }
            frontier = next;
        }

        let from_key = from.to_key();
        let mut path = vec![found?];
        let mut cur = found?.to_key();
        while prev[&cur] != from_key {
            cur = prev[&cur];
            path.push(Hex::from_key(cur));
        }
        path.reverse();
        Some(path)
    }

    fn bfs_first_hop(
        &self,
        from: Hex,
        max_hops: u8,
        passable: &dyn Fn(Hex) -> bool,
        goal: &dyn Fn(Hex) -> bool,
    ) -> Option<Hex> {
        self.bfs_path(from, max_hops, passable, goal)?.first().copied()
    }

    fn first_hop_toward(&self, from: Hex, target: Hex, max_hops: u8) -> Option<Hex> {
        if from == target { return None; }
        self.bfs_first_hop(from, max_hops, &|_| true, &|n| n == target)
    }

    /// One hop along the connected graph toward `target`, through sectors
    /// the fleet may enter. Heading home falls back to the plain shortest
    /// lane when no safe route exists; any other target is refused with the
    /// reason. True if the autopilot acted (moved, or reported why it
    /// cannot).
    fn policy_step_toward(&mut self, fleet_id: u64, target: Hex, preset: PolicyPreset, reason: &str) -> bool {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return false; };
        let from = fleet.location;
        if from == target { return false; }
        let Some(home) = self.players.get(&fleet.owner_id).map(|p| p.homeworld) else { return false; };
        let params = self.policies.get(&fleet_id).map(|p| p.params).unwrap_or_default();
        let homeward = target == home;
        let enter = |n: Hex| n == home || self.policy_entry_check(fleet, &params, n).is_ok();

        let step = match self.bfs_first_hop(from, 16, &enter, &|n| n == target) {
            Some(step) => step,
            None if homeward => {
                match self.first_hop_toward(from, target, 16).or_else(|| self.greedy_hop(from, target, &|_| true)) {
                    Some(step) => step,
                    None => return self.hold_policy(fleet_id, preset, &format!("no lane toward {target}")),
                }
            }
            None => {
                let blocker = self.bfs_path(from, 16, &|_| true, &|n| n == target).and_then(|path| {
                    path.iter().find_map(|&n| self.policy_entry_check(fleet, &params, n).err())
                });
                match (blocker, self.greedy_hop(from, target, &enter)) {
                    (None, Some(step)) => step,
                    (why, _) => {
                        let why = why.unwrap_or_else(|| "no lane it may take".to_string());
                        return self.hold_policy(fleet_id, preset, &format!("refusing the way to {target}: {why}"));
                    }
                }
            }
        };
        self.policy_move(fleet_id, step, homeward, preset, reason)
    }

    /// Neighbour of `from` closest to `target` among those `allow`s; the
    /// fallback when `target` lies beyond the search budget.
    fn greedy_hop(&self, from: Hex, target: Hex, allow: &dyn Fn(Hex) -> bool) -> Option<Hex> {
        self.world_gen.connected_neighbors(from).slice().iter().copied()
            .filter(|&n| allow(n))
            .min_by_key(|n| Hex::distance(n, &target))
    }

    /// Whether the autopilot may take `fleet` into `coord`, judged by the
    /// threat the player sees there (live, charted or the ring's estimate)
    /// against the fleet's own power: exactly the doctrine's engage ratio,
    /// whether or not hostiles sit there. A player who set it below EVEN is
    /// honoured; a default or higher setting is the bar. Err carries the reason.
    fn policy_entry_check(&self, fleet: &Fleet, params: &PolicyParams, coord: Hex) -> Result<(), String> {
        let hostile = self.sector_has_hostiles(coord);
        let need = params.engage_ratio_x10 as f32 / 10.0;
        let threat = self.known_threat(fleet.owner_id, coord);
        let power = fleet_power(fleet);
        let ratio = power / threat.est_power.max(f32::EPSILON);
        if ratio >= need { return Ok(()); }
        let what = if hostile { "holds hostiles" } else { "is unscouted or unsafe" };
        Err(format!(
            "{coord} {what} (T{} power {:.0} against your {:.0}: {ratio:.2}x {}, the doctrine needs {need:.1}x)",
            threat.rating, threat.est_power, power, scaling::ratio_label(ratio).label(),
        ))
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
            let reserve = fleet.fuel_max * min_pct as f32 / 100.0;
            let need = self.home_route(owner, step).direct as f32 * hop + reserve;
            if fuel - hop < need {
                let why = format!(
                    "a jump to {step} would leave {:.0} fuel against {:.0} needed to return home",
                    (fuel - hop).max(0.0), need,
                );
                let home = self.players.get(&owner).map(|p| p.homeworld);
                if let Some(home) = home && from != home {
                    return self.policy_step_toward(fleet_id, home, preset, &format!("returning: {why}"));
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

    /// Nearest sector (BFS over the connected graph) matching `what`.
    fn find_nearest_target(&self, from: Hex, max_hops: u8, what: PolicyTarget) -> Option<Hex> {
        let mut visited: std::collections::HashSet<u32> = std::collections::HashSet::new();
        let mut frontier = vec![from];
        visited.insert(from.to_key());

        for _ in 0..max_hops {
            let mut next = Vec::new();
            for coord in &frontier {
                for &n in self.world_gen.connected_neighbors(*coord).slice() {
                    if !visited.insert(n.to_key()) { continue; }
                    if self.policy_target_matches(n, what) { return Some(n); }
                    next.push(n);
                }
            }
            frontier = next;
        }
        None
    }

    fn policy_target_matches(&self, coord: Hex, what: PolicyTarget) -> bool {
        match what {
            PolicyTarget::Resources => {
                if self.sector_has_hostiles(coord) { return false; }
                let template = self.world_gen.generate_sector(coord);
                let ov = self.sector_overrides.get(&coord.to_key());
                let (m, c, d) = SectorOverride::effective_densities(ov, &template);
                m != Density::None || c != Density::None || d != Density::None
            }
            PolicyTarget::SalvageOrSite => {
                if self.sector_has_hostiles(coord) { return false; }
                let ov = self.sector_overrides.get(&coord.to_key());
                ov.and_then(|o| o.salvage).is_some() || self.derelict_site_at(coord).is_some()
            }
            PolicyTarget::Hostiles => self.sector_has_hostiles(coord),
        }
    }

    /// Estimated combat power of whatever is hostile in a sector.
    fn hostile_power_at(&self, coord: Hex) -> f32 {
        let mut power: f32 = 0.0;
        for npc in self.npc_fleets.values() {
            if npc.location != coord { continue; }
            for ship in &npc.ships[0..npc.ship_count as usize] {
                if ship.hull <= 0.0 { continue; }
                power += ship.weapon_power + (ship.hull + ship.shield) / 10.0;
            }
        }
        if power > 0.0 { return power; }
        let cleared = self.sector_overrides.get(&coord.to_key())
            .map(|o| o.npc_cleared_tick.is_some())
            .unwrap_or(false);
        if cleared { return 0.0; }
        if let Some(tmpl) = &self.world_gen.generate_sector(coord).npc_template {
            return tmpl.power();
        }
        0.0
    }

    /// Prospect: scan when the pings have gone stale, then walk to the
    /// nearest sector the player has never entered inside the safe range of
    /// home. With nothing left to chart it goes home and holds.
    fn policy_prospect(&mut self, fleet_id: u64) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return Ok(false); };
        let owner = fleet.owner_id;
        let location = fleet.location;
        let Some(policy) = self.policies.get(&fleet_id) else { return Ok(false); };
        let params = policy.params;
        let Some(player) = self.players.get(&owner) else { return Ok(false); };
        let home = player.homeworld;
        let range = self.policy_effective_range(fleet, &params);

        if let Some(reason) = self.policy_return_reason(fleet, &params)
            && location != home
        {
            let why = format!("returning: {}", reason);
            return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::Prospect, &why));
        }

        // Scan if this position hasn't been pinged recently.
        let scanned_here = self.scan_reveals.get(&owner)
            .map(|m| m.contains_key(&location.to_key()))
            .unwrap_or(false);
        if !scanned_here && self.handle_scan(owner, fleet_id).is_ok() {
            self.push_policy_event(fleet_id, PolicyPreset::Prospect, "scan", "pinging the dark");
            return Ok(true);
        }

        let in_range = move |n: Hex| Hex::distance(&n, &home) <= range as u16;
        let fleet = self.fleets.get(&fleet_id).unwrap();
        let uncharted = |n: Hex| n != home && !self.explored.contains(&(owner, n.to_key()));
        let may_enter = |n: Hex| n == home || (in_range(n) && self.policy_entry_check(fleet, &params, n).is_ok());
        if let Some(step) = self.bfs_first_hop(location, 16, &may_enter, &uncharted) {
            return Ok(self.policy_move(fleet_id, step, false, PolicyPreset::Prospect, "pushing frontier"));
        }

        let why = format!("{}; raise max_range or pick another doctrine", self.prospect_blocker(fleet, &params, range));
        if location != home {
            return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::Prospect, &format!("returning: {why}")));
        }
        Ok(self.hold_policy(fleet_id, PolicyPreset::Prospect, &why))
    }

    /// Which constraint stops prospect: the nearest uncharted sector is
    /// either behind a hazard, beyond the range limit, or there is none.
    fn prospect_blocker(&self, fleet: &Fleet, params: &PolicyParams, range: u8) -> String {
        let owner = fleet.owner_id;
        let Some(home) = self.players.get(&owner).map(|p| p.homeworld) else { return "no homeworld".to_string(); };
        let from = fleet.location;
        let uncharted = |n: Hex| n != home && !self.explored.contains(&(owner, n.to_key()));
        let in_range = |n: Hex| Hex::distance(&n, &home) <= range as u16;

        if let Some(path) = self.bfs_path(from, 16, &in_range, &uncharted) {
            let near = *path.last().unwrap();
            let hazard = path.iter().find_map(|&n| self.policy_entry_check(fleet, params, n).err());
            if let Some(why) = hazard {
                return format!(
                    "no safe way to the nearest uncharted sector {near} {}: {why}",
                    Self::range_phrase(range, params),
                );
            }
        }
        if let Some(path) = self.bfs_path(from, 16, &|_| true, &uncharted) {
            let near = *path.last().unwrap();
            let d = Hex::distance(&near, &home);
            return format!(
                "the nearest uncharted sector {near} is {d} hexes from home, outside {}",
                Self::range_phrase(range, params),
            );
        }
        format!("every lane within 16 hops of {from} is charted {}", Self::range_phrase(range, params))
    }

    /// MineAndReturn: shuttle between the work sector and home. Every claim
    /// is chosen by distance from home, so the fleet never creeps outward.
    fn policy_mine_and_return(&mut self, fleet_id: u64) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return Ok(false); };
        let owner = fleet.owner_id;
        let location = fleet.location;
        let Some(policy) = self.policies.get(&fleet_id) else { return Ok(false); };
        let params = policy.params;
        let mut work = policy.work_sector.unwrap_or(location);
        let Some(player) = self.players.get(&owner) else { return Ok(false); };
        let home = player.homeworld;
        let range = self.policy_effective_range(fleet, &params);

        // No claim yet, or the claim lies outside the leash (orders given
        // far from home): stake the nearest workable seam around home.
        if work == home || Hex::distance(&work, &home) > params.max_range as u16 {
            let Some(new_work) = self.find_nearest_target(home, range, PolicyTarget::Resources) else {
                let why = format!("no ore {}", Self::range_phrase(range, &params));
                if location != home {
                    return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::MineAndReturn, &format!("returning: {why}")));
                }
                return Ok(self.hold_policy(fleet_id, PolicyPreset::MineAndReturn, &why));
            };
            if let Some(p) = self.policies.get_mut(&fleet_id) {
                p.work_sector = Some(new_work);
            }
            self.dirty_policies.insert(fleet_id, ());
            work = new_work;
        }

        // Loaded or low on fuel for the way home: head home (docking
        // deposits + refuels).
        if let Some(reason) = self.policy_return_reason(fleet, &params)
            && location != home
        {
            let why = format!("returning: {}", reason);
            return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::MineAndReturn, &why));
        }

        if location == work && location != home {
            match self.handle_harvest(owner, fleet_id, HarvestResource::Auto) {
                Ok(()) => {
                    self.push_policy_event(fleet_id, PolicyPreset::MineAndReturn, "harvest", "seam holds");
                    return Ok(true);
                }
                Err(ErrorCode::CargoFull) => {
                    return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::MineAndReturn, "returning: hold is full"));
                }
                Err(ErrorCode::NoResources) | Err(ErrorCode::ResourceNotPresent) => {}
                Err(code) => {
                    let why = format!("cannot harvest ({code:?})");
                    return Ok(self.hold_policy(fleet_id, PolicyPreset::MineAndReturn, &why));
                }
            }
            // Seam's dry: next claim, again measured from home.
            let Some(new_work) = self.find_nearest_target(home, range, PolicyTarget::Resources) else {
                let why = format!("claim dry and no other ore {}", Self::range_phrase(range, &params));
                return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::MineAndReturn, &format!("returning: {why}")));
            };
            if let Some(p) = self.policies.get_mut(&fleet_id) {
                p.work_sector = Some(new_work);
            }
            self.dirty_policies.insert(fleet_id, ());
            return Ok(self.policy_step_toward(
                fleet_id, new_work, PolicyPreset::MineAndReturn, "claim dry, moving to new seam",
            ));
        }

        // Anywhere else (including docked at home): head for the claim.
        Ok(self.policy_step_toward(fleet_id, work, PolicyPreset::MineAndReturn, "heading to claim"))
    }

    /// SalvageAndSites: scoop wreckage, board derelicts, avoid fights.
    fn policy_salvage_and_sites(&mut self, fleet_id: u64) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return Ok(false); };
        let owner = fleet.owner_id;
        let location = fleet.location;
        let Some(policy) = self.policies.get(&fleet_id) else { return Ok(false); };
        let params = policy.params;
        let Some(player) = self.players.get(&owner) else { return Ok(false); };
        let home = player.homeworld;
        let range = self.policy_effective_range(fleet, &params);

        if let Some(reason) = self.policy_return_reason(fleet, &params)
            && location != home
        {
            let why = format!("returning: {}", reason);
            return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::SalvageAndSites, &why));
        }

        // Wreckage first: free money.
        let has_salvage = self.sector_overrides.get(&location.to_key())
            .and_then(|o| o.salvage)
            .is_some();
        if has_salvage && self.handle_collect_salvage(owner, fleet_id).is_ok() {
            self.push_policy_event(fleet_id, PolicyPreset::SalvageAndSites, "salvage", "scooping wreckage");
            return Ok(true);
        }

        // Then the hulk, if the odds look survivable.
        if let Some((tier, bumps)) = self.derelict_site_at(location) {
            let fleet = self.fleets.get(&fleet_id).unwrap();
            let chance = self.site_ambush_chance(fleet, location, bumps);
            let guardian_power = scaling::AMBUSH_GUARD_SHARE * scaling::npc_power(location.dist_from_origin());
            let safe = chance < 0.25
                || fleet_power(fleet) * 10.0 >= guardian_power * params.engage_ratio_x10 as f32;
            if safe && self.handle_explore_site(owner, fleet_id).is_ok() {
                let why = format!("boarding tier-{} derelict", tier);
                self.push_policy_event(fleet_id, PolicyPreset::SalvageAndSites, "explore", &why);
                return Ok(true);
            }
        }

        // Otherwise hunt the next payday, searching outward from home.
        if let Some(target) = self.find_nearest_target(home, range, PolicyTarget::SalvageOrSite)
            && target != location
        {
            return Ok(self.policy_step_toward(
                fleet_id, target, PolicyPreset::SalvageAndSites, "contact on the board",
            ));
        }
        let why = format!("board is clear {}", Self::range_phrase(range, &params));
        if location != home {
            return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::SalvageAndSites, &why));
        }
        Ok(self.hold_policy(fleet_id, PolicyPreset::SalvageAndSites, &why))
    }

    /// PatrolHome: keep the porch lights on.
    fn policy_patrol_home(&mut self, fleet_id: u64) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(fleet) = self.fleets.get(&fleet_id) else { return Ok(false); };
        let owner = fleet.owner_id;
        let location = fleet.location;
        let Some(policy) = self.policies.get(&fleet_id) else { return Ok(false); };
        let params = policy.params;
        let Some(player) = self.players.get(&owner) else { return Ok(false); };
        let home = player.homeworld;

        // Wounded → go home and dock.
        let (hull, hull_max) = fleet.ships[0..fleet.ship_count].iter()
            .fold((0.0f32, 0.0f32), |(h, hm), s| (h + s.hull.max(0.0), hm + s.hull_max));
        if hull_max > 0.0 && hull / hull_max < POLICY_PATROL_MIN_HULL && location != home {
            return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::PatrolHome, "licking wounds"));
        }
        if let Some(reason) = self.policy_return_reason(fleet, &params)
            && location != home
        {
            let why = format!("returning: {}", reason);
            return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::PatrolHome, &why));
        }

        let ours = fleet_power(fleet);

        // Engage what we can beat, where we stand.
        if self.sector_has_hostiles(location) {
            let threat = self.hostile_power_at(location);
            if ours * 10.0 >= threat * params.engage_ratio_x10 as f32 {
                let target_id = self.npc_fleets.values()
                    .find(|n| n.location == location && n.ship_count > 0)
                    .map(|n| n.id)
                    .unwrap_or(0);
                if self.handle_attack(owner, fleet_id, target_id).is_ok() {
                    self.push_policy_event(fleet_id, PolicyPreset::PatrolHome, "attack", "engaging hostiles");
                    return Ok(true);
                }
            } else {
                // Outgunned on our own doorstep — call it in.
                return Ok(self.hold_policy(fleet_id, PolicyPreset::PatrolHome, "hostiles too strong"));
            }
        }

        // Hunt hostiles inside the patrol radius.
        let fleet = &self.fleets[&fleet_id].clone();
        if let Some(target) = self.find_nearest_target(home, POLICY_PATROL_RADIUS, PolicyTarget::Hostiles)
            && self.policy_entry_check(fleet, &params, target).is_ok()
        {
            return Ok(self.policy_step_toward(fleet_id, target, PolicyPreset::PatrolHome, "intercepting contact"));
        }

        // Quiet night: wander the beat.
        let mut rng = StdRng::seed_from_u64(
            self.current_tick.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(fleet_id),
        );
        let candidates: Vec<Hex> = self.world_gen.connected_neighbors(location).slice().iter().copied()
            .filter(|n| Hex::distance(n, &home) <= POLICY_PATROL_RADIUS as u16)
            .filter(|&n| n == home || self.policy_entry_check(fleet, &params, n).is_ok())
            .collect();
        if candidates.is_empty() {
            return Ok(self.policy_step_toward(fleet_id, home, PolicyPreset::PatrolHome, "no safe sector on the beat"));
        }
        let target = candidates[rng.random_range(0..candidates.len())];
        if target != location {
            return Ok(self.policy_move(fleet_id, target, false, PolicyPreset::PatrolHome, "walking the beat"));
        }
        Ok(self.hold_policy(fleet_id, PolicyPreset::PatrolHome, "no lane to walk the beat"))
    }

    // ── Helper Methods ────────────────────────────────────────────

    /// True if the sector holds a live NPC fleet or an unspawned,
    /// uncleared NPC template.
    fn sector_has_hostiles(&self, coord: Hex) -> bool {
        if self.npc_fleets.values().any(|n| n.location == coord && n.ship_count > 0) {
            return true;
        }
        let cleared = self.sector_overrides.get(&coord.to_key())
            .map(|o| o.npc_cleared_tick.is_some())
            .unwrap_or(false);
        !cleared && self.world_gen.generate_sector(coord).npc_template.is_some()
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

    fn count_deployed_fleets(&self, player_id: u64, homeworld: Hex) -> usize {
        self.fleets.values().filter(|f| {
            f.owner_id == player_id && f.location != homeworld && f.ship_count > 0
        }).count()
    }

    /// Seeded from the world and the new player's id, so a world replays the
    /// same placements (tests, catch-up) while each world and player differ.
    fn find_homeworld_location(&self, player_id: u64) -> Option<Hex> {
        let mut rng = StdRng::seed_from_u64(
            self.world_gen.world_seed ^ player_id.wrapping_mul(0x9E3779B97F4A7C15)
        );

        let min = HOMEWORLD_MIN_DIST as i32;
        // A crowded home band spills outward one ring at a time; a world below
        // the band's capacity never reaches the extra rings, so its placements
        // are exactly what they were.
        for spill in 0..=HOMEWORLD_SPILL_RINGS {
            let max = (HOMEWORLD_MAX_DIST + spill) as i32;
            for _ in 0..100 {
                let dist = rng.random_range(min..=max) as u16;
                let q = rng.random_range((-(dist as i32))..=(dist as i32)) as i16;
                let r_min = (-(dist as i32)).max(-q as i32 - dist as i32) as i16;
                let r_max = (dist as i32).min(-q as i32 + dist as i32) as i16;
                let r = rng.random_range(r_min..=r_max);

                let candidate = Hex { q, r };
                let d = candidate.dist_from_origin();
                if !(HOMEWORLD_MIN_DIST..=HOMEWORLD_MAX_DIST + spill).contains(&d) { continue; }

                let too_close = self.players.values().any(|p| Hex::distance(&p.homeworld, &candidate) <= 1);
                if !too_close { return Some(candidate); }
            }
        }
        None
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
        self.unload_cargo(fleet_id);
        self.deliver_charts(fleet_id);

        let player = self.players.get(&player_id).ok_or("Player not found")?;
        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        fleet.fuel_max = fleet_fuel_max(fleet, player);
        fleet.fuel = fleet.fuel.min(fleet.fuel_max);
        self.refuel_fleet(fleet_id);

        self.dirty_players.insert(player_id, ());
        self.dirty_fleets.insert(fleet_id, ());

        Ok(())
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
        let cleared = self.sector_overrides.get(&coord.to_key())
            .map(|o| o.npc_cleared_tick.is_some())
            .unwrap_or(false);
        if cleared || self.npc_fleets.contains_key(&template_npc_id(coord)) {
            return None;
        }
        self.world_gen.generate_sector(coord).npc_template
    }

    fn spawn_npc_fleet(&mut self, location: Hex, npc: iac_shared::world::NpcTemplate, npc_fleet_id: u64) -> Result<NpcFleet, Box<dyn std::error::Error>> {
        let mut ships = [Ship::default(); MAX_NPC_SHIPS];
        let stats = npc.ship_class.base_stats();
        let m = npc.stat_multiplier;
        let count = (npc.count as usize).min(MAX_NPC_SHIPS);

        for ship in ships.iter_mut().take(count) {
            *ship = Ship {
                id: self.next_id(),
                ship_class: npc.ship_class,
                hull: stats.hull * m,
                hull_max: stats.hull * m,
                shield: stats.shield * m,
                shield_max: stats.shield * m,
                weapon_power: stats.weapon * m,
                speed: stats.speed,
            };
        }

        let npc_fleet = NpcFleet {
            id: npc_fleet_id,
            location,
            ships,
            ship_count: count as u8,
            behavior: npc.behavior,
            home_sector: location,
            patrol_timer: 0,
            in_combat: false,
            label: npc.label(),
            bounty: npc.ship_class.build_cost().scale(count as f32),
            power: ships[..count].iter().map(|s| s.weapon_power + (s.hull + s.shield) / 10.0).sum(),
        };

        self.npc_fleets.insert(npc_fleet_id, npc_fleet.clone());
        Ok(npc_fleet)
    }

    fn start_combat(&mut self, fleet_id: u64, npc_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let fleet = self.fleets.get(&fleet_id).ok_or("Fleet not found")?;
        if !self.npc_fleets.contains_key(&npc_id) { return Err("NPC not found".into()); }
        let sector = fleet.location;
        let owner = self.players.get(&fleet.owner_id).map(|p| p.name.clone()).unwrap_or_default();

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
                    if let Some(f) = self.fleets.get_mut(&fleet_id) {
                        f.state = FleetStatus::InCombat;
                    }
                }
                if add_npc {
                    if let Some(combat_mut) = self.active_combats.get_mut(&existing_id) {
                        combat_mut.add_npc_fleet(npc_id);
                    }
                    if let Some(n) = self.npc_fleets.get_mut(&npc_id) {
                        n.in_combat = true;
                    }
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
        };
        new_combat.add_player_fleet(fleet_id);
        new_combat.add_npc_fleet(npc_id);
        self.active_combats.insert(combat_id, new_combat);

        if let Some(f) = self.fleets.get_mut(&fleet_id) {
            f.state = FleetStatus::InCombat;
        }
        if let Some(n) = self.npc_fleets.get_mut(&npc_id) {
            n.in_combat = true;
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
            if f.ship_count == 0 { continue; }
            if existing_fleet_ids.contains(&fid) { continue; }
            
            if let Some(combat_mut) = self.active_combats.get_mut(combat_id) {
                combat_mut.add_player_fleet(fid);
            }
            if let Some(fleet_mut) = self.fleets.get_mut(&fid) {
                fleet_mut.state = FleetStatus::InCombat;
            }
        }
        Ok(())
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
        self.dirty_fleets.insert(fleet_id, ());
        self.dirty_sectors.insert(key, ());

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
        self.dirty_sectors.insert(key, ());
        (total, despawn)
    }

    fn record_explored(&mut self, player_id: u64, coord: Hex) -> Result<(), Box<dyn std::error::Error>> {
        self.explored.insert((player_id, coord.to_key()));
        let connections = self.world_gen.connected_neighbors(coord);
        for neighbor in connections.slice() {
            self.new_edges.push(ExploredEdge {
                player_id,
                from: coord,
                to: *neighbor,
                tick: self.current_tick,
            });
        }
        Ok(())
    }

    fn add_ship_to_homeworld(&mut self, player_id: u64, ship_class: ShipClass) -> Result<(), Box<dyn std::error::Error>> {
        let player = self.players.get(&player_id).ok_or("Player not found")?;
        let homeworld = player.homeworld;

        // New hulls reinforce the lowest-numbered idle fleet at home that
        // has no standing orders and room; otherwise they form their own.
        let docked_fleet_id = self.fleets.values()
            .filter(|f| {
                f.owner_id == player_id
                    && f.location == homeworld
                    && f.state == FleetStatus::Idle
                    && f.ship_count < MAX_SHIPS_PER_FLEET
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
                ships: [Ship::default(); MAX_SHIPS_PER_FLEET],
                ship_count: 0,
                cargo: Resources::default(),
                fuel: 0.0,
                fuel_max: 0.0,
                move_cooldown: 0,
                action_cooldown: 0,
                move_target: None,
                idle_ticks: 0,
                harvest_target: HarvestResource::Auto,
                charts: Vec::new(),
            };
            self.fleets.insert(new_id, new_fleet);
            new_id
        };

        let player = self.players.get(&player_id).unwrap();
        let research = player.research.clone();
        let base_stats = ship_class.base_stats();
        let stats = scaling::apply_research_to_stats(base_stats, &research);

        let ship_id = self.next_id();
        let fleet = self.fleets.get_mut(&fleet_id).unwrap();
        if fleet.ship_count >= MAX_SHIPS_PER_FLEET { return Ok(()); }

        fleet.ships[fleet.ship_count] = Ship {
            id: ship_id,
            ship_class,
            hull: stats.hull,
            hull_max: stats.hull,
            shield: stats.shield,
            shield_max: stats.shield,
            weapon_power: stats.weapon,
            speed: stats.speed,
        };
        fleet.ship_count += 1;

        let player = self.players.get(&player_id).unwrap();
        let old_max = fleet.fuel_max;
        fleet.fuel_max = fleet_fuel_max(fleet, player);
        fleet.fuel = (fleet.fuel + (fleet.fuel_max - old_max)).min(fleet.fuel_max);

        self.dirty_fleets.insert(fleet.id, ());
        Ok(())
    }

    fn recalculate_player_fleet_fuel(&mut self, player_id: u64) -> Result<(), Box<dyn std::error::Error>> {
        let player = self.players.get(&player_id).ok_or("Player not found")?;
        let fleet_ids: Vec<u64> = self.fleets.keys().copied().collect();
        for fid in fleet_ids {
            let fleet = self.fleets.get(&fid).unwrap();
            if fleet.owner_id != player.id { continue; }
            if fleet.ship_count == 0 { continue; }

            let new_max = fleet_fuel_max(fleet, player);
            if new_max > fleet.fuel_max {
                let fleet_mut = self.fleets.get_mut(&fid).unwrap();
                let bonus = new_max - fleet_mut.fuel_max;
                fleet_mut.fuel += bonus;
                fleet_mut.fuel_max = new_max;
                self.dirty_fleets.insert(fid, ());
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

    // ── Persistence ───────────────────────────────────────────────

    /// Hand everything dirty since the last call to the attached `Persist`.
    /// Cheap (clones the dirty rows); the write itself happens elsewhere.
    pub fn persist_dirty_state(&mut self) {
        let batch = self.persister.is_some().then(|| PersistBatch {
            tick: self.current_tick,
            next_id: self.next_id,
            players: self.dirty_players.keys().filter_map(|id| self.players.get(id).cloned()).collect(),
            fleets: self.dirty_fleets.keys().filter_map(|id| self.fleets.get(id).cloned()).collect(),
            deleted_fleets: self.deleted_fleet_ids.keys().copied().collect(),
            policies: self.dirty_policies.keys()
                .filter_map(|id| self.policies.get(id).map(|p| (*id, p.clone())))
                .collect(),
            deleted_policies: self.deleted_policy_ids.keys().copied().collect(),
            sectors: self.dirty_sectors.keys()
                .filter_map(|key| self.sector_overrides.get(key).map(|ov| (Hex::from_key(*key), ov.clone())))
                .collect(),
            explored_edges: self.new_edges.clone(),
            known_sectors: self.known.dirty_rows(),
            explore_credits: self.new_credits.clone(),
        });
        self.dirty_players.clear();
        self.dirty_fleets.clear();
        self.dirty_sectors.clear();
        self.deleted_fleet_ids.clear();
        self.dirty_policies.clear();
        self.deleted_policy_ids.clear();
        self.new_edges.clear();
        self.new_credits.clear();
        self.known.clear_dirty();
        if let (Some(persist), Some(batch)) = (&self.persister, batch) {
            persist.submit(batch);
        }
    }

    /// Stamp every live sector for the next persist; call before the final
    /// persist of a clean shutdown so remembered `last_seen` ticks are current.
    pub fn checkpoint_known_sectors(&mut self) {
        self.known.touch_live();
    }

    /// Block until every submitted batch is committed (shutdown, tests).
    pub fn flush_persistence(&self) -> Result<(), String> {
        self.persister.as_ref().map_or(Ok(()), |p| p.flush())
    }
}

/// Orders saved before queue ids existed come back with id 0; give each a
/// fresh id from the world's counter.
fn assign_missing_queue_ids(player: &mut Player, next_id: &mut u64) {
    let mut fresh = |id: &mut u64| {
        if *id == 0 {
            *id = *next_id;
            *next_id += 1;
        }
    };
    player.building_queue.iter_mut().for_each(|q| fresh(&mut q.id));
    player.building_pending.iter_mut().for_each(|q| fresh(&mut q.id));
    player.research_queue.iter_mut().for_each(|q| fresh(&mut q.id));
    player.research_pending.iter_mut().for_each(|q| fresh(&mut q.id));
    player.ship_queue.iter_mut().for_each(|q| fresh(&mut q.id));
    player.ship_pending.iter_mut().for_each(|q| fresh(&mut q.id));
}

// ── Helper Functions ──────────────────────────────────────────────

/// Stable id of the hostile a sector's template spawns; see `TEMPLATE_NPC_ID_BASE`.
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

pub fn template_npc_id(coord: Hex) -> u64 {
    TEMPLATE_NPC_ID_BASE + u64::from(coord.to_key())
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
    let est_power = scaling::npc_power(coord.dist_from_origin());
    ThreatInfo { rating: scaling::threat_rating(est_power), est_power, basis }
}

fn fleet_move_cooldown(fleet: &Fleet, research: Option<&ResearchLevels>) -> u16 {
    let mut min_speed: u8 = 255;
    for ship in &fleet.ships[0..fleet.ship_count] {
        if ship.speed < min_speed { min_speed = ship.speed; }
    }
    if min_speed == 0 { return MOVE_BASE_COOLDOWN; }
    let base = (MOVE_BASE_COOLDOWN as u32 * 10 / min_speed as u32) as u16;
    if let Some(r) = research {
        let reduction = scaling::navigation_cooldown_reduction(r.navigation);
        base.saturating_sub(reduction)
    } else {
        base
    }
}

fn fleet_fuel_cost(fleet: &Fleet, research: Option<&ResearchLevels>) -> f32 {
    let total_mass: f32 = fleet.ships[0..fleet.ship_count].iter().map(|s| s.hull_max).sum();
    let base = total_mass * FUEL_RATE_PER_MASS;
    if let Some(r) = research {
        base * scaling::fuel_rate_modifier(r.fuel_efficiency)
    } else {
        base
    }
}

/// Raw units per tick the fleet can extract at density multiplier 1.
fn fleet_harvest_power(fleet: &Fleet) -> f32 {
    fleet.ships[0..fleet.ship_count].iter()
        .map(|s| match s.ship_class {
            ShipClass::Hauler => 5.0,
            ShipClass::Scout => 1.0,
            _ => 0.5,
        })
        .sum()
}

fn fleet_fuel_max(fleet: &Fleet, player: &Player) -> f32 {
    let total_fuel: f32 = fleet.ships[0..fleet.ship_count].iter()
        .map(|s| s.ship_class.base_stats().fuel as f32)
        .sum();
    total_fuel
        * scaling::fuel_capacity_modifier(player.research.extended_fuel_tanks)
        * scaling::fuel_depot_modifier(player.buildings.fuel_depot)
}

/// Live combat power of a whole fleet.
pub fn fleet_power(fleet: &Fleet) -> f32 {
    fleet.ships[0..fleet.ship_count].iter()
        .filter(|s| s.hull > 0.0)
        .map(|s| s.weapon_power + (s.hull + s.shield) / 10.0)
        .sum()
}

/// What jumps a careless boarding party: a share of the ring's group.
fn ambush_guardian_template(site: Hex, rng: &mut StdRng) -> iac_shared::world::NpcTemplate {
    let (ship_class, count, stat_multiplier) = scaling::ambush_guard(site.dist_from_origin());
    let swing = rng.random_range(0.8..=1.2);
    iac_shared::world::NpcTemplate {
        ship_class,
        count: ((f32::from(count) * swing).round() as usize).clamp(1, MAX_NPC_SHIPS) as u8,
        behavior: iac_shared::world::NpcBehaviorType::Aggressive,
        stat_multiplier,
    }
}

/// What can be coaxed back to life in a wreck of this tier.
fn recoverable_ship_class(tier: u8, rng: &mut StdRng) -> ShipClass {
    let pool: &[ShipClass] = match tier {
        1 => &[ShipClass::Scout, ShipClass::Corvette],
        2 => &[ShipClass::Scout, ShipClass::Corvette, ShipClass::Hauler],
        _ => &[ShipClass::Corvette, ShipClass::Hauler, ShipClass::Frigate, ShipClass::Cruiser],
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
pub fn fleet_cargo_capacity(fleet: &Fleet, pace: &Pace) -> f32 {
    let base: f32 = fleet.ships[0..fleet.ship_count].iter()
        .map(|s| s.ship_class.base_stats().cargo as f32)
        .sum();
    base * pace.cargo_mult()
}

fn regen_resource(
    harvested: &mut f32,
    override_density: &mut Option<Density>,
    template_density: Density,
    dist: u16,
    pace: &Pace,
) -> bool {
    let current = override_density.unwrap_or(template_density);
    let rate = scaling::ore_regen_per_tick(template_density, dist, pace);
    let (density, gone) = scaling::ore_regen_step(template_density, current, *harvested, rate, dist);
    *override_density = (density != template_density).then_some(density);
    *harvested = gone;
    density != current
}
