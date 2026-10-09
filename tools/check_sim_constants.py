#!/usr/bin/env python3
"""Compare the economy numbers in the Rust game with the simulation.

docs/design/economy/sim.py holds the constants the spec's tables came from;
shared/ holds the ones the server runs. This runs the Rust side
(`cargo run -p iac-shared --example economy_constants`) and checks costs,
times, production, storage, defences, raids and presets against sim.py.

    python3 tools/check_sim_constants.py        # exit 0 when they agree
"""
import importlib.util
import json
import pathlib
import re
import subprocess
import sys

sys.dont_write_bytecode = True  # do not leave a __pycache__ in docs/

ROOT = pathlib.Path(__file__).resolve().parent.parent
SIM = ROOT / "docs/design/economy/sim.py"

BUILDINGS = {
    "metal": "MetalMine", "crystal": "CrystalMine", "deut": "DeuteriumSynthesizer", "shipyard": "Shipyard",
    "lab": "ResearchLab", "depot": "FuelDepot", "sensor": "SensorArray", "grid": "DefenseGrid",
    "vault": "StorageVault", "fab": "Fabricator",
}
RESEARCH = {
    "corvette": "CorvetteTech", "hauler": "HaulerTech", "frigate": "FrigateTech", "cruiser": "CruiserTech",
    "fuel_eff": "FuelEfficiency", "tanks": "ExtendedFuelTanks", "hulls": "ReinforcedHulls",
    "shields": "AdvancedShields", "weapons": "WeaponsResearch", "nav": "Navigation",
    "harvest": "HarvestingEfficiency", "modfab": "ModularFabrication",
}
SHIPS = {"scout": "Scout", "corvette": "Corvette", "frigate": "Frigate", "cruiser": "Cruiser", "hauler": "Hauler"}
DEFENCES = {"turret": "PulseTurret", "lancer": "LancerBattery", "bastion": "IonBastion"}
SIM_NAME = {**BUILDINGS, **RESEARCH}

problems = []


def check(what, rust, sim, rel=0.0, absolute=0.0):
    if abs(rust - sim) > max(absolute, rel * abs(sim)):
        problems.append(f"{what}: rust {rust} vs sim {sim}")


def load_sim():
    spec = importlib.util.spec_from_file_location("iac_sim", SIM)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def rust_numbers():
    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "iac-shared", "--example", "economy_constants"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def source_const(path, name):
    text = (ROOT / path).read_text()
    m = re.search(rf"pub const {name}: [^=]+= ([0-9_.]+)", text)
    if not m:
        raise SystemExit(f"{name} not found in {path}")
    return float(m.group(1).replace("_", ""))


def cost_seconds(cost, rate, speed=1.0):
    return (cost[0] + cost[1]) / rate * 3600.0 / speed


def check_buildings(sim, rust):
    misc = rust["misc"]
    check("BUILD_RATE", misc["build_rate"], sim.BUILD_RATE)
    check("FAB_SPEED", misc["fabricator_speed"], sim.FAB_SPEED, 1e-6)
    check("PROD_GROWTH", misc["production_growth"], sim.PROD_GROWTH, 1e-6)
    for key, name in BUILDINGS.items():
        r = rust["buildings"][name]
        base, growth = sim.BUILD[key]
        for level in range(1, 21):
            cost = [x * growth ** (level - 1) for x in base]
            for i in range(3):
                check(f"{name} L{level} cost[{i}]", r["cost"][level - 1][i], cost[i], absolute=0.51)
            secs = cost_seconds(cost, sim.BUILD_RATE)
            check(f"{name} L{level} ticks", r["ticks"][level - 1], secs, rel=0.01, absolute=2)
            check(f"{name} L{level} ticks fab4", r["ticks_fab4"][level - 1], secs / (1 + sim.FAB_SPEED * 4), rel=0.01, absolute=2)
        want = sim.BUILD_PREREQ.get(key)
        got = r["prereq"]
        if (want or got) and (not want or not got or [[BUILDINGS[want[0][0]], want[0][1]]] != [got]):
            problems.append(f"{name} prerequisite: rust {got} vs sim {want}")
    for key, perhour in sim.PROD_BASE.items():
        name = BUILDINGS[{"metal": "metal", "crystal": "crystal", "deut": "deut"}[key]]
        for level in range(1, 21):
            check(f"{name} L{level} per hour", rust["buildings"][name]["per_hour"][level - 1],
                  perhour * level * sim.PROD_GROWTH ** level, rel=1e-4)


def check_research(sim, rust):
    misc = rust["misc"]
    check("RES_RATE", misc["research_rate"], sim.RES_RATE)
    check("LAB_SPEED", misc["lab_speed"], sim.LAB_SPEED, 1e-6)
    for key, name in RESEARCH.items():
        base, growth, max_level, prereqs = sim.RESEARCH[key]
        r = rust["research"][name]
        check(f"{name} max level", r["max"], max_level)
        for level in range(1, max_level + 1):
            cost = [x * growth ** (level - 1) for x in base]
            for i in range(3):
                check(f"{name} L{level} cost[{i}]", r["cost"][level - 1][i], cost[i], absolute=0.51)
            secs = cost_seconds(cost, sim.RES_RATE)
            check(f"{name} L{level} ticks", r["ticks"][level - 1], secs, rel=0.01, absolute=2)
            check(f"{name} L{level} ticks lab5", r["ticks_lab5"][level - 1], secs / (1 + sim.LAB_SPEED * 5), rel=0.01, absolute=2)
        want = sorted((kind, SIM_NAME[n], lv) for kind, n, lv in prereqs)
        got = sorted((k, n, lv) for k, n, lv in r["prereqs"])
        # the sim keeps the lab requirement of every tech in RESEARCH_LAB_OK, not in the list
        got = [g for g in got if not (g[0] == "b" and g[1] == "ResearchLab" and g not in want and g[2] == 1)]
        if want != got:
            problems.append(f"{name} prerequisites: rust {got} vs sim {want}")


def check_ships_and_defences(sim, rust):
    check("SHIP_RATE", rust["misc"]["ship_rate"], sim.SHIP_RATE)
    check("SY_SPEED", rust["misc"]["shipyard_speed"], sim.SY_SPEED, 1e-6)
    for key, name in SHIPS.items():
        cost, hull, shield, weapon, cargo, fuel = sim.SHIPS[key]
        r = rust["ships"][name]
        for i in range(3):
            check(f"{name} cost[{i}]", r["cost"][i], cost[i])
        for i, v in enumerate((hull, shield, weapon, cargo, fuel)):
            check(f"{name} stat[{i}]", r["stats"][i], v)
        secs = cost_seconds(cost, sim.SHIP_RATE)
        check(f"{name} ticks", r["ticks_sy0"], secs, rel=0.01, absolute=2)
        check(f"{name} ticks sy5", r["ticks_sy5"], secs / (1 + sim.SY_SPEED * 5), rel=0.01, absolute=2)
    for key, name in DEFENCES.items():
        cost, power, prereq = sim.DEFS[key]
        r = rust["defences"][name]
        for i in range(3):
            check(f"{name} cost[{i}]", r["cost"][i], cost[i])
        check(f"{name} power", r["power"], power, 1e-4)
        check(f"{name} ticks", r["ticks_sy0"], cost_seconds(cost, sim.SHIP_RATE), rel=0.01, absolute=2)
        want = sorted(("b" if a == "grid" else "r", SIM_NAME.get(a, "DefenseGrid"), lv) for a, lv in prereq.items())
        got = sorted((k, n, lv) for k, n, lv in r["prereqs"])
        if want != got:
            problems.append(f"{name} prerequisites: rust {got} vs sim {want}")
    for level, units in sim.GRID_VIRTUAL_UNITS.items():
        check(f"Defense Grid L{level} scout units", rust["grid_scout_units"][level], units, 1e-6)
    for level in range(4, 9):
        check(f"Defense Grid L{level} scout units", rust["grid_scout_units"][level], 4.0 * 1.3 ** (level - 3), 1e-6)


def check_storage_raids_misc(sim, rust):
    misc = rust["misc"]
    check("VAULT_GROWTH", misc["vault_growth"], sim.VAULT_GROWTH, 1e-6)
    check("CAP_PACE_EXP", misc["cap_pace_exponent"], sim.CAP_PACE_EXP, 1e-4)
    for pace, rows in rust["storage_caps"].items():
        for v, cap in enumerate(rows):
            scale = sim.VAULT_GROWTH ** v * float(pace) ** sim.CAP_PACE_EXP
            for i, k in enumerate(("metal", "crystal", "deut")):
                check(f"cap {k} vault {v} pace {pace}", cap[i], sim.VAULT_BASE_CAP[k] * scale, rel=1e-4)
    for s, power in rust["raid_power_base"].items():
        check(f"raid power at S={s}", power, sim.raid_power(float(s)), rel=1e-4)
    check("raid minimum player age", source_const("shared/src/constants.rs", "RAID_MIN_PLAYER_AGE"), sim.RAID_MIN_AGE_S)
    check("fuel price", misc["fuel_deut_per_unit"], sim.FUEL_PRICE_DEUT, 1e-6)
    check("fuel rate", misc["fuel_rate_per_mass"], sim.FUEL_RATE, 1e-6)
    check("fuel depot per level", misc["fuel_depot_per_level"], 0.25, 1e-6)
    check("queue depth", misc["queue_depth"], 3)
    for i, m in enumerate(sim.START_RES):
        check(f"starting resources[{i}]", rust["starting_resources"][i], m)
    check("resource weight of 1000 each", rust["resource_weight_unit"], sum(sim.W_RES), 1e-6)
    check("score extra cap", source_const("server/src/score.rs", "EXTRA_CAP"), sim.SCORE_EXTRA_CAP, 1e-6)
    for p in sim.PRESETS:
        if p.P not in rust["presets"].values():
            problems.append(f"sim preset {p.name} (P={p.P}) is not a Rust preset: {rust['presets']}")


def check_npc_gradient(sim, rust):
    for row in rust["npc"]:
        d = row["dist"]
        check(f"npc power d={d}", row["power"], sim.npc_power(d), 1e-4)
        check(f"threat d={d}", row["threat"], sim.threat(d))
        if row["class"].lower() != sim.npc_class(d):
            problems.append(f"npc class d={d}: rust {row['class']} vs sim {sim.npc_class(d)}")
        mult = min(1.3, 0.6 + 0.02 * d)
        want = min(32, max(1, round(sim.npc_power(d) / (sim.ship_power(sim.npc_class(d)) * mult))))
        check(f"npc count d={d}", row["count"], want)
        check(f"npc stat multiplier d={d}", row["mult"], mult, 1e-5)
        check(f"npc presence d={d}", row["presence"], sim.npc_presence(d), 1e-5)
        check(f"npc passive share d={d}", row["passive"], sim.npc_passive_frac(d), 1e-5)
        check(f"npc respawn d={d}", row["respawn_h"], sim.npc_respawn_h(d), 1e-5)


def check_ore(sim, rust):
    ore = rust["ore"]
    for key, got in zip("SMRP", ore["tile_totals"]):
        check(f"tile reserve {key}", got, sim.DENS_RESERVE[key], 1e-6)
    check("harvest yield per level", ore["harvest_yield_per_level"], 0.2, 1e-6)
    for row in ore["rings"]:
        d = row["dist"]
        check(f"ring multiplier d={d}", row["ring_mult"], sim.ORE_RING_MULT(d), 1e-4)
        check(f"ore regeneration hours d={d}", row["regen_h"], sim.ore_regen_h(d), 1e-5)
        mix = sim.density_mix(d)
        for key, got in zip("SMRP", row["odds"]):
            check(f"density odds {key} d={d}", got, mix[key], 1e-5)


def check_loot(sim, rust):
    loot = rust["loot"]
    check("loot split sum", sum(loot["split"]), 1.0, 1e-6)
    for got, want in zip(loot["split"], (0.55, 0.30, 0.15)):
        check("loot split", got, want, 1e-6)
    for row in loot["rings"]:
        d = row["dist"]
        t = sim.threat(d)
        check(f"wreck value d={d}", row["wreck"], sim.kill_loot_value(d), 1e-4)
        check(f"derelict value d={d}", row["derelict"], sim.derelict_value(d), 1e-4)
        check(f"derelict tier d={d}", row["tier"], sim.derelict_tier(d))
        check(f"derelict ambush d={d}", row["ambush"], sim.derelict_ambush(d), 1e-5)
        check(f"recovery chance T{t}", row["recovery"], min(0.30, max(0.0, 0.05 * (t - 2))), 1e-5)
        check(f"data core chance T{t}", row["data_core"], min(0.15, max(0.0, 0.03 * (t - 5))), 1e-5)
        check(f"relic chance T{t}", row["relic"], min(0.06, max(0.0, 0.02 * (t - 6))), 1e-5)
    check("derelict respawn ticks", source_const("shared/src/constants.rs", "DERELICT_RESPAWN_TICKS"), sim.DERELICT_RESPAWN_H * 3600.0)
    check("derelict respawn pace exponent", rust["misc"]["finds_exponent"], sim.DERELICT_PACE_EXP, 1e-6)


def main():
    sim = load_sim()
    rust = rust_numbers()
    check_buildings(sim, rust)
    check_research(sim, rust)
    check_ships_and_defences(sim, rust)
    check_storage_raids_misc(sim, rust)
    check_npc_gradient(sim, rust)
    check_ore(sim, rust)
    check_loot(sim, rust)
    if problems:
        print(f"{len(problems)} mismatch(es) between shared/ and docs/design/economy/sim.py:")
        for p in problems[:60]:
            print("  " + p)
        sys.exit(1)
    print("economy constants agree with sim.py "
          f"({len(BUILDINGS)} buildings, {len(RESEARCH)} techs, {len(SHIPS)} ships, {len(DEFENCES)} defences, storage, raids, presets, npc gradient, ore, loot)")


if __name__ == "__main__":
    main()
