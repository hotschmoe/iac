# Grok-1 playtest feedback

Observed at tick 3260, 5.7 minutes left. The wrap-up banner was already up, so this is the closing position, not a later guess.

## 1. Result

**Rank 3 of 5. Score 926.8. Core 741.5, combat 68.9, explore 289.0. Bonus capped** (`0.25 * 741.5 = 185.4`; combat + explore was 357.9, so about 173 points of that were scored and then discarded).

Leaderboard at the same tick:

| Rank | Name | Score | Core | Combat | Explore |
|---|---|---:|---:|---:|---:|
| 1 | Haiku-2 | 1113.6 | 1096.8 | 0.0 | 16.8 |
| 2 | Grok-2 | 960.3 | 768.2 | 0.4 | 211.8 |
| 3 | Grok-1 | 926.8 | 741.5 | 68.9 | 289.0 |
| 4 | Haiku-1 | 847.0 | 821.5 | 0.1 | 25.5 |
| 5 | Admiral (human) | 11.1 | 11.1 | 0.0 | 0.0 |

I was rank 2 at tick 3235 with this same 926.8. Grok-2 passed on core between tick 3252 and 3260. I did not gain a point in that window because every queue was idle.

**Stock:** 24,814 metal / 19,983 crystal / 8,073 deuterium. Caps 6,260,338 / 1,565,085 / 1,565,085. Income 215.8 / 143.3 / 40.16 per tick. Nothing in the stockpile scores.

**Energy:** 1,253 made, 1,323 needed, fraction 0.947. The mines have been on that shortfall since about 14 minutes left. Solar Plant 16 costs 32,842 metal and 13,137 crystal (19s) and was still short on metal.

**Buildings:** Metal Mine 13, Crystal Mine 13, Deuterium Synthesizer 9, Solar Plant 15, Robotics Factory 5, Shipyard 4, Metal Storage 4, Research Lab 5. Fusion Reactor, Nanite Factory, Crystal Storage and Deuterium Tank at 0. No outpost.

**Research:** Energy 4, Combustion 4, Impulse 3, Computer 5, Espionage 2, Weapons 5, Armour 5, Laser 5, Ion 2. Shielding, Hyperspace, Hyperspace Drive, Plasma and Graviton at 0.

**Ships:** 32 Light Fighters, nothing else. 28 sit in fleet 1138 at home on manual (the raid guard). Fleet 1094 (1) had just docked and been mended. Fleet 78 (1, hull 90%) was out at 4,-3. Fleet 1017 (2) was prospecting at 5,-2. No defences, no solar satellites.

**Raid forecast:** power 1,109, 12 Light Fighters, repelled (destroyed 100% of sample fights, keep 95% of what stands there). Home power 3,944.

**Map:** 99 known sectors at tick 3252 (27 live, 72 stale). Explore was already over the cap long before that.

At tick 3260 the build, research and ship queues were all idle while several payable, positive-score upgrades sat there: Fusion Reactor 1 (900/360/180, 1s), Shipyard 5 (6,400/3,200/1,600, 4s), Crystal Storage 1, Deuterium Tank 1, Research Lab 6 (6,400/12,800/6,400, 8s, about 38 core), Robotics 6, Energy Technology 5, Laser 6, Ion 3, Espionage 3. The planner had stopped because Solar 16 and Metal Storage 5 were the "right" next spends and neither was payable. It did not fill the idle queues with the smaller ones that were.

## 2. Strategy

Score = core + min(combat + explore, 25% of core), and unspent stock is worth nothing. I treated core as the only uncapped number, kept energy near 100%, delayed Shipyard 2 so raids would not start, skipped defences (half value, and a stack of rocket launchers changes what the raid flies), and used two small fleets to chart.

What worked:

- Once a driver was actually issuing orders, the economy moved. From tick 1304 (38 min left, score 10, mines 2/2/0, solar 3) to tick 3221 (6.3 min left, score 926.8, mines 13/13/9, solar 15) is a real climb, and it was enough to pass Haiku-1 and sit second for a while.
- Crystal stayed the scarce resource, as expected. Metal storage was the right dump: full cost-weight, no crystal. Storage 1 through 4 is a visible slice of the late core.
- A home guard of light fighters holds this raid. Forecast stayed "repelled" from the first time I looked through the end. I never needed a defence structure.
- Combat and charting were easy at ring 4. One light fighter beats a T1 convoy. Two are comfortable through the nearby T2s. I lost no ships.

What did not:

- The opening. At 38 minutes left I was still Metal 2 / Crystal 2 / Solar 3 / score 10, rank 4 of 4, with about 19k metal and 12k crystal sitting unscored. That was me reading the rules and writing `/tmp/grok1_player.py` while the first mines, which finish in a few seconds at x600, were not queued. The rest of the hour was climbing out of that hole. I did not get out. Haiku-2's entire lead is core (1,096.8 vs 741.5).
- An energy heuristic that refused any upgrade leaving the fraction between 96% and 100%. Mines froze at 12/9 while the shipyard walked itself to 4 and a research reserve on Computer Technology held the lab. Raids turned on early for no benefit. I noticed around 16 minutes left, froze the yard at 4, and put mines first. They only reached 13/13/9. The lost levels stayed lost.
- Staying on Solar 16 after it was unaffordable. From roughly 8.6 minutes left the build line in the notes is just "recall thin": queues empty, energy still 1,253/1,323, metal storage 5 and solar 16 both short. Fusion Reactor 1 was payable and I never asked what it produces. Yard 5, the other storages and several techs were payable by the end. Waiting on one perfect building is how you post a flat score while Grok-2 walks past.
- Spending the bonus and then some. Explore 289 + combat 69 against a cap of 185. Every chart and kill after the cap was zero score, and the light fighters that earned it were core I could have had as labs and robotics instead. Haiku-2's explore is 16.8 and they won. Haiku-1's is 25.5 and their core (821) still beats mine. The galaxy was a trap under this formula.
- Thin patrols after the cap. Fleets 78 and 1094 kept fighting T1 convoys for nothing. Fleet 78 was shot while charging a jump home (quoted below). Standing orders do not know the score is capped.

## 3. Fun and clarity

The game is understandable, and the tool is a large part of why. Every status repeats the scoring formula, `bonus CAPPED` is unambiguous, and threat plus an odds label (`SAFE`, a ratio, repelled / lost) is enough to decide whether a fleet may jump. `state` as one JSON line is the right shape for a driver. Batch `do` plus `wait --until idle` is the right loop once you know it. I enjoyed the minute where the economy was compounding and the board was moving.

What was hard:

- The rules are a reference manual. The overview says queues hold three items; my build queue at Computer Technology 5 was "0/2 slots, 5 items max". Both may be true, but I had to learn the live numbers from `state` after planning around the overview. The outpost chapter is longer than the decision was worth in a one-hour blitz, and I never founded one.
- "Energy SHORT, mines run at 95%" reads like a fire. It is a 5% income loss. Treating it as a hard gate is what stalled the mines. The tool does not say "this next mine leaves you at 97%, payback is still 24s".
- The score cap is clear on the status line and invisible on the fleet lines. Patrol and prospect kept generating events, wreckage and "returning" messages after every extra point was being thrown away. A human who likes the map will do the same thing.
- Jump windup combat is easy to miss. A fleet that has already decided to leave can be shot, and it does not shoot back.
- Free repairs at the home dock ("mended ... for nothing") are strong and I only learned them from an alert, not from the plan I was executing.
- Admiral at 11.1 core on the same clock is the clarity result for a human. x600, with mines finishing in seconds, is not a pace at which the rules can be read and the web UI played. This table was an AI table with a spectator.

## 4. Bugs and oddities

**Wreckage figures for one kill do not agree.** Fight at 2,-1, ticks 3245-3246, fleet 1094 destroyed one light fighter. Three reports of the same pile:

- combat summary: `wreckage 279/152/76`
- `FleetDestroyed` salvage: `{"metal":89.31,"crystal":48.72,"deuterium":24.36}`
- `SalvageCollected` by fleet 78: `279.00 / 152.18 / 45.34`, then fleet 1094 took the remaining `30.75` deuterium

Collected metal and crystal match the wreckage line, and the two deuterium scoops sum to about 76. The `salvage` object on `FleetDestroyed` is about a third of that and matches neither. Same shape on the kill at 4,-3 a minute later (`wreckage 97/53/26` in the combat line). I could not tell which number is the score weight.

**"Returning" does not mean going home.** Fleet 1017 (prospect, home 4,-2) logged, in order: at 4,1; "returning: a jump to [5,2] would leave 240 fuel against 252 needed"; entered 5,1; "returning" again; entered 5,0; entered 5,-1; entered 5,-2. Each refused jump was a real stranding. The jump it took instead walked past home. At 5,-2 a single hop west is the homeworld. The policy is preserving a return, not taking one.

**Shot during jump charge.** Tick 3255: `fleet 78 was caught at [3,-1] while charging its jump to [4,-2]: it is fired on and does not answer until the jump lands in 1 tick`. It landed, and the dock mended 58 hull for nothing. If the windup is meant to be a vulnerable beat, the alert is the only thing that says so. If the fleet has already paid for the jump, being unable to fire back feels like a race.

**Rank line vs board.** Status at tick 3252 said rank 2/5 at 926.8. The scoreboard at tick 3260 said rank 3, same score, because Grok-2's core moved. `age_s` on the score object was 1-2 seconds, so this is ordinary staleness, but with under a minute of movement left it is the difference between second and third.

No command-level `ERR` in the stretch I watched. The damage I did to myself was the planner refusing legal builds, not the server refusing them.

## 5. Balance

The 25% cap decides the hour. Haiku-2 won by almost ignoring combat and explore. I "played the game" (99 sectors, a dozen convoy fights, a 28-fighter guard) and finished third, behind a neighbour with less explore and behind a neighbour with essentially none. After the cap, another chart is worth the same as losing the fleet that carries it: nothing, plus a possible loss of core. That is a coherent formula and a bad incentive. The interesting systems (threat, odds, wreckage, derelicts, the hub) turn off at the moment the player is strong enough to use them.

Defences are a trap on two axes. They score at half, and the rules tell you a pile of rocket launchers pulls cruisers into the raid. Light fighters score at full weight and hold a 12-fighter raid. I built zero defences and never regretted it. The defence catalogue might as well be absent in this format.

Storage-as-score is the logical endgame and it is dull. Metal storage converts the resource you are long on into core at full weight and asks for no crystal. From about 14 minutes left, "correct" play was buy storage, refuse ships, and wait on a solar I could not afford. Crystal storage and the deuterium tank cost the resources that research also wants, so they lose to one more lab level. The endgame choice is not really a choice.

Light fighters are the only hull that matters here. A cruiser wants Shipyard 5 and Impulse Drive 4. The stated benchmark is a cruiser at about 53 minutes. The player who won did not need one, and ships built after the bonus cap only pay their core, which labs pay without a raid clock. Yard 2 also starts raids. I stumbled into yard 4 and the raid stayed a light-fighter pack I could repel, so that threat is milder than the rules made me fear. Still, heavier hulls never got a job.

Free, immediate hull repair at the home dock makes attrition free. The only risk on a T1 patrol is losing the hull outright. Fleet 78 and 1094 spent the last ten minutes trading hull for zero score and arriving home whole.

x600 is the other balance fact. A competent economy finishes a mine in seconds, so any delay that a human would spend reading is a lost building. The human account finished at 11.1. Blitz is a legal preset and a poor one for a shared human-and-AI table.

## 6. Top 3 suggestions

1. **Keep the galaxy worth points after the economy is ahead.** Show the unused cap on the fleet lines (`next new sector: 0, you are 173 over the cap`) and give standing orders a "stop when capped" switch. Better, replace the hard clamp with a diminishing term that still pays something, so a chart delivered at minute 50 is a reason to leave home. As written, the winning policy is to never undock.
2. **Make energy and the next build a single number.** On each `next` entry, report energy fraction after the build, income after that fraction, payback at that income, and core gained. I stalled for a quarter of the session because "SHORT" and a payback computed at full power disagreed, and I believed the warning. Naming Fusion Reactor as a way to cover a 70-energy gap, when solar 16 is short 8,000 metal, would have unstuck the late game by itself.
3. **Score defences fully or stop punishing them, and slow the clock when a human is seated.** Half-value defences that also upgrade the raid are never built, so raids are a ship check and the defence list is dead weight. And a table that includes a web player needs a pace at which a mine lasts long enough to read `play costs`. Admiral's 11 points is the session saying the human could not play.

## 7. The `play` tool

What I would keep: `state` as one JSON object, the score breakdown and the cap flag on every status, batch `do` with one result line per order, `wait --until idle` returning on a free queue, `odds` / `preview` so a driver does not have to simulate combat, and the WRAP UP line. Events collapsed to one line per fight are readable. The raid forecast (ships, power, destroyed / outlasted / lost, fraction kept) is exactly the decision.

What would have changed this session:

- A shortfall on each `next` item (`SolarPlant ok:false, short 8,028 metal`) so a driver can queue a waiting order or pick something else without re-deriving cost minus stock. `ok: false` alone caused empty queues.
- Energy-after and score-gained on `costs`, as above. Payback at a 100% fraction is a lie when the build you are pricing is what breaks 100%.
- Standing orders that can see the cap. `prospect` and `patrol_home` should accept something like `stop_when_bonus_capped`, and `status` should say "fleets are still flying; bonus is capped" rather than only printing the formula.
- A true recall. "Returning" on fleet 1017 burned the last fuel walking around home. A `come_home` that picks the neighbour which reduces distance, or an explicit `recall` result in `state` (`path: [5,-2 -> 4,-2]`, `fuel: 32`), would have been trustworthy. The fuel sentence it prints is good; the hop it then takes does not match the sentence.
- One wreckage number. Label `FleetDestroyed.salvage` if it is pre-finds or someone else's share, and make it equal the wreckage line the combat summary prints.
- Queue math in the overview matched to the live line. "Holds 3" vs "2 slots, depth 5" cost a planning pass. Printing `slots from Computer Technology 5: 2 build, depth 5` on the queue line would close it.
- A `play state --keys stock,energy,queues,next,score` so a poll does not haul 99 sectors of fleet prose. The full dump is fine every few minutes. It is a lot every loop, and the interesting `ok` flags are past the point where a log gets truncated.

I stopped on the wrap-up banner with the queues idle. The points still available (lab 6, robotics 6, energy 5, fusion 1, yard 5) are real, and they are the same class of miss as the rest of the hour: a payable core upgrade, and no order sent.
