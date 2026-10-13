// Cross-language golden fixtures.
//
// Rust is the source of truth for the wire protocol and hex math. This test
// renders canonical JSON for every protocol variant (and a set of hex
// results) into `<repo>/fixtures/`. The Dart client loads the very same
// files (clients/web/test/), so a change on either side that breaks the
// contract fails a test.
//
// The Dart client's wire types are generated here too (`wire/`), into
// clients/web/lib/protocol/protocol.g.dart, and held to the same rule: the
// checked-in file must be what the Rust types generate today.
//
//   cargo test -p iac-shared --test golden_fixtures                 # verify
//   UPDATE_FIXTURES=1 cargo test -p iac-shared --test golden_fixtures   # regenerate both

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use iac_shared::Resources;
use iac_shared::constants::{Density, ResourceKind, ShipClass, TerrainType};
use iac_shared::hex::{Hex, HexDirection, hex_ring, hex_spiral};
use iac_shared::pace::Pace;
use iac_shared::protocol::*;
use iac_shared::scaling::{BuildingLevels, BuildingType, DefenceKind, RatioLabel, ResearchLevels, ResearchType, ShipyardItem};
use iac_shared::world::HubService;

mod wire;

use wire::model::Model;

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

fn sample_fleet(id: u64, loc: Hex, state: FleetStatus, policy: Option<StandingOrders>) -> FleetState {
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
        jump_fuel: 31.5,
        home_fuel: 63.0,
        hops_home: 2,
        jump_ticks: 2,
        cooldown_remaining: 3,
        power: 61.5,
        range_hops: 4,
        cargo_blocked: false,
        laden: false,
        at_outpost: false,
        outpost_fuel: None,
        policy,
    }
}

fn sample_outpost() -> OutpostState {
    OutpostState {
        location: h(10, -2),
        level: 1,
        max_level: 3,
        in_service: true,
        breached: false,
        work: Some(OutpostWorkState { kind: OutpostWork::Build, level: 2, start_tick: 4700, end_tick: 18_200 }),
        store: res(412.5, 96.0, 180.25),
        store_cap: res(9486.833, 4743.4165, 2371.7083),
        protected: res(412.5, 96.0, 180.25),
        production: res(0.010_794, 0.0, 0.005_397),
        yield_lost: vec![],
        grade: 3.0,
        platform_power: 40.0,
        defence_power: 172.0,
        raid_power: PowerRange { least: 38.3, most: 55.1 },
        raid_arrival_tick: Some(4990),
        next_level: Some(OutpostLevel {
            level: 3,
            cost: res(6250.0, 3125.0, 1250.0),
            ticks: 33_750,
            store_cap: res(59_292.707, 29_646.354, 14_823.177),
            platform_power: 250.0,
            production: res(0.032_382, 0.0, 0.016_191),
        }),
        upgrade_refusal: Some("your outpost at [10,-2] is building level 2 until tick 18200; it does one job at a time".into()),
        repair_cost: None,
        repair_waiting_for: None,
        hops_home: Some(7),
    }
}

fn breached_outpost() -> OutpostState {
    OutpostState {
        level: 3,
        in_service: false,
        breached: true,
        work: None,
        store: res(1200.0, 781.25, 0.0),
        store_cap: res(59_292.707, 29_646.354, 14_823.177),
        protected: res(1200.0, 781.25, 0.0),
        production: res(0.0, 0.0, 0.0),
        platform_power: 0.0,
        defence_power: 0.0,
        raid_arrival_tick: None,
        next_level: None,
        upgrade_refusal: Some(
            "your outpost at [10,-2] is breached; it repairs itself first, once its store covers the repair: 3360 metal, or \
             crystal at 1.5 and deuterium at 2 a unit in its place"
                .into(),
        ),
        repair_cost: Some(res(2187.5, 781.25, 0.0)),
        repair_waiting_for: Some(res(987.5, 0.0, 0.0)),
        hops_home: None,
        ..sample_outpost()
    }
}

fn outpost_sector(brief: OutpostBrief) -> SectorState {
    SectorState {
        location: h(10, -2),
        terrain: TerrainType::AsteroidField,
        resources: SectorResources { metal: Density::Rich, crystal: Density::None, deuterium: Density::Moderate },
        hostiles: None,
        player_fleets: None,
        salvage: None,
        salvage_despawn_tick: None,
        site: None,
        ore_reserve: Some(OreReserve {
            metal: TileReserve { units: 42.9, max_units: 42.9, refills_in_s: None },
            crystal: TileReserve { units: 0.0, max_units: 0.0, refills_in_s: None },
            deuterium: TileReserve { units: 19.5, max_units: 19.5, refills_in_s: None },
        }),
        outpost: Some(brief),
        threat: ThreatInfo { rating: 0, est_power: 0.0, basis: ThreatBasis::Observed },
        ..rich_sector()
    }
}

fn sample_survey() -> OutpostSurvey {
    OutpostSurvey {
        fleet_id: 7,
        sector: h(10, -2),
        ring: 10,
        site: Some(SurveySite {
            densities: SectorResources { metal: Density::Rich, crystal: Density::None, deuterium: Density::Moderate },
            grade: 3.0,
            production: res(0.021_652, 0.0, 0.010_826),
            hops_home: Some(7),
        }),
        cost: res(1000.0, 500.0, 200.0),
        build_ticks: 5400,
        raid_power: PowerRange { least: 38.3, most: 55.1 },
        platform_power: 40.0,
        refusal: None,
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
        salvage_despawn_tick: Some(4880),
        site: Some(SiteBrief { tier: 2, risk: SiteRisk::Uneasy }),
        ore_reserve: Some(OreReserve {
            metal: TileReserve { units: 9.5, max_units: 22.0, refills_in_s: Some(5400) },
            crystal: TileReserve { units: 10.0, max_units: 10.0, refills_in_s: None },
            deuterium: TileReserve { units: 0.0, max_units: 0.0, refills_in_s: None },
        }),
        chart: None,
        outpost: None,
        threat: ThreatInfo { rating: 4, est_power: 46.5, basis: ThreatBasis::Observed },
        last_seen: 4821,
        live: true,
        pins_stale: false,
    }
}

fn stale_pins_sector() -> SectorState {
    SectorState {
        location: h(5, 1),
        live: false,
        last_seen: 4700,
        pins_stale: true,
        chart: Some(ChartMark::Aboard),
        ..rich_sector()
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
        salvage_despawn_tick: None,
        site: None,
        ore_reserve: None,
        chart: None,
        outpost: None,
        threat: ThreatInfo { rating: 1, est_power: 4.0, basis: ThreatBasis::Estimate },
        last_seen: 4821,
        live: true,
        pins_stale: false,
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
        salvage: Some(res(60.0, 15.0, 9.0)),
        salvage_despawn_tick: Some(4300),
        site: Some(SiteBrief { tier: 5, risk: SiteRisk::Hot }),
        ore_reserve: Some(OreReserve {
            metal: TileReserve { units: 3.0, max_units: 3.0, refills_in_s: None },
            crystal: TileReserve { units: 41.0, max_units: 42.0, refills_in_s: Some(12000) },
            deuterium: TileReserve { units: 22.0, max_units: 22.0, refills_in_s: None },
        }),
        chart: Some(ChartMark::Delivered),
        outpost: None,
        threat: ThreatInfo { rating: 5, est_power: 65.0, basis: ThreatBasis::Template },
        last_seen: 4240,
        live: false,
        pins_stale: false,
    }
}

fn sample_homeworld() -> HomeworldState {
    use BuildingType::*;
    let buildings = [
        MetalMine, CrystalMine, DeuteriumSynthesizer, Shipyard, ResearchLab, FuelDepot,
        SensorArray, DefenseGrid, StorageVault, Fabricator,
    ]
    .iter()
    .enumerate()
    .map(|(i, b)| BuildingState { building_type: *b, level: i as u8 })
    .collect::<Vec<_>>();
    let mut levels = BuildingLevels::default();
    for b in &buildings {
        levels.set(b.building_type, b.level);
    }
    use ResearchType::*;
    let research = [
        FuelEfficiency, ExtendedFuelTanks, ReinforcedHulls, AdvancedShields, WeaponsResearch,
        Navigation, HarvestingEfficiency, CorvetteTech, FrigateTech, CruiserTech, HaulerTech,
        EmergencyJump, ModularFabrication,
    ]
    .iter()
    .enumerate()
    .map(|(i, t)| ResearchState { tech: *t, level: (i % 4) as u8 })
    .collect::<Vec<_>>();
    let mut researched = ResearchLevels::default();
    for r in &research {
        researched.set(r.tech, r.level);
    }
    HomeworldState {
        location: h(4, -2),
        production: res(2.75, 1.25, 0.4),
        storage: StorageState {
            cap: res(7500.0, 5250.0, 3750.0),
            protected: res(600.0, 420.0, 300.0),
            full_in_s: ResourceEta { metal: Some(5400), crystal: None, deuterium: Some(0) },
            capped: vec![ResourceKind::Deuterium],
        },
        buildings,
        research,
        build_queue: vec![
            BuildQueueItem { id: 311, building_type: BuildingType::CrystalMine, target_level: 5, start_tick: 100, end_tick: 460 },
            BuildQueueItem { id: 318, building_type: BuildingType::FuelDepot, target_level: 2, start_tick: 130, end_tick: 400 },
        ],
        build_pending: vec![
            QueuedBuild {
                id: 330,
                building_type: BuildingType::Shipyard,
                target_level: 3,
                reserve: true,
                cost: res(648.0, 324.0, 162.0),
                ticks: 1500,
                waiting_for: Some(res(0.0, 120.5, 0.0)),
                waiting_on: WaitReason::Resources,
                start_in: Some(95),
            },
            QueuedBuild {
                id: 331,
                building_type: BuildingType::CrystalMine,
                target_level: 6,
                reserve: false,
                cost: res(500.0, 250.0, 0.0),
                ticks: 900,
                waiting_for: None,
                waiting_on: WaitReason::Order,
                start_in: Some(95),
            },
        ],
        build_slots: 2,
        queue_waiting_max: 3,
        shipyard_pending: vec![
            QueuedShip {
                id: 335,
                item: ShipyardItem::Ship(ShipClass::Frigate),
                count: 2,
                cost: res(2000.0, 800.0, 400.0),
                unit_cost: res(1000.0, 400.0, 200.0),
                ticks: 600,
                waiting_on: BatchWait::Slot,
                start_in: Some(240),
            },
            QueuedShip {
                id: 336,
                item: ShipyardItem::Defence(DefenceKind::PulseTurret),
                count: 10,
                cost: res(1200.0, 200.0, 0.0),
                unit_cost: res(120.0, 20.0, 0.0),
                ticks: 70,
                waiting_on: BatchWait::Prerequisite,
                start_in: None,
            },
        ],
        research_pending: vec![QueuedResearch {
            id: 340,
            tech: ResearchType::CruiserTech,
            target_level: 1,
            reserve: false,
            cost: res(8000.0, 5000.0, 2500.0),
            ticks: 90000,
            waiting_for: Some(res(0.0, 0.0, 1200.0)),
            waiting_on: WaitReason::Resources,
            start_in: Some(1800),
        }],
        shipyard_queue: Some(ShipyardQueueItem {
            id: 322,
            item: ShipyardItem::Ship(ShipClass::Corvette),
            count: 2,
            built: 1,
            start_tick: 120,
            end_tick: 300,
        }),
        research_active: Some(ResearchItem {
            id: 325,
            tech: ResearchType::FuelEfficiency,
            target_level: 2,
            start_tick: 90,
            end_tick: 600,
        }),
        docked_ships: vec![ship(501, ShipClass::Cruiser, 100.0, 50.0, 18.0)],
        defences: vec![
            DefenceState { kind: DefenceKind::PulseTurret, count: 12, restoring: 2, power: 17.68 },
            DefenceState { kind: DefenceKind::LancerBattery, count: 0, restoring: 0, power: 53.04 },
        ],
        home_defence_power: 340.5,
        next_raid_estimate_power: 300.25,
        catalog: HomeworldCatalog::new(&levels, &researched, &[12, 0, 3], &Pace::PERSISTENT),
    }
}

fn idle_homeworld() -> HomeworldState {
    HomeworldState {
        location: h(0, 0),
        production: res(0.0, 0.0, 0.0),
        storage: StorageState {
            cap: res(5000.0, 3500.0, 2500.0),
            protected: res(0.0, 0.0, 0.0),
            full_in_s: ResourceEta { metal: None, crystal: None, deuterium: None },
            capped: vec![],
        },
        buildings: vec![],
        research: vec![],
        build_queue: vec![],
        build_pending: vec![],
        build_slots: 1,
        queue_waiting_max: 3,
        shipyard_queue: None,
        shipyard_pending: vec![],
        research_active: None,
        research_pending: vec![],
        docked_ships: vec![],
        defences: vec![],
        home_defence_power: 0.0,
        next_raid_estimate_power: 15.5,
        catalog: HomeworldCatalog::new(&BuildingLevels::default(), &ResearchLevels::default(), &[0; 3], &Pace::PERSISTENT),
    }
}

fn sample_player() -> PlayerState {
    PlayerState {
        id: 42,
        name: "Admiral".into(),
        resources: res(1200.5, 600.0, 150.25),
        homeworld: h(4, -2),
        exchange: sold_meter(),
    }
}

fn empty_meter() -> ExchangeMeter {
    ExchangeMeter { pressure: 0.0, span: 4185.0, fee: 0.25, window_ticks: 15_364, clear_in: Some(0) }
}

fn sold_meter() -> ExchangeMeter {
    ExchangeMeter { pressure: 632.5, span: 4185.0, fee: 0.325, window_ticks: 15_364, clear_in: Some(2322) }
}

fn sample_terms() -> ExchangeTerms {
    ExchangeTerms {
        fleet_id: 3,
        give: ResourceKind::Metal,
        want: ResourceKind::Deuterium,
        amount: 632.5,
        receive: 225.0,
        par: 316.25,
        fee: 0.2885,
        meter: empty_meter(),
        meter_after: sold_meter(),
    }
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
        Command::BuildDefence { .. } => "build_defence",
        Command::CancelBuild { .. } => "cancel_build",
        Command::CancelQueued { .. } => "cancel_queued",
        Command::PreviewMove { .. } => "preview_move",
        Command::Odds { .. } => "odds",
        Command::Leaderboard { .. } => "leaderboard",
        Command::Stop { .. } => "stop",
        Command::Scan { .. } => "scan",
        Command::ExploreSite { .. } => "explore_site",
        Command::Split { .. } => "split",
        Command::Merge { .. } => "merge",
        Command::Exchange { .. } => "exchange",
        Command::ExchangeQuote { .. } => "exchange_quote",
        Command::Load { .. } => "load",
        Command::Unload { .. } => "unload",
        Command::FoundOutpost { .. } => "found_outpost",
        Command::OutpostSurvey { .. } => "outpost_survey",
        Command::UpgradeOutpost => "upgrade_outpost",
        Command::AbandonOutpost => "abandon_outpost",
    }
}

fn server_message_name(m: &ServerMessage) -> &'static str {
    match m {
        ServerMessage::AuthResult(_) => "auth_result",
        ServerMessage::TickUpdate(_) => "tick_update",
        ServerMessage::FullState(_) => "full_state",
        ServerMessage::Event(_) => "event",
        ServerMessage::Error(_) => "error",
        ServerMessage::PreviewMove(_) => "preview_move",
        ServerMessage::Odds(_) => "odds",
        ServerMessage::Leaderboard(_) => "leaderboard",
        ServerMessage::ExchangeQuote(_) => "exchange_quote",
        ServerMessage::OutpostSurvey(_) => "outpost_survey",
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
        EventKind::DefenceBuilt(_) => "defence_built",
        EventKind::ScanCompleted(_) => "scan_completed",
        EventKind::RaidIncoming(_) => "raid_incoming",
        EventKind::RaidResolved(_) => "raid_resolved",
        EventKind::SiteExplorationStarted(_) => "site_exploration_started",
        EventKind::SiteExplored(_) => "site_explored",
        EventKind::SiteAmbush(_) => "site_ambush",
        EventKind::PolicyAction(_) => "policy_action",
        EventKind::Alert(_) => "alert",
        EventKind::SalvageDespawned(_) => "salvage_despawned",
        EventKind::StorageNearCap(_) => "storage_near_cap",
        EventKind::StorageFull(_) => "storage_full",
        EventKind::ChartDelivered(_) => "chart_delivered",
        EventKind::Queue(_) => "queue",
        EventKind::Exchanged(_) => "exchanged",
        EventKind::OutpostCompleted(_) => "outpost_completed",
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
        ErrorCode::StorageTooSmall => "storage_too_small",
        ErrorCode::DefenceLocked => "defence_locked",
        ErrorCode::InsufficientResources => "insufficient_resources",
        ErrorCode::AuthFailed => "auth_failed",
        ErrorCode::AlreadyAuthenticated => "already_authenticated",
        ErrorCode::InvalidName => "invalid_name",
        ErrorCode::TokenRequired => "token_required",
        ErrorCode::InvalidToken => "invalid_token",
        ErrorCode::WorldFull => "world_full",
        ErrorCode::ServerError => "server_error",
    }
}

const ALL_ERROR_CODES: [ErrorCode; 27] = [
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
    ErrorCode::StorageTooSmall,
    ErrorCode::DefenceLocked,
    ErrorCode::InsufficientResources,
    ErrorCode::AuthFailed,
    ErrorCode::AlreadyAuthenticated,
    ErrorCode::InvalidName,
    ErrorCode::TokenRequired,
    ErrorCode::InvalidToken,
    ErrorCode::WorldFull,
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
        ("", Command::Build { building_type: BuildingType::DefenseGrid, reserve: false }),
        ("reserve", Command::Build { building_type: BuildingType::ResearchLab, reserve: true }),
        ("", Command::Research { tech: ResearchType::EmergencyJump, reserve: false }),
        ("", Command::BuildShip { ship_class: ShipClass::Frigate, count: 3 }),
        ("", Command::BuildDefence { kind: DefenceKind::LancerBattery, count: 4 }),
        ("", Command::CancelBuild { queue_type: QueueType::Building, id: 311 }),
        ("ship", Command::CancelBuild { queue_type: QueueType::Ship, id: 322 }),
        ("research", Command::CancelBuild { queue_type: QueueType::Research, id: 325 }),
        ("", Command::CancelQueued { queue_type: QueueType::Building, id: 331 }),
        ("research", Command::CancelQueued { queue_type: QueueType::Research, id: 340 }),
        ("", Command::PreviewMove { fleet_id: 3, target: h(2, -5) }),
        ("", Command::Odds { fleet_id: 3, sectors: vec![h(2, -4), h(2, -5), h(3, -6)] }),
        ("", Command::Leaderboard { limit: 10 }),
        ("", Command::Stop { fleet_id: 6 }),
        ("", Command::Scan { fleet_id: 7 }),
        ("", Command::ExploreSite { fleet_id: 8 }),
        ("", Command::Split { fleet_id: 9, ship_ids: vec![101, 102] }),
        ("", Command::Merge { fleet_id: 9, other_fleet_id: 10 }),
        ("", Command::Exchange { fleet_id: 3, give: ResourceKind::Metal, want: ResourceKind::Deuterium, amount: Some(632.5) }),
        ("whole_hold", Command::Exchange { fleet_id: 3, give: ResourceKind::Crystal, want: ResourceKind::Metal, amount: None }),
        ("", Command::ExchangeQuote { fleet_id: 3, give: ResourceKind::Deuterium, want: ResourceKind::Crystal, amount: Some(120.0) }),
        ("whole_hold", Command::ExchangeQuote { fleet_id: 3, give: ResourceKind::Metal, want: ResourceKind::Deuterium, amount: None }),
        ("", Command::Load { fleet_id: 3, resource: ResourceKind::Metal, amount: Some(1200.5) }),
        ("all_that_fits", Command::Load { fleet_id: 3, resource: ResourceKind::Deuterium, amount: None }),
        ("", Command::Unload { fleet_id: 3 }),
        ("", Command::FoundOutpost { fleet_id: 7 }),
        ("", Command::OutpostSurvey { fleet_id: 7, sector: Some(h(10, -2)) }),
        ("own_sector", Command::OutpostSurvey { fleet_id: 7, sector: None }),
        ("", Command::UpgradeOutpost),
        ("", Command::AbandonOutpost),
    ]
}

fn client_messages() -> Vec<(&'static str, ClientMessage)> {
    vec![
        ("", ClientMessage::Auth(AuthRequest { player_name: "Admiral".into(), token: None, agent: false })),
        (
            "with_token",
            ClientMessage::Auth(AuthRequest { player_name: "Bot".into(), token: Some("9f2c4a7be1d8350c6a94f0b3d27e81c5a4b6e90f3d1c28a7b5e4f60918273645".into()), agent: true }),
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
                    fuel_budget_pct: 35,
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
                token: Some("9f2c4a7be1d8350c6a94f0b3d27e81c5a4b6e90f3d1c28a7b5e4f60918273645".into()),
                code: None,
                message: Some("account created; keep this token, it is your password".into()),
            }),
        ),
        (
            "returning".into(),
            ServerMessage::AuthResult(AuthResult {
                success: true,
                player_id: Some(42),
                token: None,
                code: None,
                message: None,
            }),
        ),
        (
            "token_required".into(),
            ServerMessage::AuthResult(AuthResult {
                success: false,
                player_id: None,
                token: None,
                code: Some(ErrorCode::TokenRequired),
                message: Some("player 'Admiral' is already registered; supply its token".into()),
            }),
        ),
        (
            "invalid_token".into(),
            ServerMessage::AuthResult(AuthResult {
                success: false,
                player_id: None,
                token: None,
                code: Some(ErrorCode::InvalidToken),
                message: Some("token rejected for player 'Admiral'".into()),
            }),
        ),
        (
            "invalid_name".into(),
            ServerMessage::AuthResult(AuthResult {
                success: false,
                player_id: None,
                token: None,
                code: Some(ErrorCode::InvalidName),
                message: Some("invalid player name: must be 3-24 characters long (got 2)".into()),
            }),
        ),
        (
            "world_full".into(),
            ServerMessage::AuthResult(AuthResult {
                success: false,
                player_id: None,
                token: None,
                code: Some(ErrorCode::WorldFull),
                message: Some("this world is full: all 24 home seats are taken".into()),
            }),
        ),
        (
            "".into(),
            ServerMessage::TickUpdate(TickUpdate {
                tick: 4821,
                player: Some(sample_player()),
                fleets: vec![
                    sample_fleet(1, h(3, -2), FleetStatus::Harvesting, Some(StandingOrders {
                        preset: PolicyPreset::PatrolHome,
                        params: PolicyParams::default(),
                    })),
                    FleetState { laden: true, ..sample_fleet(2, h(4, -2), FleetStatus::Docked, None) },
                ],
                sector_updates: Some(vec![rich_sector(), bare_sector()]),
                homeworld_update: Some(sample_homeworld()),
                outpost: None,
                events: Some(vec![
                    GameEvent {
                        tick: 4821,
                        kind: EventKind::ResourceHarvested(ResourceHarvestedEvent {
                            fleet_id: 1,
                            resources: res(24.5, 0.0, 0.0),
                            ticks: 10,
                        }),
                    },
                    GameEvent {
                        tick: 4821,
                        kind: EventKind::Alert(AlertEvent {
                            player_id: Some(3),
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
                outpost: None,
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
                outpost: None,
                events: Some(vec![]),
            }),
        ),
        (
            "outpost_breached".into(),
            ServerMessage::TickUpdate(TickUpdate {
                tick: 5000,
                player: None,
                fleets: vec![FleetState {
                    at_outpost: true,
                    home_fuel: 220.5,
                    ..sample_fleet(7, h(10, -2), FleetStatus::Idle, None)
                }],
                sector_updates: Some(vec![outpost_sector(OutpostBrief {
                    owner: "Admiral".into(),
                    level: 3,
                    breached: true,
                    mine: true,
                })]),
                homeworld_update: None,
                outpost: Some(breached_outpost()),
                events: Some(vec![GameEvent {
                    tick: 5000,
                    kind: EventKind::RaidResolved(RaidResolvedEvent {
                        player_id: 42,
                        outpost: Some(h(10, -2)),
                        defended: false,
                        breached: true,
                        resources_lost: res(0.0, 0.0, 0.0),
                        salvage_dropped: None,
                        salvage_despawn_tick: None,
                        raid_power: 310.5,
                        defense_power: 294.0,
                        structures_lost: 0,
                        structures_restored: 0,
                        protected_kept: res(1200.0, 781.25, 0.0),
                        ships_lost: 1,
                    }),
                }]),
            }),
        ),
        (
            "outpost_store_full".into(),
            ServerMessage::TickUpdate(TickUpdate {
                tick: 20_000,
                player: None,
                fleets: vec![],
                sector_updates: None,
                homeworld_update: None,
                outpost: Some(OutpostState {
                    work: None,
                    store: res(9486.833, 96.0, 180.25),
                    yield_lost: vec![ResourceKind::Metal],
                    raid_arrival_tick: None,
                    next_level: Some(OutpostLevel {
                        level: 2,
                        cost: res(2500.0, 1250.0, 500.0),
                        ticks: 13_500,
                        store_cap: res(23_717.082, 11_858.541, 5929.2705),
                        platform_power: 100.0,
                        production: res(0.021_588, 0.0, 0.010_794),
                    }),
                    upgrade_refusal: Some(
                        "outpost level 2 costs 2500 metal, 1250 crystal and 500 deuterium; the store at [10,-2] is short by \
                         1154 crystal and 320 deuterium. A level is paid in full from the outpost's own store when it is ordered"
                            .into(),
                    ),
                    ..sample_outpost()
                }),
                events: None,
            }),
        ),
        (
            "".into(),
            ServerMessage::FullState(GameState {
                tick: 4820,
                player: sample_player(),
                fleets: vec![
                    sample_fleet(1, h(3, -2), FleetStatus::Moving, Some(StandingOrders {
                        preset: PolicyPreset::SalvageAndSites,
                        params: PolicyParams {
                            min_fuel_pct: 25,
                            cargo_return_pct: 70,
                            max_range: 9,
                            engage_ratio_x10: 22,
                            fuel_budget_pct: 100,
                        },
                    })),
                    sample_fleet(2, h(4, -2), FleetStatus::Exploring, None),
                    FleetState {
                        ships: vec![],
                        cooldown_remaining: 0,
                        ..sample_fleet(3, h(-5, 7), FleetStatus::InCombat, None)
                    },
                ],
                homeworld: sample_homeworld(),
                outpost: None,
                known_sectors: vec![rich_sector(), bare_sector(), anomaly_sector(), stale_pins_sector()],
                world: WorldInfo::new(Pace::new(10.0).unwrap(), 2, 1),
            }),
        ),
        (
            "with_outpost".into(),
            ServerMessage::FullState(GameState {
                tick: 4820,
                player: sample_player(),
                fleets: vec![
                    FleetState {
                        at_outpost: true,
                        outpost_fuel: Some(0.0),
                        home_fuel: 220.5,
                        ..sample_fleet(7, h(10, -2), FleetStatus::Idle, None)
                    },
                    FleetState { outpost_fuel: Some(220.5), ..sample_fleet(2, h(4, -2), FleetStatus::Docked, None) },
                ],
                homeworld: idle_homeworld(),
                outpost: Some(sample_outpost()),
                known_sectors: vec![
                    outpost_sector(OutpostBrief { owner: "Admiral".into(), level: 1, breached: false, mine: true }),
                    SectorState {
                        location: h(14, -6),
                        live: false,
                        last_seen: 3100,
                        ..outpost_sector(OutpostBrief { owner: "Rival".into(), level: 0, breached: false, mine: false })
                    },
                ],
                world: WorldInfo::new(Pace::new(10.0).unwrap(), 4, 3),
            }),
        ),
        (
            "empty".into(),
            ServerMessage::FullState(GameState {
                tick: 0,
                player: PlayerState {
                    id: 1,
                    name: "New".into(),
                    resources: res(0.0, 0.0, 0.0),
                    homeworld: h(3, 0),
                    exchange: ExchangeMeter { pressure: 0.0, span: 0.0, fee: 0.75, window_ticks: 5238, clear_in: Some(0) },
                },
                fleets: vec![],
                homeworld: idle_homeworld(),
                outpost: None,
                known_sectors: vec![],
                world: WorldInfo::new(Pace::new(42.0).unwrap(), 2, 1),
            }),
        ),
        (
            "".into(),
            ServerMessage::Event(GameEvent {
                tick: 77,
                kind: EventKind::CombatEnded(CombatEndedEvent {
                    sector: h(2, 2),
                    player_victory: true,
                    owners: vec!["Admiral".into()],
                    mine: true,
                    salvage: Some(res(60.0, 15.0, 9.0)),
                    salvage_despawn_tick: Some(137),
                }),
            }),
        ),
    ];
    v.push((
        "".into(),
        ServerMessage::PreviewMove(MovePreview {
            fleet_id: 3,
            target: h(2, -5),
            fuel_cost: 31.5,
            fuel_after: 240.0,
            can_jump: true,
            hops_home: 3,
            charted_hops_home: Some(3),
            route_unexplored: false,
            fuel_to_return: 94.5,
            can_return: true,
            threat: ThreatInfo { rating: 3, est_power: 20.0, basis: ThreatBasis::Estimate },
            fleet_power: 60.0,
            ratio: 3.0,
            label: RatioLabel::Safe,
        }),
    ));
    v.push((
        "stranding".into(),
        ServerMessage::PreviewMove(MovePreview {
            fleet_id: 3,
            target: h(2, -6),
            fuel_cost: 31.5,
            fuel_after: 0.0,
            can_jump: false,
            hops_home: 4,
            charted_hops_home: Some(17),
            route_unexplored: true,
            fuel_to_return: 126.0,
            can_return: false,
            threat: ThreatInfo { rating: 4, est_power: 46.5, basis: ThreatBasis::Observed },
            fleet_power: 60.0,
            ratio: 1.29,
            label: RatioLabel::Even,
        }),
    ));
    v.push((
        "".into(),
        ServerMessage::Odds(FleetOdds {
            fleet_id: 3,
            sectors: vec![
                SectorOdds {
                    sector: h(2, -4),
                    threat: ThreatInfo { rating: 2, est_power: 8.9, basis: ThreatBasis::Template },
                    ratio: 10.0,
                    label: RatioLabel::Safe,
                },
                SectorOdds {
                    sector: h(2, -5),
                    threat: ThreatInfo { rating: 3, est_power: 20.0, basis: ThreatBasis::Estimate },
                    ratio: 1.62,
                    label: RatioLabel::Favourable,
                },
                SectorOdds {
                    sector: h(3, -6),
                    threat: ThreatInfo { rating: 4, est_power: 46.5, basis: ThreatBasis::Observed },
                    ratio: 0.84,
                    label: RatioLabel::Deadly,
                },
            ],
        }),
    ));
    v.push((
        "".into(),
        ServerMessage::Leaderboard(LeaderboardReply {
            tick: 4820,
            entries: vec![
                LeaderboardEntry { rank: 1, name: "Admiral".into(), agent: false, score: 412.5, core: 380.0, combat: 40.25, explore: 0.0 },
                LeaderboardEntry { rank: 2, name: "Claude-7".into(), agent: true, score: 389.0, core: 389.0, combat: 0.0, explore: 0.0 },
            ],
            you: Some(LeaderboardEntry { rank: 14, name: "Newbie".into(), agent: false, score: 3.5, core: 3.5, combat: 0.0, explore: 0.0 }),
            bots: vec![],
        }),
    ));
    v.push((
        "no_you".into(),
        ServerMessage::Leaderboard(LeaderboardReply { tick: 1, entries: vec![], you: None, bots: vec![] }),
    ));
    v.push((
        "with_bots".into(),
        ServerMessage::Leaderboard(LeaderboardReply {
            tick: 4820,
            entries: vec![
                LeaderboardEntry { rank: 1, name: "Admiral".into(), agent: false, score: 412.5, core: 380.0, combat: 40.25, explore: 0.0 },
            ],
            you: None,
            bots: vec![
                LeaderboardEntry { rank: 1, name: "BOT-explorer-2".into(), agent: false, score: 505.0, core: 404.0, combat: 3.5, explore: 310.0 },
                LeaderboardEntry { rank: 2, name: "BOT-turtle-1".into(), agent: false, score: 96.25, core: 96.25, combat: 0.0, explore: 0.0 },
            ],
        }),
    ));
    v.push(("".into(), ServerMessage::ExchangeQuote(ExchangeQuote { terms: sample_terms(), refusal: None })));
    v.push((
        "refused".into(),
        ServerMessage::ExchangeQuote(ExchangeQuote {
            terms: ExchangeTerms {
                give: ResourceKind::Deuterium,
                want: ResourceKind::Metal,
                amount: 120.0,
                receive: 85.0,
                par: 240.0,
                fee: 0.6458,
                meter: ExchangeMeter { pressure: 9000.0, span: 0.0, fee: 0.75, window_ticks: 15_364, clear_in: None },
                meter_after: ExchangeMeter { pressure: 9240.0, span: 0.0, fee: 0.75, window_ticks: 15_364, clear_in: None },
                ..sample_terms()
            },
            refusal: Some(
                "fleet 3 is at [3,-2], 3 hexes from the exchange at the hub [0,0]; a fleet trades what is in its hold, on the spot"
                    .into(),
            ),
        }),
    ));
    v.push(("".into(), ServerMessage::OutpostSurvey(sample_survey())));
    v.push((
        "refused".into(),
        ServerMessage::OutpostSurvey(OutpostSurvey {
            fleet_id: 1,
            sector: h(4, -2),
            ring: 4,
            site: Some(SurveySite {
                densities: SectorResources { metal: Density::None, crystal: Density::None, deuterium: Density::None },
                grade: 0.0,
                production: res(0.0, 0.0, 0.0),
                hops_home: Some(0),
            }),
            raid_power: PowerRange { least: 10.5, most: 15.1 },
            refusal: Some("fleet 1 is at ring 4; an outpost stands in rings 9 to 20".into()),
            ..sample_survey()
        }),
    ));
    v.push((
        "far".into(),
        ServerMessage::OutpostSurvey(OutpostSurvey {
            site: sample_survey().site.map(|site| SurveySite { hops_home: None, ..site }),
            refusal: Some(
                "fleet 7 is at [4,-2], 6 hexes from [10,-2]; an outpost is founded by a fleet standing in its sector".into(),
            ),
            ..sample_survey()
        }),
    ));
    v.push((
        "unseen".into(),
        ServerMessage::OutpostSurvey(OutpostSurvey {
            site: None,
            refusal: Some("[10,-2] is not on your chart; a survey reads the site of a sector you have seen".into()),
            ..sample_survey()
        }),
    ));
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
            sector: h(5, -5),
            attacker_fleet_id: 1,
            attacker_owner: Some("Admiral".into()),
            attacker_ship_id: 11,
            target_fleet_id: 9001,
            target_owner: None,
            target_ship_id: 22,
            damage: 18.5,
            shield_absorbed: 10.0,
            hull_damage: 8.5,
            rapid_fire: true,
            mine: true,
        })),
        ("", EventKind::ShipDestroyed(ShipDestroyedEvent {
            ship_id: 22,
            ship_class: ShipClass::Corvette,
            owner_fleet_id: 9001,
            is_npc: true,
            sector: h(5, -5),
            owner: None,
            mine: false,
        })),
        ("", EventKind::FleetDestroyed(FleetDestroyedEvent {
            fleet_id: 9001,
            is_npc: true,
            sector: h(5, -5),
            owner: None,
            mine: false,
            salvage: res(60.0, 15.0, 9.0),
            label: "3x corvette pack".into(),
        })),
        ("player_fleet", EventKind::FleetDestroyed(FleetDestroyedEvent {
            fleet_id: 4,
            is_npc: false,
            sector: h(5, -5),
            owner: Some("Admiral".into()),
            mine: true,
            salvage: res(0.0, 0.0, 0.0),
            label: String::new(),
        })),
        ("", EventKind::ResourceHarvested(ResourceHarvestedEvent {
            fleet_id: 1,
            resources: res(12.25, 6.0, 0.0),
            ticks: 10,
        })),
        ("", EventKind::SectorEntered(SectorEnteredEvent { fleet_id: 1, sector: h(-2, 3), first_visit: true })),
        ("", EventKind::CombatStarted(CombatStartedEvent {
            player_fleet_id: 1,
            owner: "Admiral".into(),
            enemy_fleet_id: 4294967400,
            enemy: "scout patrol".into(),
            sector: h(5, -5),
            mine: true,
        })),
        ("", EventKind::CombatEnded(CombatEndedEvent {
            sector: h(5, -5),
            player_victory: false,
            owners: vec!["Admiral".into(), "Rival".into()],
            mine: false,
            salvage: None,
            salvage_despawn_tick: None,
        })),
        ("", EventKind::SalvageCollected(SalvageCollectedEvent {
            fleet_id: 1,
            sector: h(5, -5),
            resources: res(5.0, 0.0, 1.5),
            remaining: None,
        })),
        ("partial", EventKind::SalvageCollected(SalvageCollectedEvent {
            fleet_id: 1,
            sector: h(5, -5),
            resources: res(40.0, 0.0, 0.0),
            remaining: Some(res(20.0, 15.0, 9.0)),
        })),
        ("", EventKind::SalvageDespawned(SalvageDespawnedEvent {
            sector: h(5, -5),
            resources: res(60.0, 15.0, 9.0),
        })),
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
        ("", EventKind::DefenceBuilt(DefenceBuiltEvent { defence: DefenceKind::IonBastion, count: 1, player_id: None })),
        ("with_player", EventKind::DefenceBuilt(DefenceBuiltEvent {
            defence: DefenceKind::PulseTurret,
            count: 1,
            player_id: Some(42),
        })),
        ("", EventKind::ScanCompleted(ScanCompletedEvent {
            fleet_id: 1,
            sector: h(1, 1),
            sectors_revealed: 7,
            hostiles_detected: 2,
            threats: vec![
                SectorThreat { sector: h(2, 1), threat: ThreatInfo { rating: 2, est_power: 7.5, basis: ThreatBasis::Observed } },
                SectorThreat { sector: h(1, 2), threat: ThreatInfo { rating: 1, est_power: 4.9, basis: ThreatBasis::Estimate } },
            ],
            signals: vec![
                SignalContact { sector: h(4, 1), signal: SignalKind::RichOre, threat_band: 1 },
                SignalContact { sector: h(-3, 2), signal: SignalKind::HostileMass, threat_band: 2 },
                SignalContact { sector: h(0, 5), signal: SignalKind::Derelict, threat_band: 1 },
                SignalContact { sector: h(6, -6), signal: SignalKind::Anomaly, threat_band: 3 },
            ],
        })),
        ("no_signals", EventKind::ScanCompleted(ScanCompletedEvent {
            fleet_id: 1,
            sector: h(1, 1),
            sectors_revealed: 0,
            hostiles_detected: 0,
            threats: vec![],
            signals: vec![],
        })),
        ("", EventKind::RaidIncoming(RaidIncomingEvent {
            player_id: 42,
            arrival_tick: 5000,
            threat: "moderate".into(),
            est_power: 353.5,
            outpost: None,
        })),
        ("outpost", EventKind::RaidIncoming(RaidIncomingEvent {
            player_id: 42,
            arrival_tick: 4990,
            threat: "light".into(),
            est_power: 41.5,
            outpost: Some(h(10, -2)),
        })),
        ("outpost", EventKind::RaidResolved(RaidResolvedEvent {
            player_id: 42,
            outpost: Some(h(10, -2)),
            defended: false,
            breached: true,
            resources_lost: res(40.625, 24.0, 32.5),
            salvage_dropped: None,
            salvage_despawn_tick: None,
            raid_power: 82.5,
            defense_power: 62.0,
            structures_lost: 0,
            structures_restored: 0,
            protected_kept: res(537.5, 0.0, 0.0),
            ships_lost: 1,
        })),
        ("outpost_defended", EventKind::RaidResolved(RaidResolvedEvent {
            player_id: 42,
            outpost: Some(h(10, -2)),
            defended: true,
            breached: false,
            resources_lost: res(0.0, 0.0, 0.0),
            salvage_dropped: None,
            salvage_despawn_tick: None,
            raid_power: 41.5,
            defense_power: 172.0,
            structures_lost: 0,
            structures_restored: 0,
            protected_kept: res(0.0, 0.0, 0.0),
            ships_lost: 0,
        })),
        ("", EventKind::OutpostCompleted(OutpostCompletedEvent { player_id: 42, sector: h(10, -2), level: 1 })),
        ("", EventKind::RaidResolved(RaidResolvedEvent {
            player_id: 42,
            outpost: None,
            defended: false,
            breached: false,
            resources_lost: res(100.0, 50.0, 25.0),
            salvage_dropped: Some(res(10.0, 5.0, 0.0)),
            salvage_despawn_tick: Some(4940),
            raid_power: 120.0,
            defense_power: 80.5,
            structures_lost: 6,
            structures_restored: 0,
            protected_kept: res(2400.0, 1680.0, 0.0),
            ships_lost: 3,
        })),
        ("defended", EventKind::RaidResolved(RaidResolvedEvent {
            player_id: 42,
            outpost: None,
            defended: true,
            breached: false,
            resources_lost: res(0.0, 0.0, 0.0),
            salvage_dropped: None,
            salvage_despawn_tick: None,
            raid_power: 40.0,
            defense_power: 80.5,
            structures_lost: 3,
            structures_restored: 2,
            protected_kept: res(0.0, 0.0, 0.0),
            ships_lost: 0,
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
            relic: true,
            points: 25.2,
        })),
        ("", EventKind::ChartDelivered(ChartDeliveredEvent { fleet_id: 1, sectors: 12, points: 0.4 })),
        ("", EventKind::Exchanged(ExchangedEvent { sector: h(0, 0), terms: sample_terms() })),
        ("started", EventKind::Queue(QueueEvent {
            player_id: Some(7),
            queue_type: QueueType::Building,
            id: 311,
            item: "Metal Mine Lv.5".into(),
            action: QueueAction::Started,
            paid: res(759.0, 190.0, 0.0),
            refunded: res(0.0, 0.0, 0.0),
            waiting_for: None,
            waiting_on: None,
            start_in: None,
        })),
        ("waiting", EventKind::Queue(QueueEvent {
            player_id: None,
            queue_type: QueueType::Building,
            id: 330,
            item: "Shipyard Lv.3".into(),
            action: QueueAction::Waiting,
            paid: res(0.0, 0.0, 0.0),
            refunded: res(0.0, 0.0, 0.0),
            waiting_for: Some(res(300.0, 0.0, 80.0)),
            waiting_on: Some(WaitReason::Resources),
            start_in: Some(42),
        })),
        ("batch_waiting", EventKind::Queue(QueueEvent {
            player_id: Some(7),
            queue_type: QueueType::Ship,
            id: 335,
            item: "Corvette x3".into(),
            action: QueueAction::Waiting,
            paid: res(1200.0, 300.0, 180.0),
            refunded: res(0.0, 0.0, 0.0),
            waiting_for: None,
            waiting_on: Some(WaitReason::Slot),
            start_in: Some(240),
        })),
        ("cancelled", EventKind::Queue(QueueEvent {
            player_id: Some(7),
            queue_type: QueueType::Research,
            id: 340,
            item: "Cruiser Tech Lv.1".into(),
            action: QueueAction::Cancelled,
            paid: res(0.0, 0.0, 0.0),
            refunded: res(4000.0, 2500.0, 1250.0),
            waiting_for: None,
            waiting_on: None,
            start_in: None,
        })),
        ("minimal", EventKind::SiteExplored(SiteExploredEvent {
            fleet_id: 1,
            sector: h(2, -4),
            tier: 1,
            resources: res(10.0, 0.0, 0.0),
            recovered_ship: None,
            tech_cache: None,
            relic: false,
            points: 0.0,
        })),
        ("", EventKind::SiteAmbush(SiteAmbushEvent { fleet_id: 1, sector: h(2, -4), npc_fleet_id: 9100 })),
        ("", EventKind::PolicyAction(PolicyActionEvent {
            fleet_id: 3,
            preset: PolicyPreset::Prospect,
            action: "scan".into(),
            reason: "no unexplored neighbours known".into(),
        })),
        ("", EventKind::Alert(AlertEvent {
            player_id: None,
            level: AlertLevel::Info,
            message: "Uplink established".into(),
            sector: None,
            fleet_id: None,
        })),
        ("", EventKind::StorageNearCap(StorageNearCapEvent {
            player_id: Some(42),
            resource: ResourceKind::Metal,
            ratio: 0.85,
            full_in_s: Some(5400),
        })),
        ("not_filling", EventKind::StorageNearCap(StorageNearCapEvent {
            player_id: None,
            resource: ResourceKind::Crystal,
            ratio: 0.9,
            full_in_s: None,
        })),
        ("", EventKind::StorageFull(StorageFullEvent { player_id: Some(42), resource: ResourceKind::Deuterium })),
        ("critical", EventKind::Alert(AlertEvent {
            player_id: Some(1),
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
                "cargo_capacity": 20.0, "fuel": 1.0, "fuel_max": 2.0, "jump_ticks": 2, "hops_home": 0
            }]}),
        ),
        (
            "sector_brief_without_classes",
            "ServerMessage",
            json!({"type": "tick_update", "tick": 9, "fleets": [], "sector_updates": [{
                "location": {"q": 1, "r": 1}, "terrain": "Nebula",
                "resources": {"metal": "Sparse", "crystal": "None", "deuterium": "None"},
                "connections": [], "last_seen": 9, "live": true,
                "threat": {"rating": 1, "est_power": 4.0, "basis": "estimate"},
                "player_fleets": [{"id": 3, "owner_name": "x", "ship_count": 2}]
            }]}),
        ),
        (
            "scan_without_signals",
            "ServerMessage",
            json!({"type": "event", "tick": 3, "kind": "ScanCompleted", "fleet_id": 1,
                   "sector": {"q": 0, "r": 0}, "sectors_revealed": 1, "hostiles_detected": 0}),
        ),
        (
            "policy_partial_params",
            "ClientMessage",
            json!({"type": "policy_update", "fleet_id": 3, "preset": "mine_and_return", "params": {"max_range": 2}}),
        ),
        (
            "policy_empty_params",
            "ClientMessage",
            json!({"type": "policy_update", "fleet_id": 3, "preset": "prospect", "params": {}}),
        ),
        ("build_without_reserve", "Command", json!({"action": "build", "building_type": "MetalMine"})),
        ("build_defence_default_count", "Command", json!({"action": "build_defence", "kind": "PulseTurret"})),
        ("raid_incoming_without_power", "ServerMessage",
            json!({"type": "event", "tick": 3, "kind": "RaidIncoming", "player_id": 1, "arrival_tick": 90, "threat": "light"})),
        ("raid_resolved_without_breached", "ServerMessage",
            json!({"type": "event", "tick": 3, "kind": "RaidResolved", "player_id": 1, "defended": true,
                   "resources_lost": {"metal": 0.0, "crystal": 0.0, "deuterium": 0.0},
                   "raid_power": 40.0, "defense_power": 80.0})),
        (
            "outpost_brief_without_mine",
            "ServerMessage",
            json!({"type": "tick_update", "tick": 9, "fleets": [], "sector_updates": [{
                "location": {"q": 10, "r": -2}, "terrain": "Empty",
                "resources": {"metal": "None", "crystal": "None", "deuterium": "None"},
                "connections": [], "last_seen": 9, "live": true,
                "threat": {"rating": 1, "est_power": 0.0, "basis": "observed"},
                "outpost": {"owner": "Rival", "level": 2, "breached": false}
            }]}),
        ),
        ("survey_without_sector", "Command", json!({"action": "outpost_survey", "fleet_id": 7})),
        ("leaderboard_default_limit", "Command", json!({"action": "leaderboard"})),
        ("auth_without_token", "ClientMessage", json!({"type": "auth", "player_name": "x"})),
        ("bare_auth_result", "ServerMessage", json!({"type": "auth_result", "success": false})),
        (
            "player_without_exchange",
            "ServerMessage",
            json!({"type": "tick_update", "tick": 9, "fleets": [], "player": {
                "id": 1, "name": "x", "resources": {"metal": 1.0, "crystal": 2.0, "deuterium": 3.0},
                "homeworld": {"q": 3, "r": 0}
            }}),
        ),
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
        "ChartMark": [ChartMark::Aboard, ChartMark::Delivered],
        "OutpostWork": [OutpostWork::Build, OutpostWork::Repair],
        "SignalKind": [SignalKind::RichOre, SignalKind::HostileMass, SignalKind::Derelict, SignalKind::Anomaly],
        "AlertLevel": [AlertLevel::Info, AlertLevel::Warning, AlertLevel::Critical],
        "HarvestResource": [HarvestResource::Metal, HarvestResource::Crystal, HarvestResource::Deuterium, HarvestResource::Auto],
        "DefenceKind": DefenceKind::ALL,
        "QueueType": [QueueType::Building, QueueType::Ship, QueueType::Research],
        "QueueAction": [QueueAction::Started, QueueAction::Waiting, QueueAction::Cancelled],
        "WaitReason": [WaitReason::Slot, WaitReason::Resources, WaitReason::Prerequisite, WaitReason::Order],
        "BatchWait": [BatchWait::Slot, BatchWait::Prerequisite],
        "PolicyPreset": PolicyPreset::ALL,
        "BuildingType": [BuildingType::MetalMine, BuildingType::CrystalMine, BuildingType::DeuteriumSynthesizer, BuildingType::Shipyard, BuildingType::ResearchLab, BuildingType::FuelDepot, BuildingType::SensorArray, BuildingType::DefenseGrid, BuildingType::StorageVault, BuildingType::Fabricator],
        "ResourceKind": ResourceKind::ALL,
        "HubService": [HubService::Exchange],
        "ResearchType": [ResearchType::FuelEfficiency, ResearchType::ExtendedFuelTanks, ResearchType::ReinforcedHulls, ResearchType::AdvancedShields, ResearchType::WeaponsResearch, ResearchType::Navigation, ResearchType::HarvestingEfficiency, ResearchType::CorvetteTech, ResearchType::FrigateTech, ResearchType::CruiserTech, ResearchType::HaulerTech, ResearchType::EmergencyJump, ResearchType::ModularFabrication],
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

    // Edges 0.9 and 1.1 are not exact in f32. The client's thresholds are the
    // same f32 values and it rounds a ratio to f32 first, so the edges
    // themselves are cases too.
    let ratios = [0.0, 0.5, 0.89, 0.9, 0.91, 1.0, 1.09, 1.1, 1.11, 1.3, 1.49, 1.5, 2.0, 2.49, 2.5, 4.0, 40.0];
    let labels: Vec<Value> = ratios
        .iter()
        .map(|&r| json!({"ratio": r, "label": serde_json::to_value(iac_shared::scaling::ratio_label(r)).unwrap()}))
        .collect();
    put(&mut f, "protocol/ratio_labels.json".into(), &Value::Array(labels));

    synth_fixtures(&mut f);
    hex_fixtures(&mut f);
    f
}

fn wire_model() -> Model {
    Model::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &wire::ROOTS)
}

/// The messages `wire::samples` makes from the model, one file for each
/// variant, one message a line: every type, variant and field of the wire,
/// which the fixtures written by hand above do not promise to cover.
fn synth_fixtures(files: &mut Files) {
    let mut by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for case in wire::samples::prove(&wire_model()).cases {
        let dir = if case.root == "ClientMessage" { "client_message" } else { "server_message" };
        by_file
            .entry(format!("protocol/synth/{dir}/{}.json", case.tags.join("__")))
            .or_default()
            .push(json!({"input": case.input, "canonical": case.canonical}).to_string());
    }
    for (rel, cases) in by_file {
        files.insert(rel, format!("[\n{}\n]\n", cases.join(",\n")));
    }
}

fn updating() -> bool {
    std::env::var("UPDATE_FIXTURES").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// The Dart client's wire types are written from the Rust ones and checked
/// in, so the client builds without cargo. This fails while they are stale.
#[test]
fn dart_protocol_matches() {
    let model = wire_model();
    let proven = wire::samples::prove(&model);
    let expected = wire::dart::render(&model, &proven.defaults, &wire::extras(&model));
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(wire::dart::PATH);

    if updating() {
        fs::write(&path, &expected).unwrap();
        eprintln!("wrote {}", path.display());
        return;
    }

    let actual = fs::read_to_string(&path).unwrap_or_default();
    let differs = expected.lines().zip(actual.lines()).position(|(want, have)| want != have);
    let line = differs.unwrap_or(expected.lines().count().min(actual.lines().count()));
    assert!(
        actual == expected,
        "{} is out of date with shared/src, first at line {}:\n  generated: {}\n  checked in: {}\n\
         run `UPDATE_FIXTURES=1 cargo test -p iac-shared`, then fix what `flutter analyze` points at in clients/web",
        wire::dart::PATH,
        line + 1,
        expected.lines().nth(line).unwrap_or("(end of file)"),
        actual.lines().nth(line).unwrap_or("(end of file)"),
    );
}

#[test]
fn golden_fixtures_match() {
    let files = generate();
    let root = fixtures_root();
    let update = updating();

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
