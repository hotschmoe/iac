# Design decisions

Owner decisions that set the game's identity. Newest first. Proposals that
conflict with these need the owner's sign-off.

## 2026-10-11 (a fleet carries no more than the ships it has left can hold)

**Decided by the agent that built it, not by the owner, and open to change.**
The hold entry below left this as found; since `load` a player could arrange
it at home.

- **One rule, wherever ships are lost.** After a fleet loses ships it keeps
  what the holds of the ships left carry, and no more. What no longer fits
  leaves the fleet, each resource in proportion to what was aboard. A fleet
  with no ships left sheds its whole cargo. Ships are lost in three places:
  a fight, a recall that breaks ships, and a lost raid at the dock.
- **In the field it becomes wreckage in that sector.** It joins the pile a
  kill leaves there, lasts as long (180 ticks from the last addition) and is
  taken with `collect_salvage` by whoever comes, the owner included. So the
  cargo of a destroyed fleet is no longer simply gone: it lies where the
  fleet fell. Before, a destroyed fleet's cargo vanished and a mauled one
  kept all of it.
- **A recall settles where the fleet jumped from.** Ships that break up in
  the emergency jump leave their share in the sector the fleet left, not at
  home.
- **At the home dock it goes to the stockpile, up to the Vault cap; the rest
  is lost.** A raid that destroys docked ships settles the holds before it
  takes its slice, so what moved to the stockpile is raided with it.
- **Nothing is made.** The pile, the stockpile and the hold together never
  hold more than the hold did, and a kill's score still follows only what
  the hostile group was worth. Losing ships on purpose to move cargo costs
  the ships and gains nothing a hold or the stockpile would not have kept.
- **The owner is told** by a warning `Alert` naming the amount and where it
  went. No new event kind.
- **Fuel was already so.** A tank is cut to what the ships left carry in all
  three places, and the excess is lost; that stands.
- **Not changed:** a fleet already over its hold in a saved world stays so
  until it next loses a ship. Split and merge share cargo by hold and cannot
  put a fleet over.

## 2026-10-11 (a hold is loaded from the stockpile at home)

Owner: the hub's exchange is a surplus sink, so a player whose stockpile is
full of one resource must be able to carry the surplus to the hub. The owner
set the frame: a `load` command for a fleet docked at home, limited by the
stockpile and by free cargo room; an order that cannot be met in full is
refused with what is short; loaded cargo stays aboard at the dock until the
fleet has been away; a way to put it back by hand; the state survives a
restart and a save; a laden fleet is otherwise an ordinary fleet; no rule is
dodged by it; the bots do not use it.

**Everything below was decided by the agent that built it, not by the owner,
and is open to change.**

- **One resource an order, not an amount of each.** `load` takes `fleet_id`,
  `resource` and an optional `amount`; three resources are three orders. The
  owner's frame said "an amount of each resource". "All that fits" has one
  meaning for one resource and none for three, the surplus is nearly always
  one resource, a refusal can then name exactly what is short, and the
  exchange already speaks in one resource at a time. Cost: a mixed load is
  not atomic.
- **"All that fits" is the same command with no amount**, as `exchange`
  reads a missing `amount`. It is refused only when nothing fits: the
  stockpile holds none or the hold is full.
- **`unload` is its own command**, not `load` with zero. It ends the state
  and returns the hold as far as the caps allow; the dock takes the rest as
  room opens. Refused for a fleet that was not loaded by hand.
- **The state is a mark on the fleet (`laden`) that ends with the fleet's
  next jump.** While it is set the dock leaves the hold alone; a fleet that
  has jumped away comes home unmarked and unloads as always. Recall from
  home and a cancelled jump do not end it. It is a column of the fleet's
  row in SQLite and a field of the fleet in the snapshot, so `SAVE_FORMAT`
  went from 2 to 3; `ECONOMY_VERSION` stands (no cost, time, production
  formula or payment moment changed, and the server adds the column to an
  existing database).
- **Standing orders and a hand-loaded hold do not mix.** `load` is refused
  for a fleet under standing orders; giving a laden fleet standing orders
  ends the mark and unloads it. A doctrine waits at the dock for a hold it
  cannot empty, so the two together would hold the fleet for ever.
- **Split and merge.** Both halves of a split laden fleet stay laden. A
  merged fleet keeps the state of the fleet that absorbs, as it keeps that
  fleet's standing orders.
- **A lost raid counts cargo aboard fleets at home with the stockpile.** One
  pile, one protected floor, the same 8/8/5 percent of what is exposed,
  taken from stockpile and holds in proportion. Before, cargo at the dock
  was out of a raid's reach, which did not matter while only a full
  stockpile left any there. This changes the rule for every fleet with
  cargo at home, loaded by hand or not. Loading therefore changes nothing a
  raid takes. Ships destroyed by the raid took nothing from the hold they
  carried when this was written; since the entry above their share goes to
  the stockpile before the raid takes its slice.
- **Holds as storage are allowed and not limited.** A fleet at home holds
  its capacity on top of the Vault. At pace 1 a Hauler holds 200 for 950
  resources of hull, 4.75 a unit stored; Vault level 3 adds 12,375 units
  for 867 (0.07 a unit), and level 20 still costs 0.6 a unit. The most
  a player can hold aboard is 8 fleets of 64 Haulers, 102,400 at pace 1:
  9.3 Vault-0 stockpiles, 1.2 at Vault 5, 0.16 at Vault 10 (at season pace
  5.2, 0.7 and 0.09; at blitz 14.6, 1.9 and 0.25, because holds grow with
  the square root of the pace and caps slower beyond a season). So it
  matters most to a young empire with an unusually large fleet.
- **Score.** Loading and unloading move none: score counts buildings,
  research, ships and defences, never stock or cargo.
- **The answer to `load` and `unload` is an info `Alert`**, as for `split`
  and `merge`, and the fleet row (`cargo`, `laden`) in the next update. No
  new event kind and no new error code.
- **Left as found here, settled in the entry above:** a fleet that lost
  ships (in a fight, to a recall, to a lost raid) kept its whole cargo even
  beyond the hold that was left. With `load` a player could put that case
  together at home.

## 2026-10-11 (ship batches pay when they are ordered)

Owner, after playing single player. Amends "ships pay per unit" in the
mechanics pass below; the queue rule for buildings and research stands.

- **A ship or defence batch is accepted only if the stockpile covers the whole
  batch now**, and it pays then. Otherwise it is refused, with the exact
  shortfall, until the resources are there. A paid batch waits only for the
  yard or for something queued ahead of it to finish.
- **Buildings and research keep queueing ahead of resources.** They are
  accepted affordable or not and pay when they start. That is how time away
  turns income into progress for a check-in player, and for a single-player
  world while its tab is closed.
- **Not decided by the owner (implemented as the simplest rule, open to
  change):** cancelling a batch refunds half of what it paid for units not yet
  delivered, waiting or running, so the yard's line cannot be used to keep
  resources out of the store.

## 2026-10-10 (the hub, PvP by zone, outposts, more worlds)

Context: `starting-positions-2026-10-10.md`. Numbers proposed there (fees,
intervals, bandwidth, level caps) are not decided by this entry.

- **PvP by zone.** No PvP in the Inner Ring (rings 1-8). PvP exists only from
  ring 9 outward. Multiplayer PvE is allowed everywhere. This settles "opt-in or
  forced" from 2026-10-09 item 4: crossing the line is the opt-in.
- **Homeworlds never move.** A player reaches better resources with an
  **outpost**, which can only be placed in PvP space: anywhere from ring 9, with
  the richest sites further out.
- **A neutral post at the centre.** The hub starts as one hex doing two jobs, an
  NPC exchange and the gate between worlds. It may grow into the seven-hex
  cluster (the centre and ring 1) with a purpose per hex as chat, items and an
  auction arrive.
- **The exchange is a surplus sink, never a faucet.** The rate worsens as a
  player trades and recovers over time, tracked per player. No hard daily cap. A
  world-wide slide is a possible later step.
- **More room means more worlds, not a third axis.** Other worlds are reached
  through the hub, and each is a flat (q,r) grid, so a player always plays on a
  (q,r) map "via world x". Worlds may open as players join; the rule that opens
  one is not decided.
- **Black market.** It sits in dangerous space, not at the hub, and moves at
  intervals: its place comes from the world seed and a time window, and a
  remembered pin goes stale. It trades resources for now and is replaced by an
  item market when an item system exists.
- **Gateway.** A research line lets fleets jump between home and the outpost.
  No jump while in combat. It carries loot, limited by a bandwidth that further
  research raises, so bulk still travels the rings.
- **Homes sit in rings 3-5, in fixed seats.** A world has a set number of seats
  (about 24) and no spill rings: a full world refuses with a clear "world full"
  and a new world is the answer. No hostile pack on or beside a home.
- **The Inner Ring is scout country.** Scout-class packs out to ring 8, so the
  jump in danger sits on the PvP line at ring 9 instead of at ring 6, where it
  killed every starter fleet.
- **Lanes are seeded per world.** Each world has its own maze. Existing
  databases are disposable.
- **Gates are one-way at first.** A fleet may enter a world as old as or older
  than its own, never a younger one, so veterans cannot descend on a new
  cohort. A gate may open both ways once the younger world has matured; the
  rule for that is not decided.
- **Home ground.** Derelicts beside a home stay; boarding one is a choice.
  `patrol_home` hunts the convoys beside home; what that pays is re-measured
  with the bots before it is tuned.
- **Threat labels tell the truth about the matchup.** A label accounts for ship
  classes (rapid fire), not power alone, so a doctrine's entry rule cannot walk
  scouts into corvettes at the ring-9 line.
- **Not decided:** whether research extends how far out an outpost may sit
  (ring 21+), the purpose of each hub hex, and how fleets, cargo and score cross
  between worlds.

## 2026-10-10 (mechanics pass: free slots start what can pay)

Amends the one-queue rule below on the line it drew: the queue is no longer
strictly first in, first out.
- **Order:** a free slot starts the earliest waiting order that can start now
  (prerequisites built, stockpile covers it). An order that cannot start keeps
  its place and is re-checked every tick. Playtest 2026-10-10b: strict FIFO left
  the second and third building slots idle behind one order that could not pay.
- **Starvation guard: `reserve`, a per-order flag.** An order sent with
  `reserve: true` holds the line while it is ready and short of resources:
  nothing behind it in that queue starts (orders ahead still do). Chosen over
  automatically reserving "nearly affordable" resources because it is explicit,
  needs no hidden accounting that an agent cannot see, keeps the start estimate
  honest, and leaves the default behaviour (use every slot) to the player who
  did not ask for anything else. The cost is that cheap orders can delay an
  unflagged expensive one; the flag is how a player says "save for this".
- **Stable ids:** every order gets an id, kept from the waiting line into the
  running slot and carried by `Queue` events. `cancel_build` and `cancel_queued`
  take the id; an id that is not in the named queue and state is refused with
  a message that says where it is (cancel by position raced the tick).
- **Ships pay per unit:** a batch is one order and each unit pays when it
  starts. A batch that cannot pay for the next unit goes back to waiting with
  its progress (`built` of `count`) and does not hold the yard. Cancelling a
  running batch refunds half of the unit under way.
- **Consistency:** research, shipyard and defence orders accept a queued Lab or
  Shipyard as a prerequisite, as buildings already did, and a running tech can
  be queued for its next level.
- **Doctrines honour `engage_ratio_x10`:** the entry rule is exactly the
  player's ratio, hostiles there or not. The old max(engage, 2.5x) override
  and the EVEN floor are gone, so a deliberately low setting works. The default
  is 15 (1.5x, the FAVOURABLE band: a fight keeps 85-95% of hull), so default
  doctrines are careful and players who want to gamble lower it. A standing hold is
  reported at most once a minute while its cause is unchanged.
- **Pins and salvage:** a remembered pile past its despawn tick leaves the
  chart; other remembered pins out of sight carry `pins_stale`. A fleet docked
  at home scoops a home salvage pile into storage by itself (it is the
  player's own planet, and the drop comes from a defended raid), with an alert
  at 60 s when it cannot.

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
The strict first-in-first-out line is amended by the mechanics pass above.
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
