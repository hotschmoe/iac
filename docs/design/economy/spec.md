# In Amber Clad: economy, pacing and exploration spec

Status: proposal for owner review. Implements the 2026-10-09 owner decisions (`docs/design/decisions.md`, items 1 to 5) and the proposals in `docs/design/gap-analysis-2026-10-09.md` sections 3 and 4. No game code was changed. Every number in this document is exercised by `sim.py` (same directory), and the simulation output is pasted at the end (section 16).

How to read it: section 1 is the pacing maths, which drives everything else. Sections 2 to 9 are the design, each with old to new values. Sections 10 to 14 are the implementation map, protocol, migration and commit order. Section 15 lists the hooks for chat, trade, PvE and PvP. Section 17 holds the open questions.

---

## 0. Summary

* **One flag, `pace` (P).** Economy timers divide by P, production multiplies by P, costs never change. The baseline (P = 1) is the persistent world. Because costs grow exponentially, the whole arc of the game is long: a competent casual player (four 12-minute sessions a day) reaches the first Cruiser on about day 16 and the full "endgame kit" on about day 74.
* **The owner's 2x and 5x guesses are far too small.** A weekly season needs about x10, a 24 hour sprint about x70 to x75, a one-hour blitz about x500 to x600, and a test setting that plays the whole arc in 10 to 15 minutes about x7000 to x8000 (it saturates there because every queue item takes at least one 1 Hz tick). The owner's x100 plays the whole arc in about 13 hours. Section 1.
* **Buildings are the economy; the piloting bonus is small and grows with distance.** In the simulation a competent player gets about 15 to 17 percent of gross income from mining, salvage and derelicts. A player who never leaves home (buildings only) finishes the endgame kit only 13 percent later than the competent player. A greedy far-miner finishes 18 percent sooner than the competent player, with 36 percent of his gross income coming from piloting (against 15 percent) and about 73k resource units of ship losses along the way. Section 16.
* **The "costs linear, times exponential" shape is fixed.** Costs and times now share the same exponent (1.5 to 1.9 per level), and build time is derived from cost (`time = (metal + crystal) / rate`), so resources and time gate progress together.
* **New systems:** Storage Vault (caps, pace-scaled), a second and third building slot (research), queue depth 3, Fabricator, defence structures with a reworked raid model, a smooth NPC power gradient with a visible 1 to 9 threat rating, ore that regenerates by time and is richer with distance, distance-scaled loot and discoveries, refuel cost, and a score that is hard to game.

---

## 1. Pacing

### 1.1 The maths

Let T(m) be the real time a given player needs to reach milestone m at pace 1. Under a pace P every economy duration becomes T(m) / P (production x P, build time / P, costs unchanged), so the arc compresses uniformly. For a target arc the required pace is:

    P = T_persistent(milestone) / T_target(milestone)

The persistent run in `sim.py` gives (competent player, median of three session schedules): first Cruiser at 15.8 days, endgame kit at 73.9 days. Therefore:

| world you want | P | first Cruiser | endgame kit |
|---|---|---|---|
| persistent | x1 | 15.8 d | 73.9 d |
| owner's weekly guess "about 2x" | x2 | 7.9 d | 37 d (a month-long season, not a week) |
| owner's blitz guess "about 5x" | x5 | 3.2 d | 14.8 d (a fortnight, not a blitz) |
| season, endgame in 7 days | x10 to x11 | 36 to 46 h | 7 d |
| 3 day weekend | x25 | 15 h | 3 d |
| 24 hour sprint | x74 | 5 h | 24 h |
| owner's testing "about 100x" | x100 | 3.8 h | about 13 h (not minutes) |
| blitz: first Cruiser at minute 45 | x500 to x600 | 42 to 45 min | about 3 h |
| test: whole arc in 10 to 15 min | x6000 to x8000 | 3 to 4 min | 12 min |

Why the guesses were low: a one-hour game is 1/1770 of a 74 day arc, and the late levels cost 1.5^L, so the late half of the arc is where the days go. The old balance (one hour is about 13 building levels, Cruiser unreachable) is itself a blitz-class pace; it only looked like "x1" because nothing in the game was tuned for longer worlds.

### 1.2 Presets (named multipliers; any positive number is accepted)

`--pace <number>` or `--world <preset>` (env `IAC_PACE`). Stored in the world database at creation. Allowed range 0.05 to 20000.

| preset | P | intent | what a competent player sees (from the simulation) |
|---|---|---|---|
| `persistent` | 1 | months-long shared world | Corvette day 3.5, Shipyard 4 day 6.6, build slot 2 day 11, Cruiser day 16, endgame day 74 |
| `fortnight` | 5 | two-week season | endgame about day 15 |
| `season` | 10 | weekly season (reset every 7 days) | Corvette 7 h, Shipyard 4 20 h, slot 2 29 h, Cruiser 46 h, endgame day 7.2 |
| `sprint` | 70 | 24 hour world | Cruiser 5 h, endgame about 24 h |
| `dev` | 100 | owner's "testing" setting; day-scale features | Cruiser 3.8 h, endgame 13 h |
| `blitz` | 600 | one hour of play | Corvette 9 min, Lab 2 3 min, Shipyard 4 19 min, slot 2 27 min, Cruiser 42 min, tier-2 derelict loot 19 min, endgame kit about 3 h |
| `test` | 8000 | smoke and regression tests | Cruiser 4 min, endgame kit 12 min |

Why `test` stops near x8000: a build or research item cannot take less than one tick, and the endgame kit is about 200 sequential items plus ships. Beyond roughly x10000 nothing gets faster. If a CI run needs less, add a separate `--instant-queues` test flag rather than a bigger pace.

### 1.3 What scales, and by how much

Four classes. The exponent is the power of P applied.

| class | exponent | quantities | reason |
|---|---|---|---|
| Economy | 1 | mine production (x P); building, research and ship build times (/ P, minimum 1 tick); ore regeneration time; NPC respawn delay; dock repair rate (if added) | These are the "economy timers" of decision 1. Costs do not scale, so everything is a pure time compression. |
| Attention | 0.75 | storage caps (x P^0.75); raid roll interval, minimum interval, minimum player age, suppress-after-loss (/ P^0.75) | A cap or raid cadence must respect how often a human or agent can look at the game. Linear scaling would make caps trivial at x600 and raids a continuous drizzle; no scaling would make a persistent cap ruinous. |
| Finds | 0.5 | loot value of kills and derelicts (x P^0.5); derelict respawn (/ P^0.5); raid warning (600 s / P^0.5, clamped 30 to 600 s) | Piloting income is limited by human action rate, not by economy time. Scaling it by P^0.5 holds the pilot share of income at 15 to 17 percent in the persistent, season and blitz presets (11 percent at x8000, 27 percent at x100; section 16); scaling by P would make piloting 600 times more valuable in a blitz, scaling by 1 would make it irrelevant. |
| Real time | 0 | move, harvest and scan cooldowns; combat rounds and damage; shield regen idle time; fuel capacity, fuel burn and fuel price; scan reveal duration (120 s); boarding time (20 s); salvage despawn (60 s); policy evaluation interval; NPC patrol interval; emergency recall; costs of everything | These are the piloting layer the owner wants to stay human-scale (decision 1), or they are not timers at all (costs, fuel). |

Rounding: `ticks = max(1, ceil(base_ticks / P))`. A lever that is easy to forget: the pace changes only timers; any new timer added later must be assigned a class in `PaceClass` (section 11) so reviewers see the choice.

### 1.4 Server API

```rust
pub struct Pace(pub f64);                   // validated 0.05..=20000
impl Pace {
    pub fn econ_ticks(&self, base: f64) -> u64     { ((base / self.0).ceil() as u64).max(1) }
    pub fn rate(&self, per_tick: f32) -> f32       { per_tick * self.0 as f32 }
    pub fn attention_ticks(&self, base: f64) -> u64{ ((base / self.0.powf(0.75)).ceil() as u64).max(1) }
    pub fn cap_mult(&self) -> f32                  { self.0.powf(0.75) as f32 }
    pub fn finds_mult(&self) -> f32                { self.0.sqrt() as f32 }
    pub fn raid_warning_ticks(&self) -> u64        { (600.0 / self.0.sqrt()).clamp(30.0, 600.0) as u64 }
}
```

`full_state` carries the pace so clients can show "x10 world, 2d 4h to endgame estimate" (section 12).

---

## 2. Core economy

### 2.1 Production (building-first)

`production_per_game_hour(b, L) = base_b * L * 1.1^L`, applied per tick as `base_b / 3600 * L * 1.1^L * P`.

| | old (per tick, at pace "1") | new (per hour, pace 1) | per tick at P = 1 | reason |
|---|---|---|---|---|
| Metal | 0.50 | 10 | 0.00278 | persistent baseline, see 1.1 |
| Crystal | 0.30 | 7 | 0.00194 | keeps old 0.6 ratio to metal, nudged up |
| Deuterium | 0.15 | 5 | 0.00139 | ratio to metal 0.30 to 0.50 (deuterium was the bottleneck for four of six playtesters) |

For reference, old metal production per tick is 180 times the new per-hour figure (0.5 per tick is 1800 per hour), while the old Metal L2 build time (135 s) is only 4.3 times shorter than the new one (576 s): the old game was a very fast economy with comparatively slow queues, and had no storage, no exponential costs and no distance-scaled bonus, so there is no exact equivalent pace. Metal L1 gives 11 per hour, L5 81, L10 259, L15 627, L20 1345 (pace 1).

### 2.2 Costs: exponential, same exponent as time

`building_cost(b, L) = base_b * growth_b^(L-1)`. L1 costs equal the old L1 costs except the Deuterium Synthesizer (225/75 down to 150/50, from gap analysis section 4).

| building | L1 cost m/c/d | growth per level | old | prerequisite | new? |
|---|---|---|---|---|---|
| Metal Mine | 60/15/0 | 1.5 | n x base | none | |
| Crystal Mine | 48/24/0 | 1.6 | n x base | none | |
| Deut Synthesizer | 150/50/0 | 1.5 | n x 225/75 | none | cheaper |
| Shipyard | 200/100/50 | 1.8 | n x base | Metal 2 | |
| Research Lab | 100/200/50 | 1.8 | n x base | Crystal 2 | |
| Fuel Depot | 150/50/100 | 1.7 | n x base | Deut 2 | |
| Sensor Array | 100/150/75 | 1.7 | n x base | Lab 1 | |
| Defense Grid | 300/200/100 | 1.8 | n x base | Shipyard 3 | |
| **Storage Vault** | 200/100/0 | 1.7 | | Metal 2 | new |
| **Fabricator** | 120/80/40 | 1.9 | | Shipyard 2 | new |

Max level stays 20 (open question 3). Example costs at pace 1 are in the table after 2.3.

### 2.3 Times: derived from cost

`building_time_hours = (metal + crystal) / 700 / (1 + 0.15 * fabricator)` then `/ P`, minimum one tick.
`research_time_hours = (metal + crystal) / 500 / (1 + 0.10 * lab)` then `/ P`. (Old: research time ignored the lab.)
`ship_time_hours = (metal + crystal) / 1200 / (1 + 0.10 * shipyard)` per ship, then `/ P`. (Old: fixed 30/60/120/240/90 s for Scout/Corvette/Frigate/Cruiser/Hauler.)

Old time was `base * L * 1.5^L` ticks, cost `n * base`. At L10 the old time was 17,300 ticks for a mine whose cost had grown only 10x; the new time and cost both grow 1.5^L, so the resource wait and the build wait stay comparable (the simulation shows slots 2 and 3 together shorten the endgame by about 14 percent: 86 days with one slot, 74 with three).

The generated tables below are the reference values for unit tests.

Generated reference tables (`python3 sim.py --tables`); the first one doubles as the table for section 7.

#### Threat, NPC power, kill loot, derelicts and ore by distance (pace-1 units; multiply loot by P^0.5 at other paces)

| dist | zone | threat | NPC group (class x count) | NPC power | old NPC power | kill loot | derelict tier / loot | ore reserve per ore tile (any) | ore refill (game h) | NPC respawn (game h) |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 | inner | T1 | scout x1 (x0.62 stats) | 4 | 5 | 4 | 1 / 162 | 13 | 144 | 2.2 |
| 2 | inner | T1 | scout x1 (x0.64 stats) | 5 | 5 | 4 | 1 / 185 | 14 | 168 | 2.5 |
| 4 | inner | T1 | scout x1 (x0.68 stats) | 7 | 5 | 6 | 1 / 240 | 16 | 216 | 3.0 |
| 6 | inner | T2 | corvette x1 (x0.72 stats) | 11 | 5 | 8 | 1 / 312 | 20 | 264 | 3.5 |
| 8 | inner | T3 | corvette x1 (x0.76 stats) | 16 | 5 | 11 | 1 / 405 | 26 | 312 | 4.0 |
| 9 | outer | T3 | corvette x1 (x0.78 stats) | 20 | 97 | 13 | 1 / 462 | 29 | 336 | 4.2 |
| 10 | outer | T3 | corvette x1 (x0.80 stats) | 24 | 97 | 15 | 1 / 527 | 33 | 360 | 4.5 |
| 12 | outer | T4 | corvette x2 (x0.84 stats) | 36 | 97 | 21 | 2 / 685 | 42 | 408 | 5.0 |
| 15 | outer | T5 | frigate x1 (x0.90 stats) | 65 | 97 | 34 | 2 / 1014 | 59 | 480 | 5.8 |
| 18 | outer | T5 | frigate x3 (x0.96 stats) | 118 | 480 | 54 | 2 / 1503 | 84 | 552 | 6.5 |
| 20 | outer | T6 | frigate x4 (x1.00 stats) | 175 | 480 | 75 | 2 / 1953 | 105 | 600 | 7.0 |
| 21 | wander | T6 | cruiser x2 (x1.02 stats) | 213 | 480 | 88 | 2 / 2226 | 107 | 624 | 7.2 |
| 25 | wander | T7 | cruiser x4 (x1.10 stats) | 473 | 480 | 166 | 3 / 3760 | 115 | 720 | 8.2 |
| 30 | wander | T9 | cruiser x10 (x1.20 stats) | 1278 | 2640 | 367 | 3 / 7240 | 115 | 840 | 9.5 |
| 35 | wander | T9 | cruiser x24 (x1.30 stats) | 3454 | 2640 | 812 | 3 / 13940 | 115 | 960 | 10.8 |

#### Building cost / time / production by level (pace 1, no Fabricator)

| L | Metal prod /h | Metal cost (m/c) | Metal time | Crystal prod /h | Deut prod /h | Shipyard cost (m/c/d) | Shipyard time | Lab time |
|---|---|---|---|---|---|---|---|---|
| 1 | 11 | 60/15 | 6m | 8 | 6 | 200/100/50 | 26m | 26m |
| 2 | 24 | 90/22 | 10m | 17 | 12 | 360/180/90 | 46m | 46m |
| 3 | 40 | 135/34 | 14m | 28 | 20 | 648/324/162 | 83m | 83m |
| 5 | 81 | 304/76 | 33m | 56 | 40 | 2100/1050/525 | 4.5h | 4.5h |
| 8 | 171 | 1025/256 | 1.8h | 120 | 86 | 12244/6122/3061 | 26.2h | 26.2h |
| 10 | 259 | 2307/577 | 4.1h | 182 | 130 | 39672/19836/9918 | 3.5d | 3.5d |
| 12 | 377 | 5190/1297 | 9.3h | 264 | 188 | 128537/64268/32134 | 11.5d | 11.5d |
| 15 | 627 | 17516/4379 | 31.3h | 439 | 313 | 749627/374813/187407 | 66.9d | 66.9d |
| 18 | 1001 | 59116/14779 | 4.4d | 701 | 500 | 4371823/2185912/1092956 | 390.3d | 390.3d |
| 20 | 1345 | 133010/33253 | 9.9d | 942 | 673 | 14164707/7082353/3541177 | 1264.7d | 1264.7d |

#### Real time to build selected items (no Fabricator / Lab / Shipyard bonus), by pace

| item | cost (m/c/d) | persistent x1 | season x10 | day x70 | blitz x600 | test x8000 |
|---|---|---|---|---|---|---|
| metal L2 | 90/22/0 | 10m | 58s | 8s | 1s | 1s |
| metal L5 | 304/76/0 | 33m | 3m | 28s | 3s | 1s |
| metal L10 | 2307/577/0 | 4.1h | 25m | 4m | 25s | 2s |
| metal L15 | 17516/4379/0 | 31.3h | 3.1h | 27m | 3m | 14s |
| shipyard L4 | 1166/583/292 | 2.5h | 15m | 2m | 15s | 1s |
| shipyard L6 | 3779/1890/945 | 8.1h | 49m | 7m | 49s | 4s |
| lab L8 | 6122/12244/3061 | 26.2h | 2.6h | 22m | 3m | 12s |
| corvette tech | 400/200/100 | 72m | 7m | 62s | 7s | 1s |
| frigate tech | 2000/1200/600 | 6.4h | 38m | 5m | 38s | 3s |
| cruiser tech | 8000/5000/2500 | 26.0h | 2.6h | 22m | 3m | 12s |
| corvette (one ship) | 400/100/60 | 25m | 2m | 21s | 2s | 1s |
| cruiser (one ship) | 3000/1500/800 | 3.8h | 22m | 3m | 22s | 2s |

#### Storage caps (metal / crystal / deut) by Vault level and pace

| Vault level | cost (m/c) | pace 1 | pace 10 | pace 600 |
|---|---|---|---|---|
| 0 | - | 5000 / 3500 / 2500 | 28117 / 19681 / 14058 | 606154 / 424308 / 303077 |
| 1 | 200/100 | 7500 / 5250 / 3750 | 42175 / 29522 / 21087 | 909231 / 636462 / 454615 |
| 2 | 340/170 | 11250 / 7875 / 5625 | 63263 / 44284 / 31631 | 1363847 / 954693 / 681923 |
| 3 | 577/288 | 16875 / 11812 / 8437 | 94895 / 66426 / 47447 | 2045771 / 1432040 / 1022885 |
| 4 | 982/491 | 25312 / 17718 / 12656 | 142342 / 99639 / 71171 | 3068657 / 2148060 / 1534328 |
| 5 | 1670/835 | 37968 / 26578 / 18984 | 213513 / 149459 / 106756 | 4602986 / 3222090 / 2301493 |
| 6 | 2839/1419 | 56953 / 39867 / 28476 | 320270 / 224189 / 160135 | 6904480 / 4833136 / 3452240 |
| 8 | 8206/4103 | 128144 / 89701 / 64072 | 720609 / 504426 / 360304 | 15535080 / 10874556 / 7767540 |
| 10 | 23717/11858 | 288325 / 201827 / 144162 | 1621371 / 1134960 / 810685 | 34953931 / 24467752 / 17476965 |
| 14 | 198091/99045 | 1459646 / 1021752 / 729823 | 8208194 / 5745736 / 4104097 | 176954278 / 123867995 / 88477139 |

#### Defence structures

| structure | cost (m/c/d) | power | res per power | requires |
|---|---|---|---|---|
| turret | 120/20/0 | 17 | 8.2 | grid 1 |
| lancer | 400/120/20 | 51 | 10.6 | grid 2, weapons 2 |
| bastion | 1200/500/100 | 135 | 13.3 | grid 4, weapons 4, shields 3 |
| (ship) scout | 200/50/30 | 9 | 31.1 | |
| (ship) corvette | 400/100/60 | 22 | 25.5 | |
| (ship) frigate | 1000/400/200 | 48 | 33.3 | |
| (ship) cruiser | 3000/1500/800 | 110 | 48.2 | |

Raid power by econ points S: S=1: 16, S=10: 67, S=100: 353, S=500: 1168, S=1000: 1961, S=2000: 3295



### 2.4 Research and ships: changed values

| item | old | new | reason |
|---|---|---|---|
| Leveled techs (Fuel Eff, Tanks, Hulls, Shields, Weapons, Nav, Harvest) | n x base | base x 1.6^(n-1), same L1 | same shape fix |
| Frigate Tech | 1000/600/300 | 2000/1200/600 | Cruiser lands on day 16 / minute 42, not day 4 |
| Cruiser Tech | 3000/2000/1000 | 8000/5000/2500 | same |
| Corvette Tech, Hauler Tech | 400/200/100, 600/300/200 | unchanged | early fun |
| Ship costs | Scout 200/50/30, Corvette 400/100/60, Frigate 1000/400/200, Cruiser 3000/1500/800, Hauler 600/200/150 | unchanged | cost per power point (31/25/33/48) is already monotone |
| Starting resources | 500/300/100 | unchanged | first day goals stay at mine 2 to 4 and Lab 1 |
| Research prerequisites | as today | as today, plus ModularFabrication needs Lab 4 | |

### 2.5 Deuterium and fuel

* Production ratio and Synthesizer cost as above.
* **Refuel cost:** docking tops up fuel at `0.08 deuterium per fuel unit` (new constant `FUEL_DEUT_PER_UNIT`). Not scaled by pace. A two-Scout fleet full refill is 9.6 deuterium; a 13-Cruiser fleet 125. The simulation charges it on every sortie. It eats 27 percent of gross pilot income in the persistent world, 8 percent in the season and about 1 percent in the blitz (fuel price does not scale, loot does), so it is a real deuterium sink in slow worlds and just the cost of range in fast ones (open question 5).
* **Fuel Depot:** capacity +25 percent per level (old +10 percent; Pi-1 measured +10 percent as marginal). `fuel_depot_modifier(L) = 1 + 0.25 L`.
* Unchanged: `FUEL_RATE_PER_MASS = 0.1`, Extended Tanks +15 percent, Fuel Efficiency -10 percent per level.
* Range matters for exploration: round-trip range in hops is `fuel / (0.1 * mass)`. A Cruiser has 120 fuel and burns 20 per hop, so an unmodified Cruiser fleet reaches only 3 hops. Fuel techs and the Depot are therefore the real gate to the outer ring for heavy fleets, not just a deuterium sink. Clients must show "hops of range" (section 12).

---

## 3. Storage

### 3.1 Caps

New building **Storage Vault**. `cap_r(V) = base_r * 1.5^V * P^0.75` with `base = 5000 metal, 3500 crystal, 2500 deuterium`. Vault level 0 still has the base cap (a fresh world is capped; no unbounded stockpiles anywhere). Cap table by Vault level and pace is in the generated tables above.

Why 5000: the largest single payment before the Vault is affordable is a Cruiser Tech (8000 metal); the engine refuses to start a queue item whose cost exceeds the cap and tells the player "needs Storage Vault level N" (error `StorageTooSmall`). The simulation's competent player builds Vault 1 by day 11 (persistent) and Vault 6 to 8 by the end.

### 3.2 What happens at the cap

* **Mines:** production above the cap is discarded. The tick still adds `min(production, cap - stock)`. UI shows `FULL`. Nothing is refunded.
* **Dock deposits** (cargo, salvage, derelict loot): deposit up to the cap; the remainder stays in the fleet hold (never destroyed). The fleet shows `cargo_blocked_by_storage`. Not a trap: the player can spend and re-dock.
* **Alerts:** `StorageNearCap` at 85 percent with a time-to-full estimate, `StorageFull` when reached.
* **Raids and later PvP:** a stockpile is only partly safe. `protected_r = min(0.6, 0.08 * V) * cap_r` is exempt from skimming. This gives the Vault a defensive role, matching OGame, and makes "spend it or lose it" a real choice (decision 2 and the "if it sits, it gets hit" pillar).
* **Pressure in the simulation (competent player):** persistent 4.8 percent of time above 90 percent of cap and 0.9 percent of production wasted; season 6.7 and 1.5 percent; blitz and test essentially zero. 

### 3.3 Why the exponent is 0.75

Linear scaling (P) makes caps meaningless in a blitz and a season (zero pressure in the simulation). Exponent 0.5 wasted 12.8 percent of a season player's production in an earlier run (the Vault could not keep up with x10 income between sessions). 0.75 gives 0.9 percent waste for the persistent player and 1.5 percent for the season player, with 5 to 7 percent of the time above 90 percent of the cap, so the cap is felt without being a tax.

---

## 4. Build slots, queue depth, Fabricator, Lab

* **Queue depth 3** for the building, research and ship queues (active item included). Queued items do not consume resources. When a slot frees, the engine starts the first queued item whose prerequisites are met and which is affordable (a blocked item does not block the ones behind it). A blocked front item shows "waiting for 1240 metal". Cancelling a queued (not started) item refunds 100 percent; cancelling an active item keeps the 50 percent refund.
* **Second building slot, unlocked by research.** New tech **Modular Fabrication** (`ResearchType::ModularFabrication`, max level 2). Cost 1500/1200/500 x 2^(L-1), prerequisite Research Lab 4. Level 1 gives building slot 2, level 2 gives slot 3. Research and shipyard remain single-slot. `building_slots = 1 + modfab_level`. Simulation: slot 2 on day 11 (persistent), hour 29 (season), minute 27 (blitz). With queue-ahead and skip-blocked-item starts the slots are a modest lever (14 percent on the endgame, 8 percent on the Cruiser date); if playtests want a stronger unlock, lower `BUILD_RATE` (longer builds) or raise the build-time exponent rather than adding slots.
* **Fabricator** building: building time / (1 + 0.15 L), applies to building items only. Cost above, prerequisite Shipyard 2, max level 20 (cap in play about 8 to 10).
* **Research Lab** now divides research time by (1 + 0.10 L).

Both slot 2 and the Fabricator attack the same dead time. The second slot is the bigger lever for mid-game, the Fabricator for the long tail. A third slot is deliberately an endgame buy (cost 3000/2400/1000, needs Lab 4).

---

## 5. Defences and raids

### 5.1 Structures (OGame style, alongside the Defense Grid)

Built from the shipyard queue as quantities, listed in the catalog as `defences`. They do not move, burn no fuel, and are rebuilt by raids (see below).

| structure | cost m/c/d | power | cost per power | requires |
|---|---|---|---|---|
| Pulse Turret | 120/20/0 | 17 | 8.2 | Defense Grid 1 |
| Lancer Battery | 400/120/20 | 51 | 10.6 | Grid 2, Weapons 2 |
| Ion Bastion | 1200/500/100 | 135 | 13.3 | Grid 4, Weapons 4, Shields 3 |
| Aegis Dome (one per world, not simulated) | 2500/2500/500 | absorbs the first 600 damage | | Grid 3, Shields 2 |

"Power" is the same `weapon + (hull + shield) / 10` used for ships so raids, scans and the threat display share one scale. Ships cost 25 to 48 per power point, defences 8 to 13, because defences cannot leave home.

**Defense Grid** stays, as the prerequisite and amplifier: structure stats +4 percent per Grid level, and its virtual platforms are halved (1.0, 2.5, 4.0 scout units for L1 to L3, then x1.3 per level; old 2, 5, 8, x1.35) because real structures now provide the bulk.

`home_defense_power = docked fleet power + grid virtual power + sum(structure power) * (1 + 0.04 * grid)`.

### 5.2 Raid model (replaces "35 to 55 percent of your own defence")

Today a raid is sized from the player's own defence, so it can never be an actual threat and building no defence at all (Grok-1) is rational. New:

* `raid_power = (5 + 11 * S^0.75) * U(0.80, 1.15)`, with `S` = the player's building and research points (section 9, excludes fleets and defences). S = 10 gives 67, S = 100 gives 353, S = 500 gives 1168, S = 1000 gives 1961 (about 15 Ion Bastions, roughly 26k resource units, a day or two of late-game income).
* Timers (attention class, divided by P^0.75): roll every 21,600 s with 30 percent chance (old 300 s, 20 percent); minimum interval 64,800 s (old 900); minimum player age 86,400 s (old 600); suppress after a lost raid 172,800 s (old 1800). Raid warning `600 / sqrt(P)` s, clamped 30 to 600 (old fixed 60). Expected spacing between raids (minimum interval plus the wait for a successful roll): about 38 hours at x1, 6.8 hours at x10 (24 per 7-day season), 19 minutes at x600 (about three in a blitz hour, the first after 12 minutes). Only for a player with a Shipyard 2 or a Grid.
* Resolution as today (±10 percent fog on defence power). **Repelled:** salvage 30 percent of raid value drops (unchanged); structures take damage `min(1, raid_power / defence_power) * 50 percent` of the structure count and **70 percent of destroyed structures are rebuilt for free** after 10 minutes / sqrt(P). **Lost:** stockpile skim of 8/8/5 percent applies only above the protected amount, 50 percent of structures are destroyed with no rebuild, raids suppressed as above.
* Eligibility unchanged (Shipyard 2 or Grid 1); the competent simulated player keeps `defence >= 1.15 x next raid` and spends about 2 percent of lifetime spend on it.

### 5.3 PvP hooks

The same defence stack is what a player attack would face. Needed later: attack costs the attacker fuel and exposes his fleet; protected storage and structure restoration (70 percent) cap what one attack can take; points for defence count 50 percent in the score (section 9) so farming defensive players is not profitable. Newbie protection and opt-in rules are open question 7.

---

## 6. Ore: building-first, richer with distance, regenerating

Principle: the home mines are the income; ore tiles are a bonus whose size grows with distance and which refills on a timer.

### 6.1 Tile reserve

A tile's reserve is the density chain. New per-step thresholds (units that must be harvested to drop one density level): Sparse 3, Moderate 7, Rich 12, Pristine 20, i.e. total reserves **3 / 10 / 22 / 42** (old 10/20/30/40 per step, i.e. 10/30/60/100 total; tiles are deliberately small nuggets: 42 units is about four hours of a level-1 metal mine, and the bonus comes from the number of tiles in reach and from the ring factor, not from any single tile). All thresholds are multiplied by a ring factor:

`ring_mult(d) = 1.10^(min(d, 20) - 3)` for d >= 3 (1.0 below). d = 8 gives 1.6x, d = 12 gives 2.4x, d = 15 gives 3.1x, d = 20 and beyond 5.05x.

**Richness drift:** the density roll shifts toward Rich and Pristine with distance. `delta = min(0.30, 0.012 d)`; probabilities Sparse `0.40 - delta`, Moderate `0.30 - 0.2 delta`, Rich `0.20 + 0.6 delta`, Pristine `0.10 + 0.6 delta` (old: fixed 40/30/20/10). Ore presence chances per terrain stay as today (zone boost 30/40/50).

### 6.2 Harvest and refine

Harvest rate per tick per ship is unchanged (Hauler 5, Scout 1, others 0.5, times density multiplier). Depletion counts raw units extracted. Harvesting Efficiency now raises the **yield** (cargo gets `raw * (1 + 0.2 level)`), not the extraction speed, so a research level does not strip tiles faster. (With tiny tile reserves, speed upgrades would only deplete tiles.)

### 6.3 Regeneration (replaces `SECTOR_REGEN_RATE = 0.0001`)

A depleted tile refills at `threshold_of_current_step * ring_mult * P / (regen_hours(d) * 3600)` per tick, where

`regen_hours(d) = 120 + 24 d` game hours (a stripped tile refills completely in 5 days + 1 day per ring at pace 1; d = 8 takes 312 h, d = 20 takes 600 h).

Regeneration pauses while a fleet is on the tile (as today). Tiles never regenerate above their generated template density. The simulation's mining income is therefore supply-limited: a tile can only be milked as fast as it refills, which is what keeps mining a bonus no matter how many haulers a player owns.

### 6.4 What it yields (persistent, per real hour; from the simulation)

| ring | threat | ore per hour, competent | ore per hour, greedy | all pilot income in the ring vs building income (greedy, day 15 to 29) |
|---|---|---|---|---|
| 3 to 6 | T1 to T2 | 7 to 10 | 6 to 10 | 1 to 2 percent |
| 7 to 8 | T2 to T3 | 6 to 9 | 6 to 9 | 1 to 2 percent |
| 9 to 14 | T3 to T4 | 0 | 9 to 22 | 2 to 10 percent |
| 15 to 20 | T5 to T6 | 0 | 26 to 81 | 5 to 18 percent |
| 21 to 40 | T6 to T9 | 0 | 188 (day 29) | 55 percent (ore, salvage and derelicts together) |

Competent play mines only the inner ring (rings 3 to 8 and only where escorted power is at least 2x the NPC power or the tile is unguarded). Details in section 16.

---

## 7. Threat and the NPC gradient

### 7.1 NPC group power

`npc_power(d) = 4 * 1.22^(d - 1)` (power units as in 5.1), replacing the four-step table. The old power by band was 5 (<= 8), 97 (9 to 15), 480 (16 to 25), 2640 (26+): an 18x cliff at ring 9. New: 1.22x per ring.

Composition for a sector at distance `d`: class `Scout` for d <= 5, `Corvette` 6 to 12, `Frigate` 13 to 20, `Cruiser` 21+; stat multiplier `m(d) = min(1.3, 0.6 + 0.02 d)`; count `max(1, round(npc_power(d) / (class_power * m)))` (never more than the 32-ship NPC limit; d = 35 gives 24 Cruisers). Presence chance `min(0.85, 0.25 + 0.025 d)` (old 0/30/70/80 by zone). Passive share `max(0, 0.5 - 0.08 (d - 8))` (old 50 percent inside ring 8, 0 outside). Respawn `(2 + 0.25 d)` game hours (old 300/600/1200 s), divided by P.

### 7.2 Threat rating

`threat(power) = clamp(1 + floor(log2(power / 4)), 1, 9)`. For the distance estimate, power is `npc_power(d)`; for a sector where a group has been observed, power is the real group power (+25 percent if Aggressive or Swarm). Thresholds by distance: T2 from d = 5, T3 from 8, T4 from 12, T5 from 15, T6 from 19, T7 from 22, T8 from 26, T9 from 29.

Displayed in three places:

1. **Map**: every known sector shows its rating (colour ramp and digit) with an age mark when the information is from a scan. Fixes the "map forgets explored space" finding: the last seen threat persists, flagged stale after the scan reveal window.
2. **Scan results**: each revealed sector gets `{rating, est_power, basis}` where `basis` is `observed` (hostiles seen), `template` (derived from the sector seed, a Sensor Array level 1 effect), or `estimate` (distance only).
3. **Move preview** (`preview_move` command, also shown in the TUI/web confirm step): "Sector (9,-8): threat T3, est. NPC power 20, your fleet power 60, ratio 3.0: SAFE. Fuel 15 of 120 (4 hops left to return)". Ratio labels, for the fleet asked about: `>= 2.5 SAFE`, `1.5 to 2.5 FAVOURABLE`, `1.1 to 1.5 EVEN`, `0.9 to 1.1 RISKY`, `< 0.9 DEADLY`. The first six-AI playtest bands (3.0 / 2.0 / 1.2) called a 1.1x fight DEADLY. Simulated fights (`combat::tests`, ignored `print_win_rates_by_ratio`) show why these cutoffs: damage exchange is quadratic in fleet size, so like-for-like fleets lose every time under 0.9, win about half the time around 1.0, win surely from 1.1 but lose 40 to 60 percent of their hull, keep 85 to 95 percent from 1.5, and 98 percent or more from 2.5. A lighter class against a heavier one (scouts v corvettes, rapid fire 3) needs about 1.5x the ratio for the same result. `preview_move` also reports the way home: `hops_home` (shortest real route), `charted_hops_home` (over entered sectors) and `route_unexplored`.

### 7.3 Table (pace-1 units)

(See the first generated table above: NPC power, old NPC power, kill loot, derelict loot, ore reserve and respawn by distance.)

---

## 8. Loot, salvage and discoveries

All loot values below are pace-1 units and are multiplied by `P^0.5` (finds class). Split between metal/crystal/deuterium is 55/30/15.

### 8.1 NPC wrecks (salvage scales with class and ring)

`wreck_value(group) = 1.2 * npc_power^0.8` units. Because `npc_power` grows 1.22x per ring and class steps up with distance, both class and ring raise the payout; it grows slower than the danger (exponent 0.8) so the risk-adjusted reward falls gradually with distance, which keeps far hunting a gamble rather than the best income. Examples: d = 4 pays 6 units, d = 12 pays 21, d = 20 pays 75, d = 30 pays 367, d = 35 pays 812. This replaces `SALVAGE_FRACTION 0.30` of the build cost of **one** ship (bug: a 30-Cruiser swarm paid like one Cruiser, and `FleetDestroyed` advertised 200/50/30 for a pile of 60/15/9). Fix: both the event and the pile use `wreck_value`.

Hunting is positive but thin: an engagement at 3x the NPC power loses about 10 percent of the loot value to attrition; at 1.5x about 80 percent; at 1.0x 270 percent (it loses money). The simulation uses `attrition(r) = 0.1 * (3/r)^3`.

### 8.2 Derelict sites

Tier follows threat: tier 1 for T1 to T3, tier 2 for T4 to T6, tier 3 for T7 to T9 (distances 1 to 11, 12 to 21, 22+; old 1 to 8, 9 to 20, 21+). Resource value `240 * 1.14^(d - 4)` units, drawn uniformly from 60 to 140 percent. A Hauler in the party still doubles resource loot. Ambush chance `min(0.35, 0.10 + 0.03 (T - 1))` (old 0.10/0.20/0.30 by tier). Spawn chance stays 8 percent of Debris and Empty sectors beyond distance 4; **derelicts respawn** 120 game hours / P^0.5 after being stripped (old: never).

**Rare finds** (resources are always paid; these are in addition):

| find | chance | notes |
|---|---|---|
| Recoverable ship | `clamp(0.05 (T - 2), 0, 0.30)` | tier 1 pool Scout/Corvette, tier 2 plus Hauler/Frigate, tier 3 Corvette/Hauler/Frigate/Cruiser |
| Data core (free research level) | `clamp(0.03 (T - 5), 0, 0.15)` | existing `grant_tech_level` |
| Ancient relic (T7 and up, 2 percent per threat point above 6) | up to 6 percent | a unique item worth 25 score points, reserved for the item system (hook, no gameplay yet) |

Old recovery odds were 0/15/25 percent and cache odds 0/0/10 percent by tier; the new curve reaches 25 percent recovery at T7 and 30 percent at T8 and above, and a 12 percent data core chance at T9.

### 8.3 What the player gets at distance (persistent, competent, from the simulation)

* A competent player's hunting and boarding adds 7 percent of building income from rings 12 to 14 at day 18 and 21 percent from rings 15 to 20 at day 37; the fleet that can win at 3x there is a Cruiser-class investment.
* First tier-2 derelict: day 7 (persistent), 21 h (season), 19 min (blitz).

---

## 9. Score

One ranking for humans and agents (decision 5). Points are decimal values shown to one decimal, recomputed when something completes, and recorded hourly for the season history.

```
weight(m, c, d)  = (1.0 m + 1.5 c + 2.0 d) / 1000          # production ratio 10 : 7 : 5 -> crystal and deuterium are scarcer
econ     = sum over completed building levels and research levels of weight(cost paid)
fleet    = sum over living ships of weight(build cost)
defence  = 0.5 * sum over living structures of weight(build cost)
combat   = sum over kills of 0.25 * weight(NPC group build cost)       (with the rules below)
explore  = sum over first-time derelict boardings of 0.15 * loot_value/1000 + 0.02 * T^1.5
         + 0.01 * T^1.5 per newly charted sector delivered to a dock
core     = econ + fleet + defence
score    = core + min(combat + explore, 0.25 * core)
```

What makes it hard to game:

| exploit | why it fails |
|---|---|
| Build and cancel to farm points | only completed levels count; cancelling an active item returns 50 percent resources and 0 points |
| Spend on ships and scuttle them | fleet points count living ships, so losses subtract |
| Cheap defence spam | defences count 50 percent |
| Farming trivial NPCs | a kill pays 0 if your engaged power is >= 8x the NPC power, and 0 again in the same sector until it respawns |
| Suicide scouts to chart space | charting points are credited only when the chart is delivered to a dock |
| Exploring instead of building | the combat plus exploration share is capped at 25 percent of core points. In the simulation the cap binds from the middle of the arc, so exploration is a tie-breaker and a differentiator, not the ladder |
| Alt feeding (later, with trade) | transfers move resources but points are computed from what was *built*; resources received from another player never count |
| PvP farming (later) | a kill pays `0.25 * weight(victim ship cost)` only if the victim's score is at least half the attacker's, and the same victim pays 1.0, 0.5, 0.25, 0 on successive kills in 24 hours |
| Agent versus human | agents and humans are one population: same formula, same board; the leaderboard may carry an `agent` flag for filtering, never for scoring |

End-of-run scores in the simulation (competent): persistent about 3,000 at day 74, season about 2,300 at day 7.2, blitz about 2,900 at 3.1 h. Season boards reset with the world; the persistent board is rolling.

---

## 10. New and changed tech and buildings

| name | type | max | prerequisite | effect |
|---|---|---|---|---|
| Storage Vault | building | 20 | Metal 2 | storage caps x1.5 per level; protects 8 percent per level (to 60 percent) from raids |
| Fabricator | building | 20 | Shipyard 2 | building time /(1 + 0.15 L) |
| Modular Fabrication | research | 2 | Lab 4 | building slots 2 (L1), 3 (L2) |
| Defense Grid | building | 20 | Shipyard 3 | prerequisite for structures; structures +4 percent per level; virtual platforms halved |
| Pulse Turret, Lancer Battery, Ion Bastion, Aegis Dome | defence | n/a | Grid and Weapons/Shields as in 5.1 | home defence |
| Research Lab | building | 20 | Crystal 2 | now also research time /(1 + 0.10 L) |
| Fuel Depot | building | 20 | Deut 2 | fuel capacity +25 percent per level (was 10) |
| Harvesting Efficiency | research | 5 | Lab 2 | yield +20 percent per level (was extraction speed) |
| Frigate Tech, Cruiser Tech | research | 1 | as today | costs raised (2.4) |

No other tree changes. Observed in the simulation: the whole 13-tech tree is finished by day 3 in the 7-day season, at minute 63 of a blitz and about day 32 in the persistent world, so a larger tree is the cheapest way to add long-term depth (open question 4).

---

## 11. Code areas that change

`shared/src/constants.rs`: add `Pace`, `FUEL_DEUT_PER_UNIT`, storage bases, raid constants (as formulas), `npc_respawn_delay` becomes a function of distance and pace, `defense_grid_scout_units` table, `derelict_tier` (by threat), `derelict_tier_odds`/`derelict_loot_ranges` replaced by `derelict_value`, `derelict_ambush`, rare-find chances; `SALVAGE_FRACTION` removed; `SECTOR_REGEN_RATE` removed; `Density::depletion_threshold` changed; `RAID_*` replaced.

`shared/src/scaling.rs`: `BuildingType` gains `StorageVault` and `Fabricator` appended after `DefenseGrid` (do not renumber; the catalog is "one entry per type in declaration order"); `ResearchType` gains `ModularFabrication` appended; `production_per_tick` takes `Pace`; `building_cost`/`building_time`/`research_cost`/`research_time`/`ship_build_time` per sections 2.2 to 2.4; `fuel_depot_modifier`; `harvest_rate_modifier` redefined as yield; new `storage_cap`, `building_slots`, `npc_power`, `npc_composition`, `threat_rating`, `wreck_value`, `derelict_value`, `ring_mult`, `regen_hours`, `raid_power`, `defence_units`, and the score weights. Golden-value unit tests copied from the generated tables.

`shared/src/world.rs`: `roll_npc_presence` (smooth composition), `roll_density` (distance drift), `roll_derelict` unchanged probability but tier by threat. Gated by `worldgen_version` (section 13).

`shared/src/protocol.rs`: section 12.

`server/src/engine.rs` (function names as today):
* `Player` and `process_homeworlds`: `building_queue` becomes `Vec<BuildQueueEntry>` plus pending lists; production multiplies by pace; apply caps and waste accounting; `StorageNearCap`/`StorageFull` events.
* `handle_build`, `handle_research`, `handle_build_ship`, `handle_cancel_build`, `process_build_queues`: slots, depth 3, auto-start, defence builds, new errors.
* `process_harvesting`, `accumulate_harvest`, `process_sector_regen`, `regen_resource`: ring-scaled thresholds, refine yield, time-based regeneration.
* `process_npc_behavior` (respawn), `spawn_npc_fleet`, `start_combat`, `process_combat` (`drop_salvage` call): `wreck_value` from the real group; fix `npc_value` using one ship's cost.
* `resolve_exploration`, `ambush_guardian_template`, `recoverable_ship_class`, `site_risk_label`, `derelict_site_at`: new tables, respawn, relic.
* `process_raids`, `resolve_raid`, `home_defense_power`: section 5.
* `dock_fleet`, `fleet_fuel_max`: refuel cost and cargo blocked by storage.
* New `score.rs` (pure functions plus a per-player accumulator) and `pace.rs`.
* Persistence: new tables `world_meta(pace, economy_version, worldgen_version)`, `player_defences`, `player_score`, queue serialisation as lists.

Every new timer must pick a `PaceClass { Economy, Attention, Finds, RealTime }` via the helpers in 1.4; a test enumerates the constants and fails if a duration constant has no class.

---

## 12. Protocol additions

All additions are optional fields or appended variants, so existing agents keep working.

```jsonc
// full_state.world (new)
{ "pace": 10.0, "preset": "season", "tick_hz": 1,
  "economy_version": 2, "worldgen_version": 2,
  "season": { "start_tick": 0, "end_tick": 604800 },        // optional
  "estimate": { "first_cruiser_s": 165000, "endgame_s": 620000 } }   // optional, from the sim table

// homeworld (new fields)
"storage": { "cap": {"metal": 28117, "crystal": 19681, "deuterium": 14058},
             "protected": {"metal": 0, "crystal": 0, "deuterium": 0},
             "full_in_s": {"metal": 5400, "crystal": null, "deuterium": null} },
"production_per_tick": {...},          // already capped; plus "capped": ["metal"]
"build_slots": 2,
"build_queue": [BuildQueueItem...],    // active items (replaces the single `build_queue` field; keep the first as `build_queue_head` for old clients)
"build_pending": [QueuedItem...],      // up to depth - active
"defences": [ { "kind": "pulse_turret", "count": 12, "power": 17 } ],
"home_defence_power": 340.0, "next_raid_estimate_power": 300.0,

// catalog (new)
"catalog": { ..., "defences": [DefenceOption{kind,count,unit_cost,ticks_per_unit,power,requires}],
             "storage": {...next vault step...}, "slots": {...next modfab step...} }

// sector (SectorState, new)
"threat": { "rating": 4, "est_power": 36.0, "basis": "template|observed|estimate", "age_ticks": 40 }
"ore_reserve": { "metal": "rich", "refills_in_s": 12000 }     // optional, only for known tiles

// fleet (FleetState, new)
"power": 60.0, "range_hops": 8, "fuel_to_return": 12.0, "cargo_blocked": false

// commands (new)
{"cmd":"preview_move","fleet_id":7,"target":{"q":9,"r":-8}}   // -> PreviewMove{threat, ratio, label, fuel_cost, fuel_after, hops_home}
{"cmd":"build_defence","kind":"lancer","count":4}
{"cmd":"cancel_queued","queue":"building","index":1}
{"cmd":"leaderboard","limit":20}                                // -> Leaderboard{ entries:[{rank,name,agent,score,core,combat,explore}] }

// events (new/extended)
StorageNearCap{resource, ratio, full_in_s}; StorageFull{resource};
RaidIncoming gains "est_power"; RaidResolved gains "structures_lost", "structures_restored", "protected_kept";
ScanCompleted sectors carry "threat"; SignalContact gains "threat_band";

// errors (new)
StorageTooSmall{resource, need_vault_level}; NoBuildSlot; QueueDepthReached; DefenceLocked{requires}
```

The `NoResources` error should name the missing resource and amount (gap analysis G8) in the same change.

---

## 13. Migration of existing worlds

Existing worlds are playtest data from before any release (`decisions.md` is the first owner decision file). Recommended path: **start new worlds; do not migrate the live playtest DBs**. If an existing DB must survive:

1. Add `world_meta` with `economy_version = 1` for a DB without it. The server refuses to start an `economy_version = 1` DB unless `iac-server migrate-economy --pace <P>` is run first.
2. `migrate-economy` does, in one transaction: set `economy_version = 2`, `worldgen_version = 1` (keeps the old sector templates: under `worldgen_version = 1` the old `roll_npc_presence`, density rolls and derelict spawns stay in force, so the maps players remember do not change and NPC groups keep their old power; the displayed threat rating is computed from the real group power, so it will honestly show the old cliff); convert in-flight queue items by remaining fraction (`new_end = now + (old_end - now) * new_duration / old_duration`); keep building, research and ship levels; clamp nothing (a stockpile above the new cap is grandfathered: `effective_cap = max(cap, stock)` until the stock first drops below the cap); convert `DefenseGrid` levels unchanged (its virtual platforms halve); set ore `harvested` counters to 0; keep `npc_cleared_tick` and `site_looted_tick`; add `StorageVault`, `Fabricator`, `ModularFabrication` at level 0; recompute score from levels and ships.
3. Pace of a migrated world is its own choice; it cannot be changed after the first tick (changing it later would rescale every timer in flight; supply a separate `iac-server set-pace` that applies the same remaining-fraction conversion, for admin use).

Clients: the new fields are optional, but old clients that read `build_queue` as a single item see only the head item until updated. Bump a `protocol_version` in `auth_result` to let agents detect the new fields.

---

## 14. Implementation order (reviewable commits)

1. **Pace plumbing, no balance change.** `Pace` struct, `PaceClass`, `world_meta`, `--pace`/`--world`, `world` block in `full_state`. All timers read through the helpers; pace 1 with the legacy constants gives identical behaviour (add a test that runs 600 ticks and compares state to the previous implementation). Needs no review of numbers.
2. **Cost, time and production formulas.** `scaling.rs` new curves, new constants, the golden-value tests from the generated tables, catalog updates (the catalog already sends cost and ticks, so clients need no change).
3. **Storage Vault and caps.** Building, cap math, discard at cap, dock blocking, protected share in raids, `storage` in the homeworld state, alerts, errors.
4. **Slots, queue depth, Fabricator, Lab speed.** Engine queue refactor and protocol (`build_queue` list, `build_pending`), `ModularFabrication`.
5. **Deuterium and fuel.** Refuel cost, Depot 25 percent, range and fuel-to-return fields, `preview_move` skeleton (fuel and hops only).
6. **Threat and the NPC gradient.** `npc_power`, composition, `worldgen_version` gate, `threat` in sectors, scans and `preview_move`; map colours in web and TUI (client work follows).
7. **Ore.** Tile thresholds, ring multipliers, density drift (new worlds only), refine yield, time-based regeneration.
8. **Loot.** `wreck_value` (fixes the one-ship bug and the event/pile mismatch), derelict tables, respawn, rare finds, relic placeholder.
9. **Defences and raids.** Structures, catalog, raid sizing from score, restoration, protected storage, new timers.
10. **Score and leaderboard.** `score.rs`, `leaderboard` command, season history table.
11. **Migration tool, docs and harness.** `migrate-economy`, `set-pace`, update the playtest harness to start `--world blitz`, update README ("The game") and the dev journal.

Each commit ships with: unit tests from the golden tables, an `economy_version` bump only in commit 2, and a re-run of `sim.py` with its constants checked against the Rust constants by a small script (`tools/check_sim_constants.py`, to be written; compares the constants table in this document with `constants.rs`/`scaling.rs`).

---

## 15. Hooks for later milestones (no design here)

* **Chat and trade:** resources are plain fungible quantities; trade moves them between stockpiles through a Hauler shipment (cargo-limited), capped by the receiver's free storage (so the Vault is also a trade throttle), and never creates score for the receiver. Prices, auctions and contracts are not designed.
* **Loot drop on death:** cargo already lives in the fleet, not in a stockpile. On fleet death the cargo drops as a salvage pile (full amount, `SALVAGE_DESPAWN_TICKS` unscaled) in addition to the 30 percent wreck value. Homeworld stockpiles are never dropped. This is the cargo-versus-stockpile split the cap model leaves room for.
* **PvE team-ups:** wreck and derelict loot would be split by damage and presence; combat points are credited to every participant at full value (no split) so cooperation is rewarded rather than punished. Pace-independent.
* **Outer-ring PvP:** threat ratings extend to players (power of the fleet seen, same scale); protected storage, structure restoration and the 50 percent defence points handle the home side; `Wandering` (d > 20) is the natural opt-in zone. The 25 percent combat cap in the score stops PvP from dwarfing the economy.
* **Items and relics:** the relic find is reserved; scoring is already defined.

---

## 16. Simulation

`sim.py` (Python standard library only) simulates one player per world under each preset. It uses the same formulas as sections 2 to 9. Run `python3 sim.py` for the full report (about 25 s), `python3 sim.py --tables` for the static tables, `python3 sim.py --sensitivity` for attention sensitivity, `python3 sim.py --pace 25` for any pace, `--set NAME=expr` to override a constant, `--modes none,near,far`.

**The player.** A scripted "competent" player: builds along a reference ladder (each mine up one level per round, other buildings at fixed ratios of the metal level, Vault when stock passes 75 percent of cap or a purchase exceeds the cap), researches in a fixed order, keeps defence at 1.15 times the next raid, buys ships by a fixed plan (Corvettes, Haulers, Frigates, Cruisers by metal level), and refills queues only during sessions (persistent: 4 sessions of 12 minutes a day; season: 8 of 15 minutes; blitz and test: continuous). Queue depth 3, three slots after Modular Fabrication, shared stockpile with a fair-share allocator across the building, research and ship queues. Milestone times are the median of three session-schedule offsets (0, 3, 6 h); the planner is a heuristic, so expect about ±15 percent noise between runs and treat single numbers as shape, not precision.

**Piloting model.** Expected values, not individual fights: ore supply is the tile reserve and refill rate of the rings in fuel range; hunting and boarding are capped by active play time and by respawn supply; each engagement pays loot and costs attrition by power ratio; every sortie pays the refuel price. Competent = engage at 3x power, mine the inner ring, hunt and board up to ring 20. Greedy = engage at 1.5x, mine the farthest ring whose net value is within 80 percent of the best, hunt and board up to ring 40. None = no piloting at all.

**Limitations.** No PvP; NPC fights are expectations, not combat; the world is a symmetric ring model (no individual sector layout); the planner may differ from a clever human; piloting activity in the fast presets is extraction-limited at about 85 percent active time; the reference "endgame kit" is a fixed bill (Metal 19, Crystal 16, Deut 13, Shipyard 11, Lab 11, Fabricator 8, Grid 5, ModularFabrication 2, Cruiser/Fuel Eff/Tanks/Weapons/Shields/Hulls techs, 12 Cruisers), chosen so the persistent arc lands at about ten weeks; it is not a game state the server knows.

### Simulation output (pasted verbatim from `python3 sim.py`)

Mode `near` = competent, `far` = greedy far-miner, `none` = buildings only. Times are real time since registration. "pilot share" is mining + salvage + derelicts over gross income (buildings + pilot), before fuel and losses.

### Milestones, competent player (mode `near`); median of 3 session-schedule offsets

| Milestone | persistent | season-7d | dev-100x | blitz-1h | test-8000x |
|---|---|---|---|---|---|
| Corvette Tech | 3.3d | 5.7h | 44m | 8m | 41s |
| First Corvette | 3.5d | 7.1h | 46m | 9m | 42s |
| Lab 2 | 33.7h | 3.5h | 18m | 3m | 18s |
| Shipyard 4 | 6.6d | 20.5h | 1.6h | 19m | 2m |
| Build slot 2 (research) | 11.2d | 28.9h | 2.8h | 27m | 2m |
| Metal Mine 10 | 10.9d | 32.1h | 1.7h | 21m | 2m |
| First tier-2 derelict loot (ring 12+) | 7.0d | 21.4h | 1.7h | 19m | 2m |
| First Cruiser | 15.8d | 45.8h | 3.8h | 42m | 4m |
| First tier-3 derelict loot (ring 22+, competent never goes) | - | - | - | - | - |
| Endgame kit complete | 74.1d | 7.2d | 13.3h | 3.1h | 12m |

### Milestones, greedy far-mining player (mode `far`)

| Milestone | persistent | season-7d | dev-100x | blitz-1h | test-8000x |
|---|---|---|---|---|---|
| Corvette Tech | 3.4d | 5.6h | 57m | 10m | 41s |
| First Corvette | 3.5d | 7.0h | 58m | 11m | 42s |
| Lab 2 | 33.4h | 3.5h | 18m | 3m | 18s |
| Shipyard 4 | 6.6d | 18.6h | 1.6h | 18m | 2m |
| Build slot 2 (research) | 11.0d | 26.7h | 2.4h | 29m | 2m |
| Metal Mine 10 | 10.0d | 27.8h | 1.9h | 26m | 2m |
| First tier-2 derelict loot (ring 12+) | 7.0d | 20.3h | 1.6h | 18m | 2m |
| First Cruiser | 15.1d | 45.4h | 3.9h | 41m | 4m |
| First tier-3 derelict loot (ring 22+, competent never goes) | 26.3d | 2.5d | 5.5h | 61m | 5m |
| Endgame kit complete | 60.8d | 5.7d | 11.4h | 2.6h | 12m |

### Milestones, buildings-only player (mode `none`, no piloting at all)

| Milestone | persistent | season-7d | dev-100x | blitz-1h | test-8000x |
|---|---|---|---|---|---|
| Corvette Tech | 3.7d | 6.9h | 52m | 11m | 41s |
| First Corvette | 3.8d | 8.5h | 54m | 11m | 43s |
| Lab 2 | 37.8h | 4.0h | 21m | 4m | 18s |
| Shipyard 4 | 7.1d | 20.5h | 1.8h | 18m | 79s |
| Build slot 2 (research) | 12.6d | 35.4h | 3.0h | 30m | 2m |
| Metal Mine 10 | 11.6d | 33.7h | 2.8h | 27m | 2m |
| First tier-2 derelict loot (ring 12+) | - | - | - | - | - |
| First Cruiser | 17.3d | 2.0d | 4.1h | 41m | 3m |
| First tier-3 derelict loot (ring 22+, competent never goes) | - | - | - | - | - |
| Endgame kit complete | 84.2d | 9.0d | 17.8h | 3.0h | 16m |

### Pace needed for a target arc (derived from the persistent run)

| world arc you want | pace P needed | first Cruiser | endgame kit | notes |
|---|---|---|---|---|
| persistent baseline | x1 | 15.8d | 74.1d | |
| owner guess: weekly "2x" | x2 | 7.9d | 37.1d | |
| owner guess: blitz "5x" | x5 | 3.2d | 14.8d | |
| endgame in 30 days | x2 | 6.4d | 30.0d | |
| endgame in 14 days | x5 | 3.0d | 14.0d | |
| endgame in 7 days (season) | x11 | 35.9h | 7.0d | |
| endgame in 3 days | x25 | 15.4h | 3.0d | |
| endgame in 24 h | x74 | 5.1h | 24.0h | |
| Cruiser at 45 min (blitz) | x507 | 45m | 3.5h | |
| endgame in 15 min (test) | x7115 | 3m | 15m | |

### Mode comparison: none = buildings only, near = competent, far = greedy far-mining

| preset | mode | first Corvette | Metal 10 | first Cruiser | endgame | pilot share of income | losses (value) | final econ+fleet pts |
|---|---|---|---|---|---|---|---|---|
| persistent | none | 3.8d | 11.6d | 17.3d | 84.2d | 0% | 0 | 2700 |
| persistent | near | 3.5d | 10.9d | 15.8d | 74.1d | 15% | 214 | 2433 |
| persistent | far | 3.5d | 10.0d | 15.1d | 60.8d | 36% | 7.28e+04 | 2051 |
| season-7d | none | 8.5h | 33.7h | 2.0d | 9.0d | 0% | 0 | 2744 |
| season-7d | near | 7.1h | 32.1h | 45.8h | 7.2d | 17% | 886 | 1845 |
| season-7d | far | 7.0h | 27.8h | 45.4h | 5.7d | 35% | 5.45e+04 | 2338 |
| dev-100x | none | 54m | 2.8h | 4.1h | 17.8h | 0% | 0 | 1986 |
| dev-100x | near | 46m | 1.7h | 3.8h | 13.3h | 27% | 1.72e+03 | 2136 |
| dev-100x | far | 58m | 1.9h | 3.9h | 11.4h | 44% | 1.76e+04 | 1992 |
| blitz-1h | none | 11m | 27m | 41m | 3.0h | 0% | 0 | 1985 |
| blitz-1h | near | 9m | 21m | 42m | 3.1h | 16% | 342 | 2739 |
| blitz-1h | far | 11m | 26m | 41m | 2.6h | 31% | 1.16e+04 | 2489 |
| test-8000x | none | 43s | 2m | 3m | 16m | 0% | 0 | 2733 |
| test-8000x | near | 42s | 2m | 4m | 12m | 10% | 126 | 1992 |
| test-8000x | far | 42s | 2m | 4m | 12m | 21% | 5.06e+03 | 2128 |

### Income split over the whole run, competent (near)

| preset | buildings | ore | salvage | derelict | fuel cost | losses | bldg share | pilot share |
|---|---|---|---|---|---|---|---|---|
| persistent | 2.04e+06 | 2.87e+04 | 1.85e+05 | 1.58e+05 | 1.01e+05 | 214 | 85% | 15% |
| season-7d | 1.63e+06 | 2.64e+04 | 1.44e+05 | 1.58e+05 | 2.62e+04 | 886 | 83% | 17% |
| dev-100x | 1.46e+06 | 2.25e+04 | 3.91e+05 | 1.32e+05 | 2.14e+04 | 1.72e+03 | 73% | 27% |
| blitz-1h | 2.27e+06 | 3.07e+04 | 2.17e+05 | 1.79e+05 | 5.31e+03 | 342 | 84% | 16% |
| test-8000x | 1.69e+06 | 2.25e+03 | 4.75e+04 | 1.41e+05 | 357 | 126 | 90% | 10% |

### Income split over the whole run, greedy far-mining

| preset | buildings | ore | salvage | derelict | fuel cost | losses | bldg share | pilot share |
|---|---|---|---|---|---|---|---|---|
| persistent | 1.54e+06 | 2.97e+05 | 3.27e+05 | 2.39e+05 | 1.78e+05 | 7.28e+04 | 64% | 36% |
| season-7d | 1.57e+06 | 2.98e+05 | 2.8e+05 | 2.66e+05 | 6.59e+04 | 5.45e+04 | 65% | 35% |
| dev-100x | 1.17e+06 | 5.37e+04 | 6.56e+05 | 1.89e+05 | 3.18e+04 | 1.76e+04 | 56% | 44% |
| blitz-1h | 1.68e+06 | 2.25e+04 | 4.25e+05 | 3e+05 | 8.72e+03 | 1.16e+04 | 69% | 31% |
| test-8000x | 1.59e+06 | 2.92e+03 | 1.18e+05 | 3.03e+05 | 739 | 5.06e+03 | 79% | 21% |

### Storage-cap pressure (competent)

| preset | time at >90% cap | time at 100% cap | wasted units (m/c/d) | wasted vs produced |
|---|---|---|---|---|
| persistent | 4.8% | 3.0% | 1.75e+04/4.89e+03/0 | 0.93% |
| season-7d | 6.7% | 3.8% | 1.39e+04/1.36e+04/1.32e+03 | 1.47% |
| dev-100x | 0.2% | 0.0% | 0/0/0 | 0.00% |
| blitz-1h | 0.0% | 0.0% | 0/0/0 | 0.00% |
| test-8000x | 0.0% | 0.0% | 0/0/0 | 0.00% |

### Score at end of run (competent)

| preset | econ pts (build+research) | fleet pts | defence pts | combat+explore (capped) | total |
|---|---|---|---|---|---|
| persistent | 2252 | 158 | 23 | 608 | 3042 |
| season-7d | 1689 | 138 | 18 | 461 | 2307 |
| dev-100x | 1970 | 145 | 22 | 534 | 2670 |
| blitz-1h | 2554 | 158 | 26 | 169 | 2907 |
| test-8000x | 1834 | 138 | 20 | 10 | 2002 |

### Season-7d: what completes each day (competent player)

**season-7d / near**

| day | buildings reached | research done | ships/defences finished | score (core+extra) |
|---|---|---|---|---|
| 1 | metal 6, crystal 7, deut 4, lab 3, shipyard 4, fab 2, depot 1, sensor 1, grid 2 | corvette 1, fuel_eff 1, harvest 1, hauler 1, weapons 1, hulls 1, shields 1, nav 1, fuel_eff 2, harvest 2, weapons 2, shields 2, hulls 2, frigate 1 | corvettex6, haulerx2, turretx8 | 41+10 |
| 2 | depot 3, metal 13, deut 7, lab 6, shipyard 7, fab 4, sensor 2, grid 3, crystal 10 | fuel_eff 3, tanks 3, weapons 3, modfab 1, modfab 2, shields 3, cruiser 1, hulls 3, harvest 3 | lancerx6, frigatex4 | 195+49 |
| 3 | vault 1, deut 9, lab 8, crystal 14, fab 5, shipyard 8, grid 4, sensor 3, metal 14 | nav 2, weapons 4, shields 4, hulls 4, tanks 4, fuel_eff 4, harvest 4, weapons 5, shields 5, hulls 5, tanks 5, fuel_eff 5, harvest 5, nav 3, nav 4, nav 5 | lancerx6, cruiserx9, bastionx1 | 499+125 |
| 4 | vault 2, fab 6, depot 4, crystal 15, shipyard 9, metal 16, deut 10 |  | bastionx4, cruiserx4 | 720+180 |
| 5 | crystal 16, vault 4, deut 12, shipyard 10, grid 5, metal 17, lab 10, fab 7 |  | bastionx3, cruiserx2 | 1190+298 |
| 6 | shipyard 11, vault 5, sensor 4, metal 19, deut 13, fab 8, depot 5, grid 6 |  | bastionx4, cruiserx1 | 1647+412 |
| 7 | vault 6, lab 11 |  | bastionx1, cruiserx2 | - |

### Resource balance over time: persistent

**persistent / near**

| time | metal | crystal | deut | cap m/c/d | prod/h m+c+d (real) | levels M/C/D/SY/Lab/Vault/Fab | ships s/co/fr/cr/ha | defs | slots | econ pts |
|---|---|---|---|---|---|---|---|---|---|---|
| 5m | 411 | 278 | 100 | 5000/3500/2500 | 19 | 1/1/0/0/0/0/0 | 2/0/0/0/0 | 0 | 1 | 0.0 |
| 7.3d | 539 | 1760 | 268 | 5000/3500/2500 | 251 | 7/6/5/4/3/0/2 | 2/6/0/0/2 | 8 | 1 | 26.4 |
| 14.7d | 11250 | 4456 | 3109 | 11250/7875/5625 | 525 | 12/7/6/6/5/2/3 | 2/6/4/0/2 | 19 | 2 | 84.0 |
| 22.0d | 8522 | 238 | 2106 | 25312/17719/12656 | 682 | 12/11/8/7/6/4/4 | 2/6/4/7/2 | 29 | 2 | 168.9 |
| 29.3d | 5944 | 6176 | 2356 | 25312/17719/12656 | 869 | 13/13/9/8/7/4/5 | 2/6/4/9/2 | 35 | 3 | 319.1 |
| 36.7d | 9860 | 24493 | 4016 | 37969/26578/18984 | 1156 | 15/14/11/9/8/5/6 | 2/6/4/12/2 | 37 | 3 | 528.5 |
| 44.0d | 43632 | 34845 | 8483 | 85430/59801/42715 | 1420 | 17/14/12/10/9/7/7 | 2/6/4/15/2 | 39 | 3 | 821.6 |
| 51.3d | 54366 | 11223 | 5599 | 192217/134552/96108 | 1561 | 18/14/12/10/10/9/7 | 2/6/4/16/2 | 43 | 3 | 1043.2 |
| 58.7d | 44130 | 8289 | 25890 | 192217/134552/96108 | 1825 | 19/15/13/11/10/9/8 | 2/6/4/18/2 | 47 | 3 | 1462.8 |
| 66.0d | 16255 | 14781 | 26564 | 192217/134552/96108 | 2085 | 20/16/13/11/10/9/8 | 2/6/4/18/2 | 47 | 3 | 1742.5 |
| 73.8d | 223470 | 37820 | 40577 | 288325/201828/144163 | 2085 | 20/16/13/12/11/10/8 | 2/6/4/21/2 | 52 | 3 | 2251.8 |


### Resource balance over time: season-7d

**season-7d / near**

| time | metal | crystal | deut | cap m/c/d | prod/h m+c+d (real) | levels M/C/D/SY/Lab/Vault/Fab | ships s/co/fr/cr/ha | defs | slots | econ pts |
|---|---|---|---|---|---|---|---|---|---|---|
| 30s | 411 | 278 | 100 | 28117/19682/14059 | 187 | 1/1/0/0/0/0/0 | 2/0/0/0/0 | 0 | 1 | 0.0 |
| 11.2h | 394 | 121 | 542 | 28117/19682/14059 | 1673 | 6/4/3/3/3/0/1 | 2/5/0/0/0 | 0 | 1 | 12.8 |
| 28.0h | 81 | 5123 | 1041 | 28117/19682/14059 | 3480 | 9/7/5/5/4/0/3 | 2/6/4/0/2 | 10 | 1 | 54.2 |
| 44.8h | 22060 | 2148 | 3015 | 28117/19682/14059 | 6986 | 13/10/7/7/6/0/4 | 2/6/4/0/2 | 14 | 3 | 155.8 |
| 2.6d | 19695 | 929 | 4104 | 42176/29523/21088 | 8691 | 13/13/9/8/7/1/5 | 2/6/4/7/2 | 20 | 3 | 314.3 |
| 3.3d | 11828 | 7095 | 9768 | 63263/44284/31632 | 10764 | 14/15/9/8/8/2/6 | 2/6/4/10/2 | 22 | 3 | 488.7 |
| 3.7d | 36464 | 44284 | 23022 | 63263/44284/31632 | 13035 | 16/15/10/9/8/2/6 | 2/6/4/13/2 | 25 | 3 | 607.2 |
| 4.4d | 25344 | 41546 | 32089 | 94895/66427/47448 | 15308 | 17/16/11/10/9/3/6 | 2/6/4/13/2 | 27 | 3 | 925.0 |
| 5.1d | 11535 | 36727 | 33573 | 213514/149460/106757 | 15622 | 17/16/12/11/10/5/7 | 2/6/4/15/2 | 31 | 3 | 1226.3 |
| 5.8d | 69410 | 95213 | 65423 | 213514/149460/106757 | 17398 | 18/16/13/11/10/5/8 | 2/6/4/16/2 | 32 | 3 | 1384.1 |
| 6.5d | 91203 | 45037 | 50651 | 320271/224190/160135 | 19011 | 19/16/13/11/11/6/8 | 2/6/4/18/2 | 33 | 3 | 1689.5 |


### Resource balance over time: blitz-1h

**blitz-1h / near**

| time | metal | crystal | deut | cap m/c/d | prod/h m+c+d (real) | levels M/C/D/SY/Lab/Vault/Fab | ships s/co/fr/cr/ha | defs | slots | econ pts |
|---|---|---|---|---|---|---|---|---|---|---|
| 1s | 412 | 279 | 100 | 606155/424308/303077 | 11220 | 1/1/0/0/0/0/0 | 2/0/0/0/0 | 0 | 1 | 0.0 |
| 15m | 35 | 698 | 952 | 606155/424308/303077 | 120399 | 6/6/3/3/3/0/2 | 2/6/0/0/2 | 0 | 1 | 15.6 |
| 35m | 10630 | 38 | 4504 | 606155/424308/303077 | 358458 | 13/7/6/6/5/0/4 | 2/6/4/0/2 | 13 | 2 | 99.6 |
| 55m | 70639 | 289 | 7457 | 606155/424308/303077 | 468850 | 14/10/7/7/6/0/4 | 2/6/4/8/2 | 16 | 2 | 178.9 |
| 75m | 139092 | 12231 | 16316 | 606155/424308/303077 | 540831 | 14/12/9/8/7/0/5 | 2/6/4/10/2 | 22 | 3 | 326.6 |
| 1.6h | 117832 | 9875 | 27434 | 606155/424308/303077 | 758563 | 16/14/11/9/8/0/6 | 2/6/4/13/2 | 25 | 3 | 560.1 |
| 1.8h | 99482 | 11882 | 47086 | 606155/424308/303077 | 872875 | 17/15/11/10/9/0/7 | 2/6/4/15/2 | 28 | 3 | 842.7 |
| 2.2h | 161722 | 15929 | 62700 | 606155/424308/303077 | 1022237 | 18/16/12/11/9/0/7 | 2/6/4/16/2 | 32 | 3 | 1200.9 |
| 2.5h | 55433 | 17052 | 103306 | 606155/424308/303077 | 1192741 | 19/17/13/11/10/0/8 | 2/6/4/18/2 | 35 | 3 | 1655.8 |
| 2.8h | 58986 | 14693 | 121860 | 606155/424308/303077 | 1302828 | 20/17/13/12/10/0/8 | 2/6/4/21/2 | 39 | 3 | 2127.9 |
| 3.1h | 218079 | 40763 | 144036 | 606155/424308/303077 | 1362268 | 20/18/13/12/11/0/8 | 2/6/4/21/2 | 41 | 3 | 2554.3 |


### Where pilot income comes from, by distance (persistent)

**competent** at 18.3d: pilot yield per real hour by ring (all resources) against building income 626/h

| ring | threat | ore /h | salvage /h | derelict /h | total /h | vs buildings |
|---|---|---|---|---|---|---|
| 3-6 | T1-T2 | 7 | 0 | 2 | 9 | 2% |
| 7-8 | T2-T3 | 6 | 0 | 3 | 10 | 2% |
| 9-11 | T3-T3 | 0 | 5 | 11 | 15 | 2% |
| 12-14 | T4-T4 | 0 | 36 | 5 | 42 | 7% |
| 15-20 | T5-T6 | 0 | 0 | 0 | 0 | 0% |
| 21-40 | T6-T9 | 0 | 0 | 0 | 0 | 0% |

**competent** at 36.7d: pilot yield per real hour by ring (all resources) against building income 1156/h

| ring | threat | ore /h | salvage /h | derelict /h | total /h | vs buildings |
|---|---|---|---|---|---|---|
| 3-6 | T1-T2 | 10 | 0 | 2 | 13 | 1% |
| 7-8 | T2-T3 | 9 | 0 | 3 | 12 | 1% |
| 9-11 | T3-T3 | 0 | 0 | 11 | 11 | 1% |
| 12-14 | T4-T4 | 0 | 0 | 20 | 20 | 2% |
| 15-20 | T5-T6 | 0 | 149 | 98 | 248 | 21% |
| 21-40 | T6-T9 | 0 | 0 | 0 | 0 | 0% |

**persistent / near**: cumulative pilot income by ring (all resources), versus building income over the run

| ring band | ore | salvage | derelict | total | share of all income |
|---|---|---|---|---|---|
| 1-4 | 5.92e+03 | 17.3 | 922 | 6.86e+03 | 0.3% |
| 5-8 | 2.35e+04 | 385 | 8.43e+03 | 3.24e+04 | 1.3% |
| 9-14 | 0 | 1.9e+04 | 4.36e+04 | 6.26e+04 | 2.6% |
| 15-20 | 0 | 1.71e+05 | 1.1e+05 | 2.81e+05 | 11.6% |
| 21-40 | 0 | 0 | 0 | 0 | 0.0% |
| buildings | | | | 2.04e+06 | 84.6% |

**greedy far-mining** at 14.7d: pilot yield per real hour by ring (all resources) against building income 550/h

| ring | threat | ore /h | salvage /h | derelict /h | total /h | vs buildings |
|---|---|---|---|---|---|---|
| 3-6 | T1-T2 | 6 | 0 | 2 | 8 | 2% |
| 7-8 | T2-T3 | 6 | 0 | 3 | 9 | 2% |
| 9-11 | T3-T3 | 9 | 5 | 11 | 24 | 4% |
| 12-14 | T4-T4 | 13 | 36 | 5 | 55 | 10% |
| 15-20 | T5-T6 | 26 | 0 | 0 | 26 | 5% |
| 21-40 | T6-T9 | 0 | 0 | 0 | 0 | 0% |

**greedy far-mining** at 29.3d: pilot yield per real hour by ring (all resources) against building income 1010/h

| ring | threat | ore /h | salvage /h | derelict /h | total /h | vs buildings |
|---|---|---|---|---|---|---|
| 3-6 | T1-T2 | 10 | 0 | 2 | 13 | 1% |
| 7-8 | T2-T3 | 9 | 0 | 3 | 12 | 1% |
| 9-11 | T3-T3 | 14 | 0 | 11 | 25 | 2% |
| 12-14 | T4-T4 | 22 | 0 | 20 | 42 | 4% |
| 15-20 | T5-T6 | 81 | 0 | 98 | 180 | 18% |
| 21-40 | T6-T9 | 188 | 282 | 86 | 556 | 55% |

**persistent / far**: cumulative pilot income by ring (all resources), versus building income over the run

| ring band | ore | salvage | derelict | total | share of all income |
|---|---|---|---|---|---|
| 1-4 | 4.7e+03 | 0 | 747 | 5.45e+03 | 0.2% |
| 5-8 | 1.88e+04 | 373 | 6.83e+03 | 2.6e+04 | 1.1% |
| 9-14 | 4.16e+04 | 1.73e+04 | 3.32e+04 | 9.21e+04 | 3.8% |
| 15-20 | 8.11e+04 | 5.47e+03 | 8.25e+04 | 1.69e+05 | 7.0% |
| 21-40 | 1.77e+05 | 3.38e+05 | 1.41e+05 | 6.56e+05 | 27.3% |
| buildings | | | | 1.54e+06 | 64.0% |



### Attention sensitivity

| world | sessions/day x minutes | first Corvette | Shipyard 4 | slot 2 | first Cruiser | endgame | wasted production |
|---|---|---|---|---|---|---|---|
| persistent | 1 x 15 min | 4.8d | 9.2d | 12.4d | 27.8d | 81.9d | 0.0% |
| persistent | 2 x 15 min | 2.9d | 7.7d | 11.0d | 16.9d | 73.8d | 1.7% |
| persistent | 4 x 12 min | 3.5d | 6.6d | 11.2d | 15.8d | 74.1d | 0.9% |
| persistent | 8 x 12 min | 2.8d | 6.8d | 11.5d | 16.2d | 81.1d | 5.1% |
| season-7d | 2 x 15 min | 21.1h | 42.3h | 2.8d | 4.4d | 11.7d | 0.6% |
| season-7d | 4 x 15 min | 10.1h | 23.8h | 34.4h | 2.2d | 7.8d | 0.0% |
| season-7d | 8 x 15 min | 7.1h | 20.5h | 28.9h | 45.8h | 7.2d | 1.5% |
| season-7d | 16 x 15 min | 6.9h | 18.5h | 28.6h | 42.5h | 6.6d | 0.0% |


Reading: a persistent player who logs in once a day reaches the Cruiser in 28 days instead of 16 and the endgame in 82 days instead of 74, so the world is not punishing low attention. A season player who logs in twice a day takes 11.7 days instead of 7.2: a weekly season rewards engagement, as intended, and depth-3 queues and caps keep sleepers from losing production (wasted production is 0 to 2 percent in every case except one noisy 5 percent for the 8-session persistent player).

### Persistent world, one year (same competent player, not stopping at the endgame kit; `python3 sim.py --longrun`)

| day | Metal/Crystal/Deut | Shipyard | Lab | Fabricator | Vault | Cruisers | defence structures | score (core+extra) | production /h |
|---|---|---|---|---|---|---|---|---|---|
| 30 | 13/13/9 | 8 | 7 | 5 | 6 | 9 | 28 | 415+104 | 869 |
| 60 | 19/15/13 | 11 | 10 | 8 | 9 | 18 | 39 | 1618+404 | 1825 |
| 74 | 20/16/14 | 12 | 11 | 8 | 10 | 21 | 46 | 2478+620 | 2126 |
| 90 | 20/17/14 | 12 | 11 | 8 | 12 | 21 | 49 | 2827+707 | 2213 |
| 120 | 20/19/15 | 13 | 12 | 9 | 13 | 23 | 61 | 4697+1174 | 2472 |
| 150 | 20/19/16 | 14 | 13 | 9 | 13 | 25 | 72 | 6341+1585 | 2527 |
| 180 | 20/20/16 | 14 | 13 | 10 | 14 | 25 | 79 | 7433+1858 | 2655 |
| 240 | 20/20/17 | 15 | 14 | 10 | 16 | 27 | 102 | 11987+2997 | 2717 |
| 300 | 20/20/17 | 15 | 14 | 10 | 17 | 27 | 112 | 13701+3425 | 2717 |
| 365 | 20/20/18 | 16 | 15 | 11 | 18 | 29 | 148 | 21994+5498 | 2788 |


### What the numbers say about the owner's targets

* **Blitz reaches the first Cruiser at minute 42 of an hour** and the first tier-2 derelict at minute 19 (competent). Tier-3 derelicts, which a competent player does not touch, are first looted at minute 61 by the greedy player.
* **A weekly season has a goal every day** (table "Season-7d: what completes each day"): day 1 Corvettes (hour 7), Shipyard 4, Frigate Tech, the first defences and the first tier-2 derelict (hour 21); day 2 Cruiser Tech, the first Cruiser (hour 46), building slots 2 and 3, Metal 13; day 3 the first Vault, a Cruiser squadron and the last of the 13 techs; days 4 to 6 Ion Bastions, the Cruiser fleet and the Shipyard/Lab ladder to levels 10 and 11, Metal 16 to 19; day 7 the endgame kit. Days 1 to 3 are research-heavy and days 4 to 7 ladder-heavy; the tech tree is exhausted on day 3, which argues for a larger tree (open question 4).
* **The persistent world keeps progressing for months**: the endgame kit lands on day 74 and the world does not stop there. Run for a year with the same player (table "Persistent world, one year"), core score keeps rising from about 2,500 at day 74 to about 22,000 at day 365, Shipyard 16 and Lab 15 (levels 12 and above cost 130k+ metal each and take days to build), Metal 20 on day 66, Crystal 20 on day 180, 29 Cruisers and 148 defence structures. Open question 3 asks whether the cap of 20 is right.
* **x100 plays the whole arc in 13 hours; x8000 in 12 minutes** (floor of one tick per item).

---

## 17. Open questions for the owner

1. **Persistent length.** Is "endgame kit on day 74, still progressing through day 365" the right meaning of months? If you want half, use P = 2 for the persistent preset (everything shown stays valid, just divided).
2. **Does a persistent player wait three and a half days for the first Corvette?** The simulation says so (Shipyard 2, Lab, Corvette Tech and the ship itself). Faster early game options: start with Shipyard 1, or let Corvette Tech need only Shipyard 1.
3. **Raise `MAX_BUILDING_LEVEL` from 20?** Mines reach 20 on day 66 (Metal) to day 180 (Crystal) and then stop being a sink; Shipyard and Lab are effectively capped near 16 to 17 by their cost (Shipyard 20 would take 1265 days to build at pace 1). If mines at 20 end the economy game too early, extend mines only.
4. **Bigger tech tree?** The 13 techs finish on day 3 of a season and day 45 of a persistent world. Candidates: ship variants, defence techs, scanning/cartography, deeper fuel techs, a salvage tech.
5. **Finds scaling `P^0.5`.** It keeps the piloting share near 15 percent in every preset but makes a blitz derelict pay 24x a persistent one. Alternative: a separate `--reward-scale` flag.
6. **Raid cadence.** Roughly one raid per 38 hours in a persistent world for any player with a Shipyard 2 or Grid (24 per season, about three per blitz hour). Should offline persistent players be spared raids entirely (a "sleep-safe" protected window)?
7. **PvP.** Opt-in or forced in the Wandering; newbie protection; do stockpiles above the protected share become lootable?
8. **Aegis Dome and relic** are specified but not simulated. Keep or defer?
9. **Seasons.** Is a reset every 7 days with a hall of fame the plan, and does a persistent world run alongside it?
10. **Score weights.** The 25 percent cap on combat plus exploration is a guess; at the cap exploration cannot change the winner, which may feel wrong for the "exploration pays" decision. Alternatives: 40 percent cap, or separate boards.
11. **Pace after creation.** Fixed for the life of a world (recommended) or admin-changeable with a conversion?
12. **Hull repair at dock.** Gap analysis G9 suggests a deuterium repair sink. Not required here (deuterium is already tight) and not simulated; confirm if wanted.

---

## Appendix A. Constant ledger (old to new)

Values are pace-1 unless stated. "unchanged" rows are listed so a reviewer sees they were considered.

| constant / formula | old | new | reason |
|---|---|---|---|
| tick rate | 1 Hz | 1 Hz | |
| production base m/c/d | 0.5 / 0.3 / 0.15 per tick x L x 1.1^L | 10 / 7 / 5 per hour x L x 1.1^L x P | persistent baseline; deuterium share up |
| building cost | n x base | base x g^(L-1), g = 1.5 to 1.9 | shape fix |
| Deut Synthesizer L1 cost | 225/75 | 150/50 | deuterium rebalance |
| building time | base x L x 1.5^L ticks, base 30 to 90 | (m + c) / 700 h, / (1 + 0.15 Fabricator), / P | tied to cost; Fabricator |
| research cost | n x base | base x 1.6^(n-1) | shape fix |
| research time | 60 x L x 1.5^L ticks; fixed 60/120/240/90 for ship techs | (m + c) / 500 h, / (1 + 0.10 Lab), / P | tied to cost; Lab matters |
| Frigate Tech / Cruiser Tech cost | 1000/600/300; 3000/2000/1000 | 2000/1200/600; 8000/5000/2500 | Cruiser on day 16 / minute 42 |
| ship cost | see 2.4 | unchanged | |
| ship time | 30/60/120/240/90 s base / (1 + 0.1 SY) | (m + c) / 1200 h / (1 + 0.1 SY) / P | cost-linked |
| starting resources, starting fleet, MAX_FLEETS 3 | 500/300/100, 2 Scouts, 3 | unchanged | |
| MAX_BUILDING_LEVEL | 20 | 20 | open question 3 |
| queue depth / building slots | 1 / 1 | 3 / 1 + ModularFabrication | |
| storage caps | none | 5000/3500/2500 x 1.5^V x P^0.75 | decision 2 |
| FUEL_RATE_PER_MASS | 0.1 | 0.1 | |
| refuel at dock | free | 0.08 deuterium per unit | sink, decision 2 |
| Fuel Depot per level | +10 percent | +25 percent | |
| Extended Tanks, Fuel Efficiency | +15 percent, -10 percent per level | unchanged | |
| Harvesting Efficiency | +20 percent extraction rate | +20 percent yield | with tiny reserves |
| Density depletion step | 10/20/30/40 | 3/7/12/20 x ring_mult | |
| ring_mult | none | 1.10^(min(d, 20) - 3) | richer with distance |
| density mix | 40/30/20/10 fixed | drifts with distance (delta = min(0.30, 0.012 d)) | |
| SECTOR_REGEN_RATE | 0.0001 per tick x threshold | step threshold x ring_mult / (120 + 24 d) h x P | regeneration |
| NPC power | 5 / 97 / 480 / 2640 by band | 4 x 1.22^(d - 1) | cliff to gradient |
| NPC presence | 0 / 30 / 70 / 80 percent by zone | min(85, 25 + 2.5 d) percent | |
| NPC respawn | 300 / 600 / 1200 s | (2 + 0.25 d) h / P | economy timer |
| salvage | 0.30 x cost of ONE ship | 1.2 x power^0.8 x P^0.5 (split 55/30/15) | class and ring, bug fix |
| SALVAGE_DESPAWN_TICKS | 60 | 60 | real time |
| derelict tier | by distance 8 / 20 | by threat 3 / 6 | |
| derelict loot | tier ranges 300 to 3500 metal | 240 x 1.14^(d - 4) x U(0.6, 1.4) x P^0.5 | |
| derelict ambush | 0.10 / 0.20 / 0.30 | min(0.35, 0.10 + 0.03 (T - 1)) | |
| ship recovery | 0 / 0.15 / 0.25 | clamp(0.05 (T - 2), 0, 0.30) | |
| data core | 0 / 0 / 0.10 | clamp(0.03 (T - 5), 0, 0.15) | |
| derelict respawn | never | 120 h / P^0.5 | |
| EXPLORE_DURATION, scout and hauler modifiers, retry bump | 20 s, -0.10, x2, +0.10 | unchanged | |
| raid roll | every 300 s, 20 percent | every 21,600 s / P^0.75, 30 percent | |
| raid warning | 60 s | clamp(600 / sqrt(P), 30, 600) s | |
| raid minimum interval, minimum age, suppress | 900, 600, 1800 s | 64,800, 86,400, 172,800 s / P^0.75 | |
| raid power | 35 to 55 percent of own defence | (5 + 11 S^0.75) x U(0.8, 1.15) | defence matters |
| raid loss caps | 8 / 8 / 5 percent of stockpile | same, above the protected amount | Vault role |
| defence | Defense Grid virtual platforms 2/5/8, x1.35 | 1.0/2.5/4.0, x1.3, plus structures | decision 2 |
| defended-raid salvage | 30 percent | 30 percent | |
| cancel refund | 50 percent | 50 percent active, 100 percent queued | |
| scan reveal, cooldowns, combat rounds, shield regen, policy intervals | as today | unchanged | real time |
| zone radii 8 / 20 | | unchanged (terrain and edge rules only) | |
| scoring | none | section 9 | decision 5 |

## Appendix B. The ten changes that matter most

1. **Pace flag** with presets x1, x10, x100, x600, x8000 (section 1).
2. **Cost and time curves** share an exponent and time is derived from cost: `time = (m + c)/rate`, cost `base x 1.5^(L-1)` (was `n x base` cost against `L x 1.5^L` time).
3. **Production rebased** to 10/7/5 per hour with deuterium raised from 30 to 50 percent of metal; Deut Synthesizer 225/75 down to 150/50.
4. **Storage Vault**: base caps 5000/3500/2500, x1.5 per level, x P^0.75; production discarded at the cap; raid-protected share.
5. **Building slots 1 to 3** (Modular Fabrication), queue depth 1 to 3, Fabricator -15 percent, Lab -10 percent research time.
6. **NPC power** `4 x 1.22^(d-1)` (an 18x cliff at ring 9 becomes 1.22x per ring), shown as threat 1 to 9.
7. **Ore**: reserves 3/10/22/42 x `1.10^(d-3)`, richness drift, refill in `120 + 24 d` hours, and `Harvesting Efficiency` now raises yield.
8. **Loot**: wreck `1.2 x power^0.8` (was 30 percent of one ship), derelicts `240 x 1.14^(d-4)` by threat tier, respawning, with rare finds; all x `P^0.5`.
9. **Refuel costs 0.08 deuterium per fuel unit**, Fuel Depot +25 percent per level.
10. **Cruiser gates**: Cruiser Tech 3000/2000/1000 to 8000/5000/2500 and Frigate Tech 1000/600/300 to 2000/1200/600, plus the raid and score rework (raids sized from points, structures, `core + min(extra, 25 percent)`).
