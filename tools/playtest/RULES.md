# In Amber Clad: live playtest rules

You are one of six AI players in a live, shared game of In Amber Clad, a
tick-based space strategy MMO (OGame x The Infinite Black). The server ticks
once per second, so the world keeps moving while you think. Other players
(other AIs, and maybe a human on the web UI) share the same hex galaxy.

The session lasts about one hour. Every `play` call prints the minutes left.
The world runs at a fixed pace (`./play status` shows it): a one-hour session
normally runs at x600, where a mine upgrade takes seconds to minutes, a
competent player builds a first Cruiser after about 54 minutes and the endgame
is about two and a half hours of play away. Check `./play costs` for real
times rather than guessing. Play the whole hour: keep queues busy (`state`
gives each queue's `slots` running at once and `depth`, the most it holds;
waiting buildings and research cost nothing until they start), keep fleets
doing something, spend before a stockpile reaches its cap, and use
`play wait --until idle` while timers run instead of stopping. Do not end your session
early. When under 6 minutes remain, write your feedback (see the end).

Your goal: finish top of the leaderboard (`./play score`) by the end of the
hour: build the biggest economy, fleet and defence you can, while exploring and
surviving. Hunting pirates and exploring add to the score, but at most 25% on
top of what your buildings, research, ships and defences are worth. Your choice of
strategy; take risks if you like. Be a good sport: no attempts to break the
server, no spamming it (the tool spaces the orders of a batch for you). Other players
run on the same machine: never kill, signal or inspect processes (no `kill`,
`pkill`, `ps`); interact with the game only through `./play`.

## Your tool: `play` (run it from your player directory)

```
./play state             ONE compact JSON line, instant: stock, caps, income/s, levels, queues (running, waiting, shortfalls,
                         and a note when a free slot starts nothing), idle queues, next costs (with what is short), fleets,
                         score (rank, bonus room, the leader and the gap), minutes left, alerts. Read this every loop.
                         `--keys stock,queues,score` prints only the top-level keys named.
./play status [--brief]  the same for reading: score/rank/time/idle queues/scoring rule first, then queues with their waiting items
./play costs [what]      next 3 levels of every upgrade: cost, time, what each unlocks, mine payback (buildings|research|ships|defences)
./play score [rows]      the leaderboard (humans and AI agents share one table; built-in bots are listed under it, unranked)
./play map [radius]      every sector you have seen near your fleets, marked LIVE or STALE <age>, with threat T1-T9 and the odds of your strongest fleet
./play sector Q R        one sector in detail (LIVE / STALE marking, threat and odds, ore reserve and refill time)
./play preview F [Q R..] fuel, threat and odds for fleet F jumping to adjacent sectors; with no Q R, every exit
./play odds F Q R [Q R..] the odds of fleet F in any sectors, near or far, seen or not (the server works them out; see Threat)
./play do '<json>' ['<json>' ...]  send commands in order (or one JSON array); one result line each, the server's answer
                         to that very order; stops at the first order refused (--keep-going sends the rest anyway)
./play events [--mine]   events since you last looked, collapsed (scans become one line, combat rounds one line per fight)
./play wait [secs] [--until idle|event|done]  sleep up to 45 s (max 60), return early when something needs you
./play rules             this file
```

Playing fast. The server handles a command at its next tick (about once a second),
and a mine at this pace finishes in seconds, so do not poll in a loop of single
commands. The loop that works:

1. `./play state` (instant). Look at `idle` (queues with a free slot and nothing waiting), `queues.*.room`
   (items you can still add; `slots` and `depth` are the live numbers for your Computer Technology),
   `queues.*.note` (why a free slot starts nothing: a `reserve` holding it, or every waiting order short, with
   the time to cover the first) and `next.*.<name>.ok` (unlocked and payable now; when it is not, `short` says
   what is missing by resource and `covered_in_s` how long your income takes to cover it).
2. Send every order you want in ONE call: `./play do '{"action":"build","building_type":"MetalMine"}'
   '{"action":"research","tech":"EnergyTechnology"}' '{"action":"build_ship","ship_class":"LightFighter","count":1}'`.
   You get one line per command (`ok ... -> started, done in 3s`, `queued [0] waiting for 35 metal`, or `ERR ...`) and
   a `queues:` line. Each line is the server's answer to that order: the tool sends a question behind every order
   and the server answers in order, so a refusal is never put on the wrong line. The batch stops at the first
   order refused and lists what it did not send (`NOT SENT ...`), so a `load` refused does not send the `move`
   after it with an empty hold; `--keep-going` sends every order anyway. An order takes up to a tick (about a
   second) to be answered, so a batch of five takes a few seconds; `--keep-going` answers them all in one tick.
3. `./play wait --until idle` returns as soon as a queue frees up or becomes payable (an alert returns it too),
   so a 3-second build does not cost you a 45-second sleep. `wait` never sleeps longer than 60 s, so it will not
   hit your shell's timeout; call it again if nothing happened. `--until event` also returns on completions,
   arrivals and fights; `--until done` returns when all queues and fleets have finished.

Scoring is shown at the top of `status`: unspent stockpile scores nothing, and combat plus exploration together
add at most 25% of core. `state.score` is refreshed every few seconds. Its `bonus` is the server's figures for you:
`banked` (combat plus explore earned), `cap` (what they may add now), `room` (cap less banked: points that still
score, below zero by as many as you are over) and `core_to_count` (the core at which everything banked counts, past
which another chart or kill scores again). `leader` is first place's name and score and the `gap` to you. The
`Standing:` line of `status` says both in words.

Moving: `move` targets must be a sector listed in your fleet's `exits` (the
direction labels E/NE/NW/W/SW/SE are shown). Each jump costs fuel and puts the
fleet on cooldown. `scan` reveals nearby sectors so you can plan routes.

Keep short notes in `notes.md` in your directory (plans, what worked), so you
can pick up where you left off if your session is restarted.

## Valid names (case-sensitive in JSON)

- BuildingType: MetalMine, CrystalMine, DeuteriumSynthesizer, SolarPlant, FusionReactor, RoboticsFactory, NaniteFactory, Shipyard, MetalStorage, CrystalStorage, DeuteriumTank, ResearchLab
- ResearchType: EspionageTechnology, ComputerTechnology, WeaponsTechnology, ShieldingTechnology, ArmourTechnology, EnergyTechnology, HyperspaceTechnology, CombustionDrive, ImpulseDrive, HyperspaceDrive, LaserTechnology, IonTechnology, PlasmaTechnology, GravitonTechnology
- ShipClass: SmallCargo, LargeCargo, LightFighter, HeavyFighter, Cruiser, Battleship, Recycler, EspionageProbe, Bomber, SolarSatellite, Destroyer, Deathstar, Battlecruiser
- DefenceKind: RocketLauncher, LightLaser, HeavyLaser, GaussCannon, IonCannon, PlasmaTurret, SmallShieldDome, LargeShieldDome
- HarvestResource: Metal, Crystal, Deuterium, Auto
- PolicyPreset: manual, prospect, mine_and_return, salvage_and_sites, patrol_home
- QueueType: Building, Ship, Research
- FleetStatus: Idle, Moving, Harvesting, InCombat, Returning, Docked, Exploring
- TerrainType: Empty, AsteroidField, Nebula, DebrisField, Anomaly
- Density: None, Sparse, Moderate, Rich, Pristine
- NpcBehavior: Passive, Patrol, Aggressive, Swarm
- SiteRisk: quiet, uneasy, hot
- OutpostWork: build, repair

### Command reference (stdin)

| action | fields | effect |
|--------|--------|--------|
| `move` | `fleet_id`, `target:{q,r}` | jump to a connected sector (fuel, cooldown). The fuel is paid when the jump is ordered, and a fleet that is charging a jump takes no other (`OnCooldown`; `stop` calls it off). From inside a fight it is flight: the fleet stops firing and is fired on every round until the jump lands, a round fewer than its `jump_ticks` and never fewer than one, and a group that was beating it may follow (see Combat). The fleet row's `flight` quotes it while the fleet is in a fight (`fuel`, `ticks`, `rounds`, the odds of the fight as it stands, `refusal`, and `chase`, the group that would follow). Refused with `InsufficientFuel` and a sentence when the tank does not hold the jump. A fight that begins in the sector while a jump is charging does not stop the jump: the fleet is in the fight as one that is leaving, fired on until it lands, which is told in an `Alert` (warning) |
| `scan` | `fleet_id` | reveal nearby sectors + faint signal contacts; a fleet with an Espionage Probe scans farther |
| `harvest` | `fleet_id`, `resource` (`Metal`, `Crystal`, `Deuterium`, `Auto`) | mine the current sector until cargo is full; `Auto` takes everything present, a named resource only that one (`ResourceNotPresent` if the sector lacks it). When the harvest ends by itself an `Alert` (warning) says why, once: the hold is full, or nothing (or none of the named resource) is left to mine there. A fleet under standing orders reports through its doctrine instead |
| `attack` | `fleet_id`, `target_fleet_id` | engage the NPC fleet listed as HOSTILE in your sector (use its id) |
| `collect_salvage` | `fleet_id` | scoop wreckage (partial if the hold fills; `CargoFull` if it is already full) |
| `recall` | `fleet_id` | emergency-jump home from another sector (2× fuel, hull risk); a jump the fleet was charging is abandoned with its fuel. The answer is an `Alert` with what the jump cost: the fuel, a jump abandoned, how many ships took hull damage and how much (it lasts until a dock mends it, which the home dock does on arrival), how many broke up; a warning when a ship was harmed, info when none was. The ships that broke up are also a `ShipsLost` event, by class and count. Refused with `InvalidCommand` and nothing changed for a fleet at your homeworld: there is nothing to come back from, a jump it is charging there goes on, and `stop` is the order that halts it |
| `stop` | `fleet_id` | abort a move, harvest, or boarding. A jump called off does not return the fuel it burned, and backing out of a boarding leaves the site on alert, so its next boarding is likelier to be ambushed: either is told in an `Alert` (warning), the first with the fuel forfeit. A fleet that calls off its jump out of a fight fights on. Halting a harvest costs nothing and says nothing |
| `explore_site` | `fleet_id` | board the derelict in the current sector (20t, ambush risk) |
| `build` | `building_type`, `reserve` (default false) | upgrade a homeworld building one level: starts at once when a building slot is free and you can pay, otherwise waits in the queue (nothing is charged until it starts). Every order gets a stable `id`, reported in its `Queue` event and in the queue lists. `reserve: true` makes the order hold the line while it is short (see Queues and slots) |
| `research` | `tech`, `reserve` | research one level, same rules (one lab) |
| `build_ship` | `ship_class`, `count` | order a ship batch (one shipyard). Unlike a building or a tech, a batch is accepted only if your stockpile covers every unit now, and it is paid in full at that moment; otherwise it is refused with `InsufficientResources` and a message naming the shortfall per resource and how many units the stockpile would cover (5 ordered with money for 2 is refused; order 2). A paid batch starts at once or waits for the yard |
| `build_defence` | `kind` (see Valid names), `count` | order home defence structures in the shipyard queue, paid in full when ordered like ships; `DefenceLocked` until the Shipyard level and research it needs are in place, `MaxLevelReached` for a second dome of a kind |
| `cancel_build` | `queue_type`, `id` | cancel the item with that `id` once it has started. A building or tech refunds 50% of its cost, and waiting items of the same building or tech move up a level. A ship or defence batch keeps the units already built and refunds 50% of the price of every unit not yet delivered, the one under way included. A `Queue` event confirms what was refunded. `InvalidTarget` names where the id actually is (another queue, still waiting, or gone) |
| `cancel_queued` | `queue_type`, `id` | remove the item with that `id` while it still waits. A waiting building or tech paid nothing, so nothing comes back; a waiting batch paid when it was ordered and gets 50% back. A `Queue` event confirms it. Refused, not redirected, when the id is not waiting in the named queue |
| `preview_move` | `fleet_id`, `target:{q,r}` | do not move: reply `preview_move` with `fuel_cost`, `fuel_after`, `can_jump`, `hops_home` (shortest real route home, charted or not), `charted_hops_home` (shortest route over sectors you have entered, null when there is none), `route_unexplored` (the `hops_home` route crosses sectors you have never entered), `fuel_to_return`, `can_return`, plus `threat` (`rating`, `est_power`, `basis`), `fleet_power`, and the odds `ratio` and `label` (`safe`, `favourable`, `even`, `risky`, `deadly`), all for the fleet you name, not your strongest. `ratio` is not `fleet_power / est_power`: see Threat. For a fleet in a fight the jump is flight and the reply also carries `flight`, the quote of the fleet row's `flight` for this `target`: its `chase` is absent when no group would follow there (home ground, the hub and a sector an outpost stands in are refuges) |
| `odds` | `fleet_id`, `sectors:[{q,r},...]` (at most 256) | do not move: reply `odds` with `fleet_id` and, for each sector in the order asked, `{sector, threat, ratio, label}`: how that fleet would fare there. Any sector may be named: adjacent or far, the one the fleet is in, seen or never seen |
| `leaderboard` | `limit` (default 20) | reply `leaderboard`: `entries` of `{rank,name,agent,score,core,combat,explore}`, plus `you` when you are below the limit, plus `bots` (same rows, ranked among themselves) when the world has built-in bots, plus `bonus` (`banked`, `cap`, `room`, `core_to_count`: your combat and exploration points against what they may add) |
| `split` | `fleet_id`, `ships:[{class,count},...]`, `damaged_first` (default false) | detach so many ships of each class named into a new fleet in the same sector; at home this launches docked ships. A ship has no id: a class is named once with a count, the whole ships of the class go first, or the damaged ones from the least hull up with `damaged_first: true`. At least one ship stays; fleet cap 8 in all, 3 away from home. The new fleet keeps the weapons, shields and armour the old one fights with, and its id arrives in an `Alert` event. Refused with `InvalidTarget` and a sentence naming the class at fault (a count of 0, a class named twice, more than the fleet has, every ship taken: "fleet 2 has 6 Light Fighter, not the 9 the split names"), `FleetLimitReached` at the caps |
| `merge` | `fleet_id`, `other_fleet_id` | fold the other fleet into `fleet_id` (same sector, both idle, at most 5000 ships together); ships, cargo and fuel pool, the other fleet's standing orders end. Where the two were refitted at different levels of Weapons, Shielding or Armour Technology the merged fleet fights at the lower of each until it docks at home, and an `Alert` says so |
| `exchange` | `fleet_id`, `give`, `want` (`metal`, `crystal`, `deuterium`, lower case), `amount` (optional) | sell `amount` of `give` from the fleet's hold to the hub's exchange and take `want` into the hold; without `amount`, all of `give` aboard. The fleet must stand idle on the hub sector (0,0). The answer is an `Exchanged` event (what was given and received, the fee, the meter before and after; see The hub and its exchange). Refused with nothing changed and a sentence saying why: `NotInSector` away from the hub, `OnCooldown` in transit, `NoResources` for more than the hold carries or none, `CargoFull` when what comes back would not fit, `InvalidCommand` for one resource named twice, an amount that is not positive, or a trade that returns less than one unit |
| `exchange_quote` | same as `exchange` | do not trade: reply `exchange_quote` with `amount`, `receive`, `par`, `fee`, `meter` and `meter_after`, exactly what `exchange` with the same fields would do (`./play do` prints it as one `quote ...` line). It answers from anywhere and for an amount the fleet does not carry; `refusal` is then the sentence the trade would be refused with right now |
| `load` | `fleet_id`, `resource` (`metal`, `crystal`, `deuterium`, lower case), `amount` (optional) | move `amount` of `resource` from your stockpile into the fleet's hold; without `amount`, all the stockpile holds and the hold has room for. One resource an order. The fleet must stand idle at your homeworld with no standing orders. It is then `laden`: the dock leaves its hold alone until the fleet has been away (see Loading a hold). The answer is an `Alert` (info) saying what went aboard. Refused with nothing moved and a sentence saying what is short: `NoResources` when the stockpile holds less than `amount` or none, `CargoFull` when the hold lacks the room, `NotInSector` away from home, `OnCooldown` in transit or busy, `InvalidCommand` for an amount that is not positive or a fleet under standing orders. At your outpost `load` takes from the outpost's store instead and does not make the fleet `laden`; see Outposts |
| `unload` | `fleet_id` | put a `laden` hold back: what the caps allow goes into the stockpile now and the dock takes the rest as room opens. The answer is an `Alert` (info). Refused with `InvalidCommand` when the fleet was not loaded by `load`, `NotInSector` away from home. At your outpost `unload` puts the whole hold of any fleet into the outpost's store as far as it has room (`NoResources` for an empty hold); see Outposts |
| `refine` | `fleet_id` | turn the deuterium in the fleet's hold into fuel, until the tank is full or the deuterium is gone. It works anywhere, takes no time, and loses 15%: a unit of deuterium is 12.5 fuel at a dock and 10.625 from the hold (see Refining). The fleet row's `refine` (`deuterium`, `fuel`) says beforehand what the order would turn into what, and is absent when it would be refused. The answer is an `Alert` (info) with both figures. Refused with nothing changed and a sentence saying why: `OnCooldown` in combat or while a jump charges, `NoResources` with no deuterium aboard, `InvalidCommand` with a full tank |
| `found_outpost` | `fleet_id` | found your outpost in the sector the fleet stands in, paying 1000 metal, 500 crystal and 200 deuterium from its hold. The fleet must stand idle with no standing orders; the sector must be in rings 9 to 20, not an Anomaly, without a derelict, without a group in it or away from it after a fleet (win the sector first: its group has to be destroyed), and at least 3 hexes from every other outpost; you may have one outpost. The answer is an `Alert` (info) with the tick it will be finished. Refused with nothing changed and a sentence saying why: `InvalidTarget` for a sector that does not qualify, `InvalidCommand` if you already have an outpost or the fleet is under standing orders, `NoResources` naming what the hold lacks, `OnCooldown` for a fleet that is moving, fighting or busy |
| `outpost_survey` | `fleet_id`, `sector:{q,r}` (optional) | do not found: reply `outpost_survey` for the named sector, or the fleet's own, with its `ring`, the `cost` and `build_ticks` of level 1, `raid_power` (`least` and `most`: the weakest and the strongest raid that ring sends), `platform` (the defences level 1 stands with) and `platform_power`, `raid` (a typical raid of that ring, its `ships`, and `odds` against a level 1 platform alone; absent outside rings 9 to 20), `site` (`densities`, `grade`, `production` a tick at level 1, `hops_home`) and `refusal`: the sentence `found_outpost` would be refused with right now, absent when it would be accepted. Any sector may be asked about. One you have never seen is answered without `site` and with a `refusal` that says it is not on your chart (or that its ring is outside 9 to 20): what follows from the ring is told, what is there is not. For a sector the fleet does not stand in, a `refusal` that only says the fleet is elsewhere means the site itself qualifies |
| `upgrade_outpost` | none | build the next level of your outpost, paid in full from the outpost's own store at that moment. The answer is an `Alert` (info). Refused with `InsufficientResources` and the exact shortfall when the store does not cover it, `MaxLevelReached` at level 3, `InvalidCommand` with no outpost, while it is building or repairing, or while it is breached. `outpost.upgrade_refusal` in your state is that refusal's sentence beforehand, absent when the order would be accepted |
| `abandon_outpost` | none | give up your outpost. Its store is left as wreckage in the sector for 180 ticks, its score is gone, nothing is refunded, and you may found another. `InvalidCommand` with no outpost |

Standing orders are a separate message type (not a command), and `./play do` sends them like any order:

```json
{"type":"policy_update","fleet_id":2,"preset":"salvage_and_sites",
 "params":{"min_fuel_pct":10,"cargo_return_pct":85,"max_range":4,"engage_ratio_x10":15,"disengage_ratio_x10":9,"fuel_budget_pct":20}}
```

`./play do` also takes `{"action":"policy_update",...}` and sends it as the message above. Sent straight to the
headless client in that form it is refused with a `client_error` that gives the form it takes.

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
counting the jumps over the real lanes, which wind: a sector four hexes away
may be six jumps. Turning home, it takes the way the fuel it carries flies (a
lane without a fight first, then one with), and says how many jumps home is,
by which sector, and the fuel against what it carries; when no way home is
within its fuel it flies the shortest, as near home as it can get, and says
so. Whenever a doctrine stands still it says why in a `PolicyAction` event
(`action: "hold"`), naming the limit that holds it: odds below the engage
ratio, the fuel there and back, or the range (with the tank named when the
tank, not `max_range`, sets it). A hold is told once, when it begins, and again
only when its reason changes; a fleet that went on holding for the same reason
says nothing more.

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
  that, so the ring estimate is the real strength of an unscouted lair. The
  Inner Ring is Light Fighter country: one or two of them to ring 8, then a
  Cruiser to ring 12, a Battlecruiser to ring 20 and a Destroyer beyond, each
  weakened or strengthened to its ring's power. The step in danger is at
  ring 9, where rapid fire starts: the two starting Light Fighters win every
  lair of ring 5 (keeping about nine tenths of their hull), nearly every one
  of ring 7 (58 of 60, keeping two thirds), two in three at ring 8 and none
  from ring 9; a third Light Fighter makes ring 8 a sure win. Every
  sector carries `threat`:
  `rating` T1 to T9 (one step per doubling of power over 40), `est_power` and
  `basis` (`observed`: a group is there, its own power, Aggressive and Swarm
  +25%; `template`: a lair that is away or cleared; `estimate`: ring only).
  A sector an outpost stands in is rated 0, power 0, `observed`: there is
  nothing to fight there and nothing comes.
  A stale chart entry keeps the rating last seen. Scans list the threat of
  every revealed sector (`threats`) and each faint contact has a
  `threat_band` (1: T1-3, 2: T4-6, 3: T7-9). Pirates return
  `2 + 0.25 x ring` game hours (divided by P) after a kill.
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
  along. Power against power is not it: four Light Fighters are 1.9x the
  power of the ring-9 Cruiser and rate 1.01x RISKY against it (33 fights of
  60 won, a quarter of the hull kept on average), because a Cruiser fires
  again five times in six at a Light Fighter; two Heavy Fighters, which it
  has no rapid fire against, rate 2.95x SAFE (92% kept). In OGame's round
  a ship at 70% of its hull or under may explode at every shot that lands,
  so a win costs ships and not only hull, and SAFE takes a fleet that ends
  the fight in a round or two: four Light Fighters at ring 7, two Heavy
  Fighters or one Cruiser at ring 9. A sector with nothing in sight is
  rated against the group its ring spawns, a remembered one against what
  was last seen there: a group seen wounded is rated as it was seen until
  the tick it was to be whole by (`whole_at`), and whole from then on; one
  seen passing through after a fleet is not counted there. A fleet in a
  fight is given the odds of that fight as it stands, both sides with what
  they have lost, in its row's `flight`. The dice of the odds are not the dice of the fight,
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
  and hull, for defences at once and for a fleet when it next docks at your
  homeworld: a fleet fights at the levels it was last refitted with
  (`fleets[].refit`: `weapons`, `shielding`, `armour`; `refit_due` while
  your research is ahead of it). A fleet docked at home is refitted the
  tick the research completes, a ship built joins a docked fleet at the
  current levels, a fleet split off keeps the levels of the one it left, a
  merged fleet takes the lower of each, and a wreck recovered in the field
  fights at the levels of the fleet that took it. Your outpost's dock mends
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
- **Fleets** — up to 3 away from your homeworld and your finished outpost (8
  in all), up to 5000 ships each, hex movement paid in
  fuel. A fleet is a count for each class and the hull of each ship that is
  not whole; a ship has no id. `fleets[].ships` is one row a class in
  catalogue order: `class`, `count`, `damaged` (how many are not whole),
  `at_risk` (how many of those stand at 70% of their hull or under, where a
  shot that lands may destroy them), `hull` and `hull_max` for the class
  together, and `unit_attack`, `unit_shield` and `unit_hull`, what one ship
  of the class fights with at the fleet's refit. The fleet's own
  `ship_count`, `hull`, `hull_max`, `attack` and `shield` are those summed.
  Docking at home unloads cargo, refuels, mends and refits but never merges
  fleets: use `split` and `merge`. New ships join an idle fleet at home that
  has no standing orders, or form their own. Each fleet reports its `power`
  (the scale raids use) and `range_hops`: how many more hops out it can fly
  and still get home.
- **Damage and repair** — a shield is full again every round and costs
  nothing; damage to a hull stays until a dock mends it. Your home dock
  mends a fleet when it docks and every tick it stands there, at once and
  for nothing. Your outpost's dock mends for a tenth of a ship's cost times
  the share of its hull restored, paid from the outpost's store in whatever
  it holds (metal first, then crystal at 1.5 and deuterium at 2, as its own
  repair is): whole ships only, the most damaged first, as far as the store
  pays, and not while it is breached or unfinished, nor for a fleet under
  standing orders. Either dock says what it did in an `Alert` ("fleet 2 was
  mended at the home dock: 2 ship(s) made whole, 67 hull restored, for
  nothing"; the outpost's names what it took from the store and how many
  ships still wait for its stock to cover them). `fleets[].repair`
  is there while a fleet carries damage: `ships`, `hull` (what is missing),
  and for `home` and for `outpost` (absent with no outpost) the `price`,
  the `payment` it would be taken as, the `ships` and `hull` it would mend
  now, and `refusal`, the sentence that says why it would mend nothing now.
- **Fuel and stranding** — each fleet reports `jump_fuel` (one jump),
  `hops_home` (the fewest hops home over the real lanes, charted or not)
  and `home_fuel` (the fuel for those hops) next to `fuel`, and `jump_ticks`: the
  ticks one jump takes it from the order to the arrival. A jump burns the
  sum of its ships' OGame fuel figures (20 for a Light Fighter, 300 for a
  Cruiser, 1000 for a Destroyer), less Energy Technology's saving, and takes
  `max(1, ceil(sqrt(50000 / speed)))` ticks at the speed of its slowest
  ship with the owner's drive research: 2 for Light Fighters and Cruisers, 3
  for a Heavy Fighter until Impulse Drive 2, 4 for a Small Cargo until
  Combustion Drive 2, 1 for Probes. Drive, fuel and tank research reach
  every fleet at once. Refuelling at the
  homeworld costs 0.08 deuterium per fuel unit (the Deuterium Tank adds +25% tank
  per level); with no deuterium the tank fills only as far as the stockpile
  pays and the rest follows as production arrives. It is paid ahead of every
  queue, except for a fleet under standing orders, which is held to its
  `fuel_budget_pct`. `preview_move` answers
  what a jump costs before you make it. A manual jump that leaves
  too little to get home still happens, but raises an `Alert` event
  (warning, or critical if the fleet will be stranded on arrival); it raises
  none while the fuel left still reaches your outpost in service, where the
  tank is filled (`outpost_fuel` in the fleet row is what that way costs). A fleet
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
  Light Fighters take one parting round, Small Cargos three, Recyclers
  four. The fleet row's `flight` quotes it beforehand (`fuel`, `ticks`,
  `rounds`, the `ratio` and `label` of the fight as it stands, a `refusal`
  sentence when the tank is short, and `chase`); its `fleeing` (`to`,
  `ticks`, `rounds`) says a fleet is leaving. `stop` calls the jump off,
  forfeits its fuel, and the fleet fights on. What flight costs is those
  rounds, the damage (it stays until a dock), the fuel, and everything the
  fight had earned: a group pays its wreckage and its score when it is
  destroyed and not before. `recall` is the other way out: at once, all the
  way home, for twice the fuel and a chance of harm to every ship.
  *Caught while charging.* The same holds for a fleet that ordered a jump
  before any fight: if a group attacks it in its sector while its jump
  charges, it is in the fight as a fleet that is leaving. It is fired on
  every round until the jump lands and does not fire back, and the jump is
  not stopped (`stop` calls it off, its fuel forfeit, and the fleet then
  fights). This is meant: a fleet that has paid for its jump has turned its
  back. One `Alert` (warning) says so when it happens, with the ticks until
  the jump lands. The home dock mends what it took when it lands at home.
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
  waiting there too. The fleet row's `pursued_by` (`group_id`, `label`,
  `from`, `arrives_in`, `catches`, `hexes_left`) and a `Pursuit` event
  (`phase`: `began`, `continued`, `abandoned` with `why`) say so; a group on
  its way shows its `errand` (`home`, `quarry`, `quarry_owner`, `to`,
  `arrives_at`, `hexes_left`) where it stands. Its own sector is empty
  meanwhile. When its leash ends or the fleet is out of reach it jumps back.
  *Pirates keep their losses and mend.* A group that is not destroyed stays
  as the fight left it (`hostiles[].ship_count` of `muster`, damaged ships
  with the hull they hold), and is found so by the next fleet. At rest in
  its own sector it mends in ten steps over half the time a destroyed group
  takes to return (1.4 to 3.5 game hours for rings 3 to 20, divided by the
  pace: 8 to 21 minutes in a season, seconds in a blitz): at each step a
  tenth of its ships return, as far as it is short, and every damaged ship
  regains a tenth of its hull. `hostiles[].whole_at` is the tick it will be
  whole by. A fight or a pursuit stops the clock, and the ticks rested
  toward the next step are lost. Killed after any number of visits it pays
  in full, as it was generated. So a group too strong to destroy in one
  fight can be worn down over several, if you come back before it has
  mended.
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
  the sector's pile, and `CombatEnded.salvage` the sum of the fight's kills;
  wreckage left there from before stacks with it, and then
  `CombatEnded.salvage_pile` is the whole pile, which is what
  `collect_salvage` takes and the sector view shows (absent when the fight's
  wreckage is all there is). The pile lasts 180 ticks from the last kill,
  its sector view carries `salvage_despawn_tick`, and a `SalvageDespawned`
  event says when one expires. `play` prints the fight's wreckage and, when
  it joined a pile, the pile. You only receive combat
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
  A sector you have entered carries `chart`: `aboard` while its chart rides
  with one of your fleets (it scores when that fleet docks at home alive, and
  is lost with the fleet), `delivered` once it is banked. A sector without
  `chart` still has its chart points to give.
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
  about ten times what they were. A kill pays `0.57 x group_power^0.8` units (55% metal, 30%
  crystal, 15% deuterium) times P^0.5, for the whole group; far hunting pays
  more but the odds fall faster than the pay rises. Derelict tier follows the
  threat (T1-3, T4-6, T7-9). A kill in a sector that has been killed in
  recently pays less: each recent kill multiplies the next one's loot and
  score by 0.75 (heat halves every 4 respawn delays), so a sector is not an
  endless tap. Holds grow with the pace like the loot: a ship's cargo is
  multiplied by P^0.5 (OGame's holds: 5,000 for a Small Cargo, 25,000 for a
  Large one, 20,000 for a Recycler, 50 for a Light Fighter; a blitz Small
  Cargo holds 122,000). A site pays `720 x 1.14^(ring-4)` units at 60 to
  140% times P^0.5; ambush odds `0.10 + 0.03 x (T-1)` (max 0.35); a recovered
  ship `0.05 x (T-2)` (max 0.30), a data core `0.03 x (T-5)` (max 0.15), and
  from T6 an ancient relic (2% per point above 5, max 8%, worth 300 explore
  points, no other use yet). A stripped derelict is replaced after 120 game hours / P^0.5.
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
  that cost (0.82 is about a fifth of what stands at home lost). `play`
  says "REPELLED and you LOSE about 18% of what stands there" for it.
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
- **What enters or leaves the stockpile** — besides production and what
  you pay for orders, repairs and fuel, the stockpile changes only when a
  hold is emptied into it at the home dock (a fleet docked with cargo, an
  `unload`, or the cargo of ships a fleet lost there), when `load` fills a
  hold from it, or when a docked fleet scoops wreckage on the homeworld.
  Each is a `Stockpile` event: `fleet_id`, `cause` (`unloaded`, `loaded`,
  `wreckage`) and the `amount`. So what an exchange put in a hold appears
  in the stockpile when the fleet docks, and `play` prints it as
  "STOCKPILE: fleet 84 unloaded into the stockpile: 4312/0/0".
- **The hub and its exchange** — the sector at the centre, 0,0, is a neutral
  post: empty space with no ore, no hostile group and no derelict, and no
  hostile group wanders in. `status` names it on the World line. A fleet
  standing there trades what is in its hold, one resource for another
  (`exchange`; ask first with `exchange_quote`, from anywhere). What comes
  back goes into the same hold and unloads at home like any cargo. The
  exchange always keeps a fee, so a trade never raises what you own and never
  touches score: it turns a surplus into what you are short of, at a loss.
  - Par values are the score weights: metal 1, crystal 1.5, deuterium 2 a
    unit. Selling counts in weight (units x par).
  - You have a meter (`status` prints it; `player.exchange` in the full
    state): the weight you sold and that has not been forgotten yet
    (`pressure`). The fee on a unit of weight is 25% of par while the meter
    is empty and rises in a straight line to 75% when it reads `span`, and
    stays there beyond. A trade pays the fee of every unit it moves the meter
    through, so one large order and the same order in pieces return the
    same; `receive` is rounded down to whole units.
  - `span` is 15% of what your homeworld makes in one window with all the
    energy it needs, in weight, so it grows with your mines; the metal and
    crystal that come with no mine count, the Fusion Reactor's burn does not.
  - The meter falls by one `span` every `window_ticks` (86400 ticks at pace
    1, divided by P^0.75), whether you are connected or not. `clear_in` is
    the ticks until it is empty.
  - Example, season pace, mines 10/9/8 (`span` 11903): 632 metal
    is worth 316 deuterium at par. From an empty meter the fee runs from 25%
    to 28% across the trade, 26% on the whole: 232 deuterium. The same 632
    sold with the meter above 11903 would bring 79.
  - What the exchange takes is what a hold carries: field cargo (ore,
    wreckage, derelict loot, cargo a full store would not accept) or a
    surplus loaded from the stockpile at home.
  - Standing orders do not use the exchange.
- **Loading a hold** — `load` moves stockpile into the hold of a fleet
  standing idle at your homeworld, one resource an order: an exact `amount`,
  or with none all that the stockpile holds and the hold has room for. An
  order that cannot be met in full moves nothing and says what is short, the
  stockpile or the room.
  - A loaded fleet is `laden` (in its fleet row): the dock, which empties
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
    goes back to the stockpile.
  - A hold is storage of a kind: a fleet at home holds what it can carry on
    top of the stores. A Small Cargo costs 4,000 resources and
    holds 5,000 x sqrt(P), but it is no hiding place: a lost raid destroys
    the ships that stayed, and what they carried is in the stockpile when
    the raid takes its slice (see Raids). Loading moves no score.
  - The trip: `load` the resource that is at its cap, fly to 0,0, `exchange`,
    fly home; the dock unloads what you bought.
- **Refining** — `refine` turns the deuterium in a fleet's hold into fuel, on
  the spot and at once, until the tank is full or the deuterium is gone. A
  dock makes 12.5 fuel of a unit of deuterium (0.08 a fuel unit); the hold
  makes 15% less, 10.625.
  - Any deuterium aboard counts: mined, salvaged, bought at the exchange or
    loaded at a dock. A cargo ship loaded with deuterium at home is a tanker for
    the fleet it flies in.
  - The fleet row's `refine` (`deuterium`, `fuel`) is what the order would
    do right now. It is absent when the order would be refused: in combat,
    while a jump charges, with no deuterium aboard, or with a full tank. A
    harvest or a boarding goes on beside it.
  - A dock fills the tank at the full rate, and a full tank refuses the
    order, so refining is for the field: a fleet that mines or salvages
    deuterium stays out on it, and one too dry to get home flies back.
  - Standing orders refine only when told to (`refine: true` in the
    `policy_update` params, off by default): then a fleet that would turn
    home for fuel refines what deuterium its hold has first and flies on if
    that was enough. The order is also taken by hand for a fleet under
    orders. Neither is charged to its fuel budget.
- **Outposts** — one station per empire, on a sector of rings 9 to 20, the
  space where PvP will be allowed. It is built from what you carry to it.
  `load` the founding cost at home, fly it out, clear the sector if a pack
  lives there, and `found_outpost`; `outpost_survey` tells you beforehand
  what the site is worth.
  - **The site** decides the yield. A sector's `grade` is the sum of its ore:
    Sparse 0.5, Moderate 1, Rich 2, Pristine 4, for metal, crystal and
    deuterium. The outpost's extractor makes, per resource and game hour,
    `6 x density x ring factor x level` times P; the ring factor is 1.10 per
    ring from ring 3 (1.77 at ring 9, 5.05 at ring 20). It reads the ore the
    sector was made with, not what harvesting has left this hour, and it
    does not empty the tiles. A site makes only what it has ore of: most
    make no deuterium.
  - **Levels**: 1 costs 3000/1500/600 and takes 15 game hours divided by P; 2
    costs 7500/3750/1500 (37.5 h); 3 costs 18750/9375/3750 (93.75 h). Level 1
    is paid from the founding fleet's hold, levels 2 and 3 from the outpost's
    store when you order them. The outpost works at its old level while it
    builds. While level 1 is being built the outpost has a store and nothing
    else, and no raid comes.
  - **The store** is its own, apart from your stockpile: 9000/4500/2250 at
    level 1, times 2.5 a level, times sqrt(P). The extractor stops at the
    cap, with one `Alert` (warning) per resource each time it fills. Nothing
    in the store scores, and nothing in it can be spent at home until a fleet
    has carried it there: `load` at the outpost, fly home, and the home dock
    unloads as always. At the outpost no hold is emptied by itself; `unload`
    does it.
  - **A dock**: a fleet without standing orders that stands at your outpost
    is refuelled from the outpost's deuterium at 0.08 a fuel unit, as far as
    it has any; a tank it cannot fill is reported once per stay, but not to
    the fleets standing there when level 1 opens (a new store is empty; a
    founder who wants a full tank unloads deuterium). Any of your
    fleets arriving at the finished outpost banks its charts. A fleet
    standing at the finished outpost does not count against your three
    deployed fleets, and needs a free slot to leave. New ships still appear
    at home. Its dock mends the ships of a fleet without standing orders
    from the store, at a tenth of their cost for the hull restored (see
    Damage and repair), every tick while it is in service; it does not
    refit them. A fleet at an outpost is not safe: see Raids below.
  - **Quiet**: while an outpost stands, no group lives in its sector or
    wanders in, and the sector reads as empty to `preview_move`, `odds` and
    its own `threat` (rating 0, power 0, ratio 10). The lair it displaced returns after its usual
    delay when the outpost is abandoned.
  - **Raids**: pirates of its ring raid it on the homeworld's raid clock,
    with the same warning, starting a day of that clock after level 1 is
    finished (`RaidIncoming` and `RaidResolved` carry `outpost`). A raid's
    power is `2 x` the ring's group power times 0.80 to 1.15: 314 to 451 at
    ring 9, 570 to 820 at ring 12, 1264 to 1816 at ring 16, 2799 to 4024 at ring
    20, whatever your empire is worth (`raid_power` in the survey and in
    your outpost's state gives the two ends; a Light Fighter is 91 of power,
    a Cruiser 675). It flies what the packs of its ring fly, one class:
    Cruisers at rings 9 to 12, Battlecruisers from 13 to 20; its
    `ships` are in the warning and in the forecast. It is fought as a raid
    on your homeworld is (Raids, above): a round a tick for at most 6, the
    raiders against your ships standing there and the platform, with the
    same three endings, the same `battles` entry (`raid.outpost` true) and
    the same rounds, the platform as a group marked `battery` (label
    "outpost platform"). The platform is fixed defences, at the catalogue's
    figures whatever your research: 4 Rocket Launchers at level 1; 8 with 4
    Light Lasers at level 2; 16 with 8 and 4 Heavy Lasers at level 3
    (`platform` in the survey, in your outpost's state and in its
    `next_level`). It stands whole again after every fight
    that is not lost: nothing of it is rolled for or paid for. A breached
    outpost has none. `outpost.raid` is the forecast, as `homeworld.raid`
    is: the raid being fought, the one on its way or a typical one of the
    ring, its ships, and the odds against the platform and the ships there
    now. Alone, level 1
    does not hold any ring, level 2 holds a typical raid to about ring 16,
    and level 3 holds every ring; a fleet standing there is what the
    difference is made of. Repelled (destroyed or outlasted): the outpost
    is as it was, your ships are as the fight left them, and there is no
    wreckage. Lost (no ship of yours left there and no platform): what your
    ships carried goes into the store as far as it has room; a quarter of
    the store above the protected floor is taken; and the outpost is
    breached: no extractor, no refuelling, no platform. The floor is what
    the next repair would take from what is there (`protected`), for the
    level it stands at or the level it is building. A fleet may leave
    before the raid or flee during it, as at home; when the last one jumps
    out the platform fights on alone. A raid on a breached outpost with no
    ship there finds nothing in its way and is lost without a fight. An
    outpost under raid cannot be abandoned. After a lost raid none comes
    for 48 game hours divided by P^0.75. No raid destroys an outpost.
  - **Repair**: a breached outpost repairs itself. A repair is priced in
    metal: 1612.5 at level 1, 4031.25 at level 2, 10078.125 at level 3 (a
    quarter of the level's cost at par). The store pays it in metal; where
    the metal runs out a unit of crystal pays for 1.5 and then a unit of
    deuterium for 2, so any store that holds the price in anything pays.
    It is whole after a quarter of that level's build time; an `Alert` says
    when it starts, with what it took, and when it ends. A breached outpost
    extracts nothing, so a store that holds less than the price never will
    by itself: the repair waits for a delivery of any resource and one
    `Alert` (warning) says how much metal is missing. A raid cannot take
    what the next repair needs, so a site that makes anything keeps
    repairing itself; only one breached before it has saved up its first
    repair waits. A breach suffered during an upgrade is repaired once the
    upgrade is done.
  - **Score**: each finished level adds its cost at score weight to `core`
    (2.15, 5.375, 13.44) while the outpost stands. It does not raise the
    raids on your homeworld.
  - **State**: `full_state.outpost` and every `tick_update.outpost` (absent
    when you have none): `location`, `level`, `max_level`, `in_service`,
    `breached`, `work` (`kind`, `level`, `start_tick`, `end_tick`), `store`,
    `store_cap`, `protected`, `production`, `yield_lost` (the resources the
    extractor makes and the store is full of, so that yield is lost),
    `grade`, `platform` (the defences it stands with, none while breached),
    `raid` (the forecast: `rounds_left` while one is fought, `arrival_tick`
    when one is on its way, `power`, `ships`, `odds`), `platform_power`, `ships_power` (your ships standing
    there), `defence_power` (the two together; three sums on the power
    scale that decide nothing), `raid_power` (`least`,
    `most`), `raid_arrival_tick`, `next_level` (what `upgrade_outpost` would
    cost and give), `upgrade_refusal` (why it would be refused now),
    `repair_cost` (what the repair takes from the store, what the store
    lacks counted as metal), `repair_waiting_for` (the metal the store lacks
    of it while a repair waits; crystal or deuterium to the same par value
    does as well), `hops_home`. Each fleet reports `at_outpost` (it is in
    your outpost's sector, finished, building or breached: `load` and
    `unload` use the store there) and `outpost_fuel`, the fuel the way there
    costs, while the outpost is in service. A sector view carries `outpost`
    (`owner`, `level`, `breached`, `mine`) when one stands there, yours or
    another empire's. `OutpostCompleted` says a level is finished.
  - Standing orders do not use the outpost: a fleet under orders measures
    from your homeworld and returns there, is not refuelled at the outpost
    and keeps its hold there; only its charts are banked if it arrives.
    `recall` still jumps to the homeworld, never to the outpost.
  - The trip: `load` at home what the outpost needs (deuterium for tanks,
    the next level's cost), fly out, `unload`, `load` what it has made, fly
    home.
- **Score** — one ranking for humans and AI agents (`leaderboard`).
  `core` = completed building and research levels + living ships and Solar
  Satellites + half of
  the defence structures + the finished levels of your outpost, each priced
  at cost x (1 metal, 1.5 crystal, 2
  deuterium) / 1000. Kills add the weight of the pile they leave (so points
  follow the finds factor and the sector's heat), in full up to 4x the NPC
  group's power and falling as 4/ratio beyond (half at 8x, a 5% floor, never
  zero), once per respawn. Exploration adds `0.5 x weight(loot) + 15 x T^1.5`
  for the first boarding of each derelict, 300 for a relic, and
  `1.5 + 1.0 x T^1.5` for each new sector whose chart you deliver to the
  homeworld or to your finished outpost (the fleet must arrive alive:
  `ChartDelivered`). `score = core + min(combat + explore, 0.25 x core)`. Cancelling
  a build or losing a ship scores nothing; resources received from others
  never count. Accounts that log in with `"agent": true` (the headless client
  does) carry the `agent` flag on the board, which labels and never scores.
- **Built-in bots** — a world may seat computer players. Their names start
  with `BOT-` (a prefix no one else can register), they play by your rules
  through the same commands, and you meet them wherever players are named
  (`player_fleets[].owner_name`, events). They are not in `entries`: the
  board lists them apart in `bots`, so your rank counts humans and AI agents
  only.


## Feedback (required, when under 6 minutes remain)

Write `feedback.md` in your player directory with these sections:

1. **Result**: final resources, buildings, research, ships; what you achieved.
2. **Strategy**: what you tried, what worked, what did not.
3. **Fun and clarity**: was the game understandable from these rules and the
   tool output? What confused you?
4. **Bugs and oddities**: anything that looked wrong (quote the event or error).
5. **Balance**: anything too strong, too weak, too slow, or pointless.
6. **Top 3 suggestions** to make the game better for AI and human players.
7. **The `play` tool**: what would have made it easier to play.

Be specific and honest; this feedback goes to the game's developer.
