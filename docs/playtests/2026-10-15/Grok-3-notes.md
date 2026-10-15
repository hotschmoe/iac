# Grok-3 playtest notes

Pace x1200, home -3,0 (ring 3). Started tick ~16 with 60 min. Rank is buildings+research+fleet at cost. Defences score 0.

## Plan
- Keep energy fraction at 1: solar before mines. Solar L ~= mine L, deut synth costs as much energy as one mine.
- Deuterium first among mines once solar can carry it: research and robotics eat the starting 800 deut.
- Research Lab, then Energy 1, Combustion 1, Computer toward 3 (second build slot) and 6 (third).
- Robotics 2 -> Shipyard, then Light Fighters (hunt T1-T2 wreckage) and later the cruiser chain (Impulse 4, Ion 2, yard 5).
- Fleet 82 (2 LF, SAFE on ring 3-5): chart and kill band-1 hostiles, scoop wreckage, dock to bank charts. Do not take RISKY. Home dock mends free.
- Spend stock. Queue depth 4 until Computer 3. Builds are 1s until metal+crystal cost passes ~2500.
- Switch from mines to ships when a mine's payback exceeds time left.

## Log
- Opened solar-before-mines, lab, deut, robotics, yard 1, energy 1, combustion 1, computer 3 (second build slot). Two extra LF. Rank slipped to 4 while thinking; Grok-2 pulled a building lead.
- Fleet 82 patrol_home (range 8, engage 1.5, disengage 1.0). Fleet 858 prospect (range 12, engage 1.2). Shipyard stays at 1 so raids do not start.
- Economy loop in /tmp/grok3_eco.py: both slots, solar whenever energy headroom < 80, deut stays ~5 behind metal, research is computer then energy (no metal). Solar sats only when deut is spare.
- ~t1200: M13 C13 D10 S16 Rob5 Lab4, rank 3, still behind Grok-2 on buildings. Laser stopped at 5 on purpose.
- A reserved Solar 17 froze the build queue (~t1266-1456). Cancelled it. Do not set reserve.
- Fleet 82 (patrol) lost one Light Fighter and sat cut off: every exit previewed DEADLY against the ring spawn, including empty sectors. Walked it home by hand after scans showed no hostiles. Dock mended it. Shipyard stays at 1.
- Computer 6 opened the third build slot. Energy 6 done. One mine at a time, and only while energy headroom stays positive. Deuterium mines crash that headroom if queued beside a metal and a crystal mine.
- ~t2250: M18 C16 D13 S18, 40 solar sats, rank 2, score 1927, Grok-2 ahead by ~500. Stock had piled up while I was between checks.
- ~t2437: Metal 19, Crystal 17 and Solar 19 all paid at once. Headroom held.
- ~t2588: Robotics 8, Deuterium 14, Computer 7. Rank slipped to 3. The gap is research: Grok-2 leads by ~700 research points, buildings are close. Energy tech is the best points per deuterium (about 5 per thousand). Computer is worse. Lab 8 and Robotics 9 eat the deuterium Energy 8 needs, so they wait.
- ~t2718: Energy 7 and Metal 20 started. ~t2780: Crystal 18 started, Solar 20 queued (~79s short of metal). Merged fleet 82 into 858 (3 Light Fighters) and set patrol_home, range 4, engage 2.5x, so it only takes SAFE fights near home.
- Plan for the last quarter: keep mines and solar on the three slots, chain Energy 8 then 9 as deuterium allows, sats only from crystal and deuterium that the next energy level does not need. No shipyard 2, no defences, no reserve.
- ~t2907 the loop did queue Deuterium 15 before I pulled it. Headroom fell 479 to 289 and stayed positive. Deuterium is out of the loop after that.
- ~t2983 rank 2, score 3611. Buildings 2705, ahead of Grok-2 by ~480. Research 732, behind by ~1074. That research gap is the whole deficit. Fleet 858 (3 Light Fighters, patrol range 4, engage 2.5x) is fighting and hauling salvage. Fuel touched 93/600 and it kept moving.
- Energy 8 was waiting on crystal, and a short waiting order leaves the research slot idle until it is covered. Combustion 5 and Armour 6 ran behind it.
- Tick 3379, about 4 min left, tool said to write feedback. Rank 3, score 4320.9. Grok-2 at 5096, gap 775. Buildings 3102 (ahead of Grok-2 by 280), research 1045 (behind by 1080), fleet 173.5. M20 C19 D15 S20 Rob8 Lab7 Yard1. Energy 8, Computer 7, 40 satellites, 3 light fighters.
- Energy 9 queued, short about 48k crystal. Armour 7 running, Armour 8 and Combustion 6 behind it. Did not buy Metal 21 or Deuterium 16: headroom was 127 and either mine would have dropped fraction under 1, and both want crystal Energy 9 needs.
- recall of fleet 858 jumped it home (144 fuel, no damage) and the patrol preset sent it straight back out. It was at -5,-2 again a minute later.
