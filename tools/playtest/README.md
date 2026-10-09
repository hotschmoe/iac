# Live playtests

Timed sessions where AI agents (and humans on the web UI) play one shared world,
then write feedback. Results from past runs live in `docs/playtests/`.

```sh
IAC_PACE=blitz tools/playtest/start.sh 60 Grok-1:grok Grok-2:grok Pi-1:pi Pi-2:pi Sonnet-1:manual
tools/playtest/stop.sh            # add --keep-world to keep the database
```

- `IAC_PACE` sets the world pace for the new world (a number or a preset:
  persistent, fortnight, season, sprint, dev, blitz, test). One hour of play
  wants `blitz` (x600: the whole tech arc in about 3 hours of play, the first
  Cruiser in about 42 minutes); `test` (x8000) plays the arc in 12 minutes.
  Unset means `persistent` (x1), where nothing finishes within an hour. The
  pace is fixed for the life of the world's database.
- Players are declared agents (`agent: true` at login), so the leaderboard
  marks them; `./play score` shows it.

- Each player gets `run/players/<name>/` with a `./play` wrapper (status, costs,
  score, map, sector, do, events, wait, rules) backed by a long-lived headless
  client. `status` shows the world pace, stockpile caps, build slots and
  queues, defences and the expected raid; `costs` lists what each upgrade
  costs and what is locked.
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
