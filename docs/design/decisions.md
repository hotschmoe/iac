# Design decisions

Owner decisions that set the game's identity. Newest first. Proposals that
conflict with these need the owner's sign-off.

## 2026-10-12 (seven loose ends: the Vault and a batch, a fight's forfeit, Stop, and where status text stands)

**Decided by the agent that built it, not by the owner, and open to change.**
Seven things earlier passes saw and left. Each is its own commit.

- **A ship or defence batch asks the Storage Vault before it asks the
  stockpile.** A batch that costs more than a stockpile cap is refused with
  `StorageTooSmall` and the sentence a building or a tech gets, even when the
  stockpile holds enough. The stockpile can hold more than its cap only
  through refunds, so this closes one door: money past the cap cannot buy
  what the Vault could never hold. It still buys anything that fits.
- **Refunds are as they were, and now written down.** A cancel returns half
  of what the order has paid, and the refund is added past the cap. That was
  reported earlier and judged harmless, because cancelling costs half; it is
  in `RULES.md` and the README under Queues and slots.
- **A fight that catches a fleet charging a jump says what fuel is forfeit**,
  as a warning `Alert`, in the terms `stop` uses: the fleet, the sector, the
  jump's target, the fuel. It is raised where the jump is abandoned
  (`engage_fleet`), so the fleet that was attacked hears it just before the
  fight is announced and a fleet drawn into another's fight just after. The
  fuel named is what a jump costs the fleet at that moment, as for `stop`.
  No event kind, no state.
- **The web Stop works for as long as the engine takes a stop.** While the
  ships fly out and the jump is not yet answered, the Stop tile and `X` send
  the order; every other act still waits for the jump. The scene gives the
  jump up at once and does not wait for the engine: if the stop was too late
  the fleet arrives and the arrival is played, and if it was in time the
  ships are already home. A jump that is never answered ends the same way
  after 2.6 seconds; it used to leave the ships off the arena.
- **A status that comes and goes has room kept for it; it is not moved under
  the controls.** The orders panel's newest report keeps five lines and the
  fleet status keeps four under its Orders row while the fleet is under
  orders. Under the form the report would be still, and off a 900 px screen:
  why a fleet holds is the first thing that panel is for. The cost is blank
  lines when the sentence is short. Two things still move: a sentence longer
  than the room grows its box, and the list of earlier reports at the foot
  of the open orders panel pushes the panels below it as it fills.
- **On the windshield the toasts stand at the top at every width**, over the
  sector head, which has no controls. They did so below 1167 px already. At
  the bottom right they lay over the foot of the column that holds every
  panel. The other screens keep the bottom right on a desktop. A panel that
  opens on a desktop no longer takes the earlier toasts down, because none
  is in its way; the phone layout's sheet still does. One toast of the
  longest sentence the engine has clears the gate cards and the fleets. A
  stack of three reaches the corner of the north-west gate card, and a
  taller one the fleet rail, as on narrower windows. The bottom left, over
  the feed, has a little more room and was not taken: a toast as wide as
  they are would stand on the dock's first tile once the dock has nine.
- **Resizing the window keeps what is open.** The windshield's panels move
  between the desktop column and the phone sheet with their state, and the
  shell's screens keep theirs when the outpost's strip and the bottom rail
  come or go in one step.
- **A new outpost says nothing about its empty store to the fleets that
  founded it.** The fleets standing there when level 1 opens count as told.
  A fleet that arrives later is warned, and so is one that fills up and
  thirsts there again. The other way, waiting until the store has ever held
  deuterium, needs a field in the outpost, the snapshot and the save for one
  message. The sentence still says "no deuterium left" to a second fleet
  that reaches an outpost nobody has stocked yet.

## 2026-10-11 (outposts: what the engine tells a client beforehand)

**Decided by the agent that built it, not by the owner, and open to change.**
The clients of slice 1 worked five things out for themselves that the engine
knew. Each is now the engine's to say, in `shared/src/protocol.rs`, and
neither client judges it.

- **Why an upgrade would be refused is in the state**
  (`OutpostState.upgrade_refusal`), in the sentence the refusal carries, as
  `OutpostSurvey.refusal` is for founding. Upgrade is off while it is
  present, with the sentence under it. This replaces "Upgrade is never
  switched off by the console" in the entry below. For it the shortfall
  sentence lost "Nothing was started", which was only true after an order.
- **A raid is a range, not an estimate.** `raid_power` in the survey and in
  the outpost's state is `least` and `most`, the bounds of the roll
  (0.80 and 1.15 times twice the ring's group power); the one middle figure
  (`next_raid_estimate_power`, and the survey's old `raid_power`) is gone
  for outposts. The homeworld keeps its estimate: its raid is sized by the
  empire's worth, which moves, and nothing asked for it. The clients print
  the two ends and say whether the defence is above the strongest, below
  the weakest or between; none prints a chance, because the outcome also
  moves the defence a tenth either way.
- **A full store and a waiting repair are state, not event kinds.**
  `yield_lost` lists the resources the extractor makes and the store is
  full of; `repair_waiting_for` is what the store lacks of the repair. The
  state says both fully, every tick, so a client that connects late knows
  them too, which an event would not tell it. The alerts stay as they were,
  for the log. A notice is raised when a resource enters the list; the
  engine's alert waits until the store has dropped below its refill mark
  before it warns again, so after a small load the notice can come once
  more than the alert.
- **A sector an outpost stands in is rated 0**, power 0, `observed`. It was
  T1 with power 0, which read as a weak group. 0 means nothing to fight and
  nothing coming; the scale for groups is still 1 to 9 and
  `scaling::threat_rating` is untouched. Only that one case is rated 0: the
  hub and home ground, where no group goes either, still carry their ring's
  estimate, and changing them would change what bots and standing orders
  read of those sectors.
- **A fleet says it is at its owner's outpost** (`FleetState.at_outpost`),
  finished, building or breached. `outpost_fuel` keeps its meaning (the way
  there, while the outpost fills tanks) and is no longer read as "0 means
  at".
- **A survey of a sector never seen is a reply, not an error.** It carries
  what the ring decides (the cost, the time, the raids, the platform) and a
  `refusal`; the site (`site`: ore, grade, yield, hops home) is absent, and
  the refusal is worked out without looking at the sector, so neither its
  ore nor an Anomaly or a derelict there is told. Its ring is no secret, so
  a sector outside rings 9 to 20 is refused for its ring. The inspector and
  the log print it as any survey.

Two more, from watching an outpost in a real browser:

- **A panel that opens on the windshield takes down the toasts that ask
  nothing.** On a phone they stand where the sheet opens and covered its top
  rows for nine seconds. Dismissed, not pushed aside: there is no free place
  on a phone to push them to, what they said is in the log, and a toast with
  buttons stays. One raised after the panel opened still shows over it.
- **The browser drive of an outpost's life is not a kept tool.** It takes
  eleven minutes and its raid is a roll. `tool/solo_e2e.dart` stays the one
  browser check, and it still founds nothing.

## 2026-10-11 (outposts, slice 1: what the clients show)

**Decided by the agent that built it, not by the owner, and open to change.**
Where the web console and the TUI depart from section 7.6 of
`docs/design/outposts.md`, or settle what it left open.

- **`O` opens the founding panel anywhere, not only in rings 9 to 20.** The
  dock's Found tile stands in those rings only, as 7.6 has it; the key works
  wherever a fleet is, because the panel is how a player learns the rule: at
  home it shows the server's sentence about the rings. It is also what the
  browser check can reach.
- **Upgrade is never switched off by the console.** (Replaced: the state
  now carries the sentence; see the entry above.) The server has no
  sentence for an upgrade it would refuse until the upgrade is ordered
  (`outpost_survey` carries one for founding; `OutpostState` carries none
  for the next level). The panel prints the shortfall as a figure and the
  job under way, leaves the button pressable, and the server's refusal comes
  back as a toast in its own words. A console that worded the refusal itself
  would be a second judge.
- **The outpost has a screen of its own**, on the rail while an outpost
  stands (`G` then `P`; typed `5`, `op` or `outpost`), beside the panel the
  windshield opens. 7.6 says "also in the rail"; a tile that only jumped to
  the windshield would have needed a fleet to be selected.
- **The HUD line is in the bar from 1100 px and a strip under it below
  that.** The bar of a phone has no room for a fourth figure.
- **The percentage in the HUD line is the fullest resource of the store**,
  not the store as a whole: one resource at its cap is what stops the
  extractor.
- **The outpost's dock is the homeworld's hold panel**, given the outpost's
  store to draw on, not a second panel. On the windshield it stands under
  the outpost panel for the fleets standing there; `L` there opens it.
- **A sector with an outpost shows the station in place of the threat
  digit** on the map. The server rates such a sector as empty.
- **The map inspector prints the survey as three lines and the verdict**, as
  7.6 has it; the founding panel prints it as rows with the cost set against
  the hold. The inspector column is now bounded and scrolls, so a long reply
  cannot run under the route panel.
- **"Raid repelled" at the outpost is a background notice too.** 7.6 lists a
  raid, a breach, a finished level and a full store; the owner's brief added
  a repair that waits. The outcome of an announced raid is told either way.
- **TUI keys.** `o`, `F`, `U`, `A` then `y` were all free and are as 7.6
  names them. `U`, `A` and Enter act inside the `o` overlay only, so that no
  stray key upgrades or abandons; `F` works from any view and opens the
  overlay on the survey.

## 2026-10-11 (outposts, slice 1: built on the design's recommendations, pending the owner's answers)

**Not decided by the owner.** `docs/design/outposts.md` ends with six
questions only the owner can answer. Slice 1 of the engine was built before
they were answered, on the design's recommended answer to each. Each is
listed here with the code it lives in, so that a different answer is a
change in a known place and not a search.

1. **An outpost produces.** Its extractor makes
   `2 x density x ring factor x level` an hour per resource. *Where:*
   `OUTPOST_YIELD_PER_HOUR` in `shared/src/constants.rs`,
   `scaling::outpost_yield_per_tick`, `GameEngine::run_extractor` in
   `sim/src/engine.rs`. *If no:* remove the extractor step from
   `process_outposts` and `production`, `grade` from the survey and the state.
2. **It is carried in holds, with `load`; no ship class was added.**
   *Where:* `GameEngine::outpost_site` (the hold must carry the cost) and
   `handle_found_outpost` (the hold pays). *If a ship:* a sixth `ShipClass`
   and the founding rule in those two functions.
3. **No NPC raid destroys an outpost.** A lost raid breaches it and it
   repairs itself from its store. *Where:* `GameEngine::resolve_outpost_raid`
   (sets `breached`, never removes the entry) and `repair_outpost`. *If yes:*
   the lost branch of `resolve_outpost_raid`, a longer grace in
   `RaidTimers`, and a line in `solo/src/away.rs`.
4. **One outpost each, in rings 9 to 20.** *Where:* `OUTPOST_MIN_RING`,
   `OUTPOST_MAX_RING`; one each is the key of `GameEngine::outposts`
   (`BTreeMap<owner, Outpost>`) and the rule in `outpost_site`. *If more than
   one:* the map's key, and every `outposts.get(&player_id)` in the engine,
   `views.rs`, `intel.rs`, `score.rs` and `solo/src/away.rs`.
5. **`recall` is left as it is.** It still jumps a fleet to the homeworld
   with its whole hold from anywhere, the outpost included, so a convoy can
   load at the outpost, and recall home with everything. *Where:*
   `GameEngine::handle_recall`, untouched. The design proposes to ration it
   with the gateway in slice 2.
6. **A fleet standing at its owner's finished outpost does not count against
   the three deployed fleets**, and needs a free slot to leave. *Where:*
   `GameEngine::count_deployed_fleets` and `docked_at_outpost`, read by
   `handle_move` and `handle_split`. *If it should count:* drop
   `docked_at_outpost` from those three.

Every `OUTPOST_*` number is the design's proposal and provisional in the way
the exchange's four are: `shared/src/constants.rs` and
`docs/design/economy/sim.py`, kept equal by `tools/check_sim_constants.py`.

**Decided by the agent that built it, where the design was silent or the
code had moved, and open to change.**

- **A sector with a group in it cannot be founded on.** The design says the
  founding fleet stands idle "so no fight is going on", which a passive pack
  does not prevent. `outpost_site` refuses while a group stands in the
  sector or its lair has not been cleared: the site is taken from its pack,
  passive or not.
- **The founding rules are checked in an order that makes a survey from
  afar useful.** Ring, Anomaly, derelict and spacing first, then "you
  already have one", then where the fleet is, then what the fleet is doing,
  the group, standing orders and the hold. A survey of a distant sector that
  is refused only because the fleet is elsewhere names a site that
  qualifies.
- **The protected floor is the cost of the next repair**, which is the
  repair of the level being built while a level is being built. The design
  says a raid never takes the materials the repair needs; with the floor at
  the standing level a breach suffered during an upgrade would have left the
  store short of the dearer repair that follows it.
- **A breach during a build or a repair waits its turn.** One job at a time,
  as the design has it: the build goes on, and the repair starts when it is
  done. A raid lost during a repair leaves the repair running, and the
  outpost is whole when it ends.
- **A breached outpost that cannot pay its repair stays breached.** It
  extracts nothing while breached, so it cannot save up the repair either.
  On most sites that is the rule the owner meets first, because the repair
  costs crystal and deuterium and most sites make neither. The warning and
  the rules say to keep the repair cost in the store.
- **A fleet charging a jump out of the outpost is not refuelled there**, and
  a tank the store cannot fill is reported once per stay. At the homeworld a
  departing fleet is still topped up; at an outpost that would have raised a
  dry-store warning on every departure.
- **`unload` at the outpost is refused for an empty hold** (`NoResources`),
  not answered with an alert that nothing moved. `load` there is refused
  for a fleet under standing orders, as at home.
- **`settle_hold` reads the dock from where the fleet is.** Cargo shed by
  ships lost at the outpost goes into its store as far as there is room, the
  rest is lost, as at the homeworld.
- **The outpost is not a field of `Player`.** As the design has it:
  `GameEngine::outposts`, `Snapshot::outposts`, a `runtime_state` row of
  kind `outpost` in SQLite, which needs no new table or column. A row of
  that kind that cannot be read refuses the world.
- **The report counts what the engine says.** `sim-run` counts outpost
  raids, what they took, yield and ticks breached. It does not count what
  `load` takes from a store: the engine answers `load` with an alert and
  nothing a report can count, and no bot hauls.

## 2026-10-11 (the windshield's phone layout: sheet, banner, toasts, guide, fleet rail)

**Decided by the agent that built it, not by the owner, and open to change.**
Four things the layout pass before it listed as still wrong.

- **An open status sheet is not covered.** A banner (NEW SECTOR, ENGAGED,
  VICTORY) is drawn under the sheet. Where the sheet takes only the right of
  a wide arena the banner is centred in what is left beside it; on a phone,
  where the sheet takes the width, the banner is not seen while the sheet is
  open. The event it announces is in the log either way.
- **The sheet stands on an opaque ground** the size of its panels, not of
  the whole sheet area: a short sheet does not black out the map below it.
- **Toasts follow the layout, not the width.** They stand at the top
  wherever the screen in view runs a dock along the bottom: every screen
  below 820 px, and the windshield in any window too narrow for its side
  columns (up to 1167 px). The other screens keep their desktop layout at
  those widths and keep their toasts at the bottom right.
- **The guide has two wordings for a gate card, and the cards were left as
  they are.** A phone's small card shows a key and a terrain. The jump and
  harvest lessons now say so there, point at the gate's colour and at the
  Map, and say "tap"; with full cards they speak of the T digit, the bars
  and hovering, as before. Putting the digit and the bars on a 92 px card
  was the other way and was not taken: the terrain name would not have
  fitted beside them.
- **The desktop fleet rail ends where the feed may begin** (the bottom of
  the south-west gate card) and scrolls inside that; the fleet selected is
  brought into view. No count of fleets is cut off and none is hidden for
  good.

## 2026-10-11 (a fleet's orders are on its row; what an order costs is said)

**Decided by the agent that built it, not by the owner, and open to change.**
The two principles are the owner's: a console shows what a fleet has been
told whoever told it, and anything that costs the player or ends their work
is told to them as an event with its reason.

- **`FleetState.policy` is one object, `{preset, params}`, or absent.** The
  doctrine and its five settings travel together and are present exactly
  when the fleet is under orders. Two fields side by side (`policy` and a
  separate `policy_params`) could have disagreed, on the wire or in a client
  that read one and not the other; one object cannot. The name `policy` was
  kept: it is the word of `policy_update`, and the object reads as the
  message that set it. A client that read `policy` as a string must be
  changed; none was kept working.
- **The settings are the ones in force**, after the server's clamping, all
  five, never a subset.
- **The web panel keeps nothing of its own.** What it remembered of orders
  it had sent is gone; it shows and starts from the fleet row.
- **Settings given without a doctrine change the orders in force.** In the
  web command bar `policy range=6` used to mean the current doctrine at the
  defaults plus that range. It now keeps the other four settings. A line
  that names a doctrine (`policy mine range=6`) still starts from the
  defaults, which is what the panel's button sends. The TUI's `p` and `P`
  keep the settings the fleet has in the same way.
- **`stop` on a charging jump says what fuel is forfeit; on a boarding, that
  the site is on alert.** Both are warning `Alert`s. Halting a harvest costs
  nothing and says nothing. The boarding case was not on the list this work
  was given; it is the same order silently costing the player, in the same
  function.
- **The fuel named is what a jump costs the fleet at the moment of the
  stop**, not a figure stored when the jump was ordered. The two differ only
  if a Fuel Efficiency level finishes during the second or two a jump
  charges. Storing it would have meant a field in the fleet, the snapshot,
  the SQLite schema and the save format for that case.
- **A recall is answered with one `Alert` that carries its whole cost**: the
  fuel, a jump abandoned with the fuel it had burned, how many ships took
  hull damage and how much, how many broke up. It is a warning when a ship
  was harmed and info when none was, so a clean recall is a log line and a
  costly one is also a toast.
- **A harvest that ends by itself says why, once, as a warning `Alert`**:
  the hold is full, or nothing (or none of the resource named) is left to
  mine there. A warning and not info because the web client raises a toast
  only for warnings, and a fleet that has stopped working is what the player
  needs to see. A fleet under standing orders is not reported this way: its
  doctrine says what it does about it in its next report.
- **No new event kind.** All of these are `Alert`s with the fleet and the
  sector, as the hold alerts are.
- **An alert toast in the web client stays nine seconds**, as a refusal
  does. These sentences run to five lines of a toast and 4.2 seconds did not
  read them.
- **Not changed, and seen:** a fight that catches a fleet charging a jump
  also forfeits the jump's fuel and says nothing of it; the fight itself is
  announced.

## 2026-10-11 (recall is refused at home)

**Decided by the agent that built it, not by the owner, and open to change.**
The principle is the owner's: an order does what its name says or is refused
with one clear sentence, and it never silently costs the player.

- **`recall` is refused for a fleet at its own homeworld**, with
  `InvalidCommand`, nothing changed and a sentence. It used to be accepted
  there: it cost nothing, did nothing to an idle fleet, called off a jump the
  fleet was charging (whose fuel was spent and was refilled from the
  stockpile), and halted a fleet harvesting at home.
- **A jump ordered at home and paid for goes on.** The refusal names its
  target and says that `stop` is the order that calls it off.
- **A called-off jump keeps the fuel it burned**, as it always has for
  `stop` and for a fight that catches a fleet charging. A recall from
  another sector abandons a charging jump the same way and then pays for
  its own. That is now written in the rules; it was true and unsaid.
- **No refund was added.** A refund on `stop` would make every jump free to
  take back until it lands; the code has long called the lost fuel "the
  price of hesitation" (`handle_stop`).

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
  has jumped away comes home unmarked and unloads as always. A cancelled
  jump does not end it (and recall at home, which did not either, is now
  refused: see the entry above). It is a column of the fleet's
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
