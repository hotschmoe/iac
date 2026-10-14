# Playtest 2026-10-14: the OGame catalogue and fight, one hour, blitz, four agents and fifteen bots

Server at commit `7cd4e33`: all five slices of `docs/design/ogame.md` (OGame's
catalogue with energy, fleets as counts, OGame's round a tick, lasting damage and
refit at home, flight and pursuit, pirates that mend, raids fought as battles,
Solar Satellites rebuilt with defences, the windshield as class marks and the
battle view).

- **World:** `IAC_PACE=blitz` (x600), port 7790, 60 minutes, 04:57 to 05:57 UTC.
- **Players:** two `grok` CLI agents (grok-4.7) and two Claude Haiku 5.5 subagents,
  free play, each told to read `tools/playtest/RULES.md`. A web account named
  Admiral joined and stayed at its starting score.
- **Bots:** `IAC_BOTS=all:all`, the fifteen styles (five personalities, each at
  check-in, hybrid and hands-on).
- **Harness:** both grok agents ran the hour and wrote feedback on their second
  launch (`harness-events.log`). The server logged no warning or error.
- **Feedback:** `Grok-1.md`, `Grok-2.md`, `Haiku-1.md`, `Haiku-2.md`, with each
  player's `-notes.md`.

## Final board (05:56 UTC)

| Rank | Player | Score | Core | Combat | Explore |
|---|---|---|---|---|---|
| 1 | Haiku-2 | 1241.4 | 1224.6 | 0.0 | 16.8 |
| 2 | Grok-2 | 1095.8 | 883.7 | 0.4 | 211.8 |
| 3 | Haiku-1 | 1012.2 | 986.7 | 0.1 | 25.5 |
| 4 | Grok-1 | 926.8 | 741.5 | 78.6 | 289.0 |
| 5 | Admiral (web) | 11.1 | 11.1 | 0.0 | 0.0 |

Bots, unranked: the five hands-on bots scored 949 to 1717 (raider 1717, mixed
1499, explorer 1442, turtle 1190, miner 949). The ten check-in and hybrid bots
scored 29 to 100.

## What it shows

**The player who never left home won.** Haiku-2 charted almost nothing and won
on core alone. Every player who flew filled the 25% cap on combat plus
exploration early (charts alone fill it), after which a chart or a kill was worth
nothing and a fleet in the field was core not built. All four players said so.
The balance study measured the same thing: one chart outweighs forty kills.

**Kills barely count, and raids not at all.** Grok-1 had 78.6 combat points after
a dozen convoy fights; Grok-2 and Haiku-2 repelled several raids each and scored
0.4 and 0.0 for them. A repelled raid pays its wreckage as resources, and the
study measured that wreckage at six to seven times what raids destroy: raids are
income.

**Deuterium starves the opening.** Both Haiku players were held for the first
several hundred ticks by deuterium (100 at the start; Robotics 2, the Research
Lab, Combustion Drive and Energy Technology all ask for it), and both fixed it
with trips to the hub exchange, which became their main lever. The study found
no bot short of deuterium in its first minutes, so this is a player's opening
order, not a hard gate; it is still the first wall a new player hits.

**One payable order cannot pass an unpayable one.** In every report: an
unaffordable building at the head of the queue held a slot while income piled up
as stock that scores nothing; `reserve` made it worse. The 2026-10-10b playtest
reported the same.

**Late stock has nowhere to go.** Mine levels in the twenties of thousands of
metal, defences scored at half and pulling stronger raiders (no player built
one), storage buildings as the dull best sink, Cruisers out of reach in the hour
(Impulse Drive 4's crystal), so the late game was Light and Heavy Fighters and
unspent stock.

**Check-in and hybrid bots do not play at blitz.** Their look-ins are counted in
wall-clock days, so in an hour they barely act (29 to 100 points against 949 to
1717 for the hands-on bots). The single-player host already seats hands-on bots
at a fast pace; the server's `--bot` does not.

**Blitz is too fast for a person and too slow for the arc.** The web account
never got past its start; the study puts the first Cruiser at 65 to 78 minutes
of blitz for a hands-on bot.

## Problems reported (as the players gave them; only the first was checked
against the code before it was listed)

1. `policy_update` refused as an unknown variant (Haiku-2): orders take
   `"action"`, a standing order takes `"type":"policy_update"`; the player sent the
   first form. The play tool could take either, and the refusal should say which.
2. One kill's wreckage printed three ways (Grok-1): the combat summary's
   wreckage, `FleetDestroyed.salvage` (about a third of it) and what was collected.
3. A `load` refusal worded about a Light Fighter, and an `exchange` from an empty
   hold printed `ok` (Haiku-1).
4. Stock jumped by thousands with no event saying why (Haiku-1): most likely a
   repelled raid's wreckage scooped at home; the event should say so.
5. "Returning" walked past home and stranded a prospecting fleet on fuel
   (Grok-1).
6. A doctrine's hold reason blamed the fuel tank when the engage ratio was what
   held it, and repeated every tick (Grok-2).
7. A "repelled" raid still destroyed a Light Fighter (Grok-2): the label says the
   raid failed, not that nothing was lost.
8. A fleet charging its jump home is fired on and cannot answer (Grok-1): the
   rule, worded only in an alert.

## Measured beside it (a balance study on headless bot worlds, three seeds each)

The proposals the owner was given, ranked by the study, with what it measured:

1. Pirates' clock (respawn, mending, farm heat) on the finds pace class: at blitz a
   group is whole again in 10 to 30 seconds today, so wearing one down does
   nothing; with the change 4 to 9 minutes at blitz and 26 to 66 at season.
2. Kills against charts: `score::KILL_SHARE` 1 to 10 with chart points a third;
   raider kills in a blitz two hours 17 to 111.
3. `RAID_DEFENSE_SALVAGE_FRACTION` 0.30 to 0.10, so a raid is not income.
4. A one-hour session at pace 1200: first Cruiser at 35 to 41 minutes.
5. `WRECK_VALUE_K` x3, so a hunt pays for its fuel.
6. Bots: the server's attention at a fast pace; the hands-on raider sits home at
   season; the hands-on miner mines with no fuel check; the explorer buys
   thousands of probes; no bot builds the Fusion Reactor, Bomber, Destroyer,
   Battlecruiser or Deathstar.
7. For the owner: the jump law (every warship ends at 2 ticks, so a fast fleet is
   not something a player builds), and pirates from ring 9 out being one ship.
