# Design decisions

Owner decisions that set the game's identity. Newest first. Proposals that
conflict with these need the owner's sign-off.

## 2026-10-09

Context: `gap-analysis-2026-10-09.md` and the 2026-10-09 six-agent playtest.

1. **Pacing is a server setting.** One world-speed flag scales the game from a
   persistent world (months or more) through faster seasonal worlds (e.g. a weekly
   reset) and a blitz, up to a very fast setting for testing (~100x). The flag
   scales economy timers (build, research, shipyard, production, regeneration,
   respawns); real-time piloting cooldowns stay human-scale.
2. **Economy.**
   - **Buildings:** they are the primary resource source.
   - **Mining:** finding and mining ore is a bonus that grows with distance from
     the inner circle.
   - **Additions:** defences, storage caps, a second building slot (unlocked by
     research), and ore that regenerates.
3. **Exploration pays.** Loot, richer ore and things to discover increase further
   from the inner circle, matched by visible, rising danger.
4. **Multiplayer.**
   - **Trading:** requires a way to communicate (chat).
   - **PvE team-ups.**
   - **PvP in the outer Wandering:** opt-in or forced is still to be decided.
5. **One ranking for AI agents and humans.** Agents are players.
6. **Web UI is the main human interface.**
   - **Redesign:** give it a full redesign to be a fun, good-looking, interactive
     game interface, especially the windshield and the map, rather than a port of
     the TUI.
   - **TUI:** it stays as a secondary client.
   - **LLM agents:** they keep playing through the CLI.
