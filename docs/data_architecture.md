# In Amber Clad — Data & Persistence Architecture

**Version:** 0.1.0
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
                          │         │   every tick, handed   │
                          │         │   to a writer thread:  │
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
8. End of every tick: persist_dirty_state()
   - Clone everything that changed this tick into a batch (section 5.2)
   - Clear dirty sets, send the batch to the persist thread (no disk I/O here)
   - The persist thread: BEGIN IMMEDIATE, write the batch(es), COMMIT
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
9. Load fleet_policies, known_sectors, explore_credits, runtime_state
       │
10. GameEngine::restore(snapshot); server ready, tick loop begins
       │
   (No further reads from SQLite during normal operation)
```

`Database::load_snapshot` builds an `iac_sim::Snapshot`, the same structure
`GameEngine::snapshot()` returns. After a clean shutdown the two are equal
(same `Snapshot::hash`), so a restart continues the world as if it had never
stopped (section 5.7).

### 2.3 Client Read Path (Memory → WebSocket → Client)

```
1. Client connects, sends auth with player_name
       │
2. Server checks for existing player by name:
   - Found → reconnect (return existing player ID + state)
   - Not found → create new player, fleet, homeworld
       │
3. Server sends full_state message (serialized from memory)
       │
4. Each tick, server sends tick_update with deltas
   (only changed fields, only entities relevant to this player)
       │
5. Client maintains its own local state mirror (client/state.zig)
   updated by applying server messages — all slice data
   (ships, player name, buildings) is deep-copied into
   client-owned allocations
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
PRAGMA busy_timeout = 1000;         -- 1s wait on lock contention (only foreign tools contend)
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
- The Persister forces a checkpoint every 30 s, so a power failure loses at most ~30 s of state.
- `FULL` was tried on the dev box and rejected: its per-commit fsync stalled for 8-16 s at times, during which the writer fell behind and unsaved ticks piled up in memory, widening the process-crash window instead of narrowing it.

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
| Fleet state, position, cargo, jump target, cooldowns, ship order | In-memory, checkpointed | Yes, in `fleets` + `ships` |
| Standing orders, their next evaluation and last reported hold | In-memory, checkpointed | Yes, in `fleet_policies` |
| NPC groups not yet met | Sector templates | No — generated from the seed |
| NPC groups that have materialised (damage, position, patrol timer) | In-memory, checkpointed | Yes, in `runtime_state` |
| Active combats (sides, round, peak power) | In-memory, checkpointed | Yes, in `runtime_state` |
| Raid clocks and announced raids, scan reveals, storage and salvage alert marks, harvest tallies | In-memory, checkpointed | Yes, in `runtime_state` |
| Built-in bots: which accounts they are, personality and attention, and what each remembers between turns (the end of its first look-in, next wake and plan ticks, its own leaderboard row as last asked, the routes its fleets are walking) | In-memory, written when a bot's turn changes it | Yes, in `runtime_state` (kind `bot`, id = player id); the account itself is an ordinary `players` row |
| Outposts: sector, level, store, the build or repair under way, breached, its own raid clock and announced raid, and which warnings its owner has been given | In-memory, written every tick it changes (the extractor changes it every tick it runs) | Yes, in `runtime_state` (kind `outpost`, id = the owner's player id). A row of this kind that cannot be read refuses the world: it holds a player's store |
| A bot's view of the world (`script::Seat`) | In-memory only | No — rebuilt from the full state at its next turn, as for a client that reconnects |
| Build/research queues | In-memory, checkpointed | Yes, in `buildings` + `research` + `build_queue` |
| Events not yet broadcast | In-memory only | No — drained to clients before every persist |

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

Dirty state is snapshotted **every tick** and written by a dedicated persist thread (`Persister` in `database.rs`). The tick path never touches SQLite: even a multi-second fsync stall on the disk only delays the writer, which coalesces whatever queued up into one transaction. (The Zig-era design wrote every 30 ticks on the tick thread; measured on the Rust port, per-arrival writes alone pushed ticks past 1 s and caused multi-second gaps.)

What a crash can lose:
- **Process crash / panic / SIGKILL**: the last tick, plus anything still queued in the persist thread. WAL commits are in the OS page cache and survive the process, so normally that is at most one tick. If the disk is stalled at that moment the queue (in memory) is lost with it.
- **OS crash / power loss**: with `synchronous=NORMAL` commits are not fsynced individually; the writer forces a WAL checkpoint (fsync) every 30 s, so the window is up to ~30 s. The database file is never corrupted.
- New registrations (and their auth tokens) are part of the next batch: a crash in that window forgets the account, and the name is simply free to register again.
- Two things are written only every 60 ticks while nothing else about them changes, so a kill can roll them back by up to a minute: the `last_seen` stamp and ore level of sectors a player is watching (re-observed on the first tick after the restart) and the part-refilled ore of a regenerating tile. A clean shutdown writes both (section 5.5).

### 5.2 Batch Write Pattern

See `GameEngine::persist_dirty_state` and `Database::apply_batch`. The pattern:

```
BEGIN IMMEDIATE
  save server_state (current_tick, next_id)
  for each dirty player: INSERT OR REPLACE into players (+ buildings, research, queues, defences)
  for each dirty fleet:  INSERT OR REPLACE into fleets + DELETE/INSERT ships
  for each fleet whose counters ran and nothing else: UPDATE fleets SET the three counters
  for each dirty policy / sector: INSERT OR REPLACE
  for each touched runtime entry: INSERT OR REPLACE its JSON, or DELETE once it is gone
  new explored edges, chart rows, explore credits
COMMIT
  clear dirty sets
```

On error the transaction is rolled back and the batches are kept and retried in order (every upsert is a whole-row replace, so replay is idempotent).

`runtime_state (kind, id, data)` holds the engine's bookkeeping maps one entry per row, as the JSON `Snapshot` uses for them (`iac_sim::persist::RuntimeEntry`): live NPC groups, combats, raid clocks, scan reveals, alert marks, harvest tallies, built-in bots. An entry is written when the engine touches it and its row is deleted when the entry is; a new map needs a `RuntimeKind` and the touch calls, not a table. A row that no longer parses is skipped on load, which starts that one entry over.

Counters that run every tick (`move_cooldown`, `action_cooldown`, `idle_ticks`) are short-lived: jumps, cooldowns and boardings last at most 20 ticks and `idle_ticks` stops at the 10 the shield rule needs, so a fleet at rest costs no writes.

World seed is persisted once at init, not on every batch. "Has this player entered this sector" is answered from an in-memory set (loaded from `explored_edges` at startup); new explored edges ride along in the batch.

### 5.3 Why BEGIN IMMEDIATE

`BEGIN IMMEDIATE` acquires the write lock at the start of the transaction. Alternatives:

- `BEGIN` (deferred): acquires the lock lazily on first write. If another connection tries to write between BEGIN and the first write, you get `SQLITE_BUSY`. Shouldn't happen in our single-writer model, but IMMEDIATE is defensive.
- `BEGIN EXCLUSIVE`: like IMMEDIATE but also blocks readers. We don't want this — WAL mode readers should be unaffected by our writes.

### 5.4 Prepared Statements

Currently, `database.zig` prepares statements per-call (prepare, bind, step, deinit). This is correct and simple. When profiling shows statement preparation as a bottleneck, we can cache prepared statements on the Database struct and reuse them with `reset()` between calls. Not needed for M1 scale.

### 5.5 Graceful Shutdown

On server shutdown signal (SIGTERM, SIGINT):

1. Signal handler sets an atomic `shutdown_requested` bool.
2. Tick loop checks the bool each iteration and exits.
3. `checkpoint()` queues what is otherwise written only every 60 ticks (live chart stamps, refilling ore), then one final `persist_dirty_state()`; wait for the persist thread to commit it (`flush_persistence`).
4. Dropping the engine joins the persist thread.

This ensures zero data loss on clean shutdown: the database then holds exactly `GameEngine::snapshot()`.

Future: broadcast "server shutting down" to connected clients before exit (requires protocol support).

### 5.6 Memory Ownership

JSON messages parsed via `std.json.parseFromSlice` produce a `Parsed(T)` that owns an
arena allocator. Slice fields in the parsed value (strings, arrays) point into this arena.

**Server (network.zig):** The `Parsed(ClientMessage)` is stored alongside the queued message
and freed after `handleMessage` processes it. Any data that must outlive the parse
(e.g., player name in `registerPlayer`) is duped into the engine's allocator before the
arena is freed.

**Client (connection.zig / state.zig):** `poll()` returns the full `Parsed(ServerMessage)`.
The caller frees it with `defer p.deinit()` after `applyServerMessage`. Inside
`applyServerMessage`, all slice data stored in `ClientState` (fleet ships, player name,
homeworld buildings/docked_ships, sector connections) is deep-copied into client-owned
allocations via `replace*` helpers. Old copies are freed on replacement and in `deinit`.

**Engine (engine.zig):** Player names are duped via `allocator.dupe(u8, name)` in both
`registerPlayer` and `loadPlayers`. `deinit` iterates the players map and frees each
name before deiniting the map itself.

### 5.7 What a Restart Restores

Everything. The rule (CLAUDE.md): state that must survive a restart goes in
`PersistBatch` and the schema as well as in `Snapshot`, and every mutation of
it marks its row dirty in the same place.

| Stop | The database holds |
|---|---|
| SIGINT / SIGTERM | `GameEngine::snapshot()` exactly: same `Snapshot::hash`, so the world continues tick for tick as it would have |
| SIGKILL, panic | the world as of the last committed tick, less the two 60-tick items of section 5.1 |

Stored floats are the value times 1000 in a REAL column; an f32 times 1000 is
exact in an f64, so they reload bit for bit. `ships.slot` keeps the order a
fleet holds its ships in, which combat rolls against.

`server/src/database.rs` checks this in process: a scripted world runs against
SQLite beside the same world run in memory, is compared with the database every
25 ticks, and is stopped and reloaded more than ten times (mid-fight, mid-jump,
mid-boarding, with a scan live and a raid announced); the hashes must match at
every restart and at the end. `server/tests/scenarios.rs` kills the real server
during a boarding and reads the result over the wire.

A database written before a column or table existed loads with that state at
rest: cooldowns zero, no raid clock, no combat. `ensure_schema` idles fleets
such a database left `moving` or `in_combat`, once, when it adds the columns.

---

## 6. Schema

Full schema with indexes and constraints. This is the authoritative version (supersedes SPEC.md 11 if they diverge).

### 6.1 M1 Schema (implemented)

```sql
CREATE TABLE IF NOT EXISTS server_state (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- Stores: current_tick, next_id, world_seed

CREATE TABLE IF NOT EXISTS players (
    id           INTEGER PRIMARY KEY,
    name         TEXT UNIQUE NOT NULL,
    homeworld_q  INTEGER NOT NULL,
    homeworld_r  INTEGER NOT NULL,
    metal        REAL DEFAULT 500,
    crystal      REAL DEFAULT 300,
    deuterium    REAL DEFAULT 100,
    token_hash   BLOB          -- SHA-256 of the account token; NULL = unclaimed (added by migration)
);
-- name has implicit index via UNIQUE constraint

CREATE TABLE IF NOT EXISTS fleets (
    id              INTEGER PRIMARY KEY,
    player_id       INTEGER REFERENCES players(id),
    q               INTEGER NOT NULL,
    r               INTEGER NOT NULL,
    state           TEXT DEFAULT 'idle',
    fuel            REAL NOT NULL,
    fuel_max        REAL NOT NULL,
    cargo_metal     REAL DEFAULT 0,
    cargo_crystal   REAL DEFAULT 0,
    cargo_deuterium REAL DEFAULT 0,
    harvest_resource TEXT DEFAULT 'auto',  -- what the current harvest order mines (added by migration)
    move_cooldown   INTEGER NOT NULL DEFAULT 0,   -- ticks left of the jump under way (added by migration, as the next four)
    action_cooldown INTEGER NOT NULL DEFAULT 0,   -- ticks until the next order; the timer of a boarding
    idle_ticks      INTEGER NOT NULL DEFAULT 0,   -- ticks at rest, up to the 10 that start shield repair
    move_q          INTEGER,                      -- jump target, NULL when not jumping
    move_r          INTEGER
);
CREATE INDEX IF NOT EXISTS idx_fleets_player ON fleets(player_id);
CREATE INDEX IF NOT EXISTS idx_fleets_location ON fleets(q, r);

CREATE TABLE IF NOT EXISTS ships (
    id           INTEGER PRIMARY KEY,
    fleet_id     INTEGER REFERENCES fleets(id),
    player_id    INTEGER REFERENCES players(id),
    class        TEXT NOT NULL,
    hull         REAL NOT NULL,
    hull_max     REAL NOT NULL,
    shield       REAL NOT NULL,
    shield_max   REAL NOT NULL,
    weapon_power REAL NOT NULL,
    speed        INTEGER NOT NULL,
    slot         INTEGER NOT NULL DEFAULT 0   -- position in the fleet (added by migration)
);
CREATE INDEX IF NOT EXISTS idx_ships_fleet ON ships(fleet_id);

CREATE TABLE IF NOT EXISTS sectors_modified (
    q              INTEGER,
    r              INTEGER,
    metal_density  INTEGER,
    crystal_density INTEGER,
    deut_density   INTEGER,
    PRIMARY KEY (q, r)
);

CREATE TABLE IF NOT EXISTS explored_edges (
    player_id      INTEGER REFERENCES players(id),
    q1             INTEGER NOT NULL,
    r1             INTEGER NOT NULL,
    q2             INTEGER NOT NULL,
    r2             INTEGER NOT NULL,
    discovered_tick INTEGER NOT NULL,
    PRIMARY KEY (player_id, q1, r1, q2, r2)
);
CREATE INDEX IF NOT EXISTS idx_explored_player ON explored_edges(player_id);

CREATE TABLE IF NOT EXISTS runtime_state (
    kind TEXT NOT NULL,      -- npc_fleet, combat, raid, scans, storage_marks, home_salvage_warned, harvest_report
    id   INTEGER NOT NULL,   -- the NPC group, combat, player or fleet the entry belongs to
    data TEXT NOT NULL,      -- the entry as JSON (iac_sim::persist::RuntimeEntry)
    PRIMARY KEY (kind, id)
) WITHOUT ROWID;
```

The tables added since M1 (`world_meta`, `buildings`, `research`, `build_queue`,
`player_defences`, `fleet_policies`, `known_sectors`, `explore_credits`) and the
later columns are in `Database::ensure_schema`, which is what runs.

### 6.2 Future Schema (not yet implemented)

These tables and columns will be added in later milestones:

- `players.token` -- authentication token (multiplayer)
- `players.created_at` -- account creation timestamp
- `buildings` table -- per-player building levels and queues
- `research` table -- per-player tech levels and queues
- `auto_policies` table -- fleet auto-action rule tables
- `ships` ON DELETE CASCADE -- requires table recreation (ships are manually deleted before re-insert for now)

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
| Server crash | Lose the last tick or so (section 5.1) | Restart, load from SQLite, resume where it stopped (section 5.7) |
| Disk full | COMMIT fails, dirty state stays in memory | Alert, free space, next persist succeeds |
| Corrupt database | Server can't start | Restore from backup |
| Power failure during write | WAL may be incomplete | SQLite auto-recovers on next open (WAL replay) |
| Client disconnect mid-action | Command was either applied or not (atomic) | Client reconnects by name, gets full state sync |

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
