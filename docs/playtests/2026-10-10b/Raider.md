# Feedback - Raider (raid strategy)

## 1. Result
Final score about 415-430, rank 1-2 of 6 (within a few points of Explorer). Core about 370, combat 15, explore 27.
Buildings: Metal Mine 13, Crystal Mine 11-12, Deut 9, Shipyard 6, Lab 5, Sensor 2-4, Fuel Depot 2, Fabricator 1, Vault 1.
Research: nearly the whole tree maxed (hulls, shields, weapons, nav, fuel, hauler, cruiser, frigate, EmergencyJump, Modular Fab 2).
Fleet: roughly 100+ corvettes, 5 frigates, 2 cruisers, 2 scouts. Almost all the fleet sat at home as defence/score.

## 2. Strategy
Economy-first for ~5 min, then Corvette Tech, then corvette swarms hunting the T1-T3 pirates around home.
Raiding yields very little: a T2/T3 group is one corvette; a fresh kill pays 250-500 resources, repeat kills in a sector fall to 40-120 (heat).
My income was 75 metal/s, so a whole fleet raiding gave roughly 1-3% of income. Combat score hit the 25%-of-core cap early anyway.
What worked: patrol_home autopilot early (kills + auto-salvage), then manual sector chains for fresh sectors further west (~280/kill).
Late game score came entirely from converting income into ships/buildings/research; corvettes (metal-heavy) were a good instant sink.
Verdict: raiding cannot compete with economy at this pace; it is a ~5% side income. Higher-tier groups (T5+) are too far for fuel (a 20-ship fleet burns ~180 fuel per jump).

## 3. Fun and clarity
Rules and tools were clear. The wait/idle loop is good. Confusing: patrol_home often sits idle (no visible contacts), fleet cap 3 deployed counts forgotten scouts, FIFO queues block cheap items behind expensive ones.

## 4. Bugs and oddities
- `recall` returned ok for scouts but they did not return; policy_update manual then errored InsufficientFuel.
- Many commands print stray "rejected: ..." lines from background events of other commands (confusing).
- `play wait` crashed with FileNotFoundError on the view cache when two play processes ran concurrently (os.replace of Raider.view.tmp).
- build_ship once failed "OnCooldown fleet 470 is in transit" because the new ship tried to join a moving fleet.
- state income/s looked stale for many minutes (74.8 constant) while stock moved.
- Building costs in status 'needs X' shortfalls jumped around (CrystalMine L12 cost 8k metal / 4k crystal) vs costs table.

## 5. Balance
- Raid loot is tiny relative to blitz economy; pirates near home are single weak ships. Kill score also capped by 25%-of-core.
- Several techs max out at low levels (Weapons 5, Shields 5, Hulls 5, Nav 5, Fuel 5) so research becomes a dead sink mid-game.
- Late mine levels cost 8-10k metal for +12/s with 20 min left: payback far beyond session; ships are the better late sink.
- Big fleets are fuel-limited (jump cost scales with fleet mass), discouraging the "push outward with a doom stack" plan.

## 6. Top 3 suggestions
1. Make pirate groups in rings 3-8 larger / richer so raiding scales with fleet size, and let loot scale with kills of bigger fleets rather than being heat-throttled so hard.
2. Make patrol_home actually scan/seek hostile sectors in range (and report why idle), and let docked fleets not count against the 3-deployed cap.
3. Let queues skip an unaffordable head item (or add an explicit reorder/priority command).

## 7. The play tool
Great overall. Wanted: `play queue-skip`/reorder, a compact `fleets` command, hostile list with ids across map, and no cache-race crash when run in parallel (I ran helper shell scripts alongside).
