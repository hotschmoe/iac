# Grok-1 feedback

Snapshot at tick 3319, 4.7 minutes left, when the tool said to wrap up. Rank 4/4 the whole second half. Two orders were still going to finish after this snapshot: Solar Plant 19 (short about 30k crystal, starts in ~72s, builds in 21s) and Armour Technology 9 (about 73s left). Armour Technology 10 is queued to start in ~290s and build for 192s, which is past the end of the session, so it will not be paid for and will not score.

## 1. Result

Score 3520.5. Buildings 1889.2, research 1482.8, fleet 148.5, outpost 0. Grok-2 led at 5096, 1575 ahead (buildings +933, research +642, fleet about even).

Homeworld 4,-2, ring 4. Stock at the snapshot: metal 182,121, crystal 14,940, deuterium 89,860. Production per second: metal 1140, crystal 416, deuterium 211. Energy 2811 made, 3336 needed, mines at 84%, 27 Solar Satellites. Metal Storage 4 had just raised the metal cap from about 1.86e6 to 7.44e6, and raid-protected metal appeared at 2.38e6 (it had been 0/0/0 all session).

Buildings: Metal Mine 20, Crystal Mine 16, Deuterium Synthesizer 15, Solar Plant 18, Robotics Factory 8, Research Lab 7, Shipyard 1, Metal Storage 4. Fusion, nanite and the other storages at 0.

Research: Computer 8, Energy 7, Laser 8, Weapons 6, Armour 8, Shielding 4, Impulse 5, Combustion 5, Ion 2. Espionage, Hyperspace, Hyperspace Drive, Plasma and Graviton at 0.

Ships: fleet 1075, 9 Light Fighters, full hull, manual, idle at home. No defences. No outpost. 39 sectors known, almost all stale. One fight all session, at 3,-1 around tick 250: the hostile group died in two rounds, no ship lost, wreckage reported as 847 metal / 462 crystal / 231 deuterium. That wreckage was gone by tick 277.

## 2. Strategy

I treated score as cost weight and tried to keep every queue full of things that would finish. Shipyard stayed at 1 and I built no defences, because either one starts raids and the forecast was a total loss the whole time (13 Light Fighters early, 43 by the end, kept 0%). That part was right. A home garrison cannot win that fight, and a lost fleet leaves the score.

What worked was spending metal on finished levels. Computer 7 and 8 and Energy 7 are where the research score actually moved, because costs double. Armour 7 and 8 were the right late spend: 64k then 128k metal, no crystal, and they finished in well under a minute at this pace. Crystal Mine 16 and Metal Mine 20 scored on completion and raised income. Metal Storage 1 through 4 turned surplus metal into building score in about a second a level; I found that too late to take it past level 4.

What did not work:

- Patrol. Fleet 78 (2 Light Fighters, patrol_home, range 4) lost one fighter. I set it to manual and merged it into the home stack. Ring fights and charts were small next to a mine level, and the wreckage from the one win was gone before `collect_salvage`.
- Energy. Mines are cheap and start at once. Solar plants wait on crystal. The mines pulled ahead, the fraction fell to about 62%, then 76%, and again to 84% at the end when Metal 20 and Crystal 16 finished while Solar 19 was still 30k crystal short. Twenty-seven satellites fixed one brownout and then ate the crystal the lab needed.
- The research queue. For several minutes it held Computer 7, Computer 8, Energy 7 and Energy 8, every one of them short of crystal, with no payable tech behind them. The lab slot sat idle. Research score stuck near 454 until those lines were cancelled and the crystal was allowed to reach Computer 7 (it was only about 7k crystal and 17s short when I looked).
- Hoarding crystal for the whole waiting research list. That kept the build queue empty while more than 500k metal sat unscored. Protecting the next item that starts within a minute is enough. Protecting levels three to ten minutes out freezes the score.
- Attention. At x1200 a minute of deciding is a minute of idle slots. The rank was lost in those gaps and in the stalled lab, not in the last fight. I also never sent a fleet to found an outpost. That is a real score line I left at zero.

Started tied for first at 9 points. Fell to 4th around a score of 670 and stayed there. At tick 3077 the board was Grok-2 4459, Grok-3 3924, Grok-4 3616, Grok-1 2871. The building gap was already the game.

## 3. Fun and clarity

The rules and `./play status` are clear enough to play from. Score formula, the idle-queue note, energy made against needed, and "short by N, about Ns of income" are the right information. I could see why a slot was idle and what it was waiting on.

The raid line is the confusing one. With Shipyard 1 and no defences, status still ends in:

`ALERT: next raid, at its typical size: power 3932, flying 43x LightFighter; forecast LOST and you LOSE about 100% of what stands there`

That is the typical-size forecast, not a raid on the clock. It reads as an incoming wipe. I played the whole hour as if combat were closed, which matched the rule that Shipyard 2 or any defence starts raids, and it meant flight, pursuit and the hub never mattered to the score.

Queue payment took several mistakes to learn. Buildings and research are paid when they start. Ships and satellites are paid in full or refused in full. `reserve` holds a line that is short of resources and, together with other short lines, leaves the free slot idle. A short order does not block a cheaper one behind it, which is why a payable armour level could start in front of a crystal-starved energy level. None of that is said in one place.

New ships joined a new fleet because fleet 78 had standing orders. Easy to miss until two fleets are sitting at home. `merge` is the fix. Standing orders also send a fleet out while you are looking at the economy, which is how the fighter died.

`score` lags the levels by a few seconds (`age_s` was 6 while Crystal Mine already showed 16 and the building score had not moved). Fine once you know to read `age_s`.

## 4. Bugs and oddities

- Wreckage vanished. Tick 250 combat ended `hostiles_destroyed`, wreckage 847/462/231, two rounds, no losses. Tick 277 the sector had no salvage. Tick 333 `collect_salvage` on fleet 78 at 3,-1 returned `NoResources` "no salvage in this sector", and because that order was first the rest of the batch was not sent. A pile is supposed to last 180 ticks. Either another player lifted it with no event I could see, or the pile did not stick. I stopped counting on salvage after that.
- Solar Plant `MaxLevelReached`. Cancelling solars that were a few minutes out and queueing them again counted the queued levels toward the cap of 20 while the built level was still 16. A later cancel of one id reported "Solar Plant Lv.19" for an order that had been queued as level 20. Ids move. Re-read the queue before cancelling.
- A full queue of unaffordable orders idles the slot. The server said it plainly: "every waiting order is short or waits for a level still to be built, so the free slot stays idle... Order something payable to use the slot now." That happened to buildings (reserved solars, short of crystal, 200k to 500k metal idle, room 0) and to research (four crystal-heavy levels, lab idle for minutes). It is consistent. It is also how a leading research score goes flat while the stockpile fills.
- Ships are all or nothing. `Solar Satellite` x4 cost 8000 crystal and 2000 deuterium, short 1271 crystal, `InsufficientResources`, nothing queued. A smaller count would have been payable. The refusal does not offer one.
- Cancel races. `cancel_queued` Research 3523 returned `InvalidTarget` "no order 3523 in any queue: it finished, was cancelled, or the id is wrong". The Impulse Drive 5 order finished between the snapshot and the cancel. With `--keep-going` the rest of the batch still ran. Without it, that error would have dropped the orders that mattered.
- Metal Storage 4 changed the metal cap from about 1.86e6 to 7.44e6 and set raid-protected metal to 2,382,348 in one tick. I had not seen protected stock leave zero before storage existed. Worth confirming the jump is the formula and not a cap being replaced rather than added.

## 5. Balance

At x1200, build time, research time and mine output scale. Fuel, cooldowns and combat do not. An hour of play is an economy game with a fight attached. One mine level is worth more than a patrol, a chart and a wreck, and it scores the same cost again as income. I stopped exploring because the numbers said to. The other players who stayed on buildings and research are the ones ahead.

Defences score 0, and the thing that lets them shoot (Shipyard 2, or any defence) starts a raid the forecast says you lose completely. Raid size grew with the economy, from about 13 fighters to 43, while a 9-fighter garrison stayed a forecast loss. There is no scored reason to fight, to found a defensive platform, or to upgrade the yard. Combat, flight and pursuit are the interesting parts of the game and they were the wrong score play for the entire hour.

Crystal is the limiter. Metal stops being the constraint somewhere around Metal Mine 15 and Solar 18, and then hundreds of thousands of metal score nothing until you notice Armour and Metal Storage, which are pure metal and finish in seconds at this pace. Storage in particular is a score sink: level 1 is 2 points, and the costs double, so a stockpile of 600k metal late in the hour is several hundred building points if you start the chain early enough. I only cashed four levels (2+4+8+16). That sink does not ask you to play the rest of the game.

Research doubling makes the last completed level the whole story. Missing Computer 8 is about 230 points, not a small tech. A lab that idles for five minutes at this pace is a rank.

Solar satellites are a weak energy patch once crystal is the research bottleneck: 2000 crystal and 500 deuterium for 30 energy and 4 points. A Solar Plant level costs more crystal and returns hundreds of energy plus the metal cost as score. The plant cap at 20 is reachable in this hour, and past it the mines brown out again with no good fix.

## 6. Top 3 suggestions

1. If the head of a queue cannot be paid, start the first order that can. A queue of four short techs should not idle the only research slot while a 64k-metal armour level would have started. The note already tells the player to do this by hand. Doing it in the engine would remove the easiest way to stall an empire, for a person and for an agent. `reserve` can stay as the explicit "hold this line" switch.
2. Say when no raid is coming. Keep the typical-size forecast, and label it as the raid you would get if you raised the shipyard or built a defence. The current alert reads as a countdown. While that is true, every scored player will pin the yard at 1 and the combat game will not be played. If raids are meant to be part of the hour, they need a path where a garrison can win one and the win is worth the cost. Defences scoring 0 plus a forecast of "lost, keep nothing" closes that path.
3. Put a finish time and a score on `next`, and put salvage on the sector. `next.buildings` and `next.research` showed cost and `ok` and a `time` of null. The queue shows ticks only after you order. A column of "score if this finishes" and "done with Ns left in the session" would make the last ten minutes a plan instead of a guess. On salvage: the sector view should show the pile and the tick it disappears, and a scoop by someone else should be an event. A reported 847/462/231 that is gone 27 seconds later, with `collect_salvage` saying there is nothing there, is not something a player can learn from.

## 7. The play tool

`./play status` is the right screen: rank, the gap split by buildings and research, stock, production, energy, queues with the shortfall in seconds of income, the raid forecast, the fleet. `./play do --keep-going` is necessary. One refused `collect_salvage` used to drop every later order in the batch. `wait --until idle` with a 60s cap matches the tick. `minutes_left` on every call, and the wrap-up line under 6 minutes, are what made the hour playable without a clock of my own.

What I kept looking up by hand:

- Payable orders. State has the costs under `next`, and the idle note names one short order. It does not list what would start if you ordered it now. I computed that outside the tool, slowly, and the slots idled while I did it.
- Which key is which. `state --keys` with the names I expected (`buildings`, `production`, `rank`) answers with the real key list. That recovery is good. The names (`levels`, `per_s`, `score` as an object) took a wasted call at the worst minute of the session.
- A stale cancel. The error is clear. A cancel that finds the order already finished could be an `ok` with "already gone" so a batch written against the last snapshot does not look like a failure.
- Session horizon. I had to add queue `ticks` to `minutes_left` myself to see that Armour 10 would not start and that Impulse 6 would not finish. The queue already knows `start_in_s` and `ticks`. Marking "will not finish this session" there would have saved the crystal I locked in Energy 8 and the metal I nearly locked in Armour 10.

I drove the hour through `./play` only, in batches, and the tool held up. The losses were mine: idle slots, a stalled lab, and a patrol. The tool's idle note was the most useful sentence it printed.
