# IAC Port Journal

## Session 1 — 2026-06-30

### Phase 0: Ingestion & iac-shared fixes (completed by prior agent)
- [x] Ingested all Zig sources (server: engine.zig, network.zig, database.zig, combat.zig, main.zig; client: renderer.zig, state.zig, input.zig, connection.zig, main.zig)
- [x] Fixed iac-shared compilation errors (Serialize/Deserialize on Hex, u64 types in constants, Hasher import in world)
- [x] Added `COUNT` constants to `BuildingType` and `ResearchType` in scaling.rs
- [x] `cargo check --package iac-shared` passes clean

### Phase 1: Port iac-server (COMPILED — 2026-07-02)
- [x] engine.rs — GameEngine struct, tick loop, movement, combat, harvesting, NPC behavior, homeworlds, build queues, command handlers, persistence
- [x] network.rs — WebSocket server, session management, message routing, state broadcasting
- [x] database.rs — SQLite schema, CRUD operations, fleet/ship loading
- [x] combat.rs — Round-based combat resolution with RNG, target selection, damage calculation
- [x] main.rs — CLI, signal handlers, tick loop with persistence
- [x] `cargo check --workspace` passes clean (3 dead-code warnings only)

### Key refactoring decisions:
- **Combat resolution**: Rewrote to use copied `CombatShip` data instead of mutable references, avoiding borrow checker issues with simultaneous fleet access
- **Method signatures**: Changed `check_homeworld_docking`, `dock_fleet`, `check_npc_encounter`, `collect_salvage`, `start_combat`, `add_ship_to_homeworld`, `recalculate_player_fleet_fuel` to take IDs instead of `&mut` references
- **Two-phase processing**: `process_movement`, `process_combat`, `process_harvesting`, `process_build_queues` all use collect-then-apply pattern to avoid double mutable borrows
- **tokio-tungstenite 0.26**: Replaced `.split()` (removed in 0.26) with `Arc<TokioMutex<WebSocketStream>>` pattern for concurrent read/write
- **rand 0.9**: Updated `gen_range` → `random_range` API
- **rusqlite**: Fixed `query_row_optional` → `query_row().optional()` pattern, added explicit type annotations to `row.get()` calls

### Phase 2: Port iac-client (COMPILED — 2026-07-02)
- [x] state.rs — ClientState, EventLog (ring buffer), View, ZoomLevel, ScrollDirection, HomeworldTab, HomeworldNav
- [x] connection.rs — Async WebSocket client with tokio-tungstenite, mpsc channel bridge for reader/writer tasks
- [x] input.rs — crossterm key mapping to InputAction union
- [x] renderer/mod.rs — View dispatch, amber color palette, shared helpers (titled_block)
- [x] renderer/command_center.rs — Resource panel, fleet panel, event log
- [x] renderer/windshield.rs — Sector view with node graph, sector info, fleet status
- [x] renderer/star_map.rs — Hex grid rendering
- [x] renderer/homeworld.rs — Tab bar, card grid, tech tree, status bar
- [x] main.rs — Tokio runtime, TUI loop, channels, CLI (clap)
- [x] `cargo build --workspace` links clean (warnings only)

### Key refactoring decisions (client):
- **Renderer modularization**: Split 1310-line Zig `renderer.zig` into 6 Rust files under `renderer/`
- **Async architecture**: WebSocket reader task + TUI event loop on main thread; communication via mpsc channels
- **tokio-tungstenite 0.26**: WebSocket split via `ws_stream.split()` → `SplitSink`/`SplitStream`, bridged to mpsc channels for the TUI thread
- **Block rendering pattern**: Call `block.inner(area)` BEFORE `frame.render_widget(&block, area)` to avoid move-after-borrow
- **Connection struct**: `rx: Mutex<mpsc::UnboundedReceiver<String>>` (raw text strings), parsed via `parse_server_message()`

### Phase 3: Verification & fidelity audit (COMPLETED — 2026-07-03)

Decision: **stay on Rust.** The Zig repo (on the `archive/zig` branch) remains the reference spec
but is no longer developed. The Rust wire protocol (serde internally-tagged
enums, PascalCase variants) intentionally differs from Zig std.json's
externally-tagged snake_case format — both server and client share
`iac-shared`, so it's self-consistent; Zig↔Rust interop is a non-goal.

Full four-way fidelity audit against the Zig source found and fixed:

**iac-shared**
- [x] `hub_fully_connected` test: Wyhash→Xxh3 swap broke the hash-luck the Zig
      test relied on; hub edges now explicitly always survive (world.rs)
- [x] `BuildingLevels::default()` now starts metal/crystal mines at level 1
      (was all-zero → new players had no production)
- [x] `BuildShip.count` gets `#[serde(default = 1)]` like the Zig protocol

**iac-server**
- [x] combat.rs: rewrote to Zig's sequential semantics — rapid-fire now grants
      chained extra shots (was: rolled but ignored); players fire first and
      damage lands immediately so NPCs killed this round don't return fire;
      target selection skips dead ships; no duplicate ShipDestroyed events;
      NPC FleetDestroyed events carry real salvage value (was zero)
- [x] engine.rs: harvesting collects ALL three resources per tick (was: first
      non-None only); fleets with nothing harvestable go Idle instead of
      spinning in Harvesting forever
- [x] engine.rs: `persist_dirty_state` wraps writes in BEGIN IMMEDIATE/COMMIT
      and clears dirty sets on success (was: unbounded dirty-set growth,
      re-issued deletes forever, partial-write risk)
- [x] engine.rs: move to unconnected sector returns NoConnection (was InvalidTarget)
- [x] engine.rs: removed nonsense IntoResearchLevels shim
- [x] network.rs: reader/writer use split sink/stream (was: shared mutex —
      writer starved up to 1s per message by the reader's timeout window);
      failed auth sends `auth_result{success:false}` (was: silent);
      incoming queue fully drained per tick (was: 100-message cap);
      `player_id.unwrap()` panic paths removed from tick loop
- [x] main.rs: SIGTERM handled like SIGINT (final persist on docker stop)

**iac-client**
- [x] input.rs: SHIFT no longer swallowed — `?` (keybinds) and `Shift+X`
      (cancel ship queue) work; `t` closes the tech tree
- [x] state.rs: no phantom "came from" marker on first fleet snapshot;
      homeworld cursor-up matches Zig at index 1
- [x] renderer: research cards show MAX state; windshield direction nodes
      render neighbor coordinates and explored-sector styling (both dropped
      in the port); status labels match Zig ("in_combat", "warning");
      titled_block no longer double-pads
- [x] main.rs: panic hook restores the terminal (raw mode/alt screen)

**Testing**
- [x] `cargo test --workspace` — 12 lib tests green
- [x] NEW `iac-server/tests/smoke.rs` — end-to-end: spawns the real server
      binary, WebSocket auth → full_state → tick_update stream → scan command
      → reconnect resumes the same player. 2 tests green.
- [x] clippy cleanup pass

### Key mappings
| Zig | Rust |
|-----|------|
| `std.AutoHashMap` | `std::collections::HashMap` |
| `std.ArrayList` | `Vec` |
| `std.mem.Allocator` | Rust ownership / `Box` |
| `zqlite` | `rusqlite` |
| `webzocket` | `tokio-tungstenite` |
| `zithril` | `ratatui` + `crossterm` |

### Architecture notes
- Server: `GameEngine` manages players/fleets/NPCs/combat with tick-based processing. `Network` uses tokio-tungstenite WebSocket server with mutex-protected message queue. `Database` uses rusqlite for SQLite persistence.
- Client: `ClientState` holds game state and UI state. `renderer` dispatches to view-specific render functions. `input` maps keys to `InputAction`. `connection` wraps tokio-tungstenite client.

## Session — 2026-07-03: Gameplay expansion (scan, raids, headless, TUI polish)

Design consult run through Codex (`codex exec`): prioritized headless mode as
foundational for LLM playtesting, insisted raids be "forecasted pressure with
upside, never a random tax," contributed the signal-pings idea and raid
tuning parameters.

**New mechanics**
- [x] Active Scan (`scan {fleet_id}`): BFS reveal from the fleet (2 hops with
      a Scout aboard, 1 without), live intel for SCAN_REVEAL_TICKS, plus
      *signal pings* one hop beyond range (RichOre / HostileMass / Derelict /
      Anomaly) — vague contacts that give humans and agents short-term goals.
      New `ScanCompleted` event. Cooldown = SCAN_COOLDOWN (finally used).
- [x] Stop (`stop {fleet_id}`): aborts a move (fuel forfeit) or harvest.
- [x] Homeworld raids + DefenseGrid effect: rolled every 300t at 20% once the
      empire is worth raiding (Shipyard≥2 or grid≥1, age≥600t), announced 60t
      ahead (`RaidIncoming`), sized to 35–55% of current defense, resolved
      against docked ships + grid (virtual scout-units, +35%/lvl past 3) with
      ±10% variance. Repel → salvage in orbit; loss → capped 8/8/5% resource
      skim, 1800t suppression. Never touches buildings or queues.

**Headless / LLM play**
- [x] `iac-client --headless`: NDJSON pipe — server messages on stdout,
      commands (full or bare `{"action":...}`) on stdin, in-band
      `client_error` for garbage, `connection_closed` marker, auto-auth with
      agent token. Verified live: scan → harvest → stop bot via jq.
- [x] README rewritten: quick start, headless protocol, command table.

**Bugs found & fixed**
- [x] Command-handler events were silently wiped every tick: run_tick
      processes commands *before* engine.tick(), whose pending_events.clear()
      discarded CombatStarted/SalvageCollected/recall events before
      broadcast. Removed the clear (drain_events already empties).
- [x] Server could never restart with saved players: values stored ×1000 as
      i64 into REAL-affinity columns come back as floats; all i64 reads
      panicked with InvalidColumnType. Reads now f64. Regression test boots
      engine twice on the same DB.
- [x] Fleet commands never checked ownership — any player could move any
      fleet by ID. All fleet handlers now go through owned_fleet(pid, fid).

**TUI UX & aesthetics**
- [x] Semantic accents over the amber identity: red = danger (hostiles,
      combat, raids, errors), green = gain (harvest, salvage, affordable
      costs, rich sectors), cyan = intel (scan, shields).
- [x] Event log colored per event kind (all views).
- [x] Target selection: [n] cycles hostiles ("»" marker), [a] attacks it.
- [x] Batch ship builds: [+/-] set count, card shows total cost, status bar
      shows batch.
- [x] Server errors now surface: red footer takeover for 8 ticks.
- [x] Raid countdown flashes red in the header in every view.
- [x] Fleet panel: status + cooldown line, hull goes red under 50%.
- [x] Star map: hostiles red "!", salvage green "$", rich sectors green.
- [x] Keybind overlay + footers updated ([v] scan, [S] stop, [n] target).

**Testing**: 25 tests green (12 shared, 8 engine unit incl. raids/scan/
restart, 2 smoke incl. real scan roundtrip, 3 headless parse), clippy clean
across the workspace.

## Session — 2026-07-04: Derelict sites + standing orders (fleet autopilot)

Codex consult picked the two features (verbatim: "Derelicts plus standing
orders immediately make scanning meaningful and make the LLM/headless angle
feel like a real automation game") and contributed tier tuning, the
risk-read idea, and the PolicyAction `reason` audit-trail requirement.
Codex also ran a post-implementation correctness review.

**Derelict sites** — content behind the "derelict transponder" ping
- [x] Worldgen: ~8% of DebrisField/Empty sectors at dist≥4 carry a hulk
      (deterministic, rolled after the existing template rolls so old
      worlds keep identical terrain/NPCs). Tier 1/2/3 by distance.
- [x] `explore_site {fleet_id}`: 20-tick boarding (FleetStatus::Exploring),
      resolved against tiered odds — ambush (10/20/30% base) spawns
      aggressive guardians and starts combat; success rolls tiered loot
      (cargo-capped, overflow drops as sector salvage), 15/25% chance of a
      recoverable battered ship, 10% tier-3 data core = instant +1 level in
      a random low tech. Sites are consumed on success; failed/aborted
      boardings bump the site's ambush odds +10% each ("angrier").
      Scout aboard −10% ambush; Hauler ×2 loot.
- [x] SectorState.site {tier, risk quiet/uneasy/hot}; SiteExplorationStarted
      / SiteExplored / SiteAmbush events; scan signal now fires for real
      unlooted sites, not just salvage. site_looted_tick/ambush_bumps
      persisted (ALTER TABLE migration via ensure_column).

**Standing orders** — the dead PolicyUpdate stub is now a typed autopilot
- [x] `{"type":"policy_update", fleet_id, preset, params?}` with presets
      manual/prospect/mine_and_return/salvage_and_sites/patrol_home and
      params {min_fuel_pct, cargo_return_pct, max_range, engage_ratio_x10}.
- [x] Engine evaluates idle/docked fleets every 3 ticks: Prospect scans
      when its sector's pings are stale then pushes the frontier (avoids
      hostiles, ranges from home); MineAndReturn shuttles claim↔home and
      re-sites itself when the seam runs dry; SalvageAndSites scoops
      wreckage, boards survivable derelicts, BFS-hunts the next payday;
      PatrolHome engages beatable hostiles within 2 hops of home and
      goes home to lick wounds under 60% hull.
- [x] Every act emits PolicyAction {action, reason} — legible audit trail
      for LLM agents. FleetState.policy shows the doctrine.
- [x] Persistence: fleet_policies table written inside the same dirty-set
      transaction as fleets (FK ordering), survives restart; orders die
      with merged/disbanded fleets.
- [x] Verified live over headless NDJSON: prospect scan→move→rescan loop,
      and salvage_and_sites navigating 5 hops to a scanned derelict,
      boarding it, cargo-capping the loot, docking to deposit, then
      returning for the overflow salvage. Autonomy porn.

**Bugs found & fixed**
- [x] handle_scan never recorded the *origin* sector as revealed — the
      prospect autopilot saw its own position as "unscanned" forever and
      scanned in place for eternity. Origin now joins the reveal set.
- [x] fleet_policies FK failed on first save: policies were written
      immediately while fleets only persist on the dirty-set flush.
      Policies now ride the same transaction, after fleets.

**TUI**
- [x] Windshield: DERELICT HULK panel (tier + color-coded risk + [x] board),
      BOARDING status in cyan, AUTO:<doctrine> on the fleet card.
- [x] Star map: cyan `D` marker, legend updated.
- [x] Event log: boarding/loot/ambush lines (ambush flashes red), auto-*
      policy lines with reasons; keybind overlay + footers ([x] board,
      [p] cycle orders).
- [x] Command center fleet list shows AUTO tag.

**Testing**: 31 tests green (12 shared, 14 engine incl. 6 new for
sites/policies/restart-persistence, 2 smoke, 3 headless), clippy clean.

**Codex post-review fixes** (codex exec found 6, 3 accepted)
- [x] policy_step_toward was greedy hex-distance descent on a sparse graph
      — could bounce off missing edges forever. Now BFS-with-predecessors
      (first_hop_toward, 16-hop budget), greedy only as a fallback.
- [x] Wire PolicyParams now clamped to the same bounds the DB reload
      enforces (behavior no longer changes across a restart; engage_ratio 0
      can't make patrols suicide into anything).
- [x] collect_salvage no longer vaporizes the uncollected remainder when a
      hold fills — leftovers stay in the sector for another trip.
- [x] (own find) MineAndReturn engaged while docked at home now stakes a
      claim at the nearest seam instead of idling forever.
      Declined: salvage persistence across restarts (deliberately
      ephemeral, 60t despawn), next_eval_tick persistence (harmless
      one-tick re-eval).

## Session — 2026-07-04 (later): Star map overhaul — the tactical plot

Map design review (codex consult concurred with the same ranking): the old
map was a scatterplot — one char per hex, no connection edges (the actual
movement topology was invisible!), no way to inspect or navigate from the
map, and live intel looked identical to stale memory.

**Tactical plot (Close zoom)**
- [x] Loose packing x=4dq+2dr, y=2dr with lanes rendered between known
      sectors: `───` east-west, `╱ ╲` diagonals. Chokepoints, dead ends
      and missing edges are now *visible*. Lanes into fog render faint.
      Sector/Region zooms keep the tight packing as strategic silhouettes.
- [x] View titles per zoom: TACTICAL PLOT / STAR MAP / REGION CHART.

**Crosshair + course plotting**
- [x] Fixed crosshair at screen center (inverse video), chart scrolls
      beneath it — plotting-table feel. Footer inspect strip reads the
      hex under the crosshair: freshness (LIVE/CHART/UNCHARTED), terrain,
      densities, hostiles, salvage, derelict tier+risk, exit count; signal
      pings show kind + age.
- [x] [Enter] plots a course: client-side BFS over *known* connections
      (final hop may exit into fog), route lights up amber on the map,
      hops auto-dispatch as the fleet idles (one move per tick).
      [Backspace] clears; any manual move/stop/recall or server error
      aborts the course. "NO KNOWN ROUTE" when the chart doesn't connect.
- [x] Verified live in tmux: plotted 3 hops to a hostile-mass signal ping,
      fleet flew it, ping resolved into hard intel on arrival.

**Intel freshness & phosphor**
- [x] sector_seen tick per sector: live-streamed sectors render bright,
      remembered charts fade to dim variants (new GREEN_DIM/CYAN_DIM);
      explored-empty sectors got the same treatment (bright when live).
- [x] Signal pings from scans persist on the chart for 120t as blinking
      glyphs in the fog: % ore, ! hostile, D derelict, ? anomaly — pruned
      when the sector streams real data.
- [x] Scan ripple: an expanding cyan ring for 4 ticks after each scan.

**Port bug found by the new map**: hex_ring stepped side-first, emitting a
skewed ring — cells drifted off-ring and the true west-side cells were
NEVER visited (the old count-only tests passed by luck). Every hex west of
a fleet was invisible on the map since the port. Fixed to emit-then-step
with geometry regression tests (cells on-ring, unique, disc coverage).

**Testing**: 34 tests green (15 shared incl. 3 new hex-geometry, 14 engine,
2 smoke, 3 headless), clippy clean, live tmux verification of plot + course.

## Session — 2026-10-08: Server fixes (fleet list, harvest, caps, tick stalls, auth)

Branch `server-fixes`, one commit per area, protocol changes migrated across
server, TUI/headless, Flutter and fixtures each time.

**Fleet list is authoritative.** `tick_update.fleet_updates` (omitted when
empty) became a required `fleets` list sent whole every tick. Combat left
wiped-out fleets in the engine with zero ships and they were still being
listed (the Flutter FleetDestroyed workaround hid that); they are now never
listed and are reaped one tick later (the owner's FleetDestroyed event still
needs the fleet to route). The list is sorted by id so clients keep a stable
selection.

**Harvest honours the resource.** Fleets carry `harvest_target`
(persisted: `fleets.harvest_resource`). Named resource mines only that one;
a resource the sector lacks is `ResourceNotPresent` (1016). Command errors
now explain themselves ("this sector has no deuterium to harvest").

**Caps and rates on the wire.** `FleetState.cargo_capacity` and
`HomeworldState.production` come from the same engine functions that apply
them. No homeworld storage caps exist in the game, so none are sent.

**Tick stalls.** Cause: per-arrival explored-edge INSERTs (autocommit, one
fsync each, rollback-journal mode) inside `tick()` under the engine lock.
Measured sim phase p50 80 ms / max 3.8 s before; 0.03 ms / 0.3 ms after. The
dev disk's fsyncs sometimes take 8-16 s, which is what produced the 4-6 s
gaps. Fix: WAL + synchronous=NORMAL, a persist thread fed a snapshot every
tick, in-memory first-visit set, interval with MissedTickBehavior::Delay.
`synchronous=FULL` was tried and rejected (writer lagged 16 s behind, which
widens the process-crash window). Harness: raw-socket Python clients, one
measuring `tick_update` arrivals, others issuing random moves.

**Auth.** Server-issued 256-bit tokens, SHA-256 at rest, constant-time
compare; unclaimed legacy accounts are claimed by their next login.
`AuthResult.code` carries `InvalidName` / `TokenRequired` / `InvalidToken`.
TUI/headless keep tokens in `~/.config/iac/tokens.json`; the web client uses
localStorage behind try/catch and re-prompts on refusal. Not done: the
spec's rate limits and lockout (see `docs/auth_spec.md`, "Implemented subset").

**Known gaps noticed, not changed:** fleet move targets / cooldowns are not
persisted across restarts. (Sector salvage is now persisted, see below.)

## Events, salvage and chart memory (branch `fix-events-intel`)

From the 2026-10-09 six-agent playtest (`docs/playtests/2026-10-09/`).

**Event scoping.** Combat and loss events now name their `sector` and the
owning empires (`owner`, `None` = NPC) and the server computes `mine` per
recipient. A player receives them only if they own a fleet involved or have a
fleet or their homeworld in that sector, decided from the event alone (the old
router looked fleets up, which fails once a fleet is reaped). Other empires'
fights elsewhere are not summarised either: a one-line digest still leaks
where and when someone fought, and the complaint was noise, not a missing
feed. A destroyed fleet raises a `Critical` `Alert` for its owner;
`AlertEvent.player_id` routes alerts (they used to go to everyone). A fleet
that died mid-fight now reports at once instead of waiting for the fight to
end (it used to be reaped before the event was sent). `ResourceHarvested` is
one event per fleet per 10 ticks, flushed when harvesting stops.

**Salvage.** `FleetDestroyed.salvage` was the ship's full cost; the pile is
`SALVAGE_FRACTION` of it. The event now carries the share it dropped, the
pile stacks on an uncollected one instead of overwriting it, and
`CombatEnded` reports the pile and its despawn tick. `collect_salvage` with a
full hold is `CargoFull` (it used to succeed silently), a partial pickup
reports `remaining`, and `SalvageDespawned` tells players present when a pile
expires. Salvage and its despawn tick are persisted (new `sectors_modified`
columns). Amounts and lifetimes are unchanged.

**Hostile id 0.** A sector's NPC exists as a template until a fleet arrives
(passive ones never materialise on their own), and the sector view listed that
template with id 0, while `attack` ignored the id and `CombatStarted`
reported the real one. Template NPCs now get a stable id,
`TEMPLATE_NPC_ID_BASE + sector key`, used by the view, `attack` (which now
rejects ids that name nothing here) and `CombatStarted`. A template NPC whose
live fleet wandered off is no longer listed twice.

**Chart memory.** `server/src/intel.rs`. Each tick, after the simulation,
every sector a player can see (fleet present, sensor array, unexpired scan)
is stored per player with the tick it was last seen; persisted in
`known_sectors` as the observed `SectorState` JSON (rows that no longer parse
are skipped and relearned). `full_state.known_sectors` returns the whole
chart; `SectorState.live` / `last_seen` mark live versus remembered, and a
tick update repeats a sector once, flagged stale, when it stops being live.
Other empires' fleet positions are never remembered. Worlds from before this
change start with an empty chart (the old `explored_edges` have no
observations to restore).

**Known gaps noticed, not changed:** sector salvage and fleet move targets /
cooldowns are not persisted across restarts.

## 2026-10-09: fleets, autopilot and fuel (playtest fixes)

Branch `fix-fleets`. Findings from `docs/playtests/2026-10-09/`.

**Starting fuel.** `register_player` hard-coded a 50000 tank and only the
first dock recomputed it. The fleet now gets `fleet_fuel_max` at creation, and
`load_world` clamps stored tanks to the real maximum so old worlds heal.

**Autopilot range.** Re-claims searched from the previous claim, so
`max_range` was a per-step leash that walked outward. Every claim is now
chosen by BFS from home, a claim beyond `max_range` of home is re-staked
(orders given away from home), and salvage runs search from home too.
`prospect` walks to the nearest sector the player has never entered
(`engine.explored`) inside range; with none left it returns home and holds.
Holds go through `hold_policy`, which emits a `PolicyAction` and repeats the
same reason at most once a minute.

**Fuel safety.** The return trigger is `fuel < route_home_cost + reserve`
(`min_fuel_pct` of the tank is now a reserve on top of the route, default 10)
and `policy_move` refuses an outward jump that would leave less than the
route back. The route is the shortest path over sectors the player has
entered (`known_hops_home`), times the fleet's jump cost. Effective range is
also capped by what the tank can afford for the round trip. `FleetState` now
carries `jump_fuel` and `home_fuel` so every client shows the risk.
Manual jumps are never blocked; one that cannot be paid back raises an
`Alert` (warning, or critical when stranded on arrival).

**Stranding.** Design: an emergency reserve. An idle fleet away from home with
less than one jump regains `jump/300` fuel per tick (exactly one jump in 300
ticks from empty), with alerts at each quarter and when ready. No new state:
the reserve is ordinary fuel, so it persists and merges like fuel does.
Rejected alternatives: auto-teleport home (removes the risk), a free
refuel-anywhere (same), and a rescue-ship mechanic (needs a fleet in range and
a new order). Because the reserve is real fuel it can be spent in any
direction, but at five minutes per jump crawling outward is not a strategy,
and the autopilot only ever uses it to go home. A rescue is also possible by
`merge`: fuel pools, so a fresh fleet that reaches the stranded one tops it up.
`recall` stays all-or-nothing at 2x jump fuel per hex; its error now states
the cost and the walking alternative.

**Fleet composition.** Docking no longer merges. Added `split` (named ship ids
to a new fleet; fuel and cargo follow in proportion to the tank and hold taken)
and `merge` (same sector, both idle; pools ships, cargo, fuel; absorbed fleet's
orders end). Limits: 64 ships per fleet, 3 deployed, 8 in all. Splitting at
home is also how docked ships are launched, so there is no separate launch
command. Newly built ships join the lowest-numbered idle home fleet without
standing orders, else form a new fleet.

**Params.** `PolicyParams` is `#[serde(default)]`, each field optional. The
headless client parsed first as `ClientMessage` and then as `Command`, so a
bad policy field surfaced as the second parse's "missing field `action`"; it
now picks the shape from the `type`/`action` key and reports that parse's
error.

**Touched outside this scope (merge note):** `is_event_relevant` Alert arm
(now scoped by `fleet_id` when present), since the new alerts carry a fleet id.
