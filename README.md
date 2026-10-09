# IN AMBER CLAD

**A multiplayer space strategy game played through an amber terminal.**

You are an admiral at a CRT console, commanding fleets across an infinite hex grid. Mine resources, build your homeworld, research technology, and push deeper into the wandering, where the map grows sparse, dead-ends can trap you, and Morning Light Mountain swarms hunt in the dark.

Humans play through a retro amber TUI. LLM agents connect over WebSocket and play the same game via JSON. Both are first-class citizens. Think OGame x The Infinite Black, with a 1 Hz tick loop hidden inside cooldowns so real-time play feels responsive to humans and tractable for LLMs.

## Layout

| Path | What |
|---|---|
| `shared/` | `iac-shared`: protocol, constants, scaling, hex math, world generation |
| `server/` | `iac-server`: authoritative engine, combat, WebSocket network, SQLite persistence |
| `clients/tui/` | `iac-client`: ratatui amber TUI plus headless NDJSON mode for agents |
| `clients/web/` | Flutter web client (amber CRT UI), served by `iac-server` |
| `docs/` | Design docs, auth spec, data architecture, HTML mockups, Rust port plan and dev journal |
| `tools/` | `playtest/` (live multi-agent sessions) and `check_sim_constants.py` (Rust economy numbers vs `docs/design/economy/sim.py`) |
| `SPEC.md` | Original technical specification (world model, combat, economy, networking) |

## Quick start

One command, browser play (builds the Flutter web client, then serves it and
the WebSocket from the server on one port, printing the LAN URL):

```sh
scripts/play.sh              # wasm build, falls back to JS; --js / --no-build; PORT=7777 default
```

Or manually:

```sh
# Terminal 1: the server (1 Hz tick loop, SQLite persistence)
cargo run -p iac-server                  # 127.0.0.1:7777, iac_world.db, pace x1 (persistent)
cargo run -p iac-server -- --pace blitz --db blitz.db   # a one-hour world (x600)
# serve the web client too (after `flutter build web --release` in clients/web):
cargo run -p iac-server -- --host 0.0.0.0 --web-dir clients/web/build/web

# Terminal 2: the TUI client
cargo run -p iac-client -- --name Admiral
```

The world pace (`--pace <number|preset>`; presets persistent, fortnight, season,
sprint, dev, blitz, test) is fixed when the database is created and stored in
it. An existing database keeps its pace and the server refuses a different
`--pace`; a database from before the economy rework is refused too (start a new
world with a new `--db`).

The server serves `clients/web/build/web` automatically if it exists
(override with `--web-dir`). WebSockets are accepted on any path (`/ws` for
the web client, `/` for the TUI); `--host` sets the bind address.

TUI basics: `w` windshield / `m` star map / `b` homeworld / `` ` `` command
center, `?` for all keybinds. In the windshield: `1-6` move, `v` scan,
`h` harvest, `a` attack, `s` salvage, `r` recall, `S` stop.

## Headless / LLM agent mode

The game is fully playable without a terminal. Headless mode turns the
client into an NDJSON pipe:

```sh
cargo run -p iac-client -- --headless --name MyAgent [--token <token>]
```

- **stdout** — every server message, one JSON object per line: `auth_result`,
  `full_state`, `tick_update` (with `events`), `error`, plus the replies to
  `preview_move` and `leaderboard`. Every `tick_update`
  carries the complete `fleets` list: a fleet missing from it is gone, and
  `"fleets":[]` means every fleet was lost. A final
  `{"type":"connection_closed"}` is emitted if the server hangs up.
- **stdin** — one command per line. Full client messages
  (`{"type":"command","action":"scan","fleet_id":2}`) or bare commands
  (`{"action":"scan","fleet_id":2}`) both work; a line with a `type` field is
  parsed as a message, one with an `action` field as a command. Malformed
  lines come back in-band as `{"type":"client_error","message":"unparseable input: ..."}`
  carrying the real parse error (the field and shape that failed).

### Accounts and tokens

A name is an account, protected by a token the server generates (256 random
bits, hex). There is no separate register step:

- **First login with a new name** creates the account. The `auth_result`
  carries the token, once: `{"type":"auth_result","success":true,"player_id":4,"token":"9f2c…"}`.
- Agents: the headless client logs in with `"agent": true`, which labels the
  account on the leaderboard (read once, at creation; it never affects score).
- **Later logins** must present it (`--token`, or `"token"` in the `auth`
  message). A known name without a token fails with code `TokenRequired`,
  a wrong one with `InvalidToken`; an unusable new name gets `InvalidName`
  (3-24 chars of `A-Za-z0-9_-`, no leading/trailing `-`/`_`, a few reserved
  words). Failures arrive as `{"type":"auth_result","success":false,"code":"TokenRequired","message":"…"}`.
- **Saved automatically**: the TUI and headless client store the token per
  server and name in `~/.config/iac/tokens.json` (`$XDG_CONFIG_HOME/iac/`;
  mode 0600) and reuse it. `--token` overrides the saved one. A refused
  login prints the `auth_result` (with its `code`) on stdout, the reason on
  stderr, and exits 1.
- **Servers upgraded from before tokens**: existing players have no token
  yet; the next login for that name claims the account and is issued one.
  Claim your name promptly: whoever logs in first owns it.
- Token lost = account lost (see `docs/auth_spec.md`); the server stores only
  its SHA-256.

Example session:

```sh
{ echo '{"type":"request_full_state"}'
  sleep 2
  echo '{"action":"scan","fleet_id":2}'
  sleep 3
} | iac-client --headless --name Probe | jq -c 'select(.type=="tick_update") | .events[]?'
```

### Command reference (stdin)

| action | fields | effect |
|--------|--------|--------|
| `move` | `fleet_id`, `target:{q,r}` | jump to a connected sector (fuel, cooldown) |
| `scan` | `fleet_id` | reveal nearby sectors + faint signal contacts; scouts scan farther |
| `harvest` | `fleet_id`, `resource` (`Metal`, `Crystal`, `Deuterium`, `Auto`) | mine the current sector until cargo is full; `Auto` takes everything present, a named resource only that one (`ResourceNotPresent` if the sector lacks it) |
| `attack` | `fleet_id`, `target_fleet_id` | engage an NPC fleet |
| `collect_salvage` | `fleet_id` | scoop wreckage |
| `recall` | `fleet_id` | emergency-jump home (2× fuel, hull risk) |
| `stop` | `fleet_id` | abort a move, harvest, or boarding |
| `explore_site` | `fleet_id` | board the derelict in the current sector (20t, ambush risk) |
| `build` | `building_type` | upgrade a homeworld building one level: starts at once when a building slot is free and you can pay, otherwise waits in the queue (nothing is charged until it starts) |
| `research` | `tech` | research one level, same rules (one lab) |
| `build_ship` | `ship_class`, `count` | queue ship production (one shipyard; the batch is paid when it starts) |
| `build_defence` | `kind` (`PulseTurret`, `LancerBattery`, `IonBastion`), `count` | queue home defence structures in the shipyard queue; `DefenceLocked` until the Defense Grid and research it needs are in place |
| `cancel_build` | `queue_type`, `index` (default 0) | cancel an item that has started (the `index`th running one) with a 50% refund; waiting items behind it move up a level |
| `cancel_queued` | `queue_type`, `index` | remove an item still waiting (the `index`th, from 0); nothing was paid, so nothing comes back |
| `preview_move` | `fleet_id`, `target:{q,r}` | do not move: reply `preview_move` with `fuel_cost`, `fuel_after`, `can_jump`, `hops_home`, `fuel_to_return`, `can_return`, plus `threat` (`rating`, `est_power`, `basis`), `fleet_power`, `ratio` and `label` (`safe`, `favourable`, `risky`, `deadly`) |
| `leaderboard` | `limit` (default 20) | reply `leaderboard`: `entries` of `{rank,name,agent,score,core,combat,explore}`, plus `you` when you are below the limit |
| `split` | `fleet_id`, `ship_ids:[id,...]` | detach those ships into a new fleet in the same sector (ids from the fleet's `ships`); at home this launches docked ships. At least one ship stays; fleet cap 8 in all, 3 away from home. The new fleet id arrives in an `Alert` event |
| `merge` | `fleet_id`, `other_fleet_id` | fold the other fleet into `fleet_id` (same sector, both idle); ships, cargo and fuel pool, the other fleet's standing orders end |

Standing orders are a separate message type (not a command):

```json
{"type":"policy_update","fleet_id":2,"preset":"salvage_and_sites",
 "params":{"min_fuel_pct":10,"cargo_return_pct":85,"max_range":4,"engage_ratio_x10":12}}
```

`params` is optional, and so is every field in it: send only what you want to
change (`"params":{"max_range":2}`) and the rest take their defaults.

| param | default | meaning |
|-------|---------|---------|
| `min_fuel_pct` | 10 | spare fuel, as a percent of the tank, kept on top of the exact fuel cost of the route home (clamped 0-90) |
| `cargo_return_pct` | 85 | head home to unload when the hold is this full (10-100) |
| `max_range` | 4 | farthest the autopilot goes from your HOMEWORLD, in hexes, for claims, prospecting and salvage runs (1-30); never measured from the last claim |
| `engage_ratio_x10` | 12 | fight only if your power is at least enemy power x this/10 (1-100) |

Fuel safety: the autopilot returns home when its fuel falls below the cost of
the shortest charted route home plus the `min_fuel_pct` reserve, and it
refuses any outward jump that would leave less than that. A range limit
shorter than `max_range` (the tank cannot afford the round trip) is reported
in the hold reason. Whenever a doctrine stands still it says why in a
`PolicyAction` event (`action: "hold"`).

Presets: `manual` (clears orders), `prospect` (scan + push the frontier),
`mine_and_return` (shuttle ore between a claim and home), `salvage_and_sites`
(scoop wreckage, board derelicts), `patrol_home` (hunt hostiles near home).
Every autopilot act emits a `PolicyAction` event with a human-readable
`reason` — an audit trail for agents (and captains). `prospect` walks to the
nearest sector you have never entered inside `max_range` of home; when none is
left it returns home and holds rather than wandering between known sectors.

## Web client

`clients/web/` is a Flutter web app with the amber CRT look: command center,
windshield and star map, driven by a command bar (`help` lists commands) and
hotkeys (`1`/`2`/`3` views, numpad 1-6 to move, Esc leaves the bar). It speaks
the Rust wire protocol over `/ws` on the page's own host and port.

- `?name=Admiral` skips the name prompt; `?token=...` supplies the account
  token (see "Accounts and tokens"); `?ws=ws://host:port/ws` points at another
  server. The token the server issues is remembered in the browser's
  `localStorage` per server and name; if a name needs a token the browser
  does not have (or storage is blocked), the login form asks for it. A new
  token is also shown once in the event log and alerts, so you can copy it.
- If the server is unreachable it falls back to an offline DEMO mode and keeps
  retrying in the background.

```sh
cd clients/web && flutter pub get && flutter analyze && flutter test
flutter run -d chrome   # dev UI; append ?ws=ws://localhost:7777/ws to reach a running server
```

### Protocol fixtures

`fixtures/` holds golden JSON generated from the Rust types in `shared/`:
every `ClientMessage`, `Command`, `ServerMessage` and `EventKind` variant plus
hex math results. `shared/tests/golden_fixtures.rs` fails if the Rust types
drift from them (regenerate with `UPDATE_FIXTURES=1 cargo test -p iac-shared`),
and the Flutter tests round-trip every fixture and check hex math cell by cell.
Change the Rust protocol, regenerate, then fix the Dart side until
`flutter test` passes.

## The game

- **World pace** — every world has a pace P, fixed when it is created and
  sent in `full_state.world` (`pace`, `preset`, and `estimate`: when a
  competent player gets a first Cruiser and reaches the endgame). Mine output
  and storage caps grow with it, build, research, ship and defence times
  shrink with it (never below one tick); costs, fuel, cooldowns and combat do
  not change. Presets: `persistent` x1 (months), `fortnight` x5, `season` x10
  (a week), `sprint` x70 (a day), `dev` x100, `blitz` x600 (one hour), `test`
  x8000 (twelve minutes). Plan against the numbers in your own `status` and
  `costs`, not against pace-1 intuition.
- **Economy** — metal / crystal / deuterium from homeworld mines and fleet
  harvesting. A mine makes `base x L x 1.1^L` per game hour (metal 10,
  crystal 7, deuterium 5, times P); costs grow x1.5 to x1.9 per level and
  times follow cost, so the late levels are slow in both money and time.
  Sector deposits are small nuggets (see Ore and loot below).
- **Ore** — an ore tile holds 3 / 10 / 22 / 42 raw units (Sparse to Pristine)
  times 1.10 per ring from ring 3 (x5 from ring 20), and the further out the
  richer the density rolls. Harvesting takes units out of the tile (depletion
  counts raw units; Harvesting Efficiency raises what lands in the hold, not
  how fast the tile empties) and the density drops a level at a time.
  A stripped tile refills completely in `120 + 24 x ring` game hours divided
  by P, but not while a fleet sits on it, and never above its generated
  density. Sector views carry `ore_reserve` per resource: `units`,
  `max_units` and `refills_in_s`. Ore is a bonus on top of the mines, not a
  living.
- **Threat** — NPC group power grows 1.22x per ring (`4 x 1.22^(ring-1)`),
  with no cliff; ships, count and stats follow (scouts to ring 5, corvettes to
  12, frigates to 20, cruisers beyond). Every sector carries `threat`:
  `rating` T1 to T9 (one step per doubling of power over 4), `est_power` and
  `basis` (`observed`: a group is there, its own power, Aggressive and Swarm
  +25%; `template`: a lair that is away or cleared; `estimate`: ring only).
  A stale chart entry keeps the rating last seen. Scans list the threat of
  every revealed sector (`threats`) and each faint contact has a
  `threat_band` (1: T1-3, 2: T4-6, 3: T7-9). `preview_move` returns the
  target's `threat`, your `fleet_power`, `ratio` and a `label`: SAFE from 3.0,
  FAVOURABLE from 2.0, RISKY from 1.2, else DEADLY. Pirates return
  `2 + 0.25 x ring` game hours (divided by P) after a kill.
- **Buildings** — 10 types incl. Shipyard, Research Lab, Sensor Array (passive
  reveal around home), Defense Grid (see raids), Storage Vault (stockpile
  caps) and Fabricator (building time / (1 + 0.15 x level)).
- **Storage** — every stockpile is capped at 5000 / 3500 / 2500 x 1.5^Vault
  x P^0.75 (`homeworld.storage`: `cap`, `protected`, `full_in_s`, `capped`).
  Mines stop at the cap and the surplus is lost; a payment bigger than a cap
  is refused (`StorageTooSmall`, naming the Vault level it needs); docked
  cargo unloads only what fits and the rest waits aboard
  (`cargo_blocked: true`). `StorageNearCap` (85%) and `StorageFull` events
  warn you. A Vault also protects 8% of the cap per level (to 60%) from raids.
- **Queues and slots** — building, research and shipyard queues each hold 3
  items (running ones included). Buildings run in `build_slots` parallel
  slots (1, plus one per Modular Fabrication level, max 3); research and the
  shipyard have one. A waiting item costs nothing, may depend on items ahead
  of it (a Shipyard queued behind the Metal Mine it needs), and starts when a
  slot is free, its prerequisites are met and you can pay; a blocked item
  does not hold back the ones behind it. `build_pending` / `research_pending`
  / `shipyard_pending` list the waiting items with `waiting_for` (what is
  still missing). `NoResources` errors name the missing resource and amount.
- **Research** — 13 techs: hulls, shields, weapons, fuel, navigation, ship
  class unlocks (Corvette Tech needs only Shipyard 1, Frigate and Cruiser
  Tech need more), emergency jump, Modular Fabrication (building slots). The Research
  Lab divides research time by (1 + 0.10 x level).
- **Fleets** — up to 3 deployed (8 in all), 64 ships each, hex movement with
  fuel mass costs. Docking at home unloads cargo and refuels but never merges
  fleets: use `split` and `merge`. New ships join an idle fleet at home that
  has no standing orders, or form their own. Each fleet reports its `power`
  (the scale raids use) and `range_hops`: how many more hops out it can fly
  and still get home.
- **Fuel and stranding** — each fleet reports `jump_fuel` (one jump) and
  `home_fuel` (the charted way home) next to `fuel`. Refuelling at the
  homeworld costs 0.08 deuterium per fuel unit (the Fuel Depot adds +25% tank
  per level); with no deuterium the tank fills only as far as the stockpile
  pays and the rest follows as production arrives. `preview_move` answers
  what a jump costs before you make it. A manual jump that leaves
  too little to get home still happens, but raises an `Alert` event
  (warning, or critical if the fleet will be stranded on arrival). A fleet
  away from home with less than one jump of fuel is stranded: its emergency
  reserve slowly recovers exactly one jump (about 5 minutes from empty,
  announced at each quarter), enough to hop toward home, then the wait starts
  over. `merge` with a fresh fleet from home pools fuel and also rescues it.
  `recall` costs 2x the jump fuel per hex, all or nothing.
- **Combat** — round-based vs pirates, shields absorb first, rapid-fire
  chains (corvettes shred scouts…), victors drop salvage (`FleetDestroyed.salvage`
  is exactly the pile `collect_salvage` can take; it lasts 60 ticks and a
  `SalvageDespawned` event says when one expires). You only receive combat
  events for fights your fleets or homeworld are in, or that happen in a
  sector where you have a fleet or your homeworld. Each carries `owner`
  (empire name, `null` for NPCs) and `mine`; losing a fleet also raises a
  `Critical` `Alert`. Hostile ids in a sector view are the ids `attack` and
  `CombatStarted.enemy_fleet_id` use; messages name a hostile by its `label`
  ("3x corvette pack"), not by id. `ResourceHarvested` is one event per fleet
  every 10 ticks (and when harvesting stops).
- **Scanning** — active `scan` reveals connected sectors (2 hops with a Scout
  in fleet, 1 without) and picks up *signals* one hop farther: rich ore,
  hostile mass, derelicts, anomalies. Scan intel is live for ~2 minutes.
  After that the sector stays on your chart: the server remembers every
  sector you have had eyes on, and `full_state.known_sectors` returns all of
  them. Each entry has `live` (true: current this tick; false: remembered) and
  `last_seen` (tick it was last observed). A stale entry shows ore, hostiles,
  wreckage and derelict **as last seen**, which may be out of date (wreckage
  lasts 60 ticks; `salvage_despawn_tick` says when). Tick updates add a
  sector to `sector_updates` once more when it turns stale.
- **Derelicts** — dead ships drift in debris fields and empty space beyond
  the shipping lanes (`D` on the map, "derelict transponder" on scans).
  Board one (`explore_site`, 20 ticks) for tiered loot, sometimes a
  recoverable ship or an instant-tech data core — or an ambush by whatever
  killed the crew. Aborted/failed boardings leave the site *hotter*. A
  Scout aboard reads the risk down; a Hauler doubles the haul.
- **Loot** — a kill pays `1.2 x group_power^0.8` units (55% metal, 30%
  crystal, 15% deuterium) times P^0.5, for the whole group; far hunting pays
  more but the odds fall faster than the pay rises. Derelict tier follows the
  threat (T1-3, T4-6, T7-9). A site pays `240 x 1.14^(ring-4)` units at 60 to
  140% times P^0.5; ambush odds `0.10 + 0.03 x (T-1)` (max 0.35); a recovered
  ship `0.05 x (T-2)` (max 0.30), a data core `0.03 x (T-5)` (max 0.15), and
  from T7 an ancient relic (2% per point above 6, worth 25 explore points, no
  other use yet). A stripped derelict is replaced after 120 game hours / P^0.5.
- **Standing orders** — assign a fleet a doctrine and the server flies it
  (see `policy_update` above). Doctrines respect fuel/cargo thresholds,
  return home to deposit and refuel, and log every decision.
- **Defences** — Pulse Turrets, Lancer Batteries and Ion Bastions are built
  from the shipyard queue, stay home, burn no fuel, and cost 8 to 13
  resources per point of power (ships cost 25 to 48). Each Defense Grid level
  adds 4% to every structure, and the grid's own virtual platforms are small
  now; the structures are the defence. `homeworld.defences` lists them,
  `home_defence_power` is what a raid meets (docked ships + grid +
  structures).
- **Raids** — once your empire is worth raiding (Shipyard ≥ 2 or a Defense
  Grid), pirates occasionally vector at your homeworld: a roll every 6 hours
  at pace 1 (30%), at most one per 18 hours, never in your first day, all
  divided by P^0.75. A raid's power is `5 + 11 x S^0.75` (S = your building
  and research points, ships and defences excluded) times a roll of 0.80 to
  1.15, so it grows with your economy and not with your defence;
  `next_raid_estimate_power` shows what to expect. `RaidIncoming` warns
  `600 / sqrt(P)` ticks ahead (30 to 600) with the raid's `est_power`.
  Docked ships, the grid and structures defend (±10% fog). Repelled: bonus
  salvage in orbit and a share of structures destroyed (up to half, scaled by
  raid over defence power), 70% of them rebuilt free shortly after. Lost: a
  hard-capped slice of the stockpile above the Vault-protected share (≤8%),
  half the structures gone for good, and a long grace period. Never
  buildings, never queues. `RaidResolved` reports `structures_lost`,
  `structures_restored` and `protected_kept`.
- **Score** — one ranking for humans and AI agents (`leaderboard`).
  `core` = completed building and research levels + living ships + half of
  the defence structures, each priced at cost x (1 metal, 1.5 crystal, 2
  deuterium) / 1000. Kills add 25% of the NPC group's weighted cost (nothing
  when your force was 8x theirs, nothing twice in a sector until it
  respawns). Exploration adds `0.15 x loot/1000 + 0.02 x T^1.5` for the first
  boarding of each derelict, 25 for a relic, and `0.01 x T^1.5` for each new
  sector whose chart you deliver to the homeworld (the fleet must dock alive:
  `ChartDelivered`). `score = core + min(combat + explore, 0.25 x core)`. Cancelling
  a build or losing a ship scores nothing; resources received from others
  never count. Accounts that log in with `"agent": true` (the headless client
  does) carry the `agent` flag on the board, which labels and never scores.

## Development

```sh
cargo test --workspace       # engine unit tests + end-to-end smoke tests
cargo clippy --workspace
```

Crates: `iac-shared` (`shared/`) (protocol, constants, scaling, hex math, worldgen) ·
`iac-server` (engine, combat, network, SQLite) · `iac-client` (ratatui TUI +
headless mode).

## Other versions

Earlier implementations are kept on archive branches (each also tagged):

| Branch / tag | What it is |
|---|---|
| `archive/zig` / `archive-zig-final` | Original Zig implementation (zithril TUI, zqlite, webzocket) with the full design history; also holds the old `clients/flutter` client |
| `archive/dart-fullstack` / `archive-dart-fullstack` | Full-stack Dart: Flutter web UI, Dart game server and shared protocol package (orphan branch) |
| `archive/numen-scout` / `archive-numen-scout` | Small slice ("Amber Scout") written in the Numen language (orphan branch) |

The Zig-era design notes, milestones and tech stack live in `README.md` on `archive/zig`; the current spec is `SPEC.md` plus `docs/`.
