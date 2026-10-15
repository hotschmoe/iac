# Grok-4 playtest notes

## Setup
- Home -1,-4, ring 4. Pace x1200 (faster than blitz). ~60 min session.
- Start: MM1 CM1 SP1, 2 Light Fighters (fleet 84), stock ~818/492/800, energy 22/22.
- Score starts 9.3, tied. Bots already ahead on buildings.
- Passive convoy (1 weakened LF, id 8589737983) sits on the home sector. T2, odds 2.4x FAVOURABLE.

## Plan
1. Solar just ahead of mines. One build slot until Computer 3.
2. Lab early, then Energy 1 -> Combustion 1 (LF) and Computer (slot at 3).
3. Robotics 2 -> Shipyard -> Light Fighters, then cargo for charts/ore.
4. Deuterium synth once solar covers it (SP3 + MM2/CM2 leaves room for DS1).
5. Spend stock. Defences score 0; build them only when raids start (SY2 or any defence triggers raids).
6. Fight only FAVOURABLE+ until the fleet is bigger. Scoop wreckage. Chart with prospect once a cargo exists.
7. Outpost is a late trip (rings 9-20, founding cargo, and a Cruiser-class raid). Skip until the economy is fat.

## Raid
- t986 a 7 LF raid wiped fleet 84 (2 LF) and took 3902/631/357. Shipyard 2 makes raids eligible; the quiet after a loss is about 14 min at this pace.
- 12 then 16 Rocket Launchers. Stay under 20 or the raid switches two fifths of its power into Cruisers, which shred launchers. Forecast moved from lost to repelled to held as the economy grew. Recheck it every few minutes.
- Convoys respawn on the home tile. Another player's fight there drafted the docked fleet once; the home dock mended it. Salvage on the home tile is scooped by a docked fleet.

## Economy rules that paid
- One build slot until Computer 3. Deuterium starved that research; build the synthesizer before spending deut on labs and the yard.
- Solar just ahead of the mines. Letting mines finish first dropped energy to 0.73 and the income penalty showed up in the score rate.
- Crystal is the limiter. Metal banks: spend it on armour, weapons, light fighters and metal storage, not on delaying a solar that the mines are waiting for.
- Mine paybacks at L13-L17 were still 4 to 7 minutes with half the hour left, so mines beat a ship dump. Ships are the sink once a solar is the thing the queue is waiting on, and for fleet score (a lost fleet is 0).

## Log
- t19: opening queue SP2, MM2, CM2, Lab. Research Energy, Combustion, Computer behind the lab. Scan then attack the home convoy.
- t2152: raid of 20 LF repelled in 2 rounds, nothing lost, wreckage 18000/6000 scooped at the dock. Fleet 2214 stays home (8 LF + 3 Cruiser). Stay under 20 Rocket Launchers.
- t2290: rank 2, score ~2065, Grok-2 ahead by ~526 (buildings ~450, research ~139, fleet we lead ~62). SP19 CM15 Lab 6 Computer 6 Armour 8 Combustion 6 done. MM19 running. SP20 queued (~2 min, metal-bound). Energy holds at 1.0 with about 15 spare after MM19; no CM16/DS11 until SP20. Third build slot is open. Crystal slack can be spent on shielding and energy tech; metal stays for SP20.
- t2452: Solar Plant 20 finished and is the maximum (`MaxLevelReached` on the next order). Energy made 2690. MM20 and CM16 fit. CM17 (t2860) pushed needed to 2725, fraction 0.987, and there is no further solar. That penalty stays.
- Raids after the cruisers all repelled: 20 LF (nothing lost, 18000/6000 scooped), 40 LF (lost 1 LF and 1 RL, RL rebuilt, 36000/12000), 42 LF (lost 1 RL, rebuilt, 37800/12600). Stayed at 16 Rocket Launchers. Grok-3 twice fought the home-tile convoy and the docked fleet was pulled in for one round; hull only.
- t3297, 5.1 min left, stopped for feedback. Rank 3, score 4296 (buildings 2511, research 1598, fleet 188). Grok-2 4874, Grok-3 4321, Grok-1 3338. Fleet 11 LF + 4 Cruisers. Levels: MM20 CM17 DS10 SP20 RF8 SY6 Lab7 Metal Storage 7; Weapons 9, Shielding 7, Impulse 5, Computer 6, Armour 8, Energy 6. Armour 9 and Energy 7 still queued and should finish. Robotics 9 is deut-blocked for ~4 min and likely will not. Never left home: at x1200 a jump is worth less than a second of crystal.
- Convoy cleared (template now). No salvage left on the tile. Fleet 84 still 2 LF, hull full, manual, staying home. Ore and wreckage pay less per minute than the mines at x1200.
- t250 Combustion 1. t357 Computer 1. t483 Computer 2. Computer 3 is reserved and is the second build slot; do not spend deuterium on labs, robotics or the yard until it has started.
- Energy rule: solar only when the next mine would drop fraction under 1. Deut synth tracks the mines so research can be paid. Build queue must stay full; think-time is the score leak.
- t490: MM7 CM6 DS3 SP8, SP9/MM8/DS4/SP10 in the queue. Rank 4, Grok-2 leading by a lot on buildings. Catch-up is slot 2 plus never idling the build queue.
- Next after that queue: MM9 CM7 MM10 SP11, then DS5 CM8 SP12 (reserve) MM11. Research behind the Computer 3 reserve: Energy 2, Combustion 2, Energy 3.
