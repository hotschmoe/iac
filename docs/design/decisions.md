# Design decisions

Owner decisions that set the game's identity. Newest first. Proposals that
conflict with these need the owner's sign-off.

## 2026-10-09 (single player and built-in bots)

Queued after the economy and exploration milestones land, in this order:

1. **Extract `iac-sim`.** A pure engine crate (state, `tick()`, command handling)
   with persistence behind a trait. The server keeps SQLite and WebSockets.
   Balance testing moves to running the real engine headless and fast, instead of
   a separate simulation that can drift from the code.
2. **Built-in bot players.**
   - **How they play:** scripted, server-side players that act only through the
     normal command path (no cheating), reusing standing orders plus build-order
     personalities (miner, raider, explorer, turtle).
   - **Role:** they make small worlds feel alive and give single-player opponents.
   - **Ranking:** bots are part of the world and are tagged BOT. They are kept out
     of the shared human and LLM ranking, or shown clearly separate.
   - **LLM agents:** these remain real outside players.
3. **Single-player browser build.**
   - **Bundle:** a build flag (e.g. `scripts/play.sh --single-player`) produces a
     static bundle: the Flutter client plus `iac-sim` compiled to WebAssembly, in
     one tab with no server.
   - **Wiring:** the client uses an in-process transport with the same JSON
     protocol, and IndexedDB for saves.
   - **Closed tab:** missed ticks are fast-forwarded on reopen.

## 2026-10-09 (economy spec and UI review)

Context: `economy/spec.md` and `ui-redesign/spec.md`.

1. **Pacing.**
   - **Persistent world:** pace 1 reaches an endgame around day 74 and keeps
     progressing for a year.
   - **Presets:** persistent 1, fortnight 5, season 10 (weekly), sprint 70,
     dev 100, blitz 600 (one-hour arc), test 8000.
   - **Fixed per world:** pace is set when a world is created and is not
     changed afterwards.
2. **Faster start:** Corvettes unlock at Shipyard 1, so a persistent-world player
   has a combat ship on day one.
3. **Tech tree:** a bigger tree is wanted, in a later milestone. The economy ships
   first.
4. **Smaller questions:** the economy spec's defaults apply to its smaller open
   questions (raid cadence, the 25% combat and exploration score cap, derelict
   finds scaling) until the owner says otherwise.
5. **Web UI direction.**
   - **Basis:** the Amber Glass prototypes are approved as the basis. Keep the
     palette and the animations.
   - **Tone:** tone down the cartoony graphics. Design it as the interface a
     1970s IBM engineer would want to play: an operator console with vector
     line-art, schematic symbols and labelled readouts.

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
