// SQLite persistence layer using rusqlite.
// Mirrors the Zig database.zig schema and operations.

use std::sync::{mpsc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusqlite::{Connection, params, OptionalExtension};
use log::{debug, error, info, warn};

use iac_shared::hex::Hex;
use iac_shared::constants::{Density, Resources, ShipClass};
use iac_shared::scaling::{BuildingType, BuildingLevels, ResearchType, ResearchLevels};

use crate::engine::{Player, Fleet, Ship, SectorOverride, BuildQueueEntry, ShipQueueEntry, ResearchQueueEntry, FleetStatus, FleetPolicy};
use iac_shared::protocol::{HarvestResource, PolicyPreset, PolicyParams};

/// Float storage: multiply by 1000 and store as integer for precision.
fn float_to_stored(val: f32) -> i64 {
    (val * 1000.0) as i64
}

// Stored values are written as i64 (millis) but the columns have REAL
// affinity, so SQLite converts them to floats on insert. Read as f64 —
// rusqlite converts both INTEGER and REAL values losslessly — or loading
// saved state fails with InvalidColumnType.
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

            CREATE TABLE IF NOT EXISTS players (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                homeworld_q INTEGER NOT NULL,
                homeworld_r INTEGER NOT NULL,
                metal REAL DEFAULT 500,
                crystal REAL DEFAULT 300,
                deuterium REAL DEFAULT 100
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
                cargo_deuterium REAL DEFAULT 0
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
                start_tick INTEGER NOT NULL,
                end_tick INTEGER NOT NULL
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

            CREATE INDEX IF NOT EXISTS idx_fleets_player ON fleets(player_id);
            CREATE INDEX IF NOT EXISTS idx_fleets_location ON fleets(q, r);
            CREATE INDEX IF NOT EXISTS idx_ships_fleet ON ships(fleet_id);
            CREATE INDEX IF NOT EXISTS idx_explored_player ON explored_edges(player_id);
            CREATE INDEX IF NOT EXISTS idx_buildings_player ON buildings(player_id);
            CREATE INDEX IF NOT EXISTS idx_research_player ON research(player_id);
            CREATE INDEX IF NOT EXISTS idx_build_queue_player ON build_queue(player_id);
            ",
        )?;

        // Columns added after release — migrate old worlds in place.
        Self::ensure_column(&conn, "sectors_modified", "site_looted_tick", "INTEGER")?;
        Self::ensure_column(&conn, "sectors_modified", "site_ambush_bumps", "INTEGER DEFAULT 0")?;
        Self::ensure_column(&conn, "fleets", "harvest_resource", "TEXT DEFAULT 'auto'")?;
        Self::ensure_column(&conn, "players", "token_hash", "BLOB")?;

        info!("Schema verified");
        Ok(())
    }

    /// ALTER TABLE ... ADD COLUMN, but only if the column is missing.
    fn ensure_column(conn: &Connection, table: &str, column: &str, ddl: &str) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let exists = stmt
            .query_map(params![], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|name| name == column);
        if !exists {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl};"))?;
            info!("Migrated: {table}.{column}");
        }
        Ok(())
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
            "INSERT OR REPLACE INTO players (id, name, homeworld_q, homeworld_r, metal, crystal, deuterium, token_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                player.id as i64,
                &player.name,
                player.homeworld.q as i64,
                player.homeworld.r as i64,
                float_to_stored(player.resources.metal),
                float_to_stored(player.resources.crystal),
                float_to_stored(player.resources.deuterium),
                player.token_hash.as_ref().map(|h| h.as_slice()),
            ],
        )?;
        Ok(())
    }

    pub fn load_players(&self) -> Result<Vec<Player>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, homeworld_q, homeworld_r, metal, crystal, deuterium, token_hash FROM players",
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
                building_queue: None,
                ship_queue: None,
                research_queue: None,
                token_hash: row.get::<_, Option<Vec<u8>>>(7)?.and_then(|v| v.try_into().ok()),
            })
        })?;
        players.collect()
    }

    // ── Fleets ────────────────────────────────────────────────────

    pub fn save_fleet(&self, fleet: &Fleet) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO fleets (id, player_id, q, r, state, fuel, fuel_max,
             cargo_metal, cargo_crystal, cargo_deuterium, harvest_resource)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
            ],
        )?;
        self.save_ships_inner(&conn, fleet)
    }

    /// Rewrites the fleet's ship rows. REPLACE, because a ship that just
    /// moved here by split or merge may still have a row under its old fleet
    /// when this fleet happens to be saved first.
    fn save_ships_inner(&self, conn: &Connection, fleet: &Fleet) -> Result<(), rusqlite::Error> {
        conn.execute(
            "DELETE FROM ships WHERE fleet_id = ?1",
            params![fleet.id as i64],
        )?;

        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO ships (id, fleet_id, player_id, class, hull, hull_max, shield, shield_max, weapon_power, speed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;

        for ship in &fleet.ships[0..fleet.ship_count] {
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
            ])?;
        }
        Ok(())
    }

    pub fn load_fleets(&self) -> Result<Vec<Fleet>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, player_id, q, r, state, fuel, fuel_max,
                    cargo_metal, cargo_crystal, cargo_deuterium, harvest_resource
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
            ))
        })?;

        for row_result in fleet_rows {
            let (fid, pid, q, r, state_str, fuel, fuel_max, cm, cc, cd, harvest) = row_result?;
            let mut fleet = Fleet {
                id: fid as u64,
                owner_id: pid as u64,
                location: Hex { q: q as i16, r: r as i16 },
                state: parse_fleet_status(&state_str),
                ships: [Ship::default(); crate::engine::MAX_SHIPS_PER_FLEET],
                ship_count: 0,
                cargo: Resources {
                    metal: stored_to_float(cm),
                    crystal: stored_to_float(cc),
                    deuterium: stored_to_float(cd),
                },
                fuel: stored_to_float(fuel),
                fuel_max: stored_to_float(fuel_max),
                move_cooldown: 0,
                action_cooldown: 0,
                move_target: None,
                idle_ticks: 0,
                harvest_target: parse_harvest_resource(harvest.as_deref().unwrap_or("auto")),
            };
            fleet.ship_count = self.load_ships_into_inner(&conn, fid, &mut fleet.ships)?;
            fleets.push(fleet);
        }
        Ok(fleets)
    }

    fn load_ships_into_inner(&self, conn: &Connection, fleet_id: i64, ships: &mut [Ship]) -> Result<usize, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT id, class, hull, hull_max, shield, shield_max, weapon_power, speed
             FROM ships WHERE fleet_id = ?1",
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
             site_looted_tick, site_ambush_bumps)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
            ],
        )?;
        Ok(())
    }

    pub fn load_sector_overrides(&self) -> Result<Vec<SectorOverrideRow>, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT q, r, metal_density, crystal_density, deut_density,
                    metal_harvested, crystal_harvested, deut_harvested,
                    npc_cleared_tick, site_looted_tick, site_ambush_bumps
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
            ))
        })?;

        for row_result in rows {
            let (q, r, md, cd, dd, mh, ch, dh, nct, slt, sab) = row_result?;
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
                    salvage: None,
                    salvage_despawn_tick: None,
                    npc_cleared_tick: nct.map(|t| t as u64),
                    site_looted_tick: slt.map(|t| t as u64),
                    site_ambush_bumps: sab.unwrap_or(0).clamp(0, 255) as u8,
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
             (fleet_id, preset, min_fuel_pct, cargo_return_pct, max_range, engage_ratio_x10, work_q, work_r)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                fleet_id as i64,
                policy_preset_to_str(policy.preset),
                policy.params.min_fuel_pct as i64,
                policy.params.cargo_return_pct as i64,
                policy.params.max_range as i64,
                policy.params.engage_ratio_x10 as i64,
                policy.work_sector.map(|h| h.q as i64),
                policy.work_sector.map(|h| h.r as i64),
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
            "SELECT fleet_id, preset, min_fuel_pct, cargo_return_pct, max_range, engage_ratio_x10, work_q, work_r
             FROM fleet_policies",
        )?;
        let rows = stmt.query_map(params![], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;

        let mut policies = Vec::new();
        for row_result in rows {
            let (fid, preset, fuel, cargo, range, engage, wq, wr) = row_result?;
            let work_sector = match (wq, wr) {
                (Some(q), Some(r)) => Some(Hex { q: q as i16, r: r as i16 }),
                _ => None,
            };
            policies.push((fid as u64, FleetPolicy {
                preset: parse_policy_preset(&preset),
                params: PolicyParams {
                    min_fuel_pct: fuel.clamp(0, 100) as u8,
                    cargo_return_pct: cargo.clamp(0, 100) as u8,
                    max_range: range.clamp(1, 30) as u8,
                    engage_ratio_x10: engage.clamp(1, 100) as u8,
                },
                work_sector,
                next_eval_tick: 0,
                last_hold: None,
            }));
        }
        Ok(policies)
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

    // ── Buildings / Research / Queue ──────────────────────────────

    pub fn save_buildings(&self, player_id: u64, levels: &BuildingLevels) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO buildings (player_id, building_type, level) VALUES (?1, ?2, ?3)",
        )?;

        for bt in 0..BuildingType::COUNT {
            let bt: BuildingType = match bt {
                0 => BuildingType::MetalMine,
                1 => BuildingType::CrystalMine,
                2 => BuildingType::DeuteriumSynthesizer,
                3 => BuildingType::Shipyard,
                4 => BuildingType::ResearchLab,
                5 => BuildingType::FuelDepot,
                6 => BuildingType::SensorArray,
                7 => BuildingType::DefenseGrid,
                _ => unreachable!(),
            };
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
            if bt_int >= 0 && bt_int < BuildingType::COUNT as i64 {
                let bt: BuildingType = match bt_int {
                    0 => BuildingType::MetalMine,
                    1 => BuildingType::CrystalMine,
                    2 => BuildingType::DeuteriumSynthesizer,
                    3 => BuildingType::Shipyard,
                    4 => BuildingType::ResearchLab,
                    5 => BuildingType::FuelDepot,
                    6 => BuildingType::SensorArray,
                    7 => BuildingType::DefenseGrid,
                    _ => unreachable!(),
                };
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

        for rt in 0..ResearchType::COUNT {
            let rt: ResearchType = match rt {
                0 => ResearchType::FuelEfficiency,
                1 => ResearchType::ExtendedFuelTanks,
                2 => ResearchType::ReinforcedHulls,
                3 => ResearchType::AdvancedShields,
                4 => ResearchType::WeaponsResearch,
                5 => ResearchType::Navigation,
                6 => ResearchType::HarvestingEfficiency,
                7 => ResearchType::CorvetteTech,
                8 => ResearchType::FrigateTech,
                9 => ResearchType::CruiserTech,
                10 => ResearchType::HaulerTech,
                11 => ResearchType::EmergencyJump,
                _ => unreachable!(),
            };
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
            if tech_int >= 0 && tech_int < ResearchType::COUNT as i64 {
                let tech: ResearchType = match tech_int {
                    0 => ResearchType::FuelEfficiency,
                    1 => ResearchType::ExtendedFuelTanks,
                    2 => ResearchType::ReinforcedHulls,
                    3 => ResearchType::AdvancedShields,
                    4 => ResearchType::WeaponsResearch,
                    5 => ResearchType::Navigation,
                    6 => ResearchType::HarvestingEfficiency,
                    7 => ResearchType::CorvetteTech,
                    8 => ResearchType::FrigateTech,
                    9 => ResearchType::CruiserTech,
                    10 => ResearchType::HaulerTech,
                    11 => ResearchType::EmergencyJump,
                    _ => unreachable!(),
                };
                levels.set(tech, level as u8);
            }
        }
        Ok(levels)
    }

    pub fn save_build_queue(&self, player_id: u64, player: &Player) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM build_queue WHERE player_id = ?1",
            params![player_id as i64],
        )?;

        let mut stmt = conn.prepare(
            "INSERT INTO build_queue (player_id, queue_type, item_type, target_level, count, built, start_tick, end_tick)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;

        if let Some(ref q) = player.building_queue {
            stmt.execute(params![
                player_id as i64,
                "building",
                q.building_type as i64,
                q.target_level as i64,
                1i64,
                0i64,
                q.start_tick as i64,
                q.end_tick as i64,
            ])?;
        }

        if let Some(ref q) = player.ship_queue {
            stmt.execute(params![
                player_id as i64,
                "ship",
                q.ship_class as i64,
                rusqlite::types::Value::Null,
                q.count as i64,
                q.built as i64,
                q.start_tick as i64,
                q.end_tick as i64,
            ])?;
        }

        if let Some(ref q) = player.research_queue {
            stmt.execute(params![
                player_id as i64,
                "research",
                q.tech as i64,
                q.target_level as i64,
                1i64,
                0i64,
                q.start_tick as i64,
                q.end_tick as i64,
            ])?;
        }

        Ok(())
    }

    pub fn load_build_queues(&self, player_id: u64) -> Result<BuildQueueData, rusqlite::Error> {
        let conn = self.conn.lock().unwrap();
        let mut data = BuildQueueData::default();
        let mut stmt = conn.prepare(
            "SELECT queue_type, item_type, target_level, count, built, start_tick, end_tick
             FROM build_queue WHERE player_id = ?1",
        )?;

        let rows = stmt.query_map(params![player_id as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?;

        for row_result in rows {
            let (qt, item_type, target_level, count, built, start_tick, end_tick) = row_result?;
            match qt.as_str() {
                "building" => {
                    if item_type >= 0 && item_type < BuildingType::COUNT as i64 {
                        let bt: BuildingType = match item_type {
                            0 => BuildingType::MetalMine,
                            1 => BuildingType::CrystalMine,
                            2 => BuildingType::DeuteriumSynthesizer,
                            3 => BuildingType::Shipyard,
                            4 => BuildingType::ResearchLab,
                            5 => BuildingType::FuelDepot,
                            6 => BuildingType::SensorArray,
                            7 => BuildingType::DefenseGrid,
                            _ => unreachable!(),
                        };
                        data.building = Some(BuildQueueEntry {
                            building_type: bt,
                            target_level: target_level.unwrap_or(0) as u8,
                            start_tick: start_tick as u64,
                            end_tick: end_tick as u64,
                        });
                    }
                }
                "ship" => {
                    if (0..5).contains(&item_type) {
                        let sc: ShipClass = match item_type {
                            0 => ShipClass::Scout,
                            1 => ShipClass::Corvette,
                            2 => ShipClass::Frigate,
                            3 => ShipClass::Cruiser,
                            4 => ShipClass::Hauler,
                            _ => unreachable!(),
                        };
                        data.ship = Some(ShipQueueEntry {
                            ship_class: sc,
                            count: count as u16,
                            built: built as u16,
                            start_tick: start_tick as u64,
                            end_tick: end_tick as u64,
                        });
                    }
                }
                "research" if (0..ResearchType::COUNT as i64).contains(&item_type) => {
                    {
                        let tech: ResearchType = match item_type {
                            0 => ResearchType::FuelEfficiency,
                            1 => ResearchType::ExtendedFuelTanks,
                            2 => ResearchType::ReinforcedHulls,
                            3 => ResearchType::AdvancedShields,
                            4 => ResearchType::WeaponsResearch,
                            5 => ResearchType::Navigation,
                            6 => ResearchType::HarvestingEfficiency,
                            7 => ResearchType::CorvetteTech,
                            8 => ResearchType::FrigateTech,
                            9 => ResearchType::CruiserTech,
                            10 => ResearchType::HaulerTech,
                            11 => ResearchType::EmergencyJump,
                            _ => unreachable!(),
                        };
                        data.research = Some(ResearchQueueEntry {
                            tech,
                            target_level: target_level.unwrap_or(0) as u8,
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
    pub building: Option<BuildQueueEntry>,
    pub ship: Option<ShipQueueEntry>,
    pub research: Option<ResearchQueueEntry>,
}


// ── Background persistence ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ExploredEdge {
    pub player_id: u64,
    pub from: Hex,
    pub to: Hex,
    pub tick: u64,
}

/// A snapshot of everything that changed since the previous batch. Rows are
/// whole-row upserts, so replaying batches in order is idempotent.
#[derive(Debug, Default)]
pub struct PersistBatch {
    pub tick: u64,
    pub next_id: u64,
    pub players: Vec<Player>,
    pub fleets: Vec<Fleet>,
    pub deleted_fleets: Vec<u64>,
    pub policies: Vec<(u64, FleetPolicy)>,
    pub deleted_policies: Vec<u64>,
    pub sectors: Vec<(Hex, SectorOverride)>,
    pub explored_edges: Vec<ExploredEdge>,
}

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
        }
        for fleet in &batch.fleets {
            self.save_fleet(fleet)?;
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
        for e in &batch.explored_edges {
            self.save_explored_edge(e.player_id, e.from, e.to, e.tick)?;
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

    pub fn submit(&self, batch: PersistBatch) {
        if let Some(tx) = &self.tx
            && tx.send(WriterMsg::Batch(Box::new(batch))).is_err()
        {
            error!("persist thread is gone; state is no longer being saved");
        }
    }

    /// Block until everything submitted so far is committed.
    pub fn flush(&self) -> Result<(), String> {
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

    fn sector_batch(tick: u64) -> PersistBatch {
        PersistBatch {
            tick,
            next_id: 9,
            sectors: vec![(Hex { q: 3, r: 4 }, SectorOverride { metal_harvested: 5.0, ..Default::default() })],
            ..Default::default()
        }
    }

    #[test]
    fn flush_commits_submitted_batches() {
        let path = temp_db("flush");
        let persister = Persister::spawn(Database::init(&path).unwrap());
        persister.submit(sector_batch(41));
        persister.submit(sector_batch(42));
        persister.flush().expect("flush");

        let reader = Database::init(&path).unwrap();
        assert_eq!(reader.load_server_state("current_tick").unwrap().as_deref(), Some("42"));
        let rows = reader.load_sector_overrides().unwrap();
        assert_eq!(rows.len(), 1);
        assert!((rows[0].override_data.metal_harvested - 5.0).abs() < 0.01);
    }

    #[test]
    fn a_blocked_database_never_blocks_the_caller_and_loses_nothing() {
        let path = temp_db("blocked");
        let persister = Persister::spawn(Database::init(&path).unwrap());

        // A foreign writer holds the lock, as a stalled disk or a backup would.
        let intruder = Connection::open(&path).unwrap();
        intruder.execute_batch("BEGIN IMMEDIATE").unwrap();

        let started = Instant::now();
        persister.submit(sector_batch(100));
        persister.submit(sector_batch(101));
        assert!(started.elapsed() < Duration::from_millis(50), "submit must not wait for the disk");
        assert!(persister.flush().is_err(), "flush reports the failed write");

        intruder.execute_batch("COMMIT").unwrap();
        persister.flush().expect("batches kept for retry commit once the lock clears");

        let reader = Database::init(&path).unwrap();
        assert_eq!(reader.load_server_state("current_tick").unwrap().as_deref(), Some("101"));
        assert_eq!(reader.load_sector_overrides().unwrap().len(), 1);
    }

    #[test]
    fn dropping_the_persister_writes_what_is_queued() {
        let path = temp_db("drop");
        {
            let persister = Persister::spawn(Database::init(&path).unwrap());
            persister.submit(sector_batch(7));
        }
        let reader = Database::init(&path).unwrap();
        assert_eq!(reader.load_server_state("current_tick").unwrap().as_deref(), Some("7"));
    }

    #[test]
    fn database_runs_in_wal_mode() {
        let path = temp_db("wal");
        let db = Database::init(&path).unwrap();
        let mode: String = db.conn.lock().unwrap().query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "wal");
    }
}
