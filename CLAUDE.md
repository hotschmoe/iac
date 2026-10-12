<!-- BEGIN:header -->
# CLAUDE.md

we love you, Claude! do your best today
<!-- END:header -->

<!-- BEGIN:rule-1-no-delete -->
## RULE 1 - NO DELETIONS (ARCHIVE INSTEAD)

You may NOT delete any file or directory. Instead, move deprecated files to `.archive/`.

**When you identify files that should be removed:**
1. Create `.archive/` directory if it doesn't exist
2. Move the file: `mv path/to/file .archive/`
3. Notify me: "Moved `path/to/file` to `.archive/` - deprecated because [reason]"

**Rules:**
- This applies to ALL files, including ones you just created (tests, tmp files, scripts, etc.)
- You do not get to decide that something is "safe" to delete
- The `.archive/` directory is gitignored - I will review and permanently delete when ready
- If `.archive/` doesn't exist and you can't create it, ask me before proceeding

**Only I can run actual delete commands** (`rm`, `git clean`, etc.) after reviewing `.archive/`.
<!-- END:rule-1-no-delete -->

<!-- BEGIN:irreversible-actions -->
### IRREVERSIBLE GIT & FILESYSTEM ACTIONS

Absolutely forbidden unless I give the **exact command and explicit approval** in the same message:

- `git reset --hard`
- `git clean -fd`
- `rm -rf`
- Any command that can delete or overwrite code/data

Rules:

1. If you are not 100% sure what a command will delete, do not propose or run it. Ask first.
2. Prefer safe tools: `git status`, `git diff`, `git stash`, copying to backups, etc.
3. After approval, restate the command verbatim, list what it will affect, and wait for confirmation.
4. When a destructive command is run, record in your response:
   - The exact user text authorizing it
   - The command run
   - When you ran it

If that audit trail is missing, then you must act as if the operation never happened.
<!-- END:irreversible-actions -->

<!-- BEGIN:code-discipline -->
### Code Editing Discipline

- Do **not** run scripts that bulk-modify code (codemods, invented one-off scripts, giant `sed`/regex refactors).
- Large mechanical changes: break into smaller, explicit edits and review diffs.
- Subtle/complex changes: edit by hand, file-by-file, with careful reasoning.
- **NO EMOJIS** - do not use emojis or non-textual characters.
- ASCII diagrams are encouraged for visualizing flows.
- No casual or explanatory comments. A comment is rare and only for something the code cannot say (a non-obvious invariant, a protocol constraint). Use external documentation for complex logic.
- Always write idiomatic Rust and idiomatic Dart.
<!-- END:code-discipline -->

<!-- BEGIN:no-legacy -->
### No Legacy Code - Full Migrations Only

We optimize for clean architecture, not backwards compatibility. **When we refactor, we fully migrate.**

- No "compat shims", "v2" file clones, or deprecation wrappers
- When changing behavior, migrate ALL callers and remove old code **in the same commit**
- No `_legacy` suffixes, no `_old` prefixes, no "will remove later" comments
- New files are only for genuinely new domains that don't fit existing modules
- The bar for adding files is very high

**Rationale**: Legacy compatibility code creates technical debt that compounds. A clean break is always better than a gradual migration that never completes.
<!-- END:no-legacy -->

<!-- BEGIN:dev-philosophy -->
## Development Philosophy

**Make it work, make it right, make it fast** - in that order.

**This codebase will outlive you** - every shortcut becomes someone else's burden. Patterns you establish will be copied. Corners you cut will be cut again.

**Fight entropy** - leave the codebase better than you found it.

**Inspiration vs. Recreation** - take the opportunity to explore unconventional or new ways to accomplish tasks. Do not be afraid to challenge assumptions or propose new ideas. BUT we also do not want to reinvent the wheel for the sake of it. If there is a well-established pattern or library take inspiration from it and make it your own. (or suggest it for inclusion in the codebase)
<!-- END:dev-philosophy -->

<!-- BEGIN:testing-philosophy -->
## Testing: End-to-End Only

**No unit tests and no TDD.** They weigh the codebase down. Verify with end-to-end tests: a real server, real clients over the real protocol, whole-engine scenario runs, the cross-language protocol fixtures, and the playtest harness. Do not add unit tests or module-level `#[cfg(test)]` cases, and do not delete existing ones without the owner's say (archive instead).

## Testing Philosophy: Diagnostics, Not Verdicts

**Tests are diagnostic tools, not success criteria.** A passing test suite does not mean the code is good. A failing test does not mean the code is wrong.

**When a test fails, ask three questions in order:**
1. Is the test itself correct and valuable?
2. Does the test align with our current design vision?
3. Is the code actually broken?

Only if all three answers are "yes" should you fix the code.

**Why this matters:**
- Tests encode assumptions. Assumptions can be wrong or outdated.
- Changing code to pass a bad test makes the codebase worse, not better.
- Evolving projects explore new territory - legacy testing assumptions don't always apply.

**What tests ARE good for:**
- **Regression detection**: Did a refactor break dependent modules? Did API changes break integrations?
- **Sanity checks**: Does initialization complete? Do core operations succeed? Does the happy path work?
- **Behavior documentation**: Tests show what the code currently does, not necessarily what it should do.

**What tests are NOT:**
- A definition of correctness
- A measure of code quality
- Something to "make pass" at all costs
- A specification to code against

**The real success metric**: Does the code further our project's vision and goals?

**Don't test the type system**: When writing tests, do not add cases for invariants or errors already enforced by the static type system (e.g., type mismatches, missing required arguments, nullability violations, return type correctness, enum exhaustiveness). The type checker handles these at compile time. Test solely runtime behaviors, business rules, algorithmic logic, and edge cases using only valid typed inputs.
<!-- END:testing-philosophy -->

<!-- BEGIN:footer -->
---

we love you, Claude! do your best today
<!-- END:footer -->


---

## Project-Specific Content

<!-- Add your project's toolchain, architecture, workflows here -->
<!-- This section will not be touched by haj.sh -->

### Project: In Amber Clad (IAC)

Multiplayer space strategy game played through an amber terminal (TUI). Humans play via a retro amber CRT interface; LLM agents connect over WebSocket and play via JSON. Both are first-class citizens. The live implementation is Rust; the Zig, full-stack Dart and Numen versions are on `archive/*` branches (see README.md).

### Layout

| Path | What |
|---|---|
| `shared/` | `iac-shared`: protocol, constants, scaling (all cost/time/production formulas, the exchange's fee), pace, hex math, worldgen (and `world::HUB`, the hub's sectors and services), `keymap.rs` (`KeyMap`/`KeySet`: hash maps with a cheap hasher for sector keys and ids; nothing may depend on their order), `maths.rs` (powers, logarithms and exponentials from one implementation on every target; the engine's only source of them) |
| `sim/` | `iac-sim`: pure deterministic game library. `engine.rs` (state, tick, command handlers; outposts: `outpost_site`, the one place a founding is refused and worded, `process_outposts`, `resolve_outpost_raid`), `commands.rs` (`execute`, `answer` for a logged-in session's message, refusal text), `views.rs` (full state / tick update per player), `host.rs` (the host core, the one hosting every host runs: `step`, the only place a tick is placed among the cast's turns and the sessions' frames, with a `Watch` for a host that looks between its phases; `Host`, a world with its roster and its logged-in sessions, whose `receive` is the gate; the `Door`, who may log in, `Tokens` or `Commander`; `Stale`, the one check and wording for a stored world of another format or version), `store.rs` (what a stored world is, for every host: the document, a header line with the versions, a checksum and the host's own part, then the `Snapshot` as JSON, with `encode`, `decode` and `SAVE_FORMAT`; `Store`, a place that holds a document as bytes, `read` and `replace`; `Due`, when a host owes its place a document), `combat.rs`, `odds.rs` (the one ratio and label for a fleet against a group, counting ship classes), `intel.rs`, `queue.rs`, `score.rs`, `auth.rs` (name rules, token checks, a token's text), `snapshot.rs` (the whole world as one value, and everything a host stores of it), `script.rs` (scripted-player trait: `ask`, `act`, `hear`, and `outlook`, what it would order next, for the report; `Seat`, an in-process client; idle player), `bot/` (built-in bots: `mod.rs` personality and attention, `home.rs` homeworld planner, `field.rs` wings, doctrines and pilot, `roster.rs` a world's bots beside the engine, the cast of a hosted world), `headless.rs` (scripted world stepped by the host core, with its counting as the step's `Watch`: score samples, field tallies, `install_outposts` for the report's outposts), `bin/sim-run.rs`. Tests in `sim/tests/` (shared helpers in `common/`, among them `Pilots`, the scripted cast of `determinism.rs` and `chart_equivalence.rs`, and `Memory`, a `Store` that is memory, with `stored`, the world as a host that stopped now would read it back): `determinism.rs`, `bots.rs` (and a world of bots restarted six times through its document), `chart_equivalence.rs`, `stored_world.rs` (one busy world played straight through and stopped and started through its document at the moments most likely to lose something; the two must be one world), `empire_arc.rs` (one empire from registration to a survived raid), `fleet_logistics.rs`, `pace_scaling.rs`, `scenarios_world.rs`, `scenarios_raids.rs`, `exchange.rs` (the hub and its exchange, loading a hold), `outposts.rs` (founding and its refusals, the extractor at three paces, the dock, levels, raids won and lost, breach and repair, a week of raids on outposts defended as their rings ask, what the report counts, a snapshot resumed mid-build, mid-warning and breached), `economy_numbers.rs` (renders `docs/design/economy/numbers.json`, the economy's numbers as the planning simulation reads them, and fails when the file is stale) |
| `server/` | `iac-server`: what a network and a disk add to the host core, and no hosting of its own. `network.rs` (the transport: axum WebSockets + static hosting, a session is a socket with a number, messages wait in a queue until the tick hands them to `Host::receive`), `store.rs` (`Files`, the world's file as a `Store`: replaced whole by write, sync and rename, with `<file>.lock` held while the server runs; `Writer`, the thread that encodes and writes the copies of the world the tick hands it), `auth.rs` (token issuing from OS entropy), `lib.rs` (those three, so tests can read a world file), `main.rs` (clap, opening or creating the world and refusing another seed or pace, the tokio tick loop around `Host::step`, tick timing, when the file is written, `--bot` seats). Tests in `server/tests/`: `smoke.rs` game flow, what a kill and a stop each lose, and the refusals of a world file; `scenarios.rs` multi-client protocol scenarios, most of them across a stop and a start; `web.rs` static hosting + WS paths (each starts its server on `--port 0`, reads the `listening on` line it prints, and keeps its world file in a directory of its own, so checkouts can run them side by side), and `hosts_agree.rs`, the cross-host guard: the file a real server wrote is loaded by `iac-solo`, the same orders on the same ticks, every frame and the bytes of the final world compared (about 45 seconds); `network.rs` keeps a few in-process tests of the real `Network` |
| `solo/` | `iac-solo`: what one player and a browser tab add to the host core, native in tests and WebAssembly in the browser. `machine.rs` (the one door: `Machine::call`, a line of JSON and a body in, the same out), `world.rs` (a `Host` behind the `Commander` door, the clock fed by the caller, catch-up after an absence, and the `Bookmark`, the commander and the clock, which is this host's part of the document of `iac_sim::store`), `away.rs` (what the absence report counts and says, the homeworld's raids and the outpost's apart), `lib.rs` (the three wasm exports), `web/worker.js` (the browser half: loads the wasm, pumps the clock, performs the two verbs of a `Store` on IndexedDB, one-tab lock). Tests in `solo/tests/`, through `Machine::call`: `single_player.rs`, and `both_targets.rs`, which builds `iac_solo.wasm`, runs it in an interpreter (`wasmi`, a dev-dependency) beside the native host and requires the same bytes from both for every request (a blitz of bots, a season day, a commander who trades at the hub and founds an outpost) |
| `clients/tui/` | `iac-client`: ratatui TUI, `--headless` NDJSON mode. It has no tests of its own: `server/tests/scenarios.rs` runs the headless client against a real server (`common::HeadlessClient`, which builds the client's binary with the cargo of the test run) |
| `clients/web/` | Flutter web client on the Rust wire protocol (`lib/protocol/` mirrors `shared/src/protocol.rs`). `lib/state/` logs a `GameController` in to an `Uplink` (a server's WebSocket, or the engine in the tab); `lib/solo/` is the page's side of the engine worker and `lib/views/solo_screen.dart` the single-player front door (build flag `IAC_SINGLE_PLAYER`). Outposts: `lib/console/outpost.dart` (the figures and phrases every screen prints) and `lib/views/outpost_view.dart` (the founding panel, the outpost's panel and its screen); the hold panel of `lib/views/homeworld/economy_tabs.dart` serves both docks. `test/support/demo_world.dart` is a scripted world for the widget tests only, not a back end of the app. `tool/solo_e2e.dart` drives the single-player bundle in headless Chromium |
| `fixtures/` | Golden JSON generated from `shared/`; checked by Rust and Flutter tests |
| `tools/` | `playtest/` (live multi-agent sessions), `check_sim_constants.py` (the formulas `docs/design/economy/sim.py` still computes for itself, against the Rust formulas' tables in `numbers.json`; no constant is compared, they are generated) |
| `docs/` | Design docs; `PORTING.md` and `journal.md` are the Rust dev log. `design/economy/sim.py` is the planning simulation (Python standard library) behind the tables of `design/economy/spec.md`; `design/economy/numbers.json` beside it is generated from Rust and never edited |
| `SPEC.md` | Original technical spec (Zig era; the Rust wire protocol has intentionally diverged) |

### Commands

```sh
cargo build --workspace
cargo test --workspace            # end-to-end tests only; retired unit tests live in the gitignored .archive/. Needs the wasm32-unknown-unknown target: solo/tests/both_targets.rs builds the browser engine
cargo test -p iac-solo --test both_targets   # the same worlds natively and as WebAssembly, reply for reply (about two minutes); run it after touching arithmetic, hashing, sorting or anything sized by usize
cargo test -p iac-server --test hosts_agree   # iac-solo loads the file a real server wrote; both must send the same frames and end with the same bytes (about 45 seconds); run it after touching a host or the stored document
cargo clippy --workspace --all-targets   # also refuses std's powf, powi, ln, exp and their kin in shared, sim and solo
cargo run -p iac-sim --release --bin sim-run -- --pace season --days 7   # headless bot run, balance report with ships lost and refuel deuterium (--seat, --sessions, --ignore-cap, --json; --cap and --meter score the same world under other field-score rules, which no world plays under; --outpost <ring>:<grade>:<level>[:<repairs>] stands an outpost for every bot at the start of day 3, which no bot founds, supplies, defends or hauls from, to measure its extractor, its raids and its store)
cargo build -p iac-solo --target wasm32-unknown-unknown --release   # the browser engine; iac-sim must stay buildable for wasm
cargo run -p iac-server           # port 7777, world.iac, pace x1
cargo run -p iac-server -- --pace blitz --world blitz.iac   # pace and seed are fixed per world; --save-every <seconds> (default 300) is the most a killed server loses
cargo run -p iac-server -- --pace season --world season.iac --bot all:check-in --player-seats 8   # built-in bots, 8 of the 24 home seats kept for players
tail -n +2 world.iac | jq '.players | length'           # a world file is a header line, then the Snapshot as JSON
UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers   # after changing an economy number: render docs/design/economy/numbers.json again (cargo test fails while it is stale)
python3 tools/check_sim_constants.py                    # after touching costs, times or production: the formulas sim.py keeps still agree with Rust
python3 docs/design/economy/sim.py --tables             # the planning simulation: --tables, the full report with no flag (about 25 s), --set NAME=value to try a number before changing the game
cargo run -p iac-client -- --name Admiral
cargo run -p iac-client -- --headless --name Agent   # NDJSON on stdin/stdout

UPDATE_FIXTURES=1 cargo test -p iac-shared   # regenerate fixtures/ after a protocol change

cd clients/web && flutter pub get && flutter analyze && flutter test
scripts/play.sh                   # build web client + serve it and WS on one port (0.0.0.0:${PORT:-7777})
cargo run -p iac-server -- --host 0.0.0.0 --web-dir clients/web/build/web   # same, without rebuilding
scripts/play.sh --single-player   # static single-player bundle (client + iac_solo.wasm) in clients/web/build/single-player, served on 127.0.0.1:${PORT:-7777}; --no-serve only builds
cd clients/web && dart run tool/solo_e2e.dart   # that bundle end to end in headless Chromium (about three minutes; needs the bundle, and target/debug/iac-server for its multiplayer check)
```

The server serves static files (SPA fallback, COOP/COEP headers) and WebSockets (any path; `/ws` for web, `/` for TUI) on one port.

Flutter lives at `~/development/flutter/bin` on the dev machine. That machine mixes Cortex-A720 and A520 cores, and the Dart VM misbehaves when a test run wanders onto the A520s (CPUs 2-5): unpinned, `flutter test` fails about two full runs in three, one different test each time, on assertions inside Flutter's own layout and text code. Pin it to the big cores: `taskset -c 0,1,6-11 flutter test`.

### Architecture

```
Server (Rust, tokio)       Clients
  Tick loop (1Hz) --> WebSocket JSON --> TUI client (ratatui)
  world.iac       --> WebSocket JSON --> headless/LLM client (NDJSON)
  (one document)  --> WebSocket JSON --> Flutter web client (served by the same port)

Single player (one browser tab, no server)
  Web Worker: worker.js + iac_solo.wasm --> the same JSON, by postMessage --> Flutter web client
  wall clock -> ticks, IndexedDB (the same document)
```

- Server runs the authoritative simulation at 1 tick/second; clients send commands and receive `full_state` / `tick_update`.
- All rules live in `iac-sim`, which must stay pure: no tokio, axum, files, wall clock (`std::time`) or OS entropy (`sim/clippy.toml` bans clock reads). Time is ticks; every RNG is seeded from the world seed, tick or ids; entity maps are `BTreeMap` so passes walk in id order.
- A world is stored whole, as one document, and the engine knows nothing of it. Anything that must survive a restart goes in `Snapshot`, and nowhere else: there is no schema, no row, no dirty mark and no second list of fields to keep in step. A host takes `GameEngine::snapshot()` between two ticks, `iac_sim::store::encode`s it with its own part of the header (the single-player host's commander and clock; nothing for the server) and puts the bytes in its place (`Store`: `server/src/store.rs` over a file, `solo/web/worker.js` over IndexedDB); starting is `decode`, which refuses another `SAVE_FORMAT`, economy or generator and a document that fails its checksum, then `GameEngine::restore`. The two hosts store the same document, so `SAVE_FORMAT` is bumped when the header or what a snapshot must hold changes, and nothing is migrated. Do not put anything in the engine that exists for storage.
- What a stop costs is the hosts' to say, with `store::Due` (a document is owed a period after the last, or sooner once hurried, but never sooner than a short gap after the last). The server writes its file at every start once the bots are seated, every `--save-every` seconds (300), within `REGISTRATION_SAVE` (2 s) of the last write once a token was issued, and at SIGINT or SIGTERM. So a stopped server loses nothing; a killed one comes back as its last file says and loses at most the period, orders included, and a registration at most about three seconds. There is no journal of inputs (the owner's decision of 2026-10-13; `docs/design/storage.md` is how to add one). The tick only copies the world out; encoding and the disk are the `Writer` thread's. The single-player host saves every four minutes, two seconds after an order and every twenty seconds while catching up, and replays the ticks since its save on reload.
- A world plays the same on every target: the same seed and orders give the same bytes from a native host and from `iac_solo.wasm`. So float maths in `shared/`, `sim/` and `solo/` is written with what IEEE 754 fixes (the four operations, `sqrt`, rounding, casts) and, for everything else, with `iac_shared::maths` (`powf`, `powi`, `ln`, `log2`, `exp`, `exp2`: the `libm` crate and a fixed order of multiplication, never the platform's library); a function that module lacks is added to it. No `cfg` may pick arithmetic by target, and nothing may depend on the width of `usize` or on the order of a hash map. The `clippy.toml` of each of the three crates bans the std methods, and `solo/tests/both_targets.rs` plays the worlds on both targets and compares every reply. A change that moves what a stored world would play next bumps `ECONOMY_VERSION`.
- Wire protocol types are defined once in `shared/` and used by server and TUI. The Flutter client mirrors them by hand in `clients/web/lib/protocol/`; `fixtures/` catches drift on both sides, so a protocol change means: edit Rust, regenerate fixtures, fix Dart until `flutter test` passes.
- A number of the economy is written once, in Rust (`shared/src/constants.rs`, `scaling.rs`, `pace.rs`, `sim/src/score.rs`). The planning simulation `docs/design/economy/sim.py` declares none: it reads `docs/design/economy/numbers.json`, which `sim/tests/economy_numbers.rs` renders and `cargo test` refuses when stale. Never type a number of the game into `sim.py` or edit the JSON. A number the simulation needs is named in Rust, added to the `named!` list of that test (or to one of its tables) and bound to a name in `sim.py`; a curve it only reads at whole rings, ratings or levels is rendered as a table by calling the Rust function. A number written out in `sim.py` is the simulation's own (the player model, the session schedules), except `zone_terrain` and `ore_prob`, a hand copy of what worldgen rolls. The file's `tables` half is what the Rust formulas give where `sim.py` computes for itself (a formula at any pace or score); `tools/check_sim_constants.py` compares those. Changing a number moves the game: the same rules about `ECONOMY_VERSION` apply.
- Only modified sectors are held (`Snapshot::sector_overrides`); unvisited sectors are generated from the world seed, which is why a world keeps the seed it was created with and the server refuses another.
- Built-in bots are ordinary accounts named `BOT-<personality>-<n>` that act only through the command path; their `BotRecord` lives in `Snapshot`. They are listed apart from the ranking (`LeaderboardReply.bots`).
- An outpost is not a field of `Player`: `GameEngine::outposts` and `Snapshot::outposts` are keyed by the owner. Bots do nothing with outposts, and a world in which nobody founds one must play as it did before they existed.
- Hosting is written once, in `iac_sim::host`, and every host runs it: the server, `iac-solo`, the headless runner, and the tests that play a world with bots. One tick is `host::step` and nothing else: the cast's turns, `tick`, its events drained, every logged-in session's `tick_update`, the cast observes. One gate is `Host::receive`: a logged-in session's message goes to `GameEngine::answer`, `auth` goes to the `Door` and is followed by a full state, anything else from outside gets `commands::not_logged_in`. A host may differ from another only on two axes. Solo against multiplayer: the `Door` (`Tokens`, which registers and mints, or `Commander`, one name and no token) and the transport. Native against web: the clock and the place the document is kept, not the document. Do not write a tick loop, a session map, a login or a stored format in a host; if a host needs to look inside a tick, it gives `step` a `Watch`. `server/tests/hosts_agree.rs` holds the two real hosts to it: the single-player host loads the file the server wrote, and every frame and the bytes of the final world are compared. It cannot catch a change to the shared order itself, which moves both hosts together; `sim/tests/chart_equivalence.rs` and `bots.rs` do.
- `iac-solo` differs from the server only where one player and no network make it differ: no token (the world knows its commander), and a clock it is told (`now_ms` on every request), from which it owes one tick a second. More than 5 ticks behind is an absence: the missed ticks are played without frames, up to 7 days, then the session gets a full state and `Alert` events that report it. The save is the document a server stores, with the commander and the clock in the header's `host` part; ticks not yet saved are replayed on reload and must come out identical, so nothing but the commander's orders may enter a single-player world from outside.

### Key Concepts

- **Hex grid**: Axial coords (q,r), infinite, procedurally generated from `hash(world_seed, q, r)`
- **Zones**: Central Hub (0), Inner Ring (1-8), Outer Ring (9-20), The Wandering (21+)
- **Hub and exchange**: the centre sector is a neutral post (`world::HUB`, sent in `WorldInfo.hub`): no ore, hostile group, derelict or fight. A fleet standing there trades one resource in its hold for another (`exchange`, quoted first by `exchange_quote`); a per-player meter on the `Player` sets the fee, and the exchange never returns more than it takes. A hold is filled from the stockpile at the home dock by `load` (one resource an order, refused unless met in full) and stays aboard (`Fleet.laden`) until the fleet's next jump or `unload`; a lost raid counts cargo aboard fleets at home with the stockpile. Its four `EXCHANGE_*` constants are provisional and live in `shared/src/constants.rs`
- **Outposts**: one station per empire on a sector of rings 9 to 20, founded from what a fleet carries there (`found_outpost`; `outpost_survey` quotes a site and says why it cannot be founded now). One level, 1 to 3, sets its store, its platform and its extractor, which reads the ore the sector was generated with. It is a dock: `load` and `unload` there move cargo to and from its store, a fleet flown by hand is refuelled from that store, charts are banked, and a fleet standing there is not deployed; its sector holds no group while it stands. Pirates of its ring raid it on the homeworld's raid clock; a lost raid takes a quarter above what the next repair would take, a quarter of the ships there, and breaches it until it has repaired itself from its store. The repair is priced in metal and paid in whatever the store holds, metal first, then crystal at 1.5 and deuterium at 2 (`scaling::outpost_repair_payment`); the owner chose this over a repair that waits for all three resources. No raid destroys it. What a client would otherwise work out is the engine's to say and is in the state: why an upgrade would be refused now (`upgrade_refusal`, as `OutpostSurvey.refusal` is for founding), the least and the most a raid of its ring can be (`raid_power`), the yield a full store is losing (`yield_lost`), what a waiting repair lacks, counted in metal (`repair_waiting_for`), that a fleet stands at it in any state (`FleetState.at_outpost`), and a threat rating of 0 for its sector; a survey of a sector never seen is a reply without `site`, not an error. Standing orders, bots and `recall` do not use it. Built on the recommendations of `docs/design/outposts.md`, whose six questions the owner has not answered (`docs/design/decisions.md`); every `OUTPOST_*` constant is provisional and lives in `shared/src/constants.rs`
- **Edge pruning**: Connectivity decreases with distance, creating maze-like deep space
- **Resources**: Metal, Crystal, Deuterium (passive homeworld production + active harvesting), capped by the Storage Vault
- **Pace**: one per-world multiplier (`shared/src/pace.rs`) scales economy timers; every duration constant declares a `PaceClass`
- **Combat**: Fleet-based, stochastic rounds per tick, OGame-style rapid-fire
- **Standing orders**: Per-fleet policy presets run by the server (`policy_update`), each with a range, an engage ratio and a fuel budget (`fuel_budget_pct` of deuterium income); every act and hold carries its reason (`PolicyAction`). Rules as players read them: `tools/playtest/RULES.md`
- **Derelicts, raids, scanning**: see the "The game" section of README.md

### Key Files

- `README.md` -- overview, quick start, headless/agent protocol
- `SPEC.md` -- original technical specification
- `docs/journal.md` -- Rust development log and decisions
- `docs/data_architecture.md` -- what a stored world is and what each host does to store one; `docs/design/storage.md` -- the study behind it, with the journal that was not built
- `docs/auth_spec.md` -- auth design
