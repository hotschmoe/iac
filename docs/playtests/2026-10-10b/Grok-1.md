# Grok-1 feedback

Wrap-up snapshot at tick 3408, 3 minutes left. The server was still running; Shipyard 6 and Modular Fabrication 2 were still waiting on resources and may finish after this note.

## Result

Rank 5 of 6. Score 322.6 (core 262.3, combat 6.9, explore 53.4). For almost the whole hour the explore+combat bonus was capped at 25% of core. It uncapped in the last few minutes: 6.9+53.4 = 60.3, and 25% of 262.3 is 65.6, so a little explore is scoring again.

Leaderboard at tick 3408: Explorer 445.1 (core 397.3), Raider 442.3 (core 399.9), Mixed 405.0 (core 384.1), Grok-2 402.5 (core 327.0), Grok-1 322.6 (core 262.3), Builder 263.0 (core 263.0). Builder had no combat and no explore, so their score was pure core. I was ahead of Builder on score and a hair behind on core.

Home 6,-3. Stock 3345 metal, 1393 crystal, 4497 deuterium. Income 52.31 / 30.26 / 14.29 per second. Caps were never close (78k / 55k / 39k).

Buildings: Metal Mine 11, Crystal Mine 10 (finished tick 3381), Deuterium Synthesizer 8, Shipyard 5, Research Lab 5, Fuel Depot 6, Sensor Array 6, Defense Grid 4, Fabricator 6, Storage Vault 0.

Research: Fuel Efficiency, Extended Tanks, Reinforced Hulls, Advanced Shields, Weapons, Navigation, and Harvesting all at 5. Corvette, Frigate, and Hauler tech done. Emergency Jump 3. Modular Fabrication still 1. Cruiser Tech 0, locked behind Shipyard 6.

Ships, all docked: 1 scout, 6 corvettes, 26 frigates, 6 haulers. 15 pulse turrets. No lancers, no ion bastions. Defence 2432 against a raid estimate of about 570.

Still queued at tick 3408: Shipyard 6 (needs 434 metal and 497 crystal, about 17s), Modular Fabrication 2 (needs 1007 crystal, about 34s, then 44s of lab time). The shipyard queue is idle. Those two may or may not finish before the hour ends.

Two raids defended with zero resources lost. Tick 2001, raid power 323 vs defence 1035. Tick 3075, raid power 502 vs defence 1782. Each raid destroyed 2 structures, which rebuilt on their own. The second raid dropped salvage 2760/690/414; scooping it is what paid for Metal Mine 11.

## Strategy

Score is core plus a capped explore/combat bonus, and unspent stock scores nothing. I treated core as the only number that mattered once charts and the early pirate fight had filled the bonus.

What worked:

- Mines first. They cost no deuterium, and the payback lines in `./play costs` were still a few minutes when half the hour remained. Metal 11, crystal 10 (finished at tick 3381), and deut 8 are why income ended at 52 metal and 30 crystal per second.
- Lab, shipyard, and fabricator as soon as the mines were not the only thing that could start. Fabricator 6 and lab 5 made every later level faster.
- A docked fleet as the real defence. Structures score half, ships score full, and a home fleet counts on defence. After the opening I kept both fleets on manual at 6,-3. Both raids were easy.
- Frigates as the score ship. One frigate is 1000/400/200, about 2.0 core, and the shipyard finishes it in a few seconds at shipyard 5. Haulers (600/200/150, about 1.2 core) were the filler when metal was under 1000. I ended with 26 frigates and 6 haulers.
- Moving a slow waiter off the front of the queue. A building that cannot pay holds both build slots, even when something behind it could pay sooner. Cancelling the front item and requeueing it at the back is how Fuel Depot 5, Sensor 5, Fabricator 5, Defense Grid 4, and Fuel Depot 6 actually started.
- Scooping raid salvage immediately. The second scoop was worth more than a minute of income and it started Metal Mine 11 on the same tick.

What did not:

- Prospecting with the only ships on the board. Explore hit the 25% cap early. A scout died at 9,-8 to a corvette patrol, and the fleet had to walk home the long way because 5,-8 has no east lane (`NoConnection`). After that, more charts did not raise the score until the last few minutes, when core finally grew past the cap.
- Sitting on an unaffordable mine. Crystal Mine 9 and several later mines spent 40 to 80 seconds as the front waiter with both build slots idle. Leaders pulled away during those windows. Rank stayed 5 from roughly the midpoint onward.
- Letting frigates snipe a payment that was a few seconds away. A frigate batch costs less crystal than a mine or a shield level, so it pays first, empties the stock, and the building timer starts over. I did this on purpose too often. Cancelling to undo it then deleted the wrong index.
- Modular Fabrication 2. The third build slot is 3000/2400/1000 and 44 seconds of lab time. I queued it all game and never reached it, because shields, frigates, and mines spent the crystal first. I believe that missing slot is a large part of a 130-core gap to Mixed, Explorer, and Raider.

## Fun and clarity

The game is understandable from RULES.md plus the tool. Every status repeats the score formula. `./play costs` shows the next three levels, the build time, what it unlocks, and mine payback in minutes. Shortfalls are in "seconds of production," which is the right unit. Threat labels (SAFE through DEADLY) and raid warnings with an arrival tick are clear.

What confused me:

- Two build slots do not mean two waiters can start. If the front waiter cannot pay, the free slot stays idle. I learned that by watching Sensor and Fuel sit behind a mine that was a minute away.
- "Accepted (already finished or not visible yet)" looks like success. It often means the order was not queued.
- Salvage has a lifetime, but RaidResolved does not say so. I found out by `SalvageDespawned` about three minutes later.
- Hex lanes are not the six geometric neighbors. The exits list is the authority. I tried to step east from 5,-8 and got `NoConnection`.
- Standing orders are a `policy_update` message, not an action. The rules say this. It is still an easy mistake.
- Fabricator level speeds construction. Modular Fabrication adds a queue slot. The names sit next to each other and I mixed them up while planning.
- `minutes_left` is a whole number, so "6 min left" lingers. The `WRAP UP` line on the tool is the signal that actually matters.

I would play this again. The economy puzzle (which queue is allowed to spend the next crystal) is the game, and the tool gives enough information to play it without a GUI.

## Bugs and oddities

- Cancel index drift. Index 0 is the first waiting item, and it changes as soon as something starts. Around tick 2201 I meant to clear a deuterium mine that had already started; the Building cancel removed Shipyard Lv.5 instead. A batch of several `cancel_queued` index 0 calls is a race against the tick.
- Queue type ignored, or the summary names the wrong queue. At tick 3047 the order was `cancel_queued` with `queue_type` Ship and index 0. The tool replied `cancelled Sensor Array Lv.6`. The Queue event on the same tick is `queue_type: Building`, item `Sensor Array Lv.6`, action `Cancelled`. The next order, a real Building cancel, returned `InvalidTarget nothing is waiting at that position`. The frigate I was trying to cancel had already started (`Frigate x2` Started at tick 3045), so the ship queue had nothing waiting. The cancel should have failed. It removed a building.
- Earlier in the session, `cancel_queued` returned `QueueFull` with a note about "2 running + 3 waiting" when the running set changed mid-batch. That error text sounds like an insert failure, not a cancel failure.
- Full queue reported as success. A Crystal Mine order while the build queue was full came back `accepted (already finished or not visible yet)` and did not appear in the queue.
- A research order before the lab existed was reported as `ERR build ResearchLab` even though the action was research Corvette Tech.
- Salvage timer. Raid at tick 2001 dropped 1800 metal, 450 crystal, 270 deuterium. `SalvageDespawned` at tick 2181, same sector 6,-3. A `collect_salvage` a few seconds later returned `NoResources no salvage in this sector`. The second raid's salvage (tick 3075, collected tick 3100) was still there. The window is on the order of three minutes and nothing in the raid event says that.
- `./play wait` returns immediately when any idle queue has something it could build. Late in the game that was `ship queue idle and now payable: Frigate` or even `PulseTurret`. A timed wait cannot be used to let a building finish accumulating if a scout or a turret is affordable. I had to queue an unaffordable frigate batch as a placeholder so the wait would actually sleep.
- The one-line score on a status that lands on the same tick as a completion sometimes omits that completion. Sensor 6 and several research levels showed up in the next call, not the call that reported `built`.
- Repeated combat noise earlier: the same pirate label in the same sector fired CombatStarted again after a kill, and prospect spent a lot of events on fuel-return holds. A one-line cancel summary has also disagreed with the Queue event underneath it. Trust the event.

## Balance

The 25% cap does its job and then some. Exploration is correct for the first few minutes and then worthless until core is enormous. Fighting is worse than worthless once you are over the cap: a lost ship comes out of core, and the kill does not raise the score. I stopped leaving home. The galaxy became a raid timer and a chart I already had.

Defences scoring half makes pulse turrets a tax you pay before the first raid and then ignore. That is fair. Ion bastions (1200/500/100, half score, needs Defense Grid 4 and Weapons 4) were unlocked and still a bad buy next to a frigate. I built none.

Mine payback is the best number on the costs screen. It stayed short enough to chase until roughly the last 12 minutes, then Crystal Mine 10 (payback 11 minutes) was no longer a premium over just building frigates. The flip is abrupt and the tool tells you. Good.

Shipyard 6 plus Cruiser Tech (8000/5000/2500, 104 seconds, and the ship itself is 3000/1500/800) does not fit in a one-hour blitz if you also build mines. Cruisers were never a real option. Frigates were the whole navy.

Raids scaled slowly. The second raid was still labeled light, at half a thousand power, against a home defence of ~1800. After the opening, raids were free salvage rather than a threat. The warning 30 ticks out is enough time to recall a fleet. I liked the warning. The threat stopped mattering.

Deuterium is the quiet constraint. Mines cost none, so the economy looks healthy while fuel depots, shields, and emergency jump empty the deut tank. I hit a few hundred deuterium more than once and only noticed because a shipyard level listed a deut shortfall.

Modular Fabrication 2 is the strongest single level in the blitz and the hardest one to cash. Anyone who fits it in during the first half of the hour gets a third build slot for the rest of the session. I never got there. That feels like the gap between rank 5 and the top three more than any combat trick.

## Top 3 suggestions

1. Let a free build slot start a later waiter that can pay. A resource-blocked front item freezing the other slot is the main way a competent queue goes idle. If that rule stays, print `held by [0]` on every later item so the seconds-of-production number is not read as "this can start in 14s."
2. Put a despawn tick on `RaidResolved` salvage, and keep an alert up until it is collected or gone. The scoop is a large swing (the second one bought a metal mine) and it is easy to miss if you are watching queues.
3. Make a rejected order an error. `accepted (already finished or not visible yet)` should not be the text for a full queue or a missing prerequisite. And `cancel_queued` should fail when that queue has nothing waiting, rather than cancel an item in a different queue.

## The play tool

`./play status --brief`, `./play costs`, and `./play do` with several JSON orders are the right interface. I could run the empire from those three. Costs with payback, unlocks, and "cannot pay yet" is the screen I trusted most. The score line on every call, including the cap, meant I did not have to remember the formula.

What would have helped:

- Cancel by item name, or return the queue after each cancel in the same batch before the next index is interpreted. Index 0 is a race.
- A wait that ignores "a cheap ship is payable" and sleeps until a named queue ticks or the timer ends. `wait --until idle` and even a plain `wait 40` both return the moment a turret or a frigate could be bought, which is most of the late game.
- One line on the idle build queue: "slot free, front item short 1187 crystal, nothing behind it can pay either." I reconstructed that from the shortfalls every time.
- The `WRAP UP` banner is timed well and impossible to miss. Keep that.
