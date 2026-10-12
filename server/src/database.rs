// SQLite persistence layer using rusqlite.
// Mirrors the Zig database.zig schema and operations.

use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusqlite::{Connection, params, OptionalExtension};
use log::{debug, error, info, warn};

use iac_shared::hex::Hex;
use iac_shared::constants::{Density, Resources, ShipClass, ECONOMY_VERSION, WORLDGEN_VERSION};
use iac_shared::pace::Pace;
use iac_shared::scaling::{BuildingType, BuildingLevels, DefenceKind, ResearchType, ResearchLevels, ShipyardItem};

use iac_sim::engine::{
    Player, Fleet, Ship, SectorOverride, BuildQueueEntry, ShipQueueEntry, ResearchQueueEntry, FleetStatus, FleetPolicy,
    PendingBuilding, PendingResearch, PendingShip, Defences,
};
use iac_shared::protocol::{HarvestResource, PolicyPreset, PolicyParams, SectorState};
use iac_sim::host::Stale;
use iac_sim::persist::{
    ExploreCredit, FleetClocks, KnownRow, Persist, PersistBatch, RuntimeEntry, RuntimeKind, RuntimeRow, WorldMeta,
};
use iac_sim::snapshot::Snapshot;

/// Float storage: the value times 1000, in a REAL column. An f32 times 1000
/// is exact in an f64 and divides back to the same f32, so what is stored
/// reloads bit for bit.
fn float_to_stored(val: f32) -> f64 {
    f64::from(val) * 1000.0
}

// Read as f64: SQLite keeps a whole-numbered REAL as an integer, and rusqlite
// converts both losslessly, where an i64 read fails with InvalidColumnType.
fn stored_to_float(val: f64) -> f32 {
    (val / 1000.0) as f32
}

fn density_to_int(d: Option<Density>) -> Option<i64> {
    d.map(|x| x as i64)
}

fn int_to_density(v: Option<i64>) -> Option<Density> {
    v.and_then(|val| {
        if !(0..=4).contains(&val) { return None; }
        Some(match val {
            0 => Density::None,
            1 => Density::Sparse,
            2 => Density::Moderate,
            3 => Density::Rich,
            4 => Density::Pristine,
            _ => unreachable!(),
        })
    })
}

fn parse_fleet_status(s: &str) -> FleetStatus {
    match s {
        "idle" => FleetStatus::Idle,
        "moving" => FleetStatus::Moving,
        "harvesting" => FleetStatus::Harvesting,
        "in_combat" => FleetStatus::InCombat,
        "returning" => FleetStatus::Returning,
        "docked" => FleetStatus::Docked,
        "exploring" => FleetStatus::Exploring,
        _ => FleetStatus::Idle,
    }
}

fn fleet_status_to_str(s: FleetStatus) -> &'static str {
    match s {
        FleetStatus::Idle => "idle",
        FleetStatus::Moving => "moving",
        FleetStatus::Harvesting => "harvesting",
        FleetStatus::InCombat => "in_combat",
        FleetStatus::Returning => "returning",
        FleetStatus::Docked => "docked",
        FleetStatus::Exploring => "exploring",
    }
}

fn harvest_resource_to_str(r: HarvestResource) -> &'static str {
    match r {
        HarvestResource::Metal => "metal",
        HarvestResource::Crystal => "crystal",
        HarvestResource::Deuterium => "deuterium",
        HarvestResource::Auto => "auto",
    }
}

fn parse_harvest_resource(s: &str) -> HarvestResource {
    match s {
        "metal" => HarvestResource::Metal,
        "crystal" => HarvestResource::Crystal,
        "deuterium" => HarvestResource::Deuterium,
        _ => HarvestResource::Auto,
    }
}

fn policy_preset_to_str(p: PolicyPreset) -> &'static str {
    match p {
        PolicyPreset::Manual => "manual",
        PolicyPreset::Prospect => "prospect",
        PolicyPreset::MineAndReturn => "mine_and_return",
        PolicyPreset::SalvageAndSites => "salvage_and_sites",
        PolicyPreset::PatrolHome => "patrol_home",
    }
}

fn parse_policy_preset(s: &str) -> PolicyPreset {
    match s {
        "prospect" => PolicyPreset::Prospect,
        "mine_and_return" => PolicyPreset::MineAndReturn,
        "salvage_and_sites" => PolicyPreset::SalvageAndSites,
        "patrol_home" => PolicyPreset::PatrolHome,
        _ => PolicyPreset::Manual,
    }
}

/// Queue rows store ships as ("ship", class index) and structures as
/// ("defence", kind index).
fn shipyard_item_to_row(item: ShipyardItem) -> (&'static str, i64) {
    match item {
        ShipyardItem::Ship(c) => ("ship", c as i64),
        ShipyardItem::Defence(d) => ("defence", d as i64),
    }
}

fn shipyard_item_from_row(queue_type: &str, index: usize) -> Option<ShipyardItem> {
    match queue_type {
        "ship" => ShipClass::ALL.get(index).copied().map(ShipyardItem::Ship),
        _ => DefenceKind::from_usize(index).map(ShipyardItem::Defence),
    }
}

fn parse_ship_class(s: &str) -> ShipClass {
    match s {
        "scout" => ShipClass::Scout,
        "corvette" => ShipClass::Corvette,
        "frigate" => ShipClass::Frigate,
        "cruiser" => ShipClass::Cruiser,
        "hauler" => ShipClass::Hauler,
        _ => ShipClass::Scout,
    }
}

fn ship_class_to_str(s: ShipClass) -> &'static str {
    match s {
        ShipClass::Scout => "scout",
        ShipClass::Corvette => "corvette",
        ShipClass::Frigate => "frigate",
        ShipClass::Cruiser => "cruiser",
        ShipClass::Hauler => "hauler",
    }
}

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug)]
pub enum WorldError {
    /// The database holds a world from before `world_meta` existed.
    Unversioned,
    /// The world was stored by another economy or another world generator.
    Stale(Stale),
    PaceMismatch { stored: Pace, requested: Pace },
    Db(rusqlite::Error),
}

impl std::fmt::Display for WorldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldError::Unversioned => write!(
                f,
                "this database was created before the economy rework (it has no world_meta); \
                 old worlds are not migrated. Start a new world with a new --db path"
            ),
            WorldError::Stale(stale) => write!(
                f,
                "{}; old worlds are not migrated. Start a new world with a new --db path",
                stale.sentence("database")
            ),
            WorldError::PaceMismatch { stored, requested } => write!(
                f,
                "this world was created at pace {} and cannot change; refusing --pace {}. \
                 Omit --pace to keep it, or start a new world with a new --db path",
                describe_pace(*stored),
                describe_pace(*requested),
            ),
            WorldError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for WorldError {}

impl From<rusqlite::Error> for WorldError {
    fn from(e: rusqlite::Error) -> Self {
        WorldError::Db(e)
    }
}

pub fn describe_pace(p: Pace) -> String {
    match p.preset() {
        Some(preset) => format!("{} ({})", p.value(), preset.name),
        None => p.value().to_string(),
    }
}

impl Database {
    pub fn init(db_path: &str) -> Result<Database, rusqlite::Error> {
        let conn = Connection::open(db_path)?;
        // Only a foreign process (the sqlite3 CLI, a backup) can hold the
        // write lock; do not let the writer sit on it for rusqlite's default 5 s.
        conn.busy_timeout(Duration::from_secs(1))?;
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.apply_pragmas()?;
        db.ensure_schema()?;
        info!("Database initialized: {}", db_path);
        Ok(db)
    }

    fn ensure_schema(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS server_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS world_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                pace REAL NOT NULL,
                economy_version INTEGER NOT NULL,
                worldgen_version INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS players (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                homeworld_q INTEGER NOT NULL,
                homeworld_r INTEGER NOT NULL,
                metal REAL DEFAULT 500,
                crystal REAL DEFAULT 300,
                deuterium REAL DEFAULT 100,
                agent INTEGER NOT NULL DEFAULT 0,
                combat_points REAL NOT NULL DEFAULT 0,
                explore_points REAL NOT NULL DEFAULT 0,
                relics INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS fleets (
                id INTEGER PRIMARY KEY,
                player_id INTEGER REFERENCES players(id),
                q INTEGER NOT NULL,
                r INTEGER NOT NULL,
                state TEXT DEFAULT 'idle',
                fuel REAL NOT NULL,
                fuel_max REAL NOT NULL,
                cargo_metal REAL DEFAULT 0,
                cargo_crystal REAL DEFAULT 0,
                cargo_deuterium REAL DEFAULT 0,
                charts TEXT NOT NULL DEFAULT ''
            );

            CREATE TABLE IF NOT EXISTS ships (
                id INTEGER PRIMARY KEY,
                fleet_id INTEGER REFERENCES fleets(id),
                player_id INTEGER REFERENCES players(id),
                class TEXT NOT NULL,
                hull REAL NOT NULL,
                hull_max REAL NOT NULL,
                shield REAL NOT NULL,
                shield_max REAL NOT NULL,
                weapon_power REAL NOT NULL,
                speed INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sectors_modified (
                q INTEGER,
                r INTEGER,
                metal_density INTEGER,
                crystal_density INTEGER,
                deut_density INTEGER,
                metal_harvested REAL DEFAULT 0,
                crystal_harvested REAL DEFAULT 0,
                deut_harvested REAL DEFAULT 0,
                npc_cleared_tick INTEGER,
                PRIMARY KEY (q, r)
            );

            CREATE TABLE IF NOT EXISTS explored_edges (
                player_id INTEGER REFERENCES players(id),
                q1 INTEGER NOT NULL,
                r1 INTEGER NOT NULL,
                q2 INTEGER NOT NULL,
                r2 INTEGER NOT NULL,
                discovered_tick INTEGER NOT NULL,
                PRIMARY KEY (player_id, q1, r1, q2, r2)
            );

            CREATE TABLE IF NOT EXISTS explore_credits (
                player_id INTEGER NOT NULL REFERENCES players(id),
                q INTEGER NOT NULL,
                r INTEGER NOT NULL,
                kind INTEGER NOT NULL,
                PRIMARY KEY (player_id, q, r, kind)
            );

            CREATE TABLE IF NOT EXISTS known_sectors (
                player_id INTEGER NOT NULL REFERENCES players(id),
                q INTEGER NOT NULL,
                r INTEGER NOT NULL,
                last_seen INTEGER NOT NULL,
                observed TEXT NOT NULL,
                PRIMARY KEY (player_id, q, r)
            );

            CREATE TABLE IF NOT EXISTS buildings (
                player_id INTEGER REFERENCES players(id),
                building_type INTEGER NOT NULL,
                level INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (player_id, building_type)
            );

            CREATE TABLE IF NOT EXISTS research (
                player_id INTEGER REFERENCES players(id),
                tech_type INTEGER NOT NULL,
                level INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (player_id, tech_type)
            );

            CREATE TABLE IF NOT EXISTS build_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                player_id INTEGER REFERENCES players(id),
                queue_type TEXT NOT NULL,
                item_type INTEGER NOT NULL,
                target_level INTEGER,
                count INTEGER DEFAULT 1,
                built INTEGER DEFAULT 0,
                pending INTEGER NOT NULL DEFAULT 0,
                start_tick INTEGER NOT NULL,
                end_tick INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS player_defences (
                player_id INTEGER NOT NULL REFERENCES players(id),
                kind INTEGER NOT NULL,
                count INTEGER NOT NULL DEFAULT 0,
                restoring INTEGER NOT NULL DEFAULT 0,
                restore_tick INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (player_id, kind)
            );

            CREATE TABLE IF NOT EXISTS fleet_policies (
                fleet_id INTEGER PRIMARY KEY REFERENCES fleets(id),
                preset TEXT NOT NULL,
                min_fuel_pct INTEGER NOT NULL,
                cargo_return_pct INTEGER NOT NULL,
                max_range INTEGER NOT NULL,
                engage_ratio_x10 INTEGER NOT NULL,
                work_q INTEGER,
                work_r INTEGER
            );

            CREATE TABLE IF NOT EXISTS runtime_state (
                kind TEXT NOT NULL,
                id INTEGER NOT NULL,
                data TEXT NOT NULL,
                PRIMARY KEY (kind, id)
            ) WITHOUT ROWID;

            CREATE INDEX IF NOT EXISTS idx_fleets_player ON fleets(player_id);
            CREATE INDEX IF NOT EXISTS idx_fleets_location ON fleets(q, r);
            CREATE INDEX IF NOT EXISTS idx_ships_fleet ON ships(fleet_id);
            CREATE INDEX IF NOT EXISTS idx_explored_player ON explored_edges(player_id);
            CREATE INDEX IF NOT EXISTS idx_known_player ON known_sectors(player_id);
            CREATE INDEX IF NOT EXISTS idx_buildings_player ON buildings(player_id);
            CREATE INDEX IF NOT EXISTS idx_research_player ON research(player_id);
            CREATE INDEX IF NOT EXISTS idx_build_queue_player ON build_queue(player_id);
            ",
        )?;

        // Columns added after release — migrate old worlds in place.
        Self::ensure_column(&conn, "sectors_modified", "site_looted_tick", "INTEGER")?;
        Self::ensure_column(&conn, "sectors_modified", "site_ambush_bumps", "INTEGER DEFAULT 0")?;
        Self::ensure_column(&conn, "sectors_modified", "salvage_metal", "REAL")?;
        Self::ensure_column(&conn, "sectors_modified", "salvage_crystal", "REAL")?;
        Self::ensure_column(&conn, "sectors_modified", "salvage_deut", "REAL")?;
        Self::ensure_column(&conn, "sectors_modified", "salvage_despawn_tick", "INTEGER")?;
        Self::ensure_column(&conn, "sectors_modified", "kill_heat", "REAL DEFAULT 0")?;
        Self::ensure_column(&conn, "sectors_modified", "kill_heat_tick", "INTEGER DEFAULT 0")?;
        Self::ensure_column(&conn, "fleets", "harvest_resource", "TEXT DEFAULT 'auto'")?;
        Self::ensure_column(&conn, "players", "token_hash", "BLOB")?;
        Self::ensure_column(&conn, "build_queue", "item_id", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "build_queue", "reserve", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "ships", "slot", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "fleets", "move_cooldown", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "fleets", "action_cooldown", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "fleets", "idle_ticks", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "fleets", "move_q", "INTEGER")?;
        // A world saved before jump targets and combats were stored has
        // nothing to resume a jump or a fight from.
        if Self::ensure_column(&conn, "fleets", "move_r", "INTEGER")? {
            conn.execute("UPDATE fleets SET state = 'idle' WHERE state IN ('moving', 'in_combat')", params![])?;
        }
        Self::ensure_column(&conn, "fleet_policies", "next_eval_tick", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "fleet_policies", "hold_reason", "TEXT")?;
        Self::ensure_column(&conn, "fleet_policies", "hold_tick", "INTEGER")?;
        // Orders given before fuel budgets existed keep refuelling unbounded.
        Self::ensure_column(&conn, "fleet_policies", "fuel_budget_pct", "INTEGER NOT NULL DEFAULT 100")?;
        Self::ensure_column(&conn, "fleet_policies", "fuel_credit", "REAL NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "fleet_policies", "fuel_since", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "players", "exchange_pressure", "REAL NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "players", "exchange_tick", "INTEGER NOT NULL DEFAULT 0")?;
        Self::ensure_column(&conn, "fleets", "laden", "INTEGER NOT NULL DEFAULT 0")?;

        info!("Schema verified");
        Ok(())
    }

    /// ALTER TABLE ... ADD COLUMN, but only if the column is missing. True if
    /// it was added.
    fn ensure_column(conn: &Connection, table: &str, column: &str, ddl: &str) -> Result<bool, rusqlite::Error> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let exists = stmt
            .query_map(params![], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|name| name == column);
        if !exists {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl};"))?;
            info!("Migrated: {table}.{column}");
        }
        Ok(!exists)
    }

    fn apply_pragmas(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        // WAL makes a commit one sequential append. synchronous=NORMAL skips
        // the per-commit fsync: a commit survives a process crash (it is in
        // the OS page cache) and the database is never corrupted, but an OS
        // crash or power loss can drop commits made since the last
        // checkpoint, which the Persister forces every CHECKPOINT_EVERY.
        // (FULL was measured on the dev box: fsyncs there stall for seconds,
        // and a stalled writer keeps unsaved ticks in memory.)
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -8000;
            PRAGMA wal_autocheckpoint = 1000;
            ",
        )?;
        info!("Pragmas applied: WAL, synchronous=NORMAL, cache_size=8MB, wal_autocheckpoint=1000");
        Ok(())
    }

    // ── World ─────────────────────────────────────────────────────

    /// Read this database's world settings, creating them on a fresh
    /// database. `requested` is the operator's `--pace`; a world keeps the
    /// pace it was created with, so a different request is an error.
    pub fn open_world(&self, requested: Option<Pace>) -> Result<WorldMeta, WorldError> {
        let conn = self.conn.lock().unwrap();
        let row: Option<(f64, i64, i64)> = conn
            .query_row(
                "SELECT pace, economy_version, worldgen_version FROM world_meta WHERE id = 1",
                params![],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((pace, economy, worldgen)) = row {
            Stale::check(economy as u32, worldgen as u32).map_err(WorldError::Stale)?;
            let stored =
                Pace::new(pace).map_err(|_| WorldError::Stale(Stale::Economy { found: economy as u32 }))?;
            if let Some(requested) = requested
                && requested != stored
            {
                return Err(WorldError::PaceMismatch { stored, requested });
            }
            return Ok(WorldMeta { pace: stored, economy_version: economy as u32, worldgen_version: worldgen as u32 });
        }

        let has_data: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM players) OR EXISTS(SELECT 1 FROM server_state)",
            params![],
            |r| r.get(0),
        )?;
        if has_data {
            return Err(WorldError::Unversioned);
        }
        let meta = WorldMeta {
            pace: requested.unwrap_or_default(),
            economy_version: ECONOMY_VERSION,
            worldgen_version: WORLDGEN_VERSION,
        };
        conn.execute(
            "INSERT INTO world_meta (id, pace, economy_version, worldgen_version) VALUES (1, ?1, ?2, ?3)",
            params![meta.pace.value(), meta.economy_version as i64, meta.worldgen_version as i64],
        )?;
        info!("New world: pace {}", describe_pace(meta.pace));
        Ok(meta)
    }

    // ── Loading a world ───────────────────────────────────────────

    /// Everything this database holds, as engine state: the world's settings
    /// (a fresh database takes `requested`) and the whole world as of the
    /// last committed batch. After a clean shutdown it equals the
    /// `snapshot()` the engine stopped with. Events awaiting broadcast are
    /// not stored; the server drains them before every persist.
    pub fn load_snapshot(&self, world_seed: u64, requested: Option<Pace>) -> Result<Snapshot, Box<dyn std::error::Error>> {
        let meta = self.open_world(requested)?;
        let mut snapshot = Snapshot::new(world_seed, meta.pace);
        snapshot.meta = meta;

        snapshot.tick = self.load_server_state("current_tick")?.and_then(|s| s.parse().ok()).unwrap_or(0);
        snapshot.next_id = self.load_server_state("next_id")?.and_then(|s| s.parse().ok()).unwrap_or(1);
        if let Some(seed_str) = self.load_server_state("world_seed")? {
            let stored_seed: u64 = seed_str.parse().unwrap_or(0);
            if stored_seed != world_seed {
                warn!("World seed mismatch: DB has {}, config has {}", stored_seed, world_seed);
            }
        }
        self.save_server_state("world_seed", &world_seed.to_string())?;

        for mut player in self.load_players()? {
            player.buildings = self.load_buildings(player.id)?;
            player.research = self.load_research(player.id)?;
            let queues = self.load_build_queues(player.id)?;
            player.building_queue = queues.building;
            player.building_pending = queues.building_pending;
            player.ship_queue = queues.ship;
            player.ship_pending = queues.ship_pending;
            player.research_queue = queues.research;
            player.research_pending = queues.research_pending;
            player.defences = self.load_defences(player.id)?;
            snapshot.players.insert(player.id, player);
        }
        snapshot.fleets = self.load_fleets()?.into_iter().map(|f| (f.id, f)).collect();
        snapshot.sector_overrides = self.load_sector_overrides()?
            .into_iter()
            .map(|row| (Hex { q: row.q, r: row.r }.to_key(), row.override_data))
            .collect();
        snapshot.policies = self.load_fleet_policies()?.into_iter().collect();
        snapshot.explored = self.load_explored_sectors()?
            .into_iter()
            .map(|(pid, hex)| (pid, hex.to_key()))
            .collect();
        snapshot.credited = self.load_explore_credits()?
            .into_iter()
            .map(|(pid, hex, kind)| (pid, hex.to_key(), kind))
            .collect();
        snapshot.known = self.load_known_sectors()?;
        snapshot.known.sort_by_key(|row| (row.player_id, row.sector.to_key()));
        for (id, entry) in self.load_runtime()? {
            snapshot.insert_runtime(id, entry);
        }

        if snapshot.players.is_empty() {
            info!("State loaded (empty -- fresh world)");
        } else {
            info!("State loaded: {} players, {} fleets, tick {}", snapshot.players.len(), snapshot.fleets.len(), snapshot.tick);
        }
        Ok(snapshot)
    }

    // ── Server State ──────────────────────────────────────────────

    /// Run `f` inside a single transaction; rolls back if it errors.
    pub fn with_transaction<F>(&self, f: F) -> Result<(), rusqlite::Error>
    where
        F: FnOnce(&Database) -> Result<(), rusqlite::Error>,
    {
        self.conn.lock().unwrap().execute_batch("BEGIN IMMEDIATE")?;
        match f(self) {
            Ok(()) => self.conn.lock().unwrap().execute_batch("COMMIT"),
            Err(e) => {
                let _ = self.conn.lock().unwrap().execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn save_server_state(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO server_state (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn load_server_state(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM server_state WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        ).optional()
    }

    // ── Players ───────────────────────────────────────────────────

    pub fn save_player(&self, player: &Player) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO players (id, name, homeworld_q, homeworld_r, metal, crystal, deuterium, token_hash,
                                             agent, combat_points, explore_points, relics,
                                             exchange_pressure, exchange_tick)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                player.id as i64,
                &player.name,
                player.homeworld.q as i64,
                player.homeworld.r as i64,
                float_to_stored(player.resources.metal),
                float_to_stored(player.resources.crystal),
                float_to_stored(player.resources.deuterium),
                player.token_hash.as_ref().map(|h| h.as_slice()),
                player.agent,
                player.combat_points as f64,
                player.explore_points as f64,
                player.relics as i64,
                player.exchange_pressure,
                player.exchange_tick as i64,
            ],
        )?;
        Ok(())
    }

    pub fn load_players(&self) -> Result<Vec<Player>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, homeworld_q, homeworld_r, metal, crystal, deuterium, token_hash,
                    agent, combat_points, explore_points, relics, exchange_pressure, exchange_tick FROM players",
        )?;
        let players = stmt.query_map(params![], |row| {
            Ok(Player {
                id: row.get::<_, i64>(0)? as u64,
                name: row.get::<_, String>(1)?,
                homeworld: Hex {
                    q: row.get::<_, i64>(2)? as i16,
                    r: row.get::<_, i64>(3)? as i16,
                },
                resources: Resources {
                    metal: stored_to_float(row.get::<_, f64>(4)?),
                    crystal: stored_to_float(row.get::<_, f64>(5)?),
                    deuterium: stored_to_float(row.get::<_, f64>(6)?),
                },
                buildings: BuildingLevels::default(),
                research: ResearchLevels::default(),
                building_queue: Vec::new(),
                building_pending: Vec::new(),
                ship_queue: None,
                ship_pending: Vec::new(),
                defences: Defences::default(),
                research_queue: None,
                research_pending: Vec::new(),
                token_hash: row.get::<_, Option<Vec<u8>>>(7)?.and_then(|v| v.try_into().ok()),
                agent: row.get::<_, bool>(8)?,
                combat_points: row.get::<_, f64>(9)? as f32,
                explore_points: row.get::<_, f64>(10)? as f32,
                relics: row.get::<_, i64>(11)? as u32,
                exchange_pressure: row.get::<_, f64>(12)?,
                exchange_tick: row.get::<_, i64>(13)? as u64,
            })
        })?;
        players.collect()
    }

    // ── Fleets ────────────────────────────────────────────────────

    pub fn save_fleet(&self, fleet: &Fleet) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO fleets (id, player_id, q, r, state, fuel, fuel_max,
             cargo_metal, cargo_crystal, cargo_deuterium, harvest_resource, charts,
             move_cooldown, action_cooldown, idle_ticks, move_q, move_r, laden)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                fleet.id as i64,
                fleet.owner_id as i64,
                fleet.location.q as i64,
                fleet.location.r as i64,
                fleet_status_to_str(fleet.state),
                float_to_stored(fleet.fuel),
                float_to_stored(fleet.fuel_max),
                float_to_stored(fleet.cargo.metal),
                float_to_stored(fleet.cargo.crystal),
                float_to_stored(fleet.cargo.deuterium),
                harvest_resource_to_str(fleet.harvest_target),
                fleet.charts.iter().map(u32::to_string).collect::<Vec<_>>().join(","),
                fleet.move_cooldown as i64,
                fleet.action_cooldown as i64,
                fleet.idle_ticks as i64,
                fleet.move_target.map(|h| h.q as i64),
                fleet.move_target.map(|h| h.r as i64),
                fleet.laden,
            ],
        )?;
        self.save_ships_inner(&conn, fleet)
    }

    pub fn save_fleet_clocks(&self, clocks: &FleetClocks) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.prepare_cached(
            "UPDATE fleets SET move_cooldown = ?2, action_cooldown = ?3, idle_ticks = ?4 WHERE id = ?1",
        )?
        .execute(params![
            clocks.fleet_id as i64,
            clocks.move_cooldown as i64,
            clocks.action_cooldown as i64,
            clocks.idle_ticks as i64,
        ])?;
        Ok(())
    }

    /// Rewrites the fleet's ship rows. REPLACE, because a ship that just
    /// moved here by split or merge may still have a row under its old fleet
    /// when this fleet happens to be saved first. `slot` keeps the order the
    /// fleet holds them in, which combat rolls against.
    fn save_ships_inner(&self, conn: &Connection, fleet: &Fleet) -> Result<(), rusqlite::Error> {
        conn.execute(
            "DELETE FROM ships WHERE fleet_id = ?1",
            params![fleet.id as i64],
        )?;

        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO ships (id, fleet_id, player_id, class, hull, hull_max, shield, shield_max, weapon_power, speed, slot)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;

        for (slot, ship) in fleet.ships[0..fleet.ship_count].iter().enumerate() {
            stmt.execute(params![
                ship.id as i64,
                fleet.id as i64,
                fleet.owner_id as i64,
                ship_class_to_str(ship.ship_class),
                float_to_stored(ship.hull),
                float_to_stored(ship.hull_max),
                float_to_stored(ship.shield),
                float_to_stored(ship.shield_max),
                float_to_stored(ship.weapon_power),
                ship.speed as i64,
                slot as i64,
            ])?;
        }
        Ok(())
    }

    pub fn load_fleets(&self) -> Result<Vec<Fleet>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, player_id, q, r, state, fuel, fuel_max,
                    cargo_metal, cargo_crystal, cargo_deuterium, harvest_resource, charts,
                    move_cooldown, action_cooldown, idle_ticks, move_q, move_r, laden
             FROM fleets",
        )?;

        let mut fleets = Vec::new();
        let fleet_rows = stmt.query_map(params![], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, f64>(7)?,
                row.get::<_, f64>(8)?,
                row.get::<_, f64>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, i64>(13)?,
                row.get::<_, i64>(14)?,
                row.get::<_, Option<i64>>(15)?,
                row.get::<_, Option<i64>>(16)?,
                row.get::<_, bool>(17)?,
            ))
        })?;

        for row_result in fleet_rows {
            let (
                fid, pid, q, r, state_str, fuel, fuel_max, cm, cc, cd, harvest, charts,
                move_cooldown, action_cooldown, idle_ticks, move_q, move_r, laden,
            ) = row_result?;
            let mut fleet = Fleet {
                id: fid as u64,
                owner_id: pid as u64,
                location: Hex { q: q as i16, r: r as i16 },
                state: parse_fleet_status(&state_str),
                ships: [Ship::default(); iac_sim::engine::MAX_SHIPS_PER_FLEET],
                ship_count: 0,
                cargo: Resources {
                    metal: stored_to_float(cm),
                    crystal: stored_to_float(cc),
                    deuterium: stored_to_float(cd),
                },
                fuel: stored_to_float(fuel),
                fuel_max: stored_to_float(fuel_max),
                move_cooldown: move_cooldown as u16,
                action_cooldown: action_cooldown as u16,
                move_target: move_q.zip(move_r).map(|(q, r)| Hex { q: q as i16, r: r as i16 }),
                idle_ticks: idle_ticks as u16,
                harvest_target: parse_harvest_resource(harvest.as_deref().unwrap_or("auto")),
                charts: charts.split(',').filter_map(|k| k.parse().ok()).collect(),
                laden,
            };
            fleet.ship_count = self.load_ships_into_inner(&conn, fid, &mut fleet.ships)?;
            fleets.push(fleet);
        }
        Ok(fleets)
    }

    fn load_ships_into_inner(&self, conn: &Connection, fleet_id: i64, ships: &mut [Ship]) -> Result<usize, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, class, hull, hull_max, shield, shield_max, weapon_power, speed
             FROM ships WHERE fleet_id = ?1 ORDER BY slot, id",
        )?;

        let mut count = 0;
        let ship_rows = stmt.query_map(params![fleet_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?;

        for row_result in ship_rows {
            if count >= ships.len() { break; }
            let (sid, class_str, hull, hull_max, shield, shield_max, wp, speed) = row_result?;
            ships[count] = Ship {
                id: sid as u64,
                ship_class: parse_ship_class(&class_str),
                hull: stored_to_float(hull),
                hull_max: stored_to_float(hull_max),
                shield: stored_to_float(shield),
                shield_max: stored_to_float(shield_max),
                weapon_power: stored_to_float(wp),
                speed: speed as u8,
            };
            count += 1;
        }
        Ok(count)
    }

    pub fn delete_fleet(&self, fleet_id: u64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM ships WHERE fleet_id = ?1", params![fleet_id as i64])?;
        conn.execute("DELETE FROM fleet_policies WHERE fleet_id = ?1", params![fleet_id as i64])?;
        conn.execute("DELETE FROM fleets WHERE id = ?1", params![fleet_id as i64])?;
        Ok(())
    }

    // ── Sector Overrides ──────────────────────────────────────────

    pub fn save_sector_override(&self, q: i16, r: i16, ov: &SectorOverride) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO sectors_modified (q, r, metal_density, crystal_density, deut_density,
             metal_harvested, crystal_harvested, deut_harvested, npc_cleared_tick,
             site_looted_tick, site_ambush_bumps,
             salvage_metal, salvage_crystal, salvage_deut, salvage_despawn_tick,
             kill_heat, kill_heat_tick)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                q as i64,
                r as i64,
                density_to_int(ov.metal_density),
                density_to_int(ov.crystal_density),
                density_to_int(ov.deut_density),
                float_to_stored(ov.metal_harvested),
                float_to_stored(ov.crystal_harvested),
                float_to_stored(ov.deut_harvested),
                ov.npc_cleared_tick.map(|t| t as i64),
                ov.site_looted_tick.map(|t| t as i64),
                ov.site_ambush_bumps as i64,
                ov.salvage.map(|r| r.metal as f64),
                ov.salvage.map(|r| r.crystal as f64),
                ov.salvage.map(|r| r.deuterium as f64),
                ov.salvage_despawn_tick.map(|t| t as i64),
                ov.kill_heat as f64,
                ov.kill_heat_tick as i64,
            ],
        )?;
        Ok(())
    }

    pub fn load_sector_overrides(&self) -> Result<Vec<SectorOverrideRow>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT q, r, metal_density, crystal_density, deut_density,
                    metal_harvested, crystal_harvested, deut_harvested,
                    npc_cleared_tick, site_looted_tick, site_ambush_bumps,
                    salvage_metal, salvage_crystal, salvage_deut, salvage_despawn_tick,
                    kill_heat, kill_heat_tick
             FROM sectors_modified",
        )?;

        let mut overrides = Vec::new();
        let rows = stmt.query_map(params![], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, f64>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, Option<i64>>(9)?,
                row.get::<_, Option<i64>>(10)?,
                row.get::<_, Option<f64>>(11)?,
                row.get::<_, Option<f64>>(12)?,
                row.get::<_, Option<f64>>(13)?,
                row.get::<_, Option<i64>>(14)?,
                row.get::<_, Option<f64>>(15)?,
                row.get::<_, Option<i64>>(16)?,
            ))
        })?;

        for row_result in rows {
            let (q, r, md, cd, dd, mh, ch, dh, nct, slt, sab, sm, sc, sd, sdt, heat, heat_tick) = row_result?;
            overrides.push(SectorOverrideRow {
                q: q as i16,
                r: r as i16,
                override_data: SectorOverride {
                    metal_density: int_to_density(md),
                    crystal_density: int_to_density(cd),
                    deut_density: int_to_density(dd),
                    metal_harvested: stored_to_float(mh),
                    crystal_harvested: stored_to_float(ch),
                    deut_harvested: stored_to_float(dh),
                    salvage: sm.map(|m| Resources {
                        metal: m as f32,
                        crystal: sc.unwrap_or(0.0) as f32,
                        deuterium: sd.unwrap_or(0.0) as f32,
                    }),
                    salvage_despawn_tick: sdt.map(|t| t as u64),
                    npc_cleared_tick: nct.map(|t| t as u64),
                    site_looted_tick: slt.map(|t| t as u64),
                    site_ambush_bumps: sab.unwrap_or(0).clamp(0, 255) as u8,
                    kill_heat: heat.unwrap_or(0.0) as f32,
                    kill_heat_tick: heat_tick.unwrap_or(0).max(0) as u64,
                },
            });
        }
        Ok(overrides)
    }

    // ── Fleet Policies ────────────────────────────────────────────

    pub fn save_fleet_policy(&self, fleet_id: u64, policy: &FleetPolicy) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO fleet_policies
             (fleet_id, preset, min_fuel_pct, cargo_return_pct, max_range, engage_ratio_x10, work_q, work_r,
              next_eval_tick, hold_reason, hold_tick, fuel_budget_pct, fuel_credit, fuel_since)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                fleet_id as i64,
                policy_preset_to_str(policy.preset),
                policy.params.min_fuel_pct as i64,
                policy.params.cargo_return_pct as i64,
                policy.params.max_range as i64,
                policy.params.engage_ratio_x10 as i64,
                policy.work_sector.map(|h| h.q as i64),
                policy.work_sector.map(|h| h.r as i64),
                policy.next_eval_tick as i64,
                policy.last_hold.as_ref().map(|(reason, _)| reason.as_str()),
                policy.last_hold.as_ref().map(|(_, tick)| *tick as i64),
                policy.params.fuel_budget_pct as i64,
                policy.fuel_credit as f64,
                policy.fuel_since as i64,
            ],
        )?;
        Ok(())
    }

    pub fn delete_fleet_policy(&self, fleet_id: u64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM fleet_policies WHERE fleet_id = ?1", params![fleet_id as i64])?;
        Ok(())
    }

    pub fn load_fleet_policies(&self) -> Result<Vec<(u64, FleetPolicy)>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT fleet_id, preset, min_fuel_pct, cargo_return_pct, max_range, engage_ratio_x10, work_q, work_r,
                    next_eval_tick, hold_reason, hold_tick, fuel_budget_pct, fuel_credit, fuel_since
             FROM fleet_policies",
        )?;
        let rows = stmt.query_map(params![], |row| {
            let work_sector = match (row.get::<_, Option<i64>>(6)?, row.get::<_, Option<i64>>(7)?) {
                (Some(q), Some(r)) => Some(Hex { q: q as i16, r: r as i16 }),
                _ => None,
            };
            let hold_reason = row.get::<_, Option<String>>(9)?;
            let hold_tick = row.get::<_, Option<i64>>(10)?;
            let policy = FleetPolicy {
                preset: parse_policy_preset(&row.get::<_, String>(1)?),
                params: PolicyParams {
                    min_fuel_pct: row.get::<_, i64>(2)?.clamp(0, 100) as u8,
                    cargo_return_pct: row.get::<_, i64>(3)?.clamp(0, 100) as u8,
                    max_range: row.get::<_, i64>(4)?.clamp(1, 30) as u8,
                    engage_ratio_x10: row.get::<_, i64>(5)?.clamp(1, 100) as u8,
                    fuel_budget_pct: row.get::<_, i64>(11)?.clamp(0, 100) as u8,
                },
                work_sector,
                next_eval_tick: row.get::<_, i64>(8)? as u64,
                last_hold: hold_reason.zip(hold_tick).map(|(reason, tick)| (reason, tick as u64)),
                fuel_credit: row.get::<_, f64>(12)? as f32,
                fuel_since: row.get::<_, i64>(13)? as u64,
            };
            Ok((row.get::<_, i64>(0)? as u64, policy))
        })?;

        let mut policies = Vec::new();
        for row_result in rows {
            policies.push(row_result?);
        }
        Ok(policies)
    }

    // ── Runtime State ─────────────────────────────────────────────

    /// Upsert the entry as its JSON, or delete the row of one that is gone.
    pub fn save_runtime(&self, row: &RuntimeRow) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let key = (row.kind.name(), row.id as i64);
        match &row.entry {
            Some(entry) => {
                let data = serde_json::to_string(entry)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                conn.prepare_cached("INSERT OR REPLACE INTO runtime_state (kind, id, data) VALUES (?1, ?2, ?3)")?
                    .execute(params![key.0, key.1, data])?;
            }
            None => {
                conn.prepare_cached("DELETE FROM runtime_state WHERE kind = ?1 AND id = ?2")?
                    .execute(params![key.0, key.1])?;
            }
        }
        Ok(())
    }

    /// Rows that no longer parse (written by an older engine) are skipped:
    /// that group, combat or clock starts over, as all of them once did on
    /// every restart. An outpost is not a clock: it holds what a player
    /// built and stored, so one that cannot be read refuses the world.
    pub fn load_runtime(&self) -> Result<Vec<(u64, RuntimeEntry)>, Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT kind, id, data FROM runtime_state")?;
        let rows = stmt.query_map(params![], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (kind, id, data) = row?;
            match serde_json::from_str::<RuntimeEntry>(&data) {
                Ok(entry) => out.push((id as u64, entry)),
                Err(e) if kind == RuntimeKind::Outpost.name() => {
                    return Err(format!(
                        "the outpost of player {id} in this database cannot be read ({e}); it holds that player's \
                         store, so the world is not opened without it"
                    )
                    .into());
                }
                Err(e) => warn!("Skipping unreadable runtime state {kind} {id}: {e}"),
            }
        }
        Ok(out)
    }

    // ── Explored Edges ────────────────────────────────────────────

    pub fn save_explored_edge(&self, player_id: u64, from: Hex, to: Hex, tick: u64) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO explored_edges (player_id, q1, r1, q2, r2, discovered_tick)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                player_id as i64,
                from.q as i64,
                from.r as i64,
                to.q as i64,
                to.r as i64,
                tick as i64,
            ],
        )?;
        Ok(())
    }

    /// Every (player, sector) pair the player has ever entered.
    pub fn load_explored_sectors(&self) -> Result<Vec<(u64, Hex)>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT DISTINCT player_id, q1, r1 FROM explored_edges")?;
        let rows = stmt.query_map(params![], |row| {
            Ok((row.get::<_, i64>(0)? as u64, Hex { q: row.get::<_, i64>(1)? as i16, r: row.get::<_, i64>(2)? as i16 }))
        })?;
        rows.collect()
    }

    // ── Explore credits ───────────────────────────────────────────

    pub fn save_explore_credit(&self, player_id: u64, sector: Hex, kind: ExploreCredit) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO explore_credits (player_id, q, r, kind) VALUES (?1, ?2, ?3, ?4)",
            params![player_id as i64, sector.q as i64, sector.r as i64, kind as i64],
        )?;
        Ok(())
    }

    pub fn load_explore_credits(&self) -> Result<Vec<(u64, Hex, ExploreCredit)>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT player_id, q, r, kind FROM explore_credits")?;
        let rows = stmt.query_map(params![], |row| {
            let kind = if row.get::<_, i64>(3)? == ExploreCredit::Boarded as i64 { ExploreCredit::Boarded } else { ExploreCredit::Charted };
            Ok((
                row.get::<_, i64>(0)? as u64,
                Hex { q: row.get::<_, i64>(1)? as i16, r: row.get::<_, i64>(2)? as i16 },
                kind,
            ))
        })?;
        rows.collect()
    }

    // ── Known Sectors ─────────────────────────────────────────────

    pub fn save_known_sector(&self, row: &KnownRow) -> Result<(), rusqlite::Error> {
        let observed = serde_json::to_string(&row.state)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO known_sectors (player_id, q, r, last_seen, observed)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                row.player_id as i64,
                row.sector.q as i64,
                row.sector.r as i64,
                row.last_seen as i64,
                observed,
            ],
        )?;
        Ok(())
    }

    /// Every player's chart. Rows that no longer parse (written by an older
    /// protocol) are skipped; the sector is simply re-learned when next seen.
    pub fn load_known_sectors(&self) -> Result<Vec<KnownRow>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT player_id, q, r, last_seen, observed FROM known_sectors")?;
        let rows = stmt.query_map(params![], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (pid, q, r, last_seen, observed) = row?;
            match serde_json::from_str::<SectorState>(&observed) {
                Ok(mut state) => {
                    state.last_seen = last_seen as u64;
                    state.live = false;
                    out.push(KnownRow {
                        player_id: pid as u64,
                        sector: Hex { q: q as i16, r: r as i16 },
                        last_seen: last_seen as u64,
                        state,
                    });
                }
                Err(e) => warn!("Skipping unreadable known sector ({q},{r}) of player {pid}: {e}"),
            }
        }
        Ok(out)
    }

    // ── Buildings / Research / Queue ──────────────────────────────

    pub fn save_buildings(&self, player_id: u64, levels: &BuildingLevels) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO buildings (player_id, building_type, level) VALUES (?1, ?2, ?3)",
        )?;

        for bt in (0..BuildingType::COUNT).filter_map(BuildingType::from_usize) {
            stmt.execute(params![
                player_id as i64,
                bt as i64,
                levels.get(bt) as i64,
            ])?;
        }
        Ok(())
    }

    pub fn load_buildings(&self, player_id: u64) -> Result<BuildingLevels, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut levels = BuildingLevels::default();
        let mut stmt = conn.prepare(
            "SELECT building_type, level FROM buildings WHERE player_id = ?1",
        )?;

        let rows = stmt.query_map(params![player_id as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;

        for row_result in rows {
            let (bt_int, level) = row_result?;
            if let Some(bt) = usize::try_from(bt_int).ok().and_then(BuildingType::from_usize) {
                levels.set(bt, level as u8);
            }
        }
        Ok(levels)
    }

    pub fn save_research(&self, player_id: u64, levels: &ResearchLevels) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO research (player_id, tech_type, level) VALUES (?1, ?2, ?3)",
        )?;

        for rt in (0..ResearchType::COUNT).filter_map(ResearchType::from_usize) {
            let lvl = levels.get(rt);
            if lvl > 0 {
                stmt.execute(params![
                    player_id as i64,
                    rt as i64,
                    lvl as i64,
                ])?;
            }
        }
        Ok(())
    }

    pub fn load_research(&self, player_id: u64) -> Result<ResearchLevels, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut levels = ResearchLevels::default();
        let mut stmt = conn.prepare(
            "SELECT tech_type, level FROM research WHERE player_id = ?1",
        )?;

        let rows = stmt.query_map(params![player_id as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;

        for row_result in rows {
            let (tech_int, level) = row_result?;
            if let Some(tech) = usize::try_from(tech_int).ok().and_then(ResearchType::from_usize) {
                levels.set(tech, level as u8);
            }
        }
        Ok(levels)
    }

    /// Rows are written in queue order (active items first, then the ones
    /// waiting) and read back by id.
    pub fn save_defences(&self, player_id: u64, d: &Defences) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO player_defences (player_id, kind, count, restoring, restore_tick)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for kind in DefenceKind::ALL {
            let i = kind as usize;
            stmt.execute(params![player_id as i64, i as i64, d.count[i] as i64, d.restoring[i] as i64, d.restore_tick[i] as i64])?;
        }
        Ok(())
    }

    pub fn load_defences(&self, player_id: u64) -> Result<Defences, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut d = Defences::default();
        let mut stmt = conn.prepare(
            "SELECT kind, count, restoring, restore_tick FROM player_defences WHERE player_id = ?1",
        )?;
        let rows = stmt.query_map(params![player_id as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
        })?;
        for row in rows {
            let (kind, count, restoring, tick) = row?;
            if let Some(kind) = usize::try_from(kind).ok().and_then(DefenceKind::from_usize) {
                let i = kind as usize;
                d.count[i] = count as u32;
                d.restoring[i] = restoring as u32;
                d.restore_tick[i] = tick as u64;
            }
        }
        Ok(d)
    }

    pub fn save_build_queue(&self, player_id: u64, player: &Player) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM build_queue WHERE player_id = ?1",
            params![player_id as i64],
        )?;

        let mut stmt = conn.prepare(
            "INSERT INTO build_queue (player_id, queue_type, item_type, target_level, count, built, pending, start_tick, end_tick, item_id, reserve)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        let pid = player_id as i64;
        let none = rusqlite::types::Value::Null;

        for q in &player.building_queue {
            stmt.execute(params![pid, "building", q.building_type as i64, q.target_level as i64, 1i64, 0i64, 0i64, q.start_tick as i64, q.end_tick as i64, q.id as i64, 0i64])?;
        }
        for q in &player.building_pending {
            stmt.execute(params![pid, "building", q.building_type as i64, q.target_level as i64, 1i64, 0i64, 1i64, 0i64, 0i64, q.id as i64, q.reserve as i64])?;
        }
        if let Some(q) = &player.ship_queue {
            let (kind, item) = shipyard_item_to_row(q.item);
            stmt.execute(params![pid, kind, item, none, q.count as i64, q.built as i64, 0i64, q.start_tick as i64, q.end_tick as i64, q.id as i64, 0i64])?;
        }
        for q in &player.ship_pending {
            let (kind, item) = shipyard_item_to_row(q.item);
            stmt.execute(params![pid, kind, item, none, q.count as i64, 0i64, 1i64, 0i64, 0i64, q.id as i64, 0i64])?;
        }
        if let Some(q) = &player.research_queue {
            stmt.execute(params![pid, "research", q.tech as i64, q.target_level as i64, 1i64, 0i64, 0i64, q.start_tick as i64, q.end_tick as i64, q.id as i64, 0i64])?;
        }
        for q in &player.research_pending {
            stmt.execute(params![pid, "research", q.tech as i64, q.target_level as i64, 1i64, 0i64, 1i64, 0i64, 0i64, q.id as i64, q.reserve as i64])?;
        }

        Ok(())
    }

    pub fn load_build_queues(&self, player_id: u64) -> Result<BuildQueueData, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut data = BuildQueueData::default();
        let mut stmt = conn.prepare(
            "SELECT queue_type, item_type, target_level, count, built, pending, start_tick, end_tick, item_id, reserve
             FROM build_queue WHERE player_id = ?1 ORDER BY id",
        )?;

        let rows = stmt.query_map(params![player_id as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)? != 0,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)? as u64,
                row.get::<_, i64>(9)? != 0,
            ))
        })?;

        for row_result in rows {
            let (qt, item_type, target_level, count, built, pending, start_tick, end_tick, id, reserve) = row_result?;
            let item = usize::try_from(item_type).ok();
            let level = target_level.unwrap_or(0) as u8;
            match qt.as_str() {
                "building" => {
                    let Some(bt) = item.and_then(BuildingType::from_usize) else { continue; };
                    if pending {
                        data.building_pending.push(PendingBuilding { id, building_type: bt, target_level: level, reserve });
                    } else {
                        data.building.push(BuildQueueEntry {
                            id,
                            building_type: bt,
                            target_level: level,
                            start_tick: start_tick as u64,
                            end_tick: end_tick as u64,
                        });
                    }
                }
                "ship" | "defence" => {
                    let Some(sc) = item.and_then(|i| shipyard_item_from_row(&qt, i)) else { continue; };
                    if pending {
                        data.ship_pending.push(PendingShip { id, item: sc, count: count as u16 });
                    } else {
                        data.ship = Some(ShipQueueEntry {
                            id,
                            item: sc,
                            count: count as u16,
                            built: built as u16,
                            start_tick: start_tick as u64,
                            end_tick: end_tick as u64,
                        });
                    }
                }
                "research" => {
                    let Some(tech) = item.and_then(ResearchType::from_usize) else { continue; };
                    if pending {
                        data.research_pending.push(PendingResearch { id, tech, target_level: level, reserve });
                    } else {
                        data.research = Some(ResearchQueueEntry {
                            id,
                            tech,
                            target_level: level,
                            start_tick: start_tick as u64,
                            end_tick: end_tick as u64,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(data)
    }
}

#[derive(Debug, Clone)]
pub struct SectorOverrideRow {
    pub q: i16,
    pub r: i16,
    pub override_data: SectorOverride,
}

#[derive(Debug, Default)]
pub struct BuildQueueData {
    pub building: Vec<BuildQueueEntry>,
    pub building_pending: Vec<PendingBuilding>,
    pub ship: Option<ShipQueueEntry>,
    pub ship_pending: Vec<PendingShip>,
    pub research: Option<ResearchQueueEntry>,
    pub research_pending: Vec<PendingResearch>,
}


// ── Background persistence ────────────────────────────────────────

impl Database {
    /// Fold the WAL into the main database file and fsync it.
    fn checkpoint(&self) -> Result<(), rusqlite::Error> {
        self.conn.lock().unwrap().query_row("PRAGMA wal_checkpoint(PASSIVE)", params![], |_| Ok(()))
    }

    /// Write one batch. Call inside `with_transaction`.
    fn apply_batch(&self, batch: &PersistBatch) -> Result<(), rusqlite::Error> {
        self.save_server_state("current_tick", &batch.tick.to_string())?;
        self.save_server_state("next_id", &batch.next_id.to_string())?;

        for player in &batch.players {
            self.save_player(player)?;
            self.save_buildings(player.id, &player.buildings)?;
            self.save_research(player.id, &player.research)?;
            self.save_build_queue(player.id, player)?;
            self.save_defences(player.id, &player.defences)?;
        }
        for fleet in &batch.fleets {
            self.save_fleet(fleet)?;
        }
        for clocks in &batch.fleet_clocks {
            self.save_fleet_clocks(clocks)?;
        }
        for &fid in &batch.deleted_fleets {
            self.delete_fleet(fid)?;
        }
        // Policies save after their fleets; deletes come first so a fleet
        // id never carries a stale policy.
        for &fid in &batch.deleted_policies {
            self.delete_fleet_policy(fid)?;
        }
        for (fid, policy) in &batch.policies {
            self.save_fleet_policy(*fid, policy)?;
        }
        for (hex, ov) in &batch.sectors {
            self.save_sector_override(hex.q, hex.r, ov)?;
        }
        for row in &batch.runtime {
            self.save_runtime(row)?;
        }
        for e in &batch.explored_edges {
            self.save_explored_edge(e.player_id, e.from, e.to, e.tick)?;
        }
        for row in &batch.known_sectors {
            self.save_known_sector(row)?;
        }
        for &(player_id, sector, kind) in &batch.explore_credits {
            self.save_explore_credit(player_id, sector, kind)?;
        }
        Ok(())
    }
}

enum WriterMsg {
    Batch(Box<PersistBatch>),
    Flush(mpsc::SyncSender<Result<(), String>>),
}

const WRITER_RETRY_DELAY: Duration = Duration::from_secs(1);
/// How often the writer fsyncs the WAL into the main file, bounding what an
/// OS crash or power loss can take back.
const CHECKPOINT_EVERY: Duration = Duration::from_secs(30);
const WRITER_SLOW_WARN: Duration = Duration::from_millis(250);

/// Owns the SQLite connection on a dedicated thread so a slow or stalled
/// disk can never delay the 1 Hz tick: the engine hands over a snapshot and
/// moves on. Batches queued while a write is in flight are coalesced into
/// one transaction; a failed write is retried (in order) until it succeeds.
pub struct Persister {
    tx: Option<mpsc::Sender<WriterMsg>>,
    handle: Option<JoinHandle<()>>,
}

impl Persister {
    pub fn spawn(db: Database) -> Persister {
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("iac-persist".into())
            .spawn(move || writer_loop(db, rx))
            .expect("spawn persist thread");
        Persister { tx: Some(tx), handle: Some(handle) }
    }
}

impl Persist for Persister {
    fn submit(&self, batch: PersistBatch) {
        if let Some(tx) = &self.tx
            && tx.send(WriterMsg::Batch(Box::new(batch))).is_err()
        {
            error!("persist thread is gone; state is no longer being saved");
        }
    }

    fn flush(&self) -> Result<(), String> {
        let tx = self.tx.as_ref().ok_or("persister already shut down")?;
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        tx.send(WriterMsg::Flush(ack_tx)).map_err(|_| "persist thread is gone".to_string())?;
        ack_rx.recv().map_err(|_| "persist thread is gone".to_string())?
    }
}

impl Drop for Persister {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn writer_loop(db: Database, rx: mpsc::Receiver<WriterMsg>) {
    let mut last_checkpoint = Instant::now();
    let mut pending: Vec<PersistBatch> = Vec::new();
    let mut flushes: Vec<mpsc::SyncSender<Result<(), String>>> = Vec::new();
    let mut closed = false;

    while !closed || !pending.is_empty() || !flushes.is_empty() {
        let wait = if pending.is_empty() { Duration::from_secs(3600) } else { WRITER_RETRY_DELAY };
        match rx.recv_timeout(wait) {
            Ok(msg) => take_msg(msg, &mut pending, &mut flushes),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => closed = true,
        }
        loop {
            match rx.try_recv() {
                Ok(msg) => take_msg(msg, &mut pending, &mut flushes),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }

        let mut outcome = Ok(());
        if !pending.is_empty() {
            let started = Instant::now();
            let result = db.with_transaction(|db| {
                pending.iter().try_for_each(|b| db.apply_batch(b))
            });
            let took = started.elapsed();
            match result {
                Ok(()) => {
                    debug!("persisted {} batch(es) in {:?}", pending.len(), took);
                    if took > WRITER_SLOW_WARN {
                        warn!("slow persist: {} batch(es) took {:?}", pending.len(), took);
                    }
                    pending.clear();
                    if last_checkpoint.elapsed() >= CHECKPOINT_EVERY {
                        last_checkpoint = Instant::now();
                        match db.checkpoint() {
                            Ok(()) => debug!("wal checkpoint took {:?}", last_checkpoint.elapsed()),
                            Err(e) => warn!("wal checkpoint failed: {}", e),
                        }
                    }
                }
                Err(e) => {
                    error!("persist failed ({} batch(es) kept for retry): {}", pending.len(), e);
                    outcome = Err(e.to_string());
                    if closed {
                        error!("shutting down with {} unsaved batch(es)", pending.len());
                        pending.clear();
                    }
                }
            }
        }
        for ack in flushes.drain(..) {
            let _ = ack.send(outcome.clone());
        }
    }
}

fn take_msg(
    msg: WriterMsg,
    pending: &mut Vec<PersistBatch>,
    flushes: &mut Vec<mpsc::SyncSender<Result<(), String>>>,
) {
    match msg {
        WriterMsg::Batch(b) => pending.push(*b),
        WriterMsg::Flush(ack) => flushes.push(ack),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> String {
        let path = std::env::temp_dir().join(format!("iac_{name}_{}.db", std::process::id()));
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
        path.to_str().unwrap().to_string()
    }

    fn world_error(path: &str, requested: Option<Pace>) -> String {
        Database::init(path).unwrap().load_snapshot(42, requested).expect_err("the world must be refused").to_string()
    }

    #[test]
    fn a_world_keeps_the_pace_it_was_created_with() {
        let path = temp_db("pace_fixed");
        let snapshot = Database::init(&path).unwrap().load_snapshot(42, Some(Pace::parse("blitz").unwrap())).unwrap();
        assert_eq!(snapshot.meta.pace.value(), 600.0);

        let kept = Database::init(&path).unwrap().load_snapshot(42, None).unwrap();
        assert_eq!(kept.meta.pace.value(), 600.0, "no --pace keeps the stored one");
        let same = Database::init(&path).unwrap().load_snapshot(42, Some(Pace::new(600.0).unwrap())).unwrap();
        assert_eq!(same.meta.pace.value(), 600.0);

        let text = world_error(&path, Some(Pace::new(10.0).unwrap()));
        assert!(text.contains("600 (blitz)") && text.contains("10 (season)") && text.contains("new --db"), "{text}");
    }

    #[test]
    fn a_database_without_world_meta_is_refused() {
        let path = temp_db("old_world");
        {
            let db = Database::init(&path).unwrap();
            db.save_server_state("world_seed", "42").unwrap();
            db.conn.lock().unwrap().execute_batch("DROP TABLE world_meta").unwrap();
        }
        let text = world_error(&path, None);
        assert!(text.contains("not migrated") && text.contains("new world"), "{text}");
    }

    #[test]
    fn a_world_from_another_generator_is_refused() {
        let path = temp_db("old_worldgen");
        Database::init(&path).unwrap().load_snapshot(42, None).unwrap();
        Connection::open(&path).unwrap().execute("UPDATE world_meta SET worldgen_version = 1", []).unwrap();
        let text = world_error(&path, None);
        assert!(text.contains("worldgen version 1"), "{text}");
    }

    use iac_shared::constants::{DEFAULT_WORLD_SEED, ResourceKind, ShipClass};
    use iac_shared::protocol::{Command, EventKind, QueueType};
    use iac_sim::GameEngine;
    use iac_sim::engine::template_npc_id;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    const TICKS: u64 = 2_000;
    const PLAYERS: usize = 4;
    const RESTART_EVERY: u64 = 250;
    const KILL_CHECK_EVERY: u64 = 25;

    /// Opens the world the way `main` does: load, restore, write back through
    /// the same connection.
    fn host(path: &str, pace: Option<Pace>) -> GameEngine {
        let db = Database::init(path).unwrap();
        let mut engine = GameEngine::restore(db.load_snapshot(DEFAULT_WORLD_SEED, pace).unwrap());
        engine.attach_persist(Box::new(Persister::spawn(db)));
        engine
    }

    fn enroll(engine: &mut GameEngine) -> Vec<u64> {
        (0..PLAYERS)
            .map(|i| engine.authenticate(&format!("Pilot{i}"), None, i % 2 == 0, || "f".repeat(64)).unwrap().player_id)
            .collect()
    }

    /// One tick of a busy world. The orders are a function of the tick and of
    /// what the engine holds, so two engines in the same state play the same.
    fn step(engine: &mut GameEngine, pids: &[u64]) -> Vec<EventKind> {
        let tick = engine.current_tick() + 1;
        let mut rng = StdRng::seed_from_u64(tick.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        if tick.is_multiple_of(300) {
            for pid in pids {
                let p = engine.players.get_mut(pid).unwrap();
                p.resources.metal = p.resources.metal.max(30_000.0);
                p.resources.crystal = p.resources.crystal.max(20_000.0);
                p.resources.deuterium = p.resources.deuterium.max(10_000.0);
            }
        }
        if tick.is_multiple_of(4) {
            for &pid in pids {
                let fleets: Vec<u64> = engine.fleets.values().filter(|f| f.owner_id == pid).map(|f| f.id).collect();
                let pick = |rng: &mut StdRng| if fleets.is_empty() { 0 } else { fleets[rng.random_range(0..fleets.len())] };
                let fleet_id = pick(&mut rng);
                let here = engine.fleets.get(&fleet_id).map(|f| f.location);
                if tick.is_multiple_of(200) {
                    let preset = PolicyPreset::ALL[rng.random_range(0..PolicyPreset::ALL.len())];
                    let _ = engine.handle_policy_update(pid, fleet_id, preset, None);
                    continue;
                }
                let cmd = match rng.random_range(0..19) {
                    0..=3 => here.map(|at| {
                        let around = engine.world_gen.connected_neighbors(at);
                        let n = around.slice();
                        Command::Move { fleet_id, target: n[rng.random_range(0..n.len())] }
                    }),
                    4 => Some(Command::Harvest { fleet_id, resource: HarvestResource::Auto }),
                    5 | 6 => Some(Command::Scan { fleet_id }),
                    7 => Some(Command::Build {
                        building_type: BuildingType::from_usize(rng.random_range(0..BuildingType::COUNT)).unwrap(),
                        reserve: rng.random_bool(0.3),
                    }),
                    8 => Some(Command::Research {
                        tech: ResearchType::from_usize(rng.random_range(0..ResearchType::COUNT)).unwrap(),
                        reserve: false,
                    }),
                    9 => Some(Command::BuildShip { ship_class: ShipClass::Scout, count: 2 }),
                    10 => Some(Command::BuildDefence { kind: DefenceKind::PulseTurret, count: 2 }),
                    11 => here.map(|at| Command::Attack { fleet_id, target_fleet_id: template_npc_id(at) }),
                    12 => Some(Command::Recall { fleet_id }),
                    13 => Some(Command::ExploreSite { fleet_id }),
                    14 => engine.fleets.get(&fleet_id).map(|f| Command::Split { fleet_id, ship_ids: vec![f.ships[0].id] }),
                    15 => Some(Command::Merge { fleet_id, other_fleet_id: pick(&mut rng) }),
                    16 => Some(Command::Load { fleet_id, resource: ResourceKind::ALL[rng.random_range(0..3)], amount: None }),
                    17 => Some(Command::Unload { fleet_id }),
                    _ => Some(Command::CollectSalvage { fleet_id }),
                };
                if let Some(cmd) = cmd {
                    let _ = engine.execute(pid, cmd);
                }
                if tick.is_multiple_of(40) {
                    let _ = engine.execute(pid, Command::CancelBuild { queue_type: QueueType::Building, id: 0 });
                }
            }
        }
        engine.tick().unwrap();
        let events = engine.drain_events().into_iter().map(|e| e.kind).collect();
        engine.persist_dirty_state();
        events
    }

    /// A fifth empire that plays no random orders: it carries an outpost to
    /// the Outer Ring, stocks it over further trips and raises it a level.
    struct Settler {
        pid: u64,
        fleet: u64,
        home: Hex,
        site: Hex,
        /// Lane hops from home to the site, the site last, by sectors where nothing attacks on sight.
        way: Vec<Hex>,
    }

    impl Settler {
        fn join(engine: &mut GameEngine) -> Settler {
            use iac_shared::world::NpcBehaviorType;
            let pid = engine.authenticate("Settler", None, false, || "f".repeat(64)).unwrap().player_id;
            let fleet = engine.fleets.values().find(|f| f.owner_id == pid).unwrap().id;
            let home = engine.players[&pid].homeworld;
            let calm = |at: Hex| {
                engine.world_gen.generate_sector(at).npc_template.is_none_or(|npc| npc.behavior == NpcBehaviorType::Passive)
            };
            let mut came_from = std::collections::HashMap::from([(home.to_key(), home)]);
            let mut frontier = std::collections::VecDeque::from([home]);
            let mut site = None;
            while let Some(at) = frontier.pop_front() {
                let t = engine.world_gen.generate_sector(at);
                let open = t.npc_template.is_none() && !t.has_derelict && t.terrain != iac_shared::constants::TerrainType::Anomaly;
                if at.dist_from_origin() >= 9 && open {
                    site = Some(at);
                    break;
                }
                for &next in engine.world_gen.connected_neighbors(at).slice() {
                    if calm(next) && !came_from.contains_key(&next.to_key()) {
                        came_from.insert(next.to_key(), at);
                        frontier.push_back(next);
                    }
                }
            }
            let site = site.expect("an outpost site reachable by calm lanes");
            let mut way = vec![site];
            while came_from[&way[way.len() - 1].to_key()] != home {
                way.push(came_from[&way[way.len() - 1].to_key()]);
            }
            way.reverse();
            Settler { pid, fleet, home, site, way }
        }

        /// The settler's order for this tick, from what the engine holds.
        fn play(&self, engine: &mut GameEngine) {
            let (pid, fleet_id) = (self.pid, self.fleet);
            let stock = &mut engine.players.get_mut(&pid).unwrap().resources;
            (stock.metal, stock.crystal, stock.deuterium) = (stock.metal.max(9000.0), stock.crystal.max(5000.0), stock.deuterium.max(3000.0));
            let Some(fleet) = engine.fleets.get(&fleet_id) else { return };
            if !matches!(fleet.state, FleetStatus::Idle | FleetStatus::Docked) || fleet.action_cooldown > 0 {
                return;
            }
            let (at, laden) = (fleet.location, fleet.cargo.total() > 0.0);
            let hop_toward = |goal: Hex| {
                let place = self.way.iter().position(|hop| *hop == at);
                if goal == self.site {
                    self.way[place.map_or(0, |i| i + 1)]
                } else {
                    place.and_then(|i| i.checked_sub(1)).map_or(self.home, |i| self.way[i])
                }
            };
            if at == self.home && !laden {
                for (resource, amount) in [(ResourceKind::Metal, 1500.0), (ResourceKind::Crystal, 800.0), (ResourceKind::Deuterium, 400.0)] {
                    let _ = engine.execute(pid, Command::Load { fleet_id, resource, amount: Some(amount) });
                }
            } else if at == self.site {
                let _ = engine.execute(pid, Command::FoundOutpost { fleet_id });
                let _ = engine.execute(pid, Command::Unload { fleet_id });
                let _ = engine.execute(pid, Command::UpgradeOutpost);
                if engine.outposts.get(&pid).is_some_and(|o| o.level >= 1) {
                    let _ = engine.execute(pid, Command::Move { fleet_id, target: hop_toward(self.home) });
                }
            } else {
                let goal = if laden { self.site } else { self.home };
                let _ = engine.execute(pid, Command::Move { fleet_id, target: hop_toward(goal) });
            }
        }
    }

    /// Where two JSON trees first differ. Leaves are compared as written, so
    /// 0.0 and -0.0 differ here as they do in the hash.
    fn first_difference(a: &serde_json::Value, b: &serde_json::Value, at: &str) -> Option<String> {
        use serde_json::Value::{Array, Object};
        match (a, b) {
            (Array(x), Array(y)) if x.len() == y.len() => {
                x.iter().zip(y).enumerate().find_map(|(i, (x, y))| first_difference(x, y, &format!("{at}[{i}]")))
            }
            (Object(x), Object(y)) if x.keys().eq(y.keys()) => {
                x.iter().find_map(|(k, v)| first_difference(v, &y[k], &format!("{at}.{k}")))
            }
            _ => {
                let (engine, database) = (a.to_string(), b.to_string());
                (engine != database).then(|| format!("{at}: {engine} in the engine, {database} in the database"))
            }
        }
    }

    fn assert_same(engine: &Snapshot, stored: &Snapshot, when: &str) {
        let (a, b) = (serde_json::to_value(engine).unwrap(), serde_json::to_value(stored).unwrap());
        if let Some(difference) = first_difference(&a, &b, "world") {
            panic!("{when}: {difference}");
        }
        assert_eq!(engine.hash(), stored.hash(), "{when}");
    }

    /// The world less what a batch leaves for later: the stamps and ore
    /// levels of charted sectors and the part-refilled ore of sectors.
    fn committed(mut world: Snapshot) -> Snapshot {
        world.known.clear();
        for ov in world.sector_overrides.values_mut() {
            (ov.metal_harvested, ov.crystal_harvested, ov.deut_harvested) = (0.0, 0.0, 0.0);
        }
        world
    }

    /// What the restarts of a run interrupted.
    #[derive(Debug, Default)]
    struct Caught {
        fights: u32,
        announced_raids: u32,
        scans: u32,
        jumps: u32,
        boardings: u32,
        all_at_once: u32,
        outpost_builds: u32,
        outpost_warnings: u32,
    }

    #[test]
    fn a_restart_through_sqlite_continues_the_same_world() {
        let path = temp_db("restart");
        let pace = Pace::parse("test").unwrap();
        let mut straight = GameEngine::new(DEFAULT_WORLD_SEED, pace);
        let mut hosted = host(&path, Some(pace));
        let pids = enroll(&mut straight);
        assert_eq!(enroll(&mut hosted), pids);
        let settler = Settler::join(&mut straight);
        assert_eq!(Settler::join(&mut hosted).pid, settler.pid);

        let mut caught = Caught::default();
        let mut raids_announced = 0u32;
        let mut last_restart = 0u64;
        let (mut raids, mut doctrine_moves, mut laden_holds) = (0u32, 0u32, 0u32);
        let (mut outpost_raids, mut stored_outposts) = (0u32, 0u32);
        for _ in 0..TICKS {
            settler.play(&mut straight);
            step(&mut straight, &pids);
            settler.play(&mut hosted);
            for event in step(&mut hosted, &pids) {
                match event {
                    EventKind::RaidIncoming(r) if r.outpost.is_some() => {}
                    EventKind::RaidResolved(r) if r.outpost.is_some() => outpost_raids += 1,
                    EventKind::RaidIncoming(_) => raids_announced += 1,
                    EventKind::RaidResolved(_) => {
                        raids_announced -= 1;
                        raids += 1;
                    }
                    EventKind::PolicyAction(p) if p.action == "move" => doctrine_moves += 1,
                    _ => {}
                }
            }
            let tick = hosted.current_tick();

            if tick.is_multiple_of(KILL_CHECK_EVERY) {
                hosted.flush_persistence().unwrap();
                let stored = Database::init(&path).unwrap().load_snapshot(DEFAULT_WORLD_SEED, None).unwrap();
                laden_holds += u32::from(stored.fleets.values().any(|f| f.laden && f.cargo.total() > 0.0));
                stored_outposts += u32::from(stored.outposts.get(&settler.pid).is_some_and(|o| o.store.total() > 0.0));
                assert_same(&committed(hosted.snapshot()), &committed(stored), &format!("killed after tick {tick}"));
            }

            let fight = !hosted.active_combats.is_empty();
            let raid = raids_announced > 0;
            let scan = pids.iter().any(|&pid| !hosted.scan_revealed_coords(pid).is_empty());
            let jump = hosted.fleets.values().any(|f| f.state == FleetStatus::Moving);
            let boarding = hosted.fleets.values().any(|f| f.state == FleetStatus::Exploring);
            let outpost = hosted.outposts.get(&settler.pid);
            let outpost_build = outpost.is_some_and(|o| o.job.is_some());
            let outpost_warning = outpost.is_some_and(|o| o.raid.incoming.is_some());
            let rare = (fight && raid && scan && jump && caught.all_at_once < 3)
                || (boarding && caught.boardings < 3)
                || (outpost_build && caught.outpost_builds < 3)
                || (outpost_warning && caught.outpost_warnings < 3);
            if tick.is_multiple_of(RESTART_EVERY) || (rare && tick > last_restart + 5) {
                last_restart = tick;
                caught.fights += u32::from(fight);
                caught.announced_raids += u32::from(raid);
                caught.scans += u32::from(scan);
                caught.jumps += u32::from(jump);
                caught.boardings += u32::from(boarding);
                caught.all_at_once += u32::from(fight && raid && scan && jump);
                caught.outpost_builds += u32::from(outpost_build);
                caught.outpost_warnings += u32::from(outpost_warning);

                hosted.checkpoint();
                hosted.persist_dirty_state();
                hosted.flush_persistence().unwrap();
                let stopped = hosted.snapshot();
                drop(hosted);

                let db = Database::init(&path).unwrap();
                let stored = db.load_snapshot(DEFAULT_WORLD_SEED, None).unwrap();
                assert_same(&stopped, &stored, &format!("stopped after tick {tick}"));
                assert_eq!(stored.hash(), straight.snapshot().hash(), "the hosted world left the unhosted one by tick {tick}");
                hosted = GameEngine::restore(stored);
                hosted.attach_persist(Box::new(Persister::spawn(db)));
            }
        }

        assert_eq!(hosted.snapshot().hash(), straight.snapshot().hash(), "restarts changed the course of the world");
        assert!(
            caught.all_at_once > 0 && caught.boardings > 0,
            "no restart interrupted a fight, an announced raid, a scan and a jump together, or none a boarding: {caught:?}",
        );
        assert!(raids > 0 && doctrine_moves > 0, "{raids} raids resolved, {doctrine_moves} moves by standing orders");
        assert!(laden_holds > 0, "no kill found a hold loaded at the dock in the database");
        let outpost = hosted.outposts.get(&settler.pid).expect("the settler founded an outpost");
        assert!(
            outpost.sector == settler.site && outpost.level >= 2,
            "the outpost was to be stocked over several trips and raised a level: it stands at level {}",
            outpost.level,
        );
        assert!(
            caught.outpost_builds > 0 && caught.outpost_warnings > 0 && outpost_raids > 0,
            "no restart fell inside an outpost's build, or none inside the warning of a raid on it ({outpost_raids} resolved): {caught:?}",
        );
        assert!(stored_outposts > 0, "no kill found a stocked outpost in the database");
    }

    #[test]
    fn a_world_with_bots_restarts_through_sqlite_as_if_it_had_never_stopped() {
        use iac_sim::bot::{Roster, Style};

        let path = temp_db("bots");
        let pace = Pace::parse("test").unwrap();
        let wanted: Vec<Style> = ["all:hands-on", "explorer:check-in", "turtle:hybrid"]
            .into_iter()
            .flat_map(|spec| Style::parse_seats(spec).unwrap())
            .map(|style| Style { sessions_per_day: 24, ..style })
            .collect();
        let seat = |engine: &mut GameEngine| {
            let mut roster = Roster::resume(engine);
            assert_eq!(roster.enlist(engine, &wanted, 0, || "b".repeat(64)), (wanted.len(), None));
            roster
        };
        let play = |engine: &mut GameEngine, roster: &mut Roster| {
            iac_sim::host::step(engine, roster, &mut (), |_, _| {}).unwrap();
        };

        let mut straight = GameEngine::new(DEFAULT_WORLD_SEED, pace);
        let mut hosted = host(&path, Some(pace));
        let (mut straight_bots, mut hosted_bots) = (seat(&mut straight), seat(&mut hosted));

        for restart in 1..=6 {
            for _ in 0..150 {
                play(&mut straight, &mut straight_bots);
                play(&mut hosted, &mut hosted_bots);
            }
            hosted.checkpoint();
            hosted.persist_dirty_state();
            hosted.flush_persistence().unwrap();
            let stopped = hosted.snapshot();
            drop(hosted);

            let db = Database::init(&path).unwrap();
            let stored = db.load_snapshot(DEFAULT_WORLD_SEED, None).unwrap();
            assert_same(&stopped, &stored, &format!("restart {restart}"));
            assert_eq!(stored.bots.len(), wanted.len());
            assert_eq!(stored.hash(), straight.snapshot().hash(), "the hosted bots left the unhosted ones by restart {restart}");
            hosted = GameEngine::restore(stored);
            hosted.attach_persist(Box::new(Persister::spawn(db)));
            hosted_bots = Roster::resume(&hosted);
            assert_eq!(hosted_bots.enlist(&mut hosted, &wanted, 0, || "b".repeat(64)), (0, None));
        }

        let score = |engine: &GameEngine| -> f32 {
            engine.bots().keys().map(|id| iac_sim::score::score_of(engine, &engine.players[id]).total()).sum()
        };
        assert!(score(&hosted) > 50.0 * wanted.len() as f32, "the bots played: {} points between them", score(&hosted));
        assert!(hosted.players.values().any(|p| p.explore_points > 0.0), "a bot's fleet flew out and came home");
    }

    #[test]
    fn a_world_saved_before_jumps_and_combats_were_stored_still_loads() {
        let path = temp_db("before_runtime");
        let mut engine = host(&path, Some(Pace::parse("test").unwrap()));
        let pids = enroll(&mut engine);
        for _ in 0..400 {
            step(&mut engine, &pids);
        }
        engine.checkpoint();
        engine.persist_dirty_state();
        engine.flush_persistence().unwrap();
        let saved = engine.snapshot();
        drop(engine);

        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "DROP TABLE runtime_state;
                 ALTER TABLE ships DROP COLUMN slot;
                 ALTER TABLE fleets DROP COLUMN move_cooldown;
                 ALTER TABLE fleets DROP COLUMN action_cooldown;
                 ALTER TABLE fleets DROP COLUMN idle_ticks;
                 ALTER TABLE fleets DROP COLUMN move_q;
                 ALTER TABLE fleets DROP COLUMN move_r;
                 ALTER TABLE fleet_policies DROP COLUMN next_eval_tick;
                 ALTER TABLE fleet_policies DROP COLUMN hold_reason;
                 ALTER TABLE fleet_policies DROP COLUMN hold_tick;
                 ALTER TABLE fleet_policies DROP COLUMN fuel_budget_pct;
                 ALTER TABLE fleet_policies DROP COLUMN fuel_credit;
                 ALTER TABLE fleet_policies DROP COLUMN fuel_since;
                 UPDATE fleets SET state = 'moving' WHERE id = (SELECT MIN(id) FROM fleets);
                 UPDATE fleets SET state = 'in_combat' WHERE id = (SELECT MAX(id) FROM fleets);",
            )
            .unwrap();

        let loaded = Database::init(&path).unwrap().load_snapshot(DEFAULT_WORLD_SEED, None).unwrap();
        assert_eq!(loaded.tick, saved.tick);
        assert_eq!(serde_json::to_value(&loaded.players).unwrap(), serde_json::to_value(&saved.players).unwrap());
        assert_eq!(loaded.fleets.keys().collect::<Vec<_>>(), saved.fleets.keys().collect::<Vec<_>>());
        assert_eq!(loaded.policies.keys().collect::<Vec<_>>(), saved.policies.keys().collect::<Vec<_>>());
        assert!(
            loaded.fleets.values().all(|f| !matches!(f.state, FleetStatus::Moving | FleetStatus::InCombat)),
            "a jump or a fight with nothing stored to resume it from must not leave the fleet stuck",
        );
        assert!(loaded.raid_states.is_empty() && loaded.combats.is_empty() && loaded.scan_reveals.is_empty());

        let mut engine = GameEngine::restore(loaded);
        engine.attach_persist(Box::new(Persister::spawn(Database::init(&path).unwrap())));
        for _ in 0..200 {
            step(&mut engine, &pids);
        }
        engine.checkpoint();
        engine.persist_dirty_state();
        engine.flush_persistence().unwrap();
        let stored = Database::init(&path).unwrap().load_snapshot(DEFAULT_WORLD_SEED, None).unwrap();
        assert_same(&engine.snapshot(), &stored, "an old world, once it has run on");
    }
}
