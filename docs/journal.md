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

