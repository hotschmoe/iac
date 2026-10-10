#!/usr/bin/env bash
# One-command play: build the Flutter web client, then serve it.
#
# Multiplayer (default): the Rust server serves the client and the WebSocket
# on a single port.
#   [PORT=7777] scripts/play.sh [--js] [--no-build] [-- extra iac-server args]
#
# Single player: a static bundle with no server. The client plus the engine
# (iac-solo) as WebAssembly, in clients/web/build/single-player; any static
# file host can serve that directory. This script serves it with Python.
#   [PORT=7777] [HOST=127.0.0.1] [BASE_HREF=/] scripts/play.sh --single-player [--js] [--no-build] [--no-serve]
#
#   --js        plain JS build instead of wasm (wasm is tried first by default)
#   --no-build  skip the builds and reuse the bundle
#   --no-serve  (single player) build the bundle and stop
#   BASE_HREF   (single player) the path the bundle will be hosted under
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$PATH:$HOME/development/flutter/bin"

mode=wasm build=1 solo=0 serve=1
while [ $# -gt 0 ]; do
  case "$1" in
    --js) mode=js ;;
    --no-build) build=0 ;;
    --single-player) solo=1 ;;
    --no-serve) serve=0 ;;
    --) shift; break ;;
    *) break ;;
  esac
  shift
done

flutter_build() {
  (
    cd clients/web
    if [ "$mode" = wasm ] && flutter build web --release --wasm "$@"; then :
    else
      [ "$mode" = wasm ] && echo "wasm build failed, falling back to JS build" >&2
      flutter build web --release "$@"
    fi
  )
}

port=${PORT:-7777}

if [ "$solo" = 1 ]; then
  out=$PWD/clients/web/build/single-player
  if [ "$build" = 1 ]; then
    cargo build --release -p iac-solo --target wasm32-unknown-unknown
    flutter_build --dart-define=IAC_SINGLE_PLAYER=true --no-web-resources-cdn --output "$out" \
      ${BASE_HREF:+--base-href "$BASE_HREF"}
    mkdir -p "$out/solo"
    cp solo/web/worker.js "$out/solo/worker.js"
    engine=target/wasm32-unknown-unknown/release/iac_solo.wasm
    if command -v wasm-tools >/dev/null; then
      wasm-tools strip "$engine" -o "$out/solo/iac_solo.wasm"
    else
      cp "$engine" "$out/solo/iac_solo.wasm"
    fi
  fi
  [ -f "$out/solo/iac_solo.wasm" ] || { echo "no single-player bundle in $out; run without --no-build" >&2; exit 1; }
  echo "Single-player bundle: $out ($(du -sh "$out" | cut -f1))"
  [ "$serve" = 1 ] || exit 0
  host=${HOST:-127.0.0.1}
  echo "Play at: http://${host}:${port}/"
  exec python3 -m http.server "$port" --bind "$host" --directory "$out"
fi

if [ "$build" = 1 ]; then flutter_build; fi

ip=$(hostname -I 2>/dev/null | awk '{print $1}')
echo "Play at: http://${ip:-localhost}:${port}/   (local: http://localhost:${port}/)"
exec cargo run --release -p iac-server -- --host 0.0.0.0 --port "$port" \
  --web-dir clients/web/build/web "$@"
