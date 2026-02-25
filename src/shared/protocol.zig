// src/shared/protocol.zig
// Wire protocol types shared between server and client.
// All messages are JSON-serialized over WebSocket.

const std = @import("std");
const hex = @import("hex.zig");
const constants = @import("constants.zig");
pub const scaling = @import("scaling.zig");

pub const Hex = hex.Hex;
pub const Resources = constants.Resources;
pub const ShipClass = constants.ShipClass;
pub const TerrainType = constants.TerrainType;
pub const Density = constants.Density;
pub const BuildingType = scaling.BuildingType;
pub const ResearchType = scaling.ResearchType;

// ── Client → Server Messages ──────────────────────────────────────

pub const ClientMessage = union(enum) {
    auth: AuthRequest,
    command: Command,
    policy_update: PolicyUpdate,
    request_full_state: void,
};

pub const AuthRequest = struct {
    player_name: []const u8,
    token: ?[]const u8 = null, // null = new player registration
};

pub const Command = union(enum) {
    move: MoveCommand,
    harvest: HarvestCommand,
    attack: AttackCommand,
    recall: RecallCommand,
    collect_salvage: CollectSalvageCommand,
    build: BuildCommand,
    research: ResearchCommand,
    build_ship: BuildShipCommand,
    cancel_build: CancelBuildCommand,
    stop: void, // cancel current action
    scan: void,

    pub const MoveCommand = struct {
        fleet_id: u64,
        target: Hex,
    };

    pub const HarvestCommand = struct {
        fleet_id: u64,
        resource: enum { metal, crystal, deuterium, auto },
    };

    pub const AttackCommand = struct {
        fleet_id: u64,
        target_fleet_id: u64,
    };

    pub const RecallCommand = struct {
        fleet_id: u64,
    };

    pub const CollectSalvageCommand = struct {
        fleet_id: u64,
    };

    pub const BuildCommand = struct {
        building_type: BuildingType,
    };

    pub const ResearchCommand = struct {
        tech: ResearchType,
    };

    pub const BuildShipCommand = struct {
        ship_class: ShipClass,
        count: u16 = 1,
    };

    pub const CancelBuildCommand = struct {
        queue_type: scaling.QueueType,
    };
};

pub const PolicyUpdate = struct {
    fleet_id: u64,
    rules: []const PolicyRule,
};

pub const PolicyRule = struct {
    condition: []const u8, // condition expression string
    action: []const u8, // action identifier
    priority: u8, // lower = higher priority
};

// ── Server → Client Messages ──────────────────────────────────────

pub const ServerMessage = union(enum) {
    auth_result: AuthResult,
    tick_update: TickUpdate,
    full_state: GameState,
    event: GameEvent,
    @"error": ErrorMessage,
};

pub const AuthResult = struct {
    success: bool,
    player_id: ?u64 = null,
    token: ?[]const u8 = null,
    message: ?[]const u8 = null,
};

pub const TickUpdate = struct {
    tick: u64,
    fleet_updates: ?[]const FleetState = null,
    sector_update: ?SectorState = null,
    homeworld_update: ?HomeworldState = null,
    events: ?[]const GameEvent = null,
};

pub const GameState = struct {
    tick: u64,
    player: PlayerState,
    fleets: []const FleetState,
    homeworld: HomeworldState,
    known_sectors: []const SectorState,
};

// ── State Types ───────────────────────────────────────────────────

pub const PlayerState = struct {
    id: u64,
    name: []const u8,
    resources: Resources,
    homeworld: Hex,
};

pub const FleetState = struct {
    id: u64,
    location: Hex,
    state: FleetStatus,
    ships: []const ShipState,
    cargo: Resources,
    fuel: f32,
    fuel_max: f32,
    cooldown_remaining: u16 = 0,
};

pub const FleetStatus = enum {
    idle,
    moving,
    harvesting,
    in_combat,
    returning,
    docked,
};

pub const ShipState = struct {
    id: u64,
    class: ShipClass,
    hull: f32,
    hull_max: f32,
    shield: f32,
    shield_max: f32,
    weapon_power: f32,
};

pub const SectorState = struct {
    location: Hex,
    terrain: TerrainType,
    resources: SectorResources,
    connections: []const Hex, // traversable neighbors
    hostiles: ?[]const NpcFleetInfo = null,
    player_fleets: ?[]const FleetBrief = null,
    salvage: ?Resources = null,
};

pub const SectorResources = struct {
    metal: Density,
    crystal: Density,
    deuterium: Density,
};

pub const NpcFleetInfo = struct {
    id: u64,
    ships: []const NpcShipInfo,
    behavior: NpcBehavior,
};

pub const NpcShipInfo = struct {
    class: ShipClass,
    count: u16,
};

pub const NpcBehavior = enum {
    passive,
    patrol,
    aggressive,
    swarm,
};

pub const FleetBrief = struct {
    id: u64,
    owner_name: []const u8,
    ship_count: u16,
};

pub const HomeworldState = struct {
    location: Hex,
    buildings: []const BuildingState,
    research: []const ResearchState,
    build_queue: ?BuildQueueItem = null,
    shipyard_queue: ?ShipyardQueueItem = null,
    research_active: ?ResearchItem = null,
    docked_ships: []const ShipState,
};

pub const BuildingState = struct {
    building_type: BuildingType,
    level: u8,
};

pub const ResearchState = struct {
    tech: ResearchType,
    level: u8,
};

pub const BuildQueueItem = struct {
    building_type: BuildingType,
    target_level: u8,
    start_tick: u64,
    end_tick: u64,
};

pub const ShipyardQueueItem = struct {
    ship_class: ShipClass,
    count: u16,
    built: u16,
    start_tick: u64,
    end_tick: u64,
};

pub const ResearchItem = struct {
    tech: ResearchType,
    target_level: u8,
    start_tick: u64,
    end_tick: u64,
};

// ── Game Events ───────────────────────────────────────────────────

pub const GameEvent = struct {
    tick: u64,
    kind: EventKind,
};

pub const EventKind = union(enum) {
    combat_round: CombatRoundEvent,
    ship_destroyed: ShipDestroyedEvent,
    fleet_destroyed: FleetDestroyedEvent,
    resource_harvested: ResourceHarvestedEvent,
    sector_entered: SectorEnteredEvent,
    combat_started: CombatStartedEvent,
    combat_ended: CombatEndedEvent,
    salvage_collected: SalvageCollectedEvent,
    fleet_arrived: FleetArrivedEvent,
    building_completed: BuildingCompletedEvent,
    research_completed: ResearchCompletedEvent,
    ship_built: ShipBuiltEvent,
    alert: AlertEvent,
};

pub const CombatRoundEvent = struct {
    attacker_ship_id: u64,
    target_ship_id: u64,
    damage: f32,
    shield_absorbed: f32,
    hull_damage: f32,
    rapid_fire: bool,
};

pub const ShipDestroyedEvent = struct {
    ship_id: u64,
    ship_class: ShipClass,
    owner_fleet_id: u64,
    is_npc: bool,
};

pub const FleetDestroyedEvent = struct {
    fleet_id: u64,
    is_npc: bool,
    salvage: Resources,
};

pub const HarvestResource = enum { metal, crystal, deuterium };

pub const ResourceHarvestedEvent = struct {
    fleet_id: u64,
    resource_type: HarvestResource,
    amount: f32,
};

pub const SectorEnteredEvent = struct {
    fleet_id: u64,
    sector: Hex,
    first_visit: bool,
};

pub const CombatStartedEvent = struct {
    player_fleet_id: u64,
    enemy_fleet_id: u64,
    sector: Hex,
};

pub const CombatEndedEvent = struct {
    sector: Hex,
    player_victory: bool,
};

pub const SalvageCollectedEvent = struct {
    fleet_id: u64,
    resources: Resources,
};

pub const FleetArrivedEvent = struct {
    fleet_id: u64,
    sector: Hex,
};

pub const BuildingCompletedEvent = struct {
    building_type: BuildingType,
    new_level: u8,
};

pub const ResearchCompletedEvent = struct {
    tech: ResearchType,
    new_level: u8,
};

pub const ShipBuiltEvent = struct {
    ship_class: ShipClass,
    count: u16,
};

pub const AlertEvent = struct {
    level: AlertLevel,
    message: []const u8,
    sector: ?Hex = null,
    fleet_id: ?u64 = null,
};

pub const AlertLevel = enum {
    info,
    warning,
    critical,
};

pub const ErrorMessage = struct {
    code: ErrorCode,
    message: []const u8,
};

pub const ErrorCode = enum(u16) {
    invalid_command = 1000,
    invalid_target = 1001,
    no_connection = 1002, // dead end
    insufficient_fuel = 1003,
    on_cooldown = 1004,
    fleet_not_found = 1005,
    not_in_sector = 1006,
    no_resources = 1007,
    cargo_full = 1008,
    prerequisites_not_met = 1009,
    max_level_reached = 1010,
    queue_full = 1011,
    ship_locked = 1012,
    no_shipyard = 1013,
    no_research_lab = 1014,
    auth_failed = 2000,
    already_authenticated = 2001,
    server_error = 5000,
};
