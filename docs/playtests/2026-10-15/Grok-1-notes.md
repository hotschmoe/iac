# Grok-1 notes

Home: 4,-2 (ring 4). Fleet 78: 2x Light Fighter. Pace x1200. Caps ~1.86e6, storage not urgent.
Start tick ~15, 60 min. Rank tied 1/4 at 9.3 (fleet 9 from the two fighters).

## Plan
- Economy first: Solar ahead of mines (energy was 22/22). Research Lab -> Energy -> Combustion -> Robotics 2 -> Shipyard -> Light Fighters.
- Computer Technology 3 for a second build slot once deut income exists.
- Mines while payback is well under time left. Deuterium synthesizer before fuel and research eat the starting 800.
- Hunt ring 4-7 with fighters (SAFE/FAVOURABLE), scoop wreckage, bank charts. Do not pick DEADLY fights.
- Score is buildings + research + ships kept. Spend stock. Defences only once raids matter (shipyard 2 or any defence triggers raids).
- Delay shipyard 2 / defences until the economy can replace losses. Two starting fighters already repel a typical raid.

## Log
- t15: queued Research Lab, Solar 2, Metal 2, Crystal 2. Scanned.
- t80s: lab 1, solar 2, mines 2. Energy dipped to 48/50.
- t130: solar 3, deut 1, robotics 2. Shipyard unlocked. Neighbours: single passive LFs. Bots already in 5,-2 and 4,-3.
- t200: shipyard 1, solar 4, deut 2, metal 3. Energy 1 queued (deut-starved until synth caught up). Moved fleet to 3,-1.
- t250: killed the ring-3 convoy in 2 rounds, no ships lost, wreckage reported 847/462/231. By t277 the sector had no salvage; collect at t333 said "no salvage". Possible snatch or a drop that never stuck. Do not count on slow looting.
- t285: Combustion 1 done. Light fighters unlocked. Shipyard stays at 1 so raids do not start. Fleet 78 on patrol_home, range 4, engage 1.5, budget 40%, refine on.
- Score is cost-weighted, so a finished mine beats a ship of the same cost (same score plus income). Saturate build and research; fighters only from surplus. New fighters spawn a home fleet because 78 is under orders. Merge that stack and keep it as a garrison.
- Research order: Computer 3 (second slot), Energy 4, Combustion 3, Laser 5, Computer 6, then weapons/armour/impulse/ion toward cruisers.
- Driver: /tmp/grok1_loop.py reads state and batches orders.
- t780-1430: score 175 -> 670, but rank fell to 4/4. Grok-2 leads by ~400, mostly buildings and research. Energy fell to 62% (1253/2022) because mines outran the solar queue, and the queue clogged with solars that were a minute from being payable. Research lab stuck at 2, so research score lagged (212 vs Grok-2's 371).
- Fix: cancel mines and far solars while power is short, reserve the next solar, spend surplus crystal on solar satellites (0 metal, 30 energy), and push the lab to 8 before more mines. Shipyard stays at 1: a typical raid is already 13 fighters and the forecast is LOST, and shipyard 2 is what starts raids.
- Fleet 78 patrols range 4 and has not lost a ship. The 8 new fighters sit at home as fleet 1075. Wreckage from the first kill was gone before collect (t250 reported 847/462/231, t277 sector had none).

## Late session

- Patrol then lost 1 Light Fighter (fleet 78, InCombat at 2,-1, hull 91%, then home with 1 ship). Set 78 to manual and merged it into 1075. Final fleet: 1075, 9 Light Fighters, manual, idle at 4,-2. Left it there. Raid forecast stayed LOST (about 36, then 43 Light Fighters). Shipyard stayed at 1. No defences, no outpost.
- Energy brownout was the queue, not the plant cap. Cheap mines start immediately; a solar with reserve sits short of crystal and blocks the slot. Satellites (27, +30 energy, 0 metal) patched one brownout and then competed with research for crystal. Solar Plant hit MaxLevelReached after cancel-and-requeue counted queued levels toward 20 while the built level was still 16.
- Research stalled for several minutes on Computer 7-8 and Energy 7-8, all short of crystal, with nothing payable behind them. Cancelling the building lines that were holding crystal let Computer 7, Energy 7 and Computer 8 finish (research 454 -> 927). A later script summed the crystal shortfall of the whole research queue and froze buildings while metal sat over 500k.
- t3075, 9 min left: rank 4/4, score 2871 (buildings 1579, research 1144, fleet 148). Grok-2 at 4460. Build queue empty, 619k metal unspent. Research waiting on Energy 8 (102k crystal, 51k deut, 256 score, would finish) and Impulse 6 (would not finish).
- t3225: cancelled Energy 8 and Impulse 6. Impulse 5 had already finished. Queued Solar 19, Crystal Mine 16 (started), Metal Mine 20, Armour 7-9. Crystal 16 and Metal 20 finished before Solar 19 could be paid, so energy fell to 84% (2811/3336).
- t3291, tool said under 6 minutes: Metal Storage 1-4 (pure metal, about a second each), Armour 10 queued. Armour 10 starts in ~290s and takes 192s, so it will not start before the clock ends. Solar 19 and Armour 9 were still going to finish.
- t3319 snapshot, 4.7 min left, rank 4/4, score 3520.5 (buildings 1889.2, research 1482.8, fleet 148.5). Leader Grok-2 5096, gap 1575 (buildings +933, research +642). Stock 182k / 15k / 90k. Production 1140 / 416 / 211 per second. Levels: Metal 20, Crystal 16, Deut 15, Solar 18, Robotics 8, Shipyard 1, Lab 7, Metal Storage 4. Tech: Computer 8, Energy 7, Laser 8, Weapons 6, Armour 8 (9 running, ~73s), Shielding 4, Impulse 5, Ion 2, Combustion 5. 27 Solar Satellites. 9 Light Fighters.
