# Storage: one document and one journal for both hosts

Date: 2026-10-13. Status: a study with measurements and a design, for owner
review. Nothing in the engine or in either host was changed when it was written.
It is written against the host core that another branch is moving both hosts onto
(one tick step and one session gate in `iac-sim`); where this text says "the host
core" it means that, whatever it ends up being called.

**Source note.** *Measured* means produced by this repository's code on the dev
machine on 2026-10-13; section 3.1 says how. *Read* means read from the code, with
the file named. *Proposed* is everything else, every constant included.

---

## 1. The answer on one page

**The question.** A world is stored two ways: the browser keeps a whole `Snapshot`
as one JSON document, the server keeps it row by row in SQLite through
`PersistBatch` and dirty tracking. Every persistent field is therefore written
three times. The candidate: both hosts store one document, plus an append-only
journal of what entered the world since that document; recovery is "load the
document, replay the journal through the host core".

**It holds.** The prototype replays 21,600 ticks of a 24-seat world from a
document and a journal of 9,309 inputs, four of them registrations, to the same
64-bit hash the live run ended on; a document written by a loop in the server's
order is played on to identical bytes by the solo host natively and as
WebAssembly. I recommend it, with five amendments (section 6, items 1 to 5), built
in an order that removes the duplication before the journal exists (section 10).

**The numbers it rests on.**

1. **The whole world is small enough to write whole.** The largest world a server
   holds, 24 seats at the end of a season week, is 13.1 MB of JSON (1.1 MB
   compressed), and 23.8 MB if every seat is an explorer flown by hand. Copying
   the 13.1 MB out of the engine takes 13 ms, encoding it 26 ms, decoding it
   54 ms natively; as WebAssembly a save takes 79 ms and a load 278 ms.
2. **Replay is fast enough that the document can be written lazily.** That same
   world replays at 382 ticks a second natively and 269 as WebAssembly, so ten
   minutes of journal are recovered in 1.6 seconds. Younger worlds replay at
   3,000 to 6,000 ticks a second.
3. **The journal is tiny.** An input is 107 bytes on average. A player giving
   orders around the clock at a hands-on bot's rate writes 0.6 MB a day; a quiet
   tick writes about 45 bytes.

**What goes.** About 2,500 lines: `server/src/database.rs` (1,640 lines and 492
of tests), `PersistBatch`, `RuntimeKind`, the `Persist` trait, and 149 lines of
`engine.rs` that mark something dirty, each of which loses state silently on a
restart if it is forgotten. About 450 lines are new. A new persistent field is
then written once, in `Snapshot`.

**What it costs.** Reading a world with SQL. A new obligation in place of the old
one: every new *kind of input* must be a journal line; there are two kinds today.
Not disk writes, which was the surprise: committing every tick as a live server
does, the row-by-row path wrote 6.8, 8.4 and 10.2 GB on the first three days of a
16-seat world, because every player's rows are rewritten every tick and a changed
row costs a whole page. A whole document every ten minutes is 0.13, 0.39 and
0.61 GB on those days, and 1.2 GB on that world's last.

**Where the work could stop, and what that would give up.** The document alone
(option b in section 5) already removes all the duplication. But a killed server
then loses up to a whole save period, where today it loses a tick, and six
end-to-end tests exist to hold that line. The journal is what buys it back.

---

## 2. What exists

```
browser                                   server
  Machine (solo/src)                        main.rs tick loop
    World { engine, roster, clock }           engine.persist_dirty_state()
    save: header line + Snapshot JSON           -> PersistBatch (dirty rows)
  worker.js                                     -> Persister (writer thread)
    gzip -> IndexedDB, one key                  -> SQLite: 15 tables, WAL
    every 240 s, 2 s after an order           start: Database::load_snapshot
    reload: replay ticks since the save              -> Snapshot -> restore
```

| | browser (`iac-solo`) | server (`iac-server`) |
|---|---|---|
| what is stored | the whole `Snapshot`, serde JSON, behind a header line | the same state as rows of 15 tables |
| code | `solo/src/save.rs`, 101 lines | `server/src/database.rs`, 1,640 lines before its tests; `sim/src/persist.rs`, 139; 194 lines of `sim/src/engine.rs` and 16 of `intel.rs` that mention persist or dirty |
| when | every 240 s, 20 s while catching up, 2 s after an order | every tick: a batch of what is dirty, queued to a writer thread |
| after a crash | the ticks since the save are played again and come out the same; an order younger than its save is lost | the batch in flight is lost; a power cut can take back up to 30 s (WAL checkpoint) |
| versions | `SAVE_FORMAT`, `ECONOMY_VERSION`, `WORLDGEN_VERSION` in the header | the last two in `world_meta`; the schema is its own format, moved forward by 27 `ensure_column` calls |

**Counted (read from the code).** 149 lines of `engine.rs` insert into one of ten
dirty sets (`dirty_fleets` 43, `dirty_runtime` 42, `dirty_players` 28,
`dirty_sectors` 16, and six smaller ones). `database.rs` holds 15 `CREATE TABLE`
statements, six pairs of functions that turn an enum into a string or a number
and back, and a float format of its own (the value times 1000 in a REAL column).
`GameEngine::checkpoint` exists because two kinds of row are deliberately not
marked every tick, and a clean shutdown has to queue them by hand.

---

## 3. Measurements

### 3.1 How they were made

A throwaway binary, `storage-probe`, kept in the scratch directory and not in the
repository. It depends on `iac-shared`, `iac-sim` and `iac-solo` by path and
includes `server/src/database.rs` unchanged with `#[path]`, so the SQLite numbers
are those of the code as it stands. It hosts the engine in the server's order
(incoming messages, bots' turns, `tick`, drain events, bots observe) and has five
commands: `world` (a world of built-in bots run to given days, measured at each,
with or without SQLite attached), `journal` (the replay prototype of 3.6), `solo`
(a document loaded, saved and advanced through `iac_solo::Machine`), `fsync`, and
`seed` (9.3). The same requests as `solo` were sent to `iac_solo.wasm` under Node
22.23.1 by a second driver.

Release builds, Rust 1.96.0, each process pinned to one Cortex-A720 core at
`nice 10`, two processes at a time, on the 12-core dev machine (NVMe, ext4).
Times are the median of five runs unless said. World seed `0x1AC05EED20261013`.
Seats are built-in bots in a fixed cycle of hands-on, check-in and hybrid, every
personality in each: 9 seats are 5 hands-on and 4 check-in, 16 are the 15 styles
and one more hands-on, 24 are the 15 styles, 5 more hands-on and 4 more check-in.
No human or LLM played; section 11 says what that leaves open.

### 3.2 The whole world as one document

Native. *Clone* is `GameEngine::snapshot()`, *encode* `serde_json::to_vec`,
*decode* `serde_json::from_slice`, *restore* `GameEngine::restore`. The last column
is the engine's own speed with these bots between the row above and this one.

| world | seats | day | JSON bytes | gzip -6 | zstd -3 | clone ms | encode ms | decode ms | restore ms | ticks/s |
|---|---|---|---|---|---|---|---|---|---|---|
| season | 9 | 1 | 1,523,080 | 127,868 | 145,929 | 0.88 | 3.33 | 6.44 | 0.26 | 11,689 |
| season | 9 | 7 | 5,373,153 | 442,217 | 467,660 | 4.02 | 10.39 | 21.62 | 0.97 | 1,485 |
| season | 16 | 1 | 1,754,515 | 145,732 | 165,444 | 1.07 | 3.83 | 7.37 | 0.28 | 9,596 |
| season | 16 | 7 | 8,587,824 | 708,886 | 722,509 | 9.15 | 17.94 | 34.82 | 2.24 | 1,129 |
| season | 24 | 1 | 3,502,511 | 292,502 | 317,314 | 2.67 | 7.69 | 14.75 | 0.74 | 4,834 |
| season | 24 | 2 | 5,753,644 | 479,124 | 522,425 | 3.59 | 11.05 | 23.97 | 1.07 | 1,927 |
| season | 24 | 4 | 9,039,951 | 767,220 | 793,895 | 6.36 | 16.95 | 36.99 | 2.02 | 976 |
| season | 24 | 7 | 13,127,453 | 1,098,450 | 1,108,837 | 13.48 | 25.88 | 53.67 | 3.41 | 509 |
| persistent | 24 | 7 | 2,495,548 | 210,822 | 236,270 | 1.59 | 4.84 | 10.66 | 0.44 | 8,638 |
| persistent | 24 | 30 | 7,505,141 | 653,116 | 678,252 | 5.61 | 15.28 | 30.81 | 1.61 | 1,445 |
| season, hands-on explorers only | 24 | 1 | 7,674,284 | 596,189 | 501,803 | 6.32 | 14.99 | 32.90 | 1.69 | 3,372 |
| season, hands-on explorers only | 24 | 7 | 23,811,193 | 1,726,442 | 1,810,479 | 29.45 | 54.85 | 99.34 | 7.97 | 564 |

The same documents through the solo host's door (`load`, then `save` five times,
then 1,800 ticks of `advance`, which is a replay with nothing to apply). *Load* is
decode, version checks, digest and restore; *save* is clone, encode and digest.

| document | bytes | native load ms | native save ms | native ticks/s | wasm load ms | wasm save ms | wasm ticks/s | wasm memory MiB |
|---|---|---|---|---|---|---|---|---|
| season, 9 seats, day 1 | 1,523,266 | 13 | 5.9 | 6,087 | 95 | 23 | 1,586 | 12 |
| season, 9 seats, day 7 | 5,373,341 | 55 | 19 | 968 | 177 | 38 | 557 | 39 |
| season, 16 seats, day 1 | 1,754,702 | 13 | 7.1 | 5,139 | 111 | 22 | 1,357 | 14 |
| season, 16 seats, day 7 | 8,588,012 | 55 | 27 | 665 | 222 | 56 | 433 | 64 |
| season, 24 seats, day 1 | 3,502,697 | 43 | 14 | 2,847 | 149 | 29 | 1,103 | 26 |
| season, 24 seats, day 7 | 13,127,640 | 98 | 50 | 382 | 278 | 79 | 269 | 95 |
| persistent, 24 seats, day 30 | 7,505,329 | 64 | 27 | 824 | 210 | 48 | 491 | 52 |

Every row ended on the same digest in three hosts: the probe's own loop in the
server's order, `iac_solo::Machine` natively, and `iac_solo.wasm` under Node. A
document written by one host is played by the other.

### 3.3 What a document is made of, and how it grows

| world | day | chart rows | most for one player | chart bytes | share | fleets (ships) | fleet bytes | sectors touched | their bytes | players | explored and credited | bots |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| season 9 | 1 | 2,578 | 783 | 1,357,256 | 89% | 15 (87) | 15,354 | 217 | 66,259 | 10,533 | 67,667 | 3,299 |
| season 9 | 7 | 8,728 | 1,867 | 4,444,902 | 83% | 39 (1,373) | 184,865 | 1,420 | 433,577 | 11,715 | 291,855 | 3,316 |
| season 16 | 1 | 2,985 | 783 | 1,566,603 | 89% | 23 (104) | 19,892 | 211 | 64,457 | 18,887 | 75,653 | 4,966 |
| season 16 | 7 | 14,341 | 1,898 | 7,341,761 | 85% | 62 (2,185) | 294,456 | 1,464 | 449,362 | 21,634 | 468,576 | 6,517 |
| season 24 | 1 | 6,056 | 783 | 3,188,438 | 91% | 37 (161) | 31,335 | 302 | 92,317 | 27,983 | 147,568 | 8,336 |
| season 24 | 7 | 22,420 | 1,912 | 11,460,167 | 87% | 105 (3,353) | 455,258 | 1,397 | 430,868 | 32,501 | 729,783 | 10,153 |
| persistent 24 | 30 | 12,729 | 1,177 | 6,659,426 | 89% | 42 (897) | 126,762 | 867 | 269,882 | 33,133 | 399,440 | 8,611 |
| season 24, explorers only | 7 | 43,407 | 1,921 | 21,113,385 | 89% | 192 (6,726) | 899,629 | 1,061 | 320,973 | 29,344 | 1,435,174 | 7,683 |

- **It is the charts.** `Snapshot::known`, what each player has seen of each
  sector, is 83 to 91 percent of every document. A row is 508 bytes of JSON.
  Everything a homeworld is (buildings, research, queues, defences, the account)
  is 1.4 KB a player; a ship is 136 bytes; a sector somebody changed is 308.
- **With players** it grows in step: after a season week the document is 0.60,
  0.54 and 0.55 MB a seat at 9, 16 and 24 seats, because each player ends the week
  knowing 900 to 970 sectors. A world has 24 home seats, so 13 MB is about where a
  season of these players ends.
- **With explored sectors** it grows at 508 bytes for every sector every player
  has seen, and nothing bounds that but how far fleets fly. A world of 24
  explorers flown by hand, the most these bots chart, ends its week at 23.8 MB
  with 43,407 rows. In every world measured the widest chart of a season week is
  about 1,900 sectors (1,867, 1,898, 1,912, 1,921), so 24 MB is the ceiling for
  these bots. A human who flies further has no such ceiling.
- **With pace** it does not: a persistent world at day 30 (7.5 MB) is where a
  season world is at day 3 (5.8 MB at day 2, 9.0 at day 4). Size follows how far
  the empires have got, not how many ticks were played.
- **Where it hurts first** is therefore the chart rows, and the first thing to
  hurt is not the disk but the pause: 13 ms to copy the world out of the engine
  at 24 seats and a week (29 ms for the explorers), on the tick's thread, and
  79 ms for a save in the browser's worker. Neither hurts yet. Of a chart row's
  508 bytes, 232 are what the generator gives for that sector unless it was
  mined out (its place twice, terrain, the connections, the ore densities) and
  19 are a tick written twice; storing only what was *seen* would roughly halve
  the document. That is an engine change for the day it is needed, not part of
  this design.

### 3.4 Is JSON the right encoding?

| world | JSON | MessagePack, named fields | gzip -1 | gzip -6 | zstd -3 | zstd -19 | zstd -3 of MessagePack |
|---|---|---|---|---|---|---|---|
| season 24, day 1 | 3,502,511 | 2,428,790 | 548,958 | 292,502 | 317,314 | 130,816 | 171,697 |
| season 24, day 7 | 13,127,453 | 9,085,378 | 2,017,139 | 1,098,450 | 1,108,837 | 455,133 | 675,004 |
| persistent 24, day 30 | 7,505,141 | 5,169,182 | 1,194,004 | 653,116 | 678,252 | 283,051 | 381,233 |

| season 24, day 7, ms | encode | decode |
|---|---|---|
| JSON | 25.9 | 53.7 |
| MessagePack, named fields | 16.4 | 32.1 |
| gzip -1 / gzip -6 of the JSON (flate2, pure Rust) | 33.5 / 239 | 19.1 |
| zstd -3 / zstd -19 of the JSON | 29.1 / 5,526 | 14.2 |
| gzip in Node (`zlib`; near what the worker's `CompressionStream` does) | 165 | not measured |

- **A positional encoding does not read back.** `postcard` and `bincode` write
  the day-7 world in 1.56 and 1.75 MB, an eighth of the JSON, and neither can
  decode what it wrote: the wire types the snapshot is made of use
  `skip_serializing_if` (a field that is not there has no position) and
  internally tagged enums. MessagePack without field names fails the same way
  (`invalid type: integer 2, expected struct NpcFleetInfo`). Making them work
  means a second set of types for storage, which is the duplication this study is
  about removing.
- **A self-describing binary encoding saves 31 percent** and about 40 percent of
  the time. Compressing saves 92 percent. After compression the two differ by
  0.4 MB.
- **Recommendation: JSON, compressed by the place if the place wants to.** It is
  the encoding the browser already stores, `Snapshot::hash` is already defined
  over it, and a document can be read with `jq`, which is what replaces SQL. The
  browser's place already gzips (`worker.js`); the server's need not at first.

### 3.5 The SQLite path as it is today

The same 16-seat and 24-seat worlds, with `Database` and `Persister` attached
exactly as `server/src/main.rs` attaches them. *Written* is every byte the
process passed to `write` (`wchar` of `/proc/self/io`): the WAL and its
checkpoints together. Two ways of running, because the writer thread folds
whatever batches have queued up behind it into one transaction:

- **one transaction a tick**: the run waits for the writer after every tick. This
  is what a live server does, whose writer is idle when each second's batch
  arrives. Slow to measure (2 to 6 ms a tick), so it was run for three days at
  16 seats and two at 24.
- **folded**: the run waits every 200 ticks, so a transaction holds up to 200
  ticks and a page they all changed is written once. No live server does this;
  it is the floor of what the rows cost, and it could be run for a week.

| world | day | document (3.2) | database file | written that day, one transaction a tick | written that day, folded | a ten-minute document that day, about | writer CPU a tick | `load_snapshot` | decode + restore (3.2) |
|---|---|---|---|---|---|---|---|---|---|
| season 16 | 1 | 1,754,515 | 3,084,288 | 6.78 GB | 0.24 GB | 0.13 GB | 2.2 ms | 11.6 ms | 7.7 ms |
| season 16 | 2 | 3,730,116 | 6,459,392 | 8.44 GB | | 0.39 GB | 3.9 ms | 23.3 ms | |
| season 16 | 3 | 4,726,963 | 8,134,656 | 10.15 GB | | 0.61 GB | 5.4 ms | 29.1 ms | |
| season 16 | 7 | 8,587,824 | 14,516,224 | not run | 1.90 GB a day over days 2 to 7; 11.67 GB in the week | 1.2 GB; 5.2 GB in the week | 5.9 ms, mean of the folded week | 53.4 ms | 37.1 ms |
| season 24 | 1 | 3,502,511 | 6,049,792 | 7.81 GB | 0.36 GB | 0.25 GB | 3.4 ms | 22.7 ms | 15.5 ms |
| season 24 | 2 | 5,753,644 | 9,854,976 | 10.12 GB | 0.96 GB | 0.67 GB | 6.3 ms | 35.7 ms | 25.0 ms |

| rows in a tick's batch, mean | players | chart rows | sectors | explored edges | runtime entries | fleets |
|---|---|---|---|---|---|---|
| season 16, day 1 | 16.0 | 1.8 | 2.3 | 0.5 | 0.7 | 0.2 |
| season 16, days 1 to 7 | 16.0 | 31.9 | 9.1 | 0.9 | 0.8 | 0.4 |
| season 24, days 1 to 2 | 24.0 | 9.5 | 4.2 | 1.3 | 1.1 | 0.5 |

- **Every player is rewritten every tick.** The batch carries every player on
  every tick, because production moves their stockpiles, and a player is its row,
  10 building rows, its research and queue rows and 3 defence rows. "Only what
  changed" is, in rows, mostly "everybody", and as the charts grow, 32 chart rows
  a tick besides.
- **Writing only what changed is not writing less. It is writing 15 to 50 times
  more.** Committing every tick, the server's path writes 7 to 10 GB a day for
  a world one to three days old: about 80 KB a tick at 16 seats on the first
  day, because a row that changes costs its whole 4 KB page in the WAL and again
  at the checkpoint. A whole document every ten minutes is 0.13 to 0.67 GB on
  those days and 1.2 GB on the 16-seat world's last. Even folded, which no server does, the
  rows wrote 11.67 GB in that world's week against 5.2 GB of documents (1,008 of
  them, 1.75 MB growing to 8.6 MB).
- **The database is larger than the document**: 1.7 times in every row.
- **It loads slower than the document**: 53 ms against 37 for the 16-seat week.
- After `checkpoint()` and a flush, what `load_snapshot` reads back hashes equal
  to the engine's own snapshot in every row. The two representations do agree
  today; they just have to be kept agreeing by hand.

The "ten-minute document" column is 144 times the mean of the document's size at
the start and the end of that day; nothing wrote those files. The run compresses
wall time, so the writer's own checkpoint every 30 seconds fires far less often
per tick than on a live server; SQLite's automatic one, every 1,000 WAL pages,
fires as it would. A live server therefore writes somewhat more than the
one-transaction column, not less.

**Checked on the server itself.** `iac-server`, release build, season pace,
`--bot all:all` (15 bots), no client, loopback port 8071, for two minutes: from
its 20th second to its 120th it passed 13,560,059 bytes to `write`, 136 KB a
tick, with a database of 479 KB at the end. At that rate a day is 11.7 GB.

### 3.6 Replay, and the journal

The prototype (`storage-probe journal`). Season pace, 24 seats: 16 built-in bots
in the roster and 8 hands-on players *outside* the engine, each a bot driven
through `GameEngine::answer` the way a session's messages are, four of them
registering only after the document was taken. The world runs a day; the document
is written (2,986,559 bytes); then every input is recorded for 21,600 more ticks
as a line `{tick, seat | player + message}`. Recovery is `restore`, `Roster::resume`,
and for each tick: apply the lines of that tick in order, then step.

| | 24 seats: 16 bots, 8 remote hands-on | 24 seats: 24 remote hands-on, no bots | 16 seats: 7 bots, 9 remote (5 hands-on, 4 check-in), document at day 2 |
|---|---|---|---|
| ticks replayed | 21,600 | 21,600 | 21,600 |
| lines (registrations among them) | 9,309 (4) | 25,464 (12) | 6,254 (4) |
| journal bytes (gzip -6) | 982,511 (33,305) | 2,696,695 (99,124) | 666,802 (23,973) |
| hash after replay equals the live run's | yes | yes | yes |
| the same with the 3,039 / 7,085 / 2,511 queries also replayed | yes | yes | yes |
| replay of the document with no journal | differs | differs | differs |
| every order applied one tick late | differs | differs | differs |
| one order dropped, 20 tried: worlds that differ | 2 | 2 | 2 |
| one order one tick late, 20 tried: worlds that differ | 1 | 2 | 2 |
| journal cut 17 bytes short: whole lines left | 9,308 of 9,309 | 25,463 of 25,464 | 6,253 of 6,254 |
| replay ticks/s (live ticks/s) | 3,659 (2,939) | 9,241 (2,688) | 2,302 (1,852) |
| bytes a remote player writes in a day, whole run | 617,024 | 632,436 | 327,958 |
| orders a remote player gives in a day | 5,770 | 5,905 | 3,105 |
| mean and largest line, bytes | 107, 143 | 107, 143 | 106, 232 |

In the third world the check-in players, who look in four times a day, gave 106
and 116 orders a day each (about 12 KB); the hands-on ones 4,500 to 5,700.

- **Replay is exact**, registrations included, and the five kinds of message
  `commands::is_query` names can be left out of the journal: replaying with and
  without them gives the same world. (All five call `&self` functions, read from
  `sim/src/commands.rs`; the prototype exercised three of them.)
- **Which tick an input lands on is part of the input.** Most single orders can
  be dropped or delayed without changing the world, because these bots re-issue
  orders the engine refuses; but some cannot, and all of them late changes it.
- **Replay without bots is three times faster than live** (the middle column),
  because what is replayed for a remote player is its orders, not its thinking.
  A world of humans and agents recovers faster than the worlds of 3.2, which are
  all bots.
- **A torn tail costs one line.**

Writing to the scratch disk (`storage-probe fsync`):

| what | median ms | 95th | worst |
|---|---|---|---|
| append 160 bytes to the journal, no sync (300 times) | 0.01 | 0.03 | 0.16 |
| append 160 bytes and `fdatasync` (300 times) | 1.28 | 1.57 | 11.6 |
| the 13.1 MB document to a temporary file, then rename, no sync (20 times) | 21.4 | 26.8 | 26.8 |
| the same with `fsync` before the rename (20 times) | 27.7 | 82.9 | 82.9 |

---

## 4. Everything that enters a world from outside

A journal must hold all of it or replay diverges. This is the whole list, read
from `server/src/main.rs`, `network.rs`, `auth.rs`, `solo/src/machine.rs`,
`world.rs`, `solo/web/worker.js`, and checked with a search of the three crates
for clock and entropy calls (the only ones in `iac-sim` are seeded `StdRng`).

### 4.1 The server

| what | where it enters | what it changes | a journal needs |
|---|---|---|---|
| `--seed`, `--pace` / `--world` | `Database::load_snapshot` at every start | the world's seed and pace | nothing: they are in the document. They belong to *creating* a world. See 9.3 for what `--seed` does today on an existing one |
| `--bot`, `--bot-sessions`, `--player-seats` | `seat_bots`, once per start, before the first tick | new bot accounts: `enlist_bot(name, record, token)`; the token is from OS entropy, only its hash is kept | nothing, if the server writes a document after seating (proposed, 7.4); otherwise a line `{name, style, token_hash}` per bot |
| a session's `auth` for a new name | `Network::log_in` -> `GameEngine::authenticate` | a player, a fleet, two ids, the `agent` flag, the hash of a token minted from OS entropy | a line `seat {name, agent, token_hash}`: the hash, never the token |
| `auth` for an account that has no hash yet | same | the hash | the same line |
| `auth` with the right token, a refused `auth`, `auth` from a session already logged in | same | nothing | nothing |
| `command` | `Network::answer` -> `GameEngine::answer` | whatever the order does. Nothing guarantees that a refused order leaves the world untouched, so refused orders are journaled too | a line `{player, msg}` unless `is_query(cmd)` |
| `policy_update` | same | the fleet's standing orders | a line `{player, msg}` |
| `request_full_state`, the five queries | same | nothing (`&self`) | nothing |
| a message from a session not logged in; a frame that does not parse; a message whose session has gone | refused or dropped before the engine | nothing | nothing; so the line is written where the input is *applied*, not where it is received |
| the order of messages from different sessions | the one `mpsc` queue, drained at the top of `run_tick` | the order they are applied in | the order of the lines |
| the wall clock | `tokio::time::interval(1 s)` in `main.rs`; nowhere else | when a tick happens, and so *which tick each message lands on*. No time value reaches the engine (`sim/clippy.toml` bans clock reads). A slow tick delays the next; missed ticks are not made up | the tick on every line; and how far the world got after its last input (amendment 2, section 6) |
| OS entropy | `server/src/auth.rs::generate_token`, for the three cases above | token hashes | the hash on the line |
| connect, disconnect | the session table in `network.rs` | nothing in the engine | nothing |
| a tick that returns an error | `run_tick` logs it and skips the broadcast, the bots' observation and the persist of that tick | deterministic, given one step function | nothing; but the error path is part of the step's meaning and must be the host core's, not each host's |
| SIGINT, SIGTERM | `main.rs` | nothing; triggers the last write | nothing |
| someone editing the database while the server is down | `sqlite3` | anything | out of scope for replay; see "what is lost", 8.3 |
| the binary | | every rule | the stamp and the pulse (amendment 5) |

**Which tick does a message land on, and is that recorded?** On the tick the
engine stands at when `process_incoming` drains the queue: after it, the bots take
their turns and `tick()` moves the world to the next. It is recorded nowhere
today, neither the tick nor the message: only the state that resulted.

### 4.2 The solo host

| what | where it enters | what it changes | a journal needs |
|---|---|---|---|
| `new_game {name, pace, bots, entropy}` | `machine.rs::new_game` | the seed (`xxh3` of the entropy), the commander, the bots, their token hashes | nothing: the world is saved as soon as it exists |
| a client frame: `command`, `policy_update` | `World::receive` -> `GameEngine::answer` | the world | a line `{player, msg}` unless a query. Today this is what forces a whole save within 2 s |
| a client frame: `auth` | `World::log_in` | nothing in the engine; the session flag | nothing |
| `now_ms` on every request | `World::advance`, `take_command`, `status` | how many ticks are played by when: the clock's anchor, the 7-day limit, a catch-up cut short. Never a value inside the world | nothing; it is the bookmark's |
| `advance {max_ticks}`, the worker's chunking and pump | `worker.js` | pacing only | nothing |
| `open`, `close`, `abandon` | `Machine::run` | host state | nothing |
| the absence report | `away.rs`, from events of the world | the bookmark (`catch_up`, `notices`); the notices go to the session, never into the engine | nothing |

The solo host has one kind of journal line and writes it for one account. That
its world takes nothing else from outside is already a rule (CLAUDE.md,
architecture) and already tested (`a_reload_plays_the_unsaved_ticks_again_and_arrives_at_the_same_world`).

---

## 5. The options, compared

- **(a)** SQLite rows, as today.
- **(b)** The server writes the whole document on a period; no journal.
- **(c)** The candidate: the document, lazily, and a journal of inputs.
- **(d)** Not asked for, the cautious one: keep rows and dirty tracking but store
  every entity the way nine kinds already are, as `(kind, id, JSON)` in
  `runtime_state`. The schema and the column mapping go; the marking stays.

| | (a) rows | (d) generic rows | (b) document on a period | (c) document and journal |
|---|---|---|---|---|
| a new persistent field is written | three times | once; a new *map* twice (the kind, its marks) | once | once |
| a new kind of input is written | nowhere | nowhere | nowhere | once, as a line kind |
| what forgetting costs | a missing dirty mark loses state on restart, silently | the same | nothing can be forgotten | an input not journaled makes replay diverge; the pulse reports it |
| a killed process loses | the tick in flight | the same | up to a period of everything, orders and registrations included | the tick in flight |
| a power cut loses | up to 30 s | up to 30 s | up to a period | up to the journal's sync interval (5 s proposed) |
| bytes written a day, 16 seats | 6.8, 8.4, 10.2 GB on days 1 to 3, measured | not measured; the same rows, as JSON | 144 documents at a 10-minute period: 0.13, 0.39, 0.61 GB on days 1 to 3 and 1.2 GB on day 7; twenty times that at a 30 s period | the same documents, plus at most 19 MB of journal |
| restart, 16 seats, day 7 | read the rows: 53 ms | about the same | decode and restore: 37 ms | the same, then at most 600 ticks: 0.9 s (1.6 s at 24 seats) |
| browser and server hold the same bytes | no | no | yes | yes |
| readable with SQL | yes | the keys; the rest is JSON | no; `jq` | no; `jq` |
| the six kill-and-restart tests in `server/tests` | pass | pass | need a period of one tick to pass | pass as they are |
| lines, roughly | 0 | -1,000 | -2,500, +250 | -2,500, +450 |

**(b) against (c).** They are the same design up to the journal, and (b) is a
stage on the way to (c) (section 10, step 3). (b) is honest only at a short
period, and a short period is expensive: to match today's power-cut bound of 30 s
a 13 MB world is written 2,880 times a day. (c) writes the same document every
ten minutes and loses less.

**(d) against (c).** (d) keeps what (c) loses (tick-by-tick durability without a
journal, rows one can list) and gets rid of the schema, which is most of
`database.rs`. It does not touch the part that fails silently, the 149 marking
lines, and it does not make the two hosts store the same thing. If the owner
decides a world must stay readable with SQL, (d) is the fallback I would take.

---

## 6. Where the candidate is wrong, or weaker than assumed

1. **Ticks do not tie a journal to its document.** In the solo host a save can be
   taken after an order was applied and before the next tick, so "replay the
   entries from the document's tick on" would apply that order twice. Lines need a
   running number, and the document must say which number it already contains.
2. **"The tick it landed on and the input" does not say how far the world got.**
   A server that ran idle for four minutes after its last order and was then
   killed would come back four minutes in the past. Nothing would be lost or
   differ, the same things would happen again, but timers would jump back. The
   server therefore also writes one short line per completed tick. The solo host
   does not need it: its clock says what is owed.
3. **The journal cannot hold what `authenticate` takes.** A registration draws a
   token from OS entropy inside the call. The journal must not hold the token (it
   is the password) and replay cannot draw the same one. The engine's entry point
   has to be split: the host mints and hashes, the engine is handed the hash.
4. **A written-ahead input can be poison.** If an order makes the engine panic,
   a journal that already holds it kills every restart at the same line. Today
   the order is never stored, because the batch is made after the tick. Lines
   must reach the place only after the step that consumed them has completed.
5. **A journal is only true for the binary that wrote it.** A rule changed
   between the crash and the restart replays the same orders to a different
   world, without any error. Three things keep that small: a clean shutdown and
   every start write a document, so a journal is only ever replayed after a
   crash; the document period bounds what can be re-decided to ten minutes; and
   each tick line carries a cheap digest of the fast-moving state (the *pulse*),
   so replay *knows* when it has parted from what was played and can refuse.
6. **In the browser "one storage interface" cannot be a call the engine makes.**
   IndexedDB is asynchronous and WebAssembly cannot wait on it. The same three
   verbs exist in both hosts, but in the browser the worker performs them and the
   engine hands it bytes through the door it already has.
7. **The duplication is removed by the document, not by the journal.** All of
   "`database.rs`'s row mapping, `PersistBatch`, `RuntimeKind` and the dirty
   tracking go" and "a new persistent field is written once" is true of option
   (b) already. What the journal adds is durability, and for the solo host an
   order that costs a 100-byte write instead of a whole save.

Smaller: the journal grows with what clients send, not with what the world is,
and the server has no limit on messages per session; a size limit that forces a
document bounds it without a protocol change (7.4). Two processes on one world
were kept apart by SQLite's lock by accident; files need a lock of their own.

---

## 7. The design

### 7.1 The document

What `solo/src/save.rs` writes today, moved beside the host core and given two
more header fields. One line of JSON, a newline, the `Snapshot` as JSON.

```
{"format":5,"economy_version":5,"worldgen_version":3,"digest":<xxh3 of the body>,
 "journal":<number of the last journal line this body already contains>,
 "host":{ ...what this host keeps beside the world... }}
{"world_seed":...,"meta":{...},"tick":...,"players":{...}, ... }
```

- `host` is the solo host's bookmark as it is today (`commander`, `clock`,
  `saved_at_ms`, `catch_up`, `notices`) and empty for the server. A host ignores
  another host's.
- The three refusals (format, economy, worldgen) and the digest check are worded
  once, for both hosts. The pace and the seed are in the body.
- `Snapshot` itself does not change. `RuntimeEntry` and `insert_runtime` go with
  the rows they served; `WorldMeta`, `ExploreCredit` and `KnownRow` move from
  `persist.rs` to `snapshot.rs`.

### 7.2 The journal

Lines of text. Each is a JSON object, a tab, and the `xxh3` of the object's bytes
in 16 hex digits.

```
{"n":41,"tick":86400,"seat":{"name":"Vega","agent":true,"token_hash":"9f1c..."}}	<sum>
{"n":42,"tick":86400,"player":7,"msg":{"type":"command","action":"move","fleet_id":12,"target":{"q":1,"r":2}}}	<sum>
{"n":43,"tick":86401,"pulse":"5d0e..."}	<sum>
```

- `n` counts up by one from the first line the world ever wrote.
- `tick` on an input is the tick the world stood at when it was applied. `msg` is
  the client's message as it came off the wire, so a new command needs no new
  line kind.
- A line with only `pulse` says the world completed that tick, and what
  `GameEngine::pulse()` read then: a digest of the tick, the id counter, each
  player's stockpile and points and each fleet's place, fuel and ships. It is
  proposed, about 15 lines, and costs microseconds; the full hash would cost the
  26 ms of an encode on every tick.
- The server writes a pulse line for every tick; the solo host writes none.
- Measured line sizes: 107 bytes mean, 143 at most, before `n` and the sum; a
  pulse line is about 45.

### 7.3 Recovery, the same in both hosts

```
recover(document, journal) -> host, report
  check format, versions and digest; restore the engine; resume the roster
  for each line, in order:
      sum wrong or n not the last n + 1  -> the journal ends here (7.6)
      n <= document.journal              -> skip: the body already contains it
      line.tick < world tick             -> refuse: not this document's journal
      step, with no session, until the world stands at line.tick
      seat    -> engine.seat(name, agent, token_hash)
      message -> the gate's answer(player, msg); replies dropped
      pulse   -> compare; a difference is reported with its tick (7.5)
```

The step is the host core's one step, the answer its one gate: there is no
second implementation of either for replay. In the prototype the whole of
recovery is 40 lines.

**The solo host's catch-up is this mechanism.** Today a reload replays the ticks
since the save because the clock says they are owed. With a journal the same loop
applies the lines that fall on those ticks. After the last line the clock decides
what is still owed, exactly as now, and the absence report (`away.rs`) counts
what it counts now. `ORDER_SAVE_MS` goes: an order is an append.

### 7.4 The storage interface

Three verbs over bytes. The host core never sees a place; a place never sees
structure.

```rust
/// Where a world's bytes are kept. Defined beside the host core.
pub trait Store {
    /// The document last put here and the journal appended since; None for a
    /// place that holds no world.
    fn read(&mut self) -> Result<Option<Stored>, String>;
    /// Add whole lines to the end of the journal.
    fn append(&mut self, lines: &[u8]) -> Result<(), String>;
    /// Put this document in place of the one held and empty the journal, as
    /// one step or in that order.
    fn replace(&mut self, document: &[u8]) -> Result<(), String>;
}

pub struct Stored {
    pub document: Vec<u8>,
    pub journal: Vec<u8>,
}
```

And three calls on the host core that produce and consume those bytes:
`Host::recover(document, journal)` (7.3), `Host::take_journal()` (the lines
buffered since the last call) and `Host::document(host_part)` (7.1; it sets the
header's `journal` to the last `n` handed out).

| verb | server | browser | tests |
|---|---|---|---|
| `read` | `world.iac` and `world.iac.journal` beside it | `get('world')`, `getAll` of the `journal` store | a clone |
| `append` | `write` to the open journal, once per tick, on the writer thread; `fdatasync` every 5 s | one `add` per line in the `journal` store | push |
| `replace` | temporary file, `fsync`, rename, `fsync` of the directory, then truncate the journal | one transaction: `put('world')`, `clear('journal')` | swap |
| one writer per world | `flock` on the journal; a second server refuses to start | the `navigator.locks` lock the worker already takes | not needed |

In the server the trait is implemented by about 100 lines over two files and
called from a writer thread like today's, so the tick never waits on the disk.
In the browser the verbs are performed by `worker.js`, which already performs
`read` and `replace`: the `Machine`'s reply line gains the lines to append, and
`load` takes the journal after the document. A crash between the rename and the
truncation is harmless: the lines left behind are numbered at or below the
header's `journal` and are skipped.

**When each host calls them (proposed constants).**

```
server, each second                         solo, as the worker pumps
  gate: each queued message                   frame: gate -> reply carries its lines
  host.step()                                        -> worker appends them
  store.append(host.take_journal())           advance: steps, no lines
  every 600 ticks, or 4 MB of journal:        save_due (240 s; page hidden;
      store.replace(host.document())            a catch-up arrived):
start:    read, recover, seat bots, replace       op save -> worker replaces
shutdown: replace
```

- The document is cloned on the tick's thread (13 ms for the mixed 24-seat world
  at its end, 29 ms for the explorers) and encoded and written on the writer's.
- Lines are handed to the place only after the step that used them (amendment 4).
- Writing a document at every start means a journal is never carried from one
  run into the next, and bots seated at a start need no journal line.
- 600 ticks: a restart replays at most ten minutes, 1.6 s for the heaviest world
  measured. 4 MB: about 37,000 inputs; a flood of messages forces a document
  instead of growing the journal.

### 7.5 A journal from another binary

The header's `economy_version` and `worldgen_version` refuse a document from
another economy or generator, as today, and the journal with it. Below that
level nothing is stamped, because nothing short of the binary's hash would be
right; the pulse decides instead. If replay reaches a pulse line and reads a
different pulse, it stops and says at which tick. Proposed: the server then
refuses to start and names two flags, one to carry on from the replayed world
(the orders are kept, the last minutes may have gone differently from what
players saw) and one to start from the document alone (the journal is moved
aside, not deleted). A crash followed by a restart of the same binary never gets
there. The solo host, which has no operator, carries on from the replayed world.

### 7.6 A damaged journal

| what is found | what it is | what happens |
|---|---|---|
| the last line incomplete or its sum wrong, nothing after it | a write the crash interrupted | dropped; the journal is cut there. One tick's inputs at most |
| a bad line or a gap in `n` with good lines after it | not a crash | server: refuses to start, names the line and the flag that starts from the document. Solo: applies what came before, drops the rest, tells the commander in a notice |
| first line numbered above the header's `journal` + 1 | a journal without its document | refused, both hosts |
| every line at or below the header's `journal` | a crash between rename and truncate | all skipped; nothing to do |
| replay panics | a poison line that reached the place after all | server: the same flag. This is the case amendment 4 makes rare, not impossible |

### 7.7 Moving a world between hosts

It follows from the document, not from the journal, and it was checked as far as
the engine goes (3.2: three hosts, one digest). What a real move still needs:

- **Browser to server.** The commander's token is derived from the new game's
  entropy and nobody has it. An import clears that account's hash, and the first
  login claims it, which is a path the engine already has for accounts without a
  hash. The solo `host` part is ignored. The bots carry on from their records.
- **Server to browser.** The header must name a commander among the accounts;
  every other human account becomes an empire nobody plays.

Neither is needed for the unification. Whether it is wanted is question 7.

---

## 8. What goes, what is new, what is lost

### 8.1 Goes

| | lines |
|---|---|
| `server/src/database.rs`: schema, 27 column migrations, row mapping, enum strings, float format, writer thread | 1,640 |
| its in-file tests | 492 |
| `sim/src/persist.rs`: `Persist`, `PersistBatch`, `RuntimeKind`, `RuntimeEntry`, `RuntimeRow`, `FleetClocks`, `ExploredEdge` | about 105 of 139 |
| `sim/src/engine.rs`: ten dirty sets, 149 marking lines, `persist_dirty_state`, `runtime_entry`, `checkpoint`, `flush_persistence`, `attach_persist` | about 195 |
| `sim/src/intel.rs`: its dirty set, `dirty_rows`, `clear_dirty`, `touch_live` | about 25 |
| `sim/src/snapshot.rs`: `insert_runtime` | 32 |
| the `rusqlite` dependency of the server, with the bundled SQLite it compiles | |

Concepts that go: a batch; a dirty mark; a runtime kind; a schema; a column
migration; a second float format; a second set of enum names; a checkpoint that
queues what is otherwise skipped; a writer that retries a failed transaction.

### 8.2 New

| | lines, estimated |
|---|---|
| the document beside the host core (moved from `solo/src/save.rs`, two header fields) | net +20 |
| the journal: line types, framing, sums, `take_journal` | 90 |
| `recover` | 50 |
| `GameEngine::seat` split out of `authenticate`; `pulse` | 30 |
| `Store` and the file place | 130 |
| the server's `main.rs`: when to append, when to replace, the flags | 60 |
| `machine.rs`, `worker.js`: lines out, journal in, a second object store | 80 |

Concepts that are new: a journal line and its number; a pulse; a place with three
verbs. The document exists already.

### 8.3 Lost

- **Reading a world with SQL**, and reading it while the server runs (WAL allows
  that today). In its place: the document is JSON after its first line, so
  `tail -n +2 world.iac | jq '.players'`; it is as old as the last document, up
  to ten minutes, where the database is as old as the last tick.
- **Editing a world by hand** needs the digest restamped. A small flag that
  rewrites the header of an edited document is the least tool that would do.
- **Writing only what changed** is lost as an idea and not as a saving: section
  3.5 has both in bytes, and the documents are the smaller sum.
- **The in-process SQLite restart tests** (below).
- **Old databases.** They are not migrated, as the house rule and the server's
  own refusal sentence already say; question 4 asks whether one is owed an
  importer.

### 8.4 Tests

**Go** (to `.archive/`, with `database.rs`): its six in-file tests.
`a_world_keeps_the_pace_it_was_created_with`,
`a_database_without_world_meta_is_refused` and
`a_world_from_another_generator_is_refused` become end-to-end cases beside
`a_world_refuses_a_different_pace` in `server/tests/smoke.rs`.
`a_restart_through_sqlite_continues_the_same_world` and
`a_world_with_bots_restarts_through_sqlite_as_if_it_had_never_stopped` test what
`snapshot_and_restore_midway_continues_identically` (`sim/tests/determinism.rs`)
and `a_world_of_bots_resumed_from_a_snapshot_plays_on_as_if_never_stopped`
(`sim/tests/bots.rs`) already test without SQLite.
`a_world_saved_before_jumps_and_combats_were_stored_still_loads` tests a
migration that no longer exists.

**Change.**

- `server/tests/smoke.rs::token_survives_restart_and_pre_token_databases_are_claimed`
  opens the database with `rusqlite` and drops a column to make an old world. The
  restart half stays; the old-world half goes with the migrations.
- The six kill-and-restart scenarios of `server/tests/scenarios.rs` and
  `server/tests/common/mod.rs` (`fresh_db`, `--db`): the flag and the file names.
  They are the journal's end-to-end tests as they stand.
- `sim/tests/exchange.rs`: two cases read what was *written* through a `Capture`
  of `PersistBatch`. They become what was *restored*.
- Four `sim/tests` files and `sim/src/headless.rs` call `persist_dirty_state()`
  on an engine with nothing attached; the line goes.
- `solo/tests/single_player.rs::a_save_this_build_cannot_read_is_refused_with_the_reason`
  (the header), and `a_reload_plays_the_unsaved_ticks_again...`, which gains an
  order given after the save.

**Added**, all end to end:

- `sim/tests`: a world of bots and scripted sessions, one registering late,
  recovered from its document and journal, is the world that ran (the prototype
  of 3.6 as a test).
- `sim/tests`: a journal cut short loses its last line and no more; one damaged
  in the middle, one without its document and one from another economy are each
  refused with their sentence.
- `sim/tests`: each of the five queries leaves the world's hash as it was, so
  that `is_query` cannot quietly stop being true.
- `solo/tests/single_player.rs`: an order given after the last save survives a
  reload, through `Machine::call`.
- `solo/tests/both_targets.rs`: a document and journal written natively are
  recovered by the wasm engine to the same bytes.
- `server/tests`: a server killed between two documents comes back at the tick
  it was killed on (the existing scenarios, at the default period); a server
  started on a save written by the solo host plays it, if question 7 is yes.

---

## 9. Stored twice, met on the way

1. **Version checks**, with nearly the same three sentences, in
   `solo/src/save.rs::decode` and `server/src/database.rs::open_world`. One place
   after step 1.
2. **The tick and the id counter**: in `Snapshot`, and as the `current_tick` and
   `next_id` rows of `server_state`.
3. **The world seed**: in `Snapshot`, in `server_state`, and on the command line.
   *Measured*: a database created under seed `0xAAAA` and loaded with
   `--seed 0xBBBB` comes back as a world of seed `0xBBBB`, with one warning in
   the log and the stored seed overwritten; the sector beside the homeworld
   changed from a debris field to an asteroid field. A server restarted without
   its `--seed` regenerates the galaxy under its players. It is independent of
   everything else here and should be refused the way a different `--pace` is.
4. **Pace and versions**: `Snapshot::meta`, the `world_meta` table, the save's
   header.
5. **The digest**: `Snapshot::hash` and the header's `digest` are the same `xxh3`
   of the same bytes, computed in two places.
6. **The solo bookmark against `server_state`.** They overlap in nothing: the
   bookmark is the commander, the clock and the absence; `server_state` is 2 and
   3 above. With the document, the bookmark is the header's `host` part and
   `server_state` is gone.
7. **Explored edges.** The database stores every edge a fleet crossed with the
   tick it was found (11,122 rows after a 16-seat day); the snapshot stores the
   sectors entered (2,047). Nothing reads the edges back except as sectors.
8. **Seating bots**: `seat_bots` in `main.rs` and `World::create` in the solo
   host. The host core's, presumably, on the other branch.
9. **Hex of a hash**: the same closure in `server/src/auth.rs`, `machine.rs` and
   wherever a journal would print one.
10. **`persist_dirty_state()` with nothing attached**: the solo host and the
    headless runner clear ten dirty sets every tick for nobody.
11. **Events awaiting broadcast** are in `Snapshot::pending_events` and not in
    the database. Both hosts drain them before they store, so both are right;
    the document makes it one answer.

Not storage, but seen: the solo host's catch-up of a week on a mature 24-seat
world would take at least 37 minutes in the browser at the 269 ticks a second
measured in 3.2. The earlier figure of 3,600 was a young world.

---

## 10. Migration, every step green

Step 0 is the other branch: both hosts on one host core.

1. **The document moves beside the host core.** `solo/src/save.rs` becomes the
   core's; the header gains `journal` (always 0 for now) and `host`. The solo
   host uses it and nothing else changes. `SAVE_FORMAT` 4 to 5, unless question
   9 keeps it. *Green:* the solo tests, with the refusal test reworded.
2. **`Store`, with the file place and a memory place.** Unused by the hosts yet.
   *Green:* a `sim/tests` case that writes and reads a document through both.
3. **The server stores documents.** `main.rs` reads through the file place and
   replaces on a period and at shutdown and start; `--db` becomes `--world`;
   `database.rs` and its tests move to `.archive/`; `rusqlite` leaves
   `server/Cargo.toml`; the `--seed` refusal of 9.3. The kill-and-restart
   scenarios run their servers with a period of one tick, which is honest: at
   this step that is what it takes. *This is option (b), complete.* *Green:*
   `cargo test --workspace`.
4. **The engine forgets rows.** `Persist`, `PersistBatch`, `RuntimeKind` and the
   rest of 8.1 go; `persist.rs` shrinks to nothing and its three surviving types
   move to `snapshot.rs`; the `Capture` cases of `exchange.rs` are rewritten;
   the stray `persist_dirty_state()` calls go. *Green:* workspace, clippy,
   `both_targets`. CLAUDE.md's sentence "anything that must survive a restart
   goes in `PersistBatch` and the SQLite schema as well as in `Snapshot`" becomes
   "goes in `Snapshot`".
5. **The journal in the core.** `GameEngine::seat` and `pulse`; the gate and the
   step buffer lines; `recover` replays them. No host appends yet. *Green:* the
   new `sim/tests` cases of 8.4.
6. **The server appends.** Lines after each step, the document every 600 ticks,
   `flock`, the two flags of 7.5. The scenarios drop their one-tick period.
   *Green:* `cargo test --workspace`; the kill scenarios are the proof.
7. **The solo host appends.** The reply line carries lines, `load` takes a
   journal, `worker.js` gets the `journal` store, `ORDER_SAVE_MS` goes. *Green:*
   the solo tests, `both_targets`, `flutter test`, `solo_e2e.dart`.
8. **Docs.** `docs/data_architecture.md` describes the Zig-era database and is
   replaced by a pointer here; README's server flags; CLAUDE.md's layout rows.

Steps 3 and 4 are listed apart because each is green by itself, but by the house
rule (migrate every caller and remove the old path in the same change) they are
one merge: after step 3 alone the engine still tracks dirt for nobody. Steps 1 to
4 can stand as the end if question 1 is answered that way. Steps 5 to 7 need
nothing from 1 to 4 to be redone.

---

## 11. Not measured

- **People.** Every world here is built-in bots. Their chart sizes and order
  rates stand in for players'; an LLM agent or a human may chart more and will
  almost certainly order less. No command rates of the recorded playtests exist
  to compare with.
- **Beyond a season week and beyond persistent day 30.** A run to season day 28
  and persistent day 60 was started and stopped for time; the persistent day 30
  falling where a season day 3 does is the ground for expecting a persistent
  world's end (day 74 by the preset) near the season week's 13 MB.
- **A real server under the design.** Nothing was built into `iac-server`; the
  loop measured is the probe's, in the server's order, without sockets.
- **A browser.** WebAssembly was measured under Node, not Chromium, and
  IndexedDB appends not at all.
- **`exchange_quote` and `outpost_survey`** among the queries: pure by their
  signatures, not exercised by the prototype, whose bots ask the other three.
- **A power cut.** The `fsync` times are this NVMe's on a quiet moment.
  `database.rs` records that fsyncs on this machine have stalled for seconds,
  which is why the design syncs on the writer's thread.
- **The pulse.** It is proposed, not prototyped; its cost is an estimate.
- **Replay across two builds.** Not tried. Section 7.5 is reasoning.

---

## 12. Questions for the owner

1. **May a killed server lose more than a tick?** If up to a save period is
   acceptable, the work can stop after step 4 and the journal is never built.
   I recommend building it: the six kill-and-restart scenarios say what the
   server promises today, and the journal is also what makes a browser order
   cost 100 bytes instead of a save.
2. **Files, or two tables in SQLite, as the server's place?** The interface
   hides it. Files: no C dependency, the same bytes as a browser save, about 100
   lines that must get rename and sync right. SQLite: atomic for free, one `.db`,
   the `sqlite3` tool still opens it (to find one JSON blob and a table of
   lines). I recommend files.
3. **Must a world stay readable with SQL?** If yes, the answer is option (d),
   not this design.
4. **Is any existing database owed a way across?** The design migrates none. A
   one-off importer is `load_snapshot` and one `replace`, and could live outside
   the workspace.
5. **Ten minutes and five seconds?** A document every 600 ticks bounds replay
   and what a new binary can re-decide; a journal sync every 5 s bounds what a
   power cut takes (30 s today). Both are provisional.
6. **A journal that replays differently under a new binary:** refuse and ask
   (proposed), carry on, or drop the journal?
7. **Is moving a world between browser and server wanted as a feature?** It
   costs an import flag and an export button beyond this design. It is not
   needed for one source of truth.
8. **Should a server's world owe ticks for the time the server was down, as the
   solo world does?** Not proposed: it pauses today and the tick lines keep it
   so. It becomes possible, which it was not.
9. **`SAVE_FORMAT` 4 to 5 refuses every browser save once more.** The header
   change can instead be made additive (`journal` defaulting to 0, the solo
   fields left where they are), at the price of a header that is less tidy.
10. **The `--seed` flag on an existing world** (9.3): refuse a different seed
    now, ahead of everything else here?
11. **Compress on the server?** Proposed no: 1.9 GB a day for the 24-seat world
    at the end of its week (3.4 GB for the explorers), a fifth of what SQLite
    writes today on that world's second day, and a file `jq` reads. zstd would
    make it 160 MB a day for 29 ms a document and a C dependency.
