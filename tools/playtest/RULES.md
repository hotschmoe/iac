# In Amber Clad: live playtest rules

You are one of six AI players in a live, shared game of In Amber Clad, a
tick-based space strategy MMO (OGame x The Infinite Black). The server ticks
once per second, so the world keeps moving while you think. Other players
(other AIs, and maybe a human on the web UI) share the same hex galaxy.

The session lasts about one hour. Every `play` call prints the minutes left.
The world runs at a fixed pace (`./play status` shows it): a one-hour session
normally runs at x600, where a mine upgrade takes seconds to minutes and the
whole tech tree takes about three hours of play. Check `./play costs` for real
times rather than guessing. Play the whole hour: keep queues busy (they hold
three items, and waiting items cost nothing until they start), keep fleets
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
./play state             ONE compact JSON line, instant: stock, caps, income/s, levels, queues (running, waiting, shortfalls),
                         idle queues, next costs, fleets, score and rank, minutes left, alerts. Read this every loop.
./play status [--brief]  the same for reading: score/rank/time/idle queues/scoring rule first, then queues with their waiting items
./play costs [what]      next 3 levels of every upgrade: cost, time, what each unlocks, mine payback (buildings|research|ships|defences)
./play score [rows]      the leaderboard (humans and AI agents share one table; built-in bots are listed under it, unranked)
./play map [radius]      every sector you have seen near your fleets, marked LIVE or STALE <age>, with threat T1-T9 and the odds of your strongest fleet
./play sector Q R        one sector in detail (LIVE / STALE marking, threat and odds, ore reserve and refill time)
./play preview F [Q R..] fuel, threat and odds for fleet F jumping to adjacent sectors; with no Q R, every exit
./play odds F Q R [Q R..] the odds of fleet F in any sectors, near or far, seen or not (the server works them out; see Threat)
./play do '<json>' ['<json>' ...]  send commands in order (or one JSON array); one result line each; returns at the server's next tick
./play events [--mine]   events since you last looked, collapsed (scans become one line, combat rounds one line per fight)
./play wait [secs] [--until idle|event|done]  sleep up to 45 s (max 60), return early when something needs you
./play rules             this file
```

Playing fast. The server handles a command at its next tick (about once a second),
and a mine at this pace finishes in seconds, so do not poll in a loop of single
commands. The loop that works:

1. `./play state` (instant). Look at `idle` (queues with a free slot and nothing waiting), `queues.*.room`
   (items you can still add; each queue holds 3) and `next.*.<name>.ok` (unlocked and payable now).
2. Send every order you want in ONE call: `./play do '{"action":"build","building_type":"MetalMine"}'
   '{"action":"research","tech":"Navigation"}' '{"action":"build_ship","ship_class":"Scout","count":1}'`.
   You get one line per command (`ok ... -> started, done in 3s`, `queued [0] waiting for 35 metal`, or `ERR ...`) and
   a `queues:` line. Orders are spaced about 0.25 s apart by the tool, so a batch stays polite.
3. `./play wait --until idle` returns as soon as a queue frees up or becomes payable (an alert returns it too),
   so a 3-second build does not cost you a 45-second sleep. `wait` never sleeps longer than 60 s, so it will not
   hit your shell's timeout; call it again if nothing happened. `--until event` also returns on completions,
   arrivals and fights; `--until done` returns when all queues and fleets have finished.

Scoring is shown at the top of `status`: unspent stockpile scores nothing, and combat plus exploration together
add at most 25% of core. `state.score` is refreshed every few seconds.

Moving: `move` targets must be a sector listed in your fleet's `exits` (the
direction labels E/NE/NW/W/SW/SE are shown). Each jump costs fuel and puts the
fleet on cooldown. `scan` reveals nearby sectors so you can plan routes.

Keep short notes in `notes.md` in your directory (plans, what worked), so you
can pick up where you left off if your session is restarted.

## Valid names (case-sensitive in JSON)

- BuildingType: MetalMine, CrystalMine, DeuteriumSynthesizer, Shipyard, ResearchLab, FuelDepot, SensorArray, DefenseGrid, StorageVault, Fabricator
- ResearchType: FuelEfficiency, ExtendedFuelTanks, ReinforcedHulls, AdvancedShields, WeaponsResearch, Navigation, HarvestingEfficiency, CorvetteTech, FrigateTech, CruiserTech, HaulerTech, EmergencyJump, ModularFabrication
- ShipClass: Scout, Corvette, Frigate, Cruiser, Hauler
- DefenceKind: PulseTurret, LancerBattery, IonBastion
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
| `move` | `fleet_id`, `target:{q,r}` | jump to a connected sector (fuel, cooldown). The fuel is paid when the jump is ordered. A fight that begins in the sector while the jump is still charging takes the fleet into it: the jump is abandoned and its fuel is not returned, which is told in an `Alert` (warning) with the fuel forfeit |
| `scan` | `fleet_id` | reveal nearby sectors + faint signal contacts; scouts scan farther |
| `harvest` | `fleet_id`, `resource` (`Metal`, `Crystal`, `Deuterium`, `Auto`) | mine the current sector until cargo is full; `Auto` takes everything present, a named resource only that one (`ResourceNotPresent` if the sector lacks it). When the harvest ends by itself an `Alert` (warning) says why, once: the hold is full, or nothing (or none of the named resource) is left to mine there. A fleet under standing orders reports through its doctrine instead |
| `attack` | `fleet_id`, `target_fleet_id` | engage the NPC fleet listed as HOSTILE in your sector (use its id) |
| `collect_salvage` | `fleet_id` | scoop wreckage (partial if the hold fills; `CargoFull` if it is already full) |
| `recall` | `fleet_id` | emergency-jump home from another sector (2× fuel, hull risk); a jump the fleet was charging is abandoned with its fuel. The answer is an `Alert` with what the jump cost: the fuel, a jump abandoned, how many ships took hull damage and how much (hull is never repaired), how many broke up; a warning when a ship was harmed, info when none was. Refused with `InvalidCommand` and nothing changed for a fleet at your homeworld: there is nothing to come back from, a jump it is charging there goes on, and `stop` is the order that halts it |
| `stop` | `fleet_id` | abort a move, harvest, or boarding. A jump called off does not return the fuel it burned, and backing out of a boarding leaves the site on alert, so its next boarding is likelier to be ambushed: either is told in an `Alert` (warning), the first with the fuel forfeit. Halting a harvest costs nothing and says nothing |
| `explore_site` | `fleet_id` | board the derelict in the current sector (20t, ambush risk) |
| `build` | `building_type`, `reserve` (default false) | upgrade a homeworld building one level: starts at once when a building slot is free and you can pay, otherwise waits in the queue (nothing is charged until it starts). Every order gets a stable `id`, reported in its `Queue` event and in the queue lists. `reserve: true` makes the order hold the line while it is short (see Queues and slots) |
| `research` | `tech`, `reserve` | research one level, same rules (one lab) |
| `build_ship` | `ship_class`, `count` | order a ship batch (one shipyard). Unlike a building or a tech, a batch is accepted only if your stockpile covers every unit now, and it is paid in full at that moment; otherwise it is refused with `InsufficientResources` and a message naming the shortfall per resource and how many units the stockpile would cover (5 ordered with money for 2 is refused; order 2). A paid batch starts at once or waits for the yard |
| `build_defence` | `kind` (`PulseTurret`, `LancerBattery`, `IonBastion`), `count` | order home defence structures in the shipyard queue, paid in full when ordered like ships; `DefenceLocked` until the Defense Grid and research it needs are in place |
| `cancel_build` | `queue_type`, `id` | cancel the item with that `id` once it has started. A building or tech refunds 50% of its cost, and waiting items of the same building or tech move up a level. A ship or defence batch keeps the units already built and refunds 50% of the price of every unit not yet delivered, the one under way included. A `Queue` event confirms what was refunded. `InvalidTarget` names where the id actually is (another queue, still waiting, or gone) |
| `cancel_queued` | `queue_type`, `id` | remove the item with that `id` while it still waits. A waiting building or tech paid nothing, so nothing comes back; a waiting batch paid when it was ordered and gets 50% back. A `Queue` event confirms it. Refused, not redirected, when the id is not waiting in the named queue |
| `preview_move` | `fleet_id`, `target:{q,r}` | do not move: reply `preview_move` with `fuel_cost`, `fuel_after`, `can_jump`, `hops_home` (shortest real route home, charted or not), `charted_hops_home` (shortest route over sectors you have entered, null when there is none), `route_unexplored` (the `hops_home` route crosses sectors you have never entered), `fuel_to_return`, `can_return`, plus `threat` (`rating`, `est_power`, `basis`), `fleet_power`, and the odds `ratio` and `label` (`safe`, `favourable`, `even`, `risky`, `deadly`), all for the fleet you name, not your strongest. `ratio` is not `fleet_power / est_power`: see Threat |
| `odds` | `fleet_id`, `sectors:[{q,r},...]` (at most 256) | do not move: reply `odds` with `fleet_id` and, for each sector in the order asked, `{sector, threat, ratio, label}`: how that fleet would fare there. Any sector may be named: adjacent or far, the one the fleet is in, seen or never seen |
| `leaderboard` | `limit` (default 20) | reply `leaderboard`: `entries` of `{rank,name,agent,score,core,combat,explore}`, plus `you` when you are below the limit, plus `bots` (same rows, ranked among themselves) when the world has built-in bots |
| `split` | `fleet_id`, `ship_ids:[id,...]` | detach those ships into a new fleet in the same sector (ids from the fleet's `ships`); at home this launches docked ships. At least one ship stays; fleet cap 8 in all, 3 away from home. The new fleet id arrives in an `Alert` event |
| `merge` | `fleet_id`, `other_fleet_id` | fold the other fleet into `fleet_id` (same sector, both idle); ships, cargo and fuel pool, the other fleet's standing orders end |
| `exchange` | `fleet_id`, `give`, `want` (`metal`, `crystal`, `deuterium`, lower case), `amount` (optional) | sell `amount` of `give` from the fleet's hold to the hub's exchange and take `want` into the hold; without `amount`, all of `give` aboard. The fleet must stand idle on the hub sector (0,0). The answer is an `Exchanged` event (what was given and received, the fee, the meter before and after; see The hub and its exchange). Refused with nothing changed and a sentence saying why: `NotInSector` away from the hub, `OnCooldown` in transit, `NoResources` for more than the hold carries or none, `CargoFull` when what comes back would not fit, `InvalidCommand` for one resource named twice, an amount that is not positive, or a trade that returns less than one unit |
| `exchange_quote` | same as `exchange` | do not trade: reply `exchange_quote` with `amount`, `receive`, `par`, `fee`, `meter` and `meter_after`, exactly what `exchange` with the same fields would do (`./play do` prints it as one `quote ...` line). It answers from anywhere and for an amount the fleet does not carry; `refusal` is then the sentence the trade would be refused with right now |
| `load` | `fleet_id`, `resource` (`metal`, `crystal`, `deuterium`, lower case), `amount` (optional) | move `amount` of `resource` from your stockpile into the fleet's hold; without `amount`, all the stockpile holds and the hold has room for. One resource an order. The fleet must stand idle at your homeworld with no standing orders. It is then `laden`: the dock leaves its hold alone until the fleet has been away (see Loading a hold). The answer is an `Alert` (info) saying what went aboard. Refused with nothing moved and a sentence saying what is short: `NoResources` when the stockpile holds less than `amount` or none, `CargoFull` when the hold lacks the room, `NotInSector` away from home, `OnCooldown` in transit or busy, `InvalidCommand` for an amount that is not positive or a fleet under standing orders. At your outpost `load` takes from the outpost's store instead and does not make the fleet `laden`; see Outposts |
| `unload` | `fleet_id` | put a `laden` hold back: what the caps allow goes into the stockpile now and the dock takes the rest as room opens. The answer is an `Alert` (info). Refused with `InvalidCommand` when the fleet was not loaded by `load`, `NotInSector` away from home. At your outpost `unload` puts the whole hold of any fleet into the outpost's store as far as it has room (`NoResources` for an empty hold); see Outposts |
| `found_outpost` | `fleet_id` | found your outpost in the sector the fleet stands in, paying 1000 metal, 500 crystal and 200 deuterium from its hold. The fleet must stand idle with no standing orders; the sector must be in rings 9 to 20, not an Anomaly, without a derelict, without a group in it (win the sector first), and at least 3 hexes from every other outpost; you may have one outpost. The answer is an `Alert` (info) with the tick it will be finished. Refused with nothing changed and a sentence saying why: `InvalidTarget` for a sector that does not qualify, `InvalidCommand` if you already have an outpost or the fleet is under standing orders, `NoResources` naming what the hold lacks, `OnCooldown` for a fleet that is moving, fighting or busy |
| `outpost_survey` | `fleet_id`, `sector:{q,r}` (optional) | do not found: reply `outpost_survey` for the named sector, or the fleet's own, with its `ring`, the `cost` and `build_ticks` of level 1, `raid_power` (`least` and `most`: the weakest and the strongest raid that ring sends), `platform_power`, `site` (`densities`, `grade`, `production` a tick at level 1, `hops_home`) and `refusal`: the sentence `found_outpost` would be refused with right now, absent when it would be accepted. Any sector may be asked about. One you have never seen is answered without `site` and with a `refusal` that says it is not on your chart (or that its ring is outside 9 to 20): what follows from the ring is told, what is there is not. For a sector the fleet does not stand in, a `refusal` that only says the fleet is elsewhere means the site itself qualifies |
| `upgrade_outpost` | none | build the next level of your outpost, paid in full from the outpost's own store at that moment. The answer is an `Alert` (info). Refused with `InsufficientResources` and the exact shortfall when the store does not cover it, `MaxLevelReached` at level 3, `InvalidCommand` with no outpost, while it is building or repairing, or while it is breached. `outpost.upgrade_refusal` in your state is that refusal's sentence beforehand, absent when the order would be accepted |
| `abandon_outpost` | none | give up your outpost. Its store is left as wreckage in the sector for 180 ticks, its score is gone, nothing is refunded, and you may found another. `InvalidCommand` with no outpost |

Standing orders are a separate message type (not a command):

```json
{"type":"policy_update","fleet_id":2,"preset":"salvage_and_sites",
 "params":{"min_fuel_pct":10,"cargo_return_pct":85,"max_range":4,"engage_ratio_x10":15,"fuel_budget_pct":20}}
```

`params` is optional, and so is every field in it: send only what you want to
change (`"params":{"max_range":2}`) and the rest take their defaults. A fleet
under orders carries them in its row: `fleets[].policy` is
`{"preset": ..., "params": {...}}` with all five settings as the server holds
them (after clamping), in `full_state` and in every `tick_update`. A fleet you
fly by hand has no `policy` field.

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
  A sector an outpost stands in is rated 0, power 0, `observed`: there is
  nothing to fight there and nothing comes.
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
  such as a Shipyard behind the Metal Mine it needs, never holds the line.
  Research and buildings can both be queued behind a prerequisite that is
  itself still queued, and a tech that is running can be queued again for its
  next level. A ship or defence batch is one order under a different rule: it
  is accepted only if the stockpile covers the whole batch now, pays for every
  unit at that moment, and is otherwise refused with `InsufficientResources`
  ("Scout x5 costs 1000 metal, 250 crystal and 150 deuterium; the stockpile is
  short by 120 metal and 30 deuterium. Nothing was ordered: a batch is paid in
  full when it is ordered. The stockpile covers 4."). Nothing in the yard ever
  waits on resources: a paid batch waits only for the yard to be free or for a
  queued Shipyard level or tech it needs, takes no `reserve`, and once started
  builds every unit without stopping (`shipyard_queue` shows `built` of
  `count`). Buildings run in `build_slots` parallel
  slots (1, plus one per Modular Fabrication level, max 3); research and the
  shipyard have one.
  Each queue holds its running items plus `queue_waiting_max` (3) waiting ones,
  so the building queue holds `build_slots + 3` and the others 4; a full queue
  refuses with `QueueFull`. Orders that can never start are refused with a
  specific error: `PrerequisitesNotMet` (naming what is missing),
  `MaxLevelReached`, `StorageTooSmall` (naming the Vault level it needs),
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
  Storage Vault cap: none of it is lost when the stockpile is full, the mines
  add nothing while a resource stands at or over its cap, and what is over the
  cap is spent like the rest (a raid counts it too).
  `paid` on the event is what it took from the stockpile: a building or tech
  pays on `Started`; a batch pays on the event that answers the order
  (`Started`, or `Waiting` when the yard is busy) and nothing when it starts
  later. A queue with nothing running but orders waiting is not idle.
- **Research** — 13 techs: hulls, shields, weapons, fuel, navigation, ship
  class unlocks (Corvette Tech needs only Shipyard 1, Frigate and Cruiser
  Tech need more), emergency jump, Modular Fabrication (building slots). The Research
  Lab divides research time by (1 + 0.10 x level).
- **Fleets** — up to 3 away from your homeworld and your finished outpost (8
  in all), 64 ships each, hex movement with
  fuel mass costs. Docking at home unloads cargo and refuels but never merges
  fleets: use `split` and `merge`. New ships join an idle fleet at home that
  has no standing orders, or form their own. Each fleet reports its `power`
  (the scale raids use) and `range_hops`: how many more hops out it can fly
  and still get home.
- **Fuel and stranding** — each fleet reports `jump_fuel` (one jump),
  `hops_home` (the fewest hops home over the real lanes, charted or not)
  and `home_fuel` (the fuel for those hops) next to `fuel`, and `jump_ticks`: the
  ticks one jump takes it from the order to the arrival, which its slowest
  ship and Navigation set. Refuelling at the
  homeworld costs 0.08 deuterium per fuel unit (the Fuel Depot adds +25% tank
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
  over. `merge` with a fresh fleet from home pools fuel and also rescues it.
  `recall` costs 2x the jump fuel per hex, all or nothing; ships that break
  up in it leave their share of the cargo where the fleet jumped from.
- **Combat** — round-based vs pirates, shields absorb first, rapid-fire
  chains (corvettes shred scouts…), victors drop salvage (`FleetDestroyed.salvage`
  is exactly the pile `collect_salvage` can take; it lasts 180 ticks, its
  sector view carries `salvage_despawn_tick`, and a `SalvageDespawned` event
  says when one expires). You only receive combat
  events for fights your fleets or homeworld are in, or that happen in a
  sector where you have a fleet or your homeworld. Each carries `owner`
  (empire name, `null` for NPCs) and `mine`; losing a fleet also raises a
  `Critical` `Alert`. A fleet that loses ships keeps only what the holds of
  the ships it has left carry: the rest, each resource in proportion, drifts
  as wreckage in that sector, where anyone can `collect_salvage` it for 180
  ticks, and a destroyed fleet leaves its whole cargo so (an `Alert` says how
  much and where). Fuel beyond the tanks that are left is lost. Hostile ids in a sector view are the ids `attack` and
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
  destroyed (at least one), and a long grace period. What the destroyed
  ships carried beyond the holds left goes to the stockpile as far as the
  Vault has room, and the rest is lost. Cargo aboard fleets at
  home counts as stockpile: the raid takes its slice of the two together,
  above one protected share, from stockpile and holds alike, so a hold at the
  dock hides nothing. Never
  buildings, never queues. `RaidResolved` reports `structures_lost`,
  `structures_restored`, `protected_kept`, `ships_lost` and, for a repelled
  raid, `salvage_despawn_tick`. The orbital salvage is on your own planet, so
  a fleet docked at home scoops it into storage by itself, up to what the
  Vault holds (`SalvageCollected` says what); with no fleet docked, or the
  Vault full, an `Alert` fires once when 60 s are left.
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
  - `span` is 15% of what your own mines make in one window, in weight, so
    it grows with your mines; an empire with no mine output pays 75%.
  - The meter falls by one `span` every `window_ticks` (86400 ticks at pace
    1, divided by P^0.75), whether you are connected or not. `clear_in` is
    the ticks until it is empty.
  - Example, season pace, mines 10/9/8 (`span` 4185): one Hauler's 632 metal
    is worth 316 deuterium at par. From an empty meter the fee runs from 25%
    to 33% across the trade, 29% on the whole: 225 deuterium. The same 632
    sold with the meter above 4185 would bring 79.
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
    top of the Vault. It is dear storage (a Hauler costs 950 resources and
    holds 200 x sqrt(P)), and no hiding place: a lost raid counts cargo at
    home with the stockpile (see Raids). Loading moves no score.
  - The trip: `load` the resource that is at its cap, fly to 0,0, `exchange`,
    fly home; the dock unloads what you bought.
- **Outposts** — one station per empire, on a sector of rings 9 to 20, the
  space where PvP will be allowed. It is built from what you carry to it.
  `load` the founding cost at home, fly it out, clear the sector if a pack
  lives there, and `found_outpost`; `outpost_survey` tells you beforehand
  what the site is worth.
  - **The site** decides the yield. A sector's `grade` is the sum of its ore:
    Sparse 0.5, Moderate 1, Rich 2, Pristine 4, for metal, crystal and
    deuterium. The outpost's extractor makes, per resource and game hour,
    `2 x density x ring factor x level` times P; the ring factor is 1.10 per
    ring from ring 3 (1.77 at ring 9, 5.05 at ring 20). It reads the ore the
    sector was made with, not what harvesting has left this hour, and it
    does not empty the tiles. A site makes only what it has ore of: most
    make no deuterium.
  - **Levels**: 1 costs 1000/500/200 and takes 15 game hours divided by P; 2
    costs 2500/1250/500 (37.5 h); 3 costs 6250/3125/1250 (93.75 h). Level 1
    is paid from the founding fleet's hold, levels 2 and 3 from the outpost's
    store when you order them. The outpost works at its old level while it
    builds. While level 1 is being built the outpost has a store and nothing
    else, and no raid comes.
  - **The store** is its own, apart from your stockpile: 3000/1500/750 at
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
    at home. Hull is not repaired there. A fleet at an outpost is not safe:
    see Raids below.
  - **Quiet**: while an outpost stands, no group lives in its sector or
    wanders in, and the sector reads as empty to `preview_move`, `odds` and
    its own `threat` (rating 0, power 0, ratio 10). The lair it displaced returns after its usual
    delay when the outpost is abandoned.
  - **Raids**: pirates of its ring raid it on the homeworld's raid clock,
    with the same warning, starting a day of that clock after level 1 is
    finished (`RaidIncoming` and `RaidResolved` carry `outpost`). A raid's
    power is `2 x` the ring's group power times 0.80 to 1.15: 31 to 45 at
    ring 9, 57 to 82 at ring 12, 126 to 182 at ring 16, 280 to 402 at ring
    20, whatever your empire is worth (`raid_power` in the survey and in
    your outpost's state gives the two ends). The platform (40, 100, 250 by
    level) and your ships standing there defend, and the defence counts a
    tenth more or less by chance. Repelled: your ships' shields are halved, nothing
    else, and no salvage. Lost: a quarter of the ships there are destroyed
    (at least one), and what they carried goes into the store; a quarter of
    the store, and of the cargo aboard fleets there, above the protected
    floor is taken; and the outpost is breached: no extractor, no
    refuelling, no platform. The floor is what the next repair would take
    from what is there (`protected`), for the level it stands at or the
    level it is building. After a lost raid none comes for 48 game hours
    divided by P^0.75. No raid destroys an outpost.
  - **Repair**: a breached outpost repairs itself. A repair is priced in
    metal: 537.5 at level 1, 1343.75 at level 2, 3359.375 at level 3 (a
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
    `grade`, `platform_power`, `defence_power`, `raid_power` (`least`,
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
  `core` = completed building and research levels + living ships + half of
  the defence structures + the finished levels of your outpost, each priced
  at cost x (1 metal, 1.5 crystal, 2
  deuterium) / 1000. Kills add the weight of the pile they leave (so points
  follow the finds factor and the sector's heat), in full up to 4x the NPC
  group's power and falling as 4/ratio beyond (half at 8x, a 5% floor, never
  zero), once per respawn. Exploration adds `0.5 x weight(loot) + 1.5 x T^1.5`
  for the first boarding of each derelict, 30 for a relic, and
  `0.15 + 0.10 x T^1.5` for each new sector whose chart you deliver to the
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
