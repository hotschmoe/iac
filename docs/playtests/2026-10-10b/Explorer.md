# Feedback - Explorer

## 1. Result
At ~8 min left: rank 1 of 6, score ~415 (core ~366, combat ~11, explore ~35). Mines M13/C12/D9, Shipyard 6, Lab 6, Fabricator ~7, Defense Grid 4, Fuel Depot 7, ModularFab 2 (3 build slots). Roughly 40 corvettes/frigates, 2 scouts, ~30+ pulse turrets. Researched most techs to max.

## 2. Strategy
Opened with mines/lab/shipyard, then set the 2 starting scouts on `prospect` autopilot and later `salvage_and_sites`. Charts + 1-2 derelicts gave ~17 explore points early, which hit the 25%-of-core cap almost immediately (explore 15 vs core 4 at t=400). After that, exploring added little: the cap means explorer cannot compete on its own; core economy decides the rank. Corvette fleet on patrol_home gave small combat points. Late game I spent on ships/defences/cheap buildings since mine payback exceeded the remaining time.

## 3. Fun and clarity
Rules are clear. The state/status/wait loop works well. Confusing: strict FIFO queues where one unaffordable item (e.g. Lab 6 needing lots of crystal) blocks cheaper items behind it while slots sit idle; I had to cancel_queued and reorder repeatedly.

## 4. Bugs and oddities
- `research X` at max level returns ok "accepted (already finished or not visible yet)" in some cases vs MaxLevelReached in others.
- "IDLE QUEUES: ship" shown right after queueing ships that had already started/finished; minor.
- `wait 60` often returns after a few seconds because of unrelated events, so loops burn many calls.
- Shell tool 120s timeout interacts badly with long loops.

## 5. Balance
- Explore/combat bonus cap (25% of core) makes pure exploring weak; the first ~20 charted sectors + 2 derelicts max it out within 7 minutes. Explorer strategy is only worth a scout or two.
- Mine upgrade costs explode past L10-13 (metal L14 = 11.7k for +13.8/s), payback 19-31 min at blitz; ships/defences become the sink. Pulse turrets (no deuterium) are a handy sink for surplus metal.
- Deuterium became the limiting resource late (Fuel Depot/Fabricator/Lab costs heavy in deut), while metal/crystal piled up.
- Haulers/ore harvesting are irrelevant: income is 75 metal/s from mines, a hauler hold is nothing.

## 6. Top 3 suggestions
1. Let queues skip unaffordable head items (or add a priority / reorder command).
2. Make exploration scale (higher cap or per-ring bonus) so scouting out to ring 8+ is a viable plan.
3. Make fleet harvest/salvage meaningfully competitive with mines, or flag it clearly as flavour.

## 7. The play tool
Very good. Wanted: `play queue-fill` presets, a "what should I buy" hint listing the best score-per-resource option, and a wait that sleeps the full time unless my own queues idle.
