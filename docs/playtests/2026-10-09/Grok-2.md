# Grok-2 feedback

## 1. Result

Final snapshot at tick 3659, about 1 minute left, fleet docked at home 5,-4. The first wrap-up line was at tick 3388. This last stretch left the fleet idle at home and waited out the research that was already queued.

- Resources in the bank: metal 670, crystal 768, deuterium 530. Production 2.00 / 0.73 / 0.36. From tick 3526 (405/671/482) to here the bank grew only from those mines; no fleet income in the last two minutes.
- Buildings: Metal Mine 3 (completed tick 3506; production went from 1.21 to 2.00 metal per tick), Crystal Mine 2, Deuterium Synthesizer 2, Shipyard 3, Research Lab 2. Build queue idle. No fuel depot, sensor, or defense grid.
- Research: Hauler Tech 1, Harvesting Efficiency 2 (completed tick 3602). Nothing else researched. All three queues idle.
- Ships: fleet 117, docked and idle, policy manual. 4 Scouts + 2 Haulers. Hull 280/280, fuel 440/440, cargo empty.
- Lost fleet 22 (5 Scouts) at tick 1453. It was wiped in one tick at 9,-8 by a corvette pack.

## 2. Strategy

Early game was mines plus scout shuttles. Four scouts on a pristine tile pull 16 per tick until the depletion threshold, and docking deposits the cargo. That paid for Shipyard 2 and 3 and then Hauler Tech (600/300/200).

What worked:

- Manual shuttles on a short loop. Harvest, hop home, leave the tile.
- Killing lone 0.6x scout NPCs. Four scouts end it in two rounds with shields intact, and a hauler can win the same fight. Collecting salvage in that sector, before leaving, is worth 60 metal.
- One hauler, then a second. Harvest power 5 and cargo 200 beat more scouts. The merged fleet (4 scouts + 2 haulers, power 14, cargo 480) stripped a pristine deuterium tile at 3,-6 and a pristine crystal tile at 0,-5 in a handful of ticks. Harvesting Efficiency made the home deuterium strip 67 on the first tick.
- Scout respawns. The same inner tiles (2,-2, 3,-2, 4,-4, 4,-5) grew a new scout after a few hundred ticks. A sweep of those tiles was the reliable metal income once the ore was gone.

What did not:

- `mine_and_return` with max_range above 1. The claim walks outward with no cap at home. That is how fleet 22 reached 9,-8 and died. Recall after the fleet was already dead returned `FleetNotFound`.
- Sitting on a moderate tile with four scouts. It falls to Sparse and then None in a few ticks. None never comes back.
- Building the second hauler into the same fleet as the scouts. Docking merges every friendly fleet at home, and there is no split. Jump fuel is 0.1 per hull, so the merged fleet costs 28 fuel per hop on a 440 tank. A round trip is about 7 hops. Pristine metal at 0,-7 and rich metal at -2,-5 were visible on the scan and still unreachable without running out of fuel on the way home.
- Research Lab 1. It unlocks no research. The useful eco tech (Harvesting Efficiency, Navigation) needs Lab 2, which is 200/400/100 and 270 ticks.

## 3. Fun and clarity

The loop of scan, hop, fight, harvest, dock is genuinely fun, and a won scout fight feels good because the salvage is real money if you grab it immediately. The rules file explains doctrines, raids, and boarding well. The live tool does not explain costs, prerequisites, density, or fuel.

Things that stayed confusing for a long time:

- Status never shows what a building or a tech costs, what it requires, or how long it takes until it is already queued. Lab 1 looking like a dead end was an expensive surprise.
- The salvage event says `salvage: 200/50/30`. The pile on the tile is 60/15/9. I spent a long stretch convinced collection was broken.
- `HOSTILE fleet id 0` sits on the home sector for the whole game and is not a fleet you can fight. Real scouts use normal ids and start combat on entry.
- Known sectors expire after about two minutes. `sector` then says the tile is unknown and the map goes blank, including places we had just mined. Exits on the fleet line still work, so you fly from memory.
- Other players' combats are printed in our event log with no player name. A corvette-versus-scout fight at tick 2004 looked like an ambush on our fleet. Our hull was still 120/120.

## 4. Bugs and oddities

- Salvage. `FleetDestroyed` reports the full ship cost (`salvage: {"metal":200,"crystal":50,"deuterium":30}`). `collect_salvage` in the same sector returns 30 percent (`SalvageCollected` metal 60, crystal 15, deuterium 9), and only what fits in cargo. The rest sits on the tile (`salvage: 12/15/9`) and despawns. Collecting a bit later, or from the next sector, returns `NoResources: no salvage in this sector`. The event text and the pile do not match, and the window is easy to miss.
- Standing-order parse error. A `policy_update` whose `params` object omits one of the four fields comes back as `unparseable input: missing field action`. The missing field is a param, not `action`.
- `mine_and_return` relocates the claim to the nearest ore within `max_range` of the old claim, not of home. max_range 3 walked a fleet from 5,-4 out through 7,-6 to 10,-8.
- Sector regen does not matter in a one-hour game. The rate is 0.0001 times the current threshold, and only while the density is still above None and no player fleet is present. A stripped tile stays None forever. Leaving a Sparse tile does not restore it on any timescale we could see.
- Chained moves during combat. `move` while fighting returns `OnCooldown: fleet is in combat`. The next move in the same script then fires at the sector we had not reached yet and returns `NoConnection: no lane from [3,-2] to [1,-3]`. The script keeps going, so one combat desynchronizes the rest of the route.
- Opening fuel read 50000/50000. After the first dock it became the real tank (60 per scout).
- Docking merges every friendly fleet on the tile into the arriving fleet. A hauler built while the miners were away spawned as fleet 212, then disappeared into fleet 117 on the next deposit. There is no split.
- `fleet id 0` is listed as a passive scout on home and on several empty tiles. `attack` is NPC-only, and this id is not a real target.
- Build and research queues go idle the tick a job finishes. Status shows the queue as `idle` with no hint of what just completed unless you catch the event.

## 5. Balance

- Corvettes are a wall, not a fight. Inside distance 8 the NPC is one 0.6x scout and our scouts win cleanly. At distance 9 the roll is 3–8 corvettes at 0.8x with rapid-fire around 12–15 against a scout hull of 30. One round deletes the fleet. The step from "safe" to "deleted" is a single hop, and patrols can wander onto the edge.
- Ore near the hub is gone by midgame and does not come back. After that, metal comes from scout salvage (60 a kill) and mine production (about 1.2 per tick). A second hauler costs 600 metal, so the ship that should be mining has nothing left to mine.
- Harvest power scales much faster than the deposits. The merged fleet emptied a pristine deuterium field in six ticks (67, 34, 17, 17, 8, 8). The interesting decision was finding the field, not working it.
- Raids turn on at Shipyard 2. The defense grid needs Shipyard 3. There is a long window where home is raid-eligible and the only defense is whatever happens to be docked. We never saw a raid, which may be luck.
- Lab 1 and Shipyard 1 are long purchases that unlock almost nothing by themselves. Shipyard 2 is the first real gate (corvettes, hulls). Lab 2 is the first research that changes mining.
- Fuel on a mixed fleet punishes the right economic choice. Two haulers plus the scouts that built them cannot make the round trip to the pristine metal the scanner just revealed.

## 6. Top 3 suggestions

1. Make the numbers on the screen match the world. Show building and research cost, prerequisites, and time before you queue them. Report salvage as the pile you can actually pick up (60/15/9, not 200/50/30), and show hop fuel and a "fuel to get home" on the fleet line.
2. Give the player a split, and cap doctrines at a distance from home. Docking should not silently weld every ship into one fleet. `mine_and_return` should search within range of home, not walk the claim into corvette space. A one-line reason when a doctrine refuses an order would have saved fleet 22.
3. Keep each player's event log to their own fights, and mark fake contacts. `fleet id 0` should not render as a hostile scout. Other empires' combat rounds should not appear as if they happened to us. Known-sector expiry can stay, but the map should keep tiles we have visited, and a missing lane should be visible before the move fails.

## 7. The `play` tool

`./play status`, `do`, `map`, and `events` are enough to play, and the minutes-left header is clear. A few changes would have removed most of the stalled turns:

- Status should include queue ETAs (it does, once something is queued), the cost of the next level, and why a building or tech is locked.
- `map` should not forget visited sectors after two minutes. Printing exits with terrain only while the intel is fresh meant a lot of blind hops.
- One `do` that is rejected should not be followed, in a shell `&&` chain, by moves aimed at a sector the fleet never reached. A small route command, or a move that waits out combat, would match how the server actually ticks.
- `events` dumps every combat in the galaxy and truncates with "earlier events omitted" just when our own harvest lines matter. A filter for our fleet ids would help.
- Policy updates should name the missing param. `missing field action` sent me looking at the wrong key.
- The wrap-up line is easy to spot. Thank you for printing it on the command that crosses the threshold.
