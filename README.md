# IN AMBER CLAD

**A multiplayer space strategy game played at an amber CRT console.**

You are an admiral at a CRT console, commanding fleets across an infinite hex grid. Mine resources, build your homeworld, research technology, and push deeper into the wandering, where the map grows sparse, dead-ends can trap you, and Morning Light Mountain swarms hunt in the dark.

Humans play in the browser, through the Flutter web client and its amber CRT look. LLM agents play the same game directly, as JSON over the wire protocol (a WebSocket, or the headless NDJSON pipe). Both are first-class citizens. Think OGame x The Infinite Black, with a 1 Hz tick loop hidden inside cooldowns so real-time play feels responsive to humans and tractable for LLMs.

## Layout

| Path | What |
|---|---|
| `shared/` | `iac-shared`: protocol, constants, scaling, hex math, world generation |
| `sim/` | `iac-sim`: the whole game as a pure, deterministic library (engine, tick, commands, combat, intel, queues, score, snapshots, built-in bots, headless runner `sim-run`), the host core (`host.rs`): the one tick, the one session gate and the one way to seat a world that every host runs, and what a stored world is (`store.rs`): one document, the same for every host; no I/O, builds for WebAssembly |
| `server/` | `iac-server`: what a network and a disk add to the host core: the WebSocket transport, tokens, the world's file and the thread that writes it, the 1 Hz tick timer |
| `solo/` | `iac-solo`: what one player and a browser tab add to it: a commander with no password, a clock it is told and the catch-up after an absence; compiled to WebAssembly and run in a browser worker (`solo/web/worker.js`) with the save, the same document a server keeps, in IndexedDB |
| `clients/web/` | Flutter web client (amber CRT UI), the interface people play through: served by `iac-server`, or bundled with `iac-solo` as a static single-player build |
| `clients/headless/` | `iac-headless`: the headless client agents and scripts play through, the game as an NDJSON pipe; its library is the WebSocket connection, the login and the token file |
| `docs/` | Design docs, auth spec, data architecture, HTML mockups, Rust port plan and dev journal |
| `tools/` | `playtest/` (live multi-agent sessions) and `check_sim_constants.py` (the formulas of the planning simulation, `docs/design/economy/sim.py`, against the game's; its numbers are not compared, they are generated from Rust) |
| `SPEC.md` | Original technical specification (world model, combat, economy, networking) |

## Quick start

One command, browser play (builds the Flutter web client, then serves it and
the WebSocket from the server on one port, printing the LAN URL):

```sh
scripts/play.sh              # wasm build, falls back to JS; --js / --no-build; PORT=7777 default
```

Single player, no server at all (the client and the engine in one browser tab,
against the built-in bots; see "Single player" below):

```sh
scripts/play.sh --single-player   # builds clients/web/build/single-player and serves it on 127.0.0.1:7777
```

Or manually:

```sh
# Terminal 1: the server (1 Hz tick loop, the world in one file)
cargo run -p iac-server                  # 127.0.0.1:7777, world.iac, pace x1 (persistent)
cargo run -p iac-server -- --pace blitz --world blitz.iac   # a one-hour world (x600)
cargo run -p iac-server -- --pace season --world season.iac --bot all:check-in --bot mixed:hands-on:2   # with built-in bots
# serve the web client too (after `flutter build web --release` in clients/web):
cargo run -p iac-server -- --host 0.0.0.0 --web-dir clients/web/build/web
```

**The world file** (`--world <path>`, default `world.iac`) is the whole world
and the only thing the server keeps: one document, a header line and the
world as JSON, the same document a single-player save is
(`tail -n +2 world.iac | jq .players` reads it). A path that does not exist
yet is a new world. The server writes the file when it starts, every
`--save-every <seconds>` (default 300) and when it is stopped, and holds
`<path>.lock` while it runs: a second server on the same world refuses to
start.

- **Stopped** (ctrl-c, SIGTERM): the file is the world as it stopped. Nothing
  is lost, so a deploy is a stop and a start.
- **Killed** (SIGKILL, a crash, a power cut): the world comes back as the
  last file written says, so up to `--save-every` seconds of play are gone,
  orders included. The file itself is never half written: a new one is
  written beside it, synced and renamed over it.
- **A new account** is the exception: its token is shown once, so the file is
  written within about three seconds of a registration, whatever the period.

The world pace (`--pace <number|preset>`; presets persistent, fortnight, season,
sprint, dev, blitz, test) and the seed (`--seed <number>`) are fixed when the
world is created and stored in its file. An existing world keeps both, and the
server refuses a different `--pace` or `--seed`; a file from another save
format, economy or world generator is refused too (start a new world with a
new `--world`). Nothing is migrated.

The server serves `clients/web/build/web` automatically if it exists
(override with `--web-dir`). WebSockets are accepted on any path (`/ws` for
the web client, `/` for the headless client); `--host` sets the bind address.

**Built-in bots** (`--bot <personality>:<attention>[:<count>]`, repeatable;
personality `turtle`, `miner`, `raider`, `explorer`, `mixed`, attention
`check-in`, `hybrid`, `hands-on`, either may be `all`) are computer players
the server runs beside the engine. `--bot` says which bots the world should
have: missing ones are added at start, in the order listed, and the ones a
world already has keep playing on every later start whether listed or not
(they are accounts of the world, stored in its file with what they
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

## Headless client / LLM agents

The game is fully playable without a terminal. The headless client,
`iac-headless`, is the game as an NDJSON pipe:

```sh
cargo run -p iac-headless -- --name MyAgent [--token <token>]
```

- **stdout** — every server message, one JSON object per line: `auth_result`,
  `full_state`, `tick_update` (with `events`), `error`, plus the replies to
  `preview_move`, `odds`, `exchange_quote`, `outpost_survey` and `leaderboard`. Every `tick_update`
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
- **Saved automatically**: the headless client stores the token per
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
} | iac-headless --name Probe | jq -c 'select(.type=="tick_update") | .events[]?'
```

### Command reference (stdin)

| action | fields | effect |
|--------|--------|--------|
| `move` | `fleet_id`, `target:{q,r}` | jump to a connected sector (fuel, cooldown). The fuel is paid when the jump is ordered, and a fleet that is charging a jump takes no other (`OnCooldown`; `stop` calls it off). From inside a fight it is flight: the fleet stops firing and is fired on every round until the jump lands, a round fewer than its `jump_ticks` and never fewer than one, and a group that was beating it may follow (see Combat). `fleets[].flight` quotes it while the fleet is in a fight (`fuel`, `ticks`, `rounds`, the odds of the fight as it stands, `refusal`, and `chase`, the group that would follow). Refused with `InsufficientFuel` and a sentence when the tank does not hold the jump. A fight that begins in the sector while a jump is charging does not stop the jump: the fleet is in the fight as one that is leaving, fired on until it lands, which is told in an `Alert` (warning) |
| `scan` | `fleet_id` | reveal nearby sectors + faint signal contacts; a fleet with an Espionage Probe scans farther (`fleets[].scan_range`) |
| `harvest` | `fleet_id`, `resource` (`Metal`, `Crystal`, `Deuterium`, `Auto`) | mine the current sector until cargo is full; `Auto` takes everything present, a named resource only that one (`ResourceNotPresent` if the sector lacks it). When the harvest ends by itself an `Alert` (warning) says why, once: the hold is full, or nothing (or none of the named resource) is left to mine there. A fleet under standing orders reports through its doctrine instead |
| `attack` | `fleet_id`, `target_fleet_id` | engage an NPC fleet |
| `collect_salvage` | `fleet_id` | scoop wreckage |
| `recall` | `fleet_id` | emergency-jump home from another sector (2× fuel, hull risk); a jump the fleet was charging is abandoned with its fuel. The answer is an `Alert` with what the jump cost: the fuel, a jump abandoned, how many ships took hull damage and how much (it lasts until a dock mends it, which the home dock does on arrival), how many broke up; a warning when a ship was harmed, info when none was. The ships that broke up are also a `ShipsLost` event, by class and count. Refused with `InvalidCommand` and nothing changed for a fleet at your homeworld: there is nothing to come back from, a jump it is charging there goes on, and `stop` is the order that halts it |
| `stop` | `fleet_id` | abort a move, harvest, or boarding. A jump called off does not return the fuel it burned, and backing out of a boarding leaves the site on alert, so its next boarding is likelier to be ambushed: either is told in an `Alert` (warning), the first with the fuel forfeit. A fleet that calls off its jump out of a fight fights on. Halting a harvest costs nothing and says nothing |
| `explore_site` | `fleet_id` | board the derelict in the current sector (20t, ambush risk) |
| `build` | `building_type`, `reserve` (default false) | upgrade a homeworld building one level: starts at once when a building slot is free and you can pay, otherwise waits in the queue (nothing is charged until it starts). Every order gets a stable `id`, reported in its `Queue` event and in the queue lists. `reserve: true` makes the order hold the line while it is short (see Queues and slots) |
| `research` | `tech`, `reserve` | research one level, same rules (one lab) |
| `build_ship` | `ship_class`, `count` | order a ship batch (one shipyard). Unlike a building or a tech, a batch is accepted only if your stockpile covers every unit now, and it is paid in full at that moment; otherwise it is refused with `InsufficientResources` and a message naming the shortfall per resource and how many units the stockpile would cover (5 ordered with money for 2 is refused; order 2). A paid batch starts at once or waits for the yard |
| `build_defence` | `kind` (`RocketLauncher`, `LightLaser`, `HeavyLaser`, `GaussCannon`, `IonCannon`, `PlasmaTurret`, `SmallShieldDome`, `LargeShieldDome`), `count` | order home defence structures in the shipyard queue, paid in full when ordered like ships; `DefenceLocked` until the Shipyard level and research it needs are in place, `MaxLevelReached` for a second dome of a kind |
| `cancel_build` | `queue_type`, `id` | cancel the item with that `id` once it has started. A building or tech refunds 50% of its cost, and waiting items of the same building or tech move up a level. A ship or defence batch keeps the units already built and refunds 50% of the price of every unit not yet delivered, the one under way included. A `Queue` event confirms what was refunded. `InvalidTarget` names where the id actually is (another queue, still waiting, or gone) |
| `cancel_queued` | `queue_type`, `id` | remove the item with that `id` while it still waits. A waiting building or tech paid nothing, so nothing comes back; a waiting batch paid when it was ordered and gets 50% back. A `Queue` event confirms it. Refused, not redirected, when the id is not waiting in the named queue |
| `preview_move` | `fleet_id`, `target:{q,r}` | do not move: reply `preview_move` with `fuel_cost`, `fuel_after`, `can_jump`, `hops_home` (shortest real route home, charted or not), `charted_hops_home` (shortest route over sectors you have entered, null when there is none), `route_unexplored` (the `hops_home` route crosses sectors you have never entered), `fuel_to_return`, `can_return`, plus `threat` (`rating`, `est_power`, `basis`), `fleet_power`, and the odds `ratio` and `label` (`safe`, `favourable`, `even`, `risky`, `deadly`), all for the fleet you name, not your strongest. `ratio` is not `fleet_power / est_power`: see Odds. For a fleet in a fight the jump is flight and the reply also carries `flight`, the quote of `fleets[].flight` for this `target`: its `chase` is absent when no group would follow there (home ground, the hub and a sector an outpost stands in are refuges) |
| `odds` | `fleet_id`, `sectors:[{q,r},...]` (at most 256) | do not move: reply `odds` with `fleet_id` and, for each sector in the order asked, `{sector, threat, ratio, label}`: how that fleet would fare there. Any sector may be named: adjacent or far, the one the fleet is in, seen or never seen |
| `leaderboard` | `limit` (default 20) | reply `leaderboard`: `entries` of `{rank,name,agent,score,buildings,research,fleet,outpost,defence}`, plus `you` when you are below the limit. Built-in bots are never in `entries`: when the world has any, `bots` lists them in the same row shape, ranked among themselves, so a rank in `entries` counts humans and AI agents only |
| `split` | `fleet_id`, `ships:[{class,count},...]`, `damaged_first` (default false) | detach so many ships of each class named into a new fleet in the same sector; at home this launches docked ships. A ship has no id: a class is named once with a count, the whole ships of the class go first, or the damaged ones from the least hull up with `damaged_first: true`. At least one ship stays; fleet cap 8 in all, 3 away from home. The new fleet keeps the weapons, shields and armour the old one fights with, and its id arrives in an `Alert` event. Refused with `InvalidTarget` and a sentence naming the class at fault (a count of 0, a class named twice, more than the fleet has, every ship taken: "fleet 2 has 6 Light Fighter, not the 9 the split names"), `FleetLimitReached` at the caps |
| `merge` | `fleet_id`, `other_fleet_id` | fold the other fleet into `fleet_id` (same sector, both idle, at most 5000 ships together); ships, cargo and fuel pool, the other fleet's standing orders end. Where the two were refitted at different levels of Weapons, Shielding or Armour Technology the merged fleet fights at the lower of each until it docks at home, and an `Alert` says so |
| `exchange` | `fleet_id`, `give`, `want` (`metal`, `crystal`, `deuterium`), `amount` (optional) | sell `amount` of `give` from the fleet's hold to the hub's exchange and take `want` into the hold; without `amount`, all of `give` aboard. The fleet must stand idle on the hub sector (`full_state.world.hub`). The answer is an `Exchanged` event with what was given and received, the fee and the meter before and after (see The hub and its exchange). Refused with nothing changed, and a sentence saying why: `NotInSector` away from the hub, `OnCooldown` in transit, `NoResources` for more than the hold carries or none, `CargoFull` when what comes back would not fit, `InvalidCommand` for one resource named twice, an amount that is not positive, or a trade that returns less than one unit |
| `exchange_quote` | same as `exchange` | do not trade: reply `exchange_quote` with `amount`, `receive`, `par`, `fee`, `meter` and `meter_after`, exactly what `exchange` with the same fields would do. It answers from anywhere and for an amount the fleet does not carry; `refusal` is then the sentence the trade would be refused with right now |
| `load` | `fleet_id`, `resource` (`metal`, `crystal`, `deuterium`), `amount` (optional) | move `amount` of `resource` from the stockpile into the fleet's hold; without `amount`, all the stockpile holds and the hold has room for. One resource an order. The fleet must stand idle at its homeworld with no standing orders. It is then `laden`: the dock leaves its hold alone until the fleet has been away (see Loading a hold). The answer is an `Alert` (info) saying what went aboard. Refused with nothing moved, and a sentence saying what is short: `NoResources` when the stockpile holds less than `amount` or none, `CargoFull` when the hold lacks the room, `NotInSector` away from home, `OnCooldown` in transit or busy, `InvalidCommand` for an amount that is not positive or a fleet under standing orders. At the player's outpost `load` takes from the outpost's store instead and does not make the fleet `laden`; see Outposts |
| `unload` | `fleet_id` | put a `laden` hold back: what the caps allow goes into the stockpile now and the dock takes the rest as room opens. The answer is an `Alert` (info). Refused with `InvalidCommand` when the fleet was not loaded by `load`, `NotInSector` away from home. At the player's outpost `unload` puts the whole hold of any fleet into the outpost's store as far as it has room (`NoResources` for an empty hold); see Outposts |
| `refine` | `fleet_id` | turn the deuterium in the fleet's hold into fuel, until the tank is full or the deuterium is gone. It works anywhere, takes no time, and loses 15%: a unit of deuterium is 12.5 fuel at a dock and 10.625 from the hold (see Refining). `fleets[].refine` (`deuterium`, `fuel`) says beforehand what the order would turn into what, and is absent when it would be refused. The answer is an `Alert` (info) with both figures. Refused with nothing changed, and a sentence saying why: `OnCooldown` in combat or while a jump charges, `NoResources` with no deuterium aboard, `InvalidCommand` with a full tank |
| `found_outpost` | `fleet_id` | found the player's outpost in the sector the fleet stands in, paying 1000 metal, 500 crystal and 200 deuterium from its hold. The fleet must stand idle with no standing orders; the sector must be in rings 9 to 20, not an Anomaly, without a derelict, without a group in it or away from it after a fleet (a lair is won when its group is destroyed), and at least 3 hexes from every other outpost; a player has one outpost. The answer is an `Alert` (info) with the tick it will be finished. Refused with nothing changed and a sentence saying why: `InvalidTarget` for a sector that does not qualify, `InvalidCommand` with an outpost already or under standing orders, `NoResources` naming what the hold lacks, `OnCooldown` for a fleet that is moving, fighting or busy |
| `outpost_survey` | `fleet_id`, `sector:{q,r}` (optional) | do not found: reply `outpost_survey` for the named sector, or the fleet's own, with its `ring`, the `cost` and `build_ticks` of level 1, `raid_power` (`least` and `most`), `platform` (the defences level 1 stands with) and `platform_power`, `raid` (a typical raid of that ring, its `ships`, and `odds` against a level 1 platform alone), `site` (`densities`, `grade`, `production` a tick at level 1, `hops_home`) and `refusal`: the sentence `found_outpost` would be refused with right now, absent when it would be accepted. Any sector may be asked about; one the player has never seen is answered without `site` and with a `refusal` that says so |
| `upgrade_outpost` | none | build the next level of the outpost, paid in full from the outpost's own store at that moment. Refused with `InsufficientResources` and the exact shortfall, `MaxLevelReached` at level 3, `InvalidCommand` with no outpost, while it is building or repairing, or while it is breached |
| `abandon_outpost` | none | give up the outpost. Its store is left as wreckage in the sector for 180 ticks, its score is gone, nothing is refunded, and another may be founded. `InvalidCommand` with no outpost |

Standing orders are a separate message type (not a command):

```json
{"type":"policy_update","fleet_id":2,"preset":"salvage_and_sites",
 "params":{"min_fuel_pct":10,"cargo_return_pct":85,"max_range":4,"engage_ratio_x10":15,"disengage_ratio_x10":9,"fuel_budget_pct":20}}
```

A line that names it as an order, `{"action":"policy_update",...}`, is refused
by the headless client with a `client_error` saying to send it with `"type"`
in place of `"action"`; an order it does not know is refused with the list of
orders and the form of standing orders.

`params` is optional, and so is every field in it: send only what you want to
change (`"params":{"max_range":2}`) and the rest take their defaults. A fleet
under orders carries them in its row: `fleets[].policy` is
`{"preset": ..., "params": {...}}` with all seven settings as the server holds
them (after clamping), in `full_state` and in every `tick_update`. A fleet you
fly by hand has no `policy` field.

| param | default | meaning |
|-------|---------|---------|
| `min_fuel_pct` | 10 | spare fuel, as a percent of the tank, kept on top of the exact fuel cost of the route home (clamped 0-90) |
| `cargo_return_pct` | 85 | head home to unload when the hold is this full (10-100) |
| `max_range` | 4 | farthest the autopilot goes from your HOMEWORLD, in hexes, for claims, prospecting, salvage runs and patrols (1-30); never measured from the last claim |
| `engage_ratio_x10` | 15 | fight only if the fleet's odds `ratio` against the hostiles is at least this/10 (1-100); also exactly the least ratio at which any doctrine will enter a sector, hostiles there or not. It is the same number `preview_move` and `odds` show. The default is FAVOURABLE. A value below EVEN (1.1) that you set deliberately is honoured |
| `disengage_ratio_x10` | 9 | leave a fight once the fleet's odds in it, read again every 3 ticks from the two sides as they then stand, fall below this/10 (1-100, and never above `engage_ratio_x10`). The fleet jumps out toward home under fire, as a `move` out of a fight does, and says so in a `PolicyAction` `flee` with the odds. The default is the edge of RISKY. `1` keeps a fleet in any fight |
| `fuel_budget_pct` | 20 | the share of your deuterium income, in percent, that this fleet may spend on fuel while it flies under these orders (0-100; 100 lifts the bound). See "Fuel budget" |
| `refine` | false | `true`: before the fleet would turn home for fuel it refines the deuterium in its hold, as a `refine` order does and at the same 15% loss, reports it as a `PolicyAction` `refine` with both figures and the shortage, and is weighed again; it turns home only if that was not enough. Not charged to the fuel budget |

Fuel safety: the autopilot returns home when its fuel falls below the cost of
the shortest route home (charted or not) plus the `min_fuel_pct` reserve, and it
refuses any outward jump that would leave less than that. It sets out only for
a sector it can fly to and home from on the fuel aboard, its reserve kept,
counting jumps over the real lanes. Turning home, it takes the way the fuel it
carries flies (a lane without a fight first, then one with) and says how many
jumps home is, by which sector, and the fuel against what it carries; when no
way home is within its fuel it flies the shortest and says so. Whenever a
doctrine stands still it says why in a `PolicyAction` event (`action:
"hold"`), naming the limit that holds it: odds below the engage ratio, the
fuel there and back, or the range (with the tank named when it, not
`max_range`, sets the range). A hold is told when it begins and again only
when its reason changes.

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
- `patrol_home` hunts the hostile groups inside `max_range` of home that the
  fleet rates at `engage_ratio_x10`, and between fights collects wreckage
  there: what its kill leaves and any other pile that repays its fuel, the
  nearest contact or pile first. It brings the hold home when it reaches
  `cargo_return_pct`, when the next pile would not fit the room left
  ("returning to unload the salvage: the hold has room for 40 of the 900
  units of wreckage at [5,-3]"), and when nothing is left in range to fight
  or collect; there it unloads, is mended and goes out again, and with the
  beat clear it waits at the dock. A Small Cargo merged into the patrol
  adds its hold and slows the fleet to its own speed; near home a few
  Light Fighters already carry what a kill leaves.

What every doctrine does:
- No doctrine moves a fleet into a sector where its odds (as `preview_move`
  and `odds` show them) are below `engage_ratio_x10`; the setting is the bar
  whether or not hostiles sit there, so lower it to push a probe through,
  raise it to be careful.
- It picks its work among the sectors it may reach. A seam, wreck, derelict
  or contact behind a refused sector is passed over for the next one; only
  when none is left does the fleet go home and hold, and the reason names the
  nearest one it could not have and what stops it (sector, threat, ratio).
- Shields are full again every round and damage to a hull lasts until a
  dock mends it. So a fleet under orders waits for nothing after a fight,
  and one whose hull is under 85% of full goes home before it does anything
  else ("returning to be mended: hull at 71% of full, and a fleet under
  orders works at 85% or more"), where the dock mends it at once. It takes a
  lane without a fight before a shorter one with a fight it would win; and
  `mine_and_return` and `salvage_and_sites` pick no seam, wreck or derelict
  in a sector whose hostiles attack on sight (a patrol can clear it first).
  A fight on the only lane is taken at `engage_ratio_x10` or better.
- In a fight, whatever doctrine it flies and however it came into it, a
  fleet under orders leaves once its odds in the fight as it stands are
  below `disengage_ratio_x10` ("left the fight at [8,-3]: odds fell to
  0.13x, DEADLY, below the doctrine's 0.9x; jumping to [7,-3], under fire
  for 1 round"). With a group on its trail it runs for home before it does
  anything else ("running for home: the 2x cruiser pack is on its trail and
  lands here in 2 ticks"); home ground is a refuge.
- A seam or a wreck is flown to only if what the hold can take of it is worth
  1.5 times the deuterium the jumps there and back burn, both at score weight
  (metal 1, crystal 1.5, deuterium 2); a sector with under 3 units of ore
  counts as stripped. Derelicts always count.
- A fleet whose cargo your full stockpile cannot take stays docked until it
  has unloaded. One that docks with a hold says what it unloaded ("unload":
  "unloaded 500 metal and 120 crystal into the stockpile") and what stays
  aboard for want of room.
- Heading home it takes a lane that meets `engage_ratio_x10`; if there is
  none, the mildest there is, down to RISKY, and says so. It does not jump
  into DEADLY odds: it holds where it is ("cut off from home") and you get a
  Warning alert.
- Every act emits a `PolicyAction` event with a human-readable `reason`: an
  audit trail for agents (and captains). A hold is repeated at most once a
  minute while its cause stays the same.

## Web client

`clients/web/` is the interface people play through, a Flutter web app with
the amber CRT look: command center,
windshield and star map, driven by a command bar (`help` lists commands) and
hotkeys (`1`/`2`/`3` views, numpad 1-6 to move, Esc leaves the bar). It speaks
the Rust wire protocol over `/ws` on the page's own host and port. In the
windshield `P` (or Orders under Fleet status) opens the standing orders of the
active fleet: the doctrine in force, what it last reported and, when the fleet
stands still, the engine's reason for the hold; the five doctrines with a line
each, steppers for the range, the least odds to fight, the odds below which
it leaves a fight, and the fuel budget, and a
switch for refining the hold before turning home for fuel. Its
button sends what the command bar sends for
`policy mine range=8 engage=1.5 disengage=0.9 fuel=40 cargo=85 reserve=10 refine=on` (any
subset; `fuel` is `fuel_budget_pct`, 100 for no bound; `refine` is `on` or
`off`; `policy manual` clears). The server never holds `disengage` above
`engage`, and the panel shows what it took. The
fleet row carries the settings with the doctrine, so the panel shows the
settings in force for any fleet under orders, whoever gave them and whenever,
and its steppers start from them; settings typed with no doctrine
(`policy range=6`) change the orders in force and keep the rest. At the hub the windshield
lists the exchange among its contacts; `T` (or the Trade tile) opens the
exchange panel, which shows the server's quote for the trade picked and
sends it. `quote metal deut 400` and `trade metal deut 400` do the same from
the command bar (no amount: all the hold carries). The homeworld's Storage
tab has a panel that loads the hold of a fleet docked at home from the
stockpile and shows what is aboard and what would fit; `L` in the windshield
opens it for the active fleet, and `load metal 400`, `load metal` (all that
fits) and `unload` do the same from the command bar. The windshield's fleet
status has a Refine button under the hold, lit while the server says what
refining would make and showing that figure; `refine` does the same from
the command bar.

A fleet is listed plainly and by class wherever it is shown: for each class
its count and the hull it holds against its full hull, and under a class
with damage a bar and how many ships are damaged and how many at risk of
exploding; the windshield's fleet status has the list with the fleet's
count, its hull, what it fires and the shield it raises in a round, the
Weapons, Shielding and Armour levels it fights with (and that a refit waits
at the home dock, when one does), and, once a ship is damaged, what the home
dock and the outpost would mend and for what, or the server's reason why
not. A hostile group is listed the same way among the contacts and in the
map's sector panel, and under its ships: how many of its muster stand when it
has losses, when it will be whole again (a time from now; for a sector out of
sensor range, as it was last seen), how far it follows a fleet that leaves
it and how long its jump takes beside the fleet's own, and, when it is out of
its sector, whom it is after or that it is on its way back. Manage fleet
opens the split picker: a count for each class, moved by steps of 1, 10 or
100 or set to all, whole ships first or the damaged first, and a Merge
button for each fleet in the sector; `split lf 100 cr 20` and
`split lf 3 damaged` do the same from the command bar.

The scope beside the lists is a picture of both sides in marks, a mark a
class, the fleet below the centre line and what it faces above. Up to four
ships of a class are drawn a glyph each. More are one glyph leading a block
of pips that grows with the square root of the count, so two, two hundred
and two thousand are told apart at a glance and five thousand is still a
block. Under a mark stand its three letters and its count (`LFT 2,000`) and a
bar in three parts, whole, damaged and at risk, which the block's pips are
tinted by as well; every figure is the state's. Both sides are drawn at one
size, the largest that fits them. The pointer, or a tap, names a mark in
full. `?` has the legend.

A round a tick is fire between the marks, from the round's own report: of a
side's volleys the eight that did the most (units destroyed, then hull
damage, then shots) are beams, in that order, heavier for more shots, and
the rest a faint haze, so twelve classes firing at twelve stay readable. A
beam that reached no hull stops at its mark's shield, which flares; bounced
shots spark off it. When the fire has landed each mark takes the round's own
count, what the round destroyed is printed over it, and a class wiped out
breaks up and leaves the scope. The scope prints the round on its centre
line and how the fight stands (the label of the fleet's own quote for
leaving, or a raid's forecast). Beside it the Battle panel has the round in
a few lines: what stands on each side and what the round took, what each
fired and the volley that did the most, and the losses so far; when the
fight ends, how, after how many rounds and at what cost, for a minute and a
half. On a phone the fight is a line over the fleets, and a round takes
down the toasts that ask nothing, which stand over the hostile half of the
scope. The log keeps one line for a fight's rounds, the last round's, and a
line for its end.

The battle view is the fight as a report, and opens over any screen: `I` on
the windshield, the Battle view button of the panel, the fight's line on a
phone, and Watch the fight on the Overview's raid card; with no fight on
show, `I` and the Fights button of the fleet status open it on the newest
fight held. It has the picture
of the round and, beside or under it, each side by class with what entered
the round, what it destroyed and what stands after it, the damaged, those at
risk and the hull; each side's fire as a sentence of its totals (shots,
power, what the other side's shields took, what reached hulls, units
destroyed) and every volley of the round as a row of who fired at whom with
what result; and the losses so far. Opened during a fight it follows the
rounds as they come; Back, Next and the arrow keys hold it on a round, and
Follow lets it go on. After a fight it steps through the rounds, and the
last eight fights that ended stay in a row at its top. The rounds are the
ones the page received: the server keeps a fight's last round and nothing of
one that is over, so a page loaded in the middle of a fight has the rounds
from then on, a reload forgets the fights before it, and the view says
which rounds it does not hold. A setting under `?` opens it by itself on the
first round of a fight of yours; it is off unless turned on.

A fleet in a fight has a way out, and the windshield shows it from the
server's quote (`fleets[].flight`): a Leave the fight panel with the odds of
the fight as it stands, what the jump out costs (fuel, ticks, rounds under
fire), who would follow and how far, and a button for each lane; each gate
card says `FLEE 1 RND` and, once the server has quoted that lane, whether a
group follows there. A gate key, a gate card, a button or `move` all send
the same ordinary `move`. When the server would refuse it the control is
greyed with its sentence. While the fleet charges its jump the panel is
Leaving the fight: where to, the count to the jump, the rounds still to
take, and Stop, with what Stop costs (the fleet fights on and the fuel is
forfeit); its side of the round is marked as fired on and not firing. On
the scope its marks turn to the lane it is leaving by and draw off toward
it, a course is drawn to the gate with `JUMP IN 2` beside the fleet, its
guns are silent and the fire it takes goes on. The
fight's end is told where the fleet lands (`FLED UNDER FIRE`). A group on a
fleet's trail (`fleets[].pursued_by`) stands in a Pursuit panel pinned over
the others: what follows, when it lands and from where, whether it lands
before the fleet can be gone, and how much further it follows; the scope
marks the lane it comes down, from the gate inward, with the ticks to its
landing and `CATCHES` or `TOO LATE`, and when it lands its marks come onto
the scope from that gate. A gate pressed while the scope is still drawing
the fleet's arrival is taken at once, which is when a pursued fleet presses
it; one pressed while a jump is charging is answered with a line and sends
nothing. The fleet's
cards on the windshield, the Overview and the map say it too, the Overview
lists it among its alerts, and the map draws the lane the group is jumping
down and a ring round the fleet. The log has a line for a pursuit begun,
continued and given up, the last with the server's reason. On a phone each
of these is a line over the fleets that opens the status sheet. `status` in
the command bar prints the quote, the flight and the pursuit of the active
fleet, and the forecast of the raid on the homeworld and on the outpost.

The homeworld (`G` then `H`) has five tabs, each drawn from the catalogue the
server sends. Buildings is a plan of the twelve buildings around the core
(the mines, the two power plants and the lab in the ring, the factories and
the yard to the west, the three stores to the east); a tile carries its level
and the first thing the building does. Over the plan stands the energy
readout: what is made and what the mines need, the share of their output the
mines run at, a warning while that is short of all of it, what each Solar
Satellite gives, and what the Fusion Reactor burns or that it has stopped for
want of deuterium. The same share is in the top bar and on the Overview.
Research draws the fourteen researches in four lanes with a line from each
research to those that require it, the lines of the selected one brightest.
Shipyard has a card a class with what a ship built now would be, the Solar
Satellite among them, which stays at home. Defence opens on the raid
forecast, which is the server's (`homeworld.raid`): where the raid stands
(none announced and one of typical size, on its way with a countdown, or
being fought with the rounds it has left), what it flies and its power, the
server's label for how it would end against what stands at home with the
sentence for it, the three shares of the fights the server played
(destroyed, outlasted, lost) as a bar, and what each side keeps. No two
powers are compared anywhere in the client. While a raid is fought the tab
has the fight under the forecast, in the few lines every fight has: the
defenders against the raiders, the rounds left of six and the odds as the
fight stands, with the battle view a press away, where the docked fleets
and the home defences, a group of their own that is no fleet, are listed
by kind and drawn as marks; and for each fleet in it the way out of the fight
(a lane, the price, that nothing follows, Stop while the jump charges). The
windshield of a fleet docked there draws the same raid: the defences as a
battery of marks beside the fleets', eight glyphs of their own and the Solar
Satellite's, the raiders above, the round of six and the forecast's label
on the centre line; so it does at the outpost, with the platform. The
ending is worded as a raid's: `RAID DESTROYED` with its wreckage, `RAID
OUTLASTED`, the raiders gone with nothing, or `RAID LOST` with what was
taken, and under the losses what was rebuilt at once. When the report is
put away the last raid stays on the tab as a summary for as long as the
page is open. Under that the tab lists what stands at home and fights:
the docked ships by class, the Solar Satellites, and a row to order each of
the eight defences from, with the rate at which defences and Solar Satellites
are rebuilt after a raid in the engine's figures; a dome is marked as one of a kind and its
order is off once one stands or is on order. A raid on its way or being
fought also stands in the top bar (under it on a phone) and on the
Overview's card, each of which opens the tab. Storage lists the three stockpiles, each with the
building that holds it and the order that raises it, beside the hold panel.
The inspector beside a tab says what a level does now and at the next, what
the item costs and requires (an amount of energy made, for Graviton
Technology), what else in the catalogue needs it, a class's figures in a
fight and in flight with the empire's research counted, and its rapid fire
in both directions. From the command bar an item is typed as its name
without the spaces, and the short names work too: `build solarplant` or
`build solar`, `research energy`, `ship lightfighter 5` or `ship lf 5`,
`defence rocketlauncher 10` or `defence rl 10`.

Outposts: in rings 9 to 20 the windshield's dock has a Found tile, and `O`
opens the founding panel anywhere: the server's survey of the sector the
fleet stands in (the platform of level 1 as the defences it is, and in the
rings an outpost stands in how a raid of the ring would go against that
platform alone), the cost of level 1 set against its hold, and a Found
button that is off, with the server's sentence under it, while the server
would refuse. Once you have an outpost `O` opens its panel instead (level and
job with its countdown, the store against cap and protected floor, the yield
an hour, what fights a raid there, the platform as rows of defences and the
fleets standing at it, the raid forecast as on the Defence tab, the
next level with its platform and Upgrade, Abandon behind a second press) with the hold panel
under it, pointed at the outpost's store for the fleets standing there. The
outpost also has a screen of its own on the rail (`G` then `P`), where a
raid's fight is told round by round under the panel with the way out for
the fleets in it and the last raid's summary after, and a line in
the HUD while it stands, a mark on the map (filled for your own) with a note
in the sector inspector, which has a Survey site button for any charted
sector. From the command bar: `found`, `survey [q r]`, `outpost upgrade`,
`outpost abandon`.

- `?name=Admiral` skips the name prompt; `?token=...` supplies the account
  token (see "Accounts and tokens"); `?ws=ws://host:port/ws` points at another
  server. The token the server issues is remembered in the browser's
  `localStorage` per server and name; if a name needs a token the browser
  does not have (or storage is blocked), the login form asks for it. A new
  token is also shown once, with a copy button, in the operator guide and
  under Alerts.
- If the server is unreachable the page says so and keeps trying. There is no
  offline demo any more: offline play is the single-player build, on the real
  engine.
- **Operator guide.** A new account gets a one-line guide under the HUD that
  walks the first steps (save the token, queue a building, scan, jump,
  harvest, unload, standing orders) and a "First hour" list on the Overview
  (raise a mine, raise the Solar Plant, build a Shipyard, build a Small
  Cargo, fly six sectors out, board a derelict, win a fight).
  Each step is ticked by what happens in the game, in any order. `Skip guide`
  turns it off; `?` (Keys and settings) turns it back on. Progress is kept per
  account in the browser.
- **Notifications.** While the tab is in the background, a finished or idle
  queue, a raid (announced with what it flies and its forecast, 60 ticks
  out, and ended with its summary), a lost fleet, a critical
  alert or full storage, and at the outpost a raid, a breach, a finished
  level, a full store or a repair that waits for its materials, is counted in
  the tab title, `(2) IN AMBER CLAD`, and,
  if you turned notifications on, raised as a browser notification. The
  browser is asked for permission only when you press the button for it (the
  one-time offer after your first order starts, or `?` > Notifications).

```sh
cd clients/web && flutter pub get && flutter analyze && flutter test
flutter run -d chrome   # dev UI; append ?ws=ws://localhost:7777/ws to reach a running server
```

### Single player

`scripts/play.sh --single-player` builds a static bundle in
`clients/web/build/single-player`: the same web client, plus `solo/iac_solo.wasm`
(the real engine and its bots) and `solo/worker.js`. Nothing in it needs a
server; put the directory on any static file host (`BASE_HREF=/path/` when it
will not sit at the root) or let the script serve it (`HOST`, `PORT`;
`--no-serve` only builds). It needs `rustup target add wasm32-unknown-unknown`
and Flutter; no other tool.

- **New game.** Commander name, pace (the server's presets, fixed for the life
  of the world), how many built-in bots (0 to 23; a world has 24 home seats)
  and how they play: check-in, hybrid, hands-on, or every pattern in turn.
  Short worlds (sprint and faster) default to hands-on bots, because a check-in
  bot looks in four times a day of ticks at any pace.
- **Same game, same protocol.** The engine runs in a Web Worker and the page
  talks to it in the server's JSON, frame for frame, so everything above the
  transport is the multiplayer client. There is no password: the world has one
  commander and the browser holds it.
- **What is saved.** The whole world (your empire, every bot and what it
  remembers, the map as each has charted it), in this browser's IndexedDB, one
  world per browser. An order is saved within two seconds; otherwise every four
  minutes, when the tab is hidden, and every twenty seconds while catching up.
  A tab killed mid-save keeps the save before it. Losing the latest save loses
  no play except orders younger than it: the ticks since are played again on
  reload and come out the same.
- **What is not saved.** Nothing leaves the browser: clearing site data, a
  private window or another browser means another (or no) world. There is no
  export or import yet. The console's log and alert list are rebuilt per
  session, as in multiplayer.
- **A closed tab.** The world does not run while the page is closed, and the
  time away is owed: on reopening, **Continue** plays the missed ticks before
  you take command, one second of the world per tick as always, with the bots
  and your standing orders acting on every one of them, raids included. It is
  the same simulation, only not drawn, so what you find is what you would have
  found had the tab stayed open. Progress, speed and time left are shown, and
  **Take command now** stops there: the ticks still owed are dropped and never
  happen. At most seven days are caught up; the rest of a longer absence is
  dropped the same way. Either way a report says how long you were away, how
  much of it the world played, what finished and what was lost, first on the
  way in and again under Alerts. A machine that slept with the tab open is
  handled the same way.
- **How long catching up takes** depends on how busy the world is. Measured in
  V8 on the development machine: a week of a persistent world with 8 check-in
  bots in 11 s (57,000 ticks a second); a week of a season world with 8
  check-in bots in 5.5 minutes (1,900 ticks/s), with 20 in 17 minutes (600); a
  blitz world with 8 hands-on bots at 350 ticks/s over its first two hours and
  180 in its third. Fewer and less attentive bots catch up faster.
- **One tab.** A second tab on the same browser is told the world is open
  elsewhere and does not run it. That takes the browser's locks, which a page
  has only in a secure context: over HTTPS, or from `localhost`. Served over
  plain http to another machine on the LAN, a second tab is not kept out and
  the two would overwrite each other's save; the front door says so there.
  Such a page cannot ask the browser to keep its save either
  (`navigator.storage` is missing too), so the browser may clear it when the
  device runs short of storage; the front door says that as well.
- **A save from another version** (save format, economy or world generation)
  is refused with the reason, as the server refuses such a world file; it stays
  stored until a new game replaces it.
- **Abandon this world** on the front door deletes the save after asking.
- **Multiplayer from the same bundle.** `?ws=` or `?name=` in the URL, or
  "Connect to a server instead", goes to a server's login as usual.
- The bundle loads nothing from other hosts. The Flutter engine would ask
  `fonts.gstatic.com` for its default Roboto; `pubspec.yaml` names the bundled
  Plex Mono as the `Roboto` family, so neither build of the client does.

```sh
scripts/play.sh --single-player --no-serve          # just the bundle
cd clients/web && dart run tool/solo_e2e.dart        # the bundle end to end in headless Chromium, a flight, a fight and a raid at home included (about five and a half minutes)
cargo test -p iac-solo                               # the host natively, through the same door of bytes
cargo test -p iac-solo --test both_targets           # the same worlds natively and as iac_solo.wasm: every reply the same bytes (about two minutes)
```

A world is the same world wherever the engine runs. `solo/tests/both_targets.rs`
builds `iac_solo.wasm`, runs it in an interpreter beside the native host and sends
both the same requests: a blitz of bots, a season day, and a commander who trades at
the hub, founds an outpost and stays away while it is raided. Every reply and every
save must be the same bytes; a failure names the tick and the first value that
differs.

And it is the same world whichever host runs it. The server and the single-player
host share their hosting (`iac_sim::host`) and keep only what a network or a tab
adds, and `server/tests/hosts_agree.rs` holds them to it: a world made by a real
server is stopped, its file is loaded by the single-player host as a save (the
same bytes, with a commander and a clock named in the header's own part), the
server is started again, and one commander plays both for 40 ticks with the same
orders on the same ticks. Every frame the two send must be equal, and so must the
bytes of the worlds they end with (`cargo test -p iac-server --test hosts_agree`,
about 45 seconds).

### The protocol has one source

The wire types are written once, in Rust (`shared/src/protocol.rs` and the
enums it uses from `constants.rs`, `scaling.rs` and `world.rs`). The Flutter
client's copy, `clients/web/lib/protocol/protocol.g.dart`, is generated from
them and checked in, so the client builds without cargo. Nobody edits it.

A protocol change is three steps:

```sh
# 1. edit the Rust types
UPDATE_FIXTURES=1 cargo test -p iac-shared   # 2. regenerate the Dart types and the fixtures
cd clients/web && flutter analyze            # 3. fix what the analyzer points at in the app
```

`cargo test -p iac-shared` (and so `cargo test --workspace`) fails while the
checked-in Dart file or a fixture is stale, and names the first line that
differs.

How it works, all in `shared/tests/` (`golden_fixtures.rs` and `wire/`):

- `wire/model.rs` reads the structs and enums out of `shared/src` with `syn`:
  everything `ClientMessage` and `ServerMessage` carry, with their serde
  attributes and doc comments. An attribute or a type it does not know is
  refused by name, not guessed at.
- `wire/samples.rs` holds that reading against serde itself. It makes
  messages from the model alone, for every type, variant and field (all
  fields present, only the required ones, every optional one empty), has
  serde parse each into the Rust type and emit it again, and requires the
  result to be what the model says. The values serde fills in for missing
  fields are learnt there, not typed.
- `wire/dart.rs` writes the Dart file, laid out as `dart format` would at the
  width the file declares.
- `wire/mod.rs` lists what the client is given beside the wire: the Rust
  functions that become columns of its enums (`ShipClass.label`, its base
  stats and power, `ResourceKind.weight`, `ErrorCode.code`) and the constants
  (`botNamePrefix`, `maxOddsSectors`, the ratio bands, the outpost rings).
  Adding one is a line there.

`clients/web/lib/protocol/protocol.dart` is written by hand and holds only
what the client adds to the generated types (extension getters such as
`WorldInfo.servicesAt`, `HomeworldState.copyWith`). `hex.dart` is written by
hand too: hex math, pinned by `fixtures/hex`.

`fixtures/` is golden JSON generated from the same Rust types and is the
end-to-end proof that the generated Dart reads and writes what serde does.
`fixtures/protocol/` has every `ClientMessage`, `Command`, `ServerMessage`
and `EventKind` variant with realistic values, written by hand in
`golden_fixtures.rs`, and under `synth/` the messages `wire/samples.rs` made,
which cover every field by construction. The Flutter tests decode and
re-encode each one (`test/protocol_fixtures_test.dart`) and check hex math
cell by cell.

### Economy numbers

A number of the economy is written once, in Rust (`shared/src/constants.rs`,
`scaling.rs`, `pace.rs` and `sim/src/score.rs`). The planning simulation,
`docs/design/economy/sim.py` (Python standard library; its `--tables` and its
full report are the economy's tables, and `docs/design/economy/spec.md` is the
record of the tuning before the catalogue was OGame's), declares none of them: it reads
`docs/design/economy/numbers.json`, which `sim/tests/economy_numbers.rs`
renders. That test runs in `cargo test --workspace` and fails when the file is
stale, naming the first line that changed.

```sh
UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers   # after changing a number: render the file again
python3 tools/check_sim_constants.py                             # the formulas sim.py keeps still agree with Rust
python3 docs/design/economy/sim.py --set BETA=0.4                # try a number in the simulation before changing the game
```

The file has two halves. `game` is what the simulation reads: the constants
under their Rust names, the cost curves, the requirements, the ships and
defences with their fight figures, rapid fire and flight, and the
curves it looks up at whole rings, threat ratings and levels (a group's class
and presence, ore odds, energy made and needed, storage, what a research
level multiplies). `tables` is what the
Rust formulas give where the simulation computes for itself, because it needs
a formula at any pace or any score: `check_sim_constants.py` compares those,
4,410 values. One copy of the game is still kept by hand in `sim.py`:
the terrain shares and ore chances of `zone_terrain` and `ore_prob`, which
worldgen rolls inside `shared/src/world.rs`.

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
- **The catalogue is classic OGame's** — the buildings, the research, the
  ships and the defences are those of OGame 0.84, with its names and its
  numbers (`docs/design/ogame.md`): 12 buildings, 14 researches, 13 ships
  and 8 defences. The game around them is this game's: the hex map, fuel
  tanks, mining, derelicts, pirates and raids, outposts, standing orders,
  the hub and pace. The fight in open space is OGame's round, one a tick,
  between fleets of up to 5000 ships, which a fleet may leave under fire
  and from which a group keeps its losses until it has mended (Combat,
  below). A raid on your homeworld or your outpost is the same fight, for
  at most six rounds, and it is where your defences fire (Raids, below).
  A new empire has a Metal Mine, a Crystal Mine and the Solar Plant that
  powers them, all at level 1, and two Light Fighters.
- **Economy** — metal / crystal / deuterium from homeworld mines and fleet
  harvesting. A mine makes `floor(base x L x 1.1^L)` per game hour (metal
  30, crystal 20, deuterium 10 times 1.2 for the homeworld's temperature;
  all times P), and 20 metal and 10 crystal an hour come with no mine at
  all. Costs double per level, or grow x1.5 and x1.6 for the mines and the
  Solar Plant and x1.8 for the Fusion Reactor; times follow cost (below).
  Sector deposits are small nuggets (see Ore and loot below).
- **Energy** — the mines need it: `ceil(10 x L x 1.1^L)` each for the Metal
  and the Crystal Mine, twice that for the Deuterium Synthesizer. The Solar
  Plant makes `floor(20 x L x 1.1^L)`, each Solar Satellite 30, and the
  Fusion Reactor `floor(30 x L x (1.05 + 0.01 x Energy Technology)^L)` while
  it burns `ceil(10 x L x 1.1^L)` deuterium an hour. Energy is not stored.
  When less is made than needed, all three mines run at made over needed
  (`homeworld.energy`: `made`, `needed`, `fraction`); the natural production
  is not touched. `homeworld.production` is what the stockpile gains a tick
  with the energy there is, less the reactor's burn, so its deuterium may be
  negative; a reactor with no deuterium to burn stands still
  (`fusion_starved`). Solar Satellites are built in the shipyard as ships,
  stay at the homeworld (`homeworld.satellites`), never join a fleet, and
  stand in every raid's fight with a hull of 200, a shield of 1 and an
  attack of 1: every armed ship fires again four times in five after a shot
  at one, so a raid that is not destroyed quickly takes them by the dozen.
  Those that fall are rebuilt as defences are, about 70 in a hundred at
  once and for nothing when the raid ends (Raids, below), and the rest are
  gone. Raise the Solar Plant before a mine, not after.
- **Ore** — an ore tile holds 9 / 30 / 66 / 126 raw units (Sparse to Pristine)
  times 1.10 per ring from ring 3 (x5 from ring 20), and the further out the
  richer the density rolls. Harvesting takes units out of the tile (depletion
  counts raw units; Laser Technology raises what lands in the hold, +20% a
  level for its first five, not how fast the tile empties) and the density
  drops a level at a time. A Recycler mines 5 units a tick, a cargo ship 1,
  any other ship 0.5, times the density's multiplier.
  A stripped tile refills completely in `120 + 24 x ring` game hours divided
  by P, but not while a fleet sits on it, and never above its generated
  density. Sector views carry `ore_reserve` per resource: `units`,
  `max_units` and `refills_in_s`. Ore is a bonus on top of the mines, not a
  living.
- **Threat** — NPC group power grows 1.22x per ring (`40 x 1.22^(ring-1)`;
  a Light Fighter is 91 on that scale), and a group's ships add up to exactly
  that, so the ring estimate is the real strength of an unscouted lair. A group flies the heaviest of Light Fighter,
  Heavy Fighter, Cruiser and Battleship of which its power makes at least
  three ships, each weakened or strengthened to its ring's power: one or
  two Light Fighters to ring 8, then packs of three to eight of them to
  ring 15, of three to seven Heavy Fighters to ring 20, of Cruisers to
  ring 25 and of Battleships beyond, so a group past the Inner Ring that
  loses a ship keeps the rest. The two starting Light Fighters win every
  lair of ring 5 (keeping about nine tenths of their hull), nearly every one
  of ring 7 (58 of 60, keeping two thirds), two in three at ring 8, one in
  ten at ring 9 and none from ring 10; a third Light Fighter makes ring 8 a
  sure win. Every
  sector carries `threat`:
  `rating` T1 to T9 (one step per doubling of power over 40), `est_power` and
  `basis` (`observed`: a group is there, its own power, Aggressive and Swarm
  +25%; `template`: a lair that is away or cleared; `estimate`: ring only).
  A stale chart entry keeps the rating last seen. Scans list the threat of
  every revealed sector (`threats`) and each faint contact has a
  `threat_band` (1: T1-3, 2: T4-6, 3: T7-9). Pirates return
  `2 + 0.25 x ring` game hours after a kill, divided by P^0.5 (a finds
  timer, as a derelict's is: 52 minutes to 2.2 hours for rings 3 to 20 in a
  season, 7 to 17 minutes in a blitz).
- **Odds** — `threat` says how big a group is; your odds against it are a
  separate number that only the server gives (`preview_move` for a jump,
  `odds` for any sectors). There is no second model of the fight: the
  server plays the real round, eight times or more with dice of its own,
  from the two sides as they stand (damage and the levels each fights at
  included; two sides of more than 200 units are scaled down together), and
  reads off what share of its worth each side keeps and how often yours
  won. That is put on the scale of a ratio between equal ships: `ratio` 1.0
  is an even match, the scale runs 0.1 to 10. Each `label` is the same
  outcome whatever the classes: SAFE from 2.5 (a win that keeps nine tenths
  of the fleet), FAVOURABLE from 1.5 (a win every time it was played,
  keeping two thirds to nine tenths), EVEN from 1.1 (won three times in
  four or better, at the cost of a third of the fleet or more), RISKY from
  0.9 (as likely lost as won, fought to the end), DEADLY below (the fleet
  is lost unless it leaves). The odds are of a fight fought to its end: a
  fleet may leave one, and they do not count on it. A fight
  that was not won every time it was played is never rated above EVEN, one
  won less than four times in five never above RISKY, and one in which the
  fleet was destroyed every time is DEADLY, whatever was kept or taken
  along. Power against power is not it: three Light Fighters
  are 1.14x the power of the ring-10 pack of three Light Fighters and rate
  1.09x RISKY against it (46 fights of 60 won, two fifths of the hull kept on
  average); one Cruiser, which fires again five times in six at a Light
  Fighter, is 2.3x the power of the ring-11 pack and rates 3.87x SAFE (95%
  kept). In OGame's round
  a ship at 70% of its hull or under may explode at every shot that lands,
  so a win costs ships and not only hull, and SAFE takes a fleet that ends the fight in a round or two: four Light
  Fighters at ring 7, six at ring 10 or one Cruiser at ring 11. A sector with nothing in sight is
  rated against the group its ring spawns, a remembered one against what
  was last seen there: a group seen wounded is rated as it was seen until
  the tick it was to be whole by (`whole_at`), and whole from then on; one
  seen passing through after a fleet is not counted there. A fleet in a
  fight is given the odds of that fight as it stands, both sides with what
  they have lost, in `fleets[].flight`. The dice of the odds are not the dice of the fight,
  and rapid fire comes in streaks, so a FAVOURABLE fight can still go badly
  once in a long while.
- **Buildings** — OGame's twelve: the three mines, Solar Plant, Fusion
  Reactor (Deuterium Synthesizer 5, Energy Technology 3), Robotics Factory
  (building time / (1 + level)), Nanite Factory (Robotics Factory 10,
  Computer Technology 10; halves building, ship and defence time a level),
  Shipyard (Robotics Factory 2; ship and defence time / (1 + level)), Metal
  Storage, Crystal Storage, Deuterium Tank (which also adds 25% a level to
  every fleet's tank) and Research Lab (research time / (1 + level)). Time
  is `(metal + crystal) / 2500` game hours for a building, a ship or a
  defence and `/ 1000` for research, before those divisors and divided by P;
  deuterium in a cost adds no time. Levels stop at 20. Every entry of
  `homeworld.catalog` carries `effects`: what the thing does at its level
  and at the next (`kind`, `now`, `next`), so no client has to know a
  formula.
- **Storage** — each resource is capped by its own building:
  `100000 + 50000 x ceil(1.6^level - 1)` (100k with none, then 150k, 200k,
  300k, 400k, 600k ...), times P^0.75 up to pace 10, then growing as P^0.25
  (`homeworld.storage`: `cap`, `protected`, `full_in_s`, `capped`).
  Mines stop at the cap and the surplus is lost; a payment bigger than a cap
  is refused (`StorageTooSmall`, naming the building and the level it
  needs); docked
  cargo unloads only what fits and the rest waits aboard
  (`cargo_blocked: true`). `StorageNearCap` (85%) and `StorageFull` events
  warn you. A storage building also protects 8% of its cap per level (to
  60%) from raids.
  A full stockpile has a way out: `load` the surplus into a hold and sell it
  at the hub (Loading a hold, below).
- **Queues and slots** — buildings and research queue ahead of resources;
  ship and defence batches are paid in full when ordered (below). A building
  or research order that can ever start is always accepted, whether or not a
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
  such as a Shipyard behind the Robotics Factory it needs, never holds the line.
  Research and buildings can both be queued behind a prerequisite that is
  itself still queued, and a tech that is running can be queued again for its
  next level. A ship or defence batch is one order under a different rule: it
  is accepted only if the stockpile covers the whole batch now, pays for every
  unit at that moment, and is otherwise refused with `InsufficientResources`
  ("Light Fighter x5 costs 15000 metal and 5000 crystal; the stockpile is
  short by 1200 metal. Nothing was ordered: a batch is paid in
  full when it is ordered. The stockpile covers 4."). Nothing in the yard ever
  waits on resources: a paid batch waits only for the yard to be free or for a
  queued Shipyard level or tech it needs, takes no `reserve`, and once started
  builds every unit without stopping (`shipyard_queue` shows `built` of
  `count`). Buildings run in `build_slots` parallel
  slots (1, plus one at Computer Technology 3 and one at 6;
  `build_slot_unlock` says what opens the next); research and the
  shipyard have one.
  Each queue holds its running items plus `queue_waiting_max` (3) waiting ones,
  so the building queue holds `build_slots + 3` and the others 4; a full queue
  refuses with `QueueFull`. Orders that can never start are refused with a
  specific error: `PrerequisitesNotMet` (naming what is missing),
  `MaxLevelReached` (also a second dome of a kind), `StorageTooSmall`
  (naming the storage level it needs),
  `NoShipyard`, `NoResearchLab`, `ShipLocked`, `DefenceLocked`. For a batch,
  `StorageTooSmall` means the whole batch costs more than a stockpile cap and
  says how many units fit; it is asked before the stockpile is, so a batch
  larger than a cap is refused even when refunds (below) have left you the
  resources for it.
  `build_pending` / `research_pending` list each waiting
  order with its `id`, `waiting_for` (the exact shortfall), `waiting_on` (`Slot`,
  `Resources`, `Prerequisite` or `Order`: held behind an earlier `reserve`
  order) and
  `start_in` (estimated ticks to start at current production, ignoring what the
  other queues spend). `shipyard_pending` lists each paid batch with its `id`,
  `count`, `cost` (what it paid), `waiting_on` (`Slot` or `Prerequisite`,
  nothing else) and `start_in`. Every order, start and cancel is answered by a
  `Queue` event: `Started`, `Waiting` (shortfall and estimate) or `Cancelled`
  (what was refunded: half for a running building or tech, nothing for a
  waiting one, half of the undelivered units for a batch, waiting or running).
  A cancel returns half of what the order has paid and no more: a running
  building or tech level gives back half its cost; a waiting one paid nothing
  and gives back nothing; a batch, waiting or running, gives back half the
  price of every unit not yet delivered (the one under way included) and keeps
  the units already built. The refund is added to the stockpile past the
  storage cap: none of it is lost when the stockpile is full, the mines
  add nothing while a resource stands at or over its cap, and what is over the
  cap is spent like the rest (a raid counts it too).
  `paid` on the event is what it took from the stockpile: a building or tech
  pays on `Started`; a batch pays on the event that answers the order
  (`Started`, or `Waiting` when the yard is busy) and nothing when it starts
  later. A queue with nothing running but orders waiting is not idle.
- **Research** — OGame's fourteen, each level twice the price of the last.
  What OGame's requirement tree asks is in each catalogue entry's `requires`.
  Weapons, Shielding and Armour Technology add 10% a level to attack, shield
  and hull, for defences at once and for a fleet when it next docks at the
  homeworld: a fleet fights at the levels it was last refitted with
  (`fleets[].refit`: `weapons`, `shielding`, `armour`; `refit_due` while the
  empire's research is ahead of it). A fleet docked at home is refitted the
  tick the research completes, a ship built joins a docked fleet at the
  current levels, a fleet split off keeps the levels of the one it left, a
  merged fleet takes the lower of each, and a wreck recovered in the field
  fights at the levels of the fleet that took it. The outpost's dock mends
  and does not refit. The three drives add 10, 20 and 30% a
  level to the speed of the ships that fly on them. Five do this game's own
  jobs: Energy Technology takes 5% a level off what a jump burns (to a half)
  and raises the Fusion Reactor's output; Espionage Technology is how many
  hops around the homeworld are seen; Computer Technology 3 and 6 open
  building slots; Laser Technology's first five levels raise ore yield;
  Hyperspace Technology takes 5 points a level off the emergency jump's
  risk. Ion and Plasma Technology are requirements only. Graviton costs no
  resources: the homeworld must be making 300,000 energy when it is ordered.
  Levels stop at 20.
- **Ships** — a ship is unlocked by its Shipyard level and research
  (`requires`), not by a tech of its own. Light Fighter (Shipyard 1,
  Combustion Drive 1), Small Cargo, Heavy Fighter, Espionage Probe, Large
  Cargo, Recycler, Cruiser, Battleship, Battlecruiser, Bomber, Destroyer,
  Deathstar and the Solar Satellite. Hull in a fight is a tenth of OGame's
  structure. This game gives each class three things OGame has no use for:
  a tank (a number of jumps of its own fuel figure: 20 for the Probe, 12 for
  the cargo ships and the Recycler, 10 for the fighters, 8 for the Cruiser
  and the Battlecruiser, 6 for the heaviest), a harvest power (above), and
  two roles: an Espionage Probe aboard widens a scan to 2 hops and reads a
  derelict before boarding, a Recycler aboard doubles what a boarding
  strips. The enum columns of the generated wire types and
  `homeworld.catalog.ships` carry every figure; `world.rapid_fire` is the
  whole rapid-fire table.
- **Fleets** — up to 3 away from the homeworld and the finished outpost (8 in
  all), up to 5000 ships each, hex movement paid in
  fuel. A fleet is a count for each class and the hull of each ship that is
  not whole; a ship has no id. `fleets[].ships` is one row a class in
  catalogue order: `class`, `count`, `damaged` (how many are not
  whole), `at_risk` (how many of those stand at 70% of their hull or under,
  where a shot that lands may destroy them), `hull` and `hull_max` for the
  class together, and `unit_attack`, `unit_shield` and `unit_hull`, what one
  ship of the class fights with at the fleet's refit. The fleet's own
  `ship_count`, `hull`, `hull_max`, `attack` and `shield` are those summed.
  Docking at home unloads cargo, refuels, mends and refits but never merges
  fleets: use `split` and `merge`. New ships join an idle fleet at home that
  has no standing orders, or form their own. Each fleet reports its `power`
  (the scale raids use) and `range_hops`: how many more hops out it can fly
  and still get home.
- **Damage and repair** — a shield is full again every round and costs
  nothing; damage to a hull stays until a dock mends it. The home dock
  mends a fleet when it docks and every tick it stands there, at once and
  for nothing. The outpost's dock mends for a tenth of a ship's cost times
  the share of its hull restored, paid from the outpost's store in whatever
  it holds (metal first, then crystal at 1.5 and deuterium at 2, as its own
  repair is): whole ships only, the most damaged first, as far as the store
  pays, and not while it is breached or unfinished, nor for a fleet under
  standing orders. Either dock says what it did in an `Alert` ("fleet 2 was
  mended at the home dock: 2 ship(s) made whole, 67 hull restored, for
  nothing"; the outpost's names what it took from the store and how many
  ships still wait for its stock to cover them). `fleets[].repair`
  is there while a fleet carries damage: `ships`, `hull` (what is missing),
  and for `home` and for `outpost` (absent with no outpost) the
  `price`, the `payment` it would be taken as, the `ships` and `hull` it
  would mend now, and `refusal`, the sentence that says why it would mend
  nothing now.
- **Fuel and stranding** — each fleet reports `jump_fuel` (one jump),
  `hops_home` (the fewest hops home over the real lanes, charted or not)
  and `home_fuel` (the fuel for those hops) next to `fuel`, and `jump_ticks`: the
  ticks one jump takes it from the order to the arrival. A jump burns the
  sum of its ships' OGame fuel figures (20 for a Light Fighter, 300 for a
  Cruiser, 1000 for a Destroyer), less Energy Technology's saving, and takes
  `max(1, ceil(25000 / speed))` ticks at the speed of its slowest
  ship with the owner's drive research: 2 for Light Fighters (1 from
  Combustion Drive 10), 2 for Cruisers until Impulse Drive 4 and 1 from it,
  3 for a Heavy Fighter until Impulse Drive 2, 5 for a Small Cargo until
  Combustion Drive 3, 4 for a Large Cargo, 13 for a Recycler, 1 for
  Probes. A fast fleet is something you build: a Cruiser on Impulse Drive 4
  outruns every pirate of the Outer Ring, whose jumps take 2 ticks, and
  heavy and cargo hulls are slow to flee. Drive, fuel and tank research reach
  every fleet at once. Refuelling at the
  homeworld costs 0.08 deuterium per fuel unit (the Deuterium Tank adds +25% tank
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
  over. `merge` with a fresh fleet from home pools fuel and also rescues it,
  and so does `refine` when there is deuterium in its hold (see Refining).
  `recall` costs 2x the jump fuel per hex, all or nothing; ships that break
  up in it leave their share of the cargo where the fleet jumped from.
- **Combat** — a fight in open space is OGame's round, one a tick, between
  every fleet and every hostile group in the sector, with no limit of
  rounds: it ends when a side is destroyed, when the last fleet has left,
  or in a stand-off. In a round every
  shield is full, every ship of both sides fires at once, and each shot
  picks one ship of the other side, any as likely as another. A shot weaker
  than a hundredth of its target's shield bounces; otherwise it takes its
  attack off the shield in hundredths of the full shield, and what the
  shield does not hold comes off the hull. A ship hit with no shield left
  and its hull at 70% or under explodes with a chance of one minus the
  share of hull it holds. Rapid fire chains by OGame's table: a Cruiser
  fires again five times in six at a Light Fighter (`world.rapid_fire`).
  What exploded is gone when the round ends. The numbers are whole and the
  dice are the fight's own, so the same fight is the same rounds wherever
  it is played. Pirates fight by the same round with their ring's
  multiplier on attack, shield and hull. A round nobody lost hull in ends
  the fight as a stand-off: both sides stay where they are, as they are,
  and nothing starts it again but an arrival or an `attack`.
  *Leaving.* A fleet in a fight may `move` to a connected sector. It pays
  the jump's fuel at once and its jump charges as any does, set by its
  slowest ship; while it charges it fires nothing and is fired on, for one
  round fewer than its `jump_ticks` and never fewer than one: Cruisers and
  Light Fighters take one parting round, Large Cargos three, Small Cargos
  four, Recyclers twelve. `fleets[].flight` quotes it beforehand (`fuel`, `ticks`, `rounds`,
  the `ratio` and `label` of the fight as it stands, a `refusal` sentence
  when the tank is short, and `chase`); `fleets[].fleeing` (`to`, `ticks`,
  `rounds`) says a fleet is leaving. `stop` calls the jump off, forfeits its
  fuel, and the fleet fights on. What flight costs is those rounds, the
  damage (it stays until a dock), the fuel, and everything the fight had
  earned: a group pays its wreckage when it is
  destroyed and not before. `recall` is the other way out: at once, all the way home, for
  twice the fuel and a chance of harm to every ship.
  *Caught while charging.* The same holds for a fleet that ordered a jump
  before any fight: if a group attacks it in its sector while its jump
  charges, it is in the fight as a fleet that is leaving, fired on every
  round until the jump lands and not firing back, and the jump goes on
  (`stop` calls it off, its fuel forfeit, and the fleet then fights). This
  is meant: a fleet that has paid for its jump has turned its back. One
  `Alert` (warning) says so when it happens, with the ticks until the jump
  lands.
  *Pursuit.* When the last fleet has left, a group goes after the strongest
  of those that jumped out, if it was beating it (its power at least the
  fleet's) and the fleet is next door: convoys never, patrols one jump,
  packs and swarms two (`hostiles[].chase_hexes`), never into home ground,
  the hub or a sector an outpost stands in. It jumps at its own speed
  (`hostiles[].jump_ticks`: its class on a drive that rises a level every
  four rings past ring 8) and starts on the tick the fleet lands. A fleet
  that jumps on at once and is as fast is never caught; one that is slower
  is fired on, without answering, for the difference in ticks at each hex,
  and one that stops is caught and the fight is joined again, by any fleet
  waiting there too. `fleets[].pursued_by` (`group_id`, `label`, `from`,
  `arrives_in`, `catches`, `hexes_left`) and a `Pursuit` event (`phase`:
  `began`, `continued`, `abandoned` with `why`) say so; a group on its way
  shows its `errand` (`home`, `quarry`, `quarry_owner`, `to`, `arrives_at`,
  `hexes_left`) where it stands. Its own sector is empty meanwhile. When
  its leash ends or the fleet is out of reach it jumps back.
  *Pirates keep their losses and mend.* A group that is not destroyed stays
  as the fight left it (`hostiles[].ship_count` of `muster`, damaged ships
  with the hull they hold), and is found so by the next fleet. At rest in
  its own sector it mends in ten steps over half the time a destroyed group
  takes to return (1.4 to 3.5 game hours for rings 3 to 20, divided by the
  square root of the pace: 26 to 66 minutes in a season, 4 to 9 minutes in
  a blitz): at each step a
  tenth of its ships return, as far as it is short, and every damaged ship
  regains a tenth of its hull. `hostiles[].whole_at` is the tick it will be
  whole by. A fight or a pursuit stops the clock, and the ticks rested
  toward the next step are lost. Killed after any number of visits it pays its whole
  wreckage, as it was generated.
  One `BattleRound` event a round takes the place of a message a shot:
  `round`, and for `fleets` and `hostiles` the `groups` as they stand after
  it (`fleet_id`, `owner`, `label`, `mine`, `fleeing` for a fleet that is
  leaving and did not fire, and by class `entered`, how many stood in the
  round when it began, `count`, how many stand after it, `damaged`,
  `at_risk`, `hull`, `hull_max` and `lost` this round), the `volleys` they
  fired by class of firing ship and of target (`shots`, `bounced`, `wasted`
  on a ship already destroyed, `absorbed` by shields, `hull_damage`,
  `destroyed`), and their sums with `power`, the attack of every shot,
  `entered`, the units the side stood in the round with, and
  `standing`, the units the side has left. It
  is the same size for five ships as for five thousand. `CombatEnded` says
  how it ended (`outcome`: `hostiles_destroyed`, `fleets_destroyed`,
  `both_destroyed`, `fled` when the last fleet jumped out under fire,
  `withdrawn` when it made the emergency jump, `standoff`), after how many
  `rounds`, and
  in `losses` what every fleet and group lost over the whole fight by
  class. While a fight goes on, `battles` in the full state and in every
  tick update holds it for the empires in it: `sector`, `rounds` and the
  `losses` so far, and in the full state the `last_round`, which a tick
  update carries as its `BattleRound` event instead. Ships that break up
  in a `recall` are a `ShipsLost` event.
  Victors drop salvage. `FleetDestroyed.salvage` is what that kill added to
  the sector's pile, `CombatEnded.salvage` the sum of the fight's kills, and
  when wreckage from before still lay there `CombatEnded.salvage_pile` is the
  whole pile, which is what `collect_salvage` takes and the sector view shows.
  The pile lasts 180 ticks from the last kill, its sector view carries
  `salvage_despawn_tick`, and a `SalvageDespawned` event says when one
  expires. You only receive combat
  events for fights your fleets or homeworld are in, or that happen in a
  sector where you have a fleet or your homeworld. Each carries `owner`
  (empire name, `null` for NPCs) and `mine`; losing a fleet also raises a
  `Critical` `Alert`. A fleet that loses ships keeps only what the holds of
  the ships it has left carry: the rest, each resource in proportion, drifts
  as wreckage in that sector, where anyone can `collect_salvage` it for 180
  ticks, and a destroyed fleet leaves its whole cargo so (an `Alert` says how
  much and where). Fuel beyond the tanks that are left is lost. Hostile ids in a sector view are the ids `attack` and
  `CombatStarted.enemy_fleet_id` use; messages name a hostile by its `label`
  ("2x light fighter pack"), not by id. `ResourceHarvested` is one event per fleet
  every 10 ticks (and when harvesting stops).
- **Scanning** — active `scan` reveals connected sectors (2 hops with an
  Espionage Probe in fleet, 1 without) and picks up *signals* one hop farther: rich ore,
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
  with one of your fleets (it pays when that fleet docks at home or at your outpost alive and is
  lost with the fleet), `"delivered"` once it is banked; a sector without
  `chart` still has its chart to pay. A remembered sector is sent
  once more when its mark changes.
  The `signals` of a scan lie one hop beyond the scanned range and never name
  the scanning sector, which is `ScanCompleted.sector`.
- **Derelicts** — dead ships drift in debris fields and empty space beyond
  the shipping lanes (`D` on the map, "derelict transponder" on scans).
  Board one (`explore_site`, 20 ticks) for tiered loot, sometimes a
  recoverable ship or an instant-tech data core — or an ambush by whatever
  killed the crew. Aborted/failed boardings leave the site *hotter*. An
  Espionage Probe aboard reads the risk down; a Recycler doubles the haul.
  An ambush party is a pack like any other: it follows a fleet that leaves
  it two jumps, keeps its losses, goes back to the wreck and mends there,
  and is gone once it is whole.
- **Loot** — everything the map pays is three times what it paid before the
  catalogue was OGame's (`MAP_PAYS`, provisional), against costs that are
  about ten times what they were. A kill pays `1.71 x group_power^0.8` units (three times
  what it paid when a kill also scored) (55% metal, 30%
  crystal, 15% deuterium) times P^0.5, for the whole group; far hunting pays
  more but the odds fall faster than the pay rises. Derelict tier follows the
  threat (T1-3, T4-6, T7-9). A kill in a sector that has been killed in
  recently pays less: each recent kill multiplies the next one's wreckage by 0.75 (heat halves every 4 respawn delays), so a sector is not an
  endless tap. Holds grow with the pace like the loot: a ship's cargo is
  multiplied by P^0.5 (OGame's holds: 5,000 for a Small Cargo, 25,000 for a
  Large one, 20,000 for a Recycler, 50 for a Light Fighter; a blitz Small
  Cargo holds 122,000). A site pays `720 x 1.14^(ring-4)` units at 60 to
  140% times P^0.5; ambush odds `0.10 + 0.03 x (T-1)` (max 0.35); a recovered
  ship `0.05 x (T-2)` (max 0.30), a data core `0.03 x (T-5)` (max 0.15), and
  from T6 an ancient relic (2% per point above 5, max 8%), with which the site pays six times its loot. A
  chart delivered to your homeworld or your finished outpost pays
  `50 + 50 x T^1.5` units of loot into your stockpile for each sector new to
  your empire (T its ring's threat rating), at any pace, as far as your
  caps allow (a `Stockpile` event with `cause: "charts"`). A stripped derelict is replaced after 120 game hours / P^0.5.
- **Standing orders** — assign a fleet a doctrine and the server flies it
  (see `policy_update` above). Doctrines respect fuel/cargo thresholds,
  return home to deposit and refuel within their fuel budget, go home to be
  mended when their hull is low, go round fights they need not take, leave
  one their odds have turned against, run for home with a group on their
  trail, and log every decision.
- **Defences** — OGame's eight: Rocket Launcher (Shipyard 1), Light Laser,
  Heavy Laser, Gauss Cannon, Ion Cannon, Plasma Turret and the two Shield
  Domes, of which a homeworld holds one each. They are built
  from the shipyard queue, stay home, burn no fuel, and cost 16 to 32
  resources per point of power (warships cost 37 to 59). Weapons, Shielding
  and Armour Technology raise every structure standing, at once.
  `homeworld.defences` lists them. They fire in one place only: the fight
  of a raid on your homeworld (Raids, next). Each is a unit of the round as
  a ship is, with a hull, a shield that is full again every round, and one
  shot a round; no defence has rapid fire, and ships have it against some:
  a Cruiser fires again nine times in ten after a shot at a Rocket
  Launcher, a Bomber nineteen in twenty at a Rocket Launcher or a Light
  Laser and nine in ten at a Heavy Laser or an Ion Cannon; nothing has it
  against the Gauss Cannon, the Plasma Turret or the domes
  (`world.rapid_fire` is the whole table). A dome is a hull and a shield
  that soak shots: a shot under a hundredth of its shield bounces, and the
  shield is full again the next round. `home_defence_power` and its three
  parts (`docked_power`, `satellite_power`, `structure_power`) are sums on
  the power scale and decide nothing. A catalogue entry with a limit
  (`max`) says how many more may be ordered now (`room`).
- **Raids** — once your empire is worth raiding (Shipyard ≥ 2 or any
  defence built), pirates occasionally vector at your homeworld: a roll every 6 hours
  at pace 1 (30%), at most one per 18 hours, never in your first day, all
  divided by P^0.75; each eligible roll that misses raises the next one's odds
  by 15 points, so a raid is never long overdue. A raid's power is `50 + 9 x S^0.75` (S = your building
  and research points, ships and defences excluded; a Rocket Launcher is 102
  of power, a Light Fighter 91) times a roll of 0.80 to
  1.15, so it grows with your economy and not with your defence.
  **What it flies** answers what you built: Light Fighters; once 20 Rocket
  Launchers stand, two fifths of its power in Cruisers; once 20 Light
  Lasers, Heavy Lasers and Ion Cannons stand between them, three tenths in
  Bombers. So a wall of one cheap defence brings the class that shreds it.
  `RaidIncoming` warns `600 / sqrt(P)` ticks ahead (30 to 600) with the
  raid's `est_power`, its `ships`, and `odds`; power and ships are fixed
  from then on.
  **The forecast.** `homeworld.raid` is in every state: the raid being
  fought (`rounds_left` set), else the raid announced (`arrival_tick` set),
  else, with none on its way, one of typical size were it
  announced now; its `power`, its `ships`, and `odds` against everything
  standing at home this tick. The server plays the fight itself 8 to 64 times
  over, as it does for a sector's odds: `destroyed`, `outlasted` and `lost`
  are the shares of those fights that ended each way, `kept` the share of
  their worth your ships, satellites and defences keep (before anything
  is rebuilt), `raiders_kept` the raiders' share, and `label` is `repelled`
  (destroyed every time), `held` (never lost, not always destroyed),
  `at_risk` (lost in some) or `lost`. Build against the label, not against
  a power figure: nothing compares two powers any more. `repelled` and
  `held` say the raid does not breach the dock, not that it costs nothing:
  what falls before the last raider does is lost, and `kept` below 1 is
  that cost.
  **The fight.** When the raid arrives it stands at your homeworld and
  fights the next tick: the round of every fight (Combat, above), one a
  tick, the raiders against every ship of yours standing at home, every
  Solar Satellite and every defence, for at most 6 rounds. Your ships fight
  with their refit, satellites and defences with your research as it
  stands. `battles` holds the fight while it lasts, with `raid` (`mine`,
  `outpost`, `power`, `ships`, `rounds_left`, `odds` from where it
  stands); `homeworld.raid` is that raid too until it ends, with the same
  `rounds_left` and odds, and no `arrival_tick`.
  Each round is a `BattleRound` with `raid: true`, your fleets as
  groups and your defences and satellites as one more group marked
  `battery` (label "home defences"). It ends one of three ways:
  no raider is left (`hostiles_destroyed`): repelled, and three tenths of
  what the raiders cost lies at home as wreckage; the rounds are over, or a
  round took no hull from anybody, and a defender still stands
  (`outlasted`): repelled, the raiders leave, nothing is taken and there
  is no wreckage; no defender is left (`fleets_destroyed`, or `fled` when
  your last fleet jumped out): lost. A raid that finds nothing at all in
  its way is lost on the spot, without a fight.
  **After it.** Of the defences that fell, and of the Solar Satellites,
  about 70 in a hundred stand again at once and for nothing, whether the
  raid was repelled or lost (fewer than ten of a kind: each on its own 70
  in a hundred; ten or more: 60 to 80 percent of them; the satellites are
  a kind of their own). Ships lost are lost. A lost raid also takes a
  hard-capped slice of
  the stockpile above the share the storage buildings protect (8% of metal
  and crystal, 5% of deuterium), and none comes for 48 game hours divided
  by P^0.75. What destroyed ships carried goes to the stockpile as far as
  the stores have room, before the raid takes its slice, so a hold at the
  dock hides nothing; the rest is lost. Never buildings, never queues.
  **Your fleets.** A fleet standing at home when the raid arrives is in
  the fight: it cannot load or unload, the dock does not mend it until it
  is over, and research finished meanwhile reaches it afterwards. It may
  leave before the raid arrives with an ordinary `move`, and during the
  fight by the same `move`, which is flight (`flight` on its row says what
  it costs): it stops firing, is fired on while its jump charges, and
  nothing follows it. A fleet that arrives home during the fight docks
  and stands in it from that tick's round; a ship, satellite or defence
  the yard finishes stands in it from the next. A fleet under standing
  orders leaves when the fight's odds fall under its disengage ratio.
  `RaidResolved` is the summary: `defended`, `outcome`, `rounds`,
  `raiders` and `raiders_lost`, `lost` (everything you lost, by unit, the
  satellites and the defences before the rebuild), `rebuilt` (the
  satellites and the defences that stand again, by unit), `ships_lost`,
  `satellites_lost` (those that fell, before the rebuild),
  `structures_lost`, `structures_restored` (the defences alone),
  `resources_lost`, `protected_kept`, and for a destroyed raid `salvage_dropped` and
  `salvage_despawn_tick`. `defended: true` with something in `lost` is a
  raid repelled at a cost. The wreckage is on your own planet, so
  a fleet docked at home scoops it into storage by itself, up to what the
  stores hold (a `Stockpile` event with `cause: "wreckage"` says what); with
  no fleet docked, or the stores full, an `Alert` fires once when 60 s are
  left.
- **What enters or leaves the stockpile** — besides production and what is
  paid for orders, repairs and fuel, the stockpile changes only when a hold
  is emptied into it at the home dock (a fleet docked with cargo, an
  `unload`, or the cargo of ships a fleet lost there), when `load` fills a
  hold from it, when a docked fleet scoops wreckage on the homeworld, or
  when a fleet delivers charts. Each is a `Stockpile` event (`fleet_id`,
  `cause`: `unloaded`, `loaded`, `wreckage`, `charts`, and the `amount`), shown in the web client's log and alerts
  and in `play`'s events in the words of `StockpileCause::phrase`.
- **The hub and its exchange** — the sector at the centre, `[0,0]`, is a
  neutral post: empty space with no ore, no hostile group and no derelict,
  and no hostile group wanders in. `full_state.world.hub` lists it with what
  it offers (`"services":["exchange"]`). A fleet standing there trades what
  is in its hold, one resource for another (`exchange`; ask first with
  `exchange_quote`, from anywhere). What comes back goes into the same hold
  and unloads at home like any cargo. The exchange always keeps a fee, so a
  trade never raises what you own and never touches score: it is a way to
  turn a surplus into what you are short of, at a loss.
  - Par values are the score weights: metal 1, crystal 1.5, deuterium 2 a
    unit. Selling counts in weight (units x par).
  - You have a meter (`player.exchange`): the weight you sold and that has
    not been forgotten yet (`pressure`). The fee on a unit of weight is 25%
    of par while the meter is empty and rises in a straight line to 75% when
    it reads `span`, and stays there beyond. A trade pays the fee of every
    unit it moves the meter through, so one large order and the same order
    in pieces return the same; `receive` is rounded down to whole units.
  - `span` is 15% of what your homeworld makes in one window with all the
    energy it needs, in weight, so it grows with your mines; the metal and
    crystal that come with no mine count, the Fusion Reactor's burn does not.
  - The meter falls by one `span` every `window_ticks` (86400 ticks at pace
    1, divided by P^0.75: about 4.3 h at season, 12 min at blitz), online or
    not. `clear_in` is the ticks until it is empty.
  - Example, season pace, mines 10/9/8 (`span` 11903): 632 metal
    is 632 weight, worth 316 deuterium at par. From an empty meter the fee
    runs from 25% to 28% across the trade, 26% on the whole: 232 deuterium.
    The meter then reads 632 and is empty again in 14 minutes. The same 632
    sold with the meter above 11903 would bring 79.
  - What the exchange takes is what a hold carries: field cargo (ore,
    wreckage, derelict loot, cargo a full store would not accept) or a
    surplus loaded from the stockpile at home.
  - The four numbers (25%, 75%, 15%, the window) are provisional.
  - Standing orders do not use the exchange, and the built-in bots do not
    either.
- **Loading a hold** — `load` moves stockpile into the hold of a fleet
  standing idle at your homeworld, one resource an order: an exact `amount`,
  or with none all that the stockpile holds and the hold has room for. An
  order that cannot be met in full moves nothing and says what is short, the
  stockpile or the room.
  - A loaded fleet is `laden` (`fleets[].laden`): the dock, which empties
    every other hold each tick, leaves this one alone. The mark ends with
    the fleet's first jump, so what it brings home unloads as cargo always
    has. To put a load back without leaving, `unload`: what the caps allow
    goes back at once, the rest as room opens.
  - A laden fleet is an ordinary fleet: same fuel, same fights, and what
    the ships it loses carried is shed like any cargo (see Combat and
    Raids). Split it and both halves stay laden; merge and
    the fleet that absorbs keeps its own state.
  - `load` is refused for a fleet under standing orders, which unload at the
    dock. Giving a laden fleet standing orders ends the mark and its hold
    goes back to the stockpile. The built-in bots do not load.
  - A hold is storage of a kind: a fleet at home holds what it can carry on
    top of the stores. A Small Cargo costs 4,000 resources and
    holds 5,000 x sqrt(P), but it is no hiding place: a lost raid destroys
    the ships that stayed, and what they carried is in the stockpile when
    the raid takes its slice (see Raids). Loading moves no score.
  - The trip: `load` the resource that is at its cap, fly to `[0,0]`,
    `exchange`, fly home; the dock unloads what you bought.
- **Refining** — `refine` turns the deuterium in a fleet's hold into fuel, on
  the spot and at once, until the tank is full or the deuterium is gone. A
  dock makes 12.5 fuel of a unit of deuterium (0.08 a fuel unit); the hold
  makes 15% less, 10.625 (`REFINE_LOSS`, provisional).
  - Any deuterium aboard counts: mined, salvaged, bought at the exchange or
    loaded at a dock. A cargo ship loaded with deuterium at home is a tanker for
    the fleet it flies in.
  - `fleets[].refine` (`deuterium`, `fuel`) is what the order would do right
    now. It is absent when the order would be refused: in combat, while a
    jump charges, with no deuterium aboard, or with a full tank. A harvest
    or a boarding goes on beside it.
  - A dock fills the tank at the full rate, and a full tank refuses the
    order, so refining is for the field: a fleet that mines or salvages
    deuterium stays out on it, and one too dry to get home flies back.
  - Standing orders refine only when told to (`refine: true` in the
    `policy_update` params, off by default): then a fleet that would turn
    home for fuel refines what deuterium its hold has first and flies on if
    that was enough. The order is also taken by hand for a fleet under
    orders. Neither is charged to its fuel budget. The built-in bots do not
    refine and do not set the switch.
- **Outposts** — one station per empire, on a sector of rings 9 to 20, built
  from what a fleet carries to it: `load` the cost at home, fly out, win the
  sector if a pack lives there, `found_outpost`. `outpost_survey` says
  beforehand what a site is worth. The rules in full are in
  `tools/playtest/RULES.md`; the design is `docs/design/outposts.md`.
  - **Yield**: the extractor makes, per resource and game hour,
    `6 x density x ring factor x level` times P, from the ore the sector was
    made with (Sparse 0.5, Moderate 1, Rich 2, Pristine 4; ring factor 1.77
    at ring 9, 5.05 at ring 20). It does not empty the tiles.
  - **Levels**: 3000/1500/600, then 7500/3750/1500 and 18750/9375/3750, taking
    15, 37.5 and 93.75 game hours divided by P. Level 1 is paid from the
    founding hold, the others from the outpost's own store
    (`upgrade_outpost`).
  - **Store and dock**: a store of its own (9000/4500/2250 x 2.5 a level x
    sqrt(P)) that `unload` fills and `load` empties there. Nothing in it
    scores or can be spent at home until a fleet has carried it home. A
    fleet flown by hand is refuelled from the store's deuterium and has its
    damaged ships mended from the store at a tenth of their cost (Damage
    and repair, above), any fleet arriving banks its charts, and a fleet
    standing there is not deployed.
  - **Raids**: pirates of its ring raid it on the homeworld's raid clock, at
    `2 x` the ring's group power, in the class the packs of that ring fly
    (Cruisers to ring 12, Battlecruisers beyond). It is the fight of a raid
    on a homeworld: a round a tick for at most 6, the raiders against the
    ships standing there and the platform, which is fixed defences at the
    catalogue's figures (4 Rocket Launchers; 8 with 4 Light Lasers; 16 with
    8 and 4 Heavy Lasers) and stands whole again after every fight that is
    not lost. Alone, level 1 holds no ring, level 2 a typical raid to about
    ring 16 and level 3 every ring. A destroyed raid leaves no wreckage
    there. A lost raid has destroyed every ship that stayed, takes a
    quarter of the store above what the next repair would
    take, and breaches the outpost: no extractor, no fuel, no platform
    until it has repaired itself from its store. A repair is priced in
    metal (1612.5, 4031.25, 10078.125 by level) and paid in metal, then
    crystal at 1.5 a unit, then deuterium at 2, so a store that holds the
    price in anything pays. No raid destroys it.
  - **State**: `outpost` in `full_state` and every `tick_update` (with
    `raid_power`, the weakest and the strongest raid of its ring; `platform`
    and `raid`, the defences it stands with and the forecast of the raid
    being fought, on its way or typical of the ring, with its ships and
    its odds;
    `upgrade_refusal`, why an upgrade would be refused now; `yield_lost` and
    `repair_waiting_for`), `at_outpost` and `outpost_fuel` on each fleet,
    `outpost` on a sector that holds one (its `threat` is rated 0), the
    `OutpostCompleted` event, and `outpost` on `RaidIncoming` and
    `RaidResolved`. Each finished level adds its cost to the score.
  - Standing orders, bots and `recall` do not use the outpost yet.
- **Score** — one ranking for humans and AI agents (`leaderboard`). Your
  score is what your empire has built and keeps: `buildings` + `research` +
  `fleet` + `outpost` + `defence`, each part the cost of what stands, at
  cost x (1 metal, 1.5 crystal, 2 deuterium) / 1000. `fleet` is the ships
  you have now, docked or away, Solar Satellites among them, so a fleet
  destroyed leaves the score; `outpost` is the finished levels of your
  outpost while it stands; `defence` is 0, as defences score nothing and pay
  for themselves in the wreckage of the raids they beat (Raids). Kills,
  charts and derelicts score nothing themselves: they pay in resources (a
  kill's wreckage, a delivered chart, a derelict's loot), which score once
  you build with them. Unspent stock scores nothing. Cancelling a build or
  losing a ship scores nothing; resources received from others never
  count. Accounts that log in with `"agent": true` (the headless client
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

# The engine must build for the browser: iac-solo is that build
cargo build -p iac-solo --target wasm32-unknown-unknown --release
```

Crates: `iac-shared` (`shared/`) (protocol, constants, scaling, hex math, worldgen) ·
`iac-sim` (`sim/`) (every game rule and all world state: no I/O, no async, no clock; time is ticks, every random draw is seeded) ·
`iac-server` (network, the world's file, tick timer around `iac-sim`) · `iac-solo` (`solo/`) (the
single-player host: a session, a clock fed by its caller and the catch-up after an
absence, behind one function of bytes; three exports as WebAssembly, no
bindings generator) · `iac-headless` (`clients/headless/`) (the NDJSON pipe agents use, and the connection, login and token file as a library).

`iac-sim` talks to its host through three seams: `GameEngine::execute` (a protocol
`Command` in, reply or refusal out), `views::{full_state, tick_update}` (what a player
sees), and the snapshot. `snapshot()` / `GameEngine::restore()` copy out and rebuild
the whole world as serde data, and `Snapshot::hash()` is a digest of it. The engine
knows nothing of storage: a host stores the snapshot whole, as the one document
`iac_sim::store` defines (a header line with the versions and the host's own part,
then the snapshot as JSON), in a place that holds bytes (`Store`: a file for the
server, IndexedDB behind the worker in the browser), when `store::Due` says one is
owed. What must survive a restart is a field of `Snapshot` and is written nowhere
else. Same seed, pace and command script give the same hash, on a server and in a
browser alike: powers, logarithms and exponentials come from `iac_shared::maths`
(the `libm` crate, one implementation for every target) and never from the
platform's maths library, which clippy refuses in the engine crates.

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
and enters sectors on the server's ratio and keeps no arithmetic of its own. It leaves
a fight whose odds as it stands (`fleets[].flight.ratio`) fall below RISKY, runs for
home with a group on its trail, and takes a wing without a warship into no fight at
all: cargo ships beat a Battlecruiser by the odds and lose twenty of themselves doing
it. At home a bot reads the forecast (`homeworld.raid`): its planner orders the
defence that answers what the raid flies (the most power for its cost, less what
the raid's rapid fire takes of it), a dome, and the research the better defences
need, and goes on while the forecast is short of `repelled`; its pilot steps a
fleet aside to the next sector when a raid on its way is `at_risk` or `lost`, and
waits there until it is over. A world's
bots are accounts with a `BotRecord` (style and the little a bot remembers) kept in
the engine, in every snapshot and in storage; `bot::Roster` is what a host holds
beside the engine: `resume` the bots a world has, `enlist` the ones it should have,
`take_turns` before `tick`, `observe` after. The server does exactly that (`--bot`),
and a browser host would do the same.

`sim-run` seats a mix and prints, for every player and sample interval, the score
split (buildings, research, fleet, outpost, what the defences cost, total) and
what the field paid, then one row per style with what the field, the raids and the
charts paid, ships lost, the fights its fleets jumped out of under fire, the
deuterium paid for refuelling, the share of production a full store turned away,
the hours fleets spent away from home, and the share of its fights begun against a
pirate group that had fought before and had not mended. A table of the raids on homeworlds follows, a row a
player and one for all: how many came, how many were destroyed, outlasted, lost
and met by nothing; the rounds fought; the ships, Solar Satellites and defences
they destroyed and the satellites and defences rebuilt; what did not come back at cost weight,
what was taken from stockpiles and the wreckage scooped. A last table says why each building slot, the lab and
the yard stood empty: the engine's reason where an order waited (resources or a
prerequisite), and where none did, what the bot's own planner would have done
with the place (`ScriptedPlayer::outlook`). A last one counts flight and pursuit by
the kind of fleet (its most numerous class and the ticks one jump takes it). `--json`
adds every order that paid and when.

```sh
sim-run --pace season --days 7                      # every style once and an idle player
sim-run --seat explorer:all:2 --seat turtle:hands-on --sessions 8 --seed 3
sim-run --pace blitz --hours 1 --seat all:hands-on --json
sim-run --pace season --days 7 --defence-share 0.5  # the week scored with defences at half
```

`--seat` is `<personality>:<attention>[:<count>]` or `idle[:<count>]`, `all` standing
for every personality or attention; `--sessions` sets the look-ins per day;
`--defence-share <share>` scores the report with defences at that share of their
cost instead of the game's (none): no bot reads the score, so the world plays as it
would. A world has 24 home seats; `sim-run` refuses a roster of more.

No bot founds an outpost, so `--outpost <ring>:<grade>:<level>[:<repairs>]` stands
one for every bot at the start of day 3, by an edit of the world's snapshot: on the
free sector of that ring whose ore is nearest that grade, three hexes from the next,
its store stocked for that many repairs (one unless said). `<level>+` has the next
level under construction. Nobody supplies it, defends it or hauls from it, so the
report's outpost table measures the extractor, the raids and the store: the site
and what level 1 there pays back in against its owner's next Metal Mine level, its
rate against its owner's mines, what it could have made, made, lost to a full store,
to raids and to repairs, what `load` took, its raids repelled and lost (those that
found it breached counted apart), and the share of its time it stood breached,
waiting for materials it could not pay, or waiting for a level to finish building.

```sh
sim-run --pace season --days 7 --outpost 14:5:3                              # level 3 on a grade-5 site in ring 14
sim-run --pace season --days 7 --seat turtle:check-in:6 --outpost 20:7:3:0   # ring 20, an empty store
sim-run --pace season --days 7 --seat turtle:check-in:6 --outpost 16:5:2+    # level 2 with level 3 building
```

## Other versions

Earlier implementations are kept on archive branches (each also tagged):

| Branch / tag | What it is |
|---|---|
| `archive/zig` / `archive-zig-final` | Original Zig implementation (zithril TUI, zqlite, webzocket) with the full design history; also holds the old `clients/flutter` client |
| `archive/dart-fullstack` / `archive-dart-fullstack` | Full-stack Dart: Flutter web UI, Dart game server and shared protocol package (orphan branch) |
| `archive/numen-scout` / `archive-numen-scout` | Small slice ("Amber Scout") written in the Numen language (orphan branch) |

The Zig-era design notes, milestones and tech stack live in `README.md` on `archive/zig`; the current spec is `SPEC.md` plus `docs/`.
