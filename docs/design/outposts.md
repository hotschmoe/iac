# Outposts

Date: 2026-10-11. Status: proposal for owner review. No game code was changed
when it was written. Since then the engine half of slice 1 has been built on the
recommended answers to the six questions of section 8, which the owner has not yet
given: `decisions.md` (2026-10-11, outposts) says where each lives in the code, and
the journal entry "outposts, slice 1: the engine" lists where the build departs
from section 7. The client half (7.6) followed: the journal entry "outposts,
slice 1: the clients" says what the web console and the TUI show and where they
depart from 7.6. The measurement section 7.8 asks for has been made: its results
stand under the table there, and no constant moved. The rest of the text below is
the proposal as it was written.

**Source note.** *Decided* means an owner decision, quoted in section 2. *Measured*
means read from the repository's code or produced by its engine; section 9 says
what was run. *Proposed* is everything else: every number in this document that
is not quoted from the code or the journal is a proposal, provisional in the way
the four `EXCHANGE_*` constants are.

---

## 1. The proposal on one page

**What it is.** An outpost is a small station a player builds on one sector of the
Outer Ring (rings 9 to 20), in the space where PvP will exist. It works the ore of
its site into its own store, and it is a dock out there: fleets refuel from that
store, bank their charts and stand garrison. What it gathers is the player's only
once a convoy has carried it home. One per player. The homeworld never moves; this
is how an empire reaches richer ground.

**The one sentence.** An outpost is a claim on a rich site that pays a small, steady
income you have to carry home through dangerous space.

**What it costs.** It is built from what is hauled to it. Costs never scale with
pace; holds do, so the trip is longer in slow worlds.

| Level | Cost (metal / crystal / deuterium) | Build time, pace 1 / season / blitz | Hauler holds to carry it, pace 1 / season / blitz |
|---|---|---|---|
| 1 (founding) | 1,000 / 500 / 200 | 15 h / 1.5 h / 1.5 min | 8.5 / 2.7 / 0.3 |
| 2 | 2,500 / 1,250 / 500 | 37.5 h / 3.75 h / 3.75 min | 21 / 6.7 / 0.9 |
| 3 | 6,250 / 3,125 / 1,250 | 93.75 h / 9.4 h / 9.4 min | 53 / 17 / 2.2 |

About a Frigate to found and three Cruisers for all three levels, plus the Haulers
and the escort that carry it. A check-in empire on day 3 of a season makes the
founding cost in 13 to 17 minutes and all three levels in 2.1 to 2.7 hours
(section 3).

**What it does.**

| Level | Extractor | Store (pace 1; x sqrt(P)) | Platform (defence power) |
|---|---|---|---|
| 1 | 1 x site rate | 3,000 / 1,500 / 750 | 40 |
| 2 | 2 x site rate | 7,500 / 3,750 / 1,875 | 100 |
| 3 | 3 x site rate | 18,750 / 9,375 / 4,688 | 250 |

The site rate is 2 units an hour (pace 1) for each grade point of the sector's ore,
times the ring factor the ore tiles already use (1.77 at ring 9, 5.05 at ring 20).
A good site at ring 12 makes 24 units an hour at level 1 and 71 at level 3; the
best an Outer Ring is likely to hold makes 250 to 300. Against home output that is
2 to 4 percent on day 3 of a season at level 1, and 4 to 16 percent on day 7 at
level 3.

**What can happen to it.** Pirates of its ring raid it on the homeworld's raid
clock, announced the same way. The platform and the ships docked there defend. A
lost raid takes a quarter of the store above the repair materials, destroys a
quarter of the ships docked there, and breaches the outpost: no extractor, no fuel,
no platform until it has repaired itself from its store. No NPC raid destroys it,
so nothing irreversible happens in an absence. An outpost left undefended in a
deep ring gives back about half of what it makes; a defended one keeps it. The
owner may abandon it and found another.

**What it is not.** Not a second homeworld. It has no mines beyond the extractor,
no building queue, no shipyard, no research, no defence structures. Ships are built
at home. Its stock scores nothing. Hull is not repaired there (nor anywhere in the
game today).

**The map.**

```
ring  0     1-2    3-5     6-8        9 .................. 20      21+
      HUB          homes   scout      Outer Ring                   The Wandering
      exchange     24      packs      outposts (slice 1)           one move, late
                   seats              corvettes, then frigates     (slice 3)
      |<------------ no PvP ------->|<-------- PvP, when it exists ---------->|
                             ring 8 | ring 9
```

**The loop.**

```
 HOMEWORLD                                              OUTPOST
 stockpile --load--> convoy (Haulers + escort) --lanes, lairs--> unload --> store <-- extractor
     ^                                                             |  |
     |                                                 refuel fleets  +-- upgrade, repair
     +-- dock unloads <----- lanes, lairs <----- convoy <--load----+
                    (slice 2: part of it by gateway, up to its bandwidth)
```

**Slices.**

| Slice | What | Rough size |
|---|---|---|
| 1 | The outpost, flown and supplied by hand | 5 to 6 agent-days (section 7.9) |
| 1b | Standing orders: a `supply_run` doctrine, doctrines based at the outpost | 2 to 3 |
| 2 | The gateway: a research line, a charge, a bandwidth | 3 |
| 3 | The one move to the Wandering | 2 |

**Six questions for the owner** are in section 8. The two that change slice 1 most:
whether an outpost produces at all (recommended: yes), and whether it is carried in
holds or by a ship built for it (recommended: holds).

---

## 2. What the owner has decided

Not reopened here. From `docs/design/decisions.md`, 2026-10-10 ("the hub, PvP by
zone, outposts, more worlds"), and the owner's own words.

- "**Homeworlds never move.** A player reaches better resources with an
  **outpost**, which can only be placed in PvP space: anywhere from ring 9, with
  the richest sites further out."
- Owner: "not move homeworld but an outpost we can only place in the wandering/pvp
  zone."
- Owner: "ring 9+ for outposts, richest sites further out (so outposts can be
  anywhere after ring 9, and maybe we allow something for endgame where they can
  move their outpost once to ring 21+? maybe its a research tree, maybe a gateway
  in research tree allows fleets to jump to your outpost and back every so often?)"
- "**Gateway.** A research line lets fleets jump between home and the outpost. No
  jump while in combat. It carries loot, limited by a bandwidth that further
  research raises, so bulk still travels the rings." Owner: "no jumping in combat,
  gateway carries loot but maybe capped by a research tree so the gateway
  'bandwidth' upgrades over time adding to the logistics side of the game."
- "**PvP by zone.** No PvP in the Inner Ring (rings 1-8). PvP exists only from
  ring 9 outward. Multiplayer PvE is allowed everywhere." PvP is not built and is
  not part of this feature.
- "**A neutral post at the centre.**" "**The exchange is a surplus sink, never a
  faucet.**" Both are built. A hold is loaded from the stockpile at the home dock
  (`load`, 2026-10-11).
- "Existing databases are disposable." No compatibility is owed.
- "**Not decided:** whether research extends how far out an outpost may sit (ring
  21+)". Section 5.6 and question 4 propose an answer.
- Later features, not designed here and not blocked: other worlds through the hub,
  the moving black market, one-way gates. Section 5.2 keeps Anomaly sectors free
  for the market's berths, outposts do not cross between worlds, and the
  home-to-outpost link is called the gateway throughout so "gate" stays free for
  worlds.

---

## 3. The game as it is: five facts that shaped the design

1. **The tank stops being the limit early (measured).** Fuel Efficiency, Extended
   Tanks and the Fuel Depot multiply one-tank reach by 4.8 to 9.6 at the levels
   check-in empires hold on day 3 of a season. Round trips on one tank, in hops:

   | | Scout | Corvette | Frigate | Cruiser | Hauler |
   |---|---|---|---|---|---|
   | No research | 10 | 5 | 3 | 3 | 6 |
   | Check-in turtle, season day 3 (Efficiency 5, Tanks 4, Depot 2) | 48 | 24 | 16 | 14 | 30 |
   | Best-equipped check-in field empire, season day 3 (Efficiency 5, Tanks 5, Depot 7) | 96 | 48 | 32 | 29 | 60 |

   A home at ring 4 is 5 to 16 hops from the Outer Ring. So by the time an empire
   can hold a sector at ring 9, a forward dock no longer buys reach. It buys reach
   only in the first days of a season and the first weeks of a persistent world.
2. **Travel is seconds; fights are what distance costs.** A jump is 1 or 2 ticks
   of cooldown (`MOVE_BASE_COOLDOWN`, real time). A sector with a hostile lair
   starts a fight on entry (`check_npc_encounter`), lairs return `2 + 0.25 x ring`
   game hours after a kill (25 to 42 minutes at season pace in rings 9 to 20), and
   28 to 75 percent of Outer Ring sectors hold one. Hull is never repaired. The way
   out and back is a gauntlet, not a wait.
3. **Ore tiles are nuggets.** A Pristine tile at ring 20 holds 212 raw units and
   refills in 25 days at pace 1 (`ore_reserve_units`, `ore_regen_hours`). "Ore is a
   bonus on top of the mines, not a living" (RULES). Nothing that only harvests the
   tiles can make an outpost pay.
4. **The mid and late game is a ladder of buildings with poor returns.** Metal
   Mine 19, which the economy model's player buys on day 6 of a season, costs
   111,000 resources for 161 more an hour (pace 1 scale), a payback of about
   690 game hours. The tech tree is finished on day 3 (`economy/spec.md`,
   section 16). There is room for a goal that pays better and asks for ships.
5. **`recall` already jumps a fleet home with its whole hold** from anywhere, for
   twice the fuel and a hull risk that Emergency Jump research cuts to almost
   nothing at these distances. Slice 2 has to deal with it (section 5.5,
   question 5).

**What a check-in player owns** (season pace, 4 look-ins a day).

| | Day 3 (measured, section 9) | Day 7 |
|---|---|---|
| Core score | 244 to 295 | 1,543 to 1,803 (journal, 2026-10-10, fuel budget entry) |
| Mines, metal / crystal / deuterium | 12-13 / 8-13 / 8-9 | about 19 / 16 / 13 (economy model's competent player, whose day-7 core of 1,845 is within 17 percent of the bots') |
| Home output per real hour | 6,100 to 7,800 units | about 19,000 units |
| Unspent stockpile, score weight | 55,000 to 100,000 | not sampled |
| Ships | 7 to 33 in the field styles | not sampled |
| Ship techs | Hauler: all five styles. Frigate: three of five. Cruiser: none | all |

Against that: the founding cost (1,700 units) is 13 to 17 minutes of day-3 output,
the three Haulers that carry it at season pace are 2,850, a garrison of four
Corvettes is 2,240, all three levels (16,575) are 2.1 to 2.7 hours of day-3 output
and 52 minutes of day-7 output. In a persistent world the economy model's player on
day 15 makes 525 units an hour: founding is 3.2 hours of output, and the nine
Haulers that carry it (8,550) are 16 hours. Cost is not the gate at any pace. The
gate is taking a sector at ring 9 or beyond, carrying the load there and holding it.

---

## 4. What the two inspirations do

Sources: the two studies in this directory (`gap-analysis-2026-10-09.md`,
`starting-positions-2026-10-10.md`), with their tags, and one web search on
2026-10-11 that confirmed the OGame points from fan wikis and Wikipedia and found
nothing further on The Infinite Black's garrisons.

**OGame: colonies.** A Colony Ship is flown to an empty slot and consumed. The
number of colonies is capped by a research (Astrophysics), and the slot decides the
planet (fields, temperature, resource bonus). A colony is a full planet with its
own mines, queues and fleet. *Take:* the site decides what the place is worth, and
a research line gates expansion (here it gates the gateway and the move, not the
first outpost). *Leave:* full second planets. Nine planets with nine queue sets is
OGame's mid-game spreadsheet, and the owner has said the homeworld is the base.

**OGame: moons and the jump gate.** A moon is made by luck from a large debris
field. Two moons with jump gates move a whole fleet instantly, with no fuel, and
then cool down for an hour; resources cannot pass. *Take:* an instant link that is
rationed. *Leave:* rationing by a cooldown, which only rewards being online, and
the ban on cargo, which the owner has decided against: here cargo passes and is
charged against the same ration as the ships.

**The Infinite Black: garrisons and starports.** A starport is a fixed, safe place
to buy, sell and repair; alliances build garrisons in PvP space "to repair and
store goods and to keep outsiders out" (gap analysis, section 1). *Take:* a place
in dangerous space that stores and services for its owner and is defended by who is
parked there; the starport is already the hub. *Leave:* territory that shuts
others out (multiplayer PvE is allowed everywhere), alliance ownership (there are
no alliances), and repair (hull is not repaired anywhere in IAC yet; if it ever is,
it should be at both docks, as its own decision).

---

## 5. The design

### 5.1 What an outpost is for

**Recommendation: a claim on a rich site that pays a small income which must be
carried home. The dock is what makes that workable, not the point.**

The four candidates were a forward dock, a second stockpile, a claim on a rich
site, and a base for standing orders.

- **Forward dock alone: not enough.** Fact 1: at season pace a day-3 empire's
  Haulers fly 30 to 60 hops on a tank. A dock that only refuels and unloads would
  cost resources and a raid target for almost nothing, a trap for everyone but a
  first-week persistent player.
- **Second stockpile: no.** The Vault stores a unit for 0.07 to 0.6 resources
  (`decisions.md`, 2026-10-11). The outpost's store is deliberately dearer and
  exposed. It exists so there is something to carry.
- **Claim on a rich site: yes.** It gives "richest sites further out" a number,
  gives a builder a reason to own a working fleet, gives days 4 to 7 of a season a
  goal that pays better than the next mine level, and gives the gateway's bandwidth
  something to ration. A site is finite and first come, first served: an Outer Ring
  has about 140 sectors of grade 5 or better and about 33 of grade 7 or better for
  24 empires (section 5.2).
- **Base for standing orders: yes, as slice 1b.** It is the bridge for check-in
  players, and it is separable.

**What it deliberately cannot do.**

- No building queue, no shipyard, no research, no defence structures, no Vault. It
  has one level, and the level sets everything.
- New ships appear at home. Fleets are split and merged at an outpost as anywhere.
- It does not repair hull.
- Its store never scores and is never spent from home: a payment at home needs the
  goods at home.
- It makes no deuterium unless its site has deuterium ore. Fuel there is what was
  hauled in, unloaded from loot (15 percent of all loot is deuterium), or extracted.
- It does not move with the homeworld's protections: it stands in PvP space.

### 5.2 Placing one

**Which sectors qualify.** All of these, checked in one place and reported as one
sentence (as `exchange_deal` does for trades):

| Rule | Reason |
|---|---|
| Ring 9 to 20 (`dist_from_origin`) | Decided: PvP space. The Wandering is the late move (5.6) |
| The founding fleet stands idle in the sector | So no fight is going on. On a lair's sector that means the fleet has won it: you take the site from its pack |
| Terrain is not Anomaly | Left free for the black market's berths |
| No derelict in the sector, stripped or not | A boarding can wake a guard; no fights in a dock |
| At least 3 hexes from every other outpost, any empire's | No two outposts adjacent or sharing a neighbour; nobody is boxed in |
| The player has no outpost | One each |
| The hold carries the level-1 cost | See below |

Empty space qualifies and makes nothing: a pure dock, if someone wants one.

**While an outpost stands, its sector generates no group**, hostile or passive, and
none wanders in, as at the hub. The lair it displaced returns after its normal
delay if the outpost is abandoned.

**What makes a site rich.** The ore the sector was generated with. Worldgen needs no
change and no new marker.

- **Grade** = the sum, over metal, crystal and deuterium, of the density's harvest
  multiplier: Sparse 0.5, Moderate 1, Rich 2, Pristine 4 (`Density::harvest_multiplier`).
  0 to 12.
- **Ring factor** = `ring_mult(ring)`, the factor ore tiles use: 1.10 per ring from
  ring 3, so 1.77 at ring 9, 2.36 at 12, 3.45 at 16, 5.05 at 20.
- The extractor reads the generated density as a grade. It does not draw down the
  tiles, which fleets may still harvest (fact 3: tiles are too small to feed it).

Sites by ring (from the roll tables in `world.rs` and `scaling.rs`; section 9):

| Ring | Sectors | Share with a hostile lair | Grade >= 3 | Grade >= 5 | Grade >= 7 | Sectors of grade >= 5 / >= 7 |
|---|---|---|---|---|---|---|
| 9 | 54 | 28% | 25% | 10% | 1.9% | 5 / 1 |
| 12 | 72 | 45% | 28% | 11% | 2.4% | 8 / 2 |
| 16 | 96 | 65% | 31% | 14% | 3.3% | 13 / 3 |
| 20 | 120 | 75% | 34% | 16% | 4.2% | 19 / 5 |

The shares are of all the ring's sectors. Grade 3 is, for example, Rich metal and
Moderate crystal. Grade 5 is Pristine and Moderate. Grade 7 is Pristine, Rich and
Moderate. Grade 9 or better (two Pristine and more) is one sector in 350 at ring 9
and one in 150 at ring 20: about five in a whole Outer Ring.

**How a player finds one.** With what exists: scans already report `RichOre`
signals one hop beyond the reveal, the chart remembers every sector's densities,
and the Outer Ring has to be scouted to be crossed. One new query, `outpost_survey`,
answers for any sector on the player's chart what an outpost there would make, what
would raid it and whether it can be founded now, with the reason if not. The server
works it out; no client holds a formula.

**What it costs and who carries it.** The founding fleet carries the level-1 cost
in its hold, loaded at home with `load`: 1,000 metal, 500 crystal, 200 deuterium.
`found_outpost` takes it from the hold. No ship class is added.

- *Why holds and not an Outpost Module ship.* `load` was built this week and this
  is its second use. Supplying, upgrading and repairing the outpost are then the
  same act as founding it. A sixth ship class touches every per-class table in
  Rust and Dart (stats, costs, rapid fire, odds, catalogs, counts, the recovery
  pools) for a ship that exists to be consumed once.
- *What it means by pace.* Holds scale with sqrt(P) and costs do not, so founding
  needs 8.5 Hauler holds at pace 1, 2.7 at season, a third of one at blitz. A
  persistent player builds a convoy for it; the convoy is what brings the yield
  home afterwards. This is question 2.
- *A worked founding at season pace:* four Corvettes and three Haulers (cost
  5,090), power 133 against the ring-12 pack's 36.

**How long.** Level 1 takes 15 hours at pace 1 (`OUTPOST_BUILD_RATE` 100 of metal
plus crystal an hour, a seventh of the home rate; Economy class): 1.5 hours at
season, 1.5 minutes at blitz. The fleet need not stay. During the build the site is
claimed, visible, quiet and has a store; it has no extractor, fuel or platform, and
is not raided.

**How many.** One per player. The owner's words imply it ("their outpost", "move
their outpost once") and nothing here argues otherwise: one site makes the choice
of site matter, keeps the screen to one more panel, and leaves a second outpost as
a later lever if the game wants one. `abandon_outpost` gives it up; the store drifts
as wreckage in the sector for the usual 180 ticks and the player may found again.

### 5.3 What it holds and does

**One level, 1 to 3.** Each level multiplies cost, store and platform by 2.5 and
adds one site rate to the extractor (tables in section 1). An upgrade is ordered
with `upgrade_outpost` and paid from the outpost's own store at that moment, or
refused with the exact shortfall: the ship-batch rule, not the queue-ahead rule,
so the store is never spent behind the player's back. The outpost keeps working at
its old level while it builds. One job at a time: build or repair.

**Extractor.** Per resource, per game hour at pace 1:
`2.0 x harvest_multiplier(density) x ring_mult(ring) x level`, times P (Economy
class, like the mines).

| Site | Level 1 | Level 3 | Level 3 as a share of a day-7 home's 1,900 an hour |
|---|---|---|---|
| Ring 9, grade 3 | 11 | 32 | 1.7% |
| Ring 12, grade 3 | 14 | 42 | 2.2% |
| Ring 12, grade 5 | 24 | 71 | 3.7% |
| Ring 16, grade 5 | 35 | 104 | 5.4% |
| Ring 20, grade 7 | 71 | 212 | 11% |
| Ring 20, grade 10 | 101 | 303 | 16% |

Level 1 on a grade-3 site at ring 12 pays its cost back in 120 game hours; the
Metal Mine level a day-3 empire buys next pays back in 135, and Metal Mine 19 in
about 690. Each further level adds one site rate for 2.5 times the cost, so the
upper levels pay on good sites only: level 3 pays back in 150 game hours on a
grade-7 site at ring 20, in 450 on a grade-5 site at ring 12, and in 1,000 on a
grade-3 site at ring 9, where it is bought for its platform and not its yield. An
outpost left alone on the ring-12, grade-3 site saves up the metal and crystal of
its own level 2 in 11 days at pace 1 (27 hours at season); the deuterium has to be
brought, because that site makes none.

**Store.** Separate from the stockpile: 3,000 / 1,500 / 750 at level 1, times 2.5
a level, times sqrt(P) like the holds that fill and empty it. That is 26, 66 and
164 Hauler holds at every pace, and at level 3 it is between a Vault 2 and a
Vault 3. It always holds the next level's cost. The extractor stops at the cap. It
holds days of yield, so a look-in missed costs nothing; what limits what the
player keeps is the raid, not the cap.

**Unloading and loading.** At home the dock empties every hold by itself. At the
outpost nothing moves by itself: `unload` puts the hold into the store as far as it
has room, `load` takes from it. A convoy that stops for fuel on its way home keeps
its cargo, and a fleet under standing orders that passes through loses nothing.

**Fuel.** A fleet without standing orders that stands at its outpost is refuelled
from the outpost's deuterium at the home price (0.08 a fuel unit), as far as the
store pays. A dry store fills nothing and says so once.

**Charts.** Any of the player's fleets arriving at their outpost banks its charts,
as at home. The economy spec already says charts pay "delivered to a dock".

**Fleets there.** A fleet standing at its owner's finished outpost is docked: it
does not count against the three deployed fleets, it defends in a raid, and a lost
raid destroys a quarter of the ships there, as at home. No fight starts in the
sector in PvE, because no group lives or wanders there. That is exactly the safety
of the home dock, in a zone that will not stay PvE (5.4).

**Eyes.** The outpost's sector is live on its owner's chart. Anyone else with eyes
on the sector sees that an outpost stands there, whose, its level and whether it is
breached.

**Routes and warnings.** `home_fuel`, `range_hops`, `preview_move` and `recall`
stay about the homeworld; their names say so. Each fleet also reports
`outpost_fuel`, and a jump raises no "cannot reach home" alert when the fleet can
still reach its working outpost.

**Standing orders (slice 1b; in slice 1 they ignore the outpost).**

- *A base, named, not "the nearest dock".* `policy_update` gains `base`: `home`
  or `outpost`; when it is left out, the dock the fleet stands at, otherwise home.
  A doctrine measures `max_range` from its base, comes back to it on fuel or a full
  hold, banks charts there, and at the outpost the engine does for it what `unload`
  does by hand. "Nearest" was rejected: a mining wing based at home whose route
  bends toward the outpost would unload there, fill a store nobody is emptying, and
  wait.
- *The fuel budget keeps its meaning.* The allowance is still earned at
  `fuel_budget_pct` of the home's deuterium production and still saves up one tank.
  It bounds what the fleet may take from whichever store fills it. "Free while the
  store is full" reads the store of the dock that fills it.
- *When the base fails.* A breached outpost or a dry store: the fleet holds there,
  says why, and the owner gets one Warning. An abandoned outpost: the base becomes
  home.
- *`supply_run`, a new doctrine based at home.* It waits at the dock until the
  outpost's store holds a load worth the fuel (the payback rule every doctrine
  uses), flies there by lanes its engage ratio allows, loads, comes home, unloads.
  A parameter sends deuterium out when the outpost's tank store is low. This is the
  one that makes an outpost a check-in feature and not a chore.

### 5.4 Risk

**PvE now: raids from the ring.**

- *Size.* `2.0 x npc_group_power(ring)` times the home raid's roll of 0.80 to 1.15.
  It follows where the outpost stands, not what its owner is worth: depth is the
  dial, and the survey shows the number before the player commits.

  | Ring | 9 | 10 | 12 | 14 | 16 | 18 | 20 |
  |---|---|---|---|---|---|---|---|
  | Raid power, range | 31-45 | 38-55 | 57-82 | 85-122 | 126-182 | 188-270 | 280-402 |

- *Clock.* The homeworld's own constants and pace class: a roll every 6 hours at
  30 percent with the pity step, at most one per 18 hours, none in the first
  24 hours after level 1 is finished, none for 48 hours after a lost one, all
  divided by P^0.75; the warning is `600 / sqrt(P)` ticks. About one raid per
  38 hours at pace 1 and per 6.8 hours at season, on top of the homeworld's.
- *Defence.* The platform (0 while breached) plus the live power of the owner's
  ships standing there, with the home raid's 10 percent fog. A Corvette is 22, a
  Frigate 48, a Cruiser 110.
  - Level 1 (40) is never safe alone: at ring 9 it loses about two raids in five.
    One Corvette makes it safe there; three at ring 12.
  - Level 2 (100) holds rings 9 to 12 alone.
  - Level 3 (250) holds through ring 17 alone; ring 18 wants a Frigate on top and
    ring 20 two Cruisers.
- *Repelled.* Docked ships' shields are halved, as at home. Nothing else: no
  salvage (at slow paces it would rival the extractor) and no wear.
- *Lost.*
  - A quarter of the ships docked there are destroyed, at least one; what they
    carried goes to the store as far as it has room.
  - A quarter of each resource in the store and in the holds docked there, above
    the protected floor, is taken. The floor is the repair cost, so a raid never
    takes the materials the repair needs.
  - The outpost is breached: extractor stopped, no refuelling, platform 0. The
    store, `load`, `unload` and chart banking still work.
  - It repairs itself: as soon as the store covers a quarter of its level's cost
    it pays that and is whole after a quarter of the level's build time (3.75 h,
    9.4 h, 23.4 h at pace 1). If the owner had emptied the store below the floor,
    the repair waits for a delivery.
  - No raid for 48 hours (pace 1; 8.5 hours at season).
- *Destroyed?* Never by an NPC raid. Question 3.
- *What an undefended outpost comes to.* A level-3 outpost beyond ring 17 with
  nobody home loses every raid. At season pace it is then down 2.3 hours in every
  11 or so, pays 2,655 a repair and loses a quarter of what sat in the store:
  about 40 percent of its yield on a grade-7 site at ring 20, about 60 on a
  grade-3 site at ring 18, and more in a persistent world, where repairs are long
  against the raid clock. Defence is not optional; it is cheap near ring 9 and
  dear near ring 20, which is the choice the owner asked for.
- *What the owner hears while away.* `RaidIncoming` and `RaidResolved` naming the
  outpost, with what was taken, the ships lost and `breached`; an Alert when the
  repair starts, when it ends, when it cannot start for lack of materials, when the
  store fills, when a tank could not be filled; `OutpostCompleted` for each level.
  The web client's background notices and the single-player absence report carry
  them.
- *Sleep-safe.* The economy spec's open question 6 asks whether offline players
  should be spared raids. This design does not answer it for homes. For outposts
  the worst an absence can do is bounded: per lost raid a quarter of the exposed
  store and a quarter of the ships left there, with at least 48 pace-1 hours
  between two, and the station still standing at the end. The ships are the only
  loss that cannot be earned back by waiting, and leaving them there is the
  owner's choice.

**PvP later (one paragraph).** An outpost is the first thing an empire owns that
another empire will be able to reach, because homes sit behind the ring-8 line.
The design leaves room in four ways. It is public: owner, level and state are on
any chart that has seen the sector. Everything a lost raid does goes through one
function, so a player's attack can later call it with the taker's holds as the
limit on what leaves. The defence is one number on the scale ships, raids and
threat ratings share. The grace after a loss belongs to the outpost, so it shields
it from everyone, not from one attacker. Not decided here and not built: whether a
player can destroy or capture an outpost, how an attack is announced, protection
by score ratio, and whether fleets docked at an outpost can be fought. The client
must not call a fleet at an outpost safe; the home dock stopped saying so this
week.

### 5.5 The gateway (slice 2)

**The research line.** `Gateway`, five levels, behind Emergency Jump 2 and
Research Lab 6. It needs a finished, unbreached outpost to do anything.

| Level | Cost (m / c / d) | Research time, pace 1 / season | Bandwidth per window (mass) | Banked at most |
|---|---|---|---|---|
| 1 | 3,000 / 3,000 / 2,000 | 7.5 h / 45 min | 80 | 320 |
| 2 | 4,800 / 4,800 / 3,200 | 11.3 h / 1.1 h | 128 | 512 |
| 3 | 7,680 / 7,680 / 5,120 | 17.1 h / 1.7 h | 205 | 819 |
| 4 | 12,288 / 12,288 / 8,192 | 1.1 d / 2.6 h | 328 | 1,311 |
| 5 | 19,661 / 19,661 / 13,107 | 1.6 d / 3.9 h | 524 | 2,097 |

The usual 1.6 growth; times assume Lab 6 to 10. The whole line is 126,000 units
(182 score points): 6.7 hours of a day-7 season empire's output, a research goal
for days 3 to 6, when the tree is otherwise finished. Level 1 is 1.0 to 1.3 hours
of day-3 output.

**The rule.**

- The gateway holds a **charge**, in mass. It gains one bandwidth per window and
  banks up to four. The window is 21,600 ticks at pace 1 (Finds class): 6 hours,
  1.9 hours at season, 14.7 minutes at blitz. No cooldown: a cooldown rewards being
  online, a bank does not, and four windows is a day of a persistent world.
- `gateway_jump` moves a fleet standing idle at the homeworld to the outpost, or
  back. It spends the fleet's **mass**: each ship's base hull (Scout 30, Corvette
  50, Hauler 80, Frigate 120, Cruiser 200) plus its cargo divided by sqrt(P), so a
  full hold weighs the same at every pace (a full Hauler is 280). All or nothing:
  a fleet heavier than the charge on hand is refused and can be split.
- It burns the fuel the shortest lane route would, from the fleet's tank.
- **What cannot pass:** a fleet in combat; a fleet that is moving, harvesting or
  boarding; anything while the outpost is under construction or breached; another
  empire's ships; the store itself, except as cargo in a hold.

**What a full bank moves** (a pace-1 day of charge):

| Level | For example | Hauler round trips (360 mass each) |
|---|---|---|
| 1 | one full Hauler, or one Cruiser and a Frigate | 0.9 |
| 3 | two full Haulers and a Cruiser | 2.3 |
| 5 | seven full Haulers | 5.8 |

At level 5 that is 1,165 units a day at pace 1 against what a level-3 outpost
makes: 69 percent of a grade-5 site at ring 12, 47 percent at ring 16, 23 percent
of a grade-7 site at ring 20, 16 percent of the best. The share is the same at
every pace. Bulk still travels the rings, and the richer the site the more of it
does.

**How it changes what an outpost is worth.** A check-in player's banked charge
brings one unescorted load home per look-in. A hands-on player can put a Cruiser
into the garrison inside a raid's warning. A deep site, whose convoy route is the
longest gauntlet, gains the most, so the raid factor must keep deep sites dear.

**What it must not break.**

- *The route-home fuel economy.* The jump burns the route's fuel, and no route
  figure or warning changes.
- *`recall`.* Today it is a gateway home with no bandwidth, no bank and no
  research: a convoy can load at the outpost, step one sector out and recall with
  everything. From slice 2 the two should be one drive under one ration: a recall
  spends charge for the cargo it carries and leaves the rest as wreckage where it
  jumped from; with no Gateway research it carries none. The ships still jump on
  fuel and hull risk as today. This changes a rule pilots use now, so it is
  question 5.
- *The raid rules.* Defence is counted at arrival, as at home, so a fleet that
  jumps in during the warning defends and one that jumps out does not. Both are
  bounded by the charge. Nothing jumps once the raid is being resolved.
- *The 25 percent cap on field score.* The gateway carries no score. Charts bank at
  either dock already. It does give a pilot more field time per hour, and pilots
  fill the cap in any case (journal: 2.0 to 3.8 times over).
- *The fleet cap.* A jump is dock to dock and uses no deployment slot.
- *Later PvP.* "No jump while in combat" is the first refusal above, and the
  bandwidth is what stops a garrison or a store from being evacuated at once.

### 5.6 Moving it (slice 3)

The owner's endgame idea, as one late act. A one-level research, `Deep Anchor`
(proposed 20,000 / 20,000 / 10,000, behind Gateway 3), allows `move_outpost` once
in an outpost's life: a founding fleet stands on a qualifying sector at ring 21 to
30 carrying half the cost of the levels the outpost has, the old site closes, the
new one opens after the level-1 build time with the same level and an empty store
(what was not carried out of the old store drifts there as wreckage). For the move
to be worth making, the Wandering has to be richer than ring 20, and today it is
not: `ring_mult` stops growing at ring 20. The slice therefore lets the outpost's
ring factor go on at 1.10 a ring to ring 30 (13.1, 2.6 times ring 20) and adds
levels 4 and 5 for outposts there (platforms 625 and 1,563 against raids of about
950 at ring 25 and 2,500 at ring 30, which is a Cruiser garrison). The study's
alternative, research that raises the farthest ring an outpost may be founded at,
with abandon and re-found as the move, is simpler to build and loses the "once";
question 4 touches it.

### 5.7 Score and balance

**Score.**

- Each finished level counts in `core` at the weight of its cost, like a building:
  2.15, 5.38 and 13.44 points, 21 in all, while the outpost stands. That is 1.2
  percent of a day-7 check-in core. It is kept apart from building and research
  points so that it does not raise the homeworld's raids (`raid_power_base` reads
  only those).
- The store scores nothing. What the yield buys at home scores when it is built,
  like any resource.
- The Gateway line scores as research does.

**Neither the only way to win nor a trap.**

- *Not the only way.* A realistic outpost adds 4 to 6 percent to late income and
  an exceptional, deep, well-defended one 11 to 16. Score is priced at cost, so
  income turns into core almost one for one: that is 4 to 6 percent, or 11 to 16,
  on what the empire builds after the outpost stands, less the ships it ties up.
  The journal's check-in cores already differ by 17 percent between play styles
  on day 7, and hands-on cores by more. An outpost moves a player within that
  spread, not out of it. In levels it is small, because costs grow 1.5 to 1.9
  times a level: 10 percent more income is about a quarter of a level across the
  board.
- *Not a trap.* Level 1 pays back about as fast as the mine level bought the same
  day, and level 3 on a site of grade 5 or better from ring 16 out two to four
  times faster than Metal Mine 19 (5.3). The survey states yield and raid before anything is spent. Nothing
  built is lost to NPCs.

**The standing targets.**

- *Check-in trails hands-on by tens of percent.* The extractor does not care who
  is watching, and the store holds days of yield, so one convoy per look-in brings
  home all of it. Expected effect: the gap closes by a point or two, not more.
  Hands-on keeps an edge only in convoys that go wrong less often.
- *Standing orders earn 50 to 70 percent of piloting.* Slice 1 does not touch
  them. Slice 1b should raise delegated haul, which is the part still short
  (13 to 37 percent in the journal), by shortening sorties from an outer base; it
  must be measured, not assumed.
- *Explorers and raiders within 15 percent of a builder, mixed above both.* An
  outpost asks a builder for an escort, Haulers and a garrison. It pushes the
  turtle toward the mixed empire the owner wants on top.
- *The 25 percent cap.* Core rises by about 1 percent, so the cap rises with it.
  Banking charts at ring 12 instead of ring 4 loses fewer charts with their
  carriers. Pilots are over the cap all week; a check-in explorer fills it sooner.

**Bots.** In slice 1 they do nothing with outposts: they neither found one nor
change a route, and a world of bots must play exactly as it does today. Later they
need three things that are not small: a site choice from `outpost_survey` over
their own chart; an expedition, which is the pilot flying a loaded fleet to a named
sector by odds (the same missing piece that keeps bots from the hub's exchange);
and a garrison rule in the home planner, platform plus garrison at 1.15 times the
raid estimate, as it already keeps turrets. With 1b they would run `supply_run`.

### 5.8 Single player

Everything above is PvE and needs no second human: the site pays, the ring raids,
the convoy runs the lairs. In slice 1 the bots do not found, so the commander has
first pick of every site in the world; when bots learn to, good sites become
contested, which is better. An absence is played tick by tick by the same engine,
so the extractor fills the store, raids come and are resolved, and repairs start
and finish while the tab is closed; the absence report gains the outpost's lines
(levels finished, raids repelled and lost with what they took, breached or whole).
Nothing enters the world from outside but the commander's orders, so a replay of
unsaved ticks comes out identical. The outpost is in the save because it is in the
`Snapshot`.

---

## 6. What is deliberately left out of slice 1

- Standing orders at the outpost and `supply_run` (1b). In slice 1 a doctrine
  measures from home, returns home and passes an outpost untouched, except that
  its charts are banked if it happens to arrive there.
- The gateway and any change to `recall` (2). The move, the Wandering, levels 4
  and 5 (3).
- Bots founding or supplying outposts.
- Hull repair. Salvage from a repelled outpost raid. Sensor range around the
  outpost. A second outpost. Cancelling an upgrade. Trade between empires at an
  outpost. Anything PvP.

---

## 7. Slice 1: build specification

### 7.1 The rules, as they would be added to `tools/playtest/RULES.md`

New rows in the command reference:

| action | fields | effect |
|--------|--------|--------|
| `found_outpost` | `fleet_id` | found your outpost in the sector the fleet stands in, paying 1000 metal, 500 crystal and 200 deuterium from its hold. The fleet must stand idle with no standing orders; the sector must be in rings 9 to 20, not an Anomaly, without a derelict, and at least 3 hexes from every other outpost; you may have one outpost. The answer is an `Alert` (info) with the tick it will be finished. Refused with nothing changed and a sentence saying why: `InvalidTarget` for a sector that does not qualify, `InvalidCommand` if you already have an outpost or the fleet is under standing orders, `NoResources` naming what the hold lacks, `OnCooldown` for a fleet that is moving, fighting or busy |
| `outpost_survey` | `fleet_id`, `sector:{q,r}` (optional) | do not found: reply `outpost_survey` for the named sector, or the fleet's own, with its `ring`, `grade`, the `cost` and `build_ticks` of level 1, `production` a tick at level 1, `raid_power` (the middle of the raids that ring sends), `platform_power`, `hops_home`, and `refusal`: the sentence `found_outpost` would be refused with right now, absent when it would be accepted. Any sector on your chart may be asked about; `InvalidTarget` for one you have never seen |
| `upgrade_outpost` | none | build the next level of your outpost, paid in full from the outpost's own store at that moment. Refused with `InsufficientResources` and the exact shortfall when the store does not cover it, `MaxLevelReached` at level 3, `InvalidCommand` with no outpost, while it is building or repairing, or while it is breached |
| `abandon_outpost` | none | give up your outpost. Its store is left as wreckage in the sector for 180 ticks, its score is gone, nothing is refunded, and you may found another. `InvalidCommand` with no outpost |

Changed rows: `load` and `unload` end with "At your outpost `load` takes from the
outpost's store and `unload` puts the whole hold into it as far as it has room; see
Outposts."

New name list: `OutpostWork: build, repair`.

New paragraph in "The game", after "Loading a hold":

- **Outposts** — one station per empire, on a sector of rings 9 to 20, the space
  where PvP will be allowed. It is built from what you carry to it. `load` the
  founding cost at home, fly it out, clear the sector if a pack lives there, and
  `found_outpost`; `outpost_survey` tells you beforehand what the site is worth.
  - **The site** decides the yield. A sector's `grade` is the sum of its ore:
    Sparse 0.5, Moderate 1, Rich 2, Pristine 4, for metal, crystal and deuterium.
    The outpost's extractor makes, per resource and game hour,
    `2 x density x ring factor x level` times P; the ring factor is 1.10 per ring
    from ring 3 (1.77 at ring 9, 5.05 at ring 20). It reads the ore the sector was
    made with, not what harvesting has left this hour, and it does not empty the
    tiles.
  - **Levels**: 1 costs 1000/500/200 and takes 15 game hours divided by P; 2 costs
    2500/1250/500 (37.5 h); 3 costs 6250/3125/1250 (93.75 h). Level 1 is paid from
    the founding fleet's hold, levels 2 and 3 from the outpost's store when you
    order them. The outpost works at its old level while it builds.
  - **The store** is its own, apart from your stockpile: 3000/1500/750 at level 1,
    times 2.5 a level, times sqrt(P). The extractor stops at the cap. Nothing in it
    scores, and nothing in it can be spent at home until a fleet has carried it
    there: `load` at the outpost, fly home, and the home dock unloads as always.
    At the outpost no hold is emptied by itself; `unload` does it.
  - **A dock**: a fleet without standing orders that stands at your outpost is
    refuelled from the outpost's deuterium at 0.08 a fuel unit, as far as it has
    any. Any of your fleets arriving there banks its charts. A fleet standing there
    does not count against your three deployed fleets. New ships still appear at
    home. Hull is not repaired there.
  - **Quiet**: while an outpost stands, no group lives in its sector or wanders
    in, and the sector reads as empty to `preview_move` and `odds`.
  - **Raids**: pirates of its ring raid it on the homeworld's raid clock, with the
    same warning (`RaidIncoming` and `RaidResolved` carry `outpost`). A raid's
    power is `2 x` the ring's group power times 0.80 to 1.15: about 38 at ring 9,
    70 at ring 12, 154 at ring 16, 341 at ring 20. The platform (40, 100, 250 by
    level) and your ships standing there defend. Repelled: your ships' shields are
    halved, nothing else. Lost: a quarter of the ships there are destroyed (at
    least one); a quarter of the store, and of the cargo aboard fleets there, above
    the protected floor is taken; and the outpost is breached: no extractor, no
    refuelling, no platform. The floor is a quarter of the cost of the level it
    stands at, which is what the repair costs. A breached outpost repairs itself:
    when its store covers the repair it pays, and it is whole after a quarter of
    that level's build time. After a lost raid none comes for 48 game hours divided
    by P^0.75. No raid destroys an outpost.
  - **Score**: each finished level adds its cost at score weight to `core` (2.15,
    5.38, 13.44) while the outpost stands. It does not raise the raids on your
    homeworld.
  - **State**: `full_state.outpost` and every `tick_update.outpost` (absent when
    you have none): `location`, `level`, `in_service`, `breached`, `work`, `store`,
    `store_cap`, `protected`, `production`, `grade`, `platform_power`,
    `defence_power`, `next_raid_estimate_power`, `raid_arrival_tick`,
    `next_level` (what `upgrade_outpost` would cost and give), `repair_cost`,
    `hops_home`. Each fleet reports `outpost_fuel`, the fuel the way there costs. A
    sector view carries `outpost` (`owner`, `level`, `breached`, `mine`) when one
    stands there, yours or another empire's.
  - Standing orders do not use the outpost: a fleet under orders measures from
    your homeworld and returns there.
  - The trip: `load` at home what the outpost needs (deuterium for tanks, the next
    level's cost), fly out, `unload`, `load` what it has made, fly home.

Also changed: the "Fleets" paragraph ("up to 3 deployed" becomes "up to 3 away
from your homeworld and your outpost"), the "Fuel and stranding" paragraph (the
route alert), and the "Score" paragraph (`core` includes outpost levels). README's
"The game" section gets the same paragraph, shortened.

### 7.2 Protocol (`shared/src/protocol.rs`)

Commands, serialised like the others:

```json
{"action":"found_outpost","fleet_id":7}
{"action":"outpost_survey","fleet_id":7,"sector":{"q":10,"r":-2}}
{"action":"upgrade_outpost"}
{"action":"abandon_outpost"}
```

`outpost_survey` is a query (`commands::is_query`), so a single-player world does
not save on it.

New reply, `ServerMessage::OutpostSurvey`:

| field | type | meaning |
|---|---|---|
| `fleet_id` | u64 | the fleet asked about |
| `sector` | Hex | the sector surveyed |
| `ring` | u16 | |
| `densities` | `SectorResources` | the generated densities |
| `grade` | f32 | 0 to 12 |
| `cost` | `Resources` | level 1 |
| `build_ticks` | u64 | level 1, at this world's pace |
| `production` | `Resources` | per tick at level 1, at this world's pace |
| `raid_power` | f32 | `2 x npc_group_power(ring) x 0.975` |
| `platform_power` | f32 | level 1 |
| `hops_home` | `Option<u32>` | lane hops to the homeworld, when within 64 |
| `refusal` | `Option<String>` | why `found_outpost` would be refused now |

New state:

```
OutpostState {
  location: Hex, level: u8, max_level: u8,
  in_service: bool,            // level >= 1 and not breached
  breached: bool,
  work: Option<OutpostWorkState>,   // { kind: "build" | "repair", level, start_tick, end_tick }
  store: Resources, store_cap: Resources, protected: Resources,
  production: Resources,       // per tick now; zero when not in service
  grade: f32,
  platform_power: f32,         // 0 while breached
  defence_power: f32,          // platform + ships standing there
  next_raid_estimate_power: f32,
  raid_arrival_tick: Option<u64>,
  next_level: Option<OutpostLevel>, // { level, cost, ticks, store_cap, platform_power, production }
  repair_cost: Option<Resources>,   // breached and not yet repairing
  hops_home: Option<u32>,
}
OutpostBrief { owner: String, level: u8, breached: bool, mine: bool }
```

- `GameState.outpost: Option<OutpostState>`; `TickUpdate.outpost: Option<OutpostState>`,
  sent every tick the player has one, as `homeworld_update` is.
- `SectorState.outpost: Option<OutpostBrief>`, live or remembered as last seen.
- `FleetState.outpost_fuel: Option<f32>`: `jump_fuel` times the lane hops to the
  owner's outpost, present while that outpost is in service.

Events:

- New `EventKind::OutpostCompleted { player_id, sector, level }`: a level is
  finished (level 1: the outpost is open).
- `RaidIncomingEvent` and `RaidResolvedEvent` gain `outpost: Option<Hex>` (absent:
  the homeworld). `RaidResolvedEvent` gains `breached: bool`. For an outpost raid
  the structure and salvage fields are zero and absent.
- Everything else is an `Alert`: founded (info), upgrade paid (info), repair
  started and finished (info), repair waiting for materials (warning, once per
  breach), store full (warning, once per fill, per resource), a tank that could
  not be filled (warning, once per stay), abandoned (info), and the existing
  `load` and `unload` answers.

No new error code. Fixtures are regenerated (`UPDATE_FIXTURES=1 cargo test -p
iac-shared`) and the Dart mirror follows until `flutter test` passes.

### 7.3 State and persistence

```
pub struct Outpost {
    pub sector: Hex,
    pub level: u8,                    // 0 until level 1 is finished
    pub store: Resources,
    pub work: Option<OutpostWork>,    // { kind: Build { level } | Repair, start_tick, end_tick }
    pub breached: bool,
    pub raid: RaidState,              // the existing struct; first_seen_tick is set when level 1 finishes
    pub full_marks: [bool; 3],        // a "store full" alert already sent, per resource
    pub repair_warned: bool,
    pub founded_tick: u64,
}
```

- **Not on `Player`.** `GameEngine.outposts: BTreeMap<u64, Outpost>`, keyed by the
  owner's player id, beside `raid_states` and `bots`. One per player is the key's
  uniqueness. A lookup by sector walks at most 24 entries.
- **`Snapshot.outposts`**, the same map, with `#[serde(default)]`;
  `GameEngine::restore` and `GameEngine::snapshot` carry it.
- **`PersistBatch`**: no new field. A new `RuntimeKind::Outpost` (name `outpost`)
  and `RuntimeEntry::Outpost(Box<Outpost>)`, written whenever the entry changes
  (`dirty_runtime`), handled in `GameEngine::runtime_entry` and
  `Snapshot::insert_runtime`. This is how a `BotRecord` is stored.
- **SQLite**: no new table and no new column. The outpost is a `runtime_state` row
  of kind `outpost` with the player id. One caution: `Database::load_runtime`
  skips rows that no longer parse; for kind `outpost` a row that does not parse
  must fail the load with a sentence, because it holds resources.
- **Single-player save**: the `Snapshot` is the save. `SAVE_FORMAT` goes from 3
  to 4, so an older save is refused with the usual sentence.
- **Versions**: `ECONOMY_VERSION` goes from 3 to 4 (a production formula is
  added). `WORLDGEN_VERSION` stands: no sector is generated differently.

### 7.4 Constants and formulas

In `shared/src/constants.rs`, mirrored in `docs/design/economy/sim.py` and checked
by `tools/check_sim_constants.py`:

| Constant | Value | Pace class |
|---|---|---|
| `OUTPOST_MIN_RING`, `OUTPOST_MAX_RING` | 9, 20 | none |
| `OUTPOST_SPACING` | 3 hexes | none |
| `OUTPOST_MAX_LEVEL` | 3 | none |
| `OUTPOST_COST` | 1000 / 500 / 200 | none (costs never scale) |
| `OUTPOST_GROWTH` | 2.5 (cost, store and platform per level) | none |
| `OUTPOST_BUILD_RATE` | 100 metal plus crystal a game hour | Economy (time / P) |
| `OUTPOST_STORE_BASE` | 3000 / 1500 / 750 | Finds (x sqrt(P), `Pace::cargo_mult`) |
| `OUTPOST_YIELD_PER_HOUR` | 2.0 per grade point | Economy (x P, `Pace::rate`) |
| `OUTPOST_PLATFORM_POWER` | 40 | none |
| `OUTPOST_RAID_FACTOR` | 2.0 | none |
| `OUTPOST_RAID_LOSS` | 0.25, every resource | none |
| `OUTPOST_REPAIR_SHARE` | 0.25 of the level's cost and of its build time | Economy for the time |

Reused unchanged for outpost raids: `RAID_ROLL_INTERVAL`, `RAID_ROLL_CHANCE`,
`RAID_PITY_STEP`, `RAID_MIN_INTERVAL`, `RAID_MIN_PLAYER_AGE` (counted from the
tick level 1 finishes), `RAID_SUPPRESS_AFTER_LOSS`, `RAID_POWER_ROLL_MIN`,
`RAID_POWER_ROLL_MAX`, `RAID_LOST_SHIP_FRACTION`, `Pace::raid_warning_ticks`,
`FUEL_DEUT_PER_UNIT`, `SALVAGE_DESPAWN_TICKS`. No new duration constant is added,
so `pace::TIMER_CLASSES` needs no entry; if the builder adds one it must be
listed there.

In `shared/src/scaling.rs`:

| Function | Formula |
|---|---|
| `outpost_cost(level)` | `OUTPOST_COST x 2.5^(level - 1)`, rounded like `grown_cost` |
| `outpost_build_ticks(level, pace)` | `pace.econ_ticks((metal + crystal) / 100 x 3600)` |
| `outpost_store_cap(level, pace)` | `OUTPOST_STORE_BASE x 2.5^(max(level, 1) - 1) x pace.cargo_mult()` |
| `outpost_platform_power(level)` | `40 x 2.5^(level - 1)`, 0 at level 0 |
| `site_grade(densities)` | sum of `harvest_multiplier` over the three |
| `outpost_yield_per_tick(densities, ring, level, pace)` | per resource `pace.rate(2.0 / 3600 x harvest_multiplier x ring_mult(ring) x level)` |
| `outpost_raid_power(ring)` | `2.0 x npc_group_power(ring)` |
| `outpost_repair_cost(level)`, `outpost_repair_ticks(level, pace)` | a quarter of the level's cost and time |
| `outpost_points(level)` | sum of `resource_weight(outpost_cost(l))` for `l` in 1 to `level` |

### 7.5 Engine changes, by function

All in `sim/src/engine.rs` unless another file is named.

**New.**

- `outpost_site(&self, player_id, fleet: Option<&Fleet>, sector) -> Result<(), Refusal>`:
  the one place every founding rule of 5.2 is checked and worded. The survey, the
  command and the tests all read it.
- `handle_found_outpost`, `outpost_survey`, `handle_upgrade_outpost`,
  `handle_abandon_outpost`.
  - Founding takes the cost from the hold, inserts the outpost at level 0 with a
    `Build { level: 1 }` job, and alerts.
  - Abandoning removes the entry, drops the store with `drop_salvage`, and sets the
    sector's `npc_cleared_tick` to now so the displaced lair returns after its
    normal delay.
- `outpost_at(&self, sector) -> Option<(u64, &Outpost)>` and
  `docked_at_outpost(&self, fleet) -> bool` (the fleet stands at its owner's
  outpost and its level is at least 1).
- `process_outposts`, called from `tick` straight after `process_homeworlds`. For
  each outpost in owner-id order:
  1. Finish a job whose `end_tick` has come: a build raises the level and emits
     `OutpostCompleted` (and, for level 1, starts the raid clock); a repair clears
     `breached`.
  2. If in service, add the yield to the store with `Resources::add_capped`.
  3. If breached, with no job and a store that covers the repair, pay and start
     it; otherwise warn once.
  4. If in service, refuel the owner's fleets standing there that have no standing
     orders, in fleet-id order, as far as the deuterium goes.
  5. Raise the store-full alerts.
- `outpost_defense_power(&self, owner)`: platform, 0 while breached, plus the live
  power of the owner's ships in the sector, summed as `home_defense_power` sums.

**Changed.**

- `process_raids`, `resolve_raid`: after the homeworld pass, the same roll,
  announcement and resolution for each finished outpost, with its own `RaidState`,
  `outpost_raid_power` in place of `raid_power_base`, and an RNG seeded from the
  world seed, the tick and the owner's id with multipliers of its own, so the two
  rolls are independent and replay. The lost branch shares one pile-and-floor
  routine with the homeworld's (store or stockpile, plus holds docked there, one
  floor, one share taken in proportion), then sets `breached` and the grace.
- `destroy_docked_ships`, `settle_hold`: take the dock they act at. A hold shed at
  the outpost goes into its store as far as the cap allows and the rest is lost,
  the homeworld's rule.
- `process_movement`: after `check_homeworld_docking`, a fleet arriving at its
  owner's finished outpost delivers its charts (`deliver_charts`), has its
  `fuel_max` recomputed, and is refuelled if it has no standing orders.
- `fleet_at_dock`, `load_order`, `handle_load`, `unload_refusal`, `handle_unload`:
  accept the outpost (level 0 included, breached included) as a dock. At the
  outpost `load` takes from the store and does not set `laden`; `unload` moves the
  whole hold into the store up to the caps for any fleet and is never refused for
  "not loaded by hand".
- `count_deployed_fleets`, and its callers `handle_move` and `handle_split`: a
  fleet standing at its owner's finished outpost is not deployed, and leaving the
  outpost needs a free deployment slot as leaving home does.
- `pending_template_in`: no template while an outpost stands in the sector.
  `sector_opposing`, `threat_among`: such a sector has no opposition (threat T1,
  `est_power` 0, basis `observed`, ratio 10.0). `process_npc_behavior`: patrols do
  not step into it.
- `warn_if_cannot_return`: no alert when the fuel left reaches the owner's outpost
  in service.
- `sim/src/score.rs`: `Score` gains `outpost`, `core()` includes it, `score_of`
  takes the player's outpost. `scaling::econ_points` is untouched.
- `sim/src/views.rs`: `full_state`, `tick_update` (the `outpost` block),
  `collect_player_fleets` (`outpost_fuel`), `present_at` (the outpost's sector
  counts as presence), `event_for` (`OutpostCompleted` by `player_id`).
- `sim/src/intel.rs`: `live_coords` and `KnownSectors::update_coverage` watch the
  owner's outpost sector from the tick it is founded; `sector_state` fills
  `SectorState.outpost`.
- `sim/src/commands.rs`: `execute`, `command_fleet_id`, `is_query`,
  `command_error_message` (the refusal sentences come from `outpost_site` and from
  the upgrade's shortfall).
- `solo/src/away.rs`: `Tally::count` separates outpost raids from homeworld raids
  and counts levels finished; `solo/src/save.rs`: `SAVE_FORMAT`.
- `sim/src/headless.rs`, `sim/src/bin/sim-run.rs`: section 7.8.
- Untouched on purpose: `handle_recall`, `home_route`, `direct_hops_home`,
  `route_home_cost`, `add_ship_to_homeworld`, `unload_cargo`, `refuel_fleet`, every
  `policy_*` function, `sim/src/bot/`.

### 7.6 What each client must show

**Web (`clients/web`).**

- *Map:* a station glyph on a sector with `outpost`, filled for the player's own.
  The sector inspector names owner, level and "breached", and has a "Survey site"
  button that sends `outpost_survey` and prints the reply as three lines (grade and
  yield, raid and platform, cost and time, or the refusal).
- *Windshield:* when the active fleet stands in rings 9 to 20, a "Found outpost"
  tile that opens the survey for its sector with the cost set against the hold,
  and a button that sends `found_outpost`; disabled with the refusal sentence when
  there is one. At the player's own outpost the station is a contact in the scene,
  as the exchange is, and `O` or its tile opens the outpost panel. Nothing says
  docked fleets are safe.
- *Outpost panel* (also in the rail when an outpost exists): level and state with
  the countdown of a build or repair; the store per resource against cap and
  floor; yield an hour; defence (platform plus docked) against the raid estimate
  and, during a warning, the arrival countdown; the next level's cost, time and
  shortfall with the Upgrade button; the fleets standing there with the existing
  "Load a hold" controls pointed at the store, and Unload; Abandon behind a second
  press.
- *Command bar:* `found`, `survey [q r]`, `outpost upgrade`, `outpost abandon`;
  `load` and `unload` unchanged.
- *Elsewhere:* one HUD line (`OUTPOST 12,-3 L2 store 41%`), the fleet card's fuel
  line adds the outpost's cost, and `console/attention.dart` raises notices for an
  outpost raid, a breach, a finished level and a full store.

**TUI (`clients/tui`).** An `o` overlay with the same figures as text; `F` surveys
the active fleet's sector and Enter founds; `U` upgrades; `A` then `y` abandons.
The fleet panel's load and unload keys work at the outpost. The star map marks an
outpost `O` (own) or `o`. The headless mode passes the new messages through.

**`tools/playtest/play.py`.** `state` and `status` gain an `outpost` line, `sector`
and `map` mark outposts, `do` prints the survey as one line. The harness itself is
on hold and is not run.

### 7.7 End-to-end tests

**`sim/tests/outposts.rs`** (new, whole engine, helpers from `common/`):

1. *A founding expedition.* A season empire with four Corvettes and three Haulers
   loads the cost at home, flies the lanes to a ring-10 sector with a hostile
   lair, wins, is quoted by `outpost_survey`, founds. The hold loses exactly the
   cost; the level is 1 after exactly the quoted ticks; `OutpostCompleted` arrives;
   re-entering the sector after the lair's respawn delay starts no fight; charts
   aboard are paid on the next arrival.
2. *Every refusal with its sentence:* ring 8, ring 21, an Anomaly, a derelict
   sector, 2 hexes from another empire's outpost, a second outpost, a hold short
   by one resource, a fleet in transit, a fleet under standing orders.
3. *The extractor makes what the survey said,* to the tick, at persistent, season
   and blitz pace, stops at the cap, and makes nothing at level 0 or breached.
4. *A convoy round trip makes nothing:* deuterium out, yield home; stockpile plus
   store plus holds, plus fuel burned at its price, equals what was there before
   plus the yield.
5. *Fuel:* a fleet is filled from the store at 0.08; a dry store fills nothing and
   alerts once; a fleet under standing orders is not filled and keeps its cargo.
6. *Upgrade:* refused with the exact shortfall, paid from the store, working at
   the old level meanwhile, then cap, platform and yield of the new level;
   `MaxLevelReached` at 3.
7. *A raid repelled and a raid lost.* A level-2 outpost at ring 10 with nobody
   home repels: store untouched. A level-1 outpost at ring 16 with two Corvettes
   docked loses: one ship gone, a quarter of the store and the holds above the
   floor gone and no more, breached, the extractor stopped, the repair starting by
   itself, ending on its tick, and no second raid inside the grace. With the store
   emptied below the floor first: the repair waits and starts on the delivery.
8. *Thirty game days of raids on an undefended outpost:* it still stands, and the
   homeworld's raid estimate never moved because of it.
9. *Score:* each level adds its weight; the store, the yield, `load` and `unload`
   add nothing; abandoning removes it.
10. *The fleet cap:* three fleets deployed and a fourth docked at the outpost;
    leaving the outpost then is refused.
11. *Abandon:* the store is a wreck pile of the same amount, the lair returns
    after its delay, and a new outpost can be founded.
12. *Determinism:* a world with an outpost, convoys and raids replays from its
    seed to the same hash, and resumed from a JSON snapshot mid-build and
    mid-warning ends on the hash of the world never stopped.

Unchanged and required to pass untouched: `sim/tests/bots.rs` and
`determinism.rs`. A world without outposts plays as before.

**Restart.** `server/src/database.rs`, the in-process SQLite restart test: founding,
`unload`, `upgrade_outpost` and an outpost raid are mixed into its orders, and at
least one restart must fall inside a build and one inside a raid warning.
`server/tests/scenarios.rs`: a real client over the WebSocket surveys, is refused
twice, founds, the server is killed, and after the restart the outpost, its store
and its countdown are as they were and the yield goes on.

**Single player.** `solo/tests/single_player.rs`, through `Machine::call`: found,
save, reload, replay the unsaved ticks to an identical world; an absence long
enough for a raid, reported by name; a format-3 save refused.

**Wire.** Fixtures for the four commands, the survey reply accepted and refused,
`OutpostCompleted`, both raid events at an outpost, a full state with an outpost
and a sector with one; checked by the Rust and Flutter fixture tests.

**Web.** Widget tests of the outpost panel and the founding panel at 1440 and 390,
with long numbers and a long refusal, against recorded states;
`test/support/demo_world.dart` gains an outpost for them.

**The browser check** (`tool/solo_e2e.dart`). Honest limit: reaching ring 9 with a
loaded convoy takes longer than the check may run, so it does not found. It opens
the founding panel for the starter fleet in the real bundle, asks the engine in
the worker for a survey, and reads the refusal on screen ("fleet 1 is at ring 4;
an outpost stands in rings 9 to 20"). Founding in a browser stays unverified until
someone plays it, and the journal entry must say so.

### 7.8 What to measure after it lands

Bots do not found outposts, so `sim-run` cannot measure one by itself. Additions:

- `FieldTally` gains `outpost_yield`, `outpost_hauled` (taken from the store by
  `load`), `outpost_raided` (taken by raids), `outpost_raids_repelled`,
  `outpost_raids_lost`, `outpost_ticks_breached`; `ScoreBreakdown` gains `outpost`.
- `sim-run --outpost <ring>:<grade>:<level>` installs that outpost for every bot
  seat at the start of day 3 through a `Snapshot` edit, as the starting-positions
  study re-homed players. Nobody hauls from it, so it measures the extractor, the
  raids and the store, not the convoy. The convoy is measured by the scenario
  tests and, in the end, by a person.

Numbers that would mean a constant is wrong:

| Measure | Expected | Wrong when | Constant |
|---|---|---|---|
| Level-3 yield on a grade-5 site at rings 12 to 16, as a share of the owner's home output on day 7 of a season | 4 to 6% | under 2% (nobody bothers) or over 15% | `OUTPOST_YIELD_PER_HOUR` |
| The same for the best site in the world | 16% | over 25%: a second homeworld | the same, or cap the grade |
| Payback of level 1 on a grade-3 site at ring 12 | 120 game hours | slower than the Metal Mine level bought the same day by more than half | `OUTPOST_COST`, yield |
| Raids lost by the platform alone where 5.4 calls it safe (level 2 to ring 12, level 3 to ring 17) | none | more than 1 in 10 | `OUTPOST_PLATFORM_POWER` |
| Raids lost by level 1 alone at ring 9 | about 2 in 5 | under 1 in 5 or over 3 in 5 | `OUTPOST_RAID_FACTOR` |
| Raids lost by the platform alone at rings 18 to 20 | most | fewer than half: depth costs nothing | `OUTPOST_RAID_FACTOR` |
| Share of a season week's yield a level-3 outpost keeps at rings 18 to 20 with nobody home | 40 to 60% | over 80%: defence is optional; under 20%: one bad week ends it | `OUTPOST_RAID_LOSS`, `OUTPOST_REPAIR_SHARE` |
| Share of a week's yield a defended one loses | none | any | the floor, the fog |
| Day-7 total of check-in against hands-on, same personality, with and without `--outpost` | within 3 points | wider | yield |
| Ticks per second of the 16-seat week | within 3% | slower | `process_outposts` |

**Measured, 2026-10-12.** Status: every row measured at season pace over 7 days;
no `OUTPOST_*` constant moved; one row is in its "wrong when" band for a reason
no constant reaches, and one was and is mended in code. The journal entry
"outposts measured" has the command lines.

What was built for it: `FieldTally::outpost_hauled`, read from a counter on the
outpost that `load` adds to and no rule reads; `sim-run --outpost
<ring>:<grade>:<level>[:<repairs>]`, which stands the outpost for every bot at the
start of day 3 through `Headless::install_outposts`, with `<level>+` for a level
under construction and `<repairs>` for how many repairs the store is stocked for;
and a scenario of garrisoned outposts in `sim/tests/outposts.rs`. The report also
counts what the extractor could have made, what a full store turned away, what the
repairs cost, raids that found the outpost already breached, and the time spent
breached with no repair running, for want of materials or behind a build.

Two kinds of world were run. The 16-seat week (every style once and an idle
player) on seeds 1 to 3 gives the rows that need the bots' homes. Six check-in
turtles, who never fly, on seeds 1 to 5 give 30 outposts for each ring and level;
a raid's outcome depends on the ring and the level only.

| Measure | Expected | Measured | Verdict |
|---|---|---|---|
| Level-3 yield on a grade-5 site at rings 12 to 16, share of the owner's home output on day 7 | 4 to 6% | 4.6%, 4.7%, 4.7% (median of 15 bots and three rings, seeds 1, 2, 3); 3.2% the least, 6.8% the most. By ring: 3.8 to 4.0% at 12, 4.6 to 4.8% at 14, 5.6 to 5.8% at 16 | as expected |
| The same for the best site in the world | 16% | 14.8%, 18.5%, 18.5% (the median bot; the richest site is grade 10 in ring 19 on seed 1 and grade 12 in ring 19 on seeds 2 and 3); 21.6% for the weakest home | as expected |
| Payback of level 1 on a grade-3 site at ring 12 | 120 game hours | 120.2. The Metal Mine levels the 45 bots bought on day 3 run from 10 to 16, the median 12, which pays back in 103 h: the outpost is 16% slower. For check-in bots alone the median is level 11, 79 h: 51% slower | on the edge for check-in empires early on day 3, inside for everyone by its end. Left |
| Raids lost by the platform alone where 5.4 calls it safe | none | none: level 2 at rings 9, 10, 11 and 12, 0 of 624 at each; level 3 at rings 12 to 17, 0 of 624 at each | as expected |
| Raids lost by level 1 alone at ring 9 | about 2 in 5 | 87 of 200 raids that met a standing platform, 44% (36 to 49% by seed) | as expected |
| Raids lost by the platform alone at rings 18 to 20 | most | 25% at ring 18 (70 of 282), 79% at ring 19 (97 of 123), all at ring 20 (98 of 98) | most at 19 and 20; ring 18 is the edge 5.4 says it is. Left |
| Share of a week's yield a level-3 outpost keeps at rings 18 to 20 with nobody home | 40 to 60% | 11%, 4% and 3% of what the extractor could have made is in the store at the end, at rings 18, 19 and 20. Had someone emptied the store before every raid: 35%, 21%, 18% | **wrong: under 20%.** Not the constants; see below |
| Share of a week's yield a defended one loses | none | none: a Corvette at ring 9 and three at ring 12 beside level 1, level 2 alone at ring 12, level 3 alone at ring 17, with a Frigate at ring 18 and with two Cruisers at ring 20, each repelled every raid of a season week | as expected |
| Day-7 total of check-in against hands-on, with and without | within 3 points | plus 0.2 to 0.3 points for the turtle pair, who never fly. For the four that fly, between minus 14 and plus 22 points, mean plus 2.0; three worlds that differ only in the ring the outposts stand in differ by as much | cannot be read: a world with outposts is another world to the bots that fly. The part that is arithmetic is 0.3 points |
| Ticks per second of the 16-seat week | within 3% | first 4.3%, 4.1% and 4.5% slower with outposts (seeds 1, 2, 3, side by side, unpinned). Over half of that was `outpost_at`, which every sector view asks and which searched the outposts one by one; it reads an index now. After: 0.7% slower (seed 1, each run pinned to a core, repeated with the cores exchanged) | was wrong, by `outpost_at` and not `process_outposts`; within 3% now |

**The row that is wrong, and why no constant fixes it.** A lost raid breaches the
outpost, a breached outpost extracts nothing, and the repair costs metal, crystal
and deuterium. Four in five of the sites these runs stood on lack one of the three
ores. Such a site pays for
exactly as many repairs as its store was stocked for: the floor keeps the first
repair from the raid, the repair spends it, and after the next lost raid the
outpost waits for a delivery. Left alone for the five days:

| Outpost, store stocked for one repair | Raids lost of those that met the platform | End the week breached and waiting | Share of the time waiting |
|---|---|---|---|
| Level 1, ring 9 | 44% | 24 of 30 | 59% |
| Level 1, ring 12 | all | 29 of 30 | 81% |
| Level 2, ring 13 | 8% | 10 of 30 | 11% |
| Level 2, ring 14 | 60% | 23 of 30 | 62% |
| Level 3, ring 18 | 25% | 23 of 30 | 48% |
| Level 3, ring 19 | 79% | 25 of 30 | 70% |
| Level 3, ring 20 | all | 26 of 30 | 74% |
| Level 2 at rings 9 to 12, level 3 at rings 12 to 17 | none | none | none |

The floor does what 5.4 says, once. At level 3 in ring 20 on sites of grade 6 to 8
lacking an ore, an empty store waits 95% of the time, a store stocked for one
repair 86%, for four 77%, and all 22 such outposts end the week waiting in each
case. The sites with all
three ores are the design's case: at rings 18, 19 and 20 none, one and two of
them ever waited, 39%, 17% and 12% of what they could have made is in a store
nobody emptied, and 76%, 55% and 46% would have come home had the store been
emptied before each raid, which is the 40 to 60 percent of this row.

`OUTPOST_RAID_LOSS` and `OUTPOST_REPAIR_SHARE` were tried at 0.10 each and put
back. What is left in the store at rings 18, 19 and 20 rose to 21%, 11% and 10%,
and 23, 24 and 24 of 30 still ended the week waiting, for 48%, 67% and 69% of the
time. The numbers size the skim and the repair; they do not let a site make the
resource it lacks. So this is the rule of 5.4 ("the repair waits for a delivery")
meeting the fact of 5.2 that most sites make one or two resources, and it is the
owner's to decide: `decisions.md`, 2026-10-12 (outposts measured), lists the
choices.

**A raid lost while a level is building.** Level 2 with level 3 under construction
(9.4 hours at season pace), store stocked for the repair of level 3: at rings 15
to 17, 28 or 29 of 30 lost the first raid that came, at ring 14 half, at ring 13
two, at ring 12 none. Each of those stood breached for 6 hours on average, 3.6 of
them waiting for the build to end and 2.3 repairing: 60 game hours at pace 1 with
no extractor, no tanks and no platform. The raid clock of a newly stood outpost gives
the first 4.3 hours free, so an outpost that has stood a while and orders level 3
is open to this for the whole build.

**What this cannot say.** Nobody hauls, so nothing reaches a homeworld: the
check-in and hands-on row measures 21 points of level and nothing of income.
"Kept" is what lies in a store nobody empties, which a raid skims again every
time; an owner who hauls keeps more, up to the second figure given. A full store
stops the extractor, so the safe outposts of these runs made half of what they
could. No convoy flew, no bot chose a site, and the garrisons were put there by
hand.

### 7.9 Size, honestly

Judged against this week's journal entries, where the exchange and `load` each
took one agent about a day with clients and tests.

| Part | Size |
|---|---|
| Engine core: state, founding, survey, store, `load` and `unload`, fuel, charts, quiet sector, fleet cap, persistence, score, constants in `sim.py` | 1 day |
| Extractor, levels, upgrade, with the pace scenarios | half a day |
| Raids, breach, repair, with the scenarios and the long run | 1 day; the most ways to be subtly wrong |
| Protocol, fixtures, Dart mirror, views, intel | half a day |
| Web: map, founding panel, outpost panel, notices, widget tests, the browser check | 1.5 days; more than a day, and the part most likely to grow |
| TUI, headless, `play.py`, RULES, README, journal | half a day |
| Server restart scenario, single-player test and absence report | half a day |

About five and a half agent-days, in three or four mergeable steps. The engine steps can be
one agent; the web client is better as a second, once the fixtures exist.

**To halve it**, cut in this order:

1. Levels 2 and 3 and `upgrade_outpost`: one level, one site rate. Saves half a
   day and a panel section.
2. Breach and repair: a lost raid takes the store slice and the ships and nothing
   else. Saves most of a day and four alerts, and costs the design its sting.
3. The TUI overlay: typed and headless commands only.
4. `SectorState.outpost` for other empires: own outpost on the map only, and the
   spacing refusal names the sector it is too close to.
5. The founding panel in the windshield: the command bar and the inspector's
   survey button only.

What should not be cut: the extractor (without it the outpost is fact 1's trap),
raids (without them it is free income), the survey (without it founding is a
guess), and the restart and replay tests.

---

## 8. Questions only the owner can answer

1. **Does an outpost produce?**
   *Recommended: yes*, the extractor of 5.3. A dock alone is worth little once
   tanks grow (section 3, fact 1), and "richest sites further out" needs
   something that is richer.
   *If no:* slice 1 loses the extractor, the grade and half the survey, about a
   day. The outpost is then a dock, a store and a chart bank, worth building in a
   persistent world's first weeks and hardly at season pace, and the gateway has
   only field loot to carry. Something else would have to carry it, most likely
   hull repair at docks, which is its own decision.

2. **Carried in holds, or by a ship built for it?**
   *Recommended: holds.* No new ship class; founding, supply, upgrade and repair
   are one act; `load` gets its second use. The price is 8.5 Hauler holds for a
   founding at pace 1 against 2.7 at season.
   *If a ship:* an Outpost Module (about 1,000 / 500 / 200, slow, unarmed, heavy
   on fuel, consumed on founding) makes founding one escort trip at every pace.
   It adds a sixth class to every table in Rust and Dart, about a day, and
   upgrades still need the store and the hauling, so both mechanisms would exist.

3. **Can NPC raids destroy an outpost?**
   *Recommended: no.* A lost raid breaches it, and it repairs itself. Nothing
   irreversible happens in an absence, and the sleep-safe question does not arise
   for outposts.
   *If yes:* a second lost raid while breached destroys it, nothing refunded. At
   season pace two raids can be 8.5 hours apart, which is one night, so the grace
   after a breach would have to grow to about 24 real hours at every pace, a new
   timer in a new pace class, and the absence report needs a line nobody wants to
   read.

4. **One outpost, rings 9 to 20 at first, the Wandering only by the late move?**
   *Recommended: yes.* It reads the owner's "move their outpost once to ring 21+"
   as the reason the Wandering is closed at first.
   *If outposts may stand at 21+ from the start:* slice 1 needs the Wandering's
   numbers now (a ring factor past 20, raids of 950 and up, more than three
   levels), and slice 3 has nothing left but a convenience. *If more than one
   outpost:* the state is already a map and the rules hold, but every screen that
   says "the outpost" becomes a list, and yield has to be re-tuned per outpost.

5. **Does `recall` keep carrying a full hold home once the gateway exists?**
   *Recommended: no, from slice 2.* A recall then spends gateway charge for its
   cargo and sheds the rest where it jumped from; without the research it carries
   none. Slice 1 leaves `recall` alone, so until slice 2 a convoy can load at the
   outpost, step out and recall with everything; the report column
   `outpost_hauled` will not show it, and bots do not do it.
   *If recall stays as it is:* the gateway's bandwidth matters only outbound, and
   its research line is worth about half what section 5.5 asks for it.
   *If the owner wants it closed in slice 1:* recall sheds the whole hold. Half a
   day, and it changes how pilots bring field loot home today.

6. **Does a fleet standing at the outpost count against the three deployed
   fleets?**
   *Recommended: no.* It is docked. A garrison then costs ships, not a slot.
   *If yes:* nothing in `count_deployed_fleets` changes (a little less work), a
   garrison and a convoy leave one slot for the field, and outposts become a tax
   on exactly the players who fly.

---

## 9. What was read and what was run

**Read.** `docs/design/decisions.md`, `starting-positions-2026-10-10.md`,
`gap-analysis-2026-10-09.md`, `economy/spec.md` (sections 1 to 9, 15 to 17);
`tools/playtest/RULES.md`; `docs/journal.md` (the bot, balance, hub, `load`,
single-player and home-seat entries); `shared/src/constants.rs`, `scaling.rs`,
`world.rs`, `pace.rs`, `protocol.rs`; `sim/src/engine.rs` (docking, refuelling,
holds, raids, standing orders, routes, encounters, persistence), `views.rs`,
`score.rs`, `snapshot.rs`, `persist.rs`, `commands.rs`, `intel.rs`, `headless.rs`;
`server/src/database.rs` (runtime rows); `solo/src/save.rs`.

**Run.** One engine run, on 2026-10-11, of the release `sim-run` at nice 10:
`sim-run --pace season --days 3 --seed 1 --seat all:check-in --every 86400 --json`
(five check-in bots, 259,200 ticks, 22 seconds). It is the source of the day-3
column in section 3. It is one seed and one bot per style, so the ranges are what
five empires did, not a distribution. It ran three days, one more than the brief
suggested, because day 3 was the figure wanted.

**Worked out, not simulated.** The reach table (from `base_stats` and the three
fuel modifiers), the site table (the probabilities of the terrain, presence and
density rolls, not the generator's RNG, so the counts are expectations for an
Outer Ring and no particular world's), every yield, payback, raid, hold and
gateway figure (the proposal's formulas on the code's constants). Day-7 holdings
are the journal's cores and the economy model's levels; no week was simulated.

**Not known.**

- What a convoy really costs in hull over a week of lairs. Nothing in the
  repository flies one, and it decides whether deep sites are worth their yield.
- How often a human would lose an outpost raid; the figures in 5.4 are the
  arithmetic of platform against roll.
- Whether 2 units an hour per grade point is right. It was set so the best site
  stays under a sixth of a late home's output; section 7.8 says how to tell.
- Everything about PvP, which does not exist in the engine.
