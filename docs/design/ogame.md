# OGame's catalogue in this game

Date: 2026-10-13. Status: study for owner review. No game code was changed when it
was written. It answers the commission in `decisions.md` (2026-10-13, "OGame's
ships, defences, buildings and research; refit at home; fuel from the hold") and is
built on the nine answers the owner gave the same day, which section 2 quotes.

**Source note.** Every statement in this document is one of five kinds, and says
which where it matters.

- *Decided*: an owner decision, quoted in section 2. Not reopened.
- *Measured*: read from this repository's code. The file and function are named.
  Nothing was built or run for this study (two other agents were timing a test on
  the same machine), so no figure here is a measurement of running code.
- *Sourced*: an OGame number or rule, taken from a source named in section 3 and
  checked against a second one where a second exists.
- *Unconfirmed*: an OGame figure or rule that only one source gives, or that the
  two sources give differently, or that is remembered and was not found. Section
  15.3 lists every one.
- *Knob*: a number this game has to supply because OGame has none for it, or a
  number the owner has called a matter for balancing. Each has a constant name, a
  starting value and what it trades against. Section 11 is the list. None of them
  is argued for as the right number.

---

## 1. The study on one page

**What changes.** The catalogue: the buildings, the research, the ships, the
defences, and what one unit does to another in a fight. They become those of
classic OGame, with its names and its numbers.

**What does not.** The game. The hex map and its lane jumps, the fuel tank, mining,
derelicts, pirate groups and their raids, outposts, standing orders, the hub and
its exchange, pace and the score all stay as they are. No OGame system is brought
in to give an OGame item a home: no planet fields, moons, colonies, espionage
reports, debris fields, missiles, expeditions or fleet slots.

**The catalogue in numbers.**

| | OGame (classic) | Taken | Left out | This game today |
|---|---|---|---|---|
| Buildings | 18 | 12 | 6: Terraformer, Alliance Depot, Missile Silo, Lunar Base, Sensor Phalanx, Jump Gate | 10 |
| Research | 16 | 14 | 2: Intergalactic Research Network, Expedition Technology | 13 |
| Ships | 14 | 13 | 1: Colony Ship | 5 |
| Defences | 10 | 8 | 2: Anti-Ballistic and Interplanetary Missiles | 3 |

Of the 47 taken, 21 have a plain place here, 21 take over the job of something this
game already has, and 5 come in only if energy does (question 1). Every building
and research this game has today is either replaced by an OGame one or has its job
carried by one; none is kept beside the OGame set (section 5.5, question 2).

**What a fleet becomes.** Today a fleet is an array of at most 64 ship records.
It becomes a count for each class, plus the hull of each damaged ship. Shields are
no longer stored at all: OGame restores them in full every round. A fleet of five
hundred undamaged ships is fourteen numbers.

**What a fight becomes.** OGame's round, unchanged: every unit fires at a random
enemy unit, shots too weak for a shield bounce off, a ship under 70 percent hull
may explode when hit, rapid fire chains. One round is fought each tick, as fights
are fought today, so it can be watched. Three things are not OGame's, and each is
decided or follows from a decision:

1. Damage lasts until the fleet is home (decided). OGame makes every survivor
   whole after a battle.
2. A fleet may leave a fight for the next hex, and stronger groups follow it a hex
   or two (decided). OGame has no flight.
3. A fight in open space has no six-round limit, because either side may leave.
   A raid on a homeworld or an outpost keeps OGame's six rounds.

**The four choices that shape the work most.**

| Choice | Recommended | Why |
|---|---|---|
| What a fleet is | Counts by class and the hull of each damaged ship | Hundreds of ships cost nothing in the stored world or on the wire; damage that lasts still has somewhere to live |
| Energy | Comes in with the mines | Without it five OGame items have no job and the mines cost about half what OGame's do |
| Raids | Fought as battles, six rounds | It is the only place a defence fires, so the only place OGame's ship-against-defence rules can exist before PvP |
| Order of work | The catalogue first, then the fleet and the fight | OGame's fight does not work on this game's present ship numbers; the present fight works on OGame's |

**One thing the owner did not ask about and has to know.** Stockpiles, holds and
stores are 32-bit floats (`Resources`, `shared/src/constants.rs`). At OGame's
scale a stockpile of four million loses a mine's whole output to rounding at pace
1. They have to become 64-bit before the catalogue lands (section 8.1).

**Slices** (section 12).

| Slice | What | Rough size |
|---|---|---|
| 0a | Fuel from the hold, on the game as it is | half a day |
| 0b | Stockpiles in 64 bits | 1 day |
| 1 | The catalogue: buildings, research, ships, defences, energy | 7 to 9 agent-days |
| 2 | The fleet and the fight: counts, lasting damage, OGame's round, repair, refit, the round report | 6 to 7 |
| 3 | Flight, pursuit, pirates that heal, the disengage rule | 3 |
| 4 | Raids fought as battles, at home and at the outpost | 2 to 3 |
| 5 | The windshield for hundreds of ships, the battle view | 3 to 4 |

**Eleven questions for the owner** are in section 14, each with the default that
will be built if it is not answered.

---

## 2. What the owner has decided

From `docs/design/decisions.md`, 2026-10-13, and three messages relayed the same
day. His words are quoted; the numbering is this document's.

The commission:

> i think i would like to actually do exact ships, defense, buildings, and
> research as ogame. identical and ship to ship and ship to defense interactions
> (which ones get bonus hits on others etc). (i enivsion fleets of hundreds of
> ships so we would need to make windsheild UI be representative, not literaly for
> number of ships. (maybe starting a fight opens a modal showing actual ships and
> firings? idk). i actually quite like ships getting refit when they get home, not
> automatically in the field (but if its a problem, we can make it easy and an
> upgrade works game wide. whatever). it would be nice if fleets could use the
> dueterium they collect/salvage/mine from their cargo and put in their fuel so
> they can fly longer (maybe have an option to 'clean duet and refuel' which
> allows cargo duet to refuel ships but at a cost like 15% what do you think?)

The first answers:

> ogames original/classic is what im after. and they use generic names so we can
> copy them. if we can do per round would be more fun imo while at windshield as
> we do now (ogames instant battles are less fun watch bcuz theyre instant). and
> if we do per round firing, can we have the ability to run to next hex as a fleet
> to flee (more dangerous enemies will chase a hex or two?) this also give
> penalties to slower ships (users may make large, fast ship fleets for guerilla
> warfare type battling).

The second answers:

> yes i would have penalties for fleeing, the ships will be damaged until home.
> sheild recharge at whatever rate is. i would have pirates heal over time slowly,
> so its an active guerilla warfare, not just 'fight them once per day for a
> week'. pirates should heal over a few hours maybe? or less, maybe an hour? these
> are all knobs we'll need to work on in balancing. i want buildings, ships,
> defenses, research to be like ogame, but our current gameplay and other setup i
> really like. we will keep everything as it is, just migrate the buildings,
> shipyard, research and such to the ogame stuff

What follows from them, and is not reopened below:

1. **Version.** Classic OGame. No lifeforms, no player classes.
2. **Names.** OGame's own names are used.
3. **A fight is fought a round at a time**, watched from the windshield as fights
   are today, and not resolved at once.
4. **A fleet may flee to the next hex, and more dangerous enemies chase it a hex
   or two.** Speed is to matter. A fleet of many fast ships that strikes and
   leaves is a way to play he wants.
5. **The catalogue changes, not the game.** Section 1 says what stays.
6. **Fleeing has a price.**
7. **Damage lasts until the fleet is home.** A known departure from OGame.
8. **Shields recharge at OGame's rate**: in full, at the start of every round.
9. **Pirate groups heal slowly**, over roughly one to a few hours, so that a group
   worn down has to be pressed. The figure is a knob.
10. **Fleets are to be hundreds of ships**, so the windshield shows a fleet and not
    each hull.
11. **Refit at home is liked**; research that works everywhere at once is accepted
    if refit is a problem. Section 9 recommends refit.
12. **Fuel from the hold at a loss "like 15%"** is a proposal an opinion was asked
    on. Section 8.5 gives it.

"These are all knobs" is taken at its word. Where this document gives a number of
its own, it gives a constant, a starting value and the trade.

---

## 3. Which OGame, and where its numbers come from

**The version.** OGame 0.84, the last of the old design, of about 2008. It is what
"classic" names: fourteen ships with the Battlecruiser the newest, eight defences
and two missiles, eighteen buildings, sixteen researches, no officers' bonuses
taken, nothing of the later redesign (Astrophysics, the storage formula of 2011,
the cheaper Ion Cannon), nothing of version 7 (Reaper, Pathfinder, Crawler, player
classes) and no lifeforms. It is also the one version with a complete open
reference to check a number against.

**Sources.**

- **S1.** `ogamespec/ogame-opensource` on GitHub, commit `efffece` (2026-09-04),
  which describes itself as "a faithful recreation of the classic OGame 0.84" in
  which "all gameplay mechanics, resource costs, production timings" are preserved.
  Read: `game/core/techs.php` (costs, growth factors, unit parameters, rapid fire,
  requirements), `game/core/prod.php` (production, energy, storage, durations),
  `game/core/defs.php` (the duration divisors), `game/battle/battle.cpp` and
  `battle.h` (the battle), `game/core/battle.php` (defence repair, debris,
  plunder, moon chance), `game/core/fleet.php` (speed, fuel, flight time, fleet
  slots), `game/install.php` (a universe's defaults). It is a reconstruction, not
  Gameforge's code. It is the source of every table in section 6 unless a line
  says otherwise.
- **S2.** `alaingilbert/ogame` on GitHub, branch `master`, a Go library for the
  live game of today (version 7 and later). Read: the file of each classic ship,
  defence, building and research under `pkg/ogame/`, and `baseBuilding.go`,
  `baseTechnology.go`, `baseShip.go`, `baseDefender.go`. It was written by other
  people from the live game, so where it agrees with S1 the number is confirmed by
  two independent readings. Where it differs, the difference is a later change to
  OGame and S1 is the classic figure.
- **The OGame wiki** (`ogame.fandom.com`) could not be read: it refused every
  request made to it from here (HTTP 402). A web search confirmed from other
  pages only the widely known outline: six rounds, 70 percent of destroyed
  defences rebuilt, 30 percent of destroyed ships to debris, the 1 percent bounce
  and the 70 percent hull rule. It gave no table.

**How well the two agree.** For the thirteen ships and eight defences taken:
structure, shield, attack, cargo, base speed, cost and requirements are the same
in S1 and S2 for every one, and so is every rapid-fire figure between classic
units. The differences are these, and in each the classic figure (S1) is used:

| Item | Classic (S1), used | Later (S2) |
|---|---|---|
| Bomber, fuel | 1,000 | 700 |
| Ion Cannon, cost | 2,000 metal, 6,000 crystal | 5,000 metal, 3,000 crystal |
| Metal Storage, level 1 | 2,000 metal | 1,000 metal |
| Crystal Storage, level 1 | 2,000 metal, 1,000 crystal | 1,000 metal, 500 crystal |
| Deuterium Tank, level 1 | 2,000 metal, 2,000 crystal | 1,000 metal, 1,000 crystal |
| Storage capacity | `100,000 + 50,000 x ceil(1.6^L - 1)` | `5,000 x floor(2.5 x e^(20L/33))` |
| Deuterium Synthesizer | `x (1.28 - 0.002 x Tmax)` | `x (1.36 - 0.004 x Tavg)` |
| Natural production | 20 metal, 10 crystal an hour | 30 metal, 15 crystal |
| Solar Satellite | `floor(Tmax / 4 + 20)` energy | `floor((Tavg + 160) / 6)` |
| Light Laser, requirements | Shipyard 2, Energy 1, Laser 3 | Shipyard 2, Laser 3 |
| Plasma Technology | a requirement only | also raises mine output |
| Hyperspace Technology | a requirement only | also enlarges holds |
| Building time, first levels | the plain formula | shortened below level 6 |

Building, research and unit cost at level 1, every growth factor, every
requirement, the mine and energy formulas and the three duration formulas are the
same in both.

**The battle** rests on S1 alone, and on the outline everyone agrees on. S2 has no
battle. Section 7.1 marks the three details of it that only S1 gives.

---

## 4. The game as it is: what the code says

Eight facts, all *measured*, that the rest of the document leans on.

1. **A fleet is a fixed array of ship records.** `Fleet.ships` is
   `[Ship; MAX_SHIPS_PER_FLEET]` with `MAX_SHIPS_PER_FLEET = 64`, and a `Ship` is
   an id, a class, `hull`, `hull_max`, `shield`, `shield_max`, `weapon_power` and
   `speed` (`sim/src/engine.rs`). An NPC group is the same with 32
   (`NPC_MAX_SHIPS`). Research is stamped into those numbers when the ship is
   built (`add_ship_to_homeworld` calls `scaling::apply_research_to_stats`) and
   never again, which is why hull, shield and weapon research does not reach ships
   already built. A player may have 3 fleets away from home and 8 in all.
2. **A fight is one round a tick, to the death.** `process_combat` calls
   `combat::resolve_combat_round` once a tick for each fight. All the players'
   ships fire first and the damage lands at once; NPC ships killed in that volley
   do not fire back. A shot picks its target by weight of `hull_max`
   (`select_target`, which walks the whole list for every shot), does 0.8 to 1.2
   times the weapon's power, and is taken off the shield before the hull. There is
   no round limit and no draw. Every shot is one `CombatRound` event on the wire.
3. **Hull is never repaired; shields come back when the fleet is idle.**
   `process_cooldowns` gives a fleet that has been idle for 10 ticks a tenth of
   its shield a tick. Nothing anywhere adds to `hull`. So this game already has
   damage that lasts, and has no place that mends it.
4. **A fleet in a fight cannot leave it by jumping.** `handle_move` refuses
   (`OnCooldown`) while the state is `InCombat`. The one way out is `recall`, the
   emergency jump home: twice the fuel of the hexes home, and each ship rolls a
   chance of 2 percent a hex (60 at most, less 5 a level of Emergency Jump) to
   lose 20 to 80 percent of its full hull (`handle_recall`). When the last fleet
   leaves, the group is removed and the sector's template stands again, whole
   (`break_off_combat`).
5. **A raid is not a fight.** `resolve_raid` compares one number with another: the
   raid's power against `home_defense_power` (docked ships, the Defense Grid's
   virtual scouts, the structures) times a roll of 0.9 to 1.1. A lost raid
   destroys a quarter of the docked ships and half the structures; a repelled one
   knocks out structures of which 70 percent return after a delay. No defence
   structure ever fires at a ship, and no research touches one.
6. **The odds are a second model of the fight.** `sim/src/odds.rs` plays
   `combat.rs` on averages, class stack against class stack, and
   `scaling::outcome_ratio` reads the result off a table of sixteen measured
   points (`RATIO_MARGINS`). Standing orders, previews and bots all read that one
   ratio.
7. **Every stock of resources is a 32-bit float.** `Resources` is three `f32`
   (`shared/src/constants.rs`): the stockpile, a hold, an outpost's store, a cost.
8. **The wire carries every ship.** `FleetState.ships` is a `Vec<ShipState>` and
   `TickUpdate.fleets` is the whole fleet list, sent every tick
   (`shared/src/protocol.rs`, `sim/src/views.rs`, `collect_player_fleets`).
   `Split` names ships by id. The web client already folds those ships into one
   group a class before it draws them and then spreads each group back out over
   eight formation slots (`lib/state/state_mapper.dart`,
   `lib/scenes/windshield/sim.dart`, `_formation`); the TUI lists them one to a
   row.

How this game's numbers stand beside OGame's today. The two are closer than they
look, because this game's mines were modelled on OGame's.

| | This game | OGame classic |
|---|---|---|
| Metal Mine, level 1 cost and growth | 60 metal, 15 crystal, x1.5 | the same |
| Crystal Mine | 48 / 24, x1.6 | the same |
| Metal an hour at level L, pace or speed 1 | `10 x L x 1.1^L` | `30 x L x 1.1^L`, and 20 free |
| Building time | cost / 700 an hour | cost / 2,500 an hour |
| Cheapest warship | Corvette, 560 | Light Fighter, 4,000 |
| Heaviest ship | Cruiser, 5,300 | Deathstar, 10,000,000 |
| Weapon against own shield | Scout 5 against 10 | Light Fighter 50 against 10 |
| Rapid fire | 3 pairs, x2 or x3 | 54 pairs, x2 to x1,250 |

The row before last matters most. This game's ships carry shields twice their own
weapon; OGame's carry weapons five times their own shield, because OGame's shields
come back every round. Two of this game's Scouts fighting two others under OGame's
round could never scratch a hull. That is why the fight cannot change before the
ships do (section 12).

---

## 5. The catalogue, item by item

Each OGame item is one of three things. **Plain**: it has a place in this game as
it stands and does here what it does in OGame. **Takes over**: it does here the job
of something this game already has, which is named. **Left out**: it has no place.
The reason is given in a line the owner can accept or strike.

### 5.1 Buildings

| OGame building | Fate | Here |
|---|---|---|
| Metal Mine | Plain | Replaces this game's Metal Mine. Same cost; three times the output. |
| Crystal Mine | Plain | Replaces the Crystal Mine. |
| Deuterium Synthesizer | Plain | Replaces the Deuterium Synthesizer. OGame's output depends on the planet's temperature; a homeworld here has none, so every home is given the same one (`HOME_TEMPERATURE_MAX`, a knob). |
| Solar Plant | Plain, if energy comes in | Powers the mines. Section 5.6, question 1. |
| Fusion Reactor | Plain, if energy comes in | Power for deuterium. |
| Robotics Factory | Takes over: the Fabricator | Divides building time by `1 + level`. |
| Nanite Factory | Plain | Halves building, ship and defence time a level. Nothing here does that today. |
| Shipyard | Plain | Replaces the Shipyard. Divides ship and defence time by `1 + level`, and is what most ships and defences require. |
| Metal Storage | Takes over: the Storage Vault, for metal | The cap on the metal stockpile, and the share of it a raid cannot take. |
| Crystal Storage | Takes over: the Storage Vault, for crystal | The same for crystal. |
| Deuterium Tank | Takes over: the Storage Vault, for deuterium, and the Fuel Depot | The same for deuterium. It also enlarges every fleet's tank, as the Fuel Depot does today. Question 2. |
| Research Lab | Plain | Replaces the Research Lab. Divides research time by `1 + level`. |
| Terraformer | Left out | It adds planet fields. There are none. |
| Alliance Depot | Left out | It fuels allied fleets in orbit. There are no alliances. |
| Missile Silo | Left out | It holds missiles, which are left out. |
| Lunar Base, Sensor Phalanx, Jump Gate | Left out | Moon buildings. There are no moons. |

### 5.2 Research

| OGame research | Fate | Here |
|---|---|---|
| Weapons Technology | Takes over: Weapons Research | +10 percent attack a level, ships and defences alike. No cap of five levels. |
| Shielding Technology | Takes over: Advanced Shields | +10 percent shield a level. |
| Armour Technology | Takes over: Reinforced Hulls | +10 percent hull a level. It no longer raises what a jump burns (section 8.4). |
| Combustion Drive | Takes over: Navigation, for its ships | +10 percent speed a level for Small and Large Cargo, Light Fighter, Recycler and Espionage Probe. Speed sets how many ticks a jump takes (section 5.7). |
| Impulse Drive | Takes over: Navigation, for its ships | +20 percent a level for Heavy Fighter, Cruiser and Bomber, and for the Small Cargo from level 5. |
| Hyperspace Drive | Takes over: Navigation, for its ships | +30 percent a level for Battleship, Battlecruiser, Destroyer and Deathstar, and for the Bomber from level 8. |
| Energy Technology | Takes over: Fuel Efficiency | A requirement of much else, as in OGame, and it raises the Fusion Reactor's output if energy comes in. Here each level also takes a share off what a jump burns. Question 2. |
| Hyperspace Technology | Takes over: Emergency Jump | A requirement only in classic OGame. Here each level lowers the chance that the emergency jump home harms a ship. |
| Computer Technology | Takes over: Modular Fabrication | In OGame it adds fleet slots, which are not brought in. Here two of its levels each open one more building slot. It is also required by the Nanite Factory. |
| Espionage Technology | Takes over: the Sensor Array | In OGame it sharpens espionage reports, which are not brought in. Here its level is how many hops around the homeworld are seen, which the Sensor Array's level is today. Required by the Espionage Probe. |
| Laser Technology | Takes over: Harvesting Efficiency | A requirement only in OGame. Here each of its first five levels raises what a unit of ore yields. Question 2. |
| Ion Technology | Plain | A requirement only, as in OGame: of the Cruiser, the Ion Cannon and Plasma Technology. |
| Plasma Technology | Plain | A requirement only, as in classic OGame: of the Bomber and the Plasma Turret. |
| Graviton Technology | Plain, if energy comes in | Required by the Deathstar. It costs 300,000 energy and nothing else, so it has no price without energy. |
| Intergalactic Research Network | Left out | It joins the labs of several planets. There is one lab. |
| Expedition Technology | Left out | It sets how many expeditions may fly. Derelicts are this game's and have no such limit. |

### 5.3 Ships

| OGame ship | Fate | Here |
|---|---|---|
| Light Fighter | Plain | The cheapest warship. Takes the Corvette's place at the bottom of the line, and the Scout's as what a new empire starts with (`STARTING_SHIPS`, a knob). |
| Heavy Fighter | Plain | |
| Cruiser | Plain | Rapid fire against Light Fighters and Rocket Launchers. The fastest warship. |
| Battleship | Plain | |
| Battlecruiser | Plain | Rapid fire against Cruisers, Battleships, Heavy Fighters and cargo ships. |
| Bomber | Plain | Rapid fire against four of the eight defences. No player fires at a defence until there is PvP, so until then it is a slow, heavy warship in a player's hands and what pirates fly at a player's defences (section 7.6). |
| Destroyer | Plain | Rapid fire against Battlecruisers and Light Lasers. |
| Deathstar | Plain, if energy comes in | The end of the line. Its moon-destroying mission is left out. |
| Small Cargo | Takes over: the Hauler's hold | 5,000 units. Changes to the Impulse Drive at level 5, twice as fast and twice the fuel, as in OGame. |
| Large Cargo | Takes over: the Hauler's hold | 25,000 units. |
| Recycler | Takes over: the Hauler's mining rig | In OGame it collects debris fields, which are not brought in. Here it is the ship that mines: the harvest power the Hauler has today, and the rigging that doubles what a boarding party strips from a derelict. Wreckage is still collected by any hold. |
| Espionage Probe | Takes over: the Scout's eyes | In OGame it spies. Here it is what a fleet carries to scan two hops instead of one and to read a derelict before boarding, as a Scout does today. It has no weapon and no shield. |
| Solar Satellite | Plain, if energy comes in | Energy. It stays at the homeworld, cannot be put in a fleet, and is shot at in a raid like any ship. |
| Colony Ship | Left out | It founds a colony and is consumed. Colonies are not brought in, and an outpost is founded from what a fleet carries (the built answer to question 2 of `outposts.md`, which the owner has not given). If he ever wants a ship built for founding, this is its name and its price. |

### 5.4 Defences

| OGame defence | Fate | Here |
|---|---|---|
| Rocket Launcher | Plain | Takes the Pulse Turret's place. |
| Light Laser | Plain | |
| Heavy Laser | Plain | Takes the Lancer Battery's place. |
| Gauss Cannon | Plain | |
| Ion Cannon | Plain | Little attack, a large shield. |
| Plasma Turret | Plain | Takes the Ion Bastion's place at the top. |
| Small Shield Dome | Takes over: the Defense Grid's cover | One only. A unit of 2,000 shield that soaks shots. |
| Large Shield Dome | Takes over: the Defense Grid's cover | One only. 10,000 shield. |
| Anti-Ballistic Missiles, Interplanetary Missiles | Left out | Missiles are not brought in. |

All eight are built in the shipyard queue as defences are today, take Weapons,
Shielding and Armour research as ships do, and are rebuilt at OGame's rate after a
fight (section 7.6).

### 5.5 This game's buildings and research: the fate of each

| Today | What it does (measured, `shared/src/scaling.rs`) | Fate |
|---|---|---|
| Metal Mine, Crystal Mine, Deuterium Synthesizer | Production | Replaced by OGame's of the same name |
| Shipyard | Ship time / `(1 + 0.1 L)`; required by ships | Replaced by OGame's Shipyard: `1 + L` |
| Research Lab | Research time / `(1 + 0.1 L)` | Replaced by OGame's Research Lab: `1 + L` |
| Fabricator | Building time / `(1 + 0.15 L)` | Replaced by the Robotics Factory: `1 + L` |
| Storage Vault | All three caps x1.5 a level; 8 percent a level safe from raids | Its job is split among Metal Storage, Crystal Storage and Deuterium Tank, each for its own resource |
| Fuel Depot | Tank x `(1 + 0.25 L)` | Folded into the Deuterium Tank (`TANK_PER_DEUT_TANK_LEVEL`) |
| Sensor Array | Sectors within L hops of home are seen | Folded into Espionage Technology |
| Defense Grid | A few virtual scouts; +4 percent a level to structures; required by structures; makes an empire worth raiding | Dropped. OGame's defences require the Shipyard and research; the three combat researches strengthen them; the domes are the cover. An empire is worth raiding once it has Shipyard 2 or any defence |
| Fuel Efficiency | Burn -10 percent a level, 5 levels | Folded into Energy Technology (`FUEL_SAVING_PER_LEVEL`) |
| Extended Tanks | Tank +15 percent a level, 5 levels | Dropped as a separate thing. It and the Fuel Depot both multiply the tank; the Deuterium Tank carries both |
| Navigation | Jump charge -1 tick a level; levels 3 to 5 do nothing | Replaced by the three drives |
| Reinforced Hulls, Advanced Shields, Weapons Research | +10 percent a level, stamped at build | Replaced by Armour, Shielding and Weapons Technology |
| Harvesting Efficiency | Ore yield +20 percent a level, 5 levels | Folded into Laser Technology, first five levels (`HARVEST_YIELD_PER_LEVEL`) |
| Corvette, Frigate, Cruiser and Hauler Tech | Each unlocks a class | Dropped. OGame's requirements unlock ships |
| Emergency Jump | Recall's harm -5 points a level, 3 levels | Folded into Hyperspace Technology (`RECALL_SAVING_PER_LEVEL`) |
| Modular Fabrication | One more building slot a level, 2 levels | Folded into Computer Technology (`BUILD_SLOT_LEVELS`) |

**Why fold and not keep.** The owner's sentence is "just migrate the buildings,
shipyard, research and such to the ogame stuff", and a research screen with
OGame's sixteen and four of this game's beside them is not that. Each fold puts
the job on an OGame item that would otherwise do nothing here: Hyperspace, Laser
and Energy Technology (without energy) are bare requirements in classic OGame, and
Computer and Espionage Technology do things in OGame that are not brought in. The
price of folding is that these five OGame researches do something here that they
do not do in OGame, and that their OGame cost, not this game's, now buys the job.
Three of the folds are a close fit (Espionage for seeing, Hyperspace for the
emergency jump, the drives for speed). Three are looser: Energy Technology for
fuel, the Deuterium Tank for the fleet's tank, Laser Technology for ore. Question
2 offers the other course for those three.

**What the folds cost in resources**, since the job is the same and the price is
not:

| Job | Today, all levels | In the OGame item |
|---|---|---|
| Half off the burn | Fuel Efficiency 1 to 5: about 6,300 | Energy Technology 1 to 10: 1,227,600. At 5 percent a level, level 5 gives a quarter off for 37,200 |
| Tank x1.75 | Extended Tanks 1 to 5: about 5,900 (the Fuel Depot, at 300 x1.7 a level, adds its own) | Deuterium Tank at 4,000 x2: levels 1 to 3 are 28,000 for x1.75 |
| Sight at 4 hops | Sensor Array 1 to 4: about 3,400 | Espionage 1 to 4: 21,000 |
| Ore yield x2 | Harvesting 1 to 5: about 5,100 | Laser 1 to 5: 9,300 |
| Two more building slots | Modular Fabrication 1 to 2: 9,600 | Computer 3 and 6: 7,000, then 56,000 more |

OGame's income at a given mine level is three times this game's, so a cost three
times higher is the same wait. Energy Technology is the outlier. Each of these is
a knob (how much a level gives), not a reason to choose differently.

### 5.6 Energy

**What it is in OGame** (*sourced*, S1 `prod.php`). Each mine needs energy:
`ceil(10 x L x 1.1^L)` for the Metal and the Crystal Mine, twice that for the
Deuterium Synthesizer. The Solar Plant makes `floor(20 x L x 1.1^L)`, the Fusion
Reactor `floor(30 x L x (1.05 + 0.01 x Energy Technology)^L)` while burning
`ceil(10 x L x 1.1^L)` deuterium an hour, and a Solar Satellite
`floor(Tmax / 4 + 20)`. If less is made than is needed, all three mines produce
the same fraction of their output: made over needed. Energy is not stored.

**What it costs an OGame player.** Mines at level L each, powered by a Solar Plant
alone (worked out from the formulas above; no satellites, no fusion):

| Mines at | Mines cost, all three | Solar Plant level needed | Solar Plant cost | Energy, as a share of the mines |
|---|---|---|---|---|
| 5 | 6,100 | 8 | 5,200 | 85 percent |
| 10 | 55,600 | 14 | 61,100 | 110 percent |
| 15 | 465,900 | 20 | 698,100 | 150 percent |
| 20 | 3,943,800 | 25 | 5,302,500 | 134 percent |

So in OGame about half of what a mine economy costs is its power. Satellites and
fusion make the upper levels cheaper than this table, at the price of crystal,
deuterium and exposure to attack.

**The two courses.**

- **Energy comes in with the mines.** Two buildings, one ship that stays home, one
  research with an effect, and the Deathstar's requirement all get their OGame
  job. Every production number is OGame's exactly, cost and output. The engine
  gains little: `Player::production_per_tick` multiplies by one fraction. What it
  adds to play is one more thing the home has to keep fed, satellites that a raid
  can shoot down, and a third use for deuterium beside fuel and costs. The state
  gains three numbers (made, needed, the fraction), every client shows them, the
  bots' home planner (`sim/src/bot/home.rs`, `next_building`) needs one rule
  (build power when the fraction is under 1), and `sim.py` has to model it.
- **The mines run without it.** Each mine level makes exactly what OGame's does,
  but an empire pays about half what an OGame player pays to get there, so the
  economy runs roughly twice as rich for the same spending. That is not a fault a
  player would see: the pace multiplier sets the tempo anyway, and the presets
  have to be estimated again whichever course is taken. What is lost is five
  catalogue items with nothing to do: the Solar Plant and the Fusion Reactor are
  left out, the Solar Satellite is left out, Energy Technology is a requirement
  and the fuel fold only, and Graviton has no price, so it and the Deathstar are
  left out or given a price of this game's invention.

**Recommended: energy comes in, in the same slice as the mines.** The Solar Plant
is among the first three things anyone builds in OGame; a building list without it
is visibly not OGame's, which is what was asked for. Bringing it in later would
move every production figure a second time. Question 1.

### 5.7 What stays this game's and needs a number for each OGame ship

The systems that stay ask things of a ship that OGame's table does not answer.
Each is a knob: a column of this game's own beside OGame's.

**Tank.** OGame has no tank; a flight is paid for when it leaves. Here the tank
stays. What a jump burns becomes OGame's fuel figure for the class (section 8.4),
so the one missing number is how many jumps a full tank holds. Starting values,
set to keep today's spread (a Scout's tank holds 20 jumps today, a Corvette's 10,
a Hauler's 12, a Cruiser's 6, before research):

| Class | `TANK_JUMPS` | Class | `TANK_JUMPS` |
|---|---|---|---|
| Espionage Probe | 20 | Battleship | 6 |
| Small Cargo, Large Cargo, Recycler | 12 | Battlecruiser | 8 |
| Light Fighter, Heavy Fighter | 10 | Bomber, Destroyer | 6 |
| Cruiser | 8 | Deathstar | 6 |

A class's tank is its `TANK_JUMPS` times its burn. The Deuterium Tank building
multiplies it. It trades reach against the worth of an outpost as a forward dock
and of deuterium carried in a hold (section 8.5).

**Speed, and the ticks a jump takes.** This is the number flight turns on. OGame
gives each class a base speed and each drive level a share more, and its flight
time goes with the inverse square root of speed (S1 `fleet.php`, `FlightTime`).
The same law, for one hop:

```
jump ticks = max(1, ceil(sqrt(JUMP_SPEED_REF / speed)))
speed      = base speed x (1 + 0.1, 0.2 or 0.3 x the level of the ship's drive)
a fleet jumps at the speed of its slowest ship
```

With `JUMP_SPEED_REF = 50,000`, a starting value chosen so that the fastest
warships jump in today's 2 ticks:

| Class | Base speed | Ticks at base | With the drive level it is first built at | Late |
|---|---|---|---|---|
| Espionage Probe | 100,000,000 | 1 | 1 | 1 |
| Cruiser | 15,000 | 2 | 2 (Impulse 4) | 1 at Impulse 12 |
| Light Fighter | 12,500 | 2 | 2 | 2 |
| Heavy Fighter | 10,000 | 3 | 2 (Impulse 2) | 2 |
| Battleship | 10,000 | 3 | 2 (Hyperspace Drive 4) | 1 at Hyperspace Drive 14 |
| Battlecruiser | 10,000 | 3 | 2 (Hyperspace Drive 5) | 1 at Hyperspace Drive 14 |
| Large Cargo | 7,500 | 3 | 3 (Combustion 6) | 2 from Combustion 7 |
| Small Cargo | 5,000 | 4 | 3 (Combustion 2) | 2 from Impulse 5 |
| Destroyer | 5,000 | 4 | 2 (Hyperspace Drive 6) | 2 |
| Bomber | 4,000 | 4 | 3 (Impulse 6) | 2 from Hyperspace Drive 8 |
| Recycler | 2,000 | 5 | 4 (Combustion 6) | 4 |
| Deathstar | 100 | 23 | 13 (Hyperspace Drive 7) | 11 at Hyperspace Drive 12 |

A jump takes 2 or 3 ticks today (`GameEngine::hop_ticks`), so travel keeps its
tempo. A higher `JUMP_SPEED_REF` spreads the classes further apart and makes
every trip longer: at 100,000 the table reads 3 to 5 ticks for warships and 8 for
the Recycler. It trades how much speed matters in flight against how long a trip
takes.

**Mining, scanning, boarding.** Today a Hauler has harvest power 5, a Scout 1 and
the rest 0.5 (`fleet_harvest_power`); a Scout aboard widens a scan to 2 hops and
lowers a derelict's ambush chance by 10 points; a Hauler aboard doubles what a
boarding strips. Starting values in OGame's classes: harvest power Recycler 5,
Small and Large Cargo 1, everything else 0.5; the Espionage Probe is the Scout
for scans and for reading a derelict; the Recycler is the Hauler for stripping
one.

**The starting fleet.** Two Scouts today (`STARTING_SCOUTS`). OGame starts with no
ships. A new empire needs something that can fly, chart and win the smallest
fights. Starting value: two Light Fighters. That is 8,000 resources of ships
where today's two Scouts are 560; one Light Fighter, or one and a Probe, are the
other settings.

---

## 6. The numbers

All *sourced* from S1 (`game/core/techs.php` unless a line names another file),
and the same in S2 except where section 3 lists a difference. Costs are metal /
crystal / deuterium. Items left out are listed too, in italics, so the tables are
OGame's whole.

### 6.1 Ships

| Ship | Cost | Structure | Shield | Attack | Cargo | Speed | Fuel | Drive |
|---|---|---|---|---|---|---|---|---|
| Small Cargo | 2,000 / 2,000 / 0 | 4,000 | 10 | 5 | 5,000 | 5,000 | 10 | Combustion |
| Large Cargo | 6,000 / 6,000 / 0 | 12,000 | 25 | 5 | 25,000 | 7,500 | 50 | Combustion |
| Light Fighter | 3,000 / 1,000 / 0 | 4,000 | 10 | 50 | 50 | 12,500 | 20 | Combustion |
| Heavy Fighter | 6,000 / 4,000 / 0 | 10,000 | 25 | 150 | 100 | 10,000 | 75 | Impulse |
| Cruiser | 20,000 / 7,000 / 2,000 | 27,000 | 50 | 400 | 800 | 15,000 | 300 | Impulse |
| Battleship | 45,000 / 15,000 / 0 | 60,000 | 200 | 1,000 | 1,500 | 10,000 | 500 | Hyperspace |
| *Colony Ship* | 10,000 / 20,000 / 10,000 | 30,000 | 100 | 50 | 7,500 | 2,500 | 1,000 | Impulse |
| Recycler | 10,000 / 6,000 / 2,000 | 16,000 | 10 | 1 | 20,000 | 2,000 | 300 | Combustion |
| Espionage Probe | 0 / 1,000 / 0 | 1,000 | 0 | 0 | 5 | 100,000,000 | 1 | Combustion |
| Bomber | 50,000 / 25,000 / 15,000 | 75,000 | 500 | 1,000 | 500 | 4,000 | 1,000 | Impulse |
| Solar Satellite | 0 / 2,000 / 500 | 2,000 | 1 | 1 | 0 | 0 | 0 | none |
| Destroyer | 60,000 / 50,000 / 15,000 | 110,000 | 500 | 2,000 | 2,000 | 5,000 | 1,000 | Hyperspace |
| Deathstar | 5,000,000 / 4,000,000 / 1,000,000 | 9,000,000 | 50,000 | 200,000 | 1,000,000 | 100 | 1 | Hyperspace |
| Battlecruiser | 30,000 / 40,000 / 15,000 | 70,000 | 400 | 700 | 750 | 10,000 | 250 | Hyperspace |

Two ships change drive (S1 `fleet.php`, `FleetSpeed` and `FleetCons`; S2 agrees).
The Small Cargo, from Impulse Drive 5, has a base speed of 10,000 on the Impulse
Drive and burns 20. The Bomber, from Hyperspace Drive 8, has a base speed of 5,000
on the Hyperspace Drive; its fuel does not change.

In a fight a unit's hull is a tenth of its structure. The structure is also the
sum of the metal and crystal it cost, for every unit.

### 6.2 Defences

| Defence | Cost | Structure | Shield | Attack |
|---|---|---|---|---|
| Rocket Launcher | 2,000 / 0 / 0 | 2,000 | 20 | 80 |
| Light Laser | 1,500 / 500 / 0 | 2,000 | 25 | 100 |
| Heavy Laser | 6,000 / 2,000 / 0 | 8,000 | 100 | 250 |
| Gauss Cannon | 20,000 / 15,000 / 2,000 | 35,000 | 200 | 1,100 |
| Ion Cannon | 2,000 / 6,000 / 0 | 8,000 | 500 | 150 |
| Plasma Turret | 50,000 / 50,000 / 30,000 | 100,000 | 300 | 3,000 |
| Small Shield Dome | 10,000 / 10,000 / 0 | 20,000 | 2,000 | 1 |
| Large Shield Dome | 50,000 / 50,000 / 0 | 100,000 | 10,000 | 1 |
| *Anti-Ballistic Missile* | 8,000 / 0 / 2,000 | 8,000 | 1 | 1 |
| *Interplanetary Missile* | 12,500 / 2,500 / 10,000 | 15,000 | 1 | 12,000 |

No defence has rapid fire. At most one dome of each kind may be built (S1
`game/core/queue.php`).

### 6.3 Rapid fire

A figure r means: after a shot at a unit of that kind, the firing unit fires
again with probability (r - 1) / r, at a new random target.

Every ship except the Espionage Probe and the Solar Satellite has rapid fire 5
against the Espionage Probe and 5 against the Solar Satellite. The Deathstar has
1,250 against both. Beyond that:

| Firing | Against |
|---|---|
| Heavy Fighter | Small Cargo 3 |
| Cruiser | Light Fighter 6, Rocket Launcher 10 |
| Bomber | Rocket Launcher 20, Light Laser 20, Heavy Laser 10, Ion Cannon 10 |
| Destroyer | Battlecruiser 2, Light Laser 10 |
| Battlecruiser | Small Cargo 3, Large Cargo 3, Heavy Fighter 4, Cruiser 4, Battleship 7 |
| Deathstar | Small Cargo 250, Large Cargo 250, Light Fighter 200, Heavy Fighter 100, Cruiser 33, Battleship 30, Colony Ship 250, Recycler 250, Bomber 25, Destroyer 5, Battlecruiser 15, Rocket Launcher 200, Light Laser 200, Heavy Laser 100, Gauss Cannon 50, Ion Cannon 100 |

Small Cargo, Large Cargo, Light Fighter, Battleship, Colony Ship and Recycler have
only the 5 against Probe and Satellite. Nothing has rapid fire against the Plasma
Turret, the domes or the Deathstar.

### 6.4 Buildings

The cost of level L is the level 1 cost times the factor to the power L - 1.

| Building | Level 1 | Factor | Requires |
|---|---|---|---|
| Metal Mine | 60 / 15 / 0 | 1.5 | |
| Crystal Mine | 48 / 24 / 0 | 1.6 | |
| Deuterium Synthesizer | 225 / 75 / 0 | 1.5 | |
| Solar Plant | 75 / 30 / 0 | 1.5 | |
| Fusion Reactor | 900 / 360 / 180 | 1.8 | Deuterium Synthesizer 5, Energy Technology 3 |
| Robotics Factory | 400 / 120 / 200 | 2 | |
| Nanite Factory | 1,000,000 / 500,000 / 100,000 | 2 | Robotics Factory 10, Computer Technology 10 |
| Shipyard | 400 / 200 / 100 | 2 | Robotics Factory 2 |
| Metal Storage | 2,000 / 0 / 0 | 2 | |
| Crystal Storage | 2,000 / 1,000 / 0 | 2 | |
| Deuterium Tank | 2,000 / 2,000 / 0 | 2 | |
| Research Lab | 200 / 400 / 200 | 2 | |
| *Terraformer* | 0 / 50,000 / 100,000 and 1,000 energy | 2 | Nanite Factory 1, Energy Technology 12 |
| *Alliance Depot* | 20,000 / 40,000 / 0 | 2 | |
| *Missile Silo* | 20,000 / 20,000 / 1,000 | 2 | Shipyard 1 |
| *Lunar Base* | 20,000 / 40,000 / 20,000 | 2 | a moon |
| *Sensor Phalanx* | 20,000 / 40,000 / 20,000 | 2 | Lunar Base 1 |
| *Jump Gate* | 2,000,000 / 4,000,000 / 2,000,000 | 2 | Lunar Base 1, Hyperspace Technology 7 |

How a cost that is not a whole number is rounded (the Metal Mine's crystal at
level 2 is 22.5) is *unconfirmed*: S1 keeps the fraction and S2 cuts it off. This game rounds to the nearest whole unit today
(`grown_cost`), and nothing is gained by changing that.

**Production an hour**, at universe speed 1, which is this game's pace 1 (S1
`prod.php`):

```
metal      floor(30 x L x 1.1^L) x factor  +  20
crystal    floor(20 x L x 1.1^L) x factor  +  10
deuterium  floor(10 x L x 1.1^L) x (1.28 - 0.002 x Tmax) x factor
factor     min(1, energy made / energy needed)
```

The 20 and the 10 are free and need no mine. Tmax is the planet's highest
temperature.

**Energy.**

```
needed   ceil(10 x L x 1.1^L)  by the Metal Mine and by the Crystal Mine
         ceil(20 x L x 1.1^L)  by the Deuterium Synthesizer
made     floor(20 x L x 1.1^L)                              Solar Plant
         floor(30 x L x (1.05 + 0.01 x Energy Technology)^L) Fusion Reactor,
             which burns ceil(10 x L x 1.1^L) deuterium an hour
         floor(Tmax / 4 + 20), at least 1                    each Solar Satellite
```

**Storage.** Each of the three storage buildings caps its own resource at

```
100,000 + 50,000 x ceil(1.6^L - 1)
```

which is 100,000 with no building and then 150,000, 200,000, 300,000, 400,000,
600,000, 900,000, 1,400,000, 2,200,000, 3,500,000 and 5,550,000 at levels 1 to 10.
A mine stops at the cap.

**Time**, in hours at speed 1, floored to whole seconds and never under one
(S1 `prod.php`, `TechDuration`; the divisors are in `defs.php`):

```
building          (metal + crystal) / (2,500 x (1 + Robotics Factory)) x 0.5^Nanite Factory
ship or defence   (metal + crystal) / (2,500 x (1 + Shipyard))         x 0.5^Nanite Factory
research          (metal + crystal) / (1,000 x (1 + Research Lab))
```

Deuterium in a cost adds no time. A Metal Mine to level 10 takes 69 minutes with
no Robotics Factory and 11.5 with level 5. A Light Fighter takes 32 minutes at
Shipyard 2, a Cruiser 108 at Shipyard 5, a Battleship 3 hours at Shipyard 7. Pace
divides all of these, as it divides this game's times today.

### 6.5 Research

Every factor is 2, except Graviton's 3.

| Research | Level 1 | Requires | In OGame |
|---|---|---|---|
| Espionage Technology | 200 / 1,000 / 200 | Research Lab 3 | Sharper espionage reports |
| Computer Technology | 0 / 400 / 600 | Research Lab 1 | One more fleet slot a level |
| Weapons Technology | 800 / 200 / 0 | Research Lab 4 | +10 percent attack a level |
| Shielding Technology | 200 / 600 / 0 | Research Lab 6, Energy 3 | +10 percent shield a level |
| Armour Technology | 1,000 / 0 / 0 | Research Lab 2 | +10 percent hull a level |
| Energy Technology | 0 / 800 / 400 | Research Lab 1 | Fusion output; a requirement |
| Hyperspace Technology | 0 / 4,000 / 2,000 | Research Lab 7, Energy 5, Shielding 5 | A requirement |
| Combustion Drive | 400 / 0 / 600 | Energy 1 | +10 percent speed a level |
| Impulse Drive | 2,000 / 4,000 / 600 | Research Lab 2, Energy 1 | +20 percent speed a level |
| Hyperspace Drive | 10,000 / 20,000 / 6,000 | Hyperspace Technology 3 | +30 percent speed a level |
| Laser Technology | 200 / 100 / 0 | Energy 2 | A requirement |
| Ion Technology | 1,000 / 300 / 100 | Research Lab 4, Laser 5, Energy 4 | A requirement |
| Plasma Technology | 2,000 / 4,000 / 1,000 | Energy 8, Laser 10, Ion 5 | A requirement |
| Graviton Technology | 300,000 energy, nothing else | Research Lab 12 | A requirement |
| *Intergalactic Research Network* | 240,000 / 400,000 / 160,000 | Research Lab 10, Computer 8, Hyperspace 8 | Joins labs |
| *Expedition Technology* | 4,000 / 8,000 / 4,000 | Espionage 4, Impulse Drive 3 | More expeditions |

Graviton needs a planet that makes 300,000 energy at the moment it is ordered and
then takes one second: research time is counted from metal and crystal, and it
costs neither.

### 6.6 What each ship and defence requires

| Unit | Requires |
|---|---|
| Light Fighter | Shipyard 1, Combustion Drive 1 |
| Small Cargo | Shipyard 2, Combustion Drive 2 |
| Heavy Fighter | Shipyard 3, Armour 2, Impulse Drive 2 |
| Espionage Probe | Shipyard 3, Combustion Drive 3, Espionage 2 |
| Large Cargo | Shipyard 4, Combustion Drive 6 |
| Recycler | Shipyard 4, Combustion Drive 6, Shielding 2 |
| Cruiser | Shipyard 5, Impulse Drive 4, Ion 2 |
| Battleship | Shipyard 7, Hyperspace Drive 4 |
| Battlecruiser | Shipyard 8, Hyperspace Technology 5, Hyperspace Drive 5, Laser 12 |
| Bomber | Shipyard 8, Impulse Drive 6, Plasma 5 |
| Destroyer | Shipyard 9, Hyperspace Technology 5, Hyperspace Drive 6 |
| Deathstar | Shipyard 12, Hyperspace Technology 6, Hyperspace Drive 7, Graviton 1 |
| Solar Satellite | Shipyard 1 |
| Rocket Launcher | Shipyard 1 |
| Light Laser | Shipyard 2, Energy 1, Laser 3 |
| Heavy Laser | Shipyard 4, Energy 3, Laser 6 |
| Ion Cannon | Shipyard 4, Ion 4 |
| Gauss Cannon | Shipyard 6, Energy 6, Weapons 3, Shielding 1 |
| Plasma Turret | Shipyard 8, Plasma 7 |
| Small Shield Dome | Shipyard 1, Shielding 2 |
| Large Shield Dome | Shipyard 6, Shielding 6 |

---

## 7. The fight

### 7.1 OGame's battle, exactly

*Sourced* from S1, `game/battle/battle.cpp` (`DoBattle`, `UnitShoot`, `RapidFire`,
`CheckFastDraw`) and `game/core/battle.php`.

**Before it.** Each side's research sets each of its units:

```
hull    = structure / 10 x (10 + Armour level) / 10
shield  = shield         x (10 + Shielding level) / 10
attack  = attack         x (10 + Weapons level) / 10      whole numbers, cut off
```

**A round.**

1. Every unit's shield is set to its full value.
2. Every unit fires. The attacker's units fire first and then the defender's, but
   a unit destroyed earlier in the round still fires, so the order gives no
   advantage: fire is simultaneous.
3. A shot picks one enemy unit at random, every unit equally likely, including
   one already destroyed this round (that shot is wasted). Then:
   - If the target's shield stands at zero, the attack comes off its hull.
   - Otherwise the shot is measured in whole hundredths of the target's full
     shield: `n = floor(attack x 100 / full shield)`. If the shield left is less
     than n hundredths, it breaks: what the shield could not take comes off the
     hull, and the shield is zero. If not, the shield loses n hundredths. A shot
     smaller than one hundredth of the full shield has n = 0 and does nothing at
     all. This is the bounce.
   - If the hull is now at or under 70 percent of full and the shield is zero,
     the unit explodes with probability `1 - hull / full hull`, and surely at
     zero.
   - If the firing unit has rapid fire r against the kind of unit it shot at, it
     fires again with probability (r - 1) / r, at a new random target, and so on.
4. Exploded units are removed.
5. If after a round no unit on either side has lost any hull, the battle ends as
   a draw.

**The end.** At most six rounds (`BATTLE_MAX_ROUND`, S1 `defs.php`). The attacker
wins if no defending unit is left,
the defender if no attacking unit is left; anything else is a draw. Every survivor
is whole again. Of the defences destroyed, 70 percent are rebuilt at once: where
fewer than ten of a kind fell each has a 70 percent chance, and where ten or more
fell a share between 60 and 80 percent is rolled for the kind. Thirty percent of
the metal and crystal of every destroyed ship becomes a debris field, a winning
attacker takes half the stockpile as far as its holds reach, and a large debris
field may make a moon. None of the last three is brought in.

**Three details rest on S1 alone** and are *unconfirmed*: that a shield takes
damage in whole hundredths (and not the plain attack once it is over one
hundredth); that a unit rolls to explode only when the shot has left its shield at
zero (the usual wording has it roll after any shot once its hull is under 70
percent); and that a round with no hull damage ends the battle. All three are
recommended as S1 has them. The second matters here more than it does in OGame
(section 7.3).

### 7.2 Spread over ticks

One round is fought each tick, where `process_combat` fights its round today. The
round is section 7.1's. What changes around it:

**Shields** are full at the start of every round and are not stored. Between
rounds, between fights and on the wire there is no shield figure any more, only a
class's shield as a property of the class.

**Hull** carries from round to round, and by decision 7 from fight to fight.
Section 7.3 says where it lives.

**The order of fire** is OGame's: simultaneous. Today the players' ships fire
first and what they kill does not answer (`resolve_combat_round`). That advantage
goes.

**The dice** are seeded each round from the world seed, the tick, the sector and
the round number, as they are today. Nothing of them is stored, and a world
stopped and started between two rounds fights the next one as it would have.

**The six-round limit does not apply in open space.** OGame needs it because its
battle is resolved in an instant and the attacker then flies home. Here either
side may stay or go, and a limit would only be a pause after which the same two
sides stand in the same sector. A fight in a sector goes on, a round a tick,
until one of three things:

1. One side has no unit left.
2. Every fleet of one side has left (section 7.4).
3. A stand-off: `STANDOFF_ROUNDS` rounds in a row (starting value 1, OGame's) in
   which no unit lost hull. This is how two sides that cannot hurt each other
   end, which the bounce and the per-round shield make possible: a Probe has no
   weapon, and three Light Fighters cannot break one Battleship's shield in a
   round. After a stand-off both sides stay where they are, with what they had. Nothing starts the fight again but what starts a fight today: a
   fleet arriving, a group arriving, or an `attack` order.

A raid keeps the limit (section 7.6).

**A fleet arriving in a sector where a fight is on** joins it at the next round,
on its side, as it does today (`start_combat`,
`enroll_all_player_fleets_in_sector_combat`). It brings its own research and its
own damage: OGame gives each participant of a joint attack its own technology,
and S1's battle takes them that way. Another empire's fleet joins the players'
side, since PvE with others is allowed everywhere. A wandering group that arrives
joins the pirates. The shots are spread over the larger target list from that
round on.

### 7.3 Damage that lasts

*Decided*: hull damage stays, in the field and from fight to fight, until the
fleet is home.

**What a fleet is.** For each class, a count, and the hull of each ship of that
class that is not whole, kept in ascending order:

```
Light Fighter   count 412   damaged [118, 240, 240, 362]
Cruiser         count 60    damaged []
```

Four hundred and eight Light Fighters are whole and are not written down. This is
the whole of what the fight needs between ticks, so it is what the stored world
holds and what `stored_world.rs` will stop and start through. An NPC group is the
same. A round builds its list of units from these in one fixed order (fleets by
id, classes in catalogue order, whole ships and then the damaged ones as listed)
and writes them back sorted, so a world resumed from its document picks the same
targets with the same dice. No ship has an id any more.

**How large the damaged list gets.** Under OGame's rule a ship under 70 percent
hull is at risk at every hit, so damaged ships do not live long in a fight: the
list is long only for large hulls taking small shots, a Battleship line under
fighter fire. At the worst it is one number a ship, and only for fleets that have
fought and not yet docked.

**What it does to the 70 percent rule.** In OGame every ship starts every battle
whole, so the rule only bites in the later rounds. Here a ship can enter a fight
already under it. A fleet that fled at half its hull loses about half the ships
that are next hit hard enough to break a shield, at the first such hit. That is
the meaning of "damaged until home" in play: a mauled fleet is brittle, and
fighting again before it docks is a gamble. It is also why S1's reading of the
rule is recommended: there a ship rolls only when a shot has broken its shield, so
a mauled Battleship is not finished off by fighters whose shots its shield
absorbs. Under the other reading any shot that does not bounce would roll for it.

**Repair.** There is none in the game today (fact 3). Recommended:

- At the homeworld's dock, every ship of a docked fleet is made whole, at once
  and for nothing (`REPAIR_COST_SHARE_HOME`, starting value 0).
- At the outpost's dock, the same, but paid from the outpost's store: for each
  ship, the share of hull restored times `REPAIR_COST_SHARE_OUTPOST` of the
  ship's cost (starting value 0.10), taken as the outpost's own repair is taken,
  in metal first (`scaling::outpost_repair_payment`). A store that cannot pay
  mends what it can pay for, the most damaged ships first. A breached outpost
  mends nothing.

A price at home, or a repair that takes time, are the same two constants at other
values and one more (`REPAIR_TICKS`, starting value 0). They trade the cost of a
lost fight against how often a fleet goes out. Home is free to start with because
the trip home is already the price the owner named. Question 6.

**What else follows.**

- `split` takes a number of ships of each class, not ids. It takes whole ships
  first; a flag sends the damaged first, which is how the wounded are sent home.
- `merge` joins the counts and the damaged lists. The 64-ship limit goes
  (`MAX_FLEET_SHIPS`, section 7.8).
- `recall` keeps its rule: each ship rolls, and one that is harmed loses 20 to 80
  percent of its full hull, which now goes on the damaged list (it used to be a
  field of the ship).
- A ship recovered from a derelict arrives at 35 to 70 percent hull, as now
  (`add_recovered_ship`), as one more entry on the list.
- A repelled raid no longer halves the shields of the ships at home; shields are
  not kept.
- Standing orders wait for shields today (`POLICY_SHIELDS_READY`). They wait for
  nothing now, and go home when hull is low (section 7.4).

### 7.4 Leaving a fight: flight and pursuit

*Decided*: a fleet may flee to the next hex; more dangerous enemies chase a hex or
two; speed matters; fleeing has a price. The rules below are the proposal, and
every number in them is a knob.

**Leaving.** A fleet in a fight may be given a `move` to a connected sector, which
is refused today. It pays the jump's fuel at once, like any jump, and its jump
charges for its jump ticks, set by its slowest ship (section 5.7). While it
charges it does not fire, and it stays a target. It takes the fire of every round
fought before it is gone: one round fewer than its jump ticks, and never fewer
than `FLEE_MIN_ROUNDS` (starting value 1). A fleet of Cruisers takes one parting
round; one that brought Recyclers takes three. `stop` cancels the jump, forfeits
its fuel as it does today, and the fleet fights on.

**The price of fleeing**, in full:

1. The parting rounds, taken without firing back.
2. The damage, carried until home.
3. The fuel of the jump.
4. Nothing earned. A group pays its wreckage and its score when it is destroyed
   and not before, as today (`drop_salvage` and `credit_kill` run on a victory
   only). A fleet that kills half a group and leaves has been paid nothing.
5. The cargo of the ships it lost, left as wreckage in the sector it fled
   (`settle_hold`, as today).
6. Pursuit, if it is the slower.

**Pursuit.** When the last fleet opposing a group has left, the group may follow
the strongest of them.

- *Who follows* is set by the group's behaviour, which already runs with the
  rings (`world.rs`: Passive by share, then Patrol to ring 15, Aggressive to 25,
  Swarm beyond). `CHASE_HEXES`: Passive 0, Patrol 1, Aggressive 2, Swarm 2. It
  is a leash: the most hexes the group will stand from its own sector.
- *It follows only what it was beating*: its power must be at least
  `CHASE_MIN_RATIO` (starting value 1.0) times the power of the fleet it
  follows, on the scale threat ratings use. A strong raiding party that strikes
  and leaves is not followed by the pack it mauled.
- *It never enters* home ground (a home seat and the six sectors beside it), the
  hub, or a sector an outpost stands in. These are already closed to wandering
  groups (`process_npc_behavior`). A dock is a refuge.
- *It jumps at its own speed.* A group's jump ticks come from its class's speed
  and a drive level that rises with the ring (`npc_drive_level`, starting value 0
  through ring 8 and one level for every four rings beyond). It starts its jump
  on the tick the fleet arrives next door.

**Who is caught** follows from the two speeds and nothing else. Let the fleet's
jump take f ticks and the group's g.

- The fleet arrives and orders its next jump at once. If g is not less than f,
  the group arrives to an empty sector, every time, until its leash ends. A fleet
  as fast as its pursuer is never caught.
- If g is less than f, the group arrives while the fleet is still charging, and
  fires on it for f - g rounds before it jumps. That happens at each hex of the
  leash. A fleet two ticks slower than an Aggressive group takes four more
  rounds, unanswered, on top of the parting ones.
- A fleet that stops running is caught when the group arrives, and the fight is
  joined again with both sides firing and all damage as it stood. A second fleet
  waiting in that sector joins it. Drawing a group onto a waiting force is a
  fair use of the rule.

When the leash ends or the quarry is out of reach, the group jumps back to its
own sector and begins to heal (section 7.5). While it is away its sector is
empty.

**What is left of today's way out.** `recall` stays as it is: an instant jump all
the way home for twice the fuel and a chance of harm to each ship. Flight is the
cheap, slow way to the next hex; recall is the dear, instant way home. Emergency
Jump research, which softened it, is folded into Hyperspace Technology (section
5.5).

**Why a fleet cannot flee every bad fight and still come out ahead.** It is paid
only for groups it destroys (4, above). Every fight it leaves costs it parting
rounds and damage it keeps. The group it left mends (section 7.5), so the work is
lost unless it comes back soon, and it comes back brittle unless it went home
first. What remains open to it, and is meant to: a fast fleet takes two or three
rounds of a group in which its rapid fire does the killing, leaves on a one-round
parting, and returns before the group has mended. That is the owner's guerilla,
and its profit is the kill at the end, not the visits.

**Standing orders.** A policy has an engage ratio (`engage_ratio_x10`): it enters
a fight only at those odds. Its twin is `disengage_ratio_x10`, starting value 9
(the edge of RISKY, `RATIO_RISKY`), bounded as the engage ratio is and never
above it. Every `POLICY_EVAL_INTERVAL` ticks of a fight the policy reads the odds
of the fight as it now stands, and below the ratio it flees toward home, with the
reason in its `PolicyAction` ("left the fight at (-3, 9): odds fell to 0.7,
DEADLY"). The doctrines that pick no fight (mining, salvage) flee any fight they
are caught in. `POLICY_SHIELDS_READY` is replaced by `POLICY_HULL_READY`
(starting value 0.85): a fleet under orders whose hull has fallen below that
share of full goes home to be mended before it does anything else.

### 7.5 Pirates heal

*Decided*: slowly, over one to a few hours, so that a group worn down has to be
pressed.

Today a group that a fleet leaves is removed and its sector's template stands
again, whole, the next tick (`break_off_combat`). That is instant healing, and
with it hit-and-run could never work. Proposed:

- **A group that has fought stays as it is**: its counts and its damaged ships
  are kept, as a fleet's are.
- **Both heal**: lost ships return and damaged ships mend.
- **The clock** is the one a destroyed group already returns on:
  `npc_respawn_hours(ring)`, which is `2 + 0.25 x ring` game hours and is an
  economy timer, divided by pace. A wounded group is whole after
  `NPC_HEAL_SHARE` of it, starting value 0.5, so a wounded group always mends
  sooner than a dead one returns.
- **In steps**: the time is cut into `NPC_HEAL_STEPS` (starting value 10). At
  each step a tenth of the group's generated ships return, as far as it is
  short, and each damaged ship regains a tenth of its full hull. A group that
  lost three ships in ten is whole after three steps.
- **Only at rest**: the clock stops while the group is in a fight or on a
  pursuit, and starts again from the last step it reached.

| | Ring 3 | Ring 10 | Ring 20 |
|---|---|---|---|
| A destroyed group returns, pace 1 | 2.75 h | 4.5 h | 7 h |
| A wounded group is whole, pace 1 | 1.4 h | 2.25 h | 3.5 h |
| The same in a season (pace 10) | 8 min | 13.5 min | 21 min |
| The same in a blitz (pace 600) | 8 s | 13.5 s | 21 s |

The first row is today's rule. The second is the owner's "an hour" to "a few
hours". `NPC_HEAL_SHARE` trades how hard a raider must press against how much a
single visit is worth: at 0.25 a group at ring 10 mends in 68 minutes at pace 1,
at 1.0 in four and a half hours.

**Whether it scales with pace.** Recommended: yes, as an economy timer, because it
is tied to the respawn it must stay under. The other candidate is the attention
class (the raid clock's, the power 0.75 of pace), which would give two hours, 21
minutes and 59 seconds at ring 10 for the three paces above from a base of two
hours; it follows a player's session better, and at a blitz it would let a
wounded group mend more slowly than a dead one returns, which is why it is not
the default.

**What a worn-down group pays.** In full, at the kill: its wreckage and its score
are worked out from the power it was generated with (`npc_salvage`,
`score::kill_points`), and the heat that stops a sector being farmed applies as
it does now.

### 7.6 Raids fought as battles

A raid today is one number against another (fact 5), and it is the only time a
defence does anything. If it stays so, OGame's defences are eight ways to buy the
same number, and the Cruiser's rapid fire against Rocket Launchers and the
Bomber's against four defences never happen: there is no PvP, so no player ever
fires at a defence. The owner asked for "ship to defense interactions". So the
raid's resolution has to become a fight, and it is the one change to raids
proposed. Everything else about them stays: the clock, the forecast, the warning,
the size (`raid_power_base`), what a lost raid may take and the quiet after one.

**The fight.** When the raid arrives it stands at the homeworld as a group of
OGame ships of the forecast power. Against it: every ship of the owner at home,
every defence, and the Solar Satellites. Defences and satellites fight with the
owner's research as it stands that tick. One round a tick, watched from the home
screen, for at most `RAID_ROUNDS`, starting value 6, OGame's: raiders came for
the stockpile and leave if they cannot reach it.

| The fight ends | The raid is | Then |
|---|---|---|
| No raider is left | repelled | Wreckage at home, as today (`RAID_DEFENSE_SALVAGE_FRACTION`) |
| Six rounds, a defender still stands | repelled | The raiders leave; nothing is taken |
| No defender is left | lost | What a lost raid takes today (`skim_dock`: 8, 8 and 5 percent above the protected share), and the quiet after it (`RAID_SUPPRESS_AFTER_LOSS`) |

After it, defences are rebuilt at OGame's rate (section 7.1,
`DEFENCE_REBUILD_PCT` 70 and `DEFENCE_REBUILD_SPREAD` 10), at once. This replaces
the delayed return of 70 percent of the structures a repelled raid knocks out,
which was modelled on it. Ships and satellites lost are lost. A fleet may leave
during the warning, as it may today, or flee during the fight by section 7.4;
raiders do not follow.

**What this changes in weight.** Today a lost raid destroys a quarter of the
docked ships and half the structures, whatever the margin. A lost fight destroys
every ship that stayed and every defence, of which 70 percent return. Losing is
dearer, and rarer: six rounds are a short time, and a defence with hull enough to
last them repels the raid though it could never have beaten it. That is OGame's
turtle, and it arrives with OGame's numbers. `RAID_ROUNDS` and the raid's size are
the knobs. Question 3 puts the softer alternative.

**What the raiders fly** is this game's to say (a knob table,
`raid_composition`): Light Fighters for the first raids; Cruisers behind them
once the empire has Rocket Launchers in number; Bombers among them once it has
lasers. The point of a table is that a wall of one cheap defence has an answer.

**The outpost.** The same fight. Its platform, a single power figure today,
becomes a fixed set of defences for each level, which stands whole again after
every fight (starting values: 4 Rocket Launchers at level 1; 8 Rocket Launchers
and 4 Light Lasers at level 2; 16 Rocket Launchers, 8 Light Lasers and 4 Heavy
Lasers at level 3). What a lost raid does to an
outpost (the breach, the quarter of the store) stays. Every `OUTPOST_*` constant
is provisional already and is set again with the pirates' strength (section 8.2).

### 7.7 The odds

The label a player and a standing order read (SAFE to DEADLY) comes from a second
model of the fight today (fact 6), fitted to today's fight. Almost nothing of it
survives: the weighting of targets, the first strike, the damage roll, the shield
that does not return, all go.

Recommended: no second model. The odds are read off the real round, played
forward a fixed number of times (`ODDS_RUNS`, starting value 8) with fixed dice
from the two sides as they stand, damage included, on forces scaled down together
so that the larger has at most `ODDS_UNITS` (starting value 200). The figure is
the share of its fleet's worth each side keeps, put on the ratio scale as now
(`scaling::outcome_ratio`), whose sixteen points have to be measured again on the
new fight. It is deterministic, it counts the bounce, rapid fire, a brittle fleet
and a stand-off without any of them being written twice, and it is remembered by
the pair of forces as today's is (`ODDS_MEMO_LIMIT`).

It is also the most expensive thing in this study to run, and it has not been
measured: eight runs of up to sixty-four rounds on four hundred units is in the
order of a million shots for a pair of forces not seen before, perhaps ten
milliseconds. A standing order looks at many sectors, but at few distinct
groups, since a ring's groups are alike. If slice 2 measures it as too slow, the
fallback is a model on averages like today's, at the cost of keeping two fights
in step.

### 7.8 What a round costs

Nothing here was measured. The estimates are counts of shots.

A round fires one shot a unit and more where rapid fire chains: a Cruiser among
Light Fighters fires six on average, a Bomber among Rocket Launchers twenty, a
Deathstar among fighters two hundred.

| Units, each side | Shots a round, about | At 10 to 20 ns a shot |
|---|---|---|
| 500 | 1,000 to 3,000 | under 0.1 ms |
| 1,000 | 2,000 to 6,000 | about 0.1 ms |
| 5,000 | 10,000 to 30,000 | under 1 ms |

A tick is a second, so a round fits a thousand times over, and seventy fights at
once fit as well. Three things have to be so for that to hold.

- **A target is picked by index.** Today's `select_target` walks every ship for
  every shot. At a thousand ships a side that is millions of steps a round.
  OGame's pick is one random index.
- **A fleet has a ceiling.** `MAX_FLEET_SHIPS`, starting value 5,000, in place of
  64. It is far above "hundreds" and bounds the worst round.
- **No shot is an event.** Today each shot is a `CombatRound` message. Thousands
  a tick would be megabytes. One report a round takes their place (section 10).

**The same on every target.** The round is whole-number arithmetic from end to
end: hull, shield in hundredths, attack, the dice. It needs nothing from
`iac_shared::maths` and nothing of a float, so the rule that a world plays to the
same bytes natively and as WebAssembly costs it nothing. Indices are 32-bit,
never `usize`.

**Where the cost is not known.** The single-player host replays up to seven days
of missed ticks when a tab is opened again (`solo/src/world.rs`). Fleets under
standing orders fight through that absence. A thousand fights of a hundred rounds
each is a hundred thousand rounds, a few seconds at the estimates above and more
with the odds each policy reads. `solo/tests/both_targets.rs` plays its worlds in
an interpreter many times slower than a browser and takes two minutes today; it
will take longer. Both are to be measured in slice 2 before anything is tuned.

---

## 8. What it does to the engine and to the rest

### 8.1 Stockpiles have to be 64-bit

`Resources` is three `f32` (fact 7). A 32-bit float holds whole numbers exactly up
to 16.7 million and steps more coarsely as it grows: by 0.0625 at one million, by
0.25 at two and a half million, by 0.5 at five million. A level 10 Metal Mine at
pace 1 adds 0.216 a tick. Added to a stockpile of 2.5 million it rounds to 0.25,
which is 16 percent too much; added to one of five million it rounds to nothing,
and the mine has stopped. The same happens in a hold being filled a little each
tick.

It does not happen today because today's caps are a few thousand. It happens
under OGame's numbers at once: a storage building at level 7 holds 1.4 million, a
Large Cargo 25,000 and a hundred of them 2.5 million, a Deathstar costs five
million metal.

Recommended: `Resources` becomes three `f64`, everywhere it is used, in a slice of
its own before the catalogue (slice 0b). Sixty-four bits are exact past nine
thousand million million. The four operations on them are the same on every
target, so the rule about arithmetic is kept. The wire's resource figures become
64-bit with them, which is what Dart's numbers already are. It changes what a
stored world holds, so `SAVE_FORMAT` and `ECONOMY_VERSION` both move.

### 8.2 Pirate groups in OGame's classes

*Measured*: a group is one class, chosen by ring (`npc_class`: Scout to ring 8,
Corvette to 12, Frigate to 20, Cruiser beyond). Its power follows
`npc_power = 4 x 1.22^(ring - 1)`; its ship count is fitted to that power with
every ship's hull, shield and weapon scaled by one multiplier of 0.6 to 1.3; it
has at most 32 ships. Each class has rapid fire against the one before, which is
why ring 9 is a step in danger.

All of that stays. Three things are knobs, and none of them can be set from here:

- **The class for each ring** (`npc_class`). OGame has the same ladder, each rung
  with rapid fire against the one below: Light Fighter, then Cruiser (6 against
  it), then Battlecruiser (4 against the Cruiser), then Destroyer (2 against the
  Battlecruiser). Starting value: the same four bands as today. The rungs are
  further apart than today's (a Cruiser is seven Light Fighters in power, a
  Corvette is two and a half Scouts), so the step at ring 9 is steeper, and the
  Heavy Fighter and the Battleship are there to put between them.
- **The scale** (`NPC_POWER_BASE`). Starting value 40 where it is 4 today. A
  Light Fighter is 91 on the scale threat ratings use (weapon plus a tenth of
  hull and shield) and a Scout is 9, so 40 keeps every ring exactly as strong
  against two starting ships as it is now.
- **The growth** (`NPC_POWER_GROWTH`, 1.22) and the ceiling (`NPC_MAX_SHIPS`,
  32). This is the knob that decides whether pirates come in hundreds. At 1.22 a
  group at ring 20 is worth one or two Battlecruisers, as it is worth three or
  four Frigates today. At 1.5 it is about sixty Battlecruisers and at 1.6 about
  two hundred. Players' fleets will be whatever OGame's economy lets them build
  at this game's paces, which the planning simulation has to say (section 8.9)
  before this number means anything.

The power of a raid on a homeworld (`raid_power_base`, `5 + 11 x econ^0.75`) is
fitted again on the same scale. Threat ratings, the derelict guard and the
outpost raid follow from the group's power and need nothing.

### 8.3 Mining, cargo and what the map pays

A hold is OGame's cargo figure times the pace's cargo factor, as it is this game's
figure times that factor today (`fleet_cargo_capacity`). Nothing else about holds
changes.

What the map pays is counted in today's money: an ore tile holds 3 to 212 units,
a derelict at ring 4 is worth 240, a group's wreckage is `1.2 x power^0.8`. OGame's
mines make three times what this game's do and its ships cost five to ten times
as much, so every one of these would shrink to a third or a tenth of the share of
an empire's income it was tuned to be. They are knobs (`ORE_*`,
`DERELICT_VALUE_BASE`, `WRECK_VALUE_K`). Starting value: each multiplied by 3,
the ratio of the mines, which keeps the map's share of income where the owner put
it ("the map pays", 2026-10-10).

One thing follows that a multiplier does not mend. A Hauler holds 200 today and a
rich tile fills it. A Small Cargo holds 5,000 and no tile will. The mining
doctrine's rule "come home when the hold is 85 percent full"
(`POLICY_DEFAULT_CARGO_RETURN_PCT`) will seldom fire on ore; it comes home when
the seam is stripped, which it already knows how to judge
(`POLICY_MIN_SEAM_UNITS`). Whether that reads well in play is for the first
playtest after slice 1.

### 8.4 Fuel and jumps

The tank stays. Three numbers under it change.

- **What a jump burns.** Today: a tenth of the fleet's total hull, less Fuel
  Efficiency (`fleet_fuel_cost`). Proposed: the sum of OGame's fuel figure for
  each ship, less the saving of Energy Technology. It is the one fuel number the
  catalogue has, and it is in this game's range already (a Cruiser's hull over
  ten would be 270; OGame's figure is 300). Two side effects: Armour Technology
  no longer makes a jump dearer, which the reading of 2026-10-13 found Reinforced
  Hulls doing; and the Deathstar burns 1, which is OGame's own oddity and is
  taken as it is.
- **How much a tank holds.** `TANK_JUMPS` for the class times its burn, times the
  Deuterium Tank's factor (section 5.7).
- **How long a jump takes.** From speed (section 5.7).

At today's price of fuel (`FUEL_DEUT_PER_UNIT`, 0.08 deuterium a unit) one jump
costs 1.6 deuterium for a Light Fighter, 24 for a Cruiser, 40 for a Battleship
and 80 for a Destroyer or a Bomber. The stranded fleet's emergency reserve, the
refuelling at docks and the fuel budget of standing orders read the tank and are
not touched.

### 8.5 Fuel from the hold

The owner's proposal: deuterium in a fleet's hold may be made into fuel at a loss
"like 15%". An opinion was asked for. It is a good rule and a small one, and it
fits the tank that stays.

- An order, `refine`, for a fleet that is not in a fight and not charging a jump.
  It turns deuterium in the hold into fuel until the tank is full or the
  deuterium is gone.
- At the home dock a unit of deuterium is 12.5 units of fuel
  (`1 / FUEL_DEUT_PER_UNIT`). From the hold it is that less `REFINE_LOSS`,
  starting value 0.15: 10.6.
- It takes no time. A cooldown (`REFINE_TICKS`) is the knob if it should.

What it does to the game: deuterium mined, salvaged or carried becomes reach, so
a fleet that works the field can stay out, and a cargo ship loaded with deuterium
at home is a tanker. That second use is the stronger. Under OGame's holds one
Large Cargo of deuterium is 265,000 units of fuel, about 880 Cruiser jumps, and
the loss hardly limits it; what limits it is the deuterium, and that nothing can
be refined in a fight. So `REFINE_LOSS` trades the reach of a fleet with cargo
ships against the worth of the home dock and the outpost as places to fill up,
and at OGame's hold sizes it is a weak brake. If it proves too weak, the stronger
knob is to refine only what the fleet itself took in the field, which needs the
hold to remember where its deuterium came from; that is not proposed.

It needs nothing else in this study and can be built first, on the game as it is
(slice 0a). Standing orders can learn to refine when they run low afterwards.

### 8.6 Outposts, the hub, the score, pace

- **Outposts.** Their rules stay. Every `OUTPOST_*` constant is in today's money
  and is provisional; cost, store and yield take the same multiplier as the map
  (section 8.3). The platform becomes defences and the raid a fight (section
  7.6). The dock mends hull for a price (section 7.3). A fleet standing at the
  outpost is refitted only at home (section 9).
- **The hub and the exchange.** Nothing. The fee meter's span is a share of what
  the player's mines make, so it follows the mines.
- **The score** (`sim/src/score.rs`). The rule stays: the cost of every level,
  ship and defence at its weight, defences at half, combat and exploration up to
  a quarter of the rest. Scores grow about tenfold with the costs. A damaged ship
  counts in full, as it does today. Satellites count as ships.
- **Pace.** OGame's universe speed is this game's pace exactly: costs do not
  move, output is multiplied and times are divided. OGame's formulas go into
  `Pace::rate` and `Pace::econ_ticks` unchanged. OGame does not scale storage
  with speed and this game does (`Pace::cap_mult`); that stays this game's. The
  two estimates a world announces (`first_cruiser_s`, `endgame_s`,
  `shared/src/pace.rs`) are today's catalogue's and have to be made again; there
  is still a Cruiser to name the first of them after.

**Storage, as a consequence the owner should see.** This game's caps are tight on
purpose: 5,000 metal with no Vault, and the Vault is an early decision. OGame's
are loose: 100,000 with no building. With OGame's figure a new empire cannot fill
its stores for days at pace 1, the warnings about a full stockpile go quiet until
the middle of the game, and the rule that an order costing more than a cap is
refused bites only for big batches. "Exact" says OGame's figure, and it is the
starting value; `STORAGE_BASE` is the knob if the early pressure is missed.
Question 8.

### 8.7 The bots

They must still play, and they act only through commands, so nothing about how
they are hosted changes. What they know by name does (*measured*,
`sim/src/bot/home.rs` and `field.rs`):

- Five profiles, each an ordered list of research by name, twelve of the
  thirteen researches among them. All five lists are written again.
- `Profile::appetite` matches on all ten buildings; `MINES` and `SUPPORT` are
  lists of names; `yard_needs` asks for the Defense Grid. Written again, with one
  new rule: build power when the mines are short of it.
- Ships are chosen by role (`Hull`: Scout, Warship, Hauler), and the warship is
  the heaviest unlocked of Cruiser, Frigate, Corvette. The roles stay; the lists
  under them change (Probe; a line from Light Fighter to Battleship; Small and
  Large Cargo with Recyclers for the miners).
- Only the Pulse Turret is ever built. It becomes the Rocket Launcher to begin
  with; a bot that answers what raids fly at it is later work.
- In the field they split by ship id, wait for shields (`SHIELDS_UP`), and fight
  on the server's odds (`SAFE_ODDS`, `RATIO_FAVOURABLE`). They split by count,
  wait for nothing and go home on hull, and read the new odds. They flee through
  the disengage ratio of the policies they already set.

What a bot remembers (`BotRecord`) names no building, research or ship, so that
part of the stored world keeps its shape.

### 8.8 The planning simulation

`docs/design/economy/sim.py` names every building, research, ship and defence
once and refuses to run when its names differ from `numbers.json`, so it stops
the moment the catalogue changes. Under the names, most of it is today's
catalogue: the mine formula, a build script in ratios to the Metal Mine's level,
a research order of 41 steps, a fleet plan (two Corvettes, two Haulers, Cruisers
by mine level, four Frigates), the kit that defines the endgame, the milestones
it reports. It has no energy. All of that is written again in slice 1, and it is
most of a day of that slice.

`sim/tests/economy_numbers.rs` renders `numbers.json`; its `by_level` table is
keyed to seven of today's researches and its `named!` list to today's constants.
`tools/check_sim_constants.py` names the Fabricator and the Shipyard.
`docs/design/economy/spec.md` describes an economy that will no longer be the
game's; it stays as the record of how the present one was tuned, with a line at
its head saying so.

What the simulation is then needed for, first: what a player owns on day 3 and
day 7 of a season under OGame's catalogue. The pirates' growth, the raid's size,
the map's multiplier and the presets' estimates all wait on that.

### 8.9 The stored world

Nothing is migrated ("existing databases are disposable"). `SAVE_FORMAT` (5,
`sim/src/store.rs`) and `ECONOMY_VERSION` (5) move in every slice, and a world
stored before a slice is refused after it.

| Part of `Snapshot` | Changes shape in |
|---|---|
| Every `Resources` (players, fleets, outposts, sector wreckage) | 0b |
| `players`: building levels, research levels, defences, the three queues | 1 |
| `fleets`, `npc_fleets`: ships | 1 (classes), 2 (counts and damage) |
| `known`: what was last seen in a sector, by class | 1, 2 |
| `pending_events` | 1, 2 |
| `combats`: who is fleeing, who pursues | 3 |
| `npc_fleets`: the healing clock | 3 |
| `raid_states`: a raid in progress | 4 |
| `bots`, `outposts`, `policies` (one new parameter in 3) | little or nothing |

The engine still knows nothing of storage. What a fight needs between ticks is in
the fleets, so there is no second copy of a battle to keep in step.

### 8.10 The wire

The types are written once in `shared/src/protocol.rs` and the Dart is generated,
so a change here is: edit Rust, regenerate, fix what the analyzer points at.

- The four enumerations grow: 5 ship classes to 13, 3 defences to 8, 10 buildings
  to 12, 13 researches to 14. Ship and defence names share one namespace in the
  shipyard queue (`ShipyardItem` is untagged); no OGame ship has a defence's
  name.
- `FleetState.ships` and `HomeworldState.docked_ships` stop being a record a ship
  and become a group a class (section 10.1). `ShipState` goes.
- `ShipClassCounts` is a struct with five named fields, filled by a five-arm
  match in `sim/src/intel.rs`. It becomes a list of class and count, like
  `NpcShipInfo`, whose count grows from 16 bits to 32.
- `Split` names a count for each class in place of ship ids.
- `CombatRound` (one a shot) and `ShipDestroyed` (one a ship) go. `BattleRound`
  (one a round) and a fuller `CombatEnded` take their place.
- The catalogue (`HomeworldCatalog`) carries cost, time and requirements already.
  It gains each unit's fight figures and its rapid fire, so that no client holds
  a table. It is sent again with every homeworld state today; at 47 entries the
  part that never changes should be sent once.
- The generator's lists (`shared/tests/wire/mod.rs`) name the columns of
  `ShipClass` the client is handed (label, hull, shield, weapon, cargo, power)
  and `ShipClassCounts` among the types with a default; both are edited. Of the
  240 fixture files, 41 name a building, research or ship and are rendered again.

---

## 9. Refit

**Today** (*measured*). Hull, shield and weapon research are multiplied into a
ship's numbers when it is built and never again. Everything else research does
(burn, tank, jump time, yield) is read from the player's levels when it is needed
and so applies to every fleet at once.

**The owner's preference**: ships take new research when they come home, not in
the field; research that applies everywhere at once is accepted if refit is a
problem.

**It is not a problem, and it is cheap.** A ship no longer has numbers of its own
to stamp. So the stamp moves to the fleet: a fleet carries the three levels
(Weapons, Shielding, Armour) it last docked at home with. The fight already takes
each fleet's own levels (section 7.2). The rules:

- A fleet docked at the homeworld has the player's levels. Research that finishes
  while it is docked reaches it at once.
- New ships are built into a fleet docked at home, as they are today
  (`add_ship_to_homeworld`), so they are current.
- A fleet split in the field gives both halves its levels. Two fleets merged in
  the field take the lower of each level, until the merged fleet docks.
- A ship recovered from a derelict fights at the levels of the fleet that found
  it.
- Defences and satellites are at home and always current.
- Only these three researches wait for the dock. The drives, the fuel saving, the
  tank and the rest apply everywhere at once, as their counterparts do today.

The state says what a fleet fights with and whether a refit is waiting (section
10.1), so that a client does not compare levels to find out.

**Against it**: research that works everywhere at once is OGame's own rule and
needs no state and no merge rule. **For it**: three small numbers a fleet; it is
what the owner prefers; and with damage that lasts until home, the dock is where
a fleet is mended anyway, so one visit does both.

**Recommended: refit at the home dock, by a stamp on the fleet.** The outpost's
dock mends hull but does not refit: the owner's word was "home", and the outpost
has no shipyard. Question 7.

---

## 10. The clients

The rule (`CLAUDE.md`): a client draws and words; it does not work out a rule of
the game. So this section starts with what the engine has to say.

### 10.1 What the state must carry

**A fleet** (`FleetState`), for each class it holds:

| Field | Meaning |
|---|---|
| `class`, `count` | How many |
| `damaged` | How many are not whole |
| `at_risk` | How many are under 70 percent hull, and may explode when next hit |
| `hull`, `hull_max` | Hull they hold between them, and at full |

and for the fleet as a whole: its attack and its shield for a round with its
refit counted, beside the `power` it has today; the three levels it fights with
and `refit_due`; `jump_ticks`, which it has; and, while they apply, `fleeing`
(where to, ticks until it is gone, rounds of fire still to take) and `pursued_by`
(which group, ticks until it arrives, whether it arrives before the fleet's next
jump can be made).

**A group in sight**: its ships by class with the same four figures; its jump
ticks; how far it follows (`chase_hexes`); whether it is following and whom; and
`mends_in`, the ticks until it is whole.

**A move previewed from inside a fight** (`MovePreview`): the parting rounds the
jump will cost, whether the group will follow, and how many rounds a hex it will
catch the fleet for.

**A round** (`BattleRound`, one event a round to everyone present):

| Part | Holds |
|---|---|
| Header | Sector, round number, which fleets and groups are in it, which are silent because they are leaving |
| Volleys | For each pair of firing class and target class, on each side: shots, shots that bounced, damage absorbed by shields, damage to hull, units destroyed |
| Standing | For each fleet and group, by class: count, damaged, at risk, after the round |
| Totals | For each side: shots, power fired, absorbed by the other side's shields. These three are what OGame's own report prints for a round |

A round between two sides of three classes each is nine volley lines a side at
most. It replaces a message a shot, of which a fight of hundreds would send
thousands a tick.

**The end of a fight** (`CombatEnded`, which exists): how it ended (a side
destroyed, a side gone, a stand-off, a raid's six rounds); what each fleet lost,
by class; for a raid, what was rebuilt.

**A client that arrives in the middle** (a reconnection, a second device) gets the
fight's last round and its totals so far in the full state. The rounds before
that are the client's to have kept; the engine does not store them.

**The catalogue**: for each ship and defence, structure, shield, attack, cargo,
speed, fuel, tank and the list of what it has rapid fire against; for the
homeworld, energy made, energy needed and the fraction the mines run at.

### 10.2 The windshield with hundreds of ships

*Measured*: the web client draws one `Unit` a ship, own and hostile, but it has
already stopped looking at single ships before it does. `state_mapper.dart` folds
a fleet into one group a class with a count and an average hull, and the scene
(`scenes/windshield/sim.dart`) spreads each group back out. The formation has
eight slots; a ninth ship is placed in a column that runs off the scope. So the
step to a representative picture is short: stop spreading the groups out.

- **One mark a class**, own and hostile, with its count beside it. How a mark
  shows size (a cluster of pips, a larger glyph) is drawing, and is the client's.
- **A hull bar a class** in three parts: whole, damaged, at risk. This is what
  tells a player a fleet is brittle before it fights again.
- **Glyphs.** `lib/design/symbols.dart` has five ship shapes for each side. It
  needs thirteen, and eight for defences at home.
- **A round.** Each volley line is drawn as fire from one class mark to another,
  its weight by the shots; bounced shots break on the shield; the target's count
  falls by the destroyed. One round a tick is one beat a second.
- **A flight.** The fleet's marks turn to the lane it is leaving by; its guns are
  silent; a count runs down ("JUMP IN 2"); the incoming fire goes on. After the
  jump, the new sector, and if the state says a group follows, a banner with its
  arrival ("PURSUIT: ARRIVES IN 1, FROM (4, -2)") and the lane it comes down
  marked. If it will not arrive in time, or has turned back, the banner says so.
  All of it is words for figures the state carries.

### 10.3 The battle view

The owner's "modal showing actual ships and firings". It is the same data as the
windshield's fight, laid out as OGame's report is: the two sides by class, counts
before and after each round, the volleys of the round as a table of who fired at
whom and with what result, the totals under it. Opened during a fight it follows
the rounds as they come. Opened after one it steps through the rounds the client
kept. Nothing in it is computed by the client.

### 10.4 Build screens, the TUI, agents

- **Web.** Rows, costs, times and requirements come from the catalogue already.
  What is written out by hand for each item and must be written again: the notes
  for ten buildings (`views/homeworld/spec.dart`), the lanes the research screen
  sorts thirteen researches into (`research_tab.dart`), the ten fixed slots of
  the homeworld scene (`scenes/homeworld/plan.dart`), the command bar's aliases
  and help (`state/command_parser.dart`), two onboarding lessons that name the
  Corvette and the Metal Mine. Energy is one more figure beside the resources.
- **The scripted world of the widget tests** (`test/support/demo_world.dart`,
  2,714 lines) has its own tables of ships and its own fight, a ship at a time.
  It has to follow in slices 1 and 2, and it is a large part of the client work.
- **TUI.** Its build screens are driven by the catalogue. Its fleet panel lists
  every ship on its own row and splits by the rows marked
  (`renderer/mod.rs`, `state.rs`); it becomes a row a class with a number to
  split off. Its log prints a line a shot; it prints a round.
- **Agents.** `tools/playtest/play.py` reads ships one by one and folds the
  damage of single shots; `bench_loop.py` has a build order and a ship list by
  name; `RULES.md` describes the catalogue and the fight in prose throughout.
  All three follow the slice that changes what they describe. An agent gets far
  less to read in a fight than it does today.

---

## 11. The knobs

Every number this document proposes, as the constant the implementation would
hold. "OGame's" means the figure is OGame's own and is listed only because the
owner may want it moved. None was measured.

**The fight.**

| Constant | Starting value | Trades |
|---|---|---|
| `RAID_ROUNDS` | 6 (OGame's) | A defence that can outlast a raid against one that must beat it |
| `STANDOFF_ROUNDS` | 1 (OGame's) | A fight ended early by a quiet round against one that drags |
| `EXPLODE_BELOW` | 0.70 (OGame's) | How brittle a damaged ship is |
| `DEFENCE_REBUILD_PCT`, `DEFENCE_REBUILD_SPREAD` | 70, 10 (OGame's) | What defences cost to hold against what ships cost |
| `MAX_FLEET_SHIPS` | 5,000 | The worst round's cost against the largest fleet allowed |
| `ODDS_RUNS`, `ODDS_UNITS` | 8, 200 | How steady the odds are against what they cost |

**Flight and pursuit.**

| Constant | Starting value | Trades |
|---|---|---|
| `JUMP_SPEED_REF` | 50,000 | How much speed matters in flight against how long every trip takes |
| `FLEE_MIN_ROUNDS` | 1 | The least any flight costs |
| `FLEE_FUEL_MULTIPLIER` | 1.0 | A dearer flight; recall's is 2.0 |
| `CHASE_HEXES` | Passive 0, Patrol 1, Aggressive 2, Swarm 2 | How far the outer rings follow |
| `CHASE_MIN_RATIO` | 1.0 | Whether a mauled group still follows |
| `npc_drive_level(ring)` | 0 to ring 8, then 1 a four rings | How fast a fleet must be to outrun a ring |
| `POLICY_DEFAULT_DISENGAGE_RATIO_X10` | 9 | How long a fleet under orders stays in a losing fight |
| `POLICY_HULL_READY` | 0.85 | How mauled a fleet under orders goes on working |

**Damage and healing.**

| Constant | Starting value | Trades |
|---|---|---|
| `REPAIR_COST_SHARE_HOME` | 0 | A second price on a lost fight |
| `REPAIR_COST_SHARE_OUTPOST` | 0.10 | The outpost as a place to mend against home |
| `REPAIR_TICKS` | 0 | Time out of the field after damage |
| `NPC_HEAL_SHARE` | 0.5 of the respawn delay | How hard a raider must press against what one visit is worth |
| `NPC_HEAL_STEPS` | 10 | How smoothly a group mends |
| `RECALL_SAVING_PER_LEVEL` | 0.05 a level of Hyperspace Technology | The emergency jump's risk |

**The catalogue's joins to this game.**

| Constant | Starting value | Trades |
|---|---|---|
| `HOME_TEMPERATURE_MAX` | 40 | Deuterium output (x1.20) and a satellite's energy (30) |
| `TANK_JUMPS` | by class, section 5.7 | Reach against the worth of docks and tankers |
| `TANK_PER_DEUT_TANK_LEVEL` | 0.25 | The same, bought at home |
| `FUEL_SAVING_PER_LEVEL` | 0.05 a level of Energy Technology, to a half | What a jump costs late |
| `FUEL_DEUT_PER_UNIT` | 0.08, as today | What a jump costs at all |
| `REFINE_LOSS`, `REFINE_TICKS` | 0.15, 0 | Reach from the hold against the worth of docks |
| `HARVEST_YIELD_PER_LEVEL` | 0.20 a level of Laser Technology, 5 levels | What ore is worth late |
| harvest power | Recycler 5, Small and Large Cargo 1, others 0.5 | Who mines |
| `BUILD_SLOT_LEVELS` | Computer Technology 3 and 6 | When a second and third building run together |
| sensor range | 1 hop a level of Espionage Technology | What is seen from home |
| `STARTING_SHIPS` | 2 Light Fighters | What a new empire can do on its first day |
| `STORAGE_BASE` | 100,000 (OGame's) | Early storage pressure |

**The world's strength, to be set from the planning simulation.**

| Constant | Starting value | Trades |
|---|---|---|
| `npc_class(ring)` | Light Fighter to 8, Cruiser to 12, Battlecruiser to 20, Destroyer beyond | Where the steps in danger fall |
| `NPC_POWER_BASE` | 40 | Every ring's strength against a starting fleet |
| `NPC_POWER_GROWTH`, `NPC_MAX_SHIPS` | 1.22, 32, as today | Whether pirates come in hundreds |
| `RAID_POWER_*` | today's law, fitted again | A raid against what an empire can field |
| `raid_composition` | Light Fighters, then Cruisers, then Bombers | Whether one cheap defence is an answer |
| Map multiplier on ore, derelicts, wreckage, outposts | 3 | The map's share of an empire's income |
| Outpost platform | by level, section 7.6 | An undefended outpost's odds |

---

## 12. Phasing

Each slice leaves a game that plays, builds as WebAssembly and passes its
end-to-end tests. Each moves `SAVE_FORMAT` and `ECONOMY_VERSION`.

**Why the catalogue comes before the fight.** The other order was looked at and
does not work. OGame's round restores every shield every round, and its ships are
built for that: a weapon five times the shield. This game's ships carry a shield
twice the weapon. Two Scouts against two under OGame's round never touch a hull,
and every small fight is a stand-off. So the fight cannot change while the ships
are this game's. The reverse does work: OGame's ships under today's fight simply
hit hard, and today's rapid-fire check is already OGame's formula
(`check_rapid_fire`).

**Why the catalogue is one slice.** Every ship's requirement is an OGame research
and every research's is an OGame building. A half of it would need this game's
ship techs kept alive beside OGame's research to unlock OGame's ships, and that
is a compatibility shim, which `CLAUDE.md` forbids. Buildings, research, ships
and defences land together.

| Slice | Lands | Leaves | Throws away |
|---|---|---|---|
| 0a | `refine`: fuel from the hold (8.5) | Today's game with one more order | Nothing |
| 0b | `Resources` in 64 bits (8.1) | Today's game, exact at any size | Nothing |
| 1 | The catalogue: the four enumerations with their costs, requirements, fight figures and rapid fire; OGame's production, energy, storage and times; the folds of 5.5; this game's columns of 5.7; pirates and raid power on the new scale; the bots' tables; `sim.py`; the wire's enumerations and catalogue; the clients' text and glyphs | OGame's catalogue, fought as fights are today: 64 ships a fleet, a record a ship, raids by power | This game's catalogue and all its tuning: the economy spec's tables, the presets' estimates, the raid fit, the outposts' measured constants, every balance run of October |
| 2 | The fleet as counts and damage; OGame's round, one a tick; repair at the docks; the refit stamp; `split` by count; the round report; the odds read off the real round; plain fleet lists and a text round in both clients | Fleets of hundreds. A fight is still to the death, and `move` is still refused in one | `Ship`, `combat.rs`, `odds.rs` and its sixteen measured points, shield regeneration, a message a shot, the 64-ship limit |
| 3 | Flight, pursuit, groups that keep their losses and heal, the disengage ratio | The owner's guerilla | The reset of a group when a fight breaks off; the refusal of `move` in a fight |
| 4 | Raids fought as battles at home and at the outpost; OGame's defence rebuild; what raiders fly; the platform as defences | Defences that fire | The comparison of two numbers; the delayed rebuild; the fixed quarter of ships |
| 5 | The windshield in class marks, flight and pursuit on it, the battle view | The picture the owner described | The windshield's unit-a-ship scene |

**Order.** 0b before 1. 1 before 2. 2 before 3, 4 and 5. 3 and 4 do not depend on
each other. 5 can start on the wire of slice 2 and takes flight from slice 3.
0a at any time.

**What must be answered before which slice.** Before 1: questions 1, 2, 8 and 9.
Before 2: 6 and 7. Before 3: 4, 5 and 11. Before 4: 3. Question 10 before 0a.
Nothing else waits on the owner.

**What is measured in slice 2 before anything is tuned**: the cost of a round at
a thousand and at five thousand units a side, natively and as WebAssembly; the
cost of the odds for a pair of forces not seen before; the time the single-player
host takes to replay a day and a week of a fleet under orders; the time
`both_targets` takes.

**Size.** Against this month's journal, where outposts' first slice was five and
a half agent-days with both clients: 0a half a day, 0b one, slice 1 seven to
nine (a day of it `sim.py`, two the clients and their scripted world, one the
bots), slice 2 six to seven, slice 3 three, slice 4 two to three, slice 5 three
to four. Twenty-three to twenty-eight in all. Slice 1 is the one most likely to
grow, because it touches every document that describes the game.

**To make it smaller**, in this order: leave the Deathstar and Graviton out of
slice 1 (two rows and nothing else, but one decision fewer); build slice 4 after
a playtest of slice 3; let the TUI's battle be a text round and nothing more.
What should not be cut: energy from slice 1 if the answer to question 1 is yes
(it would move the economy twice), the measurements of slice 2, and the healing
from slice 3 (flight without it does nothing to pirates).

---

## 13. Tests whose assumptions will not hold

Tests here are diagnostics (`CLAUDE.md`). For most of those below the second of
the three questions, whether the test agrees with the design, will have the
answer no, and the test is rewritten to say what the new design means. None is
deleted; one that is retired is moved to `.archive/`.

**Hold, with names changed or a golden recorded again.** These are about the
shape of the game and not its content: `determinism.rs`, `stored_world.rs` (it
names a Scout and a Pulse Turret and splits by ship id), `chart_equivalence.rs`
(its golden file is recorded again in every slice), `bots.rs` (soft thresholds:
a turtle's Metal Mine at 6 or more, a turtle stays home), `pace_scaling.rs` (one
named build order at two paces, a ratio of 9 to 11 that any catalogue should
give), `server/tests/smoke.rs` and `hosts_agree.rs` (named orders),
`solo/tests/both_targets.rs`, `shared/tests/golden_fixtures.rs` (it renders the
fixtures and the Dart again).

**Pin today's catalogue. Slice 1.**

- `empire_arc.rs`: a new empire has one fleet of two ships; the first four
  buildings finish in a stated order; Modular Fabrication at level 1 gives two
  building slots; a refusal names "Storage Vault level 1"; a battle group is
  sized in Corvettes.
- `exchange.rs`: one Hauler holds 632.4555 (200 times the root of 10); 632 metal
  buys 225 deuterium.
- `outposts.rs`: the platform is 40, 100 and 250; two Haulers jump for 16 on a
  tank of 200; a garrison table by ring in Corvettes, Frigates and Cruisers;
  level costs.
- `fleet_logistics.rs`: twenty Scouts recalled over 25 hexes burn exactly 3,000;
  a table of jump ticks for two Scouts and for a Scout with a Frigate at
  Navigation 0 and 1.
- `scenarios_world.rs`: Inner Ring packs are Scouts.
- `server/tests/scenarios.rs`: a starting hold of 40; an outpost's cost.
  `solo/tests/single_player.rs`: a Scout costs 200 metal.
- `economy_numbers.rs` and `tools/check_sim_constants.py`: they pass again once
  `numbers.json` is rendered and `sim.py` reads it.

**Encode today's fight. Slice 2.**

- `scenarios_world.rs`, `a_label_tells_the_outcome_whatever_the_classes`: a table
  of class, count, ring and label, each held to a win rate and a share of hull
  kept over sixty fights. What it asserts, that a label means the same outcome
  whatever the classes, stays the design; the table and the bands are the old
  fight's.
- `a_starter_fleet_wins_through_the_inner_ring_and_dies_past_it`: rings 7 and 8
  survived, 9 and 10 not. The intent stays and becomes the check on
  `NPC_POWER_BASE` and the class bands.
- `a_default_patrol_holds...`: four Scouts against eight at ring 10.
- The helpers of `sim/tests/common/mod.rs`: `muster` and `deploy` build `Ship`
  records and write hull and weapon into them; `fights` sums hull a ship;
  `fight` sets a weapon of 5,000 and expects the fight over in ten ticks;
  `lone_scout_sector` looks for one Scout.
- `fleet_logistics.rs`: recall's harm ship by ship, `split` by ids, a shield
  ratio summed over ships.
- `exchange.rs`: a `Ship` and an `NpcFleet` written out by hand.
- Every Flutter test that plays `test/support/demo_world.dart`, which fights a
  ship at a time.

**Encode what flight removes. Slice 3.** `endless_fight` in `scenarios_world.rs`
expects a fleet still `InCombat` after three ticks and the fight's membership
rules; whatever asserts that a group left alone is whole again.

**Encode the raid as a comparison. Slice 4.** `scenarios_raids.rs` (a raid's
power within the roll of `raid_power_base`: that half holds); `exchange.rs` (a
lost raid costs a quarter of the docked ships, two Haulers of eight);
`outposts.rs` (defence power is 40 plus two Corvettes at 22; a raid of 1.6 to 2.3
times the pack).

**One guard to add to.** `shared/src/pace.rs` lists every duration constant with
its pace class and a test fails when one is missing. `REPAIR_TICKS` and
`REFINE_TICKS` are real time; the healing of pirates is a share of a timer that
is already listed.

---

## 14. Questions for the owner

Each has the default that is built if it is not answered. The nine things already
decided (section 2) are not asked again.

1. **Does energy come in with the mines?**
   *Default: yes.* Solar Plant, Fusion Reactor, Solar Satellite, Energy
   Technology's effect and Graviton get their OGame job, and every production
   number is OGame's, cost and output (section 5.6).
   *If no:* those two buildings and the satellite are left out; the Deathstar and
   Graviton are left out too, or Graviton is given a price in resources that is
   this game's invention; the mines make OGame's output for about half of what
   OGame's player pays in all. Slice 1 is about a day smaller.

2. **Are this game's own jobs carried by OGame's items, or kept beside them?**
   *Default: carried, as section 5.5 lists.* Three of the folds are loose fits
   and are the ones to look at: fuel saving on Energy Technology, the fleet's
   tank on the Deuterium Tank, ore yield on Laser Technology.
   *If kept beside:* Fuel Efficiency, the Fuel Depot (with Extended Tanks folded
   into it) and Harvesting Efficiency stay as they are, at today's prices. The
   research screen is then OGame's fourteen and two of this game's, the building
   list OGame's twelve and one. The other folds (Espionage for the Sensor Array,
   Hyperspace Technology for Emergency Jump, Computer Technology for Modular
   Fabrication, the drives for Navigation) stand either way unless struck.

3. **Is a raid fought as a battle?**
   *Default: yes, for at most six rounds, and lost only when no defender is
   left* (section 7.6). It is the only way a defence fires before PvP.
   *If softer:* after six rounds the raid is lost when the raiders left are
   stronger than the defenders left; losing is then commoner and costs less.
   *If no:* a raid stays one number against another, defences are eight prices
   for the same number, rapid fire against defences never happens, and slice 4
   is not built.

4. **Does a fight in open space have OGame's six-round limit?**
   *Default: no.* It goes on until a side is destroyed, has left, or a round
   passes with no hull lost (section 7.2).
   *If yes:* after six rounds the fight stops as a draw with both sides in the
   sector, and starts again only on an `attack` order or an arrival. A fleet that
   wants out waits for the sixth round and jumps without parting fire, which
   makes flight nearly free and pursuit rare.

5. **Is this the price of flight?** A fleet that orders a jump out of a fight
   stops firing and is fired on until it is gone; groups follow by behaviour
   (Patrol one hex, Aggressive and Swarm two) and only what they were beating; a
   fleet that jumps as fast as its pursuer is never caught, a slower one is fired
   on for the difference at every hex; docks are refuges (section 7.4).
   *Default: yes.*
   *If a fleeing fleet should keep firing:* flight costs a slow fleet less, and
   speed matters only in pursuit.
   *If the leash should go by ring and not behaviour:* one table changes.

6. **What does repair cost?**
   *Default: nothing and no time at the homeworld; a tenth of a ship's cost for a
   whole hull, from the store, at the outpost; none at a breached outpost*
   (section 7.3).
   *If home should charge or take time:* two constants, no new code beyond them.

7. **Refit at the home dock, by a stamp on the fleet?**
   *Default: yes*: Weapons, Shielding and Armour only; a merged fleet takes the
   lower levels until it docks; the outpost mends but does not refit (section 9).
   *If research should work everywhere at once:* the stamp is not built, a fleet
   is a little simpler, and a research finished mid-fight strengthens the fleet
   in it.
   *If the outpost should refit too:* one line.

8. **OGame's storage, or this game's tighter one?**
   *Default: OGame's*: 100,000 of each resource with no building (section 8.6).
   *If tighter:* today's 5,000, 3,500 and 2,500 are kept as the base under
   OGame's three buildings and OGame's growth a level, and the early choice the
   Vault is today stays a choice.

9. **Four ships whose OGame job is not brought in, and the starting fleet.**
   *Default:* the Recycler is the ship that mines and strips derelicts; the
   Espionage Probe is what a fleet carries to scan further and read a derelict;
   the Colony Ship is left out; the Deathstar is in if energy is; a new empire
   starts with two Light Fighters (sections 5.3, 5.7).
   *If the Colony Ship should found the outpost:* it answers question 2 of
   `outposts.md` the other way, founding costs 40,000 and a slow escort trip, and
   upgrades are still hauled.
   *If the starting fleet should be smaller:* one constant.

10. **Fuel from the hold, as section 8.5 has it?**
    *Default: yes, at a loss of 15 percent, by an order, not in a fight, built
    first.*
    *If it should be only for deuterium the fleet took in the field:* the hold
    has to remember where its deuterium came from; a day more, and tankers do
    not work.

11. **How pirates heal.**
    *Default:* a group keeps its losses and its damage; lost ships return and
    damaged ones mend, in ten steps, over half the time a destroyed group takes
    to return, which is 1.4 to 3.5 hours at pace 1 for rings 3 to 20 and is
    divided by pace; the clock stops while the group fights or pursues (section
    7.5).
    *If it should not follow pace so closely:* the attention class, at which a
    season's group mends in about 20 minutes and a blitz's in one.

---

## 15. What was read, what the sources are, what is not confirmed

### 15.1 Read in this repository

`CLAUDE.md`; `docs/design/decisions.md` (the entry of 2026-10-13 that commissions
this); `docs/design/outposts.md` and `storage.md` for the form;
`tools/playtest/RULES.md` (the game section) and `README.md` (its outline).

Read in full: `shared/src/constants.rs`, `shared/src/scaling.rs`,
`shared/src/pace.rs`, `sim/src/combat.rs`, `sim/src/odds.rs`, `sim/src/score.rs`.
Read in the parts named in the text: `sim/src/engine.rs` (the types, `tick`,
`process_movement`, `process_combat`, raids, docking and refuelling,
`process_cooldowns`, `handle_move`, `handle_recall`, encounters, `start_combat`,
`break_off_combat`, `add_ship_to_homeworld`, the helper functions at its end).

Surveyed by two reading agents, whose reports were used for sections 4, 8.7, 8.8,
8.10, 10 and 13 and whose line references were not each checked again:
`shared/src/protocol.rs`, `sim/src/views.rs`, `sim/src/intel.rs`,
`shared/src/world.rs`, `sim/src/snapshot.rs`, `sim/src/store.rs`,
`sim/src/bot/`, `sim/src/headless.rs`, `sim/src/bin/sim-run.rs`, every test file
named in section 13, `shared/tests/wire/mod.rs`, `docs/design/economy/sim.py`,
`sim/tests/economy_numbers.rs`, `tools/check_sim_constants.py`, the web client's
windshield (`lib/views/windshield/`, `lib/scenes/windshield/`), its homeworld
screens and `test/support/demo_world.dart`, `clients/tui/src/`, and
`tools/playtest/`.

### 15.2 Run

Nothing of the game: no cargo, no Flutter, no test. One Python script of
arithmetic, kept outside the repository, produced the worked figures: the jump
ticks of section 5.7, the energy table of 5.6, the fold prices of 5.5, the
storage levels and build times of 6.4, the healing times of 7.5, the shot counts
of 7.8 and the float steps of 8.1.

### 15.3 Not confirmed

Every OGame figure in section 6 is in S1. The following rest on S1 alone, either
because S2 describes a later version or because S2 does not cover them.

1. **The classic figures where S2 differs** (the table in section 3): the
   Bomber's fuel of 1,000, the Ion Cannon's cost, the three storage costs and the
   storage formula, the Deuterium Synthesizer's temperature term, natural
   production of 20 and 10, the Solar Satellite's output, the Light Laser's need
   of Energy Technology 1. S1 is a reconstruction of 0.84; no page of the
   original game was read to check it.
2. **The Espionage Probe's shield and attack of 0 and its cargo of 5.** S2 has
   the same with a note that the live game shows 0.01.
3. **How a cost that is not a whole number is rounded.** S1 keeps the fraction,
   S2 cuts it off.
4. **Expedition Technology** (cost, factor, requirements): S1 only. S2 has its
   successor, Astrophysics, at the same level 1 cost and a factor of 1.75. It is
   left out, so nothing rests on it.
5. **In the battle**: that shield damage is counted in whole hundredths of the
   full shield; that a unit rolls to explode only when the shot has left its
   shield at zero; that a round with no hull damage ends the battle; that the
   threshold is "at or under" 70 percent and not "under". The outline (six
   rounds, a random target, the bounce under one hundredth, the roll under 70
   percent at one minus the hull's share, rapid fire at (r - 1) / r, shields
   full each round) is S1's and is what every other description found agrees on.
6. **The defence rebuild's two cases** (each unit at 70 percent under ten lost,
   a share of 60 to 80 percent above): S1 only. That 70 percent return is
   agreed everywhere.

Nothing else in section 6 is unconfirmed: unit figures, rapid fire, level 1
costs, factors and requirements are the same in both sources.

### 15.4 Not known

- **What an empire owns under OGame's catalogue at this game's paces.** No
  simulation of it exists. The pirates' growth, the raid's size, the map's
  multiplier, the outposts' numbers and the presets' estimates all wait on it.
- **What a round and the odds cost to run.** Section 7.8 counts shots; nothing
  was timed.
- **Whether OGame's fight, balanced for six rounds and whole ships, stays
  balanced without the limit and with damage that lasts.** Both changes favour
  the side that deals more damage and take away the draw that large hulls and
  shields earn in OGame. Raids keep the limit for that reason. In open space it
  is a thing to watch in the first playtest of slice 2.
- **Whether mining still reads as mining** when no hold is ever full of ore
  (section 8.3).
- **How the windshield should show size.** Section 10.2 says what it must show
  and leaves how to a design pass with the glyphs.
- **Everything about PvP**, which does not exist in the engine. The round is the
  same for any two sides, so nothing here stands in its way.



