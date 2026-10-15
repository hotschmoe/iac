# Grok-4 feedback

Session pace x1200, home -1,-4 (ring 4). Stopped at tick 3297 with 5.1 minutes left, as the tool called wrap-up.

## Result

Rank 3 of 4. Score 4296.1 (buildings 2510.6, research 1598.0, fleet 187.5, outpost 0, defence 0).

| Rank | Player | Score | Buildings | Research | Fleet |
|---|---|---|---|---|---|
| 1 | Grok-2 | 4874.3 | 2600.4 | 2124.9 | 149.0 |
| 2 | Grok-3 | 4320.9 | 3102.1 | 1045.3 | 173.5 |
| 3 | Grok-4 | 4296.1 | 2510.6 | 1598.0 | 187.5 |
| 4 | Grok-1 | 3337.6 | 1706.3 | 1482.8 | 148.5 |

Grok-2 led the whole second half. The gap at the end was 578, of which research was 527. I led Grok-2 on fleet (+38) and trailed on buildings (90) and research. Grok-3 passed me in the last minutes on buildings (3102 vs 2511) despite a much smaller lab.

Stock still in the pile, scoring nothing: metal 192650, crystal 72036, deuterium 24709. Income 1335 / 569 / 102 per second. Energy 2690 made, 2725 needed, fraction 0.987, and that penalty is permanent.

Buildings: Metal Mine 20, Crystal Mine 17, Deuterium Synthesizer 10, Solar Plant 20 (maximum), Robotics Factory 8, Shipyard 6, Metal Storage 7, Research Lab 7. Fusion, nanite, crystal storage and the deuterium tank were never built.

Research: Computer 6, Weapons 9, Shielding 7, Armour 8, Energy 6, Combustion 6, Impulse 5, Laser 5, Ion 2. No espionage, hyperspace, plasma or graviton.

Still queued, unpaid, and likely to finish after I stopped: Armour 9 (about 47 s to afford, then 96 s, 256 points), Energy 7 (about 9 s to afford, then 20 s). Robotics 9 was still 259 s short of deuterium and probably does not finish.

Ships at home, fleet 2214: 11 Light Fighters and 4 Cruisers, hull full. 16 Rocket Launchers. No solar satellites, no cargo, no outpost. Every raid that arrived was repelled (20, 40 and 42 Light Fighters in the second half; an early 7-fighter raid, before the cruisers, wiped the starting fleet and stole stock). Losses after the fleet was real: one Light Fighter and two Rocket Launchers, the launchers rebuilt on the spot. Wreckage was scooped by the docked fleet (about 18000/6000, then 36000/12000, then 37800/12600, plus two small convoy piles).

## Strategy

Build solar just ahead of the mines, keep the crystal mine as the income priority, and hold the only fleet on the home dock as the raid garrison. Stay under 20 Rocket Launchers so the raid stays a ball of Light Fighters. Spend metal that crystal cannot use on armour and metal storage. Take Computer 6 for the third build slot, Robotics and the lab high enough that build and research times stop mattering, then dump crystal into Weapons, Impulse, Shielding and Energy.

What worked:

- The raid rule. At 16 launchers every forecast stayed "repelled" with a pure Light Fighter wing, from 20 up to 50 fighters, and the wreckage more than paid for the launcher and the fighter I lost.
- Docking the fleet. The home dock mended hull for nothing after every fight, scooped the wreckage, and the cruisers were why the forecast never flipped to lost.
- Reading the score gap by part. The hole was research, not fleet. Weapons 8 and 9, Impulse 5 and Shielding 7 were the right sinks once the mines were paid.
- Not flying. At this pace a jump is still real seconds, and one second of crystal income was worth more than a convoy wreck or a chart. I looked at the six neighbouring sectors once, all stale, and never left.

What did not:

- I kept raising mines until the solar plant hit its cap, then built Crystal Mine 17 anyway. That dropped energy to 0.987 forever and shaved every mine. The crystal was worth more in Weapons 9.
- Grok-2's research lead opened in the middle of the hour and never closed. Armour 9, the 256-point tech, was queued late and was still waiting when I stopped. That was the game.
- Deuterium double-booking. Computer 7, Robotics 9 and Energy 7 all wanted the same deuterium. I cancelled Computer 7 to save Robotics 9, and Robotics 9 still will not start before the session ends. Cancelling a sure 115 points for a 251-point building that cannot finish was a loss.
- Unspent stock at the end (roughly 190 points of metal, 108 of crystal, 49 of deuterium) was reserved for those three orders. Some of it should have been cruisers.

## Fun and clarity

The rules and the tool output were enough to play the hour without guessing a formula. `covered_in_s`, the resource shortfall, the raid forecast (`label`, `kept`, the ships it will fly) and `gap_by_part` are the right information. I never had to compute a build cost, a ship stat or a combat round.

What confused me:

- Solar Plant 20 is the maximum. I learned that from `MaxLevelReached` after I had already treated "queue the next solar" as always legal. `next` only shows `maxed` once you are there.
- A short order does not hold the queue unless `reserve` is set, so a cheaper order behind it starts first and steals the stockpile. I had to cancel Armour more than once because it would have eaten the metal a solar plant was waiting on. The rule is written down. It is still easy to queue a good plan and watch it fire in the wrong order.
- The tool kept saying the free slot was idle and that I should order something payable. Most of those times the stockpile was already the savings for a larger queued order. Paying for a small thing was how I delayed Armour 9 and Impulse 5.
- Another player's fight on my home tile drafted the docked fleet for one round. Twice, Grok-3 shot the passive convoy that respawns on -1,-4, and the report included my hull. Nothing died. I had not ordered the fleet to fight.
- The leaderboard row for Grok-3 sat on exactly 3610.6 for several minutes, then jumped. Rank 2 and rank 3 swapped on stale numbers.

## Bugs and oddities

- `MaxLevelReached` on the solar plant: `ERR build SolarPlant: MaxLevelReached Solar Plant is already at its maximum level` at tick 2556. The plant had finished level 20 at tick 2452. An order placed while level 20 was still running would have been the natural way to queue the next one, and there is no next one. The refusal is clear. The cap is not visible earlier.
- A `wait 42 --until done` returned at 21 s: `RETURNED EARLY` on `ALERT[Warning] the raid of 20 Light Fighter has arrived`. Later `wait 40`, `wait 45`, `wait 60` and `wait 55 --until done` all ran the full time and printed `Nothing actionable happened` even when builds finished and a raid was fought. The early return is the more useful behaviour. I could not tell which one I was going to get.
- Grok-3's board line was 3610.6 at the tick-2955 score and still 3610.6 at tick 3040, then 4320.9 at tick 3297. Ours moved every poll. Either that player banked for several minutes or the row was stale. `age_s` on my own score was a few seconds, so I trusted it, and I misread the rank because of theirs.
- Home-tile draft, tick 2808 and again tick 3181: `CombatStarted {"player_fleet_id":858,"owner":"Grok-3","enemy_fleet_id":8589737983,...,"mine":false}` and then `your side took 38 hull and lost 0 ship(s)`. The convoy sits on the home sector all hour and respawns. Anyone who shoots it pulls the docked garrison into the round.

## Balance

The raid, played this way, is too easy and too profitable. Sixteen Rocket Launchers plus a few cruisers repelled every wave, the launchers that died were rebuilt free, and the wreckage was thousands of crystal. The interesting decision was "never build the 20th launcher", because that is the point where the raid swaps into cruisers. A single threshold that an agent can sit one under is not much of a defence game.

Solar Plant 20 is a hard ceiling. Past it the next crystal mine is a permanent percent off every mine, fusion level 1 is a toy that burns deuterium the lab needs, and solar satellites die in the raid and cost the crystal the lab also needs. I took Crystal Mine 17 and ate a 1.3% penalty for the rest of the hour. There was no clean way to power a mine past that.

At x1200 the map does not exist. Fuel, jump time and combat rounds do not scale, and the mines do. A convoy on the home tile was worth less than a second of crystal. Charts, derelicts, the hub and an outpost were all worse than one more mine level. I finished the hour having jumped nowhere. That was the correct score play, and it means a season-pace or blitz session is a spreadsheet with a raid check, not a space game.

Research levels are the score, and they double. Weapons 9 and Armour 9 are worth more than a whole mine tier. Whoever starts the armour/weapons ladder a few minutes earlier is the winner, because the follower's income advantage never catches a 500-point research lead before the hour ends. Grok-2 was that player.

Defences scoring 0 is right. Unspent stock scoring 0 is right, and it is why the last five minutes hurt: a deuterium-blocked Robotics 9 was sitting on resources that should have been ships.

## Top 3 suggestions

1. Make the map worth a minute of income at this pace, or scale jump time with the pace. As it stands an agent who leaves home is donating the game. A chart or a derelict should be worth something on the order of a mine level, and a docked fleet should not be drafted into someone else's fight against the convoy that respawns on the home tile.
2. Show the consequence of the next mine before it is built: energy after it finishes, and the cap on the solar plant. Crystal Mine 17 was a trap I walked into because `next` said `ok: true` on cost and said nothing about the fraction falling under 1 with no solar left to build. Fusion needs to be a real answer at that point, or the solar cap needs to move.
3. Soften the 20-launcher raid threshold. A mixed wing that grows with the economy, instead of a pure Light Fighter ball until the 20th launcher, would make the garrison a real choice. The wreckage of a repelled raid can stay generous. The "sit at 16 and collect" line should not be this obvious.

## The `play` tool

The tool is good. `state` with the queue note, `covered_in_s` and the score gap is the whole interface, and batching several JSON orders in one `./play do --keep-going` is the right shape for a 1 Hz tick.

What would have helped:

- A slack line: resources left after every waiting order is paid, and how many seconds each new order delays the ones already queued. The current note ("order something payable to use the slot now") pushed me to spend the metal and crystal those waiting orders were about to use.
- `reserve: true` called out on the order that is short, in the queue listing, together with which orders behind it are allowed to jump. I learned the jump rule by watching Armour start and empty the solar fund.
- The building maximum in `next` before the level is reached, not only as `maxed` afterwards.
- One scoreboard, with each row's age. A frozen opponent score changed my rank calls for several minutes.
- `wait --until done` either always returns on a raid alert or always waits the full interval. Both happened. The early return is the one I want, because a raid landing is the moment the garrison matters.

I never needed a second client. Everything above came from `./play state`, `./play score` and `./play do`.
