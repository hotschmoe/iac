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
| `shared/` | `iac-shared`: protocol, constants, scaling (all cost/time/production formulas, the exchange's fee), pace, hex math, worldgen (and `world::HUB`, the hub's sectors and services), `keymap.rs` (`KeyMap`/`KeySet`: hash maps with a cheap hasher for sector keys and ids; nothing may depend on their order) |
| `sim/` | `iac-sim`: pure deterministic game library. `engine.rs` (state, tick, command handlers), `commands.rs` (`execute`, `answer` for a logged-in session's message, refusal text), `views.rs` (full state / tick update per player), `combat.rs`, `odds.rs` (the one ratio and label for a fleet against a group, counting ship classes), `intel.rs`, `queue.rs`, `score.rs`, `auth.rs` (name rules, token checks), `persist.rs` (`Persist` trait, `PersistBatch`), `snapshot.rs`, `script.rs` (scripted-player trait: `ask`, `act`, `hear`; `Seat`, an in-process client; idle player), `bot/` (built-in bots: `mod.rs` personality and attention, `home.rs` homeworld planner, `field.rs` wings, doctrines and pilot, `roster.rs` a world's bots beside the engine), `headless.rs` (scripted world, score samples, field tallies), `bin/sim-run.rs`. Tests in `sim/tests/` (shared helpers in `common/`): `determinism.rs`, `bots.rs`, `chart_equivalence.rs`, `empire_arc.rs` (one empire from registration to a survived raid), `fleet_logistics.rs`, `pace_scaling.rs`, `scenarios_world.rs`, `scenarios_raids.rs`, `exchange.rs` (the hub and its exchange, loading a hold) |
| `server/` | `iac-server`: thin host. `network.rs` (axum WebSockets + static hosting), `database.rs` (SQLite, writer thread, `load_snapshot`), `auth.rs` (token issuing), `main.rs` (clap, tokio tick loop, bot roster). Tests in `server/tests/`: `smoke.rs` game flow, `scenarios.rs` multi-client protocol scenarios, `web.rs` static hosting + WS paths; `network.rs` and `database.rs` keep a few in-process tests of the real `Network` and SQLite |
| `solo/` | `iac-solo`: the single-player host, native in tests and WebAssembly in the browser. `machine.rs` (the one door: `Machine::call`, a line of JSON and a body in, the same out), `world.rs` (engine, roster, the commander's session, the clock fed by the caller, catch-up after an absence), `away.rs` (what the absence report counts and says), `save.rs` (the save document, refusal of other versions), `lib.rs` (the three wasm exports), `web/worker.js` (the browser half: loads the wasm, pumps the clock, IndexedDB save, one-tab lock). Tests in `solo/tests/single_player.rs`, through `Machine::call` |
| `clients/tui/` | `iac-client`: ratatui TUI, `--headless` NDJSON mode |
| `clients/web/` | Flutter web client on the Rust wire protocol (`lib/protocol/` mirrors `shared/src/protocol.rs`). `lib/state/` logs a `GameController` in to an `Uplink` (a server's WebSocket, or the engine in the tab); `lib/solo/` is the page's side of the engine worker and `lib/views/solo_screen.dart` the single-player front door (build flag `IAC_SINGLE_PLAYER`). `test/support/demo_world.dart` is a scripted world for the widget tests only, not a back end of the app. `tool/solo_e2e.dart` drives the single-player bundle in headless Chromium |
| `fixtures/` | Golden JSON generated from `shared/`; checked by Rust and Flutter tests |
| `tools/` | `playtest/` (live multi-agent sessions), `check_sim_constants.py` (Rust economy numbers vs `docs/design/economy/sim.py`) |
| `docs/` | Design docs; `PORTING.md` and `journal.md` are the Rust dev log |
| `SPEC.md` | Original technical spec (Zig era; the Rust wire protocol has intentionally diverged) |

### Commands

```sh
cargo build --workspace
cargo test --workspace            # end-to-end tests only; retired unit tests live in the gitignored .archive/
cargo clippy --workspace --all-targets
cargo run -p iac-sim --release --bin sim-run -- --pace season --days 7   # headless bot run, balance report with ships lost and refuel deuterium (--seat, --sessions, --ignore-cap, --json)
cargo build -p iac-solo --target wasm32-unknown-unknown --release   # the browser engine; iac-sim must stay buildable for wasm
cargo run -p iac-server           # port 7777, iac_world.db, pace x1
cargo run -p iac-server -- --pace blitz --db blitz.db   # pace is fixed per database
cargo run -p iac-server -- --pace season --db season.db --bot all:check-in --player-seats 8   # built-in bots, 8 of the 24 home seats kept for players
python3 tools/check_sim_constants.py                    # after touching costs, times or production
cargo run -p iac-client -- --name Admiral
cargo run -p iac-client -- --headless --name Agent   # NDJSON on stdin/stdout

UPDATE_FIXTURES=1 cargo test -p iac-shared   # regenerate fixtures/ after a protocol change

cd clients/web && flutter pub get && flutter analyze && flutter test
scripts/play.sh                   # build web client + serve it and WS on one port (0.0.0.0:${PORT:-7777})
cargo run -p iac-server -- --host 0.0.0.0 --web-dir clients/web/build/web   # same, without rebuilding
scripts/play.sh --single-player   # static single-player bundle (client + iac_solo.wasm) in clients/web/build/single-player, served on 127.0.0.1:${PORT:-7777}; --no-serve only builds
cd clients/web && dart run tool/solo_e2e.dart   # that bundle end to end in headless Chromium (about two minutes; needs the bundle, and target/debug/iac-server for its multiplayer check)
```

The server serves static files (SPA fallback, COOP/COEP headers) and WebSockets (any path; `/ws` for web, `/` for TUI) on one port.

Flutter lives at `~/development/flutter/bin` on the dev machine.

### Architecture

```
Server (Rust, tokio)       Clients
  Tick loop (1Hz) --> WebSocket JSON --> TUI client (ratatui)
  SQLite (state)  --> WebSocket JSON --> headless/LLM client (NDJSON)
                  --> WebSocket JSON --> Flutter web client (served by the same port)

Single player (one browser tab, no server)
  Web Worker: worker.js + iac_solo.wasm --> the same JSON, by postMessage --> Flutter web client
  wall clock -> ticks, IndexedDB (save)
```

- Server runs the authoritative simulation at 1 tick/second; clients send commands and receive `full_state` / `tick_update`.
- All rules live in `iac-sim`, which must stay pure: no tokio, axum, rusqlite, wall clock (`std::time`) or OS entropy (`sim/clippy.toml` bans clock reads). Time is ticks; every RNG is seeded from the world seed, tick or ids; entity maps are `BTreeMap` so passes walk in id order. Persistence is the `Persist` trait; the server loads SQLite into a `Snapshot` and calls `GameEngine::restore`. Anything that must survive a restart goes in `PersistBatch` and the SQLite schema as well as in `Snapshot`.
- Wire protocol types are defined once in `shared/` and used by server and TUI. The Flutter client mirrors them by hand in `clients/web/lib/protocol/`; `fixtures/` catches drift on both sides, so a protocol change means: edit Rust, regenerate fixtures, fix Dart until `flutter test` passes.
- Only modified sectors persisted; unvisited sectors generated from the world seed.
- Built-in bots are ordinary accounts named `BOT-<personality>-<n>` that act only through the command path; their `BotRecord` lives in `Snapshot` and in `runtime_state`. They are listed apart from the ranking (`LeaderboardReply.bots`).
- There are two hosts of `iac-sim` and they must answer alike: a logged-in session's message goes through `GameEngine::answer` in both, and each tick is bots' turns, `tick`, the session's `tick_update`, bots observe. `iac-solo` differs only where one player and no network make it differ: no token (the world knows its commander), and a clock it is told (`now_ms` on every request), from which it owes one tick a second. More than 5 ticks behind is an absence: the missed ticks are played without frames, up to 7 days, then the session gets a full state and `Alert` events that report it. The save is a whole `Snapshot` with a versioned header; ticks not yet saved are replayed on reload and must come out identical, so nothing but the commander's orders may enter a single-player world from outside.

### Key Concepts

- **Hex grid**: Axial coords (q,r), infinite, procedurally generated from `hash(world_seed, q, r)`
- **Zones**: Central Hub (0), Inner Ring (1-8), Outer Ring (9-20), The Wandering (21+)
- **Hub and exchange**: the centre sector is a neutral post (`world::HUB`, sent in `WorldInfo.hub`): no ore, hostile group, derelict or fight. A fleet standing there trades one resource in its hold for another (`exchange`, quoted first by `exchange_quote`); a per-player meter on the `Player` row sets the fee, and the exchange never returns more than it takes. A hold is filled from the stockpile at the home dock by `load` (one resource an order, refused unless met in full) and stays aboard (`Fleet.laden`) until the fleet's next jump or `unload`; a lost raid counts cargo aboard fleets at home with the stockpile. Its four `EXCHANGE_*` constants are provisional and live in `shared/src/constants.rs` and `docs/design/economy/sim.py`
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
- `docs/data_architecture.md`, `docs/auth_spec.md` -- persistence and auth design
