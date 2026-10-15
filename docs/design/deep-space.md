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

A relay is a mini outpost: a small store, `load` and `unload` of a fleet's hold
into it, and a refuel of a fleet standing there from that store. It has no
extractor, no platform, no repair, no raid. It is owner-only. The fuel in it is
what its owner hauled, so reach is logistics and not a gift.

- Founded from what a fleet carries there, on any sector of ring 9 and beyond
  (the Wandering included) that has no hub, lair or derelict.
- Limit: `RELAY_MAX` active relays per empire, *decided* about three, raised by a
  research level (a Hyperspace-line technology) rather than a building, so reach
  grows with research.
- Cost grows with the ring (`RELAY_COST_BASE`, `RELAY_COST_GROWTH`), and the
  founding load is carried, as the outpost's is.
- Store: `RELAY_STORE` units, not a pace-scaled cap, so a distant relay is a
  small, deliberate depot.
- Cheap to lose: nothing raids it, but a fleet that is not there is no defence;
  a relay left alone for `RELAY_DECAY_TICKS` (an `Attention` timer) is gone.
- Standing orders, bots and `recall` do not use relays.

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
