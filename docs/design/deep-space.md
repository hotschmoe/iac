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

- A hub is a pure function of seed and tick: `position(id, epoch) =
  hash(seed, id, tick / HUB_PERIOD_TICKS)` (rejection-sampled onto a legal
  Wandering sector). Nothing is stored, so every host agrees.
- A warning event `HUB_WARN_TICKS` before it departs, and charts or sensor range
  show where it stands now. A fleet standing in it when it departs is unharmed:
  the sector becomes an ordinary one.
- *Decided defaults:* a move every few hours at season (`HUB_PERIOD_TICKS`,
  `Attention`), a warning about ten minutes ahead (`HUB_WARN_TICKS`).
- Services: `Exchange` as the centre hub's, plus `Refuel` bought from the hold by
  the exchange's rate and fee, never from the stockpile, so a hub does not
  replace tankers.
- It stays neutral: no hostile group, no fight, no pursuit into it, no relay,
  no derelict.
- Protocol: `WorldInfo.hub` becomes a list, or the client is handed a function of
  tick. The centre hub is entry zero. Bots and the headless client read the same.

### 3.4 Where the reach could break

Measure it, do not argue it. A scripted probe plays one fleet, with research at a
given level and one or more of the three tiers allowed, and reports the farthest
ring it holds a round trip to. The probe ships with slice 2 and is re-run after
each slice. A tier that makes the Wandering an easy drive is tuned down before the
next ships.

### 3.5 Measured reach

`sim/tests/reach_probe.rs`, three tests that print these tables (`cargo test -p
iac-sim --release --test reach_probe -- --nocapture`; about 4.5 minutes wall on
four threads). The pace is season, the home is the ring-4 seat, the fleet is ten
ships of one class, and the level is Energy Technology, Impulse Drive, Deuterium
Tank and Hyperspace Technology all at once (and Weapons, Shielding and Armour for
the fleet's power). Seeds: 42, 7, 101, 2024, 31337, 555, 9001, 1234. The planner
is greedy, so every ring is a lower bound. Only `reach_with_and_without_clouds_and_relays`
(seed 42) flies its plans in the engine; the multi-seed tables are plans only. What
was cut: no flight of the multi-seed plans, two classes, five levels.

Farthest ring, median (min-max) over the eight seeds:

| class | level | off | clouds | relays | both |
|---|---|---|---|---|---|
| Light Fighter | 0 | 9 (9-9) | 9 (9-9) | 19 (19-19) | 20 (19-27) |
| Light Fighter | 3 | 14 (14-14) | 14 (14-14) | 49 (47-50) | 50 (49-50) |
| Light Fighter | 6 | 21 (21-21) | 21 (21-21) | 56.5 (55-57) | 57 (57-57) |
| Light Fighter | 10 | 39 (38-39) | 56.5 (45-61) | 72 (69-74) | 72 (72-74) |
| Light Fighter | 13 | 45 (44-46) | 64.5 (52-69) | 78 (74-79) | 78.5 (78-79) |
| Cruiser | 0 | 8 (8-8) | 8 (8-8) | 8 (8-8) | 8 (8-8) |
| Cruiser | 3 | 12 (12-12) | 12 (12-12) | 12 (12-12) | 12 (12-12) |
| Cruiser | 6 | 18 (18-18) | 18 (18-18) | 18 (18-18) | 18 (18-18) |
| Cruiser | 10 | 32 (31-32) | 32 (31-32) | 32 (31-32) | 36 (34-38) |
| Cruiser | 13 | 38 (37-38) | 38 (37-38) | 38 (37-38) | 39 (38-41) |

Fuel-reach against fight-reach, with both tiers allowed. A fight-reach of Nx is the
farthest ring a plan holds when it only enters sectors whose lair group (the
engine's `Ships::of(class, count).power(Arms::of_grade(multiplier))`, the group
the template rolls) is at most N times the fleet's own power. "Lairs on route" and
"strongest on route" are for the unguarded plan, passive groups included:

| class | level | fleet power | fuel ring | fight 1x | fight 2x | fight 4x | lairs on route | strongest, x fleet |
|---|---|---|---|---|---|---|---|---|
| Light Fighter | 0 | 910 | 20 (19-27) | 18 (17-20) | 20 (19-22) | 20 (19-23) | 9 (8-13) | 1.92 (max 7.73) |
| Light Fighter | 3 | 1183 | 50 (49-50) | 22 (20-24) | 24 (23-25) | 27 (27-28) | 37.5 (33-44) | 56.97 |
| Light Fighter | 6 | 1456 | 57 (57-57) | 22 (22-24) | 24 (23-25) | 29 (28-30) | 48.5 (39-67) | 46.29 |
| Light Fighter | 10 | 1820 | 72 (72-74) | 23 (22-25) | 25.5 (25-27) | 29 (29-32) | 63 (53-86) | 37.03 |
| Light Fighter | 13 | 2093 | 78.5 (78-79) | 23 (22-25) | 26 (26-27) | 29 (29-32) | 66.5 (58-79) | 32.20 |
| Cruiser | 0 | 6750 | 8 (8-8) | 8 (8-8) | 8 (8-8) | 8 (8-8) | 1 (0-2) | 0.02 |
| Cruiser | 3 | 8775 | 12 (12-12) | 12 (12-12) | 12 (12-12) | 12 (12-12) | 3.5 (1-5) | 0.04 |
| Cruiser | 6 | 10800 | 18 (18-18) | 18 (18-18) | 18 (18-18) | 18 (18-18) | 6.5 (3-9) | 0.10 (max 0.11) |
| Cruiser | 10 | 13500 | 36 (34-38) | 32.5 (31-34) | 35 (34-36) | 36 (34-38) | 17 (13-23) | 1.41 |
| Cruiser | 13 | 15525 | 39 (38-41) | 33 (32-34) | 36 (36-37) | 39 (38-41) | 24 (20-31) | 4.04 |

Cloud distance from the ring-4 home, in lane hops, with a Light Fighter's tank at
levels 0, 3 and 6 (10, 20 and 35 jumps):

| seed | nearest cloud | nearest deuterium Moderate+ | nearest lair |
|---|---|---|---|
| 42 | 8 | 1 | 2 |
| 7 | 7 | 1 | 1 |
| 101 | 6 | 1 | 1 |
| 2024 | 4 | 2 | 1 |
| 31337 | 13 | 1 | 1 |
| 555 | 6 | 2 | 1 |
| 9001 | 10 | 2 | 1 |
| 1234 | 9 | 1 | 1 |

Medians: cloud 7.5 hops, Moderate-or-better deuterium 1, lair 1. A cloud lies within
a Light Fighter's full tank of home in 7 of 8 seeds at level 0 and 8 of 8 at 3 and
6, and within a round trip on one tank in 1, 7 and 8 of 8.

Reading:

- Seed 42 was typical. Clouds change nothing at levels 0 to 6 in any of the eight
  seeds (min equals max), for the Light Fighter and the Cruiser alike. The cloud
  nearest home is 4 to 13 hops away, a round trip beyond a level-0 tank, and the
  fuel to reach it is the fuel the cloud would give back. Clouds start to matter at
  level 10 (Light Fighter 39 to 56.5, with a wide spread of 45 to 61 by seed) and
  not at all for a Cruiser until both tiers stack. The cloud tier is too thin where
  a new commander lives and is seed-dependent where it works.
- The relay tier is far too generous for a light fleet and does nothing for a
  heavy one. A Light Fighter at level 3 (two relays) goes from ring 14 to ring 49;
  at level 6, ring 57, which is deep Wandering for a fleet that cannot pass a
  group there. Even at level 0 (one relay) it doubles the reach. The Cruiser's
  fleet burns more per jump than the flat store holds, and the same relays add
  nothing at any level. The store is the same for ten ships and for a hundred, so
  it scales against the fleet that needs it least.
- Danger gates the light fleet, not the fuel. A Light Fighter fleet of ten at 1x to
  4x its own power reaches ring 18 to 29 whatever its level, while its fuel reaches
  50 to 78; the route past ring 21 holds groups 30 to 57 times its power. The
  Wandering is guarded, not empty, for a fleet that is not built for it. A Cruiser
  fleet's fuel and fight reach agree below level 10 (it does not out-range its
  fuel), and meets groups of its own weight only from level 10, 1.4x, to 4x at
  level 13.
- Fight-reach moves little with research (Light Fighter 18 to 23 at 1x across all
  levels): power per ring grows faster than ten ships' power does, so the guard,
  not the tank, sets how deep a small fleet goes.

Recommendations (nothing is changed here):

- `RELAY_STORE`: make it per ship or per ring rather than flat, or lower it so a
  Light Fighter fleet of ten at level 3 stops at ring 25 or so; a store that
  scales with fleet size would also let a Cruiser fleet use the tier. Keep
  `RELAY_LEVELS_PER_STEP` at 2 only if the store is cut: levels 3 to 6 are where
  the tier jumps from ring 14 to ring 49. Raising it to 3 delays the second and
  third relay to level 6 and 9 and is the smaller lever.
- `CLOUD_PCT` and `CLOUD_DEUT_PEAK`: the nearest cloud should sit within about half
  a level-0 tank (5 hops) in most seeds. At 25 percent the median is 7.5 hops with
  a worst seed at 13; raising `CLOUD_PCT` to 35 to 40 should bring the median
  near 5. A cloud must pay more than the way to it costs: raise the Sparse peak of
  the rings below 8 to Moderate, or the clouds will go on doing nothing at levels
  0 to 6.
- Re-run the probe after either change and watch the Light Fighter at level 3 to 6
  (a ring-20 to ring-30 target) and the Cruiser at levels 0 to 6, which should
  leave their fuel rings and gain from relays.

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
- How many hubs a world holds and whether two may stand within a ring.
- Whether deeper relic tiers exist at the very edge of the generated map or are
  bounded by ring 60 (where edge survival bottoms out).
