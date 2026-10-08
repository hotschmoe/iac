// Cross-language golden fixtures.
//
// Rust is the source of truth for the wire protocol and hex math. This test
// renders canonical JSON for every protocol variant (and a set of hex
// results) into `<repo>/fixtures/`. The Dart client loads the very same
// files (clients/web/test/), so a change on either side that breaks the
// contract fails a test.
//
//   cargo test -p iac-shared --test golden_fixtures                 # verify
//   UPDATE_FIXTURES=1 cargo test -p iac-shared --test golden_fixtures   # regenerate

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use iac_shared::Resources;
use iac_shared::constants::{Density, ShipClass, TerrainType};
use iac_shared::hex::{Hex, HexDirection, hex_ring, hex_spiral};
use iac_shared::protocol::*;
use iac_shared::scaling::{BuildingType, ResearchType};

// ── Plumbing ──────────────────────────────────────────────────────

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("fixtures")
}

/// relative path (e.g. "protocol/command/move.json") -> rendered content
type Files = BTreeMap<String, String>;

fn render(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap();
    s.push('\n');
    s
}

fn put(files: &mut Files, rel: String, v: &Value) {
    assert!(files.insert(rel.clone(), render(v)).is_none(), "duplicate fixture {rel}");
}

/// Serialize `value`, check it survives a Rust round trip, store it.
fn put_msg<T: Serialize + DeserializeOwned>(files: &mut Files, dir: &str, name: &str, value: &T) {
    let v = serde_json::to_value(value).unwrap();
    let back: T = serde_json::from_value(v.clone())
        .unwrap_or_else(|e| panic!("{dir}/{name} does not deserialize: {e}"));
    assert_eq!(serde_json::to_value(&back).unwrap(), v, "{dir}/{name} not round-trip stable");
    put(files, format!("protocol/{dir}/{name}.json"), &v);
}

fn files_under(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else if p.extension().is_some_and(|x| x == "json") {
                out.push(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

// ── Sample data ───────────────────────────────────────────────────

fn h(q: i16, r: i16) -> Hex {
    Hex::new(q, r)
}

fn res(metal: f32, crystal: f32, deuterium: f32) -> Resources {
    Resources { metal, crystal, deuterium }
}

fn ship(id: u64, class: ShipClass, hull: f32, shield: f32, weapon: f32) -> ShipState {
    ShipState {
        id,
        ship_class: class,
        hull,
        hull_max: 100.0,
        shield,
        shield_max: 50.0,
        weapon_power: weapon,
    }
}

fn sample_fleet(id: u64, loc: Hex, state: FleetStatus, policy: Option<PolicyPreset>) -> FleetState {
    FleetState {
        id,
        location: loc,
        state,
        ships: vec![
            ship(id * 10 + 1, ShipClass::Scout, 80.5, 25.0, 4.0),
            ship(id * 10 + 2, ShipClass::Hauler, 100.0, 0.0, 0.0),
        ],
        cargo: res(12.5, 3.0, 0.0),
        cargo_capacity: 220.0,
        fuel: 271.5,
        fuel_max: 500.0,
        cooldown_remaining: 3,
        policy,
    }
}

fn rich_sector() -> SectorState {
    SectorState {
        location: h(3, -2),
        terrain: TerrainType::AsteroidField,
        resources: SectorResources {
            metal: Density::Rich,
            crystal: Density::Moderate,
            deuterium: Density::None,
        },
        connections: vec![h(4, -2), h(4, -3), h(2, -2), h(2, -1), h(3, -1)],
        hostiles: Some(vec![
            NpcFleetInfo {
                id: 9001,
                ships: vec![
                    NpcShipInfo { ship_class: ShipClass::Corvette, count: 3 },
                    NpcShipInfo { ship_class: ShipClass::Frigate, count: 1 },
                ],
                behavior: NpcBehavior::Aggressive,
            },
            NpcFleetInfo {
                id: 9002,
                ships: vec![NpcShipInfo { ship_class: ShipClass::Scout, count: 2 }],
                behavior: NpcBehavior::Passive,
            },
        ]),
        player_fleets: Some(vec![FleetBrief {
            id: 7,
            owner_name: "Rival".into(),
            ship_count: 4,
            ship_classes: ShipClassCounts { scout: 1, corvette: 2, frigate: 0, cruiser: 0, hauler: 1 },
        }]),
        salvage: Some(res(40.0, 12.0, 2.5)),
        site: Some(SiteBrief { tier: 2, risk: SiteRisk::Uneasy }),
    }
}

fn bare_sector() -> SectorState {
    SectorState {
        location: h(-1, 0),
        terrain: TerrainType::Empty,
        resources: SectorResources {
            metal: Density::None,
            crystal: Density::None,
            deuterium: Density::None,
        },
        connections: vec![],
        hostiles: None,
        player_fleets: None,
        salvage: None,
        site: None,
    }
}

fn anomaly_sector() -> SectorState {
    SectorState {
        location: h(-5, 7),
        terrain: TerrainType::Anomaly,
        resources: SectorResources {
            metal: Density::Sparse,
            crystal: Density::Pristine,
            deuterium: Density::Rich,
        },
        connections: vec![h(-4, 7), h(-5, 6)],
        hostiles: Some(vec![]),
        player_fleets: Some(vec![]),
        salvage: None,
        site: Some(SiteBrief { tier: 5, risk: SiteRisk::Hot }),
    }
}

fn sample_homeworld() -> HomeworldState {
    use BuildingType::*;
    let buildings = [
        MetalMine, CrystalMine, DeuteriumSynthesizer, Shipyard, ResearchLab, FuelDepot,
        SensorArray, DefenseGrid,
    ]
    .iter()
    .enumerate()
    .map(|(i, b)| BuildingState { building_type: *b, level: i as u8 })
    .collect();
    use ResearchType::*;
    let research = [
        FuelEfficiency, ExtendedFuelTanks, ReinforcedHulls, AdvancedShields, WeaponsResearch,
        Navigation, HarvestingEfficiency, CorvetteTech, FrigateTech, CruiserTech, HaulerTech,
        EmergencyJump,
    ]
    .iter()
    .enumerate()
    .map(|(i, t)| ResearchState { tech: *t, level: (i % 4) as u8 })
    .collect();
    HomeworldState {
        location: h(4, -2),
        production: res(2.75, 1.25, 0.4),
        buildings,
        research,
        build_queue: Some(BuildQueueItem {
            building_type: BuildingType::CrystalMine,
            target_level: 5,
            start_tick: 100,
            end_tick: 460,
        }),
        shipyard_queue: Some(ShipyardQueueItem {
            ship_class: ShipClass::Corvette,
            count: 2,
            built: 1,
            start_tick: 120,
            end_tick: 300,
        }),
        research_active: Some(ResearchItem {
            tech: ResearchType::FuelEfficiency,
            target_level: 2,
            start_tick: 90,
            end_tick: 600,
        }),
        docked_ships: vec![ship(501, ShipClass::Cruiser, 100.0, 50.0, 18.0)],
    }
}

fn idle_homeworld() -> HomeworldState {
    HomeworldState {
        location: h(0, 0),
        production: res(0.0, 0.0, 0.0),
        buildings: vec![],
        research: vec![],
        build_queue: None,
        shipyard_queue: None,
        research_active: None,
        docked_ships: vec![],
    }
}

fn sample_player() -> PlayerState {
    PlayerState { id: 42, name: "Admiral".into(), resources: res(1200.5, 600.0, 150.25), homeworld: h(4, -2) }
}

// ── Exhaustive naming (adding a variant breaks compilation here) ──

fn client_message_name(m: &ClientMessage) -> &'static str {
    match m {
        ClientMessage::Auth(_) => "auth",
        ClientMessage::Command(_) => "command",
        ClientMessage::PolicyUpdate(_) => "policy_update",
        ClientMessage::RequestFullState => "request_full_state",
    }
}

fn command_name(c: &Command) -> &'static str {
    match c {
        Command::Move { .. } => "move",
        Command::Harvest { .. } => "harvest",
        Command::Attack { .. } => "attack",
        Command::Recall { .. } => "recall",
        Command::CollectSalvage { .. } => "collect_salvage",
        Command::Build { .. } => "build",
        Command::Research { .. } => "research",
        Command::BuildShip { .. } => "build_ship",
        Command::CancelBuild { .. } => "cancel_build",
        Command::Stop { .. } => "stop",
        Command::Scan { .. } => "scan",
        Command::ExploreSite { .. } => "explore_site",
    }
}

fn server_message_name(m: &ServerMessage) -> &'static str {
    match m {
        ServerMessage::AuthResult(_) => "auth_result",
        ServerMessage::TickUpdate(_) => "tick_update",
        ServerMessage::FullState(_) => "full_state",
        ServerMessage::Event(_) => "event",
        ServerMessage::Error(_) => "error",
    }
}

fn event_kind_name(k: &EventKind) -> &'static str {
    match k {
        EventKind::CombatRound(_) => "combat_round",
        EventKind::ShipDestroyed(_) => "ship_destroyed",
        EventKind::FleetDestroyed(_) => "fleet_destroyed",
        EventKind::ResourceHarvested(_) => "resource_harvested",
        EventKind::SectorEntered(_) => "sector_entered",
        EventKind::CombatStarted(_) => "combat_started",
        EventKind::CombatEnded(_) => "combat_ended",
        EventKind::SalvageCollected(_) => "salvage_collected",
        EventKind::FleetArrived(_) => "fleet_arrived",
        EventKind::BuildingCompleted(_) => "building_completed",
        EventKind::ResearchCompleted(_) => "research_completed",
        EventKind::ShipBuilt(_) => "ship_built",
        EventKind::ScanCompleted(_) => "scan_completed",
        EventKind::RaidIncoming(_) => "raid_incoming",
        EventKind::RaidResolved(_) => "raid_resolved",
        EventKind::SiteExplorationStarted(_) => "site_exploration_started",
        EventKind::SiteExplored(_) => "site_explored",
        EventKind::SiteAmbush(_) => "site_ambush",
        EventKind::PolicyAction(_) => "policy_action",
        EventKind::Alert(_) => "alert",
    }
}

fn error_code_name(c: ErrorCode) -> &'static str {
    match c {
        ErrorCode::InvalidCommand => "invalid_command",
        ErrorCode::InvalidTarget => "invalid_target",
        ErrorCode::NoConnection => "no_connection",
        ErrorCode::InsufficientFuel => "insufficient_fuel",
        ErrorCode::OnCooldown => "on_cooldown",
        ErrorCode::FleetNotFound => "fleet_not_found",
        ErrorCode::NotInSector => "not_in_sector",
        ErrorCode::NoResources => "no_resources",
        ErrorCode::CargoFull => "cargo_full",
        ErrorCode::PrerequisitesNotMet => "prerequisites_not_met",
        ErrorCode::MaxLevelReached => "max_level_reached",
        ErrorCode::QueueFull => "queue_full",
        ErrorCode::ShipLocked => "ship_locked",
        ErrorCode::NoShipyard => "no_shipyard",
        ErrorCode::NoResearchLab => "no_research_lab",
        ErrorCode::FleetLimitReached => "fleet_limit_reached",
        ErrorCode::ResourceNotPresent => "resource_not_present",
        ErrorCode::AuthFailed => "auth_failed",
        ErrorCode::AlreadyAuthenticated => "already_authenticated",
        ErrorCode::ServerError => "server_error",
    }
}

const ALL_ERROR_CODES: [ErrorCode; 20] = [
    ErrorCode::InvalidCommand,
    ErrorCode::InvalidTarget,
    ErrorCode::NoConnection,
    ErrorCode::InsufficientFuel,
    ErrorCode::OnCooldown,
    ErrorCode::FleetNotFound,
    ErrorCode::NotInSector,
    ErrorCode::NoResources,
    ErrorCode::CargoFull,
    ErrorCode::PrerequisitesNotMet,
    ErrorCode::MaxLevelReached,
    ErrorCode::QueueFull,
    ErrorCode::ShipLocked,
    ErrorCode::NoShipyard,
    ErrorCode::NoResearchLab,
    ErrorCode::FleetLimitReached,
    ErrorCode::ResourceNotPresent,
    ErrorCode::AuthFailed,
    ErrorCode::AlreadyAuthenticated,
    ErrorCode::ServerError,
];

// ── Instances ─────────────────────────────────────────────────────

/// (label suffix, value). Empty suffix = the primary instance.
fn commands() -> Vec<(&'static str, Command)> {
    vec![
        ("", Command::Move { fleet_id: 1, target: h(-3, 4) }),
        ("", Command::Harvest { fleet_id: 1, resource: HarvestResource::Metal }),
        ("crystal", Command::Harvest { fleet_id: 2, resource: HarvestResource::Crystal }),
        ("deuterium", Command::Harvest { fleet_id: 2, resource: HarvestResource::Deuterium }),
        ("auto", Command::Harvest { fleet_id: 3, resource: HarvestResource::Auto }),
        ("", Command::Attack { fleet_id: 1, target_fleet_id: 9001 }),
        ("", Command::Recall { fleet_id: 4 }),
        ("", Command::CollectSalvage { fleet_id: 5 }),
        ("", Command::Build { building_type: BuildingType::DefenseGrid }),
        ("", Command::Research { tech: ResearchType::EmergencyJump }),
        ("", Command::BuildShip { ship_class: ShipClass::Frigate, count: 3 }),
        ("", Command::CancelBuild { queue_type: QueueType::Building }),
        ("ship", Command::CancelBuild { queue_type: QueueType::Ship }),
        ("research", Command::CancelBuild { queue_type: QueueType::Research }),
        ("", Command::Stop { fleet_id: 6 }),
        ("", Command::Scan { fleet_id: 7 }),
        ("", Command::ExploreSite { fleet_id: 8 }),
    ]
}

fn client_messages() -> Vec<(&'static str, ClientMessage)> {
    vec![
        ("", ClientMessage::Auth(AuthRequest { player_name: "Admiral".into(), token: None })),
        (
            "with_token",
            ClientMessage::Auth(AuthRequest { player_name: "Bot".into(), token: Some("headless".into()) }),
        ),
        ("", ClientMessage::Command(Command::Move { fleet_id: 1, target: h(1, -1) })),
        ("scan", ClientMessage::Command(Command::Scan { fleet_id: 1 })),
        (
            "",
            ClientMessage::PolicyUpdate(PolicyUpdate {
                fleet_id: 3,
                preset: PolicyPreset::Prospect,
                params: None,
            }),
        ),
        (
            "with_params",
            ClientMessage::PolicyUpdate(PolicyUpdate {
                fleet_id: 3,
                preset: PolicyPreset::MineAndReturn,
                params: Some(PolicyParams {
                    min_fuel_pct: 30,
                    cargo_return_pct: 90,
                    max_range: 6,
                    engage_ratio_x10: 15,
                }),
            }),
        ),
        ("manual", ClientMessage::PolicyUpdate(PolicyUpdate {
            fleet_id: 3,
            preset: PolicyPreset::Manual,
            params: None,
        })),
        ("", ClientMessage::RequestFullState),
    ]
}

fn server_messages() -> Vec<(String, ServerMessage)> {
    let mut v: Vec<(String, ServerMessage)> = vec![
        (
            "".into(),
            ServerMessage::AuthResult(AuthResult {
                success: true,
                player_id: Some(42),
                token: Some("tok-123".into()),
                message: None,
            }),
        ),
        (
            "failure".into(),
            ServerMessage::AuthResult(AuthResult {
                success: false,
                player_id: None,
                token: None,
                message: Some("Registration failed: ServerError".into()),
            }),
        ),
        (
            "".into(),
            ServerMessage::TickUpdate(TickUpdate {
                tick: 4821,
                player: Some(sample_player()),
                fleets: vec![
                    sample_fleet(1, h(3, -2), FleetStatus::Harvesting, Some(PolicyPreset::PatrolHome)),
                    sample_fleet(2, h(4, -2), FleetStatus::Docked, None),
                ],
                sector_updates: Some(vec![rich_sector(), bare_sector()]),
                homeworld_update: Some(sample_homeworld()),
                events: Some(vec![
                    GameEvent {
                        tick: 4821,
                        kind: EventKind::ResourceHarvested(ResourceHarvestedEvent {
                            fleet_id: 1,
                            resource_type: HarvestResource::Metal,
                            amount: 24.5,
                        }),
                    },
                    GameEvent {
                        tick: 4821,
                        kind: EventKind::Alert(AlertEvent {
                            level: AlertLevel::Warning,
                            message: "Low fuel".into(),
                            sector: Some(h(3, -2)),
                            fleet_id: Some(1),
                        }),
                    },
                ]),
            }),
        ),
        (
            "minimal".into(),
            ServerMessage::TickUpdate(TickUpdate {
                tick: 1,
                player: None,
                fleets: vec![],
                sector_updates: None,
                homeworld_update: None,
                events: None,
            }),
        ),
        (
            "empty_lists".into(),
            ServerMessage::TickUpdate(TickUpdate {
                tick: 2,
                player: None,
                fleets: vec![],
                sector_updates: Some(vec![]),
                homeworld_update: Some(idle_homeworld()),
                events: Some(vec![]),
            }),
        ),
        (
            "".into(),
            ServerMessage::FullState(GameState {
                tick: 4820,
                player: sample_player(),
                fleets: vec![
                    sample_fleet(1, h(3, -2), FleetStatus::Moving, Some(PolicyPreset::SalvageAndSites)),
                    sample_fleet(2, h(4, -2), FleetStatus::Exploring, None),
                    FleetState {
                        ships: vec![],
                        cooldown_remaining: 0,
                        ..sample_fleet(3, h(-5, 7), FleetStatus::InCombat, None)
                    },
                ],
                homeworld: sample_homeworld(),
                known_sectors: vec![rich_sector(), bare_sector(), anomaly_sector()],
            }),
        ),
        (
            "empty".into(),
            ServerMessage::FullState(GameState {
                tick: 0,
                player: PlayerState { id: 1, name: "New".into(), resources: res(0.0, 0.0, 0.0), homeworld: h(0, 0) },
                fleets: vec![],
                homeworld: idle_homeworld(),
                known_sectors: vec![],
            }),
        ),
        (
            "".into(),
            ServerMessage::Event(GameEvent {
                tick: 77,
                kind: EventKind::CombatEnded(CombatEndedEvent { sector: h(2, 2), player_victory: true }),
            }),
        ),
    ];
    for code in ALL_ERROR_CODES {
        v.push((
            error_code_name(code).to_string(),
            ServerMessage::Error(ErrorMessage { code, message: format!("sample {code:?}") }),
        ));
    }
    v
}

fn event_kinds() -> Vec<(&'static str, EventKind)> {
    vec![
        ("", EventKind::CombatRound(CombatRoundEvent {
            attacker_ship_id: 11,
            target_ship_id: 22,
            damage: 18.5,
            shield_absorbed: 10.0,
            hull_damage: 8.5,
            rapid_fire: true,
        })),
        ("", EventKind::ShipDestroyed(ShipDestroyedEvent {
            ship_id: 22,
            ship_class: ShipClass::Corvette,
            owner_fleet_id: 9001,
            is_npc: true,
        })),
        ("", EventKind::FleetDestroyed(FleetDestroyedEvent {
            fleet_id: 9001,
            is_npc: true,
            salvage: res(30.0, 10.0, 5.0),
        })),
        ("", EventKind::ResourceHarvested(ResourceHarvestedEvent {
            fleet_id: 1,
            resource_type: HarvestResource::Crystal,
            amount: 12.25,
        })),
        ("", EventKind::SectorEntered(SectorEnteredEvent { fleet_id: 1, sector: h(-2, 3), first_visit: true })),
        ("", EventKind::CombatStarted(CombatStartedEvent {
            player_fleet_id: 1,
            enemy_fleet_id: 9001,
            sector: h(5, -5),
        })),
        ("", EventKind::CombatEnded(CombatEndedEvent { sector: h(5, -5), player_victory: false })),
        ("", EventKind::SalvageCollected(SalvageCollectedEvent { fleet_id: 1, resources: res(5.0, 0.0, 1.5) })),
        ("", EventKind::FleetArrived(FleetArrivedEvent { fleet_id: 2, sector: h(0, -3) })),
        ("", EventKind::BuildingCompleted(BuildingCompletedEvent {
            building_type: BuildingType::SensorArray,
            new_level: 3,
            player_id: None,
        })),
        ("with_player", EventKind::BuildingCompleted(BuildingCompletedEvent {
            building_type: BuildingType::Shipyard,
            new_level: 1,
            player_id: Some(42),
        })),
        ("", EventKind::ResearchCompleted(ResearchCompletedEvent {
            tech: ResearchType::Navigation,
            new_level: 2,
            player_id: None,
        })),
        ("with_player", EventKind::ResearchCompleted(ResearchCompletedEvent {
            tech: ResearchType::HaulerTech,
            new_level: 1,
            player_id: Some(42),
        })),
        ("", EventKind::ShipBuilt(ShipBuiltEvent { ship_class: ShipClass::Cruiser, count: 2, player_id: None })),
        ("with_player", EventKind::ShipBuilt(ShipBuiltEvent {
            ship_class: ShipClass::Hauler,
            count: 1,
            player_id: Some(42),
        })),
        ("", EventKind::ScanCompleted(ScanCompletedEvent {
            fleet_id: 1,
            sector: h(1, 1),
            sectors_revealed: 7,
            hostiles_detected: 2,
            signals: vec![
                SignalContact { sector: h(4, 1), signal: SignalKind::RichOre },
                SignalContact { sector: h(-3, 2), signal: SignalKind::HostileMass },
                SignalContact { sector: h(0, 5), signal: SignalKind::Derelict },
                SignalContact { sector: h(6, -6), signal: SignalKind::Anomaly },
            ],
        })),
        ("no_signals", EventKind::ScanCompleted(ScanCompletedEvent {
            fleet_id: 1,
            sector: h(1, 1),
            sectors_revealed: 0,
            hostiles_detected: 0,
            signals: vec![],
        })),
        ("", EventKind::RaidIncoming(RaidIncomingEvent {
            player_id: 42,
            arrival_tick: 5000,
            threat: "moderate".into(),
        })),
        ("", EventKind::RaidResolved(RaidResolvedEvent {
            player_id: 42,
            defended: false,
            resources_lost: res(100.0, 50.0, 25.0),
            salvage_dropped: Some(res(10.0, 5.0, 0.0)),
            raid_power: 120.0,
            defense_power: 80.5,
        })),
        ("defended", EventKind::RaidResolved(RaidResolvedEvent {
            player_id: 42,
            defended: true,
            resources_lost: res(0.0, 0.0, 0.0),
            salvage_dropped: None,
            raid_power: 40.0,
            defense_power: 80.5,
        })),
        ("", EventKind::SiteExplorationStarted(SiteExplorationStartedEvent {
            fleet_id: 1,
            sector: h(2, -4),
            tier: 3,
            end_tick: 900,
        })),
        ("", EventKind::SiteExplored(SiteExploredEvent {
            fleet_id: 1,
            sector: h(2, -4),
            tier: 3,
            resources: res(200.0, 80.0, 40.0),
            recovered_ship: Some(ShipClass::Frigate),
            tech_cache: Some(ResearchType::AdvancedShields),
        })),
        ("minimal", EventKind::SiteExplored(SiteExploredEvent {
            fleet_id: 1,
            sector: h(2, -4),
            tier: 1,
            resources: res(10.0, 0.0, 0.0),
            recovered_ship: None,
            tech_cache: None,
        })),
        ("", EventKind::SiteAmbush(SiteAmbushEvent { fleet_id: 1, sector: h(2, -4), npc_fleet_id: 9100 })),
        ("", EventKind::PolicyAction(PolicyActionEvent {
            fleet_id: 3,
            preset: PolicyPreset::Prospect,
            action: "scan".into(),
            reason: "no unexplored neighbours known".into(),
        })),
        ("", EventKind::Alert(AlertEvent {
            level: AlertLevel::Info,
            message: "Uplink established".into(),
            sector: None,
            fleet_id: None,
        })),
        ("critical", EventKind::Alert(AlertEvent {
            level: AlertLevel::Critical,
            message: "Homeworld under attack".into(),
            sector: Some(h(4, -2)),
            fleet_id: Some(2),
        })),
    ]
}

fn fname(base: &str, label: &str) -> String {
    if label.is_empty() { base.to_string() } else { format!("{base}__{label}") }
}

/// Inputs a lenient peer may send (defaulted / omitted fields). The
/// canonical form is whatever Rust re-emits.
fn lenient_cases() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        ("stop_without_fleet", "Command", json!({"action": "stop"})),
        ("scan_without_fleet", "Command", json!({"action": "scan"})),
        ("explore_without_fleet", "Command", json!({"action": "explore_site"})),
        ("build_ship_default_count", "Command", json!({"action": "build_ship", "ship_class": "Scout"})),
        ("bare_tick_update", "ServerMessage", json!({"type": "tick_update", "tick": 9, "fleets": []})),
        (
            "fleet_without_cooldown",
            "ServerMessage",
            json!({"type": "tick_update", "tick": 9, "fleets": [{
                "id": 1, "location": {"q": 0, "r": 0}, "state": "Idle", "ships": [],
                "cargo": {"metal": 0.0, "crystal": 0.0, "deuterium": 0.0},
                "cargo_capacity": 20.0, "fuel": 1.0, "fuel_max": 2.0
            }]}),
        ),
        (
            "sector_brief_without_classes",
            "ServerMessage",
            json!({"type": "tick_update", "tick": 9, "fleets": [], "sector_updates": [{
                "location": {"q": 1, "r": 1}, "terrain": "Nebula",
                "resources": {"metal": "Sparse", "crystal": "None", "deuterium": "None"},
                "connections": [],
                "player_fleets": [{"id": 3, "owner_name": "x", "ship_count": 2}]
            }]}),
        ),
        (
            "scan_without_signals",
            "ServerMessage",
            json!({"type": "event", "tick": 3, "kind": "ScanCompleted", "fleet_id": 1,
                   "sector": {"q": 0, "r": 0}, "sectors_revealed": 1, "hostiles_detected": 0}),
        ),
        ("auth_without_token", "ClientMessage", json!({"type": "auth", "player_name": "x"})),
        ("bare_auth_result", "ServerMessage", json!({"type": "auth_result", "success": false})),
    ]
}

fn lenient_canonical(kind: &str, input: &Value) -> Value {
    fn go<T: Serialize + DeserializeOwned>(input: &Value) -> Value {
        let t: T = serde_json::from_value(input.clone()).expect("lenient input must parse");
        serde_json::to_value(&t).unwrap()
    }
    match kind {
        "Command" => go::<Command>(input),
        "ServerMessage" => go::<ServerMessage>(input),
        "ClientMessage" => go::<ClientMessage>(input),
        _ => unreachable!(),
    }
}

// ── Hex fixtures ──────────────────────────────────────────────────

fn hv(x: Hex) -> Value {
    json!({"q": x.q, "r": x.r})
}

fn hex_fixtures(files: &mut Files) {
    let centers = [h(0, 0), h(3, -2), h(-4, 5), h(-7, -1), h(12, -20), h(-30, 15), h(1, 0), h(0, -1)];

    // Directions, in HexDirection::ALL order.
    let dirs: Vec<Value> = HexDirection::ALL
        .iter()
        .map(|d| json!({"label": d.label(), "vec": hv(d.to_vec()), "opposite": d.opposite().label()}))
        .collect();
    put(files, "hex/directions.json".into(), &Value::Array(dirs));

    let neighbors: Vec<Value> = centers
        .iter()
        .map(|c| json!({"center": hv(*c), "neighbors": c.neighbors().iter().map(|n| hv(*n)).collect::<Vec<_>>()}))
        .collect();
    put(files, "hex/neighbors.json".into(), &Value::Array(neighbors));

    let pairs = [
        (h(0, 0), h(0, 0)),
        (h(0, 0), h(3, -1)),
        (h(2, -5), h(-1, 3)),
        (h(-4, 5), h(6, -2)),
        (h(-30, 15), h(12, -20)),
        (h(-7, -1), h(-7, 6)),
        (h(5, 5), h(-5, -5)),
        (h(1, 0), h(-1, 0)),
    ];
    let dist: Vec<Value> = pairs
        .iter()
        .map(|(a, b)| json!({"a": hv(*a), "b": hv(*b), "distance": Hex::distance(a, b)}))
        .collect();
    put(files, "hex/distance.json".into(), &Value::Array(dist));

    let origin: Vec<Value> = [h(0, 0), h(3, -1), h(-4, 5), h(-7, -1), h(14, -6), h(-20, 0)]
        .iter()
        .map(|c| json!({"hex": hv(*c), "s": c.s(), "dist_from_origin": c.dist_from_origin(), "key": c.to_key()}))
        .collect();
    put(files, "hex/origin_and_key.json".into(), &Value::Array(origin));

    // Rings: ORDERED cell lists (order is part of the contract), radii 0..=6.
    // Sanity-checked here too so a broken generator can never be "blessed".
    let mut rings = Vec::new();
    for c in &centers {
        for radius in 0..=6u16 {
            let cells: Vec<Hex> = hex_ring(*c, radius).collect();
            let expect = if radius == 0 { 1 } else { radius as usize * 6 };
            assert_eq!(cells.len(), expect, "ring size {c} r{radius}");
            let mut seen = std::collections::HashSet::new();
            for cell in &cells {
                assert_eq!(Hex::distance(c, cell), radius, "off-ring {cell} for {c} r{radius}");
                assert!(seen.insert(cell.to_key()), "duplicate ring cell {cell}");
            }
            if radius > 0 {
                for w in cells.windows(2) {
                    assert_eq!(Hex::distance(&w[0], &w[1]), 1, "ring not contiguous at {c} r{radius}");
                }
            }
            rings.push(json!({"center": hv(*c), "radius": radius, "cells": cells.iter().map(|x| hv(*x)).collect::<Vec<_>>()}));
        }
    }
    put(files, "hex/ring.json".into(), &Value::Array(rings));

    // Discs (spiral order): radii 0..=4.
    let mut discs = Vec::new();
    for c in &centers[..5] {
        for radius in 0..=4u16 {
            let cells: Vec<Hex> = hex_spiral(*c, radius).collect();
            let n = radius as usize;
            assert_eq!(cells.len(), 1 + 3 * n * (n + 1), "disc size {c} r{radius}");
            for cell in &cells {
                assert!(Hex::distance(c, cell) <= radius);
            }
            discs.push(json!({"center": hv(*c), "radius": radius, "cells": cells.iter().map(|x| hv(*x)).collect::<Vec<_>>()}));
        }
    }
    put(files, "hex/disc.json".into(), &Value::Array(discs));
}

// ── Generation ────────────────────────────────────────────────────

fn generate() -> Files {
    let mut f = Files::new();

    for (label, m) in client_messages() {
        put_msg(&mut f, "client_message", &fname(client_message_name(&m), label), &m);
    }
    for (label, c) in commands() {
        put_msg(&mut f, "command", &fname(command_name(&c), label), &c);
    }
    for (label, m) in server_messages() {
        put_msg(&mut f, "server_message", &fname(server_message_name(&m), &label), &m);
    }
    for (label, k) in event_kinds() {
        let name = fname(event_kind_name(&k), label);
        put_msg(&mut f, "event_kind", &name, &GameEvent { tick: 1234, kind: k });
    }

    // Every plain enum value, so Dart can check its wire names.
    let enums = json!({
        "ShipClass": ShipClass::ALL,
        "Density": [Density::None, Density::Sparse, Density::Moderate, Density::Rich, Density::Pristine],
        "TerrainType": [TerrainType::Empty, TerrainType::AsteroidField, TerrainType::Nebula, TerrainType::DebrisField, TerrainType::Anomaly],
        "FleetStatus": [FleetStatus::Idle, FleetStatus::Moving, FleetStatus::Harvesting, FleetStatus::InCombat, FleetStatus::Returning, FleetStatus::Docked, FleetStatus::Exploring],
        "NpcBehavior": [NpcBehavior::Passive, NpcBehavior::Patrol, NpcBehavior::Aggressive, NpcBehavior::Swarm],
        "SiteRisk": [SiteRisk::Quiet, SiteRisk::Uneasy, SiteRisk::Hot],
        "SignalKind": [SignalKind::RichOre, SignalKind::HostileMass, SignalKind::Derelict, SignalKind::Anomaly],
        "AlertLevel": [AlertLevel::Info, AlertLevel::Warning, AlertLevel::Critical],
        "HarvestResource": [HarvestResource::Metal, HarvestResource::Crystal, HarvestResource::Deuterium, HarvestResource::Auto],
        "QueueType": [QueueType::Building, QueueType::Ship, QueueType::Research],
        "PolicyPreset": PolicyPreset::ALL,
        "BuildingType": [BuildingType::MetalMine, BuildingType::CrystalMine, BuildingType::DeuteriumSynthesizer, BuildingType::Shipyard, BuildingType::ResearchLab, BuildingType::FuelDepot, BuildingType::SensorArray, BuildingType::DefenseGrid],
        "ResearchType": [ResearchType::FuelEfficiency, ResearchType::ExtendedFuelTanks, ResearchType::ReinforcedHulls, ResearchType::AdvancedShields, ResearchType::WeaponsResearch, ResearchType::Navigation, ResearchType::HarvestingEfficiency, ResearchType::CorvetteTech, ResearchType::FrigateTech, ResearchType::CruiserTech, ResearchType::HaulerTech, ResearchType::EmergencyJump],
        "ErrorCode": ALL_ERROR_CODES,
    });
    put(&mut f, "protocol/enums.json".into(), &enums);

    for (name, kind, input) in lenient_cases() {
        let canonical = lenient_canonical(kind, &input);
        put(
            &mut f,
            format!("protocol/lenient/{name}.json"),
            &json!({"type": kind, "input": input, "canonical": canonical}),
        );
    }

    hex_fixtures(&mut f);
    f
}

#[test]
fn golden_fixtures_match() {
    let files = generate();
    let root = fixtures_root();
    let update = std::env::var("UPDATE_FIXTURES").is_ok_and(|v| !v.is_empty() && v != "0");

    if update {
        for sub in ["protocol", "hex"] {
            let _ = fs::remove_dir_all(root.join(sub));
        }
        for (rel, content) in &files {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
        }
        eprintln!("wrote {} fixtures to {}", files.len(), root.display());
        return;
    }

    let mut problems = Vec::new();
    for (rel, expected) in &files {
        match fs::read_to_string(root.join(rel)) {
            Ok(actual) if actual == *expected => {}
            Ok(_) => problems.push(format!("changed: {rel}")),
            Err(_) => problems.push(format!("missing: {rel}")),
        }
    }
    for sub in ["protocol", "hex"] {
        for rel in files_under(&root.join(sub)) {
            let rel = format!("{sub}/{rel}");
            if !files.contains_key(&rel) {
                problems.push(format!("stale (no longer generated): {rel}"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "fixtures out of date (run with UPDATE_FIXTURES=1 and commit):\n  {}",
        problems.join("\n  ")
    );
}
