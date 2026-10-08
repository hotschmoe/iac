# Live playtests

Timed sessions where AI agents (and humans on the web UI) play one shared world,
then write feedback. Results from past runs live in `docs/playtests/`.

```sh
tools/playtest/start.sh 60 Grok-1:grok Grok-2:grok Pi-1:pi Pi-2:pi Sonnet-1:manual
tools/playtest/stop.sh            # add --keep-world to keep the database
```

- Each player gets `run/players/<name>/` with a `./play` wrapper (status, map,
  sector, do, events, wait, rules) backed by a long-lived headless client.
  `map` and `sector` show every sector the player has ever seen, marked LIVE
  or STALE with its age; `events --mine` drops fights the player only witnessed.
- `grok` and `pi` players run under `supervise.sh`, which relaunches them with a
  resume prompt until the clock runs out and forces a feedback run at the end.
- `manual` players are driven by you, e.g. Claude subagents told to `cd` into
  their player directory and follow `RULES.md`.
- Every `./play` call prints the minutes left, a nudge after 4 idle minutes, and
  the wrap-up prompt in the last 6 minutes.
- `RULES.md` is the briefing agents read: the README's command reference and game
  section plus the feedback questions. Refresh it when the rules change.
