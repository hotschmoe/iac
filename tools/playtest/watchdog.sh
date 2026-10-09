#!/usr/bin/env bash
# Restart dead supervisors and player clients until the session (plus a grace period)
# ends. Started by start.sh as bin/w; run under flock so only one runs.
# A supervisor that finished cleanly leaves state/slotN.done and is not restarted.
set -uo pipefail
HERE="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
RUN="$(cd "$(dirname "$0")/.." && pwd)"
. "$HERE/lib.sh"
exec 9>"$RUN/state/watchdog.lock"
flock -n 9 || exit 0
echo $$ >"$RUN/state/watchdog.pid"
END=$(python3 -c "import json,sys;print(int(json.load(open(sys.argv[1]))['end']))" "$RUN/session.json")
hlog watchdog "watching, session ends $(date -d "@$END" '+%T')"
trap 'hlog watchdog "received SIGTERM, exiting"; exit 143' TERM
trap 'hlog watchdog "received SIGHUP, ignoring"' HUP
down=0
while [ ! -e "$RUN/state/stopping" ] && (( $(date +%s) < END + 1800 )); do
  n=$(slot_count)
  for ((i = 0; i < n; i++)); do
    [ -e "$RUN/state/stopping" ] && break
    name=$(slot_name "$i"); kind=$(slot_kind "$i")
    if ! alive "$RUN/state/slot$i.daemon.pid" "bin/c$i"; then
      hlog watchdog "client daemon slot $i ($name) is dead; restarting"
      launch_daemon "$i"
    fi
    if [ "$kind" != manual ] && [ ! -e "$RUN/state/slot$i.done" ] && ! alive "$RUN/state/slot$i.supervise.pid" "bin/h$i"; then
      hlog watchdog "supervisor slot $i ($name, $kind) is dead without finishing; restarting"
      launch_supervisor "$i"
    fi
  done
  if alive "$RUN/state/server.pid" "iac-server"; then down=0
  elif [ "$down" = 0 ]; then down=1; hlog watchdog "SERVER is not running (pid $(cat "$RUN/state/server.pid" 2>/dev/null))"; fi
  sleep 5 9>&-
done
hlog watchdog "exiting (session over or stopped)"
