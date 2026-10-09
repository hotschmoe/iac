#!/usr/bin/env bash
# Start a timed live playtest: server, one headless client per player, the
# session clock, and supervisors for CLI agents (grok, pi).
# Usage: tools/playtest/start.sh MINUTES PLAYER:KIND...
#   KIND = grok | pi | manual   (manual: you drive it, e.g. a Claude subagent)
# Example: tools/playtest/start.sh 60 Grok-1:grok Pi-1:pi Sonnet-1:manual
# Env: IAC_RUN (run dir, default tools/playtest/run), IAC_PORT (default 7777),
#      IAC_PACE (world pace for the new world: a number or a preset such as
#      blitz, test, season; default persistent). A world keeps its pace.
#      A one-hour session wants IAC_PACE=blitz.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
RUN="${IAC_RUN:-$HERE/run}"
PORT="${IAC_PORT:-7777}"
PACE="${IAC_PACE:-}"
MINUTES="${1:?minutes}"; shift
[ -e "$RUN/session.json" ] && { echo "$RUN already has a session; run stop.sh first" >&2; exit 1; }
mkdir -p "$RUN"/{logs,fifo,state,cfg,players}
RUN="$(cd "$RUN" && pwd)"
export IAC_RUN="$RUN" IAC_PORT="$PORT"

(cd "$REPO" && cargo build --release -q -p iac-server -p iac-client)
web="$REPO/clients/web/build/web"
[ -f "$web/index.html" ] || echo "no web build at $web (scripts/play.sh builds it); serving WS only" >&2
pace_args=(); [ -n "$PACE" ] && pace_args=(--pace "$PACE")
nohup "$REPO/target/release/iac-server" --host 0.0.0.0 --port "$PORT" --db "$RUN/world.db" \
  ${pace_args[@]+"${pace_args[@]}"} --web-dir "$web" >"$RUN/logs/server.log" 2>&1 &
echo $! >"$RUN/state/server.pid"
until ss -ltn | grep -q ":$PORT "; do
  kill -0 "$(cat "$RUN/state/server.pid")" 2>/dev/null || { cat "$RUN/logs/server.log" >&2; echo "server did not start" >&2; exit 1; }
  sleep 0.3
done

python3 -c "import json,time,sys;t=time.time();json.dump({'start':t,'end':t+60*int(sys.argv[1])},open(sys.argv[2],'w'))" \
  "$MINUTES" "$RUN/session.json"

for spec in "$@"; do
  name="${spec%%:*}" kind="${spec#*:}"
  mkdir -p "$RUN/players/$name"
  printf '#!/usr/bin/env bash\nIAC_RUN=%q IAC_PORT=%q IAC_PLAYER=%q exec python3 %q "$@"\n' \
    "$RUN" "$PORT" "$name" "$HERE/play.py" >"$RUN/players/$name/play"
  chmod +x "$RUN/players/$name/play"
  nohup python3 "$HERE/play.py" daemon "$name" >"$RUN/logs/$name.daemon" 2>&1 &
  echo $! >"$RUN/state/$name.daemon.pid"
  if [ "$kind" != manual ]; then
    nohup setsid "$HERE/supervise.sh" "$kind" "$name" >"$RUN/logs/$name.supervise" 2>&1 &
    echo $! >"$RUN/state/$name.supervise.pid"
  fi
done

ip=$(hostname -I | awk '{print $1}')
echo "playtest running for $MINUTES min in $RUN (pace ${PACE:-persistent})"
echo "web: http://$ip:$PORT/   players: $RUN/players/<name>/play"
