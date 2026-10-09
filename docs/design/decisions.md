# Design decisions

Owner decisions that set the game's identity. Newest first. Proposals that
conflict with these need the owner's sign-off.

## 2026-10-10 (play styles and balance)

**Two play styles, one world:**
- **Check-in players (OGame).** You should not need to be at the computer all day: check
  in, queue, and leave.
- **Hands-on players (The Infinite Black).** They can drive fleets, fight and explore
  themselves for fun and reward.
- **Hybrids.** Hybrid play should be natural.

**How to balance the two:**
- **Per day, not per hour.** Active players may earn more per hour of attention. A
  check-in player must not fall hopelessly behind over a week: tens of percent, not
  multiples. Field yield should have diminishing returns over a day.
- **Standing orders are the bridge.** Delegated exploring, salvage and mining should
  earn roughly 50-70% of hands-on piloting.
- **Pace matters.** Blitz is attention-heavy by nature: judge it on fun and use it as a
  stress test. Check-in balance is tuned for the persistent and season presets.
- **Balance is ongoing.** Get close, keep developing, and re-measure after significant
  changes. After the `iac-sim` split, scripted check-in, hybrid and active bots give a
  balance report per change.

## 2026-10-10 (the map pays; combat and exploration scoring)

Second playtest: staying home won; combat plus exploration never exceeded 4.8
points against cores near 400. Skilled explorers and raiders should be within
about 15% of a pure builder, and a mixed player should beat both.
- **Combat:** points follow the pile a kill leaves; full pay up to 4x the NPC
  group's power, then `4 / ratio` down to a 5% floor (the hard 8x zero is gone).
- **Anti-farming:** repeated kills in one sector pay less (0.75 per recent kill,
  heat halves every 4 respawn delays), for loot and points alike.
- **Exploration:** charts, first boardings, derelicts and relics pay points
  that scale with the finds factor (see `economy/spec.md` section 18).
- **Cap:** stays at 25% of core. The simulation shows it is enough: it binds for
  an explorer at season pace and not at blitz, and a higher cap would let an
  explorer overtake the builder with a lower core.
- **Cargo:** ship holds scale by P^0.5 like loot, so finds can be lifted and
  Haulers matter to a raider.
- **Raids:** a lost raid also destroys a quarter of the ships docked at home;
  missed rolls raise the next roll's odds.

## 2026-10-10 (one queue rule)

**Every order that can ever start is accepted; it pays when it starts.**
Supersedes the 2026-10-09 queue-payment rule below, which refused an order that
had a free slot but no money while letting the same order wait behind a running
item.
- **Accept:** buildings, research, ships and defences are accepted whenever
  they fit the queue, slot free or not, affordable or not.
- **Wait:** a waiting order costs nothing. It starts, and pays in full, the
  moment a slot is free, what it needs is built and the stockpile covers it.
- **Order:** strictly first in, first out per queue. An order short of resources
  holds back the ones behind it, so the line never reorders and the start
  estimate is honest. An order that waits only on a prerequisite does not hold
  the line.
- **Depth:** running items plus 3 waiting (`slots + 3` for buildings, 4 for
  research and shipyard), so extra build slots never remove the ability to
  queue.
- **Refuse only the impossible:** locked prerequisites, max level, a payment
  larger than any storage cap, or a full queue, each with a specific error.
- **Show:** each waiting order carries its exact shortfall, why it waits and an
  estimated start. Starting, waiting and cancelling are all confirmed by a
  `Queue` event; cancelling a running item refunds half, a waiting one nothing.
- **Why:** this suits slow persistent worlds, where players queue and leave
  (OGame-style), and it is one rule instead of a rule that depends on whether a
  slot happened to be free.

## 2026-10-09 (queue payment, superseded)

**Queued items pay when they start, not when they are queued.** The rest of the
old rule (refuse an unaffordable order that would start now) is replaced by the
2026-10-10 entry above.

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
