# Grok-1 notes

Home: 4,-2 (ring 4). Pace x600 blitz. Started tick ~9, ~60 min.
Fleet 78: 2 Light Fighters, power 182.

## Strategy
Score = core + min(combat+explore, 25% core). Core is buildings, research, living ships, half of defences, at cost weight. Unspent stock scores 0. Defences are half-value, so build them only to hold raids.

Priority:
1. Solar ahead of mines so energy fraction stays 1.0. Opening: Solar2, Metal2, Crystal2, Solar3, then Deut1.
2. Deut synth early (lab and robotics need 200 deut; starting stock is 100).
3. Research Lab -> Energy 1 -> Combustion 1 -> Robotics 2 -> Shipyard 1 -> Light Fighters (full score and home defence).
4. Computer Tech 3 for a second build slot once build times matter.
5. Crystal is the usual bottleneck (research + ships). Keep crystal mine close to metal.
6. Fleet: prospect nearby (max_range 4, engage 1.5) for chart points. Two LFs are safe through ring 7, shaky at ring 8, dead at ring 9. Do not send them past ring 8.
7. Raids can start ~12 min after shipyard 2 or any defence (first day / P^0.75). Keep fighters or rocket launchers home once that matters.
8. Do not sit on stock. Queue depth 3 waiting. Ship batches must be paid in full up front.

## Energy cheat (made / needed)
Solar L: 22, 48, 79, 117, 161, 212, 272, 343, 424, 518
Mine energy L: 11, 25, 40, 59, 81, 107, 137, 172, 213, 260 (deut is 2x)
M2 C2 D1 = 72 needs solar 3 (79)
M3 C3 D2 = 130 needs solar 5 (161); solar 4 (117) is short
M4 C4 D3 = 198 needs solar 6 (212)
M5 C5 D4 = 279 needs solar 8 (343); solar 7 is 272 (97%)

## Log
- t0: queued Solar 2, Metal 2, Crystal 2, Solar 3. Scan showed T1 hostiles next door, all SAFE for 2 LFs.
- Idle gap while the driver was written. At ~38 min left, still M2 C2 D0 S3, stock ~19k/12k/100, rank 4/4 score 10. Unspent stock scores nothing; that gap is the hole to climb out of.
- Driver /tmp/grok1_player.py: energy-safe mines while payback < ~42% of time left, deut floor, lab, robotics 2, shipyard 1, then light fighters. Shipyard stays at 1 until a home guard can hold raids (yard 2 starts them). No defences (half score, and 20 rocket launchers draw cruisers). One patrol fleet, second fleet prospects. Research: energy, combustion, computer 3 (second build slot), then armour/weapons/laser.
- t1304 38.3m rank 4/4 score 10.0 core 10.0 M2 C2 D0 S3 lab0 robo0 yard0 stock 19043/12059/100 | DeuteriumSynthesizer->1 payback 77s +2.20/s | SolarPlant->4 power DeuteriumSynthesizer L2 | MetalMine->3 payback 24s +7.83/s | CrystalMine->3 payback 28s +5.17/s | patrol
- t1356 37.4m rank 4/4 score 10.0 core 10.0 M2 C2 D0 S3 lab0 robo0 yard0 stock 19840/12562/100 | DeuteriumSynthesizer->1 payback 77s +2.20/s | SolarPlant->4 power DeuteriumSynthesizer L2 | MetalMine->3 payback 24s +7.83/s | CrystalMine->3 payback 28s +5.17/s | patrol
- t1547 34.2m rank 4/4 score 96.5 core 77.2 M10 C7 D6 S12 lab2 robo2 yard1 stock 149/4525/615 | fleets ok
- t1733 31.1m rank 4/5 score 166.2 core 133.0 M11 C7 D7 S12 lab3 robo4 yard1 stock 3587/3150/1714 | SolarPlant->13 RESERVE power MetalMine L12 in 38s | ComputerTechnology->3 RESERVE wait 11s prio 1 | fleets ok
- t1920 28.0m rank 4/5 score 233.7 core 187.0 M12 C9 D7 S13 lab3 robo4 yard1 stock 17534/3416/1321 | fleets ok
- t2101 25.0m rank 4/5 score 294.2 core 235.4 M12 C9 D7 S13 lab3 robo4 yard3 stock 33968/4094/2796 | ships x1 | fleets ok
- Around t2034 the planner raised the shipyard to 4 (raids on). Home guard grew to 19 light fighters; forecast raid is 5 fighters, odds repelled. Mines froze at M12 C9 because any upgrade that left energy between 96% and 100% was discarded, so solar and the yard spent the queue. Research reserve on Computer 4 also held the lab. Restarted the driver at 16 min left: mines with a real weighted gain go first, no research reserve, shipyard frozen at 4, one-fighter patrols recalled. Explore and combat are past the 25% cap.
- t2768 ~14m rank 3/5 score 620 core 496. Haiku-2 leads on core (~788). M13 C13 D9 S15 lab4, energy short at 1253/1323. Next solar costs more crystal than the stockpile, so the build queue went idle while metal piled up. Storage now takes that metal (full score, no crystal) until the solar is payable.
- t2945 11m rank 2/5 score 781 core 625. Haiku-2 936 core 920. Grok-2 771. Storage 2 done. Solar 16 still needs ~33k metal and ~13k crystal, so it waits while storage converts metal. Home guard holds the raid (repelled). Bonus already capped.
- t3099 8.4m rank 2/5 score 861 core 689. Lab 5, storage 3 done, storage 4 reserved. Stopped buying fighters so they no longer take the metal. Haiku-2 still leads on raw core. Energy still 1253/1323 until solar 16.
- t2674 15.4m rank 4/5 score 385.8 core 308.6 M12 C9 D7 S13 lab3 robo5 yard4 stock 119630/35320/10588 | CrystalMine->10 payback 319s +14.41/s | CrystalMine->11 payback 2057s +12.05/s | SolarPlant->14 power DeuteriumSynthesizer L8 | DeuteriumSynthesizer->8 payback 412s +7.00/s | Cryst
- t2861 12.3m rank 3/5 score 713.1 core 570.5 M13 C13 D9 S15 lab4 robo5 yard4 stock 14521/3484/3526 | WeaponsTechnology->5 prio 4 | recall thin
- t2919 11.4m rank 2/5 score 768.2 core 614.6 M13 C13 D9 S15 lab4 robo5 yard4 stock 6869/4046/569 | MetalStorage->1 metal into score | MetalStorage->2 metal into score | recall thin
- t3085 8.6m rank 2/5 score 860.8 core 688.7 M13 C13 D9 S15 lab5 robo5 yard4 stock 8533/9541/3085 | recall thin
- t3221 6.3m rank 2/5 score 926.8 core 741.5 M13 C13 D9 S15 lab5 robo5 yard4 stock 15387/13845/6303 | recall thin
- STOP at 6.1 min, tick 3235, score {'score': 926.8, 'rank': 2, 'of': 5, 'core': 741.5, 'combat': 68.4, 'explore': 289.0, 'age_s': 2, 'bonus_capped': True}
