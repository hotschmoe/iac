#!/usr/bin/env python3
"""Agent-facing CLI for a live In Amber Clad session.

Each player has a long-lived headless client (started with `play.py daemon NAME`)
whose stdin is a FIFO and whose stdout is appended to logs/NAME.ndjson.
Agents call `play <subcommand>` from their player directory.

The server answers commands at its next tick (about once a second): errors and
state changes arrive together in that tick. Read-only commands (state, status,
costs, map, sector) never wait: they read a view kept incrementally from the log.
"""
import json, math, os, re, sys, time, subprocess, pathlib, threading

HERE = pathlib.Path(__file__).resolve().parent
LIVE = pathlib.Path(os.environ.get("IAC_RUN") or HERE / "run").resolve()
LOGS, FIFOS, STATE = LIVE / "logs", LIVE / "fifo", LIVE / "state"
CLIENT = HERE.parent.parent / "target/release/iac-headless"
PORT = int(os.environ.get("IAC_PORT", "7777"))
DIRS = {(1, 0): "E", (1, -1): "NE", (0, -1): "NW", (-1, 0): "W", (-1, 1): "SW", (0, 1): "SE"}
RES = ("metal", "crystal", "deuterium")
WEIGHT = {"metal": 1.0, "crystal": 1.5, "deuterium": 2.0}
WAIT_DEFAULT, WAIT_MAX = 45, 60
SEND_GAP = 0.25
ACK_CAP = 1.6
SCORE_RULE = ("score = what your empire has built and keeps: buildings + research + the ships you have now "
              "(Solar Satellites included) + outpost levels, each at cost (metal 1: crystal 1.5: deut 2). Defences "
              "score 0. A fleet destroyed leaves the score. Unspent stock scores 0: spend it. Kills, charts and "
              "derelicts score nothing themselves: they pay in resources (a kill's wreckage, worth scooping and "
              "bringing home; a repelled raid's wreckage at home; charts paid into the stockpile when a fleet docks "
              "with them; a derelict's loot), which score once you build with them.")
QUEUE_NAMES = {"build": "Building", "research": "Research", "ship": "Ship"}
SCORE_PARTS = ("buildings", "research", "fleet", "outpost", "defence")
ALERT_LEVELS = ("Warning", "Critical")
CHART_MARKS = {"aboard": "aboard one of your fleets; it pays when that fleet docks at home or your outpost alive",
               "delivered": "delivered (already paid; no need to visit again)"}
NOTABLE = {"BuildingCompleted", "ResearchCompleted", "ShipBuilt", "DefenceBuilt", "FleetArrived", "CombatStarted",
           "CombatEnded", "ShipsLost", "FleetDestroyed", "RaidIncoming", "RaidResolved", "SiteExplored", "SiteAmbush",
           "SalvageCollected", "ChartDelivered", "ScanCompleted", "Exchanged", "Stockpile"}


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
        fs["outpost"] = m.get("outpost")
        fs["relays"] = m.get("relays")
        fs["hubs"] = m.get("hubs") or []
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


def login_refusal(name):
    """Why the server last refused this player's login ("WorldFull this world is full: ..."), or None."""
    auth = [m for m in log_lines(name)[0] if m.get("type") == "auth_result"]
    if auth and not auth[-1].get("success"):
        return " ".join(x for x in (auth[-1].get("code"), auth[-1].get("message")) if x)
    return None


def no_state(name):
    header(name)
    refusal = login_refusal(name)
    if refusal:
        print(f"LOGIN REFUSED: {refusal}")
    else:
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
        if t == "exchange_quote":
            if not errors:
                continue
            slot(("x", id(m)), "  " + quote_line(m))
            continue
        if t == "outpost_survey":
            if not errors:
                continue
            slot(("x", id(m)), "  " + survey_line(m))
            continue
        if t == "relay_survey":
            if not errors:
                continue
            slot(("x", id(m)), "  " + relay_survey_line(m))
            continue
        if t in ("leaderboard", "odds"):
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
            elif k == "BattleRound":
                key = (e["sector"]["q"], e["sector"]["r"])
                g = fights.setdefault(key, {"t0": tick, "t1": tick, "rounds": 0, "shots": 0, "mine": False, "took": 0, "dealt": 0,
                                            "lost": 0, "killed": 0, "silent": 0, "raid": False})
                ours, theirs = e["fleets"], e["hostiles"]
                g["t1"] = tick; g["rounds"] += 1; g["mine"] = g["mine"] or bool(e.get("mine"))
                g["raid"] = g["raid"] or bool(e.get("raid"))
                g["silent"] += any(group.get("fleeing") and group.get("mine") for group in ours["groups"])
                g["shots"] += ours["shots"] + theirs["shots"]
                g["took"] += theirs["hull_damage"]; g["dealt"] += ours["hull_damage"]
                g["lost"] += theirs["destroyed"]; g["killed"] += ours["destroyed"]
                slot(("fight", key))
            elif k == "PolicyAction":
                cause = re.sub(r"\([^()]*\)", "", e.get("reason") or "")
                key = (e["fleet_id"], e.get("preset"), e.get("action"), cause)
                g = policy.setdefault(key, {"n": 0, "t0": tick, "t1": tick})
                g["n"] += 1; g["t1"] = tick; g["reason"] = e.get("reason")
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
            what = "unit(s), ships and defences" if g["raid"] else "ship(s)"
            sides = (f"took {g['took']} hull and lost {g['lost']} {what}, dealt {g['dealt']} and destroyed {g['killed']}")
            who = f"your side {sides}" if g["mine"] else f"witnessed: the fleets {sides}"
            left = f"; in {g['silent']} of them a fleet of yours was leaving and did not fire" if g["silent"] else ""
            kind = "RAID fought" if g["raid"] else "combat"
            items[i] = f"  {kind} @{key[1][0]},{key[1][1]} {span}: {g['rounds']} round(s), {g['shots']} shots, {who}{left}"
        elif key[0] == "policy":
            g, f = policy[key[1]], key[1]
            span = f"x{g['n']}, ticks {g['t0']}-{g['t1']}" if g["n"] > 1 else f"tick {g['t0']}"
            items[i] = f"  fleet {f[0]} {f[1]} {f[2]}: {g['reason']} ({span})"
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
    if k == "Exchanged":
        return f"fleet {e['fleet_id']} traded at the exchange: {trade_text(e)}; {meter_text(e['meter_after'])}"
    if k == "HubArrived":
        return f"HUB {e['id']} seen at {hexs(e['sector'])} ring {e['ring']}: it leaves at tick {e['departs_at']}"
    if k == "HubDeparting":
        return f"HUB {e['id']} at {hexs(e['sector'])} departs at tick {e['departs_at']}: leave its sector or it is gone"
    if k == "CombatEnded":
        lost = "; ".join(
            f"{l.get('owner') or l.get('label') or 'a group'}" + ("'s defences" if l.get("battery") else "") + " lost "
            + (", ".join(f"{u['count']}x {u['unit']}" for u in l["lost"]) or "nothing")
            + (" and is destroyed" if l["destroyed"] else "")
            for l in e.get("losses") or [])
        what = "raid's fight" if e.get("raid") else "combat"
        return (f"{what} @{hexs(e['sector'])} ended after {e['rounds']} round(s): {e['outcome']}"
                + (f"; {lost}" if lost else "") + wreckage_words(e))
    if k == "FleetDestroyed":
        if e.get("is_npc"):
            return (f"hostile {e.get('label') or e['fleet_id']} destroyed @{hexs(e['sector'])}, "
                    f"its wreckage {res(e.get('salvage'))} added to the pile there")
        whose = "YOUR fleet" if e.get("mine") else f"{e.get('owner')}'s fleet"
        return f"{whose} {e['fleet_id']} destroyed @{hexs(e['sector'])}"
    if k == "SalvageCollected":
        rest = f", {res(e['remaining'])} left on the ground" if e.get("remaining") else ", the pile is gone"
        return f"fleet {e['fleet_id']} scooped {res(e['resources'])} of wreckage @{hexs(e['sector'])} into its hold{rest}"
    if k == "Stockpile":
        return stockpile_words(e)
    if k == "ShipsLost":
        ships = ", ".join(f"{s['count']}x {s['class']}" for s in e["ships"])
        return f"fleet {e['fleet_id']} lost {ships} at {hexs(e['sector'])}"
    if k == "RaidIncoming":
        where = f"your outpost at {hexs(e['outpost'])}" if e.get("outpost") else "your homeworld"
        odds = f"; forecast {odds_words(e['odds'])}" if e.get("odds") else ""
        return (f"RAID INCOMING on {where}: arrives tick {e['arrival_tick']} (in {age(e['arrival_tick'] - e.get('tick', 0))}), "
                f"power {e.get('est_power', 0):.0f}, flying {units_text(e.get('ships'), 'class')}{odds}. "
                f"A fleet that stays is in the fight; one that leaves before it arrives is not")
    if k == "RaidResolved":
        where = f"your outpost at {hexs(e['outpost'])}" if e.get("outpost") else "your homeworld"
        how = {"hostiles_destroyed": "REPELLED, the raiders destroyed", "both_destroyed": "REPELLED, both sides destroyed",
               "outlasted": "REPELLED, outlasted: the raiders left", "fleets_destroyed": "LOST, no defender left",
               "fled": "LOST, the last defender left", "withdrawn": "LOST, the last defender left",
               None: "LOST without a fight, nothing stood in its way"}[e.get("outcome")]
        fought = f" after {e['rounds']} round(s)" if e.get("rounds") else ""
        if e.get("defended"):
            how += ", and you LOST some of what stood there" if e.get("lost") else ", and you lost nothing"
        parts = [f"you lost {units_text(e.get('lost'))}"]
        if e.get("rebuilt"):
            parts.append(f"rebuilt at once {units_text(e['rebuilt'])}")
        parts.append(f"the raiders lost {units_text(e.get('raiders_lost'), 'class')} of {units_text(e.get('raiders'), 'class')}")
        if sum((e.get("resources_lost") or {}).values()) > 0:
            parts.append(f"it took {res(e['resources_lost'])}")
        if e.get("salvage_dropped"):
            parts.append(f"wreckage {res(e['salvage_dropped'])}")
        if e.get("breached"):
            parts.append("the outpost is BREACHED")
        return f"raid on {where} {how}{fought}: " + "; ".join(parts)
    if k == "Pursuit":
        whose = "your fleet" if e.get("mine") else f"{e['owner']}'s fleet"
        jump = f", jumping to {hexs(e['to'])}, there in {e['arrives_in']} tick(s)" if e.get("to") else ""
        if e["phase"] == "abandoned":
            return f"the {e['label']} at {hexs(e['sector'])} gave up {whose} {e['fleet_id']}: {e.get('why')}{jump}"
        verb = "is after" if e["phase"] == "began" else "is still after"
        return (f"PURSUIT: the {e['label']} at {hexs(e['sector'])} {verb} {whose} {e['fleet_id']}{jump}; "
                f"it follows {e['hexes_left']} more hex(es) after that")
    rest = {a: b for a, b in e.items() if a not in ("kind", "tick", "player_id")}
    return k + " " + json.dumps(rest, separators=(",", ":"))[:220]


STOCKPILE_PHRASES = {"unloaded": "unloaded into the stockpile", "loaded": "loaded from the stockpile",
                     "wreckage": "scooped wreckage at home into the stockpile",
                     "charts": "delivered charts, paid into the stockpile"}


def stockpile_words(e):
    """A `Stockpile` event in the server's phrase: what went into the stockpile or came out of it, and why."""
    return f"STOCKPILE: fleet {e['fleet_id']} {STOCKPILE_PHRASES.get(e['cause'], e['cause'])}: {res(e['amount'])}"


def wreckage_words(e):
    """'; wreckage 89/49/24 (on wreckage from before: 279/152/76 lie there now)': what a fight left, from `CombatEnded`."""
    if not e.get("salvage"):
        return ""
    pile = f" (on wreckage from before: {res(e['salvage_pile'])} lie there now)" if e.get("salvage_pile") else ""
    return f"; wreckage {res(e['salvage'])}{pile}"


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
            + f"; T{t['rating']} NPC power {t['est_power']:.0f} ({t['basis']}); " + odds_text(m["fleet_id"], m)
            + (f"\n  this jump is FLIGHT: {flight_words(m['flight'])}" if m.get("flight") else ""))


def flight_words(q):
    """What leaving the fight costs, from `fleets[].flight` or a preview's `flight`."""
    chase = q.get("chase")
    follows = (f"the {chase['label']} would follow {chase['hexes']} hex(es), jumping in {chase['jump_ticks']} tick(s)"
               + (f" and firing on you for {chase['rounds_per_hex']} round(s) at each" if chase["rounds_per_hex"]
                  else ", which never catches a fleet as fast as yours that jumps on at once")
               if chase else "nothing would follow")
    cost = f"{q['fuel']:.0f} fuel, gone in {q['ticks']} tick(s) after {q['rounds']} round(s) under fire without firing back"
    refused = f"  CANNOT LEAVE: {q['refusal']}" if q.get("refusal") else ""
    return f"the fight stands at {q['ratio']:.2f}x {q['label'].upper()}; a move out costs {cost}; {follows}{refused}"


def trade_text(t):
    """'632 metal -> 225 deuterium (par 316, fee 29%)' for the terms of a quote or an Exchanged event."""
    return (f"{t['amount']:.0f} {t['give']} -> {t['receive']:.0f} {t['want']} "
            f"(par {t['par']:.0f}, fee {t['fee'] * 100:.0f}%)")


def meter_text(m):
    """Where the exchange meter stands: the fee on the next unit and when it is empty again."""
    clear = m.get("clear_in")
    falls = "does not fall (no mine output)" if clear is None else "empty" if clear == 0 else f"empty in {age(clear)}"
    return f"meter {m['pressure']:.0f} of {m['span']:.0f}, next unit at {m['fee'] * 100:.0f}% fee, {falls}"


def quote_line(m):
    return (f"quote fleet {m['fleet_id']}: {trade_text(m)}; after it {meter_text(m['meter_after'])}"
            + (f"  CANNOT TRADE NOW: {m['refusal']}" if m.get("refusal") else ""))


def survey_line(m):
    """One line for an outpost_survey reply: what the site makes, what raids it, what founding costs."""
    s, site = m["sector"], m.get("site")
    home = site.get("hops_home") if site else None
    seen = (f"grade {site['grade']:g}: makes {res_rate(site['production'])} an hour at level 1" if site
            else "not on your chart, its ore is not known")
    raid = m.get("raid")
    fight = (f" (a typical one flies {units_text(raid['ships'], 'class')}: {odds_words(raid['odds'])})" if raid else "")
    return (f"survey {s['q']},{s['r']} ring {m['ring']} {seen}; "
            f"raids of {raid_range(m['raid_power'])} against a platform of {units_text(m.get('platform'))}{fight}; "
            f"costs {res(m['cost'])}, built in {age(m['build_ticks'])}"
            + (f"; {home} hops from home" if home is not None else "")
            + (f"  CANNOT FOUND NOW: {m['refusal']}" if m.get("refusal") else "  CAN BE FOUNDED NOW"))


def relay_survey_line(m):
    """One line for a relay_survey reply: what founding costs, what the store holds and why it cannot be founded."""
    s = m["sector"]
    return (f"relay survey {s['q']},{s['r']} ring {m['ring']}: costs {res(m['cost'])}; a store of {m['store_cap']:g} deuterium; "
            f"gone after {age(m['decay_ticks'])} with no fleet of yours there; you keep {m['count']} of {m['limit']} "
            f"(raised by {m['technology']})"
            + (f"  CANNOT FOUND NOW: {m['refusal']}" if m.get("refusal") else "  CAN BE FOUNDED NOW"))


def relays_line(r, now):
    """One line for the player's relays: the limit and what each holds and how long it has."""
    if not r or (not r.get("relays") and not r.get("limit")):
        return None
    stands = "; ".join(f"{x['sector']['q']},{x['sector']['r']} ring {x['ring']} holds {x['store']:.0f} of {x['store_cap']:g} "
                       f"deuterium, gone in {age(x['decay_in'])} unless a fleet of yours stands there"
                       for x in r.get("relays") or [])
    return (f"Relays {len(r.get('relays') or [])}/{r['limit']} ({r['technology']} level {r['level']})"
            + (": " + stands if stands else ""))


def hubs_line(hubs, now):
    """One line for the roaming hubs the player knows: where each stands and when it leaves."""
    if not hubs:
        return None
    parts = []
    for h in hubs:
        left = h["departs_at"] - now
        when = (f"left {age(-left)} ago, where it is now is unknown" if h.get("stale")
                else f"DEPARTING in {age(left)}" if h.get("departing") else f"leaves in {age(left)}")
        parts.append(f"hub {h['id']} at {hexs(h['sector'])} ring {h['ring']} ({'+'.join(h['services'])}; {when})")
    return "Roaming hubs: " + "; ".join(parts)


def refuel_text(q):
    """What a refuel at the hub would buy with each resource in the hold, or why not."""
    if q.get("refusal") and not q.get("offers"):
        return f"CANNOT REFUEL: {q['refusal']}"
    return ("REFUEL (tank lacks {:.0f} fuel): ".format(q["missing"])
            + "; ".join(f"{o['amount']:.0f} {o['give']} buys {o['fuel']:.0f}" + (f" at fee {o['fee'] * 100:.0f}%" if o["fee"] else "")
                        for o in q["offers"]))


def raid_range(r):
    """The weakest and the strongest raid a ring sends."""
    return f"{r['least']:.0f} to {r['most']:.0f}"


def units_text(units, key="unit"):
    """'12x LightFighter, 2x Cruiser' for a list of counts by class or unit; 'nothing' when empty."""
    return ", ".join(f"{u['count']}x {u[key]}" for u in units or []) or "nothing"


def odds_words(o):
    """A raid's odds as the server played them: the label, what it costs the defence, and how the fights it played
    ended. A raid repelled is one that does not breach the dock, not one that costs nothing."""
    lose = round((1 - o["kept"]) * 100)
    cost = "and you lose nothing that stands there" if lose == 0 else f"and you LOSE about {lose}% of what stands there"
    return (f"{o['label'].upper().replace('_', ' ')} {cost} (destroyed {o['destroyed'] * 100:.0f}%, outlasted {o['outlasted'] * 100:.0f}%, "
            f"lost {o['lost'] * 100:.0f}% of the fights played; you keep {o['kept'] * 100:.0f}% of what stands there, "
            f"the raiders {o['raiders_kept'] * 100:.0f}% of theirs)")


def forecast_words(raid, now, possible=True):
    """The raid a `raid` forecast describes: when, what it flies, and how it would end against what stands there now.
    `possible` is `HomeworldState.raids_possible`: while it is false a forecast of typical size is only what a
    raid would be, and none is on the clock."""
    if not possible and raid.get("arrival_tick") is None and raid.get("rounds_left") is None:
        return (f"NO RAID IS ON THE CLOCK: no raid comes while the Shipyard is below the raid threshold and no defence "
                f"stands. This is the raid you would get once one does: power {raid['power']:.0f}, flying "
                f"{units_text(raid['ships'], 'class')}; forecast {odds_words(raid['odds'])}")
    when = (f"RAID BEING FOUGHT, {raid['rounds_left']} round(s) left at the most" if raid.get("rounds_left") is not None
            else f"RAID ARRIVES in {age(raid['arrival_tick'] - now)}" if raid.get("arrival_tick") is not None
            else "next raid, at its typical size")
    return f"{when}: power {raid['power']:.0f}, flying {units_text(raid['ships'], 'class')}; forecast {odds_words(raid['odds'])}"


def res_rate(per_tick):
    return "/".join(f"{(per_tick or {}).get(k, 0) * 3600:.4g}" for k in RES)


def outpost_mark(o):
    """An outpost standing in a sector, as a sector view names it."""
    whose = "YOUR OUTPOST" if o.get("mine") else f"outpost of {o['owner']}"
    return f"{whose} level {o['level']}" + (" BREACHED" if o.get("breached") else "")


def outpost_line(o, now):
    """One line for the player's outpost: level and state, store, yield, its platform and the next raid."""
    work, raid = o.get("work"), o.get("raid")
    state = "BREACHED" if o["breached"] else "in service" if o["in_service"] else "under construction"
    doing = f", {work['kind']} of level {work['level']} done in {age(work['end_tick'] - now)}" if work else ""
    owed = (f", repair waits for {o['repair_waiting_for']['metal']:.0f} more metal (or crystal at 1.5, deuterium at 2 a unit)"
            if o.get("repair_waiting_for") else "")
    lost = f", FULL of {'/'.join(o['yield_lost'])} (that yield is lost)" if o.get("yield_lost") else ""
    return (f"Outpost at {o['location']['q']},{o['location']['r']} level {o['level']}/{o['max_level']} {state}{doing}{owed}: "
            f"store {res(o['store'])} of {res(o['store_cap'])} (raid-protected {res(o['protected'])}){lost}, "
            f"makes {res_rate(o['production'])} an hour, platform {units_text(o.get('platform'))} against raids of "
            f"{raid_range(o['raid_power'])}" + (f"; {forecast_words(raid, now)}" if raid else "")
            + (f"; upgrade refused now: {o['upgrade_refusal']}" if o.get("upgrade_refusal") and o.get("next_level") else ""))


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


def repair_words(repair):
    """What mending a fleet's damage takes at each dock, from `fleets[].repair`."""
    if not repair:
        return ""

    def dock(name, d):
        if d.get("refusal"):
            return f"{name}: {d['refusal']}"
        cost = res(d["payment"]) if d["price"] > 0 else "nothing"
        return f"{name} mends {d['ships']} ship(s) for {cost}"

    docks = [dock("home", repair["home"])] + ([dock("your outpost", repair["outpost"])] if repair.get("outpost") else [])
    return f"; {repair['hull']} hull missing; " + "; ".join(docks)


MAX_ODDS = 256


def ask_odds(name, wanted):
    """The server's odds for fleets in sectors; nothing here works a ratio out. WANTED maps a fleet id to
    the (q, r) sectors to rate. Returns {(fleet id, q, r): {"threat", "ratio", "label"}}; a pair the server
    did not answer for is simply missing."""
    asks = []
    for fid, coords in wanted.items():
        coords = list(dict.fromkeys(coords))
        asks += [(fid, coords[i:i + MAX_ODDS]) for i in range(0, len(coords), MAX_ODDS)]
    if not asks:
        return {}
    start = log_size(name)
    for i, (fid, coords) in enumerate(asks):
        if i:
            time.sleep(0.2)
        send(name, {"action": "odds", "fleet_id": fid, "sectors": [{"q": q, "r": r} for q, r in coords]})
    lines, _ = collect(name, start, lambda ls: sum(1 for m in ls if m.get("type") in ("odds", "error")) >= len(asks), ACK_CAP + 1.5)
    out = {}
    for m in lines:
        if m.get("type") == "odds":
            for s in m["sectors"]:
                out[(m["fleet_id"], s["sector"]["q"], s["sector"]["r"])] = s
    return out


def odds_text(fleet_id, o):
    """'fleet 7 rates 1.2x: EVEN', the server's verdict as it sent it."""
    return f"fleet {fleet_id} rates {o['ratio']:.1f}x: {str(o['label']).upper()}"


def odds_line(m):
    return f"odds fleet {m['fleet_id']}: " + "; ".join(
        f"{s['sector']['q']},{s['sector']['r']} T{s['threat']['rating']} ({s['threat']['basis']}) {s['ratio']:.1f}x {str(s['label']).upper()}"
        for s in m["sectors"])


def threat_text(t, odds=None):
    """'T4 (power 46, observed)', plus the server's odds as (fleet id, odds) when it gave them."""
    if not t:
        return "T? (no rating)"
    text = f"T{t['rating']} (power {t['est_power']:.0f}, {t['basis']})"
    if odds and odds[1]:
        text += "; " + odds_text(*odds)
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


def strongest_fleet(fs):
    """Id of the fleet with the most power, or None."""
    fleets = fs.get("fleets") or []
    return max(fleets, key=lambda f: f.get("power", 0))["id"] if fleets else None


def world_line(world):
    pace = world["pace"]
    label = f"x{pace:g}" + (f" {world['preset']}" if world.get("preset") else "")
    est = world.get("estimate")
    tail = f"; a competent player reaches a Cruiser in about {age(est['first_cruiser_s'])} and the endgame in about {age(est['endgame_s'])}" if est else ""
    hub = "; ".join(f"hub at {hexs(h['location'])} offers {', '.join(h['services'])}" for h in world.get("hub") or [])
    return (f"World: pace {label} (build, research and ship times, and mine output, scale with it; fuel, cooldowns and combat do not){tail}"
            + (f"; {hub}" if hub else ""))


def hostile_words(h, now):
    """A group's losses, when it is whole again, how it follows and what it is doing away from its sector."""
    lost = h["muster"] - h["ship_count"]
    state = f"{h['ship_count']} of {h['muster']} ships" + (f" ({lost} lost)" if lost else "")
    if h.get("whole_at") is not None:
        left = h["whole_at"] - now
        state += f", whole again at tick {h['whole_at']}" + (f" (in {left}s if left alone)" if left > 0 else " (by now)")
    chase = (f"follows a fleet that leaves it {h['chase_hexes']} hex(es)" if h["chase_hexes"] else "follows nobody")
    out = f"{state}; {chase}, jumps in {h['jump_ticks']} tick(s)"
    errand = h.get("errand")
    if errand:
        where = f" to {hexs(errand['to'])}, there at tick {errand['arrives_at']}" if errand.get("to") else ""
        if errand.get("quarry") is not None:
            out += f"; AFTER fleet {errand['quarry']} of {errand.get('quarry_owner')}{where}, {errand['hexes_left']} more hex(es)"
        else:
            out += f"; on its way back to {hexs(errand['home'])}{where}"
    return out


def describe(sec, loc, sectors, now, indent="", odds=None):
    r = sec.get("resources") or {}
    print(f"{indent}sector {loc[0]},{loc[1]} [{freshness(sec, now)}]: {sec['terrain']}  ore metal={r.get('metal')} crystal={r.get('crystal')} deut={r.get('deuterium')}")
    print(f"{indent}threat {threat_text(sec.get('threat'), odds)}")
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
        print(f"{indent}  {hostile_words(h, now)}")
    for pf in sec.get("player_fleets") or []:
        print(f"{indent}player fleet {pf['id']} of {pf['owner_name']}: {pf['ship_count']} ships")
    if sec.get("salvage"):
        left = ""
        if sec.get("salvage_despawn_tick") is not None:
            t = sec["salvage_despawn_tick"] - now
            left = f", despawns in {t}s" if t > 0 else ", despawn time passed (probably gone)"
        print(f"{indent}salvage: {res(sec['salvage'])}{left}")
    if sec.get("site"):
        stale = " (remembered, may be gone)" if sec.get("pins_stale") else ""
        print(f"{indent}derelict site tier {sec['site']['tier']} risk {sec['site']['risk']}{stale}")
    if sec.get("chart"):
        print(f"{indent}chart: " + CHART_MARKS.get(sec["chart"], sec["chart"]))
    if sec.get("outpost"):
        print(f"{indent}{outpost_mark(sec['outpost'])}")




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


def queue_note(free, waiting):
    """Why a queue with a free slot starts nothing, from the server's `waiting_on` for each waiting order.

    The server starts the earliest waiting order that is ready and payable. A ready order that is short and has
    `reserve` set holds the line: nothing behind it starts, and those that could start say `Order`. Without one,
    a short order is passed over, so the slot stays free only while every ready order is short."""
    if free <= 0 or not waiting:
        return None

    def name(w):
        return f"{w['item']}" + (f"->L{w['to']}" if "to" in w else "") + (f" #{w['id']}" if "id" in w else "")

    def need(w):
        return ", ".join(f"{v} {k}" for k, v in (w.get("short") or {}).items()) or "resources"

    holder = next((w for w in waiting if w.get("reserve") and w.get("waiting_on") == "Resources"), None)
    held = [w for w in waiting if w.get("waiting_on") == "Order"]
    if holder and held:
        cover = f", covered in ~{age(holder['covered_in_s'])}" if "covered_in_s" in holder else ", with no income of it"
        return (f"the reserve on {name(holder)} holds the free slot while it is short ({need(holder)}{cover}); "
                f"{len(held)} order(s) behind it that could start now wait for it: {', '.join(name(w) for w in held)}. "
                "cancel_queued it, or order it again without reserve, to let them start")
    short = [w for w in waiting if w.get("waiting_on") == "Resources"]
    later = [w for w in waiting if w.get("waiting_on") == "Prerequisite"]
    if short and len(short) + len(later) == len(waiting):
        every = "every waiting order is short" + (" or waits for a level still to be built" if later else "")
        covered = [w for w in short if "covered_in_s" in w]
        if not covered:
            return f"{every}, of something you have no income of; the free slot stays idle"
        first = min(covered, key=lambda w: w["covered_in_s"])
        return (f"{every}, so the free slot stays idle; the first to be covered is {name(first)} "
                f"({need(first)}) in ~{age(first['covered_in_s'])}. Order something payable to use the slot now")
    return None


def queues_of(fs):
    """The three homeworld queues as plain dicts: running items, waiting items, room."""
    hw, t = fs["homeworld"], fs["tick"]
    prod = hw.get("production") or {}
    waiting_max = hw.get("queue_waiting_max", 3)

    def mk(slots, running, waiting):
        depth = slots + waiting_max
        room = max(depth - len(running) - len(waiting), 0)
        free = max(slots - len(running), 0) if not waiting else 0
        q = {"slots": slots, "depth": depth, "running": running, "waiting": waiting, "room": room, "open": free}
        note = queue_note(slots - len(running), waiting)
        if note:
            q["note"] = note
        return q

    def wait_item(i, item, to, each, w, q=None):
        d = {"index": i, "item": item, "ticks": each}
        if q and q.get("id") is not None:
            d["id"] = q["id"]
        if q and q.get("reserve"):
            d["reserve"] = True
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
        d = wait_item(i, q["item"], None, q["ticks"], None, q)
        d["count"] = q["count"]
        d["paid"] = {k: round(q["cost"][k]) for k in RES if q.get("cost", {}).get(k, 0) > 0}
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


def orders_words(f):
    """A fleet's standing orders as the server carries them: the doctrine and every setting in force, or manual."""
    orders = f.get("policy")
    if not orders:
        return "manual"
    p = orders["params"]
    fuel = "unbounded" if p["fuel_budget_pct"] >= 100 else f"{p['fuel_budget_pct']}%"
    return (f"{orders['preset']} (range {p['max_range']}, engage {p['engage_ratio_x10'] / 10:.1f}x, "
            f"disengage {p['disengage_ratio_x10'] / 10:.1f}x, fuel {fuel}, "
            f"home at cargo {p['cargo_return_pct']}%, reserve {p['min_fuel_pct']}%{', refines its hold' if p.get('refine') else ''})")


def fleet_rows(fs):
    out = []
    sectors = {(s["location"]["q"], s["location"]["r"]): s for s in fs.get("known_sectors") or []}
    for f in fs.get("fleets") or []:
        ships = {s["class"]: s["count"] for s in f["ships"]}
        hull, hull_max = f["hull"], f["hull_max"]
        loc = [f["location"]["q"], f["location"]["r"]]
        row = {"id": f["id"], "at": loc, "state": f["state"], "policy": orders_words(f),
               "fuel": round(f["fuel"]), "fuel_max": round(f["fuel_max"]), "range_hops": f.get("range_hops"),
               "power": round(f.get("power", 0)), "cooldown": f.get("cooldown_remaining", 0),
               "cargo": [round((f.get("cargo") or {}).get(k, 0)) for k in RES], "cargo_cap": round(f.get("cargo_capacity", 0)),
               "hull_pct": round(100 * hull / hull_max) if hull_max else 0, "ships": ships,
               "jump_fuel": round(f.get("jump_fuel", 0)), "home_fuel": round(f.get("home_fuel", 0))}
        damaged = sum(s["damaged"] for s in f["ships"])
        if damaged:
            row["damaged"] = damaged
        if f.get("refit_due"):
            row["refit_due"] = True
        if f.get("cargo_blocked"):
            row["cargo_blocked"] = True
        if f.get("laden"):
            row["laden"] = True
        if f.get("refine"):
            row["refine"] = {k: round(v, 1) for k, v in f["refine"].items()}
        if f.get("refuel"):
            row["refuel"] = f["refuel"]
        if f.get("flight"):
            q = f["flight"]
            row["flight"] = {"odds": round(q["ratio"], 2), "label": q["label"], "fuel": round(q["fuel"]), "rounds": q["rounds"],
                             "chased": bool(q.get("chase")), **({"refusal": q["refusal"]} if q.get("refusal") else {})}
        if f.get("fleeing"):
            row["fleeing"] = f["fleeing"]
        if f.get("pursued_by"):
            row["pursued_by"] = {k: f["pursued_by"][k] for k in ("label", "arrives_in", "catches", "hexes_left")}
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
    raid = hw.get("raid")
    if raid and hw.get("raids_possible", True) and raid["odds"]["label"] in ("lost", "at_risk"):
        out.append(forecast_words(raid, fs["tick"]))
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
    """Own row of the cached leaderboard, with rank out of the players listed, its parts, and the leader
    with the gap to them, in all and by part."""
    if not board:
        return None
    me = fs["player"]["name"]
    rows = list(board.get("entries") or []) + ([board["you"]] if board.get("you") else [])
    mine = next((e for e in rows if e["name"] == me), None)
    if not mine:
        return None
    total = max([e["rank"] for e in rows] or [0])
    out = {"score": round(mine["score"], 1), "rank": mine["rank"], "of": total,
           **{part: round(mine[part], 1) for part in SCORE_PARTS},
           "age_s": max(fs["tick"] - board.get("tick", fs["tick"]), 0)}
    lead = next((e for e in board.get("entries") or [] if e["rank"] == 1), None)
    if lead:
        out["leader"] = {"name": lead["name"], "score": round(lead["score"], 1), "gap": round(lead["score"] - mine["score"], 1),
                         "gap_by_part": {part: round(lead[part] - mine[part], 1) for part in SCORE_PARTS
                                         if round(lead[part] - mine[part], 1)}}
    return out


def leader_words(s):
    """The leader, how far ahead, and in which parts of the score."""
    lead = s.get("leader")
    if not lead:
        return ""
    if s["rank"] == 1:
        return "you lead"
    parts = ", ".join(f"{part} {gap:+.1f}" for part, gap in lead.get("gap_by_part", {}).items())
    return f"leader {lead['name']} {lead['score']:.1f}, {lead['gap']:.1f} ahead of you" + (f" ({parts})" if parts else "")


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
            short = {k: math.ceil(step["cost"].get(k, 0) - pl["resources"].get(k, 0)) for k in RES
                     if step["cost"].get(k, 0) > pl["resources"].get(k, 0)}
            d["ok"] = not short
            if short:
                d["short"] = short
                cover = short_cover(short, hw.get("production") or {})
                if cover is not None:
                    d["covered_in_s"] = math.ceil(cover)
        after = next((b.get("energy_after") for b in hw["catalog"]["buildings"] if b["building_type"] == label), None) \
            if section == "buildings" else None
        if after:
            d["energy_after"] = round(after["fraction"], 3)
            if after["plant_maxed"]:
                d["solar_plant_maxed"] = True
        nxt.setdefault(section, {})[label] = d
    idle = [n for n, q in qs.items() if q["open"] > 0]
    docked = {s["class"]: s["count"] for s in hw.get("docked_ships") or []}
    world = fs.get("world") or {}
    doc = {
        "player": pl["name"], "tick": fs["tick"],
        "minutes_left": None if left is None else round(max(left, 0), 1),
        "score": score_of(fs, board), "score_rule": (board or {}).get("rule") or SCORE_RULE,
        "stock": {k: round(pl["resources"].get(k, 0), 1) for k in RES},
        "cap": {k: round(st["cap"].get(k, 0)) for k in RES},
        "per_s": {k: round(hw["production"].get(k, 0), 4) for k in RES},
        "energy": {"made": hw["energy"]["made"], "needed": hw["energy"]["needed"], "fraction": round(hw["energy"]["fraction"], 3)},
        "full_in_s": {k: v for k, v in (st.get("full_in_s") or {}).items() if v},
        "levels": {"buildings": {b["building_type"]: b["level"] for b in hw["buildings"]},
                   "research": {r["tech"]: r["level"] for r in hw["research"]}},
        "queues": qs, "idle": idle, "next": nxt, "fleets": fleets, "docked": docked,
        "defence": {"power": round(hw.get("home_defence_power", 0)), "raids_possible": hw.get("raids_possible", True),
                    "raid": hw.get("raid")},
        "world": {"pace": world.get("pace"), "preset": world.get("preset")},
        "alerts": derived_alerts(fs, fleets) + list(alerts or []),
    }
    if fs.get("outpost"):
        doc["outpost"] = fs["outpost"]
    if fs.get("relays") and fs["relays"].get("relays"):
        doc["relays"] = fs["relays"]
    if fs.get("hubs"):
        doc["hubs"] = fs["hubs"]
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
        tag = f"#{w['id']}" if "id" in w else f"[{w['index']}]"
        parts.append(f"waiting{tag} {what}{w['item']}{to}{need}")
    if q["room"] and not q["open"]:
        parts.append(f"{q['room']} more fit")
    if q.get("note"):
        parts.append(f"NOTE: {q['note']}")
    return " | ".join(parts)


def snapshot(fs):
    qs = queues_of(fs)
    aff = affordable(fs)
    return {"tick": fs["tick"],
            "q": {n: {"room": q["room"], "open": q["open"], "aff": aff[n]} for n, q in qs.items()},
            "fleets": {f["id"]: {"state": f["state"], "at": (f["location"]["q"], f["location"]["r"]),
                                 "auto": bool(f.get("policy")), "blocked": bool(f.get("cargo_blocked"))}
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
            if c["open"] > 0 and new and not b["aff"]:
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
                why.append(f"fleet {fid} is jumping out of its fight" if f["state"] == "Moving"
                           else f"fleet {fid} left combat ({f['state']})")
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
    sc = (f"SCORE {s['score']:.1f} rank {s['rank']}/{s['of']} ("
          + ", ".join(f"{part} {s[part]:.1f}" for part in SCORE_PARTS if s[part] or part in ("buildings", "research", "fleet"))
          + ")") if s else "SCORE unknown yet"
    idle = ", ".join(doc["idle"]) if doc["idle"] else "none"
    left = doc["minutes_left"]
    standing = leader_words(s) if s else ""
    return (f"{sc} | {'' if left is None else '%.0f min left | ' % left}IDLE QUEUES: {idle}\n"
            + (f"Standing: {standing}\n" if standing else "")
            + f"Scoring: {doc['score_rule']}")


def fleet_line(f):
    cargo = sum(f["cargo"])
    return (f"fleet {f['id']} @{f['at'][0]},{f['at'][1]} {f['state']} {f['policy']} power {f['power']} fuel {f['fuel']}/{f['fuel_max']} "
            f"range {f['range_hops']} cargo {cargo}/{f['cargo_cap']} " + "+".join(f"{n}x{c}" for c, n in f["ships"].items())
            + (" CARGO BLOCKED" if f.get("cargo_blocked") else "") + (" LADEN" if f.get("laden") else ""))


def pick_keys(doc, keys):
    """Only the top-level KEYS of the state document; an unknown key is named with the ones there are."""
    unknown = [k for k in keys if k not in doc]
    if unknown:
        return None, f"no such key in state: {', '.join(unknown)}; the keys are {', '.join(doc)}"
    return {k: doc[k] for k in keys}, None


def cmd_state(name, args=()):
    pretty = "--pretty" in args
    keys = None
    for i, a in enumerate(args):
        if a.startswith("--keys="):
            keys = a.split("=", 1)[1]
        elif a == "--keys" and i + 1 < len(args):
            keys = args[i + 1]
    fs, board = view(name)
    if not fs:
        return no_state(name)
    doc = build_state(fs, board, minutes_left(), recent_alerts(name, fs["tick"]))
    if keys is not None:
        doc, err = pick_keys(doc, [k for k in keys.split(",") if k])
        if err:
            print(err)
            return
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
        raid = doc["defence"]["raid"]
        lose = round((1 - raid["odds"]["kept"]) * 100) if raid else 0
        print("Stock " + " ".join(f"{k[0]}={st[k]:.0f}/{doc['cap'][k]}" for k in RES) + "  income/s " + "/".join(f"{doc['per_s'][k]:.4g}" for k in RES)
              + (f"  raid forecast {raid['odds']['label'].upper().replace('_', ' ')}"
                 + (f", losing about {lose}% of what stands there" if lose else ", nothing lost")
                 + ("" if doc["defence"]["raids_possible"] else " IF ONE CAME (none is on the clock)") if raid else ""))
        for n, q in doc["queues"].items():
            print("  " + queue_line(n, q))
        if fs.get("outpost"):
            print("  " + outpost_line(fs["outpost"], fs["tick"]))
        if relays_line(fs.get("relays"), fs["tick"]):
            print("  " + relays_line(fs["relays"], fs["tick"]))
        if hubs_line(fs.get("hubs"), fs["tick"]):
            print("  " + hubs_line(fs["hubs"], fs["tick"]))
        for f in doc["fleets"]:
            print("  " + fleet_line(f))
        for a in doc["alerts"]:
            print("  ALERT " + a)
        return
    summarize(fs, doc, ask_odds(name, {f["id"]: [(f["location"]["q"], f["location"]["r"])] for f in fs.get("fleets") or []}))
    for a in doc["alerts"]:
        print("ALERT: " + a)


def summarize(fs, doc, odds):
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
    en = hw.get("energy")
    if en:
        short = "" if en["made"] >= en["needed"] else f"  SHORT: the mines run at {en['fraction'] * 100:.0f}%, raise the Solar Plant"
        starved = "  the Fusion Reactor has no deuterium to burn and stands still" if en.get("fusion_starved") else ""
        sats = f", {hw.get('satellites', 0)} Solar Satellites at {en['per_satellite']} each" if hw.get("satellites") else ""
        print(f"Energy: {en['made']} made, {en['needed']} needed by the mines{sats}{short}{starved}")
    eta = [f"{k} full in {age(v)}" for k, v in st["full_in_s"].items() if v]
    print(f"Raid-protected stock: {res(st['protected'])}" + (f"   ({', '.join(eta)})" if eta else ""))
    if p.get("exchange"):
        print("Exchange (a fleet at the hub sells cargo for cargo): " + meter_text(p["exchange"]))
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
            if "paid" in w:
                need = " PAID " + ", ".join(f"{v} {k}" for k, v in w["paid"].items()) + " (half back if cancelled)"
            elif not need and q["open"] == 0 and len(q["running"]) >= q["slots"]:
                need = " (starts when a slot frees up; paid then)"
            why = f" [waits on {str(w['waiting_on']).lower()}" + (f", starts in ~{age(w['start_in_s'])}]" if "start_in_s" in w else "]") \
                if w.get("waiting_on") else ""
            tag = f"#{w['id']}" if "id" in w else f"[{w['index']}]"
            hold = " RESERVE (holds the line)" if w.get("reserve") else ""
            print(f"    waiting {tag}: {what}{w['item']}{to} ({age(w['ticks'])}{' each' if what else ''}){need}{why}{hold}")
        if q.get("note"):
            print(f"    NOTE: {q['note']}")
    stand = [f"{d['count']}x {d['kind']}" for d in hw.get("defences") or [] if d["count"]]
    print("Defences: " + (", ".join(stand) or "none") + f"; Solar Satellites: {hw.get('satellites', 0)}")
    if hw.get("raid"):
        print("  " + forecast_words(hw["raid"], fs["tick"], hw.get("raids_possible", True)))
    docked = {s["class"]: s["count"] for s in hw.get("docked_ships") or []}
    print("Docked ships: " + (", ".join(f"{n}x {c}" for c, n in docked.items()) or "none"))
    if fs.get("outpost"):
        print(outpost_line(fs["outpost"], fs["tick"]))
    if relays_line(fs.get("relays"), fs["tick"]):
        print(relays_line(fs["relays"], fs["tick"]))
    if hubs_line(fs.get("hubs"), fs["tick"]):
        print(hubs_line(fs["hubs"], fs["tick"]))
    sectors = {(s["location"]["q"], s["location"]["r"]): s for s in fs.get("known_sectors") or []}
    live = sum(1 for s in sectors.values() if s.get("live"))
    print(f"Known sectors: {len(sectors)} ({live} live, {len(sectors) - live} remembered/stale)")
    for f in fs.get("fleets") or []:
        loc = (f["location"]["q"], f["location"]["r"])
        classes = {s["class"]: s["count"] for s in f["ships"]}
        hull, hull_max = f["hull"], f["hull_max"]
        cargo = sum((f.get("cargo") or {}).values())
        print(f"\nFleet {f['id']} at {loc[0]},{loc[1]}  {f['state']}  policy={orders_words(f)}  cooldown={f.get('cooldown_remaining', 0)}"
              f"  power {f.get('power', 0):.0f}  range {f.get('range_hops', 0)} hops out and back"
              + ("  CARGO BLOCKED: stockpile full, spend something" if f.get("cargo_blocked") else "")
              + ("  LADEN: loaded by hand, stays aboard at the dock (unload puts it back)" if f.get("laden") else "")
              + (f"  REFINE: {f['refine']['deuterium']:.1f} deuterium in the hold makes {f['refine']['fuel']:.1f} fuel" if f.get("refine") else "")
              + (f"  AT A HUB. {refuel_text(f['refuel'])}" if f.get("refuel") else ""))
        print(f"  ships: " + ", ".join(f"{n}x {c}" for c, n in classes.items())
              + f"  hull {hull:.0f}/{hull_max:.0f}  fuel {f['fuel']:.0f}/{f['fuel_max']:.0f}"
              + f"  cargo {cargo:.0f}/{f['cargo_capacity']:.0f} ({res(f.get('cargo'))})")
        worn = [f"{s['damaged']} of {s['count']} {s['class']} damaged" + (f" ({s['at_risk']} at 70% hull or under)" if s["at_risk"] else "")
                for s in f["ships"] if s["damaged"]]
        if worn:
            print("  damage: " + ", ".join(worn) + repair_words(f.get("repair")))
        if f.get("flight"):
            print("  IN A FIGHT: " + flight_words(f["flight"]))
        if f.get("fleeing"):
            g = f["fleeing"]
            print(f"  LEAVING the fight for {hexs(g['to'])}: gone in {g['ticks']} tick(s), {g['rounds']} more round(s) under fire"
                  " (stop calls the jump off, its fuel is forfeit, and the fleet fights on)")
        if f.get("pursued_by"):
            p = f["pursued_by"]
            print(f"  PURSUED by the {p['label']} from {hexs(p['from'])}: it lands here in {p['arrives_in']} tick(s), "
                  + ("BEFORE a jump ordered now would be gone" if p["catches"] else "too late to catch a jump ordered now")
                  + f", and follows {p['hexes_left']} more hex(es) after that")
        r = f["refit"]
        print(f"  fights at Weapons {r['weapons']}, Shielding {r['shielding']}, Armour {r['armour']}"
              + ("  REFIT DUE: your research is ahead of it; dock at home" if f.get("refit_due") else "")
              + "  (split names classes and counts: \"ships\":[{\"class\":\"LightFighter\",\"count\":2}])")
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
            describe(sec, loc, sectors, fs["tick"], indent="  ", odds=(f["id"], odds.get((f["id"], *loc))))


GROWTH = {"MetalMine": 1.5, "CrystalMine": 1.6, "DeuteriumSynthesizer": 1.5, "SolarPlant": 1.5, "FusionReactor": 1.8}
OTHER_GROWTH = 2.0
MINES = {"MetalMine": ("metal", 30.0), "CrystalMine": ("crystal", 20.0), "DeuteriumSynthesizer": ("deuterium", 12.0)}


def weighted(cost):
    return sum(WEIGHT[k] * cost.get(k, 0) for k in RES)


def effects_of(fs):
    """label -> the catalog entry's effects: what it does at its level and at the next, as the server says it."""
    cat = fs["homeworld"]["catalog"]
    out = {o["building_type"]: o.get("effects") or [] for o in cat["buildings"]}
    out.update({o["tech"]: o.get("effects") or [] for o in cat["research"]})
    return out


def effect_text(effects):
    def one(e):
        name = e["kind"].replace("_", " ")
        return f"{name} {e['now']:.4g}" + (f" -> {e['next']:.4g}" if e.get("next") is not None else "")
    return "; ".join(one(e) for e in effects)


def future_steps(label, level, max_level, nxt, section, count=3):
    """The next steps of a leveled upgrade: the real one, then the same curve extrapolated (marked inexact)."""
    if not nxt:
        return []
    steps = [{"level": nxt["level"], "cost": nxt["cost"], "ticks": nxt["ticks"], "exact": True}]
    g = GROWTH.get(label, OTHER_GROWTH)
    while len(steps) < count and steps[-1]["level"] < (max_level or 0):
        p = steps[-1]
        cost = {k: round(p["cost"].get(k, 0) * g) for k in RES}
        was = p["cost"].get("metal", 0) + p["cost"].get("crystal", 0)
        ticks = max(1, round(p["ticks"] * (cost["metal"] + cost["crystal"]) / was)) if was else p["ticks"]
        steps.append({"level": p["level"] + 1, "cost": cost, "ticks": ticks, "exact": False})
    return steps


def mine_gain(label, level, steps, fs, effects):
    """Per-step production gain (units per tick) of a mine with all the energy it needs.

    The first step is the server's own figure (the mine's effect, now and next); the steps
    beyond it follow the curve."""
    res_name, per_hour = MINES[label]
    pace = (fs.get("world") or {}).get("pace") or 1
    def p(l):
        return per_hour / 3600 * pace * l * 1.1 ** l
    told = next((e for e in effects if e["kind"] == f"{res_name}_per_tick"), None)
    gains, lv = [], level
    for i, _ in enumerate(steps):
        exact = i == 0 and told is not None and told.get("next") is not None
        gains.append((res_name, told["next"] - told["now"] if exact else p(lv + 1) - p(lv)))
        lv += 1
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
    effects = effects_of(fs)
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
            does = effect_text(effects.get(label) or [])
            if nxt is None:
                lines.append(f"  {label:<22} L{lvl}/{mx}  maxed" + (f"   {does}" if does else ""))
                continue
            steps = future_steps(label, lvl, mx, nxt, title, depth)
            gains = mine_gain(label, lvl, steps, fs, effects.get(label) or []) if label in MINES else None
            lines.append(f"  {label:<22} L{lvl}/{mx}" + (f"   {does}" if does else ""))
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
                     "metal 1: crystal 1.5: deut 2; a mine's gain is with all the energy it needs, so raise the Solar "
                     "Plant first; mines gain +10%/level compounding on level x output)")
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
    parts = lambda e: " ".join(f"{e[part]:>9.1f}" for part in SCORE_PARTS)
    print("rank  name                    score buildings  research     fleet   outpost   defence   (one table for humans and AI agents)")
    for e in board["entries"] + ([board["you"]] if board.get("you") else []):
        tag = "AI " if e["agent"] else "   "
        print(f"{e['rank']:>4}  {e['name']:<20} {tag}{e['score']:>7.1f} {parts(e)}")
    if board.get("bots"):
        print("built-in bots (names start BOT-; they play by your rules and are not ranked with you):")
        for e in board["bots"]:
            print(f" BOT  {e['name']:<20}    {e['score']:>7.1f} {parts(e)}")
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
    at = (int(q), int(r))
    fid = strongest_fleet(fs)
    odds = ask_odds(name, {fid: [at]}) if fid is not None else {}
    describe(sec, at, sectors, fs["tick"], odds=(fid, odds.get((fid, *at))))


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
    fid = strongest_fleet(fs)
    odds = ask_odds(name, {fid: shown}) if fid is not None else {}
    live = sum(1 for s in sectors.values() if s.get("live"))
    print(f"{len(shown)} known sectors within {radius} of your fleets "
          f"(of {len(sectors)} ever seen: {live} live, {len(sectors) - live} remembered). "
          "LIVE = current; STALE Nm = as last seen N ago. "
          "T = threat rating 1-9; odds are the server's for your strongest fleet"
          + (f" ({fid})." if odds else " (no answer from the server, so none shown)."))
    for k in shown:
        s = sectors[k]
        r = s.get("resources") or {}
        bits = [freshness(s, now), s["terrain"], f"m={r.get('metal')} c={r.get('crystal')} d={r.get('deuterium')}"]
        t = s.get("threat")
        if t:
            o = odds.get((fid, *k))
            bits.append(f"T{t['rating']}" + (f" {o['ratio']:.1f}x {str(o['label']).upper()}" if o else ""))
        if s.get("hostiles"): bits.append(f"HOSTILES x{len(s['hostiles'])}")
        if s.get("player_fleets"): bits.append("players: " + ",".join(p["owner_name"] for p in s["player_fleets"]))
        if s.get("salvage"): bits.append("salvage")
        if s.get("site"): bits.append(f"derelict t{s['site']['tier']} {s['site']['risk']}")
        if s.get("chart"): bits.append(f"chart {s['chart']}")
        if s.get("outpost"): bits.append(outpost_mark(s["outpost"]))
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


def cmd_odds(name, fleet_id, *coords):
    """How a fleet would fare in the named sectors, as the server rates it."""
    fid = int(fleet_id)
    sectors = [(int(coords[i]), int(coords[i + 1])) for i in range(0, len(coords), 2)]
    odds = ask_odds(name, {fid: sectors})
    header(name, last_tick(name))
    if not odds:
        print(f"no odds from the server for fleet {fid} (is it yours?); try again")
    for q, r in sectors:
        o = odds.get((fid, q, r))
        if o:
            print(f"{q},{r}: threat {threat_text(o['threat'], (fid, o))}")


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


MESSAGES = ("policy_update",)


def normalize(c):
    """An order is {"action": ...}; standing orders are a message of their own, {"type": "policy_update", ...}.
    Either form is taken for standing orders, and the one the server reads is sent."""
    if c.get("action") in MESSAGES and "type" not in c:
        return {"type": c["action"], **{k: v for k, v in c.items() if k != "action"}}
    return c


def fence_for(fs, cmd):
    """A question sent behind an order: the server answers one player's messages in the order they came, so
    whatever comes back between the order and this answer is the order's own. It asks the odds of a fleet in
    no sector, which costs nothing and is never asked otherwise; a fleet the order does not name is preferred,
    and with no fleet at all the answer is the refusal for fleet 0. Returns (fleet id, message)."""
    ids = [f["id"] for f in (fs or {}).get("fleets") or []]
    fid = next((i for i in ids if i != cmd.get("fleet_id")), ids[0] if ids else 0)
    return fid, {"action": "odds", "fleet_id": fid, "sectors": []}


def is_fence(m, fid):
    if m.get("type") == "odds":
        return m.get("fleet_id") == fid and not m.get("sectors")
    return (m.get("type") == "error" and m.get("code") == "FleetNotFound"
            and str(m.get("message", "")).startswith(f"fleet {fid} not found"))


def split_windows(msgs, fids):
    """The logged messages cut at the answers to the fences FIDS, in order: one list per order of what came
    back for it, the number of fences answered, and what came after the last answer."""
    windows, cur = [], []
    for m in msgs:
        if len(windows) < len(fids) and is_fence(m, fids[len(windows)]):
            windows.append(cur)
            cur = []
        else:
            cur.append(m)
    return windows, len(windows), cur


def refusal_of(window):
    """The server's or the client's refusal of the order whose answers are WINDOW, or None."""
    return next((m for m in window if m.get("type") in ("error", "client_error")), None)


def flat(text):
    return re.sub(r"[\s_]", "", str(text)).lower()


def queue_outcome(cmd, queue_events, used):
    """The server's own Queue event for an order: the authoritative 'started' or 'waiting'."""
    if cmd.get("action") in ("cancel_queued", "cancel_build"):
        for i, q in enumerate(queue_events):
            if i not in used and str(q.get("action", "")).lower() == "cancelled" \
                    and (cmd.get("id") is None or q.get("id") == cmd.get("id")):
                used.add(i)
                back = {k: v for k, v in (q.get("refunded") or {}).items() if v}
                return f"cancelled {q['item']} #{q.get('id', '?')}" + (f", refunded {res(back)}" if back else ", nothing was paid yet")
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
            return f"{q['item']} #{q.get('id', '?')} started" + (f", paid {res(paid)}" if paid else "")
        if act == "waiting":
            eta = f", starts in ~{age(q['start_in'])}" if q.get("start_in") is not None else ""
            paid = {k: v for k, v in (q.get("paid") or {}).items() if v}
            took = f"ordered, paid {res(paid)}" if paid else "queued"
            return f"{q['item']} #{q.get('id', '?')} {took}, waits on {str(q.get('waiting_on', '?')).lower()}{eta}"
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
                tag = f"#{w['id']}" if "id" in w else f"[{w['index']}]"
                return f"queued {tag} waiting for " + ", ".join(f"{v} {k}" for k, v in w["short"].items())
            return f"queued #{w['id']}" if "id" in w else f"queued [{w['index']}]"
    return "accepted (already finished or not visible yet)"


FENCE_WAIT = 4.0


def reply_line(cmd, m):
    """The line for a question's answer in the window of the order that asked it; None for anything else."""
    if m.get("type") != cmd.get("action"):
        return None
    words = {"preview_move": preview_line, "odds": odds_line, "leaderboard": leaderboard_line,
             "exchange_quote": quote_line, "outpost_survey": survey_line, "relay_survey": relay_survey_line}.get(m["type"])
    return words(m) if words else None


def send_fenced(name, pairs, gap):
    """Send each (order, fence) pair and wait until every fence is answered and the tick after the last
    answer has arrived. Returns (the logged messages since sending, the fleet ids of the fences)."""
    start = log_size(name)
    fids = []
    for i, (cmd, (fid, fence)) in enumerate(pairs):
        if i:
            time.sleep(gap)
        send(name, cmd)
        send(name, fence)
        fids.append(fid)

    def answered(ls):
        _, seen, after = split_windows(ls, fids)
        return seen == len(fids) and any(m.get("type") == "tick_update" for m in after)

    lines, _ = collect(name, start, answered, FENCE_WAIT + gap * len(pairs))
    return lines, fids


def run_commands(name, cmds, gap=SEND_GAP, keep_going=False):
    """Send orders in order, each followed by a fence, so that every answer is the order's own.

    By default an order is sent only once the one before it has been answered and not refused: the first
    refusal stops the batch. With keep_going every order is sent at once, each behind its fence, and all are
    answered in one tick. Returns (results, messages): one (order, window, refusal or "timeout" or None) per
    order sent, and every message logged meanwhile."""
    results, lines = [], []
    batches = [cmds] if keep_going else [[c] for c in cmds]
    for batch in batches:
        fs, _ = view(name)
        got, fids = send_fenced(name, [(c, fence_for(fs, c)) for c in batch], gap)
        lines += got
        windows, seen, _ = split_windows(got, fids)
        for i, c in enumerate(batch):
            if i >= seen:
                results.append((c, [], "timeout"))
                continue
            results.append((c, windows[i], refusal_of(windows[i])))
        if not keep_going and results[-1][2] is not None:
            break
    return results, lines


def cmd_do(name, args):
    fast = "--fast" in args
    keep_going = "--keep-going" in args
    gap = SEND_GAP
    rest = []
    it = iter(a for a in args if a not in ("--fast", "--keep-going"))
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
    cmds = [normalize(c) for c in cmds]
    unseen, _ = log_lines(name, cursor(name))
    caught_up = not collapse_events(unseen, errors=False)
    n = len(cmds)
    tags = [f"[{i + 1}/{n}] " if n > 1 else "" for i in range(n)]
    if fast:
        for i, c in enumerate(cmds):
            if i:
                time.sleep(gap)
            send(name, c)
        time.sleep(0.15)
        header(name, last_tick(name))
        for tag, c in zip(tags, cmds):
            print(f"{tag}sent {cmd_label(c)} (not confirmed: --fast does not wait for the server)")
        return
    results, lines = run_commands(name, cmds, gap, keep_going)
    fs, _ = view(name)
    header(name, fs["tick"] if fs else None)
    qs = queues_of(fs) if fs else None
    qev = [e for m in lines if m.get("type") == "tick_update" for e in (m.get("events") or []) if e.get("kind") == "Queue"]
    used = set()
    for tag, (c, window, refused) in zip(tags, results):
        if refused == "timeout":
            print(f"{tag}NO ANSWER {cmd_label(c)}: the server did not answer in {FENCE_WAIT:.0f}s; it may still be applied, "
                  "check `./play state`")
        elif refused is not None:
            print(f"{tag}ERR {cmd_label(c)}: " + " ".join(str(x) for x in (refused.get("code"), refused.get("message")) if x))
        else:
            o = queue_outcome(c, qev, used) or outcome(c, qs)
            print(f"{tag}ok {cmd_label(c)}" + (f" -> {o}" if o else ""))
        for m in window:
            line = reply_line(c, m)
            if line:
                print("  " + line)
    unsent = cmds[len(results):]
    if unsent:
        print(f"NOT SENT, because an order before them was refused (--keep-going sends the rest anyway): "
              + "; ".join(f"{tags[len(results) + i]}{cmd_label(c)}" for i, c in enumerate(unsent)))
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
    bots = m.get("bots") or []
    return ("leaderboard: " + "; ".join(f"{e['rank']}. {e['name']} {e['score']:.1f}" for e in m["entries"][:10])
            + (f"; {len(bots)} bots unranked, best {bots[0]['name']} {bots[0]['score']:.1f}" if bots else ""))


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
    if proc_alive(pidfile, "iac-headless"):
        try:
            os.kill(int(pidfile.read_text()), 15)
            hlog(f"killed stale client of {name} left by an earlier daemon")
        except (OSError, ValueError):
            pass
    while True:
        with open(LOGS / f"{name}.ndjson", "ab") as out, open(LOGS / f"{name}.stderr", "ab") as err:
            p = subprocess.Popen([str(CLIENT), "--port", str(PORT), "--name", name],
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
  state [--pretty] [--keys a,b,...]
                         ONE compact JSON document, returns at once: stock, caps, income, levels, queues (running,
                         waiting, shortfalls, a note when a free slot starts nothing), idle queues, next costs (with
                         what is short), fleets, score (rank, its parts, the leader and the gap),
                         minutes left, alerts; --keys prints only those top-level keys, e.g. --keys stock,queues,score
  status [--brief]       readable view: headline (score, rank, time, idle queues, scoring rule), stock, queues with
                         their waiting items and shortfalls, fleets and their sector; --brief is the short form
  costs [buildings|research|ships|defences]  next 3 levels with cost, time, what each unlocks, mine payback
  score [rows]           the leaderboard (default 10 rows); your own row is added if you are below it
  map [radius]           known sectors near your fleets (default radius 4), with the server's odds for your strongest fleet
  sector Q R             details of one known sector
  preview FLEET [Q R..]  jump cost and threat for adjacent sectors; with no Q R, every exit of the fleet's sector
  odds FLEET Q R [Q R..] how that fleet would fare in any sectors, near or far, seen or not
                         (odds are the server's: it plays the real round with the ships on each side, so classes,
                         damage and rapid fire count. SAFE >= 2.5x wins and keeps nine tenths of the fleet,
                         FAVOURABLE >= 1.5x wins and keeps two thirds to nine tenths, EVEN >= 1.1x wins three times
                         in four or better at the cost of a third or more, RISKY >= 0.9x is as likely lost as won,
                         else DEADLY)
  do '<json>' ['<json>' ...]   send commands in order (or one JSON array: do '[{...},{...}]'); one result line each,
                         each the server's own answer to that order. The batch stops at the first order refused and
                         says which were not sent, so a refused load does not send the move after it; --keep-going
                         sends them all (in one tick); --fast sends them all and does not wait for any answer
                         e.g. play do '{"action":"build","building_type":"MetalMine"}' '{"action":"research","tech":"EnergyTechnology"}'
                         standing orders: play do '{"type":"policy_update","fleet_id":2,"preset":"prospect"}'
                         ({"action":"policy_update",...} is taken too and sent as the message it is)
                         with settings:   play do '{"type":"policy_update","fleet_id":2,"preset":"mine_and_return",
                                                    "params":{"max_range":8,"engage_ratio_x10":15,"fuel_budget_pct":40}}'
                         (fuel_budget_pct: the share of your deuterium income the fleet may spend on fuel, default 20,
                         100 = no bound; a fleet waiting on its budget says so in a policy hold event)
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
    elif cmd == "state": cmd_state(name, rest)
    elif cmd == "costs": cmd_costs(name, *pos[:1])
    elif cmd == "score" or cmd == "leaderboard": cmd_score(name, *pos[:1])
    elif cmd == "map": cmd_map(name, *pos[:1])
    elif cmd == "sector" and len(pos) == 2: cmd_sector(name, *pos)
    elif cmd == "preview" and len(pos) >= 1: cmd_preview(name, *pos)
    elif cmd == "odds" and len(pos) >= 3 and len(pos) % 2: cmd_odds(name, *pos)
    elif cmd == "do" and rest: cmd_do(name, rest)
    elif cmd == "events": cmd_events(name, "--mine" in rest)
    elif cmd == "wait": cmd_wait(name, rest)
    elif cmd == "rules": print((HERE / "RULES.md").read_text())
    else: print(USAGE)


if __name__ == "__main__":
    main()
