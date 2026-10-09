#!/usr/bin/env bash
# Keep one external agent (grok or pi) playing until the session ends.
# Usage: bin/hN N    (started by start.sh and restarted by watchdog.sh; N is the slot,
#                     whose player and kind are in state/slotN.conf)
# Kind dummy sleeps instead of running an agent; it exists to test the harness.
set -uo pipefail
SLOT=$1
HERE="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
RUN="$(cd "$(dirname "$0")/.." && pwd)"
. "$HERE/lib.sh"
NAME=$(slot_name "$SLOT") KIND=$(slot_kind "$SLOT")
LIVE="$RUN"
RULES="$HERE/RULES.md"
DIR="$LIVE/players/$NAME"
END=$(python3 -c "import json,sys;print(int(json.load(open(sys.argv[1]))['end']))" "$LIVE/session.json")
PIDFILE="$RUN/state/slot$SLOT.supervise.pid"
AGENTPID="$RUN/state/slot$SLOT.agent.pid"
export PATH="$HOME/.local/bin:$HOME/.nvm/versions/node/v22.23.1/bin:$PATH"
cd "$DIR"

if alive "$PIDFILE" "bin/h$SLOT" && [ "$(cat "$PIDFILE")" != "$$" ]; then
  hlog "sv$SLOT" "another supervisor (pid $(cat "$PIDFILE")) is already running for $NAME; exiting"
  exit 0
fi
echo $$ >"$PIDFILE"
rm -f "$RUN/state/slot$SLOT.done"
hlog "sv$SLOT" "supervisor up for $NAME ($KIND) pid $$ parent $PPID"

on_signal() {
  hlog "sv$SLOT" "supervisor for $NAME received SIG$1; exiting (a running agent is left alone; the watchdog restarts the supervisor)"
  exit $2
}
trap 'on_signal TERM 143' TERM
trap 'on_signal HUP 129' HUP
trap 'on_signal INT 130' INT
trap 'on_signal PIPE 141' PIPE

brief() {
  cat <<EOF
You are player $NAME in a live playtest of the space strategy game In Amber Clad.
Your player directory is $DIR. Run every command from there.
First read the rules: cat $RULES
Then play using ./play (start with ./play state and ./play status). The server ticks every second and
other AI players share the galaxy. The session lasts about one hour and every ./play call
shows the minutes left. Play the whole session: keep building, researching, exploring and
fighting; send several orders per call with ./play do and pass time with ./play wait --until idle
(at most 60 s per call). Do not stop early. Keep notes.md updated.
When under 6 minutes remain, write feedback.md as described in RULES.md, then stop.
You are playing autonomously; there is no human to answer questions.
EOF
}

resume() {
  local left=$(( (END - $(date +%s)) / 60 ))
  cat <<EOF
You are player $NAME in a live playtest of In Amber Clad, continuing a session you already
started. Your player directory is $DIR. The game is still running with about $left minutes left.
Read your notes: cat notes.md (and the rules if needed: cat $RULES).
Run ./play status, then keep playing until the timer ends; use ./play wait --until idle (max 60 s) while timers run.
When under 6 minutes remain, write feedback.md as described in RULES.md, then stop.
EOF
}

final() {
  cat <<EOF
You are player $NAME. The In Amber Clad playtest session has ended. Your player directory is $DIR.
Read notes.md, run ./play status once for your final position, and then write feedback.md
following the "Feedback" section of $RULES. Do not issue any more game commands.
EOF
}

# Run the agent in the background so a leftover one (from a killed supervisor) can be adopted.
run_agent() {
  local prompt=$1 budget=$2
  case "$KIND" in
    grok)
      timeout --signal=TERM --kill-after=30 "$budget" \
        grok --cwd "$DIR" --always-approve --output-format plain --max-turns 2000 -p "$prompt" \
        >>"$DIR/agent.out" 2>>"$DIR/agent.err" 9>&- &
      ;;
    pi)
      timeout --signal=TERM --kill-after=30 "$budget" \
        pi -p --provider hotschmoe-local --model hotschmoe-dd \
          --no-context-files --no-skills --no-extensions --exclude-tools ask_question \
          --session-dir "$DIR/.sessions" "$prompt" \
        >>"$DIR/agent.out" 2>>"$DIR/agent.err" 9>&- &
      ;;
    dummy)
      timeout --signal=TERM --kill-after=5 "$budget" sleep "${IAC_DUMMY_SECS:-3600}" >>"$DIR/agent.out" 2>>"$DIR/agent.err" &
      ;;
    *) hlog "sv$SLOT" "unknown kind $KIND"; return 2 ;;
  esac
  local pid=$!
  echo "$pid" >"$AGENTPID"
  wait "$pid"
  return $?
}

adopt_leftover() {
  if alive "$AGENTPID" "timeout"; then
    local pid; pid=$(cat "$AGENTPID")
    hlog "sv$SLOT" "adopting agent still running from an earlier supervisor (pid $pid); waiting for it to finish"
    while kill -0 "$pid" 2>/dev/null && (( $(date +%s) < END + 240 )); do sleep 3; done
    hlog "sv$SLOT" "adopted agent pid $pid is gone"
  fi
}

n=$(cat "$RUN/state/slot$SLOT.n" 2>/dev/null || echo 0)
adopt_leftover
fast_fails=0
while (( $(date +%s) < END - 120 )); do
  n=$((n + 1)); echo "$n" >"$RUN/state/slot$SLOT.n"
  budget=$(( END - $(date +%s) + 240 ))
  if (( n == 1 )); then prompt=$(brief); else prompt=$(resume); fi
  t0=$(date +%s)
  echo "=== $(date '+%T') launch $n budget=${budget}s" >>"$DIR/supervisor.log"
  hlog "sv$SLOT" "$NAME launch $n ($KIND) budget=${budget}s"
  run_agent "$prompt" "$budget"; rc=$?
  took=$(( $(date +%s) - t0 ))
  echo "=== $(date '+%T') exit $rc" >>"$DIR/supervisor.log"
  hlog "sv$SLOT" "$NAME agent exit $(sig_name $rc) after ${took}s"
  if (( took < 20 )); then fast_fails=$((fast_fails + 1)); else fast_fails=0; fi
  sleep $(( fast_fails > 0 ? (fast_fails < 12 ? fast_fails * 5 : 60) : 5 ))
done

if [[ ! -s "$DIR/feedback.md" ]]; then
  hlog "sv$SLOT" "$NAME final feedback run"
  echo "=== $(date '+%T') final feedback run" >>"$DIR/supervisor.log"
  run_agent "$(final)" 900; rc=$?
  hlog "sv$SLOT" "$NAME final run exit $(sig_name $rc)"
fi
touch "$RUN/state/slot$SLOT.done"
hlog "sv$SLOT" "supervisor done for $NAME launches=$n feedback=$([[ -s feedback.md ]] && echo yes || echo no)"
echo "=== $(date '+%T') supervisor done launches=$n" >>"$DIR/supervisor.log"
