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
its centre, built in worldgen from a coarse hash cell of the world seed. Size
grows with the ring: a cloud in the Inner Ring is a sector or two, one at ring 30
a dozen or more. Terrain of a cloud sector is Nebula.

A fleet harvests it (`harvest_power`: Recycler 5, cargo 1, others 0.5), refines the
hold into fuel at the usual 15% loss, and sits still in a sector that a pirate
pack of the ring guards. A cloud buys a few jumps and not a crossing.

- *Built.* `WORLDGEN_VERSION` is 6; no stored state is added. The map is cut in
  `CLOUD_CELL` 8 by 8 cells; a cell holds a cloud with `CLOUD_PCT` 25 percent, its
  centre at a hashed offset. A cloud reaches `CLOUD_RADIUS_BASE` 20 +
  `CLOUD_RADIUS_PER_RING` 7 x the centre's ring, in hundredths of a hex, at most
  `CLOUD_RADIUS_MAX` 300. No cloud touches a sector nearer the hub than
  `CLOUD_MIN_DIST` 2 or a home seat's ground. A sector's deuterium is the peak of
  `CLOUD_DEUT_PEAK` for the centre's ring (Sparse to ring 8, Moderate beyond),
  one level thinner each hex out, never below Sparse. Constants are in
  `shared/src/constants.rs`; `WorldGen::cloud_depth` in `shared/src/world.rs`.
- Cloud size by ring: rings up to 11 one sector, 12 to 25 seven, 26 to 39
  nineteen, 40 and beyond thirty-seven. Share of sectors in a cloud on seed 42:
  rings 2-8 0%, 9-20 1%, 21-30 5.8%, 31-45 10%, 46-60 11%.
- A cloud sector's yield is the ore the density has at its ring (`ore_regen_per_tick`
  refills it as ore refills). One Sparse sector is under a jump of fuel for a
  fleet of ten; Moderate is a few. `sim/tests/reach_probe.rs` measures it: with
  clouds, ten Light Fighters gain about 1.2 times the ring at Energy, Impulse and
  Tank level 10 and 13 and nothing below it on that seed; a ten-ship Cruiser fleet
  gains nothing, because its tank is larger than the deposits.
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
  owner in the order founded, in the one stored document (`SAVE_FORMAT` 16,
  `ECONOMY_VERSION` 18). A world in which nobody founds one plays as it did.
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
- Store: `RELAY_STORE` 1,000 units of deuterium, not pace-scaled. A jump of
  ten Cruisers burns 3,000 fuel, 240 deuterium at a dock's 12.5 fuel to the
  deuterium: one relay fills about four jumps for a fleet of that size, and a
  third of a tank. Deuterium only; `load` of anything else is refused.
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
  to a ring they could not have returned from, and back. The probe
  (`reach_probe.rs`, ring-4 home, ten ships, Hyperspace Technology and the
  other three at the row's level, relays stocked with their whole store by a
  convoy that is not counted, half spent each way, the limit of the row):

```
class          level  jumps  relays  off  clouds  relays_on  both
Light Fighter      0     10       1    9       9         19    20
Light Fighter      3     20       2   14      14         49    49
Light Fighter      6     35       4   21      21         55    57
Light Fighter     10     70       6   38      45         69    72
Light Fighter     13     85       7   45      52         75    79
Cruiser            0      8       1    8       8          8     8
Cruiser            3     16       2   12      12         12    12
Cruiser            6     28       4   18      18         18    18
Cruiser           10     56       6   32      32         32    32
Cruiser           13     68       7   38      38         38    38
```

  A fleet of cheap-burning ships (a Light Fighter burns 20 fuel a jump)
  is carried deep by one depot of 1,000 deuterium, which is 62 jumps of them;
  ten Cruisers get about two jumps of a half-store and nothing from it. That
  is the tier doing what it says, but ten Light Fighters at Hyperspace
  Technology 3 reaching ring 49 is the Wandering made easy by a hauler's
  single trip: `RELAY_STORE` is the knob to turn down first (a store of 300
  makes it 25 jumps), or the limit's step. The cruiser rows say the store is
  too small to matter to a fleet that burns what Cruisers do; that is by
  design for now (the haul that matters is a convoy of cargo ships).

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
- *Not done:* the `reach_probe` column (the next slice), a hub in the
  chart's remembered sectors, and any sign on the sector view of a hub.

### 3.4 Where the reach could break

Measure it, do not argue it. A scripted probe plays one fleet, with research at a
given level and one or more of the three tiers allowed, and reports the farthest
ring it holds a round trip to. The probe ships with slice 2 and is re-run after
each slice. A tier that makes the Wandering an easy drive is tuned down before the
next ships.

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
