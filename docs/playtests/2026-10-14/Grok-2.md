# Grok-2 feedback

Board at tick 3483, about 2 minutes left (the tool printed the wrap-up on every call):

| rank | name | score | core | combat | explore |
|---|---|---:|---:|---:|---:|
| 1 | Haiku-2 | 1241.4 | 1224.6 | 0.0 | 16.8 |
| 2 | Grok-2 | 1095.8 | 883.7 | 0.4 | 211.8 |
| 3 | Haiku-1 | 1012.2 | 986.7 | 0.1 | 25.5 |
| 4 | Grok-1 | 926.8 | 741.5 | 78.0 | 289.0 |
| 5 | Admiral | 11.1 | 11.1 | 0.0 | 0.0 |

`0.25 × 883.7 = 220.9`, and combat + explore is 212.2, so the explore bank is just under the cap (about 9 points of headroom). Live score is core plus the whole bank. Haiku-2 leads by about 146, almost all of it core. Solar 16 (about 67 s of income, then 19 s) and Impulse Drive 3 (about 86 s, then 24 s) were still unpaid, so the server’s final number can still tick up if either starts.

## 1. Result

Home is sector 1,-5. At the wrap-up snapshot (tick 3467–3483) stock was about 26,352 metal / 3,133 crystal / 3,863 deuterium, production about 227 / 151 / 36 per second, energy 1,253 made against 1,489 needed (mines at 84%).

Buildings: Metal Mine 14, Crystal Mine 14 (finished tick 3373), Deuterium Synthesizer 9, Solar Plant 15, Robotics Factory 5, Research Lab 5, Shipyard 5, Metal Storage 3, Crystal Storage 2, Deuterium Tank 2. Research: Computer 6 (started tick 3447, paid 0 / 12,800 / 19,200, finished tick 3460), Weapons 3, Armour 2, Energy 4, Combustion 2, Impulse 2, Laser 5, Ion 2. Ships: 31 Light Fighters, 9 Heavy Fighters, 4 Small Cargos. No defences, no solar satellites, no outpost, no cruiser.

Two raids were fought off with no resources lost. The second raid destroyed one Light Fighter. The next typical raid (power 1134, 12 light fighters) was still forecast `repelled` (destroyed 100%, keep 98%).

Computer Technology 6 opened the third build slot. Both the building slots and the research slot were empty of running work: Solar 16, Solar 17, Crystal 15, Impulse 3 and Impulse 4 were all waiting, short on crystal. The shipyard was idle.

## 2. Strategy

I tried to win on core. Unspent stock scores nothing, and the explore bank hit the cap early, so almost every point after the opening was a finished building, a finished tech, or a ship.

What worked:

- Mines with solar kept in front of them, up through Metal 14 / Crystal 14 / Deut 9 / Solar 15. Robotics 5 and Lab 5 kept build and research times short. Computer Technology 3 opened the second build slot; Computer 6 (the third slot) finished in the last two minutes, after the crystal and deuterium to pay it had piled up.
- A light-fighter wall before Shipyard 2. Both raids (5 fighters at tick 2001, 11 at tick 2717) were destroyed. Salvage was 4,500/1,500 and 9,900/3,300. The wall stayed docked and the forecast stayed `repelled`.
- Holding the fleet at home while `bonus_capped` was true. Extra charts could not raise the score until core grew. Parking fleet 80 on prospect range 1 did that. By tick 3483 core had almost caught the bank (about 9 points of headroom). Fleet 80 was still holding: the nearest uncharted sector is 2 hexes out and `max_range` is 1.
- A late dump into ships once mine payback no longer fit the clock: Light Fighters first, then Heavy Fighters after Impulse Drive 2 (around tick 2598, 17 minutes left). The shipyard was empty again at the end because crystal had been spent on Computer 6.

What did not:

- The cruiser ladder. Shipyard 5 and Ion 2 were done with plenty of time left. Impulse 2 sat queued on crystal for a long stretch. Impulse 3 (needs about 12,900 crystal, 24 ticks) and Impulse 4 were still unpaid at the wrap-up. Cruisers never became orderable. The fleet that scored was fighters.
- Filling the build queue with the next solar and crystal levels before they could be paid. For most of the last 15 minutes the state showed build slots idle (`running` empty) with Solar 16 and Crystal 14 short on metal. Those orders did start once the stock caught up. Crystal 14 finishing without Solar 16 dropped the energy fraction from about 0.89 to 0.84. The idle gap is what left Haiku-2 about 340 points up on core.
- Charting in the first 15 minutes. Fleet 80 delivered charts until explore was ~124 while core was still ~31. From then until core passed ~850, score was exactly 1.25 × core and further flights added a bank I could not spend. Haiku-2’s explore is 16.8 and their core is 1,225. Haiku-1 did the same (explore 25.5, core 987) and passed Grok-1, who banked even more explore (289) on a smaller core.

## 3. Fun and clarity

The rules plus `./play state` were enough to play a full session without guessing the score formula, the raid eligibility rule, or the “buildings pay when they start, ships pay when ordered” split. The one-line JSON is dense and it is the right shape for an agent.

Things that took a while to trust:

- The engage bar is sharp. A fleet at 1.49× against a 1.5× doctrine sits forever, and the reason says so in plain language. Good sentence, surprising threshold.
- `bonus_capped: true` does not say how much core is missing before another chart is worth delivering. I had to compute 4 × explore myself. At the end the bank slipped back under the cap by about 9 points and nothing in the status line said so.
- Energy fraction below 1 scales every mine and is easy to miss next to a long `next` block. Crystal 14 coming online while Solar 16 was still short made it worse, and the status line did say SHORT.
- Doctrine text is one long policy string. Readable, but the hold reason repeats (fleet 80’s “outside within 1 hops” hold fired twice in one events pull, ticks 3389–3449), which buries a real change.

## 4. Bugs and oddities

- Tick 254, Alert: `fleet 80 stopped mining at [2,-5]: nothing is left to mine there; it carries 0 of 2449`. The harvest event that tick is 0 metal and a trace of crystal and deuterium. The message is clear. The odd part is that the tile was the one I had just flown to from a scan that made it look worth mining.
- From tick 694 through well past 1100, fleet 80 held with: `no safe way to the nearest uncharted sector [3,-7] within 4 hops of home (the fuel tank limits range; max_range is 6): [3,-7] holds hostiles (T2 power 132: your fleet rates 1.49x EVEN against it, the doctrine needs 1.5x)`. The same sentence fired on essentially every check. The 0.01 gap is the real block. The “fuel tank limits range” clause describes a different constraint than the one that fired.
- Tick 2687 RaidIncoming odds were `repelled` with `destroyed: 1.0` and `kept: 0.82`. Tick 2720 RaidResolved was `defended: true`, `outcome: hostiles_destroyed`, `resources_lost` all 0, and `ships_lost: 1` (one Light Fighter, not rebuilt). “Repelled” reads as “we took no losses.” Here it meant “the raid did not breach.”
- Combat on the board stayed 0.4 after a patrol fight (tick 671, one hostile light fighter destroyed) and two home raids (5 and 11 hostile fighters destroyed). The raids paid as salvage resources, which then had to be spent to become core. The score rule says kills pay. If raid and patrol kills are meant to stay out of `combat`, the board is doing that quietly.
- Tick 3389–3449, fleet 80: `the nearest uncharted sector [3,-7] is 2 hexes from home, outside within 1 hops of home`. “Outside within” is hard to parse. The meaning (2 hops, range is 1) is right.

## 5. Balance

The explore cap dominates a one-hour blitz. Banking charts early locks you into “only core matters” for most of the hour, and charting again before core catches up adds a number that cannot raise the score. The winner charted almost nothing. A player who reads the cap and ignores the map plays a different, richer game than a player who uses the fleet. By the last two minutes my core had nearly caught a bank of 212, which is the cap doing what it says. It also means the interesting half of the game (routes, odds, derelicts) was the wrong half to play.

Cruisers are out of reach if you also build the economy that pays for them. Impulse 4’s crystal bill plus a 48-tick research at Lab 5 does not land before the dump window. Heavy fighters (Impulse 2) were the real step up from light fighters. Shipyard 5 was finished long before the drive that uses it. Computer 6, which did finish, ate the crystal that Impulse 3 was waiting on, so the third build slot opened onto an empty queue.

Defences scoring half, and a single cheap defence pulling the ship that shreds it, is the right warning. I built none. The fighter wall was the whole defence and it held, at the cost of one fighter on the second raid. Raid power ignoring ships means the wall has to stay docked, which is why a capped explore fleet should sit at home. That interaction is sound.

A queue of orders you cannot pay yet does not freeze the slot if something cheaper behind them can pay (Computer 6 started that way). It does freeze the slot when every waiting order is short. For several minutes both build slots and the research slot showed nothing running while metal income was over 200 per second. That stock scored 0 until a level finally became payable. At the end the same thing happened on crystal: 26k metal sitting next to 3k crystal, three free build slots, nothing running.

## 6. Top 3 suggestions

1. Put the bonus headroom on `./play state`: explore banked, the cap (0.25 × core), and the core total at which the next chart would score. When the bank is already over the cap, say that on the fleet hold line. Right now the hold line talks about hostiles and fuel while the charts in the hold are worth nothing. When the bank slips back under the cap, say that too: at tick 3483 I had about 9 points of room and no signal except the arithmetic.
2. When every waiting order is short, and `covered_in_s` plus build time runs past `minutes_left`, say so on that queue. Solar 16 showed about 67 s of income and 19 ticks with 2 minutes left, which can still finish. Impulse 4 and Solar 17, both “starts in ~4 m” with 2 minutes left, cannot. `cancel_queued` on an unpaid building is free, but nothing in the state flags the order as one that cannot score.
3. In a blitz hour, either soften Impulse Drive’s crystal doubling or make an earlier hull the one that is worth a shipyard. Shipyard 5, Ion 2, and Laser 5 were all done. The cruiser stayed locked on Impulse 4, and the points went into light and heavy fighters instead. A player who reads the catalogue spends a long time building a key that does not turn.

## 7. The `play` tool

`./play state` as one JSON line is the right interface. `minutes_left` on every call, `bonus_capped`, raid `odds.label`, `next` costs with `ok` / `locked`, and queue entries with `short`, `covered_in_s`, and `waiting_on` were all present and were what I actually planned from. `./play do` with several JSON orders, and `./play wait --until idle` capped at 60 seconds, is the right loop. The batch spacing is enough that I never felt I was hammering the server.

What would have saved time:

- The leaderboard, or at least first place’s score and the gap, inside `state`. `./play score` is a separate call, and for a long stretch I only knew my own rank. The gap to first is the decision between “keep booming” and “dump now.” At the end the gap was 146 points of core I was not going to close with two unpaid queues.
- A single field `score_if_queues_finish` versus `score_now`, so a queue full of unpaid solars is obviously not a plan. Computer 6 opening a third slot onto three orders that all wait on crystal is the case that field would have named.
- `wait` already returns early on a free slot, a newly payable order, or an alert. It does not return just because a queue was already idle. That is correct and easy to misuse: an agent that treats “idle” as “your turn” will spin. One clause in the wait output when the queues were idle the whole sleep (“queues unchanged, still waiting on resources”) would make that obvious. The wrap-up line when minutes drop under 6 is exactly the right nudge.
