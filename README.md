# IN AMBER CLAD

**A multiplayer space strategy game played through an amber terminal.**

You are an admiral at a CRT console, commanding fleets across an infinite hex grid. Mine resources, build your homeworld, research technology, and push deeper into the wandering, where the map grows sparse, dead-ends can trap you, and Morning Light Mountain swarms hunt in the dark.

Humans play through a retro amber TUI. LLM agents connect over WebSocket and play the same game via JSON. Both are first-class citizens. Think OGame x The Infinite Black, with a 1 Hz tick loop hidden inside cooldowns so real-time play feels responsive to humans and tractable for LLMs.

## Layout

| Path | What |
|---|---|
| `shared/` | `iac-shared`: protocol, constants, scaling, hex math, world generation |
| `sim/` | `iac-sim`: the whole game as a pure, deterministic library (engine, tick, commands, combat, intel, queues, score, snapshots, built-in bots, headless runner `sim-run`); no I/O, builds for WebAssembly |
| `server/` | `iac-server`: a thin host around `iac-sim`: WebSocket network, SQLite persistence, the 1 Hz tick timer |
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
cargo run -p iac-server -- --pace season --db season.db --bot all:check-in --bot mixed:hands-on:2   # with built-in bots
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

**Built-in bots** (`--bot <personality>:<attention>[:<count>]`, repeatable;
personality `turtle`, `miner`, `raider`, `explorer`, `mixed`, attention
`check-in`, `hybrid`, `hands-on`, either may be `all`) are computer players
the server runs beside the engine. `--bot` says which bots the world should
have: missing ones are added at start, in the order listed, and the ones a
world already has keep playing on every later start whether listed or not
(they are accounts of the world, stored in its database with what they
remember). A bot takes one of the 24 home seats. `--player-seats <N>`
(default 4) keeps seats for people: a bot is added only while more than N
seats are free, a roster that asks for more is seated as far as that allows
and the log says how many were left out, and `--player-seats 0` lets bots
fill the world. `--bot-sessions <N>` (default 4) is how often a day new
check-in and hybrid bots look in; a day is a day of ticks at any pace, so in
a blitz world only hands-on bots play through the hour. With `RUST_LOG=info` the server logs every
300 ticks what a tick cost and how much of it the bots took.

To a player a bot is another empire: it registers, builds, flies and fights
through the same commands under the same rules, with no extra information
and no privileges. Its name starts with `BOT-` (`BOT-raider-3`), a prefix
nobody else can register, so it is tagged wherever a player is named: on the
map, in fleet lists, in events. Bots are not ranked with people: the
leaderboard lists them apart (see `leaderboard` below).

TUI basics: `w` windshield / `m` star map / `b` homeworld / `` ` `` command
center, `?` for all keybinds. In the windshield: `1-6` move, `v` scan,
`h` harvest, `a` attack, `s` salvage, `r` recall, `S` stop. `p` cycles the
active fleet's standing orders; `P` steps the fuel budget they are given with
(10, 20, 35, 50 percent of deuterium income, then no bound).

## Headless / LLM agent mode

The game is fully playable without a terminal. Headless mode turns the
client into an NDJSON pipe:

```sh
cargo run -p iac-client -- --headless --name MyAgent [--token <token>]
```

- **stdout** — every server message, one JSON object per line: `auth_result`,
  `full_state`, `tick_update` (with `events`), `error`, plus the replies to
  `preview_move`, `odds` and `leaderboard`. Every `tick_update`
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
  words, and nothing starting with `BOT-` in any case: that prefix marks
  the server's built-in bots), and a new name in a world whose 24 home seats are taken gets
  `WorldFull`. Failures arrive as `{"type":"auth_result","success":false,"code":"TokenRequired","message":"…"}`.
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
| `build` | `building_type`, `reserve` (default false) | upgrade a homeworld building one level: starts at once when a building slot is free and you can pay, otherwise waits in the queue (nothing is charged until it starts). Every order gets a stable `id`, reported in its `Queue` event and in the queue lists. `reserve: true` makes the order hold the line while it is short (see Queues and slots) |
| `research` | `tech`, `reserve` | research one level, same rules (one lab) |
| `build_ship` | `ship_class`, `count`, `reserve` | queue a ship batch (one shipyard). The batch is one order that pays and starts one unit at a time, so 5 ordered with money for 2 builds 2 and waits for the third |
| `build_defence` | `kind` (`PulseTurret`, `LancerBattery`, `IonBastion`), `count`, `reserve` | queue home defence structures in the shipyard queue, paid per unit like ships; `DefenceLocked` until the Defense Grid and research it needs are in place |
| `cancel_build` | `queue_type`, `id` | cancel the item with that `id` once it has started, with a 50% refund (one unit for a ship batch; units already built are kept); waiting items of the same building or tech move up a level; a `Queue` event confirms what was refunded. `InvalidTarget` names where the id actually is (another queue, still waiting, or gone) |
| `cancel_queued` | `queue_type`, `id` | remove the item with that `id` while it still waits; nothing was paid, so nothing comes back; a `Queue` event confirms it. Refused, not redirected, when the id is not waiting in the named queue |
| `preview_move` | `fleet_id`, `target:{q,r}` | do not move: reply `preview_move` with `fuel_cost`, `fuel_after`, `can_jump`, `hops_home` (shortest real route home, charted or not), `charted_hops_home` (shortest route over sectors you have entered, null when there is none), `route_unexplored` (the `hops_home` route crosses sectors you have never entered), `fuel_to_return`, `can_return`, plus `threat` (`rating`, `est_power`, `basis`), `fleet_power`, and the odds `ratio` and `label` (`safe`, `favourable`, `even`, `risky`, `deadly`), all for the fleet you name, not your strongest. `ratio` is not `fleet_power / est_power`: see Odds |
| `odds` | `fleet_id`, `sectors:[{q,r},...]` (at most 256) | do not move: reply `odds` with `fleet_id` and, for each sector in the order asked, `{sector, threat, ratio, label}`: how that fleet would fare there. Any sector may be named: adjacent or far, the one the fleet is in, seen or never seen |
| `leaderboard` | `limit` (default 20) | reply `leaderboard`: `entries` of `{rank,name,agent,score,core,combat,explore}`, plus `you` when you are below the limit. Built-in bots are never in `entries`: when the world has any, `bots` lists them in the same row shape, ranked among themselves, so a rank in `entries` counts humans and AI agents only |
| `split` | `fleet_id`, `ship_ids:[id,...]` | detach those ships into a new fleet in the same sector (ids from the fleet's `ships`); at home this launches docked ships. At least one ship stays; fleet cap 8 in all, 3 away from home. The new fleet id arrives in an `Alert` event |
| `merge` | `fleet_id`, `other_fleet_id` | fold the other fleet into `fleet_id` (same sector, both idle); ships, cargo and fuel pool, the other fleet's standing orders end |

Standing orders are a separate message type (not a command):

```json
{"type":"policy_update","fleet_id":2,"preset":"salvage_and_sites",
 "params":{"min_fuel_pct":10,"cargo_return_pct":85,"max_range":4,"engage_ratio_x10":15,"fuel_budget_pct":20}}
```

`params` is optional, and so is every field in it: send only what you want to
change (`"params":{"max_range":2}`) and the rest take their defaults.

| param | default | meaning |
|-------|---------|---------|
| `min_fuel_pct` | 10 | spare fuel, as a percent of the tank, kept on top of the exact fuel cost of the route home (clamped 0-90) |
| `cargo_return_pct` | 85 | head home to unload when the hold is this full (10-100) |
| `max_range` | 4 | farthest the autopilot goes from your HOMEWORLD, in hexes, for claims, prospecting, salvage runs and patrols (1-30); never measured from the last claim |
| `engage_ratio_x10` | 15 | fight only if the fleet's odds `ratio` against the hostiles is at least this/10 (1-100); also exactly the least ratio at which any doctrine will enter a sector, hostiles there or not. It is the same number `preview_move` and `odds` show. The default is FAVOURABLE. A value below EVEN (1.1) that you set deliberately is honoured |
| `fuel_budget_pct` | 20 | the share of your deuterium income, in percent, that this fleet may spend on fuel while it flies under these orders (0-100; 100 lifts the bound). See "Fuel budget" |

Fuel safety: the autopilot returns home when its fuel falls below the cost of
the shortest route home (charted or not) plus the `min_fuel_pct` reserve, and it
refuses any outward jump that would leave less than that. A range limit
shorter than `max_range` (the tank cannot afford the round trip) is reported
in the hold reason. Whenever a doctrine stands still it says why in a
`PolicyAction` event (`action: "hold"`).

Fuel budget: refuelling is paid in deuterium ahead of every queue, so every
standing order carries a bound on it. The fleet earns an allowance at
`fuel_budget_pct` of your current deuterium production and can save up the
price of one full tank; it starts empty, so with no deuterium income yet a
fleet flies the tank it has and then waits (build a Deuterium Synthesizer). At
the dock the tank is filled in one go once the allowance and the stockpile
cover what is missing. Until then the fleet waits there and says so (`hold`:
"refuelling on a fuel budget of 20% of deuterium income (tank 283 of 406,
full in about 236 s)"); it never leaves on standing orders with a part-filled
tank. While your deuterium store is full the fill is free of the allowance,
since that income would overflow anyway. `100` lifts the bound: the tank takes
what the stockpile holds at once, as it does for a fleet you fly by hand
(which is never bound). The budget is per fleet, so fleets under orders
together take at most the sum of their budgets: three at the default, 60% of
your deuterium income. The `engage` `PolicyAction` that confirms new orders
states the range, ratio and budget they fly with.

Presets:
- `manual` clears the orders.
- `prospect` scans, then walks to the nearest sector inside `max_range` of
  home whose chart you have neither banked nor aboard a fleet (never entered,
  or entered by a fleet that did not come back). With none left it goes home
  and holds, naming whether a hazard, `max_range` or a fully charted board
  stops it.
- `mine_and_return` stakes the nearest seam that repays its fuel, strips it,
  stakes the next from where it stands, and brings the ore home when the hold
  reaches `cargo_return_pct`. With none left it waits at the dock for the
  tiles to refill.
- `salvage_and_sites` scoops wreckage that repays its fuel and boards
  derelicts, nearest first.
- `patrol_home` hunts the nearest hostile group inside `max_range` of home
  that the fleet rates at `engage_ratio_x10`, scoops what the kill leaves, and
  waits at the dock when the beat is clear.

What every doctrine does:
- No doctrine moves a fleet into a sector where its odds (as `preview_move`
  and `odds` show them) are below `engage_ratio_x10`; the setting is the bar
  whether or not hostiles sit there, so lower it to push a scout through,
  raise it to be careful.
- It picks its work among the sectors it may reach. A seam, wreck, derelict
  or contact behind a refused sector is passed over for the next one; only
  when none is left does the fleet go home and hold, and the reason names the
  nearest one it could not have and what stops it (sector, threat, ratio).
- Hull is never repaired and shields are. So a fleet under orders stands
  still after a fight until its shields are back above 95% ("shields
  recharging"); it takes a lane without a fight before a shorter one with a
  fight it would win; and `mine_and_return` and `salvage_and_sites` pick no
  seam, wreck or derelict in a sector whose hostiles attack on sight (a
  patrol can clear it first). A fight on the only lane is taken at
  `engage_ratio_x10` or better.
- A seam or a wreck is flown to only if what the hold can take of it is worth
  1.5 times the deuterium the jumps there and back burn, both at score weight
  (metal 1, crystal 1.5, deuterium 2); a sector with under 3 units of ore
  counts as stripped. Derelicts always count.
- A fleet whose cargo your full stockpile cannot take stays docked until it
  has unloaded.
- Heading home it takes a lane that meets `engage_ratio_x10`; if there is
  none, the mildest there is, down to RISKY, and says so. It does not jump
  into DEADLY odds: it holds where it is ("cut off from home") and you get a
  Warning alert.
- Every act emits a `PolicyAction` event with a human-readable `reason`: an
  audit trail for agents (and captains). A hold is repeated at most once a
  minute while its cause stays the same.

## Web client

`clients/web/` is a Flutter web app with the amber CRT look: command center,
windshield and star map, driven by a command bar (`help` lists commands) and
hotkeys (`1`/`2`/`3` views, numpad 1-6 to move, Esc leaves the bar). It speaks
the Rust wire protocol over `/ws` on the page's own host and port. Standing
orders take their settings on the command bar:
`policy mine range=8 engage=1.5 fuel=40 cargo=85 reserve=10` (any subset;
`fuel` is `fuel_budget_pct`, 100 for no bound).

- `?name=Admiral` skips the name prompt; `?token=...` supplies the account
  token (see "Accounts and tokens"); `?ws=ws://host:port/ws` points at another
  server. The token the server issues is remembered in the browser's
  `localStorage` per server and name; if a name needs a token the browser
  does not have (or storage is blocked), the login form asks for it. A new
  token is also shown once, with a copy button, in the operator guide and
  under Alerts.
- If the server is unreachable it falls back to an offline DEMO mode and keeps
  retrying in the background.
- **Operator guide.** A new account gets a one-line guide under the HUD that
  walks the first steps (save the token, queue a building, scan, jump,
  harvest, unload, standing orders) and a "First hour" list on the Overview.
  Each step is ticked by what happens in the game, in any order. `Skip guide`
  turns it off; `?` (Keys and settings) turns it back on. Progress is kept per
  account in the browser.
- **Notifications.** While the tab is in the background, a finished or idle
  queue, a raid (announced, 60 ticks out, resolved), a lost fleet, a critical
  alert or full storage is counted in the tab title, `(2) IN AMBER CLAD`, and,
  if you turned notifications on, raised as a browser notification. The
  browser is asked for permission only when you press the button for it (the
  one-time offer after your first order starts, or `?` > Notifications).

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
- **Homes** — a world has 24 home seats: the sectors of rings 3 to 5 whose
  `q - r` is a multiple of three (6 in ring 3, 6 in ring 4, 12 in ring 5), so
  no two homes touch and the nearest seats are two hexes apart. A new empire
  gets the free seat farthest from every occupied one, ties drawn from the
  world seed; nobody chooses. When the seats are gone a new name is refused
  with `WorldFull` and the world is left as it was; its empires log in as
  before. No hostile group lives on a seat or on the six sectors beside one
  (convoys, which never shoot first, still do), and no derelict lies on a
  seat. Lanes are drawn from the world seed: every world has its own maze.
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
  and a group's ships add up to exactly that (to the 32-ship limit at ring
  37), so the ring estimate is the real strength of an unscouted lair. The
  Inner Ring is scout country: scouts to ring 8, corvettes to 12, frigates to
  20, cruisers beyond. The step in danger is at ring 9, where rapid fire
  starts: the two starting scouts win every lair through ring 8 (keeping
  about 40% of their hull there) and lose every one from ring 9. Every
  sector carries `threat`:
  `rating` T1 to T9 (one step per doubling of power over 4), `est_power` and
  `basis` (`observed`: a group is there, its own power, Aggressive and Swarm
  +25%; `template`: a lair that is away or cleared; `estimate`: ring only).
  A stale chart entry keeps the rating last seen. Scans list the threat of
  every revealed sector (`threats`) and each faint contact has a
  `threat_band` (1: T1-3, 2: T4-6, 3: T7-9). Pirates return
  `2 + 0.25 x ring` game hours (divided by P) after a kill.
- **Odds** — `threat` says how big a group is; your odds against it are a
  separate number that only the server gives (`preview_move` for a jump,
  `odds` for any sectors). The server plays the fight out on averages with
  the ships on each side (rapid fire between classes, each shot against hull
  and shield, your fleet firing first) and puts the result on the scale of a
  power ratio between equal ships: `ratio` 1.0 is an even match, the scale
  runs 0.1 to 10. Each `label` is the same outcome whatever the classes:
  SAFE from 2.5 (you keep 98% of your hull or more), FAVOURABLE from 1.5 (a
  sure win, 1-15% of the hull lost), EVEN from 1.1 (a win in practice,
  15-50% lost), RISKY from 0.9 (anything from a lost fleet to a win that
  keeps half), DEADLY below (lost, or one win in six at best). Power against
  power is not it: four scouts are 1.8x the power of the ring-9 corvette and
  rate 1.06x RISKY against it (three fights in four won, 41% of the hull
  kept on average); one corvette is 1.4x the power of the two ring-8 scouts
  and rates 3.5x SAFE. A sector with nothing in sight is rated against the
  group its ring spawns, a remembered one against what was last seen there.
  The ratio is the expected result; rapid fire comes in streaks, so a
  FAVOURABLE fight can still go badly once in a long while.
- **Buildings** — 10 types incl. Shipyard, Research Lab, Sensor Array (passive
  reveal around home), Defense Grid (see raids), Storage Vault (stockpile
  caps) and Fabricator (building time / (1 + 0.15 x level)).
- **Storage** — every stockpile is capped at 5000 / 3500 / 2500 x 1.5^Vault
  x P^0.75 up to pace 10, then growing as P^0.25 (a blitz Vault 0 holds 78k
  metal, not 606k) (`homeworld.storage`: `cap`, `protected`, `full_in_s`, `capped`).
  Mines stop at the cap and the surplus is lost; a payment bigger than a cap
  is refused (`StorageTooSmall`, naming the Vault level it needs); docked
  cargo unloads only what fits and the rest waits aboard
  (`cargo_blocked: true`). `StorageNearCap` (85%) and `StorageFull` events
  warn you. A Vault also protects 8% of the cap per level (to 60%) from raids.
- **Queues and slots** — one rule for buildings, research, ships and
  defences. An order that can ever start is always accepted, whether or not a
  slot is free or you can pay yet. Each order has a stable `id` (cancel by it,
  never by position). Whenever a slot is free, the server starts the earliest
  waiting order that can start now: what it needs is built and the stockpile
  covers its cost, which it pays in full at that moment. An order that cannot
  start keeps its place and is checked again every tick, so a free slot never
  idles behind an order that cannot pay. The catch is that cheaper orders
  behind an expensive one can keep winning the race for the stockpile. To make
  an order wait its turn instead, send it with `reserve: true`: while it is
  ready but short of resources, nothing behind it in the same queue starts
  (orders ahead of it still do). An order that waits only on a prerequisite,
  such as a Shipyard behind the Metal Mine it needs, never holds the line.
  Research and buildings can both be queued behind a prerequisite that is
  itself still queued, and a tech that is running can be queued again for its
  next level. A ship batch is one order that pays and starts one unit at a
  time: it shows `built` of `count`, and when it cannot pay for the next unit
  it goes back to waiting (`waiting_for` is the next unit's shortfall) and
  lets another order use the yard. Buildings run in `build_slots` parallel
  slots (1, plus one per Modular Fabrication level, max 3); research and the
  shipyard have one.
  Each queue holds its running items plus `queue_waiting_max` (3) waiting ones,
  so the building queue holds `build_slots + 3` and the others 4; a full queue
  refuses with `QueueFull`. Orders that can never start are refused with a
  specific error: `PrerequisitesNotMet` (naming what is missing),
  `MaxLevelReached`, `StorageTooSmall` (naming the Vault level it needs),
  `NoShipyard`, `NoResearchLab`, `ShipLocked`, `DefenceLocked`.
  `build_pending` / `research_pending` / `shipyard_pending` list each waiting
  order with its `id`, `waiting_for` (the exact shortfall), `waiting_on` (`Slot`,
  `Resources`, `Prerequisite` or `Order`: held behind an earlier `reserve`
  order) and
  `start_in` (estimated ticks to start at current production, ignoring what the
  other queues spend). Every order, start and cancel is answered by a `Queue`
  event: `Started` (what was paid), `Waiting` (shortfall and estimate) or
  `Cancelled` (what was refunded: half for a running item, nothing for a
  waiting one). A queue with nothing running but orders waiting is not idle.
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
  pays and the rest follows as production arrives. It is paid ahead of every
  queue, except for a fleet under standing orders, which is held to its
  `fuel_budget_pct`. `preview_move` answers
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
  is exactly the pile `collect_salvage` can take; it lasts 180 ticks, its
  sector view carries `salvage_despawn_tick`, and a `SalvageDespawned` event
  says when one expires). You only receive combat
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
  wreckage and derelict **as last seen**, which may be out of date, and says
  so with `pins_stale: true`. A remembered pile whose `salvage_despawn_tick`
  has passed is dropped from the chart, and so is any pin you watched go (a
  boarding, a collection, a scan that finds nothing there). Tick updates add a
  sector to `sector_updates` once more when it turns stale or loses a pin.
  A sector you have entered carries `chart`: `"aboard"` while its chart rides
  with one of your fleets (it scores when that fleet docks at home alive and
  is lost with the fleet), `"delivered"` once it is banked; a sector without
  `chart` still has its chart points to give. A remembered sector is sent
  once more when its mark changes.
  The `signals` of a scan lie one hop beyond the scanned range and never name
  the scanning sector, which is `ScanCompleted.sector`.
- **Derelicts** — dead ships drift in debris fields and empty space beyond
  the shipping lanes (`D` on the map, "derelict transponder" on scans).
  Board one (`explore_site`, 20 ticks) for tiered loot, sometimes a
  recoverable ship or an instant-tech data core — or an ambush by whatever
  killed the crew. Aborted/failed boardings leave the site *hotter*. A
  Scout aboard reads the risk down; a Hauler doubles the haul.
- **Loot** — a kill pays `1.2 x group_power^0.8` units (55% metal, 30%
  crystal, 15% deuterium) times P^0.5, for the whole group; far hunting pays
  more but the odds fall faster than the pay rises. Derelict tier follows the
  threat (T1-3, T4-6, T7-9). A kill in a sector that has been killed in
  recently pays less: each recent kill multiplies the next one's loot and
  score by 0.75 (heat halves every 4 respawn delays), so a sector is not an
  endless tap. Holds grow with the pace like the loot: a ship's cargo is
  multiplied by P^0.5 (a blitz Hauler holds 4900, a Scout 245). A site pays `240 x 1.14^(ring-4)` units at 60 to
  140% times P^0.5; ambush odds `0.10 + 0.03 x (T-1)` (max 0.35); a recovered
  ship `0.05 x (T-2)` (max 0.30), a data core `0.03 x (T-5)` (max 0.15), and
  from T6 an ancient relic (2% per point above 5, max 8%, worth 30 explore
  points, no other use yet). A stripped derelict is replaced after 120 game hours / P^0.5.
- **Standing orders** — assign a fleet a doctrine and the server flies it
  (see `policy_update` above). Doctrines respect fuel/cargo thresholds,
  return home to deposit and refuel within their fuel budget, wait for
  shields, go round fights they need not take, and log every decision.
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
  divided by P^0.75; each eligible roll that misses raises the next one's odds
  by 15 points, so a raid is never long overdue. A raid's power is `5 + 11 x S^0.75` (S = your building
  and research points, ships and defences excluded) times a roll of 0.80 to
  1.15, so it grows with your economy and not with your defence;
  `next_raid_estimate_power` shows what to expect. `RaidIncoming` warns
  `600 / sqrt(P)` ticks ahead (30 to 600) with the raid's `est_power`.
  Docked ships, the grid and structures defend (±10% fog). Repelled: bonus
  salvage in orbit and a share of structures destroyed (up to half, scaled by
  raid over defence power), 70% of them rebuilt free shortly after. Lost: a
  hard-capped slice of the stockpile above the Vault-protected share (≤8%),
  half the structures gone for good, a quarter of the ships docked at home
  destroyed (at least one), and a long grace period. Never
  buildings, never queues. `RaidResolved` reports `structures_lost`,
  `structures_restored`, `protected_kept`, `ships_lost` and, for a repelled
  raid, `salvage_despawn_tick`. The orbital salvage is on your own planet, so
  a fleet docked at home scoops it into storage by itself, up to what the
  Vault holds (`SalvageCollected` says what); with no fleet docked, or the
  Vault full, an `Alert` fires once when 60 s are left.
- **Score** — one ranking for humans and AI agents (`leaderboard`).
  `core` = completed building and research levels + living ships + half of
  the defence structures, each priced at cost x (1 metal, 1.5 crystal, 2
  deuterium) / 1000. Kills add the weight of the pile they leave (so points
  follow the finds factor and the sector's heat), in full up to 4x the NPC
  group's power and falling as 4/ratio beyond (half at 8x, a 5% floor, never
  zero), once per respawn. Exploration adds `0.5 x weight(loot) + 1.5 x T^1.5`
  for the first boarding of each derelict, 30 for a relic, and
  `0.15 + 0.10 x T^1.5` for each new sector whose chart you deliver to the
  homeworld (the fleet must dock alive: `ChartDelivered`). `score = core + min(combat + explore, 0.25 x core)`. Cancelling
  a build or losing a ship scores nothing; resources received from others
  never count. Accounts that log in with `"agent": true` (the headless client
  does) carry the `agent` flag on the board, which labels and never scores.
  Built-in bots (names starting `BOT-`) score by the same rule and are listed
  apart, in `bots`: they take no rank among humans and agents.

## Development

```sh
cargo test --workspace       # end-to-end only: engine scenarios, real-server protocol runs, cross-language fixtures
cargo clippy --workspace

# Headless: the real engine, built-in bots, as fast as the CPU allows; prints a balance report
cargo run -p iac-sim --release --bin sim-run -- --pace season --days 7   # every style once and an idle control; a world seats 24
cargo run -p iac-sim --release --bin sim-run -- --pace blitz --hours 1 --seat all:hands-on --seat idle --json

# The engine must build for the browser (no bindings yet)
cargo build -p iac-sim --target wasm32-unknown-unknown --release
```

Crates: `iac-shared` (`shared/`) (protocol, constants, scaling, hex math, worldgen) ·
`iac-sim` (`sim/`) (every game rule and all world state: no I/O, no async, no clock; time is ticks, every random draw is seeded) ·
`iac-server` (network, SQLite, tick timer around `iac-sim`) · `iac-client` (ratatui TUI +
headless mode).

`iac-sim` talks to its host through three seams: `GameEngine::execute` (a protocol
`Command` in, reply or refusal out), `views::{full_state, tick_update}` (what a player
sees), and persistence. The engine hands `PersistBatch` deltas to a `Persist` trait the
host implements (the server's SQLite writer); `snapshot()` / `GameEngine::restore()`
save and rebuild the whole world as serde data, and `Snapshot::hash()` is a digest of
it. Same seed, pace and command script give the same hash.

### Built-in bots and balance runs

`sim/src/bot/` holds scripted players that see only the `GameState` a client gets and
send only what a client sends. A bot is two independent choices:

- **Personality** (what it builds and what its fleets are for): `turtle` (economy and
  defences, nothing leaves home), `miner`, `raider`, `explorer`, `mixed`.
- **Attention** (when it is at the controls): `check-in` (a few five-minute look-ins a
  day: queue ahead, regroup, leave every fleet to a standing order), `hybrid` (the
  same, and one look-in a day is an hour of piloting by hand), `hands-on` (always
  present, flies every fleet itself, no standing orders).

`script::Seat` is an in-process client: it runs one bot's turn before each tick
(`take_turn`) and keeps the bot's view after it (`observe`), by folding the tick
update while the bot acts tick after tick and from a full state otherwise. A turn is
questions, then orders: the bot may first ask what a client may ask (`odds`,
`preview_move`, `leaderboard`) and gets the replies before it acts, so a pilot fights
and enters sectors on the server's ratio and keeps no arithmetic of its own. A world's
bots are accounts with a `BotRecord` (style and the little a bot remembers) kept in
the engine, in every snapshot and in storage; `bot::Roster` is what a host holds
beside the engine: `resume` the bots a world has, `enlist` the ones it should have,
`take_turns` before `tick`, `observe` after. The server does exactly that (`--bot`),
and a browser host would do the same.

`sim-run` seats a mix and prints, for every player and sample interval, the score
split (econ, fleet, defence, raw combat, raw explore, the capped extra, total) and
what the field paid, then one row per style with ships lost and the deuterium
paid for refuelling:

```sh
sim-run --pace season --days 7                      # every style once and an idle player
sim-run --seat explorer:all:2 --seat turtle:hands-on --sessions 8 --seed 3
sim-run --pace blitz --hours 1 --seat all:hands-on --ignore-cap --json
```

`--seat` is `<personality>:<attention>[:<count>]` or `idle[:<count>]`, `all` standing
for every personality or attention; `--sessions` sets the look-ins per day;
`--ignore-cap` lets bots chase combat and exploration points past the score cap
(by default they stop once it is banked). A world has 24 home seats; `sim-run` refuses
a roster of more.

## Other versions

Earlier implementations are kept on archive branches (each also tagged):

| Branch / tag | What it is |
|---|---|
| `archive/zig` / `archive-zig-final` | Original Zig implementation (zithril TUI, zqlite, webzocket) with the full design history; also holds the old `clients/flutter` client |
| `archive/dart-fullstack` / `archive-dart-fullstack` | Full-stack Dart: Flutter web UI, Dart game server and shared protocol package (orphan branch) |
| `archive/numen-scout` / `archive-numen-scout` | Small slice ("Amber Scout") written in the Numen language (orphan branch) |

The Zig-era design notes, milestones and tech stack live in `README.md` on `archive/zig`; the current spec is `SPEC.md` plus `docs/`.
