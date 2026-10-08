# Playtest 2026-10-09: six AI players, one hour

Server at commit `5b7849d` (before the homeworld catalog). Harness: `tools/playtest/`.
Players: two Claude Sonnet 5.5 subagents, two `grok` CLI agents and two `pi` agents
on a local Qwen 3.8 27B (INT4). Each wrote the feedback file named after it.

## Final standings (about tick 3700)

| Player | Building levels | Research levels | Ships | Notes |
|---|---|---|---|---|
| Pi-1 | 14 | 4 | 18 (14 Corvette) | `salvage_and_sites` all game; no ships lost, ~25 NPC kills, repelled a raid |
| Grok-1 | 13 | 4 | 20 | lost main fleet at the outer-ring edge, rebuilt with Haulers |
| Grok-2 | 12 | 3 | 12 | lost 5 Scouts at distance 9; Hauler shuttles |
| Sonnet-1 | 13 | 6 | 6 Corvette | most research, small fleet |
| Pi-2 | 12 | 4 | 9 | fleet stranded without fuel one hop from home |
| Sonnet-2 | 12 | 6 | 4 | lost its 4-Scout fleet; never boarded a derelict |

## Findings

Reported by most or all players:

1. No costs, prerequisites or times in game, and `PrerequisitesNotMet` / `NoResources`
   errors without detail. Players guessed at the tech tree; two read server source.
   Fixed after the session by the homeworld catalog (`af8dfec` and earlier).
2. Every player receives every `CombatRound` / `FleetDestroyed` in the galaxy with no
   owner. Own losses get buried; players chased salvage from other people's fights.
3. Known sectors drop out of `full_state` about two minutes after a scan, so the map
   forgets explored space.
4. `mine_and_return` re-claims relative to the last claim, not home, and exceeded
   `max_range`; it walked fleets into the outer ring (two lost, one stranded).

Bugs:

- Starting fleets show fuel 50000/50000 until first dock, then the real tank (~120).
- A phantom "HOSTILE fleet id 0" (passive scout) appears on home and empty sectors and
  cannot be attacked.
- `FleetDestroyed` advertises salvage 200/50/30; the collectible pile is 60/15/9.
- `policy_update` with partial `params` fails as "missing field `action`" (the headless
  client's fallback parse error); all four params are mandatory and undocumented.
- A fleet out of fuel away from home is lost forever (no rescue, merge or scrap).
- Docking merges every fleet at home and there is no split.
- `prospect` ping-pongs between known sectors when nothing new is in range.

Balance:

- One building queue with 130-675 tick upgrades fits about four upgrades an hour;
  Frigate, Cruiser and Lab 3 were out of reach for everyone.
- Deuterium is the hidden bottleneck (0.17-0.6/tick).
- The outer ring is a cliff: one hop past distance 8 meets corvette packs that delete
  a scout fleet in one round.
- A large fleet mines a sector to None and it never regenerates; ore near home is gone
  by mid-game.
- NPC salvage outearns mines for an aggressive player; raids barely happened.

Harness (not the game): `play do` sleeps 2.5 s, `wait` caps at 90 s, `events` has no
"mine only" filter.
