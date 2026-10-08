# Amber Scout — complete scout run

A playable IAC slice in Numen: one fleet, seeded hex world, CLI + HTTP/1.0.
No SQLite, no WebSocket, no keep-alive, no threads, no floats.

Compiler: numen 0.12.1. Host: aarch64 Linux.

## Files (ownership)

| file | owner this round | notes |
|---|---|---|
| `src/hex.nmn` | done | do not rewrite |
| `src/world.nmn` | done | do not rewrite |
| `src/fleet.nmn` | fleet agent | ships + combat |
| `src/linux_sock.nmn` | Codex | socket/bind/listen/accept |
| `src/http.nmn` | Codex | HTTP/1.0 parse + respond |
| `src/game.nmn` | orchestrator | session + commands |
| `src/page.nmn` | orchestrator | amber HTML |
| `src/main.nmn` | orchestrator | CLI / `--http` |

## Win / lose

Start at hex `(3, -1)`, world seed `1`, two scout ships, fuel `60`.

- **Win:** stand on hub `(0,0)` with `metal+crystal+deuterium >= 40` after at least one combat victory.
- **Lose:** zero ships remaining, or fuel is 0 and the current sector cannot be harvested.

## Commands (same for CLI and HTTP)

| command | meaning |
|---|---|
| `look` | current sector, fleet, cargo, fuel, links |
| `move E\|NE\|NW\|W\|SW\|SE` | traverse an existing edge; costs 1 fuel |
| `scan` | list connected neighbors (terrain + npc) |
| `harvest` | pull resources from current terrain; 3 harvests deplete the sector for this session |
| `fight` | resolve combat against current NPC presence until one side is gone |
| `help` | command list |
| `quit` | exit CLI (HTTP ignores) |

Move is refused in combat (must `fight` or lose). Hub has no NPC.

## HTTP

Listen `127.0.0.1:18080` (`--http`). One request per TCP connection. HTTP/1.0, `Connection: close`. Port 8080 is often taken on this host.

- `GET /` — amber HTML of current session
- `GET /cmd?c=LOOK`
- `GET /cmd?c=MOVE&d=E`
- `GET /cmd?c=SCAN`
- `GET /cmd?c=HARVEST`
- `GET /cmd?c=FIGHT`
- `GET /cmd?c=HELP`

Unknown paths → 404. After `/cmd`, respond 200 with the same page (no redirect required).

## Public APIs the rest of the program will call

### fleet.nmn

```
ShipClass: scout=0, corvette=1, frigate=2, cruiser=3
Ship: class, hull, hull_max, shield, shield_max, weapon  (all integer)
MAX_SHIPS = 8
Fleet: ships: [MAX_SHIPS]Ship, count: u8

scoutShip() Ship
makeShip(class: ShipClass) Ship
spawnNpc(class: ShipClass, count: u8) Fleet   // cap at MAX_SHIPS
npcClassForDistance(dist: u16) ShipClass

RoundResult: player_alive, npc_alive, damage_dealt, damage_taken, concluded, player_won
resolveRound(player: *Fleet, npc: *Fleet, rng: *random.Xoshiro256StarStar) RoundResult
aliveCount(fleet: *const Fleet) u8
salvageMetal(npc_start_count: u8) u32     // 10 * count
salvageCrystal(npc_start_count: u8) u32   // 5 * count
salvageDeut(npc_start_count: u8) u32       // 2 * count
```

Combat is integer-only. Damage = `weapon * (80 + roll0_40) / 100`. Shields absorb first. No rapid-fire in v1. Compact dead ships after each round (`hull == 0` removed).

### linux_sock.nmn

```
htons(port: u16) u16
listenLoopback(port: u16) i64     // AF_INET SOCK_STREAM, SO_REUSEADDR, bind 127.0.0.1:port, listen
acceptClient(listen_fd: u32) i64  // accept, ignore peer
closeFd(fd: u32) i64
recv(fd: u32, buf: []u8) i64
sendAll(fd: u32, buf: []const u8) bool
```

Syscall numbers from `std/linux/syscalls.nmn` via `@intFromEnum`. First-class `asm(.aarch64)` like `std/linux/aarch64.nmn`. Do not edit the stdlib.

### http.nmn

```
Method: get, other
Request: method, path: []const u8, query: []const u8  // slices into caller buffer
parseRequest(buf: []const u8) ?Request   // fail closed; needs request-line
queryGet(query: []const u8, key: []const u8, out: []u8) usize  // bytes written, 0 if missing
writeHttp(fd: u32, status: u16, reason: []const u8, content_type: []const u8, body: []const u8) bool
```

## Numen constraints (read before writing a line)

- `@import("std/foo.nmn")` — always the `.nmn` suffix. Relative: `@import("hex.nmn")`.
- `test "name" { try testing.expect... }` in files **without** `main`. `numen test src/X.nmn`.
- `expectEqual(T, expected, actual)` — T is explicit. Structs: `expect(eql)`.
- Trailing commas on struct types and literals.
- No floats. No saturating `-|`. No `inline fn`. No `std` barrel import.
- Shift amounts: same integer type as the value (`x << @as(u32, 16)` if x is u32).
- Do not `@bitCast` u16↔i16 (known gap). Go through i32/u32 as hex.nmn does.
- Hosted write: `linux_io.FdWriter` + `io.Writer` + `writeAll`. Format: `fmt.formatWriter` or `fmt.format(*WriteCursor, ...)`.
- `pub fn main() u32` or `main(argc, argv: [*]const [*]const u8)`. Integer return is `$?`.
- Methods on structs/enums are fine. `*T` and by-value `@This()` both work.
- Optional `?T`, `if (x) |v|`, `orelse`, `null`.
- Wrapping: `+%` `*%`. Checked `+` in debug traps; release wraps.
- Slice an array as `buf[0..n]`, never pass a raw `[N]T` where `[]T` is required.
- Char literals `'A'` exist. Prefer ASCII in game text.
- `while` if a `for` form is unclear.

Iterate with:

```
numen check src/FILE.nmn
numen test src/FILE.nmn
```

On compiler/stdlib surprises, append a dated entry to `DOGFOOD.md` (command, slug, expected vs actual, workaround, bucket). Do not file `gh issue` from agents; the orchestrator will promote.
