// Wire protocol types shared between server and client.
// All messages are JSON-serialized over WebSocket.

use serde::{Deserialize, Serialize};

use crate::constants::{Density, ShipClass, TerrainType};
use crate::scaling::{BuildingType, ResearchType};
use crate::hex::Hex;
use crate::Resources;

// ── Client → Server Messages ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "auth")]
    Auth(AuthRequest),
    #[serde(rename = "command")]
    Command(Command),
    #[serde(rename = "policy_update")]
    PolicyUpdate(PolicyUpdate),
    #[serde(rename = "request_full_state")]
    RequestFullState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    pub player_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum Command {
    #[serde(rename = "move")]
    Move { fleet_id: u64, target: Hex },
    #[serde(rename = "harvest")]
    Harvest { fleet_id: u64, resource: HarvestResource },
    #[serde(rename = "attack")]
    Attack { fleet_id: u64, target_fleet_id: u64 },
    #[serde(rename = "recall")]
    Recall { fleet_id: u64 },
    #[serde(rename = "collect_salvage")]
    CollectSalvage { fleet_id: u64 },
    #[serde(rename = "build")]
    Build { building_type: BuildingType },
    #[serde(rename = "research")]
    Research { tech: ResearchType },
    #[serde(rename = "build_ship")]
    BuildShip {
        ship_class: ShipClass,
        #[serde(default = "default_ship_count")]
        count: u16,
    },
    #[serde(rename = "cancel_build")]
    CancelBuild { queue_type: QueueType },
    #[serde(rename = "stop")]
    Stop {
        #[serde(default)]
        fleet_id: u64,
    },
    #[serde(rename = "scan")]
    Scan {
        #[serde(default)]
        fleet_id: u64,
    },
    #[serde(rename = "explore_site")]
    ExploreSite {
        #[serde(default)]
        fleet_id: u64,
    },
}

fn default_ship_count() -> u16 {
    1
}

/// Which resource a harvest order mines. `Auto` takes everything the sector
/// has, metal then crystal then deuterium, until the hold is full; a specific
/// resource mines only that one and is rejected with
/// `ErrorCode::ResourceNotPresent` if the sector has none of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HarvestResource {
    Metal,
    Crystal,
    Deuterium,
    Auto,
}

/// Standing orders for a fleet: assign a doctrine, the server flies it.
/// `preset: "manual"` clears the policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyUpdate {
    pub fleet_id: u64,
    pub preset: PolicyPreset,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub params: Option<PolicyParams>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPreset {
    Manual,
    Prospect,
    MineAndReturn,
    SalvageAndSites,
    PatrolHome,
}

impl PolicyPreset {
    pub const ALL: [PolicyPreset; 5] = [
        PolicyPreset::Manual,
        PolicyPreset::Prospect,
        PolicyPreset::MineAndReturn,
        PolicyPreset::SalvageAndSites,
        PolicyPreset::PatrolHome,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PolicyPreset::Manual => "manual",
            PolicyPreset::Prospect => "prospect",
            PolicyPreset::MineAndReturn => "mine+return",
            PolicyPreset::SalvageAndSites => "salvage+sites",
            PolicyPreset::PatrolHome => "patrol home",
        }
    }
}

/// Doctrine thresholds. Omitted fields take the defaults in constants.rs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PolicyParams {
    /// Head home when fuel drops below this percent.
    pub min_fuel_pct: u8,
    /// Head home when cargo fills past this percent.
    pub cargo_return_pct: u8,
    /// Maximum hops from home (or from the assigned work sector).
    pub max_range: u8,
    /// Engage hostiles only if our power ≥ theirs × (this / 10).
    pub engage_ratio_x10: u8,
}

impl Default for PolicyParams {
    fn default() -> Self {
        PolicyParams {
            min_fuel_pct: crate::constants::POLICY_DEFAULT_MIN_FUEL_PCT,
            cargo_return_pct: crate::constants::POLICY_DEFAULT_CARGO_RETURN_PCT,
            max_range: crate::constants::POLICY_DEFAULT_MAX_RANGE,
            engage_ratio_x10: crate::constants::POLICY_DEFAULT_ENGAGE_RATIO_X10,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum QueueType {
    Building,
    Ship,
    Research,
}

// ── Server → Client Messages ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "auth_result")]
    AuthResult(AuthResult),
    #[serde(rename = "tick_update")]
    TickUpdate(TickUpdate),
    #[serde(rename = "full_state")]
    FullState(GameState),
    #[serde(rename = "event")]
    Event(GameEvent),
    #[serde(rename = "error")]
    Error(ErrorMessage),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
}

/// Per-tick update. `player`, `homeworld_update`, `sector_updates` and
/// `events` are optional and omitted when there is nothing to say. `fleets`
/// is not optional: it is the complete, authoritative list of the player's
/// fleets, sent whole every tick (like `GameState.fleets`), so a client
/// replaces its list wholesale. A fleet missing from the list no longer
/// exists, and an empty list means every fleet is lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickUpdate {
    pub tick: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player: Option<PlayerState>,
    pub fleets: Vec<FleetState>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sector_updates: Option<Vec<SectorState>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub homeworld_update: Option<HomeworldState>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub events: Option<Vec<GameEvent>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub tick: u64,
    pub player: PlayerState,
    pub fleets: Vec<FleetState>,
    pub homeworld: HomeworldState,
    pub known_sectors: Vec<SectorState>,
}

// ── State Types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: u64,
    pub name: String,
    pub resources: Resources,
    pub homeworld: Hex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetState {
    pub id: u64,
    pub location: Hex,
    pub state: FleetStatus,
    pub ships: Vec<ShipState>,
    pub cargo: Resources,
    /// Hold size: the most `cargo` can total, as the server enforces it.
    pub cargo_capacity: f32,
    pub fuel: f32,
    pub fuel_max: f32,
    #[serde(default)]
    pub cooldown_remaining: u16,
    /// Standing orders currently flying this fleet, if any.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub policy: Option<PolicyPreset>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipState {
    pub id: u64,
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    pub hull: f32,
    pub hull_max: f32,
    pub shield: f32,
    pub shield_max: f32,
    pub weapon_power: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorState {
    pub location: Hex,
    pub terrain: TerrainType,
    pub resources: SectorResources,
    pub connections: Vec<Hex>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hostiles: Option<Vec<NpcFleetInfo>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_fleets: Option<Vec<FleetBrief>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage: Option<Resources>,
    /// A boardable derelict hulk drifting in this sector.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub site: Option<SiteBrief>,
}

/// What a fleet on-site can tell about a derelict before boarding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SiteBrief {
    pub tier: u8,
    /// Rough boarding risk read: "quiet" / "uneasy" / "hot".
    pub risk: SiteRisk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteRisk {
    Quiet,
    Uneasy,
    Hot,
}

impl SiteRisk {
    pub fn label(self) -> &'static str {
        match self {
            SiteRisk::Quiet => "quiet",
            SiteRisk::Uneasy => "uneasy",
            SiteRisk::Hot => "hot",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SectorResources {
    pub metal: Density,
    pub crystal: Density,
    pub deuterium: Density,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcFleetInfo {
    pub id: u64,
    pub ships: Vec<NpcShipInfo>,
    pub behavior: NpcBehavior,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcShipInfo {
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    pub count: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum NpcBehavior {
    Passive,
    Patrol,
    Aggressive,
    Swarm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetBrief {
    pub id: u64,
    pub owner_name: String,
    pub ship_count: u16,
    #[serde(default)]
    pub ship_classes: ShipClassCounts,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ShipClassCounts {
    pub scout: u16,
    pub corvette: u16,
    pub frigate: u16,
    pub cruiser: u16,
    pub hauler: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeworldState {
    pub location: Hex,
    /// What the mines add to the player's stockpile each tick at current
    /// levels. The homeworld has no storage caps, so there is no capacity
    /// field: stockpiles grow without limit.
    pub production: Resources,
    pub buildings: Vec<BuildingState>,
    pub research: Vec<ResearchState>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub build_queue: Option<BuildQueueItem>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub shipyard_queue: Option<ShipyardQueueItem>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub research_active: Option<ResearchItem>,
    pub docked_ships: Vec<ShipState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingState {
    pub building_type: BuildingType,
    pub level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchState {
    pub tech: ResearchType,
    pub level: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQueueItem {
    pub building_type: BuildingType,
    pub target_level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipyardQueueItem {
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    pub count: u16,
    pub built: u16,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchItem {
    pub tech: ResearchType,
    pub target_level: u8,
    pub start_tick: u64,
    pub end_tick: u64,
}

// ── Game Events ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEvent {
    pub tick: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum EventKind {
    CombatRound(CombatRoundEvent),
    ShipDestroyed(ShipDestroyedEvent),
    FleetDestroyed(FleetDestroyedEvent),
    ResourceHarvested(ResourceHarvestedEvent),
    SectorEntered(SectorEnteredEvent),
    CombatStarted(CombatStartedEvent),
    CombatEnded(CombatEndedEvent),
    SalvageCollected(SalvageCollectedEvent),
    FleetArrived(FleetArrivedEvent),
    BuildingCompleted(BuildingCompletedEvent),
    ResearchCompleted(ResearchCompletedEvent),
    ShipBuilt(ShipBuiltEvent),
    ScanCompleted(ScanCompletedEvent),
    RaidIncoming(RaidIncomingEvent),
    RaidResolved(RaidResolvedEvent),
    SiteExplorationStarted(SiteExplorationStartedEvent),
    SiteExplored(SiteExploredEvent),
    SiteAmbush(SiteAmbushEvent),
    PolicyAction(PolicyActionEvent),
    Alert(AlertEvent),
}

/// A boarding party is aboard the derelict; loot (or trouble) at end_tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteExplorationStartedEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub tier: u8,
    pub end_tick: u64,
}

/// The boarding paid off.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteExploredEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub tier: u8,
    /// Loot taken aboard (cargo-capped; overflow drops as sector salvage).
    pub resources: Resources,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recovered_ship: Option<ShipClass>,
    /// Instant tech level granted by a recovered data core.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tech_cache: Option<ResearchType>,
}

/// Whatever killed the crew is still aboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteAmbushEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub npc_fleet_id: u64,
}

/// A standing order acted on the player's behalf — always says why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyActionEvent {
    pub fleet_id: u64,
    pub preset: PolicyPreset,
    /// Short verb: "scan", "move", "harvest", "return", "attack", …
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatRoundEvent {
    pub attacker_ship_id: u64,
    pub target_ship_id: u64,
    pub damage: f32,
    pub shield_absorbed: f32,
    pub hull_damage: f32,
    pub rapid_fire: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipDestroyedEvent {
    pub ship_id: u64,
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    pub owner_fleet_id: u64,
    pub is_npc: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetDestroyedEvent {
    pub fleet_id: u64,
    pub is_npc: bool,
    pub salvage: Resources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHarvestedEvent {
    pub fleet_id: u64,
    pub resource_type: HarvestResource,
    pub amount: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorEnteredEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub first_visit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatStartedEvent {
    pub player_fleet_id: u64,
    pub enemy_fleet_id: u64,
    pub sector: Hex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatEndedEvent {
    pub sector: Hex,
    pub player_victory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalvageCollectedEvent {
    pub fleet_id: u64,
    pub resources: Resources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetArrivedEvent {
    pub fleet_id: u64,
    pub sector: Hex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingCompletedEvent {
    pub building_type: BuildingType,
    pub new_level: u8,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCompletedEvent {
    pub tech: ResearchType,
    pub new_level: u8,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipBuiltEvent {
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    pub count: u16,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
}

/// Result of an active fleet scan: sectors revealed plus faint signal
/// contacts detected one hop beyond scan range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCompletedEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub sectors_revealed: u16,
    pub hostiles_detected: u16,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub signals: Vec<SignalContact>,
}

/// A faint contact at the edge of scan range — a hint, not full intel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalContact {
    pub sector: Hex,
    pub signal: SignalKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalKind {
    RichOre,
    HostileMass,
    Derelict,
    Anomaly,
}

impl SignalKind {
    pub fn label(self) -> &'static str {
        match self {
            SignalKind::RichOre => "rich ore signature",
            SignalKind::HostileMass => "hostile mass reading",
            SignalKind::Derelict => "derelict transponder",
            SignalKind::Anomaly => "anomalous emissions",
        }
    }
}

/// Advance warning that a raid fleet is vectoring toward the homeworld.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidIncomingEvent {
    pub player_id: u64,
    pub arrival_tick: u64,
    /// Rough size read: "light" / "moderate" / "heavy"
    pub threat: String,
}

/// Outcome of a homeworld raid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaidResolvedEvent {
    pub player_id: u64,
    pub defended: bool,
    pub resources_lost: Resources,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_dropped: Option<Resources>,
    pub raid_power: f32,
    pub defense_power: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub level: AlertLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sector: Option<Hex>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fleet_id: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMessage {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u16)]
pub enum ErrorCode {
    InvalidCommand = 1000,
    InvalidTarget = 1001,
    NoConnection = 1002,
    InsufficientFuel = 1003,
    OnCooldown = 1004,
    FleetNotFound = 1005,
    NotInSector = 1006,
    NoResources = 1007,
    CargoFull = 1008,
    PrerequisitesNotMet = 1009,
    MaxLevelReached = 1010,
    QueueFull = 1011,
    ShipLocked = 1012,
    NoShipyard = 1013,
    NoResearchLab = 1014,
    FleetLimitReached = 1015,
    ResourceNotPresent = 1016,
    AuthFailed = 2000,
    AlreadyAuthenticated = 2001,
    ServerError = 5000,
}
