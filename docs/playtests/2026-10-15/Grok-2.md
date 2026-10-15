# Grok-2 feedback

Tick 3630, session over. Rank 1 of 4. Crystal Mine 18 finished at tick 3612, after the clock had already printed 0 minutes, and the score includes it.

## 1. Result

Score 5625.5. Buildings 3223.6, research 2252.9, fleet 149.0, outpost 0, defence 0.

Second is Grok-3 at 5076.1 (gap 549), then Grok-4 at 4680.1, then Grok-1 at 3953.8. Started the hour around score 9, tied for second, with two Light Fighters and mines at level 1. At tick 3410 the lead over Grok-4 was about 825; Grok-3 then gained about 700 in the last few minutes and became second. The lead held.

Stockpile just after Crystal Mine 18, tick 3630: metal 238429, crystal 29073, deuterium 70587. Income 1169 / 670 / 273 per second. Energy 4122 made, 3883 needed, fraction 1.0. The world kept ticking after SESSION OVER, so the stockpile was still climbing at tick 3646 and the score was not.

Buildings: Metal Mine 19, Crystal Mine 18, Deuterium Synthesizer 17, Solar Plant 19, Fusion Reactor 9, Robotics Factory 8, Research Lab 8, Shipyard 1. No storages, no nanite factory.

Research: Espionage 3, Computer 7, Weapons 8, Shielding 7, Armour 8, Energy 7, Hyperspace 4, Combustion 6, Impulse 5, Hyperspace Drive 2, Laser 9, Ion 7. Plasma and Graviton not started.

Ships: the same two Light Fighters, hull full, power 327, plus 35 Solar Satellites (fleet score 149 = 9 for the fighters and 140 for the satellites). No losses. No outpost. No defences.

Energy Technology 7 finished at tick 3478 (research 2124.9 to 2252.9). Crystal Mine 18 had been waiting, short on crystal, since before the wrap-up; it paid at tick 3583 and finished at tick 3612 (buildings 2975.7 to 3223.6). Every queue was empty after that.

The home raid forecast was LOST the whole second half of the hour. After Crystal Mine 18 it was power 5635, 62 Light Fighters, lost in every sampled fight, keeping 0% of what stands at home. Shipyard stayed at 1, so that raid never launched.

Unranked, the built-in bots sit beside that table. BOT-mixed-15 finished at 6426.5 with a fleet part of 1495, ahead of every agent. The highest agent fleet part on the board is 187.5.

## 2. Strategy

Spend every resource. Mines for income, solar and fusion and satellites for the energy those mines need, research for raw score whenever the lab would otherwise sit. Three build slots after Computer 6. Never Shipyard 2, never a defence: both start raids, defences score 0, and two fighters cannot hold a 62-fighter raid.

What worked was keeping the lab and the mines ahead of the stockpile, and treating deuterium as the research gate. Solar satellites were the right energy buy once a solar plant cost more energy-per-point than a satellite (30 energy, 4 score, two seconds, no raid). Fusion 6 through 9 was a cheap energy patch and a large score chunk. Energy Technology 7 then raised the Fusion Reactor by about 58 energy, which is why Crystal Mine 18 (about 141 more energy needed) still left the fraction at 1.0. Fighting only at SAFE (the inner-ring convoys were 2.5x to 6x) brought wreckage and charts home without losing a ship. Docking refits and mends for free, and that refit is why the same two fighters ended the hour at power 327.

What did not work was queueing a solar plant and then letting a cheaper order eat the metal and crystal it was waiting on. Solar 19 sat for a long time while deut mines, armour and drives paid first. The jump rule is documented and it is correct, but I kept doing it to myself. Also, a hold that is already full of one wreckage cannot lift the next one: at -1,-6 the scoop took 992 metal and left 373/744/372, and at 2,-7 it took 366 metal and left 1045/770/385. Going back for the remnant found nothing.

## 3. Fun and clarity

The tool output is clear enough to play the hour without guessing. Queue lines name the short resource and the seconds of income. Odds are a label, not a number I had to interpret. The raid sentence says the ships, the odds, and that I would keep nothing. Refusals name the shortfall and, for ships, how many the stockpile covers.

What I had to keep in my head:

- `ok: true` on a building means the stockpile can pay. It does not mean the mines stay powered. Deuterium Synthesizer 15 was payable while the surplus was about 20 energy and that level needs about 190. I only caught it by subtracting `needed` from `made` and adding the level's energy by hand.
- A Passive group waits for `attack`. A Patrol starts the fight on the tick you enter. Moving into 5,-8 opened combat with no attack order: `t3087 fleet 80 entered 5,-8` and `CombatStarted` on the same tick, enemy "2x light fighter patrol".
- A queued order's `start_in` is not a promise. Combustion 6 was "starts in ~2m" and paid a few seconds later. Deuterium 15's row said 136s while the queue note said the first order covered was that mine in ~26s. The note was the one that came true.
- Wreckage is supposed to last 180 ticks. Several piles were gone in well under a minute. Details in the next section.

The hour was fun. The dock, the refit line, and a two-round fight at 5x are all easy to read.

## 4. Bugs and oddities

Wreckage disappeared far inside the 180-tick life, with the fleet still in the sector or back within a minute. The error was always `NoResources no salvage in this sector`.

- 2,-7, early: destroyed the convoy, wreckage 1614/880/440. Two collects about 40 seconds apart both returned no salvage.
- 2,-7, again: `t2971 fleet 80 scooped 366/0/0 of wreckage @2,-7 into its hold, 1045/770/385 left on the ground`. Back at tick 3024, 53 ticks later, still `NoResources no salvage in this sector`. Nothing in the sector line named another fleet.
- 5,-8: the patrol died at tick 3089, `wreckage 1622/884/442 added to the pile there`. Collect at tick 3107, 18 ticks later, before the move off the tile: `ERR collect_salvage 80: NoResources no salvage in this sector`. The sector view had not listed another player.

A full hold scoops metal first and leaves crystal and deuterium. Quoted above. That may be intended. It stranded the resources that score at 1.5 and 2.

`built 1x SolarSatellite` printed in the middle of an order for 4, then a later line said `built 4x SolarSatellite` or `built 3x`. The count is what finished that tick, not what was ordered. Easy to read as a partial build.

Hostile ids survive destruction. 8589541377 was the 1,-6 convoy every time it came back. 8589541376 at 0,-6, 8589475840 at 0,-7, 8589475842 at 2,-7, same ids hours apart. Useful once I noticed. Surprising the first time.

Entering a Patrol sector starts combat by itself, as above. Passive sectors do not.

## 5. Balance

At this pace the score line and the raid line disagree. Raid power grew with the economy, from about 22 Light Fighters to 62 and power 5635, while the only fleet that does not itself trigger raids is a handful of fighters. Shipyard 2, every warship past the Light Fighter, and every defence were losing moves for an agent who reads the forecast: it was already LOST, defences score 0, and a cruiser needs Shipyard 5. Optimal play on the agent table was mines, energy, and research, plus two fighters as a wreckage truck. The shipyard and the defence tab were traps.

The unranked bots do not play that way. BOT-mixed-15's fleet part alone (1495) is two thirds of my whole research score, and its total (6426) beats every agent, including this one. Either those bots are eating raids and still coming out ahead, or the forecast overstates a raid that a real fleet can hold. From this seat I cannot tell which. What I can say is that an agent who trusts the forecast never finds out, because building the yard is what starts the raid.

Inner-ring packs were never a threat after Weapons 6. Two fighters at 3x to 6x SAFE won every fight in two or three rounds and lost nothing. That is a fine tutorial. It stopped being a decision.

Crystal, then deuterium, gated every queue. Metal was the one I could spend on armour and solar. Solar satellites dominated energy past Solar 16 and also scored their full cost. That part feels right.

Unspent stock at the end was still large (about 238k / 29k / 71k right after the last mine) because the next levels each want more crystal than a minute of income. The score rule punishes that, which I understood. The waits are just long. With three minutes left, both free building slots and the lab were idle on orders that were short. Crystal Mine 18 only became payable in the last half minute, and it did finish.

## 6. Top 3 suggestions

1. Do not scale the home raid with the score of an empire that is not allowed to build a fleet yet. Either the forecast stays holdable until Shipyard 2 is a real choice, or building the shipyard does not itself start the raid. As it stands, a player who reads the forecast never builds one, and a player who does not read it loses the homeworld. The bot table suggests a fleet can still be the highest score in the world; the agent table never tested that.
2. Keep wreckage for the stated 180 ticks, and if another fleet takes it, say who. A scoop of a full hold should not fill metal and leave the crystal and deuterium on the ground when those are the short resources.
3. Put the energy consequence on the building row. `ok: true` plus `surplus 50, this level needs 190, fraction would fall to 0.95` would have saved a brownout. The raid forecast already does this kind of sentence well. The mine list should too.

## 7. The play tool

The prose status and the state JSON together are the right interface. Refusal text is specific. `--keep-going` saved batches when a scoop failed. Newline-separated JSON in one argument is the reliable way to send several orders.

What I kept paying for in extra calls:

- `map` shows `HOSTILES x1` and an odds label, not the fleet id or the ship counts. Every attack was a `sector` call first.
- A one-tick jump does not finish inside the same `do` as the move. `attack` on the next line of the batch is sent before the fleet has landed. I learned to wait.
- `wait --until idle` waits out the clock while every queued order is short and the slot sits empty. The status note says to order something payable. A wait that returns when a slot has been idle for a few seconds would match that note. Near the end, a wait with the lab and both free build slots idle returned "nothing actionable" even though crystal income was about to cover the next order. The same wait did return when Crystal Mine 18 finally paid, which is what the flag is for.
- `built 1x SolarSatellite` should say that one of the four finished this tick.
- An energy surplus on the status line, next to `made` and `needed`, is the number the whole build plan hangs on. It is a subtraction today.
