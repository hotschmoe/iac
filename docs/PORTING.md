# In Amber Clad — Rust Port Plan

> Historical. The crates now live at `shared/`, `server/` and `clients/tui/` (crate names unchanged).

**Source:** `the archive/zig branch` (Zig 0.15.2)
**Target:** Rust (stable)

## Crate Stack

| Zig dep     | Rust crate              | Role                        |
|-------------|-------------------------|-----------------------------|
| `zqlite`    | `rusqlite`              | SQLite persistence          |
| `webzocket` | `tokio` + `tokio-tungstenite` | WebSocket server/client |
| `zithril`   | `ratatui`               | TUI rendering               |
| (JSON)      | `serde` + `serde_json`  | Serialization               |
| (hash)      | `xxhash-rust`           | Wyhash replacement          |
| (CLI)       | `clap`                  | Argument parsing            |

Rationale:
- `rusqlite` is the mature, well-maintained SQLite binding. No need to reinvent.
- `tokio-tungstenite` is the standard async WebSocket stack. The Zig server uses a threaded model; we'll use tokio's async runtime which maps naturally.
- `ratatui` is the de facto Rust TUI framework (successor to tui-rs). Replaces zithril/rich_zig.
- `serde` is the standard serialization ecosystem.
- `xxhash-rust` replaces Zig's `Wyhash` — same role (fast deterministic hash), widely used.
- `clap` for CLI arg parsing.

## Workspace Layout

```
in-amber-clad-rust/
├── Cargo.toml              # workspace root
├── iac-shared/             # shared domain (server + client)
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # re-exports
│       ├── hex.rs          # axial coords, distance, iterators
│       ├── constants.rs    # zones, terrain, ships, resources
│       ├── protocol.rs     # wire protocol, state, events
│       ├── world.rs        # procedural generation
│       └── scaling.rs      # buildings, research, modifiers
├── iac-server/             # authoritative game server
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs         # entry, tick loop, signals
│       ├── engine.rs       # tick processing, movement, NPC
│       ├── combat.rs       # round-based combat resolution
│       ├── network.rs      # WebSocket server, routing
│       └── database.rs     # SQLite persistence
└── iac-client/             # amber TUI
    ├── Cargo.toml
    └── src/
        ├── main.rs         # entry, app loop
        ├── connection.rs   # WebSocket client
        ├── input.rs        # key mapping
        ├── state.rs        # client state
        └── renderer.rs     # TUI rendering
```

## Porting Phases

### Phase 1: Foundation — `iac-shared`

Pure data and logic. No I/O, no async. Easiest to port and test first.

1. **hex.rs** — `Hex` struct, axial math, distance, neighbors, ring/spiral iterators, `HexDirection`.
2. **constants.rs** — `Zone`, `Density`, `TerrainType`, `ShipClass`, `ShipStats`, `Resources`, all game constants.
3. **protocol.rs** — `ClientMessage`, `ServerMessage`, state types, events, error codes. Serde derives throughout.
4. **world.rs** — `WorldGen`, deterministic sector generation, edge pruning, NPC templates.
5. **scaling.rs** — `BuildingType`, `ResearchType`, cost/time formulas, prerequisites, modifiers.

**Tests:** Port all Zig inline tests. Hex math, world gen determinism, edge symmetry, hub connectivity.

### Phase 2: Server — `iac-server`

The authoritative simulation.

1. **database.rs** — SQLite schema, player/fleet/sector persistence. `rusqlite` with `serde` for row mapping.
2. **combat.rs** — Round-based combat resolution. Port `resolveCombatRound`, `fireShip`, `applyDamage`, rapid-fire.
3. **engine.rs** — `GameEngine` struct, tick loop, movement, harvesting, NPC behavior, build queues, salvage.
4. **network.rs** — WebSocket server with `tokio-tungstenite`. Message queue, session management, broadcast.
5. **main.rs** — Entry point, signal handling, tick loop with sleep, config.

**Key design decisions:**
- Engine uses `Arc<Mutex<>>` or `tokio::sync::RwLock` for shared state between network and tick threads.
- Prefer `RwLock` — reads (broadcast) are frequent, writes (commands) are less so.
- Database operations happen on the tick thread (batched persist every 30 ticks), no async DB needed.

### Phase 3: Client — `iac-client`

The amber TUI.

1. **connection.rs** — WebSocket client, message send/receive, auth flow.
2. **state.rs** — `ClientState`, view management, fleet selection, map centering.
3. **input.rs** — Key mapping, command construction.
4. **renderer.rs** — `ratatui` layout: star map, sector info, event log, homeworld panel.
5. **main.rs** — Entry, app initialization, event loop.

**Key design decisions:**
- `ratatui` + `crossterm` for terminal handling.
- Amber color palette: yellow/orange on black.
- The Zig client uses zithril's `App` pattern; we'll use ratatui's standard `Terminal::draw` loop.

## File Size Discipline

Keep files under ~300 lines where possible. Split when a file grows beyond that:
- Large enums with methods → split into their own module.
- Engine tick processing → one file per subsystem if it grows (movement, harvesting, NPC).
- Protocol → split into `messages.rs`, `state.rs`, `events.rs` if needed.

## Migration Strategy

Port module by module, compiling and testing each phase before moving on. The shared crate is the foundation — get it right first, then the server can stand up, then the client connects.

No compatibility shims. Clean Rust idioms throughout.
