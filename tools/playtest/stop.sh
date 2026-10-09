#!/usr/bin/env bash
# Stop a playtest started by start.sh: watchdog first (so nothing is restarted), then
# supervisors and their agents, player clients, and the server. Feedback, notes and
# events.log stay in the run dir; the world database and login tokens are removed unless --keep-world.
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
RUN="$(cd "${IAC_RUN:-$HERE/run}" && pwd)"
. "$HERE/lib.sh"
keep=0; [ "${1:-}" = --keep-world ] && keep=1

touch "$RUN/state/stopping" 2>/dev/null
hlog harness "stop requested"
kill_tree() { local p=$1; for c in $(pgrep -P "$p"); do kill_tree "$c"; done; kill "$p" 2>/dev/null; }
[ -f "$RUN/state/watchdog.pid" ] && kill_tree "$(cat "$RUN/state/watchdog.pid")"
for f in "$RUN"/state/*.pid; do
  [ -f "$f" ] && kill_tree "$(cat "$f")"
done
sleep 1
rm -rf "$RUN"/fifo "$RUN"/state "$RUN"/session.json "$RUN"/bin
[ "$keep" = 1 ] || rm -rf "$RUN"/world.db* "$RUN"/cfg
hlog harness "stopped"
echo "stopped; feedback: $(ls "$RUN"/players/*/feedback.md 2>/dev/null | wc -l) files in $RUN/players"
