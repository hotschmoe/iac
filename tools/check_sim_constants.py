#!/usr/bin/env python3
"""Hold the simulation's formulas to the game's.

docs/design/economy/sim.py declares none of the game's numbers: it reads them
from numbers.json beside it, which Rust renders, so there is no constant left
to compare. What the simulation still works out for itself (a cost at a level,
a build time behind a Robotics Factory, the energy of a homeworld and what its
mines then make, a cap at a pace, a jump's fuel, the power of a ring's group)
is compared here with what the Rust formulas give: the "tables" half of the
same file.

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


def rendered():
    run = subprocess.run(
        ["cargo", "test", "-q", "-p", "iac-sim", "--test", "economy_numbers"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if run.returncode != 0:
        sys.exit(run.stdout + run.stderr)
    return json.loads((ECONOMY / "numbers.json").read_text())


def load_sim():
    spec = importlib.util.spec_from_file_location("iac_sim", ECONOMY / "sim.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def player(sim, pace=1.0, satellites=0, research=None, **levels):
    """A simulated player at `pace` with nothing built but the given levels."""
    s = sim.Sim(sim.Preset("check", pace, 1.0, 1.0, 0, 0))
    s.b.clear()
    s.b.update(levels)
    s.r.update(research or {})
    s.ships["satellite"] = satellites
    return s


def check_queues(sim, tables):
    """Costs in whole units and times in whole ticks, as Rust rounds them."""
    plain = player(sim)
    robotics, nanite = player(sim, robotics=4), player(sim, robotics=10, nanite=1)
    lab, yard, yard_nanite = player(sim, lab=5), player(sim, shipyard=5), player(sim, shipyard=5, nanite=1)
    for row in tables["building_levels"]:
        key, level, what = sim.SHORT[row["building"]], row["level"], f"{row['building']} L{row['level']}"
        check_triple(f"{what} cost", row["cost"], plain.b_cost(key, level))
        check(f"{what} ticks", row["ticks"], plain.b_time(key, level))
        check(f"{what} ticks, Robotics Factory 4", row["ticks_robotics_4"], robotics.b_time(key, level))
        check(f"{what} ticks, Robotics Factory 10 and Nanite Factory 1", row["ticks_robotics_10_nanite_1"], nanite.b_time(key, level))
    for row in tables["research_levels"]:
        key, level, what = sim.SHORT[row["tech"]], row["level"], f"{row['tech']} L{row['level']}"
        check_triple(f"{what} cost", row["cost"], plain.r_cost(key, level))
        check(f"{what} ticks", row["ticks"], plain.r_time(key, level))
        check(f"{what} ticks, Research Lab 5", row["ticks_lab_5"], lab.r_time(key, level))
    for row in tables["shipyard"]:
        key = sim.SHORT[row["item"]]
        check(f"{row['item']} ticks", row["ticks"], plain.s_time(key))
        check(f"{row['item']} ticks, Shipyard 5", row["ticks_shipyard_5"], yard.s_time(key))
        check(f"{row['item']} ticks, Shipyard 5 and Nanite Factory 1", row["ticks_shipyard_5_nanite_1"], yard_nanite.s_time(key))


def check_production(sim, tables):
    """A homeworld's energy, the fraction its mines run at, what they then make and what the reactor burns."""
    for row in tables["production"]:
        metal, crystal, deut = row["mines"]
        pace = row["pace"]
        s = player(sim, pace, row["satellites"], {"energy": row["energy_technology"]}, metal=metal, crystal=crystal,
                   deut=deut, solar=row["solar_plant"], fusion=row["fusion_reactor"])
        what = f"home {row['mines']}, Solar Plant {row['solar_plant']}, Fusion Reactor {row['fusion_reactor']}, " \
               f"{row['satellites']} satellites, pace {pace}"
        made, needed = s.power()
        fraction = sim.power_fraction(made, needed)
        check(f"{what}: energy made", row["energy_made"], made)
        check(f"{what}: energy needed", row["energy_needed"], needed)
        check(f"{what}: fraction", row["fraction"], fraction, rel=1e-12)
        check_triple(f"{what}: mined an hour", row["mined_per_hour"], [x * pace for x in sim.mined_per_hour(s.b, fraction)], rel=1e-9)
        burn = sim.FUSION_BURN[row["fusion_reactor"]] * pace
        check(f"{what}: fusion burn an hour", row["fusion_burn_per_hour"], burn, rel=1e-9)
        check(f"{what}: deuterium an hour, net", row["mined_per_hour"][2] - row["fusion_burn_per_hour"], s.prod_h()[2] * pace, rel=1e-9, absolute=1e-9)


def check_storage_and_raids(sim, tables):
    for row in tables["storage_caps"]:
        check(f"cap at storage level {row['level']}, pace {row['pace']}", row["cap"], sim.storage_cap(row["level"], row["pace"]), rel=1e-9)
        stores = dict.fromkeys(sim.STORES, row["level"])
        check_triple(f"caps of a player at storage level {row['level']}, pace {row['pace']}", [row["cap"]] * 3,
                     player(sim, row["pace"], **stores).cap(), rel=1e-9)
    for row in tables["raid_power"]:
        check(f"raid power at {row['econ']} points", row["power"], sim.raid_power(row["econ"]), rel=1e-4)


def check_flight(sim, game, tables):
    for row in tables["flight"]:
        key = sim.SHORT[row["class"]]
        research = {"combustion": row["combustion_drive"], "impulse": row["impulse_drive"],
                    "hyperdrive": row["hyperspace_drive"], "energy": row["energy_technology"]}
        what = f"{row['class']} at drives {row['combustion_drive']}/{row['impulse_drive']}/{row['hyperspace_drive']}"
        check(f"{what}: fuel a jump", row["jump_fuel"], sim.jump_fuel(key, research), rel=1e-12)
        check(f"{what}: tank", row["tank"], sim.ship_tank(key, research, row["deuterium_tank"]), rel=1e-12)
        check(f"{what}: jump ticks at its speed", row["jump_ticks"], sim.jump_ticks(row["speed"]))
        if not any((row["combustion_drive"], row["impulse_drive"], row["hyperspace_drive"])):
            check(f"{what}: speed", row["speed"], sim.ship_drive(key, research)[0])
    for names, kind, units in ((sim.S_NAME, "ships", sim.SHIPS), (sim.D_NAME, "defences", sim.DEFS)):
        for key, rust in names.items():
            check(f"{rust} power", game[kind][rust]["power"], sim.unit_power(units[key]), rel=1e-6)


def check_rings(sim, tables):
    for row in tables["rings"]:
        d = row["ring"]
        cls, count, mult = sim.npc_group(d)
        check(f"npc power, ring {d}", row["npc_power"], sim.npc_power(d), rel=1e-4)
        check(f"threat, ring {d}", row["threat_rating"], sim.threat(d))
        check(f"npc count, ring {d}", row["count"], count)
        check(f"npc stat multiplier, ring {d}", row["stat_multiplier"], mult, rel=1e-5)
        check(f"npc group power, ring {d}", row["group_power"], count * mult * sim.ship_power(cls), rel=1e-4)
        check(f"npc group power under the ship limit, ring {d}", row["group_power"], sim.npc_group_power(d), rel=1e-4)
        check(f"outpost raid power, ring {d}", row["outpost_raid_power"], sim.outpost_raid_power(d), rel=1e-4)
        check(f"ring multiplier, ring {d}", row["ring_mult"], sim.ORE_RING_MULT(d), rel=1e-4)
        check(f"ore regeneration hours, ring {d}", row["ore_regen_hours"], sim.ore_regen_h(d), rel=1e-5)
        check(f"wreck value, ring {d}", row["wreck_value"], sim.kill_loot_value(d), rel=1e-4)
        check(f"derelict value, ring {d}", row["derelict_value"], sim.derelict_value(d), rel=1e-4)
        check(f"derelict tier, ring {d}", row["derelict_tier"], sim.derelict_tier(d))


def check_exchange(sim, tables):
    for row in tables["exchange_spans"]:
        pace = row["pace"]
        check(f"exchange window at pace {pace}", row["window"], sim.exchange_window(pace))
        check(f"exchange span of mines {row['mines']} at pace {pace}", row["span"], sim.exchange_span(row["mines"], pace), rel=1e-9)
    for row in tables["exchange_returns"]:
        check(f"exchange return {row['value']} at {row['pressure']} of {row['span']}", row["back"],
              sim.exchange_return(row["pressure"], row["value"], row["span"]), rel=1e-9, absolute=1e-9)


def check_outposts(sim, tables):
    for row in tables["outpost_levels"]:
        level, pace = row["level"], row["pace"]
        what = f"outpost L{level} at pace {pace}"
        check_triple(f"{what} store", row["store_cap"], sim.outpost_store_cap(level, pace), rel=1e-9)
        check(f"{what} platform", row["platform"], sim.outpost_platform(level), rel=1e-6)
        check_triple(f"{what} cost", row["cost"], sim.outpost_cost(level))
        check(f"{what} ticks", row["ticks"], sim.outpost_build_s(level, pace))
        check(f"{what} repair ticks", row["repair_ticks"], sim.outpost_repair_s(level, pace))
        check(f"{what} repair price", row["repair_price"], sim.outpost_repair_price(level), rel=1e-9)
        check(f"{what} points", row["points"], sim.outpost_points(level), rel=1e-4)
    for row in tables["outpost_yields"]:
        for density, per_hour in zip("SMRP", row["per_hour"]):
            check(f"outpost yield {density}, ring {row['ring']}, L{row['level']}, pace {row['pace']}", per_hour,
                  sim.outpost_yield_h(density, row["ring"], row["level"], row["pace"]), rel=1e-9)


def main():
    numbers = rendered()
    game, tables = numbers["game"], numbers["tables"]
    sim = load_sim()
    check_queues(sim, tables)
    check_production(sim, tables)
    check_storage_and_raids(sim, tables)
    check_flight(sim, game, tables)
    check_rings(sim, tables)
    check_exchange(sim, tables)
    check_outposts(sim, tables)
    if problems:
        print(f"{len(problems)} of {compared} formula values in docs/design/economy/sim.py disagree with shared/:")
        for p in problems[:60]:
            print("  " + p)
        sys.exit(1)
    print(f"sim.py's formulas agree with the game: {compared} values "
          "(costs and times by level behind the Robotics Factory, the Nanite Factory, the Research Lab and the Shipyard, "
          "energy and production, storage caps, raid power, a jump's fuel, tank and ticks, unit power, the ring gradient, "
          "exchange, outposts)")


if __name__ == "__main__":
    main()
