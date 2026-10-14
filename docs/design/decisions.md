# Design decisions

Owner decisions that set the game's identity. Newest first. Proposals that
conflict with these need the owner's sign-off.

## 2026-10-14 (the score is what an empire has built and keeps; the knobs a playtest found wrong)

The playtest of 2026-10-14 (`docs/playtests/2026-10-14/README.md`) was won by
the player who never left home: charts filled the 25% cap early, kills
barely scored, raids scored nothing, and a fleet in the field was core not
built. Asked how combat and exploration should score, the owner first chose
"Rebalance + soften cap" (kills worth more, charts a third, points past the
cap counting at a falling rate), then said:

> i kinda want kills to not count, but give more resources, so the
> resources turn into points back at home? i think ogame scored 0 for
> defense (disensenctive turtling) but scored per ship in your fleet. so you
> score was basically buildings+research+fleet. but fleets could be
> destroyed which would drop your score a lot. i LIKE 0 points or half on
> defense, if we went 0 poitns on defense BUT raids give good ressources it
> would encourage some defense building so you can acquire materials that
> turn into points via building/research/fleet creation. explore those
> avenues as well

On the jump law, the owner's words: "users may make large, fast ship fleets
for guerilla warfare type battling".

**The decision as built.**

- *The score* is `buildings + research + fleet + outpost + defence`
  (`sim/src/score.rs`), recomputed from state: the cost of every finished
  building and research level, of the ships the empire has now (Solar
  Satellites among them), of the outpost's finished levels, and of its
  defences at `DEFENCE_SHARE`, which is **0**. A fleet destroyed takes its
  ships out of the score. There are no combat or exploration points, no cap,
  no `EXTRA_CAP`, `KILL_SHARE`, kill pay factor, `BOARD_VALUE_SHARE`,
  `DISCOVERY_POINTS`, chart points or `RELIC_POINTS`; `Player.combat_points`
  and `explore_points` and the combat's `peak_power` are gone, and
  `Snapshot.credited` is `Snapshot.charted` (a first boarding is no longer
  remembered: nothing pays for it). The board's row is the parts
  (`LeaderboardEntry`: `score`, `buildings`, `research`, `fleet`, `outpost`,
  `defence`).
- *Kills pay in wreckage*: `WRECK_VALUE_K` 0.57 to **1.71** (three times).
  The farm heat stays on wreckage (`FARM_DECAY`, `FARM_HALF_LIFE_RESPAWNS`):
  it still bounds one sector as a tap.
- *Raids pay well*: `RAID_DEFENSE_SALVAGE_FRACTION` stays **0.30** (the
  owner's earlier halving to 0.15 was for a world in which defences scored).
- *Exploration pays in resources and scores nothing*, as kills do (the
  builder's recommendation; the owner did not say). A chart delivered to a
  dock pays `chart_value(T) = 50 + 50 x T^1.5` units of loot into the
  stockpile for each sector new to the empire, at any pace and as far as the
  caps allow (`CHART_VALUE_*`, `ChartDelivered.paid`); a first boarding pays
  its loot as any boarding does; a relic makes a site pay `RELIC_LOOT_MULT`
  (6) times its loot.
- *The jump law* is linear: `max(1, ceil(JUMP_SPEED_REF / speed))` with
  `JUMP_SPEED_REF` **25,000** (was `sqrt(50,000 / speed)`). A Light Fighter
  keeps its 2 ticks, a Cruiser 2 until Impulse Drive 4 and 1 from it; a
  Small Cargo takes 5, a Large Cargo 4, a Recycler 13, a Deathstar 81 on
  Hyperspace Drive 7. Pirates jump by the same law.
- *The pirates' clock* (a destroyed group's return, a wounded one's
  mending and the farm heat's half-life) is a finds timer, P^0.5
  (`scaling::npc_respawn_ticks`; `NPC_HEAL_SHARE` is classed Finds): a group
  is whole again 4 to 9 minutes after a fight at blitz, 26 to 66 at season.
- *Pirates past ring 8 are packs*: a group flies the heaviest of
  `NPC_LADDER` (Light Fighter, Heavy Fighter, Cruiser, Battleship) of which
  its ring's power makes `NPC_MIN_PACK` (3) ships. Power per ring is
  unchanged; Light Fighters to ring 15 (3 to 8 of them), Heavy Fighters to
  ring 20 (3 to 7), Cruisers to 25, Battleships beyond. `NPC_CRUISER_RING`
  and `NPC_BATTLECRUISER_RING` are gone. `WORLDGEN_VERSION` is 5.
- *The deuterium opening*: `STARTING_RESOURCES` deuterium 100 to **800**,
  what Robotics Factory 2 and the Research Lab ask of it. A starting
  Deuterium Synthesizer 1 was the other candidate; the starting stock is
  the smaller change (one number, no energy and no production moved).
- *Pace*: no preset moved. `tools/playtest/README.md` says an hour of
  agents wants `IAC_PACE=1200` and a person `IAC_PACE=300`.
- `ECONOMY_VERSION` is 15, `WORLDGEN_VERSION` 5, `SAVE_FORMAT` 13 (the
  player row and the combat lost fields, the bots' memory lost its standing).

**Measured beside it.** Bot worlds of `sim-run`, seeds 1, 2 and 3, against
master at 1b2197e: `--pace season --days 7`, `--pace blitz --days 1`,
`--pace blitz --hours 1 --seat all:all`, `--pace 1200 --hours 1 --seat
all:hands-on`; every variant one change on the branch, its diff kept with the
runs. Hands-on medians; flyers are the raider, explorer and mixed bots.

| variant | season 7 d: turtle / flyers | blitz 1 d: turtle / flyers | blitz 1 h | pace 1200, 1 h | raid wreckage, season week, all bots |
|---|---|---|---|---|---|
| master (old score, capped extras) | 9264 / 9870 | 16503 / 24046 | 1336 / 1523 | 4444 / 4972 | 14423 |
| **as built** | 8644 / 9129 (1.06) | 15607 / 20022 (1.28) | 1136 / 1645 (1.45) | 4159 / 5102 (1.23) | 15672 |
| wreckage x2 | 8648 / 8657 (1.00) | 15607 / 20039 | 1136 / 1575 | 4159 / 4742 (1.14) | 15472 |
| wreckage x4 | 8097 / 8895 (1.10) | 15607 / 20098 | 1136 / 1993 (1.75) | 4159 / 5234 | 15974 |
| raid wreckage 0.15 | 7890 / 8060 (1.02) | 15607 / 18760 (1.20) | | | 7385 |
| raid wreckage 0.20 | 9155 / 8469 (0.93) | 15607 / 18784 (1.20) | | | 10418 |
| raid wreckage 0.45 | 10504 / 9796 (0.93) | 15679 / 21249 | | | 25621 |
| exploration in points, soft cap (A) | 8648 / 9403 (1.09) | 15607 / 22633 (1.45) | 1136 / 2120 (1.87) | 4159 / 6069 (1.46) | 14694 |
| defences at half (same runs as built) | 9014 / 9384 | 16454 / 20936 | | | |
| deuterium 100 at the start | 8648 / 8773 | | 1303 / 1665 | 4302 / 5089 | 15105 |

What decided each value, and the rest of the numbers, are in
`docs/journal.md` (2026-10-14, the score rework). In short: at x3 a hunt
pays its fuel several times over at a fast pace (at pace 1200 the flyers'
wreckage is 1,921 points against 255 of fuel, about a twelfth of what the
mines made) and does not beat the mines; at season wreckage stays a few
percent of an empire's income at any multiplier tried, because loot grows
with P^0.5 and the economy with P. At 0.30 a season week's raid wreckage is
13.5% of what the mines made and the turtle and the flyers end level; at
0.45 it is 21% and the turtle wins, at 0.20 9.2% and at 0.15 6.7% (the
turtle against the flyers is a median of three bots and moves by a tenth
from seed to seed, so the ratio is a direction, not a measurement). Exploration in
points under a soft cap gave the flyers the most (1.45x a turtle in a blitz
hour against 1.28x) and keeps a second currency the owner's model has no
place for; exploration in resources is the one rule for everything the
field pays.

**What undoes each.**

- The score: `score::score_of` and `Score::total`; kill points come back
  with a `credit_kill` in `end_combat`, chart and boarding points with the
  two `explore_points` lines, and they need the player row's fields back
  (`SAVE_FORMAT`). The exploration-in-points variant is
  `scratchpad/tuning/patches/exploreA.py` of the builder's run (not in the
  repository): it is not reachable in `sim-run`, because it needs state the
  engine no longer keeps.
- Defences at half: `score::DEFENCE_SHARE` 0.5; `sim-run --defence-share
  0.5` scores any run so without changing the game.
- Wreckage: `scaling::WRECK_VALUE_K`. Raids: `RAID_DEFENSE_SALVAGE_FRACTION`.
  Charts: `scaling::CHART_VALUE_*`; a chart that pays nothing is
  `CHART_VALUE_BASE` and `CHART_VALUE_THREAT` 0. Relics: `RELIC_LOOT_MULT` 1.
- The jump law: `scaling::jump_ticks` and `JUMP_SPEED_REF`.
- The pirates' clock: `scaling::npc_respawn_ticks` and
  `npc_heal_step_ticks` back to `econ_ticks`, the `NPC_HEAL_SHARE` class.
- The packs: `scaling::npc_class` and `NPC_LADDER`, `NPC_MIN_PACK`.
- The opening: `STARTING_RESOURCES`.

**What the builder decided within it.** None of it was put to the owner.

- *Charts pay at any pace.* A fleet charts as many sectors in an hour at
  any pace, and a probe charts one every tick or two; a chart priced as loot
  (P^0.5) paid an explorer 13,700 points in a blitz hour in the first try.
  Undo: `.scale(finds_mult())` in `deliver_charts`.
- *A chart pays the homeworld's stockpile*, also when it is delivered at the
  outpost: it is information sold, not cargo.
- *A relic multiplies the site's loot* rather than paying a fixed sum; its
  roll moved before the hold is filled, with no draw moved.
- *The board shows `defence`* although it is 0 today, so the parts always
  add up to the score if the share moves.
- *The bots* (`sim/src/bot/`, the other agent's ground): the smallest change
  that compiles. With no cap there is nothing for a wing to stop for:
  `Brief.points_wanted`, `has_work`, `Bot::under`, the bot's `rule`, its
  remembered `Standing` and its leaderboard request, and `Style::ignore_cap`
  are gone. Every wing now flies whenever it has work.
- *`sim-run`* lost `--cap`, `--meter` and `--ignore-cap` and gained
  `--defence-share`, a flight table by the kind of fleet, the share of fights
  begun against a group that had not mended, and what charts paid.
- *The outposts' platforms were not retuned.* Against packs a level-2
  platform holds through ring 13 (it held through the middle of the Outer
  Ring against a lone Cruiser or Battlecruiser) and level 3 to ring 20; the
  forecast table of `sim/tests/outposts.rs` was measured again and the
  season week's post at ring 14 is level 3.

**Unsure of.**

- At season wreckage is a few percent of income at x2, x3 or x4: fighting
  near home is income a player notices at a fast pace and not at season.
  That is the finds class (P^0.5) against the economy (P), and a constant
  cannot mend it.
- The bots build defences for the half that scored, and in a blitz day the
  check-in and hybrid flyers, whose walls are largest, fall to 0.6 to 0.8 of
  a turtle at zero (0.9 to 1.2 at half). The bots' rewrite should build against
  what raids leave, not against a score.
- Bots almost never leave a fight (2 to 20 flights a world), so "a group is
  fought again while hurt" was counted 0 to 3 times a world: the clock's
  change is measured in tests, not in bot play.
- The planning simulation says flying styles lose under the new rule (in a
  blitz hour the explorer 64% of the builder, the raider 61%; at season 87%
  and 78%), where the bots say they lead; its field model pays less than the
  bots fly for.
- In a blitz hour the hands-on turtle scored 13% less with 800 deuterium
  than with 100 (its opening goes to the lab first); the season week moved 2%.

## 2026-10-14 (a patrol brings the salvage home)

`patrol_home` hunted the groups in range of home, scooped the wreckage of
its own kill where it stood, and went home only when the beat was clear or
the hold reached `cargo_return_pct`. The owner asked:

> i would like patrol home to also collect salvage and return when cargo
> full or does another policy do that? like fighting and returning with
> salvage it good income it seems.

Asked which gap he meant, he answered:

> for patrol-home salvage tweak do what you recommend

**The decision.** `patrol_home` hunts within its range at its engage ratio
as it did, and between fights it collects salvage: any pile within its
range that it can reach on lanes it may enter, not only the wreck of its own
kill, nearest first, and a pile worth the fuel to fetch it by the rule the
hauling doctrines use. It keeps doing so while its hold has room. When the
hold is full (`cargo_return_pct`), or the next pile it would fetch would not
fit, or there is nothing left within range to fight or collect, it flies
home, unloads, is mended and refitted there as any docked fleet is, and goes
out again. Its fuel budget, its disengage ratio, its refine switch and its
way home under bad odds are as they were. Every act says why.

**What undoes it.** `GameEngine::policy_patrol_home` (`sim/src/engine.rs`):
its search for the next target takes a contact or a pile that repays its
fuel on an empty hold (`Reach::wreck_repays` with `hold`); taking `|| ...`
out of that goal is the old patrol, which still scooped what lay where it
stood.

**What the builder decided within it.** None of it was put to the owner.

- *Contact and pile are one search.* The patrol goes to the nearest of
  either, by lanes it may enter (`path_through`, fights on the way allowed,
  as the patrol always flew). A pile in a sector with a group is a contact.
  Undo: two searches, contacts first.
- *"Would not fit"* is read with the one rule: the nearest pile is judged on
  an empty hold, and if what the room left would take of it no longer
  repays the fuel, the fleet goes home to unload first ("returning to unload
  the salvage: the hold has room for 40 of the 900 units of wreckage at
  [5,-3]"). A pile that repays on the room left is fetched, and the
  `cargo_return_pct` threshold brings it home after. Undo: the `Unload`
  branch of the patrol.
- *One rule of "worth the fuel"*: `Reach::wreck_repays`, which
  `salvage_and_sites` now calls too; it is `repays` on `wreck_worth`, as
  that doctrine's closure was.
- *Its own kill's wreck, or another's.* `FleetPolicy::work_sector`, unused
  by every doctrine but `mine_and_return`, holds for a patrol the sector of
  its last fight, set when a fight it stood in ends; a pile scooped there is
  "the wreckage of the kill", anywhere else "wreckage at X that it did not
  make". New orders other than `mine_and_return` start with no
  `work_sector` (they started with the sector the fleet stood in, which
  nothing but `mine_and_return` read). Nothing new is stored, so
  `SAVE_FORMAT` stays 12; what a stored world plays next moves, so
  `ECONOMY_VERSION` is 14. Undo: the line in `end_combat`'s loop over the
  fleets.
- *Turning home with a full hold* is worded for the patrol "returning to
  unload the salvage: cargo 87%" (the hauling doctrines still say
  "returning: cargo 87%"). With nothing left the hold reason is the old one
  and ", and no wreckage that repays the fuel".
- *Unloading is said by every doctrine.* A fleet under orders that docks at
  home with a hold reports `unload` ("unloaded 500 metal and 120 crystal
  into the stockpile", and what stays aboard for want of room). It is
  `report_unload` beside `dock_fleet`, so `mine_and_return` and
  `salvage_and_sites` say it too; making it the patrol's alone would have
  been a doctrine's name inside docking. Undo: the call in `dock_fleet`.
- *The bots are not touched.* Their check-in strike wings fly
  `patrol_home` and now bring salvage home. What it did to them is in
  `docs/journal.md`.

**Unsure of.**

- Whether it is the income the owner expected. Measured, a patrol alone
  brings home what it did before to the unit in most runs: the old patrol
  already scooped its own kills and went home at `cargo_return_pct`, and a
  Light Fighter's hold (50 times the square root of the pace) is such that
  a few of them carry what a kill near home leaves, so the hold seldom
  limits it; fuel does. At
  season pace ten Light Fighters burned more points of deuterium in a day
  than they brought home in salvage. The new rule matters where piles the
  patrol did not make lie in its range: other empires' fights and losses,
  and the leftovers of its own big kills. The figures are in the journal.
- A Small Cargo merged into a patrol carries the salvage and works under
  the doctrine (the scenario shows it), but in the measured runs it only
  slowed the fleet. The README and the rules say so.
- The bots' strike wings stay under the doctrine longer, since it now
  finds work more often, and a world of bots plays differently after the
  first changed fight. The sum of all totals fell by 0.5 to 2.0% in all
  five runs measured, inside a spread where single styles moved 18%.

## 2026-10-14 (Solar Satellites that fall in a raid are rebuilt as defences are)

Since slice 4 a raid at the homeworld is a fight in which the Solar
Satellites stand. A satellite that fell was lost, and a defence that fell was
rebuilt at once at OGame's rate. In a blitz day of sixteen bots 16,387
satellites fell and none came back. The owner was asked whether satellites
should stay out of the fight, be rebuilt as defences are, or go on falling,
and was given the second as the recommendation. His answer:

> for solar satellites, follow your gut and recommendation

**The decision.** Solar Satellites that fall in a raid at the homeworld are
rebuilt as defences are: the same rule and the same three constants
(`DEFENCE_REBUILD_PCT` 70, `DEFENCE_REBUILD_SPREAD` 10,
`DEFENCE_REBUILD_EACH_BELOW` 10), at once and for nothing, after every raid
that was fought, lost or not. They still stand in the fight and still soak
fire, and about three in ten that fall are gone.

**Why.** Classic OGame does not rebuild a satellite; this is a departure from
it and from section 7.6 of `docs/design/ogame.md`, which says "ships and
satellites lost are lost" and is left as the study it was. There an attack on
a planet is rare. Here a raid is a routine of every day, three or four a day
in a season, and a routine should not be the chore of buying the same
satellites again. It is one rule for everything at home that is not a ship.

**What undoes it.** The satellites' line in `GameEngine::rebuild_at_home`
(`sim/src/engine.rs`): the roll after the last defence and the row it adds.
A rate of their own would be one constant beside the three.

**What the builder decided within it.** None of it was put to the owner.
Each line ends with what undoes it. `ECONOMY_VERSION` is 13; `SAVE_FORMAT`
stays 11.

- *The satellites are a kind of their own, rolled for after the last
  defence.* Fewer than ten fell: each on its own 70 in a hundred; ten or
  more: a share of 60 to 80 percent, cut off to a whole number. The dice are
  the ones the defences are rolled with, seeded from the tick and the owner,
  and the satellites' draws come last, so no defence comes back differently
  than it did. Undo: the order of the draws in `rebuild_at_home`.
- *On the wire nothing is added.* `RaidResolved.rebuilt` has a row for the
  satellites, first, since the rows are in catalogue order as `lost` is.
  `satellites_lost` is still what fell in the fight, before the rebuild, and
  `structures_restored` is still the defences alone; how many satellites
  came back is their row. A reader that summed `rebuilt` and called it
  defences is now wrong, and the two in this repository that did
  (`settle_raid` itself and the scripted world of the widget tests) were
  moved. Undo: a `satellites_restored` beside
  `structures_restored`, which is a wire change.
- *`rebuild_defences` is `rebuild_at_home`*, since it rebuilds more than
  defences and nothing at an outpost.
- *The forecast's `kept` still counts a fallen satellite as lost*, as it
  counts a fallen defence: it is the share kept before anything is rebuilt,
  and every screen and the rules now say "anything" where they said "any
  defence". So the label of a forecast did not move. Undo: nothing to undo;
  a forecast net of the rebuild would be a second figure.
- *The absence report counts the satellites rebuilt* (`Tally` in
  `solo/src/away.rs`): "destroyed 12 Solar Satellites and 8 defences, and 9
  of the Solar Satellites and 5 of the defences were rebuilt". The tally is
  in the `host` part of a stored document's header while an absence is being
  played. See "unsure of".
- *The runner's raid table* has the satellites rebuilt beside the satellites
  fallen (`sim-run`, `raid_satellites_rebuilt` in `--json`), and its `cost`
  is net of them without a change, since it already priced `rebuilt` by
  unit.
- *The bots are not touched.* They order satellites when the mines run short
  of energy, and now find fewer missing. What that did is in
  `docs/journal.md`: in a blitz day they bought 5,703 satellites where they
  had bought 16,306, and what raids cost them fell from 78,431 points to
  36,806.

**Unsure of.**

- `SAVE_FORMAT`. The header's `host` part gained a field in the single-player
  host's tally, and the rule is that the format is bumped when the header
  changes. It was not bumped, as the brief for this work said, and nothing is
  read wrongly for it: a document of economy version 12 is refused before
  its header's `host` part is read. A build that kept version 13 and changed
  the tally again would need the bump.
- Raids cost a world of bots less than half of what they did at blitz
  (satellites were four fifths of that cost and are a little over half of
  it now). Whether that makes raids too cheap is the three constants' to
  answer, and they move the defences with the satellites; a rate for
  satellites alone is not built.
- After a raid that left no defender, about seven satellites in ten stand
  again with the defences, so the mines of a beaten homeworld keep most of
  their energy. That is the decision as worded ("lost or not"); it was not
  weighed on its own.

## 2026-10-14 (the windshield as a picture of class marks, and the battle view: what the agent decided)

Slice 5 of `docs/design/ogame.md`, section 12, the last: the windshield in
class marks, a round, a flight, a pursuit and a raid drawn on it, and the
battle view. It is the web client's, with one addition to what a round
says. None of it was put to the owner. Each line ends with what undoes it.
`SAVE_FORMAT` is 12 (it was 11); `ECONOMY_VERSION` stays 12: nothing a
world plays moved.

**In Rust.**

- *A round says what entered it.* `BattleUnits.entered` and
  `BattleSide.entered`: the units of a kind, and of a side, that stood in
  the round when it began. The battle view prints a count before a round
  and after it, and the other way to the first was adding `count` and
  `lost` in Dart. `Combat.last_round` is a round's report and is in
  `Snapshot`, so the stored document changed and `SAVE_FORMAT` is 12;
  nothing is migrated. Undo: the two fields (`shared/src/protocol.rs`,
  `combat::report`, `battle_side` in `sim/src/engine.rs`); the view would
  then add two figures of one row.
- No column and no constant was added to `shared/tests/wire/mod.rs`, and
  `numbers.json` did not change.

**The engine does not keep a fight's rounds.** The brief left it open: the
client holds the rounds it was sent, and the engine could keep them for a
reload and for a fight fought while the player was away. Measured first, in
worlds of bots played by a scratch program outside the repository (fifteen
bots, every style, seed 7):

| | A season, three days | A blitz, one day |
|---|---|---|
| Rounds fought | 1,534 | 35,631 |
| A round as JSON, mean / 95th / most | 1,296 / 2,291 / 3,316 bytes | 1,552 / 2,365 / 5,926 bytes |
| Volleys a round, both sides, mean / most | 3 / 13 | 5 / 30 |
| Rounds a fight in open space, mean / 95th / most | 2 / 4 / 5 | 1 / 2 / 185 |
| A fight's rounds together, mean / most | 1,947 / 6,403 bytes | 1,624 / 182,879 bytes |
| A raid's rounds together, mean / most | 4,212 / 11,452 bytes | 6,386 / 19,624 bytes |
| The stored document | 1,556,361 bytes | 2,917,671 bytes |
| The last four fights of every empire, all rounds | 243,905 bytes | 201,194 bytes (346,355 at the most) |
| A full state, mean / most | 109,475 / 269,958 bytes | 183,092 / 495,312 bytes |

A round of twelve classes firing at twelve, which no bot flies, is 39,531
bytes (the widget tests' written round, 288 volleys). So keeping four
fights an empire would add 7 to 16 percent to a stored world of bots that
never look at them, and 13 to 16 KB to every full state; a fleet of twelve
classes in a long fight would add megabytes unless its rounds were cut to
the last few; and one fight in the blitz ran 185 rounds. Against that: a
fight is watched while it is fought, and the page that watched it has every
round. So:

- The client keeps the rounds it receives while the page lives: the last
  120 of a fight (`Battle.roundsKept`) and the last 8 fights that ended
  (`Battle.fightsKept`), in `StateMapper`.
- A page loaded in the middle of a fight has the state's `last_round` and
  the rounds from then on, and the view says which it lacks and why
  ("ROUNDS 1 TO 4 WERE FOUGHT BEFORE THIS CONSOLE HAD THE FIGHT. THE SERVER
  KEEPS A FIGHT'S LAST ROUND AND NOTHING OF ONE THAT IS OVER...").
- A reload forgets the fights that ended before it, and a fight fought
  while the player was away is in the log and the away report, not in the
  battle view.

Undo: `Combat` keeps its rounds, the last so many, and `Snapshot` a short
list of each empire's ended fights; `BattleState` carries them and a full
state lists the ended ones; `SAVE_FORMAT` moves; `StateMapper._resumed`
takes them as a fight's `history`. The view needs no change. A cheaper
half-way is the browser's own storage for what the page holds, which
survives a reload and nothing else.

**The picture.**

- *A mark is a class.* Up to four units of a kind are drawn a glyph each.
  More are one glyph leading a block of pips, `2 x sqrt(count)` of them:
  28 for two hundred, 89 for two thousand, 141 for five thousand. The
  count is printed under it with the class's three letters (`LFT 2,000`),
  and under that
  a bar in three parts from the state's counts, whole, damaged and at
  risk; the block's pips are tinted in the same three shares. A drawing
  rule, in `MarkShape` (`lib/scenes/battle/field.dart`). Undo: `pipsFor`
  and `MarkShape.literal`.
- *One size for both sides.* The marks of a scene are drawn at the largest
  of ten sizes at which both sides fit their ground, from twice the full
  size for a handful down to a third; so a block on one side is told
  against a block on the other, and two ships look as they did when the
  scope drew a ship each. On a ground too full for whole blocks (twelve
  classes a side on a phone) the block is a strip two pips deep and the
  label is the count alone. Undo: `BattleField._sizes`.
- *The three letters are Dart's.* A ship's are the ones the gate cards
  already print (`shipAbbr`: `LFT`, `CRU`, `BSH`); the eight defences got
  theirs beside them (`unitAbbr` in `lib/console/threat.dart`: `RKL`,
  `LLS`, `HLS`, `GAU`, `ION`, `PLT`, `SDM`, `LDM`). They are wording. Undo:
  a column on `ShipClass` and `DefenceKind` in Rust, one line each in
  `shared/tests/wire/mod.rs`.
- *Eight glyphs for the defences*, emplacements with their guns up, in the
  hand of the ships' (`Sym.defence`); the Solar Satellite keeps its own.
  They are crystal blue, as the lists print a battery.
- *What the scope draws is the state's.* At rest: the fleet's rows and the
  rows of the hostile groups in sight. In a fight: the fight's last round,
  every fleet, group and battery of it. Before a raid's first round: the
  fleets at the dock, the homeworld's `satellites` and `defences` or the
  outpost's `platform`, and `battles[].raid.ships`, which carry counts and
  no hull, so those bars are drawn whole and paler. At rest the scope
  shows no defences. `Sides.at`.
- *A rock under a mark is drawn faint*, as every rock is while a fight is
  on, and the planet during a raid. Undo: `_underAMark` in
  `lib/scenes/windshield/painter.dart`.

**A round on the scene.**

- *Eight beams a side and a haze.* A side's volleys are ordered as the
  panel orders them (units destroyed, hull damage, shots). The first eight
  are beams, drawn in that order and fading down it, their width by the
  shots (the logarithm of it), with one to three pulses. The rest are one
  faint stroke each, drawn together. Twelve classes at twelve is 16 beams
  and 272 hairlines. Undo: `BattleField.beamsASide`.
- *A beam that reached no hull stops at the shield*, which flares toward
  where the fire came from; the share of a volley's shots that bounced
  sets the sparks off it. A volley that reached a hull lands on the glyph.
- *The marks wait for the fire.* The state of the tick already has the
  round's counts. The scope holds each mark at what it showed for 0.62 s,
  until the last beam has landed, then gives it the round's own figures,
  prints what the round destroyed over it (`lost`) and breaks up a class
  the round left none of. Nothing is counted down locally. Undo:
  `BattleField.landing`.
- *A volley is by kind, not by fleet.* With two fleets that both fly
  Cruisers the round does not say whose fired, so a beam leaves the larger
  mark of that kind that is not leaving. Undo: the engine would have to
  report volleys by group.
- *The scope prints the round and how the fight stands* on its centre
  line: the label of the fleet's own `flight` quote, or a raid's forecast.
  A fleet that only stands by has the round and no label.
- *Sound.* `fire` once a round and `hit` for the three heaviest beams a
  side; it was a `hit` a stroke.

**A flight and a pursuit.**

- The leaving fleet's marks turn to the lane of `fleeing.to` and draw off a
  fifth of the way toward it, where they stay in sight under fire; a
  course is drawn to the gate, with `JUMP IN n` (`fleeing.ticks`) and GUNS
  SILENT beside the fleet. The scope starts this from the state, so a
  flight ordered from the command bar, the Defence tab or by a doctrine is
  drawn too. The stars do not streak until the jump is made. Undo:
  `WindshieldSim._status` and `_flight` in the painter.
- A pursuer's lane (the one to `pursued_by.from`) is marked from its gate
  inward, with `PURSUIT n` (`arrives_in`) and `CATCHES` or `TOO LATE`
  (`catches`). When it lands, its marks come onto the scope from that
  gate. Undo: `_pursuit`, `WindshieldSim._followPursuit`.
- Hostiles are always drawn on the upper half, so a fleet leaving by a
  northern lane draws off toward them. The lanes are where the gates are;
  the two sides are not.
- *A gate key is never dropped without a word.* The scope draws a fleet's
  arrival for about half a second after it lands, and a gate pressed in
  that time did nothing and said nothing; a fleet that lands with a pack
  on its trail is when the next gate is pressed at once. Now the key is
  taken: the order is sent at once and the scope starts the new jump from
  where it is (`WindshieldSim.startWarp`). The dock's tiles act during an
  arrival too, all but Stop, which has no jump to call off. A gate pressed
  while a jump ordered here is still charging is answered with a toast
  (the fleet is charging a jump already, Stop calls it off) and sends
  nothing. The end-to-end check's waits after a landing were left as they
  are. Undo: the `waiting` conditions in `jump` and `act` of
  `windshield_view.dart`.

**The battle view.**

- *A control opens it, not the fight.* The owner said "maybe starting a
  fight opens a modal ... idk", and that watching from the windshield is
  the fun of it. So a fight is watched where it is fought, and the view is
  one press away: `I` on the windshield, the Battle view button of the
  round's panel (on the windshield, the Defence tab and the outpost's
  screen), the fight's line on a phone, and Watch the fight on the
  Overview's raid card. With no fight on show, `I` and a Fights button in
  the fleet status open it on the newest fight held. A setting under `?`,
  off by default, opens it by
  itself on the first round of a fight of the player's. Undo: flip the
  default of `ConsolePrefs.battleView`.
- *A modal over any screen*, hosted by the shell (`Console.openBattle`),
  not a sixth screen: a raid is watched from the homeworld's tab and a
  fight from the windshield without leaving either. Esc shuts it.
- *It follows, and a step holds it.* Opened during a fight it shows the
  newest round as each comes. Back, Next and the arrow keys hold it on a
  round; Follow lets it go on; Replay plays the round's fire again. After
  a fight it steps through the rounds held, and the fights held are a row
  of buttons at its top.
- *Laid out as OGame's report.* Each side by class: before (`entered`),
  lost, after (`count`), damaged, at risk, hull. Each side's fire as a
  sentence of its totals, then every volley as a row, the heaviest first.
  The rows are built as they scroll into view, so 288 volleys cost what
  the 40 in sight cost.
- *The round's panel became a few lines*: what stands and what the round
  took, what each side fired, the heaviest volley, the losses so far, the
  end. Its tables are the battle view's. Undo: none that keeps the view.
- *What stands behind the view is not read out with it* (`BlockSemantics`).

**Removed.** The scope's sixteen marks at one scale (`markScale`,
`marksFor`, `WindshieldSim.marksASide`, the `Unit` list, the EACH MARK
legend), the brackets round each hostile mark and the trails behind the
fleet's; the round panel's standing table, its three volleys and its Show
last round (`volleysShown`, `fireFigures`). Nothing was moved to
`.archive/`: no file was deleted.

**Phones.**

- The scope at 390 px is 278 px across. Twelve classes a side fit as
  strips with their counts; a fleet of three or four classes keeps whole
  blocks and names.
- The lines over the fleet rail (pursuit, leaving, leave, the round) stand
  under the hex, not on it. The round's line opens the battle view; the
  others open the sheet with the flee control, as before.
- The toasts stand over the upper half of the scope, where the hostile
  side is. While a fight is fought where the fleet stands, each round
  takes down the ones that ask nothing; what they said is in the log and
  among the alerts. Undo: `_clearForScope` in `windshield_view.dart`.
- In the battle view the picture is pinned over the tables.

**What a frame costs**, measured in a widget test (a debug build under
`flutter_tester`, which is several times slower than the released page),
for the largest written state: twelve classes a side, 4,992 ships a side
and then 60,000, a round of 288 volleys every sixty frames. The scope's
tick and paint: 1.1 to 1.9 ms a frame, 8 to 18 ms at the worst (the frame
a round lands in), at 1440 and at 390. The frame that takes a tick's state
is the whole console built again, as it was before this slice: 37 to 44 ms
at 1440 and 21 to 23 ms at 390 with the battle view shut, 52 to 60 ms and
31 to 33 ms with it open. Master's client, measured the same way in a copy
outside the repository, with its round panel: 55 to 71 ms and 25 to 32 ms,
and 1.0 to 1.9 ms for its scope. The scope draws 192 to 1,008 points for
those fleets and never a ship each. The journal has the browser's figures.

**The scripted world of the widget tests** sends `entered` with every
round. Nothing else of it changed.

**The end-to-end check** reads the lines beside the scope, opens the
battle view with `I` during the six's fight where it catches it and from
the report after it, steps back a round where the fight had two, and
reads a raid's rounds in the view the Defence tab opens. It measures the
gaps between the page's frames with the scope at rest and through the
two's fight, and prints them; it asserts nothing of them. Its flight step
now flies the two home when it stands down, which it did not, so the run
that first met that branch failed. Undo: `standDown` in `flight` of
`tool/solo_e2e.dart`; the check then fails whenever the two win their
fight before they can leave it, which was one run in nine here.

**Unsure of.**

- Whether the battle view should open by itself. It does not; the setting
  is there.
- The step from four glyphs to a block at five ships: five ships are one
  glyph and four pips.
- A ship at risk is ember in the lists and on the scope, and the hostile
  side is ember too; a hostile mark's whole part is drawn a darker red so
  the three parts can be told, which is weaker than on the fleet's side.
- The banner raised when a fight begins stands over the northern gate's
  card for its three seconds on a desktop; it used to stand on what is now
  the hostile half.
- A fleet leaving northward draws off toward the hostiles (above).
- Whether a fight's rounds should survive a reload. The measurements and
  the way to it are above.
- The frame that takes a tick's state builds the whole console, 37 to 44
  ms in a debug build with a fleet of twelve classes (master: 55 to 71).
  It is one frame a second, the one a round's fire starts in, and whether
  it is a dropped frame in the released page with such a fleet was not
  measured: no world the check can reach has one. It was not taken up.

## 2026-10-14 (the web client on raids fought as battles: what the agent decided)

Slice 4 of `docs/design/ogame.md`, section 12, stage 2: the hand-written Dart
of `clients/web` on what the engine sends since raids are fights, after
master was merged into the branch. None of it was put to the owner. Each
line ends with what undoes it. `SAVE_FORMAT` and `ECONOMY_VERSION` are as
stage 1 left them, 11 and 12.

**In Rust.**

- *The forecast is the raid being fought until it ends.* Stage 1 left
  `homeworld.raid` as the forecast of the next raid while one was fought,
  and its own session called that a surprise: a screen that read only the
  forecast said no raid was announced during a raid. `RaidForecast` has
  `rounds_left`, present while the raid is fought; `homeworld.raid` and
  `outpost.raid` are then that raid, with what it flew in with and the odds
  from where the fight stands, and no `arrival_tick`. So one field says
  where a raid stands, for the web client, for an agent and for a bot, and
  nobody looks in `battles` to find out. The fight's rounds are still in
  `battles`. The bots read the forecast, so a world of bots plays a little
  differently while a raid is fought (the journal has the two runs); no
  world of economy version 12 exists outside this branch, so the version
  did not move. Undo: `GameEngine::raid_fought` and its two callers, and the
  field; a client then finds the fight in `battles` first, as stage 1 said.
- *The engine's refusal of a defence past its limit* said "stand, are being
  rebuilt or are on order". Nothing is being rebuilt any more: it says
  "stand or are on order". Undo: the sentence in `commands.rs`.
- *The hand-written raid fixtures* stand at the sample player's homeworld
  (they stood in another sector), and the sample homeworld's raid arrives
  after the sample tick, not before it. Undo: `golden_fixtures.rs`.
- *For the client*: the generated constant `defenceRebuildEachBelow`
  (`DEFENCE_REBUILD_EACH_BELOW`), so the rebuild's rule is printed whole.
  *For its scripted world*: `numbers.json` names that constant, the three
  `RAID_LOSS_CAP_*` and `RAID_DEFENSE_SALVAGE_FRACTION`, and each outpost
  level's `platform_units`.

**In the client.**

- *Where a raid's fight is told.* On the Defence tab, under the forecast,
  and on the outpost's own screen, under its panel: the round panel every
  fight has, with the sides named defenders and raiders and the defences a
  group headed as no fleet. It is reached from any screen by the raid's
  field in the top bar (a line under the bar on a phone) and from the
  Overview's card. The windshield shows it too, to a fleet standing in it.
  No battle view was built: that is slice 5. Undo: `DefenceTab`,
  `OutpostRaid`.
- *The top bar's world name gives way to the raid* while one is on its way
  or fought, so the bar grows no wider; on a phone a line appears under the
  bar for that time and the screen below moves down by its height. Undo:
  `_RaidField`, `RaidStrip`.
- *The way out of a raid from the homeworld's and the outpost's screens* is
  the windshield's flee control without its keys (`RaidExits`): a button a
  lane, the price, that nothing follows, Stop while the jump charges. It
  asks for no preview of a lane, since no raider follows anywhere. Leaving
  during the warning is an ordinary jump from the windshield or the map, as
  before; the tab says that a fleet flown out then is not in the fight and
  offers no button for it. Undo: `RaidExits`.
- *An ending is worded from two fields.* `RaidResolved.defended` and
  `outcome` give one of four: destroyed, outlasted, lost, lost without a
  fight (`RaidEnding`). The words are Dart's, as the words for a fight's
  outcome are. Undo: a column on an enum in Rust, if the owner wants the
  words to be the engine's.
- *The last raid is the client's to keep.* The state has the raid on its
  way and the one being fought, and nothing of one that is over. The
  Defence tab and the outpost's screen show the summary of the last
  `RaidResolved` the page saw, and a reload forgets it. Undo: a field in
  the state, which is a Rust change.
- *A raid's end is two lines of the log*, a headline short enough for a
  toast and the summary under it, and a raid's own `CombatEnded` is not
  logged beside them. Undo: `StateMapper`, the `RaidResolvedEvent` case.
- *No screen reads a power sum.* `home_defence_power` with its three parts,
  `next_raid_estimate_power`, the outpost's `platform_power`, `ships_power`
  and `defence_power`, `RaidResolved.raid_power` and `defense_power`,
  `OutpostSurvey.platform_power` and `OutpostLevel.platform_power`,
  `OutpostState.raid_arrival_tick` and `RaidIncoming.threat` are read by
  nothing in `clients/web/lib`. The Overview's "Defence power" figure
  became a count of defences. Two powers are still printed, each as what a
  force is and beside nothing it could be compared with: a forecast's
  `power`, and the least and the most a raid of an outpost's ring can be
  (`raid_power`). Taking the unread fields off the wire is the engine's to
  do; the scripted world of the tests still fills the required ones.
- *The three shares are each rounded to a whole percent*, so 0.625 and
  0.375 print as 63 and 38. Undo: `sharePct`.
- *The scripted world's raids* are fights shaped as the engine's and played
  without dice, as its other fights are. Its forecast is that one fight
  played forward, so a share is all or nothing and the label is never
  `at_risk`; a raid flies one class at its base figures; the rebuild is
  the engine's rate cut off to a whole number. Its header says all of it.
  The label `at_risk`, a raid of several classes and the dice are in the
  recorded fixtures and in the browser against the engine.
- *The end-to-end check orders its first defence late.* An empire with a
  defence or a Shipyard at 2 is raided, and the check passes the world four
  minutes at a time: a raid then would be fought unseen with the docked
  fleet in it. The Rocket Launcher it used to order in its first steps is
  ordered in its raid step, after which it passes the world twenty ticks at
  a time, less than the warning, until the state says a raid is on its way.
  Undo: `raid` in `tool/solo_e2e.dart`.

**Unsure of.**

- A phone gets a new line under the top bar whenever a raid is on its way,
  which moves the screen under it. A raid is rare and the line says what
  matters; it may still read as a jolt on the windshield.
- The outpost's panel still prints the least and the most a raid of its
  ring can be, beside the forecast of a typical one. It is the range the
  roll moves in and may be one figure too many.
- `Taken: nothing` is printed for a lost raid whose storage protected all
  of the stockpile. It is true and reads oddly.
- A fleet docked at home when a raid of one Light Fighter arrives is
  offered the way out with the engine's quote at SAFE. That is right, and
  it puts a panel about leaving on a tab where nothing needs leaving.
- Whether the Defence tab should offer a button to fly a docked fleet out
  during the warning. It says in words that such a fleet is out of the
  fight, and the jump is made elsewhere.

## 2026-10-14 (raids fought as battles: what the agent decided)

Slice 4 of `docs/design/ogame.md`, sections 7.6 and 12, stage 1: everything
that is not hand-written Dart. The owner chose the study's defaults. What
follows is what the study left open or what was done otherwise than it
says. None of it was put to the owner. Each line ends with what undoes it.
`SAVE_FORMAT` is 11 and `ECONOMY_VERSION` 12; nothing is migrated.

**Where the work departs from the study.**

- *The raiders are not a group in the sector.* The study has the raid
  "stand at the homeworld as a group of OGame ships". It stands there as a
  `Siege` inside the fight's `Combat`, with its ships, and not as an
  `NpcFleet`: no sector view lists it, no threat rating, odds reply,
  standing order or pursuit of anybody knows it, and it fights nobody but
  the empire it came for. A group at a homeworld would have had to be
  taught to every reader of `npc_fleets` (home ground holds none), and a
  second empire's fleet passing through would have been in the fight.
  Undo: an `NpcFleet` at the dock listed in `Combat.npc_fleet_ids`, and a
  rule for each of those readers.
- *No wreckage at an outpost.* "The same fight", and at home a raid
  destroyed to its last ship leaves three tenths of its cost. At an outpost
  that would be 5,000 (ring 9) to 53,000 (ring 20) resources a raid, days of
  an extractor's yield, for a platform that costs nothing to keep. No raid
  on an outpost left wreckage before, and none does. Undo: the
  `siege.dock == Dock::Home` in `GameEngine::end_siege`.
- *The outpost's platform fights at the catalogue's figures*, not with its
  owner's research, which the study gives to "defences and satellites".
  A platform is the same for every empire, as its level's table says, and
  the forecast of a site is then the same for whoever surveys it. Undo:
  `GameEngine::battery_arms`.
- *A quiet round ends a raid.* The study's second ending is "six rounds, a
  defender still stands". A round that takes no hull from anybody ends it
  the same way at once, as `STANDOFF_ROUNDS` ends any fight: a dome that
  nothing can scratch against raiders it cannot hurt would otherwise be
  fought for six ticks to the same end. Undo: the `quiet_rounds` half of
  the condition in `siege_round`.
- *Gone with the old resolution*: `RAID_LOST_SHIP_FRACTION`,
  `RAID_LOST_STRUCTURE_FRACTION`, `RAID_STRUCTURE_DAMAGE`,
  `RAID_STRUCTURE_RESTORE`, `RAID_RESTORE_TICKS`, the delayed return
  (`Defences.restoring`, `DefenceState.restoring`, its pass and its alert),
  the fog of a tenth on the defence, and the skim of holds at the dock:
  a lost raid leaves no ship of the owner there, and what the destroyed
  ships carried went into the stockpile or the store first, as far as it
  had room. The quarter of the ships at a lost outpost raid is gone with
  it. Undo: git, at `cea4528`.

**What the study left to the builder.**

- *What a raid flies* (`scaling::RAID_WINGS`, the study's knob table):
  Light Fighters; two fifths of its power in Cruisers once 20 Rocket
  Launchers stand; three tenths in Bombers once 20 Light Lasers, Heavy
  Lasers and Ion Cannons stand between them. A wing flies when its share
  buys a whole ship. The power is `raid_power_base`'s and is not moved by
  the table; what the whole ships do not add up to is a multiplier on
  their figures, as a pirate group's is. An outpost's raid flies the one
  class the groups of its ring fly (`outpost_raid_composition`). Undo: the
  table.
- *The composition is fixed when the raid is announced*, from the defences
  standing then, with its power. What is built during the warning changes
  the odds and not the raiders. Undo: compose in `begin_siege`.
- *The forecast with no raid announced* is a raid at the middle of the
  roll (0.975 of the base), composed for the defences standing now.
- *The label of a forecast* (`RaidLabel`, `odds::Outlook::raid`), over the
  fights the server plays: `lost` when every one was lost, `at_risk` when
  any was, `repelled` when the raiders were destroyed in every one, `held`
  otherwise. A fight in which both sides fell counts as destroyed. Undo:
  `Outlook::raid`.
- *`RaidIncoming.threat`*, a word older clients read, is now from the
  label: `light` is repelled, `moderate` held, `heavy` at risk or lost.
- *Who is in the fight.* Every fleet of the raided empire standing at the
  dock, one charging a jump out included (it is in it as a fleet that is
  leaving, slice 3's rule). A fleet of another empire standing there is
  not, and is not fired on. Undo: `enlist_garrison`.
- *A raid that meets nothing* (no ship, no satellite, no defence; at an
  outpost, breached with no ship) is lost on the spot: no fight, no round,
  no `CombatEnded`, and `RaidResolved.outcome` is absent.
- *Both sides destroyed in one round*: repelled, and the wreckage is left.
- *An outlasted raid leaves no wreckage*, whatever it lost: the wreckage
  is of a raid destroyed to its last ship, by the study's table.
- *The last fleet jumps out*: with defences or satellites standing the
  fight goes on without it; with none it is over and lost (`fled`).
- *During the fight.* A fleet that arrives at the dock docks as any
  arrival (unloads, refuels, is refitted, is mended at home) and stands in
  the round of that tick. A ship the yard finishes is a fleet of its own,
  since the fleets docked there are fighting, and stands in the next
  round; a defence or a Solar Satellite stands in the next round. Research
  finished: the defences and satellites fight with it from the next round
  (the study: "as it stands that tick"), a fleet in the fight after it
  (the home dock does not tend a fleet in a fight). `load` and `unload`
  are refused for a fleet in the fight. Undo, each: `enlist_garrison`,
  `reinforce_battery`, `battery_arms`, `tend_at_home`.
- *Flight from a dock needs a free slot.* A fleet standing at home or at
  the outpost is not deployed; one that flees a raid is, so its flight is
  refused at the deployed-fleet limit, as its leaving before the raid is.
  Undo: the limit check in `flight_order`.
- *The rebuild's dice* (`rebuild_defences`): of a kind of which fewer than
  ten fell, each comes back on its own 70 in a hundred; of ten or more, a
  share of 60 to 80 percent is rolled for the kind and cut off to a whole
  number (`DEFENCE_REBUILD_EACH_BELOW`, OGame's own rule beside the two
  constants the study names). Seeded from the tick and the owner. A dome
  is not rebuilt beside one ordered since it fell. Undo: the function.
- *One raid at a time at a dock*: no raid is rolled for a dock while one
  is being fought there, and an outpost under raid cannot be abandoned.
- *The alert of an arrival is a warning.* It says what the raid meets and
  for how many rounds. A fleet destroyed in the fight has the critical
  alert of any lost fleet; the ending itself is `RaidResolved` and no
  alert.
- *The sums stay on the wire and decide nothing*: `home_defence_power`
  with its three parts, `next_raid_estimate_power`, the outpost's
  `platform_power`, `ships_power` and `defence_power`, and
  `RaidResolved.raid_power` and `defense_power`. A client that compares
  two of them is wrong; `raid.odds` is what replaces every such
  comparison. Removing them is a wire change for the slice that moves the
  web client. Undo: nothing to undo; remove them when no client reads
  them.
- *The bots* (not a rule of the game, but it moves every world of bots).
  The home planner orders, of the six armed defences it may build, the one
  with the most power for its cost divided by what the forecast raid's
  rapid fire does to it, one Small and one Large Shield Dome, and the
  research the better defences need; it keeps the defence at its style's
  margin over the raid and tops it up while the forecast is short of
  `repelled` with 0.85 kept. A pilot steps a fleet aside to the next
  sector when a raid on its way is `at_risk` or `lost`, and waits beside
  home while one threatens or is fought. Bots still do nothing with
  outposts. Undo: `sim/src/bot/home.rs` (`ARSENAL`, `answer`,
  `defences_wanted`) and `field.rs` (`raid_threatens`, `step_aside`).

**The knobs, as left** (measured in `docs/journal.md`): `RAID_ROUNDS` 6,
the study's; the raid's size unchanged (`RAID_POWER_PER_POINT` 9); the
table above. Four rounds and a raid of twice the size were played and are
in the journal with what they did.

## 2026-10-14 (the web client on flight, pursuit and pirates that mend: what the agent decided)

Slice 3 of `docs/design/ogame.md`, section 12, stage 2: the hand-written Dart
of `clients/web` on the engine stage 1 built, after master (slice 2, engine
and web client) was merged into the branch. The windshield as a picture of
class marks with flight and pursuit drawn on it is slice 5 and was not
built: what is here is controls, panels and words. What follows is what the
agent that built it chose. None of it was put to the owner. Each line ends
with what undoes it. `SAVE_FORMAT` and `ECONOMY_VERSION` did not move, and
no wire type changed: every figure a view shows was already in the state
stage 1 sends.

**What Rust gained.** Nothing on the wire. For the scripted world of the
widget tests, which types no number of the game,
`sim/tests/economy_numbers.rs` names `CHASE_HEXES` and `NPC_HEAL_STEPS`,
gives a ring's row in the `rings` table its `npc_jump_ticks`, and renders a
`group_healing` table (ring, pace, the ticks between two steps of a group's
mending). `sim.py` reads none of them. Undo: those four lines; the scripted
world would then have to type the leash and the mending clock.

**The way out of a fight.**

- *A panel, the gates and the keys, and no second press.* A fleet in a
  fight gets a Leave the fight panel from `flight`: the odds as the fight
  stands, the price, who would follow, and a button a lane. Every gate card
  says `FLEE n RND` in place of its intel line, and a gate key in a fight is
  flight. Fire asks for a second F on deadly odds; leaving asks for nothing,
  because the order can be called off and a fleet that needs to leave has
  no second to spare. Undo: the `quote` branch of
  `_WindshieldViewState.jump` for a confirmation; `FlightPanel` and
  `GateCard.flight` in `lib/views/windshield/parts.dart` for the rest.
- *Every lane is asked about while the fight lasts.* The fleet's own quote
  says who would follow it out; only a `preview_move` says whether a group
  follows into a given sector, which it does not into home ground, the hub
  or an outpost's sector. So the windshield asks the server for each lane of
  a fleet in a fight, as it does for a gate under the mouse, at most once
  every three ticks a lane, and prints "NOTHING FOLLOWS TO ..." for a lane
  whose quote has no `chase` while the fleet's has one. Undo: the
  `requestPreview` in the gate loop of `WindshieldView.build`.
- *Stop is offered where the fleet is leaving, with its price.* The Leaving
  the fight panel has a Stop button and the sentence that the fleet fights
  on and the fuel is forfeit; the dock's Stop tile carries the same sentence
  as its hint while the fleet is leaving (`DockAction.note`). Undo: the
  note, and `stopFlightRule` in `lib/console/battle.dart`.
- *The end of a fight is told where the fleet lands.* The report of a fight
  used to be the one in the fleet's sector, and a fleet that has fled stands
  somewhere else. The mapper now remembers where each fleet last fought
  (`FleetState.foughtAt` of the client's model) and `GameState.battleOf`
  gives a fleet the report of the fight it left once that has ended. Undo:
  `battleOf` back to `battleAt(f.sector)` in the windshield.
- *FLEEING is the client's word.* On the wire a fleeing fleet is `Moving`.
  The fleet rail, the map's chips and the Overview's cards print
  `statusLabel`, which is the status's own word but for a fleet with
  `fleeing`. Undo: `statusLabel` and `underFire` in `lib/models/fleet.dart`.

**Pursuit.**

- *Pinned.* The Pursuit panel stands over the windshield's other panels and
  does not scroll with them: the first version stood first in the scrolled
  column, and a column scrolled down to a button hid it. The screen's edges
  pulse as they do in a fight, the fleet's card turns ember and says when
  the group lands, a banner is shown when the pursuit begins, and on a
  phone it is a line over the fleets. Undo: `panels` in
  `WindshieldView.build`, which would put `PursuitPanel` back in the column.
- *Said on every screen that lists the fleet.* The Overview's card has a
  PURSUED tag and the sentence, its alerts have "F3 IS PURSUED" while the
  state says so (the alert the event raised is not shown beside it), the
  map's chip and sector panel say it, and the map draws the lane the group
  is jumping down and a ring round the fleet; a fleet's own flight is a lane
  too. A pursuit begun is one of the things the console tells a player whose
  tab is in the background. Undo: the three views; `_drawChases` in
  `lib/scenes/map/map_painter.dart`; the `PursuitEvent` case in
  `lib/console/attention.dart`.
- *Whether it catches is the state's word, worded twice.* `catches` means
  one thing for a fleet that is charging a jump and another for one that is
  not, and the panel says which: "the jump under way is gone before it
  lands" or "a jump ordered now is gone before it lands; a fleet that stays
  is caught". The client reads the fleet's status to pick the sentence and
  works out nothing. Undo: `pursuitVerdict`.

**A group.**

- *One brief for every screen.* `GroupBrief` prints, under a group's ships:
  when it is whole again, how far it follows and how long its jump takes
  with the selected fleet's own beside it, and its errand. The heading says
  "3 OF 5 SHIPS" when it has losses. The contacts, the map's sector panel
  and the route's hold panel use it. No screen says which of the two jumps
  is the faster: `flight.chase` says what a chase would do. Undo:
  `GroupBrief` and `groupHeading` in `lib/views/battle_view.dart`.
- *`whole_at` is a time from now, and old news is said to be old.* In
  sensor range: "WHOLE AGAIN IN 4:10". On a sector last seen earlier: the
  same "AS LAST SEEN", or "WHOLE AGAIN BY NOW, IF NOTHING DISTURBED IT" once
  the tick is past. A sentence beside it says that it mends at rest in its
  own sector and that a fight or a pursuit stops the clock. Undo:
  `GroupBrief._mending`.

**Standing orders.**

- *The seventh setting has no gauge of its own.* "Leave a fight below"
  steps through 0.1, 0.3, 0.5, 0.7, 0.9, 1.1, 1.3, 1.5, 2, 3 and 10 (any
  ratio the server takes can be typed, `policy ... disengage=1.2`). It
  stands under "Least odds to fight" and shares that stepper's gauge and one
  note for the two. The panel does not clamp it to the engage ratio: the
  server does, and the settings in force, now on two lines, show what it
  took. Undo: `disengageSteps` and the stepper in `_OrdersPanelState`.
- *The form no longer leaves room for the guide strip.* With seven settings
  the Set button of a fleet under orders still shows without scrolling on a
  900 px window, but the 66 px the widget test kept free under it for the
  onboarding guide strip are gone, and the test now asks for the first
  only. Undo: nothing short of a shorter form; the five doctrines with two
  lines each are the largest part of it.
- A doctrine that leaves a fight reports `flee`; the panel's heading words
  it "LEFT A FIGHT" and the log marks the line as the player's to see.

**Words.** `fled` is "FLED UNDER FIRE". `withdrawn`, which now means the
emergency jump only, is "RECALLED" (it was "FLEETS WITHDREW"). The sentence
for a stand-off says both sides stay as they are, and no sentence says a
group is whole again after a fight. An ended fight in which nobody lost a
ship says so in place of "NOTHING YET". Undo: `outcomeTitle` and
`outcomeSentence`.

**The scripted world of the widget tests** flies, follows and mends by the
engine's rules, with the engine's sentences for a second order and for
Stop, and differs in one thing its header states: only a jump out of a
fight is charged before it is made; every other jump there is made at once
with a cooldown after it, as it was. So in it a fleet that jumps on the
moment it lands is gone when its pursuer lands, whatever the two speeds,
and one that stays is caught. Its groups jump in the ticks the ring's own
class takes, and its odds are power against power, as they were. Undo:
`_stepGroups` and `_landOutOfFight` in `test/support/demo_world.dart`.

**The end-to-end check** sends the two Light Fighters it splits off to
scout, into the group and out again, and then the six along the same way.
The six wait a jump away until a scan shows the group back in its sector
and whole, because at a blitz a group the two hurt mends in seconds and
the check counts what the six destroy. Of two groups it would take
otherwise, it takes the one that outweighs the two. Undo: `flight` in
`tool/solo_e2e.dart` and the two lines of the hunt's order.

**Unsure of.**

- Whether leaving should ask for a second press when the odds are SAFE, the
  way Fire does when they are DEADLY.
- The flee control stands above the round panel and pushes it down. In a
  long fight a player may want the round first.
- "RECALLED" for `withdrawn` names the order, not what the fleet did; the
  engine's alert of the recall says what it cost.
- A phone's map keeps the route in its sheet and not the sector, so a
  wounded group's panel cannot be read on the map at 390 px. That was so
  for a whole group too.
- The windshield's scope still spreads the fleet toward the gate while it
  charges and does nothing else for a flight or a pursuer: slice 5.

## 2026-10-14 (flight, pursuit and pirates that mend: what the agent decided)

Slice 3 of `docs/design/ogame.md`, section 12, stage 1: everything that is
not hand-written Dart. The owner chose the study's defaults, questions 4, 5
and 11 among them, and said in his own words that a fleet may run to the
next hex, that more dangerous enemies chase a hex or two, that slower ships
pay for it, that fleeing has a price and the damage stays until home, and
that pirates heal slowly, "over a few hours maybe? or less, maybe an hour".
What follows is what the study left open or what was done otherwise than it
says. None of it was put to the owner. Each line ends with what undoes it.
`SAVE_FORMAT` is 10 and `ECONOMY_VERSION` 11; nothing is migrated.

**Where the work departs from the study.**

- *One disengage rule for every doctrine.* The study (7.4) has the
  doctrines that pick no fight, mining and salvage, "flee any fight they are
  caught in". They do pick fights: with no lane free of one they take a
  fight their fleet rates at its engage ratio (`Reach::path_worth`), and a
  doctrine that entered a fight on purpose and left it at once would pay
  parting rounds for nothing. So every doctrine leaves a fight when its odds
  in it are below `disengage_ratio_x10`, and none otherwise. A cargo wing
  caught by something it cannot hurt rates DEADLY and leaves. Undo: a match
  on the preset at the head of `GameEngine::policy_disengage`.
- *Groups do not wander.* The study speaks of wandering groups
  (`process_npc_behavior` moved a Patrol or an Aggressive group to a random
  neighbour every 15 ticks). That code only ever ran on a group that was not
  in a fight, and until this slice no group outlived its fight, so it moved
  nothing in play. With groups that keep their losses it would have come
  alive on every wounded patrol, which would then have left the sector it
  is to mend in. It is removed, with `NPC_PATROL_INTERVAL` and
  `NpcFleet.patrol_timer`: nothing moves a group but a pursuit and the way
  back. Undo: the loop is in git at `8e21928`; it would need a rule for
  where a group that has wandered mends.
- *The leash counts jumps, not hexes.* The study calls `CHASE_HEXES` "the
  most hexes the group will stand from its own sector". A fleet as fast as
  its pursuer could then lead a pack round its own lair for ever. The leash
  is the jumps a group has made after a fleet since it was last home
  (`NpcFleet.strayed`, `leash`): a patrol makes one, a pack two, and it is
  home again before it makes another. Undo: `NpcFleet::leash` on
  `Hex::distance` to `home_sector`.
- *A fleet that any fight catches while it charges a jump keeps the jump.*
  The study gives this for the pursuer that lands on a fleeing fleet. It is
  made the rule for every fight: the fleet is in the fight as one that is
  leaving, fired on and silent until it lands. The old rule (the jump
  abandoned, its fuel forfeit) is gone with its alert. Undo:
  `GameEngine::engage_fleet`.
- *A fleet that is charging a jump takes no second `move`.* It used to take
  one in the last tick of its charge, paying again and starting over; a
  fleeing fleet could have used that to change its mind for the price of a
  jump. `stop` is the order that calls a jump off. Undo: the first refusal
  of `GameEngine::handle_move`.
- *`mends_in` and the pursuer's arrival are ticks, not countdowns.* The
  study (10.1) names `mends_in` and "ticks until it arrives" on a group in
  sight. A sector is remembered as it was seen, and a countdown in a
  remembered sector is wrong the tick after. A hostile carries `whole_at`
  and its errand `arrives_at`; the fleet's own `pursued_by` carries
  `arrives_in`, since a fleet's row is never remembered. Undo: the two
  fields of `NpcFleetInfo` and `GroupErrand`.

**What the study left to the builder.**

- *No fleet status for flight.* A fleeing fleet is `Moving` and still in its
  fight's list; a round stands it as a silent troop (`combat::Troop.silent`)
  and `process_movement` takes it out of the fight when it lands. So the
  stored world holds no second note of who is leaving, and `FleetStatus` on
  the wire is as it was; `FleetState.fleeing` says it. Undo: a status of its
  own, and every place that asks for `Moving`.
- *When the group fires on a fleet it catches.* `process_pursuit` runs after
  `process_movement` and before `process_combat`: a fleet that lands is gone
  before the group lands, and a group that lands fires the same tick. That
  order is what makes the study's arithmetic true (a fleet as fast is never
  caught, a slower one is fired on for the difference). Undo: the order of
  the three calls in `GameEngine::tick`, and the two speeds part by a tick.
- *Whom a group follows.* The strongest of the fleets that jumped out of
  the fight which it can reach (next door, not a refuge), and that one only
  if the group's power, as it stands when the fight breaks off, is at least
  `CHASE_MIN_RATIO` times the fleet's. It is not weighed again at the next
  hex. A fight that ends with the last fleet destroyed or recalled still
  sends the group after one that had jumped out earlier. A stand-off sends
  it after nobody. Undo: `GameEngine::begin_pursuit`.
- *A group on its way fights what it lands on.* Its quarry, or any other
  fleet standing there, another empire's included: the study's "a second
  fleet waiting in that sector joins it" and what a wandering group did.
  Not a convoy, and not in a refuge. Undo: `met` in `process_pursuit`.
- *A group that is whole, at rest and at home is taken out of the world.*
  The lair is its template again, as it was before the fight; an ambush
  party at a derelict is gone, as it was when a fight broke off before this
  slice. So only wounded groups are stored. Undo: the removal at the end of
  `GameEngine::settle_group`.
- *The mending clock.* `rested` counts ticks at rest in the group's own
  sector, the tick the fight ended being the first; at
  `scaling::npc_heal_step_ticks` a step is taken and it starts again. A
  fight or a pursuit sets it to nothing, which is the study's "starts again
  from the last step it reached". Of a group's ships `ceil(muster / 10)`
  return at a step, at least one, and a damaged ship regains
  `ceil(hull / 10)`. Undo: `Ships::heal`, `scaling::npc_heal_ships` and
  `npc_heal_hull`.
- *A kill is paid by lair.* A group destroyed in a fight another group
  survives is paid when the fight ends, wreckage in the sector of the fight
  and score once for its lair; the lair of a group destroyed away from home
  is the one that is cleared. Before, only a fight in which every group
  fell paid anything. Undo: `kills` in `GameEngine::end_combat`.
- *What a fleet under orders does about a pursuer.* It runs for home before
  anything else (`policy_pursued`), and a group that sets out after it wakes
  its orders at once. The study says only that it flees toward home. Undo:
  the first branch of `process_policies`.
- *When a doctrine reads the odds.* At its first turn in the fight, which is
  the tick after the fight began, and every `POLICY_EVAL_INTERVAL` ticks
  after. The odds are of the fleets that are firing against every group in
  the fight, as they stand (`GameEngine::fight_ratio`). A fleet that cannot
  leave says why, once a minute. Undo: `engage_fleet` waking the orders.
- *The odds of a remembered sector.* A group seen wounded is rated as it was
  seen until its `whole_at`, and whole from that tick; one seen passing
  through on an errand is not counted where it was seen. Without the first a
  chart would send a fleet back into a group long since mended. Undo:
  `remembered_opposing`.
- *An outpost is not founded on a lair whose group is away.* Its sector is
  empty while it follows a fleet, and it comes back. The refusal names
  where it is. Undo: the second group check of `GameEngine::outpost_site`.
- *The pilots.* A bot flying by hand leaves a fight whose odds as it stands
  are under RISKY (`bot::field::LEAVE_ODDS`), where the doctrines' default
  leaves too, and runs for home when pursued. And a wing with no warship in
  it picks no fight at any odds (`bot::field::armed`). The study (8.7) has
  the bots flee "through the disengage ratio of the policies they already
  set", which covers the check-in styles and not the hands-on ones. The
  second rule is this slice's own and was measured: see the journal. Undo:
  `Admiral::fight`, and `armed` out of `beats` and `boardable`.
- *No event when a group does not follow.* `Pursuit` says began, continued
  and abandoned; a fight left by a fleet nobody follows says `fled` and the
  fleet's row has no `pursued_by`. Undo: an event from `begin_pursuit`.

**The knobs**, each at the study's starting value: `FLEE_MIN_ROUNDS` 1,
`FLEE_FUEL_MULTIPLIER` 1.0, `CHASE_HEXES` 0, 1, 2, 2 (convoy, patrol, pack,
swarm), `CHASE_MIN_RATIO` 1.0, the groups' drive (`NPC_DRIVE_FREE_RINGS` 8,
`NPC_DRIVE_RINGS_PER_LEVEL` 4), `NPC_HEAL_SHARE` 0.5, `NPC_HEAL_STEPS` 10,
`POLICY_DEFAULT_DISENGAGE_RATIO_X10` 9 (bounds 1 to 100, never above the
engage ratio). This slice's own: the pilots' `LEAVE_ODDS`, RISKY's edge.
`STANDOFF_ROUNDS` stays 1 and `JUMP_SPEED_REF` 50,000. None was tuned.

**What the healing rate means at each pace.** It is an economy timer, as
the study recommends, so it shortens with the pace: at ring 8 a group that
has to take all ten steps is whole in two hours at pace 1, twelve minutes
in a season and twenty seconds in a blitz (a step is a whole number of
ticks, two there; at the test pace one, so ten ticks). A group that lost
a third of its ships or of a hull takes four steps and not ten. A fleet that goes home to be mended and comes
back finds the group still hurt in a season (played over a real server: the
journal) and whole in a blitz, where a visit and the next have to be a few
seconds apart. The other class the study names (attention) would give a
blitz about a minute. That is the owner's knob.

**Bands and guards that moved, and why.** For each: the test is still worth
having, it agrees with the design, and the code was not wrong.

- `scenarios_world.rs`, the jump a fight catches: it asserted the old rule
  (the jump abandoned, the fuel forfeit, the fleet in the fight). It asserts
  the new one: the jump goes on, the fleet stands one round without firing,
  is shown as leaving, takes no second jump, and its owner is told once.
- `scenarios_world.rs`, the stand-off: its two fights still end as they
  did (nothing in them was hurt, so nothing told the reset from its
  absence). It gained a third: Probes hurt in a first round and then left
  by a fleet that fires nothing, a round nobody is hurt in, after which the
  group is as the fight had left it and not whole.
- `exchange.rs`, the hub: it proved "no group enters the hub" with forty
  ticks of wandering patrols beside it. With wandering gone it proves it
  with a pack that would follow a fleet two hexes anywhere else and does
  not follow it into the hub, by the preview and by twenty ticks of play.
- `fleet_logistics.rs`, the boarding a fight catches: the patrol that walked
  into it was a wandering one found by searching the patrol dice. It is a
  group landing in its own sector on its way back.
- `hosts_agree.rs`: the commander jumps out of the fight it flew into, and
  both hosts must send the same parting round, `fled`, the flight quote and
  the fleet's `fleeing`; it used to require a fight to its end in which
  every round had shots from the fleets.
- `smoke.rs` and `single_player.rs` name format 9 as the one refused, and a
  later format is 11; two tests that compare a fleet's standing orders
  whole name the seventh setting.
- `chart_golden.txt` is recorded again, as in every slice.

## 2026-10-13 (the web client on the new fleet and fight: what the agent decided)

Slice 2 of `docs/design/ogame.md`, section 12, stage 2: the hand-written Dart
of `clients/web` on the fleet and the round stage 1 built. Its part of the
study's table is "plain fleet lists and a text round"; the windshield in
class marks and the battle view are slice 5 and were not built. What follows
is what the agent that built it chose. None of it was put to the owner. Each
line ends with what undoes it. `SAVE_FORMAT` and `ECONOMY_VERSION` did not
move: nothing here changes what a stored world holds or plays.

**What the state now says, for the views.**

- `PlayerState.ship_count`: every ship of every fleet the player has. The
  Overview's Ships figure prints it; it used to add the fleets' counts to a
  count it parsed out of a string. Undo: the field in
  `shared/src/protocol.rs` and its line in `sim/src/views.rs`; the Overview
  would then have to sum `FleetState.ship_count` itself.
- `BattleSide.standing`: the units a side has left after a round, in all its
  groups. The round panel and a phone's line for a fight print it. Undo: the
  field and its line in `battle_side` (`sim/src/engine.rs`).
- A tick update's `battles` no longer carry `last_round`; a full state's do.
  Stage 1 left this open: a fighting tick carried the round twice, as the
  `BattleRound` event and inside `battles`. The client needs `last_round`
  only to seed itself when it arrives in the middle of a fight, which is a
  full state. `Seat`, the in-process client the bots play through, is handed
  no events, so it reads its fights with `views::battles_of`, as a full
  state lists them; `the_view_a_bot_folds_from_tick_updates_is_the_full_state`
  holds it to that. Undo: `battles(engine, player, true)` in `tick_update`.
- `REPAIR_COST_SHARE_HOME` and `REPAIR_COST_SHARE_OUTPOST` are in
  `docs/design/economy/numbers.json` (the `named!` list of
  `sim/tests/economy_numbers.rs`). The scripted world of the widget tests
  quotes a dock's repair with them and types neither. `sim.py` does not read
  them. Undo: the two names in the list.
- No column and no constant was added to `shared/tests/wire/mod.rs`.

**The scope.**

- *A mark, not a ship.* A side is drawn with at most sixteen marks
  (`WindshieldSim.marksASide`). One mark stands for 1, 2, 5, 10, 20, 50 and
  so on ships, the smallest of those steps that fits both sides, and the
  same for both, so the scope shows which is the larger; a class with a ship
  in it always has a mark, which sixteen has room for. The scope prints
  "EACH MARK: UP TO n SHIPS" when n is more than one, and the list beside it
  has the counts. Undo: `markScale` in `lib/console/battle.dart` and
  `marksASide`.
- *Rows that fit.* The marks of a side stand in rows, the fleet below the
  centre and the hostiles above it, drawn at the largest of seven sizes whose
  rows fit the ground (`WindshieldSim._rows`). The wedge of eight slots is
  gone: a ninth ship used to run off the scope, and twelve classes of
  different sizes overlapped in it. Undo: `_rows` and the two `_form`
  functions.
- *Fire and losses come from the round.* A `BattleRound` in the fleet's
  sector draws strokes of fire for the six heaviest volleys of each side,
  one to three strokes a volley by its shots, between marks of the two
  classes; a stroke that reached no hull lights the target's shield. The
  marks a round's losses leave nothing for explode once the fire has landed,
  and the loss is printed over the class ("-12"). Between rounds the marks
  are set from the state without a sound. The client keeps no hull, no
  shield and no count of its own; the bars over each ship and the shield
  gauge it kept by subtraction are gone, and the fleet status prints what
  the fleet fires and the shield it raises in a round instead. Undo: none
  that keeps the rule; the gauge had no state behind it.

**The round as text.**

- *Where it stands.* At desktop width the Battle panel opens at the top of
  the windshield's right column, over the fleet status, while the state
  lists a fight in the fleet's sector, and stays for 90 ticks after the
  fight's end (`battleReportTicks`) or until its Dismiss is pressed. On a
  phone a line over the fleet rail
  says the round and what stands and fell on each side, and opens the panel
  in the status sheet. Undo: the two places in `windshield_view.dart`.
- *What it shows first.* Standing is a table, a row a class: left, lost this
  round, damaged, at risk. A side's fire is its totals and then its volleys
  ordered by units destroyed, then hull damage, then shots; three are shown
  (`volleysShown`) and a button opens the rest. A round of twelve classes at
  twelve is 144 volleys a side and reads as two tables and six volleys.
  Undo: `volleysByWeight` and `volleysShown`.
- *The log.* A fight keeps one line of the log for its rounds, the last
  round's, and a line for its end; a line a round would fill the log of a
  long fight. The shell now tells a new line by the lines it has seen, not
  by the newest one, since a replaced line is no longer where it was. Undo:
  `_roundLines` in `state_mapper.dart`.

**Lists, the picker and the words.**

- *One list everywhere.* `ShipRows` is the list of the fleet status, the
  contacts, the map's sector panel and its hold panel, the outpost and the
  docked ships: a class, its count, its hull against full hull, and under a
  class with damage a bar and "n DAMAGED  n AT RISK". Damaged is amber
  (`C.a300`) and at risk ember, here and in the round panel; a list with a
  ship at risk says once what that means, with the engine's 70 percent.
  Undo: `lib/views/battle_view.dart`.
- *Figures.* A count or a hull figure in these rows is exact up to 99,999
  and three figures with K, M or G above ("164,160", "165K", "1.28M"), so a
  row is as wide for five thousand ships as for five. Undo: `shortCount`.
- *The picker.* A row a class with - and +, the count between them and All;
  the step of - and + is 1, 10 or 100, chosen once above the rows; two
  buttons choose whole ships first or the damaged first. Split is off while
  nothing is picked or every ship is. A merge the engine would refuse (more
  than `MAX_FLEET_SHIPS` together) is sent and answered. Undo:
  `FleetManagePanel` in `lib/views/windshield/parts.dart`.
- *The console.* `split <class> [count] [<class> [count] ...] [damaged]`; a
  class with no count is one ship. The parser no longer checks the counts
  against the fleet: the engine does and says which class is at fault.
  `status` prints the fleet by class, its hull, what it fights with and each
  dock's answer to its damage.
- *The hull dial* rounds down, so it reads 100 only for a whole fleet.

**Removed.**

- `SectorInfo`, the strings the mapper made of the focused sector, which no
  view read; and `GameState.docked`, the docked ships as a string, which
  only the Overview parsed. Both restated the state.

**The scripted world of the widget tests.**

- Its fleets are the engine's: a count a class and the hull of each damaged
  ship. Its fight sends `BattleRound`, `CombatEnded` and `battles` shaped as
  the engine's, but is played without dice: every ship fires once at the
  other side's classes in proportion to their numbers, and as often again as
  its rapid fire says; a shot under a hundredth of a shield bounces, each
  other one meets a full shield, and a ship hit while at risk explodes.
  Pirates fight on their class's base figures. It is not OGame's round and
  no test reads it as one.
- A ship the yard delivers joins the fleet docked at home, as the engine's
  does; it used to stand in a list of its own. `move` is refused in a fight
  in the engine's sentence. A hulk recovered from a derelict arrives
  damaged, by a roll of the script's own.

**Unsure of.**

- Whether sixteen marks and one scale for both sides is the picture the
  owner wants until slice 5: a fleet of 512 against a swarm of 3,230 is
  three marks against eight.
- Whether a fight's report should leave by itself after a minute and a half
  or stay until it is dismissed.
- The outpost's quote in the fleet status is shown wherever the fleet is,
  not only at the outpost.

## 2026-10-13 (the fleet and the fight are OGame's: what the agent decided)

Slice 2 of `docs/design/ogame.md`, section 12, stage 1: everything that is
not hand-written Dart. The owner chose the study's defaults, questions 6 and
7 among them. What follows is what the study left open or what was done
otherwise than it says. None of it was put to the owner. Each line ends with
what undoes it. `SAVE_FORMAT` is 9 and `ECONOMY_VERSION` 10; nothing is
migrated.

**Where the work departs from the study.**

- *A stand-off makes the group whole again.* The study (7.2) has both sides
  stay "with what they had". In this slice a fight that ends without the
  group destroyed ends as every such fight ends today: the group is gone
  from the sector's live fleets and its template stands again, whole
  (`GameEngine::end_combat`). The fleets stay where they are and nothing
  starts the fight again but an arrival or an `attack`, as the study says.
  It differs from the study only where a group lost ships before the round
  nobody was hurt in. Pirates that keep their losses are slice 3 (7.5), and
  that is what undoes it.
- *The odds are held under the band their win record does not earn.* The
  study (7.7) puts the share of worth kept on the ratio scale and nothing
  more. Measured on the new round that was not enough for small fights: two
  ships against two ended anywhere from a clean win to a lost fleet and
  averaged into FAVOURABLE while winning 78 fights in 100. So `Outlook`
  also carries how often the fleet won and how often it was destroyed, and
  `scaling::outcome_ratio` holds the ratio just under FAVOURABLE when a
  fight played was not won, just under EVEN when fewer than four in five
  were (`EVEN_WINS` 0.8), and under RISKY when the fleet was destroyed in
  every one. Undo: the `most` of `outcome_ratio` and its two parameters.
- *Small fights are played more often.* `ODDS_RUNS` 8 is the least; two
  sides of fewer than `ODDS_UNITS` are played `8 x 200 / units` times, at
  most `ODDS_RUNS_MOST` 64, which is the same number of shots. Eight runs
  of a two-ship fight could not tell a win in four from a win in five.
  Undo: `ODDS_RUNS_MOST = ODDS_RUNS`.
- *A class that is there stays there when the odds scale two sides down.*
  One Deathstar among 4,000 fighters is still one Deathstar among 200. The
  study does not say what scaling does to a class under half a unit. It
  overstates a side that leans on a few heavy ships among very many light
  ones. Undo: the `.max(1)` of `odds::scaled`.
- *The outpost's dock mends whole ships only.* The study has a store that
  cannot pay mend "what it can pay for, the most damaged ships first". It
  is read as ships, not as hull: a ship the store does not cover is left as
  it is, and so is every ship after it. A ship half mended would be one
  more number to price and to explain. Undo: `engine::mending`.
- *A fleet under standing orders is not mended at the outpost.* The
  outpost is not used by standing orders anywhere else, and a doctrine that
  stopped there to spend the store was not asked for. Undo: the third
  refusal of `GameEngine::dock_mending`.

**What the study left to the builder.**

- *The dice.* SplitMix64 of its own in `combat.rs`, seeded from the world
  seed, the tick, the sector and the round, and `below(n)` as the high half
  of a 64-bit product. Nothing of it is stored. The engine's other dice
  are not touched, so no draw outside a fight moved. Undo: `combat::Dice`.
- *The shield breaks when what is left of it is less than n hundredths*,
  as 7.1 words it, and a unit whose attack is 0 fires no shot (the Probe
  and the Solar Satellite), so it is in no volley and wastes no dice.
- *Pirates fight at their ring's multiplier on all three figures.* A group
  carries `Arms` in thousandths (`Arms::of_grade`), and
  `scaling::unit_figures` cuts every figure off to a whole number, as the
  round needs. `power` everywhere is now computed from those whole figures:
  a Solar Satellite at Weapons 1 is 21.1 and not 21.2, its attack of 1 being
  still 1. Undo: `UnitFigures::power` would have to take fractions again,
  and the round and the power would then disagree.
- *A battle's unit is a `ShipyardItem`*, a ship or a defence, although no
  defence fights yet: `BattleUnits.unit`, `Volley.from` and `at`, and
  `UnitCount.unit` are typed so, so that slice 4 changes no wire type.
- *Both sides destroyed in one round* (`BattleOutcome::BothDestroyed`)
  leaves the wreckage and pays the kill, since the group was destroyed;
  `player_victory` is false. Undo: `killed` in `end_combat`.
- *Refit.* A fleet standing at home is refitted the tick research
  completes, without an alert; a fleet arriving is told what it was
  refitted to and what it flew with. A ship built joins a docked fleet,
  which is at the current levels by that rule. A wreck recovered in the
  field takes the levels of the fleet that took it. The outpost's dock does
  not refit. Undo: `GameEngine::tend_at_home` and its call in
  `process_homeworlds`.
- *Repair takes no time and `REPAIR_TICKS` is an action cooldown.* At 0 it
  does nothing; at another value a mended fleet takes no order for so many
  ticks. The study names the constant and not what it delays.
- *The home dock mends every tick a fleet stands there*, not only on
  docking, so a ship recovered or a fleet merged at home is mended too.
- *What a fleet under orders does about damage.* `POLICY_HULL_READY` 0.85
  takes the place of `POLICY_SHIELDS_READY`: under it the fleet goes home
  before anything else, and waits at the dock until it is mended, which at
  the home dock is at once. The bots fly by the same figure
  (`bot::field::HULL_READY`). Flight under fire is slice 3.
- *`MAX_FLEET_SHIPS` 5,000* bounds a merge and the docked fleet new ships
  join; ships past it form another fleet. `NPC_MAX_SHIPS` stays 32.
- *The bots' bar is the number it was*, `SAFE_ODDS` 2.0, and means
  something else: a fight won every time it was played that keeps about
  five sixths of the fleet's worth, where it used to keep 97 percent of the
  hull. The doctrines' default `engage_ratio_x10` 15 is FAVOURABLE as
  before. Neither was tuned.

**What the wire says that the study did not list.**

- `battles` is in every `tick_update` as well as in the full state, so a
  client that folds updates holds the fights without asking again
  (`sim/tests/bots.rs` folds and compares). Undo: the field of
  `TickUpdate` and the line of `script::apply`.
- `ShipsLost`, for the ships a `recall` breaks up: they are lost outside
  any round. `ShipDestroyed` and `CombatRound` are gone.
- On a fleet: `ship_count`, `hull`, `hull_max`, `attack`, `shield` (sums a
  client would otherwise add up), `refit`, `refit_due`, and `repair`, what
  each dock would do now and why it would not. On a class row
  (`ShipGroupState`): `at_risk`, and `unit_attack`, `unit_shield` and
  `unit_hull` at the fleet's refit. On the homeworld `docked_ship_count`,
  on a hostile group `ship_count`, on the outpost `ships_power`. The
  generated columns gain `RatioLabel.outcome`, the sentence a label stands
  for, and the constants `MAX_FLEET_SHIPS`, `EXPLODE_BELOW_PCT` and
  `POLICY_HULL_READY`.
- A split names a class once with a count (`ShipCount`, whose key on the
  wire is `class`, as it was) and is refused with `InvalidTarget` and a
  sentence that names the class at fault.

**Measured, and what was decided from it.**

- A round of 5,000 a side costs 0.3 to 0.4 ms natively and 0.4 to 0.55 ms
  as WebAssembly in V8; 5,000 Deathstars against 5,000 Light Fighters, a
  million shots, 6.7 and 9.2 ms. Nothing of section 7.8's remedies is
  needed, and none was taken.
- The odds for a pair not seen before cost 0.14 to 0.46 ms natively. Played
  through `combat::round` they halved the speed of a blitz day of bots (427
  ticks a second against master's 850): every round stood both sides up
  again and built a report nobody read. `combat::fight` plays the same
  rounds on the same dice without the reports and the day runs at 786; the
  world is the same to the byte. There is still one model of the fight. Undo: `odds::outlook` back
  on `combat::round`.

**Bands and guards that moved, and why.** For each: the test is still worth
having, it agrees with the design, and the code was not wrong; the number
in the test was the old fight's.

- `scenarios_world.rs`, the labels: SAFE is won 57 times in 60 or more and
  keeps 0.84 of the hull or more; FAVOURABLE the same wins and 0.60 to
  0.93; EVEN 42 wins or more and 0.33 to 0.92; DEADLY 27 wins or fewer and
  at most 0.22. They were 98 percent of the hull for SAFE and so on down,
  which no fight keeps where a ship at 70 percent may explode. The thirteen
  pairings are new and none of them is RISKY, a label that is 10 to 51 wins
  of 60 in the grid of 492 the journal sums up: too wide for a band.
- `scenarios_world.rs`, the starting fleet: every lair of ring 5 won, 50 to
  60 of 60 at ring 7, 25 to 55 at ring 8, at most 3 at rings 9 and 10. It
  used to say every lair through ring 8. Two Light Fighters against two at
  0.88 of their figures is two fights in three. `NPC_POWER_BASE` was not
  changed for it: that is the owner's knob (question 8 of the study).
- `stored_world.rs`: the moment it waits for was a fight, a raid warning, a
  scan and a jump at once. Fights last two or three rounds now and the four
  never met; it asks for fights at six stops and for the other three
  together. Its pilots are given a yard so that raids come.
- `hosts_agree.rs`: the commander flies into the nearest group that shoots
  first, and both hosts must send the same rounds; six leaderboards for
  five; about 64 seconds for 45.
- `both_targets.rs`: a fourth world, a commander's fight; the blitz world
  must also hold a fleet of eight ships or more and damage at two
  checkpoints of ten. 95 seconds with the engine built, for 94.
- `server/src/network.rs`: three Solar Satellites are 63.3 of power, not
  63.6 (above).
- `smoke.rs` and `single_player.rs` name format 8 as the one refused.
- `fleet_logistics.rs`: the doctrine that waited for shields is gone with
  the shields, and its test is one of a mauled fleet going home; the
  prospector tuned to a ratio is found by halving, since which lair a ratio
  opens moved.
- `chart_golden.txt` is recorded again, as in every slice.

**Not built, as the slice says.** Leaving a fight under fire, pursuit,
pirates that heal, raids as battles, defences in a round. A caught fleet
fights to the death: in a blitz day of bots the hands-on raider lost 146
fleets, 9,212 ships, most of them cargo wings a patrol found in the outer
ring, where on master it lost 100 ships in all. Forty-eight Small Cargos
take 96 hundredths off a ring-15 Battlecruiser's shield in a round (two a
shot) and it is full again the next, so they cannot hurt it at all, while
under the old fight they wore it down and won. Slice 3's flight is the
cure; nothing was changed in the bots for it.

## 2026-10-13 (the web client on OGame's catalogue: what the agent decided)

Slice 1 of `docs/design/ogame.md`, section 12, stage 2: the hand-written Dart
of `clients/web` on the catalogue stage 1 built. What follows is what the
agent that built it chose for the screens. None of it was put to the owner.
Each line ends with what undoes it.

**What the state now says, for the views.**

- A home's defence is sent in its three parts: `HomeworldState.docked_power`
  (the ships standing at home), `satellite_power` (stage 1's) and
  `structure_power` (the defences). `home_defence_power` is their sum, and
  `GameEngine::home_defense_power` is written as that sum. The defence tab's
  bar and the overview's raid card draw the parts and add nothing. Undo: the
  two fields in `shared/src/protocol.rs`, `GameEngine::docked_power`, the
  two lines of `sim/src/views.rs`.
- `DefenceOption.room`: how many more of a limited defence may be ordered
  now, `max` less those standing, being rebuilt and on order; absent where
  there is no limit. The client clamps a batch to it and greys the order at
  0. `HomeworldCatalog::new` takes the committed counts for it. Undo: the
  field and the parameter; the client would then send the order and print
  the engine's refusal.
- No column and no constant was added to `shared/tests/wire/mod.rs`.

**The plan of the homeworld.**

- Twelve buildings around the homeworld hex: the ring of six is the Metal
  Mine, the Crystal Mine, the Deuterium Synthesizer, the Fusion Reactor, the
  Solar Plant and the Research Lab, clockwise from the top; a column to the
  west holds the Robotics Factory, the Nanite Factory and the Shipyard, a
  column to the east the Metal Storage, the Crystal Storage and the
  Deuterium Tank. Undo: `planSlot` in
  `clients/web/lib/scenes/homeworld/plan.dart`.
- Each tile prints one figure under its level, the first of the building's
  `effects` as it stands now (`METAL 43,200/H`, `ENERGY MADE 79`, `STORAGE
  1,565,085`). A building at level 0 prints what that effect is at 0, which
  for the Nanite Factory is the speed the Robotics Factory already gives.
  Undo: `effectCaption` in `lib/views/homeworld/spec.dart`.
- Icons for the six buildings that had none are drawn in
  `_buildingIcon` of the same `plan.dart`: a sun for the Solar Plant, an atom
  for the Fusion Reactor, an arm for the Robotics Factory, three cells for
  the Nanite Factory, a crate with a crystal and a barrel for the two new
  stores.

**Thirteen classes on the scope.**

- Three letters a class, written in Dart (`shipAbbr`,
  `lib/console/threat.dart`): SCG LCG LFT HFT CRU BSH REC PRB BMB SAT DES DTH
  BCR. A glyph a class for the player's ships and another for hostiles
  (`Sym.own`, `Sym.hostile`, `lib/design/symbols.dart`), and a sprite size a
  class in the windshield (`_pxOf`, `lib/scenes/windshield/sim.dart`). These
  are drawing; if the abbreviations should be the engine's, they are one
  column in `shared/tests/wire/mod.rs`.

**Research.**

- Four lanes: energy and beams (Energy, Laser, Ion, Plasma, Graviton), drives
  (Combustion, Impulse, Hyperspace Technology, Hyperspace Drive), combat
  (Weapons, Shielding, Armour), systems (Espionage, Computer). The lane a
  technology stands in is the client's; every edge drawn is a `requires` of
  the catalogue between two researches, and a requirement on a building or
  on energy is printed in the inspector and not drawn. Undo: `_Lane.of` in
  `lib/views/homeworld/research_tab.dart`.

**Energy.**

- One readout (`EnergyReadout`, `lib/views/homeworld/economy_tabs.dart`),
  from `HomeworldState.energy`: made, needed, the share the mines run at,
  the satellites and what each adds, the Fusion Reactor's burn and whether
  it is starved. It stands above the plan on the buildings tab and under the
  resource tiles on the overview. The header carries a chip with the share
  when energy is short, and at any time where the header has the width.
- Short of energy is an alert on the overview and what NEXT UP says after a
  raid warning and a stranded fleet, ahead of a full store and an idle slot.
  Its button orders a level of the Solar Plant
  when the catalogue says one can be ordered and none is on order. That it
  is the Solar Plant and not a satellite or the reactor is the client's
  advice, not a rule. Undo: `nextUp` in
  `lib/views/overview/overview_logic.dart`.
- The share is printed as a whole percent rounded down, so it reads 100 only
  when the mines have all they need.

**Solar Satellites and defences.**

- A satellite is a card of the shipyard like any class, with `STAYS AT HOME`
  where the others print a drive, what one adds in energy
  (`EnergyState.per_satellite`) and how many stand. They are counted again
  on the defence tab, with their share of the home defence and the quarter a
  lost raid takes (`RAID_LOST_SHIP_FRACTION`), in the overview's hero and in
  the energy readout.
- The defence tab lists all eight defences as orders. A batch is the
  shipyard's (x1, x5, x10), clamped to `room`; a defence with a limit shows
  no batch buttons, a tag `ONE OF A KIND`, and the limit's sentence when no
  more may be ordered. Undo: `defenceBatch`, `limitReason` in
  `lib/console/economy_view.dart`; `ItemSpec.batched`.
- The console takes `defence <kind> [count]` (also `defense`, `def`), and
  every item of the four tables by its label without spaces or by a short
  word (`metal`, `solar`, `yard`, `tank`, `lab`; `lf`, `sc`, `probe`, `sat`;
  `rl`, `ll`, `gauss`). `help` and the refusals of a wrong word are made
  from the generated enums. Undo: the four maps of
  `lib/state/command_parser.dart`.

**Figures and words.**

- A rate is printed by the game hour, signed, in the header, the overview
  and the storage tab; a stockpile that falls (deuterium under a Fusion
  Reactor) prints `FALLING` in the alarm colour where the others print when
  they are full. Undo: `signedAnHour` in `lib/console/intel.dart`.
- A time of a day or more prints its days (`3d 4:10:22`); OGame's prices
  make such times ordinary.
- Two figures the client used to work out are gone and nothing replaces
  them: the resources-to-power ratio on a ship card, and the amount a raid
  could take on the storage tab. The tab prints what the state says is safe
  (`StorageState.protected`) and the engine's `full_in_s`.
- The storage tab is one row a resource, each with the order for its own
  building, and stands beside the hold panel from 980 pixels of width.
- The sentences of the outpost that say "a quarter" are as they were, typed.
- The first-hour list of the guide is: raise a mine, raise the Solar Plant,
  build a Shipyard, build a Small Cargo, fly 6 sectors, board a derelict,
  win a fight. The names are the generated labels. Undo: `Lesson` in
  `lib/console/onboarding.dart`.

**The scripted world of the widget tests.**

- `test/support/demo_world.dart` types no number of the game. Its catalogue
  rows are the engine's, read from `fixtures/protocol/catalog_by_level.json`,
  which `shared/tests/golden_fixtures.rs` renders: twenty-one catalogues at
  season pace, every building and research at one level, the Nanite Factory
  held at 0. Its rings, outposts, raids and constants are read from
  `docs/design/economy/numbers.json`. The demo is therefore a season world.
  Where its levels are mixed it takes each row from the catalogue of that
  item's level and carries a time by the ratio of the two speeds the
  catalogue states; a time may be a tick off the engine's. Undo: the fixture
  and `catalog_levels` in `golden_fixtures.rs`; the demo would need another
  source.
- Its home raid destroys satellites and docked ships and takes no resources,
  and its pirates outside the homeworld are passive or on patrol only. No
  view is tested on a plundered stockpile.

## 2026-10-13 (the catalogue is OGame's: what the agent decided)

Slice 1 of `docs/design/ogame.md`, section 12, stage 1: the engine, the wire,
the bots, the planning simulation and the documents. The owner decided that
the catalogue is classic OGame's with its numbers, energy included; the study
gave the folds (5.5), this game's columns (5.7) and the knobs (11). What
follows is what neither settled and the agent that built it chose. None of it
was put to the owner. Each line ends with what undoes it.

**A new empire.**

- It stands with a Metal Mine, a Crystal Mine and a Solar Plant at level 1
  (22 energy made, 22 needed). OGame begins with nothing built; an empire
  with no Solar Plant and one order would have had its mines at a stand for
  its first minutes. Undo: `BuildingLevels::default`.
- `STARTING_RESOURCES` stays 500 / 300 / 100. OGame gives 500 / 500 / 0. Undo:
  the constant.
- Levels stop at 20 for buildings and for research (Graviton at 1). OGame has
  no ceiling. At blitz pace a bot at the controls all day reaches mines at 20
  within the day. Undo: `MAX_BUILDING_LEVEL`, `MAX_RESEARCH_LEVEL`.

**OGame's formulas, where they met this engine.**

- The curves in `1.1^level` (mine output, energy needed, Solar Plant, the
  Fusion Reactor's burn) are worked in whole numbers (`11^L / 10^L` in 128
  bits). In floating point `10 x 1.1` is 11.000000000000002 and its ceiling
  12, where OGame's table says 11. The Fusion Reactor's output, whose base is
  not a tenth, is 64-bit float through `iac_shared::maths`. Undo: nothing to
  undo; the tables of the study are what both give.
- The Fusion Reactor runs while the stockpile holds deuterium, and its burn
  is taken after the tick's production, never below zero. At an empty
  stockpile it runs one tick and stands the next. Undo:
  `Player::power`, `process_homeworlds`.
- Solar Satellites are a count on the player, ordered in the shipyard as a
  ship. They never join a fleet. Until raids are battles they stand in the
  homeworld's defence at their fight figures (21 of power each), a lost raid
  destroys a quarter of them rounded up, and they score as ships. Undo:
  `Player.satellites`, `home_defense_power`, the raid's loss.
- Graviton Technology asks for 300,000 energy made when it is ordered and
  again when it starts; the energy is not spent. No derelict's data core
  gives it. Undo: `research_ready`, `pick_tech_cache`.
- A homeworld holds one Small and one Large Shield Dome, counting those
  standing, being rebuilt and on order; a second is refused with
  `MaxLevelReached`. Until raids are battles a dome is a hull and a shield in
  the sum of the defence and nothing else. Undo: `DefenceKind::max_count`.
- Each storage building protects 8 percent of its own cap a level from a
  raid, to 60 percent: the Storage Vault's rule, one resource at a time.
  Undo: `storage_protected_at`.
- The time of a research is `(metal + crystal) / 1000` hours behind the Lab's
  level alone; OGame 0.84 has no research network and none was added.

**This game's columns.**

- A ship's record no longer carries its speed. Speed is the class's, with the
  owner's drive research, everywhere at once (the study's section 9 says so
  of drives; the record held a copy that could only disagree). Tanks follow
  the same rule: every fleet's is counted again when a Deuterium Tank level
  or the Impulse Drive level that moves the Small Cargo completes.
- A ship's tank is its jumps (5.7) times what it burns a jump, so the Small
  Cargo's doubles with its burn at Impulse Drive 5 and its reach stays.
- Wrecks to recover from a derelict: an Espionage Probe or a Light Fighter at
  tier 1; a Light Fighter, a Small Cargo or a Heavy Fighter at tier 2; a
  Heavy Fighter, a Large Cargo, a Cruiser or a Battleship at tier 3. Undo:
  `recoverable_ship_class`.

**The world's strength** (the study leaves these to be fitted).

- A raid's mean power is `50 + 9 x S^0.75` (`RAID_POWER_FLOOR`,
  `RAID_POWER_PER_POINT`; the exponent is unchanged). The floor is about half
  a Light Fighter, as it was half a Scout. The 9 is the old 11 carried over
  so that the same share of an empire's worth, put into the cheapest
  defence, meets its raid: an empire's points are ten times what they were,
  and a point of Rocket Launchers is 51 of power where a point of Pulse
  Turrets was 113 (`11 x 10^0.25 x 51 / 113` is 8.8). An empire of 1,000
  points meets its raid with 3 percent of its worth in launchers. It was
  carried over by arithmetic and not fitted against play: in the seven-day
  bot world 2 raids were lost and 405 repelled, and the planning
  simulation's competent player has several times the strongest raid docked
  at home. Any empire that keeps a fleet at home is safe from it, as before.
  Undo: the two constants.
- An empire is raided once its Shipyard is at 2 or it has built any defence
  (`RAID_MIN_SHIPYARD`): the Defense Grid's part in that rule went with it.
- What a repelled raid leaves in orbit is priced as Light Fighters of the
  raid's power, as it was priced as Corvettes.
- The map multiplier of 3 (`MAP_PAYS`) is applied to ore in a tile, to what a
  doctrine counts as a seam worth the trip, to a derelict's hold and to the
  outpost's cost, store and yield; the outpost's build rate takes it too, so
  its levels take the hours they took.
- **Wreckage is not three times the old constant.** A group leaves
  `K x power^0.8`, and power is now counted on a scale ten times the old one
  (`NPC_POWER_BASE` 4 to 40). Three times the old K (3.6) would have left 19
  times the wreckage ring for ring. `WRECK_VALUE_K` is 0.57
  (`1.2 x 3 / 10^0.8`), which leaves three times what a ring left. The other
  reading of the study, that the constant itself is tripled, is one number
  away. Undo: the constant.
- The outpost's platform is the power of the defences the study names by
  level (4 Rocket Launchers; 8 and 4 Light Lasers; 16, 8 and 4 Heavy Lasers),
  at their base figures: 408, 1306, 3972. `OUTPOST_PLATFORM_POWER` and its
  growth are gone. Undo: `scaling::outpost_platform`.

**The score.** Core points are cost over 1000, and OGame's costs are about
ten times this game's. The points that are not a cost were multiplied by ten
so that they stay the share of a score they were: a chart (`1.5 + 1.0 x
T^1.5`), a first boarding (`15 x T^1.5`), a relic (300). Kill points follow
the wreckage and needed nothing. Undo: `CHART_BASE_POINTS`,
`CHART_THREAT_POINTS`, `DISCOVERY_POINTS`, `RELIC_POINTS`.

**The exchange.** Its span is 15 percent of what the homeworld makes in a
window with all the energy it needs, the natural 20 metal and 10 crystal an
hour included and the Fusion Reactor's burn left out. So no empire has a span
of zero any more, and a reactor cannot make a span negative. Undo:
`Player::mined_per_tick` at the span.

**The wire.** Static figures of a class or a defence are columns of the
generated enumerations; what depends on the player (research in the figures,
times, requirements, effects) is in the catalogue of the homeworld's state;
the rapid-fire table is sent once, in `WorldInfo.rapid_fire`. A building's or
a research's entry carries `effects`, what it does at its level and the next,
so that no client holds one of OGame's formulas. `Requirement` gained
`energy` and counts in 32 bits. `HomeworldState.production` is what the
stockpile gains, so its deuterium may be negative. `ShipClassCounts` became a
list of `ShipCount`. Undo: these are additions but for the last two.

**The bots.**

- The planner reads requirements from the catalogue's tree, builds the Solar
  Plant before a mine that would run short, orders Solar Satellites only once
  the Solar Plant is at 20, and builds Rocket Launchers and no other defence.
- Its scarcity rule, which tilts the choice of mine toward the resource the
  stockpile is short of, read each stockpile against its cap. OGame's stores
  begin at 100,000 and the rule went blind: two explorers stood at Crystal
  Mine 1 for a week of a season. It now reads each resource as a share of the
  stockpile, and `SCARCITY_PULL` is 2.0 where it was 4.0.
- The pilots fight at `SAFE_ODDS` 2.0 where it was 3.0, and the careful
  doctrine at 2.0 where it was 2.5. OGame's ships carry a shield a fifth of
  their weapon, and at 3.0 the bots of the blitz world that
  `both_targets.rs` plays found no fight to take.
- Undo: `sim/src/bot/home.rs`, `field.rs`.

**Tests whose bands moved, and why** (the assertions were not weakened to
pass; each was read against the three questions first).

- `scenarios_raids.rs`, raids in a blitz day on a small empire: 1.2 to 3.0 a
  game day became 2.0 to 4.5. Two Light Fighters repel every raid a new
  empire draws, so the two-day quiet after a lost raid never starts.
- `both_targets.rs`, the guard that the two worlds were busy enough to
  prove anything: 12 sectors heated by a kill in the blitz world where it
  asked 20 (it has 16), and one empire with combat points and one heated
  sector in the season day where it asked 4 and 20 (it has 2 and 2). A
  season day on OGame's prices is an empire's first Light Fighters; the
  fighting that the comparison needs is the blitz world's.
- `bots.rs`, the check-in builder that pays for every batch it orders: five
  days where it ran two, and the guard that it did order batches reads its
  fleet points, because what its yard builds first is Solar Satellites,
  which fly in no fleet. The assertion itself, that no order was refused, is
  as it was.
- `exchange.rs`: the case of an empire with no mine output is gone with the
  natural production; it is now the span that production gives.

## 2026-10-13 (amounts are 64-bit: where the line runs)

Slice 0b of `docs/design/ogame.md`, section 12. The owner decided that
`Resources` becomes three `f64` (the study's default, section 8.1). Where the
64 bits stop was left to the agent that built it, and is recorded here because
every later slice writes numbers on one side of the line or the other. None
of it was put to the owner.

**64-bit: an amount, and what only ever scales one.**

- `Resources` wherever it is used: the stockpile, production a tick, the
  storage caps and what a raid cannot take, a hold and its capacity, the
  outpost's store, its caps and its yield, salvage, loot, a group's bounty,
  costs, payments and refunds, what a raid takes.
- Ore in the ground: a tile's units, its steps and reserve, what has been
  harvested from it, what it regains a tick.
- Fuel: a fleet's fuel and tank, what a jump, a way home and a recall cost, a
  doctrine's fuel credit and income. The tank is filled from the deuterium
  stockpile and the outpost's store, and by `refine` from the hold, so the
  two are one kind of number, and `FUEL_DEUT_PER_UNIT` and `REFINE_LOSS`
  between them are `f64`: no width is crossed at a dock or in
  `engine::refining`, and the wire's `Refining` carries both figures in 64
  bits.
- The exchange's figures on the wire (`ExchangeTerms`, `ExchangeMeter`), the
  fee included. The engine already worked them out in 64 bits and narrowed
  them to send them.
- The fill of a stockpile against its cap (`StorageNearCapEvent.ratio`,
  `STORAGE_NEAR_CAP_RATIO`): one amount divided by another, compared as it is
  computed. The web client's `storageNearCap` is now that same division.
- Constants and curves whose only use is to multiply an amount: the raid's
  shares of a stockpile, the refund on a cancelled order, the Vault's growth
  and protected share, the loot split, the wreck and derelict values, the
  ring's ore factor, the fuel a unit of mass burns and the three fuel
  modifiers, the harvest yield, `ResourceKind::weight`, the pace's
  multipliers for caps, holds and finds, `Pace::rate`. Each keeps the figure
  it was declared with (0.08 is 0.08); as an `f64` it is the nearest double
  to that figure and no longer the nearest float.

**Still 32-bit: what is not an amount.** Hull, shield and weapon; power,
threat and the odds ratio; score and every point; chances and every roll of
the dice; site grade; hours of regeneration and respawn. The fight is replaced
in slice 2 and these go with it or stay as they are.

**Where the two meet**, each in one place:

- `scaling::resource_weight`: an amount becomes points (`as f32`). Score, the
  doctrines' judgment of whether a seam repays its fuel, and the bots'
  budgets are points from there on.
- A jump's fuel: the fleet's mass is its hulls, 32-bit, widened exactly, then
  multiplied in 64 bits (`fleet_fuel_cost`).
- Loot: a group's power, a kill's farm factor and a derelict's roll are
  32-bit and are widened exactly where they scale an amount.
- No roll of the dice changed type. A draw that was an `f32` is still one and
  is widened after it is drawn, so every random stream gives what it gave.

**JSON.** `serde_json` reads an `f64` back exactly only with its
`float_roundtrip` feature; without it one in ten comes back a unit in the last
place away (measured: 200,843 of 2,000,000). A 32-bit float never showed
this. The feature is on in `shared/Cargo.toml` and `sim/Cargo.toml`, and a
stored world, an order and the cross-language fixtures depend on it.

**`WORLDGEN_VERSION` stays 3.** The generator rolls the same sectors. The
units a full tile holds are its density's reserve times the ring's factor,
now multiplied in 64 bits, so they differ from before in the eighth digit;
that is the economy's arithmetic, and `ECONOMY_VERSION` (8) and `SAVE_FORMAT`
(7) refuse every older world as it is.

## 2026-10-13 (standing orders may refine: one switch, off by default)

The study left this for afterwards ("standing orders can learn to refine when
they run low"), and the owner asked for fleets that "fly longer" on what they
mine and salvage. Built as its own commit on top of the `refine` order, so it
can be left out. None of what follows is the owner's.

1. **One switch on the orders, `PolicyParams.refine`, false unless set.** A
   world in which nobody sets it plays as before, and the built-in bots do
   not set it.
   *To reverse:* revert the commit; or make it the default in
   `PolicyParams::default`, which changes what every fleet under orders does.
2. **It refines at the two places a doctrine gives up for fuel**, not only
   the one the brief named: when fuel no longer covers the way home and the
   reserve (`policy_return_reason`), and when the next jump outward would
   leave too little for them (`policy_move`). The second is where a fleet
   that still has its way home in the tank turns round, which is most of the
   time; without it the switch would only ever save a fleet already short.
   It does not refine to relieve a full hold.
   *To reverse:* the one line in `policy_move` that calls `policy_refine`.
3. **It refines all it can, once, and is weighed again.** The same amount as
   the order: every unit of deuterium aboard, or a full tank. The report is a
   `PolicyAction` with the action `refine` and both figures in front of the
   shortage that caused it. At its next evaluation the fleet either flies on
   or, with no deuterium left or a full tank that is still too small, turns
   home as it would have.
   So a fleet under this switch that runs short spends the deuterium it
   carries on staying out and brings home what is left: stockpile deuterium
   for time in the field, at 15 percent.
   *To reverse:* an amount (only what the way home and the reserve lack) in
   `policy_refine`.
4. **Not charged to the fuel budget.** `fuel_budget_pct` bounds what a fleet
   under orders takes from the stockpile at the dock. Deuterium in a hold
   never was in the stockpile. A player who wants the budget to bound it too
   has the switch off.
   *To reverse:* charge `FleetPolicy::fuel_credit` in `policy_refine`.
5. **`SAVE_FORMAT` 5 to 6; `ECONOMY_VERSION` stays 7.** The orders are
   stored in the snapshot, and they hold one more field. The rule is that the
   format moves when what a snapshot must hold changes and nothing is
   migrated, and the precedent is `laden`, a field of the fleet, which moved
   it from 2 to 3. **The cost, for the owner to weigh:** every stored world of
   format 5 is refused, a server's file and a browser's save alike. A
   document of format 5 would in fact read correctly, because serde gives the
   missing field its default, which is off; keeping format 5 is the one
   constant in `sim/src/store.rs` and the two tests that carry the number
   (`server/tests/smoke.rs`, `solo/tests/single_player.rs`). The branch that
   makes `Resources` 64-bit will move the format as well; one bump between
   the two is enough.
6. **The web client has the switch in the orders panel and as `refine=on` or
   `refine=off` in the typed `policy`.** The panel has room for one row and
   no sentence under it at 900 pixels of height, so the row's label says what
   it does. The settings line above the form does not show it. The TUI was
   left alone.

## 2026-10-13 (fuel from the hold: what building `refine` had to settle)

The owner's decision is item 10 of the entry below: by an order, not in a
fight, at a loss of 15 percent. `REFINE_LOSS` is 0.15 in
`shared/src/constants.rs`, provisional and his. Building it meant choosing the
rest. None of these is the owner's; each says what he would change to reverse
it.

1. **Refused in two states and no others: in combat and while a jump
   charges.** Those are the study's two. A fleet that is harvesting or
   boarding a derelict refines and goes on with its work, and so does one
   whose cooldown is running after a scan. The order takes no time, so a
   cooldown, which says when a fleet takes its next act, has nothing of it to
   time; and a miner that had to stop mining to refine would pay a tick of ore
   for nothing.
   *To reverse:* the first line of `engine::refining`, which names the two
   states. `fleet_is_free` there would make it an order for idle fleets only.
2. **No rule of its own at a dock.** At home the dock keeps the tank full, so
   the order is refused there as it is anywhere with a full tank. A docked
   fleet whose tank is not full (a dry stockpile, a breached outpost) may
   refine what is in its hold, at the loss.
   *To reverse:* a refusal by place in `GameEngine::refine_order`.
3. **A fleet under standing orders takes the order by hand**, as it takes
   every fleet order but `load`. What it refines is not charged to its fuel
   budget: the budget is a share of the homeworld's deuterium income, and this
   deuterium is in a hold.
   *To reverse:* refuse it in `refine_order` as `load_at_dock` refuses, or
   charge `FleetPolicy::fuel_credit` in `handle_refine`.
4. **All of it or a full tank, with no amount.** The study's words. A player
   who wants to keep deuterium for the stockpile cannot say how much.
   *To reverse:* an optional `amount` on the command, as `load` has.
5. **Nothing is rounded and nothing is slack.** A hold with any deuterium and
   a tank with any room refine. A tank that is filled reads exactly its size,
   and a hold that is emptied exactly zero.
6. **The state says what the order would do** (`FleetState.refine`: the
   deuterium and the fuel), and leaves it out when the order would be
   refused. A client lights its control on that and prints the figure; it
   does not multiply. The reason for a refusal is not in the state: the
   control says the rule in general, and the refused order is answered with
   the engine's sentence.
   *To reverse:* nothing to reverse; a `refusal` sentence beside it is the
   addition if a client should say which of the four it is.
7. **The answer is an `Alert` (info)**, as for `load` and `unload`, not a new
   kind of event.
8. **`REFINE_LOSS` is not in `numbers.json`.** The planning simulation flies
   no fleet on its hold. Two constants are handed to the web client
   (`shared/tests/wire/mod.rs`): `REFINE_LOSS`, which its words quote, and
   `FUEL_DEUT_PER_UNIT`, which only the scripted test world uses to keep to
   the engine's rule.
9. **The versions stand: `ECONOMY_VERSION` 7, `SAVE_FORMAT` 5.** No cost,
   time, formula or payment moment moved, and a stored world plays on exactly
   as it would have until somebody gives the new order. The snapshot holds
   nothing new. The built-in bots do not refine.
10. **Where the clients put it.** The web client: a Refine button under the
    hold in the windshield's fleet status, always there and greyed without an
    offer, and `refine` in the command bar. No key and no tile in the dock
    row: it is offered where the hold and the tank are shown, and a tile
    there was not tried against the phone layout. `tools/playtest/play.py`
    prints the offer in the fleet row. The TUI has neither: a `d` key in its
    fleet panel was written first and taken out again the same day, when the
    owner retired the TUI.

## 2026-10-13 (the web client is the interface; the TUI is secondary)

The owner:

> also update docs where needed, flutter UI is primary ui, TUI is not
> secondary and not necessariliy supported. (humans via flutter ui, llms
> interact with engine directly)

Read as "now secondary"; he then asked whether the TUI should be kept in play
after all, and was advised not, for these reasons. The first players come
from a static page, where only the web client and the engine in the tab
exist; the TUI needs a server. Agents use the headless NDJSON mode, not the
screens. The TUI has no tests of its own. And the move to OGame's catalogue
would have every screen built twice.

So: people play through the Flutter web client. Agents play against the wire
protocol. A feature is done when the protocol and the web client have it. The
`clients/tui` crate keeps building, because the headless client (about 400 of
its 5,000 lines) and the server's scenario tests live in it; its ratatui
screens need not follow new features and may fall behind. Nothing is removed.
If the screens ever stop being worth compiling, the headless client is split
out first.

**The same day, the split and the TUI's leaving.** The owner asked for the
headless client and the TUI to become two crates ("strip out headless client
to its own crate, and TUI as its own crate. then we can archive TUI later (and
its in history to pull if we ever want a TUI again)"), and, told that the
OGame catalogue would otherwise have to keep the TUI's screens compiling
through every change of type, answered: "yes take tui out of workspace after
the split is committed". So once the split is merged, the TUI's crate leaves
the workspace and the tree, moved to the gitignored `.archive/` as every
retired file is; the commit of the split is where it can be had back from.

## 2026-10-13 (the Flutter test flake is left alone)

`docs/design/flutter_test_flake.md` found that this machine's kernel loses
floating-point state around SVE, in a C program with no Dart in it, and that
this is the likely cause of `flutter test` failing some runs. The owner:

> thats a arm specific problem so no issue in wasm or on other machines
> (probably). we can ignore it for now, add a note in claudemd as needed

So nothing is done about it: no kernel parameter, no new kernel, no retry
around the suite, no change to the client. The suite is run with a timeout
and a failure is run again before it is read as the client's (`CLAUDE.md`).
"Probably" is the owner's word and stands: that the fault is what breaks the
suite was not shown, and no other machine has run the suite to compare.

## 2026-10-13 (the OGame study's eleven questions: the defaults)

The owner was shown the eleven questions of `docs/design/ogame.md`, section 14,
each with its default, and answered "defaults". So, by the study's numbering:

1. **Energy comes in with the mines.** Solar Plant, Fusion Reactor, Solar
   Satellite, Energy Technology's effect and Graviton have their OGame job.
2. **This game's own jobs are carried by OGame's items**, as section 5.5
   lists, the three loose fits included (fuel saving on Energy Technology, the
   fleet's tank on the Deuterium Tank, ore yield on Laser Technology). The
   owner was told that the fuel saving then costs far more than it does today.
3. **A raid is fought as a battle**, at most six rounds, lost only when no
   defender is left.
4. **A fight in open space has no six-round limit.** It ends when a side is
   destroyed or has left, or a round passes with no hull lost.
5. **The price of flight is the study's.** A fleet that orders a jump out of a
   fight stops firing and is fired on until it is gone; Patrol groups follow
   one hex, Aggressive and Swarm two, and only what they were beating; a fleet
   as fast as its pursuer is never caught; docks are refuges.
6. **Repair is free and at once at the homeworld**, a tenth of a ship's cost
   from the store at the outpost, and not to be had at a breached outpost.
7. **Refit at the home dock, by a stamp on the fleet**: Weapons, Shielding and
   Armour; a merged fleet takes the lower levels until it docks; the outpost
   mends and does not refit.
8. **OGame's storage**: 100,000 of each resource with no building. The owner
   was told this removes the early squeeze the Storage Vault makes today.
9. **The Recycler mines and strips derelicts, the Espionage Probe scans and
   reads a derelict, the Colony Ship is left out, the Deathstar is in, and a
   new empire starts with two Light Fighters.**
10. **Fuel from the hold**: by an order, not in a fight, at a loss of 15
    percent, built first.
11. **Pirates keep their losses and their damage and mend in ten steps** over
    half the time a destroyed group takes to return (1.4 to 3.5 hours at pace
    1 for rings 3 to 20), divided by pace; the clock stops while the group
    fights or pursues.

Every figure here is a knob, in the owner's word, and is named as a constant
when built. The order of work is the study's section 12: fuel from the hold,
stockpiles in 64 bits, the catalogue as one change, the fleet and the fight,
flight and pursuit, raids as battles, the windshield and the battle view.

**Standing instruction, the same message.** "dispatch agents as work comes in.
commit and push at milestones": master is pushed when a piece of work is
merged and verified, without asking each time.

## 2026-10-13 (OGame's ships, defences, buildings and research; refit at home; fuel from the hold)

The owner's words, after being shown what each research does today:

> i think i would like to actually do exact ships, defense, buildings, and
> research as ogame. identical and ship to ship and ship to defense
> interactions (which ones get bonus hits on others etc). (i enivsion fleets
> of hundreds of ships so we would need to make windsheild UI be
> representative, not literaly for number of ships. (maybe starting a fight
> opens a modal showing actual ships and firings? idk). i actually quite like
> ships getting refit when they get home, not automatically in the field (but
> if its a problem, we can make it easy and an upgrade works game wide.
> whatever). it would be nice if fleets could use the dueterium they
> collect/salvage/mine from their cargo and put in their fuel so they can fly
> longer (maybe have an option to 'clean duet and refuel' which allows cargo
> duet to refuel ships but at a cost like 15% what do you think?)

**Decided: the direction.** The ships, the defences, the buildings and the
research are to be OGame's, with its numbers, and so is what one does to
another in a fight (rapid fire among ships and against defences). A fleet may
be hundreds of ships, so the windshield shows a fleet and not each hull, and
a fight may open a view of the ships and their fire.

**Not decided, and nothing is built.** What "exact" means where this game has
what OGame lacks (the hex map and its jumps, fuel tanks, harvesting,
derelicts, pirates, raids, outposts, standing orders, the hub, pace) and where
OGame has what this game lacks (energy, fields, moons, espionage, debris) is
the work of a study, `docs/design/ogame.md`, commissioned the same day. It
ends in questions for the owner, as `outposts.md` and `storage.md` did.

**Refit.** The owner likes a ship taking new research when it comes home and
not in the field, and accepts research that works everywhere at once if refit
turns out to be a problem. The study recommends one.

**Fuel from the hold.** The owner's proposal: deuterium in a fleet's hold may
be made into fuel at a loss, "like 15%". An opinion was asked for, not an
order given.

**What this overtakes.** What the reading of research found (Navigation's
levels 3 to 5 do nothing, hull, shield and weapon research never reach ships
already built, Reinforced Hulls raises the fuel a jump costs, defences take
nothing from research) is not patched on its own: it falls to the OGame work.

**Standing instruction.** A worktree whose work is merged is removed as the
work comes back ("be sure to clean stale worktrees as work comes back").

**Answered the same day**, before the study was written:

> ogames original/classic is what im after. and they use generic names so we
> can copy them. if we can do per round would be more fun imo while at
> windshield as we do now (ogames instant battles are less fun watch bcuz
> theyre instant). and if we do per round firing, can we have the ability to
> run to next hex as a fleet to flee (more dangerous enemies will chase a hex
> or two?) this also give penalties to slower ships (users may make large,
> fast ship fleets for guerilla warfare type battling).

1. **Which OGame: the original, classic rules.**
2. **Names: OGame's own.** The owner's judgment is that they are generic.
3. **A battle is fought a round at a time**, watched from the windshield as
   fights are now, and not resolved at once as OGame does.
4. **A fleet may flee a fight to the next hex**, and a more dangerous enemy
   may chase it a hex or two. Speed is to matter: a slow ship is a liability
   in a fleet that means to run, and a fleet of many fast ships that strikes
   and leaves is a way to play the owner wants. This is asked for, and its
   rules are the study's to propose: what leaving costs, who chases, how far,
   and what becomes of damage when a fight breaks off.

**And then, on what the study had been told to ask:**

> yes i would have penalties for fleeing, the ships will be damaged until
> home. sheild recharge at whatever rate is. i would have pirates heal over
> time slowly, so its an active guerilla warfare, not just "fight them once
> per day for a week". pirates should heal over a few hours maybe? or less,
> maybe an hour? these are all knobs we'll need to work on in balancing. i
> want buildings, ships, defenses, research to be like ogame, but our current
> gameplay and other setup i really like. we will keep everything as it is,
> just migrate the buildings, shipyard, research and such to the ogame stuff

5. **The scope is the catalogue, not the game.** The buildings, the ships,
   the defences, the research and what they do to one another become
   OGame's. Everything else stays as it is: the map and its jumps, fuel,
   mining, derelicts, pirates and their raids, outposts, standing orders, the
   hub, pace. Where an OGame item has no place in that game, the study says
   so item by item; it does not bring OGame's other systems in to give it one.
6. **Fleeing has a price.**
7. **Damage lasts until the fleet is home.** This departs from OGame, where a
   survivor is whole after a battle, and is the owner's choice.
8. **Shields recharge at OGame's rate.**
9. **Pirates heal slowly, over about an hour to a few hours**, so that a
   group worn down must be pressed and cannot be finished at leisure over a
   week. The figure is a knob for balancing, as are the others here.

## 2026-10-13 (the web client stops working the rules out for itself)

Merged into master and pushed on 2026-10-13.
The journal entry of the same day has what was run.

**The owner's request.** The sweep that came with the generated wire types
listed places where the web client works a rule out for itself and gets
another answer than the engine. The owner asked for them to be fixed.

**The rule.** A client draws and words. It does not work out a rule of the
game. What a view shows comes from one of three places, tried in this order:
the state the server already sends; a constant or a table generated from Rust
through `shared/tests/wire/mod.rs`; a field added to the state for it. Only
where none fits does the client keep a formula, with a comment naming the Rust
function it follows. No value the game plays by moved: `ECONOMY_VERSION` is 5,
`chart_golden.txt` and `numbers.json` are untouched.

**The eight disagreements, as settled.**

1. **Every tech has a node.** The research tab listed 12 techs by hand and
   Modular Fabrication was not one. The lanes are now an exhaustive `switch`
   over the generated `ResearchType`, filled from the catalog. A tech added
   in Rust does not compile in the client until it has a lane.
2. **As many build slots as the state says.** `HomeworldState.build_slots` is
   1 plus the level of Modular Fabrication, so 1 to 3. The queue strip has a
   pill for each, and one locked pill for the next slot while Modular
   Fabrication has a level left. The overview keeps four cards: the second
   card lists every slot after the first. Each slot after the first has its
   own Cancel. On the overview an unlocked slot with nothing in it reads
   IDLE; before, it read LOCKED. The research and shipyard cards count their
   line against 1 plus the waiting line, not against the building queue's
   depth.
3. **Boarding takes the engine's 20 ticks.** `EXPLORE_DURATION_TICKS` is of
   the real-time class: 20 ticks at every pace. While a fleet boards, its
   `cooldown_remaining` is what the boarding has left (a server test already
   held it to `end_tick - tick`). The ring is drawn from that and from the
   generated constant, so it is right after a reconnect too. It was a timer
   of 8 seconds.
4. **No loot tier is invented.** The map printed `1 + min(3, ring / 6)` for
   every sector. The engine's tier is the threat band of the ring (1 to ring
   11, 2 to ring 21, 3 beyond) and belongs to a derelict. The line is gone;
   a sector that holds a derelict shows the tier the server sent, as before.
5. **The Fabricator shortens building time.** Its note said it speeds up the
   shipyard, and its tile said SLOT B. Slots come from Modular Fabrication.
   README and `RULES.md` already had it right.
6. **One source for zones.** `Zone::INNER_RING_RADIUS` and
   `OUTER_RING_RADIUS` are generated. `WorldZone` in `lib/protocol/protocol.dart`
   is the one place a ring is given its zone and its name; six copies of the
   ternary and four of the names are gone. The map says `R21+ THE WANDERING`.
7. **A route's time and its way home.** The engine charges a jump for
   `10 / slowest speed` ticks, less the Navigation level, and lands the fleet
   on the tick after: a Scout fleet takes 2 ticks a jump, one with a Frigate
   3, and Navigation takes one off each. The state now says so:
   `FleetState.jump_ticks`. The route panel's ETA is hops times that. The way
   home from a route's end is counted over the lanes on the player's chart,
   not by straight distance, and the panel says so (`OK BY THE CHART, 4 hops,
   margin 65 fuel`). The server counts the way home over every lane, charted
   or not, so its count is never more than the chart's: a return the panel
   calls possible is possible. For a route of one jump the server's
   `preview_move` answers, as before. `preview_move` takes an adjacent sector
   only, so it cannot answer for a longer route.
8. **A derelict can be boarded with a group in the sector.** The engine has
   no rule against it. It refuses a boarding only where there is no derelict,
   or while the fleet fights, moves, already boards or is on cooldown, and it
   words each refusal. The windshield now greys Board only where the state
   shows no derelict, and sends the order otherwise; a refusal arrives as the
   engine's sentence. A group that attacks on sight has the fleet in combat
   the tick it arrives, so the case this opens is mostly a derelict beside a
   passive group.

**The hand-copied numbers.** Generated and used: the highest threat (9), the
scan ranges (1 and 2) and the scan cooldown (5), the most a site's grade can
be (12, now `SITE_GRADE_MAX`), the lane search limit (64, now
`LANE_SEARCH_HOPS`), the longest name (24, moved to `shared` with its
shortest), the bounds of the doctrine settings (now `POLICY_*_MIN` and
`_MAX`), the fuel budget that lifts the bound (100), the rings of an outpost
where three sentences spelled them, and the four ratios the gauge is drawn
from. The storage threshold is generated too and compared as the engine
compares it: `stock / cap` as an `f32` against the `f32` 0.85. Arithmetic in
double precision against 0.8500000238 would have moved the edge; this does
not, and a stockpile exactly on it reads NEAR CAP in the client on the tick
the engine says `StorageNearCap`.

Left as literals, with the reason: the right end of the ratio gauge (4.0, how
far the scale is drawn); the ten colours of the threat palette (a drawing
table, indexed by its own length); `mapRMax` 22 (how far the map draws); the
steps the doctrine panel offers between the generated bounds.

**What the client still copies.** Each is small and has a comment or a type
that holds it: `WorldZone.ofRing` follows `Zone::from_distance` on generated
radii; the status panel picks the scan range by "a Scout aboard", as
`handle_scan` does; the locked slot says Modular Fabrication opens it, as
`scaling::building_slots` does.

**Found in the engine, and left as found. The owner's to decide.**

1. **A boarding pays a tick after `end_tick`.** `SiteExplorationStarted`
   carries `end_tick = tick + 20`. The timer reaches 0 on that tick, and
   `process_exploration` runs before `process_cooldowns`, so the loot or the
   ambush comes on `end_tick + 1`. By reading, not by a run.
2. **A fight drops a boarding without a word.** A patrol that wanders in
   takes the fleet into combat. The boarding is gone, the site is not put on
   alert, no `Alert` says so, and the fleet keeps what was left of the
   boarding timer as its cooldown after the fight.
3. **`home_fuel` is not the charted way home.** The protocol and the README
   say it is the fewest hops over lanes the player has charted. The engine
   prices `direct_hops_home`: every lane, charted or not.
4. **`preview_move` cannot answer for a route.** A sector more than one jump
   away is refused with `NoConnection`. If the server should judge a planned
   route's return, that is a new request.

**For the owner.**

1. Merge or not. The protocol gained one field, `FleetState.jump_ticks`.
2. Four views still count "hops home" and "fuel for N hops" by straight
   distance or by dividing rounded figures (the sweep listed them; the brief
   did not). `FleetState.range_hops` is sent and shown nowhere. A `hops_home`
   on the fleet would settle them.
3. The route panel adds up whole-number fuel (`jump_fuel` rounded, times
   hops). Over ten jumps that can be a few units off the engine's figure.
4. The map inspector still calls a sector RICH, TYPICAL or POOR "for this
   distance" by a formula of its own (`1.2 + 0.11 * ring`). No rule of the
   engine stands behind it.
5. The scripted world of the widget tests still gives ring 8 Corvettes where
   the engine gives Scouts (`npc_class`), and moves a fleet at once where the
   engine has it in transit. Its jump now takes as many ticks as the
   engine's, and its boarding 20.
6. The captions under the ratio gauge (98 percent, 1 to 15 percent) repeat
   the comments on the `RATIO_*` constants, which the sweep found at odds
   with `RATIO_MARGINS`.

## 2026-10-13 (the economy's numbers are written once, in Rust; `sim.py` reads a file Rust renders)

Merged into master and pushed on 2026-10-13.

The owner: "The Dart protocol mirror and sim.py could be generated from the
Rust source instead of mirrored by hand: seems like a good idea." This is the
`sim.py` half. No number of the game moved: `ECONOMY_VERSION` is still 5, the
fixtures are untouched, and `sim.py` prints what it printed, byte for byte, in
ten runs (`--tables`, the full report and eight more). The journal entry of
the same day has what was run.

**As built.**

- **A number of the economy is written in Rust and nowhere else.**
  `sim/tests/economy_numbers.rs` renders `docs/design/economy/numbers.json`
  from `shared/src` and `sim/src/score.rs`; `sim.py` loads it and declares no
  number of the game. The test runs in `cargo test --workspace` and fails when
  the file in the repository is not what it renders, naming the first line that
  changed. `UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers`
  renders it again: the switch of the protocol fixtures.
- **The generator is a test of `iac-sim`**, not the example of `iac-shared` it
  replaces (`shared/examples/economy_constants.rs`, archived). The score
  constants are in `sim/src/score.rs`, which `iac-shared` cannot see, and the
  old check read them out of the source with a regular expression. A test is
  also the one place `cargo test` can render and compare.
- **JSON, not a Python module.** Nothing is imported, so nothing is compiled
  into `docs/`.
- **What is generated, by this rule.** Where Rust names every coefficient of a
  formula, `sim.py` keeps the formula and reads the coefficients, so it
  computes what it computed, to the bit, and `--set` reaches them. Where Rust
  writes the coefficients inside a function and the simulation only asks at
  whole rings, threat ratings or levels, the file carries the function's
  values and `sim.py` looks them up: a group's class, presence, passive share,
  respawn and base multiplier, the ore odds, a derelict's ambush chance and
  tier, the relic chance, the Defense Grid's units, what a research level
  multiplies, the building slots. Where the simulation needs such a formula at
  any pace or any score, its coefficients got names in Rust: six of them,
  `RAID_POWER_FLOOR`, `RAID_POWER_PER_POINT`, `RAID_POWER_EXPONENT`,
  `DERELICT_VALUE_RING`, `CAP_EXPONENT_PAST_KNEE` and
  `THREAT_POINTS_EXPONENT`. Naming them changed no value: the rendered file was
  the same bytes before and after.
- **The tables are the game's own `f32` values**, each written as the shortest
  decimal that reads back as it (0.08, not 0.07999999821186066). Four of the
  looked-up curves carry `f32` rounding the Python formulas did not have
  (the ore odds, the passive share at rings 9, 13 and 14, the grid's units
  from level 5, the base multiplier). The simulation's state moved by at most
  5 parts in 100 million over 40 runs, no level, ship or milestone moved, and
  nothing printed changed.
- **`tools/check_sim_constants.py` compares formulas only**, 2,390 values,
  against the `tables` half of the same file, with the tolerances it had: a
  cost within half a unit (the game rounds, the simulation does not), a time
  within 1 percent or 2 ticks (the game counts whole ticks), `f32` against
  `f64` elsewhere. It covers costs and times by level, production, storage
  caps at three paces, raid power, the ring gradient, chart, discovery and
  kill points, the exchange and the outposts. It is 172 lines where it was
  315, and no longer holds a mapping of constants.
- **`--set NAME=value` works as it did**, on the same names. More names can be
  set now, because inline figures became names (`NPC_POWER_GROWTH`,
  `RAID_POWER_EXPONENT`, `DERELICT_VALUE_BASE` and others).
- **One copy of the game is still kept by hand in `sim.py`**: the terrain
  shares and ore chances of `zone_terrain` and `ore_prob`. Rust rolls them
  inside `shared/src/world.rs`, with no table to render, and nothing compares
  them. See the first question below.

**Found on the way, and left as found. The owner's to decide.**

1. **The terrain copy is slightly off, and unguarded.** Worldgen rolls a
   number from 0 to 100, which is 101 outcomes, so its shares are 55, 15, 10
   and 21 in 101 where `sim.py` has .55, .15, .10 and .20, and an ore chance
   of 90 is 90 in 101. About 1 percent. Rendering these from Rust needs the
   rolls of `roll_terrain` and `roll_resources` as a table; giving the
   simulation the true shares would move its reports.
2. **`sim.py --tables` prints the wrong storage caps at pace 600.** Its table
   uses P^0.75 at every pace (606,154 metal with no Vault); the game, and the
   simulation's own runs, use the knee at pace 10 (78,254). The same column is
   in `spec.md`, section 3. One line to fix; it changes the table.
3. **Rust holds numbers that come from `sim.py`, pasted by hand**: the
   `first_cruiser_s` and `endgame_s` of `pace::PRESETS`. They have drifted
   from what the simulation prints today (persistent: 15.8 and 73.9 days in
   Rust, 19.3 and 77.0 printed; dev: 13 hours to the endgame in Rust, 19.2
   printed). Nothing compares them.
4. **The simulation's exchange and outpost functions feed no report.** They
   exist to be compared with Rust. They were kept, on generated constants, and
   stay compared; removing them and their rows is cleaner if nobody calls them
   by hand.
5. **Smaller differences the model has always had**: its queue depth of 3
   counts running and waiting orders together where the game lets 3 wait
   behind the running ones (it now reads `QUEUE_WAITING`); it plays from ring
   4, the middle of the home seats 3 to 5; it makes a player eligible for
   raids at Shipyard 2 where the game also accepts a Defense Grid.

## 2026-10-13 (the Dart wire types are generated from the Rust ones)

Merged into master and pushed on 2026-10-13. The journal entry of the same
day has what was run.

**The owner's words.** "The Dart protocol mirror and sim.py could be generated
from the Rust source instead of mirrored by hand: seems like a good idea."
This entry is the Dart half. `sim.py` was another session's work.

**What is decided by building it.**

- **The Rust types are the only place the wire is written.**
  `clients/web/lib/protocol/protocol.g.dart` is generated from them, checked
  in, and never edited. `UPDATE_FIXTURES=1 cargo test -p iac-shared`
  regenerates it with the fixtures; `cargo test -p iac-shared` fails while it
  is stale. A protocol change is: edit Rust, regenerate, fix what
  `flutter analyze` points at.
- **The generator reads the source and is proved against serde.** It reads
  `shared/src` with `syn` (a dev-dependency of `iac-shared`, already in the
  lockfile for serde's derive; no engine build compiles it). Reading source
  means interpreting serde's attributes, which could go wrong silently, so two
  things stand against that. It refuses, by name, any serde attribute, type or
  skip test it does not know. And it makes sample messages from its own model
  for every type, variant and field, has serde parse and re-emit each, and
  requires the result to match the model. A misread `rename`, `default`,
  `flatten`, tag or `skip_serializing_if` fails `cargo test` before any Dart
  runs. The defaults serde fills in are learnt from those round trips.
- **Why the others lost.** `serde-reflection` cannot describe internally
  tagged enums or `flatten`, and the protocol is built on both. `typeshare`
  has no Dart output and reads neither. `schemars` and `specta` need a derive
  on every wire type, so a dependency of `iac-shared` that the wasm build
  would compile or a feature gate on some 120 types; both re-read serde's
  attributes themselves and check nothing against serde; and a JSON Schema
  has lost what the Dart API is named after: the Rust field name behind a
  `rename` (`ShipState.shipClass` is `class` on the wire), the name of a
  flattened field (`GameEvent.kind`, `ExchangeQuote.terms`), and which struct
  a tagged variant wraps. A declarative description that the Rust types are
  generated from would have rewritten `protocol.rs` as macro input and put
  its doc comments out of reach of rustdoc.
- **The shape of the Dart, by rule and not by table.** A struct is a class
  with a `const` constructor, `fromJson` and `toJson`. An enum of unit
  variants is a Dart enum with its `wire` name. A tagged enum is a sealed
  class. A struct that one tagged variant wraps and nothing else names
  extends that sealed class (`AuthResult` is a `ServerMessage`); a variant
  with fields or none is a class named variant plus the enum's last word
  (`MoveCommand`, `RequestFullStateMessage`); a variant that wraps a type
  used elsewhere is a wrapper named the same way (`CommandMessage`,
  `EventMessage`). A constructor parameter is optional exactly where serde
  fills the field, with serde's value, and for a struct with `Default` in
  Rust every parameter has that default. A struct with `Eq` in Rust has
  value equality in Dart. Doc comments are Rust's.
- **What Rust knows beside the wire is generated too**, where it is a
  constant or a per-variant value: the labels of eleven enums, `ShipClass` hull,
  shield, weapon, cargo and power (the client's own table in
  `lib/console/threat.dart` and a second one in the windshield painter are
  gone), `ResourceKind.weight`, `ErrorCode.code`, `botNamePrefix`,
  `maxOddsSectors`, the four ratio bands, the outpost rings. One line in
  `shared/tests/wire/mod.rs` each, which calls the Rust function or names the
  constant. An `f32` constant is written as the exact value of that `f32`, so
  a comparison with a number off the wire falls as it does on the server.
- **What stays written by hand**, and what guards it:
  `lib/protocol/protocol.dart` (155 lines) is the client's own: extension
  getters and `HomeworldState.copyWith`, checked by the Dart compiler against
  the generated classes. `lib/protocol/hex.dart` is hex math with the two
  fields of `Hex`, guarded by `fixtures/hex` and by every fixture that
  carries a coordinate. Formulas and sentences in the views that repeat
  `scaling.rs` or the engine are not touched; the journal lists them.
- **Decoding is as strict as it was.** Unknown tags and enum names throw
  `FormatException`, a missing required field throws, an optional field is
  nullable and left out when null. What went is a convenience of the hand
  mirror: its constructors had defaults serde does not have (eight fields of
  `HomeworldState`, `DefenceState.restoring`, `HomeworldCatalog.defences`,
  `waitingOn` of the three queued orders, `HarvestCommand.resource`,
  `StandingOrders.params`). A generated constructor asks for what serde asks
  for, and the test world that leaned on those defaults now names the values.
- **Three names changed**, by the rules above, with every caller:
  `RequestFullState` is `RequestFullStateMessage`; a lone event is
  `EventMessage(GameEvent)`, because `GameEvent` is also what `TickUpdate`
  lists and cannot carry a `type` there (`GameEvent.toEventJson` is
  `toJson`); `RatioLabel.forRatio` is the function `ratioLabelFor`, and
  `OddsCommand.maxOddsSectors` the constant `maxOddsSectors`.

**For the owner.**

1. Merge or not.
2. The generated file declares `// dart format width=120`, the width the
   hand-written file was typed at; the rest of the client is not formatted
   with `dart format` at all (on master 91 of its 99 files would change). 80
   is one constant in `wire/dart.rs`.
3. `fixtures/protocol/synth` adds 67 files and 337 KB of generated messages,
   one a line. They make the Dart proof complete without anyone writing a
   fixture. If that is too much to carry, they can be made in memory by a
   Dart test that shells out to cargo, at the price of the client's tests
   needing cargo.
4. The inventory in the journal lists what the client still repeats of the
   engine by hand, and the places where the two disagree today. None was
   changed here. Which of them are bugs is the owner's call.

## 2026-10-13 (one stored world: both hosts store one document, and there is no journal)

The owner's answers to the questions of `docs/design/storage.md`, section 12,
given on 2026-10-13, and what was built on them the same day: steps 1 to 4 and
8 of that study's section 10, merged into master and pushed on 2026-10-13.
`docs/data_architecture.md` describes the result; the
journal entry of the same day has what was run.

**The five answers.**

1. **A killed server may lose more than one tick.** The owner: "this is not
   mission critical infrastructure, we want to be professional but we don't
   need to be perfect". So there is **no journal** of inputs. None was built
   and nothing was left in the code for one; the study stays as the record of
   how to add it.
2. **Reading a world with SQL is given up.** A world file is a header line and
   JSON, read with `jq`.
3. **The server's place is files.** The owner has no preference between files
   and SQLite; files were chosen by the session that briefed the build, for
   about 100 lines and because `rusqlite`, with the SQLite it compiles, leaves
   the workspace. The browser's place stays IndexedDB behind the worker.
4. **The save format number is the builder's to judge by what is cleanest**:
   no additive compatibility, no importer for old databases or saves. All
   current worlds are disposable. Built: `SAVE_FORMAT` 4 to 5, because the
   header changed shape (what a host keeps beside its world moved into one
   `host` field). A browser save and a server database from before are
   refused or ignored; nothing is migrated.
5. **No compression on the server.**

**What follows from them, as built.**

- **A stored world is one document**, the same for both hosts
  (`iac_sim::store`): a header line (format, economy and worldgen versions, a
  checksum of the body, the host's own part) and the `Snapshot` as JSON. The
  server stores it in a file (`--world`, which `--db` became), the browser in
  IndexedDB. The single-player host loads the bytes a server wrote once the
  header's own part names a commander and a clock, and a server plays a
  single-player save as it is; `server/tests/hosts_agree.rs` holds both.
- **A new persistent field is written once, in `Snapshot`.** The engine knows
  nothing of storage. `server/src/database.rs` (1,630 lines and 487 of
  tests), `sim/src/persist.rs` (`Persist`, `PersistBatch`, `RuntimeKind`), the
  engine's ten dirty sets and the lines that marked them, the one in
  `intel.rs`, the `Persisted` phase of the host core's step (the entry below
  still names it) and the repairs `restore` made to rows read back out of
  step are gone. Rule for what comes next: nothing may
  go into the engine that exists for storage.
- **What a stop costs, and this is the decision's price.** A server that is
  stopped (SIGINT, SIGTERM) writes its world and loses nothing, so a deploy
  loses nothing. A server that is killed comes back as its last file says:
  it loses up to the period, orders included, where it lost one tick before.
- **The period is 300 seconds** (`--save-every`): the owner's choice of
  2026-10-13 ("300s is fine"), from this table. It was built at 60 seconds,
  set by the session that briefed the build without weighing the bytes:

  | `--save-every` | a kill loses at most | written a day by the largest world measured (24 seats at the end of a season week, 13.2 MB a document) |
  |---|---|---|
  | 60 s (as built) | a minute | 18.9 GB |
  | 300 s (the default) | five minutes | 3.8 GB |
  | 600 s (the study) | ten minutes | 1.9 GB |

  The rows this replaces were measured at 7 to 10 GB a day in a world's first
  three days. A young world writes far less either way (0.4 GB a day in its
  first minutes with 15 bots).
- **A registration does not wait for the period.** A token is shown once, so
  an account the world forgot would leave a player holding a password to
  nothing. Once a token is issued the file is written within two seconds of
  the last write (about three seconds in all, since the tick that does it
  comes once a second). A world has 24 home seats, so this can happen at most
  24 times in a world's life, and claims.
- **A world keeps the seed and the pace it was created with.** Both are in
  its file; a server started on an existing world with another `--seed` or
  `--pace` refuses with a sentence. Before, another seed regenerated the
  galaxy under its players with a line in the log (the study, 9.3). The old
  `--world <preset>` flag, a second spelling of `--pace <preset>` that nothing
  used, gave its name to the file.
- **One server per world.** The file's lock (`<file>.lock`, `flock`) is held
  while a server runs; a second one refuses to start. SQLite's own lock did
  this by accident before.
- **"When is a document owed" is said once**, in `store::Due`, for both
  hosts: a period after the last one, or sooner once hurried, never sooner
  than a short gap after the last. The times stay each host's, because what
  a lost document costs differs: the single-player host replays the ticks
  since its save, so it hurries for an order and saves every four minutes;
  the server replays nothing, so its period is its loss and it hurries only
  for a token.

**Not decided here, and the owner's.**

- **Moving a world between browser and server** (the study's question 7). It
  is not built. What is left to build is small and is listed in
  `docs/data_architecture.md`: an import and an export on the page, and
  clearing the commander's token so that the first login claims the account.
- **A server's world still pauses while its server is down** (question 8).
  Nothing changed there.
- **Editing a world by hand** needs the checksum restamped, and there is no
  tool for it.

## 2026-10-13 (one host core: hosting is written once)

Asked for by the owner: "Remove ALL duplications so we only have one source of
truth for shared pieces. Standalone code that differs should be strictly
solo<->multi (like ws and tokens) and native<->web (like storage); anything we
can move to a shared crate we should." Merged into master and pushed on
2026-10-13. Storage was left to the study running beside this
work. The journal entry of the same day has what was run.

- **The rule.** The rules were already written once, in `iac-sim`. Now the
  hosting around them is too, in `iac_sim::host`, and every host runs it: the
  server, `iac-solo`, the headless runner and the tests that play a world with
  bots. A host may differ from another only in who may log in and how bytes
  reach it (solo against multiplayer), and in its clock and its storage
  (native against web).
- **One tick: `host::step`.** The cast takes its turns, `GameEngine::tick`,
  its events are drained, every logged-in session's `tick_update`, the cast
  observes, `persist_dirty_state`. That order was written three times
  (`server/main.rs`, `solo/world.rs`, `sim/headless.rs`) and four more times
  in tests. The three hosts did not agree on it to the letter: the server
  drained and sent before the bots observed, the single-player host sent after
  the bots had observed and after the persist, the headless runner drained
  after its seats had observed. **Nothing moved by choosing one**, and this was
  checked, not assumed: observing takes the engine by shared reference,
  `views::tick_update` reads what the tick's refresh left (`lapsed`,
  `expired`, `marked`) and never the dirty sets a persist clears, and the
  events are handed to the views as an argument. The season report of
  `sim-run` is the same bytes before and after, and the chart recording passes
  untouched.
- **A host that must look inside a tick gives the step a `Watch`**, called at
  each of the five phase boundaries with the engine, the cast and the tick's
  events. The headless runner's counting is one (it notes tanks and stores once
  the seats have played, and counts when the tick is simulated), and so are
  the server's tick timings. This is the alternative to a host copying the
  sequence to get between two of its lines.
- **One gate: `Host::receive`.** A logged-in session's message goes to
  `GameEngine::answer`; `auth` goes to the `Door` and a session it admits is
  sent the result and then its full state; anything else from outside is
  `commands::not_logged_in`. The session-to-account map is the host core's.
- **The seam is the `Door`,** one method, with both sides next to each other
  in `host.rs`: `Tokens` (a new name registers, a known one presents its
  token; the host gives it a mint) and `Commander` (one name, no token). The
  server contributes only the mint, from OS entropy. They live in the core and
  not in the hosts so that the whole of the difference can be read in one
  place; a host chooses one.
- **Seating a world is `Host::new` and `Host::enlist`**, for a world just
  made and for one resumed alike. What a host says when the seats run out
  stays its own: an operator reads about `--player-seats`, a player about the
  seats beside theirs.
- **One check of a stored world's versions: `host::Stale`.** It decides and it
  words the refusal; a host names what it stores ("save", "database") and adds
  what its reader can do about it. The server's sentence now says "this build
  runs" where it said "this server runs"; the single-player sentences are
  unchanged to the letter. The save's format number and the database's pace
  and missing `world_meta` stay with their hosts, which alone have them.
- **`iac-server` is a library and a binary.** The cross-host guard has to
  read a server's database as a `Snapshot`, and only the server can
  (`Database::load_snapshot`); a crate with nothing but a binary lends
  nothing to its tests. `lib.rs` is three `pub mod` lines. The engine is no
  longer behind `Arc<Mutex<..>>`: only the tick ever touched it, and the
  transport now holds sockets and a queue and no world.
- **What the transport still decides, as before:** a message whose session
  has gone by the time the tick drains the queue is dropped unprocessed. The
  socket map is the transport's, so this stayed there. A session's leaving
  reaches the host core as an entry in the same queue.
- **The cross-host guard is `server/tests/hosts_agree.rs`**, in
  `cargo test --workspace`. A real server makes a world (seven bots at tick 0,
  then an account over the wire) and is stopped; what its database holds is
  read as a `Snapshot` and handed to `iac-solo` as a save document; the server
  is started again. One commander then plays 40 ticks on both: nine batches of
  orders, accepted and refused, with questions about the whole world. Every
  frame of the two sessions, from the answer to the login on, and the final
  worlds (`Snapshot::hash` after a clean stop) must be equal. About 45 seconds.
  - *Why a stored world and not two new games.* The front doors cannot make
    the same world: the single-player host seats its commander first and the
    bots after, all at tick 0; a server seats its bots at tick 0 and can admit
    a player no earlier than tick 1. That difference is real and was left
    alone. Resuming one stored world in both also puts SQLite and the save
    document side by side.
  - *How orders land on known ticks.* Nothing is assumed about the server's
    clock. Each batch travels between two `request_full_state`; the server
    answers a session when it takes its messages in, before that tick's
    update, so the update after the two full states names the tick, and the
    single-player host is given the batch before the same tick. A batch the
    server took in two ticks cannot be placed, and fails the test saying so
    instead of guessing; it needs a stall of about a second in the middle of
    three small frames.
  - *What it catches and what it cannot.* It catches a host that departs from
    the core: with the step's order broken for the server alone it failed at
    the fifth frame, naming the field. It cannot catch a change to the shared
    order, which moves both hosts together (run so, it passed);
    `chart_equivalence.rs` and `bots.rs` catch that (run so, both failed).
- **Left as they are, for the owner to rule on:**
  - The headless runner seats its players as ordinary accounts
    (`Seat::join`), the hosts seat bots as bot accounts with a record
    (`Roster`). Making the runner use the roster would list its bots apart on
    the leaderboard they read, and move every report.
  - `sim/tests/common` and `scenarios_world.rs` tick a bare engine
    (`tick`, then read the events). That is the engine's own interface under
    test, with no cast, session or storage, and some of those tests call
    `persist_dirty_state` at chosen moments to read one batch; they were not
    put on the step.
  - Three test files carry the same 12-line `first_difference` over JSON
    (`database.rs`, `both_targets.rs`, `hosts_agree.rs`), and the two solo
    test files the same `call`/`frames`/`send`. One copy needs a
    test-support crate, and the bar for new crates is the owner's.
  - What a client does with a `tick_update` is written in `script.rs` (for a
    seat), in the TUI's `state.rs` and in Dart. The TUI keeps its own shape of
    state; the Dart side is native against web by nature.
- **`CLAUDE.md` carries the rule**, added at the merge: the layout rows of the
  three crates, the guard's command line, and the architecture bullet that
  says a host writes no tick loop, session map or login of its own.

## 2026-10-13 (the question that led to one stored world)

The owner asked whether the browser's save and the server's database can be
one source of truth. `docs/design/storage.md` studied it, proposed one
document and a journal of inputs beside it, and put eleven questions. The
owner answered the same day; the decision is the entry at the top of this
file.

## 2026-10-12 (one world on every target: where the engine's maths comes from)

Asked for by the owner: the same seed and the same orders must give the same
world from a native build and from the WebAssembly build, with a test that
keeps it so. Merged into master and pushed on 2026-10-12. The journal
entry of the same day has the cause and the measurements.

- **The rule.** Float maths in `shared/`, `sim/` and `solo/` uses what IEEE 754
  fixes (the four operations, `sqrt`, rounding, casts) as it comes, and takes
  everything else from `iac_shared::maths`: `powf`, `powi`, `ln`, `log2`, `exp`,
  `exp2`. That module is the `libm` crate, which is Rust's port of musl's maths
  compiled for whatever the target is, and an integer power that multiplies in
  a fixed order. A function it lacks is added to it. No `cfg` picks arithmetic
  by target. `clippy.toml` in each of the three crates bans the std methods
  (26 of them, for `f32` and `f64`), so `cargo clippy` refuses a new one.
- **Chosen so that the wasm build did not move.** Rust's own maths on
  `wasm32-unknown-unknown` is the same port, and `maths::powi` multiplies in the
  order the compiler's routine does, so every world a browser has played plays
  on to the same bytes. The native build is the one that changed: it used the
  system's libm, and its wrecks, sector heat, raid sizes, chart and boarding
  points and odds now differ from before in the last bit. Costs, build times,
  production and storage caps did not move on either target.
- **`libm` is pinned to one release** (`=0.2.16` in `shared/Cargo.toml`).
  Another release may round a function differently, and that would change what
  every stored world plays next. Moving the pin is an economy change.
- **`ECONOMY_VERSION` 4 to 5; `SAVE_FORMAT` stands at 4.** A database written
  by the native build of version 4 would play on differently under this one,
  and the constant exists so that such a database is refused rather than
  carried on in silence. The owner has not ruled on this bump. The constant's comment now says so: it is bumped whenever a
  stored world would play on differently, the arithmetic included. The save
  format did not change. **The cost, for the owner to weigh:** a browser save
  carries the economy version too, so the saves of version 4 are refused with
  the usual sentence although they would have played on to the same bytes.
  One number cannot refuse the native databases and keep the browser saves.
  Both are disposable by the owner's word; if the browser saves matter more,
  the bump is one line to take back.
- **The guard is a test in `cargo test --workspace`**:
  `solo/tests/both_targets.rs`. It builds `iac_solo.wasm` with the cargo of the
  test run, runs it in `wasmi` (an interpreter, a dev-dependency of `iac-solo`
  alone) beside the native host and sends both the same requests; every reply
  and every save must be the same bytes. It needs the `wasm32-unknown-unknown`
  target and fails with that sentence when it is missing; it needs no network,
  no Node and no browser. Three worlds, about two minutes together, the season
  day being 114 seconds of it. Why there and not in `tool/solo_e2e.dart`: the
  browser check runs when someone runs it, and a rule that every change to a
  formula can break belongs where every change is tested. Why `wasmi` and not
  `wasmtime`: six small crates and 25 seconds to build against a compiler of
  its own, and an interpreter is fast enough (the season day takes it 94
  seconds, the native build 9).
  WebAssembly leaves nothing but the bits of a NaN to the runtime, so what
  `wasmi` computes is what a browser computes; Node and `wasmi` were checked
  against each other by hand on the season day.
- **`CLAUDE.md` carries the rule**, added at the merge: the layout rows of
  `maths.rs` and `both_targets.rs`, the two command lines, and one bullet of
  the architecture.

## 2026-10-12 (the owner's decision: an outpost's repair is paid by weight)

- **Decided by the owner**, of the four choices under "outposts measured"
  below: choice 2. A repair is priced in metal, the par value of a quarter of
  the level's cost (537.5, 1343.75 and 3359.375), and the store pays it in
  metal, then crystal at 1.5 a unit, then deuterium at 2
  (`scaling::outpost_repair_payment`). The protected floor is that payment
  read off the store as it stands, and in a raid off the store and the holds
  docked there as one pile.
- **What it was chosen on.** Season pace, 7 days, 30 unattended outposts a
  row. A level-3 outpost alone at rings 18, 19 and 20, emptied before every
  raid, keeps 56, 58 and 54 percent of what its extractor could have made
  (35, 21 and 18 before; 40 to 60 expected). None of 30 ends the week
  breached and waiting where 23 to 29 did. The rows the design calls safe did
  not move, and a world without outposts plays as before. The other branch
  built, an unpaid repair at four times the time (`outpost-repair-slow`, not
  merged), left rings 19 and 20 at 36 and 29 percent and made paying for a
  repair a loss at low levels.
- **What still waits.** An outpost breached before its store has saved up the
  price: 3 of 30 at level 1, ring 9, from an empty store.
- **Not measured.** Persistent and blitz pace, and nobody has played it.
- **Still the owner's.** Whether a repair should interrupt a build.

## 2026-10-12 (seven loose ends: the Vault and a batch, a fight's forfeit, Stop, and where status text stands)

**Decided by the agent that built it, not by the owner, and open to change.**
Seven things earlier passes saw and left. Each is its own commit.

- **A ship or defence batch asks the Storage Vault before it asks the
  stockpile.** A batch that costs more than a stockpile cap is refused with
  `StorageTooSmall` and the sentence a building or a tech gets, even when the
  stockpile holds enough. The stockpile can hold more than its cap only
  through refunds, so this closes one door: money past the cap cannot buy
  what the Vault could never hold. It still buys anything that fits.
- **Refunds are as they were, and now written down.** A cancel returns half
  of what the order has paid, and the refund is added past the cap. That was
  reported earlier and judged harmless, because cancelling costs half; it is
  in `RULES.md` and the README under Queues and slots.
- **A fight that catches a fleet charging a jump says what fuel is forfeit**,
  as a warning `Alert`, in the terms `stop` uses: the fleet, the sector, the
  jump's target, the fuel. It is raised where the jump is abandoned
  (`engage_fleet`), so the fleet that was attacked hears it just before the
  fight is announced and a fleet drawn into another's fight just after. The
  fuel named is what a jump costs the fleet at that moment, as for `stop`.
  No event kind, no state.
- **The web Stop works for as long as the engine takes a stop.** While the
  ships fly out and the jump is not yet answered, the Stop tile and `X` send
  the order; every other act still waits for the jump. The scene gives the
  jump up at once and does not wait for the engine: if the stop was too late
  the fleet arrives and the arrival is played, and if it was in time the
  ships are already home. A jump that is never answered ends the same way
  after 2.6 seconds; it used to leave the ships off the arena.
- **A status that comes and goes has room kept for it; it is not moved under
  the controls.** The orders panel's newest report keeps five lines and the
  fleet status keeps four under its Orders row while the fleet is under
  orders. Under the form the report would be still, and off a 900 px screen:
  why a fleet holds is the first thing that panel is for. The cost is blank
  lines when the sentence is short. Two things still move: a sentence longer
  than the room grows its box, and the list of earlier reports at the foot
  of the open orders panel pushes the panels below it as it fills.
- **On the windshield the toasts stand at the top at every width**, over the
  sector head, which has no controls. They did so below 1167 px already. At
  the bottom right they lay over the foot of the column that holds every
  panel. The other screens keep the bottom right on a desktop. A panel that
  opens on a desktop no longer takes the earlier toasts down, because none
  is in its way; the phone layout's sheet still does. One toast of the
  longest sentence the engine has clears the gate cards and the fleets. A
  stack of three reaches the corner of the north-west gate card, and a
  taller one the fleet rail, as on narrower windows. The bottom left, over
  the feed, has a little more room and was not taken: a toast as wide as
  they are would stand on the dock's first tile once the dock has nine.
- **Resizing the window keeps what is open.** The windshield's panels move
  between the desktop column and the phone sheet with their state, and the
  shell's screens keep theirs when the outpost's strip and the bottom rail
  come or go in one step.
- **A new outpost says nothing about its empty store to the fleets that
  founded it.** The fleets standing there when level 1 opens count as told.
  A fleet that arrives later is warned, and so is one that fills up and
  thirsts there again. The other way, waiting until the store has ever held
  deuterium, needs a field in the outpost, the snapshot and the save for one
  message. The sentence still says "no deuterium left" to a second fleet
  that reaches an outpost nobody has stocked yet.

## 2026-10-12 (outposts measured: no constant moved, one rule for the owner)

**Not decided by the owner.** The measurement `docs/design/outposts.md` asks
for in 7.8 was made; its table of results stands there and the journal entry
"outposts measured" has the command lines. Every `OUTPOST_*` constant is
still the design's proposal and still provisional.

- **No constant moved.** Nine of the ten rows read as the design expected or
  cannot be read; the tenth is wrong for a reason no constant reaches.
  - Yield (`OUTPOST_YIELD_PER_HOUR`, 2.0): level 3 on a grade-5 site at rings
    12 to 16 is 4.6 to 4.7 percent of its owner's home output on day 7, and
    the richest site of a world 15 to 19 percent, against 4 to 6 and 16
    expected.
  - Cost (`OUTPOST_COST`): level 1 on a grade-3 site at ring 12 pays back in
    120 game hours. The Metal Mine levels bots buy on day 3 run from 10 to
    16. Against the median of all 45 bots (level 12, 103 hours) the outpost
    is 16 percent slower; against the median of the check-in bots (level
    11, 79 hours) it is 51 percent slower, one point past the half the
    design allows. Left alone: by the end of day 3 all but two bots are
    buying level 12 or higher, and a convoy takes longer than that to get
    there.
  - Platform and raid (`OUTPOST_PLATFORM_POWER`, `OUTPOST_RAID_FACTOR`): no
    raid lost where the design calls the platform safe; level 1 alone at
    ring 9 loses 44 percent; level 3 alone loses 25, 79 and 100 percent at
    rings 18, 19 and 20.
  - A defended outpost lost nothing in a season week, at each garrison the
    design names.
- **The row that is wrong is a rule.** A level-3 outpost left alone at rings
  18 to 20 keeps 11, 4 and 3 percent of what its extractor could have made,
  against 40 to 60 expected and 20 as the floor. A repair costs metal,
  crystal and deuterium; four sites in five lack one of them; a breached
  outpost extracts nothing. So a site that lacks an ore pays for as many
  repairs as it was stocked for and then waits for a delivery: 23 to 26 of
  30 unattended outposts end the week breached and waiting wherever raids
  are lost, and spend half to three quarters of it so. Sites with all three
  ores behave as the design says. `OUTPOST_RAID_LOSS` and
  `OUTPOST_REPAIR_SHARE` were tried at 0.10 and put back: 23 to 24 of 30
  still end the week waiting. The choices, none of them built:
  1. *Keep it.* An undefended outpost that loses raids is dead until its
     owner comes. Then the design's "gives back about half" should read
     "gives back nothing once it has lost twice", and the client should say
     so before the founding.
  2. *The repair is paid by weight.* It costs the score weight of a quarter
     of the level (250 metal, 125 crystal and 50 deuterium weigh 537.5 at
     level 1) in whatever the store holds. Any site that makes anything
     repairs itself. This is the smallest change that gives the design's
     numbers.
  3. *An unpaid repair takes longer.* A store that cannot pay repairs
     anyway, in four times the time. Nothing is ever stuck, and a delivery
     is a speed-up.
  4. *A breached outpost extracts at a quarter rate.* It helps only sites
     with all three ores, which need no help.
- **A breach during a build waits for the build.** Measured: level 2 with
  level 3 building loses the first raid at rings 15 to 17 and then stands
  breached for 6 hours of a season world, 3.6 of them behind the build.
  Whether a repair should interrupt a build is the owner's.
- **`outpost_at` reads an index.** The engine keeps the owner of the outpost
  in each sector beside `outposts` (`GameEngine::outpost_sites`), changed by
  founding and abandoning only. Every sector view asks whether an outpost
  stands there, and with fifteen outposts the search through them was half
  of what outposts cost a 16-seat week. The answers are the same.
- **What the report's outposts are.** `sim-run --outpost` edits the world's
  snapshot. Nothing is founded, carried or paid, the site is the free sector
  of the ring nearest the grade asked, and a lair there is displaced as a
  founding displaces it. No bot supplies, defends or hauls from it.

## 2026-10-11 (outposts: what the engine tells a client beforehand)

**Decided by the agent that built it, not by the owner, and open to change.**
The clients of slice 1 worked five things out for themselves that the engine
knew. Each is now the engine's to say, in `shared/src/protocol.rs`, and
neither client judges it.

- **Why an upgrade would be refused is in the state**
  (`OutpostState.upgrade_refusal`), in the sentence the refusal carries, as
  `OutpostSurvey.refusal` is for founding. Upgrade is off while it is
  present, with the sentence under it. This replaces "Upgrade is never
  switched off by the console" in the entry below. For it the shortfall
  sentence lost "Nothing was started", which was only true after an order.
- **A raid is a range, not an estimate.** `raid_power` in the survey and in
  the outpost's state is `least` and `most`, the bounds of the roll
  (0.80 and 1.15 times twice the ring's group power); the one middle figure
  (`next_raid_estimate_power`, and the survey's old `raid_power`) is gone
  for outposts. The homeworld keeps its estimate: its raid is sized by the
  empire's worth, which moves, and nothing asked for it. The clients print
  the two ends and say whether the defence is above the strongest, below
  the weakest or between; none prints a chance, because the outcome also
  moves the defence a tenth either way.
- **A full store and a waiting repair are state, not event kinds.**
  `yield_lost` lists the resources the extractor makes and the store is
  full of; `repair_waiting_for` is what the store lacks of the repair. The
  state says both fully, every tick, so a client that connects late knows
  them too, which an event would not tell it. The alerts stay as they were,
  for the log. A notice is raised when a resource enters the list; the
  engine's alert waits until the store has dropped below its refill mark
  before it warns again, so after a small load the notice can come once
  more than the alert.
- **A sector an outpost stands in is rated 0**, power 0, `observed`. It was
  T1 with power 0, which read as a weak group. 0 means nothing to fight and
  nothing coming; the scale for groups is still 1 to 9 and
  `scaling::threat_rating` is untouched. Only that one case is rated 0: the
  hub and home ground, where no group goes either, still carry their ring's
  estimate, and changing them would change what bots and standing orders
  read of those sectors.
- **A fleet says it is at its owner's outpost** (`FleetState.at_outpost`),
  finished, building or breached. `outpost_fuel` keeps its meaning (the way
  there, while the outpost fills tanks) and is no longer read as "0 means
  at".
- **A survey of a sector never seen is a reply, not an error.** It carries
  what the ring decides (the cost, the time, the raids, the platform) and a
  `refusal`; the site (`site`: ore, grade, yield, hops home) is absent, and
  the refusal is worked out without looking at the sector, so neither its
  ore nor an Anomaly or a derelict there is told. Its ring is no secret, so
  a sector outside rings 9 to 20 is refused for its ring. The inspector and
  the log print it as any survey.

Two more, from watching an outpost in a real browser:

- **A panel that opens on the windshield takes down the toasts that ask
  nothing.** On a phone they stand where the sheet opens and covered its top
  rows for nine seconds. Dismissed, not pushed aside: there is no free place
  on a phone to push them to, what they said is in the log, and a toast with
  buttons stays. One raised after the panel opened still shows over it.
- **The browser drive of an outpost's life is not a kept tool.** It takes
  eleven minutes and its raid is a roll. `tool/solo_e2e.dart` stays the one
  browser check, and it still founds nothing.

## 2026-10-11 (outposts, slice 1: what the clients show)

**Decided by the agent that built it, not by the owner, and open to change.**
Where the web console and the TUI depart from section 7.6 of
`docs/design/outposts.md`, or settle what it left open.

- **`O` opens the founding panel anywhere, not only in rings 9 to 20.** The
  dock's Found tile stands in those rings only, as 7.6 has it; the key works
  wherever a fleet is, because the panel is how a player learns the rule: at
  home it shows the server's sentence about the rings. It is also what the
  browser check can reach.
- **Upgrade is never switched off by the console.** (Replaced: the state
  now carries the sentence; see the entry above.) The server has no
  sentence for an upgrade it would refuse until the upgrade is ordered
  (`outpost_survey` carries one for founding; `OutpostState` carries none
  for the next level). The panel prints the shortfall as a figure and the
  job under way, leaves the button pressable, and the server's refusal comes
  back as a toast in its own words. A console that worded the refusal itself
  would be a second judge.
- **The outpost has a screen of its own**, on the rail while an outpost
  stands (`G` then `P`; typed `5`, `op` or `outpost`), beside the panel the
  windshield opens. 7.6 says "also in the rail"; a tile that only jumped to
  the windshield would have needed a fleet to be selected.
- **The HUD line is in the bar from 1100 px and a strip under it below
  that.** The bar of a phone has no room for a fourth figure.
- **The percentage in the HUD line is the fullest resource of the store**,
  not the store as a whole: one resource at its cap is what stops the
  extractor.
- **The outpost's dock is the homeworld's hold panel**, given the outpost's
  store to draw on, not a second panel. On the windshield it stands under
  the outpost panel for the fleets standing there; `L` there opens it.
- **A sector with an outpost shows the station in place of the threat
  digit** on the map. The server rates such a sector as empty.
- **The map inspector prints the survey as three lines and the verdict**, as
  7.6 has it; the founding panel prints it as rows with the cost set against
  the hold. The inspector column is now bounded and scrolls, so a long reply
  cannot run under the route panel.
- **"Raid repelled" at the outpost is a background notice too.** 7.6 lists a
  raid, a breach, a finished level and a full store; the owner's brief added
  a repair that waits. The outcome of an announced raid is told either way.
- **TUI keys.** `o`, `F`, `U`, `A` then `y` were all free and are as 7.6
  names them. `U`, `A` and Enter act inside the `o` overlay only, so that no
  stray key upgrades or abandons; `F` works from any view and opens the
  overlay on the survey.

## 2026-10-11 (outposts, slice 1: built on the design's recommendations, pending the owner's answers)

**Not decided by the owner.** `docs/design/outposts.md` ends with six
questions only the owner can answer. Slice 1 of the engine was built before
they were answered, on the design's recommended answer to each. Each is
listed here with the code it lives in, so that a different answer is a
change in a known place and not a search.

1. **An outpost produces.** Its extractor makes
   `2 x density x ring factor x level` an hour per resource. *Where:*
   `OUTPOST_YIELD_PER_HOUR` in `shared/src/constants.rs`,
   `scaling::outpost_yield_per_tick`, `GameEngine::run_extractor` in
   `sim/src/engine.rs`. *If no:* remove the extractor step from
   `process_outposts` and `production`, `grade` from the survey and the state.
2. **It is carried in holds, with `load`; no ship class was added.**
   *Where:* `GameEngine::outpost_site` (the hold must carry the cost) and
   `handle_found_outpost` (the hold pays). *If a ship:* a sixth `ShipClass`
   and the founding rule in those two functions.
3. **No NPC raid destroys an outpost.** A lost raid breaches it and it
   repairs itself from its store. *Where:* `GameEngine::resolve_outpost_raid`
   (sets `breached`, never removes the entry) and `repair_outpost`. *If yes:*
   the lost branch of `resolve_outpost_raid`, a longer grace in
   `RaidTimers`, and a line in `solo/src/away.rs`.
4. **One outpost each, in rings 9 to 20.** *Where:* `OUTPOST_MIN_RING`,
   `OUTPOST_MAX_RING`; one each is the key of `GameEngine::outposts`
   (`BTreeMap<owner, Outpost>`) and the rule in `outpost_site`. *If more than
   one:* the map's key, and every `outposts.get(&player_id)` in the engine,
   `views.rs`, `intel.rs`, `score.rs` and `solo/src/away.rs`.
5. **`recall` is left as it is.** It still jumps a fleet to the homeworld
   with its whole hold from anywhere, the outpost included, so a convoy can
   load at the outpost, and recall home with everything. *Where:*
   `GameEngine::handle_recall`, untouched. The design proposes to ration it
   with the gateway in slice 2.
6. **A fleet standing at its owner's finished outpost does not count against
   the three deployed fleets**, and needs a free slot to leave. *Where:*
   `GameEngine::count_deployed_fleets` and `docked_at_outpost`, read by
   `handle_move` and `handle_split`. *If it should count:* drop
   `docked_at_outpost` from those three.

Every `OUTPOST_*` number is the design's proposal and provisional in the way
the exchange's four are: `shared/src/constants.rs` and
`docs/design/economy/sim.py`, kept equal by `tools/check_sim_constants.py`.

**Decided by the agent that built it, where the design was silent or the
code had moved, and open to change.**

- **A sector with a group in it cannot be founded on.** The design says the
  founding fleet stands idle "so no fight is going on", which a passive pack
  does not prevent. `outpost_site` refuses while a group stands in the
  sector or its lair has not been cleared: the site is taken from its pack,
  passive or not.
- **The founding rules are checked in an order that makes a survey from
  afar useful.** Ring, Anomaly, derelict and spacing first, then "you
  already have one", then where the fleet is, then what the fleet is doing,
  the group, standing orders and the hold. A survey of a distant sector that
  is refused only because the fleet is elsewhere names a site that
  qualifies.
- **The protected floor is the cost of the next repair**, which is the
  repair of the level being built while a level is being built. The design
  says a raid never takes the materials the repair needs; with the floor at
  the standing level a breach suffered during an upgrade would have left the
  store short of the dearer repair that follows it.
- **A breach during a build or a repair waits its turn.** One job at a time,
  as the design has it: the build goes on, and the repair starts when it is
  done. A raid lost during a repair leaves the repair running, and the
  outpost is whole when it ends.
- **A breached outpost that cannot pay its repair stays breached.** It
  extracts nothing while breached, so it cannot save up the repair either.
  On most sites that is the rule the owner meets first, because the repair
  costs crystal and deuterium and most sites make neither. The warning and
  the rules say to keep the repair cost in the store.
- **A fleet charging a jump out of the outpost is not refuelled there**, and
  a tank the store cannot fill is reported once per stay. At the homeworld a
  departing fleet is still topped up; at an outpost that would have raised a
  dry-store warning on every departure.
- **`unload` at the outpost is refused for an empty hold** (`NoResources`),
  not answered with an alert that nothing moved. `load` there is refused
  for a fleet under standing orders, as at home.
- **`settle_hold` reads the dock from where the fleet is.** Cargo shed by
  ships lost at the outpost goes into its store as far as there is room, the
  rest is lost, as at the homeworld.
- **The outpost is not a field of `Player`.** As the design has it:
  `GameEngine::outposts`, `Snapshot::outposts`, a `runtime_state` row of
  kind `outpost` in SQLite, which needs no new table or column. A row of
  that kind that cannot be read refuses the world.
- **The report counts what the engine says.** `sim-run` counts outpost
  raids, what they took, yield and ticks breached. What `load` takes from a
  store it reads from `Outpost::hauled`, a counter `load` adds to and no rule
  reads; no bot hauls.

## 2026-10-11 (the windshield's phone layout: sheet, banner, toasts, guide, fleet rail)

**Decided by the agent that built it, not by the owner, and open to change.**
Four things the layout pass before it listed as still wrong.

- **An open status sheet is not covered.** A banner (NEW SECTOR, ENGAGED,
  VICTORY) is drawn under the sheet. Where the sheet takes only the right of
  a wide arena the banner is centred in what is left beside it; on a phone,
  where the sheet takes the width, the banner is not seen while the sheet is
  open. The event it announces is in the log either way.
- **The sheet stands on an opaque ground** the size of its panels, not of
  the whole sheet area: a short sheet does not black out the map below it.
- **Toasts follow the layout, not the width.** They stand at the top
  wherever the screen in view runs a dock along the bottom: every screen
  below 820 px, and the windshield in any window too narrow for its side
  columns (up to 1167 px). The other screens keep their desktop layout at
  those widths and keep their toasts at the bottom right.
- **The guide has two wordings for a gate card, and the cards were left as
  they are.** A phone's small card shows a key and a terrain. The jump and
  harvest lessons now say so there, point at the gate's colour and at the
  Map, and say "tap"; with full cards they speak of the T digit, the bars
  and hovering, as before. Putting the digit and the bars on a 92 px card
  was the other way and was not taken: the terrain name would not have
  fitted beside them.
- **The desktop fleet rail ends where the feed may begin** (the bottom of
  the south-west gate card) and scrolls inside that; the fleet selected is
  brought into view. No count of fleets is cut off and none is hidden for
  good.

## 2026-10-11 (a fleet's orders are on its row; what an order costs is said)

**Decided by the agent that built it, not by the owner, and open to change.**
The two principles are the owner's: a console shows what a fleet has been
told whoever told it, and anything that costs the player or ends their work
is told to them as an event with its reason.

- **`FleetState.policy` is one object, `{preset, params}`, or absent.** The
  doctrine and its five settings travel together and are present exactly
  when the fleet is under orders. Two fields side by side (`policy` and a
  separate `policy_params`) could have disagreed, on the wire or in a client
  that read one and not the other; one object cannot. The name `policy` was
  kept: it is the word of `policy_update`, and the object reads as the
  message that set it. A client that read `policy` as a string must be
  changed; none was kept working.
- **The settings are the ones in force**, after the server's clamping, all
  five, never a subset.
- **The web panel keeps nothing of its own.** What it remembered of orders
  it had sent is gone; it shows and starts from the fleet row.
- **Settings given without a doctrine change the orders in force.** In the
  web command bar `policy range=6` used to mean the current doctrine at the
  defaults plus that range. It now keeps the other four settings. A line
  that names a doctrine (`policy mine range=6`) still starts from the
  defaults, which is what the panel's button sends. The TUI's `p` and `P`
  keep the settings the fleet has in the same way.
- **`stop` on a charging jump says what fuel is forfeit; on a boarding, that
  the site is on alert.** Both are warning `Alert`s. Halting a harvest costs
  nothing and says nothing. The boarding case was not on the list this work
  was given; it is the same order silently costing the player, in the same
  function.
- **The fuel named is what a jump costs the fleet at the moment of the
  stop**, not a figure stored when the jump was ordered. The two differ only
  if a Fuel Efficiency level finishes during the second or two a jump
  charges. Storing it would have meant a field in the fleet, the snapshot,
  the SQLite schema and the save format for that case.
- **A recall is answered with one `Alert` that carries its whole cost**: the
  fuel, a jump abandoned with the fuel it had burned, how many ships took
  hull damage and how much, how many broke up. It is a warning when a ship
  was harmed and info when none was, so a clean recall is a log line and a
  costly one is also a toast.
- **A harvest that ends by itself says why, once, as a warning `Alert`**:
  the hold is full, or nothing (or none of the resource named) is left to
  mine there. A warning and not info because the web client raises a toast
  only for warnings, and a fleet that has stopped working is what the player
  needs to see. A fleet under standing orders is not reported this way: its
  doctrine says what it does about it in its next report.
- **No new event kind.** All of these are `Alert`s with the fleet and the
  sector, as the hold alerts are.
- **An alert toast in the web client stays nine seconds**, as a refusal
  does. These sentences run to five lines of a toast and 4.2 seconds did not
  read them.
- **Not changed, and seen:** a fight that catches a fleet charging a jump
  also forfeits the jump's fuel and says nothing of it; the fight itself is
  announced.

## 2026-10-11 (recall is refused at home)

**Decided by the agent that built it, not by the owner, and open to change.**
The principle is the owner's: an order does what its name says or is refused
with one clear sentence, and it never silently costs the player.

- **`recall` is refused for a fleet at its own homeworld**, with
  `InvalidCommand`, nothing changed and a sentence. It used to be accepted
  there: it cost nothing, did nothing to an idle fleet, called off a jump the
  fleet was charging (whose fuel was spent and was refilled from the
  stockpile), and halted a fleet harvesting at home.
- **A jump ordered at home and paid for goes on.** The refusal names its
  target and says that `stop` is the order that calls it off.
- **A called-off jump keeps the fuel it burned**, as it always has for
  `stop` and for a fight that catches a fleet charging. A recall from
  another sector abandons a charging jump the same way and then pays for
  its own. That is now written in the rules; it was true and unsaid.
- **No refund was added.** A refund on `stop` would make every jump free to
  take back until it lands; the code has long called the lost fuel "the
  price of hesitation" (`handle_stop`).

## 2026-10-11 (a fleet carries no more than the ships it has left can hold)

**Decided by the agent that built it, not by the owner, and open to change.**
The hold entry below left this as found; since `load` a player could arrange
it at home.

- **One rule, wherever ships are lost.** After a fleet loses ships it keeps
  what the holds of the ships left carry, and no more. What no longer fits
  leaves the fleet, each resource in proportion to what was aboard. A fleet
  with no ships left sheds its whole cargo. Ships are lost in three places:
  a fight, a recall that breaks ships, and a lost raid at the dock.
- **In the field it becomes wreckage in that sector.** It joins the pile a
  kill leaves there, lasts as long (180 ticks from the last addition) and is
  taken with `collect_salvage` by whoever comes, the owner included. So the
  cargo of a destroyed fleet is no longer simply gone: it lies where the
  fleet fell. Before, a destroyed fleet's cargo vanished and a mauled one
  kept all of it.
- **A recall settles where the fleet jumped from.** Ships that break up in
  the emergency jump leave their share in the sector the fleet left, not at
  home.
- **At the home dock it goes to the stockpile, up to the Vault cap; the rest
  is lost.** A raid that destroys docked ships settles the holds before it
  takes its slice, so what moved to the stockpile is raided with it.
- **Nothing is made.** The pile, the stockpile and the hold together never
  hold more than the hold did, and a kill's score still follows only what
  the hostile group was worth. Losing ships on purpose to move cargo costs
  the ships and gains nothing a hold or the stockpile would not have kept.
- **The owner is told** by a warning `Alert` naming the amount and where it
  went. No new event kind.
- **Fuel was already so.** A tank is cut to what the ships left carry in all
  three places, and the excess is lost; that stands.
- **Not changed:** a fleet already over its hold in a saved world stays so
  until it next loses a ship. Split and merge share cargo by hold and cannot
  put a fleet over.

## 2026-10-11 (a hold is loaded from the stockpile at home)

Owner: the hub's exchange is a surplus sink, so a player whose stockpile is
full of one resource must be able to carry the surplus to the hub. The owner
set the frame: a `load` command for a fleet docked at home, limited by the
stockpile and by free cargo room; an order that cannot be met in full is
refused with what is short; loaded cargo stays aboard at the dock until the
fleet has been away; a way to put it back by hand; the state survives a
restart and a save; a laden fleet is otherwise an ordinary fleet; no rule is
dodged by it; the bots do not use it.

**Everything below was decided by the agent that built it, not by the owner,
and is open to change.**

- **One resource an order, not an amount of each.** `load` takes `fleet_id`,
  `resource` and an optional `amount`; three resources are three orders. The
  owner's frame said "an amount of each resource". "All that fits" has one
  meaning for one resource and none for three, the surplus is nearly always
  one resource, a refusal can then name exactly what is short, and the
  exchange already speaks in one resource at a time. Cost: a mixed load is
  not atomic.
- **"All that fits" is the same command with no amount**, as `exchange`
  reads a missing `amount`. It is refused only when nothing fits: the
  stockpile holds none or the hold is full.
- **`unload` is its own command**, not `load` with zero. It ends the state
  and returns the hold as far as the caps allow; the dock takes the rest as
  room opens. Refused for a fleet that was not loaded by hand.
- **The state is a mark on the fleet (`laden`) that ends with the fleet's
  next jump.** While it is set the dock leaves the hold alone; a fleet that
  has jumped away comes home unmarked and unloads as always. A cancelled
  jump does not end it (and recall at home, which did not either, is now
  refused: see the entry above). It is a column of the fleet's
  row in SQLite and a field of the fleet in the snapshot, so `SAVE_FORMAT`
  went from 2 to 3; `ECONOMY_VERSION` stands (no cost, time, production
  formula or payment moment changed, and the server adds the column to an
  existing database).
- **Standing orders and a hand-loaded hold do not mix.** `load` is refused
  for a fleet under standing orders; giving a laden fleet standing orders
  ends the mark and unloads it. A doctrine waits at the dock for a hold it
  cannot empty, so the two together would hold the fleet for ever.
- **Split and merge.** Both halves of a split laden fleet stay laden. A
  merged fleet keeps the state of the fleet that absorbs, as it keeps that
  fleet's standing orders.
- **A lost raid counts cargo aboard fleets at home with the stockpile.** One
  pile, one protected floor, the same 8/8/5 percent of what is exposed,
  taken from stockpile and holds in proportion. Before, cargo at the dock
  was out of a raid's reach, which did not matter while only a full
  stockpile left any there. This changes the rule for every fleet with
  cargo at home, loaded by hand or not. Loading therefore changes nothing a
  raid takes. Ships destroyed by the raid took nothing from the hold they
  carried when this was written; since the entry above their share goes to
  the stockpile before the raid takes its slice.
- **Holds as storage are allowed and not limited.** A fleet at home holds
  its capacity on top of the Vault. At pace 1 a Hauler holds 200 for 950
  resources of hull, 4.75 a unit stored; Vault level 3 adds 12,375 units
  for 867 (0.07 a unit), and level 20 still costs 0.6 a unit. The most
  a player can hold aboard is 8 fleets of 64 Haulers, 102,400 at pace 1:
  9.3 Vault-0 stockpiles, 1.2 at Vault 5, 0.16 at Vault 10 (at season pace
  5.2, 0.7 and 0.09; at blitz 14.6, 1.9 and 0.25, because holds grow with
  the square root of the pace and caps slower beyond a season). So it
  matters most to a young empire with an unusually large fleet.
- **Score.** Loading and unloading move none: score counts buildings,
  research, ships and defences, never stock or cargo.
- **The answer to `load` and `unload` is an info `Alert`**, as for `split`
  and `merge`, and the fleet row (`cargo`, `laden`) in the next update. No
  new event kind and no new error code.
- **Left as found here, settled in the entry above:** a fleet that lost
  ships (in a fight, to a recall, to a lost raid) kept its whole cargo even
  beyond the hold that was left. With `load` a player could put that case
  together at home.

## 2026-10-11 (ship batches pay when they are ordered)

Owner, after playing single player. Amends "ships pay per unit" in the
mechanics pass below; the queue rule for buildings and research stands.

- **A ship or defence batch is accepted only if the stockpile covers the whole
  batch now**, and it pays then. Otherwise it is refused, with the exact
  shortfall, until the resources are there. A paid batch waits only for the
  yard or for something queued ahead of it to finish.
- **Buildings and research keep queueing ahead of resources.** They are
  accepted affordable or not and pay when they start. That is how time away
  turns income into progress for a check-in player, and for a single-player
  world while its tab is closed.
- **Not decided by the owner (implemented as the simplest rule, open to
  change):** cancelling a batch refunds half of what it paid for units not yet
  delivered, waiting or running, so the yard's line cannot be used to keep
  resources out of the store.

## 2026-10-10 (the hub, PvP by zone, outposts, more worlds)

Context: `starting-positions-2026-10-10.md`. Numbers proposed there (fees,
intervals, bandwidth, level caps) are not decided by this entry.

- **PvP by zone.** No PvP in the Inner Ring (rings 1-8). PvP exists only from
  ring 9 outward. Multiplayer PvE is allowed everywhere. This settles "opt-in or
  forced" from 2026-10-09 item 4: crossing the line is the opt-in.
- **Homeworlds never move.** A player reaches better resources with an
  **outpost**, which can only be placed in PvP space: anywhere from ring 9, with
  the richest sites further out.
- **A neutral post at the centre.** The hub starts as one hex doing two jobs, an
  NPC exchange and the gate between worlds. It may grow into the seven-hex
  cluster (the centre and ring 1) with a purpose per hex as chat, items and an
  auction arrive.
- **The exchange is a surplus sink, never a faucet.** The rate worsens as a
  player trades and recovers over time, tracked per player. No hard daily cap. A
  world-wide slide is a possible later step.
- **More room means more worlds, not a third axis.** Other worlds are reached
  through the hub, and each is a flat (q,r) grid, so a player always plays on a
  (q,r) map "via world x". Worlds may open as players join; the rule that opens
  one is not decided.
- **Black market.** It sits in dangerous space, not at the hub, and moves at
  intervals: its place comes from the world seed and a time window, and a
  remembered pin goes stale. It trades resources for now and is replaced by an
  item market when an item system exists.
- **Gateway.** A research line lets fleets jump between home and the outpost.
  No jump while in combat. It carries loot, limited by a bandwidth that further
  research raises, so bulk still travels the rings.
- **Homes sit in rings 3-5, in fixed seats.** A world has a set number of seats
  (about 24) and no spill rings: a full world refuses with a clear "world full"
  and a new world is the answer. No hostile pack on or beside a home.
- **The Inner Ring is scout country.** Scout-class packs out to ring 8, so the
  jump in danger sits on the PvP line at ring 9 instead of at ring 6, where it
  killed every starter fleet.
- **Lanes are seeded per world.** Each world has its own maze. Existing
  databases are disposable.
- **Gates are one-way at first.** A fleet may enter a world as old as or older
  than its own, never a younger one, so veterans cannot descend on a new
  cohort. A gate may open both ways once the younger world has matured; the
  rule for that is not decided.
- **Home ground.** Derelicts beside a home stay; boarding one is a choice.
  `patrol_home` hunts the convoys beside home; what that pays is re-measured
  with the bots before it is tuned.
- **Threat labels tell the truth about the matchup.** A label accounts for ship
  classes (rapid fire), not power alone, so a doctrine's entry rule cannot walk
  scouts into corvettes at the ring-9 line.
- **Not decided:** whether research extends how far out an outpost may sit
  (ring 21+), the purpose of each hub hex, and how fleets, cargo and score cross
  between worlds.

## 2026-10-10 (mechanics pass: free slots start what can pay)

Amends the one-queue rule below on the line it drew: the queue is no longer
strictly first in, first out.
- **Order:** a free slot starts the earliest waiting order that can start now
  (prerequisites built, stockpile covers it). An order that cannot start keeps
  its place and is re-checked every tick. Playtest 2026-10-10b: strict FIFO left
  the second and third building slots idle behind one order that could not pay.
- **Starvation guard: `reserve`, a per-order flag.** An order sent with
  `reserve: true` holds the line while it is ready and short of resources:
  nothing behind it in that queue starts (orders ahead still do). Chosen over
  automatically reserving "nearly affordable" resources because it is explicit,
  needs no hidden accounting that an agent cannot see, keeps the start estimate
  honest, and leaves the default behaviour (use every slot) to the player who
  did not ask for anything else. The cost is that cheap orders can delay an
  unflagged expensive one; the flag is how a player says "save for this".
- **Stable ids:** every order gets an id, kept from the waiting line into the
  running slot and carried by `Queue` events. `cancel_build` and `cancel_queued`
  take the id; an id that is not in the named queue and state is refused with
  a message that says where it is (cancel by position raced the tick).
- **Ships pay per unit:** a batch is one order and each unit pays when it
  starts. A batch that cannot pay for the next unit goes back to waiting with
  its progress (`built` of `count`) and does not hold the yard. Cancelling a
  running batch refunds half of the unit under way.
- **Consistency:** research, shipyard and defence orders accept a queued Lab or
  Shipyard as a prerequisite, as buildings already did, and a running tech can
  be queued for its next level.
- **Doctrines honour `engage_ratio_x10`:** the entry rule is exactly the
  player's ratio, hostiles there or not. The old max(engage, 2.5x) override
  and the EVEN floor are gone, so a deliberately low setting works. The default
  is 15 (1.5x, the FAVOURABLE band: a fight keeps 85-95% of hull), so default
  doctrines are careful and players who want to gamble lower it. A standing hold is
  reported at most once a minute while its cause is unchanged.
- **Pins and salvage:** a remembered pile past its despawn tick leaves the
  chart; other remembered pins out of sight carry `pins_stale`. A fleet docked
  at home scoops a home salvage pile into storage by itself (it is the
  player's own planet, and the drop comes from a defended raid), with an alert
  at 60 s when it cannot.

## 2026-10-10 (play styles and balance)

**Two play styles, one world:**
- **Check-in players (OGame).** You should not need to be at the computer all day: check
  in, queue, and leave.
- **Hands-on players (The Infinite Black).** They can drive fleets, fight and explore
  themselves for fun and reward.
- **Hybrids.** Hybrid play should be natural.

**How to balance the two:**
- **Per day, not per hour.** Active players may earn more per hour of attention. A
  check-in player must not fall hopelessly behind over a week: tens of percent, not
  multiples. Field yield should have diminishing returns over a day.
- **Standing orders are the bridge.** Delegated exploring, salvage and mining should
  earn roughly 50-70% of hands-on piloting.
- **Pace matters.** Blitz is attention-heavy by nature: judge it on fun and use it as a
  stress test. Check-in balance is tuned for the persistent and season presets.
- **Balance is ongoing.** Get close, keep developing, and re-measure after significant
  changes. After the `iac-sim` split, scripted check-in, hybrid and active bots give a
  balance report per change.

## 2026-10-10 (the map pays; combat and exploration scoring)

Second playtest: staying home won; combat plus exploration never exceeded 4.8
points against cores near 400. Skilled explorers and raiders should be within
about 15% of a pure builder, and a mixed player should beat both.
- **Combat:** points follow the pile a kill leaves; full pay up to 4x the NPC
  group's power, then `4 / ratio` down to a 5% floor (the hard 8x zero is gone).
- **Anti-farming:** repeated kills in one sector pay less (0.75 per recent kill,
  heat halves every 4 respawn delays), for loot and points alike.
- **Exploration:** charts, first boardings, derelicts and relics pay points
  that scale with the finds factor (see `economy/spec.md` section 18).
- **Cap:** stays at 25% of core. The simulation shows it is enough: it binds for
  an explorer at season pace and not at blitz, and a higher cap would let an
  explorer overtake the builder with a lower core.
- **Cargo:** ship holds scale by P^0.5 like loot, so finds can be lifted and
  Haulers matter to a raider.
- **Raids:** a lost raid also destroys a quarter of the ships docked at home;
  missed rolls raise the next roll's odds.

## 2026-10-10 (one queue rule)

**Every order that can ever start is accepted; it pays when it starts.**
The strict first-in-first-out line is amended by the mechanics pass above.
Supersedes the 2026-10-09 queue-payment rule below, which refused an order that
had a free slot but no money while letting the same order wait behind a running
item.
- **Accept:** buildings, research, ships and defences are accepted whenever
  they fit the queue, slot free or not, affordable or not.
- **Wait:** a waiting order costs nothing. It starts, and pays in full, the
  moment a slot is free, what it needs is built and the stockpile covers it.
- **Order:** strictly first in, first out per queue. An order short of resources
  holds back the ones behind it, so the line never reorders and the start
  estimate is honest. An order that waits only on a prerequisite does not hold
  the line.
- **Depth:** running items plus 3 waiting (`slots + 3` for buildings, 4 for
  research and shipyard), so extra build slots never remove the ability to
  queue.
- **Refuse only the impossible:** locked prerequisites, max level, a payment
  larger than any storage cap, or a full queue, each with a specific error.
- **Show:** each waiting order carries its exact shortfall, why it waits and an
  estimated start. Starting, waiting and cancelling are all confirmed by a
  `Queue` event; cancelling a running item refunds half, a waiting one nothing.
- **Why:** this suits slow persistent worlds, where players queue and leave
  (OGame-style), and it is one rule instead of a rule that depends on whether a
  slot happened to be free.

## 2026-10-09 (queue payment, superseded)

**Queued items pay when they start, not when they are queued.** The rest of the
old rule (refuse an unaffordable order that would start now) is replaced by the
2026-10-10 entry above.

## 2026-10-09 (single player and built-in bots)

Queued after the economy and exploration milestones land, in this order:

1. **Extract `iac-sim`.** A pure engine crate (state, `tick()`, command handling)
   with persistence behind a trait. The server keeps SQLite and WebSockets.
   Balance testing moves to running the real engine headless and fast, instead of
   a separate simulation that can drift from the code.
2. **Built-in bot players.**
   - **How they play:** scripted, server-side players that act only through the
     normal command path (no cheating), reusing standing orders plus build-order
     personalities (miner, raider, explorer, turtle).
   - **Role:** they make small worlds feel alive and give single-player opponents.
   - **Ranking:** bots are part of the world and are tagged BOT. They are kept out
     of the shared human and LLM ranking, or shown clearly separate.
   - **LLM agents:** these remain real outside players.
3. **Single-player browser build.**
   - **Bundle:** a build flag (e.g. `scripts/play.sh --single-player`) produces a
     static bundle: the Flutter client plus `iac-sim` compiled to WebAssembly, in
     one tab with no server.
   - **Wiring:** the client uses an in-process transport with the same JSON
     protocol, and IndexedDB for saves.
   - **Closed tab:** missed ticks are fast-forwarded on reopen.

## 2026-10-09 (economy spec and UI review)

Context: `economy/spec.md` and `ui-redesign/spec.md`.

1. **Pacing.**
   - **Persistent world:** pace 1 reaches an endgame around day 74 and keeps
     progressing for a year.
   - **Presets:** persistent 1, fortnight 5, season 10 (weekly), sprint 70,
     dev 100, blitz 600 (one-hour arc), test 8000.
   - **Fixed per world:** pace is set when a world is created and is not
     changed afterwards.
2. **Faster start:** Corvettes unlock at Shipyard 1, so a persistent-world player
   has a combat ship on day one.
3. **Tech tree:** a bigger tree is wanted, in a later milestone. The economy ships
   first.
4. **Smaller questions:** the economy spec's defaults apply to its smaller open
   questions (raid cadence, the 25% combat and exploration score cap, derelict
   finds scaling) until the owner says otherwise.
5. **Web UI direction.**
   - **Basis:** the Amber Glass prototypes are approved as the basis. Keep the
     palette and the animations.
   - **Tone:** tone down the cartoony graphics. Design it as the interface a
     1970s IBM engineer would want to play: an operator console with vector
     line-art, schematic symbols and labelled readouts.

## 2026-10-09

Context: `gap-analysis-2026-10-09.md` and the 2026-10-09 six-agent playtest.

1. **Pacing is a server setting.** One world-speed flag scales the game from a
   persistent world (months or more) through faster seasonal worlds (e.g. a weekly
   reset) and a blitz, up to a very fast setting for testing (~100x). The flag
   scales economy timers (build, research, shipyard, production, regeneration,
   respawns); real-time piloting cooldowns stay human-scale.
2. **Economy.**
   - **Buildings:** they are the primary resource source.
   - **Mining:** finding and mining ore is a bonus that grows with distance from
     the inner circle.
   - **Additions:** defences, storage caps, a second building slot (unlocked by
     research), and ore that regenerates.
3. **Exploration pays.** Loot, richer ore and things to discover increase further
   from the inner circle, matched by visible, rising danger.
4. **Multiplayer.**
   - **Trading:** requires a way to communicate (chat).
   - **PvE team-ups.**
   - **PvP in the outer Wandering:** opt-in or forced is still to be decided.
5. **One ranking for AI agents and humans.** Agents are players.
6. **Web UI is the main human interface.**
   - **Redesign:** give it a full redesign to be a fun, good-looking, interactive
     game interface, especially the windshield and the map, rather than a port of
     the TUI.
   - **TUI:** it stays as a secondary client.
   - **LLM agents:** they keep playing through the CLI.
