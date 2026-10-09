# Feedback - Sonnet-1

## 1. Result
Rank 1 at ~6 min left (score ~350 vs next ~325). Buildings: Metal 13, Crystal 12, Deut 10, Shipyard 6, Lab 6, FuelDepot 6, Sensor 5, DefenseGrid 4-5, Vault 5, Fabricator 5. Research: most techs maxed except CruiserTech (never afforded), ModularFab, Corvette/Frigate/Hauler 1. ~40 Frigates, 2 Scouts, 21 Pulse Turrets, 23 Lancers. No raid seen. Combat and explore score ~0.

## 2. Strategy
Rush mines early, Shipyard/Lab, then research for defences, then pour all resources into Frigates and Lancer Batteries (score is cost-based, so any spend counts). Crystal was the early bottleneck, later metal. Mines got too expensive (x1.9/level) by minute ~25; switching to pure score-spending was right.

## 3. Fun and clarity
Rules were clear. Confusing: "waiting" queue items - a build that cannot be paid is refused with NoResources when no slot is free to start it, yet appears to queue sometimes (it queues only when a slot is running). The "needs X crystal" numbers on waiting items looked inflated/stacked. Tick vs wall-clock: ticks run faster than 1/sec at times.

## 4. Bugs and oddities
- Queued items shown as "needs 3958 crystal" while stockpile larger; starts oddly late.
- Prospect doctrine holds "nothing uncharted and safe within N hops" even at max_range 8 with only 17 sectors known.
- patrol_home frigates never attacked adjacent T1 pirates (probably the 8x rule, but no message said so).
- `./play wait 90` sometimes killed the shell tool (timeouts).

## 5. Balance
Combat/explore bonus is negligible (derelict ~0.04 pts) versus core, so hunting/exploring is pointless for score; killing weak pirates gives nothing at 8x ratio. Early game is extremely metal/crystal tight, late game mines are too costly to pay back. Storage caps are irrelevant at blitz. Pulse Turrets are very cheap per power.

## 6. Top 3 suggestions
1. Make exploration/combat worthwhile or remove the pretense; give a reason to leave home.
2. Let unaffordable items queue and auto-start reliably, and show the true remaining shortfall.
3. Provide a "spend advisor" or auto-build doctrine for buildings, like fleets have.

## 7. The play tool
Needs a batch command (several actions per call), a status --brief, and a persistent auto-queue. Wait >60s should be allowed without killing the tool.
