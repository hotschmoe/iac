#!/usr/bin/env python3
"""Agent-facing CLI for a live In Amber Clad session.

Each player has a long-lived headless client (started with `play.py daemon NAME`)
whose stdin is a FIFO and whose stdout is appended to logs/NAME.ndjson.
Agents call `play <subcommand>` from their player directory.

The server answers commands at its next tick (about once a second): errors and
state changes arrive together in that tick. Read-only commands (state, status,
costs, map, sector) never wait: they read a view kept incrementally from the log.
"""
import json, os, re, sys, time, subprocess, pathlib, threading

HERE = pathlib.Path(__file__).resolve().parent
LIVE = pathlib.Path(os.environ.get("IAC_RUN") or HERE / "run").resolve()
LOGS, FIFOS, STATE = LIVE / "logs", LIVE / "fifo", LIVE / "state"
CLIENT = HERE.parent.parent / "target/release/iac-client"
PORT = int(os.environ.get("IAC_PORT", "7777"))
DIRS = {(1, 0): "E", (1, -1): "NE", (0, -1): "NW", (-1, 0): "W", (-1, 1): "SW", (0, 1): "SE"}
RES = ("metal", "crystal", "deuterium")
WEIGHT = {"metal": 1.0, "crystal": 1.5, "deuterium": 2.0}
WAIT_DEFAULT, WAIT_MAX = 45, 60
SEND_GAP = 0.25
ACK_CAP = 1.6
SCORE_RULE = ("score = core + min(combat + explore, 25% of core); core = built levels, living ships and half of "
              "defences at cost (metal 1: crystal 1.5: deut 2). Unspent stock scores 0: spend it. Kills pay in full "
              "up to 4x your strength and less above; repeat kills in one sector pay less; charts pay when a fleet "
              "docks with them; derelicts pay on first boarding.")
QUEUE_NAMES = {"build": "Building", "research": "Research", "ship": "Ship"}
ALERT_LEVELS = ("Warning", "Critical")
NOTABLE = {"BuildingCompleted", "ResearchCompleted", "ShipBuilt", "DefenceBuilt", "FleetArrived", "CombatStarted",
           "ShipDestroyed", "FleetDestroyed", "RaidIncoming", "RaidResolved", "SiteExplored", "SiteAmbush",
           "SalvageCollected", "ChartDelivered", "ScanCompleted"}


def session():
    try:
        return json.loads((LIVE / "session.json").read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def minutes_left():
    s = session()
    return (s.get("end", 0) - time.time()) / 60 if s else None


def header(name, tick=None):
    s = session()
    left = minutes_left()
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
        try:
            return int(p.read_text())
        except (FileNotFoundError, ValueError):
            return 0
    p.write_text(str(value))


def send(name, obj):
    with open(FIFOS / f"{name}.in", "w") as f:
        f.write(json.dumps(obj) + "\n")
    with open(STATE / f"{name}.actions", "a") as f:
        f.write(json.dumps({"t": time.time(), "msg": obj}) + "\n")


def merge_view(fs, m):
    """Fold one logged server message into the running view of the game."""
    t = m.get("type")
    if t == "full_state":
        return m
    if fs is None:
        return fs
    if t == "tick_update":
        fs["tick"] = m["tick"]
        for k, dst in (("player", "player"), ("homeworld_update", "homeworld")):
            if m.get(k) is not None:
                fs[dst] = m[k]
        fs["fleets"] = m.get("fleets", fs.get("fleets"))
        if m.get("sector_updates"):
            idx = {(s["location"]["q"], s["location"]["r"]): i for i, s in enumerate(fs["known_sectors"])}
            for s in m["sector_updates"]:
                k = (s["location"]["q"], s["location"]["r"])
                if k in idx:
                    fs["known_sectors"][idx[k]] = s
                else:
                    idx[k] = len(fs["known_sectors"])
                    fs["known_sectors"].append(s)
    return fs


def view(name):
    """The latest game state, rebuilt incrementally from the log (no waiting).

    Cached in state/NAME.view.json together with the log offset it covers and the
    newest leaderboard the daemon fetched. Returns (full_state-shaped dict, board).
    """
    cache = STATE / f"{name}.view.json"
    fs, board, off = None, None, 0
    try:
        c = json.loads(cache.read_text())
        fs, board, off = c["fs"], c.get("board"), c["off"]
        if off > log_size(name):
            fs, board, off = None, None, 0
    except (FileNotFoundError, json.JSONDecodeError, KeyError):
        pass
    lines, end = log_lines(name, off)
    for m in lines:
        if m.get("type") == "leaderboard":
            board = m
        fs = merge_view(fs, m)
    if lines or not cache.exists():
        tmp = cache.with_suffix(".tmp")
        tmp.write_text(json.dumps({"fs": fs, "board": board, "off": end}))
        os.replace(tmp, cache)
    return fs, board


def last_tick(name):
    fs, _ = view(name)
    return fs["tick"] if fs else None


def no_state(name):
    header(name)
    print("No state received (server or client down?). Try again shortly.")


def raw_events(msgs):
    """Every game event in a batch of logged messages, in order."""
    out = []
    for m in msgs:
        if m.get("type") == "tick_update":
            out.extend(m.get("events") or [])
        elif m.get("type") == "event":
            out.append(m)
    return out


def is_alert(e):
    k = e.get("kind", "")
    if k == "Alert":
        return e.get("level") in ALERT_LEVELS
    if k == "FleetDestroyed":
        return bool(e.get("mine")) and not e.get("is_npc")
    return k in ("RaidIncoming", "RaidResolved", "StorageFull", "StorageNearCap", "SiteAmbush") or k.endswith("Warning")


def hexs(h):
    return f"{h['q']},{h['r']}" if isinstance(h, dict) and "q" in h else "?"


def scan_line(group):
    n = group["n"]
    seen = {}
    for s in group["signals"]:
        key = (hexs(s["sector"]), s["signal"])
        seen[key] = max(seen.get(key, 0), s.get("threat_band", 0))
    kinds = {}
    for (at, kind), band in sorted(seen.items()):
        kinds.setdefault(kind, []).append((at, band))
    sigs = ", ".join(f"{len(v)} {k}" + (f" @{','.join(a for a, _ in v)}" if len(v) <= 2 else "")
                     + f" (band {'/'.join(str(b) for b in sorted({b for _, b in v}))})" for k, v in sorted(kinds.items()))
    worst = max([t["threat"]["rating"] for t in group["threats"]] or [0])
    head = f"scan{' x%d' % n if n > 1 else ''} fleet {group['fleet']} @{group['at']}: {group['revealed']} sectors revealed, " \
           f"{group['hostiles']} hostile{'s' if group['hostiles'] != 1 else ''}" + (f", worst T{worst}" if worst else "")
    return head + (f"; signals: {sigs}" if sigs else "; no signals") + f" (tick {group['t0']}{'-%d' % group['t1'] if group['t1'] != group['t0'] else ''})"


def collapse_events(msgs, mine_only=False, errors=True):
    """Compact text lines for a batch of logged messages.

    Scans collapse to one line per fleet, combat rounds to one line per sector,
    repeated autopilot holds and identical lines to a count; completions are
    short sentences. Errors and replies keep their data.
    """
    items = []
    scans, fights, built, policy = {}, {}, {}, {}
    seen = {}

    def slot(key, line=None):
        if key not in seen:
            seen[key] = len(items)
            items.append(line)
        return seen[key]

    for m in msgs:
        t = m.get("type")
        if t in ("error", "client_error"):
            if errors:
                slot(("x", id(m)), "  ERROR " + " ".join(x for x in (m.get("code"), m.get("message")) if x))
            continue
        if t == "connection_closed":
            slot(("x", id(m)), "  CONNECTION CLOSED")
            continue
        if t == "preview_move":
            if not errors:
                continue
            slot(("x", id(m)), "  " + preview_line(m))
            continue
        if t == "leaderboard":
            continue
        for e in (m.get("events") or []) if t == "tick_update" else ([m] if t == "event" else []):
            k = e.get("kind")
            if mine_only and e.get("mine") is False:
                continue
            tick = e.get("tick", 0)
            if k == "ScanCompleted":
                g = scans.setdefault(e["fleet_id"], {"n": 0, "fleet": e["fleet_id"], "at": hexs(e["sector"]), "revealed": 0,
                                                     "hostiles": 0, "signals": [], "threats": [], "t0": tick, "t1": tick})
                g["n"] += 1; g["t1"] = tick; g["at"] = hexs(e["sector"])
                g["revealed"] += e.get("sectors_revealed", 0); g["hostiles"] += e.get("hostiles_detected", 0)
                g["signals"] += e.get("signals") or []; g["threats"] += e.get("threats") or []
                slot(("scan", e["fleet_id"]))
            elif k == "CombatRound":
                key = (e["sector"]["q"], e["sector"]["r"])
                g = fights.setdefault(key, {"t0": tick, "t1": tick, "shots": 0, "mine": False, "took": 0.0, "dealt": 0.0, "others": 0.0})
                g["t1"] = tick; g["shots"] += 1; g["mine"] = g["mine"] or bool(e.get("mine"))
                f = ("took" if e.get("target_owner") is not None else "dealt") if e.get("mine") else "others"
                g[f] += e.get("hull_damage", 0.0)
                slot(("fight", key))
            elif k == "PolicyAction":
                key = (e["fleet_id"], e.get("preset"), e.get("action"), e.get("reason"))
                g = policy.setdefault(key, {"n": 0, "t0": tick, "t1": tick})
                g["n"] += 1; g["t1"] = tick
                slot(("policy", key))
            elif k == "ShipBuilt":
                built[e["ship_class"]] = built.get(e["ship_class"], 0) + e.get("count", 1)
                slot(("built", e["ship_class"]))
            else:
                slot(("line", len(items)), f"  t{tick} {event_text(e)}")
    for key, i in list(seen.items()):
        if key[0] == "scan":
            items[i] = "  " + scan_line(scans[key[1]])
        elif key[0] == "fight":
            g = fights[key[1]]
            span = f"ticks {g['t0']}-{g['t1']}" if g["t1"] != g["t0"] else f"tick {g['t0']}"
            who = (f"you took {g['took']:.0f} hull, dealt {g['dealt']:.0f}" if g["mine"] else f"witnessed, {g['others']:.0f} hull damage")
            items[i] = f"  combat @{key[1][0]},{key[1][1]} {span}: {g['shots']} shots, {who}"
        elif key[0] == "policy":
            g, f = policy[key[1]], key[1]
            span = f"x{g['n']}, ticks {g['t0']}-{g['t1']}" if g["n"] > 1 else f"tick {g['t0']}"
            items[i] = f"  fleet {f[0]} {f[1]} {f[2]}: {f[3]} ({span})"
        elif key[0] == "built":
            items[i] = f"  built {built[key[1]]}x {key[1]}"
    out = []
    for line in items:
        if line is None:
            continue
        if out and out[-1][0] == line:
            out[-1][1] += 1
        else:
            out.append([line, 1])
    return [l + (f" (x{n})" if n > 1 else "") for l, n in out]


def event_text(e):
    k = e.get("kind", "?")
    if k == "BuildingCompleted":
        return f"built {e['building_type']} L{e['new_level']}"
    if k == "ResearchCompleted":
        return f"researched {e['tech']} L{e['new_level']}"
    if k == "DefenceBuilt":
        return f"defence built {e['count']}x {e['defence']}"
    if k == "FleetArrived":
        return f"fleet {e['fleet_id']} arrived at {hexs(e['sector'])}"
    if k == "SectorEntered":
        return f"fleet {e['fleet_id']} entered {hexs(e['sector'])}" + (" (first visit)" if e.get("first_visit") else "")
    if k == "Alert":
        where = f" @{hexs(e['sector'])}" if e.get("sector") else ""
        return f"ALERT[{e.get('level')}]{where} {e.get('message')}"
    if k == "PolicyAction":
        return f"fleet {e['fleet_id']} {e.get('preset')} {e.get('action')}: {e.get('reason')}"
    if k == "ResourceHarvested":
        return f"fleet {e['fleet_id']} harvested {res(e.get('resources'))} in {e.get('ticks')}t"
    rest = {a: b for a, b in e.items() if a not in ("kind", "tick", "player_id")}
    return k + " " + json.dumps(rest, separators=(",", ":"))[:220]


def print_events(lines, limit=40):
    if not lines:
        print("  (no new events)")
        return
    if len(lines) > limit:
        print(f"  ... {len(lines) - limit} earlier lines omitted")
    for line in lines[-limit:]:
        print(line)


def preview_line(m):
    t = m["threat"]
    return (f"preview fleet {m['fleet_id']} -> {m['target']['q']},{m['target']['r']}: fuel {m['fuel_cost']:.0f} "
            f"(leaves {m['fuel_after']:.0f}), {m['hops_home']} hops home need {m['fuel_to_return']:.0f}"
            + (f" (charted route {m['charted_hops_home']} hops)"
               if m.get("charted_hops_home") not in (None, m["hops_home"]) else "")
            + (" across unexplored space" if m.get("route_unexplored") else "")
            + ("" if m["can_jump"] else "  CANNOT JUMP: not enough fuel")
            + ("" if m["can_return"] else "  CANNOT RETURN afterwards")
            + f"; T{t['rating']} NPC power {t['est_power']:.0f} ({t['basis']}), your power {m['fleet_power']:.0f}, "
              f"ratio {m['ratio']:.1f}: {str(m['label']).upper()}")


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


def verdict(ratio):
    """The server's ratio labels: fleet power over the sector's threat power."""
    return ("SAFE" if ratio >= 2.5 else "FAVOURABLE" if ratio >= 1.5 else "EVEN" if ratio >= 1.1
            else "RISKY" if ratio >= 0.9 else "DEADLY")


def threat_text(t, power=None):
    """'T4 (power 46, observed)', plus the odds when you pass your fleet's power."""
    if not t:
        return "T? (no rating)"
    text = f"T{t['rating']} (power {t['est_power']:.0f}, {t['basis']})"
    if power:
        ratio = power / max(t["est_power"], 1e-6)
        text += f"; your {power:.0f} is {ratio:.1f}x: {verdict(ratio)}"
    return text


def ore_text(reserve, now_age=0):
    """Units left on each tile and when a stripped one is full again."""
    if not reserve:
        return None
    parts = []
    for k in ("metal", "crystal", "deuterium"):
        t = reserve[k]
        if t["max_units"] <= 0:
            continue
        refill = f", full in {age(t['refills_in_s'] - now_age)}" if t.get("refills_in_s") else ""
        parts.append(f"{k} {t['units']:.1f}/{t['max_units']:.1f}{refill}")
    return "; ".join(parts) or None


def best_power(fs):
    return max([f.get("power", 0) for f in fs.get("fleets") or []] or [0])


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
    waiting_max = hw.get("queue_waiting_max", 3)
    running = hw.get("build_queue") or []
    print(f"Build queue ({len(running)}/{hw.get('build_slots', 1)} slots, {waiting_max} may wait): "
          + ("; ".join(f"{q['building_type']} -> L{q['target_level']} done in {age(q['end_tick'] - t)}" for q in running)
             or ("none running" if hw.get("build_pending") else "idle")))
    for i, q in enumerate(hw.get("build_pending") or []):
        print(f"  waiting [{i}]: {q['building_type']} -> L{q['target_level']} ({age(q['ticks'])}){waiting(q.get('waiting_for'))}")
    q = hw.get("research_active")
    print("Research queue: " + (f"{q['tech']} -> L{q['target_level']} done in {age(q['end_tick'] - t)}" if q
                                else "none running" if hw.get("research_pending") else "idle"))
    for i, q in enumerate(hw.get("research_pending") or []):
        print(f"  waiting [{i}]: {q['tech']} -> L{q['target_level']} ({age(q['ticks'])}){waiting(q.get('waiting_for'))}")
    q = hw.get("shipyard_queue")
    print("Shipyard queue: " + (f"{q['item']} {q['built']}/{q['count']}, next unit in {age(q['end_tick'] - t)}" if q
                                else "none running" if hw.get("shipyard_pending") else "idle"))
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
            describe(sec, loc, sectors, fs["tick"], indent="  ", power=f.get("power"))


def describe(sec, loc, sectors, now, indent="", power=None):
    r = sec.get("resources") or {}
    print(f"{indent}sector {loc[0]},{loc[1]} [{freshness(sec, now)}]: {sec['terrain']}  ore metal={r.get('metal')} crystal={r.get('crystal')} deut={r.get('deuterium')}")
    print(f"{indent}threat {threat_text(sec.get('threat'), power)}")
    ore = ore_text(sec.get("ore_reserve"), 0 if sec.get("live") else max(now - sec.get("last_seen", now), 0))
    if ore:
        print(f"{indent}ore reserve (raw units{'' if sec.get('live') else ', as last seen'}): {ore}")
    if not sec.get("live"):
        print(f"{indent}(remembered: ore, hostiles, salvage and derelict below are as last seen, not current)")
    exits = []
    for c in sec.get("connections") or []:
        d = DIRS.get((c["q"] - loc[0], c["r"] - loc[1]), "?")
        known = sectors.get((c["q"], c["r"]))
        tag = ""
        if known:
            rating = f" T{known['threat']['rating']}" if known.get("threat") else ""
            if known.get("hostiles"):
                tag = f" hostiles:{len(known['hostiles'])}{rating}"
            elif known.get("terrain"):
                tag = f" {known['terrain']}{rating}"
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




def norm(s):
    return "".join(ch for ch in str(s).lower() if ch.isalnum())


def short_cover(short, prod):
    """Seconds of current production until a shortfall is covered; None if never."""
    need = 0.0
    for k in RES:
        d = (short or {}).get(k, 0)
        if d > 0:
            if prod.get(k, 0) <= 0:
                return None
            need = max(need, d / prod[k])
    return need


def queues_of(fs):
    """The three homeworld queues as plain dicts: running items, waiting items, room."""
    hw, t = fs["homeworld"], fs["tick"]
    prod = hw.get("production") or {}
    waiting_max = hw.get("queue_waiting_max", 3)

    def mk(slots, running, waiting):
        depth = slots + waiting_max
        room = max(depth - len(running) - len(waiting), 0)
        free = max(slots - len(running), 0) if not waiting else 0
        return {"slots": slots, "depth": depth, "running": running, "waiting": waiting, "room": room, "open": free}

    def wait_item(i, item, to, each, w, q=None):
        d = {"index": i, "item": item, "ticks": each}
        if q and q.get("waiting_on"):
            d["waiting_on"] = q["waiting_on"]
        if q and q.get("start_in") is not None:
            d["start_in_s"] = q["start_in"]
        if to is not None:
            d["to"] = to
        short = {k: round(w[k]) for k in RES if (w or {}).get(k, 0) > 0.5}
        if short:
            d["short"] = short
            cover = short_cover(w, prod)
            if cover is not None:
                d["covered_in_s"] = round(cover)
        return d

    run_b = [{"item": q["building_type"], "to": q["target_level"], "eta_s": max(q["end_tick"] - t, 0)} for q in hw.get("build_queue") or []]
    wait_b = [wait_item(i, q["building_type"], q["target_level"], q["ticks"], q.get("waiting_for"), q) for i, q in enumerate(hw.get("build_pending") or [])]
    ra = hw.get("research_active")
    run_r = [{"item": ra["tech"], "to": ra["target_level"], "eta_s": max(ra["end_tick"] - t, 0)}] if ra else []
    wait_r = [wait_item(i, q["tech"], q["target_level"], q["ticks"], q.get("waiting_for"), q) for i, q in enumerate(hw.get("research_pending") or [])]
    sq = hw.get("shipyard_queue")
    run_s = [{"item": sq["item"], "done": sq["built"], "of": sq["count"], "eta_s": max(sq["end_tick"] - t, 0)}] if sq else []
    wait_s = []
    for i, q in enumerate(hw.get("shipyard_pending") or []):
        d = wait_item(i, q["item"], None, q["ticks"], q.get("waiting_for"), q)
        d["count"] = q["count"]
        wait_s.append(d)
    return {"build": mk(hw.get("build_slots", 1), run_b, wait_b), "research": mk(1, run_r, wait_r), "ship": mk(1, run_s, wait_s)}


def catalog_rows(fs):
    """(section, label, level text, step, requires, extra) for every catalog entry."""
    cat = fs["homeworld"]["catalog"]
    rows = []
    for o in cat["buildings"]:
        rows.append(("buildings", o["building_type"], o["level"], o.get("max_level"), o.get("next"), o["requires"]))
    for o in cat["research"]:
        rows.append(("research", o["tech"], o["level"], o.get("max_level"), o.get("next"), o["requires"]))
    for o in cat["ships"]:
        rows.append(("ships", o["ship_class"], None, None, {"cost": o["unit_cost"], "ticks": o["ticks_per_ship"]}, o["requires"]))
    for o in cat.get("defences") or []:
        rows.append(("defences", o["kind"], o["count"], None, {"cost": o["unit_cost"], "ticks": o["ticks_per_unit"]}, o["requires"]))
    return rows


def affordable(fs):
    """Per queue, the catalog items that are unlocked and payable from the stockpile now."""
    stock = fs["player"]["resources"]
    qmap = {"buildings": "build", "research": "research", "ships": "ship", "defences": "ship"}
    out = {"build": set(), "research": set(), "ship": set()}
    for section, label, _lvl, _mx, step, requires in catalog_rows(fs):
        if step is None or any(not r["met"] for r in requires):
            continue
        if all(stock.get(k, 0) >= step["cost"].get(k, 0) for k in RES):
            out[qmap[section]].add(label)
    return out


def fleet_rows(fs):
    out = []
    sectors = {(s["location"]["q"], s["location"]["r"]): s for s in fs.get("known_sectors") or []}
    for f in fs.get("fleets") or []:
        ships = {}
        hull = hull_max = 0
        for s in f["ships"]:
            ships[s["class"]] = ships.get(s["class"], 0) + 1
            hull += s["hull"]; hull_max += s["hull_max"]
        loc = [f["location"]["q"], f["location"]["r"]]
        row = {"id": f["id"], "at": loc, "state": f["state"], "policy": f.get("policy") or "manual",
               "fuel": round(f["fuel"]), "fuel_max": round(f["fuel_max"]), "range_hops": f.get("range_hops"),
               "power": round(f.get("power", 0)), "cooldown": f.get("cooldown_remaining", 0),
               "cargo": [round((f.get("cargo") or {}).get(k, 0)) for k in RES], "cargo_cap": round(f.get("cargo_capacity", 0)),
               "hull_pct": round(100 * hull / hull_max) if hull_max else 0, "ships": ships,
               "jump_fuel": round(f.get("jump_fuel", 0)), "home_fuel": round(f.get("home_fuel", 0))}
        if f.get("cargo_blocked"):
            row["cargo_blocked"] = True
        sec = sectors.get(tuple(loc))
        if sec:
            row["exits"] = [DIRS.get((c["q"] - loc[0], c["r"] - loc[1]), "?") + f"->{c['q']},{c['r']}" for c in sec.get("connections") or []]
            if sec.get("threat"):
                row["sector_threat"] = sec["threat"]["rating"]
            if sec.get("hostiles"):
                row["hostiles"] = [{"id": h["id"], "behavior": h["behavior"]} for h in sec["hostiles"]]
        out.append(row)
    return out


def derived_alerts(fs, fleets):
    out = []
    hw = fs["homeworld"]
    st = hw["storage"]
    for k in RES:
        cap = st["cap"].get(k, 0)
        have = fs["player"]["resources"].get(k, 0)
        if k in st.get("capped", []):
            out.append(f"{k} stockpile FULL: production is being wasted, spend it")
        elif cap and have / cap >= 0.85:
            out.append(f"{k} stockpile at {100 * have / cap:.0f}% of cap")
    if hw.get("next_raid_estimate_power", 0) > hw.get("home_defence_power", 0):
        out.append(f"expected raid power {hw['next_raid_estimate_power']:.0f} exceeds home defence {hw.get('home_defence_power', 0):.0f}")
    for f in fleets:
        if f.get("cargo_blocked"):
            out.append(f"fleet {f['id']} cargo blocked: stockpile full")
        if f["home_fuel"] and f["fuel"] < f["jump_fuel"]:
            out.append(f"fleet {f['id']} stranded: fuel {f['fuel']} below one jump")
        elif f["home_fuel"] > f["fuel"]:
            out.append(f"fleet {f['id']} cannot reach home: fuel {f['fuel']} < {f['home_fuel']}")
    for w in (fs.get("warnings") or fs["player"].get("warnings") or []):
        out.append(w if isinstance(w, str) else json.dumps(w, separators=(",", ":")))
    return out


def recent_alerts(name, tick, window=300):
    lines, _ = log_lines(name, max(0, log_size(name) - 400000))
    seen, out = set(), []
    for e in raw_events(lines):
        if is_alert(e) and e.get("tick", 0) >= tick - window:
            text = f"t{e.get('tick')} {event_text(e)}"
            if text not in seen:
                seen.add(text)
                out.append(text)
    return out[-8:]


def score_of(fs, board):
    """Own row of the cached leaderboard, with rank out of the players listed."""
    if not board:
        return None
    me = fs["player"]["name"]
    rows = list(board.get("entries") or []) + ([board["you"]] if board.get("you") else [])
    mine = next((e for e in rows if e["name"] == me), None)
    if not mine:
        return None
    total = max([e["rank"] for e in rows] or [0])
    return {"score": round(mine["score"], 1), "rank": mine["rank"], "of": total, "core": round(mine["core"], 1),
            "combat": round(mine["combat"], 1), "explore": round(mine["explore"], 1),
            "age_s": max(fs["tick"] - board.get("tick", fs["tick"]), 0),
            "bonus_capped": mine["combat"] + mine["explore"] > 0.25 * mine["core"]}


def build_state(fs, board=None, left=None, alerts=None):
    """The single machine-readable document an agent loop reads."""
    hw, pl = fs["homeworld"], fs["player"]
    qs = queues_of(fs)
    fleets = fleet_rows(fs)
    st = hw["storage"]
    nxt = {}
    for section, label, lvl, mx, step, requires in catalog_rows(fs):
        d = {}
        if lvl is not None:
            d["level"] = lvl
        if step is None:
            d["maxed"] = True
        else:
            d["to"] = step.get("level")
            d["cost"] = [round(step["cost"].get(k, 0)) for k in RES]
            d["secs"] = step["ticks"]
        unmet = [f"{r['name']} {r['need']}" for r in requires if not r["met"]]
        if unmet:
            d["locked"] = unmet
        elif step is not None:
            d["ok"] = all(pl["resources"].get(k, 0) >= step["cost"].get(k, 0) for k in RES)
        nxt.setdefault(section, {})[label] = d
    idle = [n for n, q in qs.items() if q["open"] > 0]
    docked = {}
    for s in hw.get("docked_ships") or []:
        docked[s["class"]] = docked.get(s["class"], 0) + 1
    world = fs.get("world") or {}
    doc = {
        "player": pl["name"], "tick": fs["tick"],
        "minutes_left": None if left is None else round(max(left, 0), 1),
        "score": score_of(fs, board), "score_rule": (board or {}).get("rule") or SCORE_RULE,
        "stock": {k: round(pl["resources"].get(k, 0), 1) for k in RES},
        "cap": {k: round(st["cap"].get(k, 0)) for k in RES},
        "per_s": {k: round(hw["production"].get(k, 0), 4) for k in RES},
        "full_in_s": {k: v for k, v in (st.get("full_in_s") or {}).items() if v},
        "levels": {"buildings": {b["building_type"]: b["level"] for b in hw["buildings"]},
                   "research": {r["tech"]: r["level"] for r in hw["research"]}},
        "queues": qs, "idle": idle, "next": nxt, "fleets": fleets, "docked": docked,
        "defence": {"power": round(hw.get("home_defence_power", 0)), "raid_estimate": round(hw.get("next_raid_estimate_power", 0))},
        "world": {"pace": world.get("pace"), "preset": world.get("preset")},
        "alerts": derived_alerts(fs, fleets) + list(alerts or []),
    }
    return doc


def queue_line(label, q):
    run = "; ".join(f"{r['item']}" + (f"->L{r['to']}" if "to" in r else f" {r['done']}/{r['of']}") + f" done in {age(r['eta_s'])}" for r in q["running"])
    parts = [f"{label} {len(q['running'])}/{q['slots']} running: " + (run or "IDLE")]
    for w in q["waiting"]:
        what = f"{w.get('count', '')}x " if "count" in w else ""
        to = f"->L{w['to']}" if "to" in w else ""
        need = ""
        if w.get("short"):
            need = " needs " + ", ".join(f"{v} {k}" for k, v in w["short"].items())
            need += f" (~{age(w['covered_in_s'])} of production)" if "covered_in_s" in w else " (no income of it)"
        parts.append(f"waiting[{w['index']}] {what}{w['item']}{to}{need}")
    if q["room"] and not q["open"]:
        parts.append(f"{q['room']} more fit")
    return " | ".join(parts)


def snapshot(fs):
    qs = queues_of(fs)
    aff = affordable(fs)
    return {"tick": fs["tick"],
            "q": {n: {"room": q["room"], "open": q["open"], "aff": aff[n]} for n, q in qs.items()},
            "fleets": {f["id"]: {"state": f["state"], "at": (f["location"]["q"], f["location"]["r"]),
                                 "auto": (f.get("policy") or "manual") != "manual", "blocked": bool(f.get("cargo_blocked"))}
                       for f in fs.get("fleets") or []},
            "busy": {n: len(q["running"]) + len(q["waiting"]) for n, q in qs.items()}}


QUIET = ("Idle", "Docked")


def wait_reasons(base, cur, events, until="any"):
    """Why a wait should return now: a list of reasons, empty to keep waiting.

    until: any (queue, fleet or alert), idle (queue or alert only), event (any
    notable event too), done (everything finished).
    """
    why = []
    for e in events:
        if is_alert(e):
            why.append(f"alert: {event_text(e)}")
        elif until == "event" and e.get("kind") in NOTABLE:
            why.append(event_text(e))
    for n, c in ({} if until == "done" else cur["q"]).items():
        b = base["q"][n]
        if c["room"] > b["room"]:
            why.append(f"{n} queue: an item finished, {c['room']} slot(s) free to queue")
        elif c["open"] > b["open"]:
            why.append(f"{n} queue idle")
        else:
            new = c["aff"] - b["aff"]
            if c["open"] > 0 and new:
                why.append(f"{n} queue idle and now payable: {', '.join(sorted(new))}")
    if until not in ("idle", "done"):
        for fid, f in cur["fleets"].items():
            b = base["fleets"].get(fid)
            if b is None:
                why.append(f"fleet {fid} appeared")
                continue
            if f["blocked"] and not b["blocked"]:
                why.append(f"fleet {fid} cargo blocked")
            if f["auto"]:
                if f["state"] == "InCombat" and b["state"] != "InCombat":
                    why.append(f"fleet {fid} in combat")
                continue
            if f["state"] in QUIET and (b["state"] not in QUIET or f["at"] != b["at"]):
                why.append(f"fleet {fid} {f['state'].lower()} at {f['at'][0]},{f['at'][1]}")
            elif f["state"] != b["state"] and b["state"] == "InCombat":
                why.append(f"fleet {fid} left combat ({f['state']})")
        for fid in base["fleets"]:
            if fid not in cur["fleets"]:
                why.append(f"fleet {fid} is gone")
    if until == "done":
        busy = sum(cur["busy"].values())
        moving = [fid for fid, f in cur["fleets"].items() if not f["auto"] and f["state"] not in QUIET]
        if busy == 0 and not moving:
            why.append("everything finished: queues empty, fleets idle")
    return why


def headline(name, doc):
    s = doc["score"]
    sc = (f"SCORE {s['score']:.1f} rank {s['rank']}/{s['of']} (core {s['core']:.1f}, combat {s['combat']:.1f}, explore {s['explore']:.1f}"
          + (", bonus CAPPED at 25% of core" if s["bonus_capped"] else "") + ")") if s else "SCORE unknown yet"
    idle = ", ".join(doc["idle"]) if doc["idle"] else "none"
    left = doc["minutes_left"]
    return (f"{sc} | {'' if left is None else '%.0f min left | ' % left}IDLE QUEUES: {idle}\n"
            f"Scoring: {doc['score_rule']}")


def fleet_line(f):
    cargo = sum(f["cargo"])
    return (f"fleet {f['id']} @{f['at'][0]},{f['at'][1]} {f['state']} {f['policy']} power {f['power']} fuel {f['fuel']}/{f['fuel_max']} "
            f"range {f['range_hops']} cargo {cargo}/{f['cargo_cap']} " + "+".join(f"{n}x{c}" for c, n in f["ships"].items())
            + (" CARGO BLOCKED" if f.get("cargo_blocked") else ""))


def cmd_state(name, pretty=False):
    fs, board = view(name)
    if not fs:
        return no_state(name)
    doc = build_state(fs, board, minutes_left(), recent_alerts(name, fs["tick"]))
    print(json.dumps(doc, indent=1 if pretty else None, separators=None if pretty else (",", ":")))


def cmd_status(name, brief=False):
    fs, board = view(name)
    if not fs:
        return no_state(name)
    header(name, fs["tick"])
    doc = build_state(fs, board, minutes_left(), recent_alerts(name, fs["tick"]))
    print(headline(name, doc))
    if brief:
        st = doc["stock"]
        print("Stock " + " ".join(f"{k[0]}={st[k]:.0f}/{doc['cap'][k]}" for k in RES) + "  income/s " + "/".join(f"{doc['per_s'][k]:.4g}" for k in RES)
              + f"  defence {doc['defence']['power']} vs raid ~{doc['defence']['raid_estimate']}")
        for n, q in doc["queues"].items():
            print("  " + queue_line(n, q))
        for f in doc["fleets"]:
            print("  " + fleet_line(f))
        for a in doc["alerts"]:
            print("  ALERT " + a)
        return
    summarize(fs, doc)
    for a in doc["alerts"]:
        print("ALERT: " + a)


def summarize(fs, doc):
    hw = fs["homeworld"]
    p = fs["player"]
    print(f"Empire {p['name']} (id {p['id']})  homeworld {hw['location']['q']},{hw['location']['r']}")
    if fs.get("world"):
        print(world_line(fs["world"]))
    st = hw["storage"]
    stock = p["resources"]
    caps = "  ".join(f"{k} {stock[k]:.0f}/{st['cap'][k]:.0f}" + (" FULL" if k in st["capped"] else "")
                     for k in RES)
    per_tick = [hw["production"].get(k, 0) for k in RES]
    print(f"Stockpile: {caps}   production per tick: " + "/".join(f"{x:.4g}" for x in per_tick)
          + "  (per hour: " + "/".join(f"{x * 3600:.4g}" for x in per_tick) + ")")
    eta = [f"{k} full in {age(v)}" for k, v in st["full_in_s"].items() if v]
    print(f"Raid-protected stock: {res(st['protected'])}" + (f"   ({', '.join(eta)})" if eta else ""))
    print("Buildings: " + ", ".join(f"{b['building_type']} {b['level']}" for b in hw["buildings"]))
    print("Research:  " + (", ".join(f"{r['tech']} {r['level']}" for r in hw["research"]) or "none"))
    labels = {"build": "Build queue", "research": "Research queue", "ship": "Shipyard queue"}
    for n, q in doc["queues"].items():
        head = f"{labels[n]} ({len(q['running'])}/{q['slots']} slots, {q['depth']} items max, {q['room']} free): "
        run = "; ".join(f"{r['item']}" + (f" -> L{r['to']}" if "to" in r else f" {r['done']}/{r['of']}") + f" done in {age(r['eta_s'])}" for r in q["running"])
        print(head + (run or ("none running" if q["waiting"] else "idle")))
        for w in q["waiting"]:
            what = f"{w['count']}x " if "count" in w else ""
            to = f" -> L{w['to']}" if "to" in w else ""
            need = ""
            if w.get("short"):
                need = " NEEDS " + ", ".join(f"{v} {k}" for k, v in w["short"].items())
                need += f" (about {age(w['covered_in_s'])} of income)" if "covered_in_s" in w else " (no income of it yet)"
            if not need and q["open"] == 0 and len(q["running"]) >= q["slots"]:
                need = " (starts when a slot frees up; paid then)"
            why = f" [waits on {str(w['waiting_on']).lower()}" + (f", starts in ~{age(w['start_in_s'])}]" if "start_in_s" in w else "]") \
                if w.get("waiting_on") else ""
            print(f"    waiting [{w['index']}]: {what}{w['item']}{to} ({age(w['ticks'])}{' each' if what else ''}){need}{why}")
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
            describe(sec, loc, sectors, fs["tick"], indent="  ", power=f.get("power"))


GROWTH = {"MetalMine": 1.5, "CrystalMine": 1.6, "DeuteriumSynthesizer": 1.5, "Shipyard": 1.8, "ResearchLab": 1.8,
          "FuelDepot": 1.7, "SensorArray": 1.7, "DefenseGrid": 1.8, "StorageVault": 1.7, "Fabricator": 1.9,
          "ModularFabrication": 2.0}
RESEARCH_GROWTH = 1.6
MINES = {"MetalMine": ("metal", 10.0), "CrystalMine": ("crystal", 7.0), "DeuteriumSynthesizer": ("deuterium", 5.0)}
EFFECT = {"Shipyard": "ships build 10% faster per level; unlocks ships and defences", "ResearchLab": "research 10% faster per level",
          "Fabricator": "buildings build 15% faster per level", "StorageVault": "stockpile caps x1.5 per level, raid-protected share +8%",
          "FuelDepot": "more fuel per fleet", "SensorArray": "wider live sensor range", "DefenseGrid": "stronger home defence"}


def weighted(cost):
    return sum(WEIGHT[k] * cost.get(k, 0) for k in RES)


def future_steps(label, level, max_level, nxt, section, count=3):
    """The next steps of a leveled upgrade: the real one, then the same curve extrapolated (marked inexact)."""
    if not nxt:
        return []
    steps = [{"level": nxt["level"], "cost": nxt["cost"], "ticks": nxt["ticks"], "exact": True}]
    g = GROWTH.get(label, RESEARCH_GROWTH if section == "research" else 1.6)
    while len(steps) < count and steps[-1]["level"] < (max_level or 0):
        p = steps[-1]
        cost = {k: round(p["cost"].get(k, 0) * g) for k in RES}
        was = p["cost"].get("metal", 0) + p["cost"].get("crystal", 0)
        ticks = max(1, round(p["ticks"] * (cost["metal"] + cost["crystal"]) / was)) if was else p["ticks"]
        steps.append({"level": p["level"] + 1, "cost": cost, "ticks": ticks, "exact": False})
    return steps


def mine_gain(label, level, steps, fs):
    """Per-step production gain (units per tick) of a mine, from its current output."""
    res_name, per_hour = MINES[label]
    pace = (fs.get("world") or {}).get("pace") or 1
    def p(l):
        return per_hour / 3600 * pace * l * 1.1 ** l
    now = fs["homeworld"]["production"].get(res_name, 0)
    gains, lv, cur = [], level, now
    for s in steps:
        base = p(lv)
        nxt = cur * p(lv + 1) / base if base > 0 and cur > 0 else p(lv + 1)
        gains.append((res_name, nxt - cur))
        cur, lv = nxt, lv + 1
    return gains


def unlock_text(rows, label, level, cap=5):
    """Entries whose requirement on LABEL is met by reaching LEVEL: opened outright, or still short of other things."""
    full, part = [], []
    for section, other, _l, _m, _s, requires in rows:
        for r in requires:
            if norm(r["name"]) == norm(label) and r["need"] == level and not r["met"]:
                (part if sum(1 for q in requires if not q["met"]) > 1 else full).append(other)
    text = ", ".join(full[:cap]) + (f" +{len(full) - cap}" if len(full) > cap else "")
    if part:
        text += (", " if text else "") + "toward " + ", ".join(part[:3]) + (f" +{len(part) - 3}" if len(part) > 3 else "")
    return [text] if text else []


def costs_lines(fs, what=None, depth=3):
    cat_rows = catalog_rows(fs)
    stock = fs["player"]["resources"]
    lines = []

    def afford(cost):
        return "" if all(stock[k] >= cost.get(k, 0) for k in stock) else "  cannot pay yet"

    def need(requires):
        unmet = [f"{r['name']} {r['need']} (have {r['have']})" for r in requires if not r["met"]]
        return f"  LOCKED: needs {', '.join(unmet)}" if unmet else ""

    for title in ("buildings", "research", "ships", "defences"):
        if what and what != title:
            continue
        lines.append(f"{title}:")
        for section, label, lvl, mx, nxt, requires in cat_rows:
            if section != title:
                continue
            if title in ("ships", "defences"):
                tag = "per ship" if title == "ships" else f"{lvl} standing"
                lines.append(f"  {label:<22} {tag:<12} costs {res(nxt['cost'])} in {age(nxt['ticks'])}{afford(nxt['cost'])}{need(requires)}")
                continue
            if nxt is None:
                lines.append(f"  {label:<22} L{lvl}/{mx}  maxed")
                continue
            steps = future_steps(label, lvl, mx, nxt, title, depth)
            gains = mine_gain(label, lvl, steps, fs) if label in MINES else None
            lines.append(f"  {label:<22} L{lvl}/{mx}" + (f"   {EFFECT[label]}" if label in EFFECT else ""))
            for i, s in enumerate(steps):
                unl = unlock_text(cat_rows, label, s["level"])
                bits = [f"    -> L{s['level']}{'' if s['exact'] else '~'} costs {res(s['cost'])} in {age(s['ticks'])}"]
                if gains:
                    rname, g = gains[i]
                    pay = weighted(s["cost"]) / (WEIGHT[rname] * g) if g > 0 else None
                    bits.append(f"+{g:.3g} {rname}/s" + (f", payback {age(pay)}" if pay else ""))
                if unl:
                    bits.append("unlocks " + ", ".join(unl))
                if i == 0:
                    bits.append((afford(s["cost"]) + need(requires)).strip())
                lines.append("  ".join(b for b in bits if b))
    if not what:
        lines.append("(-> L~ lines past the first are extrapolated along the cost curve; payback = cost / extra output, "
                     "metal 1: crystal 1.5: deut 2; mines gain +10%/level compounding on level x output)")
    return lines


def cmd_costs(name, what=None):
    fs, _ = view(name)
    if not fs:
        return no_state(name)
    header(name, fs["tick"])
    for line in costs_lines(fs, what):
        print(line)


def cmd_score(name, limit=10):
    start = log_size(name)
    send(name, {"action": "leaderboard", "limit": int(limit)})
    board = None
    deadline = time.time() + 3
    while time.time() < deadline and not board:
        time.sleep(0.1)
        lines, _ = log_lines(name, start)
        board = next((m for m in lines if m.get("type") == "leaderboard"), None)
    header(name, last_tick(name))
    if not board:
        print("No answer from the server yet. Try again shortly.")
        return
    print("rank  name                  score    core  combat  explore   (one table for humans and AI agents)")
    for e in board["entries"] + ([board["you"]] if board.get("you") else []):
        tag = "AI " if e["agent"] else "   "
        print(f"{e['rank']:>4}  {e['name']:<20} {tag}{e['score']:>7.1f} {e['core']:>7.1f} {e['combat']:>7.1f} {e['explore']:>8.1f}")
    print(board.get("rule") or SCORE_RULE)


def cmd_sector(name, q, r):
    fs, _ = view(name)
    if not fs:
        return no_state(name)
    header(name, fs["tick"])
    sectors = {(s["location"]["q"], s["location"]["r"]): s for s in fs.get("known_sectors") or []}
    sec = sectors.get((int(q), int(r)))
    if not sec:
        print(f"sector {q},{r} was never seen by you (scan or visit it)")
        return
    describe(sec, (int(q), int(r)), sectors, fs["tick"], power=best_power(fs))


def hex_dist(a, b):
    dq, dr = a[0] - b[0], a[1] - b[1]
    return (abs(dq) + abs(dr) + abs(dq + dr)) // 2


def cmd_map(name, radius=4):
    fs, _ = view(name)
    if not fs:
        return no_state(name)
    header(name, fs["tick"])
    sectors = {(s["location"]["q"], s["location"]["r"]): s for s in fs.get("known_sectors") or []}
    centers = [(f["location"]["q"], f["location"]["r"]) for f in fs.get("fleets") or []] or [
        (fs["homeworld"]["location"]["q"], fs["homeworld"]["location"]["r"])]
    shown = sorted(k for k in sectors if min(hex_dist(k, c) for c in centers) <= int(radius))
    now = fs["tick"]
    power = best_power(fs)
    live = sum(1 for s in sectors.values() if s.get("live"))
    print(f"{len(shown)} known sectors within {radius} of your fleets "
          f"(of {len(sectors)} ever seen: {live} live, {len(sectors) - live} remembered). "
          "LIVE = current; STALE Nm = as last seen N ago. "
          f"T = threat rating 1-9; odds use your strongest fleet (power {power:.0f}).")
    for k in shown:
        s = sectors[k]
        r = s.get("resources") or {}
        bits = [freshness(s, now), s["terrain"], f"m={r.get('metal')} c={r.get('crystal')} d={r.get('deuterium')}"]
        t = s.get("threat")
        if t:
            ratio = power / max(t["est_power"], 1e-6)
            bits.append(f"T{t['rating']} {ratio:.1f}x {verdict(ratio)}")
        if s.get("hostiles"): bits.append(f"HOSTILES x{len(s['hostiles'])}")
        if s.get("player_fleets"): bits.append("players: " + ",".join(p["owner_name"] for p in s["player_fleets"]))
        if s.get("salvage"): bits.append("salvage")
        if s.get("site"): bits.append(f"derelict t{s['site']['tier']} {s['site']['risk']}")
        print(f"  {k[0]},{k[1]}: " + "  ".join(bits))


def collect(name, start, want, timeout):
    """Read the log from START until WANT(msgs) is true or the timeout passes."""
    deadline = time.time() + timeout
    while True:
        lines, end = log_lines(name, start)
        if want(lines) or time.time() >= deadline:
            return lines, end
        time.sleep(0.05)


def cmd_preview(name, fleet_id, *coords):
    """What jumping a fleet to adjacent sectors costs and meets; no coordinates previews every exit."""
    fs, _ = view(name)
    fid = int(fleet_id)
    if len(coords) % 2:
        print("preview FLEET [Q R ...]")
        return
    targets = [(int(coords[i]), int(coords[i + 1])) for i in range(0, len(coords), 2)]
    if not targets:
        f = next((f for f in (fs or {}).get("fleets") or [] if f["id"] == fid), None)
        if not f:
            header(name)
            print(f"you have no fleet {fid}")
            return
        loc = (f["location"]["q"], f["location"]["r"])
        sec = next((s for s in fs["known_sectors"] if (s["location"]["q"], s["location"]["r"]) == loc), None)
        targets = [(c["q"], c["r"]) for c in (sec or {}).get("connections") or []]
        if not targets:
            header(name)
            print("no known exits from that fleet's sector")
            return
    start = log_size(name)
    for i, (q, r) in enumerate(targets):
        if i:
            time.sleep(0.2)
        send(name, {"action": "preview_move", "fleet_id": fid, "target": {"q": q, "r": r}})
    lines, _ = collect(name, start, lambda ls: sum(1 for m in ls if m.get("type") in ("preview_move", "error")) >= len(targets), ACK_CAP + 1.5)
    header(name, last_tick(name))
    shown = [m for m in lines if m.get("type") in ("preview_move", "error")]
    if not shown:
        print("no answer; try again")
    for m in shown:
        print(("refused: " + f"{m.get('code')} {m.get('message')}") if m["type"] == "error" else preview_line(m))


def parse_commands(args):
    """JSON commands from CLI arguments: objects, arrays, or newline-separated objects."""
    def load(text):
        text = text.strip()
        try:
            v = json.loads(text)
            return v if isinstance(v, list) else [v]
        except json.JSONDecodeError:
            return [json.loads(line) for line in text.splitlines() if line.strip()]
    out = []
    try:
        for a in args:
            out += load(a)
    except json.JSONDecodeError:
        try:
            out = load(" ".join(args))
        except json.JSONDecodeError as e:
            return None, f"not valid JSON: {e}"
    bad = [c for c in out if not isinstance(c, dict)]
    return (None, f"each command must be a JSON object, got {bad[0]!r}") if bad else (out, None)


def cmd_label(c):
    head = c.get("action") or c.get("type") or "?"
    rest = [str(v) if not isinstance(v, (dict, list)) else json.dumps(v, separators=(",", ":")) for k, v in c.items() if k not in ("action", "type")]
    return head + (" " + " ".join(rest) if rest else "")


ERR_HINTS = {"NoConnection": ("move", "preview_move"), "ResourceNotPresent": ("harvest",),
             "CargoFull": ("collect_salvage", "harvest")}


QUEUE_WORDS = {"building queue": ("build",), "research queue": ("research",),
               "shipyard queue": ("build_ship", "build_defence"), "ship queue": ("build_ship", "build_defence")}


def mentions(c, msg):
    """True when the error text names something in the command: its fleet or one of its names."""
    fid = c.get("fleet_id")
    if fid is not None and re.search(rf"\bfleet {fid}\b", msg):
        return True
    flat = re.sub(r"[\s_]", "", msg).lower()   # the server writes "Corvette Tech" for CorvetteTech
    return any(re.sub(r"[\s_]", "", v).lower() in flat for k, v in c.items() if isinstance(v, str) and k not in ("action", "type"))


def match_errors(cmds, errors):
    """Attribute server errors (which carry no command id) to commands, in order.

    A command takes at most one error. An error goes to the first free command whose
    fleet id or building, tech, ship or defence name its text mentions; else to the
    first free command its code fits; else to the first free one. Returns
    (per-command error or None, errors that fit nothing).
    """
    got, left = [None] * len(cmds), []
    for e in errors:
        msg = e.get("message", "")
        free = [i for i in range(len(cmds)) if got[i] is None]
        pick = next((i for i in free if mentions(cmds[i], msg)), None)
        if pick is None:
            hint = ERR_HINTS.get(e.get("code"), ())
            for words, acts in QUEUE_WORDS.items():
                if words in msg.lower():
                    hint = acts
            fits = [i for i in free if cmds[i].get("action") in hint]
            pick = (fits[-1] if e.get("code") == "QueueFull" else fits[0]) if fits else None
        if pick is None and free and len(errors) >= len(free):
            pick = free[0]
        if pick is None:
            left.append(e)
        else:
            got[pick] = e
    return got, left


def flat(text):
    return re.sub(r"[\s_]", "", str(text)).lower()


def queue_outcome(cmd, queue_events, used):
    """The server's own Queue event for an order: the authoritative 'started' or 'waiting'."""
    if cmd.get("action") in ("cancel_queued", "cancel_build"):
        for i, q in enumerate(queue_events):
            if i not in used and str(q.get("action", "")).lower() == "cancelled":
                used.add(i)
                back = {k: v for k, v in (q.get("refunded") or {}).items() if v}
                return f"cancelled {q['item']}" + (f", refunded {res(back)}" if back else ", nothing was paid yet")
        return None
    name = cmd.get("building_type") or cmd.get("tech") or cmd.get("ship_class") or cmd.get("kind")
    if not name:
        return None
    for i, q in enumerate(queue_events):
        if i in used or not flat(q.get("item", "")).startswith(flat(name)):
            continue
        used.add(i)
        act = str(q.get("action", "")).lower()
        if act == "started":
            paid = {k: v for k, v in (q.get("paid") or {}).items() if v}
            return f"{q['item']} started" + (f", paid {res(paid)}" if paid else "")
        if act == "waiting":
            eta = f", starts in ~{age(q['start_in'])}" if q.get("start_in") is not None else ""
            return f"{q['item']} queued, waits on {str(q.get('waiting_on', '?')).lower()}{eta}"
        return f"{q['item']} {act}"
    return None


def outcome(cmd, qs):
    """What became of a queue order, read from the queues after the server's next tick."""
    a = cmd.get("action")
    key = {"build": ("build", cmd.get("building_type")), "research": ("research", cmd.get("tech")),
           "build_ship": ("ship", cmd.get("ship_class")), "build_defence": ("ship", cmd.get("kind"))}.get(a)
    if not key or not qs:
        return None
    q = qs[key[0]]
    for r in q["running"]:
        if r["item"] == key[1]:
            return f"started, done in {age(r['eta_s'])}"
    for w in reversed(q["waiting"]):
        if w["item"] == key[1]:
            if w.get("short"):
                return f"queued [{w['index']}] waiting for " + ", ".join(f"{v} {k}" for k, v in w["short"].items())
            return f"queued [{w['index']}]"
    return "accepted (already finished or not visible yet)"


def run_commands(name, cmds, gap=SEND_GAP, fast=False):
    """Send commands in order and report each. Returns after the server's next tick.

    Commands are spaced by GAP seconds. The server handles them at its next tick, so errors
    and state changes show up together there; with fast=True nothing waits for it.
    """
    start = log_size(name)
    base = start
    for i, c in enumerate(cmds):
        if i:
            time.sleep(gap)
        base = log_size(name)
        send(name, c)
    if fast:
        time.sleep(0.15)
    else:
        deadline = time.time() + ACK_CAP
        while time.time() < deadline:
            time.sleep(0.04)
            after, _ = log_lines(name, base)
            if any(m.get("type") == "tick_update" for m in after):
                time.sleep(0.05)
                break
    lines, _ = log_lines(name, start)
    errors = [m for m in lines if m.get("type") in ("error", "client_error")]
    fs, _b = view(name)
    qs = queues_of(fs) if fs and not fast else None
    return lines, errors, qs


def cmd_do(name, args):
    fast = "--fast" in args
    gap = SEND_GAP
    rest = []
    it = iter(a for a in args if a != "--fast")
    for a in it:
        if a == "--gap":
            gap = float(next(it, SEND_GAP))
        else:
            rest.append(a)
    cmds, err = parse_commands(rest)
    if err or not cmds:
        header(name)
        print(err or "no command given")
        return
    unseen, _ = log_lines(name, cursor(name))
    caught_up = not collapse_events(unseen, errors=False)
    lines, errors, qs = run_commands(name, cmds, gap, fast)
    fs, _ = view(name)
    header(name, fs["tick"] if fs else None)
    client_errs = [e for e in errors if e["type"] == "client_error"]
    server_errs = [e for e in errors if e["type"] == "error"]
    per, left = match_errors(cmds, server_errs)
    qev = [e for m in lines if m.get("type") == "tick_update" for e in (m.get("events") or []) if e.get("kind") == "Queue"]
    used = set()
    n = len(cmds)
    for i, c in enumerate(cmds):
        tag = f"[{i + 1}/{n}] " if n > 1 else ""
        if per[i] is not None:
            print(f"{tag}ERR {cmd_label(c)}: {per[i].get('code')} {per[i].get('message')}")
        elif fast:
            print(f"{tag}sent {cmd_label(c)}")
        else:
            o = queue_outcome(c, qev, used) or outcome(c, qs)
            print(f"{tag}ok {cmd_label(c)}" + (f" -> {o}" if o else ""))
    for e in client_errs + left:
        print("  rejected: " + " ".join(x for x in (e.get("code"), e.get("message")) if x))
    for m in lines:
        if m.get("type") in ("preview_move", "leaderboard"):
            print("  " + (preview_line(m) if m["type"] == "preview_move" else leaderboard_line(m)))
    ev = collapse_events([m for m in lines if m.get("type") == "tick_update"], errors=False)
    if ev:
        print_events(ev, 8)
    if caught_up:
        cursor(name, log_size(name))
    if qs:
        idle = [k for k, q in qs.items() if q["open"] > 0]
        print("queues: " + " | ".join(f"{k} {len(q['running'])}/{q['slots']}+{len(q['waiting'])}w" for k, q in qs.items())
              + (f" | IDLE: {', '.join(idle)}" if idle else ""))


def leaderboard_line(m):
    return "leaderboard: " + "; ".join(f"{e['rank']}. {e['name']} {e['score']:.1f}" for e in m["entries"][:10])


def cmd_events(name, mine_only=False, advance=True):
    start = cursor(name)
    lines, end = log_lines(name, start)
    header(name, last_tick(name))
    print_events(collapse_events(lines, mine_only, errors=False))
    if advance:
        cursor(name, end)


def cmd_wait(name, args):
    mine = "--mine" in args
    until = "any"
    secs = WAIT_DEFAULT
    it = iter(args)
    for a in it:
        if a == "--until":
            until = next(it, "any")
        elif a.startswith("--until="):
            until = a.split("=", 1)[1]
        elif a != "--mine":
            try:
                secs = int(float(a))
            except ValueError:
                print(f"wait [seconds] [--until idle|event|done] [--mine]; got {a!r}")
                return
    if until not in ("any", "idle", "event", "done"):
        print("--until must be idle, event or done")
        return
    note = ""
    if secs > WAIT_MAX:
        note = f" (asked {secs}s; waits are capped at {WAIT_MAX}s so your tool call does not time out; call wait again)"
    secs = max(2, min(secs, WAIT_MAX))
    fs, _ = view(name)
    if not fs:
        return no_state(name)
    base = snapshot(fs)
    start = log_size(name)
    t0 = time.time()
    why = wait_reasons(base, base, [], until) if until == "done" else []
    events = []
    while not why and time.time() - t0 < secs:
        time.sleep(0.3)
        lines, _ = log_lines(name, start)
        events = raw_events(lines)
        cur_fs, _b = view(name)
        if cur_fs:
            why = wait_reasons(base, snapshot(cur_fs), events, until)
    waited = time.time() - t0
    fs, board = view(name)
    header(name, fs["tick"] if fs else None)
    if why:
        print(f"(waited {waited:.0f}s of max {secs}s) RETURNED EARLY:")
        for w in dict.fromkeys(why):
            print("  * " + w)
    else:
        print(f"(waited the full {secs}s; one wait call is capped at {WAIT_MAX}s. Nothing actionable happened)")
    if note:
        print("note:" + note.strip(" ()"))
    start_c = cursor(name)
    lines, end = log_lines(name, start_c)
    print_events(collapse_events(lines, mine, errors=False))
    cursor(name, end)
    if fs:
        qs = queues_of(fs)
        idle = [k for k, q in qs.items() if q["open"] > 0]
        print("queues: " + " | ".join(f"{k} {len(q['running'])}/{q['slots']}+{len(q['waiting'])}w" for k, q in qs.items())
              + (f" | IDLE: {', '.join(idle)}" if idle else "") + (f" | score {score_of(fs, board)['score']}" if score_of(fs, board) else ""))


def daemon(name):
    """Run a headless client for NAME with a FIFO for stdin; restarts on exit."""
    for d in (LOGS, FIFOS, STATE):
        d.mkdir(parents=True, exist_ok=True)
    fifo = FIFOS / f"{name}.in"
    if not fifo.exists():
        os.mkfifo(fifo)
    env = dict(os.environ, XDG_CONFIG_HOME=str(LIVE / "cfg"))
    keeper = os.open(fifo, os.O_RDWR)  # read-write: writers never block and EOF never hits

    def poll_board():
        while True:
            time.sleep(8)
            try:
                os.write(keeper, (json.dumps({"action": "leaderboard", "limit": 50}) + "\n").encode())
            except OSError:
                pass
    threading.Thread(target=poll_board, daemon=True).start()
    threading.Thread(target=watch_watchdog, daemon=True).start()
    pidfile = STATE / f"{name}.client.pid"
    if proc_alive(pidfile, "iac-client"):
        try:
            os.kill(int(pidfile.read_text()), 15)
            hlog(f"killed stale client of {name} left by an earlier daemon")
        except (OSError, ValueError):
            pass
    while True:
        with open(LOGS / f"{name}.ndjson", "ab") as out, open(LOGS / f"{name}.stderr", "ab") as err:
            p = subprocess.Popen([str(CLIENT), "--headless", "--port", str(PORT), "--name", name],
                                 stdin=keeper, stdout=out, stderr=err, env=env)
            pidfile.write_text(str(p.pid))
            code = p.wait()
        hlog(f"client {name} exited code {code}; restarting")
        time.sleep(2)


def proc_alive(pidfile, marker):
    try:
        pid = int(pathlib.Path(pidfile).read_text().strip())
        os.kill(pid, 0)
        return marker.encode() in pathlib.Path(f"/proc/{pid}/cmdline").read_bytes()
    except (OSError, ValueError):
        return False


def watch_watchdog():
    """The clients watch the watchdog; it watches everything else. flock in it prevents duplicates."""
    while True:
        time.sleep(15)
        s = session()
        w = LIVE / "bin" / "w"
        if not s or (STATE / "stopping").exists() or time.time() > s.get("end", 0) + 1800 or not w.exists():
            continue
        if not proc_alive(STATE / "watchdog.pid", "bin/w"):
            hlog("watchdog is not running; starting it")
            subprocess.Popen(["setsid", str(w)], stdout=open(LOGS / "watchdog.log", "ab"), stderr=subprocess.STDOUT,
                             stdin=subprocess.DEVNULL, env=dict(os.environ), close_fds=True)


def hlog(msg):
    try:
        with open(LIVE / "events.log", "a") as f:
            f.write(f"{time.strftime('%Y-%m-%d %H:%M:%S')} daemon {msg}\n")
    except OSError:
        pass


USAGE = """usage: play <command>
  state [--pretty]       ONE compact JSON document, returns at once: stock, caps, income, levels, queues (running,
                         waiting, shortfalls), idle queues, next costs, fleets, score and rank, minutes left, alerts
  status [--brief]       readable view: headline (score, rank, time, idle queues, scoring rule), stock, queues with
                         their waiting items and shortfalls, fleets and their sector; --brief is the short form
  costs [buildings|research|ships|defences]  next 3 levels with cost, time, what each unlocks, mine payback
  score [rows]           the leaderboard (default 10 rows); your own row is added if you are below it
  map [radius]           known sectors near your fleets (default radius 4)
  sector Q R             details of one known sector
  preview FLEET [Q R..]  jump cost and threat for adjacent sectors; with no Q R, every exit of the fleet's sector
                         (SAFE >= 2.5x, FAVOURABLE >= 1.5x, EVEN >= 1.1x, RISKY >= 0.9x, else DEADLY)
  do '<json>' ['<json>' ...]   send commands in order (or one JSON array: do '[{...},{...}]'); one result line each,
                         returns at the server's next tick (about 0.5 s) or at once on an error; --fast skips that wait
                         e.g. play do '{"action":"build","building_type":"MetalMine"}' '{"action":"research","tech":"Navigation"}'
                         standing orders: play do '{"type":"policy_update","fleet_id":2,"preset":"prospect"}'
  events [--mine]        events since you last looked, collapsed (scans, fights); --mine drops fights you only witnessed
  wait [secs] [--until idle|event|done] [--mine]
                         sleeps up to secs (default 45, MAX 60) and returns early when a queue goes idle or becomes
                         payable, a manual fleet arrives or finishes, or an alert fires; --until idle: queues only;
                         event: any notable event; done: everything finished
  rules                  print RULES.md"""


def main():
    a = sys.argv[1:]
    if a[:1] == ["daemon"]:
        if a[1:2] == ["--slot"]:
            a = ["daemon", (STATE / f"slot{a[2]}.conf").read_text().split()[0]]
        return daemon(a[1])
    name = os.environ.get("IAC_PLAYER")
    if not name or not a:
        print(USAGE)
        return
    cmd, rest = a[0], a[1:]
    flags = [x for x in rest if x.startswith("--")]
    pos = [x for x in rest if not x.startswith("--")]
    if cmd == "status": cmd_status(name, "--brief" in flags)
    elif cmd == "state": cmd_state(name, "--pretty" in flags)
    elif cmd == "costs": cmd_costs(name, *pos[:1])
    elif cmd == "score" or cmd == "leaderboard": cmd_score(name, *pos[:1])
    elif cmd == "map": cmd_map(name, *pos[:1])
    elif cmd == "sector" and len(pos) == 2: cmd_sector(name, *pos)
    elif cmd == "preview" and len(pos) >= 1: cmd_preview(name, *pos)
    elif cmd == "do" and rest: cmd_do(name, rest)
    elif cmd == "events": cmd_events(name, "--mine" in rest)
    elif cmd == "wait": cmd_wait(name, rest)
    elif cmd == "rules": print((HERE / "RULES.md").read_text())
    else: print(USAGE)


if __name__ == "__main__":
    main()
