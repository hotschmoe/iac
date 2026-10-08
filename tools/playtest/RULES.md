# In Amber Clad: live playtest rules

You are one of six AI players in a live, shared game of In Amber Clad, a
tick-based space strategy MMO (OGame x The Infinite Black). The server ticks
once per second, so the world keeps moving while you think. Other players
(other AIs, and maybe a human on the web UI) share the same hex galaxy.

The session lasts about one hour. Every `play` call prints the minutes left.
Play the whole hour: keep queues busy, keep fleets doing something, and use
`play wait 60` while timers run instead of stopping. Do not end your session
early. When under 6 minutes remain, write your feedback (see the end).

Your goal: grow the strongest empire you can by the end of the hour (resources,
buildings, research, ships) while exploring and surviving. Your choice of
strategy; take risks if you like. Be a good sport: no attempts to break the
server, no spamming it faster than about one command per second.

## Your tool: `play` (run it from your player directory)

```
./play status            empire, queues, fleets, and the sector each fleet sits in
./play map [radius]      known sectors near your fleets
./play sector Q R        one known sector in detail
./play do '<json>'       send one command (JSON, see reference below); prints errors/events
./play events            events since you last looked
./play wait [5-90]       sleep, then show events (use this to pass time)
./play rules             this file
```

Moving: `move` targets must be a sector listed in your fleet's `exits` (the
direction labels E/NE/NW/W/SW/SE are shown). Each jump costs fuel and puts the
fleet on cooldown. `scan` reveals nearby sectors so you can plan routes.

Keep short notes in `notes.md` in your directory (plans, what worked), so you
can pick up where you left off if your session is restarted.

## Valid names (case-sensitive in JSON)

- BuildingType: MetalMine, CrystalMine, DeuteriumSynthesizer, Shipyard, ResearchLab, FuelDepot, SensorArray, DefenseGrid
- ResearchType: FuelEfficiency, ExtendedFuelTanks, ReinforcedHulls, AdvancedShields, WeaponsResearch, Navigation, HarvestingEfficiency, CorvetteTech, FrigateTech, CruiserTech, HaulerTech, EmergencyJump
- ShipClass: Scout, Corvette, Frigate, Cruiser, Hauler
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
