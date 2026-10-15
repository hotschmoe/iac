# Deep space: reach, relays, roaming hubs and fading relics

Date: 2026-10-15. Status: proposal, built slice by slice; owner decisions of 2026-10-15
are marked *decided*. Every number below is provisional unless it is quoted from code.

**Source note.** *Decided* is the owner's word, from the conversation of 2026-10-15
after the playtest in `docs/playtests/2026-10-15/`. *Measured* is read from the code.
*Proposed* is everything else.

## 1. Why

The 2026-10-15 playtest (one hour at pace 1200) showed the map paying nothing:
four agents played mines and research, three never built a yard, and the one who
left the dock to harvest said a jump was worth less than a second of crystal. The
owner wants the opposite: big fleets, and the deep Wandering as the allure. No
points for visiting a tile. An incentive to reach further, paid in things the
economy cannot make at home.

*Decided.* Balance for human-time play (the season preset, pace 10, a week).
Faster paces are compressions of it and must keep the same relation between a
find and the income that surrounds it.

## 2. The invariant (pace)

*Measured.* Production, build and research times divide by P. Jumps, fuel,
cooldowns and combat are `RealTime`. Loot and derelict respawn are `Finds`
(P^0.5), and a delivered chart is not scaled at all (`CHART_VALUE_*`). At pace
1200 a find is therefore worth about one second of income; at season, minutes.

**Rule.** A find is worth the same number of minutes of the empire's income at
every pace. Loot, wreckage, derelict value, relic value and chart value follow
production (the `Economy` class). Travel, fuel, combat and respawn of lairs stay
as they are, because they are the human-time part of the game.

**Proof.** `tools/check_sim_constants.py` (or a table in `economy_numbers.json`)
prints, for a ring-N derelict, a ring-N chart and a threat-T wreck, the minutes
of a reference empire's gross production they equal at `persistent`, `season`,
`blitz` and `test`. They agree within a small tolerance. The ECONOMY_VERSION is
bumped.

Fast simulation does not need a hotter pace. The engine is ticks and no clock, so
`sim-run --pace season` plays a week as fast as the CPU allows. A faster pace is
for agent playtests, where the invariant above is what makes the map worth a trip.

## 3. Reach

*Measured.* A ship's tank is `tank_jumps` of its burn (10 fighters, 8 cruisers,
6 battleships); Energy Technology saves fuel, the Deuterium Tank adds tank,
`refine` turns a hold's deuterium into fuel at `REFINE_LOSS` 0.15, and an outpost
refuels from its store. Outposts stand on rings 9 to 20 only, one per empire. The
Wandering begins at ring 21, so a deep run ends about ring 25 to 28.

Reach is one ladder, from the least to the most deliberate:

```
  cloud  --> relay --> roaming hub
 (a few      (a chain   (rare, shared,
  jumps)      you haul)  trade and fuel)
```

A tier gives range, and none gives it free. Danger scales with the ring, and the
fleet that goes deep is a large one.

### 3.1 Deuterium clouds (slice 2)

A cloud is a seeded cluster of sectors with deuterium density that falls off from
its centre, built in worldgen from a coarse hash cell of the world seed. A cloud
is small near home and large in the Wandering. Terrain of a cloud sector is Nebula.

A fleet harvests it (`harvest_power`: Recycler 5, cargo 1, others 0.5), refines the
hold into fuel at the usual 15% loss, and sits still in a sector that a pirate
pack of the ring guards. A cloud buys a few jumps and not a crossing, and
`GameEngine::refining` keeps it so: standing in a cloud sector a fleet's tank
fills only to `depot_fill_jumps` of its own burn (the fill of a relay or a hub at
the same Hyperspace Technology), whatever the hold carries. Elsewhere (open
space, a dock, an outpost) refining fills the tank as it did. `FleetState.refine`
is that same function, and the refusal says the cloud's limit in the engine's
sentence.

- *Built.* `WORLDGEN_VERSION` is 8 and `ECONOMY_VERSION` 21; no stored state is
  added. The map is cut in `CLOUD_CELL` 5 by 5 cells; a cell holds a cloud with
  the chance of `CLOUD_BANDS` for the ring of its centre, the centre at a hashed
  offset. A cloud is its centre and the sectors within the band's `radius`; each
  hex from the centre is `CLOUD_FALLOFF_LEVELS` 2 levels thinner (never below
  Sparse). No cloud touches a sector nearer the hub than `CLOUD_MIN_DIST` 2 or a
  home seat's ground. The bands (centre ring, chance, radius and size, peak):

  | rings | chance | radius | sectors | centre |
  |---|---|---|---|---|
  | 2-10 | 50% | 1 | 7 | Rich |
  | 11-13 | 50% | 1 | 7 | Moderate |
  | 14-20 | 50% | 1 | 7 | Sparse |
  | 21-30 | 7% | 1 | 7 | Moderate |
  | 31-45 | 4% | 2 | 19 | Moderate |
  | 46+ | 3% | 3 | 37 | Rich |

  Constants are in `shared/src/constants.rs` (`cloud_band`);
  `WorldGen::cloud_depth` in `shared/src/world.rs`.
- Why the cap is in the refine and not in the density (measured, 2026-10-15):
  Sparse clouds beyond ring 20 once let a Light Fighter fleet at Energy
  Technology 10 chain free fuel, +20 rings, because a tank refilled to full at
  every cloud is an unlimited number of relays. The cap makes a cloud no better
  than one relay of that research. It does not stop a chain of clouds closer
  together than the fill (a relay is limited in number, a cloud is not), so the
  deep bands are rare: at 12, 8 and 6 percent clouds alone added +17 rings at
  level 6 and +10 at level 3; at 7, 4 and 3 percent +11.5 and +8, with the
  census below.
- Share of sectors in a cloud on seed 42: rings 2-8 5%, 9-20 13%, 21-30 2.2%,
  31-45 2.3%, 46-60 4.6%.
- A cloud sector's yield is the ore the density has at its ring (`ore_regen_per_tick`
  refills it as ore refills). Measured by `reach_probe.rs` for ten Light
  Fighters over eight seeds: the nearest cloud is a median 4.5 hops from the
  ring-4 home (8 of 8 within a full tank at level 0) and clouds add +3.5 rings
  at level 0 (a gain in 7 of 8 seeds), +8 at level 3, +11.5 at level 6, +3 at
  level 10 and +2 at level 13: the cap bites on the big tanks, which a cloud can
  no longer fill. Ten Cruisers and Battleships gain nothing: their jump burns
  more than a deposit holds.
- Pace: the ore regenerates as ore does now (`ore_regen_per_tick`); the fuel it
  makes is real time.

### 3.2 Relays (slice 3)

A relay is a mini outpost: a small store of deuterium, `load` and `unload` of
a fleet's hold into it, and a refuel of a fleet standing there from that
store. It has no extractor, no platform, no repair, no raid and no garrison.
It is owner-only. The fuel in it is what its owner hauled, so reach is
logistics and not a gift.

- *Built.* `Relay` is `{sector, store, founded_tick, visited_tick,
  dry_tanks}`; `GameEngine::relays` and `Snapshot::relays` are a list per
  owner in the order founded, in the one stored document (`SAVE_FORMAT` 16). A world in which nobody founds one plays as it did.
- Founded by a fleet standing on a sector of ring `RELAY_MIN_RING` 9 or deeper
  (the Wandering included) with no hub, Anomaly, derelict, lair, group,
  outpost or relay (`relay_site`, the one place it is refused and worded;
  `relay_survey` quotes it, as `outpost_survey` does). The cost is carried in
  the founding fleet's hold: `RELAY_COST_BASE` 500 metal, 250 crystal and 100
  deuterium at ring 9, plus `RELAY_COST_GROWTH` 0.1 of it for every ring
  deeper (`scaling::relay_cost`; 2.1 times at ring 20, 3.1 at ring 30). It
  does not scale with pace.
- Limit: `RELAY_BASE_MAX` 1 and one more for every `RELAY_LEVELS_PER_STEP` 2
  levels of Hyperspace Technology (`scaling::relay_limit`), so three at level
  4 and seven at level 12. Hyperspace Technology does nothing of its own but gate
  the Hyperspace Drive and the heavy ships and carry `RecallRisk`, which is
  why it carries this. The refusal names the
  limit and the technology.
- Store: `RELAY_STORE` 800 units of deuterium, not pace-scaled. Deuterium
  only; `load` of anything else is refused. How much of it a fleet may draw
  is not flat: a relay (and a roaming hub, 3.3) fills a tank to at most
  `scaling::depot_fill_jumps` jumps of the fleet's own burn, which is
  `DEPOT_FILL_JUMPS_BASE` 10 and `DEPOT_FILL_JUMPS_PER_LEVEL` 3 for every level
  of Hyperspace Technology (10, 19, 28, 40 and 49 jumps at levels 0, 3, 6, 10
  and 13; `RelaysState.fill_jumps`, `GameEngine::depot_room`). So the fill is
  the fleet's own measure: it is a tank for ten Light Fighters (a jump of them
  is 200 fuel) and a few jumps for ten Cruisers (a jump is 3,000 fuel), and
  research is what lets a long tank be topped up in the deep. A flat store
  could not do both jobs: a store that carries ten Cruisers refills ten Light
  Fighters from empty to a full 85-jump tank at level 13 and takes them to
  ring 78; the fill cap holds that at ring 63.
- Decay: a relay no fleet of its owner has stood at for `RELAY_DECAY_TICKS`
  172,800 pace-1 ticks (an `Attention` timer: two days at pace 1, 4.3 hours at
  season, 24 minutes at blitz) is gone with its store. A fleet in the sector
  every tick holds it off; a visit sets the clock back. `RelayDecayed` and an
  alert say so.
- A fleet flown by hand that arrives at a relay, or stands there as its store
  is stocked, has its tank filled from it at the dock's rate; one under
  standing orders is left alone, and `load` is refused it. A fleet at a relay
  still counts among the fleets away from the docks. Bots and `recall` do not
  use relays. Pirates do not look for
  them; a founding needs no group in the sector, and a lair is refused, so no
  lair template can respawn into one.
- Wire: `found_relay`, `relay_survey` (a query); `RelaySurvey`; `relays`
  (`RelaysState`: `limit`, `technology`, `level`, and a `RelayState` each with
  `sector`, `ring`, `store`, `store_cap`, `decay_in`) in the full state and in
  every tick update; `FleetState.at_relay`; events `RelayFounded` and
  `RelayDecayed`. The web client has a dock tile (Y) with the founding survey
  and, at a relay, the store, its decay and load and unload; the map draws a
  mark.
- *Measured.* `sim/tests/relays.rs` hauls 600 deuterium to a relay with a cargo
  fleet, brings ten Light Fighters to it, refuels them there and has them fly
  to a ring they could not have returned from, and back; one more checks the
  fill cap at two levels of research. The probe (3.5) has the relays column.
  Before the cap and with the store at 1,000, and with the planner as it is
  now (the first probe's planner left heavy fleets' relay chains unfound and
  reported "nothing" for ten Cruisers, which was its own limit and not the
  tier's): relays took ten Light Fighters to 47 at level 3 and 78.5 at level 13
  and ten Cruisers to 16 and 60.5. The cap and the 800 store make those 47 and
  63 for the Light Fighter, and 14 and 46 for the Cruiser; level 13 is where
  the cap bites a fleet whose tank outruns it.

### 3.3 Roaming neutral hubs (slice 4)

A rare neutral post in the Wandering that refuels and trades and moves.

- *Built.* `HUB_COUNT` 4 hubs, ids 0 to 3, each in a band of its own:
  `HUB_BAND_RINGS` 10 rings from `HUB_FIRST_RING` 22 (22-31, 32-41, 42-51,
  52-61), so a deep hub always exists and two hubs can never meet. A hub
  stays `HUB_PERIOD_TICKS` 86,400 pace-1 ticks (an `Attention` timer: a day at
  pace 1, 4.3 hours at season, 12 minutes at blitz) and its stays are offset
  by `id / HUB_COUNT` of a period, so they end a quarter-period apart.
  `HUB_WARN_TICKS` 3,456 (also `Attention`, so the warning is the same share
  of a stay at every pace: ten minutes at season, a twenty-fifth of a stay) comes
  before the departure.
- Position: `WorldGen::hub_sites(id, epoch)` hashes `(seed, id, epoch, attempt)`
  to a ring in the hub's band and a place on it, and keeps those that
  `hub_legal` accepts: no lair template, no derelict, no Anomaly, off home
  ground, joined to the centre by open lanes. `hub_site` is the first of
  them. That is a pure function of seed, id and epoch (`tick + id x period /
  count` over the period), and it is the whole answer in a world where
  nothing stands in the way.
- *Collisions with state.* Something a player owns can stand on a sector the
  seed offers, so the position is the first of `hub_sites` that
  `GameEngine::hub_blocked` does not refuse (a relay, an outpost, a pirate
  group, a fight, another hub), taken at the tick the stay begins, and kept
  (`GameEngine::hubs`, `Snapshot::hubs`: `SAVE_FORMAT` 17) because the
  state that decided it is gone by the time anybody asks again. Where every
  candidate is blocked the hub stays another stay. The other direction
  is refused outright: no relay is founded where a hub stands and no group
  follows into one (`is_refuge`). A fleet standing in a sector when the hub
  leaves is unharmed and the sector is ordinary.
- Knowledge: nobody knows where a hub is until something of theirs has eyes
  on its sector (a fleet there, an unexpired scan, the homeworld's sensors;
  a chart delivered earlier does not carry it, because a hub is not part of
  a sector's template). `GameEngine::hub_sightings` keeps, for each player
  and hub, the stay and sector last seen; `GameState.hubs` and every tick
  update list them (`HubState`: `sector`, `ring`, `services`, `departs_at`,
  `departing`, `stale`). A stay that is over is `stale` and says where the
  hub was, not where it is. `HubArrived` is sent once for each stay a player
  first has eyes on, `HubDeparting` `HUB_WARN_TICKS` before the departure to
  everyone who knows the stay. The centre hub stays in `WorldInfo.hub`.
- Services (`ROAMING_HUB`): `Exchange`, the centre's, on the same meter and
  through the same `exchange_terms`; and `Refuel`. `refuel` turns the hold
  into fuel, never the stockpile: deuterium at the dock's rate with no loss
  (`FUEL_DEUT_PER_UNIT`, better than `refine`'s 15% loss), metal and crystal
  priced by `exchange_terms` into the deuterium the tank wants, at the
  player's fee and meter, so the hub returns no more than it takes. It
  finds the least amount that fills the tank. `FleetState.refuel` quotes
  each resource in the hold at a hub that refuels, with the refusal.
- Neutral: the sector has no lair template and no derelict, so no group is
  generated there; `is_refuge` keeps pursuit out; no fight starts. A fleet
  standing there counts among the fleets away from the docks, as at the
  centre. A hub sector keeps the ore its seed rolled.
- Bots do not use a roaming hub and the centre hub plays as before; no
  bot reads `hubs`. The play tool lists the hubs and the `refuel` quote.
- A hub's refuel is held to the same `depot_fill_jumps` as a relay's (the
  quote says what it would add, the refusal the jumps and the level). Without
  it a hold of deuterium (ten Light Fighters carry 1,581 at season) refilled a
  tank twice at a hub in rings 22 to 61 and took a fleet to ring 86.
- *Not done:* a hub in the chart's remembered sectors, and any sign on the
  sector view of a hub.

### 3.4 Where the reach could break

Measure it, do not argue it. A scripted probe plays one fleet, with research at a
given level and one or more of the three tiers allowed, and reports the farthest
ring it holds a round trip to. The probe ships with slice 2 and is re-run after
each slice. A tier that makes the Wandering an easy drive is tuned down before the
next ships.

### 3.5 Measured reach

`sim/tests/reach_probe.rs`, four tests that print these tables (`cargo test -p
iac-sim --release --test reach_probe -- --nocapture`; about ten minutes wall on
six threads). The pace is season, the home is the ring-4 seat, the fleet is ten
ships of one class, and the level is Energy Technology, Impulse Drive, Deuterium
Tank and Hyperspace Technology all at once (and Weapons, Shielding and Armour for
the fleet's power). Seeds: 42, 7, 101, 2024, 31337, 555, 9001, 1234. The planner
is greedy (relay chains by three kinds of next stop, every ring tried as a
target), so every ring is a lower bound. Only
`reach_with_and_without_clouds_and_relays` (seed 42) flies its plans in the
engine, and relays there are stocked with a quarter held back; the multi-seed
tables are plans only. The hubs column is a plan: a hold of deuterium (the
best case: what a fleet carries from its stockpile) refuels at the hub at the
position the world gives it at tick 0, half on the way out and half on the way
back, filled to `depot_fill_jumps`, at most one hub on a route. A hub stays
hours of ticks and a plan is flown in a few hundred (the first hub leaves at
tick 3,842 and a Light Fighter jump takes 2), so a hub that moves is not
modelled; a fleet that arrives to find it gone is stranded, which is what the
warning is for.

What changed since the first version of this section: the target search used to
try only the eight deepest sectors, which hid every relay chain of a heavy fleet
(its ten Cruisers "gained nothing"), and a heavy fleet's relays are now also
tried at the first sector that wastes none of the fill. The old constants (store
1,000, no cap, clouds at 8 by 8 and 25 percent, Moderate to the Wandering) with
this planner:

| class | level | off | clouds | relays | clouds+relays |
|---|---|---|---|---|---|
| Light Fighter | 0 | 9 | 9 (9-9) | 19 (19-19) | 19.5 (19-25) |
| Light Fighter | 3 | 14 | 18 (14-22) | 47.5 (43-49) | 49.5 (49-50) |
| Light Fighter | 6 | 21 | 36 (34-40) | 55 (53-57) | 57 (57-57) |
| Light Fighter | 10 | 39 | 59.5 (58-62) | 72 (72-74) | 72 (72-74) |
| Light Fighter | 13 | 45 | 65.5 (65-69) | 78.5 (78-79) | 79 (78-79) |
| Cruiser | 3 | 12 | 12 (12-12) | 16 (15-16) | 16 (15-17) |
| Cruiser | 10 | 32 | 32 (31-33) | 52 (49-53) | 52.5 (51-54) |
| Cruiser | 13 | 38 | 38 (38-38) | 60.5 (57-63) | 61 (59-63) |
| Battleship | 3 | 10 | 10 (10-10) | 12 (11-12) | 12 (11-12) |
| Battleship | 10 | 25 | 25 (25-25) | 37.5 (36-39) | 38 (36-39) |
| Battleship | 13 | 29 | 29 (29-30) | 43 (42-45) | 43 (42-45) |

Now (store 800, fill 10 + 3 a level, the clouds of 3.1 with the cap of
`refining` and clouds beyond ring 20). Farthest ring, median (min-max) over the
eight seeds. The clouds column before the cap and the deep clouds (clouds of
rings 2 to 20 only, a tank filled to full) was Light Fighter 12.5, 20, 30, 51 and
57 at levels 0, 3, 6, 10 and 13, and the all-three column 23.5, 48, 53.5, 60 and
65.5; Cruiser and Battleship columns are unchanged within a ring:

| class | level | off | clouds | relays | hubs | clouds+relays | all three |
|---|---|---|---|---|---|---|---|
| Light Fighter | 0 | 9 (9-9) | 12.5 (9-14) | 19 (18-19) | 9 (9-9) | 23.5 (21-26) | 24.5 (21-31) |
| Light Fighter | 3 | 14 (14-14) | 22 (19-26) | 47 (43-48) | 14 (14-14) | 48 (46-49) | 48 (48-58) |
| Light Fighter | 6 | 21 (21-21) | 32.5 (27-38) | 53 (52-54) | 35 (21-42) | 53 (53-54) | 53.5 (53-59) |
| Light Fighter | 10 | 39 (38-39) | 42 (40-44) | 59.5 (59-60) | 52 (42-56) | 59.5 (59-60) | 59.5 (59-60) |
| Light Fighter | 13 | 45 (44-46) | 47 (46-48) | 63 (62-64) | 61.5 (59-66) | 63 (62-64) | 63 (62-66) |
| Cruiser | 0 | 8 (8-8) | 8 (8-8) | 9 (8-9) | 8 (8-8) | 9 (8-9) | 9 (8-9) |
| Cruiser | 3 | 12 (12-12) | 12 (12-12) | 14 (13-14) | 12 (12-12) | 14.5 (13-15) | 14.5 (13-15) |
| Cruiser | 6 | 18 (18-18) | 18 (18-18) | 24 (23-25) | 35 (18-42) | 25 (24-25) | 39.5 (36-47) |
| Cruiser | 10 | 32 (31-32) | 32 (31-32) | 40 (37-42) | 52 (40-56) | 42 (40-45) | 54 (48-60) |
| Cruiser | 13 | 38 (37-38) | 38 (37-38) | 46 (42-49) | 61.5 (57-66) | 47 (45-51) | 62 (59-66) |
| Battleship | 3 | 10 (10-10) | 10 (10-10) | 11 (10-11) | 10 (10-10) | 11 (10-11) | 11 (10-11) |
| Battleship | 10 | 25 (25-25) | 25 (25-25) | 31 (29-34) | 52 (40-56) | 32.5 (30-34) | 52.5 (40-56) |
| Battleship | 13 | 29 (29-29) | 29 (29-29) | 36.5 (35-39) | 61.5 (57-66) | 37.5 (35-39) | 61.5 (57-66) |

Fuel-reach against fight-reach, with all three allowed. A fight-reach of Nx is
the farthest ring a plan holds when it only enters sectors whose lair group is at
most N times the fleet's own power:

| class | level | fleet power | fuel ring | fight 1x | fight 2x | fight 4x | lairs on route | strongest, x fleet |
|---|---|---|---|---|---|---|---|---|
| Light Fighter | 0 | 910 | 24.5 (21-31) | 19 (17-21) | 21 (21-22) | 23 (21-24) | 14.5 (7-17) | 4.72 (max 14.04) |
| Light Fighter | 3 | 1183 | 48 (48-58) | 22 (20-24) | 24 (23-24) | 27 (27-28) | 38 (33-40) | 56.97 |
| Light Fighter | 6 | 1456 | 53.5 (53-59) | 22 (22-24) | 24 (23-25) | 28.5 (28-30) | 44 (37-56) | 46.29 |
| Light Fighter | 10 | 1820 | 59.5 (59-60) | 23 (22-24) | 25 (25-27) | 29 (29-32) | 49.5 (37-75) | 37.03 |
| Light Fighter | 13 | 2093 | 63 (62-66) | 23 (22-24) | 26 (26-27) | 29 (29-32) | 52 (45-67) | 32.20 |
| Cruiser | 3 | 8775 | 14.5 (13-15) | 14.5 (13-15) | 14.5 (13-15) | 14.5 (13-15) | 5 (2-6) | 0.06 |
| Cruiser | 6 | 10800 | 39.5 (36-47) | 30 (24-31) | 33 (32-34) | 36.5 (35-38) | 26 (24-31) | 6.24 |
| Cruiser | 10 | 13500 | 54 (48-60) | 33 (32-35) | 35 (35-36) | 39 (38-39) | 39.5 (35-51) | 4.99 |
| Cruiser | 13 | 15525 | 62 (59-66) | 33 (32-35) | 36 (36-38) | 39 (39-41) | 50 (42-54) | 4.34 |
| Battleship | 10 | 32400 | 52.5 (40-56) | 36 (35-38) | 39 (38-40) | 52.5 (40-56) | 36.5 (30-48) | 2.08 |
| Battleship | 13 | 37260 | 61.5 (57-66) | 36 (36-38) | 61.5 (57-66) | 61.5 (57-66) | 47 (43-54) | 1.81 |

Cloud distance from the ring-4 home, in lane hops, and the rings clouds add for a
Light Fighter at levels 0, 3 and 6 (tank 10, 20 and 35 jumps):

| seed | nearest cloud | rings added, levels 0/3/6 |
|---|---|---|
| 42 | 5 | +4/+6/+12 |
| 7 | 2 | +3/+6/+10 |
| 101 | 6 | +5/+11/+11 |
| 2024 | 4 | +1/+8/+14 |
| 31337 | 4 | +4/+9/+17 |
| 555 | 6 | +0/+8/+10 |
| 9001 | 4 | +2/+5/+6 |
| 1234 | 6 | +4/+12/+14 |

Reading:

- Relays now gain more for a heavy fleet than for a light one by what it cannot
  do for itself: ten Cruisers go +2 (level 3), +8 (10) and +8 (13) rings, ten
  Battleships +1, +6 and +7.5, and ten Light Fighters are no further than they
  were at levels up to 6 and held at ring 63 at level 13 (78.5 before). Level 3
  is the weak row: the two relays of Hyperspace Technology 3 fill a Cruiser's
  tank by 14 jumps in all, and the store's 800 deuterium is under three jumps of
  it; a store of 1,000 gave +4 there and +16 at level 10 (the relays double with
  the level, so the gain does too). The store was set where level 10 is about +8.
- No column is past ring 66 at level 13 (before the hub's cap it was 86 for a
  Light Fighter fleet). Light Fighter at 65.5 with clouds and relays is the
  deepest.
- Danger still gates the light fleet, not the fuel: 1x to 4x its power a Light
  Fighter fleet of ten reaches ring 17 to 32 at every level while its fuel is
  at 23 to 65. A Cruiser fleet's fight-reach is 30 to 41 from level 6 on, and
  its fuel is what reaches there; the heavy fleet is the one the tiers carry
  as far as the guard lets it (level 13: fuel 62, 4x fight 39).
- Hubs are the deepest tier for a heavy fleet at levels 6 to 13 (Cruiser 35,
  52, 61.5; Battleship 52, 61.5 at levels 10 and 13) and do nothing for the
  fleet at levels 0 and 3, whose tank never reaches the Wandering's first hub
  (ring 22). For a Light Fighter at level 13 a hub alone (61.5) is within four
  rings of all three together (65.5): the tiers do not stack, because each
  fills the same capped tank.
- Clouds with the cap: alone they add +3.5, +8 and +11.5 rings at levels 0, 3
  and 6 for ten Light Fighters (the first level near +8 is the top-up asked
  for; level 6 is a few rings over it) and +3 and +2 at levels 10 and 13, where
  the cap, not the deposit, is the limit: a cloud fills a big tank only to the
  jumps a relay would. Together with relays and hubs nothing moves: the all-three
  column at level 13 is ring 63 for Light Fighters and 62 for Cruisers, no deeper
  than before the deep clouds, and the heavy fleets' columns are unchanged.
- The deep bands are rare because clouds are free relays in unlimited number: at
  12, 8 and 6 percent they added +17 rings at level 6, and a rarer deep band
  was chosen over a lower cap (the owner's reading: tune density, not the cap).
- Not reached: level 3 for ten Cruisers is +2, not +4 to +10, with the store at
  800 and the quarter held back in the plan; the flown plan on seed 42 is one
  ring better (13 against 12 off). Clouds at levels 6 to 13 add +9 to +12 rather
  than a top-up of a few rings: a Rich centre in rings 9 to 10 is what gives a
  level-0 fleet something to reach, and a Moderate centre there moved level 0
  from +3.5 to +0 in six of eight seeds; the same clouds are what a bigger
  tank passes on its way.

## 4. Fading relics (slice 5)

*Decided.* A relic is a standing percentage bonus that fades to zero over time.

- A site in the Wandering (and a rarer one in the Outer Ring) held by a pirate
  group of its ring. The site is claimed by delivering the find to a dock (home,
  outpost or relay), so it is carried risk.
- The bonus is `pct * (1 - elapsed / RELIC_FADE_TICKS)` of the claim tick,
  computed on read; the player stores a short list of `(kind, pct, claimed_tick)`.
  The same kind refreshes, never stacks, and the sum has a cap.
- Kinds (provisional): a mine's output, a ship class's fuel burn, a research or
  build time, a shield or hull at the refit. *Not* a free level of any technology:
  a fading level would give the score a number to disagree with.
- **Never counts toward score.** The score is buildings, research and fleet at
  their real levels; a relic moves nothing on the board in either direction.
- *Decided defaults:* a fade over about a day at season (`RELIC_FADE_TICKS`,
  `Economy`), the deepest tier about 25%, nearer ones less.
- Sites respawn on a long finds timer, as derelicts do, so the trip is a loop:
  go deep, spend the bonus, come back out.

### 4.1 Built (slice 5)

*Measured*, from the code. Every number is provisional and named in
`shared/src/constants.rs`; `ECONOMY_VERSION` is 22 and `SAVE_FORMAT` 18
(`Player.relics` is a list now, and `Fleet` gained `relic`). Generation is not touched here:
`WORLDGEN_VERSION` is 8, from the clouds.

**Site.** The carrier is the derelict that already rolled an ancient relic
(`relic_chance`: 2% per threat point above 5, to 8%; threat 6 is ring 19, so
the Outer Ring's last rings have them rarely and the Wandering more often).
Its loot is still `RELIC_LOOT_MULT` times larger. The kind and the strength
are rolled from the same dice as the boarding (seed, sector, tick and fleet),
after every draw the boarding already made, so a world that finds no relic
plays as it did. Sites come back on `DERELICT_RESPAWN_TICKS` as before.

**Item.** `Fleet.relic: Option<RelicItem{kind, pct}>`. One a fleet. A second
find is kept only when stronger. `merge` keeps the stronger of the two; `split`
leaves it with the fleet that was split. It does not fade aboard, and a fleet
destroyed loses it. It is claimed (`claim_relic`) at the arrival at, or the
recall to, the owner's homeworld, finished outpost or relay.

**Claim.** `Player.relics: Vec<ClaimedRelic{kind, pct, claimed_tick}>`, one a
kind. A claim replaces a held relic of its kind only when its `pct` is above
what the held one gives now (the fade clock of the winner is the claim's);
otherwise the item is spent and `RelicClaimed.applied` is false. A relic is
dropped, with a `RelicFaded` event, on the tick it reaches nothing.

**Fade.** `pct * (1 - elapsed / RELIC_FADE_TICKS)`, on read, from the claim
tick (`ClaimedRelic::now`); nothing ticks. `RELIC_FADE_TICKS` is 864,000, an
`Economy` timer, so a day at the season preset (86,400 ticks at pace 10).

**Kinds and where each is applied** (one input at the one place):

| kind | effect | place |
|---|---|---|
| `metal_output`, `crystal_output`, `deuterium_output` | mine's output x (1 + b), natural production included | `scaling::production_per_tick` |
| `fuel_burn` | every jump's burn x (1 - b), with Energy Technology | `scaling::fuel_rate_modifier` |
| `research_time` | time of a research that starts now x (1 - b) | `scaling::research_time` |
| `building_time` | time of a building that starts now x (1 - b) | `scaling::building_time` |

`RelicBonus::of` sums the held relics at a tick. A family (the three outputs,
the fuel burn alone, the two times) is held to `RELIC_TOTAL_CAP` (0.40): over
it, each kind of the family is scaled down together. The exchange's span
reads the mines without relics (its meter must not wobble as a relic fades),
and the bots' planner plans without them.

**Strength.** `relic_ceiling(ring)` is `RELIC_PCT_OUTER_MAX` (8%) to ring 20,
then linear to `RELIC_PCT_DEEP_MAX` (25%) at `RELIC_DEEP_RING` (36); a roll
is between `RELIC_PCT_FLOOR_SHARE` (half) of it and all of it.

**Score.** `sim/src/score.rs` does not read relics; `sim/tests/relics.rs`
plays two otherwise equal empires, one holding relics, and compares the board.

**Wire.** `PlayerState.relics` (`RelicState`: `kind`, `pct`, `pct_now`,
`claimed_tick`, `expires_at`), `FleetState.relic`, and the events
`RelicFound`, `RelicClaimed`, `RelicFaded`. The web client lists the relics on
the Overview (a bar from `claimed_tick` to `expires_at`) and marks a fleet
that carries one.

*Open.* Bots claim what they carry and do not plan a trip for one. Whether
the item already running in a queue should speed up when a relic is claimed
(it does not) is not decided.

## 5. Slices, in order

| # | Slice | Engine | Wire | Web | Proof |
|---|---|---|---|---|---|
| 1 | Pace invariant: loot, wreck, derelict and chart follow production | yes | no | no | the table of section 2; `economy_numbers` |
| 2 | Clouds and the reach probe | worldgen | no | map | the probe's table at three research levels |
| 3 | Relays | yes | yes | panel | an e2e scenario: haul, found, refuel, jump past the old limit |
| 4 | Roaming hubs | yes | yes | map, panel | determinism, `hosts_agree`, `both_targets` |
| 5 | Fading relics | yes | yes | panel | a relic fades to zero; same bytes across a stop |

Every slice bumps `ECONOMY_VERSION` (or `WORLDGEN_VERSION`), and the save format
when a snapshot gains a field. A feature is done when the wire and the web client
have it.

## 6. Not decided

- Whether a relay may be raised by another empire's fleet (taken, destroyed).
- How many hubs a world holds (four is a guess) and whether the exchange of a
  roaming hub should have a meter of its own.
- Whether deeper relic tiers exist at the very edge of the generated map or are
  bounded by ring 60 (where edge survival bottoms out).
