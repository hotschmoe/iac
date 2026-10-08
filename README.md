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
| `clients/web/` | Flutter web client (amber CRT UI) - in progress, see below |
| `docs/` | Design docs, auth spec, data architecture, HTML mockups, Rust port plan and dev journal |
| `SPEC.md` | Original technical specification (world model, combat, economy, networking) |

## Quick start

```sh
# Terminal 1: the server (1 Hz tick loop, SQLite persistence)
cargo run -p iac-server                  # port 7777, iac_world.db

# Terminal 2: the TUI client
cargo run -p iac-client -- --name Admiral
```

TUI basics: `w` windshield / `m` star map / `b` homeworld / `` ` `` command
center, `?` for all keybinds. In the windshield: `1-6` move, `v` scan,
`h` harvest, `a` attack, `s` salvage, `r` recall, `S` stop.

## Headless / LLM agent mode

The game is fully playable without a terminal. Headless mode turns the
client into an NDJSON pipe:

```sh
cargo run -p iac-client -- --headless --name MyAgent [--token secret]
```

- **stdout** — every server message, one JSON object per line: `auth_result`,
  `full_state`, `tick_update` (with `events`), `error`. A final
  `{"type":"connection_closed"}` is emitted if the server hangs up.
- **stdin** — one command per line. Full client messages
  (`{"type":"command","action":"scan","fleet_id":2}`) or bare commands
  (`{"action":"scan","fleet_id":2}`) both work. Malformed lines come back
  in-band as `{"type":"client_error",...}`.

Auth is automatic from `--name`; the same name always resumes the same
empire. Example session:

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
| `harvest` | `fleet_id`, `resource` | mine the current sector until cargo is full |
| `attack` | `fleet_id`, `target_fleet_id` | engage an NPC fleet |
| `collect_salvage` | `fleet_id` | scoop wreckage |
| `recall` | `fleet_id` | emergency-jump home (2× fuel, hull risk) |
| `stop` | `fleet_id` | abort a move, harvest, or boarding |
| `explore_site` | `fleet_id` | board the derelict in the current sector (20t, ambush risk) |
| `build` | `building_type` | queue a homeworld building upgrade |
| `research` | `tech` | queue research |
| `build_ship` | `ship_class`, `count` | queue ship production |
| `cancel_build` | `queue_type` | cancel with 50% refund |

Standing orders are a separate message type (not a command):

```json
{"type":"policy_update","fleet_id":2,"preset":"salvage_and_sites",
 "params":{"min_fuel_pct":30,"cargo_return_pct":85,"max_range":4,"engage_ratio_x10":12}}
```

Presets: `manual` (clears orders), `prospect` (scan + push the frontier),
`mine_and_return` (shuttle ore between a claim and home), `salvage_and_sites`
(scoop wreckage, board derelicts), `patrol_home` (hunt hostiles near home).
`params` is optional. Every autopilot act emits a `PolicyAction` event with a
human-readable `reason` — an audit trail for agents (and captains).

## Web client (in progress)

`clients/web/` is a Flutter web app with the amber CRT look (windshield, star map, command center, demo mode). It was written against a Dart server's protocol, not the Rust wire protocol, so it currently runs in demo mode only. Rewiring it to the Rust server is the next step. It keeps a local copy of the old protocol package at `clients/web/packages/iac_shared` until then.

```sh
cd clients/web && flutter pub get && flutter analyze
flutter run -d chrome
```

## The game

- **Economy** — metal / crystal / deuterium from homeworld mines and fleet
  harvesting. Sector deposits deplete and regenerate.
- **Buildings** — 8 types incl. Shipyard, Research Lab, Sensor Array (passive
  reveal around home) and Defense Grid (see raids).
- **Research** — 12 techs: hulls, shields, weapons, fuel, navigation, ship
  class unlocks, emergency jump.
- **Fleets** — up to 3 deployed, 64 ships each, hex movement with fuel mass
  costs, auto-dock/merge/refuel at home.
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
