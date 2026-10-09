# Live playtests

Timed sessions where AI agents (and humans on the web UI) play one shared world,
then write feedback. Results from past runs live in `docs/playtests/`.

```sh
IAC_PACE=blitz tools/playtest/start.sh 60 Grok-1:grok Grok-2:grok Pi-1:pi Pi-2:pi Sonnet-1:manual
tools/playtest/stop.sh            # add --keep-world to keep the database
python3 -m unittest discover -s tools/playtest/tests    # tests for the CLI's parsers and formatters
```

- `IAC_PACE` sets the world pace for the new world (a number or a preset:
  persistent, fortnight, season, sprint, dev, blitz, test). One hour of play
  wants `blitz` (x600: the whole tech arc in about 3 hours of play, the first
  Cruiser in about 42 minutes); `test` (x8000) plays the arc in 12 minutes.
  Unset means `persistent` (x1), where nothing finishes within an hour. The
  pace is fixed for the life of the world's database.
- Players are declared agents (`agent: true` at login), so the leaderboard
  marks them; `./play score` shows it.
- Kinds: `grok`, `pi` (supervised CLI agents), `manual` (you drive it, e.g. Claude subagents told to
  `cd` into their player directory and follow `RULES.md`), `dummy` (a supervised process that only sleeps, for testing the harness).

## The `play` CLI

Each player gets `run/players/<name>/play`, backed by a long-lived headless client. The server answers
commands at its next tick (once a second), so the tool never polls with fixed sleeps. Read commands
(`state`, `status`, `costs`, `map`, `sector`) use a view rebuilt incrementally from the client's log and return in
about 0.1 s.

| command | what it does |
|---------|--------------|
| `state [--pretty]` | One compact JSON document: `stock`, `cap`, `per_s` (income), `levels`, `queues` (`build`/`research`/`ship`, each with `slots`, `depth`, `running`, `waiting` with `short` shortfall and `covered_in_s`, `room`, `open`), `idle`, `next` (cost, seconds, `ok`/`locked` per catalog entry), `fleets` (position, state, policy, fuel, range, power, cargo, exits, hostiles), `docked`, `defence`, `score` (rank, core, combat, explore, cap flag), `score_rule`, `minutes_left`, `alerts`, `world`. |
| `status [--brief]` | Headline (score, rank, minutes left, idle queues, scoring rule), then stock and queues with waiting items shown under their queue with shortfall and income cover time, fleets and sectors. `--brief` is the headline plus one line per queue and fleet. |
| `costs [section]` | Next 3 levels per upgrade (levels past the first are extrapolated along the cost curve and marked `~`), time, what each level unlocks, and for mines the extra output per second and payback. |
| `do '<json>' ['<json>' ...]` | Commands in order (several arguments, one JSON array, or newline-separated). One result line each: `ok ... -> started, done in 3s`, `queued [i] waiting for ...`, or `ERR ...`; then new events and a `queues:` line. Returns at the first server tick after the last command (about 0.5 s on average) or at once if the client rejects it. Orders are 0.25 s apart (`--gap S`). `--fast` returns after 0.15 s without confirmation. The server's errors carry no command id, so they are matched to commands by the fleet or item they name. |
| `wait [secs] [--until idle\|event\|done] [--mine]` | Default 45 s, never more than 60 s (it says so). Returns early when a queue gets a free slot or an idle queue becomes payable, a manual fleet arrives or stops, a fleet is lost or blocked, or an alert fires. `idle`: queues and alerts only. `event`: also any completion, arrival or fight. `done`: queues empty and fleets idle. Prints the reasons, the collapsed events and a `queues:` line. |
| `events [--mine]` | Events since last looked: scans collapse to one line per fleet (sectors, hostiles, signals by kind with threat band), combat rounds to one line per sector, repeated autopilot holds to a count. `--mine` drops fights the player only witnessed. |
| `preview F [Q R ...]` | Jump cost, return fuel, threat and odds for adjacent sectors; no coordinates previews every exit of the fleet's sector. |
| `score [rows]`, `map [r]`, `sector Q R`, `rules` | As before. |

Every call prints the minutes left, a nudge after 4 idle minutes, and the wrap-up prompt in the last 6 minutes.
The daemon asks the server for the leaderboard every 8 s so `state` and `status` can show score and rank without waiting.

`bench_loop.py` runs an agent-style loop (`state`, batched `do`, `wait --until idle`) and reports orders per minute
and how much queue time was lost while something payable waited; `--old WRAPPER` replays the same decisions through
the previous tool (one order per `do`, `wait 5`).

## Harness robustness

`start.sh` writes `state/slotN.conf` (player and kind) and symlinks `bin/hN` (supervisor), `bin/cN` (player client)
and `bin/w` (watchdog) to the scripts, so their command lines carry slot numbers, never agent names or kinds: an
agent running `pkill -f grok` or `pkill -f Grok-2` does not match them.

- `supervise.sh` (relaunches a `grok`/`pi` agent with a resume prompt until the clock runs out, then forces a feedback
  run) logs every launch, every exit with its code or signal (`137 (killed by SIGKILL)`, `124 (agent hit its time
  budget)`), and the signal it receives if someone terminates it. A pidfile check stops duplicates, quick crash loops
  back off, and a replacement supervisor adopts an agent left running by a dead one instead of starting a second.
- `watchdog.sh` checks every 5 s that each client daemon and each unfinished supervisor is alive and restarts it,
  logging why. It exits when the session plus 30 minutes is over or `stop.sh` runs. The player clients check on the
  watchdog every 15 s and restart it (it holds a lock, so only one runs), so no single kill leaves the session
  unsupervised.
- Everything goes to `run/events.log`: session start, launches, restarts, signals, exits, client crashes, server loss,
  stop. Per-player `supervisor.log`, `agent.out` and `agent.err` stay in the player directories.
- `stop.sh` removes the watchdog first, then supervisors, agents, clients and the server, by pid.
- Fault test: `start.sh 20 A1:manual B1:dummy` then `kill -9` the pid in `run/state/slot1.supervise.pid`; within 5 s
  `events.log` shows the watchdog restarting it.

`RULES.md` is the briefing agents read: the tool section, the command reference and game section, and the feedback
questions. Refresh it when the rules change.
