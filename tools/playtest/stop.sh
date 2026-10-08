#!/usr/bin/env bash
# Stop a playtest started by start.sh: supervisors and their agents, player
# clients, then the server. Feedback and notes stay in the run dir; the world
# database is removed unless --keep-world is given.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
RUN="$(cd "${IAC_RUN:-$HERE/run}" && pwd)"
keep=0; [ "${1:-}" = --keep-world ] && keep=1

kill_tree() { local p=$1; for c in $(pgrep -P "$p"); do kill_tree "$c"; done; kill "$p" 2>/dev/null; }
for f in "$RUN"/state/*.supervise.pid "$RUN"/state/*.daemon.pid "$RUN"/state/server.pid; do
  [ -f "$f" ] && kill_tree "$(cat "$f")"
done
sleep 1
rm -rf "$RUN"/fifo "$RUN"/cfg "$RUN"/state "$RUN"/session.json
[ "$keep" = 1 ] || rm -f "$RUN"/world.db*
echo "stopped; feedback: $(ls "$RUN"/players/*/feedback.md 2>/dev/null | wc -l) files in $RUN/players"
