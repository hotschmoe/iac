# Grok-2 playtest feedback

Wrapped up at tick 3469 with 2 minutes left. Rank 6 of 6. Queues were already idle after Metal Mine 10 finished near tick 3214, so the last minutes only grew the stockpile. My score did not move between the tick-3349 check and this one.

## 1. Result

Final stockpile at tick 3462: metal 11361 / 1363848, crystal 12264 / 954694, deuterium 7651 / 681924.
Production per tick: 43.23 metal, 24.76 crystal, 11.37 deuterium (about 1.56e5 / 8.91e4 / 4.09e4 per hour).
Raid-protected stock: 218216 / 152751 / 109108. Nothing near a cap.

Buildings: Metal Mine 10, Crystal Mine 9, Deuterium Synthesizer 7, Shipyard 3, Research Lab 4, Fuel Depot 2, Fabricator 3, Sensor Array 3, Storage Vault 2, Defense Grid 1.

Research: Modular Fabrication 2 (3 build slots), Extended Fuel Tanks 3, Advanced Shields 2, Navigation 2, Fuel Efficiency 1, Weapons Research 1, Harvesting Efficiency 1, Corvette Tech 1. No frigate or cruiser tech.

Ships: 19 corvettes and 2 scouts. Fleet 6 (1 scout + 19 corvettes, power 427) and fleet 230 (1 scout, prospect) both idle at home. No defence structures. Home defence power 445 against an expected raid of about 246. No raid landed on me. Known sectors 144 (35 live).

Leaderboard at the wrap-up `./play score` (tick 3469, 2 minutes left):

| rank | name | score | core | combat | explore |
|------|------|-------|------|--------|---------|
| 1 | Sonnet-1 | 375.0 | 375.0 | 0.0 | 0.0 |
| 2 | Sonnet-2 | 368.6 | 368.2 | 0.2 | 0.3 |
| 3 | Haiku-1 | 357.3 | 355.4 | 0.8 | 1.2 |
| 4 | Haiku-2 | 315.5 | 312.9 | 1.8 | 0.7 |
| 5 | Grok-1 | 133.4 | 129.0 | 4.1 | 0.3 |
| 6 | Grok-2 | 78.9 | 77.0 | 0.0 | 1.9 |

Sonnet-2 passed Haiku-1 earlier and added about 1.2 core between tick 3415 and 3469. Grok-1's combat ticked from 3.9 to 4.1. My score was unchanged (raw event: core 76.952, explore 1.9298102). The raw leaderboard event at tick 3469 still shows Sonnet-1 explore 0.01, printed as 0.0 by `./play score`. The prospect scout held again at tick 3424: "nothing uncharted and safe within 8 hops of home".

I took over an empire that had already been idle: first `./play status` was tick 1283, 39 minutes left, mines at 3, shipyard 1, lab 1, score 3.9, and every queue empty. By the time queues were busy again the leaders were near 220. The rest of the hour was catching up on mines and never getting close.

## 2. Strategy

Score is the weighted cost of finished buildings, research and ships, plus at most 25% from combat and exploration. I treated production as the bottleneck and the building queue as the thing that raises production.

What worked:

- Modular Fabrication 1 then 2. Three build slots, and a second mine running beside the first, is the biggest speedup in the tree.
- Upgrading whichever mine added the most weighted output per second of build time. Metal stayed the scarce resource. Crystal and deuterium piled up.
- Keeping corvettes docked. They are full-value score and they are the defence. Power went from 18 to 445 while the raid estimate went from about 30 to 246. I never needed a turret.
- One scout on `prospect` (max range 8). It came home and exploration scored 1.9, the best explore total on the board. The cap would have allowed about 19, so the scout was worth sending and not worth a fleet. At tick 3364 it scanned once more and held: "nothing uncharted and safe within 8 hops of home; raise max_range or pick another doctrine". The chart inside that ring was finished.

What did not:

- Spending metal on corvettes and on small research while a metal mine was queued and short. A mine that needs 1025 metal waits. A shield tech that needs 100 starts immediately, because a blocked order does not hold the queue. Several of those and metal production never catches the next mine. Metal Mine sat at 7 for several minutes because of this, with 3 build slots idle.
- Saving a crystal reserve for Modular Fabrication 2. That reserve also froze the build queue. Better to start the tech and let mines run.
- "Score sink" buildings while metal was short. Sensor Array reached 3, Storage Vault 2 and Defense Grid 1. They scored their cost and did nothing for production. At x600 the stockpile cap is already ~600k, so the vault is decorative. The grid unlocked turrets I never built, because turrets score at half their cost and docked ships do not.
- Frigates and cruisers. Shipyard stayed at 3. The yard was never the limit. Metal was. Unlocking a hull that spends metal faster would not have raised the score.
- Fighting. At 8x enemy power a kill is worth no combat score, and a single corvette is worth about 0.17 even when it counts. I left the home patrol alone. Combat stayed 0.

## 3. Fun and clarity

The rules file is good. Building names, the score cap, queue depth, "waiting orders are free until they start", pace, and the threat labels were all understandable from the text plus `./play costs`. I could plan without guessing times.

What confused me:

- Threat label DEADLY at 1.1x. Two scouts (power 18) on top of one corvette (power 16) were called DEADLY. The bands are SAFE 3.0, FAVOURABLE 2.0, RISKY 1.2, else DEADLY, so 1.1x is DEADLY even though it is a close fight, not a wipe. I stayed docked, which was right for a different reason (corvettes shred scouts), but the word oversold it.
- "The way home" in fuel warnings is the charted route, not the hex distance. See bugs.
- Whether a lost raid destroys docked ships. The rules say you lose stockpile above the vault shield and half the structures. They do not say the ships live. I assumed they live and stacked corvettes. I never got a raid, so I still do not know from play.
- Raid "points" versus score points. `next_raid_estimate_power` tracks building and research score, not ship count. That is in the rules and it is easy to miss while reading a status line that only shows the resulting power.

The game is readable. The hour is not fun at this pace if you play one order at a time, because the interesting choices (where to jump, what to fight) are worth less than keeping three mine upgrades from stalling.

## 4. Bugs and oddities

Repeated destroy events for one pirate. The same fleet id, the same sector, and the same salvage to the unit, fired again and again while `mine` was false. I was not in that fight. Examples:

- tick 2504: `FleetDestroyed` fleet 4295426042, npc, sector -6,6, salvage metal 147.37497 / crystal 80.386345 / deuterium 40.193172, label "corvette patrol", mine false
- tick 2700: the same object
- tick 2839, 2988, 3046: the same object again

At tick 3349 that same id was still the live HOSTILE on -6,6: one Patrol corvette, power 16, listed as an attack target. The destroy events did not remove it, and the salvage figures never changed by a single digit. This looks like a replayed event, not a respawn. Salvage on the home sector did not obviously grow. `collect_salvage` was a one-shot and I did not get a clear cargo event back, so I cannot say the scoop failed.

Fuel warning used a long charted route for a short hop:

`fleet 230 jumping to [-4,4] with 41 fuel after the jump; the way home is 17 hops, 46 fuel: it cannot reach home without help` (tick 2480, Warning)

Home is -6,6. From -4,4 that is 2 hexes, not 17. The scout was on `prospect`, which will not path through hostiles, so the charted safe route really may have been 17 hops. The warning is then "correct" and still misleading: the ship can walk home in two jumps through unexplored hexes. It did get home (fleet 230 ended docked, fuel 130, policy still prospect). The message reads like a stranding.

No other server errors after the early `NoResources` on Deut Synthesizer level 2 (tick ~114, short 41 metal), which was just me ordering something I could not pay.

## 5. Balance

Mines are the game. At x600 a competent economy is a few big mines and whatever research you can afford with the leftover crystal. Sonnet-1 led at 375.0 with 0 combat and 0 explore. That is the optimal shape, and it means the map, pirates, derelicts and ship classes are optional content.

The 25% cap makes exploration a side quest. My scout's 1.9 explore was the highest on the board and it was 2.5% of my core, 0.5% of the leader's. Hunting is similar: nothing if you are 8x stronger, and the points still sit under the cap. I stopped sending ships out. That was correct and a bit dull.

Defences are the weak score. Half credit, and they share the shipyard queue with ships that score full credit and also defend while docked. Pulse turrets are only right if you are out of fleet slots or a raid is inside the warning window and the yard is busy. I never reached that.

Storage Vault does not matter in blitz. Cap multiplier is pace^0.75, about 121, so the base cap is hundreds of thousands. A vault level also does nothing for income. I built two by mistake and would not build one on purpose.

Ore is correctly a snack. A pristine tile is not a living, and harvesting it would have pulled a fleet off the defence for less than one mine tick. I ignored it.

Ship classes above corvette are gated behind shipyard levels that compete with mines. I would only walk Shipyard to 6 after the metal mine was high enough to feed a cruiser every 15 seconds (around metal mine 19). In a one-hour blitz that moment comes late or not at all. Corvette tech being a one-level unlock at shipyard 1 is the only ship tech that matters.

Fabricator and Modular Fabrication are strong and feel fair. The second and third build slots change the shape of the game. They should stay.

Raids scale off economy score, not off fleets, which is the right pressure. Mine never arrived. With defence at roughly double the estimate the whole time, I cannot say if the warning window is long enough.

## 6. Top 3 suggestions

1. Let the map move the score in a one-hour game. Either raise the combat-plus-explore cap, or make a delivered chart and a cleared pirate worth more relative to one mine level. Right now the leader can ignore the galaxy and win.
2. When a queued building is short on metal and a cheaper research or ship is about to start, say so on the status line (`Metal Mine L11 waiting on 800 metal; Shields L3 will spend 256 first`). The rule is written. The failure looks like a stuck queue with no error, and it cost me several minutes of the best mine.
3. Fuel warnings and the autopilot should use the real shortest path, and say when the long number is only the charted safe route (`charted 17 hops, direct 2, direct crosses unexplored`). Prospect is the right doctrine for a scout and the warning made it look lost.

## 7. The `play` tool

`./play do` waits about 2.5 seconds for events after every order. `./play status` and `./play costs` each take another round trip. A mine at this pace is 3 to 20 seconds. Playing by hand, most of the hour is the tool, not the game. I ended up sending orders through the same FIFO the tool uses, one per second, because that was the only way to keep three queues full.

What would have made it playable:

- `./play state` printing one JSON document (stock, levels, queues, fleets, catalog costs) and returning immediately.
- `./play do` accepting several orders in one call, or a flag to skip the 2.5 second wait.
- The status line already has the right numbers (slots, pending, `waiting_for`, defence versus raid). Keep that. Add the research levels that are easy to miss when you only print non-zero in a summary, and add fleet fuel versus both charted hops and direct hops.
- `./play costs` is the best command. Times and lock text were enough to plan without a spreadsheet.

I would not slow the tick. I would make the client able to keep up with it.
