# Grok-1 playtest feedback

Wrapped up on the tool's WRAP UP line, tick 3462, about 2 minutes left. Homeworld -3,5. Pace x600 blitz. An earlier draft of this file was written at tick 3372; numbers below are the later snapshot. Core did not move between the two.

## 1. Result

5th of 6. Score 133.4 (core 129.0, combat 4.1, explore 0.3). Sonnet-1 led at 375.0 with combat 0 and explore 0.01 (the board rounds that to 0.0). Sonnet-2 368.6, Haiku-1 357.3, Haiku-2 315.5. Grok-2 was 6th at 78.9.

Final stock 6119 metal / 3747 crystal / 3731 deuterium, caps 606155 / 424308 / 303077. Production 35.37 / 20.01 / 11.37 per tick. All three queues idle. From tick 3372 to 3462 the stockpile grew by roughly a minute and a half of mine output and the score gained 0.4, all of it combat. Unspent resources scored nothing, and nothing new finished.

Buildings: Metal Mine 9, Crystal Mine 8, Deuterium Synthesizer 7, Shipyard 5, Research Lab 4, Fabricator 4. Fuel Depot, Sensor Array, Defense Grid and Storage Vault stayed 0. Modular Fabrication 2, so three build slots.

Research: Fuel Efficiency 5, Reinforced Hulls 3, Advanced Shields 1, Weapons Research 3, Navigation 3, Harvesting Efficiency 4, Corvette Tech, Frigate Tech, Emergency Jump 3, Modular Fabrication 2. No Cruiser Tech, Hauler Tech, or Extended Tanks.

Ships: 52 corvettes and 2 frigates docked in fleet 10 (power 1430, full fuel 2760/2760, empty cargo), plus fleet 205, a 2-corvette patrol (power 44, fuel 70/100, cargo 0/20) still flying near home. Defence power 1430 against an expected raid of about 315. No structures. No RaidIncoming all session. 47 known sectors (2 live).

I resumed at tick ~1292 (38 min left) already in 5th with score 4.4 and every queue idle. The leaders were at 60-70 then and pulled away to ~370. The idle gap at the start of my session was most of the loss. From the resume to the end I gained about 125 core; the leaders gained about 300. Between the two wrap snapshots Sonnet-2 alone went from 352.4 to 368.6 while my core stayed 129.0.

## 2. Strategy

Score is the weighted cost of finished buildings, research and ships. Stockpiles score nothing, defences score half, and combat plus explore cannot add more than 25% of core. So the game is "turn production into completed items before the hour ends."

What I tried:

- Lab to 4, then Modular Fabrication twice, for three build slots. This was the right first move.
- Fabricator to 4, because build times were about to matter.
- Mines while payback was inside the session. Ended at 9/8/7. Crystal was usually the resource a cruiser recipe would run out of; metal became the limiter after the corvette spam.
- Shipyard to 5 and Frigate Tech. I wanted cruisers (best score per shipyard-second when the yard is the bottleneck) but never reached Shipyard 6 or Cruiser Tech. At wrap-up a cruiser was still locked on Cruiser Tech (cost line: 3000/1500/800 in 15s, have 0).
- A 2-corvette patrol near home, sized to stay under the 8x overwhelm rule. It killed the same corvette patrol over and over, including at ticks 3399 and 3438 in -2,6, with zero hull damage taken. That is why my combat (4.1) is the highest on the board and still irrelevant to the ranking. The bonus cap room was about 32 points and I used about 4 of them.
- Two scouts on prospect. They added a little chart (explore 0.3, 47 known sectors) and then were gone.

What worked: three build slots, fabricator, and staying far ahead of the raid estimate with docked ships. What did not: spending crystal and metal on filler research (Fuel Efficiency 5, Emergency Jump 3, Harvesting) and on a drip of single corvettes. Both kept the next mine or the frigate unlock unaffordable, so build slots and the shipyard sat idle while income trickled in. Frigates only just beat corvettes on score per second at my metal/crystal ratio, and I only got two of them out. A pure core player who never undocked (Sonnet-1, 375 core, ~0 combat, ~0 explore) beat every mixed strategy on the board. At the end I still had enough stock for several frigates or a mine level (6119/3747/3731; a frigate is 1000/400/200) and nothing queued, because the wrap-up arrived while the pilot was between orders and I stopped issuing builds once the tool said to write this file.

## 3. Fun and clarity

The rules are enough to play, and they matched the server once I trusted `costs` over pace-1 intuition. `status` is readable: stock, production, queues, raid estimate, fleet power, live vs stale.

What was confusing:

- Unspent resources are worth zero, and the 25% cap makes the map optional. That is written down, but `status` does not say it, so it is easy to play the space game and lose to someone who never leaves dock.
- `costs` shows only the next level. You cannot see the crystal and the minutes between Shipyard 4 and Cruiser Tech without walking the chain.
- Queue depth is 3 including whatever is already running. Modular Fabrication adds build slots, not queue slots. With 3 slots the queue can never hold a waiting order. I did not understand that until slots were idle with nothing legal to park.
- The rules say an order you cannot pay "waits in the queue (nothing is charged until it starts)." With a free slot the server instead rejects it. See bugs.
- Threat odds on the map use the strongest fleet. A scout's real odds are worse than the line `T2 2.0x FAVOURABLE`.
- Prospect avoids sectors that currently have hostiles, then aggressive fleets still walk onto the scout. Mine disappeared without a useful explore score.
- Salvage lasts 60 ticks and a full hold leaves a remainder. I lost the deuterium tail of a home kill twice (`SalvageDespawned` with deuterium still on the tile). The patrol that made the late kills has cargo 0/20, so the 2000+ metal piles at -2,6 expired unread. The tick 3438 pile (metal 2830, crystal 1849, deuterium 924, despawn 3498) was still on the tile when I stopped, and the patrol was already flying away from it.

## 4. Bugs and oddities

- Ordering a building while a build slot is free and the stockpile is short returns NoResources instead of a waiting order. Quoted from the log: `NoResources not enough resources for Shipyard Lv.3: costs 648 metal, 324 crystal, 162 deuterium; short 246 crystal`. The same happened for Fabricator Lv.2 (`short 79 crystal`, then 55, 30, 5 as crystal ticked in) and Fabricator Lv.3 (`short 220 crystal`). When a slot was already busy, Shipyard L4 did wait: `waiting [0]: Shipyard -> L4 (11s) (needs 510 crystal)`. The two behaviors should be one behavior.
- The same NPC fleet id was destroyed over and over. `FleetDestroyed` / `CombatEnded` for fleet 4295426046, label "corvette patrol", sector -2,6, at ticks 2556, 2589, 2631, 2754, 2793, 2835, 2877, 2916, 3000, 3039, 3126, 3165, 3204, 3320, 3359, 3399 and 3438. Early salvage was about 147/80/40. Later piles, each a two-tick fight with ~36 hull dealt and 0 taken: tick 3320 `{"metal":2387.5,"crystal":1607.7,"deuterium":803.9}` despawn 3381; tick 3359 `{"metal":2534.9,"crystal":1688.1,"deuterium":844.1}` despawn 3420; tick 3399 `{"metal":2682.2,"crystal":1768.5,"deuterium":884.2}` despawn 3459; tick 3438 `{"metal":2829.6,"crystal":1848.9,"deuterium":924.4}` despawn 3498. Same id, same label, loot growing about 5-6% per kill, now ~15x the early pile. Either the respawn reuses the id, the lair is not clearing, or salvage is stacking onto a corpse that combat still treats as a fresh group. Combat score still ticked up (3.3 to 3.8 to 4.1), so the repeat kills counted.
- Near the end of the earlier stretch, status showed `Shipyard queue: idle` and on the next line `waiting [0]: 3x Corvette (2s each) (needs 672 metal, 94 crystal)`. Idle plus a waiting batch is hard to read.
- No RaidIncoming all session, while `next_raid_estimate_power` climbed from about 32 to 315 and the shipyard had been past level 2 for most of an hour. That may be the 30% roll and the 18-hour cap. Flagging it in case the scheduler did not arm.
- Fleet 205's patrol doctrine flipped between "intercepting contact" and "returning: a jump would leave 25 fuel against 30 needed to return home" every few hops. At tick 3428 it turned for home (`returning: a jump to [-2,3] would leave 25 fuel against 30 needed to return home`), at 3431 it was still returning, and at 3434 it turned around again (`intercepting contact`) and re-fought the same patrol at 3437. It never deposited salvage. Cargo stayed 0/20 through both of the last kills.

## 5. Balance

- Core is the whole game at this pace. The winner has zero combat and zero explore. The 25% cap means a successful hunter cannot catch a player who spent that attention on mines and ships. I like that economy matters. I do not like that the hex map, derelicts and pirates are skippable for the ranking the tool tells you to win.
- Docked ships both score full value and defend. The raid scales with buildings and research only, so a fleet that exists to score also makes the raid harmless (1430 vs 315, no structures). Defense Grid, turrets and the vault never came up. Vault 0 caps are ~600k and `full_in` stayed in hours (metal 4h, crystal 5h, deuterium 7h at the end). In a blitz those buildings are traps unless something forces a real raid.
- Ore on the home tile (rich metal, pristine crystal, ~50 units) is one tick of late-mine output. The rules are right that ore is not a living. Derelict payouts are multiplied by sqrt(pace), so the number is large, but cargo and the 60-tick salvage timer throw most of it away unless you brought a hauler fleet. I never had a reason to unlock haulers. The late corvette-patrol piles (~5600 resources and still growing) would have been a real income spike and my patrol could not lift them.
- Cruisers are the best score per yard-second, but only when the yard is the bottleneck. My yard was resource-bound the entire game (frigate gather time ~30s, build time 5s). Walking Shipyard 4 to 6 and then a two-minute Cruiser Tech is a long detour for a small gain over frigates, which are themselves only a small gain over corvettes when metal is the short resource. The ship ladder does not change the score much if you cannot feed it.
- Early mine levels finish in 2-5 seconds. An agent that spends ten seconds choosing leaves the queue empty. That, more than any cost curve, is why a slow start cannot be repaired. The leaders' ~240 point lead was mostly minutes of queues I was not there to fill. The last 90 seconds showed the same thing in miniature: leaders still completing items, my idle queues converting 35 metal per tick into a stockpile.

## 6. Top 3 suggestions

1. Let an order wait when a slot is free and the player is short, as the rules already say, and make queue depth grow with build slots (or at least to 3 waiting plus the running ones). Idle slots while saving were the main stall. Status should show the short resource and the seconds until the next order can start.
2. Put the scoring rule on the status line: unspent stock is worth 0, and combat plus explore are capped at 25% of core. Optionally show core and the unused bonus room. Players who enjoy the map currently cannot see that the leaderboard will ignore them.
3. At blitz speed, `costs` should show the next three levels, not one, and the shipyard line should say which ship is unlocked at this yard level. The cruiser gate (yard 6, then Frigate Tech, then a two-minute Cruiser Tech) is invisible until you are standing on it. Also stop a killed NPC group from respawning as the same fleet id with a larger salvage pile every ~40 ticks; farming one sector should not be an infinite score tap, even a small one.

## 7. The `play` tool

`status`, `costs`, `score`, `map` and `events` are the right set, and printing minutes left on every call is what makes a timed session playable. The command JSON matches the rules. The WRAP UP line when under 6 minutes is unambiguous.

`./play do` waits about 2.5 seconds before it returns. Builds at this pace finish in 2-5 seconds, and three queues need a new order that often. A client that blocks for 2.5s cannot keep the empire busy. I had to send orders on the same FIFO about once a second, which is inside the "don't spam" rule, but it is not what the CLI invites. A `do` that returns when the server ack arrives, plus `status` JSON (the full_state is already on the wire), would be the whole fix.

`wait 60` is right for a long research and wrong for a 4-second mine. A wait that returns early when a queue hits empty would match how the game actually moves.

Smaller wants: map odds computed for a chosen fleet, not always the strongest; the waiting-order line kept visually inside the queue it belongs to; the NUDGE when no command has been sent is good and I would have wanted it during the idle stretch at the start.
