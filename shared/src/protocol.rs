// Wire protocol types shared between server and client.
// All messages are JSON-serialized over WebSocket.

use serde::{Deserialize, Serialize};

use crate::constants::{Density, ResourceKind, ShipClass, TerrainType};
use crate::scaling::{self, BuildingLevels, BuildingType, ResearchLevels, ResearchPrereqKind, ResearchType};
use crate::hex::Hex;
use crate::pace::Pace;
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

/// First message on a connection. The server decides what it means from the
/// name alone, so there is no separate register/login action:
///
/// - unknown name: valid names register a new account and the reply carries
///   the new secret in `AuthResult.token` (this is the only time it is sent);
/// - known name: `token` must match the one issued earlier, otherwise the
///   reply is `TokenRequired` / `InvalidToken`;
/// - an account from before tokens existed has none yet and is claimed by
///   its next login, which is issued one the same way.
///
/// Names are matched case-insensitively ("admiral" logs in to "Admiral"), so
/// no two accounts can differ only by case. An exact-case match wins, which
/// keeps any case-variant accounts registered before this rule reachable.
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
    /// Detach the listed ships from `fleet_id` into a new fleet at the same
    /// sector. The new fleet starts with no standing orders and takes a share
    /// of fuel and cargo proportional to the tank and hold it carries off.
    /// At least one ship must stay behind. Both fleets must be idle. Rejected
    /// with `FleetLimitReached` at the fleet cap, `InvalidTarget` for ship ids
    /// not in the fleet or an attempt to take every ship. At the homeworld
    /// this is also how docked ships are launched as their own fleet. The
    /// reply is an `Alert` (info) event naming the new fleet id.
    #[serde(rename = "split")]
    Split { fleet_id: u64, ship_ids: Vec<u64> },
    /// Fold `other_fleet_id` into `fleet_id`. Both must be yours, idle, and in
    /// the same sector (`NotInSector` otherwise). Ships, cargo and fuel are
    /// pooled (fuel up to the combined tank), the merged fleet keeps the
    /// standing orders of `fleet_id`, and `other_fleet_id` ceases to exist.
    /// This works anywhere, so a fresh fleet can rescue a stranded one.
    #[serde(rename = "merge")]
    Merge { fleet_id: u64, other_fleet_id: u64 },
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

/// Doctrine thresholds. Every field is optional on the wire; an omitted field
/// (or an omitted `params` object) takes the default in constants.rs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyParams {
    /// Spare fuel, as a percent of the tank, kept on top of the exact fuel
    /// cost of the route home. The autopilot heads home when fuel falls below
    /// route cost plus this reserve, and never takes a hop outward that would
    /// leave it unable to return. Clamped to 0..=90. Default 10.
    pub min_fuel_pct: u8,
    /// Head home when cargo fills past this percent. Clamped to 10..=100.
    /// Default 85.
    pub cargo_return_pct: u8,
    /// Maximum distance in hexes from the HOMEWORLD that the autopilot will
    /// claim, prospect or wander to; never measured from the last claim.
    /// Clamped to 1..=30. Default 4.
    pub max_range: u8,
    /// Engage hostiles only if our power >= theirs x (this / 10). Clamped to
    /// 1..=100. Default 12 (1.2x).
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

/// Reply to `AuthRequest`. On success `token` is set only when the server just
/// issued one (new account or claimed legacy account): the client must store
/// it. On failure `code` says why (`InvalidName`, `TokenRequired`,
/// `InvalidToken`, `ServerError`) and `message` is readable text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub code: Option<ErrorCode>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub message: Option<String>,
}

/// Per-tick update. `player`, `homeworld_update`, `sector_updates` and
/// `events` are optional and omitted when there is nothing to say.
/// `sector_updates` holds every live sector (current this tick) plus, once,
/// each sector that stopped being live this tick (`live: false`, so the
/// client knows to dim it). Stale sectors are not repeated after that. `fleets`
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

/// Full snapshot, sent on login and on `request_full_state`.
///
/// `known_sectors` is the player's whole chart: every sector the player has
/// ever had eyes on (fleet present, sensor array, active scan), not only the
/// ones with fresh intel. Entries with `live: true` are current; the rest
/// carry what was last observed and the tick it was observed (`last_seen`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub tick: u64,
    pub player: PlayerState,
    pub fleets: Vec<FleetState>,
    pub homeworld: HomeworldState,
    pub known_sectors: Vec<SectorState>,
    pub world: WorldInfo,
}

/// The world's fixed settings, sent with every `full_state`. `pace` scales
/// economy timers (see `docs/design/economy/spec.md` section 1) and never
/// changes for the life of a world.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldInfo {
    pub pace: f64,
    /// Name of the preset the pace equals, if it equals one.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub preset: Option<String>,
    pub tick_hz: u32,
    pub economy_version: u32,
    pub worldgen_version: u32,
    /// Simulation estimate for a competent player; present for presets.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub estimate: Option<WorldEstimate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEstimate {
    pub first_cruiser_s: u64,
    pub endgame_s: u64,
}

impl WorldInfo {
    pub fn new(pace: Pace, economy_version: u32, worldgen_version: u32) -> Self {
        let preset = pace.preset();
        WorldInfo {
            pace: pace.value(),
            preset: preset.map(|p| p.name.to_string()),
            tick_hz: crate::constants::TICK_RATE_HZ as u32,
            economy_version,
            worldgen_version,
            estimate: preset.map(|p| WorldEstimate {
                first_cruiser_s: p.first_cruiser_s,
                endgame_s: p.endgame_s,
            }),
        }
    }
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
    /// Fuel one jump costs this fleet right now (it scales with the fleet's
    /// hull mass and fuel research). A fleet with less than this and away
    /// from home is stranded and only regains fuel from the emergency
    /// reserve, one jump per five minutes.
    #[serde(default)]
    pub jump_fuel: f32,
    /// Fuel the way home costs: the fewest hops over lanes the player has
    /// charted, times `jump_fuel`; 0 at the homeworld. Compare with `fuel`.
    #[serde(default)]
    pub home_fuel: f32,
    #[serde(default)]
    pub cooldown_remaining: u16,
    /// At the homeworld with cargo the stockpile has no room for; it stays
    /// aboard and unloads as space frees up.
    #[serde(default)]
    pub cargo_blocked: bool,
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

/// One sector as the player knows it.
///
/// `live: true` means the server is watching the sector this tick (a fleet
/// is there, the sensor array covers it, or a scan has not expired) and every
/// field is current. `live: false` is chart memory: `resources`, `hostiles`,
/// `salvage` and `site` are what was last observed at tick `last_seen` and
/// may have changed since (a wreck despawns, a hostile moves, a derelict is
/// looted). `terrain` and `connections` never change. `player_fleets` is
/// only ever sent for live sectors; other empires' positions are not
/// remembered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorState {
    pub location: Hex,
    pub terrain: TerrainType,
    pub resources: SectorResources,
    pub connections: Vec<Hex>,
    /// Hostile fleets. `id` is the fleet id to pass to `attack`: it is stable
    /// whether or not the fleet has materialised yet, and it is the same id
    /// that later appears as `enemy_fleet_id` in `CombatStarted`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hostiles: Option<Vec<NpcFleetInfo>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_fleets: Option<Vec<FleetBrief>>,
    /// Collectible wreckage pile (what `collect_salvage` would take).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage: Option<Resources>,
    /// Tick the pile despawns if nobody collects it.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_despawn_tick: Option<u64>,
    /// A boardable derelict hulk drifting in this sector.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub site: Option<SiteBrief>,
    /// Tick this sector was last observed; the current tick when `live`.
    pub last_seen: u64,
    /// See the struct docs.
    pub live: bool,
}

/// What a fleet on-site can tell about a derelict before boarding.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectorResources {
    pub metal: Density,
    pub crystal: Density,
    pub deuterium: Density,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcFleetInfo {
    pub id: u64,
    pub ships: Vec<NpcShipInfo>,
    pub behavior: NpcBehavior,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcShipInfo {
    #[serde(rename = "class")]
    pub ship_class: ShipClass,
    pub count: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpcBehavior {
    Passive,
    Patrol,
    Aggressive,
    Swarm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetBrief {
    pub id: u64,
    pub owner_name: String,
    pub ship_count: u16,
    #[serde(default)]
    pub ship_classes: ShipClassCounts,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
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
    /// What the mines make each tick at current levels (before the storage
    /// cap discards the excess; see `storage`).
    pub production: Resources,
    pub storage: StorageState,
    pub buildings: Vec<BuildingState>,
    pub research: Vec<ResearchState>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub build_queue: Option<BuildQueueItem>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub shipyard_queue: Option<ShipyardQueueItem>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub research_active: Option<ResearchItem>,
    pub docked_ships: Vec<ShipState>,
    /// What can be queued right now and what it costs; see `HomeworldCatalog`.
    pub catalog: HomeworldCatalog,
}

/// Stockpile limits, from the Storage Vault level and the world pace.
/// Mines stop at `cap`; docking cargo unloads only up to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StorageState {
    pub cap: Resources,
    /// What a raid cannot skim.
    pub protected: Resources,
    /// Seconds until each stockpile is full at the current production;
    /// 0 when it is, null when it is not filling.
    pub full_in_s: ResourceEta,
    /// Resources whose stockpile is at the cap right now.
    pub capped: Vec<ResourceKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceEta {
    pub metal: Option<u64>,
    pub crystal: Option<u64>,
    pub deuterium: Option<u64>,
}

/// Everything a client needs to offer homeworld actions without knowing any
/// balance formulas: costs, build times and prerequisites, computed by the
/// server from `scaling.rs` for the player's current levels. Resent with
/// every `HomeworldState`, so it tracks level changes the tick they land.
/// Whether the player can pay is left to the client (compare `cost` with the
/// stockpile in `PlayerState.resources`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HomeworldCatalog {
    /// One entry per `BuildingType`, in declaration order.
    pub buildings: Vec<BuildingOption>,
    /// One entry per `ResearchType`, in declaration order.
    pub research: Vec<ResearchOption>,
    /// One entry per `ShipClass`, in declaration order.
    pub ships: Vec<ShipOption>,
}

/// One prerequisite: a building or tech that must reach `need`, with the
/// player's current level. The server's "prerequisites not met" errors are
/// written from the same values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Requirement {
    /// Building or tech label ("Shipyard", "Frigate Tech").
    pub name: String,
    pub need: u8,
    pub have: u8,
    pub met: bool,
}

impl Requirement {
    fn new(name: &str, need: u8, have: u8) -> Self {
        Requirement { name: name.to_string(), need, have, met: have >= need }
    }

    /// Compact form for cards: "Shipyard >= 4".
    pub fn label(&self) -> String {
        format!("{} >= {}", self.name, self.need)
    }
}

/// "Frigate needs Shipyard level 4 (you have 2) and Frigate Tech level 1
/// (you have 0)"; None when every requirement is met.
pub fn missing_requirements_message(subject: &str, requires: &[Requirement]) -> Option<String> {
    let parts: Vec<String> = requires
        .iter()
        .filter(|r| !r.met)
        .map(|r| format!("{} level {} (you have {})", r.name, r.need, r.have))
        .collect();
    let (last, rest) = parts.split_last()?;
    let list = if rest.is_empty() { last.clone() } else { format!("{} and {}", rest.join(", "), last) };
    Some(format!("{subject} needs {list}"))
}

fn building_req(levels: &BuildingLevels, b: BuildingType, need: u8) -> Requirement {
    Requirement::new(b.label(), need, levels.get(b))
}

fn tech_req(levels: &ResearchLevels, t: ResearchType, need: u8) -> Requirement {
    Requirement::new(t.label(), need, levels.get(t))
}

/// Everything standing between the player and the next level of `b`.
pub fn building_requirements(levels: &BuildingLevels, b: BuildingType) -> Vec<Requirement> {
    scaling::building_prerequisites(b)
        .map(|p| building_req(levels, p.building, p.level))
        .into_iter()
        .collect()
}

/// Prerequisites for researching `t`, including the Research Lab the server
/// requires before any research.
pub fn research_requirements(buildings: &BuildingLevels, research: &ResearchLevels, t: ResearchType) -> Vec<Requirement> {
    let prereqs: Vec<_> = scaling::research_prerequisites(t).into_iter().flatten().collect();
    // Every tech needs a lab; skip the generic entry when the tech names a lab level itself.
    let names_lab = prereqs.iter().any(|p| {
        matches!(p, ResearchPrereqKind::Building(b) if b.building == BuildingType::ResearchLab)
    });
    let mut requires = Vec::new();
    if !names_lab {
        requires.push(building_req(buildings, BuildingType::ResearchLab, 1));
    }
    for p in prereqs {
        requires.push(match p {
            ResearchPrereqKind::Building(b) => building_req(buildings, b.building, b.level),
            ResearchPrereqKind::Research { tech, level } => tech_req(research, tech, level),
        });
    }
    requires
}

/// Prerequisites for building `c`: a Shipyard plus the class's unlock tech.
pub fn ship_requirements(buildings: &BuildingLevels, research: &ResearchLevels, c: ShipClass) -> Vec<Requirement> {
    let mut requires = vec![building_req(buildings, BuildingType::Shipyard, 1)];
    requires.extend(scaling::ship_class_tech(c).map(|t| tech_req(research, t, 1)));
    requires
}

/// Cost and duration of the next level of a building or tech.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UpgradeStep {
    pub level: u8,
    pub cost: Resources,
    pub ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingOption {
    pub building_type: BuildingType,
    pub level: u8,
    pub max_level: u8,
    /// The upgrade to `level + 1`; absent at `max_level`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next: Option<UpgradeStep>,
    /// All prerequisites with their status; the entry is locked while any is unmet.
    pub requires: Vec<Requirement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResearchOption {
    pub tech: ResearchType,
    pub level: u8,
    pub max_level: u8,
    /// The upgrade to `level + 1`; absent at `max_level`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub next: Option<UpgradeStep>,
    /// Includes the Research Lab the server requires before any research.
    pub requires: Vec<Requirement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShipOption {
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    /// Cost of one ship; a batch of `count` costs `count` times this.
    pub unit_cost: Resources,
    /// Ticks to build one ship at the current shipyard level.
    pub ticks_per_ship: u64,
    /// Includes the Shipyard the server requires before any ship.
    pub requires: Vec<Requirement>,
}

impl HomeworldCatalog {
    pub fn new(buildings: &BuildingLevels, research: &ResearchLevels, pace: &Pace) -> Self {
        let building_options = (0..BuildingType::COUNT)
            .filter_map(BuildingType::from_usize)
            .map(|b| {
                let level = buildings.get(b);
                BuildingOption {
                    building_type: b,
                    level,
                    max_level: scaling::MAX_BUILDING_LEVEL,
                    next: (level < scaling::MAX_BUILDING_LEVEL).then(|| UpgradeStep {
                        level: level + 1,
                        cost: scaling::building_cost(b, level + 1),
                        ticks: scaling::building_time(b, level + 1, pace),
                    }),
                    requires: building_requirements(buildings, b),
                }
            })
            .collect();

        let research_options = ResearchType::ALL
            .iter()
            .map(|&t| {
                let level = research.get(t);
                let max_level = scaling::research_max_level(t);
                ResearchOption {
                    tech: t,
                    level,
                    max_level,
                    next: (level < max_level).then(|| UpgradeStep {
                        level: level + 1,
                        cost: scaling::research_cost(t, level + 1),
                        ticks: scaling::research_time(t, level + 1, pace),
                    }),
                    requires: research_requirements(buildings, research, t),
                }
            })
            .collect();

        let ship_options = ShipClass::ALL
            .iter()
            .map(|&c| ShipOption {
                ship_class: c,
                unit_cost: c.build_cost(),
                ticks_per_ship: scaling::ship_build_time(c, buildings.get(BuildingType::Shipyard), pace),
                requires: ship_requirements(buildings, research, c),
            })
            .collect();

        HomeworldCatalog {
            buildings: building_options,
            research: research_options,
            ships: ship_options,
        }
    }
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
//
// Scoping. A player only receives events that concern them: their own
// fleets, homeworld and queues, plus combat and loss detail for fights in
// any sector where they have a fleet or their homeworld. Other empires'
// fights elsewhere are not reported at all.
//
// Ownership. Combat and loss events name the empire that owns each fleet
// (`owner`, `None` for an NPC; empire names are public) and carry `mine`,
// computed per recipient: true when the event involves one of the
// receiver's own fleets. A player can therefore always tell their own
// losses from a fight they merely witnessed.

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
    SalvageDespawned(SalvageDespawnedEvent),
    StorageNearCap(StorageNearCapEvent),
    StorageFull(StorageFullEvent),
}

/// A stockpile passed 85 percent of its cap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageNearCapEvent {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    pub resource: ResourceKind,
    pub ratio: f32,
    /// Seconds until full at the current production; absent when not filling.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub full_in_s: Option<u64>,
}

/// A stockpile reached its cap; further production is discarded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageFullEvent {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
    pub resource: ResourceKind,
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

/// One shot in a fight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatRoundEvent {
    pub sector: Hex,
    pub attacker_fleet_id: u64,
    /// Empire owning the attacking fleet; `None` for an NPC.
    pub attacker_owner: Option<String>,
    pub attacker_ship_id: u64,
    pub target_fleet_id: u64,
    /// Empire owning the target fleet; `None` for an NPC.
    pub target_owner: Option<String>,
    pub target_ship_id: u64,
    pub damage: f32,
    pub shield_absorbed: f32,
    pub hull_damage: f32,
    pub rapid_fire: bool,
    /// One of the two fleets is the receiver's.
    #[serde(default)]
    pub mine: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipDestroyedEvent {
    pub ship_id: u64,
    #[serde(rename = "ship_class")]
    pub ship_class: ShipClass,
    pub owner_fleet_id: u64,
    pub is_npc: bool,
    pub sector: Hex,
    /// Empire that owned the ship; `None` for an NPC.
    pub owner: Option<String>,
    /// The ship was the receiver's.
    #[serde(default)]
    pub mine: bool,
}

/// Every ship of a fleet is gone. For an NPC fleet killed by players,
/// `salvage` is the wreckage it left in `sector`: exactly what
/// `collect_salvage` can take (when several NPC fleets die in one fight the
/// sector pile is the sum of their events). Player fleets leave no wreckage,
/// so `salvage` is zero. A destroyed fleet of the receiver's also produces a
/// `Critical` `Alert`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetDestroyedEvent {
    pub fleet_id: u64,
    pub is_npc: bool,
    pub sector: Hex,
    /// Empire that owned the fleet; `None` for an NPC.
    pub owner: Option<String>,
    /// The fleet was the receiver's.
    #[serde(default)]
    pub mine: bool,
    pub salvage: Resources,
}

/// What a harvesting fleet took aboard. Aggregated: one event per fleet
/// every `HARVEST_REPORT_TICKS` ticks (and when harvesting stops) rather
/// than one per resource per tick; `ticks` is how many ticks it covers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceHarvestedEvent {
    pub fleet_id: u64,
    pub resources: Resources,
    pub ticks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectorEnteredEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub first_visit: bool,
}

/// `enemy_fleet_id` is the same id the sector view listed under `hostiles`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatStartedEvent {
    pub player_fleet_id: u64,
    /// Empire owning `player_fleet_id`.
    pub owner: String,
    pub enemy_fleet_id: u64,
    pub sector: Hex,
    /// `player_fleet_id` is the receiver's.
    #[serde(default)]
    pub mine: bool,
}

/// `owners` lists the empires that fought. `salvage` is the pile the victory
/// left in the sector (absent on defeat); it despawns at `salvage_despawn_tick`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatEndedEvent {
    pub sector: Hex,
    pub player_victory: bool,
    pub owners: Vec<String>,
    #[serde(default)]
    pub mine: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage: Option<Resources>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub salvage_despawn_tick: Option<u64>,
}

/// A fleet scooped wreckage. `resources` is what went into the hold; if the
/// hold filled first, `remaining` is what is still on the ground (it keeps
/// its original despawn time). Absent when the pile is gone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalvageCollectedEvent {
    pub fleet_id: u64,
    pub sector: Hex,
    pub resources: Resources,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub remaining: Option<Resources>,
}

/// A wreckage pile expired uncollected. Sent to players with a fleet or
/// their homeworld in `sector`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalvageDespawnedEvent {
    pub sector: Hex,
    /// What was lost.
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

/// A message for the player. `player_id` names the recipient; `None` is a
/// server-wide notice. Losing a fleet raises a `Critical` alert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub player_id: Option<u64>,
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
    StorageTooSmall = 1017,
    AuthFailed = 2000,
    AlreadyAuthenticated = 2001,
    InvalidName = 2002,
    TokenRequired = 2003,
    InvalidToken = 2004,
    ServerError = 5000,
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    fn fresh() -> (BuildingLevels, ResearchLevels) {
        (BuildingLevels::default(), ResearchLevels::default())
    }

    fn building(cat: &HomeworldCatalog, b: BuildingType) -> &BuildingOption {
        cat.buildings.iter().find(|o| o.building_type == b).unwrap()
    }

    #[test]
    fn building_costs_match_scaling() {
        let (b, r) = fresh();
        let cat = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        assert_eq!(cat.buildings.len(), BuildingType::COUNT);
        let mine = building(&cat, BuildingType::MetalMine);
        let next = mine.next.unwrap();
        assert_eq!(mine.level, 1);
        assert_eq!(next.level, 2);
        assert_eq!(next.cost, scaling::building_cost(BuildingType::MetalMine, 2));
        assert_eq!(next.ticks, scaling::building_time(BuildingType::MetalMine, 2, &Pace::PERSISTENT));
    }

    #[test]
    fn building_prerequisite_unmet_then_met() {
        let (mut b, r) = fresh();
        let before = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        let yard = building(&before, BuildingType::Shipyard);
        assert_eq!(yard.requires.len(), 1);
        assert_eq!(yard.requires[0].name, "Metal Mine");
        assert_eq!((yard.requires[0].need, yard.requires[0].have), (2, 1));
        assert!(!yard.requires[0].met);

        b.set(BuildingType::MetalMine, 2);
        let after = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        assert!(building(&after, BuildingType::Shipyard).requires.iter().all(|q| q.met));
    }

    #[test]
    fn max_level_has_no_next_step() {
        let (mut b, mut r) = fresh();
        b.set(BuildingType::MetalMine, scaling::MAX_BUILDING_LEVEL);
        r.set(ResearchType::CorvetteTech, 1);
        let cat = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        assert!(building(&cat, BuildingType::MetalMine).next.is_none());
        let corvette = cat.research.iter().find(|o| o.tech == ResearchType::CorvetteTech).unwrap();
        assert!(corvette.next.is_none());
        assert_eq!(corvette.max_level, 1);
    }

    #[test]
    fn research_requires_lab_and_chain() {
        let (mut b, mut r) = fresh();
        let cat = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        let frigate = cat.research.iter().find(|o| o.tech == ResearchType::FrigateTech).unwrap();
        let unmet: Vec<_> = frigate.requires.iter().filter(|q| !q.met).map(|q| (q.name.as_str(), q.need)).collect();
        assert_eq!(unmet, [("Research Lab", 1), ("Corvette Tech", 1), ("Shipyard", 4)]);
        assert_eq!(
            frigate.next.unwrap().cost,
            scaling::research_cost(ResearchType::FrigateTech, 1)
        );

        b.set(BuildingType::ResearchLab, 1);
        b.set(BuildingType::Shipyard, 4);
        r.set(ResearchType::CorvetteTech, 1);
        let cat = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        let frigate = cat.research.iter().find(|o| o.tech == ResearchType::FrigateTech).unwrap();
        assert!(frigate.requires.iter().all(|q| q.met));
    }

    #[test]
    fn research_lists_one_lab_requirement_at_the_highest_level() {
        let (b, r) = fresh();
        let cat = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        for opt in &cat.research {
            let labs: Vec<u8> = opt.requires.iter().filter(|q| q.name == "Research Lab").map(|q| q.need).collect();
            assert_eq!(labs.len(), 1, "{:?} lists the lab {} times", opt.tech, labs.len());
        }
        let nav = cat.research.iter().find(|o| o.tech == ResearchType::Navigation).unwrap();
        assert!(nav.requires.iter().any(|q| q.name == "Research Lab" && q.need == 2));
    }

    #[test]
    fn ships_follow_shipyard_level_and_unlocks() {
        let (mut b, mut r) = fresh();
        let cat = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        assert_eq!(cat.ships.len(), ShipClass::ALL.len());
        let corvette = cat.ships.iter().find(|o| o.ship_class == ShipClass::Corvette).unwrap();
        assert_eq!(corvette.unit_cost, ShipClass::Corvette.build_cost());
        assert_eq!(corvette.ticks_per_ship, scaling::ship_build_time(ShipClass::Corvette, 0, &Pace::PERSISTENT));
        assert_eq!(corvette.requires.iter().filter(|q| !q.met).count(), 2);
        let scout = cat.ships.iter().find(|o| o.ship_class == ShipClass::Scout).unwrap();
        assert_eq!(scout.requires.len(), 1);

        b.set(BuildingType::Shipyard, 3);
        r.set(ResearchType::CorvetteTech, 1);
        let cat = HomeworldCatalog::new(&b, &r, &Pace::PERSISTENT);
        let corvette = cat.ships.iter().find(|o| o.ship_class == ShipClass::Corvette).unwrap();
        assert_eq!(corvette.ticks_per_ship, scaling::ship_build_time(ShipClass::Corvette, 3, &Pace::PERSISTENT));
        assert!(corvette.requires.iter().all(|q| q.met));
    }

    #[test]
    fn error_message_names_what_is_missing() {
        let (mut b, r) = fresh();
        b.set(BuildingType::Shipyard, 2);
        let msg = missing_requirements_message("Frigate", &ship_requirements(&b, &r, ShipClass::Frigate));
        assert_eq!(msg.unwrap(), "Frigate needs Frigate Tech level 1 (you have 0)");

        let research = research_requirements(&b, &r, ResearchType::FrigateTech);
        assert_eq!(
            missing_requirements_message("Frigate Tech", &research).unwrap(),
            "Frigate Tech needs Research Lab level 1 (you have 0), Corvette Tech level 1 (you have 0) and Shipyard level 4 (you have 2)"
        );

        b.set(BuildingType::Shipyard, 1);
        assert!(missing_requirements_message("Scout", &ship_requirements(&b, &r, ShipClass::Scout)).is_none());
    }
}

#[cfg(test)]
mod policy_params_tests {
    use super::*;
    use crate::constants::*;

    fn parse(json: &str) -> ClientMessage {
        serde_json::from_str(json).expect("policy_update parses")
    }

    fn params(msg: ClientMessage) -> PolicyParams {
        match msg {
            ClientMessage::PolicyUpdate(u) => u.params.expect("params present"),
            other => panic!("not a policy update: {other:?}"),
        }
    }

    #[test]
    fn partial_params_fill_the_rest_with_defaults() {
        let p = params(parse(r#"{"type":"policy_update","fleet_id":1,"preset":"mine_and_return","params":{"max_range":2}}"#));
        assert_eq!(p.max_range, 2);
        assert_eq!(p.min_fuel_pct, POLICY_DEFAULT_MIN_FUEL_PCT);
        assert_eq!(p.cargo_return_pct, POLICY_DEFAULT_CARGO_RETURN_PCT);
        assert_eq!(p.engage_ratio_x10, POLICY_DEFAULT_ENGAGE_RATIO_X10);
    }

    #[test]
    fn empty_params_are_all_defaults() {
        let p = params(parse(r#"{"type":"policy_update","fleet_id":1,"preset":"prospect","params":{}}"#));
        let d = PolicyParams::default();
        assert_eq!((p.min_fuel_pct, p.cargo_return_pct, p.max_range, p.engage_ratio_x10),
                   (d.min_fuel_pct, d.cargo_return_pct, d.max_range, d.engage_ratio_x10));
    }

    #[test]
    fn omitted_params_stay_none() {
        match parse(r#"{"type":"policy_update","fleet_id":1,"preset":"manual"}"#) {
            ClientMessage::PolicyUpdate(u) => assert!(u.params.is_none()),
            other => panic!("{other:?}"),
        }
    }
}
