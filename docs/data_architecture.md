# Data architecture: one stored world

How a world is kept between two runs, as built on 2026-10-13. The study that
led here, with its measurements and with the journal of inputs that was
designed and not built, is `docs/design/storage.md`; the decision is in
`docs/design/decisions.md`. The Zig-era text this file used to hold described
a database that no longer exists.

## The rule

**The game runs in memory. What is stored is the whole world, as one
document, the same document for every host.**

```
                 iac-sim (pure)                         a host's place
  GameEngine  --snapshot()-->  Snapshot  --encode-->  bytes  --replace-->  [ document ]
  GameEngine  <--restore()---  Snapshot  <--decode--  bytes  <--read-----  [ document ]
                               store.rs: the document,        server:  a file
                               Store (the place), Due (when)  browser: IndexedDB, behind worker.js
                                                              tests:   memory
```

- `Snapshot` (`sim/src/snapshot.rs`) is the whole world as one serde value.
  **Anything that must survive a restart is a field of `Snapshot`, and it is
  written nowhere else.** There is no schema, no row, no dirty mark.
- The engine knows nothing of storage. It can be copied out (`snapshot`) and
  rebuilt (`restore`), and that is all a host needs of it.
- Clients never see a stored world. They speak the wire protocol to a host
  whose state is in memory.

## The document (`sim/src/store.rs`)

A header line of JSON, a newline, the `Snapshot` as JSON:

```
{"format":6,"economy_version":8,"worldgen_version":3,"digest":<xxh3 of the body>,"host":{...}}
{"world_seed":...,"meta":{"pace":...},"tick":...,"next_id":...,"players":{...},"fleets":{...},...}
```

| field | what |
|---|---|
| `format` | `SAVE_FORMAT`. Bumped when the header or what a snapshot must hold changes. Nothing is migrated: another format is refused |
| `economy_version`, `worldgen_version` | the rules and the generator the world was played under; another of either is refused (`host::Stale`) |
| `digest` | `xxh3` of the body's bytes; a document cut short or altered is refused |
| `host` | what the host that wrote it keeps beside the world. The single-player host: its commander's account, its clock, when it saved, an absence under way and notices not yet read. The server: `{}`. A host reads its own; the server reads a document whatever stands there |
| body | the `Snapshot`. The seed and the pace are the world's, so they are here and not in the header |

`decode` checks in that order and answers with `Unread::Stale` (another
build's world) or `Unread::Damaged` (not a whole document), each worded once
for both hosts; a host adds what its reader can do about it.

It is JSON because the browser already stored JSON, `Snapshot::hash` is
defined over it, and a world can be read with ordinary tools, which is what
replaced SQL:

```sh
tail -n +2 world.iac | jq '.tick, (.players | length)'
```

An amount of a resource, and a fleet's fuel, is an `f64` (format 6), written
as the shortest decimal that names it. Reading one back to the same bits
needs `serde_json`'s `float_roundtrip` feature, which `shared/Cargo.toml`
and `sim/Cargo.toml` turn on: without it about one `f64` in ten comes back
one unit in the last place away, and a restarted world plays on differently
(`sim/tests/stored_world.rs`, `bots.rs` and the last test of
`scenarios_world.rs` fail).

## The place (`Store`)

Two verbs over bytes: `read` (the document last put here, or none) and
`replace` (put this document in place of the one held, whole or not at all).
A place never sees a world and nothing that plays a world sees a place.

| | server (`server/src/store.rs`) | browser (`solo/web/worker.js`) | tests |
|---|---|---|---|
| `read` | `--world <path>` (default `world.iac`) | `get('world')` from IndexedDB, gunzipped | a clone |
| `replace` | write `<path>.tmp`, `fsync`, rename over `<path>`, `fsync` the directory | gzip, one `put('world')` | a swap |
| one writer | `<path>.lock`, held (`flock`) while the server runs; a second server refuses to start | the `navigator.locks` lock of the tab | not needed |
| who calls it | the `Writer` thread; the tick only copies the world out | the worker, when the engine's reply says `save_due` | the test |

In the browser the engine cannot call the place, because WebAssembly cannot
wait on IndexedDB: the worker performs the verbs and the bytes cross the
door (`Machine::call`, `load` and `save`).

## When (`store::Due`)

One rule for both hosts: a document is owed a period after the last one, or
sooner once the host has hurried it, but never sooner than a short gap after
the last, so a burst is one document or two. The times are each host's,
because what a lost document costs differs.

| | period | hurried by | gap | also |
|---|---|---|---|---|
| server | `--save-every`, 300 s | a token issued (a registration or a claim) | 2 s | at every start, once the bots are seated; at SIGINT and SIGTERM |
| single player | 240 s; 20 s while catching up | an order; an absence ending; notices read | 2 s | when the page is hidden |

Why they differ. The single-player host replays the ticks since its save
when it is reloaded, and they come out the same, so a lost save costs only
time and an order is the one thing worth hurrying. The server has no replay:
whatever happened since its last document is gone when it is killed. That
was accepted by the owner on 2026-10-13 ("this is not mission critical
infrastructure"); the one thing hurried is a token, because it is shown once
and a player would be left holding a password to an account the world forgot.

## What a stop costs

| | server | single player |
|---|---|---|
| stopped cleanly (SIGINT, SIGTERM; the tab hidden) | nothing: the file is the world as it stopped | nothing: the world is saved when the tab is hidden |
| killed (SIGKILL, a crash, a power cut; a tab killed) | everything since the last document: at most `--save-every` seconds, orders included; a registration at most about 3 s | no play: the ticks since the save are played again. Orders younger than the save, at most 2 s old |
| killed while writing | the document before: the file is replaced by a rename, never written in place | the save before: one IndexedDB transaction |

There is no journal of inputs. `docs/design/storage.md` (sections 7.2 to 7.6)
is the design for one, should a killed server ever have to lose less.

## A world belongs to its file

The seed and the pace are fixed when a world is created and stored in it.
Starting a server on an existing world with another `--seed` or `--pace` is
refused with a sentence (before, another seed regenerated the galaxy under
its players). So is a file of another format, economy or generator, and one
that fails its checksum; the file is left as it was. A file that does not
exist is a new world.

## Moving a world between hosts

The two hosts store the same document, and `server/tests/hosts_agree.rs`
loads the file a real server wrote into the single-player host, byte for
byte, with nothing added but the header's `host` part. A move is therefore
small, and not built:

- **Server to browser.** The header must name a commander among the accounts
  and give it a clock; every other human account becomes an empire nobody
  plays. The page needs a way to take a file in (there is no import).
- **Browser to server.** A server starts on a browser's save as it is: it
  ignores the `host` part. But nobody holds the commander's token (it was
  derived from the new game's entropy), so the account's hash has to be
  cleared; the first login then claims it, a path the engine has and
  `server/tests/smoke.rs` tests. The page needs a way to give the file out
  (there is no export, and the save in IndexedDB is gzipped).

## Measured

A real `iac-server`, release build, on the dev machine, loopback, no client;
the journal entry of 2026-10-13 has the runs. The first row is a new season
world with 15 bots over five minutes, written every 60 s. The others are
the 24-seat season world of the study at four ages, written by a headless
run and then hosted by the server, a dozen documents each.

| world | document | copied out, on the tick | encoded | written (sync, rename, sync) | a day of documents at 300 s |
|---|---|---|---|---|---|
| 15 bots, first five minutes | 0.26 to 0.33 MB | 0.3 to 0.4 ms | 0.9 to 1.1 ms | 6 to 14 ms | 0.09 GB |
| 24 seats, day 1 | 3.2 MB | 7 ms | 25 ms | 17 ms | 0.9 GB |
| 24 seats, day 2 | 5.3 MB | 13 ms | 33 ms | 21 ms | 1.5 GB |
| 24 seats, day 4 | 8.9 MB | 17 ms | 36 ms | 27 ms | 2.6 GB |
| 24 seats, day 7 | 13.2 MB | 33 ms | 46 ms | 34 ms | 3.8 GB |

- **The tick pays the copy and nothing else**: 33 ms once a period for the
  largest world these bots make, of a budget of 1000. The tick itself took
  0.63 ms on the new world.
- **The copy is two to three times what a tight loop measures.** The study
  timed 13 ms for the day-7 world, and so does the server for the one
  document it writes while it is still warm from loading (12 ms). A server
  that sleeps a second between ticks and frees each copy on the writer's
  thread pays 33.
- **Bytes written are the documents and nothing else.** The new world wrote
  5 KB a tick, where the rows it replaced wrote 136 KB a tick in the same
  setup. But a document is the whole world, so the bytes follow its size
  times how often: at the default period the day-7 world writes 3.8 GB a
  day, under what the rows were measured to write in a world's first three
  days (7 to 10 GB). `--save-every 60` makes it 18.9 GB and the loss after a
  kill a minute; `--save-every 600`, 1.9 GB and ten minutes.
