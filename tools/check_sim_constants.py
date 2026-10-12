#!/usr/bin/env python3
"""Hold the simulation's formulas to the game's.

docs/design/economy/sim.py declares none of the game's numbers: it reads them
from numbers.json beside it, which Rust renders, so there is no constant left
to compare. What the simulation still works out for itself (a cost at a level,
a build time, a cap at a pace, the power of a ring's group) is compared here
with what the Rust formulas give: the "tables" half of the same file.

    python3 tools/check_sim_constants.py        # exit 0 when they agree

It runs `cargo test -p iac-sim --test economy_numbers` first, so the file it
reads is what Rust renders now.
"""
import importlib.util
import json
import pathlib
import subprocess
import sys

sys.dont_write_bytecode = True  # do not leave a __pycache__ in docs/

ROOT = pathlib.Path(__file__).resolve().parent.parent
ECONOMY = ROOT / "docs/design/economy"

problems = []
compared = 0


def check(what, rust, sim, rel=0.0, absolute=0.0):
    global compared
    compared += 1
    if abs(rust - sim) > max(absolute, rel * abs(sim)):
        problems.append(f"{what}: rust {rust} vs sim {sim}")


def check_triple(what, rust, sim, **tolerance):
    for i in range(3):
        check(f"{what}[{i}]", rust[i], sim[i], **tolerance)


def rendered_tables():
    run = subprocess.run(
        ["cargo", "test", "-q", "-p", "iac-sim", "--test", "economy_numbers"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if run.returncode != 0:
        sys.exit(run.stdout + run.stderr)
    return json.loads((ECONOMY / "numbers.json").read_text())["tables"]


def load_sim():
    spec = importlib.util.spec_from_file_location("iac_sim", ECONOMY / "sim.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def player(sim, pace=1.0, **levels):
    """A simulated player at `pace` with the given building levels."""
    s = sim.Sim(sim.Preset("check", pace, 1.0, 1.0, 0, 0))
    s.b.update(levels)
    return s


def check_queues(sim, tables):
    """Costs unrounded against whole units; seconds against whole ticks."""
    short = {rust: key for names in (sim.B_NAME, sim.R_NAME, sim.S_NAME, sim.D_NAME) for key, rust in names.items()}
    plain, fab4, lab5, yard5 = player(sim), player(sim, fab=4), player(sim, lab=5), player(sim, shipyard=5)
    for row in tables["building_levels"]:
        key, level, what = sim.SHORT[row["building"]], row["level"], f"{row['building']} L{row['level']}"
        check_triple(f"{what} cost", row["cost"], plain.b_cost(key, level), absolute=0.51)
        check(f"{what} ticks", row["ticks"], plain.b_time(key, level), rel=0.01, absolute=2)
        check(f"{what} ticks, Fabricator 4", row["ticks_fabricator_4"], fab4.b_time(key, level), rel=0.01, absolute=2)
        if key in sim.PROD_BASE:
            mine = ("metal", "crystal", "deut").index(key)
            check(f"{what} per hour", row["per_hour"], player(sim, **{key: level}).prod_h()[mine], rel=1e-4)
    for row in tables["research_levels"]:
        key, level, what = sim.SHORT[row["tech"]], row["level"], f"{row['tech']} L{row['level']}"
        check_triple(f"{what} cost", row["cost"], plain.r_cost(key, level), absolute=0.51)
        check(f"{what} ticks", row["ticks"], plain.r_time(key, level), rel=0.01, absolute=2)
        check(f"{what} ticks, Lab 5", row["ticks_lab_5"], lab5.r_time(key, level), rel=0.01, absolute=2)
    for row in tables["shipyard"]:
        key = short[row["item"]]
        check(f"{row['item']} ticks", row["ticks"], plain.s_time(key), rel=0.01, absolute=2)
        check(f"{row['item']} ticks, Shipyard 5", row["ticks_shipyard_5"], yard5.s_time(key), rel=0.01, absolute=2)


def check_storage_and_raids(sim, tables):
    for row in tables["storage_caps"]:
        cap = player(sim, row["pace"], vault=row["vault"]).cap()
        check_triple(f"cap at Vault {row['vault']}, pace {row['pace']}", row["cap"], cap, rel=1e-4)
    for row in tables["raid_power"]:
        check(f"raid power at {row['econ']} points", row["power"], sim.raid_power(row["econ"]), rel=1e-4)


def check_rings(sim, tables):
    for row in tables["rings"]:
        d = row["ring"]
        cls, count, mult = sim.npc_group(d)
        check(f"npc power, ring {d}", row["npc_power"], sim.npc_power(d), rel=1e-4)
        check(f"threat, ring {d}", row["threat_rating"], sim.threat(d))
        check(f"npc count, ring {d}", row["count"], count)
        check(f"npc stat multiplier, ring {d}", row["stat_multiplier"], mult, rel=1e-5)
        check(f"npc group power, ring {d}", row["group_power"], count * mult * sim.ship_power(cls), rel=1e-4)
        check(f"outpost raid power, ring {d}", row["outpost_raid_power"], sim.outpost_raid_power(d), rel=1e-4)
        check(f"ring multiplier, ring {d}", row["ring_mult"], sim.ORE_RING_MULT(d), rel=1e-4)
        check(f"ore regeneration hours, ring {d}", row["ore_regen_hours"], sim.ore_regen_h(d), rel=1e-5)
        check(f"wreck value, ring {d}", row["wreck_value"], sim.kill_loot_value(d), rel=1e-4)
        check(f"derelict value, ring {d}", row["derelict_value"], sim.derelict_value(d), rel=1e-4)
        check(f"derelict tier, ring {d}", row["derelict_tier"], sim.derelict_tier(d))


def check_score(sim, tables):
    for row in tables["ratings"]:
        t = row["rating"]
        check(f"chart points at T{t}", row["chart_points"], sim.chart_points(t), rel=1e-5)
        check(f"discovery points at T{t}", row["discovery_points"], sim.discovery_points(t), rel=1e-5)
    for row in tables["kill_pay"]:
        check(f"kill pay at ratio {row['ratio']}", row["pay"], sim.kill_pay(row["ratio"]), rel=1e-6)


def check_exchange(sim, tables):
    for row in tables["exchange_spans"]:
        pace = row["pace"]
        check(f"exchange window at pace {pace}", row["window"], sim.exchange_window(pace), absolute=1)
        check(f"exchange span of mines {row['mines']} at pace {pace}", row["span"], sim.exchange_span(row["mines"], pace), rel=1e-3)
    for row in tables["exchange_returns"]:
        check(f"exchange return {row['value']} at {row['pressure']} of {row['span']}", row["back"],
              sim.exchange_return(row["pressure"], row["value"], row["span"]), rel=1e-9, absolute=1e-9)


def check_outposts(sim, tables):
    for row in tables["outpost_levels"]:
        level, pace = row["level"], row["pace"]
        what = f"outpost L{level} at pace {pace}"
        check_triple(f"{what} store", row["store_cap"], sim.outpost_store_cap(level, pace), rel=1e-5)
        check(f"{what} platform", row["platform"], sim.outpost_platform(level), rel=1e-6)
        if level == 0:
            continue  # under construction: it has the store of level 1 and no cost of its own
        seconds = sim.outpost_build_s(level, pace)
        check_triple(f"{what} cost", row["cost"], sim.outpost_cost(level), absolute=0.51)
        check(f"{what} ticks", row["ticks"], seconds, rel=0.01, absolute=1)
        check(f"{what} repair ticks", row["repair_ticks"], seconds * sim.OUTPOST_REPAIR_SHARE, rel=0.01, absolute=1)
        check(f"{what} repair price", row["repair_price"], sim.outpost_repair_price(level), rel=1e-6)
        check(f"{what} points", row["points"], sim.outpost_points(level), rel=1e-4)
    for row in tables["outpost_yields"]:
        for density, per_hour in zip("SMRP", row["per_hour"]):
            check(f"outpost yield {density}, ring {row['ring']}, L{row['level']}, pace {row['pace']}", per_hour,
                  sim.outpost_yield_h(density, row["ring"], row["level"], row["pace"]), rel=1e-4)


def main():
    tables = rendered_tables()
    sim = load_sim()
    check_queues(sim, tables)
    check_storage_and_raids(sim, tables)
    check_rings(sim, tables)
    check_score(sim, tables)
    check_exchange(sim, tables)
    check_outposts(sim, tables)
    if problems:
        print(f"{len(problems)} of {compared} formula values in docs/design/economy/sim.py disagree with shared/:")
        for p in problems[:60]:
            print("  " + p)
        sys.exit(1)
    print(f"sim.py's formulas agree with the game: {compared} values "
          "(costs and times by level, production, storage caps, raid power, the ring gradient, score points, exchange, outposts)")


if __name__ == "__main__":
    main()
