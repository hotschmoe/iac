# Design decisions

Owner decisions that set the game's identity. Newest first. Proposals that
conflict with these need the owner's sign-off.

## 2026-10-13 (the web client stops working the rules out for itself)

On the branch `worktree-agent-aef205bfd9346a5b7`, not merged and not pushed.
The journal entry of the same day has what was run.

**The owner's request.** The sweep that came with the generated wire types
listed places where the web client works a rule out for itself and gets
another answer than the engine. The owner asked for them to be fixed.

**The rule.** A client draws and words. It does not work out a rule of the
game. What a view shows comes from one of three places, tried in this order:
the state the server already sends; a constant or a table generated from Rust
through `shared/tests/wire/mod.rs`; a field added to the state for it. Only
where none fits does the client keep a formula, with a comment naming the Rust
function it follows. No value the game plays by moved: `ECONOMY_VERSION` is 5,
`chart_golden.txt` and `numbers.json` are untouched.

**The eight disagreements, as settled.**

1. **Every tech has a node.** The research tab listed 12 techs by hand and
   Modular Fabrication was not one. The lanes are now an exhaustive `switch`
   over the generated `ResearchType`, filled from the catalog. A tech added
   in Rust does not compile in the client until it has a lane.
2. **As many build slots as the state says.** `HomeworldState.build_slots` is
   1 plus the level of Modular Fabrication, so 1 to 3. The queue strip has a
   pill for each, and one locked pill for the next slot while Modular
   Fabrication has a level left. The overview keeps four cards: the second
   card lists every slot after the first. Each slot after the first has its
   own Cancel. On the overview an unlocked slot with nothing in it reads
   IDLE; before, it read LOCKED. The research and shipyard cards count their
   line against 1 plus the waiting line, not against the building queue's
   depth.
3. **Boarding takes the engine's 20 ticks.** `EXPLORE_DURATION_TICKS` is of
   the real-time class: 20 ticks at every pace. While a fleet boards, its
   `cooldown_remaining` is what the boarding has left (a server test already
   held it to `end_tick - tick`). The ring is drawn from that and from the
   generated constant, so it is right after a reconnect too. It was a timer
   of 8 seconds.
4. **No loot tier is invented.** The map printed `1 + min(3, ring / 6)` for
   every sector. The engine's tier is the threat band of the ring (1 to ring
   11, 2 to ring 21, 3 beyond) and belongs to a derelict. The line is gone;
   a sector that holds a derelict shows the tier the server sent, as before.
5. **The Fabricator shortens building time.** Its note said it speeds up the
   shipyard, and its tile said SLOT B. Slots come from Modular Fabrication.
   README and `RULES.md` already had it right.
6. **One source for zones.** `Zone::INNER_RING_RADIUS` and
   `OUTER_RING_RADIUS` are generated. `WorldZone` in `lib/protocol/protocol.dart`
   is the one place a ring is given its zone and its name; six copies of the
   ternary and four of the names are gone. The map says `R21+ THE WANDERING`.
7. **A route's time and its way home.** The engine charges a jump for
   `10 / slowest speed` ticks, less the Navigation level, and lands the fleet
   on the tick after: a Scout fleet takes 2 ticks a jump, one with a Frigate
   3, and Navigation takes one off each. The state now says so:
   `FleetState.jump_ticks`. The route panel's ETA is hops times that. The way
   home from a route's end is counted over the lanes on the player's chart,
   not by straight distance, and the panel says so (`OK BY THE CHART, 4 hops,
   margin 65 fuel`). The server counts the way home over every lane, charted
   or not, so its count is never more than the chart's: a return the panel
   calls possible is possible. For a route of one jump the server's
   `preview_move` answers, as before. `preview_move` takes an adjacent sector
   only, so it cannot answer for a longer route.
8. **A derelict can be boarded with a group in the sector.** The engine has
   no rule against it. It refuses a boarding only where there is no derelict,
   or while the fleet fights, moves, already boards or is on cooldown, and it
   words each refusal. The windshield now greys Board only where the state
   shows no derelict, and sends the order otherwise; a refusal arrives as the
   engine's sentence. A group that attacks on sight has the fleet in combat
   the tick it arrives, so the case this opens is mostly a derelict beside a
   passive group.

**The hand-copied numbers.** Generated and used: the highest threat (9), the
scan ranges (1 and 2) and the scan cooldown (5), the most a site's grade can
be (12, now `SITE_GRADE_MAX`), the lane search limit (64, now
`LANE_SEARCH_HOPS`), the longest name (24, moved to `shared` with its
shortest), the bounds of the doctrine settings (now `POLICY_*_MIN` and
`_MAX`), the fuel budget that lifts the bound (100), the rings of an outpost
where three sentences spelled them, and the four ratios the gauge is drawn
from. The storage threshold is generated too and compared as the engine
compares it: `stock / cap` as an `f32` against the `f32` 0.85. Arithmetic in
double precision against 0.8500000238 would have moved the edge; this does
not, and a stockpile exactly on it reads NEAR CAP in the client on the tick
the engine says `StorageNearCap`.

Left as literals, with the reason: the right end of the ratio gauge (4.0, how
far the scale is drawn); the ten colours of the threat palette (a drawing
table, indexed by its own length); `mapRMax` 22 (how far the map draws); the
steps the doctrine panel offers between the generated bounds.

**What the client still copies.** Each is small and has a comment or a type
that holds it: `WorldZone.ofRing` follows `Zone::from_distance` on generated
radii; the status panel picks the scan range by "a Scout aboard", as
`handle_scan` does; the locked slot says Modular Fabrication opens it, as
`scaling::building_slots` does.

**Found in the engine, and left as found. The owner's to decide.**

1. **A boarding pays a tick after `end_tick`.** `SiteExplorationStarted`
   carries `end_tick = tick + 20`. The timer reaches 0 on that tick, and
   `process_exploration` runs before `process_cooldowns`, so the loot or the
   ambush comes on `end_tick + 1`. By reading, not by a run.
2. **A fight drops a boarding without a word.** A patrol that wanders in
   takes the fleet into combat. The boarding is gone, the site is not put on
   alert, no `Alert` says so, and the fleet keeps what was left of the
   boarding timer as its cooldown after the fight.
3. **`home_fuel` is not the charted way home.** The protocol and the README
   say it is the fewest hops over lanes the player has charted. The engine
   prices `direct_hops_home`: every lane, charted or not.
4. **`preview_move` cannot answer for a route.** A sector more than one jump
   away is refused with `NoConnection`. If the server should judge a planned
   route's return, that is a new request.

**For the owner.**

1. Merge or not. The protocol gained one field, `FleetState.jump_ticks`.
2. Four views still count "hops home" and "fuel for N hops" by straight
   distance or by dividing rounded figures (the sweep listed them; the brief
   did not). `FleetState.range_hops` is sent and shown nowhere. A `hops_home`
   on the fleet would settle them.
3. The route panel adds up whole-number fuel (`jump_fuel` rounded, times
   hops). Over ten jumps that can be a few units off the engine's figure.
4. The map inspector still calls a sector RICH, TYPICAL or POOR "for this
   distance" by a formula of its own (`1.2 + 0.11 * ring`). No rule of the
   engine stands behind it.
5. The scripted world of the widget tests still gives ring 8 Corvettes where
   the engine gives Scouts (`npc_class`), and moves a fleet at once where the
   engine has it in transit. Its jump now takes as many ticks as the
   engine's, and its boarding 20.
6. The captions under the ratio gauge (98 percent, 1 to 15 percent) repeat
   the comments on the `RATIO_*` constants, which the sweep found at odds
   with `RATIO_MARGINS`.

## 2026-10-13 (the economy's numbers are written once, in Rust; `sim.py` reads a file Rust renders)

Merged into master and pushed on 2026-10-13.

The owner: "The Dart protocol mirror and sim.py could be generated from the
Rust source instead of mirrored by hand: seems like a good idea." This is the
`sim.py` half. No number of the game moved: `ECONOMY_VERSION` is still 5, the
fixtures are untouched, and `sim.py` prints what it printed, byte for byte, in
ten runs (`--tables`, the full report and eight more). The journal entry of
the same day has what was run.

**As built.**

- **A number of the economy is written in Rust and nowhere else.**
  `sim/tests/economy_numbers.rs` renders `docs/design/economy/numbers.json`
  from `shared/src` and `sim/src/score.rs`; `sim.py` loads it and declares no
  number of the game. The test runs in `cargo test --workspace` and fails when
  the file in the repository is not what it renders, naming the first line that
  changed. `UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers`
  renders it again: the switch of the protocol fixtures.
- **The generator is a test of `iac-sim`**, not the example of `iac-shared` it
  replaces (`shared/examples/economy_constants.rs`, archived). The score
  constants are in `sim/src/score.rs`, which `iac-shared` cannot see, and the
  old check read them out of the source with a regular expression. A test is
  also the one place `cargo test` can render and compare.
- **JSON, not a Python module.** Nothing is imported, so nothing is compiled
  into `docs/`.
- **What is generated, by this rule.** Where Rust names every coefficient of a
  formula, `sim.py` keeps the formula and reads the coefficients, so it
  computes what it computed, to the bit, and `--set` reaches them. Where Rust
  writes the coefficients inside a function and the simulation only asks at
  whole rings, threat ratings or levels, the file carries the function's
  values and `sim.py` looks them up: a group's class, presence, passive share,
  respawn and base multiplier, the ore odds, a derelict's ambush chance and
  tier, the relic chance, the Defense Grid's units, what a research level
  multiplies, the building slots. Where the simulation needs such a formula at
  any pace or any score, its coefficients got names in Rust: six of them,
  `RAID_POWER_FLOOR`, `RAID_POWER_PER_POINT`, `RAID_POWER_EXPONENT`,
  `DERELICT_VALUE_RING`, `CAP_EXPONENT_PAST_KNEE` and
  `THREAT_POINTS_EXPONENT`. Naming them changed no value: the rendered file was
  the same bytes before and after.
- **The tables are the game's own `f32` values**, each written as the shortest
  decimal that reads back as it (0.08, not 0.07999999821186066). Four of the
  looked-up curves carry `f32` rounding the Python formulas did not have
  (the ore odds, the passive share at rings 9, 13 and 14, the grid's units
  from level 5, the base multiplier). The simulation's state moved by at most
  5 parts in 100 million over 40 runs, no level, ship or milestone moved, and
  nothing printed changed.
- **`tools/check_sim_constants.py` compares formulas only**, 2,390 values,
  against the `tables` half of the same file, with the tolerances it had: a
  cost within half a unit (the game rounds, the simulation does not), a time
  within 1 percent or 2 ticks (the game counts whole ticks), `f32` against
  `f64` elsewhere. It covers costs and times by level, production, storage
  caps at three paces, raid power, the ring gradient, chart, discovery and
  kill points, the exchange and the outposts. It is 172 lines where it was
  315, and no longer holds a mapping of constants.
- **`--set NAME=value` works as it did**, on the same names. More names can be
  set now, because inline figures became names (`NPC_POWER_GROWTH`,
  `RAID_POWER_EXPONENT`, `DERELICT_VALUE_BASE` and others).
- **One copy of the game is still kept by hand in `sim.py`**: the terrain
  shares and ore chances of `zone_terrain` and `ore_prob`. Rust rolls them
  inside `shared/src/world.rs`, with no table to render, and nothing compares
  them. See the first question below.

**Found on the way, and left as found. The owner's to decide.**

1. **The terrain copy is slightly off, and unguarded.** Worldgen rolls a
   number from 0 to 100, which is 101 outcomes, so its shares are 55, 15, 10
   and 21 in 101 where `sim.py` has .55, .15, .10 and .20, and an ore chance
   of 90 is 90 in 101. About 1 percent. Rendering these from Rust needs the
   rolls of `roll_terrain` and `roll_resources` as a table; giving the
   simulation the true shares would move its reports.
2. **`sim.py --tables` prints the wrong storage caps at pace 600.** Its table
   uses P^0.75 at every pace (606,154 metal with no Vault); the game, and the
   simulation's own runs, use the knee at pace 10 (78,254). The same column is
   in `spec.md`, section 3. One line to fix; it changes the table.
3. **Rust holds numbers that come from `sim.py`, pasted by hand**: the
   `first_cruiser_s` and `endgame_s` of `pace::PRESETS`. They have drifted
   from what the simulation prints today (persistent: 15.8 and 73.9 days in
   Rust, 19.3 and 77.0 printed; dev: 13 hours to the endgame in Rust, 19.2
   printed). Nothing compares them.
4. **The simulation's exchange and outpost functions feed no report.** They
   exist to be compared with Rust. They were kept, on generated constants, and
   stay compared; removing them and their rows is cleaner if nobody calls them
   by hand.
5. **Smaller differences the model has always had**: its queue depth of 3
   counts running and waiting orders together where the game lets 3 wait
   behind the running ones (it now reads `QUEUE_WAITING`); it plays from ring
   4, the middle of the home seats 3 to 5; it makes a player eligible for
   raids at Shipyard 2 where the game also accepts a Defense Grid.

## 2026-10-13 (the Dart wire types are generated from the Rust ones)

Merged into master and pushed on 2026-10-13. The journal entry of the same
day has what was run.

**The owner's words.** "The Dart protocol mirror and sim.py could be generated
from the Rust source instead of mirrored by hand: seems like a good idea."
This entry is the Dart half. `sim.py` was another session's work.

**What is decided by building it.**

- **The Rust types are the only place the wire is written.**
  `clients/web/lib/protocol/protocol.g.dart` is generated from them, checked
  in, and never edited. `UPDATE_FIXTURES=1 cargo test -p iac-shared`
  regenerates it with the fixtures; `cargo test -p iac-shared` fails while it
  is stale. A protocol change is: edit Rust, regenerate, fix what
  `flutter analyze` points at.
- **The generator reads the source and is proved against serde.** It reads
  `shared/src` with `syn` (a dev-dependency of `iac-shared`, already in the
  lockfile for serde's derive; no engine build compiles it). Reading source
  means interpreting serde's attributes, which could go wrong silently, so two
  things stand against that. It refuses, by name, any serde attribute, type or
  skip test it does not know. And it makes sample messages from its own model
  for every type, variant and field, has serde parse and re-emit each, and
  requires the result to match the model. A misread `rename`, `default`,
  `flatten`, tag or `skip_serializing_if` fails `cargo test` before any Dart
  runs. The defaults serde fills in are learnt from those round trips.
- **Why the others lost.** `serde-reflection` cannot describe internally
  tagged enums or `flatten`, and the protocol is built on both. `typeshare`
  has no Dart output and reads neither. `schemars` and `specta` need a derive
  on every wire type, so a dependency of `iac-shared` that the wasm build
  would compile or a feature gate on some 120 types; both re-read serde's
  attributes themselves and check nothing against serde; and a JSON Schema
  has lost what the Dart API is named after: the Rust field name behind a
  `rename` (`ShipState.shipClass` is `class` on the wire), the name of a
  flattened field (`GameEvent.kind`, `ExchangeQuote.terms`), and which struct
  a tagged variant wraps. A declarative description that the Rust types are
  generated from would have rewritten `protocol.rs` as macro input and put
  its doc comments out of reach of rustdoc.
- **The shape of the Dart, by rule and not by table.** A struct is a class
  with a `const` constructor, `fromJson` and `toJson`. An enum of unit
  variants is a Dart enum with its `wire` name. A tagged enum is a sealed
  class. A struct that one tagged variant wraps and nothing else names
  extends that sealed class (`AuthResult` is a `ServerMessage`); a variant
  with fields or none is a class named variant plus the enum's last word
  (`MoveCommand`, `RequestFullStateMessage`); a variant that wraps a type
  used elsewhere is a wrapper named the same way (`CommandMessage`,
  `EventMessage`). A constructor parameter is optional exactly where serde
  fills the field, with serde's value, and for a struct with `Default` in
  Rust every parameter has that default. A struct with `Eq` in Rust has
  value equality in Dart. Doc comments are Rust's.
- **What Rust knows beside the wire is generated too**, where it is a
  constant or a per-variant value: the labels of eleven enums, `ShipClass` hull,
  shield, weapon, cargo and power (the client's own table in
  `lib/console/threat.dart` and a second one in the windshield painter are
  gone), `ResourceKind.weight`, `ErrorCode.code`, `botNamePrefix`,
  `maxOddsSectors`, the four ratio bands, the outpost rings. One line in
  `shared/tests/wire/mod.rs` each, which calls the Rust function or names the
  constant. An `f32` constant is written as the exact value of that `f32`, so
  a comparison with a number off the wire falls as it does on the server.
- **What stays written by hand**, and what guards it:
  `lib/protocol/protocol.dart` (155 lines) is the client's own: extension
  getters and `HomeworldState.copyWith`, checked by the Dart compiler against
  the generated classes. `lib/protocol/hex.dart` is hex math with the two
  fields of `Hex`, guarded by `fixtures/hex` and by every fixture that
  carries a coordinate. Formulas and sentences in the views that repeat
  `scaling.rs` or the engine are not touched; the journal lists them.
- **Decoding is as strict as it was.** Unknown tags and enum names throw
  `FormatException`, a missing required field throws, an optional field is
  nullable and left out when null. What went is a convenience of the hand
  mirror: its constructors had defaults serde does not have (eight fields of
  `HomeworldState`, `DefenceState.restoring`, `HomeworldCatalog.defences`,
  `waitingOn` of the three queued orders, `HarvestCommand.resource`,
  `StandingOrders.params`). A generated constructor asks for what serde asks
  for, and the test world that leaned on those defaults now names the values.
- **Three names changed**, by the rules above, with every caller:
  `RequestFullState` is `RequestFullStateMessage`; a lone event is
  `EventMessage(GameEvent)`, because `GameEvent` is also what `TickUpdate`
  lists and cannot carry a `type` there (`GameEvent.toEventJson` is
  `toJson`); `RatioLabel.forRatio` is the function `ratioLabelFor`, and
  `OddsCommand.maxOddsSectors` the constant `maxOddsSectors`.

**For the owner.**

1. Merge or not.
2. The generated file declares `// dart format width=120`, the width the
   hand-written file was typed at; the rest of the client is not formatted
   with `dart format` at all (on master 91 of its 99 files would change). 80
   is one constant in `wire/dart.rs`.
3. `fixtures/protocol/synth` adds 67 files and 337 KB of generated messages,
   one a line. They make the Dart proof complete without anyone writing a
   fixture. If that is too much to carry, they can be made in memory by a
   Dart test that shells out to cargo, at the price of the client's tests
   needing cargo.
4. The inventory in the journal lists what the client still repeats of the
   engine by hand, and the places where the two disagree today. None was
   changed here. Which of them are bugs is the owner's call.

## 2026-10-13 (one stored world: both hosts store one document, and there is no journal)

The owner's answers to the questions of `docs/design/storage.md`, section 12,
given on 2026-10-13, and what was built on them the same day: steps 1 to 4 and
8 of that study's section 10, merged into master and pushed on 2026-10-13.
`docs/data_architecture.md` describes the result; the
journal entry of the same day has what was run.

**The five answers.**

1. **A killed server may lose more than one tick.** The owner: "this is not
   mission critical infrastructure, we want to be professional but we don't
   need to be perfect". So there is **no journal** of inputs. None was built
   and nothing was left in the code for one; the study stays as the record of
   how to add it.
2. **Reading a world with SQL is given up.** A world file is a header line and
   JSON, read with `jq`.
3. **The server's place is files.** The owner has no preference between files
   and SQLite; files were chosen by the session that briefed the build, for
   about 100 lines and because `rusqlite`, with the SQLite it compiles, leaves
   the workspace. The browser's place stays IndexedDB behind the worker.
4. **The save format number is the builder's to judge by what is cleanest**:
   no additive compatibility, no importer for old databases or saves. All
   current worlds are disposable. Built: `SAVE_FORMAT` 4 to 5, because the
   header changed shape (what a host keeps beside its world moved into one
   `host` field). A browser save and a server database from before are
   refused or ignored; nothing is migrated.
5. **No compression on the server.**

**What follows from them, as built.**

- **A stored world is one document**, the same for both hosts
  (`iac_sim::store`): a header line (format, economy and worldgen versions, a
  checksum of the body, the host's own part) and the `Snapshot` as JSON. The
  server stores it in a file (`--world`, which `--db` became), the browser in
  IndexedDB. The single-player host loads the bytes a server wrote once the
  header's own part names a commander and a clock, and a server plays a
  single-player save as it is; `server/tests/hosts_agree.rs` holds both.
- **A new persistent field is written once, in `Snapshot`.** The engine knows
  nothing of storage. `server/src/database.rs` (1,630 lines and 487 of
  tests), `sim/src/persist.rs` (`Persist`, `PersistBatch`, `RuntimeKind`), the
  engine's ten dirty sets and the lines that marked them, the one in
  `intel.rs`, the `Persisted` phase of the host core's step (the entry below
  still names it) and the repairs `restore` made to rows read back out of
  step are gone. Rule for what comes next: nothing may
  go into the engine that exists for storage.
- **What a stop costs, and this is the decision's price.** A server that is
  stopped (SIGINT, SIGTERM) writes its world and loses nothing, so a deploy
  loses nothing. A server that is killed comes back as its last file says:
  it loses up to the period, orders included, where it lost one tick before.
- **The period is 300 seconds** (`--save-every`): the owner's choice of
  2026-10-13 ("300s is fine"), from this table. It was built at 60 seconds,
  set by the session that briefed the build without weighing the bytes:

  | `--save-every` | a kill loses at most | written a day by the largest world measured (24 seats at the end of a season week, 13.2 MB a document) |
  |---|---|---|
  | 60 s (as built) | a minute | 18.9 GB |
  | 300 s (the default) | five minutes | 3.8 GB |
  | 600 s (the study) | ten minutes | 1.9 GB |

  The rows this replaces were measured at 7 to 10 GB a day in a world's first
  three days. A young world writes far less either way (0.4 GB a day in its
  first minutes with 15 bots).
- **A registration does not wait for the period.** A token is shown once, so
  an account the world forgot would leave a player holding a password to
  nothing. Once a token is issued the file is written within two seconds of
  the last write (about three seconds in all, since the tick that does it
  comes once a second). A world has 24 home seats, so this can happen at most
  24 times in a world's life, and claims.
- **A world keeps the seed and the pace it was created with.** Both are in
  its file; a server started on an existing world with another `--seed` or
  `--pace` refuses with a sentence. Before, another seed regenerated the
  galaxy under its players with a line in the log (the study, 9.3). The old
  `--world <preset>` flag, a second spelling of `--pace <preset>` that nothing
  used, gave its name to the file.
- **One server per world.** The file's lock (`<file>.lock`, `flock`) is held
  while a server runs; a second one refuses to start. SQLite's own lock did
  this by accident before.
- **"When is a document owed" is said once**, in `store::Due`, for both
  hosts: a period after the last one, or sooner once hurried, never sooner
  than a short gap after the last. The times stay each host's, because what
  a lost document costs differs: the single-player host replays the ticks
  since its save, so it hurries for an order and saves every four minutes;
  the server replays nothing, so its period is its loss and it hurries only
  for a token.

**Not decided here, and the owner's.**

- **Moving a world between browser and server** (the study's question 7). It
  is not built. What is left to build is small and is listed in
  `docs/data_architecture.md`: an import and an export on the page, and
  clearing the commander's token so that the first login claims the account.
- **A server's world still pauses while its server is down** (question 8).
  Nothing changed there.
- **Editing a world by hand** needs the checksum restamped, and there is no
  tool for it.

## 2026-10-13 (one host core: hosting is written once)

Asked for by the owner: "Remove ALL duplications so we only have one source of
truth for shared pieces. Standalone code that differs should be strictly
solo<->multi (like ws and tokens) and native<->web (like storage); anything we
can move to a shared crate we should." Merged into master and pushed on
2026-10-13. Storage was left to the study running beside this
work. The journal entry of the same day has what was run.

- **The rule.** The rules were already written once, in `iac-sim`. Now the
  hosting around them is too, in `iac_sim::host`, and every host runs it: the
  server, `iac-solo`, the headless runner and the tests that play a world with
  bots. A host may differ from another only in who may log in and how bytes
  reach it (solo against multiplayer), and in its clock and its storage
  (native against web).
- **One tick: `host::step`.** The cast takes its turns, `GameEngine::tick`,
  its events are drained, every logged-in session's `tick_update`, the cast
  observes, `persist_dirty_state`. That order was written three times
  (`server/main.rs`, `solo/world.rs`, `sim/headless.rs`) and four more times
  in tests. The three hosts did not agree on it to the letter: the server
  drained and sent before the bots observed, the single-player host sent after
  the bots had observed and after the persist, the headless runner drained
  after its seats had observed. **Nothing moved by choosing one**, and this was
  checked, not assumed: observing takes the engine by shared reference,
  `views::tick_update` reads what the tick's refresh left (`lapsed`,
  `expired`, `marked`) and never the dirty sets a persist clears, and the
  events are handed to the views as an argument. The season report of
  `sim-run` is the same bytes before and after, and the chart recording passes
  untouched.
- **A host that must look inside a tick gives the step a `Watch`**, called at
  each of the five phase boundaries with the engine, the cast and the tick's
  events. The headless runner's counting is one (it notes tanks and stores once
  the seats have played, and counts when the tick is simulated), and so are
  the server's tick timings. This is the alternative to a host copying the
  sequence to get between two of its lines.
- **One gate: `Host::receive`.** A logged-in session's message goes to
  `GameEngine::answer`; `auth` goes to the `Door` and a session it admits is
  sent the result and then its full state; anything else from outside is
  `commands::not_logged_in`. The session-to-account map is the host core's.
- **The seam is the `Door`,** one method, with both sides next to each other
  in `host.rs`: `Tokens` (a new name registers, a known one presents its
  token; the host gives it a mint) and `Commander` (one name, no token). The
  server contributes only the mint, from OS entropy. They live in the core and
  not in the hosts so that the whole of the difference can be read in one
  place; a host chooses one.
- **Seating a world is `Host::new` and `Host::enlist`**, for a world just
  made and for one resumed alike. What a host says when the seats run out
  stays its own: an operator reads about `--player-seats`, a player about the
  seats beside theirs.
- **One check of a stored world's versions: `host::Stale`.** It decides and it
  words the refusal; a host names what it stores ("save", "database") and adds
  what its reader can do about it. The server's sentence now says "this build
  runs" where it said "this server runs"; the single-player sentences are
  unchanged to the letter. The save's format number and the database's pace
  and missing `world_meta` stay with their hosts, which alone have them.
- **`iac-server` is a library and a binary.** The cross-host guard has to
  read a server's database as a `Snapshot`, and only the server can
  (`Database::load_snapshot`); a crate with nothing but a binary lends
  nothing to its tests. `lib.rs` is three `pub mod` lines. The engine is no
  longer behind `Arc<Mutex<..>>`: only the tick ever touched it, and the
  transport now holds sockets and a queue and no world.
- **What the transport still decides, as before:** a message whose session
  has gone by the time the tick drains the queue is dropped unprocessed. The
  socket map is the transport's, so this stayed there. A session's leaving
  reaches the host core as an entry in the same queue.
- **The cross-host guard is `server/tests/hosts_agree.rs`**, in
  `cargo test --workspace`. A real server makes a world (seven bots at tick 0,
  then an account over the wire) and is stopped; what its database holds is
  read as a `Snapshot` and handed to `iac-solo` as a save document; the server
  is started again. One commander then plays 40 ticks on both: nine batches of
  orders, accepted and refused, with questions about the whole world. Every
  frame of the two sessions, from the answer to the login on, and the final
  worlds (`Snapshot::hash` after a clean stop) must be equal. About 45 seconds.
  - *Why a stored world and not two new games.* The front doors cannot make
    the same world: the single-player host seats its commander first and the
    bots after, all at tick 0; a server seats its bots at tick 0 and can admit
    a player no earlier than tick 1. That difference is real and was left
    alone. Resuming one stored world in both also puts SQLite and the save
    document side by side.
  - *How orders land on known ticks.* Nothing is assumed about the server's
    clock. Each batch travels between two `request_full_state`; the server
    answers a session when it takes its messages in, before that tick's
    update, so the update after the two full states names the tick, and the
    single-player host is given the batch before the same tick. A batch the
    server took in two ticks cannot be placed, and fails the test saying so
    instead of guessing; it needs a stall of about a second in the middle of
    three small frames.
  - *What it catches and what it cannot.* It catches a host that departs from
    the core: with the step's order broken for the server alone it failed at
    the fifth frame, naming the field. It cannot catch a change to the shared
    order, which moves both hosts together (run so, it passed);
    `chart_equivalence.rs` and `bots.rs` catch that (run so, both failed).
- **Left as they are, for the owner to rule on:**
  - The headless runner seats its players as ordinary accounts
    (`Seat::join`), the hosts seat bots as bot accounts with a record
    (`Roster`). Making the runner use the roster would list its bots apart on
    the leaderboard they read, and move every report.
  - `sim/tests/common` and `scenarios_world.rs` tick a bare engine
    (`tick`, then read the events). That is the engine's own interface under
    test, with no cast, session or storage, and some of those tests call
    `persist_dirty_state` at chosen moments to read one batch; they were not
    put on the step.
  - Three test files carry the same 12-line `first_difference` over JSON
    (`database.rs`, `both_targets.rs`, `hosts_agree.rs`), and the two solo
    test files the same `call`/`frames`/`send`. One copy needs a
    test-support crate, and the bar for new crates is the owner's.
  - What a client does with a `tick_update` is written in `script.rs` (for a
    seat), in the TUI's `state.rs` and in Dart. The TUI keeps its own shape of
    state; the Dart side is native against web by nature.
- **`CLAUDE.md` carries the rule**, added at the merge: the layout rows of the
  three crates, the guard's command line, and the architecture bullet that
  says a host writes no tick loop, session map or login of its own.

## 2026-10-13 (the question that led to one stored world)

The owner asked whether the browser's save and the server's database can be
one source of truth. `docs/design/storage.md` studied it, proposed one
document and a journal of inputs beside it, and put eleven questions. The
owner answered the same day; the decision is the entry at the top of this
file.

## 2026-10-12 (one world on every target: where the engine's maths comes from)

Asked for by the owner: the same seed and the same orders must give the same
world from a native build and from the WebAssembly build, with a test that
keeps it so. Merged into master and pushed on 2026-10-12. The journal
entry of the same day has the cause and the measurements.

- **The rule.** Float maths in `shared/`, `sim/` and `solo/` uses what IEEE 754
  fixes (the four operations, `sqrt`, rounding, casts) as it comes, and takes
  everything else from `iac_shared::maths`: `powf`, `powi`, `ln`, `log2`, `exp`,
  `exp2`. That module is the `libm` crate, which is Rust's port of musl's maths
  compiled for whatever the target is, and an integer power that multiplies in
  a fixed order. A function it lacks is added to it. No `cfg` picks arithmetic
  by target. `clippy.toml` in each of the three crates bans the std methods
  (26 of them, for `f32` and `f64`), so `cargo clippy` refuses a new one.
- **Chosen so that the wasm build did not move.** Rust's own maths on
  `wasm32-unknown-unknown` is the same port, and `maths::powi` multiplies in the
  order the compiler's routine does, so every world a browser has played plays
  on to the same bytes. The native build is the one that changed: it used the
  system's libm, and its wrecks, sector heat, raid sizes, chart and boarding
  points and odds now differ from before in the last bit. Costs, build times,
  production and storage caps did not move on either target.
- **`libm` is pinned to one release** (`=0.2.16` in `shared/Cargo.toml`).
  Another release may round a function differently, and that would change what
  every stored world plays next. Moving the pin is an economy change.
- **`ECONOMY_VERSION` 4 to 5; `SAVE_FORMAT` stands at 4.** A database written
  by the native build of version 4 would play on differently under this one,
  and the constant exists so that such a database is refused rather than
  carried on in silence. The owner has not ruled on this bump. The constant's comment now says so: it is bumped whenever a
  stored world would play on differently, the arithmetic included. The save
  format did not change. **The cost, for the owner to weigh:** a browser save
  carries the economy version too, so the saves of version 4 are refused with
  the usual sentence although they would have played on to the same bytes.
  One number cannot refuse the native databases and keep the browser saves.
  Both are disposable by the owner's word; if the browser saves matter more,
  the bump is one line to take back.
- **The guard is a test in `cargo test --workspace`**:
  `solo/tests/both_targets.rs`. It builds `iac_solo.wasm` with the cargo of the
  test run, runs it in `wasmi` (an interpreter, a dev-dependency of `iac-solo`
  alone) beside the native host and sends both the same requests; every reply
  and every save must be the same bytes. It needs the `wasm32-unknown-unknown`
  target and fails with that sentence when it is missing; it needs no network,
  no Node and no browser. Three worlds, about two minutes together, the season
  day being 114 seconds of it. Why there and not in `tool/solo_e2e.dart`: the
  browser check runs when someone runs it, and a rule that every change to a
  formula can break belongs where every change is tested. Why `wasmi` and not
  `wasmtime`: six small crates and 25 seconds to build against a compiler of
  its own, and an interpreter is fast enough (the season day takes it 94
  seconds, the native build 9).
  WebAssembly leaves nothing but the bits of a NaN to the runtime, so what
  `wasmi` computes is what a browser computes; Node and `wasmi` were checked
  against each other by hand on the season day.
- **`CLAUDE.md` carries the rule**, added at the merge: the layout rows of
  `maths.rs` and `both_targets.rs`, the two command lines, and one bullet of
  the architecture.

## 2026-10-12 (the owner's decision: an outpost's repair is paid by weight)

- **Decided by the owner**, of the four choices under "outposts measured"
  below: choice 2. A repair is priced in metal, the par value of a quarter of
  the level's cost (537.5, 1343.75 and 3359.375), and the store pays it in
  metal, then crystal at 1.5 a unit, then deuterium at 2
  (`scaling::outpost_repair_payment`). The protected floor is that payment
  read off the store as it stands, and in a raid off the store and the holds
  docked there as one pile.
- **What it was chosen on.** Season pace, 7 days, 30 unattended outposts a
  row. A level-3 outpost alone at rings 18, 19 and 20, emptied before every
  raid, keeps 56, 58 and 54 percent of what its extractor could have made
  (35, 21 and 18 before; 40 to 60 expected). None of 30 ends the week
  breached and waiting where 23 to 29 did. The rows the design calls safe did
  not move, and a world without outposts plays as before. The other branch
  built, an unpaid repair at four times the time (`outpost-repair-slow`, not
  merged), left rings 19 and 20 at 36 and 29 percent and made paying for a
  repair a loss at low levels.
- **What still waits.** An outpost breached before its store has saved up the
  price: 3 of 30 at level 1, ring 9, from an empty store.
- **Not measured.** Persistent and blitz pace, and nobody has played it.
- **Still the owner's.** Whether a repair should interrupt a build.

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

## 2026-10-12 (outposts measured: no constant moved, one rule for the owner)

**Not decided by the owner.** The measurement `docs/design/outposts.md` asks
for in 7.8 was made; its table of results stands there and the journal entry
"outposts measured" has the command lines. Every `OUTPOST_*` constant is
still the design's proposal and still provisional.

- **No constant moved.** Nine of the ten rows read as the design expected or
  cannot be read; the tenth is wrong for a reason no constant reaches.
  - Yield (`OUTPOST_YIELD_PER_HOUR`, 2.0): level 3 on a grade-5 site at rings
    12 to 16 is 4.6 to 4.7 percent of its owner's home output on day 7, and
    the richest site of a world 15 to 19 percent, against 4 to 6 and 16
    expected.
  - Cost (`OUTPOST_COST`): level 1 on a grade-3 site at ring 12 pays back in
    120 game hours. The Metal Mine levels bots buy on day 3 run from 10 to
    16. Against the median of all 45 bots (level 12, 103 hours) the outpost
    is 16 percent slower; against the median of the check-in bots (level
    11, 79 hours) it is 51 percent slower, one point past the half the
    design allows. Left alone: by the end of day 3 all but two bots are
    buying level 12 or higher, and a convoy takes longer than that to get
    there.
  - Platform and raid (`OUTPOST_PLATFORM_POWER`, `OUTPOST_RAID_FACTOR`): no
    raid lost where the design calls the platform safe; level 1 alone at
    ring 9 loses 44 percent; level 3 alone loses 25, 79 and 100 percent at
    rings 18, 19 and 20.
  - A defended outpost lost nothing in a season week, at each garrison the
    design names.
- **The row that is wrong is a rule.** A level-3 outpost left alone at rings
  18 to 20 keeps 11, 4 and 3 percent of what its extractor could have made,
  against 40 to 60 expected and 20 as the floor. A repair costs metal,
  crystal and deuterium; four sites in five lack one of them; a breached
  outpost extracts nothing. So a site that lacks an ore pays for as many
  repairs as it was stocked for and then waits for a delivery: 23 to 26 of
  30 unattended outposts end the week breached and waiting wherever raids
  are lost, and spend half to three quarters of it so. Sites with all three
  ores behave as the design says. `OUTPOST_RAID_LOSS` and
  `OUTPOST_REPAIR_SHARE` were tried at 0.10 and put back: 23 to 24 of 30
  still end the week waiting. The choices, none of them built:
  1. *Keep it.* An undefended outpost that loses raids is dead until its
     owner comes. Then the design's "gives back about half" should read
     "gives back nothing once it has lost twice", and the client should say
     so before the founding.
  2. *The repair is paid by weight.* It costs the score weight of a quarter
     of the level (250 metal, 125 crystal and 50 deuterium weigh 537.5 at
     level 1) in whatever the store holds. Any site that makes anything
     repairs itself. This is the smallest change that gives the design's
     numbers.
  3. *An unpaid repair takes longer.* A store that cannot pay repairs
     anyway, in four times the time. Nothing is ever stuck, and a delivery
     is a speed-up.
  4. *A breached outpost extracts at a quarter rate.* It helps only sites
     with all three ores, which need no help.
- **A breach during a build waits for the build.** Measured: level 2 with
  level 3 building loses the first raid at rings 15 to 17 and then stands
  breached for 6 hours of a season world, 3.6 of them behind the build.
  Whether a repair should interrupt a build is the owner's.
- **`outpost_at` reads an index.** The engine keeps the owner of the outpost
  in each sector beside `outposts` (`GameEngine::outpost_sites`), changed by
  founding and abandoning only. Every sector view asks whether an outpost
  stands there, and with fifteen outposts the search through them was half
  of what outposts cost a 16-seat week. The answers are the same.
- **What the report's outposts are.** `sim-run --outpost` edits the world's
  snapshot. Nothing is founded, carried or paid, the site is the free sector
  of the ring nearest the grade asked, and a lair there is displaced as a
  founding displaces it. No bot supplies, defends or hauls from it.

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
  raids, what they took, yield and ticks breached. What `load` takes from a
  store it reads from `Outpost::hauled`, a counter `load` adds to and no rule
  reads; no bot hauls.

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
