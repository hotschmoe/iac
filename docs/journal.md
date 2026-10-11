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

## Economy core: pace, formulas, storage, queues, fuel, defences, score (2026-10-09)

Implements steps 1 to 5, 9 and 10 of `docs/design/economy/spec.md` section 14
plus the docs and harness part of step 11, one commit per step. Threat and the
NPC gradient, ore and loot (steps 6 to 8) and the migration tool are left out
on purpose; see "Hooks" below.

**Pace** (`shared/src/pace.rs`). `Pace` with the four helper classes (economy
x1, attention x0.75, finds x0.5, real time), the presets of decisions.md
(persistent 1, fortnight 5, season 10, sprint 70, dev 100, blitz 600, test
8000) and their simulation estimates. The world's pace lives in the new
`world_meta` table beside `economy_version` 2 and `worldgen_version` 1. The
server takes `--pace <number|preset>` (or `--world <preset>`), keeps the stored
pace, refuses a different one, and refuses a database without `world_meta`
(an old world) with "start a new world". Every duration constant in
`constants.rs` must appear in `pace::TIMER_CLASSES`; a test scans the file and
fails when one is missing.

**Formulas** (`shared/src/scaling.rs`). Costs are `base x growth^(L-1)`
rounded to whole units with ties to even (`22.5` is `22`, as the spec's tables
print it); times are `(metal + crystal) / rate` hours per pace, with the
Fabricator, Research Lab and Shipyard as divisors; production is per game hour
times `L x 1.1^L` times P. Golden tests copy the spec's tables.
`tools/check_sim_constants.py` runs `cargo run -p iac-shared --example
economy_constants` and compares costs, times, production, prerequisites,
storage caps, defences, raid power, grid units and presets with `sim.py`. Run
it after touching any of them. The spec's vault table truncates costs
(`2839`) where the rest of its tables round (`2840` is right for L6); the code
rounds everywhere.

**Decisions that differ from the spec text.**
- Corvette Tech needs Shipyard 1 (decisions.md item 2); `sim.py` was changed
  to match.
- An order that would start at once must be affordable now, otherwise it is
  refused with `NoResources` naming the shortfall. An order behind a running
  item queues and waits, unpaid, with `waiting_for` showing what is missing.
  The spec is ambiguous about a free slot with an unaffordable order; always
  queueing would let a player queue and leave, which suits the slow worlds
  better. Open question for the owner.
- `QueueFull` now means "three items already"; there is no `QueueDepthReached`
  or `NoBuildSlot` code (a busy slot just queues).
- Storage `capped` and `full_in_s` sit inside `homeworld.storage`; `production`
  stays the mine output. The catalog has no `storage`/`slots` entries because
  the Storage Vault and Modular Fabrication already appear in it.
- `FleetState` keeps the existing `home_fuel` as the spec's `fuel_to_return`.
  `range_hops = floor((fuel - home_fuel) / (2 x jump_fuel))`.
- `cancel_queued` takes `queue_type` like `cancel_build` (the spec says
  `queue`); `cancel_build` gained `index`.
- Shipyard queue items carry `item` (a ship class or a defence name, untagged)
  instead of `ship_class`. `DefenceBuilt.defence` avoids clashing with the
  event tag `kind`.
- Aegis Dome, the season history table and exploration points are not built.
  Exploration points and chart-delivery points need the threat rating; the
  `explore` column exists, is persisted, and stays 0 until the derelict work
  adds to `Player::explore_points`.
- Emergency Jump follows the other leveled techs (x1.6 per level).
- A new hull adds only its own tank to a fleet at home; docking refuels at
  0.08 deuterium per unit and the rest follows as deuterium arrives.

**Score** (`server/src/score.rs`). econ + fleet + defence (half) as core,
combat and exploration capped at 25 percent of core. Kills credit every
participant, pay nothing when the force was 8x the NPC group's power, and
nothing twice in a sector until it respawns. The `agent` flag is declared at
login (`AuthRequest.agent`; the headless client sets it), stored once, shown on
the leaderboard, never used for scoring.

**Clients.** TUI: status bar shows caps, slots, queues, defences; `c/C/Z`
cancel waiting items, `l` shows the leaderboard overlay, defences are on the
Shipyard tab. Flutter: protocol and mapper migrated, the homeworld view shows
caps, queues with waiting items and slots, defences, the leaderboard; the demo
provider plays a blitz-pace world. The auth test fake now serves the golden
`full_state` so it cannot drift.

**Live check** (port 7812, `IAC_PACE=test`, two scripted players following the
simulation's ladder, buildings only, no piloting). Seconds since server start;
the spec's prediction is the blitz row divided by 13.3 (8000/600):

| milestone | spec (test) | measured |
|---|---|---|
| Lab 2 | 14 s | 24 s |
| Shipyard 4 | 86 s | 53 s |
| Corvette Tech / first Corvette | 41 s | 49 s / 73 s |
| building slot 2 (Modular Fabrication 1) | 122 s | 137 s |
| building slot 3 | n/a | 223 to 228 s |
| Cruiser Tech, first Cruiser | 240 s | 299 to 306 s, 300 to 307 s |
| 12 Cruisers | n/a | 412 to 428 s |
| endgame kit (buildings, research) | 720 s | not complete at 840 s (Metal 19, Crystal 16, Deuterium 13, Yard 11, Fab 8 at about 790 s; Lab 10 of 11) |

The bot has no piloting income and starves crystal (stock of metal piles up
unused), so a Cruiser 25 percent late and an endgame about 15 percent late
fits the simulation's own spread (buildings-only is 13 percent behind a
competent player). Raids arrived from 100 s, as the attention timers predict
(`RaidIncoming` 30 s before arrival), and were mostly repelled with turrets,
lancers and bastions; one Alpha raid (3711 against 3273, after the stockpile edit
below) was lost for 8 percent of its metal, the hard cap. A persistent world (x1) showed Metal Mine 2 in 9 minutes
(`576` ticks, the spec's pace-1 figure), and the 8000 world capped Bravo's
metal at 4229485 with `StorageFull`. A cap cannot be reached naturally in a
ten-minute run (caps grow as P^0.75, production as P); that part was driven by
editing the stockpile in the stopped world's database.

**Hooks for the threat, ore and loot work.**
- `protocol::MovePreview` reserves threat, ratio and label (see its doc);
  `GameEngine::preview_move` is where to fill them.
- `NpcFleet.bounty` and `.power` are set in `spawn_npc_fleet` from the ships
  actually spawned; the score reads them, so a new composition needs no score
  change.
- `Player::explore_points` (persisted, shown as `explore`) is the place for
  first-boarding and charting points; `score::EXTRA_CAP` already applies.
- `scaling::resource_weight` prices loot for score; the finds multiplier is
  `Pace::finds_mult` (applied today to NPC salvage and derelict loot).
- `world_meta.worldgen_version` is 1; bump `WORLDGEN_VERSION` when generation
  changes.

## Exploration economy: threat, ore, loot, exploration score (2026-10-09)

Implements steps 6 to 8 of `docs/design/economy/spec.md` section 14 plus the
exploration points of section 9, one commit per step (branch `world-explore`).
The worldgen version is 2 and a world from another generator is refused at
startup like an old economy (`WorldError::OldWorldgen`).

**Threat.** `npc_power(d) = 4 x 1.22^(d-1)`; composition per the spec (class by
ring, stat multiplier `min(1.3, 0.6 + 0.02 d)`, count rounded to hit the power,
presence `25 + 2.5 d` percent, passive share, respawn `2 + 0.25 d` game hours
over P, which replaced `NPC_RESPAWN_*`). `threat_rating` is T1..T9. Every
`SectorState` has `threat {rating, est_power, basis}`: `observed` (a group is
there or its template is about to spawn; Aggressive and Swarm +25 percent),
`template` (a lair whose group is away or cleared) or `estimate` (ring only).
`preview_move` rates the target as the player knows it (live: the truth;
charted: the rating last seen; otherwise the ring estimate, so it leaks
nothing) and returns `fleet_power`, `ratio` and `label`. Scans carry
`threats` and faint contacts a `threat_band`.

**Ore.** Tile steps 3 / 7 / 12 / 20 raw units (totals 3 / 10 / 22 / 42) times
`ring_mult`; density odds drift with distance; `harvest_yield` replaces
`harvest_rate_modifier` (research raises cargo, not extraction speed); a
harvest tick takes at most what is left in the current density level.
Regeneration is `scaling::ore_regen_step`: each tick the stripped tile regains
`total_reserve x P / (regen_hours x 3600)` raw units, walking back up the
levels, paused under a fleet. Sectors carry `ore_reserve {metal, crystal,
deuterium: {units, max_units, refills_in_s}}`. The old regen never revived a
tile emptied to None (threshold 0) and refilled partial levels at once; both
are gone.

**Loot.** `wreck_value = 1.2 x group_power^0.8` split 55/30/15 times
`finds_mult`, from the group actually spawned (`NpcFleet.power`), for the event
and the pile. Derelicts: tier by threat band, value `240 x 1.14^(d-4)` at 60 to
140 percent, ambush `0.10 + 0.03 (T-1)` (cap 0.35), recovery, data core and
relic chances per the spec, guards at 0.6 x the ring's group power, respawn
`DERELICT_RESPAWN_TICKS` (120 h) over P^0.5. The relic is a persisted counter
(`players.relics`) and a `relic` flag on `SiteExplored`.

**Exploration score.** First successful boarding of a derelict by a player pays
`0.15 value/1000 + 0.02 T^1.5` (value without the pace factor, so fast worlds
do not inflate it), a relic 25, and every sector newly charted pays
`0.01 T^1.5` when the charting fleet docks at the homeworld
(`Fleet.charts`, copied on split, unioned on merge, lost with the fleet;
`ChartDelivered`). Credits live in `explore_credits`. The 25 percent cap and the
leaderboard column were already there.

**Hostile names.** `CombatStarted.enemy` and `FleetDestroyed.label` carry
"3x corvette pack"; the fleet-lost alert and the TUI and web logs use it.

**Deviations from the spec text.**
- Regeneration refills the whole tile in `regen_hours` (what section 6.3's
  sentence and `sim.py` do); the formula "threshold of the current step x ..."
  would take up to four times as long. Documented here, not in the spec.
- `ore_reserve` is per resource, not one `refills_in_s` for the sector.
- `age_ticks` is not sent: `last_seen` and `live` say the same.
- The spec's "NPC power" column is the target; the real group differs by up to
  half a ship (a lone frigate at ring 15 is 43 against 65). Observed threat
  uses the real group, so T4 shows there where the table says T5.
- Ambush guards are 0.6 x the ring group (the sim's figure) instead of the old
  tier tables.

**Live check** (port 7816, pace test, one player, a ladder bot to 14 Cruisers
and 10 Scouts; the finds factor is sqrt(8000) = 89.4 and divided out below).

| ring | spec group (target power, T) | seen (real power, T) | pile per kill, seen / spec | mean ore tile, seen / spec |
|---|---|---|---|---|
| 6 | scout..corvette, 11, T2 | 1 corvette 16, T2 | 11 / 8 | 30 / 20 (n=6) |
| 9 | corvette, 20, T3 | 1 corvette 17, T3 | 12 / 13 | 25 / 29 |
| 12 | 2 corvettes, 36, T4 | 2 corvettes 37, T4 | 21 / 18 (ring 11) | 38 / 42 |
| 15 | frigate, 65, T5 | 1 frigate 43, T4 | 24 / 34 | 79 / 59 |
| 18 | 3 frigates, 118, T5 | 3 frigates 138 (173 aggressive), T6 | 62 / 54 | 80 / 84 |
| 20 | 4 frigates, 175, T6 | 4 frigates 192 (240), T7 | n/a | 102 / 105 |
| 22 | 2 cruisers, 260, T7 | 2 cruisers 229 (286), T7 | 93 / 103 | 111 / 109 |
| 25 | 4 cruisers, 473, T7 | 4 cruisers 484 (605), T8 | 169 / 166 (18 kills) | n/a |
| 27 | 6 cruisers, 704, T8 | 6 cruisers 752 (940), T9 | not fought | 179 / 115 (n=3) |

Ore means come from few tiles per ring and are noisy; the density mix and the
means over the whole generated world match the spec's column in
`scaling::golden_tests`. A derelict at ring 11 paid 807 (spec mean 601, range
360 to 841) and at ring 16 1085 (mean 1156, range 694 to 1619). A tile
stripped at ring 5 refilled in 106 s against the schedule's 108 s
((120 + 24 x 5) x 3600 / 8000) and `refills_in_s` counted down with it. The
leaderboard explore column went from 0 to 0.92 on the first chart delivery
(12 sectors), then 1.18 and 1.56 for two first boardings; combat stayed 0
because the fleet was 64x the groups it killed. The derelict respawn is 80
minutes at the test pace (432000 s over sqrt(8000)), so it was only checked in
unit tests.

**Web.** `lib/protocol` and the mapper carry the new fields; the console reads
the server's rating in `console/threat.dart` (`SectorThreat.of`). Left for the
console: the map and the route preview still derive the ratio from fleet and
sector power instead of calling `preview_move`, and show the rating only for
sectors with hostiles (`ringRating` is there for the rest); the ore reserve is
not drawn.

## Session — 2026-10-10: rules pass after the second six-AI playtest

Playtest findings in `docs/playtests/2026-10-10/`. This entry grows with each
commit of the `rules-pass` branch.

### One queue rule

- An order that can ever start is accepted whether or not a slot is free or it
  is affordable; `NoResources` is gone from build, research, ship and defence
  commands. A queue holds its running items plus 3 waiting (`QUEUE_WAITING`;
  `slots + 3` for buildings). The old `QUEUE_DEPTH` (3, running included) left a
  3-slot player no waiting room at all, which is what the playtest hit.
- Strict FIFO per queue: the first order whose prerequisites are in place either
  starts (slot free and affordable) or holds the line. Orders waiting only on a
  prerequisite are skipped. The old "skip any blocked item" let a cheap order
  starve an expensive one and made start estimates meaningless.
- `server/src/queue.rs` projects every waiting order: `waiting_on` (`Slot`,
  `Resources`, `Prerequisite`, `Order`), exact `waiting_for` shortfall and
  `start_in` ticks at current production (ignores the other queues' spending
  and mines that finish meanwhile). Dependencies inside the line (Metal Mine
  L5 behind L4, a Shipyard behind its Metal Mine) are walked through the known
  finish times.
- New `EventKind::Queue(QueueEvent)` with `Started` / `Waiting` / `Cancelled`:
  the reply to every order and cancel, and the notice when a waiting order
  starts by itself. This fixes "(no new events)" after a cancel. A queue with
  nothing running but orders waiting is shown as "none running", not "idle".
- Protocol: `queue_depth` is replaced by `queue_waiting_max`; `QueuedBuild`,
  `QueuedResearch` and `QueuedShip` gained `waiting_on` and `start_in`.
- Bug found by the raid-cadence test: `fleet.idle_ticks` overflowed (debug
  panic) after 65535 idle ticks; it saturates now.

### The map pays (combat, exploration, cargo, raids, storage)

- **Why it was dead:** loot scales with P^0.5 and cargo did not, so a blitz
  pile of thousands met holds of 10 to 40, and points were priced off the
  unscaled NPC build cost. `Pace::cargo_mult` (= P^0.5) now multiplies every
  hold (`fleet_cargo_capacity(fleet, pace)`). Salvage piles last 180 ticks.
- **Combat points:** `KILL_SHARE x weight(pile) x clamp(4 / ratio, 0.05, 1)`;
  the pile already carries the finds factor and the sector's heat.
- **Anti-farming:** `SectorOverride.kill_heat` / `kill_heat_tick` (new columns,
  migrated by `ensure_column`). 0.75 per recent kill, half-life 4 respawn delays.
  The playtest's "same template NPC destroyed repeatedly" was this tap.
- **Exploration:** chart `0.15 + 0.10 T^1.5`; first boarding `0.5 x weight(loot
  incl. finds) + 1.5 T^1.5`; relic 30 at 2% per T above 5 (max 8%).
- **Cap:** kept at 25% (`sim.py --compare`, spec section 18). Raising it to 35%
  made a season explorer 141% of a builder.
- **Raids:** the scheduler was not broken: engine tests over 12 blitz players
  give 1.2 to 3 forecasts an hour and about 3 a season day. The playtest's quiet
  hours were luck (a 30% roll about 13 times) plus nobody reading
  `RaidIncoming`. Pity (+15 points per missed roll) removes the bad tail. A lost
  raid destroys a quarter of the docked ships (`RaidResolved.ships_lost`). Not
  done: hull wear on a repelled raid (no repair exists, so it would be a creeping
  tax); an over-defended turtle stays safe on purpose.
- **Storage:** cap multiplier is P^0.75 to pace 10, then P^0.25. The sim now
  shows the blitz Vault in use (3.7% of the time above 90% of cap, was 0).
- **Mines:** payback at a blitz (cost over extra output): Metal 12 about 8 min,
  15 about 18 min, 18 about 42 min. Players stopping near 12 to 13 were limited
  by queue and metal, not by payback; no change.
- **Sim:** `PILOT_SKILL`, `ATTENTION_PENALTY`, `CHART_RATE_PER_FLEET_H` and the
  four styles are assumptions, flagged in `sim.py`. `check_sim_constants.py`
  now compares the score, farm and cap constants.
- **Protocol:** `RaidResolved.ships_lost`; `Queue` event; `queue_waiting_max`;
  `waiting_on` and `start_in` on queued orders.
## 2026-10-10: autopilot safety, route warnings and threat bands

From the second six-AI playtest (Haiku-1, Sonnet-1, Grok-1, Grok-2).

**Root causes.**
- A scout died on `prospect` because the autopilot only avoided sectors that
  held hostiles at the moment it moved. It never compared the fleet's power
  with the sector's threat, so it entered unscanned ring-6 sectors where a
  corvette patrol (rapid fire 3 against scouts) shreds scouts.
- `prospect` held "nothing uncharted and safe" with the frontier unvisited
  because every sector with a lair, passive or beatable, was a wall: 40 to 45
  percent of the sectors inside ring 8 hold one, and they cut the reachable
  graph (in three worlds 14 to 48 free unexplored sectors stayed behind
  lairs while the fleet held). The message named no constraint.
- "The way home is 17 hops" was `known_hops_home`, which walks only sectors the
  player has entered. The autopilot's return test used the same figure, so it
  turned for home on a charted detour while the fleet then walked the direct
  lane. Both now use the shortest real route.

**Autopilot.** Every doctrine enters a sector only when fleet power divided by
the threat the player sees there (live, charted or the ring estimate) is at
least `engage_ratio_x10` (default 1.2), never below EVEN, and at least SAFE
(2.5) where hostiles sit, unless a patrol is hunting. Heading home it prefers
a safe lane and falls back to the shortest. Refusals are `hold`
`PolicyAction`s naming sector, threat and ratio. `prospect` says whether a
hazard, `max_range` or a fully charted board stops it. Over nine 6000-tick
runs a two-scout prospector no longer died (before: 4 of 9 within 500 ticks)
and charted 99 to 123 sectors. Live (blitz, port 7818, about 15 minutes):
the scout fought passive scout patrols at 2.5x and refused sectors at
0.96x to 0.99x (RISKY).

**Label bands.** `ratio_label`: SAFE 2.5, FAVOURABLE 1.5, EVEN 1.1, RISKY 0.9,
else DEADLY (was 3.0 / 2.0 / 1.2). From `combat::tests` (ignored
`print_win_rates_by_ratio`, 400 fights per row, like-for-like classes): below
0.9 the fleet wins 0 to 1 percent; at 0.94 to 1.0 wins swing from 25 to 99
percent; from 1.1 it wins every time but keeps only 40 to 60 percent of its
hull; at 1.5 it keeps 85 to 95; at 2.5 98 or more. Damage exchange is
quadratic, so the bands crowd around 1.0. A lighter class against a heavier
one (scouts v corvettes) needs about 1.5x the ratio.

**Protocol.** `MovePreview` gains `charted_hops_home` (nullable) and
`route_unexplored`; `hops_home` is now the shortest real route and
`fuel_to_return` follows it. `RatioLabel` gains `even`. Warnings read "the
charted route home is 17 hops, 46 fuel; the direct route is 2 hops, 12 fuel,
across unexplored space". Fixture `protocol/ratio_labels.json` ties the web's
`RatioLabel.forRatio` to the Rust bands.

**Web.** Route preview and gate hover call `preview_move` for the selected
fleet (200 ms debounce, refreshed every 3 ticks, shown for 8). The inspector
shows the server verdict, basis and the way home; far hops of a route and the
sector the fleet is in are still divided client-side from the server's
`est_power` because `preview_move` takes adjacent sectors only. Scan contacts
show `threat_band`, `ScanCompleted.threats` drives the contact banner, ore
reserves with refill times appear in the inspector and the windshield contacts,
and a waiting order no longer reads as the running one. Not done: `FleetState`
does not carry both home routes, and `play.py` still needs the new labels.

### Live check of the rules pass (port 7817, blitz, 16 minutes, three scripted players)

Scripts in the session scratchpad drive three headless clients with the same
economy loop; the explorer and mixed players also fly scouts out, scan, board
derelicts, collect salvage and fight what they out-power by 3x. Final
leaderboard (score = core + capped extra):

| player | score | core | combat | explore (raw) |
|---|---|---|---|---|
| Run-Mixed | 22.5 | 18.0 | 2.7 | 50.8 |
| Run-Explorer | 21.1 | 16.9 | 0.8 | 40.8 |
| Run-Builder | 15.0 | 15.0 | 0 | 0 |

Reading: queue orders never errored with `NoResources`; only the real refusals
(prerequisites, `QueueFull`) appeared. Raw explore points ran at 40 to 50
against a cap of 25% of a 17-point core, so the cap bound from the first
minutes: at 16 minutes (about a quarter of the blitz arc) piloting was worth
+41% (explorer) and +50% (mixed) over the builder, and the pilots' core was
also 13 to 20% higher from loot. The simulation's 100 to 124% assumed a
chart tempo of 30 sectors per hour; the script charts about 250 per hour (66
to 86 sectors in 16 minutes). If the board should stay closer, lower the cap,
not the point values; the values only matter before the cap binds. Not
measured: the full hour, Haulers in a fleet, raids (one RaidIncoming and
RaidResolved for the builder).

## 2026-10-10: mechanics pass after the third six-AI playtest (branch `mechanics-2`)

Findings 1-6 of `docs/playtests/2026-10-10b/`; no balance numbers changed.

**Queues.** `advance_queues` now takes the first waiting order that is ready
and affordable (`first_startable`), per queue, instead of stopping at the first
ready one. `reserve` (a field on `build`, `research`, `build_ship`,
`build_defence`) makes a ready-but-unaffordable order stop the scan. Every
order has an id from the engine's `next_id()` counter, so ids are unique across
the world and persist (`build_queue.item_id`, `reserve` columns; ids missing
from an old database are assigned on load). `cancel_build` / `cancel_queued`
take `id`; the refusal text is built from where the id really lives. Ship
batches: the running entry is one paid unit; on completion the batch re-enters
`ship_pending` at the front with `built + 1`. The projection (`queue.rs`)
estimates each order against the stockpile minus earlier orders that start no
later, and treats only reserving orders as gates. Research and shipyard orders
now accept a queued Lab/Shipyard (they already used projected levels for the
other checks). The "tech that just started" `MaxLevelReached` in the playtest
was a tech at its real maximum; a multi-level tech queues its next level.

**Doctrines.** `policy_entry_check` needs exactly `engage_ratio_x10`; the
`Entry::{Passage,Hunt}` distinction existed only for the 2.5x override and is
removed. The repeated hold was mostly the same cause with drifting figures in
the reason text ("your 18" then "your 19"), so the dedupe key now drops
parenthesised figures (`hold_key`) and a cause reports at most once a minute.
`patrol_home` and `salvage_and_sites` go through the same entry check and hold
path, so they share both fixes.

**Salvage and pins.** `SectorState.pins_stale`; `KnownSectors::refresh` drops a
remembered pile past its despawn tick and `tick_updates` sends that sector once.
`RaidResolved.salvage_despawn_tick`. `process_home_salvage` scoops a home pile
into storage for a player with a fleet docked at home (what fits under the
cap), and otherwise alerts once at 60 s left (`HOME_SALVAGE_WARN_TICKS`,
RealTime). Scan `signals` already excluded the scanning sector (they come from a
ring one hop beyond the scanned range); the confusion was the event's own
`sector` field, now documented, and a test pins it.

**Clients.** The TUI cancels the first running or waiting item by looking up its
id in the latest state. The web changes are in the web commit.

**For `play.py` (not done here):** cancel by `id`; read `id`, `reserve`, `built`,
`unit_cost` from the queue lists and `id` from `Queue` events; optional
`reserve` on build commands; `pins_stale`, `RaidResolved.salvage_despawn_tick`
and the home-salvage `Alert`.


## 2026-10-10: `iac-sim` extracted (roadmap step 1 of single player and bots)

All game rules and state now live in `sim/` (`iac-sim`); `server/` is a host
(axum WebSockets, SQLite writer thread, tokio tick timer). Wire protocol and
behaviour are unchanged; a live blitz session on 7821 (build, move, scan, fight,
restart with the world kept) matched before and after.

**Seams.** `GameEngine::new(seed, pace)`, `restore(Snapshot)`, `snapshot()`,
`attach_persist(Box<dyn Persist>)`, `execute(player, Command) -> CommandOutcome`
(reply and wording of refusals, moved from `network.rs`), `views::{full_state,
tick_update}`, `persist_dirty_state()`. `Persist` (`submit(PersistBatch)`,
`flush()`) is implemented by the server's `Persister`;
`Database::load_snapshot` turns SQLite into a partial `Snapshot` (NPC groups,
combats, scans, raid clocks stay empty, as before) and `restore` applies the
old load-time fixes. `Snapshot` is serde, canonical (ordered maps) and has
`hash()`. Token issuing stays in the server (needs OS entropy); `authenticate`
takes the token source as an argument; hashing and checks are in `sim/auth.rs`.

**Purity.** No tokio/axum/rusqlite/getrandom dependency (rand without OS
entropy); `sim/clippy.toml` bans `SystemTime::now` and `Instant::now`. Builds
for `wasm32-unknown-unknown`; no bindings yet (milestone 3).

**Bugs found on the way.** Combat rounds were seeded from the wall clock; they
now use world seed, tick, sector and round. Engine maps were `HashMap`s walked in
random order (ship ids differed between two identical runs); they are
`BTreeMap`s now. A world past about 28 empires could not register more players
(home band rings 3-6); homeworlds now spill outward ring by ring, worlds under
capacity place as before.

**Speed.** The per-tick chart refresh (`KnownSectors::refresh`) is about 95
percent of tick time with many players; `WorldGen` now memoises sectors, lanes
and sensor reach (about 10x). Remaining cost grows with sensor footprints, so
long runs slow as empires grow. Next lever: refresh charts only when observed.

**Tests.** `sim/tests/determinism.rs` (same seed and script, equal hashes every
1000 ticks; mid-run snapshot through JSON resumes identically). The SQLite-based
restart tests in the engine now restart from a snapshot; SQLite round trips are
covered in `server/src/database.rs`. Pace and world-version refusal tests moved
there too.

**Next:** bots on `ScriptedPlayer` (sees `GameState`, emits `Command`s only).

## 2026-10-10: Flutter test flakiness on the dev host

- **Symptom:** the Flutter suite failed about 10% of full runs on this aarch64 host
  (vendor 6.6.10 kernel), with assertions that cannot fail by program logic: NaN from
  constants, sizes "not meeting" equal constraints.
- **Cause:** it correlates with the Dart VM profiler's SIGPROF storm (about 600/s
  into `flutter_tester`). Throttling it removed the failures in 30 of 30 runs:
  `DART_VM_OPTIONS=--profile_period=100000000 flutter test`.
- **Real bugs found on the way, now fixed:**
  - the app read wall time directly (HUD interpolation, toasts, scan timers, map clock);
    it now goes through `package:clock`;
  - the windshield sim used an unseeded `Random()`.
- **Status:** after the fix, 8 of 8 runs pass with and without the flag. If the host
  flakiness returns, use the flag before chasing the code.

## 2026-10-10: chart upkeep follows changes

`KnownSectors::refresh` no longer rebuilds every live sector of every player each
tick. Coverage (fleet cells, sensor footprint, scans) is kept per player and
re-derived only when its inputs differ from the last tick; watchers per sector are
kept alongside. A watched sector is re-read once per tick, shared by its watchers,
and only if an override, an NPC group or the place an NPC group left could have
changed it. A live sector's `last_seen` is implicit (the last refresh) and is
written when it lapses; salvage expiry is an ordered per-player index. Views build
sectors from one pass over fleets and NPC groups instead of one pass per sector.

`sim-run --pace season --builders 50 --idle 2`: 4.9k ticks/s for day 1 before,
about 29k after; day 2 1.4k before, 32k after; the week finishes in 20 s (31k
ticks/s) where it did not in 76 minutes. Wire output and persisted rows are
unchanged: the per-tick `tick_update` and `full_state` JSON of a 4000-tick scripted
world was byte-identical to the old algorithm, the `sim-run` snapshot hash matched,
and `sim/tests/chart_equivalence.rs` keeps comparing against hashes recorded from
the old path (`sim/tests/data/chart_golden.txt`). The old module is in `.archive/`.

Remaining profile: libm calls (about 13 percent) and the per-tick walks over
`sector_overrides` and `npc_fleets` in the regen and NPC passes.

## 2026-10-10: tests are end-to-end only

The owner approved retiring the unit suite. Rust went from 273 tests to 69 and
`cargo test --workspace` from about 75 s to about 40 s; Flutter from 351 to 299
tests and from about 30 s to about 15 s. Retired tests are in the gitignored
`.archive/`, mirroring their source paths, and in git history.

- **Archived:** the inline tests of `shared` (75), `sim` (`engine.rs` 145, `auth`,
  `combat`, `score`) and `server` (`auth`, four of the `database` tests);
  Flutter's `widget_test`, `demo_world_test`, `economy_view_test`, `queue_ids_test`
  and single-purpose cases in the screen tests.
- **Kept:** the golden fixtures (Rust and Dart), `determinism`, `smoke`, `web`,
  the database reload and refusal tests, the `Network` tests in `network.rs`
  (the server is a binary crate, so they cannot move to `tests/`), the screen and
  shell tests in Flutter.
- **Rewritten against the public API** (`sim/tests/scenarios_*.rs`): the tests that
  run the engine over time. State that a client cannot set is edited through
  `Snapshot` and `GameEngine::restore`.
- **New scenarios:** `empire_arc.rs`, `fleet_logistics.rs`, `pace_scaling.rs` in
  `sim/tests/`, and `server/tests/scenarios.rs` (case-insensitive accounts, event
  scoping between two players, a cancel aimed at an order that has started).

## 2026-10-10: built-in bots and the first balance report from the real engine

Roadmap step 2 of single player and bots, the `iac-sim` part. Not done: seating bots
in the server, the BOT tag, the wasm bundle. No balance constant was changed.

**Seam.** `ScriptedPlayer::act` emits `Order`s (a `Command` or a `PolicyUpdate`),
`hear` gets replies (a bot asks for its own leaderboard row), `seated` hands over the
world seed and player id. `script::Seat` binds a player to an account (`join` or
`resume`) and runs its turn before a tick (`take_turn`); `Headless` keeps seats, a
server or wasm host would do the same. `CheckInBuilder` is gone.

**Bots** (`sim/src/bot/`): personality x attention.
- *Home* (`home.rs`, shared by all): support buildings and techs are ordered once they
  cost no more than an appetite times the next Metal Mine level; mines by payback,
  tilted toward the resource that sits lowest against its cap; fleet value held at a
  share of econ points (raider 20%, mixed 15, explorer 10, miner 5, turtle 0); Pulse
  Turrets to 1.15x the next raid (turtle 1.5x). Orders carry `reserve`. A present
  player queues one order at a time; one about to leave fills the queues, rates each
  building place by the income it must absorb, and puts unclaimed stock into one batch.
- *Fleets* (`field.rs`): ships are kept in wings (mining, survey, recovery, strike).
  Away: `mine_and_return`, `prospect`, `salvage_and_sites`, `patrol_home`. Present: a
  pilot that scans before entering, walks to the nearest uncharted sector, ore, wreck
  or beatable group, boards quiet derelicts, fights at 3x (8x against a class with
  rapid fire on it) and only with shields up, comes home on fuel or a full hold.
- *Attention*: check-in = 4 look-ins of 5 minutes a day (`--sessions`), hybrid = the
  same with one of them an hour of piloting, hands-on = always present, no doctrines.
- *Two rules the engine forced.* Refuelling takes deuterium ahead of every queue, so a
  pilot leaves only with a tank to spare beyond the next orders, and a fleet goes under
  a doctrine only while half the deuterium production covers its burn. Wings that fly
  for points stay home once combat plus explore exceed 1.2x the cap (`--ignore-cap`
  turns that off).

**`sim-run`**: `--seat <personality>:<attention>[:<n>]` / `idle[:<n>]` (default: every
style once and an idle player), `--sessions`, `--every`, `--ignore-cap`, `--json`.
Prints the score split per player and sample and a table by style. `--builders` and
`--idle` are gone.

**Engine**: `WorldGen::lane_hops` memoises lane distance; `route_home_cost` and the
autopilot's fuel check use it (was a breadth-first search per fleet per view per
tick, the largest item in the profile with fleets far out). Outputs unchanged.

**Measured.** Season, 7 days, seeds 1 to 3, 16 players each
(`sim-run --pace season --days 7 --seed N --json`). Mean (min-max over seeds).

| style | total | core | combat | explore | cap filled | haul | raid salvage |
|---|---|---|---|---|---|---|---|
| turtle / check-in | 1756 (1556-1875) | 1756 | 0 | 0 | 0% | 0 | 233 |
| miner / check-in | 1694 (1676-1721) | 1693 | 0 | 1 | 0% | 0 | 249 |
| raider / check-in | 1634 (1399-1797) | 1595 | 1 | 39 | 10% | 5 | 231 |
| explorer / check-in | 1953 (1746-2258) | 1754 | 5 | 199 | 45% | 25 | 247 |
| mixed / check-in | 1675 (1541-1930) | 1595 | 2 | 78 | 20% | 6 | 241 |
| turtle / hybrid | 1876 (1817-1980) | 1876 | 0 | 0 | 0% | 0 | 299 |
| miner / hybrid | 2751 (2537-2884) | 2201 | 58 | 1140 | 100% | 319 | 296 |
| raider / hybrid | 1977 (1531-2240) | 1581 | 25 | 729 | 100% | 51 | 250 |
| explorer / hybrid | 2221 (2133-2331) | 1777 | 26 | 1060 | 100% | 84 | 295 |
| mixed / hybrid | 2221 (2127-2273) | 1777 | 19 | 798 | 100% | 48 | 295 |
| turtle / hands-on | 2400 (2400-2400) | 2400 | 0 | 0 | 0% | 0 | 374 |
| miner / hands-on | 3919 (3917-3922) | 3135 | 191 | 1683 | 100% | 994 | 500 |
| raider / hands-on | 3001 (2793-3179) | 2401 | 219 | 2059 | 100% | 528 | 295 |
| explorer / hands-on | 3132 (3036-3248) | 2506 | 65 | 1727 | 100% | 438 | 402 |
| mixed / hands-on | 3140 (3046-3326) | 2512 | 293 | 2479 | 100% | 686 | 371 |

Haul and raid salvage are resources in the points they buy (1 = 1000 metal).

Share of the same personality flown hands-on, by day (mean of seeds):

| | d1 | d2 | d3 | d4 | d5 | d6 | d7 (min-max) |
|---|---|---|---|---|---|---|---|
| turtle check-in | 40% | 50% | 48% | 57% | 68% | 81% | 73% (65-78) |
| turtle hybrid | 44% | 50% | 64% | 70% | 76% | 90% | 78% (76-83) |
| explorer check-in | 31% | 35% | 41% | 45% | 47% | 55% | 62% (54-73) |
| explorer hybrid | 48% | 44% | 45% | 54% | 57% | 65% | 71% (68-75) |
| raider check-in | 39% | 35% | 38% | 48% | 52% | 58% | 55% (44-61) |
| mixed check-in | 39% | 46% | 55% | 53% | 55% | 62% | 54% (47-63) |
| miner check-in | 22% | 15% | 22% | 31% | 34% | 36% | 43% (43-44) |

Look-ins per day, turtle only, day 7, share of hands-on (check-in / hybrid): 1: 15% /
55%; 2: 50% / 72%; 4: 74% / 82%; 8: 87% / 92%; 16: 88% / 85%. At persistent pace 4
look-ins reach 92% (89-96). Blitz, one hour, hands-on, 3 seeds: turtle 364, miner 481,
raider 434 (374-500), explorer 520 (385-671), mixed 504.

**What it says about the targets.**
- *Check-in must not fall hopelessly behind over a week.* Holds at 4 look-ins for the
  builder (73% on day 7) and fails early: 40% after day 1. The queue is the limit:
  3 waiting orders, and early orders take minutes at season pace. Two look-ins a day
  give 50%, one gives 15%.
- *Standing orders earn 50 to 70% of piloting.* No. Charts delivered: 0 to 18% of the
  hands-on count; haul: 0 to 6%; score that counted: explorer 32%, mixed 13%, raider
  6%, miner 0%. `mine_and_return` brought home 0.2 points of ore in a week against 367
  by hand: it goes back to the nearest seam, which it has stripped. `patrol_home`
  stays within 2 hops and leaves the wrecks. `prospect` rates an empty sector by its
  ring, so scouts stop where a pilot walks through. No doctrine can be given a fuel
  budget.
- *Explorers and raiders within 15% of a builder, mixed above both.* Hands-on: raider
  125%, explorer 131%, mixed 131% of the turtle, all from the bonus; their cores are
  100 to 105% of the turtle's. Check-in: 94 to 112%. The hands-on miner is at 163%:
  its haul is worth 30% of its core (seed 1: ore 367, wrecks 263, derelicts 205).
- *The 25% cap at season pace.* It binds for every pilot from the first hours: raw
  combat plus explore is 5 to 21 times the cap after day 1 and 2.4 to 4.4 times
  after day 7, bots heeding the cap; with `--ignore-cap` (seed 1) 8 to 22 times, then
  2.7 to 3.7. One hour of piloting a day keeps it bound all week (1.9 to 2.5 times on
  day 7); under standing orders alone only the explorer reaches it (days 2 to 4 on
  average, all week on one seed). Exploration is the supply: combat is 65 to 293 points against a cap of
  600 to 780.

**Engine behaviour found on the way (not changed).**
- A homeworld can sit on a lair (seed 1: 12 of 25, 7 not passive). Arriving home starts
  the fight; a ring-6 corvette lair kills the two starting scouts on their first return.
- A fight another player starts in a home sector enrols the fleets docked there; the
  idle player collected kills and salvage.
- `recall` during combat moves the fleet home and leaves it in the fight: shots keep
  landing on it while docked.
- Raid rolls are seeded from tick and player id, not the world seed.
- Shipyard units pay as soon as they can, ahead of a costlier `reserve` building in
  another queue: a look-in player cannot say "spend what is left".
- Pulse Turrets are the cheapest power (8.2 resources a point against 10.6 and 13.3).

**Speed.** 52 idle players 32k ticks/s; 50 check-in turtles 9.6k; 50 hands-on turtles
4.4k (a turn each 20 ticks); 16 seats 5.8k on day 1, and 19 minutes for the week (530
ticks/s, four runs side by side), slowing as charts grow: a turn clones the whole
chart in `full_state`.

**Limits.** Bots are heuristics; hands-on is an always-on machine, closer to an LLM
agent than to a person. Doctrine thresholds are fixed (ratio 1.5, strike 3.0). One
bot per style per world: the hands-on turtle has no seed variance at all.

**Tests.** `sim/tests/bots.rs`: a world of bots replays from its seed; bots build, fly
and delegate with orders the engine accepts.

**Next (server seating).** Keep `Seat`s beside the engine and call `take_turn` before
`tick`; persist which accounts are bots and their style; a restarted bot forgets
which sectors it charted (a `charted` flag on `SectorState` would serve clients too);
feed bots from `tick_update` instead of `full_state`; give them their own score row
without ranking the whole board.

## 2026-10-10: a restart loses nothing

`Database::load_snapshot` now returns the whole world. After SIGINT/SIGTERM it
equals `GameEngine::snapshot()` (same `Snapshot::hash`); after a kill it is the
last committed tick. Wire protocol unchanged.

**What a restart did before** (read from the code; the two marked "live" were
also watched on a master server at blitz):

- A fleet mid-jump came back `moving` with no target: it never arrived, and its
  standing orders never ran again, until the player sent `move` or `stop`. A
  fleet mid-fight came back `in_combat` with no combat, refusing everything but
  `recall`. The NPC group it was fighting was back at full strength, an ambush
  guardian gone.
- Every cooldown was 0: scan and harvest cooldowns were skipped and a boarding
  resolved on the first tick (live: one with 18 ticks left).
- Scan reveals were dropped (live: 16 of 17 live sectors went stale).
- The raid clock started over: an announced raid never arrived, the new-player
  grace period and the pity counter restarted, the minimum interval and the
  suppression after a lost raid were forgotten.
- Standing orders re-evaluated at once and repeated their last hold report;
  storage and home-salvage alerts fired again; up to 10 ticks of harvest were
  missing from the next `ResourceHarvested`.
- Ships came back in id order, not fleet order, so the next fight rolled
  differently. Stored floats were cut to thousandths.
- Part-mined ore of a tile under a fleet was written only when the tile dropped
  a density step, so a restart gave that ore back.

Nothing regenerated identically from the seed except NPC groups nobody had met.

**Now stored.** Typed columns where a typed table existed: `fleets.move_q/r`,
`move_cooldown`, `action_cooldown`, `idle_ticks`; `ships.slot`;
`fleet_policies.next_eval_tick`, `hold_reason`, `hold_tick`. The engine's
bookkeeping maps (NPC groups, combats, raid states, scan reveals, storage marks,
home-salvage warnings, harvest tallies) go entry by entry into one table,
`runtime_state (kind, id, data)`, as the JSON `Snapshot` uses for them
(`persist::RuntimeEntry`). The engine marks `(kind, id)` dirty at each mutation;
the batch carries the entry as it is at persist time, or `None`, which deletes
the row. Floats are stored as before (times 1000, REAL) but no longer truncated:
f32 times 1000 is exact in f64, so old rows read the same and new rows reload bit
for bit.

**Engine.** Dirty marks added wherever a stored field changed without one
(combat entry, harvest stopping, a raid re-sizing tanks, scan cooldown, policy
evaluation, every harvested unit of ore, the tick a tile finishes refilling).
Counters that run each tick go out as a three-column `UPDATE`
(`PersistBatch::fleet_clocks`) unless the fleet is written anyway. `idle_ticks`
stops at `SHIELD_REGEN_IDLE_TICKS`, the only value it is compared with, so a
resting fleet stops changing. `checkpoint()` (was `checkpoint_known_sectors`)
also queues refilling tiles, which like live chart stamps are otherwise written
every 60 ticks. Unused ship slots are cleared after a recall loses a ship.

`restore` no longer clamps `fuel_max` to the tank of the ships present. It was
there for 50000-fuel fleets from before the starting-fuel fix, which predates
`world_meta`, so no loadable database has one; what it did do was shrink the
tank of any fleet that had lost ships in a fight, on every restore, including a
`Snapshot` round trip.

**Old databases** load: columns by `ensure_column`, the table by `CREATE IF NOT
EXISTS`; missing state is at rest (no cooldown, no raid clock). When the jump
columns are added, fleets saved as `moving` or `in_combat` are idled once. A
`runtime_state` row that no longer parses is skipped with a warning. A master
build's blitz database was opened by this one and played on. Not reversible: an
older binary rewrites rows without the new columns.

**Tests.** `database.rs`: `a_restart_through_sqlite_continues_the_same_world`
plays 2000 ticks at test pace against SQLite beside the same world in memory,
compares database and engine every 25 ticks (all but the two 60-tick items) and
reloads 14 times through the clean path, three of them with a fight, a jump, a
scan and an announced raid at once and three mid-boarding; hashes equal at every
restart and at the end. Run once with the comparison on every tick for 4000 ticks
(test pace) and 6000 (blitz). `a_world_saved_before_jumps_and_combats_were_stored_still_loads`
drops the new columns and table from a played world. `scenarios.rs` kills the
real server during a boarding with a scan live and checks both over the wire.
The old approximate reload test is in `.archive/tests/server/src/`.
`libsqlite3-sys` and `rusqlite` build at opt-level 2 in dev, like `iac-sim`.
`cargo test --workspace` about 53 s.

**Live** (release server on 7840 at blitz, three headless clients, two flying
standing orders). Kill -9 at tick 16: a boarding with 18 ticks left came back
`Exploring` with 16 left at tick 18 (master: resolved at once), 17 scanned
sectors stayed live. SIGTERM at tick 147: same. A raid announced at tick 897
for tick 927; SIGTERM and restart at 901; it resolved at tick 927 with the
announced power 89.9. Raid clocks kept `first_seen_tick` 2 through all three
restarts. Database 168 KB after 930 ticks. No fight happened to be on at a live
restart; that case is covered by the in-process test only.

**Cost** (release, 67 players, 900 ticks at blitz, best of three; 67 is what one
world holds before `find_homeworld_location` gives up):

| | master | now |
|---|---|---|
| every player commanding every 4 ticks: writer per tick | 7.86 ms | 8.03 ms |
| same: tick thread, clone into the batch | 0.037 ms | 0.043 ms |
| same: extra rows per tick | | 10.6 clock updates, 6.4 runtime, 1.6 policy, 1.6 sector |
| nobody commanding: writer per tick | 6.56 ms | 6.58 ms |
| database after 900 ticks, busy / idle | 2.18 / 0.23 MB | 2.21 / 0.25 MB |

`runtime_state` rows: raid 124 B, storage marks 24 B, scans about 400 B (up to
1 KB), combat 129 B, harvest tally 107 B, NPC group 400 B (about 4 KB at 32 ships).
Two rows per player plus what is in flight; nothing grows with time. The writer
is dominated, as before, by rewriting every player every tick (about 0.1 ms each).

**Not stored.** Events awaiting broadcast: the server drains them before every
persist, and nobody is connected to a server that is down. On a kill only: up to
60 ticks of a live sector's `last_seen` and ore reading (re-observed on the first
tick) and of a tile's refill.

**Found on the way, fixed after** (rules, not persistence; a restart used to
clear the first two by accident and no longer did). One commit each, each with
a scenario that failed before it:

- *Jump cut short.* A fleet still charging a jump is in its sector's fights
  (`enroll_all_player_fleets_in_sector_combat`); it came out idle with the
  jump's `move_target` and `move_cooldown`, which only a moving fleet counts
  down, so `process_policies` skipped it until a manual move. Recall mid-jump
  did the same. Entering a fight (`engage_fleet`) and recalling now drop the
  jump as `stop` does; the fuel stays spent. `scenarios_world.rs`:
  `a_jump_cut_short_by_a_fight_or_a_recall_leaves_the_fleet_free_for_standing_orders`.
- *Recall and abandoned fights.* `handle_recall` left the fleet on the combat's
  list and rounds are resolved from the list wherever the fleets are: a
  recalled fleet banked its cargo and kept trading fire from home (measured:
  hull still falling five ticks after arrival), which is how a study trial saw
  one destroyed at its homeworld. Recall now takes it out (`leave_combat`).
  A fight left with no player fleet was dropped as it stood, its NPC groups
  `in_combat` for good; it is now broken off the way a lost fight ends
  (`break_off_combat`): groups removed so the templates stand again at full
  strength, listed fleets idle, `CombatEnded` with `player_victory: false` to
  those who fought (the web client already reads that as "withdrew or was
  lost"). Recall in combat stays allowed. Keeping the damaged group alive
  instead would switch on NPC roaming, which never runs today because groups
  exist only inside fights. `a_recalled_fleet_is_out_of_its_fight_and_the_hostiles_it_left_can_be_fought_again`,
  `a_fight_nobody_is_left_in_frees_its_hostiles_for_the_next_fleet`.
- *Ninth fleet.* `MAX_COMBAT_FLEETS` was the length of a fixed array in the Zig
  engine; in Rust the lists are `Vec`s and nothing else needs it. Past eight, a
  fleet was marked `InCombat` without being listed: it never fired, was never
  credited and was never freed. The cap is removed, so the ninth joins: the
  enrolment says every fleet in the sector fights, and staying out would leave a
  fleet next to hostiles it can neither be shot by nor attack.
  `every_fleet_in_a_crowded_sector_fights_scores_and_comes_out_free`.
- *Tank after losses.* Combat was the one place ships left a fleet without
  `fuel_max` being recomputed. It now is, fuel above it lost with the ship as
  when a raid sinks docked ships. For clients: `fuel <= fuel_max`, `jump_fuel`
  and `home_fuel` already followed the surviving ships, and `range_hops` no
  longer exceeds what the survivors' own tank reaches (it used to drop on
  docking). `fleet_logistics.rs`:
  `a_fleet_that_loses_a_ship_in_a_fight_shows_the_tank_and_range_of_the_ships_it_has_left`.
  With it: the tank of a fleet with no ships was `-0.0` (an empty float `sum()`),
  which SQLite stores as 0; `fleet_fuel_max` folds from `0.0`. Found by running
  the restart test with the comparison on every tick (2000 ticks at test pace,
  5000 at blitz, both pass; in each run two fights ended by a recall, 8 and 26
  fleets lost some of their ships and 16 and 37 all of them).

`chart_equivalence` still matches its recorded hashes after all four.

**Still open:**

- A fleet that loses haulers in a fight keeps cargo above its hold (as after a
  raid); harvesting stops and it unloads normally.
- Recalling out of a boarding drops it without the alert bump `stop` gives the
  site.
- A fleet that recalls out of a fight others stay in gets no event saying it
  left; its state in the next `tick_update` is the only sign.

## 2026-10-10: home seats, quiet home ground, scout Inner Ring, per-world lanes

Owner decisions of 2026-10-10 (`docs/design/decisions.md`), from the study in
`docs/design/starting-positions-2026-10-10.md`. `WORLDGEN_VERSION` is 3; older
databases are refused on load and nothing is migrated.

- **Seats.** A world has 24 home seats: the sectors of rings 3 to 5 whose `q - r`
  is a multiple of three (`world::is_home_seat`; 6 in ring 3, 6 in ring 4, 12 in
  ring 5). That lattice is the only one of its three cosets that every rotation
  and mirror of the grid maps onto itself, its points are never adjacent (nearest
  seats two hexes apart, as the old "not within one hex" rule gave in a full
  band), and it lands on the owner's 24 exactly. A new empire gets the free seat
  whose nearest occupied seat is farthest, ties split by the sector seed
  (`next_home_seat`). `HOMEWORLD_SPILL_RINGS` and the random tries are gone.
- **Full world.** `register_player` returns `ErrorCode::WorldFull` (2005) and
  changes nothing, not even the id counter; `AuthError::Server` became
  `AuthError::WorldFull`. Clients show "this world is full: all 24 home seats are
  taken": the web login form (with the fixture as the fake server's reply), the
  TUI and headless client (with a hint), `play` in place of "No state received".
- **Home ground** (`world::is_home_ground`: a seat or a sector beside one, 102
  sectors: all of rings 3 to 5, half of ring 2, two thirds of ring 6). Worldgen
  generates no hostile group there; Passive convoys stay. No derelict on a seat.
  Derelicts beside a seat stay: a guard exists only for the fight a boarding
  starts, and removing them too would take 16 of the 28 derelicts left in rings
  4 to 8 (8 seeds; 2 sat on seats). Patrol movement skips home ground, though
  no group outlives its fight today, so nothing roams.
- **Gradient.** Scouts through ring 8, corvettes 9 to 12. The count is still
  sized on `0.6 + 0.02 d`, but the stat multiplier is now fitted so the ships add
  up to `npc_power(d)`; an extra ship is added when that would need more than
  x1.3. Real power equals the ring estimate for rings 1 to 36 (it was 0.67 to
  1.47 times it); from 37 the 32-ship limit holds it at 4576. Derelict guards are
  built the same way from 60 percent. `sim.py`, the spec (section 7.1 and the
  first table) and `check_sim_constants.py` follow, and the check now also
  compares group power with the estimate.
- **Lanes.** `edge_exists` mixes the world seed into the pair hash: 2561 of 2791
  sectors (rings 0 to 30) have a different lane mask between two seeds, 0 before.
- **`patrol_home`** named fleet 0 when the group in its sector had not
  materialised, so it stopped for good at the first Passive convoy. It attacks
  the sector's template now. Found because home ground leaves it little else.

Measured with the real engine, season pace, the study's 8 seeds (48 for the fill):

| | Before | After |
|---|---|---|
| Join refused (min / median / max) | 57 / 68 / 77, "could not create the account" | 25 / 25 / 25, `WorldFull` |
| First home past ring 6 / past ring 8 | join 27 / 46 (median) | never |
| Nearest earlier home at joins 2, 3, 4, 5-6, 7-8, 9+ (minimum) | 2 throughout | 8, 6, 5, 3, 2, 2 |
| First 24 homes with a hostile lair on the tile / on or beside it | 16% / 74% | 0 / 0 |
| Starter fleet survives a lair at rings 1-2 / 3-5 / 6 / 7 / 8 / 9 / 10 | 100 / 100 / 4 / 0 / 4 / 3 / 1 % | 100 / no lairs / 100 / 100 / 100 / 0 / 0 % |
| Hull the survivors keep at rings 6 / 7 / 8 | 22 / - / 19 % | 90 / 75 / 41 % |
| Estimate against real power, rings 1-36 | 0.67 to 1.47 | 1.00 |
| Starter fleets lost in day 1 (mine, prospect, salvage; one empire per home) | 38 of 324, 14 on their home tile | 0 of 72 |
| Hostile lairs within 2 / 4 hops of a seat | 3.0 / 10.6 | 0.7 / 5.1 |
| `sim-run --builders 24 --days 7`, distinct scores | 1 (1422.11, 28 builders) | 1 (1422.11) |

Labels compare power only. Scouts against corvettes still do worse than the label
says, as before: at ring 9 three scouts are EVEN (1.38) and survive 25 percent of
fights, four are FAVOURABLE (1.83) and survive 75 percent.

`sim/tests/data/chart_golden.txt` was recorded again: the script plays a new
world, and the algorithm the old hashes came from is archived. The test passed on
the old world before the change, so the recording comes from an implementation
that had matched the original; it now pins the chart against change
(`UPDATE_CHART_GOLDEN=1`) and no longer compares two algorithms. The scripted
world still fights (79 combats, 6 fleets lost; 169 and 14 before). New scenarios:
`scenarios_world.rs` (seating order, refusal, quiet home ground, estimate against
template for 36 rings, the starter fleet either side of ring 9) and
`server/tests/scenarios.rs` (a full world over the wire and after a restart).

Left alone: `sector_connections` still gives a sector with no lane a one-way lane
out (6 to 15 in rings 0 to 30, by seed); ratings of observed Aggressive and Swarm
groups (ring 16 out) run 25 percent above the ring estimate by design; `sim-run`
with more than 24 players stops at "cannot add player ... WorldFull".

## 2026-10-10: web client operator guide and background notices

The two parts of `design/ui-redesign/spec.md` that had no code: section 6
(onboarding) and the session hooks of section 5.8. Client only; the wire
protocol is untouched.

### Operator guide (`console/onboarding.dart`, `views/onboarding.dart`)

- **Shape.** One strip under the HUD, in its own row so it never covers a
  control, names the lesson the player is on with one `NOTE` sentence, the keys
  and a button that goes to the right screen. The control it means carries
  stepped corner brackets (`GuideTarget`). Once the walk is through, a
  "First hour" list is pinned on the Overview until the player closes it.
- **Lessons, and what ticks each:**

  | Lesson | Ticked by |
  |---|---|
  | Save your account token | the player pressing "I have saved it" (only offered in the session the server issued the token) |
  | Queue a building | a building order in the homeworld state, or a `Queue`/`BuildingCompleted` event |
  | Scan the lanes | `ScanCompleted` |
  | Jump through a gate | a fleet somewhere other than the homeworld |
  | Harvest ore | a fleet in `Harvesting`, or `ResourceHarvested` |
  | Recall and unload | the fleet that harvested being back at the homeworld |
  | Set standing orders | a fleet with a policy other than manual |
  | Metal Mine to level 3, Build a Corvette, Fly 6 sectors from home, Board a derelict, Win a fight | building level, ship lists, fleet distance, `SiteExplored`, a won `CombatEnded` |

  Order is free: the strip shows the first lesson not yet done. Notes follow the
  state too (no ore here, hold nearly full, which gates are quiet).
- **Who gets it.** It starts by itself only when the server issues a token in
  this session, that is for a new account. Anyone else turns it on under
  `?` > Guide, which starts it over against the current state. `Skip guide`
  turns it off. The record (`on|done lessons|harvesting fleet`) is kept per
  server and name in `ConsolePrefs`, so it survives a reload and a second
  account on the same browser is separate.
- **Token.** The new-account token is no longer written into the event log. The
  guide shows it with a copy button, and the alert entry keeps it for the
  session.

### Background notices (`console/attention.dart`, `console/notifier*.dart`)

- **What counts:** a queue order finishing, a queue running dry (inferred from
  the homeworld state, since the server has no `queue_idle` event), `RaidIncoming`,
  the raid coming within 60 ticks (inferred from the arrival tick; skipped when the
  announcement itself is that late, as at blitz pace), `RaidResolved`, a lost fleet,
  a `Critical` server alert, `StorageFull`.
- **While the page is in the background** each one adds to the tab title,
  `(2) IN AMBER CLAD`, and, if the player turned notifications on, is raised
  through the browser's `Notification` API. Back in front the title clears and
  the last four are shown as toasts. In front nothing new happens: the log and
  its toasts already report them. The raid-within-60 warning is written to the
  log, so it gets the alert toast and cue there.
- **Permission** is requested only inside a button press: the one-time offer
  toast after the first order starts ("Tell you when this finishes?"), or
  `?` > Notifications, which is also the off switch. The setting is per browser.
- **Title without frames.** A hidden page builds no frames, so the title is set
  with `SystemChrome.setApplicationSwitcherDescription` from the listener, not
  from a `Title` widget. Foreground and background come from Flutter's
  lifecycle (`visibilitychange`, focus and blur on web), so the only
  browser-only code is the `Notification` wrapper behind the stub/web import.

### Found on the way

Rendering the golden `full_state` in the whole shell (the new tests log in to a
fake server that sends the fixtures) showed two overflows the demo world never
hit: a waiting order with a long reason on the Overview queue cards, and a gate
card footer with a salvage clock and a derelict. Both wrap now.

### Not done, and why

- **Seeded safe contact and derelict** (spec 6, beats 4:00 and 4:30) need the
  server to guarantee them near a new homeworld (spec 8.5). They are first-hour
  goals instead, ticked when they happen.
- **Notices while the tab is closed** need a push service; **"while you were
  away" at login** needs an event backlog from the server (spec 8.5
  `events_since`). The missed-notice toasts only cover a tab left open.
- **Phones.** Android Chrome only allows notifications through a service
  worker, which the client does not have; there the tab title is all there is.

### Checked

- `test/onboarding_test.dart` logs a real `GameController` in to a WebSocket
  server that sends the golden frames (`test/support/fake_server.dart`, shared
  with the login tests) and drives the whole shell from token card to standing
  orders, then logs in again. `test/attention_test.dart` runs the demo world in
  the background through a raid, a ship and a building.
- Seen in headless Chromium against `iac-server --pace blitz`: two new accounts
  walked from the token to standing orders, progress kept over a reload, the
  phone width, the refused-permission message, and, with permission granted and
  the page hidden, `(1) IN AMBER CLAD` plus a `Notification` constructed for a
  finished build.
- Not seen: the notification as the operating system draws it (headless
  Chromium does not), a phone, a human playing through it.

## 2026-10-10: odds that count the ship classes

Owner decision (decisions.md, the hub entry): "Threat labels tell the truth
about the matchup."

**Root cause.** A ratio was fleet power over the sector's `est_power`, and power
is `weapon + (hull + shield) / 10` per ship. It knows nothing of rapid fire, of
shot size against shield and hull, or of who fires first. Measured with 2000
engine fights a cell (80 fleets against the group of every ring, placed in a
real lair and sent `attack`): of the old FAVOURABLE cells in rings 1 to 25 that
were not foregone, 35 percent kept less than the band says, down to 19 percent
of the hull and 47 percent wins; 34 percent of EVEN cells likewise, down to 1
percent wins. Four scouts read 1.83x at the ring-9 corvette (77 percent wins,
41 percent of the hull) and 1.50x at ring 10 (47 percent wins), so a default
doctrine flew them in. It erred the other way too: one corvette against the two
ring-8 scouts read 1.37x EVEN and kept 98 percent.

**The rule.** `iac_sim::odds::outlook` plays the fight on averages under the
rules of `resolve_combat_round`: ships summed by class, the fleet firing first,
rapid fire as burst length, each ship's hits as a count with the right mean and
spread, shields before hull, a volley in two halves. It returns the share of
hull each side is expected to keep. `scaling::outcome_ratio` reads the ratio
from that with a table measured between equal ship classes (1.5 is a fight that
keeps 86 percent, 2.5 one that keeps 99), so the bands keep their numbers and
mean one outcome whatever the classes. A closed form on damage times hit points
(the square law with rapid fire) was tried first: it fixed who wins but still
called FAVOURABLE fights that kept 70 percent, because big guns on small ships
skip the shields; the outcome has to be played, not ratioed.

**One number.** `GameEngine::fleet_odds` is the only place a ratio is made: the
fleet's ships against the ships in sight, the groups remembered at the power
last seen, or the group the ring spawns. `preview_move`, the new `odds` command
(a fleet and up to 256 sectors anywhere), the doctrine entry check, the
patrol's engage test and the derelict guard test call it. The 25 percent an
observed Aggressive or Swarm group adds stays in `est_power` and the rating and
is not in the ratio. The ring estimate follows the 32-ship limit past ring 36.

**After**, same grid, rings 1 to 25, unscouted estimate (the labels in sight
land in the same bands, for every behaviour): SAFE 276 cells, hull kept 98 to 100 percent; FAVOURABLE
192, all won, 83 to 100; EVEN 113, 95 to 100 percent wins, 52 to 88; RISKY 90,
0 to 100 percent wins, 0 to 57; DEADLY 131, 0 percent wins at the median and 31
at most (two cells above one in six). No cell promises more than its band
within 3 points of hull; rings 26 to 40 the same. Expected hull kept is off by
0.7 points on average, 2 at the 90th percentile, 20 at worst (close fights,
where both sides end near nothing). With default orders no scout fleet of 2 to
6 enters rings 9 to 12; 8 scouts enter rings 9 and 10 and win every fight with
89 to 93 percent. 24 empires of 6 scouts on default `prospect` and
`patrol_home` for 20,000 ticks: 11 fleets lost before, none after.

**Clients.** Web: every ratio is the server's (`GameState.oddsFor`, fed by
`odds` and previews); `RatioInfo.of`, the route planner's powers and the
per-label win percentages are gone; the defence tab shows plain cover instead
of fleet bands. `play.py`: `status`, `sector`, `map` ask for odds, `play odds`
added. TUI prints an `odds` reply. The demo world keeps a power stand-in for
its own made-up answers.

**Cost.** One outlook is about 2 microseconds (11 at worst), memoised by the
two sides in the engine. 24 fleets on standing orders, where behaviour did not
change (2 scouts each): 385 to 416 microseconds a tick, 8 percent. That cost
was and is the route search, which rebuilds the player's live-sector list for
every sector it tests (`known_opposition`); left alone here.

**Still not true.**
- The ratio is the expected result, not its spread. Rapid fire comes in bursts:
  in the 20,000-tick run one corvette fired 26 times in a round (about 1 in
  25,000) and took 5 of 12 scouts from a fleet rated 1.61x.
- A fleet heading home falls back to the shortest lane when no safe one
  exists, whatever the odds; that fleet died on such a hop.
- The ratio rates one fleet against everything in the sector. Other fleets
  that would join the fight are not counted, nor a patrol that walks in later.
- A remembered group is rebuilt from its classes and the power last seen, so a
  damaged group is taken as a full one of that power.
- Above 4x the scale only says "no hull lost"; it tops out at 10.
- The demo world's odds are power against power.

**Not done.** `CLAUDE.md`'s layout table does not list `sim/src/odds.rs`.
Measurement code is in the session scratchpad (`labels/`), not in the repo.

## 2026-10-10: bots seated in the server, tagged BOT, kept through a restart

Roadmap step 2, the server half. No balance constant was changed.

**Hosting.** `iac-server --bot <personality>:<attention>[:<count>]` (the `sim-run`
seat syntax, repeatable). `bot::Roster` holds the seats beside the engine;
`run_tick` calls `take_turns` before `tick` and `observe` after it. A bot's orders go
through `GameEngine::execute` and `handle_policy_update` as a client's do. `--bot`
states what the world should have: missing bots are added at start, existing ones
play on whether listed or not. `--player-seats <N>` (default 4) keeps home seats for
people: a bot is added only while more than N of the 24 are free; a roster that asks
for more is seated in the order given as far as that goes and the log says what was
left out. `--bot-sessions` sets the look-ins of new check-in and hybrid bots.
`sim-run` refuses more than 24 seats before it starts.

**Identity: the name is the tag.** Bot accounts are named `BOT-<personality>-<n>`,
and `BOT-` (any case) is refused for everyone else with `InvalidName`. Every place
that names a player to another player already carries the name, so nothing else on
the wire had to change for the tag: fleets in a sector, events, the board. The
leaderboard reply gained `bots` (omitted when empty): bots are never in `entries`,
they are ranked among themselves, and `you` is a bot's own row when a bot asks. LLM
agents stay in `entries` with their `agent` flag. Fixtures: `leaderboard__with_bots`
added, `enums.json` and `full_state.json` changed.
- TUI: a "BUILT-IN BOTS" block under the ranking; `chart:` in the sector readout.
- Web: bots under the ranking with a BOT tag and `---` for a rank; a log line counts
  them; the sector panel shows the chart mark.
- `play.py`: `score` lists bots under the table, `map` and `sector` show the mark.

**Chart marks replace the bot's logbook.** `SectorState.chart` is `aboard` (the chart
rides with one of the viewer's fleets) or `delivered` (banked), absent otherwise. It
is laid over the chart per viewer when a view is built, and a remembered sector is
sent once more when its mark moves (charts delivered, a carrier lost). A human sees
which sectors still pay and which fleet must come home; a bot no longer keeps a list
of its own, so what it must remember shrank to a few numbers.

**Restart.** A bot is an account plus a `BotRecord` in the engine: style (personality,
attention, look-ins, cap rule) and memory (end of the first look-in, next wake and
plan ticks, its own board row as last asked, the lanes its fleets are walking). The
record is in `Snapshot` and goes to SQLite as a `runtime_state` row of kind `bot`,
written when a turn changes it. The view a seat holds is not stored: a restarted bot
starts from the full state as a reconnecting client does, and that state is the one
it would have folded. Checked three ways: a world of bots resumed from a JSON
snapshot ends on the same hash as one never stopped (`sim/tests/bots.rs`); the same
through SQLite, six restarts in a row (`server/src/database.rs`); the real binary
killed and restarted (`server/tests/scenarios.rs`).

**A view bug the restart work found.** A fleet that loses its last ship lingers one
tick before it is reaped. The full state dropped its charts' marks at once, tick
updates a tick later, so a client that folds updates and one that asks for the full
state disagreed for that tick, and a bot acting on it would have left the path of an
unrestarted one. Both now change at the reap. Found by folding updates for 20 bots
over 9000 blitz ticks and comparing with the full state every tick (180 000 views,
0 differ after the fix; one did before, at tick 5382). The scenario in `bots.rs`
compares every view a seat holds, on a smaller world.

**Cost: the incremental view is not the cheaper one.** The step-2 seat built a full
state for every turn. Folding `tick_update` instead was the plan, and measured it
loses: an update must be taken every tick, a full state only on a turn, and an
update for an active account already costs about 40% of the full state (fleets with
their way home, the homeworld block and live sectors are in both; the chart copy is
the rest: at hour 6 of a blitz world 116 us against 275 us with 380 sectors charted).
So a seat now follows by updates only while its player acts again within two ticks
(`ScriptedPlayer::watching`) and starts from a full state otherwise. Whole-world mean
per tick, 20 bots and 3 idle accounts, release build, world before the merge:

| roster, pace, length | full state per turn | update every tick while present | update only when the next turn is near (kept) |
|---|---|---|---|
| 5 styles x (hands-on, hybrid, 2 check-in), blitz, 6 h | 3.49 ms | 3.47 ms | 3.35 ms |
| 20 hands-on, blitz, 6 h | 9.84 ms | 11.89 ms | 10.63 ms |
| the mixed 20, season, 2 days | 0.59 ms | 0.72 ms | 0.52 ms |
| 20 hands-on, season, 2 days | 0.82 ms | 1.41 ms | 0.87 ms |

The runs are not the same games (bot turns differ by up to 5%); per turn the kept
rule is within 4% of the full-state seat for all-hands-on worlds and 4 to 11% under
it for mixed ones. Split of the blitz mixed world: engine tick 1.3 ms, bot turns
0.9 ms, following 1.2 ms; worst single tick 18 to 43 ms. The live servers (below)
ran 1.9 to 5.3 ms a tick on average with 20 bots and three clients, 15 ms at worst:
under 2% of the second. The lever left is the view itself, for bots and people
alike: 100 to 300 us per active account per tick late in a world.

**World full.** `AuthError::Server` went with master's home seats; a bot without a
seat is refused `WorldFull` like anyone. See `--player-seats` above.

**Live check** (release, own databases, ports 7833 and 7834, three headless clients
each, one of them flying toward the nearest bot home and scanning). Blitz and season
worlds asked for 23 and 20 bots: 20 seated in each, 4 seats kept, the blitz log
naming the 3 left out. The clients saw `BOT-` fleets in scanned sectors (7 and 6
different bots), `bots` on the board with 20 rows, themselves ranked 1 to 3, and
`chart: aboard` on the sector they had entered. Blitz was killed (SIGKILL) at tick
626 and started with no `--bot`; season stopped (SIGTERM) and started asking for
more bots than fit: both resumed 20 bots, added none, and scores went on (blitz: 12
bots higher after 330 ticks, 2 lower by 0.1, the other check-in ones waiting for
their next look-in; season, where a building takes minutes: 1 higher, none lower).

**Attention is in ticks.** A check-in bot looks in four times a day of ticks at any
pace, so in a blitz hour it plays its first five minutes and no more. Short worlds
want hands-on bots.

**Odds from the server.** After the odds merge a bot keeps no power arithmetic. A
turn is now questions, then orders (`ScriptedPlayer::ask`): the seat sends the
questions (`odds`, `preview_move`, `leaderboard` only) and the answers reach `hear`
before `act`, as a client gets its replies within the tick. The pilot asks `odds`
for each fleet it will fly against the sectors within 12 hops where the answer
settles something (a fight on entering, prey, a derelict that may be guarded) and
for home; a guarded sector with no answer is closed to it. Thresholds unchanged: 3.0
to fight or enter, 1.5 and 3.0 on the doctrines. Gone: `fleet.power >= k x
est_power` and the 8x rule against classes with rapid fire on the fleet, which the
server's ratio already counts. A risky derelict is boarded when its sector reads
favourable (1.5); the old test was power against the guard's power, a number no
client is given. Answers live for one turn, so the stored record did not grow.
Asking everything every turn cost 8.5 ms a tick in the 20-bot blitz world; asking
only for fleets about to fly, within the horizon, 1.3 ms (0.9 before odds). The
final build ran live at 2.2 to 3.0 ms a tick on average, 9 ms at worst (20 bots,
three clients, blitz), was killed at tick 654 and came back with its 20 bots, all 20
higher by tick 851. Folded views against full states on that build: 180 000
compared, 0 differ.

### The season week again (7 days, seeds 1 to 3, the same 16 seats)

Four runs, to keep causes apart. A: the first report (world before home seats, bots
of step 2). B: the same world, bots as they were before the odds merge (chart marks,
prompt re-planning). C: world generation 3, the same bots. D: master with odds, bots
on the server's ratio. One bot per style and three seeds: the ranges are wide, and a
difference inside them is not a finding.

Day 7, mean (min-max over seeds), D:

| style | total | core | combat | explore | cap filled | haul | raid salvage | ships lost |
|---|---|---|---|---|---|---|---|---|
| turtle / check-in | 1755 (1556-1874) | 1755 | 0 | 0 | 0% | 0 | 232 | 0 |
| miner / check-in | 1659 (1509-1784) | 1658 | 0 | 1 | 0% | 0 | 251 | 1 |
| raider / check-in | 1743 (1509-1917) | 1630 | 3 | 110 | 28% | 8 | 227 | 3 |
| explorer / check-in | 2020 (1887-2198) | 1751 | 5 | 264 | 61% | 11 | 245 | 9 |
| mixed / check-in | 1905 (1722-2102) | 1649 | 7 | 249 | 62% | 13 | 256 | 48 |
| turtle / hybrid | 1876 (1817-1980) | 1876 | 0 | 0 | 0% | 0 | 299 | 0 |
| miner / hybrid | 2649 (2409-2795) | 2120 | 39 | 968 | 100% | 235 | 297 | 5 |
| raider / hybrid | 1916 (1507-2163) | 1533 | 21 | 733 | 100% | 56 | 241 | 8 |
| explorer / hybrid | 2134 (2083-2188) | 1707 | 27 | 833 | 100% | 73 | 281 | 1 |
| mixed / hybrid | 2099 (1991-2164) | 1679 | 18 | 759 | 100% | 56 | 290 | 3 |
| turtle / hands-on | 2400 (2400-2400) | 2400 | 0 | 0 | 0% | 0 | 373 | 0 |
| miner / hands-on | 4232 (4215-4242) | 3386 | 155 | 1538 | 100% | 901 | 537 | 19 |
| raider / hands-on | 2982 (2918-3017) | 2386 | 105 | 1681 | 100% | 383 | 286 | 9 |
| explorer / hands-on | 3664 (3114-4048) | 2931 | 162 | 2464 | 100% | 658 | 434 | 3 |
| mixed / hands-on | 3165 (3116-3260) | 2532 | 149 | 2249 | 100% | 647 | 388 | 13 |

Totals across the four runs (A is the table in the first report):

| style | A | B | C | D |
|---|---|---|---|---|
| turtle, all three attentions | 1756 / 1876 / 2400 | the same | the same | the same |
| miner / check-in | 1694 | 1711 | 1659 | 1659 |
| raider / check-in | 1634 | 1743 | 1797 | 1743 |
| explorer / check-in | 1953 | 1912 | 2087 | 2020 |
| mixed / check-in | 1675 | 1670 | 1773 | 1905 |
| miner / hybrid | 2751 | 2713 | 2552 | 2649 |
| raider / hybrid | 1977 | 2051 | 2038 | 1916 |
| explorer / hybrid | 2221 | 2203 | 2180 | 2134 |
| mixed / hybrid | 2221 | 2247 | 2150 | 2099 |
| miner / hands-on | 3919 | 4418 | 4027 | 4232 |
| raider / hands-on | 3001 | 3093 | 2966 | 2982 |
| explorer / hands-on | 3132 | 2840 | 3208 | 3664 |
| mixed / hands-on | 3140 | 3277 | 3233 | 3165 |

What moved, and what did not:

- *Builders: nothing.* The turtle rows are the same within a point in all four runs.
  Neither the world nor the odds touch an empire that keeps its ships home.
- *Ships lost by pilots.* The largest change, and it is the odds. Hands-on miner
  166 (B), 100 (C), 19 (D); raider 74, 87, 9; mixed 58, 65, 13. A ratio of 3.0 on
  the server's scale is a fight without lasting damage; 3.0 in power was not. The
  pilots' combat points fell with it (raider 215, 216, 105; mixed 183, 269, 149)
  and the cores did not suffer.
- *Delegated fleets against piloted ones* (check-in as a share of hands-on, same
  personality, day 7; A, C, D). Charts delivered: raider 4%, 11%, 12%; explorer
  18%, 26%, 24%; mixed 10%, 16%, 27%; miner 0. Haul: 0 to 6%, 0 to 7%, 0 to 2%.
  Score that counted: explorer 32%, 50%, 37%; raider 7%, 22%, 19%; mixed 13%, 20%,
  40%; miner 0. Better than the first report: for raider and explorer the gain came
  with world 3 (B: 10% and 35%), for mixed with the odds (B 19%, C 20%, D 40%). Still
  short of the 50 to 70% target everywhere. `mine_and_return` still brings home next
  to nothing (haul 0 in every run).
- *Explorers under orders hold at the ring-9 line.* Explorer / check-in from C to
  D: ships lost 32 to 9, explore 331 to 264, haul 36 to 11. Fewer scouts die and
  fewer far charts come home.
- *Check-in against hands-on by day* (total, D; d1 to d7): turtle 40, 50, 48, 57,
  68, 81, 73%; miner 17, 15, 20, 29, 32, 33, 39%; raider 39, 34, 39, 47, 56, 62,
  58%; explorer 31, 34, 39, 40, 43, 51, 56%; mixed 40, 48, 57, 59, 60, 59, 60%.
  Day 7 in A: 73, 43, 55, 62, 54. The explorer's share fell because the hands-on
  explorer gained (3132 to 3664), not because the check-in one lost. Hybrid on day 7,
  D: 78, 63, 64, 59, 66% (A: 78, 70, 66, 71, 71).
- *Cap fill by style* (raw combat plus explore over the cap, day 7; A, D). Check-in:
  miner 0.00x, 0.00x; raider 0.10x, 0.27x; explorer 0.46x, 0.61x (above 1x on days
  2 to 4 in D); mixed 0.20x, 0.63x. Hybrid 1.85 to 2.45x, 1.85 to 2.03x. Hands-on
  2.4 to 4.4x, 2.0 to 3.8x. The cap still binds every pilot all week and no fleet
  under orders at the end of it.
- *`patrol_home`* (the strike wing of check-in raiders and mixed; A, C, D). Raider:
  combat points 0.7, 2.2, 2.7 (D: 0.1 to 5.8); kills 84, 58, 62 (D: 7 to 135);
  salvage 1.2, 9.2, 3.9. Mixed: combat 2.0, 4.6, 7.0; kills 74, 115, 148; salvage
  4.2, 4.0, 8.9. Hunting the convoys beside home is worth 3 to 7 combat points and 4
  to 9 points of salvage a week to an empire with 200 to 250 points of ships, against
  a cap near 410: about 0.05 points a kill. The mixed check-in empire loses 48 ships a week (21 to 86) over all its wings, as it
  did before (A 26, B 46, C 42).
- *Personality against the turtle, day 7, D.* Check-in: miner 95%, raider 100%,
  explorer 116%, mixed 109%. Hands-on: miner 176%, raider 124%, explorer 153%, mixed
  132%. The hands-on miner and explorer are far outside the 15% band; both are
  haul and capped bonus on a core 22 to 41% above the turtle's.

Not rerun: the blitz hour table, the look-ins-per-day sweep and the persistent-pace
check of the first report.

## 2026-10-10: single player in one browser tab

Roadmap step 3. No rule, balance constant or wire message was changed.

```
page (Flutter client, unchanged above the transport)
  GameController -- Uplink --+-- WebSocket ----------------> iac-server
                             +-- postMessage, same JSON ---> Web Worker
                                                               solo/web/worker.js   clock pump, IndexedDB, one-tab lock
                                                               iac_solo.wasm        iac-solo: Machine > World > iac-sim + bot::Roster
```

**The host is a crate, `solo/` (`iac-solo`).** One world, its roster, one commander.
Its whole surface is `Machine::call(bytes) -> bytes`: a line of JSON naming the
operation, a newline, a body; the reply likewise. Frames in the bodies are the wire
protocol, one per line. As WebAssembly that is three exports (`iac_request(len)`,
`iac_call()`, `iac_reply()`) and one import for panic text. No wasm-bindgen: with
strings across the boundary it would have added a CLI to install and keep in step
for nothing. The tests drive the same function natively.

**Same answers as the server by construction.** `GameEngine::answer(pid, msg)` now
words everything a logged-in session is sent, and `server/network.rs` calls it too
(its three in-process tests go through `handle_message`). A tick is the server's:
bots take turns, `tick`, the session's `tick_update`, bots observe. Two differences,
both stated in `world.rs`: no token (the world knows its commander; `auth` with that
name logs in, any other name is `InvalidName`), and the clock.

**The clock is the caller's.** Every request carries `now_ms`. Tick `t` is due at
`anchor_ms + (t - anchor_tick)` seconds; the anchor is in the save. Up to 5 ticks
behind are played one frame each. More is an absence, whether the tab was closed or
the machine slept with it open: the missed ticks are played without frames, bots and
standing orders acting on every one, so the result is the world an open tab would
have reached. Limit 7 days per absence; the player may take command early; in both
cases the rest is dropped and the clock re-anchored. A catch-up of 300 ticks or more
ends in `Alert` events (kept in the save until a session has had them): how long,
how much of it was played, buildings, research, ships and defences finished, raids
repelled and lost with what they took, fleets and ships lost. The page shows the same
sentences before the player enters. A hands-on bot is hands-on through the absence
too, which the new-game form says.

**Worker, by measurement.** Under node, back to back, a live tick one hour into a
blitz world costs 3.8 ms median with 8 hands-on bots and 13 ms (24 at worst) with
20, and one 500-tick slice of a catch-up took 4.7 s. In the browser worker, where a
tick comes once a second on a cold core of a shared machine: 2 to 10 ms mean in the
first minute of the blitz world with 8 hands-on bots (worst single tick 24 to 100
ms), and two hours in, which is late for blitz, 29 ms mean and 65 ms at worst. None
of that fits beside a 16 ms frame, so the engine never runs on the page's thread;
on its own thread 29 ms is 3% of the second. The worker sizes each catch-up call to
about 30 ms and yields in between, so `take_command` and `save` get in; the page
drew every 50 to 67 ms at worst while a two-hour absence was played.

**Catch-up speed** (V8 under node, this machine, niced; a tick is a second of the
world):

| world | absence | time | ticks/s | save |
|---|---|---|---|---|
| persistent, 8 check-in bots, new | 7 days | 11 s | 57,000 | 0.07 MB |
| season, 20 check-in, new | 8 hours | 0.9 s | 31,000 | 0.05 MB |
| season, 20 check-in, new | 2 days | 58 s | 3,000 | 1.1 MB |
| season, 8 check-in, new | 7 days | 5.5 min | 1,850 | 1.3 MB |
| season, 20 check-in, new | 7 days | 17 min | 600, 190 at the end | 4.6 MB |
| blitz, 8 hands-on, new | 1 hour | 5.7 s | 640 | 1.8 MB |
| blitz, 8 hands-on, new | 3 hours | 42 s | 254, 180 at the end | 3.4 MB |
| blitz, 20 hands-on, new | 1 hour | 17 s | 216 | 5.4 MB |

In the browser worker the blitz world with 8 hands-on bots caught up two hours at
330 to 390 ticks/s, as under node. So the owner's two cases: a week away at persistent pace is seconds; eight
hours at season pace is a second in a young world and up to two minutes in a
week-old one with 20 bots. Hence progress, rate and time left on screen, a way out
(take command now), 8 check-in bots as the default, and the 7-day limit, which is
where the wait reaches minutes even for the modest roster. Fat LTO with one codegen
unit made the wasm 11% smaller and 3% slower: left out.

**Saves are whole snapshots, written rarely.** One document: a header line (format,
economy and worldgen versions, commander, clock, an interrupted catch-up, unread
report, xxh3 of the rest) and `Snapshot` as JSON; 81% of it is the charts
(`known`). Gzipped in the worker (1.07 MB to 80 kB), one `put` in one IndexedDB
transaction, so a tab killed mid-save keeps the one before. The first cut saved
every 15 s, which for a week-old world is 26 GB a day of writes. But a lost save
costs only orders: the ticks since are replayed on reload and come out the same
(`a_reload_plays_the_unsaved_ticks_again...` compares the snapshots byte for byte).
So: within 2 s of an order or policy change (not of a query; `commands::is_query`),
every 4 minutes otherwise (under the 300-tick report threshold, so a reload is never
taken for an absence), every 20 s while catching up, and when the page is hidden.
Another format, economy or worldgen version, a failed digest or an unreadable
snapshot is refused with a sentence, the save left in place until a new game
replaces it. The row-by-row `PersistBatch` path was not used: it needs a second
copy of the SQLite loader for 80 kB a save.

**Client.** `GameConnection` connects over a `Dial` (any `StreamChannel` of text
frames) and `GameController` logs in to an `Uplink`. `lib/solo/` is the worker's
other end; `SoloScreen` is the front door of the bundle built with
`IAC_SINGLE_PLAYER` (`scripts/play.sh --single-player`, which also builds the wasm,
copies it and the worker in, and turns Flutter's CDN off). The operator guide starts
for a new commander as it does for a new account, minus the token step.

**The demo world is out of the app.** It was a second, drifting copy of the rules in
Dart. Offline play is now the real engine, so `LinkState.demo`, the fall-back to it
and the Offline demo button are gone; an unreachable server says so and keeps
trying. The file moved to `test/support/demo_world.dart`: the widget tests run on
the Dart VM, which cannot run the engine, and they want its fixed script (the raid
at tick 279). It reaches the controller through `GameController.receive`.

### Checked

- `cargo test -p iac-solo`: four scenarios through `Machine::call`.
- `clients/web/tool/solo_e2e.dart`, one command, 137 s, 30 checks: the bundle from
  a plain static host with every other host unreachable, in headless Chromium with
  real clicks and keys and a tap on the worker's frames. New game at blitz with 8
  bots; homeworld and guide on screen; `build metal` typed in the palette, Metal
  Mine level 2 completed; 8 `BOT-` rows on the board and a bot's score rising; a
  second tab kept out; reload finds the save and replays to the same homeworld; a
  two-hour absence (the worker's `Date.now` is shifted) caught up, reported on
  screen and as `Alert` frames; a 30-minute stall with the page open; abandon;
  then the same bundle served by a real `iac-server` logs in over a WebSocket and
  leaves the worker alone. Run on Flutter's wasm build and, with `--software`, on
  its JavaScript fallback.
- First load on localhost, cold cache, wasm client: 20 files, 9.3 MB (3.5 MB
  gzipped), of which the engine is 2.1 MB (0.57 MB gzipped); first frame after 0.44
  to 0.64 s, engine ready after 0.74 to 1.05 s, new-game form after 2.4 to 2.7 s
  (the boot animation). JavaScript fallback: 19 files, 11.7 MB; 1.0 s, 1.1 s, 3.1 s.
  The bundle directory is 47 MB because Flutter ships every renderer.

### Found on the way

- The Flutter engine asks `fonts.gstatic.com` for Roboto at start, in every build
  of the client. Unreachable, nothing changes on screen. A `Roboto` family in
  `pubspec.yaml` would stop the request; left alone because it touches the
  multiplayer client's fonts.
- Headless Chromium here waits 25 s before its first request of a run (not the
  page: any URL). The check spends that on `version.json` and times the page after.
- Engine speed is the single-player limit, not the browser: 1 to 5 ms a tick in a
  busy world, most of it bot turns and the views they are built from.

### Not done, and why

- **Export and import of a save**, and a second save slot: one world per browser
  was the brief.
- **An economy-only fast path for long absences** would be a second set of rules.
- **A panic in the engine** reaches the page as "the engine stopped" with the
  panic text (the hook and the import are there); no way was found to provoke one
  for a test.

### Not seen

A human playing it. A browser other than Chromium 154. A phone. Storage refused or
full. A real closed tab overnight (the clock was shifted instead). The season and
persistent rows of the table in a browser (node only).

## 2026-10-10: standing orders that do their jobs, and a fuel budget

Owner target (decisions.md, play styles): delegated exploring, salvage and mining
earn roughly 50 to 70% of hands-on piloting. Before this a check-in empire delivered
12 to 27% of a pilot's charts, hauled 0 to 2% and counted 0 to 40% of the field
score. No cost, time, production, loot, point value, cap, ship stat, combat rule, NPC
group or odds was changed. Branch `worktree-agent-a1f425bf64f31a5f0`, not merged.

**How it was found.** A scratch runner seats bots on the real engine and counts every
`PolicyAction` by doctrine, action and reason, and fleet-time by doctrine, place and
state. Three days of four check-in empires before the work: 64 to 85% of fleet-time
docked with no orders at all, most of the rest holding.

**Doctrine defects (engine), each with a scenario in `sim/tests/`.**
- *`mine_and_return` shuttled between stripped seams.* A tile that had refilled by a
  fraction of a unit counted as ore: 9012 harvests and 8824 jumps in three days for
  0.5 points. A sector under 3 units is stripped (`POLICY_MIN_SEAM_UNITS`); a seam or
  a wreck is flown to only if what the hold can take is worth 1.5 times the deuterium
  of the jumps there and back at score weight (`POLICY_FUEL_PAYBACK`); the next seam
  is staked from where the fleet stands.
- *A target behind a refused sector pinned the fleet.* Targets were the nearest by
  lane from home whatever the odds on the way; the fleet then refused the way once a
  minute for good (one salvage fleet: 2408 refusals in three days); a miner whose
  nearest ore lay behind one never left the dock. One view per evaluation (`Reach`: the fleet's
  odds per sector, the groups on the map, free hold, price of a jump) searches through
  the sectors the doctrine may enter; when nothing is left the hold names the nearest
  target it could not have and what stops it.
- *Fights it need not take.* Hull is never repaired. A lane with no fight on it is
  taken before a shorter one with a fight the fleet would win, and `mine_and_return`
  and `salvage_and_sites` pick no target among hostiles that attack on sight. A fight
  on the only lane is still taken at the engage ratio: the entry rule is unchanged.
- *Shields.* They return after ten idle ticks; a doctrine acted every three, so after
  its first fight a fleet flew the rest of the sortie with shields down. It waits for
  95% (`POLICY_SHIELDS_READY`).
- *`patrol_home`* walked a random beat within two hops for ever (it finds contacts
  without looking), ignored `max_range`, left its wrecks and went home to lick wounds
  docking does not heal. It hunts the nearest group inside `max_range` that it rates
  at its ratio, scoops the wreck, waits at the dock. `POLICY_PATROL_RADIUS` and
  `POLICY_PATROL_MIN_HULL` are gone.
- *The way home* fell back to the shortest lane whatever the odds. Now: a lane at the
  engage ratio, else the mildest down to RISKY and the reason says so, never DEADLY:
  the fleet holds ("cut off from home") and its owner gets a Warning alert.
- *A full store.* Cargo the stockpile could not take stayed aboard, and the fleet
  flew out and straight back with it for as long as the store was full. It stays
  docked until it has unloaded.
- *`prospect`* looked for sectors never entered; a chart lost with its carrier was
  never fetched again. It looks for charts neither banked nor aboard a fleet.

Five of the new scenarios were checked the other way round: with the shield wait, the
unload wait, the quiet-lane preference and the fuel payback switched off they fail.

**Fuel budget (protocol).** `PolicyParams.fuel_budget_pct`, 0 to 100, default 20, 100
lifts the bound. The fleet earns an allowance at that share of the current deuterium
production and saves up to one tank's price. The tank is filled in one go when
allowance and stockpile cover it; until then the fleet holds at the dock and says how
long; it does not leave on orders with a part tank. A full deuterium store fills it
free of the allowance. A fleet without orders refuels as before. Two numbers on the
policy (credit, tick settled), nothing per tick; in `Snapshot` and `fleet_policies`
(`fuel_budget_pct`, `fuel_credit`, `fuel_since`; rows from before load as 100). The
restart test through SQLite runs with budgets and ends on the unhosted world's hash.
Fixtures regenerated; Dart mirror; web `policy mine range=8 fuel=40 ...`; TUI `P`;
`play.py` help and hold grouping; RULES and README.
- *The allowance starts empty.* A first version gave one tank with the first orders.
  It is paid from the 100 deuterium an empire starts with: in one of twelve check-in
  field seats (raider, seed 3) the 8 deuterium it took left the stock short of the
  next order with no income to make it up, and the empire sat at 2.5 points all week
  with full stores. That is the bot planner's deadlock (an order waiting on deuterium
  and no synthesizer queued), but the budget exists to keep fuel away from it.

**Bots (what was the bot's fault).**
- Their own fuel rule (delegate only while half the deuterium production covers a
  jump every four ticks) is removed: it stood in for the missing bound and was why
  most wings sat docked without orders. Budgets: 50% for doctrines that haul, 20 for
  those that fly for points.
- `max_range` 8 for mining; now 30 for every wing (odds and fuel are the leash).
- A doctrine out of work held until the map changed. A fleet docked with a full tank
  as a look-in ends gets its wing's other doctrine (mine and salvage swap; survey and
  strike fall back to salvage) and swaps back when that runs dry.
- Every doctrine but the patrol flew at 1.5 whatever the fleet. The ratio is the odds
  at which a doctrine fights where it has to. `prospect` has to (a lair's chart is in
  the lair) and a wing with warships rates 1.5 where every lane has a fight: at 1.5
  the explorer's recovery wing lost 27 to 66 ships a week, and with the cap ignored
  the survey wings lost 463 and 340. Those two now fly at 2.5 (SAFE) once they have
  four ships; hauling wings of scouts and haulers pick no fight for a payday and keep
  1.5: at 2.5 they lost a fifth to three quarters of their haul to save one to five
  ships a week.
- Not changed: wings that fly for points stay home once the cap is banked, as a
  pilot's do, so a check-in empire's charts and kills stop where its cap is.

**Measured.** Season, 7 days, seeds 1 to 3, the same 16 seats, before (master with the
refuelling count added to `sim-run`) and after. Mean (min-max over seeds).

| style | total | core | combat | explore | haul | charts | ships lost | refuel, k deuterium |
|---|---|---|---|---|---|---|---|---|
| miner / check-in, before | 1659 (1509-1784) | 1658 | 0 | 1 | 0 | 3 | 1 | 29 (7-56) |
| miner / check-in, after | 2228 (2022-2332) | 1803 | 5 | 432 | 107 (89-120) | 404 (362-458) | 5 | 14 (11-16) |
| raider / check-in, before | 1743 (1509-1917) | 1630 | 3 | 110 | 8 | 164 (37-243) | 3 | 9 (4-16) |
| raider / check-in, after | 1929 (1702-2234) | 1543 | 6 | 520 | 91 (81-105) | 545 (498-636) | 3 | 18 (13-21) |
| explorer / check-in, before | 2020 (1887-2198) | 1751 | 5 | 264 | 11 | 366 (330-407) | 9 | 9 (8-10) |
| explorer / check-in, after | 2187 (1933-2354) | 1749 | 15 | 759 | 121 (83-149) | 692 (513-1034) | 1 | 24 (20-32) |
| mixed / check-in, before | 1905 (1722-2102) | 1649 | 7 | 249 | 13 | 401 (385-424) | 48 (21-86) | 13 (8-16) |
| mixed / check-in, after | 2181 (2012-2349) | 1745 | 7 | 540 | 83 (39-154) | 669 (619-739) | 3 | 24 (20-29) |
| miner / hands-on, before | 4232 (4215-4242) | 3386 | 155 | 1538 | 901 | 1236 | 19 | 333 |
| miner / hands-on, after | 4026 (3912-4250) | 3221 | 161 | 1485 | 823 | 1204 | 15 | 307 |
| raider / hands-on, before | 2982 (2918-3017) | 2386 | 105 | 1681 | 383 | 1365 | 9 | 58 |
| raider / hands-on, after | 2926 (2861-3002) | 2341 | 100 | 1518 | 314 | 1286 | 13 | 58 |
| explorer / hands-on, before | 3664 (3114-4048) | 2931 | 162 | 2464 | 658 | 1514 | 3 | 98 |
| explorer / hands-on, after | 3406 (3029-3701) | 2724 | 106 | 1895 | 334 | 1277 | 7 | 45 |
| mixed / hands-on, before | 3165 (3116-3260) | 2532 | 149 | 2249 | 647 | 1479 | 13 | 92 |
| mixed / hands-on, after | 3086 (2889-3255) | 2469 | 165 | 2206 | 555 | 1488 | 9 | 90 |

Turtles are the same to the point in both (1755, 1876, 2400). Hybrids, total: miner
2649 to 2893, raider 1916 to 2225, explorer 2134 to 2389, mixed 2099 to 2160.

Delegated as a share of piloted, same personality, day 7, before to after:

| | charts | haul | combat | counted score | total |
|---|---|---|---|---|---|
| miner | 0 to 34% (30-40) | 0 to 13% (11-15) | 0 to 3% | 0 to 53% (47-56) | 39 to 55% (52-60) |
| raider | 12 to 42% (38-51) | 2 to 29% (26-33) | 3 to 6% | 19 to 66% (57-77) | 58 to 66% (57-77) |
| explorer | 25 to 54% (38-77) | 2 to 37% (22-48) | 3 to 15% | 37 to 65% (55-75) | 56 to 65% (55-75) |
| mixed | 27 to 45% (41-47) | 2 to 14% (7-23) | 5 to 4% | 40 to 71% (65-76) | 60 to 71% (65-76) |

Check-in against hands-on by day (total, d1 to d7), before then after: miner 17, 15,
20, 29, 32, 33, 39 then 25, 22, 31, 43, 50, 51, 55%; raider 39, 34, 39, 47, 56, 62, 58
then 56, 44, 49, 60, 67, 72, 66%; explorer 31, 34, 39, 40, 43, 51, 56 then 37, 36, 41,
40, 51, 60, 65%; mixed 40, 48, 57, 59, 60, 59, 60 then 39, 45, 60, 64, 66, 70, 71%;
turtle 40, 50, 48, 57, 68, 81, 73 both times. Hybrid on day 7: 63, 64, 59, 66 then 72,
76, 70, 71%. Against the turtle, check-in, day 7: miner 95 to 127%, raider 100 to
112%, explorer 116 to 125%, mixed 109 to 125%.

What is noise: one bot per style and three seeds. A hands-on explorer ranges 3029 to
3701 on its own and a check-in raider 1702 to 2234. The charts, haul and counted
columns moved clear of their ranges for every style. The combat column and the
raider's and explorer's totals moved inside them and are not findings.

**What it says about the target.**
- *Counted score* (what combat and exploration add after the cap): 53 to 71%, from
  0 to 40. Every check-in empire now fills its cap (miner 94%, the rest 100%), so
  this share is the ratio of the cores and cannot rise further by doctrine.
- *Charts*: 34 to 54%, from 0 to 27. The cap sets it: survey and strike wings stay
  home once it is banked. With the cap ignored (`--ignore-cap`, seed 1, eight seats)
  delegated charts are 42, 74, 63 and 96% of piloted (before: 0, 18, 43, 35).
- *Haul*: 13 to 37%, from 0 to 2. Short of the band. A pilot's haul is the wreckage
  of what it kills and what it boards (per week: 1100 to 4000 kills, 13 to 28
  boardings; wrecks 215 to 355 points, derelicts 95 to 213); a check-in empire kills
  60 to 300, because its hauling doctrines pick no fight and its patrol is benched
  with the cap. With the cap ignored the patrols hunt all week and delegated haul is
  152% (raider) and 161% (mixed) of piloted, totals 91 and 98%; miner 18%, explorer
  13%. Ore is the smallest part: 12 to 35 points a week against a hands-on miner's
  321, who strips rings a hauler wing under orders does not rate.
- *Combat points*: 3 to 15%. The cap is full without them.
- *Ships lost* under orders: 1 to 5 a week (pilots 7 to 15); mixed 48 before.
- *Fuel*: check-in empires pay 14 to 24 thousand deuterium a week for it (pilots 45
  to 307); the miner's bill halved while its haul went from nothing to 107 points.

**Tried and not kept** (seed 1, eight seats unless said).
- Hauling doctrines taking paydays among hostiles at the engage ratio: explorer
  check-in 98% of the pilot's total, raider 90%, and 100 (miner) and 127 (explorer)
  ships lost. Parity bought with hulls; they pick no fight for a payday.
- Every wing of four at 2.5 (three seeds, sixteen seats, one of them the stuck
  raider): haul 11, 22, 48, 11%.
- Every doctrine at 1.5: haul 19, 46, 101, 32% on seed 1 of sixteen seats, and the
  explorer's recovery wing lost 66 ships.

**What remains short, and what it would take.**
- One number is both "enter quiet space" and "start a fight". A pilot walks through
  any sector it has seen empty and fights at 3.0; a doctrine enters every sector at
  its ratio against the ring's group and fights at the same ratio. So the bot (or a
  player) chooses between reach at 1.5 with hull lost and hull kept at 2.5 with less
  reach. A second ratio on the standing order (fight at this, enter at that) would
  give doctrines the pilot's two rules. It touches the owner's "the entry rule is
  exactly the player's ratio" and is not done.
- The cap benches patrols, and patrols are where delegated haul would come from.
  Letting strike wings fly past the cap for their wreckage is a bot rule and reaches
  or passes parity for raider and mixed (above). Not done: hands-on is to stay
  better.
- No constant on the forbidden list closes the haul gap by itself. Ore is 11 to 38%
  of what a check-in empire hauls (12 to 35 points); doubling what the tiles hold
  would add as much again and no more.
- Check-in cores are 56 to 71% of hands-on cores (turtle 73%): much of every gap is
  the home economy, not the fleets.

**Engine behaviour found on the way (not changed).**
- Doctrines know what the map holds where their owner has never looked: ore, wrecks,
  derelicts and groups are read from the world, only the odds from the owner's chart.
- A sector in sight with nobody in it is rated at its ring's group, so a doctrine
  cannot walk through space it can see is empty where a pilot does.
- A fleet whose orders are cleared refuels without bound at once; the budget binds
  under orders only. The bots clear orders to regroup at a look-in.
- The bot home planner can wait for ever on an order that needs deuterium with no
  synthesizer and none queued (the stuck raider above).
- Holds repeat once a minute for as long as they last; a fleet waiting half an hour
  for fuel says so thirty times. `play.py` groups them.

**Speed.** The 16-seat week: 556 ticks/s before, 502 to 519 after (20 minutes a seed).

**Tests.** `sim/tests/scenarios_world.rs`: mining to the last seam and waiting for the
refill; wreckage passed over for wreckage in reach; wreckage only when it repays the
fuel; going round a fight; shields; the full store; the lost chart; the patrol.
`fleet_logistics.rs`: the budget wait and fill, a full store or no bound, a young
empire's stock, the mildest lane home, cut off by DEADLY odds. `bots.rs`: a check-in
miner brings home more ore than the fuel is worth in a season day. The chart
recording is unchanged.

**Not verified.** No live server session: the web `policy` settings and the TUI `P`
key are compiled and analysed, not driven; `play.py` is syntax-checked (the harness
is on hold). Persistent pace and blitz were not measured. The uncapped comparison is
one seed.

## 2026-10-11: a busy world ticks two to four times as fast, to the same results

Engine speed was the limit of the single-player build: a week away from a season
world with 20 check-in bots took 17 minutes to catch up (entry "single player in one
browser tab"). No rule, constant, wire message or bot decision was changed. The same
seed and the same orders give the same world, tick for tick, and that was the test of
every step. Branch `worktree-agent-a4fd2784a38fcfc24`: eleven commits on master as of
7845559, where everything below was measured, then master merged in (see "Found on
the way"). Not merged into master.

**How "the same" was checked.** A scratch harness (not in the repository) drives
`iac_solo::Machine::call` the way the worker does: `new_game`, `advance` in slices,
`save`, and reads the digest of the snapshot from the save's header. The same
sequence runs against `iac_solo.wasm` under node. Before the first change the hashes
were recorded from master:

| case | world | length | hashes |
|---|---|---|---|
| A | season, 20 check-in bots in the client's seat order, the solo host | 7 days | 28, every 6 h |
| B | blitz, 8 hands-on bots, the solo host | 3 h | 18, every 10 min |
| C | blitz, 20 hands-on bots, the solo host | 1 h | 6 |
| D | season, all 15 styles and an idle seat, `Headless` | 2 days | 12 |
| E | persistent, 8 check-in bots, the solo host | 7 days | 7 |
| F | blitz, all 15 styles and an idle seat, `Headless` | 2 h | 12 |

A and B were also stopped halfway (day 3.5, minute 90), saved, loaded into a new
machine and played on: the hashes after the load must be those of the run that never
stopped. B was also played two hours and then 600 ticks one at a time with a session
logged in, as a live page does. The week was saved every six hours so that a slice of
the slow late state (L6: day 6.0 to 6.25) can be replayed in half a minute. After each
step B, C, D, E, F and L6 had to match before the commit; A, the two restarts and the
wasm runs were checked on the last commit. Two runs of master a build apart gave the
same 28 hashes for A, so a mismatch means the change and not the machine.

**Where a tick went.** `perf` at 499 Hz on a build with frame pointers (native
aarch64; this `perf` cannot read symbols, so samples were resolved with `addr2line`,
inlined frames included). Shares of all samples.

Season, 20 check-in bots, around day 5.5 (about 4 ms a tick):

| what | share |
|---|---|
| standing orders (`process_policies`) | 59% |
| of that, lane searches (`bfs_path_by_hops` and the questions it asks of each sector) | 57%: `mine_and_return` 41%, `salvage_and_sites` 18% |
| chart upkeep (`KnownSectors::refresh`) | 31%: writing sectors into watchers' memories 15%, building sector states 8% |
| ore refill | 5% |
| bots (their turns and the views they follow) | 2% |
| the rest of the tick | 3% |

Blitz, 8 hands-on bots, hours 2 to 3 (about 4 ms a tick):

| what | share |
|---|---|
| bots following the world (a `tick_update` per watching seat, folded in) | 37%: sector states 25%, folding 7% |
| bots' turns | 38%: `odds` answers 15%, the route home worked out on every jump 10%, a full state after a pause 5%, their own planning 6% |
| the tick | 25%: chart upkeep 20% |

Across both, a quarter to a third of all samples were inside the standard hash map,
hashing a sector key with SipHash and probing.

What the season world is doing by then explains the first table. At day 6 all 58
fleets are docked and the 19 under standing orders are all holding (no seam that
repays the fuel, a contact among hostiles, a tank filling). A hold is looked at again
every 15 ticks, so 1.3 evaluations a tick, each flooding the map up to three times (a
lane without a fight, then any lane, then the reason nothing was found): 3.2 searches
a tick, 940 sectors and 4300 lane checks a search. And 530 of the 844 changed sectors
have ore refilling, a new number every tick, written every tick into the memory of
every empire whose sensors cover them (sensor level 9 is nine hops).

The suspects named beforehand:

| suspect | found |
|---|---|
| bots' odds queries | 15% with hands-on bots, nothing with check-in bots |
| views built for sessions nobody reads | none: a seat follows the world only on the tick before a turn. The views that are read cost 30% with hands-on bots |
| lane searches repeated by standing orders | the largest cost of a long absence |
| `persist_dirty_state` with no persister | under 0.5% |
| world generation repeated | already remembered; the cost was the hash to find it (9%) and asking for the same sector five times per view |
| snapshot size | not a tick cost. A save of the week-old world takes the wasm memory from 15 to 39 MiB for the moment, before and after alike |

**What was done.** One commit a step, the largest remaining cost each time. Ticks per
second, native: L6 is the season world from day 6.0 to 6.25, B the blitz world of 8
hands-on bots over its first three hours. Where a step was measured more than once
the range is given.

| step | L6 | B |
|---|---|---|
| master | 231 to 246 | 310 to 325 |
| 1. a cheap hasher for sector keys and ids | 324 | 456 |
| 2. an evaluation looks at each sector once | 367 | 450 |
| 3. the lane search indexes a table | 378 | 445 |
| 4. the chart records refilling ore without copying the sector | 446 | 481 |
| 5. a sector view looks its template and override up once | 475 | 543 |
| 6. ore refill asks once where the fleets are; a jump asks for the route home only to warn | 509 to 518 | 608 |
| 7. what an evaluation knows sits in a table around home | 531 to 549 | 606 |
| 8. odds against a ring's group once per ring | 627 | 620 |
| 9. a sector's full ore reserve generated with the sector | 663 | 635 |
| 10. refilling ore kept once per sector, not once per watcher | 866 | 638 |
| 11. lair respawn and salvage despawn walk the overrides in place | 870 to 911 | 657 to 693 |

Why none of them can change a result:

1. `shared/src/keymap.rs`: `KeyMap` and `KeySet` are the standard maps with a hasher
   that multiplies and folds. Used for the world generator's memo, `intel.rs`, and in
   the engine the search, the odds memo, `explored` and `credited`. A map's answers
   do not depend on its hasher. Its iteration order does, and that was already
   different in every process, so nothing could have relied on it.
2. `Reach` (one standing-order evaluation) already kept the odds it had worked out per
   sector. It now also keeps whether entering starts a fight, the seam and its worth,
   and whether the doctrine boards the derelict. The engine is borrowed for the
   length of the evaluation, so the answers are the same calls with the same inputs,
   asked once instead of once per flood.
3. A sector `max_hops` jumps out lies within as many hexes, so the search's "reached
   from" map is a table over that square (`Window`). Same order of discovery, same
   path read back.
4. A memory that differed from the observation only in its ore was copied twice
   (once to compare with the ore set aside, once to keep). The ore is now swapped in
   and compared in place.
5. `ore_reserve_in`, `derelict_site_in`, `pending_template_in` and `threat_among` take
   the template and the override from a caller that has them; the by-coordinate
   functions fetch them and delegate. A tile's refill rate is worked out only for a
   tile with something to refill (it was computed and then not used).
6. The refill pass asked "is a fleet here" by walking every fleet for every damaged
   sector; it collects the occupied sectors once. `warn_if_cannot_return` built both
   routes home (two searches of 64 hops) to compare one hop count with the tank; the
   count comes from `direct_hops_home`, which is that number by its own comment, and
   the routes are built when the warning is due.
7. The per-evaluation answers of step 2 move from hash maps to tables over the
   doctrine's range of home, with a map for the few sectors asked about beyond it.
8. Most sectors a fleet is rated in hold nothing its owner has counted, so the answer
   is the fleet against the ring's group, a function of the distance from the hub.
   `Opposing` says "the ring's group" or "these ships"; the ratio against a ring is
   kept per ring for one `odds` request or one evaluation (`RingOdds`). It is the same
   `odds_ratio` of the same two forces.
9. `SectorTemplate::ore_units` is `scaling::ore_reserve_units` of the generated
   densities, computed where the sector is generated instead of on every look.
10. The one step that changes what a structure means. A live sector's `last_seen` was
    already implicit in the chart memory (the last refresh) and written down when the
    sector lapsed. Its ore now is too. What the watchers of a sector with moving parts
    were last shown is kept once (`KnownSectors::shown`). When only the ore moved, only
    that copy is updated. Memories are rewritten when anything else changes, as
    before, and take the ore from the shared copy wherever it can be observed: when
    the sector lapses for a watcher (before the copy is refreshed, so it is the ore of
    the last tick the sector was live), and when a row or a stale view is built for a
    sector still covered (`settled`). The chart recording, the snapshot hashes and
    the restart tests are what hold this one up.
11. Two passes copied every override key into a list each tick and looked each up
    again; they walk the map once, in the same order.

None of the new state is saved: the hasher has none, the tables live for one
evaluation, `ore_units` is regenerated with the sector, and `shown` is rebuilt by the
first refresh after a load, which re-observes every covered sector.

**Before and after.** Same machine, same sitting, niced, one run at a time. Ticks per
second; a range is three or more runs, a single figure one run.

| case | native before | native after | wasm (node) before | wasm (node) after |
|---|---|---|---|---|
| A: season, 20 check-in, the week | 1320 s and 1354 s | 365 s (and 365 s with the restart) | 1636 s | 473 s |
| A: its last six hours | 202 to 216 | 801 | 182 | 641 |
| L6: the same world, day 6.0 to 6.25 | 231 to 246 | 870 to 911 | 182 to 193 | 687 to 701 |
| B: blitz, 8 hands-on, 3 h | 310 to 325 | 657 to 693 | 235 to 266 | 558 to 563 |
| C: blitz, 20 hands-on, 1 h | 318 to 347 | 709 to 732 | 269 to 276 | 566 to 570 |
| D: season, 16 seats, 2 days | 2522 | 5642 | | |
| E: persistent, 8 check-in, 7 days | 16500 | 41700 | 12000 | 26900 |
| F: blitz, 16 seats, 2 h | 576 | 1161 | | |

A live tick two hours into the blitz world with 8 hands-on bots, a session logged in,
600 ticks: native mean 4.2 ms (95th percentile 5.8, worst 7.8) before, 2.0 ms (2.7,
3.4) after; wasm under node 5.1 ms (6.9, 9.4) before, 2.4 ms (3.4, 5.3) after.

**Memory.** Wasm linear memory, which only grows: 15.3 MiB after loading the week-old
world and playing an hour, before and after; 39.1 MiB once that world has been saved
once, before and after (the save, not the engine, sets the peak). Over a whole week
with a save every six hours the peak was 29.8 MiB before and 29.6 MiB after; a run
of saves in the middle of the week reached 33.3 MiB before and 37.3 MiB after, which
is the allocator laying the same transient buffers out differently. Native peak resident set over the week,
saves included: 31.8 MB before, 32.5 MB after. What was added: `shown`, one sector
state per watched sector with moving parts (about 0.2 MB at day 7); 12 bytes per
generated sector; tables of up to 130 kB that live for one evaluation.

**Tried and not kept.**
- Reading a live sector's threat and force in one pass, and keeping the odds against
  a sleeping lair's group per group as for a ring: 1.5% and 1%, inside the spread.
  Reverted.
- `ring_mult` as a table: not attempted. A table filled at compile time would be
  `powi` of constants, which the compiler may fold with a different last bit than
  the routine it calls at run time.
- A recorded-hash test in `sim/tests/`: the hashes pin every rule, and two branches
  were changing rules while this was written. The harness stayed in the scratchpad.

**What is left, and what it would take.** After the work the season world at day 6 is
59% lane searches, 20% chart upkeep, 4% ore refill; the blitz world is 39% the bots'
views, 37% their turns (17% `odds`), 17% chart upkeep. The next gains are not cheaper
ways to compute the same thing.
- *Remember a hold.* A fleet that found nothing to do asks the whole question again
  every 15 ticks, and in a quiet world the answer does not change for hours. Keeping
  it needs to know when it can change: a chart, a group, a wreck, the fleet itself,
  and the refilling ore, which moves every tick but crosses a seam's threshold at a
  tick that can be worked out. That is a dependency model for doctrines, not a memo.
  It would remove most of the 59%: about twice as fast again over a long absence.
- *Seats that fold changes.* A bot one tick from its turn is sent every live sector,
  300 to 400 of them at sensor level 10, rebuilt whether or not anything moved. A
  seat is in-process and could be handed only what changed, which is a second way to
  build a view beside `tick_update` and has to be kept equal to it. Worth most of
  the 39% in a hands-on world.
- *Refilling sectors observed without being rebuilt.* The upkeep still builds the
  whole state of each of the 530 sectors every tick to learn one number. Knowing
  that nothing but the ore moved needs the override compared field by field with
  the last one seen. About 10% of a season tick.

**Found on the way.**
- Native and wasm do not play the same world. Master's blitz case B has the same
  hashes on both up to tick 1800 and different ones from 2400; the season week
  parts between ticks 64800 and 86400. Before and after are alike in this, so wasm
  was compared with wasm. Not looked into; the float library (`ln`, `exp`, `powf`
  in the odds) is the likely place. It matters the day a save moves between a server
  and a browser.
- Master moved while this was done (the hub's exchange, ship batches that pay when
  ordered) and was merged into the branch at the end. Git found no conflict outside
  this journal, and the result did not compile: the hub's sector is written out by
  hand in `compute_sector` and needed its `ore_units`. The hashes were then taken
  again, the new master against the merged branch: B, C, D, E, F, the first four
  days of A and both restarts match (71 hashes and the two restarts). The speeds
  are the same as above within the spread (B 303 to 631, D 2529 to 5627).

**Checked.** On the last commit before the merge and again on the merge:
`cargo build --workspace --all-targets`, `cargo test --workspace --no-fail-fast`
(the chart recording untouched), `cargo clippy --workspace --all-targets`,
`tools/check_sim_constants.py`, and the wasm builds of `iac-sim` and `iac-solo`.
Before the merge, native: all 83 hashes of A to F, both restarts and the live-tick
run match master; wasm: A, B, C, E, L6 and the live-tick run match master's wasm.

**Not verified.** A browser: wasm was run under node only. Other seeds than the one
each case uses. A server with SQLite beyond its own tests. After the merge: the
whole week, wasm, and the live-tick run were not repeated.

## 2026-10-11: the hub and its exchange

Owner decision (decisions.md, "the hub, PvP by zone, outposts, more worlds"): a
neutral post at the centre with an NPC exchange that is a surplus sink, never a
faucet, with a per-player rate that worsens with use and recovers, and no hard cap.
The numbers are the proposal of `starting-positions-2026-10-10.md` section 6.3, which
the owner has not signed off: four constants in `shared/src/constants.rs`
(`EXCHANGE_FEE_MIN` 0.25, `EXCHANGE_FEE_MAX` 0.75, `EXCHANGE_METER_SHARE` 0.15,
`EXCHANGE_WINDOW_TICKS` 86400, attention class), mirrored in `sim.py` and checked by
`check_sim_constants.py`. Branch `worktree-agent-aecb404c2c2075d43`, not merged.

**The rule.** A fleet standing idle on the hub sector sells one resource from its hold
and takes another into the same hold. Par is the score weight (`ResourceKind::weight`,
which `resource_weight` now reads too). Each player has a meter in weight units: two
numbers on the `Player` row, the reading and the tick it was read, so it costs nothing
per tick. The fee on a unit of weight is 25% with the meter empty, rises in a straight
line to 75% at one span and is flat beyond; a trade pays the integral
(`scaling::exchange_return`), so an order in pieces returns what the whole order would.
The span is 15% of one window of the player's own mine output; the meter falls one
span per window. The receipt is rounded down to whole units, which is the only place
an order in pieces can differ, and only downward.

**Choices made here, for the owner to confirm.**
- *The hub is data.* `world::HUB` lists hub sectors with their services (one sector,
  one service today) and goes out in `WorldInfo.hub`; nothing tests for the origin. A
  hub sector generates as empty space with no ore, group or derelict, which the origin
  already was, so no sector changed and `WORLDGEN_VERSION` stands. Patrols no longer
  jump into a hub sector; with nothing to attack and nothing wandering in, there is no
  fight there.
- *A trade that would not fit is refused*, not trimmed, with the numbers. Only
  deuterium for metal and crystal for metal can grow the load.
- *Refused too:* a fleet elsewhere or in transit, more than the hold carries, an empty
  hold, one resource named twice, an amount that is not positive, and a trade that
  returns less than one unit. Every refusal is a sentence from one place
  (`GameEngine::exchange_deal`), which the quote, the error and the tests all read.
- *The quote answers from anywhere* and for an amount the fleet does not carry, with
  the reason the trade itself would be refused now. A player decides at home whether
  the trip pays.
- *The answer to a trade is an event* (`Exchanged`), like the answer to a queue order,
  carrying the terms and the meter before and after; `PlayerState.exchange` carries
  the meter every tick.
- *The meter falls at the rate of the mines the player has now.* A mine finished
  between two trades speeds up the whole interval. Settling the meter at each mine
  completion would be exact; it would also put exchange code in the queue handlers.
- *No standing order trades, and the bots do not either.* A pilot that flies to the
  hub by the chart, with odds for every hop and cargo the store refused, is not a
  small addition to `bot/field.rs`.

**What it does not do.** Nothing loads a hold from the stockpile. The exchange
therefore takes field cargo only: ore, wreckage, derelict loot, and cargo a full store
would not accept. Mine output lost at the cap still has no way out. A `load` command
needs a rule for cargo that must stay aboard at the dock, where every hold unloads each
tick; that is a decision, not a detail. A new empire's span is small (144 weight at
season pace with two level-1 mines), so its first hold already pays about half.
A sector in sight with nobody in it is still rated at its ring's group, the hub
included: previews and doctrines see T1 odds at a place where nothing can fight.

**Clients.** Web: the hub is drawn on the map from `WorldInfo.hub` and the inspector
says what the next unit would pay; in the windshield the exchange is a contact and a
station in the scene, `T` or the Trade tile opens a panel that asks the server for a
quote on every change and sends the trade (it never computes a rate), and `quote` and
`trade` work from the command bar. TUI: `e` opens an overlay that does the same, the
hub is `X` on the star map. Headless: the two commands and their replies pass through
unchanged. `play.py` prints quotes, trades and the meter.

**Tests.** `sim/tests/exchange.rs`: a Hauler flies 632 metal to the hub and gets the
quoted 225 deuterium; a seeded run of trades with waits between never returns more
than 75% of the weight given and never moves the score; four orders in pieces against
the whole order; the meter falls one span per window, is written with the player's
row, and comes back identical from a JSON snapshot; every refusal with its sentence;
the hub empty on six seeds and over 200 patrol jumps from its neighbours that never
enter it (the last one fails with the rule switched off). `server/tests/scenarios.rs`: a
real client harvests, is quoted from afar, flies to the hub, trades over the
WebSocket, and after the server is killed and restarted the meter and the hold are as
they were. Flutter: fixtures for every new shape, the panel at 1440 and 390 with long
numbers and a long refusal, the trade against the scripted world. `solo_e2e.dart`
asks the engine in the worker for a quote. No recorded hash changed: `SAVE_FORMAT` is
2 because the snapshot's player rows grew.

**Not verified.** No trade was made in a browser against a real engine: the web panel
is driven in widget tests against recorded states and the scripted world, and the
single-player check only quotes. The TUI overlay was run in a pseudo-terminal against
a real server and shown a quote; Enter was pressed away from the hub, not at it.
Persistent and blitz pace were not played, only season, test and pace 1. `play.py`
was exercised on fixtures, not in a session (the harness is on hold).

## 2026-10-11: batches pay when they are ordered, and queue boxes that fit

Owner decision (decisions.md, top entry): a ship or defence batch needs its whole
price in the stockpile when it is ordered; buildings and research still queue ahead
of resources. With it a web client fault: the small boxes over the homeworld
overflowed when an order was waiting. No cost, time, production, pay, point value or
cap was changed. Branch `worktree-agent-a14873c5a5d25dc14`, not merged.

**The rule.**
- *Buildings and research* are as they were. An order that can ever start is
  accepted whether or not the stockpile covers it, pays when it starts, reports
  what it is short of while it waits (`WaitReason::Resources`, `short`) and may hold
  the line with `reserve`. Cancelled while it runs: half back. Cancelled while it
  waits: nothing, it paid nothing.
- *A ship or defence batch* is accepted only if the stockpile covers every unit now,
  and the whole price leaves the stockpile at that moment. Otherwise it is refused
  with `InsufficientResources` (1019, new), nothing is queued and nothing is charged:
  with 600 metal, 250 crystal and 70 deuterium in stock, five scouts get `Scout x5
  costs 1000 metal, 250 crystal and 150 deuterium; the stockpile is short by 400
  metal and 80 deuterium. Nothing was ordered: a batch is paid in full when it is
  ordered. The stockpile covers 2.` (the last sentence only for a batch of more than
  one with at least one unit covered). A batch the Storage Vault could
  never hold is still `StorageTooSmall`, and that sentence now adds how many units
  would fit.
- A paid batch can still wait: for the yard (another batch is in it) or for a
  Shipyard or prerequisite that is queued and not built. It cannot wait for
  resources, so a ship entry has no `short` and no `reserve`; its wait is a
  `BatchWait` (`Slot` or `Prerequisite`), a type with no third case.
- Before this a batch paid one unit at a time as each unit started, and a batch that
  ran short went back to waiting part built. Both are gone: the running batch
  carries `built` and starts its next unit in place.
- *Cancelling a batch* returns half of what it paid for every unit not yet
  delivered, the unit under way included, whether the batch waits or runs. Units
  delivered stay.

**Protocol.** `BuildShip` and `BuildDefence` lose `reserve`. `QueuedShip` is `id,
item, count, cost, unit_cost, ticks, waiting_on: BatchWait, start_in`. `QueueEvent.paid`
is what that event took from the stockpile: a building or tech pays on `Started`; a
batch pays on the event that answers the order, `Started` or `Waiting`, and its later
`Started` carries zero. `ECONOMY_VERSION` 2 to 3, so the server refuses a database
from before and single player refuses an older save (`SAVE_FORMAT` 1 to 2): a
waiting batch in an old world was unpaid and would load as paid. No SQLite column was
added or removed; ship rows write 0 for `reserve`. Fixtures regenerated
(`queue__batch_waiting` is new), Dart mirror follows.

**Engine.** `order_shipyard` pays `ShipyardItem::batch_cost(count)` and then starts
or queues. `queue::batches` gives each waiting batch its estimate: the running batch
holds the yard for its remaining units, each waiting one for its count.

**Bots.** The home planner is unchanged for buildings and research. For the yard it
orders only what is paid for: a wanted batch is cut to the units the stockpile
covers, and it orders nothing while a batch already waits (money paid far ahead of
the yard is money the buildings cannot use). When not one unit is covered the
planner remembers one unit's deuterium as what the yard is saving for, and the pilot
leaves that much out of its fuel money as it did when the unpaid order sat in the
queue (`Memory.yard_deuterium`). Surplus batches are cut the same way.

**The box that overflowed.** `clients/web/lib/views/homeworld/queue_strip.dart`, the
strip of boxes over the homeworld (build slot A, slot B, research, shipyard). Each box
drew its title in an `Expanded` and then the queue's time text at 15 px with no
flex. For a running item that text is a clock. For a waiting order it was the whole
sentence ("short 274 metal, 27 crystal, 70 deuterium, starts in about 12:40"), wider
than the box: 474 px over in the building box and 42 px over in the shipyard box at
1440. Now the clock stays beside the title for a running item only, and a waiting
order gets its own line under the title, name first, where the sentence wraps. The
research box shows a waiting tech as the others do, and Cancel on a box with nothing
running cancels the waiting order. `PanelHeader` gave title and subtitle equal
thirds and cut "BUILD SLOT A" short on the overview cards; the title now has three
parts of the row to the subtitle's two.

**The order control.** In the web shipyard and defence tabs a batch the stockpile
does not cover at the chosen count shows `X5 UNAVAILABLE` and `Short by 858 Fe, 78
Cr, 130 De` on its card, the build button is off with the same line under it, and
neither the button, Enter nor activating the card sends a command. A typed `ship`
command still goes to the server and gets its refusal. The TUI card shows `SHORT`
with the amounts and Enter sends nothing; a paid waiting batch reads "paid, yard
busy" or "paid, needs prerequisite". `play.py` prints what a waiting batch paid.

**Tests.** `sim/tests/empire_arc.rs`: a batch refused with the exact sentence, paid
in full, two more waiting and paid, a waiting one cancelled for half, a part-built
one cancelled for half of what is not delivered, the next starting with nothing more
to pay. With the check switched off the scene fails. `sim/tests/bots.rs`: a check-in
builder orders only ships and turrets its stockpile covers. `solo/tests`: the refusal
and the unchanged state through the worker protocol.
`clients/web/test/order_fit_test.dart`: homeworld boxes, ship and defence cards and
overview tiles at 1440 and 390 px with a shortfall of 1,234,567 of each resource and
a paid batch of 1000 waiting; it fails on any layout overflow and on any text that
leaves its box or is cut. `tool/solo_e2e.dart` in the real browser: a batch of five
is unavailable and says why, button and Enter send nothing, one scout is paid at
once, a second is refused and costs nothing, a Shipyard the stockpile cannot pay for
queues and shows its shortfall, pictures at both widths. The chart recording
(`chart_golden.txt`) is unchanged.

**Measured.** Season, 7 days, seeds 1 to 3, the default 16 seats, before (the commit
before this work with the idle count added to `sim-run`) and after. `sim-run` now
reports, per player, the share of the week each of the building slot, lab and yard
stood empty. Mean (min-max over seeds), day 7.

| style | total before | total after | core before | core after | idle building / lab / yard, before | after |
|---|---|---|---|---|---|---|
| turtle / check-in | 1755 (1556-1874) | 1616 (1466-1874) | 1755 | 1616 | 82/95/97% | 84/95/97% |
| miner / check-in | 2228 (2022-2332) | 2252 (2065-2360) | 1803 | 1801 | 82/95/97% | 82/95/97% |
| raider / check-in | 1929 (1702-2234) | 2072 (1857-2350) | 1543 | 1672 | 85/94/94% | 84/95/94% |
| explorer / check-in | 2187 (1933-2354) | 2272 (2133-2507) | 1749 | 1817 | 83/95/96% | 82/95/96% |
| mixed / check-in | 2181 (2012-2349) | 2116 (2014-2265) | 1745 | 1693 | 84/95/95% | 84/95/95% |
| turtle / hands-on | 2400 (2400-2400) | 2400 (2400-2400) | 2400 | 2400 | 77/95/98% | 77/95/98% |
| miner / hands-on | 4026 (3912-4250) | 3849 (3718-3917) | 3221 | 3079 | 71/95/95% | 72/95/95% |
| raider / hands-on | 2926 (2861-3002) | 2905 (2873-2931) | 2341 | 2324 | 81/95/92% | 81/95/92% |
| explorer / hands-on | 3406 (3029-3701) | 3508 (3021-3804) | 2724 | 2806 | 76/95/94% | 75/95/94% |
| mixed / hands-on | 3086 (2889-3255) | 3195 (3120-3281) | 2469 | 2556 | 79/95/93% | 78/95/92% |

Hybrids, total: turtle 1876 to 1877, miner 2893 to 2803, raider 2225 to 2257,
explorer 2389 to 2438, mixed 2160 to 2163.

Check-in as a share of the same personality flown hands-on, total by day (mean of
seeds; day 7 with min-max), before then after:

| | d1 | d2 | d3 | d4 | d5 | d6 | d7 |
|---|---|---|---|---|---|---|---|
| turtle, before | 40% | 50% | 48% | 57% | 68% | 81% | 73 (65-78)% |
| turtle, after | 40% | 46% | 38% | 48% | 61% | 78% | 67 (61-78)% |
| miner, before | 25% | 22% | 31% | 43% | 50% | 51% | 55 (52-60)% |
| miner, after | 25% | 22% | 31% | 42% | 51% | 53% | 58 (56-60)% |
| raider, before | 56% | 44% | 49% | 60% | 67% | 72% | 66 (57-77)% |
| raider, after | 55% | 46% | 48% | 59% | 64% | 71% | 71 (63-82)% |
| explorer, before | 37% | 36% | 41% | 40% | 51% | 60% | 65 (55-75)% |
| explorer, after | 36% | 37% | 42% | 41% | 52% | 54% | 65 (56-72)% |
| mixed, before | 39% | 45% | 60% | 64% | 66% | 70% | 71 (65-76)% |
| mixed, after | 38% | 44% | 53% | 63% | 64% | 67% | 66 (61-71)% |

Hybrid on day 7, before then after: turtle 78 then 78%, miner 72 then 73%, raider 76
then 78%, explorer 70 then 70%, mixed 71 then 68%.

What is noise: all of it, as far as three seeds can say. Every after range overlaps
its before range. The one number that looks like a finding is the check-in turtle
(73 to 67%), and it is one seat: seed 1 is the same to the point (1874), seed 2 is
1556 to 1509, seed 3 is 1836 to 1466. A turtle flies nothing, so its week is the home
planner alone and cheap to run: eight check-in turtles alone in a world, seeds 1 to
10, before and after (80 turtle-weeks each, ten seconds a world). Mean day-7 score
1761 before and 1751 after (median 1824 and 1821); 52 of 80 seats the same to two
points, 11 up by more than 20 (361 the most) and 10 down (686 the most); idle
82/95/97% both times. A different order time early in the week sends about a quarter
of the seats down a different road, in either direction, and the rule does not
favour one. The idle shares did not move for any style.

**What it says about the gap.** The rule did not open it and does not close it.
Check-in stays at 58 to 71% of hands-on on day 7 and 25 to 55% on day 1. The idle
columns say where to look: a check-in empire's building slot stands empty 82 to 84%
of the week against 72 to 81% for a pilot, and lab and yard are empty about 95% for
everybody. Every style leaves its slots empty most of the week, so the queues are
short of resources or of orders far more than of time. The count does not yet say
which: an empty slot with an order waiting on resources and an empty slot with no
order at all are counted alike. Splitting them is the next measurement; it would say
whether the limit is income (production, costs and times, none of them open here) or
the planner (what it queues at a look-in, and how many look-ins a day: `--sessions`,
default 4). Neither was tried.

**Seen on the way (not changed).**
- A refund is added to the stockpile without regard to the Storage Vault, as a
  running building's refund already was. Ordering a batch and cancelling it parks
  half its price above the cap at the cost of the other half. A lost raid takes at
  most 8% of exposed metal and crystal and 5% of deuterium, so hiding stock in a
  batch from a raid loses more than the raid would.
- A paid batch whose queued Shipyard or prerequisite is cancelled stays paid and
  waiting until its owner cancels it for half or queues the prerequisite again.
- The boxes over the homeworld write a shortfall as "short 1234567 metal" (the
  server's summary) and the cards as "Short by 1,234,567 Fe". Both fit; they do not
  match.
- The offer of browser notices covers the order button at desktop width and the
  lower ship cards at phone width for its thirty seconds, and its buttons are not in
  the semantics tree. The browser check waits it out before its pictures.

**Not verified.** The TUI is compiled and its code path read, not driven against a
live server. `play.py` is syntax-checked and its offline tests pass; the harness is
on hold and was not run. Persistent pace and blitz were not measured. The browser
run is headless Chromium at 1440 and 390 px; no other browser and no real phone.

## 2026-10-11: a hold is loaded from the stockpile at home

Owner decision: the exchange is a surplus sink, so a player whose stockpile is full of
one resource must be able to carry the surplus to the hub. Until now nothing moved
stockpile into a hold and the dock emptied every hold each tick, so the exchange took
field cargo only. The owner set the frame of the rule; the choices inside it are the
agent's and are listed as such in decisions.md under the same date. Branch
`worktree-agent-a2b101c4105620314`, not merged.

**The rule.** `load` takes a fleet standing idle at its owner's homeworld, one
resource and an optional amount. It moves that much from the stockpile into the hold;
with no amount, all the stockpile holds and the hold has room for. An order that
cannot be met in full moves nothing and says what is short: the stockpile ("the
stockpile holds 400.0 metal; fleet 7 cannot load 400.5") or the room ("the hold of
fleet 7 has room for 632.5 more (it carries 0 of 632); it cannot load 700.0
deuterium"). The fleet is then laden (`Fleet.laden`, `FleetState.laden`):
`unload_cargo`, which the dock runs for every fleet at home each tick and on arrival,
leaves a laden hold alone. The mark ends with the fleet's next jump, so a fleet that
has been away comes home unmarked and unloads as it always has. `unload` ends it by
hand: what the caps allow goes back at once and the dock takes the rest as room
opens. Both answer with an info `Alert`, as `split` and `merge` do.

**Where the frame was not followed.** The owner's frame said `load` takes an amount
of each resource. It takes one resource an order. "All that fits" has one meaning for
one resource and none for three; a surplus is nearly always one resource; the refusal
can then name exactly what is short; and `exchange` already reads `give`, `want`,
`amount` this way. A mixed load is two or three orders and is not atomic.

**How the mark ends.** The first version cleared it when the fleet docked. That left
it set in the field, where it means nothing, made recall-at-home a second unload, and
needed the rule in three places. It is now cleared in one: where a jump completes in
`process_movement`. A fleet still leaving (state `Moving`, location still home) keeps
it, or the dock would empty the hold during the jump's countdown.

**Standing orders.** A doctrine that comes home with a hold over `cargo_return_pct`
waits at the dock until it is off (`policy_unload_wait`); with a hold the dock may not
take, it would wait for ever. So `load` is refused for a fleet under standing orders,
with the doctrine named, and giving a laden fleet standing orders ends the mark and
unloads it at once (the policy event says so). Unloading at once matters: orders are
evaluated before the dock runs in a tick, and a hold still aboard at the first
evaluation would announce "waiting to unload" for one round.

**What it must not dodge.**
- *The Vault cap.* Unload goes through `unload_cargo`, which stops at the cap. A hold
  at home is storage beyond the cap, which the owner allowed; decisions.md has the
  numbers (a Hauler stores a unit for 4.75 resources of hull at pace 1, the Vault for
  0.07 at level 3 and 0.6 at level 20; a full complement of 512 Haulers holds 9.3
  Vault-0 stockpiles and 0.16 Vault-10 ones).
- *Raids.* Cargo at the dock was out of a lost raid's reach: `resolve_raid` took its
  share of the stockpile and never looked at holds. That did not matter while only a
  full stockpile left cargo there; with `load` it would have been a hiding place.
  A lost raid now adds the cargo aboard the owner's fleets at home to the stockpile,
  applies the one protected floor and the same 8/8/5 percent to the sum, and takes
  each part's share. With no cargo at home the arithmetic is the old one to the bit
  (the share of the whole is multiplied by exactly 1.0). `resources_lost` and
  `protected_kept` cover both. This changes the rule for every fleet with cargo at
  home, laden or blocked.
- *Score.* Unchanged by construction: `score_of` reads buildings, research, ships and
  defences. The tests assert the row is equal before and after a load and an unload.
- *Fuel.* A laden fleet burns what its hull mass burns; cargo has no mass in the fuel
  rule, for any fleet.

**Persistence.** `laden` is a column of `fleets` (added to an existing database by
`ensure_column`, as the exchange meter was) and a field of the fleet in `Snapshot`,
so `SAVE_FORMAT` is 3 and an older single-player save is refused with the usual
sentence. `ECONOMY_VERSION` stands at 3. The SQLite restart test in
`server/src/database.rs` now mixes `load` and `unload` into its orders and requires
that at least one of its kills finds a laden hold in the database.

**Clients.** Web: the homeworld's Storage tab has a "Load a hold" panel under the
Vault: the fleets docked at home, what is aboard and the size of the hold, the
stockpile per resource with FULL on a capped one, a share of what fits (25, 50, 75
percent or all, the last sent with no amount), a button that names the load ("Load
400 Fe"), and Unload. It works nothing out that the server decides; a disabled Load
says why. In the windshield the homeworld contact has a "Load hold" button (`L`)
that opens the panel for the active fleet, the cargo gauge reads "CARGO - LADEN", and
`load <resource> [amount|all]` and `unload` work from the command bar. Help stays at
13 lines: the two hub lines were rewritten to carry the two commands. TUI: the fleet
panel (`f`) shows the hold and the stockpile and loads with `r` (resource),
Left/Right (share), `l`, and unloads with `u`; the fleet list and the windshield tag
a laden fleet. `play.py` tags it too. The bots do not load.

**Tests.** `sim/tests/exchange.rs`: a stockpile at its cap, four Haulers loaded with
all that fits, forty ticks at the dock with the hold untouched and the mines filling
the room, the flight to the hub, the trade, the flight home and the dock unloading
what was bought; `unload` against a nearly full store and the dock taking the rest;
every refusal with its sentence; standing orders ending the mark with no "waiting to
unload" in 120 ticks; two worlds raided alike, one with 5000 aboard, losing the same
and keeping the same floor; the mark in the fleet's persisted row and through a JSON
snapshot; split and merge. `server/tests/scenarios.rs`: a real client loads over the
WebSocket, is refused twice, the server is killed, and after the restart the fleet is
still laden and stays so, flies out and back, and the dock unloads it.
`solo/tests/single_player.rs`: the same through `Machine::call`, with sixty ticks
replayed from the save and the two worlds equal. Flutter: fixtures for the two
commands and the field, the panel at 1440 and 390 against the scripted world, the
typed commands and their refusals, the windshield button and key. `solo_e2e.dart`
loads 100 deuterium through the panel in the real bundle, checks it is still aboard
four ticks later, and unloads it by typed command.

**Seen on the way (not changed).**
- A fleet that loses ships keeps all its cargo, in a fight, on a recall and in a lost
  raid: nothing trims a hold to the capacity that is left. One survivor of a
  64-Hauler fleet carries 64 holds. With `load` a player can now arrange that at
  home (load, lose a quarter of the ships to a raid, keep the cargo less 8%).
- The server tests take their ports from literals in the test files (17931 to 17956,
  7910), so two checkouts on one machine cannot run them at once; the temporary
  databases are named by port as well.
- The web windshield says "DOCKED FLEETS ARE SAFE" at home. A lost raid destroys a
  quarter of them.
- `recall` at home is accepted: it costs nothing, cancels a jump under way (the fuel
  of that jump is already spent) and runs the dock again.

**Run.** `cargo build --workspace --all-targets`; `cargo test --workspace
--no-fail-fast`, 114 tests, all pass; clippy clean with `-D warnings`;
`check_sim_constants.py` agrees; the wasm release build; `flutter analyze` clean and
`flutter test` 355 of 355 in four full runs of four on the final tree. One full run
before that failed in "firing on deadly odds needs a second F" with a layout overflow
in the HUD; it passed in three runs of its file and did not come back (the profiler
flag of the 2026-10-10 entry was on). `play.sh --single-player --no-serve` and
`solo_e2e.dart --port 7932`: OK in 153 s, the hold step included, with pictures of the
panel at 1440 and 390 read. The TUI fleet panel was driven in a pseudo-terminal
against a real server (port 7934, pace 1): 30 crystal at 75 percent, all that fits of
deuterium, the refusal of a third load in the footer, LADEN in the fleet list, unload,
and the refusal of a second unload.

**Not verified.** The web panel against a multiplayer server (it ran against the
engine in the tab and the scripted world; the client code is the same). The
windshield's Load hold button and `L` in a real browser (widget test only). No real
phone and no browser but headless Chromium. The balance report was not run again:
the raid now reaches cargo that a full stockpile left aboard a bot's docked fleet, and
what that does to a bot's week was not measured. `play.py` passes its offline tests;
the harness was not run.

## 2026-10-11: the web client after its first played evening

The owner played the single-player bundle over the LAN. This is the pass after it:
five known faults, then every screen read at 1440 and 390 px in the browser check's
pictures. Web client only (`clients/web`, `solo/web/worker.js`, two README
bullets); no rule, constant or wire message. Branch
`worktree-agent-ab87eca5a1d51d195`, not merged.

**The five.**
- *Toasts cut off.* Real, and not the frame governor: the slide-in always finishes
  (350 ms). The key sat on the tile inside an unkeyed `Padding`, so the column
  matched its children by position, and every time one toast left, or a fifth
  pushed the first out, every toast after it was built again and slid in again
  from nothing. Measured in real-time headless Chromium: four restarts in 1.3 s as
  three toasts expired. The check's picture caught one mid-slide because its wait
  ended the instant a toast left. The fifth toast had also been pushing out the
  offer of browser notices, which asks a question and is meant to stay 30 s. Now
  the key is on the column's child; a toast that asks is the last to make room;
  its buttons are in the semantics tree; a plain toast goes away on a tap. A toast
  is at most 340 px wide and, on a phone, the screen less 16 px a side.
- *Two wordings.* One function, `shortBy`: "short by 1,234 Fe, 56 Cr", in the
  units the counters carry, for the queue boxes, the cards, the tiles and the log.
  The estimate beside it is the clock every other timer uses ("starts in about
  34:17:36", not "2057:36"). An order short of something the homeworld does not
  produce has no estimate and now says "which home production will not cover": a
  new empire has 100 deuterium and makes none, and its second order sat there
  without a word.
- *A typed order the engine refuses.* Left to the engine: its sentence is exact
  and Dart would only copy the rule. What was missing was the telling. A refusal
  went to the log and the alert list, both on the Overview; typed on any other
  screen it showed nothing. Every refusal is now a red toast for nine seconds with
  the engine's sentence, the log line is "Refused: ..." instead of "ERR 1019
  InsufficientResources: ...", and it no longer lights the alert badge for good.
- *Roboto.* Both renderers fetch it from `fonts.gstatic.com` unless the font
  manifest has a family of that name. `pubspec.yaml` names the bundled Plex Mono
  as `Roboto`: nothing added to the bundle, nothing asked of another host. The
  check now fails on any request elsewhere, at load and over the whole session.
- *Plain http.* On `http://<lan address>` the page is no secure context: no
  `navigator.locks`, so the worker locks nothing and a second tab opens the same
  save. Nothing said so. The worker's status now carries `exclusive`, and the
  front door says in one sentence to play in one tab. No second lock was built.
  `navigator.storage` is missing there too, so the save cannot be marked
  persistent; that is not said (it is best effort on https as well).

**Found reading the pictures, and fixed.**
- The defence tab added up the power of one structure of each kind, built or not:
  "STRUCTURES 203" on an empire with none, and "docked fleet and grid 0" beside
  it. A row's power is now count times unit power.
- Below about 1400 px wide the HUD drew its three stockpiles smaller the narrower
  the window, down to a few pixels at 1100 (a spacer took half their room). They
  now keep their size; the name beside the logo, then the commander's name and
  rank, then the rate per tick give way. At phone width the three numbers had no
  labels; they have.
- The automatic leaderboard refresh printed two log lines every 30 ticks, in a log
  that holds 14. It prints only when the player types `leaderboard`.
- States that said the wrong thing on a new empire: "Research lab idle. Queue
  something" with no lab, "Idle: pick a tech" and "Pick one" likewise, "Upgrade the
  Shipyard" with none built, "One fleet out, no docked" with the fleet at home.
  The cards now name the nearest thing that would unlock something ("Scout needs
  Shipyard level 1"), in the engine's words rather than "Shipyard >= 1".
- "1S BUILD" read as "15 BUILD"; amounts in the log were "90M 22C 0D"; "1 HOPS
  HOME"; "asking the server" in a world with no server; the sector note ran under
  the north-west gate card; the windshield feed ran under the dock below 1400 px;
  "NO RAID ANNOUNCED" was cut to "NO RAID ..." on a phone; the "Go to" toast
  outlived the jump; `help` typed away from the Overview showed its first line and
  nothing else.
- "DOCKED FLEETS ARE SAFE" on the windshield's homeworld contact was not true: a
  raid that breaks through destroys a quarter of the docked ships (at least one)
  and, since holds can be loaded, takes its share of a laden one. The card now
  says what the dock does and what it does not: "A docked fleet refuels and
  unloads, and nothing in the field reaches it. A raid that breaks through still
  destroys some docked ships", and for a laden hold that it stays aboard and that
  the raid takes its share.

**The check.** `solo_e2e.dart` answers the offer from its own button, holds three
toasts up and fails if one narrows while another leaves, expects the refusal on
screen, pictures every screen at both widths (and the windshield at 1100), and
loads the page from `http://iac-lan.test` (mapped to this machine) to see the
missing lock and the sentence about it. Its old wait for the waiting Shipyard
raced the tab change and failed once here on master's code; it now waits for the
line it reads. After the merge of the hold (the entry above) it also presses the
windshield's Load hold button and `L` in the real page, which the hold's own
entry lists as not done, and pictures the laden windshield at both widths.

**Merged master** (`2e9d399`, loading a hold) into the branch: two conflicts, the
check and this file, both kept whole.

**Left for the owner** (layout or taste; pictures in the hand-off). The windshield
between 980 and 1400 px, where gate cards run under the side columns, and at any
width its feed, which runs over the south-west gate card when its lines wrap. Overview
tiles for buildings and research are off when the stockpile is short, though such
an order queues. There is no standing-orders panel in the web client: `policy`
is typed. Toasts cover the order button and, on a phone, the dock. The log keeps
14 lines and `help` is 13.

**The test flake, found and not ours.** `flutter test` fails now and then with a
whole file red: 3 of 14 full runs here, none of 30 runs of single files. The
three failures are three different impossible results on doubles, none in client
code. `TextStyle.apply` asserting `fontSizeFactor == 1.0` false for its own default
argument, reached from `ThemeData.dark()` through `Typography.material2021` (this
is the "assertion inside ThemeData" seen before; it fails every test of the file
once it has happened in that process). `offset.isFinite` in the semantics sort of
a frame that is the same in every run. `Infinity or NaN toInt` in the scripted
world's storage estimate. The machine is a CIX P1 (arm64) and the VM Dart 3.12.1;
the fault is in that pair under load, not in the tests or the widgets. A failed
file passes when run again. Output kept in the hand-off.

**Not verified.** No real phone and no browser but headless Chromium. The whole
check was run on the WebAssembly build; on the JavaScript build (`--software`)
only the load, a new game and the four screens, to see that no font is fetched
elsewhere there either.

## 2026-10-11: three loose ends: holds after losses, recall at home, test ports

Three things the hold entry above listed under "Seen on the way". Rust only
(`sim`, `server`, their tests, the rules text); no wire message, constant or save
format changed. The choices are the agent's and are in decisions.md under the same
date. Branch `worktree-agent-aead3c1377ff261e8`, not merged.

**A fleet that loses ships.** Ships leave a fleet in three places: a combat round
(`process_combat`), a recall that breaks ships (`handle_recall`) and a lost raid at
the dock (`destroy_docked_ships`). In all three the tank was already cut to what the
ships left carry, and the hold was not: cargo stayed whole. One function,
`settle_hold`, now runs in all three. The fleet keeps `fleet_cargo_capacity` of the
ships left, each resource scaled alike, and sheds the rest:
- *away from home* into the sector's wreckage through `drop_salvage`, the pile a kill
  leaves, with the same 180 ticks from the last addition. A fleet with no ships left
  has a hold of nothing and sheds everything, so a destroyed fleet's cargo now lies
  where it fell instead of vanishing. A won fight reports the whole pile in
  `CombatEnded.salvage`, the shed cargo included; the kill's score still follows only
  what the hostile group was worth.
- *in a recall* before the fleet is moved, so the share of the ships that broke up
  stays in the sector it jumped from. Settling it at home would have handed the
  cargo of destroyed ships to the stockpile.
- *at the home dock* into the stockpile up to the Vault cap, the rest lost. The raid
  settles holds before it takes its slice, so what moved is raided with the rest.

The owner gets a warning `Alert` with the amount and where it went. Nothing is
created anywhere: hold, pile and stockpile together never hold more than the hold
did. Split and merge share cargo by hold and cannot put a fleet over, so they are
unchanged. A fleet already over its hold in a stored world stays so until it next
loses a ship. `fleet_cargo_capacity` summed no floats to -0.0 for an empty fleet,
which scaled the cargo to -0.0; it is folded from 0.0 as `fleet_fuel_max` is.

**Recall at home.** Reproduced on a new empire at pace 1: `recall` for the starting
fleet docked at home answered Ok and changed nothing. After `move` to a neighbour
(tank 120 to 114, state Moving) `recall` answered Ok again: state Idle, no target, and
the dock refilled the 6 fuel for 0.48 deuterium. So it called off a paid jump and
said nothing. The chart recording showed a third effect: it halted a fleet harvesting
at its own homeworld. It is now refused with `InvalidCommand` and nothing changed
(the tests compare the snapshot hash before and after), with one of three sentences:
already at home; charging a jump to a named sector, which `stop` calls off without
returning the fuel; at work there, which `stop` halts. A jump ordered at home goes
on. Away from home nothing changed: recall abandons a jump the fleet was charging
with its fuel, as `stop` and a fight do, and RULES.md and the README now say that of
both orders. No refund was added to `stop`.

**The chart recording, twice.** `chart_golden.txt` pins what a scripted blitz world
sends for 4000 ticks, and both rules change that world. Each was traced with a
temporary print before recording again.
- Holds: the old recording held to tick 1010. At 1011 a fleet is destroyed at [1,6]
  with 34.9 units aboard (23.5 metal, 11.4 crystal); the update of that tick now has
  `salvage` of exactly that with `salvage_despawn_tick` 1191. 280 lines differ: 1011
  to 1107, 1150, 1191, and 2892 to 3072, the 180 ticks of a second such pile at
  [-7,5]. Four other fleets die with cargo (ticks 277, 1579, 1706, 2364) and leave
  piles that change no line of the recording.
- Recall: that recording held to tick 111, where the script recalls a fleet
  harvesting at its homeworld [-5,1]. Refused, it mines on and the ore reads lower
  from 112. The script does the same at 427 and 1615; 434 lines differ. No recall of
  a fleet charging a jump at home occurs in the script.
The determinism tests compare two runs with each other and needed nothing.

**Server tests on ports of their own.** `smoke.rs`, `scenarios.rs` and `web.rs` named
their ports (17931 to 17956, 7910, 7931) and their databases after them. Now
`iac-server --port 0` binds a port the system picks and the server prints
`listening on <address>:<port>` on stdout once it listens, on any port. The helpers
in `server/tests/common` start every server that way, read the line, and hand the
port back with the guard; a thread keeps reading the pipe so the server never
blocks on it. A test connects only after its server listens. Picking a free port in
the test and passing it on was the smaller change and was not taken: the port is
free between the two binds, and a client retrying against a port in the dynamic
range can connect to itself. Databases are in `<temp>/iac_<test>_<pid>/world.db`.
Cleaning up is as before for the database tests: a passing test removes its
database files, now also the directory it then leaves empty; a failing one leaves
them. `web.rs` used to leave its directory behind and clear it at the next start;
with the process id in the name that would never happen, so a passing web test now
removes its own directory. `web.rs` uses the shared helpers; its copy of the guard is
gone.

**One refusal sentence.** A `policy_update` for a fleet that is not the player's was
refused with "PolicyUpdate failed". It names the fleet now.

**Tests.** `sim/tests/fleet_logistics.rs`: a two-Scout fleet nine tenths full loses
one ship to a hostile and has half its hold, the rest in the sector to the unit, the
alert, the victory reporting the pile, and a full hold scooping nothing back; a fleet
destroyed with cargo, its cargo in the sector, and another player's fleet collecting
all of it; a recall from 25 hexes that breaks ships of twenty, their share at the
sector left and the rest home; recall at home refused idle, harvesting and charging
a jump, the jump arriving, and a recall from one hop out; standing orders for a
rival's fleet. `sim/tests/exchange.rs`: eight laden Haulers lose two to a raid, with
room in the stockpile (nothing lost but the raid's take) and with a full one (the
two holds lost, the stockpile not over its cap); the older raid test now loads what
six Haulers carry. `server/tests/smoke.rs`: two servers at once, one name registered
on each, each port answering with its own world's pace. `scenarios.rs`: the recall
refusal over the WebSocket. `scenarios_world.rs`: the recall that frees a fleet for
standing orders is given in the field.

**Run.** `cargo build --workspace --all-targets`; `cargo test --workspace
--no-fail-fast`, 121 tests (114 before), all pass, while a second `cargo test -p
iac-server` from another target directory ran its 37 at the same time, all pass;
clippy clean with `-D warnings`; `check_sim_constants.py` agrees; the wasm release
build; the 36 offline tests of `play.py`.

**Not verified.** The Flutter tests and `solo_e2e.dart` were not run: `clients/web`
is untouched and no wire type changed, but a typed `recall` at home now shows a
refusal there. The TUI was not driven by hand; it shows the server's sentence in
its status line as for any refusal. The balance report was not run again: bots
whose fleets die with cargo now leave it as wreckage, which was not measured.

**Seen on the way (not changed).**
- `stop` on a jump returns Ok and sends no event; the fuel is gone. The rules say so
  now, the engine still does not (`handle_stop`).
- A recall damages hulls without an event unless a ship is destroyed; the fleet row
  shows it.
- A harvest that ends because the hold is full or the seam is out only changes the
  fleet's state (`process_harvesting`).
- A cancelled order's refund is added past the Vault cap (`handle_cancel_build`,
  `handle_cancel_queued`).
- The in-process tests of `server/src/database.rs` name their databases
  `iac_<name>_<pid>.db` in the temporary directory and never remove them: several
  thousand files on this machine.
- `solo_e2e.dart` defaults to port 7881 (and the next for its multiplayer check) and
  the playtest harness to 7777, so two checkouts running either with its defaults
  collide; both take another port by `--port` or `IAC_PORT`. The browser check's
  profile and database are in fresh temporary directories, its pictures in one
  fixed one (`<temp>/iac-solo-e2e-shots`, `--shots`); the harness keeps its run in
  the checkout.
