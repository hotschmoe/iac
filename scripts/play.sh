#!/usr/bin/env bash
# One-command play: build the Flutter web client, then serve it plus the
# WebSocket from the Rust server on a single port.
# Usage: [PORT=7777] scripts/play.sh [--js] [--no-build] [-- extra iac-server args]
#   --js        plain JS build instead of wasm (wasm is tried first by default)
#   --no-build  skip the Flutter build and reuse clients/web/build/web
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$PATH:$HOME/development/flutter/bin"

mode=wasm build=1
while [ $# -gt 0 ]; do
  case "$1" in
    --js) mode=js ;;
    --no-build) build=0 ;;
    --) shift; break ;;
    *) break ;;
  esac
  shift
done

if [ "$build" = 1 ]; then
  (
    cd clients/web
    if [ "$mode" = wasm ] && flutter build web --release --wasm; then :
    else
      [ "$mode" = wasm ] && echo "wasm build failed, falling back to JS build" >&2
      flutter build web --release
    fi
  )
fi

port=${PORT:-7777}
ip=$(hostname -I 2>/dev/null | awk '{print $1}')
echo "Play at: http://${ip:-localhost}:${port}/   (local: http://localhost:${port}/)"
exec cargo run --release -p iac-server -- --host 0.0.0.0 --port "$port" \
  --web-dir clients/web/build/web "$@"
