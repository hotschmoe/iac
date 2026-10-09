# Playtest 2026-10-10: six AI players, one hour, blitz pace

Server at commit `c0b42fb`: the economy rework (pace, storage, slots, defences,
raids, score), exploration (threat T1-T9, ore, loot) and chart memory.

- **World:** `IAC_PACE=blitz` (x600).
- **Players:**
  - two Claude Sonnet 5.5 subagents: Sonnet-1 a balanced builder, Sonnet-2 told to
    explore and raid;
  - two `grok` CLI agents;
  - two Claude Haiku 5.5 subagents, standing in for the pi/Qwen agents (the local
    model server was down). Haiku-2 was told to lean on standing orders.
- **Grok outage:** both Grok supervisors died about 7 minutes in, cause not
  established, and were relaunched about 14 minutes later. That idle start
  explains most of their gap.

## Final leaderboard

| Rank | Player | Score | Core | Combat | Explore |
|---|---|---|---|---|---|
| 1 | Sonnet-2 | 399.4 | 398.9 | 0.2 | 0.3 |
| 2 | Sonnet-1 | 375.0 | 375.0 | 0.0 | 0.0 |
| 3 | Haiku-1 | 357.3 | 355.4 | 0.8 | 1.2 |
| 4 | Haiku-2 | 315.5 | 312.9 | 1.8 | 0.7 |
| 5 | Grok-1 | 134.0 | 129.0 | 4.8 | 0.3 |
| 6 | Grok-2 | 78.9 | 77.0 | 0.0 | 1.9 |

## Compared with 2026-10-09

**Fixed:**
- **Errors:** they now name exactly what is missing ("short 73 metal, 25 crystal",
  "needs Research Lab level 4 (you have 2)"). Nobody guessed the tech tree or read
  source code.
- **Logistics losses:** no fleet was stranded or lost to autopilot drift.
- **Map memory:** the map kept explored sectors.
- **Late game:** every player reached Frigates, and several reached Cruisers or
  Cruiser Tech. On 2026-10-09 nobody got past Corvettes.
- **Economy arc:** three build slots, the Fabricator and research all mattered, and
  players described the economy as readable and fair.

**New headline problem: the map does not matter.**
- The winners scored essentially only core. Combat plus exploration never exceeded
  4.8 points against a 25% cap.
- Kills by a fleet more than 8x stronger score nothing. A boarded derelict was worth
  about 0.04 to 0.12 points, and a tier-1 boarding paid 0 resources.
- Salvage piles worth thousands at blitz pace could not be lifted: cargo is 10 to
  40 per ship and does not scale with pace, while finds do.
- Every player concluded that staying home wins.

## Findings

Reported by several players:

1. **Inconsistent queueing.** An order with a free slot but short resources is
   refused, while the same order behind a running item waits. Players want one rule:
   it queues and waits, showing the shortfall and when it can start. Queue depth
   counts the running items, so with three slots nothing can ever wait.
2. **Exploration and combat are worth almost nothing** (see above). The 8x rule
   zeroes most kills, and repeated kills of the same respawning patrol were the only
   combat points anyone earned.
3. **Cargo versus finds.** Loot scales with pace by P^0.5 and cargo does not, so most
   salvage despawns unlifted.
4. **Prospect.**
   - It walked a scout into a DEADLY sector (Haiku-1).
   - It holds "nothing uncharted and safe" too early (Sonnet-1, Grok-2).
   - Fuel warnings quote the charted safe route ("home is 17 hops") when home is 2
     hexes away directly.
5. **Threat labels.**
   - DEADLY at a ratio of 1.1 oversells an even fight.
   - Map odds use the strongest fleet rather than the selected one.
6. **Raids barely happened.** Several players never saw one in an hour while their
   estimate climbed. Turtling with docked ships made raids harmless anyway.
7. **Blitz details.**
   - The Storage Vault is irrelevant (caps of about 600k).
   - Mine levels past about 12 stop paying back within the hour.
   - Crystal or deuterium piled up unused for some players.
8. **The play tool is too slow for blitz.**
   - `do` blocks for about 2.5 s while builds finish in 2 to 5 s.
   - Wanted: batch orders, a one-shot JSON `state`, `wait` that returns when a queue
     empties, collapsed ScanCompleted events, a status line with score and the
     scoring rule, and waits that stay under the agent's tool timeout.

Possible bugs:

- The same template NPC id was destroyed repeatedly in one sector, with salvage
  stacking about 5% larger per kill. Respawn reuses the template id by design, but
  this makes a single farmable tap.
- Status showed "Shipyard queue: idle" next to a waiting batch.
- Cancelling replied "(no new events)" with no confirmation.
