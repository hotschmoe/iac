# In Amber Clad: starting positions, fairness and expansion

Date: 2026-10-10. Scope: measurement, web research and design proposals; no game code was changed.

**Source note.** Three kinds of statement appear here and are kept apart. *Measured* means produced by running the repository's real world generation and engine (`iac-shared`, `iac-sim`) from a throwaway harness; section 9 says how. *Sourced* means taken from the web; each claim carries its URL and a tag: **[F]** the page was fetched and read, **[S]** search snippet only, page not fetched. Fandom wikis (HTTP 402), the Wayback Machine, the old spellbook.com wiki and forum (404) and Steam guides (429) were unreachable, so several facts about The Infinite Black rest on snippets or could not be confirmed; section 8 lists them. *Judgment* means a design proposal of this study; engineering sizes are estimates from reading the code, not measured effort. IAC numbers are quoted from `shared/src/constants.rs`, `shared/src/scaling.rs`, `shared/src/world.rs`, `shared/src/pace.rs` and `sim/src/engine.rs` as of this date.

---

## Headline

- **Pure builders:** position does not matter at all. Identical check-in builders score identically at every slot.
- **Fleets:** position matters a lot, in three ways:
  - **Ring-6 cliff:** the starter fleet wins every fight in rings 1-5 and loses almost every fight from ring 6 outward.
  - **Lairs on home tiles:** about 17% of ring-6 home tiles hold a pack that kills the starter fleet, and a fleet returning home fights it there.
  - **Reach:** home ring fixes how far a fleet can fly and still get home.
- **Capacity:** a world is fair up to about 21-30 players, and registration fails outright at about 66.
- **PvP line:** spill placement puts homes in the Outer Ring (PvP space under the zone rule below) from about the 47th player.

## 0. Design direction this study assumes

The owner set the following direction while the study ran; the proposals below are built on it.

- **Worlds joined at the hub** is the leading expansion: another "dimension" is a portal at the central hub to other worlds, each still a flat (q,r) grid. It is worked out in section 5; the alternatives are compared briefly.
- **PvP by zone:** no PvP in the Inner Ring, multiplayer PvE everywhere, PvP proper only outside the Inner Ring. Section 1.2 measures where homes land against the ring 8/9 line.
- **No homeworld relocation.** Outposts instead, placeable anywhere from ring 9, with the richest sites further out (section 5.3).
- **Hub** starts as one hex doing two jobs (an NPC exchange and the gate between worlds) and can grow into a seven-hex cluster as chat, items and an auction arrive (section 6).
- **Exchange** is a surplus sink, never a faucet, with a per-player rate that worsens with volume and recovers over time (section 6.3). A world-wide slide is a possible later step.
- **Black market** sits in dangerous space, moves on a schedule derived from the world seed, is found again by exploring, and trades resources until an item system exists (section 6.4).
- **Gateway** between home and outpost: no jump in combat, it carries loot, and it has a researched bandwidth (section 5.4). Two tentative ideas are evaluated there: a one-time move of an outpost out to ring 21+, and "research raises how far out an outpost may be placed".

## 1. Are some starting positions better? (measured)

### 1.1 What placement does

Code: `find_homeworld_location` in `sim/src/engine.rs` (line 4139).

- The RNG is seeded from `world_seed ^ player_id`.
- For each spill level 0 to 4 it makes 100 tries: pick a radius uniformly in 3..(6+spill), pick a point inside that disc, keep it if its ring is in band and no other home is within 1 hex.
- It does not look at terrain, lairs or lanes.
- If every try fails, registration fails and the client sees "could not create the account".

### 1.2 Fill curve and the PvP line (48 seeds)

| Event | Join number (min / median / max) |
|---|---|
| First home in a spill ring (ring 7+) | 21 / 27 / 30 |
| First home in the Outer Ring (ring 9+, PvP space) | 35 / 47 / 53 |
| Registration refused | 59 / 67 / 78 |

- When a world is full, 15 homes on average (7 to 24) sit in the Outer Ring, and 28 are at ring 8 or beyond.
- At 60 players, 53% of homes are within 2 hops of PvP space.

### 1.3 What a slot looks like by ring (8 seeds, every hex as a candidate home)

| Ring | Ore within 4 hops, mean (p10-p90) | Hostile lairs within 2 hops | Home tile holds a hostile lair | Hops to nearest lair that kills the starter fleet (median) | Derelicts within 6 hops | Hops to ring 9 / 15 / 21 |
|---|---|---|---|---|---|---|
| 3 | 1140 (915-1370) | 3.2 | 21% (scout, harmless) | 3 | 2.0 | 6 / 12 / 18 |
| 4 | 1267 (1019-1495) | 3.1 | 14% (scout, harmless) | 3 | 2.2 | 5 / 11 / 17 |
| 5 | 1379 (1141-1609) | 3.2 | 18% (scout, harmless) | 2 | 2.5 | 4 / 10 / 16 |
| 6 | 1504 (1244-1746) | 3.3 | 17% (corvette, lethal) | 1 (10% of slots: 0) | 2.8 | 3 / 9 / 15 |
| 7 | 1619 | 3.6 | 19% | 1 | 3.1 | 2 / 8 / 14 |
| 8 | 1740 | 4.1 | 18% | 1 | 3.2 | 1 / 7 / 13 |
| 9 | 1836 | 4.6 | 30% | 1 | 3.2 | 0 / 6 / 12 |
| 10 | 2008 | 5.4 | 35% | 1 | 3.4 | 0 / 5 / 11 |

- **Nobody is boxed in.** Homes in the band have 4 to 6 lanes (mean 5.7) and reach 54 to 60 of the 60 sectors within 4 hops.
- **Lanes ignore the world seed.** `edge_exists` in `shared/src/world.rs` hashes only the two sector keys. The lane maze was identical between two seeds for all 2,791 sectors compared (rings 0-30).
- **A few sectors cannot be entered.** In rings 0-40, 24 sectors have no surviving lane; the "at least one connection" guarantee in `sector_connections` gives each a single outbound lane that the neighbour does not mirror. None is in the home band.

### 1.4 The ring-6 cliff

The starter fleet (2 scouts, power 18) was sent into hostile lairs with the real engine.

| Ring of lair | Lair | Real power | Starter fleet survives |
|---|---|---|---|
| 1-5 | scout patrol | 5.6-6.3 | 100% (123 fights), no hull lost |
| 6-10 | corvette patrol | 15.8-17.6 | 0-4% (377 fights) |
| 11-12 | 2x corvette patrol | 36-37 | 0% (192 fights) |

- The economy spec says threat grows 1.22x per ring with no cliff. In practice the class switches to Corvette at ring 6 and the count rounds up to 1, so real power is 15.8 against a nominal 10.8, and corvettes have rapid fire against scouts.
- The ring estimate for an unscouted ring-6 sector rates the fight at 1.67, FAVOURABLE. So the default doctrine enters and dies.
- All 37 starter fleets lost in day 1 across the 108 band slots of the default seed (three doctrines) died at ring 6 to a corvette patrol. Fourteen of them died on their own home tile.
- Verified: a fleet returning to a home tile with a hostile lair fights there. Of two ring-6 homes tested, one fleet was destroyed and one survived with a third to two thirds of its hull gone.

### 1.5 The default world with 28 players

| Measure | Min | Median | Max |
|---|---|---|---|
| Ore within 4 hops | 1104 | 1399 | 2151 |
| Deuterium ore within 4 hops | 48 | 245 | 436 |
| Ore within 4 hops after sharing with every home that reaches it | 114 | 259 | 890 |
| Hostile lairs within 2 hops | 1 | 3 | 6 |
| Hops to nearest lair that kills the starter | 0 (3 homes) | 2 | 4 |
| Derelicts within 6 hops | 0 | 3 | 6 |
| Other homes within 4 hops | 5 | 8 | 11 |

Every home has a neighbour exactly 2 hexes away, and 79-100% of each home's 4-hop area is also inside another home's.

Crowding as a world fills (8 seeds pooled, real placement):

| Players | Nearest neighbour (hexes, mean) | Other homes within 4 hops (mean) | Share of the 4-hop area that is contested | Fair share of local ore against a solo player |
|---|---|---|---|---|
| 1 | - | 0 | 0% | 100% |
| 4 | 4.3 | 0.6 | 52% | 74% |
| 8 | 2.8 | 1.8 | 83% | 51% |
| 12 | 2.4 | 3.3 | 90% | 38% |
| 16 | 2.2 | 4.5 | 93% | 31% |
| 20 | 2.1 | 5.6 | 95% | 27% |
| 28 | 2.0 | 7.4 | 97% | 21% |
| 48 | 2.0 | 9.3 | 98% | 17% |
| 64 | 2.0 | 10.0 | 98% | 16% |

### 1.6 Does it change outcomes?

**Ore barely matters.**
- The whole Inner Ring holds about 5,200 ore units and refills about 490 per day at pace 1. That is about what one new player's two level-1 mines produce (449 per day).
- A mid-game empire makes about 15,700 weighted units per day.
- So contested ore is real but nearly irrelevant after the first day.

**Identical players at different slots (season pace).** Runs are deterministic and the builder control has zero spread, so every difference below is position, not noise.

| Script | Result |
|---|---|
| `sim-run`, 28 and 60 identical builders, 7 days | Every score exactly 1422.11 |
| Builder alone at each of 624 hexes (rings 3-10, 2 seeds), 3 days | Every score identical |
| Builder + mine-and-return, full worlds, 3 days | Core identical; total spread 0.7% (best/worst 1.03) |
| Builder + prospect, full worlds, 3 days | Spread 2.7-2.9% (best/worst 1.09-1.10); correlation with ring -0.3 at 28 players, -0.7 at 60 |

- The 25% cap on combat plus exploration bounds how far this can go.
- The mine-and-return fleet harvested 143 units in rings 3-6, 2-6 in rings 7-8, and 0 in rings 9-10.
- By join order in 60-player worlds it harvested 170, 78, 8, then 0.
- Prospecting earned 8.7 points in the band, 2.3 at rings 7-8, and 0 at rings 9-10.
- At rings 8-10 the default doctrines never leave home, because every exit fails the 1.5x entry rule.

**Caveat on a larger number.** With a builder that checks in every 300 ticks, the mining variant scored 19-23% higher in rings 3-5. That is a bot artefact, not ore income: refuelling drained deuterium, which forced the "cheapest affordable" builder into mines first. It is not counted as a position effect.

**What plausibly changes outcomes (judgment):**
1. Losing the starter fleet early: ring 6 and spill slots.
2. Reach: one tank gives Scout 10 hops out and back, Hauler 6, Corvette 5, Frigate 3, Cruiser 3. A ring-6 home reaches three rings further than a ring-3 home at equal tech.
3. Under the zone PvP rule, spill homes inside PvP space.

## 2. How to balance it (judgment, grounded in the above)

1. **Fixed home slots, no spill.** Use a slot table in rings 3-5, or one or two rings only so hub and frontier distance are equal. Hand slots out farthest-from-occupied first. Remove `HOMEWORLD_SPILL_RINGS`; a full world opens the next world instead.
2. **Home tiles never hold a lair.** Clear the home tile, and preferably its six neighbours, at registration.
3. **Remove the ring-6 cliff.** Either keep scout-class lairs through ring 8, or make the ring estimate use the real composition power so labels are honest. Then re-check with the combat run.
4. **Seed lanes with the world seed.** This needs a `WORLDGEN_VERSION` bump.
5. **Do not equalise ore.** It does not affect outcomes.

## 3. What OGame and The Infinite Black do (sourced)

Tags: **[F]** the page was fetched and read; **[S]** search snippet only.

**OGame**
- Home planets have 163 fields whatever the slot. [F] https://ogame.life/ogame/blog/ogame-planet-sizes-2026-complete-guide-to-fields-slots-colonization-strategy
  - A Fandom snippet says 150-190. [S] https://ogame.fandom.com/wiki/Fields
- Coordinates are "randomly assigned". [F] https://en.wikipedia.org/wiki/OGame
- Slot effects apply to colonies. [F] https://board.us.ogame.gameforge.com/index.php?thread%2F101991-colonization%2F=
  - Slot 1: 96-172 fields, 260 C, +40% crystal.
  - Slot 8: 156-252 fields, +35% metal.
  - Slot 15: 90-164 fields, -90 C, best deuterium.
- Layout: 4-9 galaxies, 499 systems, 15 slots. Donut galaxies and systems are settings. [F] https://forum.origin.ogame.gameforge.com/forum/thread/32-universe-types-settings/
  - With wraparound, systems 100 and 400 are 199 apart. [F] https://board.en.ogame.gameforge.com/index.php?thread/810246-guide-00-universe-guide/
- Late joiners: no placement rule found. Newbie protection uses point ratios (1:5 to 50k points, 1:10 to 500k). [F] https://board.us.ogame.gameforge.com/index.php?thread%2F103097-newbie-protection-stronger-newbie-mechanics%2F=
- Relocation costs 240,000 dark matter with a 24 h countdown. [F] https://sidian.app/s/ogame-wiki/planets-moons
- Emptying universes are merged into target universes; planets move and the system tries to keep coordinates. [F] https://board.en.ogame.gameforge.com/index.php?thread%2F851917-merges-2025%2F=
- Capacity: 9 x 499 x 15 = 67,365 slots (arithmetic from the layout above). An old article quoted a 14,000-player maximum. [F] https://en.wikipedia.org/wiki/Talk:OGame
  - French universes on 2026-10-10 range from 62 to 2,221 active players. [F] https://www.mmorpg-stat.eu/?pagex=40
- The merchant swaps resources for dark matter, capped by free storage (Wikipedia, above). The rate is about 3:2:1. [S] https://ogame.fandom.com/wiki/Resource_Market

**The Infinite Black**
- Everyone starts at Earth, the hub for buying ships. [F] https://mmos.com/?p=137659
- Zones: blue sectors allow no combat; grey is PvP-free newbie space; black is "anything goes". [F] https://www.spellbookgame.com/tib/tutorial.php
  - Grey is about 10 sectors around Earth. [F] https://zeus-tib.forumotion.com/t20-new-player-guide
- Starports buy, sell and repair, and there is an auction house (tutorial, above). There is one fixed starport at Earth. [S] https://steamcommunity.com/sharedfiles/filedetails/?id=775977268
- The Rift is "a second map layer" with end-game content. Servers (Red, Blue, Green, Grey, Gold) were separate worlds; Red, Green, Blue and Grey merged into "Unity" in 2016. [F] https://steamcommunity.com/app/489000/discussions/0/343788552527704360/
- Not verified:
  - the 3x3 central cluster and per-sector purposes;
  - a black market sector;
  - whether Earth is at the geometric centre;
  - the 80x80 map size (one forum snippet only);
  - peak concurrent players.

**Comparison points**
- Travian: players pick a quadrant; early players sit in the middle and later ones outward. [F] https://travianlibrary.wordpress.com/2009/03/14/148/
- EVE: starter systems with PvP disabled, then a security gradient. [F] https://www.eveonline.com/news/view/introducing-exordium

**Lesson (judgment).** Neither game equalises positions inside a crowded world. OGame makes home planets identical and opens new universes. TIB starts everyone in one protected core and scales danger with distance.

## 4. How many players is fair?

**Definition, in measurable terms:**
- **(a) Safe:** no lair on or beside the home that kills the starter fleet.
- **(b) Protected:** home at least 3 hops inside non-PvP space.
- **(c) Equal reach:** hub and frontier distance within 2 hops of every other home.
- **(d) Equal outcome:** identical scripted players within about 5% of each other.
- **(e) Elbow room** (optional): each home keeps at least half of a solo player's local ore.

**Numbers under the current geometry:**

| Definition | Players per world |
|---|---|
| (a) to (d) | About 21-30, the point where spill starts |
| (a) to (d) with ring 6 excluded until the cliff is fixed | About 20-24 |
| (e) elbow room | About 8 (fair share is 51% at 8, 21% at 28) |
| Hard limit | About 66 (59-78) |

**What breaks first as the count rises:**
1. About 8: local ore is contested. Harmless.
2. About 27: spill begins. Delegated starter fleets do nothing or die.
3. About 47: homes land in PvP space.
4. About 66: registration fails with a generic error.

Tick cost is not a limit. `sim-run` ran 28 builders at 27,700 ticks/s and 60 at 16,100. The two 60-player worlds with active fleets that were timed ran at about 5,000-6,000 ticks/s. Chart size grows with exploration, not player count.

## 5. Expansion

### 5.1 Worlds joined at the hub (leading option)

**Structure (judgment, checked against the code).**
- One `GameEngine` per world, each with its own seed, `Snapshot` and SQLite file.
- A pure `Cluster` type in `iac-sim` owns the engines, the accounts, the hub state and fleets in transit.
- Coordinates stay (q,r). The world is a property of the session, not of `Hex`.

**Determinism.**
- The cluster ticks worlds in id order.
- A gate crossing removes the fleet from world A at tick t and inserts it into world B at tick t+k as a logged input.
- A cluster snapshot is the world snapshots plus hub state plus transits.

**Persistence.**
- The seven (q,r)-keyed tables are unchanged, one database per world.
- A small cluster database holds accounts, the world registry, exchange meters and a transit outbox, so a crash between two world writes can be replayed.
- Engine ids are per-engine counters, so an arriving fleet gets fresh ids.

**What crosses.**
- Fleets with fuel and hold cargo cross.
- Charts stay in the world they describe.
- Stockpiles, buildings and outposts never cross.
- Score is one ranking: home-world core plus combat and exploration from every world, with the 25% cap applied to the total.
- A visitor's "home" in a foreign world is the hub hex. Every home-route and fuel rule keeps working; visitors dock there to refuel and deliver charts but do not unload.

**Constraints.**
- Gated worlds must share one pace. Otherwise cargo carried from a fast world to a slow one is a faucet.
- Gate direction: fleets may enter worlds as old as or older than their home world, never younger. This stops veterans descending on a new cohort.

**Growing dynamically.**
- Open a new world when the newest one's home slots fill, or when it is older than a set age. This replaces the spill rings.
- New players always go to the newest world, so cohorts stay close in age.
- The planned built-in bots seed a new world so it is not empty.
- A join code can let friends pick an older world knowingly.
- Widening an existing world is the wrong lever. Rings 6 and beyond carry the cliff, ring 9 and beyond is PvP space, and it widens the reach gap.

**Single-player wasm build.** A cluster with one world is exactly today's engine. Extra bot worlds multiply tick cost and the fast-forward cost on reopening the tab.

**Cost: (world,q,r) against true 3D.**

| Area | Worlds joined at the hub | True 3D |
|---|---|---|
| `shared/` hex math and worldgen | Unchanged | `Hex`, the u32 key packing, 6-direction lists and masks all change |
| Rust call sites | Cluster type, visitor players, transit | 278 `Hex` mentions and 102 key uses to review |
| Protocol | World id in `WorldInfo`, about three new messages | Every struct carrying a `Hex`; all fixtures regenerated |
| SQLite | Unchanged per world; one new cluster database | Seven tables' columns and primary keys |
| Server | `network.rs` and the tick loop map a session to (world, player) | Unchanged structure |
| Flutter | World switcher and hub panel; a transfer is a new `full_state` | Map painter, map view and map panels gain layers; 3,213-line protocol mirror |
| TUI | A world name and a gate command (it clears and reloads on `full_state` already) | Star map renderer |
| Readability on a 2D amber map | Unchanged | Worse |

### 5.2 Alternatives (what is given up)

| Rank | Option | Fairness and capacity | Feel and readability | Cost |
|---|---|---|---|---|
| 1 | Worlds joined at the hub | About 24 fair seats per world, unbounded worlds, cohorts | Map unchanged; "via world x" | Medium for shards with shared ranking; large with fleet crossing |
| 2 | Several hubs on one plane | Same capacity; no cohorts or per-world seasons | Still 2D; hubs are 60+ hops apart | Medium: 21 Rust and 22 Dart distance-from-origin call sites, worldgen and placement |
| 3 | Seasonal worlds | Resets all advantage; no capacity gain alone | Unchanged | Small; pace presets exist |
| 4 | Late-joiner catch-up inside one world | Fixes time, not position | Unchanged | Small to medium; needs a rule nobody can farm |
| 5 | Larger inner ring or wider band | About 66 seats, but unequal reach and hub access | Unchanged | Small in code, medium to large in rebalancing |
| 6 | Stacked z-planes joined near the hub | Same as option 1 | Layer UI needed | Large: all the 3D cost for no gain over option 1 |
| 7 | True 3D adjacency | More room, no fairer | Poor on a 2D terminal | Largest |

### 5.3 Outposts, from ring 9 outward (design proposal)

The starting point is the outpost sketch in `gap-analysis-2026-10-09.md` (G6).

- **Limit:** one per player at first.
- **Founding:** an Outpost Module ship is consumed on a free sector at ring 9 or beyond.
- **Dock:** unload, refuel, repair. This removes the reach gap between ring-3 and ring-6 homes, which is its main fairness value.
- **Extractor:** mines only what the tile holds. Level cap rises with distance: 3 in rings 9-14, 4 in rings 15-20, 5 in the Wandering.
  - Level 3 makes 40 / 28 / 20 per hour at pace 1, about 19% of a mid-game home's weighted income.
  - Level 5 makes 81 / 56 / 40 per hour, about 13% of a late home's.
- **Store:** small and separate, about 20% of a Vault-0 cap. Production stops when it is full.
- **What an attacker takes:** a capped share of the outpost store above a protected slice, plus structure damage. Never the home stockpile, buildings or queues.
- **Score:** outpost levels count in core while it stands. Stolen goods score only when spent. PvP points fall under the 25% cap.
- **Order of work:** outposts can ship before PvP as forward bases under NPC pressure.
- **Engine cost:** large. 22 code paths in `engine.rs` test "is this the player's homeworld" and would become "is this one of the player's docks". Add a new table, protocol state, fixtures and map symbols.

### 5.4 Gateway and the endgame ideas

**Bandwidth.**
- **Unit:** mass, with ships and cargo both counting. Ship mass is hull, the number fuel already uses. Cargo mass is units divided by P^0.5, so a full hold weighs the same at every pace.
  - A cargo-only cap would make the gate a free military teleporter.
  - A ship-count cap would let Haulers bypass the rings.
- **Capacity:** 60 x 1.6^(level-1), giving 60 / 96 / 154 / 246 / 393 over five levels.
- **Recharge:** one full capacity per window. The window is Finds class, base 21,600 ticks: 6 h persistent, 1.9 h season, 14.7 min blitz.
- **Banking:** up to four windows, so a check-in player loses nothing to a hands-on one.

**Example.** A level-3 outpost makes 527 units per window (pace-normalised) at every pace, which is 2.6 Hauler loads.

| Gate level | What a round trip moves per window | Share of a level-3 outpost's output |
|---|---|---|
| 1 | A Scout with about 15 cargo | 3% |
| 2 | A Scout with about 24 cargo | 5% |
| 3 | Scouts with about 38 cargo | 7% |
| 4 | A Hauler with about 86 cargo | 16% |
| 5 | A Hauler with a full 200 hold | 38% |

Halve the shares for a level-5 outpost. Bulk still goes by Hauler through the rings.

**One line or two.** One line (bandwidth) is enough. If a second is wanted, make it bank depth, not cooldown: cooldown only rewards being online.

**PvP.**
- Jumps only from a dock, not from the field, and not in combat.
- Bandwidth limits how much of a fleet can evacuate.
- The existing `recall` already teleports a fleet home with its cargo from anywhere, and it works during combat. In one trial the cargo was banked, the fleet stayed enrolled in the fight and was destroyed at home. That needs the same no-combat rule before PvP.

**One-time outpost move to ring 21+.** Not recommended as a special move. "Research raises how far out an outpost may be placed" is the simpler form: for example maximum ring 12 / 16 / 20 / 24 / 30 by level, with dismantle and re-found at the existing 50% refund.

**Tech tree.** Two lines, because they serve different players:
- Outpost reach, behind Hauler Tech: serves explorers.
- Gateway, behind Emergency Jump: serves check-in logistics.

### 5.5 Recommended path

1. **Cheap first step** (sim and shared only, no protocol change):
   - fixed home slots with no spill and a clear "world full" result;
   - no lairs on or beside homes;
   - fix the ring-6 cliff or the estimate;
   - seed lanes from the world seed.
2. Multi-world host with one account, one ranking and auto-open.
3. Hub exchange.
4. Gate crossing.
5. Outposts, then PvP, the black market and the gateway.

## 6. The centre

### 6.1 Today (measured)

- (0,0) is Empty terrain with no ore, no NPC and no derelict. It has six guaranteed lanes.
- No rule in `sim/` treats it specially. The clients only label it.
- Ring 1 holds 72-158 ore units in total, 1.5-2.8% of Inner Ring ore. It has 1-4 lairs, of which 0-2 are hostile scout patrols.
- Converting all seven hexes (the centre plus ring 1, the hex equivalent of a 3x3) displaces almost nothing.

### 6.2 Hub shape

- Start with one hex doing exchange and gate.
- It should also sell fuel. A Hauler's one-tank reach is 6 hops, so a ring-6 home barely makes the round trip and spill homes cannot.
- Homes in rings 3-5 reach the hub entirely through non-PvP space, so camping is not possible under the zone rule.
- Seven-hex purposes:
  - Supportable now: exchange, gate, refuel and repair.
  - Needs chat: social.
  - Needs items and chat: auction, upgrades.

### 6.3 Exchange: per-player sliding rate

**Rule.**
- Par values are the score weights: metal 1, crystal 1.5, deuterium 2.
- Each player has a pressure meter in weight units.
- The marginal fee is 25% at zero pressure, rising linearly to 75% at pressure M, and flat beyond.
- The fee is integrated over the trade, so splitting an order changes nothing.
- M is 15% of the player's own mine output over one window.
- The window is Attention class, base 86,400 ticks. The meter drains M per window.
- State is two numbers on the player row.

**Examples for a mid-game empire (mines 10 / 9 / 8).**

| Pace | Window | Output per window (weight) | M | One Hauler of metal at an empty meter |
|---|---|---|---|---|
| Persistent | 24 h | 15,700 | 2,353 | 200 metal gives 73 deuterium (27% fee) |
| Season | 4.3 h | 27,900 | 4,185 | 632 metal gives 225 deuterium (29% fee) |
| Blitz | 11.9 min | 77,600 | 11,647 | 4,899 metal gives 1,580 deuterium (35.5% fee) |

Selling a whole M at once averages a 50% fee.

**Why it cannot loop or raise score.**
- Every trade returns at most 75% of the weighted value put in. Any cycle across resources returns at most 56%.
- Core score prices everything with the same weights, so a stockpile's maximum score only falls.
- The one gain is spending what would have been lost at the storage cap, which is the purpose.
- Across worlds: par is one constant, the meter lives on the account, and gated worlds share a pace.
- Receipts land in the hold and unload only up to the cap, an existing rule.

**Compared with a hard cap.** A hard per-window cap is simpler to explain. But with no wall clock it resets on a tick boundary for everyone, which rewards logging in at reset.

**World-wide slide.** It favours whoever is online first. Keep it as a later step.

### 6.4 Black market

**Location.**
- A pure function of world seed and epoch (tick divided by the interval). Nothing about location is persisted.
- Berths are Anomaly sectors in rings 12-20. Four seeds have 41-58 of them, all with two-way lanes, and about two thirds hold a lair.
- Anomalies already emit the "anomalous emissions" scan signal and do nothing today.

**Interval.**
- Economy class, base 432,000 ticks: 5 days persistent, 12 h season, 12 min blitz.
- That is about 15 moves per world arc at every pace.
- Finds class would give under one move per blitz.

**Finding it.**
- An occupied berth shows a distinct signal at scan range and a pin in the sector view.
- `pins_stale` fits: it already marks remembered salvage and derelict pins on non-live sectors (`sim/src/intel.rs` line 334), and a look that finds nothing drops the pin.
- A known list of berths is what makes charting pay.

**What it trades.**
- A few seeded lots per epoch, each "wants X, pays Y".
- Flat 10% fee, no per-player meter, a finite world-wide stock.
- Suggested stock: about 4,000 weight per lot times P^0.5.

**Why it is worth the trip.**
- Season example: a six-Hauler convoy of metal (3,792 weight) returns 1,986 at the hub from an empty meter and 3,413 at the market.
- With the hub meter saturated, the same convoy returns 948 at the hub.
- It is still below par, so no loop exists with the hub.

**Difference from the hub.** The hub is safe, always there and shallow. The market is dangerous, moving, deep but finite, and contested.

**Replacement by items.** Define the wire shape as lots (goods, price, stock) from day one. When items exist, the lot generator emits items and the resource lots are deleted in the same change.

## 7. Open decisions for the owner

1. Home band: rings 3-5, or a single ring? And seats per world (about 24)?
2. Ring-6 fix: scout-class lairs through ring 8, or only an honest estimate?
3. Expansion phase 1 as shards with one ranking before fleets can cross?
4. Gate direction rule (older worlds only) and one pace per cluster?
5. Outposts: one per player, level caps by zone, and can an outpost be destroyed outright?
6. Exchange numbers: 25-75% fee and M at 15% of window output?
7. Black market interval class (Economy recommended) and whether its departure time is shown.
8. Gateway: mass-based bandwidth with a four-window bank; a second line for bank depth or none?
9. `recall` during combat: forbid it before PvP?
10. Seed lanes per world, accepting a worldgen version bump?

## 8. Not measured or not verified

- No competent hands-on or hybrid bot exists in the repository. The fairness runs use the check-in builder plus default standing orders on the starter fleet, at season pace, for 3 days. A 7-day run with fleets in full worlds was too slow and was stopped; no number in this document comes from it.
- The mine-and-return doctrine took only about 380 of roughly 1,400 reachable ore units in 3 days, so the harvest figures understate a human miner.
- The `recall` in combat finding is one trial.
- Player-versus-player combat does not exist in the engine, so every PvP statement is design judgment.
- Engineering sizes are estimates from reading the code, not measured effort.
- TIB's 3x3 hub, its black market, its map size and OGame's exact home placement rule could not be sourced (section 3).
- No background run finished after the study report was first returned, so no number here differs from it; the fight counts in section 1.4 were re-tallied from the combat run's own output when this file was written.

## 9. How it was measured

Everything ran from a scratch cargo project that depended on `iac-shared` and `iac-sim` by path, so world generation, placement, combat, standing orders and scoring were the repository's own code. The harness did not persist. Seeds: the default world seed `0xDEADBEEFCAFEBABE` plus 1, 2, 3, 42, 1337, 20261010 and `0x123456789ABCDEF0` ("8 seeds"); the fill-curve run added 40 more arithmetic seeds ("48 seeds"). All engine runs used the season pace (x10).

- **Static geometry.** For every hex of rings 1-12 as a candidate home, on 8 seeds, a lane breadth-first search from `WorldGen::connected_neighbors` counted lanes, sectors reachable in 2/4/6 hops, hops to the hub and to rings 9/15/21, ore reserve (`ore_reserve_units` per resource) within 2 and 4 hops, hostile lairs (non-passive `npc_template`) within 1/2/4 hops with their real power, and derelicts within 4 and 6 hops. It also compared lane masks between two seeds and counted one-way lanes in rings 0-40. Source of sections 1.3 and 6.1.
- **Placement and crowding.** Players were registered through `GameEngine::register_player` until registration failed, on 8 seeds (per-slot tables, crowding by player count from 1 to 64) and on 48 seeds (join number of the first spill-ring home, first Outer Ring home and refusal). Neighbour distance, homes within 2 and 4 hops, contested area and fair-share ore were computed from the resulting homeworlds. Source of sections 1.2, 1.5 and 4.
- **Zone ore supply and reach.** Standing ore and refill per pace-1 day (`ore_regen_hours`) were summed by ring band over 8 seeds; one-tank round-trip reach per ship class came from `base_stats` and `FUEL_RATE_PER_MASS`. Source of sections 1.6 and 6.2.
- **Starter-fleet combat.** On 8 seeds, up to 12 hostile lairs per ring (rings 1-12) were each entered once by a fresh 2-scout starter fleet placed on a neighbouring sector through a `Snapshot` edit; the engine was ticked until the fight ended. 692 fights in all. Source of section 1.4.
- **Home-tile lair check.** On the default seed, a player was re-homed (through a `Snapshot` edit) onto three home tiles with hostile lairs, moved one hop out and back, and ticked for 300 ticks.
- **Doctrine deaths.** On the default seed, one player alone at each of the 108 band hexes, for each of the mine-and-return, prospect and salvage-and-sites doctrines, one day; the sector and killer of the first fleet loss were recorded.
- **Solo fairness runs.** One player alone at every hex of rings 3-10 (312 hexes) on two seeds, with the repository's `CheckInBuilder` plus one standing order on the starter fleet set through `handle_policy_update`: one day with the hourly builder for four scripts (builder only, mine, prospect, sites), and three days with a 300-tick builder for three scripts (builder only, mine, sites). Score, harvest, salvage, fleet losses and raids were tallied from engine events.
- **Crowd fairness runs.** Full worlds of 28 and 60 players by real placement on four seeds, hourly `CheckInBuilder` plus one doctrine (mine, sites, prospect), three days. Also the repository's own `sim-run --pace season --builders 28|60 --days 7` on the default seed, which gave the identical-builder scores and the tick rates in section 4.
- **Recall in combat.** One trial on the default seed: a starter fleet carrying cargo entered a ring-6 corvette lair three hexes from home, `handle_recall` was issued once combat had started, and the engine was ticked for 60 ticks.
- **Anomaly berths.** Anomaly-terrain sectors were counted by ring band on four seeds, with whether each has a two-way lane and a lair. Source of section 6.4.
- **Design arithmetic.** Exchange, gateway and outpost examples were computed from `production_per_tick`, `storage_cap`, the `Pace` multipliers and ship stats; they are worked examples, not simulation output.
