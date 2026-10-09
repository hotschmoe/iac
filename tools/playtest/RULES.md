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
./play score [rows]      the leaderboard (humans and AI agents share one table)
./play map [radius]      every sector you have seen near your fleets, marked LIVE or STALE <age>, with threat T1-T9 and the odds of your strongest fleet
./play sector Q R        one sector in detail (LIVE / STALE marking, threat and odds, ore reserve and refill time)
./play preview F [Q R..] fuel, threat and odds for fleet F jumping to adjacent sectors; with no Q R, every exit
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

### Command reference (stdin)

| action | fields | effect |
|--------|--------|--------|
| `move` | `fleet_id`, `target:{q,r}` | jump to a connected sector (fuel, cooldown) |
| `scan` | `fleet_id` | reveal nearby sectors + faint signal contacts; scouts scan farther |
| `harvest` | `fleet_id`, `resource` (`Metal`, `Crystal`, `Deuterium`, `Auto`) | mine the current sector until cargo is full; `Auto` takes everything present, a named resource only that one (`ResourceNotPresent` if the sector lacks it) |
| `attack` | `fleet_id`, `target_fleet_id` | engage the NPC fleet listed as HOSTILE in your sector (use its id) |
| `collect_salvage` | `fleet_id` | scoop wreckage (partial if the hold fills; `CargoFull` if it is already full) |
| `recall` | `fleet_id` | emergency-jump home (2× fuel, hull risk) |
| `stop` | `fleet_id` | abort a move, harvest, or boarding |
| `explore_site` | `fleet_id` | board the derelict in the current sector (20t, ambush risk) |
| `build` | `building_type` | upgrade a homeworld building one level: starts at once when a building slot is free and you can pay, otherwise waits in the queue (nothing is charged until it starts) |
| `research` | `tech` | research one level, same rules (one lab) |
| `build_ship` | `ship_class`, `count` | queue ship production (one shipyard; the batch is paid when it starts) |
| `build_defence` | `kind` (`PulseTurret`, `LancerBattery`, `IonBastion`), `count` | queue home defence structures in the shipyard queue; `DefenceLocked` until the Defense Grid and research it needs are in place |
| `cancel_build` | `queue_type`, `index` (default 0) | cancel an item that has started (the `index`th running one) with a 50% refund; waiting items behind it move up a level; a `Queue` event confirms what was refunded |
| `cancel_queued` | `queue_type`, `index` | remove an item still waiting (the `index`th, from 0); nothing was paid, so nothing comes back; a `Queue` event confirms it |
| `preview_move` | `fleet_id`, `target:{q,r}` | do not move: reply `preview_move` with `fuel_cost`, `fuel_after`, `can_jump`, `hops_home` (shortest real route home, charted or not), `charted_hops_home` (shortest route over sectors you have entered, null when there is none), `route_unexplored` (the `hops_home` route crosses sectors you have never entered), `fuel_to_return`, `can_return`, plus `threat` (`rating`, `est_power`, `basis`), `fleet_power`, `ratio` and `label` (`safe`, `favourable`, `even`, `risky`, `deadly`), all for the fleet you name, not your strongest |
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
| `engage_ratio_x10` | 12 | fight only if your power is at least enemy power x this/10 (1-100); also the least ratio at which any doctrine will enter a sector, never below EVEN (1.1) and SAFE (2.5) where hostiles sit |

Fuel safety: the autopilot returns home when its fuel falls below the cost of
the shortest route home (charted or not) plus the `min_fuel_pct` reserve, and it
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
No doctrine moves a fleet into a sector whose threat (as `preview_move` shows
it) the fleet would not beat at `engage_ratio_x10`, and none passes through a
sector holding hostiles below SAFE odds. A refusal is a `hold` `PolicyAction`
saying which sector, its threat and the ratio; `prospect` names whether a
hazard, `max_range` or a fully charted board stops it. Heading home it takes
the safest lane and falls back to the shortest.

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
  target's `threat`, the named fleet's `fleet_power`, `ratio` and a `label`: SAFE from
  2.5, FAVOURABLE from 1.5, EVEN from 1.1, RISKY from 0.9, else DEADLY (simulated
  fights: a win at EVEN costs half the hull, below 0.9 like-for-like fleets
  lose; a lighter class against a heavier one needs about 1.5x more). Pirates return
  `2 + 0.25 x ring` game hours (divided by P) after a kill.
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
  slot is free or you can pay yet. It waits, strictly first in, first out, and
  starts and pays in full the moment a slot is free, what it needs is built and
  the stockpile covers the cost; an order short of resources holds back the
  ones behind it, so the line never reorders (an order that waits only on a
  prerequisite, such as a Shipyard behind the Metal Mine it needs, does not
  hold the line). Buildings run in `build_slots` parallel slots (1, plus one
  per Modular Fabrication level, max 3); research and the shipyard have one.
  Each queue holds its running items plus `queue_waiting_max` (3) waiting ones,
  so the building queue holds `build_slots + 3` and the others 4; a full queue
  refuses with `QueueFull`. Orders that can never start are refused with a
  specific error: `PrerequisitesNotMet` (naming what is missing),
  `MaxLevelReached`, `StorageTooSmall` (naming the Vault level it needs),
  `NoShipyard`, `NoResearchLab`, `ShipLocked`, `DefenceLocked`.
  `build_pending` / `research_pending` / `shipyard_pending` list each waiting
  order with `waiting_for` (the exact shortfall), `waiting_on` (`Slot`,
  `Resources`, `Prerequisite` or `Order`: held behind an earlier order) and
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
  is exactly the pile `collect_salvage` can take; it lasts 180 ticks and a
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
  lasts 180 ticks; `salvage_despawn_tick` says when). Tick updates add a
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
  `structures_restored`, `protected_kept` and `ships_lost`.
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
