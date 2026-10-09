// Game engine: tick loop, movement, combat, harvesting, NPC behavior, homeworlds, build queues.
// Ported from Zig engine.zig.

use std::collections::{HashMap, HashSet};

use log::{info, warn};
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
use crate::database::{Database, ExploreCredit, ExploredEdge, PersistBatch, Persister, WorldMeta};
use crate::intel::KnownSectors;

// ── Constants ─────────────────────────────────────────────────────

pub const MAX_SHIPS_PER_FLEET: usize = 64;
pub const MAX_NPC_SHIPS: usize = scaling::NPC_MAX_SHIPS;
pub const MAX_COMBAT_FLEETS: usize = 8;
/// Part-refilled tiles are written to disk this often (ticks).
const REGEN_PERSIST_TICKS: u64 = 60;

// ── Core Data Types ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FleetStatus {
    Idle,
    Moving,
    Harvesting,
    InCombat,
    Returning,
    Docked,
    Exploring,
}

#[derive(Debug, Clone, Copy, Default)]
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

#[derive(Debug, Clone)]
pub struct Fleet {
    pub id: u64,
    pub owner_id: u64,
    pub location: Hex,
    pub state: FleetStatus,
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
#[derive(Debug, Clone)]
pub struct NpcFleet {
    pub id: u64,
    pub location: Hex,
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

#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct PendingBuilding {
    pub id: u64,
    pub building_type: BuildingType,
    pub target_level: u8,
    pub reserve: bool,
}

#[derive(Debug, Clone)]
pub struct PendingResearch {
    pub id: u64,
    pub tech: ResearchType,
    pub target_level: u8,
    pub reserve: bool,
}

/// A shipyard order: a batch that pays per unit. `built` units are done; a
/// batch between units sits here again until the next unit can be paid.
#[derive(Debug, Clone)]
pub struct PendingShip {
    pub id: u64,
    pub item: ShipyardItem,
    pub count: u16,
    pub built: u16,
    pub reserve: bool,
}

/// Defence structures standing at home, and those a repelled raid knocked
/// out that come back free at `restore_tick`.
#[derive(Debug, Clone, Default, PartialEq)]
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

#[derive(Debug, Clone)]
pub struct BuildQueueEntry {
    pub id: u64,
    pub building_type: BuildingType,
    pub target_level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct ResearchQueueEntry {
    pub id: u64,
    pub tech: ResearchType,
    pub target_level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Default)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Default)]
struct HarvestReport {
    resources: Resources,
    ticks: u16,
    /// Tick the first unreported harvest happened.
    since: u64,
}

/// A fleet's standing orders plus the bit of memory the autopilot needs.
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Copy)]
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
    persister: Persister,
    pub current_tick: u64,

    pub players: HashMap<u64, Player>,
    pub fleets: HashMap<u64, Fleet>,
    pub npc_fleets: HashMap<u64, NpcFleet>,
    pub active_combats: HashMap<u64, Combat>,
    pub sector_overrides: HashMap<u32, SectorOverride>,

    /// Actively-scanned sectors per player: hex key → expiry tick.
    scan_reveals: HashMap<u64, HashMap<u32, u64>>,
    raid_states: HashMap<u64, RaidState>,
    /// Per player and resource: 0 below 80 percent of the cap, 1 once
    /// `StorageNearCap` fired, 2 once `StorageFull` fired.
    storage_marks: HashMap<u64, [u8; 3]>,
    /// Per player: the despawn tick of the home pile already warned about.
    home_salvage_warned: HashMap<u64, u64>,
    /// Harvest yield per fleet awaiting its next `ResourceHarvested` event.
    harvest_reports: HashMap<u64, HarvestReport>,
    /// Standing orders per fleet.
    pub policies: HashMap<u64, FleetPolicy>,

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
    /// `pace` is the operator's request; a database that already holds a
    /// world keeps its own pace and refuses a different one.
    pub fn init(world_seed: u64, db: Database, pace: Option<Pace>) -> Result<GameEngine, Box<dyn std::error::Error>> {
        let meta = db.open_world(pace)?;
        let world = load_world(&db, world_seed)?;
        db.save_server_state("world_seed", &world_seed.to_string())?;

        Ok(GameEngine {
            world_gen: WorldGen::init(world_seed),
            world: meta,
            persister: Persister::spawn(db),
            current_tick: world.tick,
            players: world.players,
            fleets: world.fleets,
            npc_fleets: HashMap::new(),
            active_combats: HashMap::new(),
            sector_overrides: world.sector_overrides,
            scan_reveals: HashMap::new(),
            raid_states: HashMap::new(),
            storage_marks: HashMap::new(),
            home_salvage_warned: HashMap::new(),
            harvest_reports: HashMap::new(),
            policies: world.policies,
            pending_events: Vec::new(),
            pending_arrivals: Vec::new(),
            next_id: world.next_id,
            dirty_players: HashMap::new(),
            dirty_fleets: HashMap::new(),
            dirty_sectors: HashMap::new(),
            deleted_fleet_ids: HashMap::new(),
            dirty_policies: HashMap::new(),
            deleted_policy_ids: HashMap::new(),
            explored: world.explored,
            new_edges: Vec::new(),
            credited: world.credited,
            new_credits: Vec::new(),
            known: KnownSectors::load(world.known_sectors),
        })
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
    pub fn authenticate(&mut self, name: &str, presented: Option<&str>, agent: bool) -> Result<Authenticated, AuthError> {
        let Some(player_id) = self.find_player_by_name(name) else {
            auth::validate_name(name).map_err(AuthError::InvalidName)?;
            let player_id = self.register_player(name.to_string()).map_err(|_| AuthError::Server)?;
            if let Some(p) = self.players.get_mut(&player_id) {
                p.agent = agent;
            }
            let token = self.issue_token(player_id);
            return Ok(Authenticated { player_id, new_token: Some(token), registered: true });
        };

        match self.players[&player_id].token_hash {
            None => {
                info!("Player '{}' (id={}) claimed a token", name, player_id);
                let token = self.issue_token(player_id);
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

    fn issue_token(&mut self, player_id: u64) -> String {
        let token = auth::generate_token();
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
        let max = HOMEWORLD_MAX_DIST as i32;

        for _ in 0..100 {
            let dist = rng.random_range(min..=max) as u16;
            let q = rng.random_range((-(dist as i32))..=(dist as i32)) as i16;
            let r_min = (-(dist as i32)).max(-q as i32 - dist as i32) as i16;
            let r_max = (dist as i32).min(-q as i32 + dist as i32) as i16;
            let r = rng.random_range(r_min..=r_max);

            let candidate = Hex { q, r };
            let d = candidate.dist_from_origin();
            if !(HOMEWORLD_MIN_DIST..=HOMEWORLD_MAX_DIST).contains(&d) { continue; }

            let too_close = self.players.values().any(|p| Hex::distance(&p.homeworld, &candidate) <= 1);
            if !too_close { return Some(candidate); }
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
        if max_hops == 0 { return Vec::new(); }

        let mut visited: HashMap<u32, ()> = HashMap::new();
        let mut current_frontier: Vec<Hex> = Vec::new();
        let mut result: Vec<Hex> = Vec::new();

        visited.insert(origin.to_key(), ());
        current_frontier.push(origin);

        for _ in 0..max_hops {
            let mut next_frontier: Vec<Hex> = Vec::new();
            for coord in &current_frontier {
                let neighbors = self.world_gen.connected_neighbors(*coord);
                for n in neighbors.slice() {
                    let key = n.to_key();
                    if visited.contains_key(&key) { continue; }
                    visited.insert(key, ());
                    next_frontier.push(*n);
                    result.push(*n);
                }
            }
            current_frontier = next_frontier;
        }

        result
    }

    // ── Events ────────────────────────────────────────────────────

    pub fn drain_events(&mut self) -> Vec<GameEvent> {
        std::mem::take(&mut self.pending_events)
    }

    // ── Persistence ───────────────────────────────────────────────

    /// Hand everything dirty since the last call to the persist thread.
    /// Cheap (clones the dirty rows); the SQLite write happens off-thread.
    pub fn persist_dirty_state(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let batch = PersistBatch {
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
            explored_edges: std::mem::take(&mut self.new_edges),
            known_sectors: self.known.take_dirty(),
            explore_credits: std::mem::take(&mut self.new_credits),
        };
        self.dirty_players.clear();
        self.dirty_fleets.clear();
        self.dirty_sectors.clear();
        self.deleted_fleet_ids.clear();
        self.dirty_policies.clear();
        self.deleted_policy_ids.clear();
        self.persister.submit(batch);
        Ok(())
    }

    /// Stamp every live sector for the next persist; call before the final
    /// persist of a clean shutdown so remembered `last_seen` ticks are current.
    pub fn checkpoint_known_sectors(&mut self) {
        self.known.touch_live();
    }

    /// Block until every submitted batch is committed (shutdown, tests).
    pub fn flush_persistence(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.persister.flush().map_err(|e| e.into())
    }
}

struct LoadedWorld {
    tick: u64,
    next_id: u64,
    players: HashMap<u64, Player>,
    fleets: HashMap<u64, Fleet>,
    sector_overrides: HashMap<u32, SectorOverride>,
    policies: HashMap<u64, FleetPolicy>,
    explored: HashSet<(u64, u32)>,
    credited: HashSet<(u64, u32, ExploreCredit)>,
    known_sectors: Vec<crate::database::KnownRow>,
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

fn load_world(db: &Database, world_seed: u64) -> Result<LoadedWorld, Box<dyn std::error::Error>> {
    let tick = db.load_server_state("current_tick")?.and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut next_id: u64 = db.load_server_state("next_id")?.and_then(|s| s.parse().ok()).unwrap_or(1);
    if let Some(seed_str) = db.load_server_state("world_seed")? {
        let stored_seed: u64 = seed_str.parse().unwrap_or(0);
        if stored_seed != world_seed {
            warn!("World seed mismatch: DB has {}, config has {}", stored_seed, world_seed);
        }
    }

    let mut players = HashMap::new();
    for mut player in db.load_players()? {
        player.buildings = db.load_buildings(player.id)?;
        player.research = db.load_research(player.id)?;
        let queues = db.load_build_queues(player.id)?;
        player.building_queue = queues.building;
        player.building_pending = queues.building_pending;
        player.ship_queue = queues.ship;
        player.ship_pending = queues.ship_pending;
        player.research_queue = queues.research;
        player.research_pending = queues.research_pending;
        assign_missing_queue_ids(&mut player, &mut next_id);
        player.defences = db.load_defences(player.id)?;
        players.insert(player.id, player);
    }

    let mut fleets: HashMap<u64, Fleet> = db.load_fleets()?.into_iter().map(|f| (f.id, f)).collect();
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

    let sector_overrides = db.load_sector_overrides()?
        .into_iter()
        .map(|row| (Hex { q: row.q, r: row.r }.to_key(), row.override_data))
        .collect();

    let policies = db.load_fleet_policies()?
        .into_iter()
        .filter(|(fleet_id, _)| fleets.contains_key(fleet_id))
        .collect();

    let explored = db.load_explored_sectors()?
        .into_iter()
        .map(|(pid, hex)| (pid, hex.to_key()))
        .collect();

    if players.is_empty() {
        info!("State loaded (empty -- fresh world)");
    } else {
        info!("State loaded: {} players, {} fleets, tick {}", players.len(), fleets.len(), tick);
    }

    let credited = db.load_explore_credits()?
        .into_iter()
        .map(|(pid, hex, kind)| (pid, hex.to_key(), kind))
        .collect();

    let known_sectors = db.load_known_sectors()?;

    Ok(LoadedWorld { tick, next_id, players, fleets, sector_overrides, policies, explored, credited, known_sectors })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine() -> GameEngine {
        let db = Database::init(":memory:").expect("in-memory db");
        GameEngine::init(42, db, None).expect("engine init")
    }

    fn register(engine: &mut GameEngine, name: &str) -> (u64, u64) {
        let pid = engine.register_player(name.to_string()).expect("register");
        let fid = engine.fleets.values()
            .find(|f| f.owner_id == pid)
            .map(|f| f.id)
            .expect("starting fleet");
        (pid, fid)
    }

    #[test]
    fn scan_reveals_sectors_and_sets_cooldown() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Scanner");

        engine.handle_scan(pid, fid).expect("scan should succeed");

        let revealed = engine.scan_revealed_coords(pid);
        assert!(!revealed.is_empty(), "scan revealed no sectors");
        assert_eq!(engine.fleets[&fid].action_cooldown, SCAN_COOLDOWN);

        let events = engine.drain_events();
        let scan_event = events.iter().find(|e| matches!(e.kind, EventKind::ScanCompleted(_)));
        assert!(scan_event.is_some(), "no ScanCompleted event");

        // Second scan is blocked by the cooldown.
        assert_eq!(engine.handle_scan(pid, fid), Err(ErrorCode::OnCooldown));
    }

    #[test]
    fn scan_reveals_expire() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Scanner");
        engine.handle_scan(pid, fid).unwrap();
        assert!(!engine.scan_revealed_coords(pid).is_empty());

        engine.current_tick += SCAN_REVEAL_TICKS + 1;
        engine.prune_scan_reveals();
        assert!(engine.scan_revealed_coords(pid).is_empty(), "reveals should expire");
    }

    #[test]
    fn commands_reject_foreign_fleets() {
        let mut engine = test_engine();
        let (_pid_a, fid_a) = register(&mut engine, "Alice");
        let (pid_b, _fid_b) = register(&mut engine, "Bob");

        assert_eq!(engine.handle_scan(pid_b, fid_a), Err(ErrorCode::FleetNotFound));
        assert_eq!(engine.handle_stop(pid_b, fid_a), Err(ErrorCode::FleetNotFound));
        assert_eq!(engine.handle_recall(pid_b, fid_a), Err(ErrorCode::FleetNotFound));
        assert_eq!(engine.handle_harvest(pid_b, fid_a, HarvestResource::Auto), Err(ErrorCode::FleetNotFound));
    }

    /// A sector satisfying `pred` on its (metal, crystal, deuterium) densities.
    fn find_sector_with(
        engine: &GameEngine,
        pred: impl Fn(Density, Density, Density) -> bool,
    ) -> Hex {
        find_sector_beyond(engine, 0, pred)
    }

    /// Like `find_sector_with`, at least `min_dist` rings from the hub.
    fn find_sector_beyond(
        engine: &GameEngine,
        min_dist: u16,
        pred: impl Fn(Density, Density, Density) -> bool,
    ) -> Hex {
        for q in -30i16..30 {
            for r in -30i16..30 {
                let coord = Hex { q, r };
                let t = engine.world_gen.generate_sector(coord);
                if coord.dist_from_origin() >= min_dist && pred(t.metal_density, t.crystal_density, t.deut_density) {
                    return coord;
                }
            }
        }
        panic!("no matching sector in a 60x60 window");
    }

    #[test]
    fn harvest_mines_only_the_chosen_resource() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Miner");
        let site = find_sector_with(&engine, |m, c, _| m != Density::None && c != Density::None);
        engine.fleets.get_mut(&fid).unwrap().location = site;

        engine.handle_harvest(pid, fid, HarvestResource::Crystal).expect("harvest crystal");
        engine.tick().expect("tick");
        let cargo = engine.fleets[&fid].cargo;
        assert!(cargo.crystal > 0.0, "crystal not mined");
        assert_eq!(cargo.metal, 0.0, "metal mined despite crystal order");
        assert_eq!(cargo.deuterium, 0.0);
    }

    #[test]
    fn harvest_events_are_aggregated_per_fleet() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Sweeper");
        let site = find_sector_beyond(&engine, 20, |m, _, _| m == Density::Moderate);
        engine.fleets.get_mut(&fid).unwrap().location = site;
        engine.handle_harvest(pid, fid, HarvestResource::Metal).expect("harvest");

        let mut reports = Vec::new();
        for _ in 0..(2 * HARVEST_REPORT_TICKS) {
            engine.tick().expect("tick");
            for e in engine.drain_events() {
                if let EventKind::ResourceHarvested(h) = e.kind {
                    reports.push(h);
                }
            }
        }
        // Stopping flushes the unreported tail immediately.
        engine.handle_stop(pid, fid).expect("stop");
        engine.tick().expect("tick");
        for e in engine.drain_events() {
            if let EventKind::ResourceHarvested(h) = e.kind {
                reports.push(h);
            }
        }
        let mined = engine.fleets[&fid].cargo;
        assert!(reports.len() <= 3, "one event per fleet per report interval, got {}", reports.len());
        let total = reports.iter().fold(Resources::default(), |acc, h| acc.add(h.resources));
        assert!((total.metal - mined.metal).abs() < 0.01, "reported {total:?} but cargo holds {mined:?}");
        assert!(reports.iter().all(|h| h.fleet_id == fid && h.ticks > 0));
    }

    #[test]
    fn auto_harvest_takes_everything_present() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Sweeper");
        let site = find_sector_with(&engine, |m, c, _| m != Density::None && c != Density::None);
        engine.fleets.get_mut(&fid).unwrap().location = site;

        engine.handle_harvest(pid, fid, HarvestResource::Auto).expect("harvest auto");
        engine.tick().expect("tick");
        let cargo = engine.fleets[&fid].cargo;
        assert!(cargo.metal > 0.0 && cargo.crystal > 0.0, "auto should mine metal and crystal: {cargo:?}");
    }

    #[test]
    fn harvest_rejects_a_resource_the_sector_lacks() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Picky");
        let site = find_sector_with(&engine, |m, _, d| m != Density::None && d == Density::None);
        engine.fleets.get_mut(&fid).unwrap().location = site;

        assert_eq!(
            engine.handle_harvest(pid, fid, HarvestResource::Deuterium),
            Err(ErrorCode::ResourceNotPresent)
        );
        assert_eq!(engine.fleets[&fid].state, FleetStatus::Idle, "rejected order must not start mining");
        assert!(engine.handle_harvest(pid, fid, HarvestResource::Metal).is_ok());
    }

    #[test]
    fn harvest_choice_survives_restart() {
        let path = std::env::temp_dir().join(format!(
            "iac_harvest_restart_test_{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let fid = {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            let mut engine = GameEngine::init(42, db, None).unwrap();
            let (pid, fid) = register(&mut engine, "Persistent");
            let site = find_sector_with(&engine, |m, c, _| m != Density::None && c != Density::None);
            engine.fleets.get_mut(&fid).unwrap().location = site;
            engine.handle_harvest(pid, fid, HarvestResource::Crystal).unwrap();
            engine.persist_dirty_state().unwrap();
            fid
        };

        let db = Database::init(path.to_str().unwrap()).unwrap();
        let engine = GameEngine::init(42, db, None).expect("reload");
        let fleet = &engine.fleets[&fid];
        assert_eq!(fleet.state, FleetStatus::Harvesting);
        assert_eq!(fleet.harvest_target, HarvestResource::Crystal);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn first_visit_memory_survives_restart_without_db_reads() {
        let path = std::env::temp_dir().join(format!(
            "iac_explored_restart_test_{}.db",
            std::process::id()
        ));
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }

        let (pid, visited) = {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            let mut engine = GameEngine::init(42, db, None).unwrap();
            let (pid, fid) = register(&mut engine, "Wanderer");
            let home = engine.fleets[&fid].location;
            let visited = engine.world_gen.connected_neighbors(home).slice()[0];
            assert!(!engine.explored.contains(&(pid, visited.to_key())));
            engine.record_explored(pid, visited).unwrap();
            assert!(engine.explored.contains(&(pid, visited.to_key())), "visible at once, before any write");
            engine.persist_dirty_state().unwrap();
            (pid, visited)
        };

        let db = Database::init(path.to_str().unwrap()).unwrap();
        let engine = GameEngine::init(42, db, None).unwrap();
        assert!(engine.explored.contains(&(pid, visited.to_key())), "explored sectors reload from explored_edges");

        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }

    #[test]
    fn authenticate_registers_claims_and_rejects() {
        let mut engine = test_engine();

        // New name: account created, token issued once.
        let fresh = engine.authenticate("Fresh", None, false).unwrap();
        assert!(fresh.registered);
        let token = fresh.new_token.expect("token issued on registration");
        assert_eq!(engine.authenticate("Fresh", None, false).err(), Some(AuthError::TokenRequired));
        assert_eq!(engine.authenticate("Fresh", Some("nope"), false).err(), Some(AuthError::InvalidToken));
        let back = engine.authenticate("Fresh", Some(&token), false).unwrap();
        assert_eq!(back.player_id, fresh.player_id);
        assert!(back.new_token.is_none());
        assert!(!back.registered);

        // Only the hash is kept.
        let stored = engine.players[&fresh.player_id].token_hash.unwrap();
        assert_eq!(stored, auth::hash_token(&token));
        assert!(!stored.iter().eq(token.as_bytes().iter().take(32)));

        // An account without a token (pre-auth database) is claimed by the
        // next login, whatever it presents, and is protected afterwards.
        let (legacy_id, _) = register(&mut engine, "Legacy");
        assert!(engine.players[&legacy_id].token_hash.is_none());
        let claimed = engine.authenticate("Legacy", Some("whatever"), false).unwrap();
        assert_eq!(claimed.player_id, legacy_id);
        let legacy_token = claimed.new_token.expect("claim issues a token");
        assert!(engine.dirty_players.contains_key(&legacy_id), "claim must be persisted");
        assert_eq!(engine.authenticate("Legacy", None, false).err(), Some(AuthError::TokenRequired));
        assert!(engine.authenticate("Legacy", Some(&legacy_token), false).is_ok());

        // Bad names never register.
        assert!(matches!(engine.authenticate("x", None, false), Err(AuthError::InvalidName(_))));
        assert_eq!(engine.players.values().filter(|p| p.name == "x").count(), 0);
    }

    #[test]
    fn names_are_case_insensitive_accounts() {
        let mut engine = test_engine();
        let admiral = engine.authenticate("Admiral", None, false).unwrap();
        let token = admiral.new_token.unwrap();

        // Another casing is the same account, not a lookalike registration.
        assert_eq!(engine.authenticate("ADMIRAL", None, false).err(), Some(AuthError::TokenRequired));
        assert_eq!(engine.authenticate("admiral", Some("nope"), false).err(), Some(AuthError::InvalidToken));
        let back = engine.authenticate("admiral", Some(&token), false).unwrap();
        assert_eq!(back.player_id, admiral.player_id);
        assert!(!back.registered);
        assert!(engine.register_player("aDmIrAl".to_string()).is_err());
        assert_eq!(engine.players.values().filter(|p| p.name.eq_ignore_ascii_case("admiral")).count(), 1);

        // Case-variant accounts created before the rule stay reachable by
        // their exact name.
        let mut twin = engine.players[&admiral.player_id].clone();
        twin.id = engine.next_id();
        twin.name = "admiral".to_string();
        twin.token_hash = None;
        let twin_id = twin.id;
        engine.players.insert(twin_id, twin);
        assert_eq!(engine.authenticate("admiral", None, false).unwrap().player_id, twin_id);
        assert_eq!(engine.authenticate("Admiral", Some(&token), false).unwrap().player_id, admiral.player_id);
    }

    #[test]
    fn stop_cancels_movement() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Stopper");

        // Idle fleet has nothing to stop.
        assert_eq!(engine.handle_stop(pid, fid), Err(ErrorCode::InvalidCommand));

        let home = engine.fleets[&fid].location;
        let target = engine.world_gen.connected_neighbors(home).slice()[0];
        engine.handle_move(pid, fid, target).expect("move");
        assert_eq!(engine.fleets[&fid].state, FleetStatus::Moving);

        engine.handle_stop(pid, fid).expect("stop");
        let fleet = &engine.fleets[&fid];
        assert_eq!(fleet.state, FleetStatus::Idle);
        assert_eq!(fleet.move_target, None);
        // The fleet never left.
        assert_eq!(fleet.location, home);
    }

    #[test]
    fn defense_grid_adds_power() {
        let mut engine = test_engine();
        let (pid, _fid) = register(&mut engine, "Defender");

        let base = engine.home_defense_power(pid);
        engine.players.get_mut(&pid).unwrap().buildings.defense_grid = 3;
        let with_grid = engine.home_defense_power(pid);
        assert!(with_grid > base, "grid should add power: {base} -> {with_grid}");

        let expected_bonus = defense_grid_scout_units(3) * scaling::ship_power(ShipClass::Scout);
        assert!((with_grid - base - expected_bonus).abs() < 0.01);
    }

    #[test]
    fn repelled_raid_drops_salvage_and_spares_resources() {
        let mut engine = test_engine();
        let (pid, _fid) = register(&mut engine, "Winner");
        let before = engine.players[&pid].resources;

        // Tiny raid vs the starting defense (2 scouts): guaranteed repel
        // even with the ±10% variance.
        engine.resolve_raid(pid, 0.1).expect("resolve");

        let after = engine.players[&pid].resources;
        assert_eq!(before.metal, after.metal, "repelled raid must not cost resources");

        let home_key = engine.players[&pid].homeworld.to_key();
        let salvage = engine.sector_overrides.get(&home_key).and_then(|o| o.salvage);
        assert!(salvage.is_some(), "repelled raid should drop salvage");

        let events = engine.drain_events();
        match events.iter().find_map(|e| match &e.kind {
            EventKind::RaidResolved(r) => Some(r),
            _ => None,
        }) {
            Some(r) => assert!(r.defended),
            None => panic!("no RaidResolved event"),
        }
    }

    #[test]
    fn state_survives_server_restart() {
        // Regression: REAL-affinity columns were read back as i64, which
        // made every restart with saved players panic with
        // InvalidColumnType.
        let path = std::env::temp_dir().join(format!(
            "iac_restart_test_{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let (pid, fid, resources) = {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            let mut engine = GameEngine::init(42, db, None).unwrap();
            let (pid, fid) = register(&mut engine, "Restarter");
            engine.persist_dirty_state().unwrap();
            (pid, fid, engine.players[&pid].resources)
        };

        let db = Database::init(path.to_str().unwrap()).unwrap();
        let engine = GameEngine::init(42, db, None).expect("engine must reload saved state");
        let player = engine.players.get(&pid).expect("player survived restart");
        assert!((player.resources.metal - resources.metal).abs() < 0.01);
        let fleet = engine.fleets.get(&fid).expect("fleet survived restart");
        assert_eq!(fleet.ship_count, STARTING_SCOUTS);
        assert!(fleet.fuel > 0.0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn lost_raid_caps_losses_and_suppresses_raids() {
        let mut engine = test_engine();
        let (pid, _fid) = register(&mut engine, "Loser");
        let before = engine.players[&pid].resources;

        // Overwhelming raid: guaranteed loss.
        engine.resolve_raid(pid, 1_000_000.0).expect("resolve");

        let after = engine.players[&pid].resources;
        let metal_lost = before.metal - after.metal;
        assert!(metal_lost > 0.0, "lost raid should cost resources");
        assert!(
            metal_lost <= before.metal * RAID_LOSS_CAP_METAL + 0.01,
            "metal loss over cap: {metal_lost}"
        );
        assert!(
            before.deuterium - after.deuterium <= before.deuterium * RAID_LOSS_CAP_DEUT + 0.01
        );

        let state = engine.raid_states.get(&pid).expect("raid state");
        assert!(
            state.suppress_until > engine.current_tick,
            "losing a raid must suppress the next one"
        );
    }

    /// Find a sector with an unlooted derelict, scanning outward.
    fn find_derelict_sector(engine: &GameEngine) -> Hex {
        for q in -30i16..30 {
            for r in -30i16..30 {
                let coord = Hex { q, r };
                if engine.derelict_site_at(coord).is_some() {
                    return coord;
                }
            }
        }
        panic!("no derelict in a 60x60 window — spawn rate broken?");
    }

    #[test]
    fn derelicts_spawn_beyond_min_dist_only() {
        let engine = test_engine();
        let mut found = 0;
        for q in -30i16..30 {
            for r in -30i16..30 {
                let coord = Hex { q, r };
                let t = engine.world_gen.generate_sector(coord);
                if t.has_derelict {
                    found += 1;
                    assert!(
                        coord.dist_from_origin() >= iac_shared::constants::DERELICT_MIN_DIST,
                        "derelict too close to hub at {coord}"
                    );
                    assert!(
                        matches!(t.terrain, iac_shared::constants::TerrainType::DebrisField
                            | iac_shared::constants::TerrainType::Empty),
                        "derelict in wrong terrain at {coord}"
                    );
                }
            }
        }
        assert!(found > 0, "no derelicts anywhere");
    }

    #[test]
    fn explore_site_boards_and_resolves() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Boarder");

        let site = find_derelict_sector(&engine);
        engine.fleets.get_mut(&fid).unwrap().location = site;

        // No derelict → InvalidTarget (the hub never has one).
        engine.fleets.get_mut(&fid).unwrap().location = Hex::ORIGIN;
        assert_eq!(engine.handle_explore_site(pid, fid), Err(ErrorCode::InvalidTarget));
        engine.fleets.get_mut(&fid).unwrap().location = site;

        engine.handle_explore_site(pid, fid).expect("board");
        assert_eq!(engine.fleets[&fid].state, FleetStatus::Exploring);
        assert_eq!(engine.fleets[&fid].action_cooldown, EXPLORE_DURATION_TICKS);
        let events = engine.drain_events();
        assert!(events.iter().any(|e| matches!(e.kind, EventKind::SiteExplorationStarted(_))));

        // Double-board is rejected while working.
        assert_eq!(engine.handle_explore_site(pid, fid), Err(ErrorCode::OnCooldown));

        // Run the clock out.
        for _ in 0..=EXPLORE_DURATION_TICKS {
            engine.tick().expect("tick");
        }

        let events = engine.drain_events();
        let looted = events.iter().any(|e| matches!(e.kind, EventKind::SiteExplored(_)));
        let ambushed = events.iter().any(|e| matches!(e.kind, EventKind::SiteAmbush(_)));
        assert!(looted || ambushed, "boarding must resolve one way or the other");

        let ov = engine.sector_overrides.get(&site.to_key());
        if looted {
            let fleet = &engine.fleets[&fid];
            let cargo = fleet.cargo.metal + fleet.cargo.crystal + fleet.cargo.deuterium;
            assert!(cargo > 0.0, "loot must land in cargo");
            assert!(ov.and_then(|o| o.site_looted_tick).is_some(), "site must be consumed");
            assert!(engine.derelict_site_at(site).is_none());
        } else {
            assert_eq!(engine.fleets[&fid].state, FleetStatus::InCombat);
            assert!(ov.map(|o| o.site_ambush_bumps >= 1).unwrap_or(false));
            assert!(engine.derelict_site_at(site).is_some(), "ambush leaves the site");
        }
    }

    #[test]
    fn aborting_a_boarding_angers_the_site() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Coward");
        let site = find_derelict_sector(&engine);
        engine.fleets.get_mut(&fid).unwrap().location = site;

        engine.handle_explore_site(pid, fid).expect("board");
        engine.handle_stop(pid, fid).expect("abort");

        let fleet = &engine.fleets[&fid];
        assert_eq!(fleet.state, FleetStatus::Idle);
        assert_eq!(fleet.action_cooldown, 0, "abort must release the boarding timer");
        let bumps = engine.sector_overrides.get(&site.to_key()).map(|o| o.site_ambush_bumps);
        assert_eq!(bumps, Some(1));
    }

    #[test]
    fn policy_set_and_clear() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Boss");
        let (pid_b, _) = register(&mut engine, "Rival");

        // Foreign fleets can't be given orders.
        assert_eq!(
            engine.handle_policy_update(pid_b, fid, PolicyPreset::Prospect, None),
            Err(ErrorCode::FleetNotFound)
        );

        engine.handle_policy_update(pid, fid, PolicyPreset::MineAndReturn, None).expect("set");
        let policy = engine.policies.get(&fid).expect("policy stored");
        assert_eq!(policy.preset, PolicyPreset::MineAndReturn);
        assert_eq!(policy.work_sector, Some(engine.fleets[&fid].location));

        engine.handle_policy_update(pid, fid, PolicyPreset::Manual, None).expect("clear");
        assert!(!engine.policies.contains_key(&fid), "manual clears orders");

        let events = engine.drain_events();
        let policy_events: Vec<_> = events.iter()
            .filter(|e| matches!(e.kind, EventKind::PolicyAction(_)))
            .collect();
        assert_eq!(policy_events.len(), 2, "engage + clear events");
    }

    #[test]
    fn mine_policy_heads_home_when_loaded() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Miner");
        let home = engine.players[&pid].homeworld;

        // Park the fleet one hop out with a stuffed hold.
        let away = engine.world_gen.connected_neighbors(home).slice()[0];
        {
            let fleet = engine.fleets.get_mut(&fid).unwrap();
            fleet.location = away;
            fleet.cargo.metal = 10_000.0; // way past any cargo threshold
        }

        engine.handle_policy_update(pid, fid, PolicyPreset::MineAndReturn, None).expect("set");
        engine.drain_events();

        engine.current_tick += 2;
        engine.process_policies().expect("policies");

        let fleet = &engine.fleets[&fid];
        assert_eq!(fleet.state, FleetStatus::Moving, "loaded miner must head home");
        assert_eq!(fleet.move_target, Some(home));

        let events = engine.drain_events();
        let acted = events.iter().find_map(|e| match &e.kind {
            EventKind::PolicyAction(p) => Some(p),
            _ => None,
        }).expect("policy action event");
        assert_eq!(acted.action, "move");
        assert!(acted.reason.contains("cargo"), "reason must say why: {}", acted.reason);
    }

    #[test]
    fn policies_and_sites_survive_restart() {
        let path = std::env::temp_dir().join(format!(
            "iac_policy_restart_test_{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let (fid, site) = {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            let mut engine = GameEngine::init(42, db, None).unwrap();
            let (pid, fid) = register(&mut engine, "Persistent");
            engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, None).unwrap();

            let site = find_derelict_sector(&engine);
            let ov = engine.ensure_override(site.to_key());
            ov.site_looted_tick = Some(7);
            ov.site_ambush_bumps = 2;
            engine.dirty_sectors.insert(site.to_key(), ());
            engine.persist_dirty_state().unwrap();
            (fid, site)
        };

        let db = Database::init(path.to_str().unwrap()).unwrap();
        let engine = GameEngine::init(42, db, None).expect("reload");
        let policy = engine.policies.get(&fid).expect("policy survived restart");
        assert_eq!(policy.preset, PolicyPreset::PatrolHome);

        let ov = engine.sector_overrides.get(&site.to_key()).expect("override survived");
        assert_eq!(ov.site_looted_tick, Some(7));
        assert_eq!(ov.site_ambush_bumps, 2);
        assert!(engine.derelict_site_at(site).is_none(), "looted site stays looted");

        let _ = std::fs::remove_file(&path);
    }

    // ── Hostile ids, salvage and chart memory ─────────────────────

    /// A sector within the 40x40 window whose template spawns an NPC with `behavior`.
    fn find_npc_sector(engine: &GameEngine, behavior: iac_shared::world::NpcBehaviorType) -> Hex {
        for q in -20i16..20 {
            for r in -20i16..20 {
                let coord = Hex { q, r };
                if let Some(t) = engine.world_gen.generate_sector(coord).npc_template
                    && t.behavior == behavior
                    && t.count == 1
                    && t.ship_class == ShipClass::Scout
                    && engine.derelict_site_at(coord).is_none()
                {
                    return coord;
                }
            }
        }
        panic!("no such NPC sector in a 40x40 window");
    }

    /// Make a fleet strong enough to win any early fight in one round.
    fn arm(engine: &mut GameEngine, fid: u64) {
        let f = engine.fleets.get_mut(&fid).unwrap();
        for ship in &mut f.ships[..f.ship_count] {
            ship.weapon_power = 5000.0;
            ship.hull = 1e6;
            ship.hull_max = 1e6;
        }
    }

    /// Fight the sector's NPC with an armed fleet; returns all events.
    fn win_a_fight(engine: &mut GameEngine, pid: u64, fid: u64, at: Hex) -> Vec<GameEvent> {
        engine.fleets.get_mut(&fid).unwrap().location = at;
        arm(engine, fid);
        let target = template_npc_id(at);
        engine.handle_attack(pid, fid, target).expect("attack the listed hostile");
        let mut events = engine.drain_events();
        for _ in 0..5 {
            engine.tick().unwrap();
            events.extend(engine.drain_events());
            if engine.active_combats.is_empty() { break; }
        }
        assert!(engine.active_combats.is_empty(), "fight did not conclude");
        events
    }

    #[test]
    fn listed_hostile_id_is_the_id_combat_uses() {
        use iac_shared::world::NpcBehaviorType::Passive;
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Hunter");
        let at = find_npc_sector(&engine, Passive);

        let view = crate::intel::build_sector_state(&engine, at, Some(pid));
        let listed = view.hostiles.expect("passive scout is listed");
        assert_eq!(listed.len(), 1);
        assert_ne!(listed[0].id, 0, "no placeholder id");
        assert_eq!(listed[0].id, template_npc_id(at));

        engine.fleets.get_mut(&fid).unwrap().location = at;
        assert_eq!(engine.handle_attack(pid, fid, 0), Err(ErrorCode::InvalidTarget), "0 names nothing");
        engine.handle_attack(pid, fid, listed[0].id).expect("listed id attacks");
        let started = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::CombatStarted(c) => Some(c),
            _ => None,
        }).expect("CombatStarted");
        assert_eq!(started.enemy_fleet_id, listed[0].id);
        assert_eq!(started.owner, "Hunter");
    }

    #[test]
    fn no_sector_lists_a_hostile_with_id_zero() {
        let engine = test_engine();
        for q in -15i16..15 {
            for r in -15i16..15 {
                let s = crate::intel::build_sector_state(&engine, Hex { q, r }, None);
                assert!(s.hostiles.iter().flatten().all(|h| h.id != 0), "id 0 at {q},{r}");
            }
        }
    }

    #[test]
    fn a_wandering_template_hostile_is_not_listed_twice() {
        use iac_shared::world::NpcBehaviorType::Patrol;
        let mut engine = test_engine();
        let at = find_npc_sector(&engine, Patrol);
        let tmpl = engine.pending_template_npc(at).unwrap();
        let id = template_npc_id(at);
        engine.spawn_npc_fleet(Hex { q: at.q + 40, r: at.r }, tmpl, id).unwrap();
        assert!(engine.pending_template_npc(at).is_none(), "its fleet is alive elsewhere");
        assert!(crate::intel::build_sector_state(&engine, at, None).hostiles.is_none());
    }

    #[test]
    fn destroyed_npc_event_reports_the_pile_on_the_ground() {
        use iac_shared::world::NpcBehaviorType::Passive;
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Salvager");
        let at = find_npc_sector(&engine, Passive);
        let events = win_a_fight(&mut engine, pid, fid, at);

        let destroyed: Vec<_> = events.iter().filter_map(|e| match &e.kind {
            EventKind::FleetDestroyed(d) if d.is_npc => Some(d.clone()),
            _ => None,
        }).collect();
        assert_eq!(destroyed.len(), 1);
        let pile = engine.sector_overrides[&at.to_key()].salvage.expect("pile dropped");
        assert_eq!(destroyed[0].salvage, pile, "event advertises what can be collected");
        assert_eq!(destroyed[0].sector, at);
        assert_eq!(destroyed[0].owner, None);
        let full_cost = ShipClass::Scout.build_cost();
        assert!(pile.metal < full_cost.metal, "pile is a fraction of the ship's cost");

        let ended = events.iter().find_map(|e| match &e.kind {
            EventKind::CombatEnded(c) => Some(c.clone()),
            _ => None,
        }).unwrap();
        assert_eq!(ended.salvage, Some(pile));
        assert_eq!(ended.salvage_despawn_tick, engine.sector_overrides[&at.to_key()].salvage_despawn_tick);

        engine.fleets.get_mut(&fid).unwrap().ships[0].ship_class = ShipClass::Hauler;
        engine.handle_collect_salvage(pid, fid).expect("collect");
        let got = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::SalvageCollected(c) => Some(c),
            _ => None,
        }).unwrap();
        assert_eq!(got.resources, pile, "collected exactly what was advertised");
        assert_eq!(got.remaining, None);
        assert!(engine.sector_overrides[&at.to_key()].salvage.is_none());
    }

    #[test]
    fn a_full_hold_leaves_the_rest_and_says_so() {
        use iac_shared::world::NpcBehaviorType::Passive;
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Hauler");
        let at = find_npc_sector(&engine, Passive);
        win_a_fight(&mut engine, pid, fid, at);
        let pile = engine.sector_overrides[&at.to_key()].salvage.unwrap();

        let cap = fleet_cargo_capacity(&engine.fleets[&fid], &engine.world.pace);
        let room = pile.metal / 2.0;
        engine.fleets.get_mut(&fid).unwrap().cargo = Resources { metal: cap - room, ..Default::default() };
        engine.handle_collect_salvage(pid, fid).unwrap();
        let got = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::SalvageCollected(c) => Some(c),
            _ => None,
        }).unwrap();
        assert!((got.resources.metal - room).abs() < 1e-3);
        let left = got.remaining.expect("remainder reported");
        assert!((left.metal - (pile.metal - room)).abs() < 1e-3);
        assert_eq!(engine.sector_overrides[&at.to_key()].salvage, Some(left));

        assert_eq!(engine.handle_collect_salvage(pid, fid), Err(ErrorCode::CargoFull));
    }

    #[test]
    fn a_second_kill_stacks_on_an_uncollected_pile() {
        let mut engine = test_engine();
        let at = Hex { q: 3, r: 3 };
        let (first, _) = engine.drop_salvage(at, Resources { metal: 60.0, crystal: 15.0, deuterium: 9.0 });
        let (second, _) = engine.drop_salvage(at, Resources { metal: 60.0, crystal: 15.0, deuterium: 9.0 });
        assert_eq!(second.metal, first.metal * 2.0);
        assert_eq!(engine.sector_overrides[&at.to_key()].salvage, Some(second));
    }

    #[test]
    fn despawning_salvage_tells_the_players_present() {
        let mut engine = test_engine();
        let (_pid, fid) = register(&mut engine, "Witness");
        let at = Hex { q: 4, r: -2 };
        engine.fleets.get_mut(&fid).unwrap().location = at;
        let (pile, despawn) = engine.drop_salvage(at, Resources { metal: 60.0, crystal: 15.0, deuterium: 9.0 });
        engine.drain_events();

        while engine.current_tick + 1 < despawn { engine.tick().unwrap(); }
        assert!(engine.drain_events().iter().all(|e| !matches!(e.kind, EventKind::SalvageDespawned(_))));
        engine.tick().unwrap();
        let gone: Vec<_> = engine.drain_events().into_iter().filter_map(|e| match e.kind {
            EventKind::SalvageDespawned(d) => Some(d),
            _ => None,
        }).collect();
        assert_eq!(gone.len(), 1);
        assert_eq!(gone[0].sector, at);
        assert_eq!(gone[0].resources, pile);
        assert!(engine.sector_overrides[&at.to_key()].salvage.is_none());
    }

    fn alerts_for(engine: &mut GameEngine, pid: u64) -> Vec<iac_shared::protocol::AlertEvent> {
        engine.drain_events().into_iter().filter_map(|e| match e.kind {
            EventKind::Alert(a) if a.player_id == Some(pid) => Some(a),
            _ => None,
        }).collect()
    }

    #[test]
    fn a_raid_reports_when_its_salvage_despawns() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Winner");
        engine.drain_events();
        engine.resolve_raid(pid, 0.1).unwrap();
        let resolved = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::RaidResolved(r) => Some(r),
            _ => None,
        }).unwrap();
        let home = engine.players[&pid].homeworld.to_key();
        assert!(resolved.salvage_dropped.is_some());
        assert_eq!(resolved.salvage_despawn_tick, engine.sector_overrides[&home].salvage_despawn_tick);
        assert_eq!(resolved.salvage_despawn_tick, Some(engine.current_tick + SALVAGE_DESPAWN_TICKS as u64));
    }

    #[test]
    fn home_salvage_nobody_can_scoop_raises_one_alert_near_the_end() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Away");
        let home = engine.players[&pid].homeworld;
        engine.fleets.get_mut(&fid).unwrap().location = Hex { q: home.q + 3, r: home.r };
        let (_, despawn) = engine.drop_salvage(home, Resources { metal: 900.0, crystal: 90.0, deuterium: 9.0 });
        engine.drain_events();

        let mut alerts = Vec::new();
        while engine.current_tick + 1 < despawn {
            engine.tick().unwrap();
            alerts.extend(alerts_for(&mut engine, pid).into_iter().map(|a| (engine.current_tick, a)));
        }
        assert_eq!(alerts.len(), 1, "one alert, not one per tick: {alerts:?}");
        let (at, alert) = &alerts[0];
        assert_eq!(*at, despawn - u64::from(HOME_SALVAGE_WARN_TICKS));
        assert!(alert.message.contains("salvage at home") && alert.message.contains("60s"), "{}", alert.message);
        assert_eq!(alert.sector, Some(home));
    }

    #[test]
    fn a_fleet_docked_at_home_scoops_home_salvage_into_storage() {
        let mut engine = test_engine();
        let (pid, _fid) = register(&mut engine, "Home");
        let home = engine.players[&pid].homeworld;
        engine.players.get_mut(&pid).unwrap().resources = Resources::default();
        let pile = Resources { metal: 900.0, crystal: 90.0, deuterium: 9.0 };
        engine.drop_salvage(home, pile);
        engine.drain_events();
        engine.tick().unwrap();
        let p = &engine.players[&pid];
        assert!(p.resources.metal >= pile.metal && p.resources.crystal >= pile.crystal, "{:?}", p.resources);
        assert!(engine.sector_overrides[&home.to_key()].salvage.is_none());
        let events = engine.drain_events();
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::SalvageCollected(c) if c.resources == pile && c.remaining.is_none())));
    }

    #[test]
    fn home_salvage_beyond_storage_stays_and_alerts() {
        let mut engine = test_engine();
        let (pid, _fid) = register(&mut engine, "Full");
        let home = engine.players[&pid].homeworld;
        let cap = scaling::storage_cap(engine.players[&pid].buildings.storage_vault, &engine.world.pace);
        engine.players.get_mut(&pid).unwrap().resources = cap;
        let (_, despawn) = engine.drop_salvage(home, Resources { metal: 500.0, crystal: 0.0, deuterium: 0.0 });
        engine.drain_events();
        while engine.current_tick + 1 < despawn {
            engine.tick().unwrap();
        }
        let ov = &engine.sector_overrides[&home.to_key()];
        assert_eq!(ov.salvage.map(|s| s.metal), Some(500.0), "nothing fits, so the pile stays");
        let events = engine.drain_events();
        let alerts: Vec<_> = events.iter().filter(|e| matches!(&e.kind, EventKind::Alert(a) if a.message.contains("storage is full"))).collect();
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn the_chart_drops_a_remembered_pile_once_it_has_despawned() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Chartist");
        let home = engine.players[&pid].homeworld;
        let at = Hex { q: home.q + 4, r: home.r };
        engine.fleets.get_mut(&fid).unwrap().location = at;
        let (pile, despawn) = engine.drop_salvage(at, Resources { metal: 60.0, crystal: 15.0, deuterium: 9.0 });
        engine.tick().unwrap();
        let live = &chart_of(&engine, pid)[&at.to_key()];
        assert!(live.live && live.salvage == Some(pile) && live.salvage_despawn_tick == Some(despawn));
        assert!(!live.pins_stale);

        engine.fleets.get_mut(&fid).unwrap().location = home;
        engine.tick().unwrap();
        let stale = &chart_of(&engine, pid)[&at.to_key()];
        assert!(!stale.live && stale.salvage == Some(pile), "out of sight the pin is kept");
        assert!(stale.pins_stale, "but marked unconfirmed");
        assert_eq!(stale.salvage_despawn_tick, Some(despawn), "with its countdown");

        let mut cleared_updates = 0;
        while engine.current_tick < despawn {
            engine.tick().unwrap();
            cleared_updates += engine.known.tick_updates(&engine, pid).iter()
                .filter(|s| s.location == at && s.salvage.is_none()).count();
        }
        let gone = &chart_of(&engine, pid)[&at.to_key()];
        assert!(gone.salvage.is_none() && gone.salvage_despawn_tick.is_none() && !gone.pins_stale);
        assert_eq!(cleared_updates, 1, "the client is told once");
    }

    #[test]
    fn a_boarded_derelict_leaves_the_chart_when_the_player_watched_it_go() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Boarder");
        let home = engine.players[&pid].homeworld;
        let site = find_derelict_sector(&engine);
        engine.fleets.get_mut(&fid).unwrap().location = site;
        engine.tick().unwrap();
        assert!(chart_of(&engine, pid)[&site.to_key()].site.is_some());

        engine.fleets.get_mut(&fid).unwrap().location = home;
        engine.tick().unwrap();
        let away = &chart_of(&engine, pid)[&site.to_key()];
        assert!(away.site.is_some() && away.pins_stale, "unconfirmed from afar");

        engine.fleets.get_mut(&fid).unwrap().location = site;
        engine.sector_overrides.entry(site.to_key()).or_default().site_looted_tick = Some(engine.current_tick);
        engine.tick().unwrap();
        let seen = &chart_of(&engine, pid)[&site.to_key()];
        assert!(seen.live && seen.site.is_none(), "seen gone, so the pin is dropped");
        engine.fleets.get_mut(&fid).unwrap().location = home;
        engine.tick().unwrap();
        let later = &chart_of(&engine, pid)[&site.to_key()];
        assert!(later.site.is_none() && !later.pins_stale);
    }

    #[test]
    fn scan_signals_never_name_the_scanning_sector() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Pinger");
        let origin = engine.fleets[&fid].location;
        engine.handle_scan(pid, fid).unwrap();
        let scan = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::ScanCompleted(s) => Some(s),
            _ => None,
        }).unwrap();
        assert_eq!(scan.sector, origin);
        assert!(scan.signals.iter().all(|c| c.sector != origin));
        assert!(scan.threats.iter().all(|t| t.sector != origin));
    }

    #[test]
    fn salvage_survives_restart() {
        let path = std::env::temp_dir().join(format!("iac_salvage_restart_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let at = Hex { q: 2, r: 5 };
        let (pile, despawn) = {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            let mut engine = GameEngine::init(42, db, None).unwrap();
            let dropped = engine.drop_salvage(at, Resources { metal: 60.0, crystal: 15.0, deuterium: 9.0 });
            engine.persist_dirty_state().unwrap();
            dropped
        };
        let db = Database::init(path.to_str().unwrap()).unwrap();
        let engine = GameEngine::init(42, db, None).unwrap();
        let ov = &engine.sector_overrides[&at.to_key()];
        assert_eq!(ov.salvage, Some(pile));
        assert_eq!(ov.salvage_despawn_tick, Some(despawn));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn losing_a_fleet_raises_a_critical_alert_for_its_owner_only() {
        use iac_shared::protocol::AlertLevel;
        use iac_shared::world::NpcBehaviorType::Patrol;
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Doomed");
        let (other, _) = register(&mut engine, "Bystander");
        let at = find_npc_sector(&engine, Patrol);
        engine.fleets.get_mut(&fid).unwrap().location = at;
        for ship in &mut engine.fleets.get_mut(&fid).unwrap().ships[..STARTING_SCOUTS] {
            ship.hull = 0.5;
            ship.shield = 0.0;
        }
        engine.handle_attack(pid, fid, template_npc_id(at)).unwrap();
        let mut events = engine.drain_events();
        for _ in 0..10 {
            engine.tick().unwrap();
            events.extend(engine.drain_events());
        }
        let lost = events.iter().find_map(|e| match &e.kind {
            EventKind::FleetDestroyed(d) if !d.is_npc => Some(d.clone()),
            _ => None,
        }).expect("player fleet destroyed");
        assert_eq!(lost.owner.as_deref(), Some("Doomed"));
        assert_eq!(lost.salvage, Resources::default());
        let alerts: Vec<_> = events.iter().filter_map(|e| match &e.kind {
            EventKind::Alert(a) => Some(a.clone()),
            _ => None,
        }).collect();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].player_id, Some(pid));
        assert_ne!(alerts[0].player_id, Some(other));
        assert!(matches!(alerts[0].level, AlertLevel::Critical));
        assert_eq!(alerts[0].fleet_id, Some(fid));
    }

    fn chart_of(engine: &GameEngine, pid: u64) -> HashMap<u32, iac_shared::protocol::SectorState> {
        engine.known.chart(engine, pid).into_iter().map(|s| (s.location.to_key(), s)).collect()
    }

    #[test]
    fn explored_sectors_outlive_the_scan_that_found_them() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Cartographer");
        engine.handle_scan(pid, fid).unwrap();
        engine.tick().unwrap();
        let revealed = engine.scan_revealed_coords(pid);
        assert!(!revealed.is_empty());
        let chart = chart_of(&engine, pid);
        for c in &revealed {
            let s = &chart[&c.to_key()];
            assert!(s.live);
            assert_eq!(s.last_seen, engine.current_tick);
        }
        let scanned_at = engine.current_tick;
        let known_before = chart.len();

        for _ in 0..(SCAN_REVEAL_TICKS + 2) {
            engine.tick().unwrap();
        }
        assert!(engine.scan_revealed_coords(pid).is_empty());
        let chart = chart_of(&engine, pid);
        assert_eq!(chart.len(), known_before, "no sector is forgotten");
        let here = engine.fleets[&fid].location;
        for c in revealed.iter().filter(|c| **c != here) {
            let s = &chart[&c.to_key()];
            assert!(!s.live, "{c:?} should be stale");
            assert!(s.last_seen >= scanned_at && s.last_seen < engine.current_tick);
        }
        assert!(chart[&here.to_key()].live, "the fleet's own sector stays live");
    }

    #[test]
    fn a_sector_goes_stale_once_in_the_tick_updates() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Watcher");
        engine.handle_scan(pid, fid).unwrap();
        engine.tick().unwrap();
        let mut stale_notices: HashMap<u32, u32> = HashMap::new();
        for _ in 0..(SCAN_REVEAL_TICKS + 5) {
            engine.tick().unwrap();
            for s in engine.known.tick_updates(&engine, pid) {
                if !s.live {
                    *stale_notices.entry(s.location.to_key()).or_default() += 1;
                }
            }
        }
        assert!(!stale_notices.is_empty());
        assert!(stale_notices.values().all(|&n| n == 1), "stale deltas are sent once: {stale_notices:?}");
    }

    #[test]
    fn stale_entry_keeps_what_was_last_seen() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Witness");
        let at = Hex { q: 1, r: 1 };
        engine.fleets.get_mut(&fid).unwrap().location = at;
        let (pile, despawn) = engine.drop_salvage(at, Resources { metal: 60.0, crystal: 15.0, deuterium: 9.0 });
        engine.tick().unwrap();
        let seen_at = engine.current_tick;
        assert_eq!(chart_of(&engine, pid)[&at.to_key()].salvage, Some(pile));

        // Fly away: the pile is remembered as seen, flagged unconfirmed, until its own timer runs out.
        engine.fleets.get_mut(&fid).unwrap().location = engine.players[&pid].homeworld;
        while engine.current_tick + 2 < despawn { engine.tick().unwrap(); }
        let s = &chart_of(&engine, pid)[&at.to_key()];
        assert!(!s.live && s.pins_stale);
        assert_eq!(s.salvage, Some(pile), "remembered as seen");
        assert_eq!(s.salvage_despawn_tick, Some(despawn));
        assert_eq!(s.last_seen, seen_at);

        // Its despawn tick passed while nobody looked: the player knows it is gone.
        while engine.current_tick <= despawn + 1 { engine.tick().unwrap(); }
        let s = &chart_of(&engine, pid)[&at.to_key()];
        assert!(!s.live && s.salvage.is_none() && !s.pins_stale);
        assert_eq!(s.last_seen, seen_at);
    }

    #[test]
    fn chart_survives_server_restart_marked_stale() {
        let path = std::env::temp_dir().join(format!("iac_chart_restart_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (pid, seen) = {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            let mut engine = GameEngine::init(42, db, None).unwrap();
            let (pid, fid) = register(&mut engine, "Veteran");
            engine.handle_scan(pid, fid).unwrap();
            for _ in 0..3 { engine.tick().unwrap(); }
            engine.checkpoint_known_sectors();
            engine.persist_dirty_state().unwrap();
            engine.flush_persistence().unwrap();
            (pid, chart_of(&engine, pid))
        };
        assert!(seen.len() > 1);

        let db = Database::init(path.to_str().unwrap()).unwrap();
        let mut engine = GameEngine::init(42, db, None).unwrap();
        // The scan reveal itself is memory-only, so only the sensor/fleet sectors are live now.
        let after = chart_of(&engine, pid);
        assert_eq!(after.len(), seen.len(), "every remembered sector reloaded");
        for (key, old) in &seen {
            let new = &after[key];
            assert_eq!(new.last_seen, old.last_seen);
            assert_eq!(new.resources, old.resources);
            assert_eq!(new.hostiles, old.hostiles);
        }
        let stale = after.values().filter(|s| !s.live).count();
        assert!(stale > 0, "scan-only sectors come back stale");

        engine.tick().unwrap();
        let again = chart_of(&engine, pid);
        assert!(again.len() >= seen.len());
        let _ = std::fs::remove_file(&path);
    }

    // ── Fleets, fuel and autopilot ────────────────────────────────

    /// A walk of `len` charted hops out from home, each hop one step
    /// farther away; marks every sector on it explored by the player.
    fn charted_chain(engine: &mut GameEngine, pid: u64, len: usize) -> Vec<Hex> {
        fn extend(engine: &GameEngine, chain: &mut Vec<Hex>, len: usize, home: Hex) -> bool {
            if chain.len() == len { return true; }
            let tip = *chain.last().unwrap();
            for &n in engine.world_gen.connected_neighbors(tip).slice() {
                if Hex::distance(&n, &home) != chain.len() as u16 { continue; }
                chain.push(n);
                if extend(engine, chain, len, home) { return true; }
                chain.pop();
            }
            false
        }
        let home = engine.players[&pid].homeworld;
        let mut chain = vec![home];
        assert!(extend(engine, &mut chain, len + 1, home), "no straight {len}-hop lane from home");
        let walk: Vec<Hex> = chain[1..].to_vec();
        for h in &walk { engine.explored.insert((pid, h.to_key())); }
        walk
    }

    fn place(engine: &mut GameEngine, fid: u64, at: Hex, fuel: f32) {
        let f = engine.fleets.get_mut(&fid).unwrap();
        f.location = at;
        f.state = FleetStatus::Idle;
        f.fuel = fuel;
    }

    fn policy_events(engine: &mut GameEngine) -> Vec<iac_shared::protocol::PolicyActionEvent> {
        engine.drain_events().into_iter().filter_map(|e| match e.kind {
            EventKind::PolicyAction(p) => Some(p),
            _ => None,
        }).collect()
    }

    fn alerts(engine: &mut GameEngine) -> Vec<AlertEvent> {
        engine.drain_events().into_iter().filter_map(|e| match e.kind {
            EventKind::Alert(a) => Some(a),
            _ => None,
        }).collect()
    }

    #[test]
    fn starting_fleet_has_the_real_tank_from_tick_zero() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Fresh");
        let fleet = &engine.fleets[&fid];
        let expected = STARTING_SCOUTS as f32 * ShipClass::Scout.base_stats().fuel as f32;
        assert_eq!(fleet.fuel_max, expected);
        assert_eq!(fleet.fuel, fleet.fuel_max);
        assert!(fleet.fuel_max < 1000.0, "no 50000 placeholder tank");
        assert!(fleet.fuel_max >= engine.hop_fuel_cost(fleet), "the tank covers at least one jump");
        let _ = pid;
    }

    #[test]
    fn mine_claims_stay_within_range_of_home() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Miner");
        let home = engine.players[&pid].homeworld;
        let params = PolicyParams { max_range: 2, ..PolicyParams::default() };

        // Orders given four hops out must not adopt that spot as the claim.
        let far = charted_chain(&mut engine, pid, 4);
        place(&mut engine, fid, far[3], 1_000_000.0);
        engine.fleets.get_mut(&fid).unwrap().fuel_max = 1_000_000.0;
        engine.handle_policy_update(pid, fid, PolicyPreset::MineAndReturn, Some(params)).unwrap();
        engine.drain_events();

        let mut dried = 0;
        for tick in 0..400 {
            engine.tick().unwrap();
            let Some(policy) = engine.policies.get(&fid) else { break; };
            if let Some(work) = policy.work_sector {
                assert!(
                    Hex::distance(&work, &home) <= 2 || work == home,
                    "claim {work} is {} from home at tick {tick}", Hex::distance(&work, &home),
                );
                // Exhaust each claim in turn to force re-claims.
                if tick % 25 == 24 && work != home && dried < 6 {
                    let ov = engine.ensure_override(work.to_key());
                    ov.metal_density = Some(Density::None);
                    ov.crystal_density = Some(Density::None);
                    ov.deut_density = Some(Density::None);
                    dried += 1;
                }
            }
        }
    }

    /// Everything within `radius` of home counts as entered.
    fn chart_around_home(engine: &mut GameEngine, pid: u64, radius: u16) {
        let home = engine.players[&pid].homeworld;
        for q in -(radius as i16)..=radius as i16 {
            for r in -(radius as i16)..=radius as i16 {
                let h = Hex { q: home.q + q, r: home.r + r };
                if Hex::distance(&h, &home) <= radius { engine.explored.insert((pid, h.to_key())); }
            }
        }
    }

    fn set_weapons(engine: &mut GameEngine, fid: u64, weapon: f32) {
        let f = engine.fleets.get_mut(&fid).unwrap();
        for ship in f.ships[..f.ship_count].iter_mut() {
            ship.weapon_power = weapon;
            if weapon < 1.0 { ship.hull = 1.0; ship.shield = 0.0; }
        }
    }

    /// Runs the autopilot and returns (moves, hold reasons).
    fn run_policy(engine: &mut GameEngine, ticks: u32) -> (u32, Vec<String>) {
        let (mut moves, mut holds) = (0, Vec::new());
        for _ in 0..ticks {
            engine.tick().unwrap();
            for ev in policy_events(engine) {
                match ev.action.as_str() {
                    "move" => moves += 1,
                    "hold" => holds.push(ev.reason),
                    _ => {}
                }
            }
        }
        (moves, holds)
    }

    #[test]
    fn prospect_never_enters_a_sector_it_would_lose_in() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Weakling");
        let home = engine.players[&pid].homeworld;
        set_weapons(&mut engine, fid, 0.1);
        chart_around_home(&mut engine, pid, 2);
        let params = PolicyParams { max_range: 4, ..PolicyParams::default() };
        engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
        engine.drain_events();

        let mut seen_out = false;
        let mut holds = Vec::new();
        for _ in 0..300 {
            engine.tick().unwrap();
            let at = engine.fleets[&fid].location;
            seen_out |= !engine.explored.contains(&(pid, at.to_key())) && at != home;
            holds.extend(policy_events(&mut engine).into_iter().filter(|e| e.action == "hold").map(|e| e.reason));
        }
        assert!(!seen_out, "the fleet left the charted ring into a sector it cannot beat");
        assert!(holds.iter().any(|h| h.contains("DEADLY")), "the refusal names the verdict: {holds:?}");
        assert!(holds[0].contains("doctrine needs"), "{}", holds[0]);
    }

    /// A sector holding hostiles, and the weapon power at which `fid` stands
    /// at `ratio` times their threat.
    fn tune_ratio_against_hostiles(engine: &mut GameEngine, pid: u64, fid: u64, ratio: f32) -> Hex {
        let coord = (-12i16..12)
            .flat_map(|q| (-12i16..12).map(move |r| Hex { q, r }))
            .find(|&h| h.dist_from_origin() > 3 && engine.sector_has_hostiles(h))
            .expect("a hostile sector");
        let threat = engine.known_threat(pid, coord).est_power;
        let (mut lo, mut hi) = (0.0f32, 1e6f32);
        for _ in 0..60 {
            let mid = (lo + hi) / 2.0;
            set_weapons(engine, fid, mid);
            if fleet_power(&engine.fleets[&fid]) / threat < ratio { lo = mid } else { hi = mid }
        }
        coord
    }

    #[test]
    fn the_doctrine_enters_hostile_sectors_at_the_engage_ratio_the_player_set() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Dialled");
        let coord = tune_ratio_against_hostiles(&mut engine, pid, fid, 1.6);
        let fleet = engine.fleets[&fid].clone();
        let with = |x10: u8| PolicyParams { engage_ratio_x10: x10, ..PolicyParams::default() };

        assert!(engine.policy_entry_check(&fleet, &with(12), coord).is_ok(), "1.6x clears the default 1.2x; no 2.5x override");
        assert!(engine.policy_entry_check(&fleet, &with(15), coord).is_ok());
        let err = engine.policy_entry_check(&fleet, &with(20), coord).unwrap_err();
        assert!(err.contains("the doctrine needs 2.0x"), "{err}");

        let coord = tune_ratio_against_hostiles(&mut engine, pid, fid, 0.7);
        let fleet = engine.fleets[&fid].clone();
        assert!(engine.policy_entry_check(&fleet, &with(6), coord).is_ok(), "a setting below EVEN is honoured");
        assert!(engine.policy_entry_check(&fleet, &with(8), coord).is_err());
    }

    #[test]
    fn a_standing_hold_is_reported_once_a_minute_not_every_evaluation() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Patient");
        set_weapons(&mut engine, fid, 0.1);
        chart_around_home(&mut engine, pid, 2);
        let params = PolicyParams { max_range: 4, ..PolicyParams::default() };
        engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
        engine.drain_events();
        let (_, first_minute) = run_policy(&mut engine, 55);
        assert_eq!(first_minute.len(), 1, "one report, not one per evaluation: {first_minute:?}");
        let (_, later) = run_policy(&mut engine, 125);
        assert!(later.len() <= 3, "at most one a minute afterwards: {later:?}");
    }

    #[test]
    fn prospect_pushes_the_frontier_when_a_safe_path_exists() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Bruiser");
        let home = engine.players[&pid].homeworld;
        set_weapons(&mut engine, fid, 5000.0);
        chart_around_home(&mut engine, pid, 2);
        let params = PolicyParams { max_range: 4, ..PolicyParams::default() };
        engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
        engine.drain_events();

        let (moves, _) = run_policy(&mut engine, 400);
        assert!(moves > 0);
        let beyond = engine.explored.iter()
            .filter(|(p, k)| *p == pid && Hex::distance(&Hex::from_key(*k), &home) > 2)
            .count();
        assert!(beyond >= 3, "a strong fleet charts past the ring: {beyond}");
    }

    #[test]
    fn the_prospect_hold_names_the_constraint_that_blocks_it() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Boxed");
        set_weapons(&mut engine, fid, 5000.0);
        chart_around_home(&mut engine, pid, 2);
        let params = PolicyParams { max_range: 2, ..PolicyParams::default() };
        engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
        engine.drain_events();
        let (_, holds) = run_policy(&mut engine, 100);
        assert!(holds[0].contains("outside within 2 hops of home"), "range: {}", holds[0]);

        let (pid, fid) = register(&mut engine, "Hunted");
        set_weapons(&mut engine, fid, 0.1);
        chart_around_home(&mut engine, pid, 2);
        let params = PolicyParams { max_range: 4, ..PolicyParams::default() };
        engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
        engine.drain_events();
        let (_, holds) = run_policy(&mut engine, 100);
        assert!(holds[0].contains("no safe way to the nearest uncharted sector"), "hazard: {}", holds[0]);
    }

    #[test]
    fn standing_orders_refuse_a_lane_through_a_sector_they_would_lose_in() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Hauler");
        let home = engine.players[&pid].homeworld;
        let lair = engine.find_nearest_target(home, 8, PolicyTarget::Hostiles).expect("a lair near home");
        set_weapons(&mut engine, fid, 0.1);
        engine.drain_events();

        for preset in [PolicyPreset::MineAndReturn, PolicyPreset::SalvageAndSites] {
            assert!(!engine.policy_step_toward(fid, lair, preset, "test"));
            let events = policy_events(&mut engine);
            assert_eq!(events.len(), 1, "{events:?}");
            assert_eq!(events[0].action, "hold");
            assert!(events[0].reason.contains("refusing the way to"), "{}", events[0].reason);
            assert_eq!(engine.fleets[&fid].location, home);
            if let Some(p) = engine.policies.get_mut(&fid) { p.last_hold = None; }
        }

        set_weapons(&mut engine, fid, 5000.0);
        assert!(engine.policy_step_toward(fid, lair, PolicyPreset::SalvageAndSites, "test"));
    }

    #[test]
    fn hopes_of_getting_home_use_the_shortest_real_route() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Wanderer");
        let home = engine.players[&pid].homeworld;
        // A sector with a direct lane home and a longer charted detour.
        let mut found = None;
        'search: for q in -4i16..=4 {
            for r in -4i16..=4 {
                let x = Hex { q: home.q + q, r: home.r + r };
                let d = Hex::distance(&x, &home);
                if !(2..=3).contains(&d) { continue; }
                let Some(direct) = engine.lane_path(x, home, 64, &|_| true) else { continue; };
                let mid: Vec<Hex> = direct[..direct.len() - 1].to_vec();
                if let Some(detour) = engine.lane_path(x, home, 64, &|n| !mid.contains(&n))
                    && detour.len() > direct.len()
                {
                    found = Some((x, direct.len() as u32, detour));
                    break 'search;
                }
            }
        }
        let (x, direct, detour) = found.expect("a sector with a longer charted detour");
        for h in &detour { engine.explored.insert((pid, h.to_key())); }
        engine.explored.insert((pid, x.to_key()));

        let route = engine.home_route(pid, x);
        assert_eq!(route.direct, direct);
        assert_eq!(route.charted, Some(detour.len() as u32));
        assert!(route.unexplored, "the direct lanes cross uncharted space");

        // Fuel for the direct route, not for the detour: no warning, no early return.
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);
        place(&mut engine, fid, x, hop * (direct as f32 + 0.5));
        let fleet = engine.fleets[&fid].clone();
        let params = PolicyParams { min_fuel_pct: 0, ..PolicyParams::default() };
        assert!(engine.policy_return_reason(&fleet, &params).is_none(), "the direct route is affordable");

        // One hop short even of the direct route: the warning quotes both.
        engine.fleets.get_mut(&fid).unwrap().fuel = hop * (direct as f32 - 0.5);
        engine.drain_events();
        engine.warn_if_cannot_return(fid, x, hop);
        let msg = alerts(&mut engine).remove(0).message;
        assert!(msg.contains(&format!("the charted route home is {} hops", detour.len())), "{msg}");
        assert!(msg.contains(&format!("the direct route is {direct} hops")), "{msg}");
        assert!(msg.contains("across unexplored space"), "{msg}");
    }

    #[test]
    fn warnings_without_a_charted_route_say_so() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Pathfinder");
        let chain = charted_chain(&mut engine, pid, 2);
        engine.explored.remove(&(pid, chain[0].to_key()));
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);
        place(&mut engine, fid, chain[1], hop * 1.5);
        engine.drain_events();
        engine.warn_if_cannot_return(fid, chain[1], hop);
        let a = alerts(&mut engine).remove(0);
        assert!(a.message.contains("no charted route home; the direct route is 2 hops"), "{}", a.message);
        assert!(a.message.contains("across unexplored space"), "{}", a.message);
    }

    #[test]
    fn previews_rate_the_odds_of_the_fleet_asked_about() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Twins");
        let home = engine.players[&pid].homeworld;
        let ids: Vec<u64> = engine.fleets[&fid].ships[..engine.fleets[&fid].ship_count].iter().map(|s| s.id).collect();
        let other = engine.handle_split(pid, fid, &ids[..1]).unwrap();
        set_weapons(&mut engine, other, 400.0);
        let target = engine.world_gen.connected_neighbors(home).slice()[0];

        let weak = engine.preview_move(pid, fid, target).unwrap();
        let strong = engine.preview_move(pid, other, target).unwrap();
        assert_eq!(weak.fleet_id, fid);
        assert!(strong.fleet_power > weak.fleet_power * 10.0);
        assert!(strong.ratio > weak.ratio * 10.0);
        assert!(strong.label > weak.label, "{:?} vs {:?}", strong.label, weak.label);
    }

    #[test]
    fn prospect_holds_instead_of_thrashing_when_the_range_is_charted() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Prospector");
        let home = engine.players[&pid].homeworld;
        let params = PolicyParams { max_range: 2, ..PolicyParams::default() };
        for q in -2i16..=2 {
            for r in -2i16..=2 {
                let h = Hex { q: home.q + q, r: home.r + r };
                if Hex::distance(&h, &home) <= 2 { engine.explored.insert((pid, h.to_key())); }
            }
        }
        engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
        engine.drain_events();

        let mut moves = 0;
        let mut holds = Vec::new();
        for _ in 0..200 {
            engine.tick().unwrap();
            for ev in policy_events(&mut engine) {
                match ev.action.as_str() {
                    "move" => moves += 1,
                    "hold" => holds.push(ev.reason),
                    _ => {}
                }
            }
        }
        assert_eq!(moves, 0, "everything in range is charted: nothing to move to");
        assert!(!holds.is_empty(), "the hold must be announced");
        assert!(holds[0].contains("uncharted"), "reason: {}", holds[0]);
        assert!(holds.len() <= 4, "the same hold must not flood events: {}", holds.len());
        assert_eq!(engine.fleets[&fid].location, home);
    }

    #[test]
    fn autopilot_returns_on_route_cost_not_fuel_percent() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Cautious");
        let chain = charted_chain(&mut engine, pid, 3);
        let params = PolicyParams { min_fuel_pct: 0, ..PolicyParams::default() };
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);

        // Three hops out, plenty of tank: a bare 20% would have tripped the
        // old rule, yet the route home is affordable so nothing forces a return.
        engine.fleets.get_mut(&fid).unwrap().fuel_max = hop * 100.0;
        place(&mut engine, fid, chain[2], hop * 3.5);
        let fleet = engine.fleets[&fid].clone();
        assert!(engine.policy_return_reason(&fleet, &params).is_none());

        place(&mut engine, fid, chain[2], hop * 2.9);
        let fleet = engine.fleets[&fid].clone();
        let reason = engine.policy_return_reason(&fleet, &params).expect("short of the route home");
        assert!(reason.contains("route home"), "{reason}");
    }

    #[test]
    fn autopilot_refuses_a_hop_that_would_strand_it() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Careful");
        let chain = charted_chain(&mut engine, pid, 2);
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);
        let params = PolicyParams { min_fuel_pct: 0, ..PolicyParams::default() };
        engine.handle_policy_update(pid, fid, PolicyPreset::Prospect, Some(params)).unwrap();
        engine.drain_events();

        // At chain[0] with fuel for the way home plus one hop but not two:
        // going out one more would leave too little for the return.
        place(&mut engine, fid, chain[0], hop * 2.5);
        let acted = engine.policy_move(fid, chain[1], false, PolicyPreset::Prospect, "pushing frontier");
        assert!(acted, "turning for home counts as acting");
        let fleet = &engine.fleets[&fid];
        assert_eq!(fleet.move_target, Some(engine.players[&pid].homeworld), "must turn for home");
        let events = policy_events(&mut engine);
        assert!(events.iter().any(|e| e.action == "move" && e.reason.starts_with("returning")), "{events:?}");
    }

    #[test]
    fn manual_move_that_strands_warns() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Reckless");
        let chain = charted_chain(&mut engine, pid, 3);
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);

        // Jump to hop 3 with fuel left for less than one more hop: stranded.
        place(&mut engine, fid, chain[1], hop * 1.5);
        engine.drain_events();
        engine.handle_move(pid, fid, chain[2]).expect("risky moves are allowed");
        let a = alerts(&mut engine);
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(matches!(a[0].level, AlertLevel::Critical));
        assert_eq!(a[0].fleet_id, Some(fid));
        assert!(a[0].message.contains("stranded"), "{}", a[0].message);

        // Enough to move again but not to get home: a plain warning.
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Reckless");
        let chain = charted_chain(&mut engine, pid, 3);
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);
        place(&mut engine, fid, chain[1], hop * 3.0);
        engine.drain_events();
        engine.handle_move(pid, fid, chain[2]).unwrap();
        let a = alerts(&mut engine);
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(matches!(a[0].level, AlertLevel::Warning));

        // A safe move says nothing.
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Reckless");
        let chain = charted_chain(&mut engine, pid, 2);
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);
        place(&mut engine, fid, chain[0], hop * 10.0);
        engine.drain_events();
        engine.handle_move(pid, fid, chain[1]).unwrap();
        assert!(alerts(&mut engine).is_empty());
    }

    #[test]
    fn stranded_fleet_regains_one_jump_slowly() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Adrift");
        let chain = charted_chain(&mut engine, pid, 2);
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);
        place(&mut engine, fid, chain[1], 0.0);
        assert_eq!(engine.handle_move(pid, fid, chain[0]), Err(ErrorCode::InsufficientFuel));
        engine.drain_events();

        let mut announced = 0;
        for _ in 0..STRANDED_RECOVERY_TICKS - 1 {
            engine.process_stranded();
            announced += alerts(&mut engine).len();
        }
        assert!(engine.fleets[&fid].fuel < hop, "still short one tick before the end");
        assert_eq!(engine.handle_move(pid, fid, chain[0]), Err(ErrorCode::InsufficientFuel));
        assert!(announced >= 3, "quarter-way progress is announced, got {announced}");

        engine.process_stranded();
        let a = alerts(&mut engine);
        assert!(a.iter().any(|e| e.message.contains("reserve ready")), "{a:?}");
        engine.handle_move(pid, fid, chain[0]).expect("one jump toward home");

        // Spent: the next hop needs the whole wait again.
        assert!(engine.fleets[&fid].fuel < hop * 0.01);
    }

    #[test]
    fn reserve_does_not_trickle_at_home_or_en_route() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Docked");
        engine.fleets.get_mut(&fid).unwrap().fuel = 0.0;
        engine.process_stranded();
        assert_eq!(engine.fleets[&fid].fuel, 0.0, "home refuels by docking, not the reserve");

        let chain = charted_chain(&mut engine, pid, 1);
        place(&mut engine, fid, chain[0], 0.0);
        engine.fleets.get_mut(&fid).unwrap().state = FleetStatus::Harvesting;
        engine.process_stranded();
        assert_eq!(engine.fleets[&fid].fuel, 0.0);
    }

    #[test]
    fn recall_without_the_fuel_leaves_the_fleet_alone() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Short");
        let chain = charted_chain(&mut engine, pid, 2);
        let hop = engine.hop_fuel_cost(&engine.fleets[&fid]);

        // Two hexes out a recall costs 2 x 2 hops of fuel.
        place(&mut engine, fid, chain[1], hop * 3.9);
        assert_eq!(engine.handle_recall(pid, fid), Err(ErrorCode::InsufficientFuel));
        let fleet = &engine.fleets[&fid];
        assert_eq!(fleet.location, chain[1]);
        assert_eq!(fleet.fuel, hop * 3.9, "a refused recall burns nothing");

        // Walking costs less than recalling, and still works.
        engine.handle_move(pid, fid, chain[0]).expect("walking home is cheaper than the emergency jump");
    }

    #[test]
    fn split_and_merge_move_ships_fuel_and_cargo() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Fleeter");
        engine.add_ship_to_homeworld(pid, ShipClass::Hauler).unwrap();
        assert_eq!(engine.fleets[&fid].ship_count, 3, "new hulls reinforce the idle fleet at home");
        engine.fleets.get_mut(&fid).unwrap().cargo = Resources { metal: 30.0, crystal: 0.0, deuterium: 0.0 };
        let hauler = engine.fleets[&fid].ships[2];
        assert_eq!(hauler.ship_class, ShipClass::Hauler);
        let (fuel_before, cargo_before) = {
            let f = &engine.fleets[&fid];
            (f.fuel, f.cargo.metal)
        };

        let new_id = engine.handle_split(pid, fid, &[hauler.id]).expect("split");
        assert_eq!(engine.fleets[&fid].ship_count, 2);
        let new_fleet = &engine.fleets[&new_id];
        assert_eq!(new_fleet.ship_count, 1);
        assert_eq!(new_fleet.ships[0].id, hauler.id);
        assert_eq!(new_fleet.location, engine.fleets[&fid].location);
        let (a, b) = (&engine.fleets[&fid], new_fleet);
        assert!((a.fuel + b.fuel - fuel_before).abs() < 0.01, "fuel is conserved");
        assert!((a.cargo.metal + b.cargo.metal - cargo_before).abs() < 0.01, "cargo is conserved");
        assert!(a.fuel <= a.fuel_max && b.fuel <= b.fuel_max);
        assert!(alerts(&mut engine).iter().any(|e| e.fleet_id == Some(new_id)), "split announced");

        engine.handle_merge(pid, fid, new_id).expect("merge");
        assert!(!engine.fleets.contains_key(&new_id));
        let merged = &engine.fleets[&fid];
        assert_eq!(merged.ship_count, 3);
        assert!((merged.fuel - fuel_before).abs() < 0.01);
        assert!((merged.cargo.metal - cargo_before).abs() < 0.01);
    }

    #[test]
    fn split_and_merge_reject_bad_requests() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Fleeter");
        let (other, other_fid) = register(&mut engine, "Rival");
        let ids: Vec<u64> = engine.fleets[&fid].ships[..2].iter().map(|s| s.id).collect();

        assert_eq!(engine.handle_split(pid, fid, &[]), Err(ErrorCode::InvalidTarget));
        assert_eq!(engine.handle_split(pid, fid, &ids), Err(ErrorCode::InvalidTarget), "one ship must stay");
        assert_eq!(engine.handle_split(pid, fid, &[999_999]), Err(ErrorCode::InvalidTarget));
        assert_eq!(engine.handle_split(other, fid, &ids[..1]), Err(ErrorCode::FleetNotFound));
        assert_eq!(engine.handle_merge(pid, fid, fid), Err(ErrorCode::InvalidTarget));
        assert_eq!(engine.handle_merge(pid, fid, other_fid), Err(ErrorCode::FleetNotFound));

        // Total fleet cap.
        let mut made = 1;
        loop {
            let donor = engine.fleets.values().filter(|f| f.owner_id == pid && f.ship_count > 1).map(|f| f.id).next();
            let Some(donor) = donor else { break; };
            let ship = engine.fleets[&donor].ships[0].id;
            match engine.handle_split(pid, donor, &[ship]) {
                Ok(_) => made += 1,
                Err(e) => { assert_eq!(e, ErrorCode::FleetLimitReached); break; }
            }
            engine.add_ship_to_homeworld(pid, ShipClass::Scout).unwrap();
        }
        assert!(made >= 2);
        assert!(engine.fleets.values().filter(|f| f.owner_id == pid).count() <= MAX_FLEETS_TOTAL);
    }

    #[test]
    fn merge_needs_a_shared_sector_and_idle_fleets() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Fleeter");
        engine.add_ship_to_homeworld(pid, ShipClass::Scout).unwrap();
        let ship = engine.fleets[&fid].ships[2].id;
        let second = engine.handle_split(pid, fid, &[ship]).unwrap();

        let chain = charted_chain(&mut engine, pid, 1);
        place(&mut engine, second, chain[0], 10.0);
        assert_eq!(engine.handle_merge(pid, fid, second), Err(ErrorCode::NotInSector));

        let home = engine.players[&pid].homeworld;
        place(&mut engine, second, home, 10.0);
        engine.fleets.get_mut(&second).unwrap().state = FleetStatus::Harvesting;
        assert_eq!(engine.handle_merge(pid, fid, second), Err(ErrorCode::OnCooldown));
    }

    #[test]
    fn docking_does_not_merge_fleets() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Docker");
        engine.add_ship_to_homeworld(pid, ShipClass::Scout).unwrap();
        let ship = engine.fleets[&fid].ships[2].id;
        let second = engine.handle_split(pid, fid, &[ship]).unwrap();
        let home = engine.players[&pid].homeworld;
        let chain = charted_chain(&mut engine, pid, 1);

        // The first fleet goes out and comes back.
        place(&mut engine, fid, chain[0], 10.0);
        engine.handle_move(pid, fid, home).unwrap();
        for _ in 0..10 { engine.tick().unwrap(); }
        assert_eq!(engine.fleets[&fid].location, home);
        assert!(engine.fleets.contains_key(&second), "arrival must not absorb the other fleet");
        assert_eq!(engine.fleets[&fid].ship_count, 2);
        assert_eq!(engine.fleets[&second].ship_count, 1);
    }

    #[test]
    fn new_hulls_do_not_join_a_fleet_under_orders() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Orders");
        engine.handle_policy_update(pid, fid, PolicyPreset::PatrolHome, None).unwrap();
        engine.add_ship_to_homeworld(pid, ShipClass::Scout).unwrap();
        assert_eq!(engine.fleets[&fid].ship_count, STARTING_SCOUTS, "the patrol keeps its roster");
        assert_eq!(engine.fleets.values().filter(|f| f.owner_id == pid).count(), 2);
    }

    #[test]
    fn registered_before_the_fix_clamps_on_reload() {
        let path = std::env::temp_dir().join(format!("iac_fuel_reload_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let fid = {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            let mut engine = GameEngine::init(42, db, None).unwrap();
            let (_, fid) = register(&mut engine, "Old");
            let f = engine.fleets.get_mut(&fid).unwrap();
            f.fuel = 50_000.0;
            f.fuel_max = 50_000.0;
            engine.dirty_fleets.insert(fid, ());
            engine.persist_dirty_state().unwrap();
            fid
        };
        let db = Database::init(path.to_str().unwrap()).unwrap();
        let engine = GameEngine::init(42, db, None).unwrap();
        assert!(engine.fleets[&fid].fuel_max < 1000.0);
        assert!(engine.fleets[&fid].fuel <= engine.fleets[&fid].fuel_max);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn split_and_merge_survive_persistence_and_restart() {
        let path = std::env::temp_dir().join(format!("iac_split_persist_test_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let (pid, fid, new_id) = {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            let mut engine = GameEngine::init(42, db, None).unwrap();
            let (pid, fid) = register(&mut engine, "Splitter");
            engine.add_ship_to_homeworld(pid, ShipClass::Hauler).unwrap();
            engine.persist_dirty_state().unwrap();
            let ship = engine.fleets[&fid].ships[2].id;
            let new_id = engine.handle_split(pid, fid, &[ship]).unwrap();
            engine.persist_dirty_state().unwrap();
            engine.flush_persistence().unwrap();
            (pid, fid, new_id)
        };

        let db = Database::init(path.to_str().unwrap()).unwrap();
        let mut engine = GameEngine::init(42, db, None).unwrap();
        assert_eq!(engine.fleets[&fid].ship_count, 2);
        assert_eq!(engine.fleets[&new_id].ship_count, 1);

        engine.handle_merge(pid, fid, new_id).unwrap();
        engine.persist_dirty_state().unwrap();
        engine.flush_persistence().unwrap();
        drop(engine);

        let db = Database::init(path.to_str().unwrap()).unwrap();
        let engine = GameEngine::init(42, db, None).unwrap();
        assert_eq!(engine.fleets[&fid].ship_count, 3);
        assert!(!engine.fleets.contains_key(&new_id));
        let _ = std::fs::remove_file(&path);
    }

    fn temp_world(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("iac_{tag}_{}.db", std::process::id()));
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        path
    }

    fn engine_at(path: &std::path::Path, pace: Option<Pace>) -> Result<GameEngine, Box<dyn std::error::Error>> {
        GameEngine::init(42, Database::init(path.to_str().unwrap()).unwrap(), pace)
    }

    #[test]
    fn a_world_keeps_the_pace_it_was_created_with() {
        let path = temp_world("pace_fixed");
        {
            let mut engine = engine_at(&path, Some(Pace::parse("blitz").unwrap())).unwrap();
            register(&mut engine, "Pacer");
            engine.persist_dirty_state().unwrap();
            engine.flush_persistence().unwrap();
        }
        let engine = engine_at(&path, None).unwrap();
        assert_eq!(engine.pace().value(), 600.0, "no --pace keeps the stored one");
        drop(engine);
        let engine = engine_at(&path, Some(Pace::new(600.0).unwrap())).unwrap();
        assert_eq!(engine.pace().value(), 600.0);
        drop(engine);
        let err = engine_at(&path, Some(Pace::new(10.0).unwrap())).err().expect("a different pace is refused");
        let text = err.to_string();
        assert!(text.contains("600 (blitz)") && text.contains("10 (season)") && text.contains("new --db"), "{text}");
    }

    #[test]
    fn a_database_without_world_meta_is_refused() {
        let path = temp_world("old_world");
        {
            let db = Database::init(path.to_str().unwrap()).unwrap();
            db.save_server_state("world_seed", "42").unwrap();
        }
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("DROP TABLE world_meta").unwrap();
        }
        let err = engine_at(&path, None).err().expect("an old world must not start");
        let text = err.to_string();
        assert!(text.contains("not migrated") && text.contains("new world"), "{text}");
    }

    #[test]
    fn pace_scales_production_and_economy_timers() {
        let slow = test_engine();
        let fast = {
            let db = Database::init(":memory:").unwrap();
            GameEngine::init(42, db, Some(Pace::new(10.0).unwrap())).unwrap()
        };
        let mut a = slow;
        let mut b = fast;
        let (pa, _) = register(&mut a, "Slow");
        let (pb, _) = register(&mut b, "Fast");
        let prod_a = a.players[&pa].production_per_tick(&a.pace());
        let prod_b = b.players[&pb].production_per_tick(&b.pace());
        assert!((prod_b.metal / prod_a.metal - 10.0).abs() < 1e-4);

        a.players.get_mut(&pa).unwrap().resources = Resources { metal: 1e6, crystal: 1e6, deuterium: 1e6 };
        b.players.get_mut(&pb).unwrap().resources = Resources { metal: 1e6, crystal: 1e6, deuterium: 1e6 };
        a.handle_build(pa, BuildingType::MetalMine, false).unwrap();
        b.handle_build(pb, BuildingType::MetalMine, false).unwrap();
        let ticks_a = a.players[&pa].building_queue[0].end_tick - a.current_tick;
        let ticks_b = b.players[&pb].building_queue[0].end_tick - b.current_tick;
        assert_eq!(ticks_b, ticks_a.div_ceil(10));
    }

    fn fund(engine: &mut GameEngine, pid: u64, m: f32, c: f32, d: f32) {
        engine.players.get_mut(&pid).unwrap().resources = Resources { metal: m, crystal: c, deuterium: d };
    }

    fn events_of(engine: &mut GameEngine) -> Vec<EventKind> {
        engine.drain_events().into_iter().map(|e| e.kind).collect()
    }

    #[test]
    fn mines_stop_at_the_cap_and_waste_the_rest() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Hoarder");
        fund(&mut engine, pid, 4999.999, 100.0, 100.0);
        engine.tick().unwrap();
        engine.tick().unwrap();
        let r = engine.players[&pid].resources;
        assert_eq!(r.metal, 5000.0, "metal stops at the no-vault cap");
        assert!(r.crystal > 100.0, "other resources keep filling");
        let state = engine.storage_state(&engine.players[&pid]);
        assert_eq!(state.capped, vec![ResourceKind::Metal]);
        assert_eq!(state.full_in_s.metal, Some(0));
        assert!(state.full_in_s.crystal.unwrap() > 1000);
        assert_eq!(state.full_in_s.deuterium, None, "no synthesizer: not filling");
    }

    #[test]
    fn a_vault_raises_the_cap_by_half_per_level() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Banker");
        engine.players.get_mut(&pid).unwrap().buildings.storage_vault = 2;
        let state = engine.storage_state(&engine.players[&pid]);
        assert_eq!(state.cap.metal, 11250.0);
        assert_eq!(state.protected.metal, 11250.0 * 0.16);
    }

    #[test]
    fn docking_unloads_only_what_fits_and_the_rest_waits() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Hauler");
        fund(&mut engine, pid, 4900.0, 0.0, 0.0);
        engine.fleets.get_mut(&fid).unwrap().cargo = Resources { metal: 300.0, crystal: 50.0, deuterium: 0.0 };
        engine.dock_fleet(fid).unwrap();
        let r = engine.players[&pid].resources;
        assert_eq!(r.metal, 5000.0);
        assert_eq!(r.crystal, 50.0, "crystal had room");
        assert_eq!(engine.fleets[&fid].cargo.metal, 200.0, "the surplus stays aboard");
        assert_eq!(engine.fleets[&fid].cargo.crystal, 0.0);

        // Spending makes room; the next tick unloads without another dock.
        fund(&mut engine, pid, 4000.0, 0.0, 0.0);
        engine.tick().unwrap();
        assert_eq!(engine.fleets[&fid].cargo.metal, 0.0);
        assert!(engine.players[&pid].resources.metal >= 4200.0);
    }

    #[test]
    fn a_payment_above_the_cap_names_the_vault_it_needs() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Planner");
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.research_lab = 4;
            p.buildings.shipyard = 6;
            p.research.frigate_tech = 1;
            p.resources = Resources { metal: 5000.0, crystal: 3500.0, deuterium: 2500.0 };
        }
        // Cruiser Tech costs 8000 metal; the base cap is 5000.
        assert_eq!(engine.handle_research(pid, ResearchType::CruiserTech, false), Err(ErrorCode::StorageTooSmall));
        engine.players.get_mut(&pid).unwrap().buildings.storage_vault = 2;
        engine.players.get_mut(&pid).unwrap().resources = Resources { metal: 8000.0, crystal: 4999.0, deuterium: 2500.0 };
        assert_eq!(engine.handle_research(pid, ResearchType::CruiserTech, false), Ok(()), "fits now; crystal is short, so it waits");
        assert_eq!(engine.players[&pid].research_pending.len(), 1);
    }

    #[test]
    fn storage_alerts_fire_once_per_fill() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Watcher");
        engine.drain_events();
        fund(&mut engine, pid, 4300.0, 0.0, 0.0);
        engine.tick().unwrap();
        let near: Vec<_> = events_of(&mut engine).into_iter()
            .filter(|e| matches!(e, EventKind::StorageNearCap(n) if n.resource == ResourceKind::Metal && n.ratio >= 0.85))
            .collect();
        assert_eq!(near.len(), 1);
        engine.tick().unwrap();
        assert!(events_of(&mut engine).is_empty(), "no repeat while it stays near the cap");

        fund(&mut engine, pid, 5000.0, 0.0, 0.0);
        engine.tick().unwrap();
        let full = events_of(&mut engine);
        assert_eq!(full.iter().filter(|e| matches!(e, EventKind::StorageFull(f) if f.resource == ResourceKind::Metal)).count(), 1);

        fund(&mut engine, pid, 4990.0, 0.0, 0.0);
        engine.tick().unwrap();
        assert!(events_of(&mut engine).is_empty(), "hovering at the cap does not repeat Full");

        fund(&mut engine, pid, 100.0, 0.0, 0.0);
        engine.tick().unwrap();
        events_of(&mut engine);
        fund(&mut engine, pid, 4400.0, 0.0, 0.0);
        engine.tick().unwrap();
        assert_eq!(events_of(&mut engine).iter().filter(|e| matches!(e, EventKind::StorageNearCap(_))).count(), 1, "re-arms after draining");
    }

    #[test]
    fn a_lost_raid_cannot_skim_the_protected_stock() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Fortress");
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.storage_vault = 2;
            p.resources = Resources { metal: 10_000.0, crystal: 0.0, deuterium: 0.0 };
        }
        let protected = scaling::storage_protected(2, &engine.pace()).metal;
        engine.resolve_raid(pid, 1_000_000.0).unwrap();
        let lost = 10_000.0 - engine.players[&pid].resources.metal;
        assert!((lost - (10_000.0 - protected) * RAID_LOSS_CAP_METAL).abs() < 0.5, "lost {lost}");

        fund(&mut engine, pid, protected - 1.0, 0.0, 0.0);
        engine.resolve_raid(pid, 1_000_000.0).unwrap();
        assert_eq!(engine.players[&pid].resources.metal, protected - 1.0, "below the protected line nothing is skimmed");
    }

    fn fast_engine() -> GameEngine {
        let db = Database::init(":memory:").unwrap();
        GameEngine::init(42, db, Some(Pace::new(100.0).unwrap())).unwrap()
    }

    fn rich(engine: &mut GameEngine, pid: u64) {
        let p = engine.players.get_mut(&pid).unwrap();
        p.resources = Resources { metal: 4_000.0, crystal: 3_000.0, deuterium: 2_000.0 };
    }

    fn running_id(engine: &GameEngine, pid: u64, queue: iac_shared::protocol::QueueType) -> u64 {
        use iac_shared::protocol::QueueType;
        let p = &engine.players[&pid];
        match queue {
            QueueType::Building => p.building_queue[0].id,
            QueueType::Research => p.research_queue.as_ref().unwrap().id,
            QueueType::Ship => p.ship_queue.as_ref().unwrap().id,
        }
    }

    fn waiting_id(engine: &GameEngine, pid: u64, queue: iac_shared::protocol::QueueType, nth: usize) -> u64 {
        use iac_shared::protocol::QueueType;
        let p = &engine.players[&pid];
        match queue {
            QueueType::Building => p.building_pending[nth].id,
            QueueType::Research => p.research_pending[nth].id,
            QueueType::Ship => p.ship_pending[nth].id,
        }
    }

    fn tick_n(engine: &mut GameEngine, n: u32) {
        for _ in 0..n {
            engine.tick().unwrap();
        }
    }

    #[test]
    fn a_busy_slot_queues_the_next_order_without_charging_for_it() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Queuer");
        rich(&mut engine, pid);
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        let after_first = engine.players[&pid].resources;
        engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        let p = &engine.players[&pid];
        assert_eq!(p.building_queue.len(), 1);
        assert_eq!(p.building_pending.len(), 1);
        assert_eq!(p.resources, after_first, "waiting items cost nothing yet");

        engine.handle_build(pid, BuildingType::DeuteriumSynthesizer, false).unwrap();
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        assert_eq!(engine.players[&pid].building_pending.len(), 3, "one running plus three waiting");
        assert_eq!(engine.handle_build(pid, BuildingType::MetalMine, false), Err(ErrorCode::QueueFull), "slots + 3");
    }

    #[test]
    fn waiting_items_start_as_slots_and_funds_allow() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Chain");
        rich(&mut engine, pid);
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        assert_eq!(engine.players[&pid].building_pending[0].target_level, 3, "a second order builds on the first");
        engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        tick_n(&mut engine, 60);
        let p = &engine.players[&pid];
        assert_eq!(p.buildings.metal_mine, 3);
        assert_eq!(p.buildings.crystal_mine, 2);
        assert!(p.building_queue.is_empty() && p.building_pending.is_empty());
    }

    fn queue_events(engine: &mut GameEngine) -> Vec<iac_shared::protocol::QueueEvent> {
        engine.drain_events().into_iter().filter_map(|e| match e.kind {
            EventKind::Queue(q) => Some(q),
            _ => None,
        }).collect()
    }

    fn broke(engine: &mut GameEngine, pid: u64) {
        engine.players.get_mut(&pid).unwrap().resources = Resources::default();
    }

    #[test]
    fn an_unaffordable_order_waits_even_with_a_free_slot() {
        use iac_shared::protocol::{QueueAction, WaitReason};
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Waiter");
        broke(&mut engine, pid);
        engine.drain_events();
        assert_eq!(engine.handle_build(pid, BuildingType::MetalMine, false), Ok(()));
        let p = &engine.players[&pid];
        assert!(p.building_queue.is_empty(), "nothing started");
        assert_eq!(p.building_pending.len(), 1, "but the order is in line");

        let ev = queue_events(&mut engine);
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].action, QueueAction::Waiting);
        assert_eq!(ev[0].waiting_on, Some(WaitReason::Resources));
        assert_eq!(ev[0].waiting_for, Some(scaling::building_cost(BuildingType::MetalMine, 2)), "the exact shortfall");
        assert!(ev[0].start_in.is_some_and(|t| t > 0));

        let cost = scaling::building_cost(BuildingType::MetalMine, 2);
        engine.players.get_mut(&pid).unwrap().resources = cost;
        engine.tick().unwrap();
        let p = &engine.players[&pid];
        assert_eq!(p.building_queue.len(), 1, "it started once it was affordable");
        assert!(p.resources.metal < 5.0, "and paid then, not before");
        let ev = queue_events(&mut engine);
        assert!(ev.iter().any(|e| e.action == QueueAction::Started && e.paid == cost));
    }

    #[test]
    fn research_and_shipyard_orders_wait_for_money_too() {
        use iac_shared::protocol::WaitReason;
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Everything");
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.research_lab = 1;
            p.buildings.shipyard = 1;
        }
        broke(&mut engine, pid);
        assert_eq!(engine.handle_research(pid, ResearchType::CorvetteTech, false), Ok(()));
        assert_eq!(engine.handle_build_ship(pid, ShipClass::Scout, 2, false), Ok(()));
        assert_eq!(engine.handle_build_defence(pid, DefenceKind::PulseTurret, 1, false), Err(ErrorCode::DefenceLocked), "locked orders still refuse");
        let p = &engine.players[&pid];
        assert!(p.research_queue.is_none() && p.ship_queue.is_none());
        assert_eq!((p.research_pending.len(), p.ship_pending.len()), (1, 1));
        let waits = crate::queue::project(p, engine.current_tick, &engine.world.pace);
        assert_eq!(waits.research[0].reason, WaitReason::Resources);
        assert_eq!(waits.ships[0].reason, WaitReason::Resources);
    }

    #[test]
    fn a_free_slot_starts_a_later_order_that_can_pay() {
        use iac_shared::protocol::{QueueAction, WaitReason};
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Slots");
        engine.players.get_mut(&pid).unwrap().research.modular_fabrication = 2;
        broke(&mut engine, pid);
        engine.handle_build(pid, BuildingType::DeuteriumSynthesizer, false).unwrap();
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        assert!(engine.players[&pid].building_queue.is_empty());
        let first = waiting_id(&engine, pid, iac_shared::protocol::QueueType::Building, 0);
        engine.drain_events();

        engine.players.get_mut(&pid).unwrap().resources = Resources { metal: 100.0, crystal: 100.0, deuterium: 0.0 };
        engine.advance_queues(pid);
        let p = &engine.players[&pid];
        let running: Vec<_> = p.building_queue.iter().map(|q| q.building_type).collect();
        assert_eq!(running, vec![BuildingType::MetalMine], "the cheap order took the free slot; the rest cannot pay");
        assert_eq!(p.building_pending.len(), 2);
        assert_eq!(p.building_pending[0].id, first, "the unaffordable head keeps its place");
        let waits = crate::queue::project(p, engine.current_tick, &engine.world.pace);
        assert_eq!(waits.buildings[0].reason, WaitReason::Resources);
        let ev = queue_events(&mut engine);
        assert!(ev.iter().any(|e| e.action == QueueAction::Started && e.item == "Metal Mine Lv.2"));

        engine.players.get_mut(&pid).unwrap().resources = Resources { metal: 150.0, crystal: 100.0, deuterium: 0.0 };
        engine.advance_queues(pid);
        let p = &engine.players[&pid];
        assert_eq!(p.building_queue.len(), 2, "the head starts as soon as it can pay");
        assert_eq!(p.building_queue[1].id, first, "and keeps the id it was queued with");
    }

    #[test]
    fn research_queues_behind_a_prerequisite_that_is_itself_queued() {
        use iac_shared::protocol::WaitReason;
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Chain");
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.research_lab = 2;
            p.buildings.crystal_mine = 2;
        }
        rich(&mut engine, pid);
        assert_eq!(engine.handle_research(pid, ResearchType::AdvancedShields, false), Err(ErrorCode::PrerequisitesNotMet));
        engine.handle_build(pid, BuildingType::ResearchLab, false).unwrap();
        assert_eq!(engine.handle_research(pid, ResearchType::AdvancedShields, false), Ok(()), "Lab 3 is under way");
        let p = &engine.players[&pid];
        assert_eq!(p.research_pending.len(), 1);
        let waits = crate::queue::project(p, engine.current_tick, &engine.world.pace);
        assert_eq!(waits.research[0].reason, WaitReason::Prerequisite);
        tick_n(&mut engine, 400);
        assert_eq!(engine.players[&pid].buildings.research_lab, 3);
        assert!(engine.players[&pid].research_queue.is_some() || engine.players[&pid].research.advanced_shields == 1);
    }

    #[test]
    fn a_tech_that_just_started_can_be_queued_for_its_next_level() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Next");
        engine.players.get_mut(&pid).unwrap().buildings.research_lab = 2;
        rich(&mut engine, pid);
        engine.handle_research(pid, ResearchType::Navigation, false).unwrap();
        assert_eq!(engine.players[&pid].research_queue.as_ref().unwrap().target_level, 1);
        engine.handle_research(pid, ResearchType::Navigation, false).unwrap();
        engine.handle_research(pid, ResearchType::Navigation, false).unwrap();
        let p = &engine.players[&pid];
        assert_eq!(p.research_pending.iter().map(|q| q.target_level).collect::<Vec<_>>(), vec![2, 3]);
    }

    #[test]
    fn cancel_by_id_is_exact_and_refuses_the_wrong_queue() {
        use iac_shared::protocol::QueueType;
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Ids");
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.research_lab = 1;
            p.buildings.shipyard = 1;
        }
        rich(&mut engine, pid);
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        engine.handle_research(pid, ResearchType::CorvetteTech, false).unwrap();
        engine.handle_build_ship(pid, ShipClass::Scout, 2, false).unwrap();
        let waiting_building = waiting_id(&engine, pid, QueueType::Building, 0);
        let running_ship = running_id(&engine, pid, QueueType::Ship);

        assert_eq!(engine.handle_cancel_queued(pid, QueueType::Ship, waiting_building), Err(ErrorCode::InvalidTarget), "a building id in the ship queue");
        assert_eq!(engine.handle_cancel_queued(pid, QueueType::Building, running_ship), Err(ErrorCode::InvalidTarget));
        assert_eq!(engine.handle_cancel_build(pid, QueueType::Building, waiting_building), Err(ErrorCode::InvalidTarget), "waiting is not running");
        assert_eq!(engine.handle_cancel_queued(pid, QueueType::Ship, running_ship), Err(ErrorCode::InvalidTarget), "running is not waiting");
        assert_eq!(engine.players[&pid].building_pending.len(), 1);
        assert!(engine.players[&pid].ship_queue.is_some());

        // The line moves under the caller: the id still names the same order.
        tick_n(&mut engine, 2000);
        assert_eq!(engine.handle_cancel_queued(pid, QueueType::Building, waiting_building), Err(ErrorCode::InvalidTarget), "it started meanwhile");
        let ev = queue_events(&mut engine);
        assert!(ev.iter().any(|e| e.id == waiting_building && e.item == "Crystal Mine Lv.2"));
    }

    #[test]
    fn ship_batches_pay_and_start_one_unit_at_a_time() {
        use iac_shared::protocol::{QueueAction, QueueType, WaitReason};
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Batch");
        engine.players.get_mut(&pid).unwrap().buildings.shipyard = 1;
        let unit = ShipClass::Scout.build_cost();
        engine.players.get_mut(&pid).unwrap().resources = unit.scale(2.0);
        engine.drain_events();
        engine.handle_build_ship(pid, ShipClass::Scout, 5, false).unwrap();
        let id = running_id(&engine, pid, QueueType::Ship);
        let p = &engine.players[&pid];
        assert_eq!(p.resources, unit, "only the first unit was paid");
        let ev = queue_events(&mut engine);
        assert_eq!(ev.iter().filter(|e| e.action == QueueAction::Started).count(), 1);
        assert_eq!(ev[0].paid, unit);

        // The first unit finishes and the second is already paid for.
        let now = engine.current_tick;
        engine.players.get_mut(&pid).unwrap().ship_queue.as_mut().unwrap().end_tick = now;
        engine.process_build_queues().unwrap();
        let q = engine.players[&pid].ship_queue.as_ref().unwrap();
        assert_eq!((q.id, q.built), (id, 1), "the batch keeps its id across units");
        assert_eq!(engine.players[&pid].resources, Resources::default());

        // Broke between units: the batch returns to the line with its progress.
        engine.players.get_mut(&pid).unwrap().ship_queue.as_mut().unwrap().end_tick = now;
        engine.process_build_queues().unwrap();
        let p = &engine.players[&pid];
        assert!(p.ship_queue.is_none());
        assert_eq!(p.ship_pending.len(), 1);
        assert_eq!((p.ship_pending[0].id, p.ship_pending[0].built), (id, 2));
        let waits = crate::queue::project(p, engine.current_tick, &engine.world.pace);
        assert_eq!(waits.ships[0].reason, WaitReason::Resources);
        assert_eq!(waits.ships[0].short, Some(unit), "the shortfall is the next unit, not the batch");

        engine.players.get_mut(&pid).unwrap().resources = unit;
        engine.advance_queues(pid);
        assert!(engine.players[&pid].ship_queue.is_some(), "paying for one unit resumes the batch");
    }

    #[test]
    fn a_reserving_order_holds_the_line_until_it_can_pay() {
        use iac_shared::protocol::WaitReason;
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Line");
        engine.players.get_mut(&pid).unwrap().research.modular_fabrication = 2;
        broke(&mut engine, pid);
        // Deuterium Synthesizer 1 costs 150 metal 50 crystal; Metal Mine 2 costs 90 / 22.
        engine.handle_build(pid, BuildingType::DeuteriumSynthesizer, true).unwrap();
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();

        engine.players.get_mut(&pid).unwrap().resources = Resources { metal: 100.0, crystal: 100.0, deuterium: 0.0 };
        engine.advance_queues(pid);
        let p = &engine.players[&pid];
        assert!(p.building_queue.is_empty(), "the cheap order may not jump a reserving one");
        assert_eq!(p.building_pending.len(), 2);
        let waits = crate::queue::project(p, engine.current_tick, &engine.world.pace);
        assert_eq!(waits.buildings[0].reason, WaitReason::Resources);
        assert_eq!(waits.buildings[1].reason, WaitReason::Order, "affordable, but held behind the reserving first");
        assert!(waits.buildings[1].start_in >= waits.buildings[0].start_in);

        engine.players.get_mut(&pid).unwrap().resources = Resources { metal: 150.0, crystal: 100.0, deuterium: 0.0 };
        engine.advance_queues(pid);
        let p = &engine.players[&pid];
        assert_eq!(p.building_queue.len(), 1);
        assert_eq!(p.building_queue[0].building_type, BuildingType::DeuteriumSynthesizer);
        assert_eq!(p.resources.metal, 0.0, "the first order paid in full when it started");
        assert_eq!(p.building_pending.len(), 1, "the second waits for its own money");

        engine.players.get_mut(&pid).unwrap().resources = Resources { metal: 90.0, crystal: 22.0, deuterium: 0.0 };
        engine.advance_queues(pid);
        assert_eq!(engine.players[&pid].building_queue.len(), 2);
        assert!(engine.players[&pid].building_pending.is_empty());
    }

    #[test]
    fn a_prerequisite_wait_does_not_hold_the_line() {
        use iac_shared::protocol::WaitReason;
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Skipper");
        engine.players.get_mut(&pid).unwrap().research.modular_fabrication = 1;
        rich(&mut engine, pid);
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        // Shipyard needs Metal Mine 2, which is still building.
        engine.handle_build(pid, BuildingType::Shipyard, false).unwrap();
        engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        let p = &engine.players[&pid];
        assert_eq!(p.building_queue.len(), 2, "the crystal mine ran past the shipyard that waits on a prerequisite");
        let waits = crate::queue::project(p, engine.current_tick, &engine.world.pace);
        assert_eq!(waits.buildings[0].reason, WaitReason::Prerequisite);
        let metal_done = p.building_queue.iter().find(|q| q.building_type == BuildingType::MetalMine).unwrap().end_tick;
        assert!(waits.buildings[0].start_in.is_some_and(|t| t >= metal_done - engine.current_tick), "it starts once the mine lands");
    }

    #[test]
    fn cancelling_says_what_was_cancelled_and_refunded() {
        use iac_shared::protocol::{QueueAction, QueueType};
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Canceller");
        rich(&mut engine, pid);
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        engine.drain_events();

        let waiting = waiting_id(&engine, pid, QueueType::Building, 0);
        engine.handle_cancel_queued(pid, QueueType::Building, waiting).unwrap();
        let ev = queue_events(&mut engine);
        assert_eq!(ev.len(), 1);
        assert_eq!((ev[0].action, ev[0].item.as_str(), ev[0].id), (QueueAction::Cancelled, "Crystal Mine Lv.2", waiting));
        assert_eq!(ev[0].refunded, Resources::default(), "a waiting order paid nothing");

        let before = engine.players[&pid].resources;
        let running = running_id(&engine, pid, QueueType::Building);
        engine.handle_cancel_build(pid, QueueType::Building, running).unwrap();
        let ev = queue_events(&mut engine);
        let cost = scaling::building_cost(BuildingType::MetalMine, 2);
        assert_eq!((ev[0].action, ev[0].item.as_str()), (QueueAction::Cancelled, "Metal Mine Lv.2"));
        assert_eq!(ev[0].refunded, cost.scale(CANCEL_REFUND_FRACTION));
        assert_eq!(engine.players[&pid].resources, before.add(ev[0].refunded));
    }

    #[test]
    fn a_full_line_refuses_and_says_how_full() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Crowd");
        broke(&mut engine, pid);
        engine.players.get_mut(&pid).unwrap().research.modular_fabrication = 2;
        // Three slots plus three waiting: six orders fit even when none can start.
        for _ in 0..6 {
            engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        }
        assert_eq!(engine.handle_build(pid, BuildingType::MetalMine, false), Err(ErrorCode::QueueFull));
        assert_eq!(engine.players[&pid].building_pending.len(), 6);
    }

    #[test]
    fn queued_items_can_depend_on_the_ones_ahead() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Planner");
        rich(&mut engine, pid);
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::Shipyard, false).unwrap();
        assert_eq!(engine.players[&pid].building_pending.len(), 1, "Shipyard waits for Metal Mine 2");
        tick_n(&mut engine, 60);
        assert_eq!(engine.players[&pid].buildings.shipyard, 1);
        assert_eq!(engine.handle_build(pid, BuildingType::DefenseGrid, false), Err(ErrorCode::PrerequisitesNotMet));
    }

    #[test]
    fn modular_fabrication_adds_a_slot_and_starts_waiting_work() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Twin");
        rich(&mut engine, pid);
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.research_lab = 4;
            p.research.modular_fabrication = 1;
        }
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        let p = &engine.players[&pid];
        assert_eq!((p.building_queue.len(), p.building_pending.len()), (2, 0), "two slots, two builds");
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        assert_eq!(engine.players[&pid].building_pending.len(), 1, "same type waits for its predecessor");
    }

    #[test]
    fn completing_modular_fabrication_unblocks_the_queue() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Unlock");
        rich(&mut engine, pid);
        engine.players.get_mut(&pid).unwrap().buildings.research_lab = 4;
        engine.handle_research(pid, ResearchType::ModularFabrication, false).unwrap();
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
        assert_eq!(engine.players[&pid].building_queue.len(), 1);
        tick_n(&mut engine, 400);
        let p = &engine.players[&pid];
        assert_eq!(p.research.modular_fabrication, 1);
        assert_eq!(p.buildings.crystal_mine, 2);
    }

    #[test]
    fn cancelling_refunds_half_when_started_and_nothing_when_waiting() {
        use iac_shared::protocol::QueueType;
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Canceller");
        rich(&mut engine, pid);
        let start = engine.players[&pid].resources;
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        let paid = scaling::building_cost(BuildingType::MetalMine, 2);
        let waiting = waiting_id(&engine, pid, QueueType::Building, 0);
        engine.handle_cancel_queued(pid, QueueType::Building, waiting).unwrap();
        assert_eq!(engine.players[&pid].building_pending[0].target_level, 3, "the later order moves up");
        assert_eq!(engine.players[&pid].resources, start.sub(paid));

        let running = running_id(&engine, pid, QueueType::Building);
        engine.handle_cancel_build(pid, QueueType::Building, running).unwrap();
        assert_eq!(engine.players[&pid].resources, start.sub(paid.scale(1.5)), "half back, then the freed slot is paid for again");
        let p = &engine.players[&pid];
        assert_eq!(p.building_queue.len(), 1, "the waiting item took the freed slot");
        assert_eq!(p.building_queue[0].target_level, 2, "and renumbered to follow level 1");
        assert!(engine.handle_cancel_queued(pid, QueueType::Building, 5).is_err());
        assert_eq!(engine.handle_cancel_build(pid, QueueType::Ship, running), Err(ErrorCode::InvalidTarget));
    }

    #[test]
    fn fabricator_and_lab_shorten_their_queues() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Speedy");
        rich(&mut engine, pid);
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.metal_mine = 9;
            p.buildings.research_lab = 1;
        }
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        let plain = engine.players[&pid].building_queue[0].end_tick - engine.current_tick;
        let running = running_id(&engine, pid, iac_shared::protocol::QueueType::Building);
        engine.handle_cancel_build(pid, iac_shared::protocol::QueueType::Building, running).unwrap();
        engine.players.get_mut(&pid).unwrap().buildings.fabricator = 8;
        rich(&mut engine, pid);
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        let fast = engine.players[&pid].building_queue[0].end_tick - engine.current_tick;
        assert!(fast * 2 <= plain + 1, "fabricator 8 is 2.2x faster: {plain} vs {fast}");

        rich(&mut engine, pid);
        engine.handle_research(pid, ResearchType::Navigation, false).unwrap_err();
        engine.players.get_mut(&pid).unwrap().buildings.research_lab = 2;
        engine.handle_research(pid, ResearchType::Navigation, false).unwrap();
        let lab2 = engine.players[&pid].research_queue.as_ref().unwrap().end_tick - engine.current_tick;
        let base = scaling::research_time(ResearchType::Navigation, 1, 0, &engine.pace());
        assert!(lab2 < base, "lab 2 shortens research: {lab2} vs {base}");
    }

    #[test]
    fn queues_survive_a_restart() {
        let path = temp_world("queue_restart");
        let pid = {
            let mut engine = engine_at(&path, Some(Pace::new(100.0).unwrap())).unwrap();
            let (pid, _) = register(&mut engine, "Durable");
            rich(&mut engine, pid);
            engine.players.get_mut(&pid).unwrap().buildings.shipyard = 1;
            engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
            engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
            engine.handle_build(pid, BuildingType::CrystalMine, false).unwrap();
            engine.handle_build_ship(pid, ShipClass::Scout, 2, false).unwrap();
            engine.handle_build_ship(pid, ShipClass::Scout, 1, false).unwrap();
            engine.persist_dirty_state().unwrap();
            engine.flush_persistence().unwrap();
            pid
        };
        let engine = engine_at(&path, None).unwrap();
        let p = &engine.players[&pid];
        assert_eq!(p.building_queue.len(), 1);
        assert_eq!(p.building_pending.iter().map(|q| (q.building_type, q.target_level)).collect::<Vec<_>>(),
            vec![(BuildingType::MetalMine, 3), (BuildingType::CrystalMine, 2)]);
        assert_eq!(p.ship_queue.as_ref().unwrap().count, 2);
        assert_eq!(p.ship_pending.len(), 1);
    }

    #[test]
    fn ship_orders_queue_and_start_in_turn() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Yard");
        rich(&mut engine, pid);
        engine.players.get_mut(&pid).unwrap().buildings.shipyard = 1;
        engine.handle_build_ship(pid, ShipClass::Scout, 1, false).unwrap();
        engine.handle_build_ship(pid, ShipClass::Scout, 1, false).unwrap();
        assert_eq!(engine.players[&pid].ship_pending.len(), 1);
        assert_eq!(engine.handle_build_ship(pid, ShipClass::Scout, 0, false), Err(ErrorCode::InvalidCommand));
        let docked_before: usize = engine.fleets.values().map(|f| f.ship_count).sum();
        tick_n(&mut engine, 200);
        let docked_after: usize = engine.fleets.values().map(|f| f.ship_count).sum();
        assert_eq!(docked_after - docked_before, 2);
        assert!(engine.players[&pid].ship_pending.is_empty() && engine.players[&pid].ship_queue.is_none());
    }

    #[test]
    fn docking_pays_deuterium_for_fuel() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Tanker");
        fund(&mut engine, pid, 0.0, 0.0, 100.0);
        engine.fleets.get_mut(&fid).unwrap().fuel = 20.0;
        let missing = engine.fleets[&fid].fuel_max - 20.0;
        engine.dock_fleet(fid).unwrap();
        let paid = 100.0 - engine.players[&pid].resources.deuterium;
        assert!((paid - missing * FUEL_DEUT_PER_UNIT).abs() < 1e-3, "paid {paid} for {missing} fuel");
        assert_eq!(engine.fleets[&fid].fuel, engine.fleets[&fid].fuel_max);
    }

    #[test]
    fn a_dry_stockpile_refuels_as_far_as_it_can_and_the_rest_follows() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Thirsty");
        fund(&mut engine, pid, 0.0, 0.0, 0.8);
        engine.fleets.get_mut(&fid).unwrap().fuel = 0.0;
        engine.dock_fleet(fid).unwrap();
        assert!((engine.fleets[&fid].fuel - 10.0).abs() < 1e-3, "0.8 deuterium buys 10 fuel");
        assert!(engine.players[&pid].resources.deuterium < 1e-4);

        // Production keeps arriving; a docked fleet drinks it a little each tick.
        engine.players.get_mut(&pid).unwrap().buildings.deuterium_synthesizer = 5;
        tick_n(&mut engine, 5);
        assert!(engine.fleets[&fid].fuel > 10.0);
    }

    #[test]
    fn a_new_hull_brings_its_own_tank_not_a_free_refill() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Builder");
        engine.fleets.get_mut(&fid).unwrap().fuel = 30.0;
        engine.add_ship_to_homeworld(pid, ShipClass::Hauler).unwrap();
        let f = &engine.fleets[&fid];
        assert_eq!(f.ship_count, 3);
        let hauler_tank = ShipClass::Hauler.base_stats().fuel as f32;
        assert_eq!(f.fuel, 30.0 + hauler_tank);
        assert!(f.fuel < f.fuel_max);
    }

    #[test]
    fn the_fuel_depot_raises_the_tank_by_a_quarter_per_level() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Depot");
        let before = engine.fleets[&fid].fuel_max;
        engine.players.get_mut(&pid).unwrap().buildings.fuel_depot = 2;
        engine.dock_fleet(fid).unwrap();
        assert_eq!(engine.fleets[&fid].fuel_max, before * 1.5);
    }

    #[test]
    fn preview_move_reports_fuel_and_the_way_home() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Scout");
        let chain = charted_chain(&mut engine, pid, 3);
        let cost = engine.hop_fuel_cost(&engine.fleets[&fid]);
        place(&mut engine, fid, chain[0], 3.5 * cost);

        let p = engine.preview_move(pid, fid, chain[1]).unwrap();
        assert_eq!(p.fuel_cost, cost);
        assert_eq!(p.fuel_after, 2.5 * cost);
        assert!(p.can_jump);
        assert_eq!(p.hops_home, 2);
        assert_eq!(p.fuel_to_return, 2.0 * cost);
        assert!(p.can_return);
        assert_eq!(engine.fleets[&fid].location, chain[0], "a preview moves nothing");
        assert_eq!(engine.fleets[&fid].fuel, 3.5 * cost);

        place(&mut engine, fid, chain[0], 2.5 * cost);
        let p = engine.preview_move(pid, fid, chain[1]).unwrap();
        assert!(p.can_jump && !p.can_return, "1.5 jumps left cannot pay two hops home");

        place(&mut engine, fid, chain[0], 0.5 * cost);
        let p = engine.preview_move(pid, fid, chain[1]).unwrap();
        assert!(!p.can_jump);
        assert_eq!(p.fuel_after, 0.0);

        assert_eq!(engine.preview_move(pid, fid, chain[2]), Err(ErrorCode::NoConnection), "only adjacent sectors");
    }

    /// A sector and a connected neighbour whose template holds a group.
    fn lair_next_to(engine: &GameEngine) -> (Hex, Hex) {
        for q in -12i16..12 {
            for r in -12i16..12 {
                let from = Hex { q, r };
                for &to in engine.world_gen.connected_neighbors(from).slice() {
                    let lair = engine.world_gen.generate_sector(to).npc_template.is_some();
                    if lair && to.dist_from_origin() >= 3 && !engine.players.values().any(|p| p.homeworld == to) {
                        return (from, to);
                    }
                }
            }
        }
        panic!("no lair in the window");
    }

    #[test]
    fn preview_move_rates_the_destination_and_the_ratio() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Rater");
        let (from, to) = lair_next_to(&engine);
        place(&mut engine, fid, from, 1000.0);

        let unseen = engine.preview_move(pid, fid, to).unwrap();
        assert_eq!(unseen.threat.basis, ThreatBasis::Estimate, "an unseen sector is rated by distance alone");
        let ring = scaling::npc_power(to.dist_from_origin());
        assert_eq!(unseen.threat.est_power, ring);
        assert_eq!(unseen.threat.rating, scaling::threat_rating(ring));

        engine.handle_scan(pid, fid).unwrap();
        let seen = engine.preview_move(pid, fid, to).unwrap();
        let tmpl = engine.world_gen.generate_sector(to).npc_template.unwrap();
        assert_eq!(seen.threat.basis, ThreatBasis::Observed);
        let want = tmpl.power() * behavior_threat_bonus(tmpl.behavior);
        assert!((seen.threat.est_power - want).abs() < 1e-3);
        assert_eq!(seen.threat.rating, scaling::threat_rating(want));
        assert_eq!(seen.fleet_power, fleet_power(&engine.fleets[&fid]));
        assert!((seen.ratio - seen.fleet_power / seen.threat.est_power).abs() < 1e-5);
        assert_eq!(seen.label, scaling::ratio_label(seen.ratio));

        for ship in &mut engine.fleets.get_mut(&fid).unwrap().ships[..] {
            ship.weapon_power = 5000.0;
        }
        assert_eq!(engine.preview_move(pid, fid, to).unwrap().label, scaling::RatioLabel::Safe);
        let f = engine.fleets.get_mut(&fid).unwrap();
        for ship in &mut f.ships[..] {
            ship.weapon_power = 0.0;
            ship.hull = 1.0;
            ship.shield = 0.0;
        }
        assert_eq!(engine.preview_move(pid, fid, to).unwrap().label, scaling::RatioLabel::Deadly);
    }

    #[test]
    fn a_sector_is_rated_by_its_group_else_by_its_ring() {
        let mut engine = test_engine();
        let (_, to) = lair_next_to(&engine);
        let tmpl = engine.world_gen.generate_sector(to).npc_template.unwrap();
        let before = engine.sector_threat(to);
        assert_eq!(before.basis, ThreatBasis::Observed, "the template group counts before it spawns");

        engine.ensure_override(to.to_key()).npc_cleared_tick = Some(0);
        let cleared = engine.sector_threat(to);
        assert_eq!(cleared.basis, ThreatBasis::Template);
        assert_eq!(cleared.est_power, scaling::npc_power(to.dist_from_origin()));

        engine.ensure_override(to.to_key()).npc_cleared_tick = None;
        let npc = engine.spawn_npc_fleet(to, tmpl.clone(), template_npc_id(to)).unwrap();
        let live = engine.sector_threat(to);
        assert!((live.est_power - npc.power * behavior_threat_bonus(tmpl.behavior)).abs() < 1e-3);

        engine.npc_fleets.get_mut(&npc.id).unwrap().ships[0].hull = 0.0;
        let hurt = engine.sector_threat(to);
        assert!(hurt.est_power <= live.est_power);
    }

    #[test]
    fn a_spawned_group_has_the_templates_power_and_follows_the_ring() {
        let mut engine = test_engine();
        let mut last = 0.0;
        for dist in [1u16, 4, 8, 12, 16, 20, 25, 30] {
            let (ship_class, count, stat_multiplier) = scaling::npc_composition(dist);
            let tmpl = iac_shared::world::NpcTemplate {
                ship_class, count, stat_multiplier,
                behavior: iac_shared::world::NpcBehaviorType::Patrol,
            };
            let npc = engine.spawn_npc_fleet(Hex { q: dist as i16, r: 0 }, tmpl.clone(), 9_000 + u64::from(dist)).unwrap();
            assert!((npc.power - tmpl.power()).abs() < 0.01 * tmpl.power(), "d={dist}");
            assert!(npc.power > last, "power never drops going out: d={dist}");
            last = npc.power;
        }
    }

    #[test]
    fn scans_report_the_threat_of_what_they_reveal() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Scanner");
        let (from, _) = lair_next_to(&engine);
        place(&mut engine, fid, from, 1000.0);
        engine.handle_scan(pid, fid).unwrap();
        let scan = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::ScanCompleted(s) => Some(s),
            _ => None,
        }).unwrap();
        assert_eq!(scan.threats.len(), scan.sectors_revealed as usize);
        for t in &scan.threats {
            assert_eq!(t.threat, engine.sector_threat(t.sector));
        }
        for signal in &scan.signals {
            assert_eq!(signal.threat_band, scaling::threat_band(engine.sector_threat(signal.sector).rating));
            assert!((1..=3).contains(&signal.threat_band));
        }
    }

    #[test]
    fn a_world_from_another_generator_is_refused() {
        let path = temp_world("old_worldgen");
        drop(engine_at(&path, None).unwrap());
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute("UPDATE world_meta SET worldgen_version = 1", []).unwrap();
        }
        let err = engine_at(&path, None).err().expect("a world from the old generator must not start");
        assert!(err.to_string().contains("worldgen version 1"), "{err}");
    }

    fn engine_at_pace(pace: f64) -> GameEngine {
        let db = Database::init(":memory:").unwrap();
        GameEngine::init(42, db, Some(Pace::new(pace).unwrap())).unwrap()
    }

    /// A sector far enough out for the ring factor to bite, with rich metal.
    fn metal_tile(engine: &GameEngine) -> Hex {
        find_sector_beyond(engine, 6, |m, _, _| m == Density::Pristine)
    }

    fn strip_metal(engine: &mut GameEngine, at: Hex) {
        let ov = engine.ensure_override(at.to_key());
        ov.metal_density = Some(Density::None);
        ov.metal_harvested = 0.0;
    }

    #[test]
    fn a_stripped_tile_refills_on_the_spec_schedule_scaled_by_pace() {
        let mut engine = engine_at_pace(8000.0);
        let at = metal_tile(&engine);
        let dist = at.dist_from_origin();
        strip_metal(&mut engine, at);
        let want = (f64::from(scaling::ore_regen_hours(dist)) * 3600.0 / 8000.0).ceil() as u32;
        assert!(want > 50, "the test pace leaves enough ticks to measure: {want}");

        let first = engine.ore_reserve_at(at).unwrap().metal;
        assert_eq!(first.units, 0.0);
        assert!((first.refills_in_s.unwrap() as i64 - i64::from(want)).abs() <= 1, "{:?} vs {want}", first.refills_in_s);

        for _ in 0..want - 2 {
            engine.process_sector_regen().unwrap();
        }
        let almost = engine.ore_reserve_at(at).unwrap().metal;
        assert!(almost.units < almost.max_units && almost.refills_in_s.is_some(), "{almost:?}");
        for _ in 0..3 {
            engine.process_sector_regen().unwrap();
        }
        let full = engine.ore_reserve_at(at).unwrap().metal;
        assert_eq!(full.units, full.max_units);
        assert_eq!(full.refills_in_s, None);
        let ov = &engine.sector_overrides[&at.to_key()];
        assert_eq!((ov.metal_density, ov.metal_harvested), (None, 0.0), "back at the generated density, never above");
    }

    #[test]
    fn regeneration_runs_ten_times_faster_at_ten_times_the_pace() {
        let refilled = |pace: f64| {
            let mut engine = engine_at_pace(pace);
            let at = metal_tile(&engine);
            strip_metal(&mut engine, at);
            for _ in 0..2000 {
                engine.process_sector_regen().unwrap();
            }
            engine.ore_reserve_at(at).unwrap().metal.units
        };
        let (slow, fast) = (refilled(1.0), refilled(10.0));
        assert!(slow > 0.0 && (fast / slow - 10.0).abs() < 0.5, "{slow} {fast}");
    }

    #[test]
    fn a_tile_does_not_refill_under_a_fleet() {
        let mut engine = engine_at_pace(8000.0);
        let (_, fid) = register(&mut engine, "Squatter");
        let at = metal_tile(&engine);
        strip_metal(&mut engine, at);
        engine.fleets.get_mut(&fid).unwrap().location = at;
        for _ in 0..50 {
            engine.process_sector_regen().unwrap();
        }
        assert_eq!(engine.ore_reserve_at(at).unwrap().metal.units, 0.0);
        engine.fleets.get_mut(&fid).unwrap().location = Hex { q: 0, r: 0 };
        engine.process_sector_regen().unwrap();
        assert!(engine.ore_reserve_at(at).unwrap().metal.units > 0.0);
    }

    #[test]
    fn harvesting_empties_a_tile_by_its_ring_reserve_and_research_only_raises_the_yield() {
        let mine = |level: u8| {
            let mut engine = test_engine();
            let (pid, fid) = register(&mut engine, "Miner");
            let at = metal_tile(&engine);
            engine.players.get_mut(&pid).unwrap().research.harvesting_efficiency = level;
            let f = engine.fleets.get_mut(&fid).unwrap();
            f.location = at;
            for ship in &mut f.ships[..f.ship_count] {
                ship.ship_class = ShipClass::Hauler;
            }
            engine.handle_harvest(pid, fid, HarvestResource::Metal).unwrap();
            for _ in 0..400 {
                engine.process_harvesting().unwrap();
            }
            (engine.fleets[&fid].cargo.metal, engine.ore_reserve_at(at).unwrap().metal, at)
        };
        let (plain, tile, at) = mine(0);
        let total = scaling::ore_reserve_units(Density::Pristine, at.dist_from_origin());
        assert_eq!(tile.units, 0.0, "the tile is emptied");
        assert!((plain - total).abs() < 0.5, "cargo holds the whole reserve: {plain} vs {total}");
        let (boosted, _, _) = mine(2);
        assert!((boosted / plain - 1.4).abs() < 0.01, "level 2 is a 40 percent better yield: {boosted} vs {plain}");
    }

    #[test]
    fn a_harvest_stops_at_the_end_of_a_density_step() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Miner");
        let at = metal_tile(&engine);
        let f = engine.fleets.get_mut(&fid).unwrap();
        f.location = at;
        f.ships[0].ship_class = ShipClass::Hauler;
        engine.handle_harvest(pid, fid, HarvestResource::Metal).unwrap();
        engine.process_harvesting().unwrap();
        let step = scaling::ore_step_units(Density::Pristine, at.dist_from_origin());
        let after = engine.ore_reserve_at(at).unwrap().metal;
        let got = engine.fleets[&fid].cargo.metal;
        assert!(got <= step + 1e-3, "one tick never takes more than the level holds: {got} vs {step}");
        assert!((after.units + got - after.max_units).abs() < 1e-2);
    }

    /// A sector whose template holds a non-passive group of at least `min` ships.
    fn find_group_sector(engine: &GameEngine, min: u8) -> Hex {
        for q in -20i16..20 {
            for r in -20i16..20 {
                let coord = Hex { q, r };
                if let Some(t) = engine.world_gen.generate_sector(coord).npc_template
                    && t.count >= min
                    && t.count <= 4
                    && t.behavior != iac_shared::world::NpcBehaviorType::Passive
                    && engine.derelict_site_at(coord).is_none()
                    && !engine.players.values().any(|p| p.homeworld == coord)
                {
                    return coord;
                }
            }
        }
        panic!("no such group in the window");
    }

    #[test]
    fn a_kill_pays_the_worth_of_the_whole_group_and_the_event_agrees() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Hunter");
        let at = find_group_sector(&engine, 2);
        let tmpl = engine.world_gen.generate_sector(at).npc_template.unwrap();
        engine.fleets.get_mut(&fid).unwrap().location = at;
        arm(&mut engine, fid);
        engine.handle_attack(pid, fid, template_npc_id(at)).unwrap();
        let group_power = engine.npc_fleets[&template_npc_id(at)].power;
        assert!((group_power - tmpl.power()).abs() < 0.01 * tmpl.power());

        let mut events = engine.drain_events();
        for _ in 0..30 {
            engine.tick().unwrap();
            events.extend(engine.drain_events());
            if engine.active_combats.is_empty() { break; }
        }
        let pile = engine.sector_overrides[&at.to_key()].salvage.expect("pile dropped");
        let want = scaling::split_loot(scaling::wreck_value(group_power)).scale(engine.pace().finds_mult());
        assert!((pile.metal - want.metal).abs() < 1e-3 && (pile.crystal - want.crystal).abs() < 1e-3 && (pile.deuterium - want.deuterium).abs() < 1e-3, "{pile:?} vs {want:?}");
        let one_ship = scaling::wreck_value(scaling::ship_power(tmpl.ship_class) * tmpl.stat_multiplier);
        assert!(pile.total() > one_ship * 1.5, "{} ships pay more than one", tmpl.count);
        let advertised = events.iter().find_map(|e| match &e.kind {
            EventKind::FleetDestroyed(d) if d.is_npc => Some(d.salvage),
            _ => None,
        }).unwrap();
        assert_eq!(advertised, pile, "the event and the pile are one number");
    }

    #[test]
    fn holds_grow_with_the_pace_like_the_loot_they_carry() {
        let (mut engine, mut blitz) = (test_engine(), engine_at_pace(600.0));
        let (_, fid) = register(&mut engine, "Slow");
        let (_, bid) = register(&mut blitz, "Fast");
        let slow = fleet_cargo_capacity(&engine.fleets[&fid], &engine.world.pace);
        let fast = fleet_cargo_capacity(&blitz.fleets[&bid], &blitz.world.pace);
        assert_eq!(slow, 40.0, "two scouts at pace 1");
        assert!((fast / slow - 600f32.sqrt()).abs() < 1e-3, "{fast}");
        assert_eq!(fleet_cargo_capacity(&blitz.fleets[&bid], &blitz.world.pace), fast);
    }

    #[test]
    fn a_hauler_lifts_the_deepest_pile_a_fleet_can_win_at_any_pace() {
        // Pile and hold both scale with P^0.5, so the ratio is a constant of the rules.
        for pace in [1.0, 10.0, 600.0] {
            let pace = Pace::new(pace).unwrap();
            let hold = f32::from(ShipClass::Hauler.base_stats().cargo) * pace.cargo_mult();
            for ring in [8u16, 14, 20, 30] {
                let pile = scaling::wreck_value(scaling::npc_power(ring)) * pace.finds_mult();
                assert!(pile < hold * 0.5 || ring > 20, "ring {ring} pile {pile} against a hauler's {hold}");
            }
            let scouts = 2.0 * f32::from(ShipClass::Scout.base_stats().cargo) * pace.cargo_mult();
            let ring8 = scaling::wreck_value(scaling::npc_power(8)) * pace.finds_mult();
            assert!(ring8 < scouts, "two scouts lift a ring-8 pile");
        }
    }

    #[test]
    fn a_kill_scales_with_the_worlds_finds_factor() {
        let pay = |pace: f64| {
            let mut engine = engine_at_pace(pace);
            let (pid, fid) = register(&mut engine, "Hunter");
            let at = find_npc_sector(&engine, iac_shared::world::NpcBehaviorType::Passive);
            win_a_fight(&mut engine, pid, fid, at);
            engine.sector_overrides[&at.to_key()].salvage.unwrap().total()
        };
        assert!((pay(100.0) / pay(1.0) - 10.0).abs() < 0.01, "P^0.5");
    }

    /// Board the derelict at `site`; returns the event and everything the
    /// roll was worth (what came aboard plus the overflow left as salvage).
    fn board(engine: &mut GameEngine, pid: u64, fid: u64, site: Hex, tick: u64) -> Option<(iac_shared::protocol::SiteExploredEvent, f32)> {
        engine.current_tick = tick;
        engine.npc_fleets.clear();
        engine.active_combats.clear();
        let ov = engine.ensure_override(site.to_key());
        ov.site_looted_tick = None;
        ov.site_ambush_bumps = 0;
        ov.salvage = None;
        let f = engine.fleets.get_mut(&fid).unwrap();
        f.location = site;
        f.state = FleetStatus::Exploring;
        f.action_cooldown = 0;
        f.cargo = Resources::default();
        engine.drain_events();
        let _ = pid;
        engine.resolve_exploration(fid).unwrap();
        let overflow = engine.sector_overrides[&site.to_key()].salvage.map_or(0.0, |s| s.total());
        engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::SiteExplored(x) => Some((x.clone(), x.resources.total() + overflow)),
            _ => None,
        })
    }

    /// The fleet becomes `n` ships: a Scout (which helps against ambushes)
    /// and Corvettes, or all Haulers.
    fn shape_fleet(engine: &mut GameEngine, fid: u64, hauler: bool) {
        let f = engine.fleets.get_mut(&fid).unwrap();
        f.ship_count = 4;
        for i in 0..4 {
            f.ships[i] = f.ships[0];
            f.ships[i].id = 9_000 + i as u64;
            f.ships[i].ship_class = if hauler { ShipClass::Hauler } else if i == 0 { ShipClass::Scout } else { ShipClass::Corvette };
        }
    }

    /// The derelict nearest the hub that is at least `min_dist` out.
    fn derelict_beyond(engine: &GameEngine, min_dist: u16) -> Hex {
        let mut best: Option<Hex> = None;
        for q in -45i16..45 {
            for r in -45i16..45 {
                let coord = Hex { q, r };
                let d = coord.dist_from_origin();
                if d >= min_dist && engine.derelict_site_at(coord).is_some() && best.is_none_or(|b| d < b.dist_from_origin()) {
                    best = Some(coord);
                }
            }
        }
        best.unwrap_or_else(|| panic!("no derelict beyond {min_dist}"))
    }

    #[test]
    fn a_derelict_pays_its_ring_value_between_60_and_140_percent() {
        for (dist, pace) in [(4u16, 1.0f64), (14, 1.0), (24, 1.0), (14, 100.0)] {
            let mut engine = engine_at_pace(pace);
            let (pid, fid) = register(&mut engine, "Boarder");
            shape_fleet(&mut engine, fid, false);
            let site = derelict_beyond(&engine, dist);
            let ring = site.dist_from_origin();
            let mean = scaling::derelict_value(ring) * engine.pace().finds_mult();
            let mut seen = Vec::new();
            for tick in 1000..1400 {
                shape_fleet(&mut engine, fid, false);
                if let Some((_, rolled)) = board(&mut engine, pid, fid, site, tick) {
                    seen.push(rolled / mean);
                }
            }
            assert!(seen.len() > 100, "enough clean breaches: {}", seen.len());
            let (lo, hi) = seen.iter().fold((f32::MAX, 0.0f32), |(l, h), &v| (l.min(v), h.max(v)));
            assert!(lo >= 0.6 - 1e-3 && hi <= 1.4 + 1e-3, "d={ring}: {lo}..{hi}");
            assert!(lo < 0.7 && hi > 1.3, "the whole range is used: {lo}..{hi}");
            let avg = seen.iter().sum::<f32>() / seen.len() as f32;
            assert!((avg - 1.0).abs() < 0.06, "mean is the table value: {avg}");
        }
    }

    #[test]
    fn a_hauler_doubles_a_derelicts_resources() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Boarder");
        let site = derelict_beyond(&engine, 6);
        shape_fleet(&mut engine, fid, false);
        let (tick, (_, plain)) = (1000..1400).find_map(|t| board(&mut engine, pid, fid, site, t).map(|x| (t, x))).unwrap();
        shape_fleet(&mut engine, fid, true);
        let (_, hauled) = board(&mut engine, pid, fid, site, tick).expect("the same roll, no ambush");
        assert!((hauled / plain - 2.0).abs() < 1e-3, "{hauled} vs {plain}");
    }

    #[test]
    fn a_stripped_derelict_comes_back_on_the_finds_timer() {
        for pace in [1.0, 100.0] {
            let mut engine = engine_at_pace(pace);
            let (pid, fid) = register(&mut engine, "Boarder");
            shape_fleet(&mut engine, fid, true);
            let site = derelict_beyond(&engine, 5);
            let tick = (1000..1400).find(|&t| board(&mut engine, pid, fid, site, t).is_some()).unwrap();
            assert!(engine.derelict_site_at(site).is_none(), "stripped");
            engine.sector_overrides.get_mut(&site.to_key()).unwrap().site_ambush_bumps = 3;

            let delay = engine.pace().finds_ticks(DERELICT_RESPAWN_TICKS as f64);
            assert_eq!(delay, (432_000.0 / pace.sqrt()).ceil() as u64, "120 game hours over P^0.5");
            engine.current_tick = tick + delay - 1;
            engine.process_derelict_respawn();
            assert!(engine.derelict_site_at(site).is_none(), "one tick early");
            engine.current_tick = tick + delay;
            engine.process_derelict_respawn();
            let (tier, bumps) = engine.derelict_site_at(site).expect("back");
            assert_eq!((tier, bumps), (scaling::derelict_tier(site.dist_from_origin()), 0), "calm again");
            assert!(board(&mut engine, pid, fid, site, tick + delay + 5).is_some() || engine.derelict_site_at(site).is_some());
        }
    }

    #[test]
    fn deep_derelicts_hold_relics_and_shallow_ones_never_do() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Boarder");
        shape_fleet(&mut engine, fid, true);
        let deep = derelict_beyond(&engine, 30);
        let shallow = derelict_beyond(&engine, 4);
        let (mut deep_relics, mut shallow_relics, mut breaches) = (0, 0, 0);
        for tick in 1000..2500 {
            if let Some((x, _)) = board(&mut engine, pid, fid, deep, tick) {
                breaches += 1;
                deep_relics += u32::from(x.relic);
            }
            if let Some((x, _)) = board(&mut engine, pid, fid, shallow, tick) {
                shallow_relics += u32::from(x.relic);
            }
        }
        assert!(deep_relics > 0 && deep_relics < breaches / 4, "{deep_relics} of {breaches}");
        assert_eq!(shallow_relics, 0);
        assert_eq!(engine.players[&pid].relics, deep_relics);
    }

    #[test]
    fn boarding_odds_follow_the_threat_and_a_scout_helps() {
        let mut engine = test_engine();
        let (_, fid) = register(&mut engine, "Boarder");
        let near = Hex { q: 4, r: 0 };
        let far = Hex { q: 30, r: 0 };
        let fleet = engine.fleets[&fid].clone();
        let near_odds = engine.site_ambush_chance(&fleet, near, 0);
        let far_odds = engine.site_ambush_chance(&fleet, far, 0);
        assert!((near_odds - 0.02).abs() < 1e-6, "T1 odds 0.10 less the scout's 0.10, floored: {near_odds}");
        assert!((far_odds - 0.24).abs() < 1e-6, "T9 odds 0.34 less 0.10: {far_odds}");
        assert!(engine.site_ambush_chance(&fleet, far, 2) > far_odds + 0.19);
        assert_eq!(site_risk_label(near, 0), iac_shared::protocol::SiteRisk::Quiet);
        assert_eq!(site_risk_label(far, 0), iac_shared::protocol::SiteRisk::Hot);
    }

    /// Fly one hop and wait for the arrival.
    fn fly(engine: &mut GameEngine, pid: u64, fid: u64, to: Hex) {
        engine.fleets.get_mut(&fid).unwrap().action_cooldown = 0;
        engine.fleets.get_mut(&fid).unwrap().fuel = 1e6;
        engine.handle_move(pid, fid, to).unwrap();
        for _ in 0..60 {
            engine.tick().unwrap();
            if engine.fleets[&fid].location == to { return; }
        }
        panic!("never arrived");
    }

    /// Two hops out and back on lanes the world really has.
    fn out_and_back(engine: &GameEngine, pid: u64) -> (Hex, Hex) {
        let home = engine.players[&pid].homeworld;
        let quiet = |h: Hex| engine.world_gen.generate_sector(h).npc_template.is_none();
        for &a in engine.world_gen.connected_neighbors(home).slice().iter().filter(|&&h| quiet(h)) {
            for &b in engine.world_gen.connected_neighbors(a).slice().iter().filter(|&&h| quiet(h)) {
                if b != home && !engine.players.values().any(|p| p.homeworld == a || p.homeworld == b) {
                    return (a, b);
                }
            }
        }
        panic!("no lane");
    }

    fn explore_of(engine: &GameEngine, pid: u64) -> f32 {
        engine.players[&pid].explore_points
    }

    #[test]
    fn a_chart_pays_only_when_delivered_to_a_dock() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Cartographer");
        let home = engine.players[&pid].homeworld;
        let (a, b) = out_and_back(&engine, pid);
        fly(&mut engine, pid, fid, a);
        fly(&mut engine, pid, fid, b);
        assert!(engine.fleets[&fid].charts.len() >= 2 || b == home);
        assert_eq!(explore_of(&engine, pid), 0.0, "nothing is credited in the field");

        let carried = engine.fleets[&fid].charts.clone();
        fly(&mut engine, pid, fid, a);
        fly(&mut engine, pid, fid, home);
        let want: f32 = carried.iter().map(|&k| score::chart_points(ring_rating(Hex::from_key(k)))).sum();
        assert!(want > 0.0);
        assert!((explore_of(&engine, pid) - want).abs() < 1e-5, "{} vs {want}", explore_of(&engine, pid));
        assert!(engine.fleets[&fid].charts.is_empty());
        let delivered = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::ChartDelivered(d) => Some(d),
            _ => None,
        }).expect("event");
        assert_eq!(delivered.sectors as usize, carried.len());

        fly(&mut engine, pid, fid, a);
        fly(&mut engine, pid, fid, home);
        assert!((explore_of(&engine, pid) - want).abs() < 1e-5, "the same sectors never pay twice");
    }

    #[test]
    fn a_fleet_lost_in_the_field_takes_its_chart_with_it() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Suicide");
        let home = engine.players[&pid].homeworld;
        let (a, b) = out_and_back(&engine, pid);
        fly(&mut engine, pid, fid, a);
        fly(&mut engine, pid, fid, b);
        engine.remove_fleet(fid);

        let (_, fid2) = register(&mut engine, "Second");
        let home2 = engine.players[&engine.fleets[&fid2].owner_id].homeworld;
        assert_ne!(home, home2);
        assert_eq!(explore_of(&engine, pid), 0.0);
        assert!(engine.credited.iter().all(|c| c.0 != pid), "nothing was ever credited");
    }

    #[test]
    fn a_chart_follows_a_split_and_a_merge_without_paying_twice() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Splitter");
        let home = engine.players[&pid].homeworld;
        let (a, _) = out_and_back(&engine, pid);
        fly(&mut engine, pid, fid, a);
        let ship = engine.fleets[&fid].ships[1].id;
        engine.fleets.get_mut(&fid).unwrap().action_cooldown = 0;
        let child = engine.handle_split(pid, fid, &[ship]).unwrap();
        assert_eq!(engine.fleets[&child].charts, engine.fleets[&fid].charts);
        for id in [fid, child] {
            fly(&mut engine, pid, id, home);
        }
        let once = explore_of(&engine, pid);
        assert!((once - score::chart_points(ring_rating(a))).abs() < 1e-5, "both carried it, one payment");
    }

    #[test]
    fn the_first_boarding_pays_points_and_a_respawned_site_pays_only_resources() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Boarder");
        let site = derelict_beyond(&engine, 5);
        let tick = (1000..1400).find(|&t| {
            shape_fleet(&mut engine, fid, true);
            board(&mut engine, pid, fid, site, t).is_some()
        }).unwrap();
        let rating = ring_rating(site);
        let first = explore_of(&engine, pid);
        assert!(first >= 0.02 * f32::from(rating).powf(1.5), "{first}");

        let tick = (tick + 1..tick + 400).find(|&t| {
            shape_fleet(&mut engine, fid, true);
            board(&mut engine, pid, fid, site, t).is_some()
        }).unwrap();
        let _ = tick;
        let relic_bonus = engine.players[&pid].relics as f32 * scaling::RELIC_POINTS;
        assert!((explore_of(&engine, pid) - first - relic_bonus).abs() < 1e-4, "no second payment for the same site");
    }

    #[test]
    fn a_first_boarding_pays_the_loot_share_and_a_discovery_bonus_at_the_worlds_scale() {
        for pace in [1.0, 600.0] {
            let mut engine = engine_at_pace(pace);
            let (pid, fid) = register(&mut engine, "Boarder");
            let site = derelict_beyond(&engine, 10);
            let (event, total) = (1000..1400).find_map(|t| {
                shape_fleet(&mut engine, fid, false);
                board(&mut engine, pid, fid, site, t)
            }).expect("a clean boarding");
            let rating = f32::from(ring_rating(site));
            // Without a hauler the whole roll is the finds-scaled base value.
            let weight = scaling::resource_weight(&scaling::split_loot(total));
            let relic = if event.relic { scaling::RELIC_POINTS } else { 0.0 };
            let want = score::BOARD_VALUE_SHARE * weight + score::DISCOVERY_POINTS * rating.powf(1.5) + relic;
            assert!((event.points - want).abs() < 1e-3 * want, "pace {pace}: {} vs {want}", event.points);
            assert!(event.points > score::DISCOVERY_POINTS * rating.powf(1.5) * 0.99, "pace {pace}");
            if pace > 100.0 {
                assert!(weight > 5.0, "a derelict at a blitz is worth real points, not hundredths: {weight}");
            }
        }
    }

    #[test]
    fn a_relic_is_worth_its_points() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Boarder");
        let site = derelict_beyond(&engine, 30);
        let mut before = 0.0;
        let found = (1000..4000).find_map(|t| {
            shape_fleet(&mut engine, fid, true);
            before = explore_of(&engine, pid);
            board(&mut engine, pid, fid, site, t).filter(|(x, _)| x.relic).map(|(x, _)| x)
        }).expect("a relic turns up in a few thousand boardings at T9");
        assert!(found.points >= scaling::RELIC_POINTS);
        assert!((explore_of(&engine, pid) - before - found.points).abs() < 1e-4);
    }

    #[test]
    fn exploration_shows_on_the_leaderboard_inside_the_cap() {
        let mut engine = test_engine();
        let (a, _) = register(&mut engine, "Explorer");
        let (b, _) = register(&mut engine, "Builder");
        engine.players.get_mut(&a).unwrap().explore_points = 0.5;
        let (rows, _) = score::leaderboard(&engine, 10, a);
        let mine = rows.iter().find(|r| r.name == "Explorer").unwrap();
        assert_eq!(mine.explore, 0.5);
        let other = rows.iter().find(|r| r.name == "Builder").unwrap();
        assert!(mine.score > other.score);

        engine.players.get_mut(&a).unwrap().explore_points = 1.0e6;
        let (rows, _) = score::leaderboard(&engine, 10, a);
        let mine = rows.iter().find(|r| r.name == "Explorer").unwrap();
        assert!((mine.score - (1.0 + score::EXTRA_CAP) * mine.core).abs() < 1e-3, "exploration adds at most the cap share of core");
        let _ = b;
    }

    #[test]
    fn credits_charts_and_relics_survive_a_restart() {
        let path = temp_world("explore_restart");
        let (pid, fid, a, credited) = {
            let mut engine = engine_at(&path, None).unwrap();
            let (pid, fid) = register(&mut engine, "Keeper");
            let (a, _) = out_and_back(&engine, pid);
            fly(&mut engine, pid, fid, a);
            engine.players.get_mut(&pid).unwrap().relics = 2;
            engine.dirty_players.insert(pid, ());
            let site = derelict_beyond(&engine, 5);
            let credit = engine.credited.insert((pid, site.to_key(), ExploreCredit::Boarded));
            engine.new_credits.push((pid, site, ExploreCredit::Boarded));
            assert!(credit);
            engine.persist_dirty_state().unwrap();
            engine.flush_persistence().unwrap();
            (pid, fid, a, (pid, site.to_key(), ExploreCredit::Boarded))
        };
        let engine = engine_at(&path, None).unwrap();
        assert_eq!(engine.players[&pid].relics, 2);
        assert!(engine.fleets[&fid].charts.contains(&a.to_key()));
        assert!(engine.credited.contains(&credited));
    }

    fn give_defences(engine: &mut GameEngine, pid: u64, turrets: u32, lancers: u32) {
        let p = engine.players.get_mut(&pid).unwrap();
        p.defences.count[DefenceKind::PulseTurret as usize] = turrets;
        p.defences.count[DefenceKind::LancerBattery as usize] = lancers;
    }

    #[test]
    fn defences_are_locked_until_the_grid_and_research_allow() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Wall");
        rich(&mut engine, pid);
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.shipyard = 1;
        }
        assert_eq!(engine.handle_build_defence(pid, DefenceKind::PulseTurret, 2, false), Err(ErrorCode::DefenceLocked));
        engine.players.get_mut(&pid).unwrap().buildings.defense_grid = 1;
        engine.handle_build_defence(pid, DefenceKind::PulseTurret, 2, false).unwrap();
        assert_eq!(engine.handle_build_defence(pid, DefenceKind::LancerBattery, 1, false), Err(ErrorCode::DefenceLocked));
        assert_eq!(engine.handle_build_defence(pid, DefenceKind::PulseTurret, 0, false), Err(ErrorCode::InvalidCommand));

        let before = engine.home_defense_power(pid);
        tick_n(&mut engine, 120);
        let p = &engine.players[&pid];
        assert_eq!(p.defences.count[DefenceKind::PulseTurret as usize], 2);
        assert!(p.ship_queue.is_none());
        let gain = engine.home_defense_power(pid) - before;
        assert!((gain - 2.0 * 17.0 * 1.04).abs() < 0.01, "two turrets with a level-1 grid: {gain}");
        assert!(engine.drain_events().iter().any(|e| matches!(e.kind, EventKind::DefenceBuilt(_))));
    }

    #[test]
    fn defences_share_the_shipyard_queue_and_refund_like_ships() {
        use iac_shared::protocol::QueueType;
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Mixed");
        rich(&mut engine, pid);
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.shipyard = 1;
            p.buildings.defense_grid = 1;
        }
        let start = engine.players[&pid].resources;
        engine.handle_build_defence(pid, DefenceKind::PulseTurret, 4, false).unwrap();
        engine.handle_build_ship(pid, ShipClass::Scout, 1, false).unwrap();
        assert_eq!(engine.players[&pid].ship_pending.len(), 1);
        let running = running_id(&engine, pid, QueueType::Ship);
        engine.handle_cancel_build(pid, QueueType::Ship, running).unwrap();
        let paid = DefenceKind::PulseTurret.build_cost();
        // only the unit under way was paid; half of it back, then the waiting scout started and paid
        let expect = start.sub(paid.scale(0.5)).sub(ShipClass::Scout.build_cost());
        assert_eq!(engine.players[&pid].resources, expect);
    }

    #[test]
    fn a_raid_is_sized_from_economy_points_not_from_defence() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Target");
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.shipyard = 2;
            p.buildings.metal_mine = 10;
        }
        let base = {
            let p = &engine.players[&pid];
            scaling::raid_power_base(scaling::econ_points(&p.buildings, &p.research))
        };
        engine.current_tick = 1_000_000;
        for _ in 0..400 {
            engine.current_tick += 1;
            engine.raid_states.insert(pid, RaidState {
                first_seen_tick: 0,
                next_roll_tick: engine.current_tick,
                last_raid_tick: 0,
                suppress_until: 0,
                missed_rolls: 0,
                incoming: None,
            });
            engine.process_raids().unwrap();
            if let Some(i) = engine.raid_states[&pid].incoming {
                assert!(i.power >= base * RAID_POWER_ROLL_MIN && i.power <= base * RAID_POWER_ROLL_MAX, "{} vs {base}", i.power);
                assert_eq!(i.arrival_tick - engine.current_tick, 600, "pace 1 warns 600 s ahead");
                let ev = engine.drain_events().into_iter().find_map(|e| match e.kind {
                    EventKind::RaidIncoming(r) => Some(r),
                    _ => None,
                }).unwrap();
                assert_eq!(ev.est_power, i.power);
                return;
            }
        }
        panic!("no raid forecast in 400 rolls at 30 percent");
    }

    #[test]
    fn raids_wait_for_age_and_the_long_interval() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Newcomer");
        engine.players.get_mut(&pid).unwrap().buildings.shipyard = 2;
        for t in 0..2_000u64 {
            engine.current_tick = 100 + t;
            if let Some(s) = engine.raid_states.get_mut(&pid) {
                s.next_roll_tick = 0;
            }
            engine.process_raids().unwrap();
            assert!(engine.raid_states[&pid].incoming.is_none(), "no raid within a day of arriving");
        }
    }

    #[test]
    fn the_raid_cadence_and_warning_follow_the_pace() {
        let blitz = RaidTimers::new(&Pace::parse("blitz").unwrap());
        assert_eq!(blitz.warning, 30);
        assert!((150..200).contains(&blitz.roll_interval), "{}", blitz.roll_interval);
        assert!((500..600).contains(&blitz.min_interval));
        let slow = RaidTimers::new(&Pace::PERSISTENT);
        assert_eq!((slow.roll_interval, slow.warning, slow.min_interval), (21_600, 600, 64_800));
        assert_eq!((slow.min_player_age, slow.suppress_after_loss), (86_400, 172_800));
    }

    /// Raid forecasts per player over `ticks`, for `players` fresh empires
    /// that all have Shipyard 2.
    fn raid_counts(pace: &str, ticks: u64, players: usize) -> Vec<usize> {
        let db = Database::init(":memory:").unwrap();
        let mut engine = GameEngine::init(42, db, Some(Pace::parse(pace).unwrap())).unwrap();
        let pids: Vec<u64> = (0..players).map(|i| register(&mut engine, &format!("Raided{i}")).0).collect();
        for pid in &pids {
            engine.players.get_mut(pid).unwrap().buildings.shipyard = 2;
        }
        let mut counts = vec![0; players];
        for _ in 0..ticks {
            engine.tick().unwrap();
            for e in engine.drain_events() {
                if let EventKind::RaidIncoming(r) = e.kind
                    && let Some(i) = pids.iter().position(|p| *p == r.player_id)
                {
                    counts[i] += 1;
                }
            }
        }
        counts
    }

    #[test]
    fn blitz_players_see_raids_within_the_hour() {
        let counts = raid_counts("blitz", 3600, 12);
        let quiet = counts.iter().filter(|c| **c == 0).count();
        let mean = counts.iter().sum::<usize>() as f32 / counts.len() as f32;
        assert!(quiet <= 1, "{quiet} of 12 blitz hours had no raid: {counts:?}");
        assert!((1.2..=3.0).contains(&mean), "mean {mean} raids per blitz hour: {counts:?}");
    }

    #[test]
    fn season_players_see_about_three_raids_a_day() {
        let counts = raid_counts("season", 24 * 3600, 6);
        let mean = counts.iter().sum::<usize>() as f32 / counts.len() as f32;
        assert!((1.8..=4.5).contains(&mean), "mean {mean} raids per 24 h of season: {counts:?}");
    }

    #[test]
    fn missed_rolls_raise_the_chance_until_a_raid_comes() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Overdue");
        engine.players.get_mut(&pid).unwrap().buildings.shipyard = 2;
        engine.current_tick = 10_000_000;
        engine.raid_states.insert(pid, RaidState {
            first_seen_tick: 0,
            next_roll_tick: engine.current_tick,
            last_raid_tick: 0,
            suppress_until: 0,
            missed_rolls: 6,
            incoming: None,
        });
        engine.process_raids().unwrap();
        let state = &engine.raid_states[&pid];
        assert!(state.incoming.is_some(), "six misses make the next roll certain (0.30 + 6 x 0.15)");
        assert_eq!(state.missed_rolls, 0);
    }

    #[test]
    fn a_lost_raid_destroys_a_quarter_of_the_docked_ships() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Overrun");
        {
            let f = engine.fleets.get_mut(&fid).unwrap();
            f.ship_count = 8;
            for i in 0..8 {
                f.ships[i] = f.ships[0];
                f.ships[i].id = 8_000 + i as u64;
            }
        }
        engine.drain_events();
        engine.resolve_raid(pid, 1.0e9).unwrap();
        let ev = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::RaidResolved(r) => Some(r),
            _ => None,
        }).unwrap();
        assert!(!ev.defended);
        assert_eq!(ev.ships_lost, 2, "a quarter of 8");
        assert_eq!(engine.fleets[&fid].ship_count, 6);
        assert!(engine.fleets[&fid].ships[6..].iter().all(|s| s.hull == 0.0), "the slots behind are cleared");

        // A defended raid costs no ships.
        for s in &mut engine.fleets.get_mut(&fid).unwrap().ships[..6] {
            s.weapon_power = 1.0e6;
        }
        engine.resolve_raid(pid, 10.0).unwrap();
        let ev = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::RaidResolved(r) => Some(r),
            _ => None,
        }).unwrap();
        assert!(ev.defended);
        assert_eq!(ev.ships_lost, 0);
        assert_eq!(engine.fleets[&fid].ship_count, 6);
    }

    #[test]
    fn a_repelled_raid_wears_structures_down_and_most_come_back() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Bulwark");
        give_defences(&mut engine, pid, 20, 10);
        let defence = engine.home_defense_power(pid);
        // A raid at the defence's strength: half the structures fall.
        engine.resolve_raid(pid, defence * 0.5).unwrap();
        let ev = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::RaidResolved(r) => Some(r),
            _ => None,
        }).unwrap();
        assert!(ev.defended);
        let frac = 0.5 * 0.5;
        let lost_turrets = (20.0 * frac as f32).round() as u32;
        let lost_lancers = (10.0 * frac as f32).round() as u32;
        assert_eq!(ev.structures_lost, lost_turrets + lost_lancers);
        let back = (lost_turrets as f32 * 0.7).round() as u32 + (lost_lancers as f32 * 0.7).round() as u32;
        assert_eq!(ev.structures_restored, back);
        let p = &engine.players[&pid];
        assert_eq!(p.defences.count[0], 20 - lost_turrets);
        assert_eq!(p.defences.restoring[0], (lost_turrets as f32 * 0.7).round() as u32);

        // Nothing returns early; everything promised returns on time.
        let due = p.defences.restore_tick[0];
        engine.current_tick = due - 1;
        engine.process_defence_restore();
        assert_eq!(engine.players[&pid].defences.count[0], 20 - lost_turrets);
        engine.current_tick = due;
        engine.process_defence_restore();
        let p = &engine.players[&pid];
        assert_eq!(p.defences.count[0], 20 - lost_turrets + (lost_turrets as f32 * 0.7).round() as u32);
        assert_eq!(p.defences.restoring, [0; 3]);
    }

    #[test]
    fn a_lost_raid_destroys_half_the_structures_for_good_and_protects_the_vault_share() {
        let mut engine = fast_engine();
        let (pid, _) = register(&mut engine, "Overrun");
        give_defences(&mut engine, pid, 10, 4);
        {
            let p = engine.players.get_mut(&pid).unwrap();
            p.buildings.storage_vault = 2;
            p.resources = Resources { metal: 200_000.0, crystal: 0.0, deuterium: 0.0 };
        }
        let protected = scaling::storage_protected(2, &engine.pace());
        engine.resolve_raid(pid, 1.0e7).unwrap();
        let ev = engine.drain_events().into_iter().find_map(|e| match e.kind {
            EventKind::RaidResolved(r) => Some(r),
            _ => None,
        }).unwrap();
        assert!(!ev.defended);
        assert_eq!(ev.structures_lost, 5 + 2);
        assert_eq!(ev.structures_restored, 0);
        assert_eq!(ev.protected_kept.metal, protected.metal);
        let p = &engine.players[&pid];
        assert_eq!((p.defences.count[0], p.defences.count[1]), (5, 2));
        assert_eq!(p.defences.restoring, [0; 3]);
    }

    #[test]
    fn defences_and_their_queue_survive_a_restart() {
        let path = temp_world("defence_restart");
        let pid = {
            let mut engine = engine_at(&path, Some(Pace::new(100.0).unwrap())).unwrap();
            let (pid, _) = register(&mut engine, "Keeper");
            rich(&mut engine, pid);
            {
                let p = engine.players.get_mut(&pid).unwrap();
                p.buildings.shipyard = 1;
                p.buildings.defense_grid = 1;
            }
            give_defences(&mut engine, pid, 7, 0);
            engine.players.get_mut(&pid).unwrap().defences.restoring[0] = 2;
            engine.players.get_mut(&pid).unwrap().defences.restore_tick[0] = 999;
            engine.handle_build_defence(pid, DefenceKind::PulseTurret, 3, false).unwrap();
            engine.handle_build_ship(pid, ShipClass::Scout, 1, false).unwrap();
            engine.handle_build_defence(pid, DefenceKind::PulseTurret, 1, false).unwrap();
            engine.persist_dirty_state().unwrap();
            engine.flush_persistence().unwrap();
            pid
        };
        let engine = engine_at(&path, None).unwrap();
        let p = &engine.players[&pid];
        assert_eq!(p.defences.count, [7, 0, 0]);
        assert_eq!((p.defences.restoring[0], p.defences.restore_tick[0]), (2, 999));
        assert_eq!(p.ship_queue.as_ref().unwrap().item, ShipyardItem::Defence(DefenceKind::PulseTurret));
        assert_eq!(p.ship_pending.iter().map(|q| q.item).collect::<Vec<_>>(),
            vec![ShipyardItem::Ship(ShipClass::Scout), ShipyardItem::Defence(DefenceKind::PulseTurret)]);
    }

    fn score_of_player(engine: &GameEngine, pid: u64) -> score::Score {
        score::score_of(&engine.players[&pid], engine.fleets.values())
    }

    #[test]
    fn a_new_player_scores_for_the_mines_and_scouts_they_start_with() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Fresh");
        let s = score_of_player(&engine, pid);
        // metal L1 60/15 + crystal L1 48/24 -> (60 + 22.5 + 48 + 36) / 1000
        assert!((s.econ - 0.1665).abs() < 1e-3, "{s:?}");
        assert!((s.fleet - 2.0 * 0.335).abs() < 1e-4, "two scouts at 200/50/30: {s:?}");
        assert_eq!((s.defence, s.combat, s.explore), (0.0, 0.0, 0.0));
    }

    #[test]
    fn only_completed_work_counts_and_losses_subtract() {
        use iac_shared::protocol::QueueType;
        let mut engine = fast_engine();
        let (pid, fid) = register(&mut engine, "Honest");
        rich(&mut engine, pid);
        let before = score_of_player(&engine, pid).total();
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        assert_eq!(score_of_player(&engine, pid).total(), before, "an item under construction scores nothing");
        let running = running_id(&engine, pid, QueueType::Building);
        engine.handle_cancel_build(pid, QueueType::Building, running).unwrap();
        assert_eq!(score_of_player(&engine, pid).total(), before, "cancelling scores nothing either");
        tick_n(&mut engine, 1);
        engine.handle_build(pid, BuildingType::MetalMine, false).unwrap();
        tick_n(&mut engine, 50);
        let after = score_of_player(&engine, pid).total();
        assert!(after > before, "the finished level counts: {before} -> {after}");

        let fleet_before = score_of_player(&engine, pid).fleet;
        let f = engine.fleets.get_mut(&fid).unwrap();
        f.ships[0].hull = 0.0;
        assert!(score_of_player(&engine, pid).fleet < fleet_before, "a dead ship no longer counts");
    }

    #[test]
    fn structures_count_for_half_and_extras_are_capped() {
        let mut engine = test_engine();
        let (pid, _) = register(&mut engine, "Builder");
        give_defences(&mut engine, pid, 10, 0);
        let s = score_of_player(&engine, pid);
        let turret = scaling::resource_weight(&DefenceKind::PulseTurret.build_cost());
        assert!((s.defence - 0.5 * 10.0 * turret).abs() < 1e-4);

        engine.players.get_mut(&pid).unwrap().combat_points = 1.0e6;
        let s = score_of_player(&engine, pid);
        assert!((s.total() - (1.0 + score::EXTRA_CAP) * s.core()).abs() < 1e-3, "combat adds at most the cap share of core");
    }

    #[test]
    fn a_kill_scores_the_pile_once_per_respawn() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Hunter");
        let at = find_npc_sector(&engine, iac_shared::world::NpcBehaviorType::Patrol);

        engine.fleets.get_mut(&fid).unwrap().location = at;
        engine.handle_attack(pid, fid, template_npc_id(at)).unwrap();
        let npc_power = engine.npc_fleets.values().map(|n| n.power).sum::<f32>();
        let engaged = fleet_power(&engine.fleets[&fid]);
        for n in engine.npc_fleets.values_mut() {
            for sh in n.ships[..n.ship_count as usize].iter_mut() {
                sh.hull = 1.0;
                sh.shield = 0.0;
            }
        }
        for _ in 0..6 {
            engine.tick().unwrap();
        }
        assert!(engine.active_combats.is_empty());
        let key = at.to_key();
        let pile = engine.sector_overrides[&key].salvage.expect("pile");
        let paid = engine.players[&pid].combat_points;
        assert!(paid > 0.0);
        let want = score::kill_points(pile, engaged, npc_power);
        assert!((paid - want).abs() < 0.2 * want, "paid {paid}, pile and ratio say {want}");

        // The same sector pays nothing again until the NPC respawns.
        assert!(engine.sector_overrides[&key].npc_cleared_tick.is_some());
        engine.active_combats.insert(77, Combat {
            id: 77, sector: at, player_fleet_ids: vec![fid], npc_fleet_ids: vec![], round: 0, peak_power: 1.0,
        });
        engine.credit_kill(77, &[fid], &[], key, pile);
        assert_eq!(engine.players[&pid].combat_points, paid);
    }

    #[test]
    fn overwhelming_force_earns_a_little_not_nothing() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Bully");
        let at = find_npc_sector(&engine, iac_shared::world::NpcBehaviorType::Patrol);
        win_a_fight(&mut engine, pid, fid, at);
        let pile = engine.sector_overrides[&at.to_key()].salvage.unwrap();
        let paid = engine.players[&pid].combat_points;
        let floor = score::KILL_SHARE * scaling::resource_weight(&pile) * score::MIN_PAY;
        assert!(paid > 0.0, "a dominant fleet still earns something");
        assert!((paid - floor).abs() < 1e-3 * floor.max(1.0), "5000-damage hulls against a scout: the floor {floor}, paid {paid}");
    }

    #[test]
    fn killing_the_respawning_group_again_and_again_pays_less_each_time() {
        let mut engine = test_engine();
        let (pid, fid) = register(&mut engine, "Farmer");
        let at = find_npc_sector(&engine, iac_shared::world::NpcBehaviorType::Patrol);
        let key = at.to_key();
        let mut piles = Vec::new();
        let mut points = Vec::new();
        for _ in 0..4 {
            let before = engine.players[&pid].combat_points;
            engine.sector_overrides.entry(key).or_default().salvage = None;
            win_a_fight(&mut engine, pid, fid, at);
            piles.push(engine.sector_overrides[&key].salvage.unwrap().total());
            points.push(engine.players[&pid].combat_points - before);
            // The group respawns at once; no time passes, so the heat stays.
            engine.sector_overrides.get_mut(&key).unwrap().npc_cleared_tick = None;
            engine.npc_fleets.clear();
        }
        for i in 1..4 {
            assert!((piles[i] / piles[i - 1] - scaling::FARM_DECAY).abs() < 1e-3, "pile {i}: {piles:?}");
            assert!((points[i] / points[i - 1] - scaling::FARM_DECAY).abs() < 1e-3, "points {i}: {points:?}");
        }
        let total: f32 = piles.iter().sum();
        assert!(total < piles[0] / (1.0 - scaling::FARM_DECAY), "the tap is bounded");

        // Left alone for four respawn delays the heat has halved; long enough and it is gone.
        let half = engine.farm_half_life(at);
        engine.current_tick += half;
        let one_half = engine.sector_heat(at);
        engine.current_tick += 10 * half;
        assert!(engine.sector_heat(at) < one_half / 500.0);
        assert!(engine.sector_farm_factor(at) > 0.99);
    }

    #[test]
    fn the_leaderboard_ranks_one_population_and_labels_agents() {
        let mut engine = test_engine();
        let human = engine.authenticate("Human", None, false).unwrap().player_id;
        let bot = engine.authenticate("Botty", None, true).unwrap().player_id;
        engine.authenticate("Third", None, false).unwrap();
        engine.players.get_mut(&bot).unwrap().buildings.metal_mine = 8;

        let (rows, you) = score::leaderboard(&engine, 2, human);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Botty");
        assert!(rows[0].agent && !rows[1].agent);
        assert_eq!((rows[0].rank, rows[1].rank), (1, 2));
        assert!(rows[0].score > rows[1].score);
        assert!(you.is_none(), "the viewer is already in the table");

        let third = engine.players.values().find(|p| p.name == "Third").unwrap().id;
        let (_, you) = score::leaderboard(&engine, 1, third);
        assert_eq!(you.unwrap().rank, 3, "outside the limit the viewer still sees their row");
        // The flag is read once, at creation.
        assert!(!engine.authenticate("Human", None, true).map(|a| a.registered).unwrap_or(false) || engine.players[&human].agent);
        assert!(!engine.players[&human].agent);
    }

    #[test]
    fn score_state_survives_a_restart() {
        let path = temp_world("score_restart");
        let pid = {
            let mut engine = engine_at(&path, None).unwrap();
            let pid = engine.authenticate("Keeper", None, true).unwrap().player_id;
            engine.players.get_mut(&pid).unwrap().combat_points = 3.25;
            engine.players.get_mut(&pid).unwrap().explore_points = 1.5;
            engine.persist_dirty_state().unwrap();
            engine.flush_persistence().unwrap();
            pid
        };
        let engine = engine_at(&path, None).unwrap();
        let p = &engine.players[&pid];
        assert!(p.agent);
        assert_eq!((p.combat_points, p.explore_points), (3.25, 1.5));
    }
}
