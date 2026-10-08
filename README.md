# IN AMBER CLAD — Full Dart Stack

Monorepo: **Flutter amber CRT UI** + **Dart game server** + **shared protocol**.

```
iac_dart/
  lib/                      # Flutter web client (amber UI)
  packages/iac_shared/      # Hex, world gen, protocol, balance
  packages/iac_server/      # Tick engine + WebSocket + static host
  build/web/                # flutter build web output
  tool/serve.dart           # one-command launcher
```

## Quick start (LAN)

```bash
# 1) Build UI (Wasm preferred)
flutter build web --wasm --release

# 2) Run server (serves UI + /ws)
dart run tool/serve.dart --host 0.0.0.0 --port 7777
# or:
cd packages/iac_server && dart run bin/server.dart \
  --host 0.0.0.0 --port 7777 --web-dir ../../build/web
```

Open **http://&lt;lan-ip&gt;:7777** — UI talks to **ws://&lt;same-host&gt;:7777/ws**.

Header shows **LIVE** when the WebSocket is up, **DEMO** if the server is unreachable (offline tick loop).

### Wasm build notes

```bash
flutter build web --wasm --release
# Produces main.dart.wasm + main.dart.mjs (+ JS fallback)
ls -lh build/web/main.dart.wasm
```

Still a web bundle — Dart compiles to WasmGC, not a bare game `.wasm` export.

### Dev UI only (no server)

```bash
flutter run -d web-server --web-hostname 0.0.0.0 --web-port 8080
# Falls back to demo mode unless ?ws=ws://host:7777/ws
```

## Live commands (command bar)

| Input | Action |
|-------|--------|
| `harvest` / `h` | Harvest current sector |
| `attack` / `a` | Engage hostiles |
| `build` / `build metal` | Queue metal mine upgrade |
| `build crystal` / `shipyard` / `lab` / `deut` | Other buildings |
| `research` | Queue fuel efficiency research |
| `ship scout` / `ship corvette` | Shipyard |
| `recall` | Emergency recall home |
| `stop` | Cancel move/harvest |
| `status` / `fleet` | Diagnostics |
| Numpad 1–6 (Windshield) | Move active fleet |

## Architecture

```
Browser (Flutter wasm/js)
    │  WebSocket JSON
    ▼
iac_server (shelf)
  ├── /ws     → GameEngine ticks @ 1 Hz
  └── /*      → static build/web
```

Shared package is the single source of truth for hex math, world edges, and wire protocol so UI maps match server connectivity.

## Tests

```bash
cd packages/iac_shared && dart test
cd packages/iac_server && dart test
```
