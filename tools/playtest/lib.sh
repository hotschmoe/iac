# Shared helpers for start.sh, stop.sh, supervise.sh and watchdog.sh. Source it after setting RUN.

hlog() { printf '%s %-10s %s\n' "$(date '+%F %T')" "$1" "$2" >>"$RUN/events.log"; }

# alive PIDFILE MARKER: the pid is running and its command line contains MARKER.
alive() {
  local pid
  pid=$(cat "$1" 2>/dev/null) || return 1
  [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && grep -qa -- "$2" "/proc/$pid/cmdline" 2>/dev/null
}

slot_name() { awk 'NR==1{print $1}' "$RUN/state/slot$1.conf"; }
slot_kind() { awk 'NR==1{print $2}' "$RUN/state/slot$1.conf"; }
slot_count() { ls "$RUN"/state/slot*.conf 2>/dev/null | wc -l; }

sig_name() { # exit status -> readable text
  local rc=$1
  if [ "$rc" -eq 124 ]; then echo "124 (agent hit its time budget)"
  elif [ "$rc" -gt 128 ]; then echo "$rc (killed by SIG$(kill -l $((rc - 128)) 2>/dev/null))"
  else echo "$rc"; fi
}

# Supervisors and clients run under bin/ symlinks so their command lines carry a
# slot number, never the agent's name or kind (a stray `pkill -f grok` misses them).
launch_supervisor() {
  local i=$1
  nohup setsid "$RUN/bin/h$i" "$i" >>"$RUN/logs/slot$i.supervise" 2>&1 9>&- &
  hlog harness "launched supervisor slot $i ($(slot_name "$i"), $(slot_kind "$i")) pid $!"
}

launch_daemon() {
  local i=$1
  nohup python3 "$RUN/bin/c$i" daemon --slot "$i" >>"$RUN/logs/slot$i.daemon" 2>&1 9>&- &
  echo $! >"$RUN/state/slot$i.daemon.pid"
  hlog harness "launched client daemon slot $i ($(slot_name "$i")) pid $!"
}

launch_watchdog() {
  nohup setsid "$RUN/bin/w" >>"$RUN/logs/watchdog.log" 2>&1 &
  hlog harness "launched watchdog pid $!"
}
