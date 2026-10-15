# Playtest 2026-10-15: one hour at pace 1200, four agents and fifteen bots

Server at commit `70d09b0`. The score is buildings, research and the fleet
kept. Kills and charts pay in resources and score nothing themselves.
Defences score nothing. The built-in bots play that score: a strike wing
under standing orders scoops and boards, and no wing charts or patrols for
points.

- **World:** `IAC_PACE=1200`, port 7790, 60 minutes, 00:24 to 01:24 CST
  (16:24 to 17:24 UTC on 14 October). The last agents stopped at 01:28.
- **Players:** four `grok` CLI agents (grok-4.7), free play, each told to
  read `tools/playtest/RULES.md`. No web account joined.
- **Bots:** `IAC_BOTS=all:all`, the fifteen styles (five personalities, each
  at check-in, hybrid and hands-on).
- **Harness:** all four agents ran the hour and wrote feedback. Exits were 0.
  Grok-3 finished on its first launch, Grok-1 and Grok-4 on their second,
  Grok-2 on its third. The server log is the one line `listening on
  0.0.0.0:7790`, and nothing else.
- **Feedback:** `Grok-1.md`, `Grok-2.md`, `Grok-3.md`, `Grok-4.md`, with each
  player's `-notes.md`. Grok-2's essay is the end of the hour. The other
  three were written at the six-minute warning, so the scores in those
  essays are earlier than the table.

The run is in `tools/playtest/run-2026-10-15/`, world included. The October
14 world was left where it was: this build reads format 15 and refused that
file (format 12).

## Final board (tick 3782, server stopped)

Outpost and defence are 0 on every row, players and bots.

| Rank | Player | Score | Buildings | Research | Fleet |
|---|---|---|---|---|---|
| 1 | Grok-2 | 5625.5 | 3223.6 | 2252.9 | 149.0 |
| 2 | Grok-3 | 5076.1 | 3102.1 | 1800.5 | 173.5 |
| 3 | Grok-4 | 4680.1 | 2510.6 | 1982.0 | 187.5 |
| 4 | Grok-1 | 3985.8 | 2098.5 | 1738.8 | 148.5 |

Bots, unranked. Hands-on: mixed 6623.8 (fleet 1544.5), turtle 5424.1,
raider 4261.5, explorer 4132.2, miner 4014.1. Check-in: mixed 4553.3,
explorer 4296.2, turtle 4234.9, raider 3723.0, miner 2067.5. Hybrid:
raider 3732.4, mixed 3030.8, miner 2914.5, turtle 2733.4, explorer 2345.6.

## What it shows

**The hour was mines and research.** Three of the four agents left the
shipyard at 1 and built no defence. The typical-size forecast said the raid
was lost and they would keep nothing, and a shipyard at 2 or any defence is
what makes that raid real. Defences score 0, so they never started it.
Grok-2 won on research (2252.9 against Grok-3's 1800.5) with two Light
Fighters and 35 Solar Satellites. Fleet parts on the agent table are 149 to
187.5.

**One agent did fight the raid, and finished third.** Grok-4 raised the
shipyard to 6, kept 11 Light Fighters, 4 Cruisers and 16 Rocket Launchers
at the dock, and repelled the waves (20, then 40, then 42 Light Fighters;
an early 7-fighter raid, before the cruisers, wiped the starting fleet).
The wreckage paid for what was lost. The rank was still lost on research.
Sitting at 16 launchers, one under the count where the raid brings cruisers,
was the decision they named.

**Check-in bots play at this pace.** At blitz on 14 October the ten check-in
and hybrid bots scored 29 to 100. Here a check-in day is about 40 minutes,
and check-in scores run from 2067 to 4553, hybrid from 2346 to 3732. The
hands-on mixed bot leads the world at 6623.8, ahead of every agent, on a
fleet part of 1544.5. The agents who trusted the forecast never built a
fleet that could be compared with it.

**The map did not pay.** At x1200 a mine level is worth more than a chart,
a convoy wreck and a jump. Grok-4 never left home and called that the
correct score play. Grok-1 fought once, in two rounds, and stopped. Grok-2's
two fighters won the inner-ring packs at 3x to 6x and lost nothing, and
that salvage did not move the rank.

## Problems reported

As the players gave them. None of these was checked against the code before
it was listed.

1. Wreckage gone inside its 180-tick life (Grok-1, Grok-2). A pile reported
   at the end of the fight was `NoResources` "no salvage in this sector" 18
   to 53 ticks later, with the fleet still there or back in the sector, and
   no other fleet named on the sector line. A full hold scooped the metal
   and left the crystal and deuterium on the ground.
2. The typical-size forecast reads as a raid on the clock (Grok-1, Grok-2,
   Grok-3). With the shipyard at 1 and no defence, status still ends in an
   alert that the next raid is lost and nothing at home is kept. All three
   played the hour as if combat were closed.
3. A short order at the head of a queue idles the slot (all four). The tool
   says so, and tells the player to order something payable. A queue of
   crystal-starved research left the lab idle for minutes while a metal
   armour level would have started. `reserve` holds the slot on purpose.
4. `wait --until idle` returns when any queue is idle, including a shipyard
   that was never used (Grok-3). `wait` sometimes returned early when a raid
   arrived and sometimes ran out the clock through builds and a fight
   (Grok-4).
5. A docked fleet was pulled into someone else's fight (Grok-4). Grok-3
   shot the convoy that respawns on Grok-4's home tile, and Grok-4's garrison
   took hull in that round. Nothing was lost.
6. A patrol will not path home through an unseen hex (Grok-3). The odds are
   the ring's estimate, the scan of the hex showed no hostiles, and the
   damaged fighter sat until it was walked home by hand.
7. The Solar Plant cap is invisible until the order is refused (Grok-4).
   Level 20 finished, a further order returned `MaxLevelReached`, and
   Crystal Mine 17 then dropped the energy fraction under 1 for the rest of
   the hour. `next` had said the mine was payable and said nothing about
   the fraction.
