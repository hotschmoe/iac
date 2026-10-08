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
cargo run -p iac-server                  # 127.0.0.1:7777, iac_world.db
# serve the web client too (after `flutter build web --release` in clients/web):
cargo run -p iac-server -- --host 0.0.0.0 --web-dir clients/web/build/web

# Terminal 2: the TUI client
cargo run -p iac-client -- --name Admiral
```

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
  `full_state`, `tick_update` (with `events`), `error`. Every `tick_update`
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
| `build` | `building_type` | queue a homeworld building upgrade |
| `research` | `tech` | queue research |
| `build_ship` | `ship_class`, `count` | queue ship production |
| `cancel_build` | `queue_type` | cancel with 50% refund |
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

- **Economy** — metal / crystal / deuterium from homeworld mines and fleet
  harvesting. Sector deposits deplete and regenerate.
- **Buildings** — 8 types incl. Shipyard, Research Lab, Sensor Array (passive
  reveal around home) and Defense Grid (see raids).
- **Research** — 12 techs: hulls, shields, weapons, fuel, navigation, ship
  class unlocks, emergency jump.
- **Fleets** — up to 3 deployed (8 in all), 64 ships each, hex movement with
  fuel mass costs. Docking at home unloads cargo and refuels but never merges
  fleets: use `split` and `merge`. New ships join an idle fleet at home that
  has no standing orders, or form their own.
- **Fuel and stranding** — each fleet reports `jump_fuel` (one jump) and
  `home_fuel` (the charted way home) next to `fuel`. A manual jump that leaves
  too little to get home still happens, but raises an `Alert` event
  (warning, or critical if the fleet will be stranded on arrival). A fleet
  away from home with less than one jump of fuel is stranded: its emergency
  reserve slowly recovers exactly one jump (about 5 minutes from empty,
  announced at each quarter), enough to hop toward home, then the wait starts
  over. `merge` with a fresh fleet from home pools fuel and also rescues it.
  `recall` costs 2x the jump fuel per hex, all or nothing.
- **Combat** — round-based vs pirates, shields absorb first, rapid-fire
  chains (corvettes shred scouts…), victors drop salvage.
- **Scanning** — active `scan` reveals connected sectors (2 hops with a Scout
  in fleet, 1 without) and picks up *signals* one hop farther: rich ore,
  hostile mass, derelicts, anomalies. Intel stays live for ~2 minutes.
- **Derelicts** — dead ships drift in debris fields and empty space beyond
  the shipping lanes (`D` on the map, "derelict transponder" on scans).
  Board one (`explore_site`, 20 ticks) for tiered loot, sometimes a
  recoverable ship or an instant-tech data core — or an ambush by whatever
  killed the crew. Aborted/failed boardings leave the site *hotter*. A
  Scout aboard reads the risk down; a Hauler doubles the haul.
- **Standing orders** — assign a fleet a doctrine and the server flies it
  (see `policy_update` above). Doctrines respect fuel/cargo thresholds,
  return home to deposit and refuel, and log every decision.
- **Raids** — once your empire is worth raiding (Shipyard ≥ 2 or a Defense
  Grid), pirates occasionally vector at your homeworld. You get a warning
  (`RaidIncoming`, ~60 ticks), docked ships + Defense Grid auto-defend,
  repelling drops bonus salvage in orbit, losing costs a hard-capped slice
  of stockpiles (≤8%) — never buildings, never queues, and a lost raid
  buys you a long grace period.

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
