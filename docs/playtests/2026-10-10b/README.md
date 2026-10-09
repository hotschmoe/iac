# Playtest 2026-10-10 (b): assigned strategies, one hour, blitz

Server at commit `6ab6337`: the rules pass (one queue rule, combat falloff,
anti-farming, exploration points, cargo scaled by P^0.5, raid pity and docked-ship
loss), autopilot safety, the five ratio labels, and the fast agent CLI with the
harness watchdog.

- **World:** `IAC_PACE=blitz`.
- **Players:** four Claude Sonnet 5.5 subagents with assigned strategies (Builder,
  Mixed, Explorer, Raider) and two `grok` CLI agents with free play.
- **Harness:** both Grok agents stayed up for the full hour under the new watchdog
  (`harness-events.log`).

## Final leaderboard

| Rank | Player | Strategy | Score | Core | Combat | Explore |
|---|---|---|---|---|---|---|
| 1 | Explorer | explorer | 491.5 | 442.8 | 13.5 | 35.2 |
| 2 | Raider | raider | 442.3 | 399.9 | 15.1 | 27.3 |
| 3 | Grok-2 | free (derelicts, haulers) | 409.3 | 333.8 | 5.1 | 70.4 |
| 4 | Mixed | mixed | 405.7 | 384.1 | 14.2 | 7.3 |
| 5 | Grok-1 | free (economy) | 339.7 | 279.4 | 6.9 | 53.4 |
| 6 | Builder | pure builder | 263.0 | 263.0 | 0 | 0 |

## What it shows

**Leaving home now pays something.** Every player who went into the field beat the
pure builder. In the previous run, staying home won outright.

**Read the scoreboard carefully, though.**
- **Explorer and Raider won on their economy.** Both hit the 25% bonus cap within
  minutes, then played the economy (cores of 443 and 400).
- **The Builder lost mostly to weak play.** It spent metal on Corvettes instead of
  mines.
- **So field play is a worthwhile early bonus and a resource source, not yet a main
  strategy.**
  - At blitz, raiding is "side income of a few percent": a fresh kill pays 250–500
    against about 75 metal per second, and big fleets burn about 180 fuel per jump.
  - Derelicts are the exception. One quiet tier-1 site paid about 5,000 metal and
    funded Grok-2's tech.

**Owner direction:** balance for "check-in" players against hands-on pilots per day,
not per hour, with standing orders as the bridge (`decisions.md`). Blitz is an
attention-heavy mode, so judge it on fun and use it as a stress test, not as the
check-in balance target.

## Findings

Mechanics, reported by most players:

1. **Head-of-line blocking.** Strict FIFO freezes every free build slot behind one
   order that can't pay, so the second and third slots often sat idle. Players
   cancelled and requeued by hand to work around it. They want a free slot to start
   the next order that can pay, with the blocked order keeping its place.
2. **Cancel by index races the tick.** Indices shift when an item starts, so batches of
   `cancel_queued index 0` removed the wrong items, once in a different queue than
   asked. Queue items need stable ids.
3. **Research and buildings disagree.** Buildings queue behind a prerequisite that is
   itself only queued; research refuses (`PrerequisitesNotMet`, `MaxLevelReached`
   for a tech that just started).
4. **Ship batches wait for the whole batch.** A batch of 2 waits until both can be paid,
   idling the yard. Pay per unit.
5. **Prospect still idles.**
   - With hostiles present the doctrine requires max(engage, 2.5x), overriding a lower
     `engage_ratio`. Grok-2's scouts sat home for half the game on "1.36x EVEN, needs
     1.5x".
   - The hold reason repeats every tick.
6. **Stale map pins.**
   - The map keeps "derelict t1 quiet" and "salvage" on stale sectors after the site
     or pile is gone, so players flew to empty pins.
   - Despawn countdowns are not shown on `map`, `sector` or raid events.
   - Raid salvage on a player's own homeworld despawned unnoticed.
   - Sensor contacts that name the scanner's own sector look like targets.
7. **Waits return too early.** `wait --until idle` keeps returning for items the player
   deliberately isn't building (a payable corvette or turret), so the loop polls.
8. **CLI acknowledgements.**
   - "accepted (already finished or not visible yet)" hid a full queue.
   - A research error was attributed to a build order.
   - A cancel summary named the wrong queue.
   - Players learned to trust the `Queue` event over the summary line.

Balance, for the ongoing retune:

- The 25% cap binds in minutes at blitz; after that, field play only adds resources.
- Cruisers arrive with under 7 minutes left at blitz (Shipyard 6, an 8000/5000/2500
  tech, then 104 s of research), so frigates carry the game.
- Late-game bottlenecks shift between metal, crystal and deuterium. Mine payback is
  sharp and well shown by `costs`.
- Raids felt fair: they were hit-or-miss, home defence held them easily, and nothing
  was lost.
