# Amber Scout — Numen dogfood journal

Capture friction in the moment. Promote to `gh issue` on `hotschmoe/numen`
only when a friction is reproducible, fired more than once, or blocked a
phase — and is not "please be Zig."

Buckets: `bug` / `stdlib-gap` / `qol` / `teaching` / `want`

Compiler: numen 0.12.1 (`numen version`)
Host: aarch64 Linux

## Later projects (not this repo)

- **Numen WASM interpreter.** Laminae's "wasm interpreter" is wasm3 (C)
  wrapped as an ICC service, plus demo modules under `user/wasm/`. It is
  a kernel door for foreign programs, not an IAC mechanic. A from-scratch
  Numen interpreter (decode / validate / instantiate / trap / bounded
  linear memory) is excellent Laminae-rewrite dogfood, but it is 20–40
  files and shares no types with hex/HTTP. Do it after Amber Scout.

## Log

<!-- newest first -->

### 2026-09-01 session 2 — complete scout run (CLI + HTTP)

- **complete:** 8 `.nmn` files. CLI win and HTTP win both verified (seed 1,
  harvest, fight at [5,-2], walk to hub). `amber-scout --http` listens on
  `127.0.0.1:18080`. Codex wrote `linux_sock.nmn` + `http.nmn` (gpt-5.5;
  `gpt-5.6-sol` on this CLI 0.142.0 is rejected). Grok subagent +
  orchestrator wrote `fleet.nmn`. Orchestrator: `game.nmn`, `page.nmn`,
  `main.nmn`.
- **bug:** `return .east` from `fn () ?HexDirection` is
  `internal_unexpected_nir_tag`. Workaround: `return HexDirection.east`.
  Filed: https://github.com/hotschmoe/numen/issues/19
- **stdlib-gap closed for this project:** socket/bind/listen/accept/sendto/
  recvfrom/setsockopt via first-class asm in-tree, not std. Bind of 8080
  failed errno 98 (EADDRINUSE, a python server). Sockets themselves worked.
- **qol:** Codex first test failure on `parseRequest(...) == null` was
  replaced by an explicit optional helper; a later `== null` check on an
  incomplete line passed. Journal only unless it fires again.
- **want:** listen errno printed (we added that). Port as argv later.

### 2026-09-01 session 2 — linux_sock + http (Codex)

- **qol / bug:** `numen test src/http.nmn` failed when a negative parser
  test used `try testing.expect(parseRequest("...") == null)` for `?Request`.
  Expected: null optional comparison passes. Actual: assertion failed while
  an explicit `if (parseRequest("...")) |_| return error.TestUnexpectedResult;`
  check passed. Workaround: use an explicit optional check helper. Bucket:
  `bug`.

### 2026-09-01 session 1 — hex + world + dump

- **hex.nmn / world.nmn / main.nmn** compile and run on 0.12.1. `numen test`
  hex: 9 passed. world: 5 passed. `cd amber-scout && numen build` emits
  `numen-out/bin/amber-scout` (29 864 bytes). Dump of seed 1 hub + ring 1
  looks right after the ring fix.
- **bug (ours, not compiler):** IAC `hexRing` iterator counted 6 cells but
  walked through the origin on radius 1. Tests only asserted count. Fixed
  to Red Blob Games (yield, then walk `radius` steps on each side). Added
  `distFromOrigin() == radius` asserts. Not a Numen issue.
- **qol / teaching:** `numen build --project-root amber-scout --list-steps`
  errors `build arguments mix direct and script forms`. `--project-root`
  is a *direct-mode* bound; script mode wants `cd` or `--build-file`.
  `help project` says "from the project root: numen build", so the flag
  name reads like "run that project". `--build-file amber-scout/build.nmn
  --list-steps` works. Filed: https://github.com/hotschmoe/numen/issues/17
- **teaching:** CLI errors have no diagnostic slug. `numen explain error
  build_arguments` → `'build_arguments' is not a compiler diagnostic slug
  or idiom topic`. Filed: https://github.com/hotschmoe/numen/issues/18
- **qol (journal only):** script-mode success line is still `sc build: 2
  step(s) ok`.
- **want:** `numen build --project-root DIR` with no `.nmn` positional
  could mean script mode in DIR. Not filed; the mix-error fix is enough.

Sub-word `@bitCast(u16)<->i16` gap (compiler fixture comment) was avoided
in `Hex.toKey`/`fromKey` by going through i32/u32. Did not fire.
