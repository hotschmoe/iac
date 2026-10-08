#!/usr/bin/env bash
# Keep one external agent (grok or pi) playing until the session ends.
# Usage: supervise.sh grok|pi PLAYER
set -uo pipefail
KIND=$1 NAME=$2
HERE="$(cd "$(dirname "$0")" && pwd)"
LIVE="$(cd "${IAC_RUN:-$HERE/run}" && pwd)"
RULES="$HERE/RULES.md"
DIR="$LIVE/players/$NAME"
END=$(python3 -c "import json;print(int(json.load(open('$LIVE/session.json'))['end']))")
export PATH="$HOME/.local/bin:$HOME/.nvm/versions/node/v22.23.1/bin:$PATH"
cd "$DIR"

brief() {
  cat <<EOF
You are player $NAME in a live playtest of the space strategy game In Amber Clad.
Your player directory is $DIR. Run every command from there.
First read the rules: cat $RULES
Then play using ./play (start with ./play status). The server ticks every second and
other AI players share the galaxy. The session lasts about one hour and every ./play call
shows the minutes left. Play the whole session: keep building, researching, exploring and
fighting; use ./play wait 60 while timers run. Do not stop early. Keep notes.md updated.
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
Run ./play status, then keep playing until the timer ends; use ./play wait 60 while timers run.
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

run_agent() {
  local prompt=$1 budget=$2
  case "$KIND" in
    grok)
      timeout --signal=TERM --kill-after=30 "$budget" \
        grok --cwd "$DIR" --always-approve --output-format plain --max-turns 2000 -p "$prompt"
      ;;
    pi)
      timeout --signal=TERM --kill-after=30 "$budget" \
        pi -p --provider hotschmoe-local --model hotschmoe-dd \
          --no-context-files --no-skills --no-extensions --exclude-tools ask_question \
          --session-dir "$DIR/.sessions" "$prompt"
      ;;
  esac
}

n=0
while (( $(date +%s) < END - 120 )); do
  n=$((n + 1))
  budget=$(( END - $(date +%s) + 240 ))
  if (( n == 1 )); then prompt=$(brief); else prompt=$(resume); fi
  echo "=== $(date '+%T') launch $n budget=${budget}s" >> "$DIR/supervisor.log"
  run_agent "$prompt" "$budget" >> "$DIR/agent.out" 2>> "$DIR/agent.err"
  echo "=== $(date '+%T') exit $? " >> "$DIR/supervisor.log"
  sleep 5
done

if [[ ! -s "$DIR/feedback.md" ]]; then
  echo "=== $(date '+%T') final feedback run" >> "$DIR/supervisor.log"
  run_agent "$(final)" 900 >> "$DIR/agent.out" 2>> "$DIR/agent.err"
fi
echo "=== $(date '+%T') supervisor done launches=$n feedback=$([[ -s feedback.md ]] && echo yes || echo no)" >> "$DIR/supervisor.log"
