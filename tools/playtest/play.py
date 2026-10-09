#!/usr/bin/env python3
"""Agent-facing CLI for a live In Amber Clad session.

Each player has a long-lived headless client (started with `play.py daemon NAME`)
whose stdin is a FIFO and whose stdout is appended to logs/NAME.ndjson.
Agents call `play <subcommand>` from their player directory.
"""
import json, os, sys, time, subprocess, pathlib

HERE = pathlib.Path(__file__).resolve().parent
LIVE = pathlib.Path(os.environ.get("IAC_RUN") or HERE / "run").resolve()
LOGS, FIFOS, STATE = LIVE / "logs", LIVE / "fifo", LIVE / "state"
CLIENT = HERE.parent.parent / "target/release/iac-client"
PORT = int(os.environ.get("IAC_PORT", "7777"))
DIRS = {(1, 0): "E", (1, -1): "NE", (0, -1): "NW", (-1, 0): "W", (-1, 1): "SW", (0, 1): "SE"}


def session():
    try:
        return json.loads((LIVE / "session.json").read_text())
    except FileNotFoundError:
        return {}


def header(name, tick=None):
    s = session()
    left = (s.get("end", 0) - time.time()) / 60 if s else None
    parts = [name]
    if tick is not None:
        parts.append(f"tick {tick}")
    if left is not None:
        parts.append(f"{max(left, 0):.0f} min left" if left > 0 else "SESSION OVER")
    print("[" + " | ".join(parts) + "]")
    if left is not None and left <= 0:
        print("SESSION OVER: write feedback.md now (see RULES.md 'Feedback'), then stop.")
    elif left is not None and left <= 6:
        print("WRAP UP: under 6 minutes left. Write feedback.md now (see RULES.md 'Feedback').")
    elif left is not None:
        acts = STATE / f"{name}.actions"
        idle = time.time() - (acts.stat().st_mtime if acts.exists() else s.get("start", time.time()))
        if idle > 240:
            print(f"NUDGE: no command sent for {idle / 60:.0f} min. Are your queues and fleets busy? "
                  "Check `./play status` and give orders; the session runs until the timer ends.")


def log_lines(name, start=0):
    path = LOGS / f"{name}.ndjson"
    if not path.exists():
        return [], 0
    with open(path, "rb") as f:
        f.seek(start)
        data = f.read()
    end = start + len(data)
    if not data.endswith(b"\n"):
        cut = data.rfind(b"\n") + 1
        data, end = data[:cut], start + cut
    out = []
    for raw in data.splitlines():
        try:
            out.append(json.loads(raw))
        except json.JSONDecodeError:
            pass
    return out, end


def log_size(name):
    p = LOGS / f"{name}.ndjson"
    return p.stat().st_size if p.exists() else 0


def cursor(name, value=None):
    p = STATE / f"{name}.cursor"
    if value is None:
        return int(p.read_text()) if p.exists() else 0
    p.write_text(str(value))


def send(name, obj):
    with open(FIFOS / f"{name}.in", "w") as f:
        f.write(json.dumps(obj) + "\n")
    with open(STATE / f"{name}.actions", "a") as f:
        f.write(json.dumps({"t": time.time(), "msg": obj}) + "\n")


def last_tick(name):
    lines, _ = log_lines(name, max(0, log_size(name) - 20000))
    for m in reversed(lines):
        if "tick" in m and m.get("type") in ("tick_update", "full_state"):
            return m["tick"]
    return None


def digest(msgs, mine_only=False):
    """Events, errors and notable messages; skips per-tick noise.

    Per-shot CombatRound events collapse to one line per fight and tick.
    With mine_only, events about other empires' fights (mine=false) are dropped.
    """
    out = []
    rounds = {}
    for m in msgs:
        t = m.get("type")
        if t == "tick_update":
            events = [("event", e) for e in m.get("events") or []]
        elif t in ("event", "error", "connection_closed", "client_error", "preview_move", "leaderboard"):
            events = [(t, m)]
        else:
            continue
        for kind, e in events:
            if mine_only and kind == "event" and e.get("mine") is False:
                continue
            if kind == "event" and e.get("kind") == "CombatRound":
                key = (e["tick"], e["sector"]["q"], e["sector"]["r"])
                if key not in rounds:
                    rounds[key] = {"tick": e["tick"], "kind": "CombatRounds", "sector": e["sector"],
                                   "shots": 0, "mine": False, "hull_damage_to_you": 0.0,
                                   "hull_damage_dealt": 0.0, "hull_damage_others": 0.0}
                    out.append(("event", rounds[key]))
                r = rounds[key]
                r["shots"] += 1
                r["mine"] = r["mine"] or bool(e.get("mine"))
                field = ("hull_damage_to_you" if e.get("target_owner") is not None else "hull_damage_dealt") \
                    if e.get("mine") else "hull_damage_others"
                r[field] = round(r[field] + e["hull_damage"], 1)
                continue
            out.append((kind, e))
    return out


def fmt(kind, m):
    m = {k: v for k, v in m.items() if k != "type"}
    return f"  {kind}: {json.dumps(m, separators=(',', ':'))}"


def print_digest(items, limit=40):
    if not items:
        print("  (no new events)")
        return
    if len(items) > limit:
        print(f"  ... {len(items) - limit} earlier events omitted")
    for kind, m in items[-limit:]:
        print(fmt(kind, m))


def fresh_state(name, timeout=8):
    start = log_size(name)
    send(name, {"type": "request_full_state"})
    deadline = time.time() + timeout
    while time.time() < deadline:
        time.sleep(0.3)
        lines, _ = log_lines(name, start)
        for m in lines:
            if m.get("type") == "full_state":
                return m
    return None


def age(ticks):
    """Ticks are seconds: 45s, 7m, 3h, 2d."""
    ticks = max(int(ticks), 0)
    return (f"{ticks}s" if ticks < 120 else f"{ticks // 60}m" if ticks < 7200
            else f"{ticks // 3600}h" if ticks < 172800 else f"{ticks // 86400}d")


def freshness(sec, now):
    """'LIVE' for current intel, else how old the remembered entry is."""
    return "LIVE" if sec.get("live") else f"STALE {age(now - sec.get('last_seen', now))} old"


def res(r):
    return "/".join(f"{(r or {}).get(k, 0):.0f}" for k in ("metal", "crystal", "deuterium"))


def waiting(short):
    """'(needs 35 metal)' for a queued item the stockpile cannot pay yet."""
    parts = [f"{short[k]:.0f} {k}" for k in ("metal", "crystal", "deuterium") if (short or {}).get(k, 0) > 0]
    return f" (needs {', '.join(parts)})" if parts else ""


def world_line(world):
    pace = world["pace"]
    label = f"x{pace:g}" + (f" {world['preset']}" if world.get("preset") else "")
    est = world.get("estimate")
    tail = f"; a competent player reaches a Cruiser in about {age(est['first_cruiser_s'])} and the endgame in about {age(est['endgame_s'])}" if est else ""
    return f"World: pace {label} (build, research and ship times, and mine output, scale with it; fuel, cooldowns and combat do not){tail}"


def summarize(fs):
    hw = fs["homeworld"]
    p = fs["player"]
    print(f"Empire {p['name']} (id {p['id']})  homeworld {hw['location']['q']},{hw['location']['r']}")
    if fs.get("world"):
        print(world_line(fs["world"]))
    st = hw["storage"]
    stock = p["resources"]
    caps = "  ".join(f"{k} {stock[k]:.0f}/{st['cap'][k]:.0f}" + (" FULL" if k in st["capped"] else "")
                     for k in ("metal", "crystal", "deuterium"))
    per_tick = [hw["production"].get(k, 0) for k in ("metal", "crystal", "deuterium")]
    print(f"Stockpile: {caps}   production per tick: " + "/".join(f"{x:.4g}" for x in per_tick)
          + "  (per hour: " + "/".join(f"{x * 3600:.4g}" for x in per_tick) + ")")
    eta = [f"{k} full in {age(v)}" for k, v in st["full_in_s"].items() if v]
    print(f"Raid-protected stock: {res(st['protected'])}" + (f"   ({', '.join(eta)})" if eta else ""))
    print("Buildings: " + ", ".join(f"{b['building_type']} {b['level']}" for b in hw["buildings"]))
    print("Research:  " + (", ".join(f"{r['tech']} {r['level']}" for r in hw["research"]) or "none"))
    t = fs["tick"]
    depth = hw.get("queue_depth", 3)
    running = hw.get("build_queue") or []
    print(f"Build queue ({len(running)}/{hw.get('build_slots', 1)} slots, {depth} items max): "
          + ("; ".join(f"{q['building_type']} -> L{q['target_level']} done in {age(q['end_tick'] - t)}" for q in running) or "idle"))
    for i, q in enumerate(hw.get("build_pending") or []):
        print(f"  waiting [{i}]: {q['building_type']} -> L{q['target_level']} ({age(q['ticks'])}){waiting(q.get('waiting_for'))}")
    q = hw.get("research_active")
    print("Research queue: " + (f"{q['tech']} -> L{q['target_level']} done in {age(q['end_tick'] - t)}" if q else "idle"))
    for i, q in enumerate(hw.get("research_pending") or []):
        print(f"  waiting [{i}]: {q['tech']} -> L{q['target_level']} ({age(q['ticks'])}){waiting(q.get('waiting_for'))}")
    q = hw.get("shipyard_queue")
    print("Shipyard queue: " + (f"{q['item']} {q['built']}/{q['count']}, next unit in {age(q['end_tick'] - t)}" if q else "idle"))
    for i, q in enumerate(hw.get("shipyard_pending") or []):
        print(f"  waiting [{i}]: {q['count']}x {q['item']} ({age(q['ticks'])} each){waiting(q.get('waiting_for'))}")
    stand = [f"{d['count']}x {d['kind']}" + (f" (+{d['restoring']} being rebuilt)" if d.get("restoring") else "")
             for d in hw.get("defences") or [] if d["count"] or d.get("restoring")]
    print(f"Defence power {hw.get('home_defence_power', 0):.0f} against an expected raid of about "
          f"{hw.get('next_raid_estimate_power', 0):.0f}; structures: " + (", ".join(stand) or "none"))
    docked = {}
    for s in hw.get("docked_ships") or []:
        docked[s["class"]] = docked.get(s["class"], 0) + 1
    print("Docked ships: " + (", ".join(f"{n}x {c}" for c, n in docked.items()) or "none"))
    sectors = {(s["location"]["q"], s["location"]["r"]): s for s in fs.get("known_sectors") or []}
    live = sum(1 for s in sectors.values() if s.get("live"))
    print(f"Known sectors: {len(sectors)} ({live} live, {len(sectors) - live} remembered/stale)")
    for f in fs.get("fleets") or []:
        loc = (f["location"]["q"], f["location"]["r"])
        classes = {}
        hull = hull_max = 0
        for s in f["ships"]:
            classes[s["class"]] = classes.get(s["class"], 0) + 1
            hull += s["hull"]; hull_max += s["hull_max"]
        cargo = sum((f.get("cargo") or {}).values())
        print(f"\nFleet {f['id']} at {loc[0]},{loc[1]}  {f['state']}  policy={f.get('policy') or 'manual'}  cooldown={f.get('cooldown_remaining', 0)}"
              f"  power {f.get('power', 0):.0f}  range {f.get('range_hops', 0)} hops out and back"
              + ("  CARGO BLOCKED: stockpile full, spend something" if f.get("cargo_blocked") else ""))
        print(f"  ships: " + ", ".join(f"{n}x {c}" for c, n in classes.items())
              + f"  hull {hull:.0f}/{hull_max:.0f}  fuel {f['fuel']:.0f}/{f['fuel_max']:.0f}"
              + f"  cargo {cargo:.0f}/{f['cargo_capacity']:.0f} ({res(f.get('cargo'))})")
        print("  ship ids: " + ", ".join(f"{s['id']}={s['class']}" for s in f["ships"]))
        jump, home = f.get("jump_fuel", 0), f.get("home_fuel", 0)
        if jump:
            note = ""
            if home and f["fuel"] < jump:
                note = "  STRANDED: emergency reserve recharging (about 5 min per jump)"
            elif home > f["fuel"]:
                note = "  WARNING: not enough fuel to get home"
            print(f"  fuel cost: jump {jump:.0f}, way home {home:.0f}{note}")
        sec = sectors.get(loc)
        if sec:
            describe(sec, loc, sectors, fs["tick"], indent="  ")


def describe(sec, loc, sectors, now, indent=""):
    r = sec.get("resources") or {}
    print(f"{indent}sector {loc[0]},{loc[1]} [{freshness(sec, now)}]: {sec['terrain']}  ore metal={r.get('metal')} crystal={r.get('crystal')} deut={r.get('deuterium')}")
    if not sec.get("live"):
        print(f"{indent}(remembered: ore, hostiles, salvage and derelict below are as last seen, not current)")
    exits = []
    for c in sec.get("connections") or []:
        d = DIRS.get((c["q"] - loc[0], c["r"] - loc[1]), "?")
        known = sectors.get((c["q"], c["r"]))
        tag = ""
        if known:
            if known.get("hostiles"):
                tag = f" hostiles:{len(known['hostiles'])}"
            elif known.get("terrain"):
                tag = f" {known['terrain']}"
            if not known.get("live"):
                tag += f" (stale {age(now - known.get('last_seen', now))})"
        exits.append(f"{d}->{c['q']},{c['r']}{tag}")
    print(f"{indent}exits: " + (", ".join(exits) or "none"))
    for h in sec.get("hostiles") or []:
        print(f"{indent}HOSTILE fleet id {h['id']} {h['behavior']} (attack target_fleet_id {h['id']}): " + json.dumps(h.get("ships"), separators=(",", ":"))[:200])
    for pf in sec.get("player_fleets") or []:
        print(f"{indent}player fleet {pf['id']} of {pf['owner_name']}: {pf['ship_count']} ships")
    if sec.get("salvage"):
        left = ""
        if sec.get("salvage_despawn_tick") is not None:
            t = sec["salvage_despawn_tick"] - now
            left = f", despawns in {t}s" if t > 0 else ", despawn time passed (probably gone)"
        print(f"{indent}salvage: {res(sec['salvage'])}{left}")
    if sec.get("site"):
        print(f"{indent}derelict site tier {sec['site']['tier']} risk {sec['site']['risk']}")


def cmd_costs(name, what=None):
    """Costs, build times and unmet requirements from the server's catalog."""
    fs = fresh_state(name)
    header(name, fs["tick"] if fs else None)
    if not fs:
        print("No state received (server or client down?). Try again shortly.")
        return
    cat, stock = fs["homeworld"]["catalog"], fs["player"]["resources"]

    def need(requires):
        unmet = [f"{r['name']} {r['need']} (have {r['have']})" for r in requires if not r["met"]]
        return f"  LOCKED: needs {', '.join(unmet)}" if unmet else ""

    def afford(cost):
        return "" if all(stock[k] >= cost.get(k, 0) for k in stock) else "  cannot pay yet"

    sections = (
        ("buildings", [(o["building_type"], f"L{o['level']}", o.get("next"), o["requires"]) for o in cat["buildings"]]),
        ("research", [(o["tech"], f"L{o['level']}/{o['max_level']}", o.get("next"), o["requires"]) for o in cat["research"]]),
        ("ships", [(o["ship_class"], "per ship", {"cost": o["unit_cost"], "ticks": o["ticks_per_ship"]}, o["requires"]) for o in cat["ships"]]),
        ("defences", [(o["kind"], f"{o['count']} standing, power {o['power']:.0f} each",
                       {"cost": o["unit_cost"], "ticks": o["ticks_per_unit"]}, o["requires"]) for o in cat.get("defences") or []]),
    )
    for title, rows in sections:
        if what and what != title:
            continue
        print(f"{title}:")
        for label, state, nxt, requires in rows:
            if nxt is None:
                print(f"  {label:<22} {state:<28} maxed")
                continue
            lvl = f" -> L{nxt['level']}" if "level" in nxt else ""
            print(f"  {label:<22} {state:<28}{lvl} costs {res(nxt['cost'])} in {age(nxt['ticks'])}{afford(nxt['cost'])}{need(requires)}")


def cmd_score(name, limit=10):
    start = log_size(name)
    send(name, {"action": "leaderboard", "limit": int(limit)})
    board = None
    for _ in range(20):
        time.sleep(0.4)
        lines, _ = log_lines(name, start)
        board = next((m for m in lines if m.get("type") == "leaderboard"), None)
        if board:
            break
    header(name, last_tick(name))
    if not board:
        print("No answer from the server yet. Try again shortly.")
        return
    print("rank  name                  score    core  combat  explore   (one table for humans and AI agents)")
    for e in board["entries"] + ([board["you"]] if board.get("you") else []):
        tag = "AI " if e["agent"] else "   "
        print(f"{e['rank']:>4}  {e['name']:<20} {tag}{e['score']:>7.1f} {e['core']:>7.1f} {e['combat']:>7.1f} {e['explore']:>8.1f}")


def cmd_status(name):
    fs = fresh_state(name)
    header(name, fs["tick"] if fs else last_tick(name))
    if not fs:
        print("No state received (server or client down?). Try again shortly.")
        return
    summarize(fs)


def cmd_sector(name, q, r):
    fs = fresh_state(name)
    header(name, fs["tick"] if fs else None)
    sectors = {(s["location"]["q"], s["location"]["r"]): s for s in (fs or {}).get("known_sectors") or []}
    if not fs:
        print("No state received (server or client down?). Try again shortly.")
        return
    sec = sectors.get((int(q), int(r)))
    if not sec:
        print(f"sector {q},{r} was never seen by you (scan or visit it)")
        return
    describe(sec, (int(q), int(r)), sectors, fs["tick"])


def cmd_map(name, radius=4):
    fs = fresh_state(name)
    header(name, fs["tick"] if fs else None)
    if not fs:
        return
    sectors = {(s["location"]["q"], s["location"]["r"]): s for s in fs.get("known_sectors") or []}
    centers = [(f["location"]["q"], f["location"]["r"]) for f in fs.get("fleets") or []] or [
        (fs["homeworld"]["location"]["q"], fs["homeworld"]["location"]["r"])]
    def dist(a, b):
        dq, dr = a[0] - b[0], a[1] - b[1]
        return (abs(dq) + abs(dr) + abs(dq + dr)) // 2
    shown = sorted(k for k in sectors if min(dist(k, c) for c in centers) <= int(radius))
    now = fs["tick"]
    live = sum(1 for s in sectors.values() if s.get("live"))
    print(f"{len(shown)} known sectors within {radius} of your fleets "
          f"(of {len(sectors)} ever seen: {live} live, {len(sectors) - live} remembered). "
          "LIVE = current; STALE Nm = as last seen N ago.")
    for k in shown:
        s = sectors[k]
        r = s.get("resources") or {}
        bits = [freshness(s, now), s["terrain"], f"m={r.get('metal')} c={r.get('crystal')} d={r.get('deuterium')}"]
        if s.get("hostiles"): bits.append(f"HOSTILES x{len(s['hostiles'])}")
        if s.get("player_fleets"): bits.append("players: " + ",".join(p["owner_name"] for p in s["player_fleets"]))
        if s.get("salvage"): bits.append("salvage")
        if s.get("site"): bits.append(f"derelict t{s['site']['tier']} {s['site']['risk']}")
        print(f"  {k[0]},{k[1]}: " + "  ".join(bits))


def cmd_do(name, payload):
    try:
        obj = json.loads(payload)
    except json.JSONDecodeError as e:
        header(name)
        print(f"not valid JSON: {e}")
        return
    start = log_size(name)
    send(name, obj)
    time.sleep(2.5)
    lines, _ = log_lines(name, start)
    header(name, last_tick(name))
    print(f"sent: {json.dumps(obj, separators=(',', ':'))}")
    print_digest(digest(lines))


def cmd_events(name, mine_only=False, advance=True):
    start = cursor(name)
    lines, end = log_lines(name, start)
    header(name, last_tick(name))
    print_digest(digest(lines, mine_only))
    if advance:
        cursor(name, end)


def cmd_wait(name, secs, mine_only=False):
    secs = max(5, min(int(secs), 90))
    time.sleep(secs)
    print(f"(waited {secs}s)")
    cmd_events(name, mine_only)


def daemon(name):
    """Run a headless client for NAME with a FIFO for stdin; restarts on exit."""
    for d in (LOGS, FIFOS, STATE):
        d.mkdir(parents=True, exist_ok=True)
    fifo = FIFOS / f"{name}.in"
    if not fifo.exists():
        os.mkfifo(fifo)
    env = dict(os.environ, XDG_CONFIG_HOME=str(LIVE / "cfg"))
    keeper = os.open(fifo, os.O_RDWR)  # read-write: writers never block and EOF never hits
    while True:
        with open(LOGS / f"{name}.ndjson", "ab") as out, open(LOGS / f"{name}.stderr", "ab") as err:
            p = subprocess.Popen([str(CLIENT), "--headless", "--port", str(PORT), "--name", name],
                                 stdin=keeper, stdout=out, stderr=err, env=env)
            p.wait()
        time.sleep(2)


USAGE = """usage: play <command>
  status                 your empire, queues, fleets and the sector each fleet is in
  costs [buildings|research|ships|defences]  what each upgrade costs, how long it takes, what is locked
  score [rows]           the leaderboard (default 10 rows); your own row is added if you are below it
  map [radius]           known sectors near your fleets (default radius 4)
  sector Q R             details of one known sector
  do '<json>'            send a command, e.g. play do '{"action":"scan","fleet_id":2}'
                         or standing orders: play do '{"type":"policy_update","fleet_id":2,"preset":"prospect"}'
                         or split ships off: play do '{"action":"split","fleet_id":2,"ship_ids":[41,42]}'
                         or check a jump's fuel: play do '{"action":"preview_move","fleet_id":2,"target":{"q":3,"r":-1}}'
  events [--mine]        events since you last looked; --mine drops fights you only witnessed
  wait [seconds] [--mine] sleep (5-90s, default 45) then show events; use while timers run
  rules                  print RULES.md"""


def main():
    a = sys.argv[1:]
    if a[:1] == ["daemon"]:
        return daemon(a[1])
    name = os.environ.get("IAC_PLAYER")
    if not name or not a:
        print(USAGE)
        return
    cmd, rest = a[0], a[1:]
    if cmd == "status": cmd_status(name)
    elif cmd == "costs": cmd_costs(name, *rest[:1])
    elif cmd == "score" or cmd == "leaderboard": cmd_score(name, *rest[:1])
    elif cmd == "map": cmd_map(name, *rest[:1])
    elif cmd == "sector" and len(rest) == 2: cmd_sector(name, *rest)
    elif cmd == "do" and rest: cmd_do(name, " ".join(rest))
    elif cmd == "events": cmd_events(name, "--mine" in rest)
    elif cmd == "wait":
        nums = [x for x in rest if x != "--mine"]
        cmd_wait(name, nums[0] if nums else 45, "--mine" in rest)
    elif cmd == "rules": print((HERE / "RULES.md").read_text())
    else: print(USAGE)


if __name__ == "__main__":
    main()
