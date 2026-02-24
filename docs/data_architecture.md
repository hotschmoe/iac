# In Amber Clad — Data & Persistence Architecture

**Version:** 0.1.0-draft
**Audience:** Implementors working on the server engine and database layer.

---

## 1. Core Principle

**The game runs in memory. The database is a checkpoint.**

All authoritative game state lives in Zig data structures (hashmaps, arrays, structs) inside the `GameEngine`. The server reads from memory, writes to memory, and broadcasts from memory. SQLite exists for exactly two purposes:

1. **Crash recovery** — if the server dies, restart from the last checkpoint.
2. **Cold start** — load everything into memory when the server boots.

Clients never read from or write to the database. They interact exclusively through the WebSocket protocol, which reads from the server's in-memory state.

---

## 2. Data Flow

```
                          ┌──────────────────────────────────┐
  Client A ──WebSocket──▶ │         SERVER (in memory)       │
  Client B ──WebSocket──▶ │                                  │
  Client C ──WebSocket──▶ │  command    ┌──────────────┐     │
                          │  queue ───▶ │  GameEngine   │     │
                          │             │  (hashmaps,   │     │
                          │             │   structs)    │     │
                          │             └──────┬───────┘     │
                          │                    │              │
                          │         ┌──────────┼──────────┐  │
                          │         │          │          │  │
                          │         ▼          ▼          ▼  │
                          │    broadcast   mark dirty   tick │
                          │    to clients  entities     ++   │
                          │         │          │             │
                          │         │          ▼             │
                          │         │   every 30 ticks:      │
                          │         │   ┌─────────────┐      │
                          │         │   │   SQLite    │      │
                          │         │   │  (WAL mode) │      │
                          │         │   └─────────────┘      │
                          └─────────┼────────────────────────┘
                                    │
                                    ▼
                            Client receives
                            state via WebSocket
                            (never from DB)
```

### 2.1 Write Path (Client → Memory → Eventually Disk)

```
1. Client sends command over WebSocket
       │
2. Network layer deserializes, queues in memory
       │
3. Tick loop picks up queued commands
       │
4. GameEngine validates and applies to in-memory state
   (e.g., fleet.location = new_hex, player.resources -= cost)
       │
5. Entity marked dirty: dirty_players.put(player_id, {})
       │
6. Tick continues: combat, harvesting, production, etc.
       │
7. State deltas broadcast to clients (from memory)
       │
   ... ticks continue ...
       │
8. Every 30 ticks: persistDirtyState()
   - BEGIN IMMEDIATE
   - Write all dirty players, fleets, sectors
   - COMMIT
   - Clear dirty sets
```

### 2.2 Read Path (Disk → Memory, at startup only)

```
1. Server starts
       │
2. Open SQLite database
       │
3. PRAGMA journal_mode=WAL
       │
4. Load server_state table → restore tick counter, next_id, world_seed
       │
5. Load players table → populate players hashmap
       │
6. Load fleets + ships tables → populate fleets hashmap
       │
7. Load sectors_modified → populate sector_overrides hashmap
       │
8. Load explored_edges → populate per-player explored sets
       │
9. Server ready, tick loop begins
       │
   (No further reads from SQLite during normal operation)
```

### 2.3 Client Read Path (Memory → WebSocket → Client)

```
1. Client connects, authenticates
       │
2. Server sends full_state message (serialized from memory)
       │
3. Each tick, server sends tick_update with deltas
   (only changed fields, only entities relevant to this player)
       │
4. Client maintains its own local state mirror (client/state.zig)
   updated by applying server messages
       │
   (Client never queries the database)
```

---

## 3. SQLite Configuration

### 3.1 Pragmas (set once at connection open)

```sql
PRAGMA journal_mode = WAL;          -- concurrent readers + one writer
PRAGMA synchronous = NORMAL;        -- safe with WAL, faster than FULL
PRAGMA foreign_keys = ON;           -- enforce referential integrity
PRAGMA cache_size = -8000;          -- 8MB page cache
PRAGMA busy_timeout = 5000;         -- 5s wait on lock contention (shouldn't happen)
PRAGMA wal_autocheckpoint = 1000;   -- checkpoint every 1000 pages
```

### 3.2 Why WAL Mode

Default SQLite uses rollback journals: readers and writers block each other. WAL (Write-Ahead Logging) changes this:

- **Unlimited concurrent readers**, each seeing a consistent snapshot.
- **One concurrent writer**, non-blocking with readers.
- Writers append to a WAL file; readers read from the main DB + WAL.
- Periodic checkpoints merge WAL back into the main DB.

For IAC: the server is the sole writer. Any external tools (admin dashboard, backup scripts, analytics) can read freely without impacting game performance.

### 3.3 Why SYNCHRONOUS = NORMAL

With WAL mode, `SYNCHRONOUS = NORMAL` means:

- WAL writes are not synced to disk on every commit (faster).
- Checkpoints do sync (durable).
- Risk: a power failure *during a write* could lose that transaction.
- For a game: losing up to 30 seconds of state on a power failure is acceptable. The alternative (`FULL`) adds ~10ms per transaction for disk sync.

---

## 4. In-Memory State Model

### 4.1 Primary Data Structures

```zig
// GameEngine owns all live state:

players:         AutoHashMap(u64, Player)          // player_id → player
fleets:          AutoHashMap(u64, Fleet)            // fleet_id → fleet
npc_fleets:      AutoHashMap(u64, NpcFleet)         // npc_fleet_id → npc fleet
active_combats:  AutoHashMap(u64, Combat)           // combat_id → combat
sector_overrides: AutoHashMap(u32, SectorOverride)  // hex_key → override

// Dirty tracking (cleared after each persist):
dirty_players:   AutoHashMap(u64, void)
dirty_fleets:    AutoHashMap(u64, void)
dirty_sectors:   AutoHashMap(u32, void)
```

### 4.2 What Lives in Memory vs. What's Generated

| Data | Source | Stored in SQLite? |
|---|---|---|
| Base sector properties (terrain, resources, NPCs) | Procedural generation from `(q, r, world_seed)` | No — regenerated on demand |
| Sector modifications (depleted resources, salvage) | In-memory overlay on procedural base | Yes, in `sectors_modified` |
| Edge connectivity (which hexes connect) | Procedural from symmetric edge hash | No — recalculated on demand |
| Player explored edges | In-memory set per player | Yes, in `explored_edges` |
| Player resources, homeworld | In-memory, checkpointed | Yes, in `players` |
| Fleet state, position, cargo | In-memory, checkpointed | Yes, in `fleets` + `ships` |
| NPC fleets | Spawned from sector templates, ephemeral | No — respawned from seed |
| Active combats | In-memory only, transient | No — if server dies mid-combat, combat resets |
| Build/research queues | In-memory, checkpointed | Yes, in `buildings` + `research` |

### 4.3 Memory Budget Estimate

For a server with 1,000 concurrent players:

| Data | Per-entity size | Count | Total |
|---|---|---|---|
| Player | ~128 bytes | 1,000 | 128 KB |
| Fleet | ~4 KB (64 ships × 64 bytes) | 3,000 | 12 MB |
| NPC Fleet | ~2 KB | 5,000 | 10 MB |
| Sector Override | ~64 bytes | 50,000 | 3.2 MB |
| Explored Edges | ~8 bytes per edge | 500,000 | 4 MB |
| Active Combats | ~256 bytes | 200 | 51 KB |

**Total: ~30 MB** for 1,000 players. Trivial. Even 10,000 players would be ~300 MB — well within a modest server's RAM.

---

## 5. Persistence Strategy

### 5.1 Checkpoint Interval

Write dirty state every **30 ticks** (30 seconds at 1 Hz). This is the **maximum data loss window** on crash.

Rationale:
- 30s of lost game state is imperceptible. Players' homeworld queues (minutes/hours) are barely affected. Fleet positions might rewind one or two sectors.
- 30 ticks of batched writes is efficient. Individual writes per tick would be 1 transaction/second with potentially dozens of statements — wasteful and slower.
- The dirty tracking system means we only write entities that actually changed, not the entire state.

### 5.2 Batch Write Pattern

```zig
pub fn persistDirtyState(self: *GameEngine) !void {
    // Single transaction for atomicity and performance
    try self.db.exec("BEGIN IMMEDIATE");

    // Players
    var player_iter = self.dirty_players.iterator();
    while (player_iter.next()) |entry| {
        if (self.players.get(entry.key_ptr.*)) |player| {
            try self.db.savePlayer(player);
        }
    }

    // Fleets + their ships
    var fleet_iter = self.dirty_fleets.iterator();
    while (fleet_iter.next()) |entry| {
        if (self.fleets.get(entry.key_ptr.*)) |fleet| {
            try self.db.saveFleet(fleet);
        }
    }

    // Modified sectors
    var sector_iter = self.dirty_sectors.iterator();
    while (sector_iter.next()) |entry| {
        if (self.sector_overrides.get(entry.key_ptr.*)) |override| {
            const hex = shared.Hex.fromKey(entry.key_ptr.*);
            try self.db.saveSectorOverride(hex.q, hex.r, override);
        }
    }

    // Server state (tick counter, next_id)
    try self.db.saveServerState("tick", self.tick);
    try self.db.saveServerState("next_id", self.next_id);

    try self.db.exec("COMMIT");

    // Clear dirty tracking
    self.dirty_players.clearRetainingCapacity();
    self.dirty_fleets.clearRetainingCapacity();
    self.dirty_sectors.clearRetainingCapacity();
}
```

### 5.3 Why BEGIN IMMEDIATE

`BEGIN IMMEDIATE` acquires the write lock at the start of the transaction. Alternatives:

- `BEGIN` (deferred): acquires the lock lazily on first write. If another connection tries to write between BEGIN and the first write, you get `SQLITE_BUSY`. Shouldn't happen in our single-writer model, but IMMEDIATE is defensive.
- `BEGIN EXCLUSIVE`: like IMMEDIATE but also blocks readers. We don't want this — WAL mode readers should be unaffected by our writes.

### 5.4 Prepared Statements

All SQL statements are prepared once at database initialization and reused with parameter binding. This avoids re-parsing SQL on every write.

```zig
// At Database.init():
self.stmt_save_player = try self.db.prepare(
    \\INSERT OR REPLACE INTO players
    \\  (id, name, homeworld_q, homeworld_r, metal, crystal, deuterium)
    \\VALUES (?, ?, ?, ?, ?, ?, ?)
);

self.stmt_save_fleet = try self.db.prepare(
    \\INSERT OR REPLACE INTO fleets
    \\  (id, player_id, q, r, state, fuel, fuel_max,
    \\   cargo_metal, cargo_crystal, cargo_deuterium)
    \\VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
);

self.stmt_save_ship = try self.db.prepare(
    \\INSERT OR REPLACE INTO ships
    \\  (id, player_id, fleet_id, class, hull, hull_max,
    \\   shield, shield_max, weapon_power, speed)
    \\VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
);

// At write time:
self.stmt_save_player.reset();
self.stmt_save_player.bind(.{
    player.id, player.name,
    player.homeworld.q, player.homeworld.r,
    player.resources.metal, player.resources.crystal,
    player.resources.deuterium,
});
try self.stmt_save_player.step();
```

Performance impact: prepared statements skip the SQL parser and query planner on reuse. For our batch writes of potentially hundreds of entities, this saves meaningful time.

### 5.5 Graceful Shutdown

On server shutdown signal (SIGTERM, SIGINT):

1. Stop accepting new client connections.
2. Broadcast "server shutting down" message to all clients.
3. Run one final `persistDirtyState()` to flush everything.
4. Close database connection (triggers WAL checkpoint).
5. Exit.

This ensures zero data loss on clean shutdown regardless of where in the 30-tick cycle we are.

---

## 6. Schema

Full schema with indexes and constraints. This is the authoritative version (supersedes SPEC.md §11 if they diverge).

```sql
-- Server metadata
CREATE TABLE IF NOT EXISTS server_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Stores: tick, next_id, world_seed

-- Players
CREATE TABLE IF NOT EXISTS players (
    id           INTEGER PRIMARY KEY,
    name         TEXT UNIQUE NOT NULL,
    token        TEXT NOT NULL,
    homeworld_q  INTEGER NOT NULL,
    homeworld_r  INTEGER NOT NULL,
    metal        REAL NOT NULL DEFAULT 500,
    crystal      REAL NOT NULL DEFAULT 300,
    deuterium    REAL NOT NULL DEFAULT 100,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX IF NOT EXISTS idx_players_name ON players(name);

-- Buildings (one row per building type per player)
CREATE TABLE IF NOT EXISTS buildings (
    player_id      INTEGER NOT NULL REFERENCES players(id),
    building_type  TEXT NOT NULL,
    level          INTEGER NOT NULL DEFAULT 0,
    build_start_tick INTEGER,
    build_end_tick   INTEGER,
    PRIMARY KEY (player_id, building_type)
);

-- Research (one row per tech per player)
CREATE TABLE IF NOT EXISTS research (
    player_id          INTEGER NOT NULL REFERENCES players(id),
    tech               TEXT NOT NULL,
    level              INTEGER NOT NULL DEFAULT 0,
    research_start_tick INTEGER,
    research_end_tick   INTEGER,
    PRIMARY KEY (player_id, tech)
);

-- Fleets
CREATE TABLE IF NOT EXISTS fleets (
    id              INTEGER PRIMARY KEY,
    player_id       INTEGER NOT NULL REFERENCES players(id),
    q               INTEGER NOT NULL,
    r               INTEGER NOT NULL,
    state           TEXT NOT NULL DEFAULT 'idle',
    fuel            REAL NOT NULL,
    fuel_max        REAL NOT NULL,
    cargo_metal     REAL NOT NULL DEFAULT 0,
    cargo_crystal   REAL NOT NULL DEFAULT 0,
    cargo_deuterium REAL NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_fleets_player ON fleets(player_id);
CREATE INDEX IF NOT EXISTS idx_fleets_location ON fleets(q, r);

-- Ships (belong to a fleet)
CREATE TABLE IF NOT EXISTS ships (
    id           INTEGER PRIMARY KEY,
    player_id    INTEGER NOT NULL REFERENCES players(id),
    fleet_id     INTEGER NOT NULL REFERENCES fleets(id) ON DELETE CASCADE,
    class        TEXT NOT NULL,
    hull         REAL NOT NULL,
    hull_max     REAL NOT NULL,
    shield       REAL NOT NULL,
    shield_max   REAL NOT NULL,
    weapon_power REAL NOT NULL,
    speed        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ships_fleet ON ships(fleet_id);

-- Sector modifications (overlay on procedural generation)
-- Only sectors that have been modified from their base state.
CREATE TABLE IF NOT EXISTS sectors_modified (
    q                   INTEGER NOT NULL,
    r                   INTEGER NOT NULL,
    resources_metal     REAL,
    resources_crystal   REAL,
    resources_deuterium REAL,
    last_cleared_tick   INTEGER,
    PRIMARY KEY (q, r)
);

-- Explored edges per player (for waypoint pathfinding)
-- Stores each traversed hex-to-hex edge.
-- Normalized: q1,r1 < q2,r2 by hex key ordering.
CREATE TABLE IF NOT EXISTS explored_edges (
    player_id      INTEGER NOT NULL REFERENCES players(id),
    q1             INTEGER NOT NULL,
    r1             INTEGER NOT NULL,
    q2             INTEGER NOT NULL,
    r2             INTEGER NOT NULL,
    discovered_tick INTEGER NOT NULL,
    PRIMARY KEY (player_id, q1, r1, q2, r2)
);
CREATE INDEX IF NOT EXISTS idx_explored_player ON explored_edges(player_id);

-- Fleet auto-action policies
CREATE TABLE IF NOT EXISTS auto_policies (
    player_id  INTEGER NOT NULL REFERENCES players(id),
    fleet_id   INTEGER NOT NULL,
    priority   INTEGER NOT NULL,
    condition  TEXT NOT NULL,
    action     TEXT NOT NULL,
    PRIMARY KEY (player_id, fleet_id, priority)
);
```

---

## 7. External Access

Because SQLite with WAL mode supports concurrent readers, external tools can open the database file read-only without affecting the game server:

- **Admin dashboard:** read player stats, fleet positions, economy overview.
- **Backup script:** `sqlite3 iac_world.db ".backup backup.db"` — safe while server is running.
- **Analytics:** query explored_edges to visualize how far players have pushed into the wandering.
- **Debug tools:** inspect sector_overrides to see resource depletion patterns.

External tools should open the database in **read-only mode** (`SQLITE_OPEN_READONLY`) and should not attempt writes.

---

## 8. Failure Modes

| Failure | Impact | Recovery |
|---|---|---|
| Server crash | Lose up to 30s of state (last dirty window) | Restart, load from SQLite, resume |
| Disk full | COMMIT fails, dirty state stays in memory | Alert, free space, next persist succeeds |
| Corrupt database | Server can't start | Restore from backup |
| Power failure during write | WAL may be incomplete | SQLite auto-recovers on next open (WAL replay) |
| Client disconnect mid-action | Command was either applied or not (atomic) | Client reconnects, gets full state sync |

### 8.1 Backup Strategy

Run periodic backups using SQLite's online backup API or the `.backup` command:

```bash
# Safe to run while server is live (WAL mode)
sqlite3 iac_world.db ".backup /backups/iac_$(date +%s).db"
```

Recommended: every 15 minutes, retain last 24 hours of backups.

---

## 9. Future Considerations

### 9.1 Scaling Beyond SQLite

SQLite comfortably handles this game up to ~10,000 concurrent players on a single server. If IAC ever needs to scale beyond that:

- **Sharding by region:** split the hex grid into shards, each with its own server + SQLite. Players near shard boundaries need cross-shard communication.
- **PostgreSQL migration:** the schema is simple enough to port. The in-memory-first architecture stays the same — just swap the persistence backend.

These are distant concerns. SQLite is the right choice for years of development and growth.

### 9.2 Event Sourcing (Optional)

An alternative architecture: instead of checkpointing state, log every command and event to an append-only table. State can be rebuilt by replaying the log. Benefits: perfect audit trail, replayable games, time-travel debugging. Cost: more complex, larger database, slower recovery. Not recommended for M1 but worth considering for later milestones if replay/spectating becomes a feature.
