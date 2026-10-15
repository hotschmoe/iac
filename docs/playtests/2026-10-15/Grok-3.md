# Grok-3 feedback

Snapshot at tick 3379, about 4 minutes left, when `./play status` said to write this. Queued work may still finish after that: Armour 7 (4 seconds), Armour 8, Energy 9 (short 48084 crystal, about 62 seconds, then 77 seconds to research), Combustion 6.

## 1. Result

Rank 3 of 4. Score 4320.9. Grok-2 led at 5096.0, 775 ahead. The parts were buildings 3102.1 (280 ahead of Grok-2), research 1045.3 (1080 behind), fleet 173.5 (24 ahead). Grok-4 was just above me on the previous look and the rank had been flipping between 2 and 3 for the last ten minutes.

Homeworld -3,0, pace x1200.

Buildings: Metal Mine 20, Crystal Mine 19, Deuterium Synthesizer 15, Solar Plant 20, Robotics Factory 8, Research Lab 7, Shipyard 1. No fusion reactor, nanite factory, or storage.

Research finished: Energy 8, Computer 7, Laser 7, Armour 6, Weapons 5, Ion 5, Combustion 5, Impulse 3. Nothing in shielding, espionage, hyperspace, plasma, or graviton.

Ships: 40 solar satellites and 3 light fighters in fleet 858, on patrol. No defences and no outpost. Fleet score is almost all satellites (40 times 4.0, plus 13.5 for the fighters).

Stock at the snapshot: metal 323780, crystal 156716, deuterium 127956. Income per second 1352 / 778 / 250. Energy 3890 made against 3763 needed, fraction 1, headroom 127. The build queue and the shipyard were idle. The lab was not.

## 2. Strategy

Score is what you have spent and still hold. Stock on the pile scores nothing, defences score nothing, and a destroyed ship leaves the score. I treated every idle pile as a mistake.

Shipyard stayed at 1 for the whole hour. The raid forecast was LOST from the first time I looked, raiders keeping about 99 to 100 percent, and it grew from 27 light fighters to 50 while I never built a defence. Building the yard to 2, or building any defence, is what starts that raid. I did not.

Energy before mines. One mine at a time once headroom got tight, solar whenever it was actually payable. A deuterium synthesizer costs about as much energy as a metal mine and becomes payable while the next metal mine is still short of metal, so a loop that queues "whatever is ok" will buy the deuterium mine and delay the better one. That happened once: Deuterium 15 started while headroom was 479 and cut it to 289. Fraction stayed 1. I took deuterium out of the loop after that.

Computer Technology went to 6 for the third build slot, then stopped. The next computer level is worse score per deuterium than Energy Technology (about 3 per thousand deuterium against about 5). Energy also matches how the mines pay, roughly two crystal per deuterium. That chain is Energy 6, 7, 8, and 9 queued. I found it late. The whole deficit to Grok-2 is research.

Solar satellites are the fleet. They cost crystal and deuterium, they score 4 each, they add 30 energy, and they do not start a raid. I built them when those two resources were not spoken for by the next energy level. Forty of them are why the fleet part is ahead.

Lab 8 and Robotics 9 were payable near the end and I did not build them. Both spend the crystal and deuterium Energy 9 was waiting on, and the build slots were idle because the next mine cost more metal than I had, not because the buildings were slow.

What did not work:

- One Solar Plant order was sent with reserve set while it was short about 33500 metal. Reserve holds the free slot. The queue sat from roughly tick 1266 to 1456. I cancelled it. I did not use reserve again.
- Weapons, laser, ion, impulse, and armour bought early spent metal the mines needed. Laser stopped being useful for the fleet long before level 7. Those points were real and still the wrong pile.
- The third build slot did not close a metal gap. Past Metal 18 the next level costs more metal than a minute of income, so two of the three slots sat empty while crystal and deuterium had to be spent on research and satellites instead.
- Patrol on a single damaged light fighter. Covered under bugs. Three fighters on a 2.5x engage bar did fight and did bring salvage home.

## 3. Fun and clarity

The rules and the tool output are enough to play. Every status prints the score formula. `next` says ok, the shortfall, and how many seconds of income cover it. The queue note that a short waiting order leaves the slot idle is the sentence that saved the last few minutes. I did not have to recompute a fight, a flight, or a raid. The server's words were the rules.

What confused me:

- An empty sector and a sector with a fleet in it use the same odds words. DEADLY on an unvisited hex is the ring's spawn, not a fleet I have seen. I played it as a fleet, and so did the patrol.
- The rank and the gap move between two calls a few seconds apart when the board is a second or two old. The race looked jumpier than the empires were.
- `homeworld.satellites` in the compact state was easy to miss (a query for it came back empty) while energy was climbing by 30 a satellite. The readable status does say "40 Solar Satellites at 30 each."

The game is legible. The hour was a production puzzle with a fleet on the side, and the production puzzle is the one the score rewards.

## 4. Bugs and oddities

- A short order with reserve holds the slot. Solar Plant 17, order 2072, reserve true, short about 33500 metal. Builds behind it did not start until I cancelled it.
- The same trap without reserve, stated by the tool: "every waiting order is short, so the free slot stays idle; the first to be covered is EnergyTechnology->L9 #4003 (93996 crystal) in ~2m. Order something payable to use the slot now." Energy 9 sat there while the lab did nothing. Armour 7, ordered afterwards, started immediately and paid 64000 metal. The short order does not start itself, and it does not tell you it can be jumped until you read that note.
- `./play wait --until idle` returns as soon as any queue is idle. An empty shipyard counts. A loop written the way the session brief suggests hot-spins and over-orders. `./play wait 6` was the fix.
- Patrol refuses a hex whose odds are below `engage_ratio_x10` whether or not anyone is there. With one damaged fighter, every exit toward home previewed DEADLY against the ring estimate. Scans of those hexes showed 0 hostiles. The fleet sat. I switched it to manual and walked it home one hop at a time. The dock then mended it for nothing (1 ship, 156 hull).
- `recall` on a fleet that still has a preset is cancelled by the preset on the next tick. The answer was an emergency jump home from -3,2 for 144 fuel, no ship damaged, abandoning a jump that had already burned 36 fuel. The same tick the doctrine logged "intercepting contact." A minute later fleet 858 was idle at -5,-2 on patrol again.
- Fair refusals, clear wording: move -5,0 to -5,1 returned NoConnection (the exit is -4,0). Attack on a hostile that was not in the sector returned "InvalidTarget hostile fleet is not in this sector."

## 5. Balance

At x1200 the leaderboard is metal income plus the metal-free research, in that order. Combat is optional and, if you open it the way the yard offers it, punished.

The raid forecast never looked like a fight I could take. It said I would lose about 100 percent of whatever stood at the dock, and the raiders would keep about all of theirs, while the only ships I could build without raising the yard were light fighters and satellites. Raising the yard or building a defence is what makes that forecast real. Defences score 0, so the anti-raid answer does not even pay on the board. I skipped combat as a scoring plan and I think that was correct. A player who spent the hour on cruisers would have started a raid and lost the dock.

Mine energy is the constraint the numbers make obvious, and they punish the obvious mistake: a mine that finishes before the solar plant that was supposed to feed it. That part feels right.

Energy Technology dominating Computer Technology is something a player has to notice. Grok-2's board is that notice taken earlier: fewer buildings than me for a while, far more research, and a lead of several hundred that my mines could not eat.

Charts and one light-fighter wreck are rounding error next to 1352 metal a second. Fleet 858 did fight on the 2.5x bar and did unload salvage, and the salvage did not move the rank. Exploration on this pace is something the fleet does while the lab works.

Three build slots do not help once each level costs a minute of metal. The extra slots are idle. More robotics does not shorten a resource wait.

## 6. Top 3 suggestions

1. Split the ring's spawn estimate from odds against a fleet you have actually seen. Use a different word. A patrol, and a human, should be willing to path home through an empty hex. The bar that "applies whether or not hostiles sit there" is what stranded the damaged fighter.
2. A short queued order should not leave the only slot idle, and a payable order sent later should not have to be discovered as the workaround. Either let the short order wait without taking the slot, or start it the moment the stock covers it and say so in the order's answer. The note is honest. The default still wastes the lab.
3. Do not make the shipyard the button that starts an unwinnable raid, and do not score the defences that would answer it at zero. At this pace the rational game is mines, energy tech, and satellites. The fight, the pursuit, and the raid are the parts the client draws, and the score tells you not to show up for them.

## 7. The `play` tool

`state` as one JSON document, `status --brief`, `next.ok`, `covered_in_s`, and the score broken into buildings, research, and fleet are what made an hour of this playable. I could decide from one call.

`wait --until idle` should ignore a queue I have never used, or it should mean every queue is idle. As it is, the session's own advice to pass time that way spins. A fixed `wait N` (at most 60) is fine and is what I used.

`odds` and `preview` with no sector reprint the entire help page. A one-line "need Q R" would have been enough.

Stacking the research queue worked: Energy 9 waiting on crystal, Armour 8 waiting on the level ahead of it, Combustion 6 waiting on the slot. The "starts in" estimates and the shortfall on the waiting row are the right shape. Keep them.

`merge` and `policy_update` did what the rules said on the first try. `recall` should clear the preset, or the answer should say the doctrine will send the fleet straight back out. I spent the fuel and the abandoned jump for a trip home that lasted one tick.

The number I most wanted on the brief, and did not have, was headroom after the mines already in the queue. Made minus needed is there. The mines that have been paid for and have not finished yet are not.
