# Grok-1 feedback

Session stopped at tick 3387, when `./play` printed `WRAP UP: under 6 minutes left`. Homeworld -2,5 (empire id 9).

## Result

Bank at the stop: metal 340, crystal 302, deuterium 144. Production per tick: 2.00 / 1.20 / 0.60.

Buildings: MetalMine 3, CrystalMine 3, DeuteriumSynthesizer 3, Shipyard 3, ResearchLab 1. FuelDepot, SensorArray, and DefenseGrid stayed 0.

Research: ReinforcedHulls 1, CorvetteTech 1, HaulerTech 1. Everything else 0. Research was idle at the end. The build queue and the shipyard were idle.

Ships still alive:

- Fleet 217, docked at home, manual: 4 corvettes, 2 haulers, 1 scout. Hull 429/429, fuel 460/460, cargo 0/460.
- Fleet 251, one hauler, still harvesting at -1,4 when the wrap-up hit. Hull 88/88, fuel 47/100, cargo about 25 metal and 25 crystal of a 200 hold. That load is not in the bank. The sector had already fallen from Moderate to Sparse.

Earlier, fleet 10 (4 scouts and 2 corvettes) was destroyed at tick 2442–2444 around sector 3,7.

What I actually finished: a shipyard-3 economy, three haulers, a corvette stack that won every inner-ring scout fight it took, and two salvaged wrecks deposited (60/15/9 each). I did not reach lab 2, so HarvestingEfficiency and Navigation never started. I never saw a raid. I never fired on another player.

## Strategy

I opened on the homeworld. Metal 2, crystal 2, deuterium 3, then the shipyard, with crystal 3 slotted in before shipyard 2 so crystal income could carry haulers. Two extra scouts joined the starting pair. That shuttle on -1,5 (pristine metal, one hop east) paid for the early game: a small hold filled before the deposit died, they flew home, and the seam was pristine again next visit.

The same policy on a bigger fleet did the opposite. Four scouts plus corvettes filled nothing before the seam was gone, `mine_and_return` staked the next nearest ore from the claim rather than from home, and the fleet walked itself out to distance ~10. An outer-ring corvette patrol rapid-fired the scouts and then the corvettes. `FleetDestroyed` for fleet 10 reported salvage 0. Recall on the way out had already cut that fleet's hull from 120 to 99, and the damage never healed.

After the wipe I rebuilt four corvettes as one fleet (217), took shipyard 3, HaulerTech, and MetalMine 3, and queued haulers whenever crystal was at 200 and deuterium at 150. Deuterium was the gate: home makes 0.60 per tick and a hauler costs 150. I skipped lab 2 (200 metal, 400 crystal, 100 deuterium, 270 ticks) because that crystal was two haulers and the hour was already late.

`mine_and_return` with max_range 2, issued at home, held immediately: "no ore in range of home". -1,5, -2,6, -2,4, -3,5, 0,4, and 0,5 all read None. The only living rock left on the map I could still see was -1,4, Moderate metal and Moderate crystal, with one passive pirate scout. I killed that scout and two more (at -2,4 and -3,5) with the corvettes. Inner-ring pirates are one weak scout and they die in a single combat tick. I scooped two wrecks and missed the third. The surviving hauler was put on -1,4 at the end because it was the only seam still above None.

I kept DefenseGrid unbuilt and kept the fleet off home for most of the hour, specifically so a raid would not roll. A short dock to unload cargo was the only time the homeworld had guns.

## Fun and clarity

The minute-to-minute loop is good. `./play status` is a real captain's screen: bank, production, queues, hull, fuel, cargo, exits, hostiles. Winning a scout fight feels clean. The early pristine shuttle felt like I had understood the game.

The cliff after that did not. Nothing on the status screen says a sector is permanently used up, that this fleet is too hungry for the seam, or that the autopilot will walk away from home until it hits a corvette pack. I learned those by losing the fleet and by watching -1,5 stay None for the rest of the hour. Building costs, research costs, prereqs, and production are not in `./play rules` or status. I played the first half by guessing and the second half by watching what the events actually did.

Combat events are the other clarity failure. A `FleetDestroyed` line of `salvage: {metal: 200, crystal: 50, deuterium: 30}` is followed by `SalvageCollected` of 60 / 15 / 9. The event advertises the dead ship's cost. The wreck on the ground is a third of that. I also cannot tell, from the log alone, whether a fight is mine.

## Bugs and oddities

These are the ones I could see from play, with the lines that showed them.

- Explored space is forgotten. After the scan window (~2 minutes) `./play map` collapsed to whatever sector a fleet was standing in. At the end, "7 known sectors" was just the two fleets' current neighborhoods. Routes I had already flown came back as blank exits.
- Galaxy-wide combat noise. `CombatRound` and NPC `FleetDestroyed` arrive for other empires' fights. I issued `collect_salvage` on the strength of those lines and got `{"code":"NoResources","message":"no salvage in this sector"}` more than once. The same error came back at tick 3351 for wreckage that was mine: the -3,5 scout died at tick 3286, and the wreck was gone before the hauler landed.
- `mine_and_return` issued at home, tick ~2853: `{"action":"hold","reason":"no ore in range of home"}`. The fleet sat on an empty homeworld with full tanks until I cleared the policy and moved it by hand. max_range of 2 was not enough to see a living seam, and the order never said which neighbors were dead.
- Dry claims migrate outward. The work sector becomes the next ore within max_range of the dead claim. There is no leash back to home. That is the path fleet 10 took to 3,7. `stop` while that policy is set does not stick; the next policy tick orders harvest again. Clearing the policy to `manual` does stick. I saw that at tick 3119: "standing orders cleared".
- A full hold never scoops, and the mining policy never scoops. Salvage at -1,7 despawned while the fleet flew home to unload and came back.
- Docking merges every fleet already at home, and a ship that finishes while a fleet is docked joins that fleet. I never had a split or a transfer. The replacement corvettes, both docked haulers, and the scout all ended inside fleet 217. The one time a hauler spawned as its own fleet (251) was because 217 was away. Bringing 217 home later would have eaten 251.
- Deposit has no event. Fleet 217 jumped from -3,5 to -2,5 at tick 3323 with 120/30/18 in the hold. Status afterwards showed cargo 0 and the bank higher by that amount. Easy to miss if you are not subtracting.
- Sector view and the combat log disagree about ids. At -1,4 status said `HOSTILE fleet id 0`. I attacked 0. The event was `CombatStarted` against `enemy_fleet_id` 248, and that attack did kill the scout. It worked, and it was confusing.
- Some hostiles fire as you arrive and some do not. The -1,4 scout was labeled Passive and sat there until I sent `attack`. Entering -2,4 (tick 3256) and -3,5 (tick 3286) produced `SectorEntered` and `CombatStarted` on the same tick. I could not tell from the scan signal (`HostileMass`) which kind I was jumping onto.
- The opening fuel readout of 50000/50000 on the starter scouts was not the tank. The first docking set them to 120, which is two scout tanks. Jump cost tracks hull, so ReinforcedHulls (corvettes at 55 hull, hauler at 88) made every jump more expensive without giving more fuel.

The depletion rule underneath a lot of this: a tick that crosses a tier still grants the full harvest, zeroes the counter, and drops the sector one tier. If you leave on that tick, the sector climbs back. If a fleet is large enough to ride the sector all the way to None, it stays None. -1,5 is the example. I am confident in the observation. I am less sure I could have saved that sector once the big fleet had sat on it.

## Balance

The early scout shuttle is strong and readable. The hauler, which the whole build path points at, is how you delete the map. Harvest power 5 and a 200 cargo hold means one hauler exhausts a pristine sector down to None before `cargo_return_pct` can fire, because the harvest does not stop at a tier boundary and the autopilot only rethinks when the fleet is idle. Adding corvettes to the same fleet made that worse. The "correct" economic ship and the "correct" autopilot fight each other.

Outer ring versus inner ring is a wall. Distance 8 and in, one weak scout, a fair fight for two scouts. Distance 9 and out, several corvettes, and rapid-fire deletes scouts before the corvettes even matter. The map does not show that line. My fleet found it by dying.

Deuterium is the quiet limiter. Three synthesizer levels make 0.60 a tick. Haulers cost 150, corvettes 100, and the lab wants deuterium too. Metal was never the thing I was short of after MetalMine 2, except right after paying for a hauler. Crystal sat in the middle. A one-hour game with one building queue does not fit lab 2 (270 ticks, 400 crystal) and a hauler program. Frigate and cruiser, which need shipyard 4 plus a research chain, were never real options. Sensor array and fuel depot were the same. I did not feel those were choices I declined. They were out of reach.

Raids were optional. Staying undocked and never building a defense grid meant the homeworld was not worth visiting, and no `RaidIncoming` ever arrived. Combat paid (60/15/9 per scout, plus the sector if it still had ore) but only inside the inner ring, and only if I got there before the 60-tick despawn.

ReinforcedHulls 1 was a mixed buy. New corvettes came out at 55 hull instead of 50, which is real, and every jump got more expensive because fuel is a fraction of hull.

## Top 3 suggestions

1. Make deposits survive a big fleet. Cap a harvest tick at the ore left in the current tier, or leave the leftover in the next tier's counter, and let None regenerate back toward the template once the fleet leaves. Put "Metal: Rich, 12 until it drops" on the sector line. Right now a careful two-scout shuttle is an infinite faucet and a hauler is a permanent hole, and the player only learns which one they built after the sector reads None.

2. Give the autopilot a home leash, and give the player their map and their wrecks. `mine_and_return` should pick the next seam within range of home, and it should scoop salvage in the sector it is already standing in. Remember the last ore, terrain, and hostile result for visited sectors after the scan timer ends. Raise the salvage lifetime, or freeze it while one of my fleets is in the sector. Print the ground amount (the 60/15/9 I can actually pick up) in `FleetDestroyed`, and tag events that are not mine.

3. Put the numbers in the client, and let me split a fleet. A `costs` or an expanded `rules` with build cost, time, prereq, and production per level would have saved a long blind stretch. A ship-transfer or split order would have let the corvettes hunt while a two-scout fleet kept the faucet. Dock-merge as the only fleet composition tool means every successful dock collapses the empire back into one stack.

## The `play` tool

`status` is the right shape. Tick, minutes left, production, queues, and per-fleet exits are exactly what I needed, and the header on every call made the hour easy to pace. Error JSON is clear. `wait` is the right tool between shipyard timers.

What hurt:

- Every `do` sleeps about 2.5 seconds. Salvage lives 60 ticks and a harvest tier dies in two or three. I could not arrive, scoop, and leave on a schedule, and I could not stop a harvest on the tick a tier broke. A batch form (`do` several JSON commands, one sleep at the end) and a `--no-sleep` for a single urgent order would match the one-second tick.
- `events` is an unfiltered galaxy feed. A `--mine` switch, or a fleet id on `CombatRound`, would have stopped the false salvage runs.
- The map forgets. `map 4` near the end listed six or seven sectors, all of them under a fleet's feet. A persistent "last seen" map, even stale, would have been the difference between exploring and re-exploring.
- Unloading cargo, staking a claim, and "this sector is None forever" produce no dedicated message. The policy `reason` strings are good when they fire. More of those, for deposit, merge, despawn, and tier drop, would make the log an audit trail instead of a combat ticker.
- `wait` and `do` share one cursor. I learned not to send a command during a wait. The tool could say that the moment a second call arrives, instead of racing.

I would rather have one slightly chattier status and a memory of the map than any new command besides fleet split and a cost list.
