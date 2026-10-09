# Grok-2 playtest feedback

## Result

4th of 6 at the wrap-up, tick 3446, about 3 minutes left. Score 402.5 (core 327.0, combat 5.1, explore 70.4). Combat + explore is 75.5 against a 25% cap of 81.8, so the bonus has a few points of room. For most of the hour it was capped and rank was pure core. The last cruisers are what moved the score from 388.8 at tick 3298 to 402.5.

Leaderboard at tick 3446: Explorer 473.4 (core 425.4, combat 12.8, explore 35.2), Raider 442.3 (core 399.9, combat 15.1, explore 27.3), Mixed 404.9 (core 383.9, combat 13.6, explore 7.3), Grok-2 402.5, Grok-1 322.6 (core 262.3, combat 6.9, explore 53.4), Builder 263.0 (core 263.0, combat 0, explore 0). The gap that matters is still core. Mixed has 7 explore and finishes 2 points ahead of me. I have the largest explore pile on the board (70.4) and it is worth about 6 points over a pure-core score, which is the only reason the displayed gap to Mixed is small. Explorer added about 28 core in the last few minutes and pulled further away.

Economy, homeworld 3,-3: income 52.31 metal / 30.26 crystal / 17.68 deuterium per tick. Stock on hand 135 / 7118 / 874, which scores nothing. Crystal is piled up because every remaining purchase is metal-gated. Buildings: Metal Mine 11, Crystal Mine 10, Deuterium Synthesizer 9, Shipyard 6, Research Lab 6, Fuel Depot 6, Sensor Array 6, Defense Grid 4, Storage Vault 6, Fabricator 6. Every research track is maxed, including Cruiser Tech, Emergency Jump 3, and Modular Fabrication 2 (three build slots). Build and research queues are idle because the next levels cost more metal than is on hand (Metal Mine 12 is 5190/1297). Docked ships: 2 scouts, 18 corvettes, 11 frigates, 2 haulers, 5 cruisers. The fourth finished around tick 3390 and the fifth started at tick 3435 (paid 3000/1500/800) and completed at tick 3446. Defence 2137 against a raid estimate of 703, with 6 pulse turrets. Every home raid was defended and no stock was lost. Fleets 6, 185, and 714 are all home and idle. Known sectors: 188 (122 live, 66 stale).

## Strategy

Mines and lab first, then a hauler, then whatever the derelicts paid for. That part worked. The tier-1 site at 9,-6 paid roughly 5000 metal on the first boarding and two follow-up scoops of crystal and deuterium, and that haul funded Modular Fabrication, Frigate Tech, and the high mines. Corvettes and then frigates held the raid window (first raid around tick 1106, defence 409 vs raid 155, resources lost 0). Later raids at ticks 2001 and 2896 were also clean wins (defence 880 vs 347, then 1137 vs 671).

What did not work was treating explore and combat as score. Once the bonus capped, kills of respawning scout patrols (about 60 metal and a fraction of a point) and chart deliveries did not move rank. Mixed proved the point: a small explore number and a bigger core finishes ahead. I kept the two scouts on prospect anyway, and they spent the whole late game idle at home because the doctrine would not enter a 1.36x sector.

The catch-up plan was Shipyard 6, Cruiser Tech (8000/5000/2500, started at tick 2890), then cruisers at 6.85 core each. It was the right purchase and it was late. Cruiser Tech sat behind metal for about two minutes while build slots were idle. Five cruisers did finish by tick 3446 (3000/1500/800 each; the last one started at tick 3435). That added about 14 core in the final minutes and did not close the gap. Explorer is about 98 core ahead, Mixed about 57. Nothing left on the tree changes rank in the time that remains, and the metal stock is 135.

A self-inflicted stall: fleet 272 (one hauler, two corvettes) jumped toward a derelict after a preview said the hop would leave 17 fuel against 45 needed to get home. It arrived at 2,1 with 1 fuel. Emergency reserve at 50% still wanted about 150 ticks for a single jump. A second fleet walked out and `merge` pooled the fuel, which is a good rescue rule, but the trip back and the later empty run to 1,5 and 2,5 burned the clock. Those two sites were still marked "derelict t1 quiet" on the map and both returned `InvalidTarget no derelict to board`.

## Fun and clarity

The rules and `./play state` are clear enough to play without a UI. Score formula, queue shortfalls ("needs N metal, ~Ns"), raid estimate versus defence, and preview labels (SAFE / EVEN / CANNOT RETURN) all answered the questions I actually had.

What stayed confusing:

- Sensor signals name a coordinate that is often the scanner's own sector. `explore_site` at -2,-2 returned `InvalidTarget no derelict to board`. I could not tell a contact from the origin.
- The map keeps "derelict t1 quiet" and "salvage" on STALE sectors after the site is gone and after `despawn time passed`. I flew to 1,5 and 2,5 on that memory and found nothing.
- Hex distance is not connectivity. 2,4 and 2,5 are one hex apart and `preview_move` returned `NoConnection no lane from [2,4] to [2,5]`. The real door was 2,4 → 1,5 → 2,5.
- A ship batch of count 2 waits until both ships can be paid. That is easy to miss and it idles the yard while 1000 metal sits there.
- `range_hops` reads as "hops you can still make outward and get home," which is right, but a manual move that fails that test still happens and only then raises an Alert.

## Bugs and oddities

- Early build ack disagreed with the queue. The ack said a Deuterium Synthesizer had started while a Crystal Mine was queued ahead of it. The later Queue event had the real order.
- Fleet power flickered 18 → 17 → 18 after a fight that dealt 0 hull damage to us. No ship was lost.
- `policy_update` during combat: `ERR OnCooldown fleet 6 is in combat`. A later line in the same batch still said standing orders were cleared.
- Cancels shift under you. In one batch, `cancel_queued` Ship 0 removed a hauler I did not mean, and a building cancel removed Crystal Mine 11. The ack text also mentioned a frigate. Cancelling from the back is mandatory, and a queue that starts mid-batch returns `QueueFull` / `InvalidTarget nothing is waiting at that position` for the index you just aimed at.
- Research refuses to wait on a missing prerequisite (`PrerequisitesNotMet` for Weapons before Lab 3, `MaxLevelReached` if you re-queue a tech that just started). Buildings will sit in queue ahead of a prerequisite that is itself only queued. The two queues do not use the same rule.
- All build slots freeze when the front order cannot pay. After Modular Fabrication 2 I had three slots and still watched `open: 0` for a minute because Shipyard 6 or a metal mine was short a few hundred metal. Nothing behind it starts. The extra slots do not skip a blocked head.
- Doctrine safety ignores the engage setting. Prospect hold, repeated from about tick 1704 through tick 3424: `no safe way to the nearest uncharted sector [7,-3] within 8 hops of home: [7,-3] is unscouted or unsafe (T2 power 13 against your 18: 1.36x EVEN, the doctrine needs 1.5x)`. With hostiles present the floor is max(engage, 2.5x) even if engage was set lower. The two scouts scanned home a couple more times (32 sectors, worst T2, ticks 3249–3374) and otherwise never left. The same hold was still the latest fleet event at tick 3424.
- Stranding is allowed. Preview said a jump would leave 17 fuel versus 45 to get home. The move was accepted. Critical alert at 2,1: fuel 1, jump cost 15, four hops and 60 fuel home. Emergency reserve ticks up in quarters and needs on the order of five minutes for one jump.
- Home salvage from the tick 2896 raid, `salvage_dropped` 3720/930/558, despawned at tick 3076 while a hauler was docked in that sector and simply had not been told to scoop. An earlier 1920 metal pile at 3,-3 despawned the same way. 180 ticks is short when you are mid-route, and nothing in `status --brief` shouts that a pile is sitting on your own homeworld.
- `wait --until idle` returns immediately, over and over, on the prospect-hold line and on "ship queue idle and now payable" for a corvette I am deliberately not building. In the last 15 minutes a 20 second wait often returned in 1–3 seconds, so the loop polls.

## Balance

Core-per-metal decides the game, and the bonus cap makes explore a resource faucet rather than a path to first place. That is a fair design if it is what you want, but it makes the most interesting systems (charts, derelict risk, patrol fights) optional for anyone who can read the formula. Builder, who never fought or explored, still beat a sloppy explorer on core discipline. Mixed did the same to me.

Derelicts are swingy in a good way. One quiet T1 site funded several levels of tech. Missing the despawn, or trusting a stale map pin, gives you nothing. I would keep that swing and make the timer visible.

Mines stay correct for a long time (Metal 10 paid back in about 6 minutes, Metal 11 in about 8). By 15 minutes left, Metal 12's payback was longer than the remaining game, while a cruiser was 6.85 core for 3000 metal and a Lab level was 17 core. The switch from "always mine" to "spend on core density" is sharp and easy to miss.

Raids felt fair. The estimate tracked my defence, docked ships mattered, and I never lost stock. Pulse turrets are efficient defence and weak score (half cost), so I stopped building them once the margin was comfortable. That seems right.

Cruisers arrive late. Shipyard 6 plus an 8000/5000/2500 tech plus 104 seconds of research meant the first cruiser landed with under 7 minutes left. Frigates at 2.0 core carried the midgame. If the cruiser is supposed to be the late-game prize, the gate is doing that job. It also means the player who hits Shipyard 6 ten minutes earlier probably wins, which matches the table (the top three cores are within 8 points of each other and 70 ahead of me).

Deuterium becomes the quiet constraint on Cruiser Tech (2500) and then on every cruiser (800). I spent deuterium on Fuel Depot and Fabricator and pushed the tech start back. Income of ~14–18/s only just covers one cruiser per metal cycle (3000 metal is ~58s, which produces ~900 deuterium). That is a nice tension. It is not explained anywhere in the tool output except by doing the arithmetic yourself.

## Top 3 suggestions

1. Let a free build slot start the next order that can pay, instead of freezing every slot behind one short order. Modular Fabrication's third slot was often decorative. A `promote` or an automatic skip, with the short order staying queued, would match how players think about three yards.
2. Make prospect path around a sector instead of sitting at home for half the game. Honor `engage_ratio` (the 2.5x floor overrode it), and print the blocking sector once, not on every tick. The same goes for `salvage_and_sites`.
3. Put despawn countdowns and "site still here / already boarded" on `map` and `sector`, and make sensor contacts distinct from the sector being scanned. I lost two raids' worth of salvage and a round trip to empty derelict pins because the stale label looked like a live one.

## The play tool

`state` in one JSON line is the right interface. Shortfalls, `ok` flags, `home_fuel`, defence versus raid estimate, and `minutes_left` are what I used every turn. `preview` saved the second fleet. `score` on one shared table is how you notice you are losing on core and not on kills.

What I wanted and did not have:

- A fuel-safe `move_toward` that refuses a hop unless `fuel_after` covers `fuel_to_return`, and that routes around missing lanes. I wrote that walker badly once and stranded a fleet. The preview already computes every number. The dangerous part is that the raw `move` ignores them.
- Queue reorder. Cancel-from-the-back is fragile when a tick starts an item mid-batch and the indices move. `promote index` or `replace queue` would remove a whole class of mis-cancels.
- `wait --until idle` should not wake on a repeating prospect-hold or on "payable: Corvette" when the ship queue is idle on purpose. A filter, or waking only when a shortfall hits zero or a running item finishes, would keep the 20 second wait honest.
- One line in `status --brief` when salvage is on top of a fleet, with the despawn tick. The event exists (`salvage_despawn_tick` on CombatEnded, `SalvageDespawned` after the fact). The brief status I checked between orders did not show the 3720 metal sitting at home.

I never needed to fight the tool's batching. Spacing orders and returning one line each is the right politeness for a shared server.
