#!/usr/bin/env python3
"""Keep every queue busy through the play CLI and count accepted orders per minute.

  bench_loop.py PLAY_WRAPPER SECONDS [--old PLAY_WRAPPER_OF_OLD_TOOL]

New mode reads `state`, sends all orders in one batched `do`, and sleeps with
`wait --until idle`. With --old PLAY_WRAPPER_OF_OLD_TOOL the same decisions are sent one
order per `do` call followed by `wait 5` (the old tool's minimum), as the old CLI required.
A sampler thread reads `state` once a second and reports how much queue time was lost while a queue sat empty
although something it could run was payable (idle_payable_pct).
"""
import json, subprocess, sys, threading, time

BUILD_ORDER = ["MetalMine", "CrystalMine", "DeuteriumSynthesizer", "ResearchLab", "Shipyard", "Fabricator",
               "StorageVault", "SensorArray", "FuelDepot", "DefenseGrid"]
SHIPS = ["Scout", "Corvette", "Hauler", "Frigate"]


def run(play, *args):
    return subprocess.run([play, *args], capture_output=True, text=True, timeout=120).stdout


def get_state(play):
    return json.loads(run(play, "state"))


def decide(st):
    """One order for each queue that has room and something payable."""
    stock = st["stock"]
    cmds = []
    spend = dict(stock)

    def pay(cost):
        if all(spend[k] >= c for k, c in zip(("metal", "crystal", "deuterium"), cost)):
            for k, c in zip(("metal", "crystal", "deuterium"), cost):
                spend[k] -= c
            return True
        return False

    q = st["queues"]
    nb = st["next"]
    if q["build"]["room"] > 0:
        opts = [(st["levels"]["buildings"].get(b, 0), BUILD_ORDER.index(b), b) for b in BUILD_ORDER
                if b in nb["buildings"] and nb["buildings"][b].get("ok")]
        for _lvl, _i, b in sorted(opts):
            if pay(nb["buildings"][b]["cost"]):
                cmds.append({"action": "build", "building_type": b})
                break
    if q["research"]["room"] > 0:
        opts = sorted((v["cost"][0] + v["cost"][1], t) for t, v in nb["research"].items() if v.get("ok"))
        for _c, t in opts:
            if pay(nb["research"][t]["cost"]):
                cmds.append({"action": "research", "tech": t})
                break
    if q["ship"]["room"] > 0:
        for s in SHIPS:
            v = nb["ships"].get(s)
            if v and v.get("ok") and pay(v["cost"]):
                cmds.append({"action": "build_ship", "ship_class": s, "count": 1})
                break
    return cmds


def main():
    play, secs = sys.argv[1], int(sys.argv[2])
    old = sys.argv[sys.argv.index("--old") + 1] if "--old" in sys.argv else None
    idle = {"n": 0, "slots": 0}
    stop = threading.Event()

    def sampler():
        while not stop.is_set():
            try:
                st = get_state(play)
                want = {{"build": "build", "research": "research", "build_ship": "ship"}[c["action"]] for c in decide(st)}
                idle["n"] += sum(1 for k in want if st["queues"][k]["open"] > 0)
                idle["slots"] += 3
            except Exception:
                pass
            time.sleep(1)
    threading.Thread(target=sampler, daemon=True).start()
    t0, ok, err, calls = time.time(), 0, 0, 0
    while time.time() - t0 < secs:
        st = get_state(play)
        cmds = decide(st)
        if cmds and old:
            for c in cmds:
                out = run(old, "do", json.dumps(c))
                calls += 1
                bad = "  error:" in out or "client_error" in out
                ok += not bad; err += bad
            run(old, "wait", "5")
        elif cmds:
            out = run(play, "do", *[json.dumps(c) for c in cmds])
            calls += 1
            ok += sum(1 for line in out.splitlines() if " ok " in line or line.startswith("ok "))
            err += sum(1 for line in out.splitlines() if " ERR " in line)
            time.sleep(0.2)
        else:
            run(old, "wait", "5") if old else run(play, "wait", "30", "--until", "idle")
            calls += 1
    stop.set()
    mins = (time.time() - t0) / 60
    print(json.dumps({"mode": "old" if old else "new", "minutes": round(mins, 2), "orders_ok": ok, "orders_rejected": err,
                      "orders_per_min": round(ok / mins, 1), "cli_calls": calls,
                      "idle_payable_pct": round(100 * idle["n"] / max(idle["slots"], 1), 1)}))


if __name__ == "__main__":
    main()
