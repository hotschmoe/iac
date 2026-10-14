#!/usr/bin/env python3
"""
IAC economy / pacing / exploration simulation (Python stdlib only).

Simulates one scripted "competent player" per pacing preset on the game's catalogue,
which is classic OGame's (docs/design/ogame.md): its buildings, research, ships and
defences, with energy.  The game's numbers are not written here: they are read from
numbers.json beside this file, which the Rust side renders
(`UPDATE_FIXTURES=1 cargo test -p iac-sim --test economy_numbers`) and
`cargo test` refuses when it is stale.  To change a number of the game, change it
in Rust and render the file again; to try one first, use --set.

    python3 sim.py                    # full markdown report (under a minute)
    python3 sim.py --preset season    # one preset
    python3 sim.py --pace 25          # any pace (custom preset)
    python3 sim.py --tables           # formula tables, and the presets' two estimates in seconds
    python3 sim.py --sensitivity      # how session frequency changes the arcs
    python3 sim.py --longrun          # persistent world for a year
    python3 sim.py --modes none,near,far --brief
    python3 sim.py --set BETA=0.4     # override any module constant for experiments

Player modes: none = buildings only, near = competent (engage at 3x power, mine the
inner ring, hunt and board up to ring 20), far = greedy (engage at 1.5x, mine the
farthest ring whose net value is within 80% of the best).  The competent player's
milestone times are the median of three session-schedule offsets.  Piloting is
modelled as expected values (ore refill supply, respawn supply, active-time caps,
attrition, refuel cost), not as individual fights.  A raid on the homeworld is not
fought here either: the game fights it as a battle of at most RAID_ROUNDS rounds, and
this player only buys defence until its power is over the strongest raid's, a rule of
thumb of its own that no rule of the game reads.
Not modelled: the hub exchange as a way to turn one resource into another, outposts,
refining fuel from a hold, what a drive's research adds to speed, what a raid's fight
destroys and what a lost raid takes.  The planner is heuristic: expect about +-15%
run-to-run noise.
"""
import math, sys, argparse, json, pathlib, functools
from collections import defaultdict

# =====================================================================
# 1. THE GAME'S NUMBERS (pace-1 baseline: the "persistent world")
#    Times below are GAME hours at pace 1.  Real time = game time / P.
#
#    A name bound to GAME or to G below follows the Rust source by itself.
#    A number written out in this file is the simulation's own: a planner
#    heuristic, a session schedule, an assumption about players.  The one
#    copy of the game kept by hand is zone_terrain and ore_prob, whose
#    figures Rust keeps inside worldgen.
# =====================================================================
GAME = json.loads(pathlib.Path(__file__).with_name('numbers.json').read_text())['game']
G = GAME['constants']

# the simulation's short names for what the game calls by its Rust name
B_NAME = {'metal': 'MetalMine', 'crystal': 'CrystalMine', 'deut': 'DeuteriumSynthesizer', 'solar': 'SolarPlant',
          'fusion': 'FusionReactor', 'robotics': 'RoboticsFactory', 'nanite': 'NaniteFactory', 'shipyard': 'Shipyard',
          'metal_store': 'MetalStorage', 'crystal_store': 'CrystalStorage', 'deut_tank': 'DeuteriumTank',
          'lab': 'ResearchLab'}
R_NAME = {'espionage': 'EspionageTechnology', 'computer': 'ComputerTechnology', 'weapons': 'WeaponsTechnology',
          'shielding': 'ShieldingTechnology', 'armour': 'ArmourTechnology', 'energy': 'EnergyTechnology',
          'hyperspace': 'HyperspaceTechnology', 'combustion': 'CombustionDrive', 'impulse': 'ImpulseDrive',
          'hyperdrive': 'HyperspaceDrive', 'laser': 'LaserTechnology', 'ion': 'IonTechnology',
          'plasma': 'PlasmaTechnology', 'graviton': 'GravitonTechnology'}
S_NAME = {'small_cargo': 'SmallCargo', 'large_cargo': 'LargeCargo', 'light_fighter': 'LightFighter',
          'heavy_fighter': 'HeavyFighter', 'cruiser': 'Cruiser', 'battleship': 'Battleship', 'recycler': 'Recycler',
          'probe': 'EspionageProbe', 'bomber': 'Bomber', 'satellite': 'SolarSatellite', 'destroyer': 'Destroyer',
          'deathstar': 'Deathstar', 'battlecruiser': 'Battlecruiser'}
D_NAME = {'rocket': 'RocketLauncher', 'light_laser': 'LightLaser', 'heavy_laser': 'HeavyLaser',
          'gauss': 'GaussCannon', 'ion_cannon': 'IonCannon', 'plasma_turret': 'PlasmaTurret',
          'small_dome': 'SmallShieldDome', 'large_dome': 'LargeShieldDome'}
KINDS = ((B_NAME, 'buildings'), (R_NAME, 'research'), (S_NAME, 'ships'), (D_NAME, 'defences'))
for names, kind in KINDS:
    if set(names.values()) != set(GAME[kind]):
        sys.exit(f'sim.py names {sorted(names.values())} as {kind}; numbers.json has {sorted(GAME[kind])}')
SHORT = {rust: short for names, _ in KINDS for short, rust in names.items()}
LABEL = {short: GAME[kind][rust]['label'] for names, kind in KINDS for short, rust in names.items()}
def requires(entry):         # [('b' or 'r', short name, level)]: buildings and research needed first
    return [(kind[0], SHORT[name], level) for kind, name, level in entry['requires']]
LEVEL = GAME['by_level']                                 # what a level gives, from level 0
RING = {row['ring']: row for row in GAME['rings']}       # the curves read at whole rings, 1 to 60
RATING = {row['rating']: row for row in GAME['ratings']} # ... and at threat ratings, T1 to T9
PACE = GAME['presets']
ATTENTION_EXP = GAME['pace_exponent']['Attention']       # caps, raid cadence and the exchange window: P ** 0.75
FINDS_EXP = GAME['pace_exponent']['Finds']               # loot, holds and derelict respawn: P ** 0.5
INNER_RING, OUTER_RING = GAME['zone_radius']['InnerRing'], GAME['zone_radius']['OuterRing']
MAX_LEVEL = G['MAX_BUILDING_LEVEL']
W_RES = tuple(GAME['resource_weight'][k] for k in ('metal', 'crystal', 'deuterium'))

def wv(c):                   # score points of a cost: its weight in thousands
    return (c[0] * W_RES[0] + c[1] * W_RES[1] + c[2] * W_RES[2]) / 1000.0

@functools.lru_cache(maxsize=None)
def grown_cost(base, growth, level):
    """base * growth^(level - 1), in whole units as Rust rounds them (ties to even)."""
    factor = growth ** (max(level, 1) - 1)
    return tuple(float(round(x * factor)) for x in base)

# (L1 cost m,c,d), cost growth per level
BUILD = {k: (tuple(GAME['buildings'][n]['cost']), GAME['buildings'][n]['growth']) for k, n in B_NAME.items()}
BUILD_PREREQ = {k: requires(GAME['buildings'][n]) for k, n in B_NAME.items()}
START_LEVELS = {k: GAME['buildings'][n]['starts_at'] for k, n in B_NAME.items() if GAME['buildings'][n]['starts_at']}
BUILD_RATE = G['BUILD_RATE']       # (metal+crystal) of build cost per game-hour, before 1 + Robotics Factory
RES_RATE = G['RESEARCH_RATE']      # research: (m+c) per game-hour, before 1 + Research Lab
SHIP_RATE = G['SHIP_RATE']         # ships and defences: (m+c) per game-hour per unit, before 1 + Shipyard
QUEUE_DEPTH = G['QUEUE_WAITING']   # orders that may wait behind the running ones in each queue
def cost_seconds(c, rate, speed):  # deuterium in a cost adds no time
    return (c[0] + c[1]) / rate * 3600.0 / speed
def econ_ticks(seconds, P):        # an economy timer at pace P, in whole ticks
    return max(1, math.ceil(seconds / P))

# L1 cost, growth, max, prereqs (b=building, r=research)
RESEARCH = {k: (tuple(t['cost']), t['growth'], t['max_level'], requires(t))
            for k, t in ((k, GAME['research'][n]) for k, n in R_NAME.items())}

# a ship or a defence as the game gives it, with its cost and what it requires in the simulation's names
SHIPS = {k: dict(GAME['ships'][n], cost=tuple(GAME['ships'][n]['cost']), requires=requires(GAME['ships'][n]))
         for k, n in S_NAME.items()}
DEFS = {k: dict(GAME['defences'][n], cost=tuple(GAME['defences'][n]['cost']), requires=requires(GAME['defences'][n]))
        for k, n in D_NAME.items()}
ITEMS = {**SHIPS, **DEFS}          # what the shipyard builds
START_SHIPS = {SHORT[name]: count for name, count in GAME['starting_ships']}
START_RES = tuple(G['STARTING_RESOURCES'])
BUILDING_SLOTS = LEVEL['building_slots']   # by Computer Technology level
WEAPON_MOD, HULL_MOD, SHIELD_MOD = LEVEL['weapon_modifier'], LEVEL['hull_modifier'], LEVEL['shield_modifier']
def unit_power(u, wm=1.0, hm=1.0, sm=1.0):   # weapon plus a tenth of hull and shield, each with its research
    return u['weapon'] * wm + (u['hull'] * hm + u['shield'] * sm) / 10.0
def ship_power(c):
    return unit_power(SHIPS[c])

# energy and production: the mines run at min(1, made / needed); natural production needs no mine
MINES = ('metal', 'crystal', 'deut')
MINE_OUTPUT = {k: LEVEL['mine_output_per_hour'][B_NAME[k]] for k in MINES}   # per game-hour at full power
MINE_ENERGY = {k: LEVEL['energy_needed'][B_NAME[k]] for k in MINES}
NATURAL = (G['NATURAL_PER_HOUR_METAL'], G['NATURAL_PER_HOUR_CRYSTAL'], 0.0)
SOLAR_ENERGY = LEVEL['solar_plant_energy']
FUSION_ENERGY = LEVEL['fusion_energy_by_energy_technology']   # [Energy Technology][Fusion Reactor]
FUSION_BURN = LEVEL['fusion_burn_per_hour']                   # deuterium per game-hour
SATELLITE_ENERGY = GAME['satellite_energy']
def energy_needed(b):
    return sum(MINE_ENERGY[k][b.get(k, 0)] for k in MINES)
def energy_made(b, r, satellites, fusion_fed=True):   # the reactor runs only while there is deuterium to burn
    fusion = FUSION_ENERGY[r.get('energy', 0)][b.get('fusion', 0)] if fusion_fed else 0
    return SOLAR_ENERGY[b.get('solar', 0)] + satellites * SATELLITE_ENERGY + fusion
def power_fraction(made, needed):
    return 1.0 if needed == 0 else min(1.0, made / needed)
def mined_per_hour(b, fraction):   # per game-hour, natural production included, the reactor's burn not taken off
    return [MINE_OUTPUT[k][b.get(k, 0)] * fraction + NATURAL[i] for i, k in enumerate(MINES)]

# storage: one building per resource
STORES = tuple(SHORT[GAME['storage_building'][k]] for k in ('metal', 'crystal', 'deuterium'))
STORAGE_CAP = LEVEL['storage_cap']             # by the level of the resource's own building, at pace 1
CAP_PACE_EXP = ATTENTION_EXP                   # cap multiplier = P ** this up to CAP_KNEE ...
CAP_KNEE = G['CAP_KNEE']                       # ... and 10**0.75 * (P / 10) ** CAP_EXP_HIGH above it
CAP_EXP_HIGH = G['CAP_EXPONENT_PAST_KNEE']
def cap_mult(P):
    if P <= CAP_KNEE:
        return P ** CAP_PACE_EXP
    return CAP_KNEE ** CAP_PACE_EXP * (P / CAP_KNEE) ** CAP_EXP_HIGH
def storage_cap(level, P=1.0):
    return STORAGE_CAP[level] * cap_mult(P)

# pilot-economy scale: ore/loot/salvage values and holds scale with P ** BETA
BETA = FINDS_EXP

# NPC / threat
NPC_POWER_BASE, NPC_POWER_GROWTH = G['NPC_POWER_BASE'], G['NPC_POWER_GROWTH']
MAX_THREAT = G['MAX_THREAT']
def npc_power(d):            # smooth: ~1.22x per ring step
    return NPC_POWER_BASE * NPC_POWER_GROWTH ** (d - 1)
def threat(d):
    return max(1, min(MAX_THREAT, 1 + int(math.floor(math.log2(npc_power(d) / NPC_POWER_BASE)))))
def npc_class(d):            # Light Fighters through the inner ring (d <= 8)
    return SHORT[RING[d]['npc_class']]
NPC_MAX_SHIPS, NPC_MULT_MAX = G['NPC_MAX_SHIPS'], G['NPC_STAT_MULT_MAX']
def npc_base_mult(d):        # the stat multiplier a group is sized around
    return min(NPC_MULT_MAX, RING[d]['npc_base_multiplier'])
def npc_group(d, power=None):
    # (class, count, stat multiplier): the count from the base multiplier (more if the ships
    # would need over NPC_MULT_MAX), the multiplier fitted so the ships add up to the power
    c = npc_class(d)
    p = npc_power(d) if power is None else power
    per = ship_power(c)
    sized = math.floor(p / (per * npc_base_mult(d)) + 0.5)
    count = max(1, min(NPC_MAX_SHIPS, max(sized, math.ceil(p / (per * NPC_MULT_MAX)))))
    return c, count, min(NPC_MULT_MAX, p / (per * count))
def npc_group_power(d):      # npc_power until the ship limit holds it down
    return min(npc_power(d), ship_power(npc_class(d)) * NPC_MULT_MAX * NPC_MAX_SHIPS)
def npc_presence(d):
    return RING[d]['npc_presence_pct'] / 100.0
def npc_passive_frac(d):
    return RING[d]['npc_passive_share']
def npc_respawn_h(d):        # game hours
    return RING[d]['npc_respawn_hours']
KILL_LOOT_K, KILL_LOOT_EXP = G['WRECK_VALUE_K'], G['WRECK_VALUE_EXPONENT']
def kill_loot_value(d):      # total resource value of one kill's wreck (units)
    return KILL_LOOT_K * npc_power(d) ** KILL_LOOT_EXP

# ore
DENSITY = {'S': GAME['densities']['Sparse'], 'M': GAME['densities']['Moderate'],
           'R': GAME['densities']['Rich'], 'P': GAME['densities']['Pristine']}
DENS_RESERVE = {k: v['reserve_units'] for k, v in DENSITY.items()}   # total reserve per density
DENS_YIELD = {k: v['harvest_multiplier'] for k, v in DENSITY.items()}   # raw units a tick per point of harvest power
ORE_RING_GROWTH, ORE_RING_FIRST, ORE_RING_LAST = G['ORE_RING_GROWTH'], G['ORE_RING_FIRST'], G['ORE_RING_LAST']
ORE_RING_MULT = lambda d: ORE_RING_GROWTH ** (min(d, ORE_RING_LAST) - ORE_RING_FIRST) if d >= ORE_RING_FIRST else 1.0
ORE_REGEN_BASE_H, ORE_REGEN_PER_RING_H = G['ORE_REGEN_BASE_HOURS'], G['ORE_REGEN_HOURS_PER_RING']
def ore_regen_h(d):          # game hours for a stripped tile to refill completely
    return ORE_REGEN_BASE_H + ORE_REGEN_PER_RING_H * d
def ring_coverage(d):         # share of a ring's sectors that are connected and inside a fleet's reach
    return 0.5 if d <= INNER_RING else 0.4 if d <= OUTER_RING else 0.25
def density_mix(d):
    return dict(zip('SMRP', RING[d]['ore_density_odds']))
def mean_reserve(d):
    mix = density_mix(d)
    return sum(mix[k] * DENS_RESERVE[k] for k in mix)
def mean_harvest_mult(d):
    mix = density_mix(d)
    return sum(mix[k] * DENS_YIELD[k] for k in mix)
def zone_terrain(d):         # kept by hand: worldgen rolls these inside shared/src/world.rs (roll_terrain, roll_resources)
    if d <= INNER_RING:  return dict(ast=.55, neb=.15, deb=.10, emp=.20, ano=0.0), 30
    if d <= OUTER_RING:  return dict(ast=.35, neb=.15, deb=.20, emp=.25, ano=.05), 40
    return dict(ast=.30, neb=.15, deb=.20, emp=.20, ano=.15), 50
def ore_prob(d):             # P(ore) per resource at a random sector
    t, boost = zone_terrain(d)
    pm = t['ast'] * (60 + boost) / 100 + (t['neb'] + t['deb'] + t['ano']) * 0.20
    pc = (t['ast'] + t['neb']) * (40 + boost) / 100 + (t['deb'] + t['ano']) * 0.10
    pd = t['neb'] * (50 + boost) / 100 + (t['ast'] + t['deb'] + t['ano']) * 0.15
    return {'metal': min(pm, 1), 'crystal': min(pc, 1), 'deut': min(pd, 1)}
def tile_reserve(d):         # expected reserve units per sector per resource, incl. ring multiplier
    pr = ore_prob(d)
    r = mean_reserve(d) * ORE_RING_MULT(d)
    return {k: pr[k] * r for k in pr}
def ore_tile(d):             # raw units of all resources a sector with ore holds when full; the three rolls taken as independent
    pr = ore_prob(d)
    any_ore = 1.0 - (1 - pr['metal']) * (1 - pr['crystal']) * (1 - pr['deut'])
    return sum(tile_reserve(d).values()) / any_ore

# derelicts
DERELICT_PCT = G['DERELICT_SPAWN_PCT'] / 100.0
DERELICT_RESPAWN_H = G['DERELICT_RESPAWN_TICKS'] / 3600.0
DERELICT_PACE_EXP = FINDS_EXP   # derelict respawn scales with P**0.5 (finds are not an economy timer)
DERELICT_VALUE_BASE, DERELICT_VALUE_GROWTH, DERELICT_VALUE_RING = G['DERELICT_VALUE_BASE'], G['DERELICT_VALUE_GROWTH'], G['DERELICT_VALUE_RING']
def derelict_value(d):       # total loot value of one site (metal+crystal+deut units)
    return DERELICT_VALUE_BASE * DERELICT_VALUE_GROWTH ** (d - DERELICT_VALUE_RING)
def derelict_tier(d):        # tier follows threat: T1-3 -> 1, T4-6 -> 2, T7-9 -> 3
    return RATING[threat(d)]['threat_band']
def derelict_ambush(d):
    return RATING[threat(d)]['derelict_ambush']
AMBUSH_GUARD_SHARE = G['AMBUSH_GUARD_SHARE']   # share of the ring's group power a derelict's guard fields
def derelict_sites_per_ring(d):
    t, _ = zone_terrain(d)
    return 6 * d * (t['deb'] + t['emp']) * DERELICT_PCT

# flight: a jump burns each ship's fuel figure, a tank holds tank_jumps of it
FUEL_PRICE_DEUT = G['FUEL_DEUT_PER_UNIT']   # deuterium per fuel unit refuelled
HOME_DIST = (G['HOMEWORLD_MIN_DIST'] + G['HOMEWORLD_MAX_DIST']) // 2   # the middle ring of the home seats
FUEL_RATE_MOD = LEVEL['fuel_rate_modifier']   # by Energy Technology level
TANK_MOD = LEVEL['tank_modifier']             # by Deuterium Tank level
HARVEST_YIELD = LEVEL['harvest_yield']        # by Laser Technology level
JUMP_SPEED_REF = G['JUMP_SPEED_REF']
def ship_drive(c, r):
    """(base speed, fuel one jump burns) of a ship with research r: the Small Cargo and the Bomber change drive."""
    s = SHIPS[c]
    if c == 'small_cargo' and r.get('impulse', 0) >= G['SMALL_CARGO_IMPULSE_LEVEL']:
        return G['SMALL_CARGO_IMPULSE_SPEED'], G['SMALL_CARGO_IMPULSE_FUEL']
    if c == 'bomber' and r.get('hyperdrive', 0) >= G['BOMBER_HYPERSPACE_LEVEL']:
        return G['BOMBER_HYPERSPACE_SPEED'], s['fuel']
    return s['speed'], s['fuel']
def jump_ticks(speed):
    return max(1, math.ceil(JUMP_SPEED_REF / speed))
def jump_fuel(c, r):
    return ship_drive(c, r)[1] * FUEL_RATE_MOD[r.get('energy', 0)]
def ship_tank(c, r, deut_tank=0):
    return SHIPS[c]['tank_jumps'] * ship_drive(c, r)[1] * TANK_MOD[deut_tank]

# combat expectation
def attrition(r):            # expected ship-loss value per unit of loot taken, at power ratio r = mine/theirs
    return min(10.0, 0.1 * (3.0 / max(r, 0.3)) ** 3)

# raids / defence
RAID_MIN_AGE_S = G['RAID_MIN_PLAYER_AGE']   # 24 h at pace 1; scaled by P**0.75
RAID_MIN_SHIPYARD = G['RAID_MIN_SHIPYARD']  # an empire is raided from this Shipyard level, or once it has any defence
RAID_POWER_FLOOR, RAID_POWER_PER_POINT, RAID_POWER_EXPONENT = G['RAID_POWER_FLOOR'], G['RAID_POWER_PER_POINT'], G['RAID_POWER_EXPONENT']
RAID_POWER_ROLL_MIN, RAID_POWER_ROLL_MAX = G['RAID_POWER_ROLL_MIN'], G['RAID_POWER_ROLL_MAX']   # a raid is rolled between these multiples of raid_power
def raid_power(econ_pts):
    return RAID_POWER_FLOOR + RAID_POWER_PER_POINT * econ_pts ** RAID_POWER_EXPONENT

# score: what the empire has built and keeps; kills, charts and derelicts pay in resources
DEFENCE_SHARE = G['DEFENCE_SHARE']           # a defence structure scores this share of its cost
RELIC_LOOT_MULT = G['RELIC_LOOT_MULT']       # a site with a relic aboard pays this many times its loot
def relic_chance(t):
    return RATING[t]['relic_chance']
def chart_value(t):                          # units of loot a delivered chart pays, at any pace
    return RATING[t]['chart_value']
FARM_DECAY = G['FARM_DECAY']   # each recent kill in a sector multiplies the next kill's wreckage by this
FARM_HALF_LIFE_RESPAWNS = G['FARM_HALF_LIFE_RESPAWNS']
def farm_steady(u):
    """Mean wreckage share of a sector worked at utilisation u (kills taken / respawns offered)."""
    if u <= 0: return 1.0
    d = 0.5 ** (1.0 / (FARM_HALF_LIFE_RESPAWNS * u))
    h = d / (1.0 - d) if d < 1 else 1e9
    return FARM_DECAY ** h
LOOT_SPLIT = tuple(G['LOOT_SPLIT'])   # metal, crystal and deuterium shares of any loot value
LOOT_WEIGHT = LOOT_SPLIT[0] * W_RES[0] + LOOT_SPLIT[1] * W_RES[1] + LOOT_SPLIT[2] * W_RES[2]   # weight per unit of loot

# hub exchange (provisional: the owner has not signed off on these four numbers)
EXCHANGE_FEE_MIN = G['EXCHANGE_FEE_MIN']           # fee on a unit of weight sold with an empty meter, as a share of par (W_RES)
EXCHANGE_FEE_MAX = G['EXCHANGE_FEE_MAX']           # fee once the meter reads one span or more
EXCHANGE_METER_SHARE = G['EXCHANGE_METER_SHARE']   # span = this share of the player's mine output over one window, in weight
EXCHANGE_WINDOW_S = G['EXCHANGE_WINDOW_TICKS']     # window at pace 1; scaled by P**0.75 (attention class); the meter forgets a span per window
def exchange_window(P):      # whole ticks
    return max(1, math.ceil(EXCHANGE_WINDOW_S / P ** ATTENTION_EXP))
def exchange_span(mine_levels, P):
    """Span of the meter for mines at (metal, crystal, deut) levels running at full power, in weight units."""
    per_hour = sum(w * x for w, x in zip(W_RES, mined_per_hour(dict(zip(MINES, mine_levels)), 1.0)))
    return EXCHANGE_METER_SHARE * per_hour * P / 3600.0 * exchange_window(P)
def exchange_return(pressure, value, span):
    """Weight handed back for `value` weight sold with the meter at `pressure`: the fee integrated over the trade."""
    def kept(x):
        ramp = min(x, span)
        on_ramp = (1.0 - EXCHANGE_FEE_MIN) * ramp - (EXCHANGE_FEE_MAX - EXCHANGE_FEE_MIN) * ramp * ramp / (2.0 * span) if span > 0 else 0.0
        return on_ramp + (1.0 - EXCHANGE_FEE_MAX) * (x - ramp)
    return kept(pressure + value) - kept(pressure)

# outposts (provisional: the owner has not signed off on these numbers; docs/design/outposts.md)
OUTPOST_MIN_RING, OUTPOST_MAX_RING = G['OUTPOST_MIN_RING'], G['OUTPOST_MAX_RING']
OUTPOST_SPACING = G['OUTPOST_SPACING']       # fewest hexes between two outposts
OUTPOST_MAX_LEVEL = G['OUTPOST_MAX_LEVEL']
OUTPOST_COST = tuple(G['OUTPOST_COST'])      # level 1; costs never scale with pace
OUTPOST_GROWTH = G['OUTPOST_GROWTH']         # cost and store per level
OUTPOST_BUILD_RATE = G['OUTPOST_BUILD_RATE'] # (metal+crystal) of a level's cost per game-hour of build time
OUTPOST_STORE_BASE = tuple(G['OUTPOST_STORE_BASE'])   # level 1 at pace 1; x P**0.5, like the holds that fill it
OUTPOST_YIELD_PER_HOUR = G['OUTPOST_YIELD_PER_HOUR']  # per grade point, per level, x ring multiplier, x P
OUTPOST_PLATFORM = LEVEL['outpost_platform']      # the defences the platform stands for, by level
OUTPOST_RAID_FACTOR = G['OUTPOST_RAID_FACTOR']    # a raid is this many times the ring's group, x the home raid's 0.80..1.15 roll
OUTPOST_RAID_LOSS = G['OUTPOST_RAID_LOSS']        # share of every resource above the floor that a lost raid takes
OUTPOST_REPAIR_SHARE = G['OUTPOST_REPAIR_SHARE']  # a repair costs the par value of this share of the level's cost, and this share of its build time
def outpost_cost(level):
    return grown_cost(OUTPOST_COST, OUTPOST_GROWTH, level)
def outpost_build_s(level, P=1.0):
    return econ_ticks(cost_seconds(outpost_cost(level), OUTPOST_BUILD_RATE, 1.0), P)
def outpost_repair_s(level, P=1.0):
    return econ_ticks(cost_seconds(outpost_cost(level), OUTPOST_BUILD_RATE, 1.0) * OUTPOST_REPAIR_SHARE, P)
def outpost_store_cap(level, P=1.0):
    return tuple(x * OUTPOST_GROWTH ** (max(level, 1) - 1) * P ** FINDS_EXP for x in OUTPOST_STORE_BASE)
def outpost_platform(level):
    return sum(count * unit_power(DEFS[SHORT[kind]]) for kind, count in OUTPOST_PLATFORM[level])
def outpost_yield_h(density, d, level, P=1.0):
    """Units a game hour of one resource whose ore is `density` ('S', 'M', 'R', 'P'), at ring d."""
    return OUTPOST_YIELD_PER_HOUR * DENS_YIELD[density] * ORE_RING_MULT(d) * level * P
def outpost_raid_power(d):   # before the 0.80..1.15 roll
    return OUTPOST_RAID_FACTOR * npc_group_power(d)
def outpost_repair_price(level):   # in metal; crystal and deuterium pay it at W_RES when the metal runs out
    return sum(x * w * OUTPOST_REPAIR_SHARE for x, w in zip(outpost_cost(level), W_RES)) if level else 0.0
def outpost_points(level):
    return sum(wv(outpost_cost(l)) for l in range(1, level + 1))

# =====================================================================
# 2. THE PLAYER: everything below this line to section 3 is the simulation's own
# =====================================================================
PILOT_SKILL = 0.5     # share of the ideal piloting tempo a skilled human or agent reaches (assumption; the live test checks it)
ATTENTION_PENALTY = 0.20   # building income lost by a player who spends all his attention in the field (scaled by his pilot share)
PILOT_SHARE = {'none': 0.0, 'near': 0.4, 'explore': 0.85, 'raid': 0.85, 'far': 0.85}   # share of attention spent piloting
CHART_RATE_PER_FLEET_H = 60.0   # new sectors a scouting fleet charts and delivers per active hour (assumption; live test checks it)
MINE_TURNAROUND_S = 20.0        # seconds a mining sortie spends docking and under orders, beside its jumps and its harvest
SECONDS_PER_KILL, SECONDS_PER_BOARDING = 60.0, 48.0   # the tempo of a hunt and of a round of derelicts while the player is at the helm
KILLS_PER_HUNT, SITES_PER_ROUND = 4, 2    # what one flight out and back takes in, with a jump between each
FUEL_INCOME_SHARE = 0.5                   # the field may burn this share of deuterium income, and none of the stock
def worth_flying(loot, deuterium):        # a flight is made when what it brings outweighs the deuterium it burns, at the score's weights
    return wv(loot) >= wv((0.0, 0.0, deuterium))

# the build script: (building, its level as a ratio of the Metal Mine's, the least it is built to, the Metal Mine level
# it is first built at).  Power comes with the mines (Sim.powered), storage when a cap is in the way.
SCRIPT = [('crystal', 1.0, 1, 2), ('deut', 0.78, 1, 3), ('robotics', 0.45, 2, 5), ('lab', 0.42, 1, 5),
          ('shipyard', 0.42, 2, 6), ('deut_tank', 0.25, 1, 8)]
SCARCE, SCARCE_AHEAD = 0.35, 2            # when a mine is raised out of turn, and how far past the script (Sim.scarce_mine)
STORE_FILL, STORE_HEADROOM = 0.75, 0.85   # raise a store when it is this full, or when the next order is this share of its cap
FUSION_PAYBACK_H = 24.0 * 30   # a Fusion Reactor level is priced with this many game-hours of the deuterium it burns

# the research order: what each step is for, and the levels it asks; what a level requires is walked from the game's tree
RESEARCH_PLAN = [
    ('Light Fighter, then Small Cargo', [('combustion', 2)]),
    ('a second building slot', [('computer', 3)]),
    ('ore yield, and the way to the Cruiser', [('laser', 5)]),
    ('Heavy Fighter', [('armour', 2), ('impulse', 2)]),
    ('Cruiser', [('ion', 2), ('impulse', 4)]),
    ('sight, the fight and fuel', [('espionage', 2), ('weapons', 3), ('armour', 3), ('energy', 5), ('shielding', 3)]),
    ('a third building slot', [('computer', 6)]),
    ('Battleship', [('espionage', 4), ('shielding', 5), ('hyperspace', 3), ('hyperdrive', 4)]),
    ('the endgame kit', [('weapons', 6), ('armour', 6), ('shielding', 6), ('energy', 6)]),
    ('the long run', [(tech, level) for level in range(7, 13) for tech in ('weapons', 'armour', 'shielding')]
                     + [('energy', 10), ('hyperspace', 5)]),
]
RESEARCH_GOALS = [goal for _, goals in RESEARCH_PLAN for goal in goals]

# the fleet plan: which classes do which job, and how many of each the player wants (Sim.fleet_wants)
MINERS = ('small_cargo', 'large_cargo', 'recycler')
WARSHIPS = ('light_fighter', 'heavy_fighter', 'cruiser', 'battleship', 'battlecruiser', 'bomber', 'destroyer')
LINE = WARSHIPS[:4]                   # the warships the plan orders
FIRST_MINERS, MOST_MINERS = 2, 3      # Small Cargo: ordered first, and in all; none for a player who never flies
MOST_LIGHT_FIGHTERS, MOST_HEAVY_FIGHTERS, MOST_CRUISERS = 6, 4, 8
CRUISERS_FROM_METAL, CRUISERS_PER_LEVEL = 9, 1.5       # Cruisers wanted: 1.5 for each Metal Mine level above 9
BATTLESHIPS_FROM_METAL, BATTLESHIPS_PER_LEVEL = 12, 2  # Battleships wanted: 2 for each Metal Mine level above 12 ...
BATTLESHIPS_PER_SHIPYARD = 4                           # ... and 4 for each Shipyard level above the kit's
SHIP_BATCH = 2            # cheap hulls are ordered this many at a time
ENERGY_BATCH = 20         # Solar Satellites in one order
DEFENCE_BATCH = 20        # defences in one order

# the "endgame kit" used as the end of the arc: the analogue under OGame's catalogue of the old kit
# (mines 19/16/13, Shipyard 11, Lab 11, mid research and 12 Cruisers, when the Cruiser was the heaviest ship)
KIT_B = dict(metal=19, crystal=17, deut=15, robotics=8, shipyard=8, lab=8)
KIT_R = dict(computer=6, weapons=6, shielding=6, armour=6, energy=6, hyperdrive=4)
KIT_SHIPS = dict(battleship=12)
DAILY_EVERY_S = 86400.0
OWN_DAYS = (3, 7)         # the days of a season at which the report lists what a player owns

# =====================================================================
# 3. PACING PRESETS
# =====================================================================
class Preset:
    def __init__(self, name, P, dt, horizon_h, sessions, session_min, continuous=False):
        self.name, self.P, self.dt, self.horizon_h = name, P, dt, horizon_h
        self.sessions, self.session_min = sessions, session_min
        self.continuous = continuous

# how each of the game's presets is played here: step (s), horizon (real h), sessions a day, minutes a session.
# A world with no sessions is attended without a break: it is over within a day.
SCHEDULE = {
    'persistent': (300.0, 24 * 220, 4, 12),
    'fortnight':  (60.0,  24 * 45,  6, 15),
    'season':     (30.0,  24 * 22,  8, 15),
    'sprint':     (5.0,   24 * 3.5, 0, 0),
    'dev':        (5.0,   24 * 3,   0, 0),
    'blitz':      (1.0,   10.0,     0, 0),
    'test':       (0.25,  0.8,      0, 0),
}
if set(SCHEDULE) != set(PACE):
    sys.exit(f'sim.py schedules the presets {sorted(SCHEDULE)}; numbers.json has {sorted(PACE)}')
PRESETS = [Preset(name, PACE[name], dt, horizon_h, sessions, minutes, continuous=not sessions)
           for name, (dt, horizon_h, sessions, minutes) in SCHEDULE.items()]

# =====================================================================
# 4. SIMULATION
# =====================================================================
class Sim:
    def __init__(self, preset, mode='near', max_depth=None, phase_h=0.0):
        self.phase = phase_h * 3600.0
        self.p = preset
        self.P = preset.P
        self.mode = mode           # 'none' builder | 'near' mixed | 'explore' explorer | 'raid' raider | 'far' greedy far-miner
        self.t = 0.0               # real seconds
        self.res = list(START_RES)
        self.b = defaultdict(int, START_LEVELS)
        self.r = defaultdict(int)
        self.ships = defaultdict(int, START_SHIPS)
        self.defs = defaultdict(int)
        self.bact, self.bplan, self.urgent = [], [], []
        self.ract, self.rplan = None, []
        self.sact, self.splan = None, []
        self.depth = QUEUE_DEPTH if max_depth is None else max_depth
        self.spent = {'b': 1.0, 'r': 1.0, 's': 1.0}
        self.share = {'b': 0.50, 'r': 0.25, 's': 0.25}
        if mode in ('explore', 'raid', 'far'):
            self.share = {'b': 0.40, 'r': 0.20, 's': 0.40}   # fleet first: ships instead of mines
        self.charted = 3 * HOME_DIST * (HOME_DIST + 1)       # sectors within the sensors at start
        self.milestones = {}
        self.income = defaultdict(float)     # source -> units (value-weighted by raw sum)
        self.income_by_ring = defaultdict(float)
        self.waste = [0.0, 0.0, 0.0]
        self.cap_pressure_time = 0.0         # seconds with any stock > 90% cap
        self.cap_full_time = 0.0
        self.short_power_time = 0.0          # seconds with the mines short of energy
        self.loss_value = 0.0
        self.econ_pts = sum(wv(grown_cost(*BUILD[k], l)) for k, top in START_LEVELS.items() for l in range(1, top + 1))
        self.fleet_pts = sum(n * wv(SHIPS[c]['cost']) for c, n in START_SHIPS.items())
        self.def_pts = 0.0
        self.samples = []
        self.owned = {}            # mark (real seconds) -> what the player owns then
        self.pilot_rate = None; self.pilot_dirty = True; self.pilot_next = 0.0
        self.script_b = self.build_script()
        self.log = []          # (t, kind, text)
        self.daily = []

    # ------------------------------------------------------------------ helpers
    def slots(self):
        return BUILDING_SLOTS[self.r['computer']]
    def cap(self):
        s = cap_mult(self.P)
        return [STORAGE_CAP[self.b[n]] * s for n in STORES]
    def power(self):    # energy made and needed now
        return energy_made(self.b, self.r, self.ships['satellite'], self.res[2] > 0), energy_needed(self.b)
    def prod_h(self):   # per game hour, the reactor's burn taken off
        out = mined_per_hour(self.b, power_fraction(*self.power()))
        if self.b['fusion'] and self.res[2] > 0:
            out[2] -= FUSION_BURN[self.b['fusion']]
        return out
    def b_cost(self, name, L):
        return grown_cost(*BUILD[name], L)
    def r_cost(self, name, L):
        return grown_cost(*RESEARCH[name][:2], L)
    def b_time(self, name, L):   # real seconds
        speed = (1 + self.b['robotics']) * 2 ** self.b['nanite']
        return econ_ticks(cost_seconds(self.b_cost(name, L), BUILD_RATE, speed), self.P)
    def r_time(self, name, L):
        return econ_ticks(cost_seconds(self.r_cost(name, L), RES_RATE, 1 + self.b['lab']), self.P)
    def s_time(self, item):
        speed = (1 + self.b['shipyard']) * 2 ** self.b['nanite']
        return econ_ticks(cost_seconds(ITEMS[item]['cost'], SHIP_RATE, speed), self.P)
    def can_pay(self, c, reserve=(0, 0, 0)):   # what another queue waits for is left alone; a resource the cost does not ask is not in the way
        return all(c[i] <= 0 or self.res[i] - reserve[i] >= c[i] - 1e-9 for i in range(3))
    def pay(self, c):
        for i in range(3):
            self.res[i] -= c[i]
    def mark(self, key):
        if key not in self.milestones:
            self.milestones[key] = self.t
    def met(self, prereqs, blv=None, rlv=None):
        blv, rlv = self.b if blv is None else blv, self.r if rlv is None else rlv
        return all((blv if kind == 'b' else rlv).get(n, 0) >= l for kind, n, l in prereqs)

    # ------------------------------------------------------------------ planning scripts
    def build_script(self):
        cur = defaultdict(int, START_LEVELS)
        out = []
        def ensure(name, target):
            target = min(target, MAX_LEVEL)
            while cur[name] < target:
                cur[name] += 1
                out.append((name, cur[name]))
        for r in range(2, 31):
            ensure('metal', r)
            for name, ratio, least, start in SCRIPT:
                if r >= start:
                    ensure(name, max(least, int(round(ratio * r))))
        return out

    # projected levels including everything in progress/planned
    def proj_b(self):
        m = dict(self.b)
        for it in self.bact + self.bplan:
            m[it[0]] = max(m.get(it[0], 0), it[1])
        return m
    def proj_r(self):
        m = dict(self.r)
        for it in ([self.ract] if self.ract else []) + self.rplan:
            m[it[0]] = max(m.get(it[0], 0), it[1])
        return m

    def next_scripted_building(self, pb):
        for (n, l) in self.script_b:
            if pb.get(n, 0) == l - 1 and self.met(BUILD_PREREQ[n], pb):
                return (n, l)
        return None

    def want_storage(self, pb):
        cap = self.cap()
        nxt = self.next_scripted_building(pb)
        for i, name in enumerate(STORES):
            if pb.get(name, 0) > self.b[name] or pb.get(name, 0) >= MAX_LEVEL:
                continue
            if self.res[i] > STORE_FILL * cap[i] or (nxt and self.b_cost(*nxt)[i] > STORE_HEADROOM * cap[i]):
                return (name, pb.get(name, 0) + 1)
        return None

    def scarce_mine(self, pb):
        """The next level of the Crystal Mine or the Deuterium Synthesizer when its resource is the one
        the empire is out of: the stock is the fewest hours of its own production, and under SCARCE of
        the most plentiful's.  The mine goes no more than SCARCE_AHEAD levels past the script."""
        ph = self.prod_h()
        hrs = [self.res[i] / max(ph[i], 5.0) for i in range(3)]
        lo = min(range(3), key=lambda i: hrs[i])
        n = MINES[lo]
        ahead = [max(least, int(round(ratio * self.b['metal']))) + SCARCE_AHEAD for name, ratio, least, _ in SCRIPT if name == n]
        if not ahead or hrs[lo] >= SCARCE * max(hrs) or pb.get(n, 0) >= min(MAX_LEVEL, ahead[0]):
            return None
        if any(x[0] == n for x in self.bplan) or any(x[0] == n for x in self.bact):
            return None
        return (n, pb.get(n, 0) + 1)

    def satellites_short(self):
        """Solar Satellites still to order for the mines that stand, are being built or wait in the plan."""
        levels = self.proj_b()
        short = energy_needed(levels) - energy_made(levels, self.r, self.ships['satellite'])
        return max(0, math.ceil(short / SATELLITE_ENERGY))

    def powered(self, mine, pb):
        """What to order for `mine`: the mine, or first the power it would run short of.  The cheapest
        energy at the score's weights is taken: the next Solar Plant level, Solar Satellites (which
        the shipyard builds while the mine waits its turn) or the next Fusion Reactor level with its burn."""
        after = dict(pb); after[mine[0]] = mine[1]
        short = energy_needed(after) - energy_made(pb, self.r, self.ships['satellite'])
        if short <= 0:
            return mine
        options = []
        solar, fusion, tech = pb.get('solar', 0), pb.get('fusion', 0), self.r['energy']
        if solar < MAX_LEVEL:
            options.append((wv(self.b_cost('solar', solar + 1)) / (SOLAR_ENERGY[solar + 1] - SOLAR_ENERGY[solar]), 'solar'))
        if self.met(SHIPS['satellite']['requires']):
            options.append((wv(SHIPS['satellite']['cost']) / SATELLITE_ENERGY, 'satellite'))
        if fusion < MAX_LEVEL and self.met(BUILD_PREREQ['fusion']):
            burn = (FUSION_BURN[fusion + 1] - FUSION_BURN[fusion]) * FUSION_PAYBACK_H * W_RES[2] / 1000.0
            gain = FUSION_ENERGY[tech][fusion + 1] - FUSION_ENERGY[tech][fusion]
            options.append(((wv(self.b_cost('fusion', fusion + 1)) + burn) / gain, 'fusion'))
        source = min(options)[1]
        return mine if source == 'satellite' else (source, pb.get(source, 0) + 1)

    def power_stands_for(self, mine, level):
        """Whether the energy for `mine` at `level` stands or is being built, beside the mines that do."""
        levels = dict(self.b)
        for a in self.bact:
            levels[a[0]] = a[1]
        levels[mine] = level
        coming = self.sact[1] if self.sact and self.sact[0] == 'satellite' else 0
        return energy_made(levels, self.r, self.ships['satellite'] + coming) >= energy_needed(levels)

    def plan_buildings(self):
        pb = self.proj_b()
        # the mine of the resource everything waits for, or its power, is planned beside the script's orders, one
        # at a time, and has the first call on what it costs (try_start)
        self.urgent = [it for it in self.urgent if it in self.bplan]
        scarce = self.scarce_mine(pb) if not self.urgent else None
        if scarce:
            scarce = self.powered(scarce, pb)
            self.bplan.append(scarce)
            self.urgent.append(scarce)
            pb[scarce[0]] = scarce[1]
        while len(self.bplan) - len(self.urgent) < self.depth:
            item = self.want_storage(pb) or self.next_scripted_building(pb)
            if item is None:
                break
            if item[0] in MINES:
                item = self.powered(item, pb)
            if item in self.bplan or any(a[0] == item[0] and a[1] == item[1] for a in self.bact):
                break
            self.bplan.append(item)
            pb[item[0]] = item[1]

    def research_step(self, name, level, pr):
        """The next research to order on the way to `name` at `level`: its own next level once what it
        requires stands, else the next level of the first research it waits for.  None when it is
        reached, or while a building it requires is not built."""
        if pr.get(name, 0) >= min(level, RESEARCH[name][2]):
            return None
        for kind, n, l in RESEARCH[name][3]:
            if kind == 'r' and pr.get(n, 0) < l:
                return self.research_step(n, l, pr)
        if not self.b['lab'] or not self.met([p for p in RESEARCH[name][3] if p[0] == 'b']):
            return None
        return (name, pr.get(name, 0) + 1)

    def plan_research(self):
        pr = self.proj_r()
        for name, level in RESEARCH_GOALS:
            while len(self.rplan) < self.depth:
                step = self.research_step(name, level, pr)
                if step is None:
                    break
                self.rplan.append(step); pr[step[0]] = step[1]
            if len(self.rplan) >= self.depth:
                break

    def squads(self):
        """(miners, warships) as class -> count: cargo ships mine; until the first of them the starting fighters do."""
        sh = self.ships
        miners = {c: sh[c] for c in MINERS if sh[c]}
        if not miners:
            miners = {c: min(sh[c], n) for c, n in START_SHIPS.items() if sh[c]}
        return miners, {c: sh[c] - miners.get(c, 0) for c in WARSHIPS if sh[c] - miners.get(c, 0) > 0}

    def mods(self):
        return WEAPON_MOD[self.r['weapons']], HULL_MOD[self.r['armour']], SHIELD_MOD[self.r['shielding']]

    def combat_power(self, count=None):
        mods = self.mods()
        count = self.squads()[1] if count is None else count
        return sum(n * unit_power(SHIPS[c], *mods) for c, n in count.items())

    def def_power(self):
        """Home defence: the warships docked there, the Solar Satellites and every defence."""
        mods = self.mods()
        away = self.active_frac() if self.mode != 'none' else 0.0
        return ((1.0 - away) * self.combat_power()
                + self.ships['satellite'] * unit_power(SHIPS['satellite'], *mods)
                + sum(n * unit_power(DEFS[k], *mods) for k, n in self.defs.items()))

    def raid_eligible(self):
        worth = self.b['shipyard'] >= RAID_MIN_SHIPYARD or any(self.defs.values())
        return worth and self.t >= RAID_MIN_AGE_S / self.P ** ATTENTION_EXP

    def best_defence(self):
        """The unlocked defence with the most power for its cost at the score's weights; a dome is built once."""
        mods = self.mods()
        open_ = [k for k, d in DEFS.items() if self.met(d['requires']) and not (d['max_count'] and self.defs[k] >= d['max_count'])]
        return max(open_, key=lambda k: unit_power(DEFS[k], *mods) / wv(DEFS[k]['cost']), default=None)

    def fleet_wants(self):
        """The next order of the fleet plan: miners for a player who flies, then the heaviest line unlocked."""
        sh, metal, flies = self.ships, self.b['metal'], self.mode != 'none'
        battleships = ((metal - BATTLESHIPS_FROM_METAL) * BATTLESHIPS_PER_LEVEL
                       + max(0, self.b['shipyard'] - KIT_B['shipyard']) * BATTLESHIPS_PER_SHIPYARD)
        cruisers = min(MOST_CRUISERS, int((metal - CRUISERS_FROM_METAL) * CRUISERS_PER_LEVEL))
        plan = [('small_cargo', FIRST_MINERS if flies else 0, SHIP_BATCH),
                ('battleship', max(1, battleships), 1),
                ('cruiser', max(1, cruisers), 1),
                ('heavy_fighter', MOST_HEAVY_FIGHTERS, 1),
                ('light_fighter', MOST_LIGHT_FIGHTERS if flies else 0, SHIP_BATCH),
                ('small_cargo', MOST_MINERS if flies else 0, 1)]
        for cls, want, batch in plan:
            if sh[cls] < want and self.met(SHIPS[cls]['requires']):
                return (cls, min(batch, want - sh[cls]))
        if not self.bplan and not self.rplan and self.met(SHIPS['battleship']['requires']):
            return ('battleship', 1)   # the script and the research order are done: what is left over is fleet
        return None

    def plan_ships(self):
        if not self.b['shipyard'] or self.splan or self.sact:
            return
        item = None
        satellites = self.satellites_short() if self.met(SHIPS['satellite']['requires']) else 0
        if satellites > 0:
            item = ('satellite', min(satellites, ENERGY_BATCH))
        elif self.raid_eligible():
            # defence first while the strongest raid of the forecast outweighs what stands at home: this
            # player's own rule of thumb; the game fights the raid as a battle and compares no two powers
            short = RAID_POWER_ROLL_MAX * raid_power(self.econ_pts) - self.def_power()
            pick = self.best_defence() if short > 0 else None
            if pick:
                item = (pick, max(1, min(DEFENCE_BATCH, int(short / unit_power(DEFS[pick], *self.mods())) + 1)))
        item = item or self.fleet_wants()
        if item:
            self.splan.append(item)

    # ------------------------------------------------------------------ queue engine
    def queue_store_for(self, c):
        """Queue first the storage a cost of `c` does not fit under.  True when the plan changed."""
        cap = self.cap(); changed = False
        for i, name in enumerate(STORES):
            if c[i] <= cap[i] or self.b[name] >= MAX_LEVEL:
                continue
            if not any(it[0] == name for it in self.bplan) and not any(a[0] == name for a in self.bact):
                self.bplan.insert(0, (name, self.b[name] + 1))
                changed = True
        return changed
    def over_cap(self, c):
        cap = self.cap()
        return any(c[i] > cap[i] for i in range(3))

    def can_begin(self, n, l):
        """Whether building `n` to `l` waits for nothing but its cost: a mine is not begun before its power."""
        if not self.met(BUILD_PREREQ[n]) or self.b[n] != l - 1 or any(a[0] == n for a in self.bact):
            return False
        return n not in MINES or self.power_stands_for(n, l)
    def begin(self, n, l, c):
        self.pay(c); self.spent['b'] += wv(c)
        self.bplan.remove((n, l))
        self.bact.append([n, l, self.t + self.b_time(n, l), c])

    def try_start(self):
        # fair-share ordering of queues; Solar Satellites the mines wait for come before everything
        order = sorted(('b', 'r', 's'), key=lambda q: self.spent[q] / self.share[q])
        if self.splan and self.splan[0][0] == 'satellite':
            order.remove('s'); order.insert(0, 's')
        reserve = [0.0, 0.0, 0.0]
        reserved = False
        for n, l in [it for it in self.urgent if it in self.bplan]:
            c = self.b_cost(n, l)
            if len(self.bact) < self.slots() and self.can_begin(n, l) and not self.over_cap(c):
                if self.can_pay(c):
                    self.begin(n, l, c)
                else:
                    reserve = list(c); reserved = True
        for q in order:
            if q == 'b':
                started = True
                while started and len(self.bact) < self.slots() and self.bplan:
                    started = False
                    for idx, (n, l) in enumerate(list(self.bplan)):
                        if not self.can_begin(n, l):
                            continue
                        c = self.b_cost(n, l)
                        if n not in STORES and self.over_cap(c):
                            if self.queue_store_for(c):
                                started = True      # re-scan with the store first
                                break
                            continue
                        if not self.can_pay(c, reserve):
                            if not reserved and idx == 0:
                                # only reserve when we could afford it within ~24 game-hours of income
                                ph = self.prod_h()
                                short = max((c[i] - self.res[i]) / max(ph[i], 1e-9) for i in range(3) if c[i] > 0)
                                if short <= 24.0:
                                    reserve = list(c); reserved = True
                            continue
                        self.begin(n, l, c)
                        started = True
                        break
            elif q == 'r':
                if self.ract is None and self.b['lab']:
                    for idx, (n, l) in enumerate(self.rplan):
                        if self.r[n] != l - 1 or not self.met(RESEARCH[n][3]):
                            continue
                        c = self.r_cost(n, l)
                        if self.over_cap(c):
                            self.queue_store_for(c)
                            continue
                        if not self.can_pay(c, reserve):
                            if not reserved:
                                reserve = list(c); reserved = True
                            continue
                        self.pay(c); self.spent['r'] += wv(c)
                        self.rplan.pop(idx)
                        self.ract = [n, l, self.t + self.r_time(n, l), c]
                        break
            else:
                if self.sact is None and self.splan:
                    item, qty = self.splan[0]
                    unit = ITEMS[item]['cost']
                    if self.over_cap(unit):
                        self.queue_store_for(unit)
                        continue
                    # buy as many as affordable (min 1) up to qty, respecting reserve
                    n = 0
                    for k in range(qty, 0, -1):
                        c = tuple(u * k for u in unit)
                        if not self.over_cap(c) and self.can_pay(c, reserve):
                            n = k; break
                    if n == 0:
                        if not reserved:
                            reserve = [u for u in unit]; reserved = True
                        continue
                    c = tuple(u * n for u in unit)
                    self.pay(c); self.spent['s'] += wv(c)
                    self.splan.pop(0)
                    self.sact = [item, n, self.t + self.s_time(item)]
                    if n < qty:
                        self.splan.insert(0, (item, qty - n))

    def complete(self):
        done = [a for a in self.bact if a[2] <= self.t]
        for a in done:
            self.bact.remove(a)
            self.b[a[0]] = a[1]
            self.log.append((self.t, 'b', f'{a[0]} {a[1]}'))
            self.econ_pts += wv(a[3])
            self.on_building(a[0], a[1])
            self.pilot_dirty = True
        if self.ract and self.ract[2] <= self.t:
            n, l = self.ract[0], self.ract[1]
            self.r[n] = l
            self.log.append((self.t, 'r', f'{n} {l}'))
            self.econ_pts += wv(self.ract[3])
            self.ract = None
            self.on_research(n, l)
            self.pilot_dirty = True
        while self.sact and self.sact[2] <= self.t:
            item = self.sact[0]
            self.log.append((self.t, 's', item))
            if item in SHIPS:
                self.ships[item] += 1
                self.mark('first_' + item)
                self.fleet_pts += wv(SHIPS[item]['cost'])
            else:
                self.defs[item] += 1
                self.def_pts += DEFENCE_SHARE * wv(DEFS[item]['cost'])
            self.pilot_dirty = True
            self.sact[1] -= 1
            if self.sact[1] <= 0:
                self.sact = None
            else:
                self.sact[2] += self.s_time(item)
            self.check_endgame()

    def on_building(self, n, l):
        if n == 'metal' and l == 10: self.mark('metal10')
        if n == 'shipyard' and l == RAID_MIN_SHIPYARD: self.mark('raided')
        if n == 'robotics' and l == 2: self.mark('robotics2')
        if n == 'lab' and l == 1: self.mark('lab1')
        self.check_endgame()
    def on_research(self, n, l):
        if n == 'computer' and BUILDING_SLOTS[l] > BUILDING_SLOTS[l - 1]:
            self.mark('slot%d' % BUILDING_SLOTS[l])
        self.check_endgame()
    def check_endgame(self):
        if 'endgame' in self.milestones: return
        made, needed = self.power()
        if all(self.b[k] >= v for k, v in KIT_B.items()) and all(self.r[k] >= v for k, v in KIT_R.items()) \
                and all(self.ships[k] >= v for k, v in KIT_SHIPS.items()) and made >= needed:
            self.mark('endgame')

    # ------------------------------------------------------------------ pilot economy
    def pilot_scale(self):
        return self.P ** BETA
    def fleet(self, counts):
        """A fleet of `counts`: fuel one jump burns, its tank, the hops out it can fly and come back from,
        its hold, its harvest power and the ticks one jump takes its slowest ship.  The speed is the
        class's own: what a drive's research adds is not in numbers.json."""
        r = self.r
        burn = sum(n * jump_fuel(c, r) for c, n in counts.items())
        tank = sum(n * ship_tank(c, r, self.b['deut_tank']) for c, n in counts.items())
        return dict(burn=burn, tank=tank, hops=int((tank / burn) // 2) if burn > 0 else 0,
                    cargo=sum(n * SHIPS[c]['cargo'] for c, n in counts.items()) * self.P ** FINDS_EXP,
                    harvest=sum(n * SHIPS[c]['harvest_power'] for c, n in counts.items()),
                    ticks=max((jump_ticks(ship_drive(c, r)[0]) for c in counts), default=1))
    def pilot_rates(self):
        """Expected pilot income per REAL hour: ore[3], salv[3], der[3], fuel (deut), loss (value), reach ring.
        mode 'near' = competent (engage at >=3x power, mine <=ring 8, avoid guarded tiles);
        mode 'far'  = greedy (engage at >=1.5x, pick the farthest ring whose net value is within 80% of best).
        Neither flies to a ring where the loot does not outweigh the fuel (worth_flying)."""
        out = dict(ore=[0, 0, 0], salv=[0, 0, 0], der=[0, 0, 0], fuel=0.0, loss=0.0, reach=HOME_DIST,
                   ore_ring={}, salv_ring={}, der_ring={}, hunt_d=0, kills_ring={}, sites_ring={}, farm={})
        if self.mode == 'none':
            return out
        s = self.pilot_scale(); P = self.P
        far = self.mode == 'far'
        ratio_req = 1.5 if far else 3.0
        hunt_ring_cap = 40 if far else 20
        harvest_mod = HARVEST_YIELD[self.r['laser']]
        price = FUEL_PRICE_DEUT
        active = self.active_frac()
        miners, warships = self.squads()
        mine, war = self.fleet(miners), self.fleet(warships)
        cargo, H, fuel_hop = mine['cargo'], mine['hops'], mine['burn']
        mp = sum(n * unit_power(SHIPS[c], *self.mods()) for c, n in miners.items())
        cps = self.combat_power(warships)
        cH, chop = war['hops'], war['burn']
        creach = HOME_DIST + cH
        reach = HOME_DIST + H
        out['reach'] = max(reach if cargo > 0 else HOME_DIST, creach if cps > 0 else HOME_DIST)

        def ore_for(ring_hi):
            tot = [0.0, 0.0, 0.0]; fuel = 0.0; loss = 0.0; ring = {}
            if cargo <= 0:
                return tot, fuel, loss, ring
            for d in range(3, int(ring_hi) + 1):
                hops = max(1, abs(d - HOME_DIST) + 1)
                if hops > H:
                    continue
                n_tiles = min(6 * d, 6 * H) * ring_coverage(d)
                guarded = npc_presence(d) * (1 - npc_passive_frac(d))
                tr = tile_reserve(d)
                esc = cps + mp
                if not far:
                    avail = 1.0 if esc / npc_power(d) >= 2.0 else 1.0 - guarded
                else:
                    avail = 1.0
                sup = [n_tiles * avail * tr[k] / ore_regen_h(d) * P * harvest_mod for k in MINES]
                haul = min(cargo * 0.9, ore_tile(d) * harvest_mod)
                burn = sum(sup) / max(haul, 1) * 2 * hops * fuel_hop * price
                if not worth_flying(sup, burn):
                    continue
                for i in range(3): tot[i] += sup[i]
                ring[d] = sum(sup)
                fuel += burn
                if guarded > 0 and far:
                    p_enc = 1 - (1 - guarded * 0.5) ** hops
                    r = (esc + 1e-9) / npc_power(d)
                    loss += p_enc * attrition(r) * sum(sup) / 1.0 * 0.5
            # what the fleet can bring home in an hour: a sortie jumps out, strips one tile and jumps back
            d = int(ring_hi)
            haul = min(cargo * 0.9, ore_tile(d) * harvest_mod)
            strip = mean_reserve(d) * ORE_RING_MULT(d) / (mean_harvest_mult(d) * mine['harvest'])
            cap_units = haul * 3600 / (MINE_TURNAROUND_S + 2 * max(1, d - HOME_DIST) * mine['ticks'] + strip) * 0.7
            t = sum(tot)
            if t > cap_units and t > 0:
                f = cap_units / t
                tot = [x * f for x in tot]; fuel *= f; loss *= f
                ring = {k: v * f for k, v in ring.items()}
            return tot, fuel, loss, ring

        if far:
            cands = []
            for rh in range(4, min(40, max(4, int(reach))) + 1):
                o, f, l, rg = ore_for(rh)
                net = sum(o) - f - l
                cands.append((rh, net, o, f, l, rg))
            if cands:
                mx = max(c[1] for c in cands)
                ok = [c for c in cands if c[1] >= 0.8 * mx and c[1] > 0]
                pick = max(ok, key=lambda c: c[0]) if ok else None
                if pick:
                    out['ore'], out['fuel'], out['loss'], out['ore_ring'] = pick[2], pick[3], pick[4], pick[5]
        else:
            o, f, l, rg = ore_for(min(INNER_RING, reach))
            out['ore'], out['fuel'], out['loss'], out['ore_ring'] = o, f, l, rg

        # ---- salvage (hunting): needs combat ships; capped by active time
        if cps > 0:
            hunt_cap = 3600.0 / SECONDS_PER_KILL * active
            cand = [d for d in range(2, min(creach, hunt_ring_cap) + 1) if cps / npc_power(d) >= ratio_req]
            best_d = max(cand) if cand else 0
            kills = []
            for d in cand:
                hops = max(1, abs(d - HOME_DIST) + 1)
                nsec = min(6 * d, 6 * cH) * ring_coverage(d) * npc_presence(d) * (1 - npc_passive_frac(d))
                kills.append((d, nsec / npc_respawn_h(d) * P ** FINDS_EXP, hops))
            kills.sort(key=lambda x: -kill_loot_value(x[0]))
            room = hunt_cap; salv = 0.0
            for d, sup, hops in kills:
                k = min(sup, room)
                if k <= 0: break
                util = k / sup if sup > 0 else 0.0
                f = farm_steady(util)
                val = kill_loot_value(d) * s * k * f
                squad = min(1.0, 2.0 * ratio_req * npc_power(d) / cps)   # the share of the fleet a hunt at this ring takes
                burn = k / KILLS_PER_HUNT * (2 * hops + KILLS_PER_HUNT) * chop * squad * price
                if not worth_flying([val * share for share in LOOT_SPLIT], burn):
                    continue
                room -= k
                out['farm'][d] = f
                salv += val
                out['salv_ring'][d] = val
                out['kills_ring'][d] = k
                r = cps / npc_power(d)
                out['loss'] += val * attrition(r)
                out['fuel'] += burn
            out['salv'] = [salv * share for share in LOOT_SPLIT]
            out['hunt_d'] = best_d
            # ---- derelicts: boarded by the combat fleet (a Recycler in company doubles loot -> ignored)
            der_cap = 3600.0 / SECONDS_PER_BOARDING * active
            room = der_cap; der = 0.0
            for d in range(HOME_DIST, min(creach, hunt_ring_cap) + 1):
                if cps / npc_power(d) < ratio_req * 0.8:
                    continue
                sites = derelict_sites_per_ring(d) * ring_coverage(d)
                sup = sites / DERELICT_RESPAWN_H * P ** DERELICT_PACE_EXP
                k = min(sup, room)
                if k <= 0: break
                relic = 1 + relic_chance(threat(d)) * (RELIC_LOOT_MULT - 1)
                val = derelict_value(d) * relic * s * k * (1 - derelict_ambush(d) * 0.5)
                hops = max(1, abs(d - HOME_DIST) + 1)
                squad = min(1.0, 2.0 * ratio_req * npc_power(d) / cps)
                burn = k / SITES_PER_ROUND * (2 * hops + SITES_PER_ROUND) * chop * squad * price
                if not worth_flying([val * share for share in LOOT_SPLIT], burn):
                    continue
                room -= k
                der += val
                out['der_ring'][d] = val
                out['sites_ring'][d] = k
                r = cps / (npc_power(d) * AMBUSH_GUARD_SHARE)
                out['loss'] += val * derelict_ambush(d) * attrition(r)
                out['fuel'] += burn
            out['der'] = [der * share for share in LOOT_SPLIT]
        return out

    def active_frac(self):
        p = self.p
        if p.continuous:
            return PILOT_SHARE[self.mode]
        return p.sessions * p.session_min / 1440.0

    def apply_pilot(self, dt):
        if self.mode == 'none':
            return
        if self.pilot_dirty or self.t >= self.pilot_next:
            self.pilot_rate = self.pilot_rates()
            self.pilot_dirty = False
            self.pilot_next = self.t + max(60.0, 3600.0 / self.P * 6)
            for d, v in self.pilot_rate['der_ring'].items():
                if v > 0 and derelict_tier(d) > 1:
                    self.mark('tier%d_loot' % derelict_tier(d))
        r = self.pilot_rate
        if not r: return
        need = r['fuel'] * dt / 3600.0
        f = 1.0
        if need > 0:
            income = max(0.0, self.prod_h()[2]) * self.P * dt / 3600.0
            f = max(0.0, min(1.0, FUEL_INCOME_SHARE * income / (need * PILOT_SKILL)))
        f *= PILOT_SKILL
        r = dict(r)
        for k in ('ore', 'salv', 'der'):
            r[k] = [x * f for x in r[k]]
        r['fuel'] *= f; r['loss'] *= f
        for src, key in (('ore', 'ore'), ('salv', 'salv'), ('der', 'der')):
            for i in range(3):
                amt = r[key][i] * dt / 3600.0
                self.res[i] += amt
                self.income[src] += amt
        net_fuel = r['fuel'] * dt / 3600.0
        self.res[2] -= net_fuel
        self.income['fuel_cost'] -= net_fuel
        loss = r['loss'] * dt / 3600.0
        # losses are modelled as replacement cost drained proportionally from stock
        self.loss_value += loss
        for i in range(3):
            self.res[i] -= loss * LOOT_SPLIT[i]
        fleets = {'explore': 2.0, 'near': 1.0}.get(self.mode, 0.0)
        if fleets and self.charted < 3 * (r['reach'] + 2) * (r['reach'] + 3):
            n = PILOT_SKILL * fleets * CHART_RATE_PER_FLEET_H * self.active_frac() * dt / 3600.0 * (0.5 if self.mode == 'near' else 1.0)
            d = 1
            while 3 * d * (d + 1) < self.charted:
                d += 1
            self.charted += n
            for i in range(3):
                amt = n * chart_value(threat(d)) * LOOT_SPLIT[i]
                self.res[i] += amt
                self.income['charts'] += amt
        for rg, v in r['ore_ring'].items():
            self.income_by_ring[('ore', rg)] += v * dt / 3600.0
        for rg, v in r['salv_ring'].items():
            self.income_by_ring[('salv', rg)] += v * dt / 3600.0
        for rg, v in r['der_ring'].items():
            self.income_by_ring[('der', rg)] += v * dt / 3600.0

    # ------------------------------------------------------------------ attention
    def attentive(self):
        p = self.p
        if p.continuous:
            return True
        hod = (7.0 + (self.t + self.phase) / 3600.0) % 24.0
        n = p.sessions
        # sessions evenly spread 07:00 .. 23:00
        for i in range(n):
            start = 7.0 + (16.0 * i / max(1, n - 1) if n > 1 else 0)
            if start <= hod < start + p.session_min / 60.0:
                return True
        return False

    # ------------------------------------------------------------------ main loop
    def step(self, dt):
        # production (real dt seconds, game rate x P)
        ph = self.prod_h()
        cap = self.cap()
        made, needed = self.power()
        if made < needed:
            self.short_power_time += dt
        press = False; full = False
        for i in range(3):
            inc = ph[i] * self.P * dt / 3600.0 * (1.0 - ATTENTION_PENALTY * PILOT_SHARE[self.mode])
            self.income['buildings'] += inc
            self.res[i] += inc
        self.apply_pilot(dt)
        for i in range(3):
            if self.res[i] > cap[i]:
                self.waste[i] += self.res[i] - cap[i]
                self.res[i] = cap[i]
                full = True
            if self.res[i] > 0.9 * cap[i]:
                press = True
            if self.res[i] < 0:
                self.res[i] = 0.0
        if press: self.cap_pressure_time += dt
        if full: self.cap_full_time += dt
        self.complete()
        if self.attentive():
            self.plan_buildings(); self.plan_research(); self.plan_ships()
        self.try_start()
        self.t += dt

    def score(self):
        return self.econ_pts + self.fleet_pts + self.def_pts

    def inventory(self):
        """Everything the player owns now, and how it stands against the map and the next raid."""
        cps = self.combat_power()
        beats = max((d for d in RING if cps >= 3.0 * npc_group_power(d)), default=0)
        mean_raid = raid_power(self.econ_pts)
        return dict(t=self.t, res=list(self.res), cap=self.cap(), prod=[x * self.P for x in self.prod_h()],
                    b=dict(self.b), r=dict(self.r), ships=dict(self.ships), defs=dict(self.defs),
                    energy=self.power(), slots=self.slots(), score=self.score(),
                    econ_pts=self.econ_pts, fleet_pts=self.fleet_pts, def_pts=self.def_pts,
                    fleet_power=cps, beats=beats, reach=HOME_DIST + self.fleet(self.squads()[1])['hops'],
                    def_power=self.def_power(), raid=(RAID_POWER_ROLL_MIN * mean_raid, RAID_POWER_ROLL_MAX * mean_raid),
                    raided=self.raid_eligible())

    def snapshot(self):
        x = self.inventory()
        x['inc'] = dict(self.income)
        x['pil'] = self.pilot_rate and dict(ore=sum(self.pilot_rate['ore']), salv=sum(self.pilot_rate['salv']),
                                            der=sum(self.pilot_rate['der']), reach=self.pilot_rate['reach'])
        x['rings'] = self.pilot_rate and dict(ore=dict(self.pilot_rate['ore_ring']), salv=dict(self.pilot_rate['salv_ring']),
                                              der=dict(self.pilot_rate['der_ring']))
        self.samples.append(x)

    def run(self, marks=()):
        """Play to the endgame kit and past every mark (real seconds), or to the horizon."""
        end = self.p.horizon_h * 3600.0
        dt = self.p.dt
        sample_every = end / 60.0
        next_s = 0.0
        next_day = DAILY_EVERY_S
        marks = sorted(marks)
        while self.t < end:
            self.step(dt)
            if self.t >= next_s:
                self.snapshot(); next_s += sample_every
            if self.t >= next_day:
                self.daily.append((self.t, self.score()))
                next_day += DAILY_EVERY_S
            while marks and self.t >= marks[0]:
                self.owned[marks.pop(0)] = self.inventory()
            if 'endgame' in self.milestones and not marks:
                break
        self.snapshot()
        return self


# =====================================================================
# 5. REPORTING
# =====================================================================
def fmt_t(sec):
    if sec is None: return '-'
    if sec < 90: return f'{sec:.0f}s'
    if sec < 5400: return f'{sec/60:.0f}m'
    if sec < 86400 * 2: return f'{sec/3600:.1f}h'
    return f'{sec/86400:.1f}d'

def fmt_cost(c):
    return '/'.join(f'{x:.0f}' for x in c)

def fmt_requires(prereqs):
    return ', '.join(f'{LABEL[n]} {l}' for _, n, l in prereqs)

def tier_from(tier):         # the first ring whose derelicts are of `tier`
    return next(d for d in sorted(RING) if derelict_tier(d) >= tier)
MILESTONES = [('robotics2', 'Robotics Factory 2'), ('lab1', 'Research Lab 1'),
              ('raided', f'Shipyard {RAID_MIN_SHIPYARD} (worth raiding)'), ('first_light_fighter', 'First Light Fighter'),
              ('first_small_cargo', 'First Small Cargo'),
              ('slot2', 'Build slot 2 (Computer Technology)'), ('metal10', 'Metal Mine 10'),
              ('first_heavy_fighter', 'First Heavy Fighter'), ('first_cruiser', 'First Cruiser'),
              ('slot3', 'Build slot 3 (Computer Technology)'),
              ('tier2_loot', f'First tier-2 derelict loot (ring {tier_from(2)}+)'), ('first_battleship', 'First Battleship'),
              ('tier3_loot', f'First tier-3 derelict loot (ring {tier_from(3)}+)'), ('endgame', 'Endgame kit complete')]

def kit_text():
    parts = [f'{LABEL[k]} {v}' for k, v in list(KIT_B.items()) + list(KIT_R.items())]
    ships = ', '.join(f'{v} {LABEL[k]}s' for k, v in KIT_SHIPS.items())
    return ', '.join(parts) + f', the mines at full power, and {ships}'

def player_text():
    L = ['The player is the simulation\'s own: a build script in ratios to the Metal Mine ('
         + ', '.join(f'{LABEL[n]} x{ratio:g}' for n, ratio, _, _ in SCRIPT)
         + f'), the Crystal Mine or the Deuterium Synthesizer raised up to {SCARCE_AHEAD} levels past it when its resource runs out, '
         'power ordered with each mine from the cheapest source at the score\'s weights and no mine begun before its power, '
         'a store raised when a cap is in the way, defence bought while the strongest raid of the forecast outweighs the warships '
         'docked at home, the Solar Satellites and the defences together on the power scale (the game fights a raid as a battle; '
         'the comparison is this player\'s rule of thumb), and this research order:', '']
    for what, goals in RESEARCH_PLAN:
        L.append(f'- {what}: ' + ', '.join(f'{LABEL[n]} {l}' for n, l in goals))
    asked = {n for _, goals in RESEARCH_PLAN for n, _ in goals}
    needed = set()
    def walk(name):
        for kind, n, _ in RESEARCH[name][3]:
            if kind == 'r' and n not in needed:
                needed.add(n); walk(n)
    for n in asked: walk(n)
    never_r = [LABEL[n] for n in R_NAME if n not in asked | needed]
    never_s = [LABEL[n] for n in S_NAME if n not in LINE + ('small_cargo', 'satellite')]
    scripted = {'metal', 'solar', 'fusion'} | set(STORES) | {n for n, _, _, _ in SCRIPT}
    never_b = [LABEL[n] for n in B_NAME if n not in scripted]
    L += ['', f'The fleet plan: {FIRST_MINERS} Small Cargo for a player who flies, then the heaviest line unlocked '
          f'({BATTLESHIPS_PER_LEVEL} Battleships for each Metal Mine level above {BATTLESHIPS_FROM_METAL}, '
          f'{CRUISERS_PER_LEVEL:g} Cruisers for each above {CRUISERS_FROM_METAL} up to {MOST_CRUISERS}, {MOST_HEAVY_FIGHTERS} Heavy Fighters), '
          f'Light Fighters up to {MOST_LIGHT_FIGHTERS} and Small Cargo up to {MOST_MINERS}; Solar Satellites when they are the cheapest energy. '
          'In the field a flight is made only when what it brings outweighs the deuterium it burns, at the score\'s weights, and the field '
          f'burns no more than {100 * FUEL_INCOME_SHARE:.0f} percent of the deuterium the homeworld makes. It never orders ' + ', '.join(never_s) + '; it never researches ' + ', '.join(never_r)
          + '; it never builds ' + ', '.join(never_b) + '.', '',
          f'**The endgame kit** (the simulation\'s choice): {kit_text()}.', '',
          'How each world is played:', '', '| preset | pace | step | attention |', '|---|---|---|---|']
    for p in PRESETS:
        attention = 'without a break' if p.continuous else f'{p.sessions} sessions a day of {p.session_min} minutes'
        L.append(f'| {p.name} | x{p.P:g} | {p.dt:g} s | {attention} |')
    return '\n'.join(L)

PHASES = (0.0, 3.0, 6.0)      # session-schedule offsets (hours) used for the competent player's median; the other modes play the first
def marks_for(p):
    """Real seconds at which a run of preset `p` is read: the game time of day 3 and day 7 of a season,
    and those two real days where the world is still younger than that."""
    game = [d * 86400.0 * PACE['season'] / p.P for d in OWN_DAYS]
    return sorted(set(game + [d * 86400.0 for d in OWN_DAYS if d * 86400.0 < game[-1]]))

def run_all(modes=('near',), presets=None):
    out = {}
    for p in presets or PRESETS:
        for m in modes:
            runs = [Sim(p, mode=m, phase_h=ph).run(marks_for(p))
                    for ph in (PHASES if m == 'near' and not p.continuous else PHASES[:1])]
            base = runs[0]
            med = {}
            keys = set(k for r in runs for k in r.milestones)
            for k in keys:
                vals = sorted(r.milestones[k] for r in runs if k in r.milestones)
                if len(vals) * 2 > len(runs):
                    med[k] = vals[len(vals) // 2]
            base.milestones = med
            base.runs = runs
            out[(p.name, m)] = base
    return out

def static_tables():
    L = []
    L.append('### Threat, NPC power, kill loot, derelicts and ore by distance (pace-1 units; multiply loot by P^0.5 at other paces)')
    L.append('')
    L.append('| dist | zone | threat | NPC group (class x count) | NPC power | kill loot | derelict tier / loot | ore reserve per ore tile (any) | ore refill (game h) | NPC respawn (game h) |')
    L.append('|---|---|---|---|---|---|---|---|---|---|')
    for d in (1, 2, 4, 6, 8, 9, 10, 12, 13, 15, 18, 20, 21, 25, 30, 35):
        z = 'inner' if d <= INNER_RING else 'outer' if d <= OUTER_RING else 'wander'
        c, cnt, m = npc_group(d)
        L.append(f'| {d} | {z} | T{threat(d)} | {LABEL[c]} x{cnt} (x{m:.2f} stats) | {npc_power(d):.0f} | {kill_loot_value(d):.0f} | '
                 f'{derelict_tier(d)} / {derelict_value(d):.0f} | {mean_reserve(d)*ORE_RING_MULT(d):.0f} | {ore_regen_h(d):.0f} | {npc_respawn_h(d):.1f} |')
    L.append('')
    return '\n'.join(L)

def storage_cap_table():
    L = ['### Storage cap of one resource by the level of its own building, and pace', '',
         '| level | ' + ' | '.join(f'{LABEL[n]} cost (m/c)' for n in STORES) + ' | pace 1 | pace 10 | pace 600 |',
         '|---|---|---|---|---|---|---|']
    for lv in (0, 1, 2, 3, 4, 5, 6, 8, 10, 14):
        costs = ['-' if lv == 0 else fmt_cost(grown_cost(*BUILD[n], lv)[:2]) for n in STORES]
        L.append(f'| {lv} | ' + ' | '.join(costs) + ' | ' + ' | '.join(f'{storage_cap(lv, P):.0f}' for P in (1, 10, 600)) + ' |')
    L.append('')
    return '\n'.join(L)

def time_table():
    L = ['### Real time to build selected items (no Robotics Factory / Lab / Shipyard level behind them), by preset', '',
         '| item | cost (m/c/d) | ' + ' | '.join(f'{n} x{P:g}' for n, P in PACE.items()) + ' |', '|---|---|' + '---|' * len(PACE)]
    def row(name, cost, rate):
        cells = [fmt_t(econ_ticks(cost_seconds(cost, rate, 1.0), P)) for P in PACE.values()]
        L.append(f'| {name} | {fmt_cost(cost)} | ' + ' | '.join(cells) + ' |')
    for nm, lv in (('metal', 2), ('metal', 5), ('metal', 10), ('metal', 15), ('metal', 19), ('solar', 12),
                   ('robotics', 8), ('shipyard', 5), ('shipyard', 8), ('lab', 8)):
        row(f'{LABEL[nm]} {lv}', grown_cost(*BUILD[nm], lv), BUILD_RATE)
    for nm, lv in (('combustion', 2), ('computer', 3), ('impulse', 4), ('hyperdrive', 4)):
        row(f'{LABEL[nm]} {lv}', grown_cost(*RESEARCH[nm][:2], lv), RES_RATE)
    for nm in ('light_fighter', 'small_cargo', 'cruiser', 'battleship', 'satellite', 'rocket'):
        row(f'{LABEL[nm]} (one)', ITEMS[nm]['cost'], SHIP_RATE)
    L.append('')
    return '\n'.join(L)

def defence_table():
    L = ['### Defences and ships as one number: power = weapon + (hull + shield) / 10', '',
         '| unit | cost (m/c/d) | power | weight of cost per power | requires |', '|---|---|---|---|---|']
    for k, d in DEFS.items():
        once = ' (one only)' if d['max_count'] else ''
        L.append(f"| {LABEL[k]}{once} | {fmt_cost(d['cost'])} | {unit_power(d):.0f} | {1000*wv(d['cost'])/unit_power(d):.1f} | {fmt_requires(d['requires'])} |")
    for k, s in SHIPS.items():
        L.append(f"| (ship) {LABEL[k]} | {fmt_cost(s['cost'])} | {unit_power(s):.0f} | {1000*wv(s['cost'])/unit_power(s):.1f} | {fmt_requires(s['requires'])} |")
    L.append('')
    L.append('Mean raid power by econ points S (a raid is rolled between '
             f'{RAID_POWER_ROLL_MIN:g} and {RAID_POWER_ROLL_MAX:g} of it): '
             + ', '.join(f'S={x}: {raid_power(x):.0f}' for x in (1, 10, 100, 500, 1000, 2000, 20000)))
    L.append('')
    return '\n'.join(L)

def flight_table():
    L = ['### Flight, before research: fuel a jump, tank, hold, and ticks a jump at the class\'s own speed', '',
         '| ship | fuel a jump | deuterium a jump | tank | jumps in a tank | hold (pace 1) | harvest power | jump ticks |',
         '|---|---|---|---|---|---|---|---|']
    for k, s in SHIPS.items():
        if not s['flies']: continue
        L.append(f"| {LABEL[k]} | {jump_fuel(k, {}):.0f} | {jump_fuel(k, {})*FUEL_PRICE_DEUT:.1f} | {ship_tank(k, {}):.0f} | {s['tank_jumps']} | "
                 f"{s['cargo']} | {s['harvest_power']:g} | {jump_ticks(ship_drive(k, {})[0])} |")
    L.append('')
    L.append('Energy Technology takes ' + ', '.join(f'{100*(1-FUEL_RATE_MOD[l]):.0f}% at {l}' for l in (1, 5, 10))
             + ' off the burn; the Deuterium Tank multiplies every tank by '
             + ', '.join(f'{TANK_MOD[l]:g} at {l}' for l in (1, 3, 5)) + '.')
    L.append('')
    return '\n'.join(L)

def fuel_table():
    L = ['### What a hunt and a round of derelicts pay against the deuterium they burn', '',
         'The squad is the ships of one class of the plan\'s line that outnumber the ring\'s group 3 to 1 at the least fuel, with no '
         f'research, from a homeworld at ring {HOME_DIST}. A hunt is {KILLS_PER_HUNT} kills and a round {SITES_PER_ROUND} boardings, with '
         'a jump between each. Loot grows with P^0.5 and fuel does not, so each pays its fuel, at the score\'s weights, from the pace in '
         'the column after it.', '',
         '| ring | squad | deuterium a jump | a hunt burns | its loot at pace 1 | pays from | a round burns | its loot at pace 1 | pays from |',
         '|---|---|---|---|---|---|---|---|---|']
    def pays_from(loot, deuterium):
        pace = (wv((0.0, 0.0, deuterium)) / wv([loot * share for share in LOOT_SPLIT])) ** (1.0 / FINDS_EXP)
        return 'pace 1' if pace <= 1 else f'pace {pace:.0f}'
    for d in (4, 6, 8, 10, 12, 15, 18, 20):
        need = 3.0 * npc_group_power(d)
        burn, c = min((math.ceil(need / ship_power(c)) * jump_fuel(c, {}), c) for c in LINE)
        jump = burn * FUEL_PRICE_DEUT
        hops = max(1, abs(d - HOME_DIST) + 1)
        hunt, kills = (2 * hops + KILLS_PER_HUNT) * jump, KILLS_PER_HUNT * kill_loot_value(d)
        visit, sites = (2 * hops + SITES_PER_ROUND) * jump, SITES_PER_ROUND * derelict_value(d)
        L.append(f'| {d} | {math.ceil(need / ship_power(c))} {LABEL[c]} | {jump:.0f} | {hunt:.0f} | {kills:.0f} | {pays_from(kills, hunt)} | '
                 f'{visit:.0f} | {sites:.0f} | {pays_from(sites, visit)} |')
    L.append('')
    return '\n'.join(L)

def prod_tables():
    L = ['### Mines by level (pace 1, no Robotics Factory): output at full power, cost, time and the energy they need', '',
         '| L | Metal /h | Metal cost (m/c) | Metal time | Crystal /h | Crystal cost (m/c) | Deuterium /h | Deuterium cost (m/c) | energy: metal or crystal mine | energy: synthesizer | Solar Plant makes | Solar Plant cost (m/c) |',
         '|---|---|---|---|---|---|---|---|---|---|---|---|']
    plain = Sim(PRESETS[0])
    for l in (1, 2, 3, 5, 8, 10, 12, 15, 18, 20):
        cost = {k: grown_cost(*BUILD[k], l) for k in MINES + ('solar',)}
        L.append(f"| {l} | {MINE_OUTPUT['metal'][l]:.0f} | {fmt_cost(cost['metal'][:2])} | {fmt_t(plain.b_time('metal', l))} | "
                 f"{MINE_OUTPUT['crystal'][l]:.0f} | {fmt_cost(cost['crystal'][:2])} | {MINE_OUTPUT['deut'][l]:.0f} | {fmt_cost(cost['deut'][:2])} | "
                 f"{MINE_ENERGY['metal'][l]} | {MINE_ENERGY['deut'][l]} | {SOLAR_ENERGY[l]} | {fmt_cost(cost['solar'][:2])} |")
    L.append('')
    L.append(f'Natural production, with no mine and no energy: {NATURAL[0]:g} metal and {NATURAL[1]:g} crystal an hour.')
    L.append('')
    L += ['### What powering the mines costs: the three mines at one level each', '',
          '| mines at | mines cost (all levels, m+c+d) | energy needed | Solar Plant level that covers it | Solar Plant cost to there | Solar Satellites that cover it | their cost |',
          '|---|---|---|---|---|---|---|']
    def total(name, top):
        return sum(sum(grown_cost(*BUILD[name], l)) for l in range(1, top + 1))
    for l in (5, 10, 15, 20):
        need = energy_needed(dict.fromkeys(MINES, l))
        plant = next((s for s in range(MAX_LEVEL + 1) if SOLAR_ENERGY[s] >= need), None)
        sats = math.ceil(need / SATELLITE_ENERGY)
        L.append(f"| {l} | {sum(total(k, l) for k in MINES):.0f} | {need} | {plant if plant is not None else f'none ({MAX_LEVEL} makes {SOLAR_ENERGY[MAX_LEVEL]})'} | "
                 f"{'-' if plant is None else '%.0f' % total('solar', plant)} | {sats} | {sats * sum(SHIPS['satellite']['cost']):.0f} |")
    L.append('')
    L += ['### Facilities by level (pace 1): cost (m/c/d) and time with nothing behind it', '',
          '| L | Robotics Factory | time | Shipyard | time | Research Lab | time |', '|---|---|---|---|---|---|---|']
    for l in (1, 2, 4, 6, 8, 10, 12):
        cells = []
        for name in ('robotics', 'shipyard', 'lab'):
            cells += [fmt_cost(grown_cost(*BUILD[name], l)), fmt_t(plain.b_time(name, l))]
        L.append(f'| {l} | ' + ' | '.join(cells) + ' |')
    L.append('')
    return '\n'.join(L)

def estimates_table(results):
    """The two estimates a world announces, for every preset of the game, in seconds."""
    L = ['| preset | pace | first_cruiser_s | endgame_s | first Cruiser | endgame kit |', '|---|---|---|---|---|---|']
    def secs(x):
        return '-' if x is None else '%d' % round(x)
    for p in PRESETS:
        s = results.get((p.name, 'near'))
        if not s: continue
        c, e = s.milestones.get('first_cruiser'), s.milestones.get('endgame')
        L.append(f'| {p.name} | {p.P:g} | {secs(c)} | {secs(e)} | {fmt_t(c)} | {fmt_t(e)} |')
    return '\n'.join(L)

def milestone_table(results, mode='near'):
    names = [p.name for p in PRESETS if (p.name, mode) in results]
    L = ['| Milestone | ' + ' | '.join(names) + ' |', '|---|' + '---|' * len(names)]
    for key, label in MILESTONES:
        L.append('| ' + ' | '.join([label] + [fmt_t(results[(nm, mode)].milestones.get(key)) for nm in names]) + ' |')
    return '\n'.join(L)

BALANCE_B = ('metal', 'crystal', 'deut', 'solar', 'robotics', 'shipyard', 'lab')
BALANCE_S = ('light_fighter', 'heavy_fighter', 'cruiser', 'battleship', 'small_cargo', 'satellite')
def balance_table(s, n=10):
    L = [f'**{s.p.name} / {s.mode}**', '',
         '| time | metal | crystal | deut | cap m/c/d | prod/h m+c+d (real) | energy made/needed | levels ' + '/'.join(LABEL[k].split()[0] for k in BALANCE_B)
         + ' | ships ' + '/'.join(''.join(w[0] for w in LABEL[k].split()) for k in BALANCE_S) + ' | defs | slots | econ pts |',
         '|---|---|---|---|---|---|---|---|---|---|---|---|']
    S = s.samples
    idxs = sorted(set(int(i * (len(S) - 1) / n) for i in range(n + 1)))
    for i in idxs:
        x = S[i]
        L.append(f"| {fmt_t(x['t'])} | {x['res'][0]:.0f} | {x['res'][1]:.0f} | {x['res'][2]:.0f} | "
                 f"{fmt_cost(x['cap'])} | {sum(x['prod']):.0f} | {x['energy'][0]}/{x['energy'][1]} | "
                 + '/'.join(str(x['b'].get(k, 0)) for k in BALANCE_B) + ' | '
                 + '/'.join(str(x['ships'].get(k, 0)) for k in BALANCE_S)
                 + f" | {sum(x['defs'].values())} | {x['slots']} | {x['econ_pts']:.1f} |")
    L.append('')
    return '\n'.join(L)

def levels_text(levels, names):
    return ', '.join(f'{LABEL[k]} {levels[k]}' for k in names if levels.get(k)) or '-'
def counts_text(counts, names):
    return ', '.join(f'{counts[k]} {LABEL[k]}' for k in names if counts.get(k)) or '-'

def owns_table(s):
    """What the first run of a preset owned at each of its marks, in full."""
    marks = sorted(s.owned)
    L = [f'**{s.p.name} (x{s.P:g}) / {s.mode}**', '',
         '| | ' + ' | '.join(f'at {fmt_t(t)} ({t * s.P / 86400:.0f} game days)' for t in marks) + ' |', '|---|' + '---|' * len(marks)]
    def row(label, cell):
        L.append(f'| {label} | ' + ' | '.join(cell(s.owned[t]) for t in marks) + ' |')
    row('mines (metal / crystal / deuterium)', lambda x: '/'.join(str(x['b'].get(k, 0)) for k in MINES))
    row('Solar Plant / Fusion Reactor / Solar Satellites', lambda x: f"{x['b'].get('solar', 0)} / {x['b'].get('fusion', 0)} / {x['ships'].get('satellite', 0)}")
    row('energy made / needed', lambda x: f"{x['energy'][0]} / {x['energy'][1]}")
    row('Robotics Factory / Nanite Factory / Shipyard / Research Lab', lambda x: ' / '.join(str(x['b'].get(k, 0)) for k in ('robotics', 'nanite', 'shipyard', 'lab')))
    row('Metal Storage / Crystal Storage / Deuterium Tank', lambda x: ' / '.join(str(x['b'].get(k, 0)) for k in STORES))
    row('building slots', lambda x: str(x['slots']))
    row('research', lambda x: levels_text(x['r'], R_NAME))
    row('ships', lambda x: counts_text(x['ships'], S_NAME))
    row('defences', lambda x: counts_text(x['defs'], D_NAME))
    row('stock (m/c/d)', lambda x: fmt_cost(x['res']))
    row('caps (m/c/d)', lambda x: fmt_cost(x['cap']))
    row('mines make per real hour (m/c/d)', lambda x: fmt_cost(x['prod']))
    row('warships\' power', lambda x: f"{x['fleet_power']:.0f}")
    row('deepest ring whose group they outnumber 3 to 1 / ring in reach', lambda x: f"{x['beats']} / {x['reach']}")
    row('home defence power (docked warships, satellites, defences)', lambda x: f"{x['def_power']:.0f}")
    row('the next raid, weakest to strongest', lambda x: f"{x['raid'][0]:.0f} to {x['raid'][1]:.0f}" + ('' if x['raided'] else ' (not raided yet)'))
    row('score: buildings and research + fleet + defences', lambda x: f"{x['econ_pts']:.0f} + {x['fleet_pts']:.0f} + {x['def_pts']:.0f}")
    row('score', lambda x: f"{x['score']:.0f}")
    L.append('')
    return '\n'.join(L)

OWNS_R = ('computer', 'energy', 'weapons', 'shielding', 'armour', 'combustion', 'impulse', 'hyperdrive', 'laser')
def owns_summary(results, mode='near'):
    """One line for each preset and mark."""
    L = ['| preset | at | game days | mines M/C/D | Solar / satellites | energy made/needed | Robotics/Shipyard/Lab | slots | research '
         + '/'.join(''.join(w[0] for w in LABEL[k].split()) for k in OWNS_R) + ' | warships LF/HF/CR/BS | Small Cargo | defences | warships\' power | home defence | strongest raid | score |',
         '|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|']
    for p in PRESETS:
        s = results.get((p.name, mode))
        if not s: continue
        for t in sorted(s.owned):
            x = s.owned[t]
            L.append(f"| {p.name} | {fmt_t(t)} | {t * p.P / 86400:.0f} | " + '/'.join(str(x['b'].get(k, 0)) for k in MINES)
                     + f" | {x['b'].get('solar', 0)} / {x['ships'].get('satellite', 0)} | {x['energy'][0]}/{x['energy'][1]} | "
                     + '/'.join(str(x['b'].get(k, 0)) for k in ('robotics', 'shipyard', 'lab')) + f" | {x['slots']} | "
                     + '/'.join(str(x['r'].get(k, 0)) for k in OWNS_R) + ' | '
                     + '/'.join(str(x['ships'].get(k, 0)) for k in ('light_fighter', 'heavy_fighter', 'cruiser', 'battleship'))
                     + f" | {x['ships'].get('small_cargo', 0)} | {sum(x['defs'].values())} | {x['fleet_power']:.0f} | {x['def_power']:.0f} | "
                     f"{x['raid'][1]:.0f} | {x['score']:.0f} |")
    return '\n'.join(L)

def income_table(results, mode):
    L = ['| preset | buildings | ore | salvage | derelict | fuel cost | losses | bldg share | pilot share |', '|---|---|---|---|---|---|---|---|---|']
    for p in PRESETS:
        s = results.get((p.name, mode))
        if not s: continue
        i = s.income
        bld = i['buildings']; ore = i['ore']; sal = i['salv']; der = i['der']
        fuel = -i['fuel_cost']
        tot = bld + ore + sal + der
        pil = ore + sal + der
        L.append(f'| {p.name} | {bld:.3g} | {ore:.3g} | {sal:.3g} | {der:.3g} | {fuel:.3g} | {s.loss_value:.3g} | '
                 f'{100*bld/tot:.0f}% | {100*pil/tot:.0f}% |' if tot else f'| {p.name} | - |')
    return '\n'.join(L)

def storage_table(results, mode='near'):
    L = ['| preset | time at >90% of a cap | time at a cap | wasted units (m/c/d) | wasted vs produced | time with the mines short of energy |', '|---|---|---|---|---|---|']
    for p in PRESETS:
        s = results.get((p.name, mode))
        if not s: continue
        tot = s.t
        prod_total = s.income['buildings'] + s.income['ore'] + s.income['salv'] + s.income['der']
        w = sum(s.waste)
        L.append(f'| {p.name} | {100*s.cap_pressure_time/tot:.1f}% | {100*s.cap_full_time/tot:.1f}% | '
                 f'{s.waste[0]:.3g}/{s.waste[1]:.3g}/{s.waste[2]:.3g} | {100*w/max(prod_total,1):.2f}% | {100*s.short_power_time/tot:.1f}% |')
    return '\n'.join(L)

def ring_income_table(results, preset_name, mode):
    s = results.get((preset_name, mode))
    L = [f'**{preset_name} / {mode}**: cumulative pilot income by ring (all resources), versus building income over the run', '',
         '| ring band | ore | salvage | derelict | total | share of all income |', '|---|---|---|---|---|---|']
    tot_all = sum(s.income[k] for k in ('buildings', 'ore', 'salv', 'der'))
    bands = [(1, 4), (5, 8), (9, 14), (15, 20), (21, 40)]
    for lo, hi in bands:
        o = sum(v for (k, d), v in s.income_by_ring.items() if k == 'ore' and lo <= d <= hi)
        sv = sum(v for (k, d), v in s.income_by_ring.items() if k == 'salv' and lo <= d <= hi)
        dv = sum(v for (k, d), v in s.income_by_ring.items() if k == 'der' and lo <= d <= hi)
        t = o + sv + dv
        L.append(f'| {lo}-{hi} | {o:.3g} | {sv:.3g} | {dv:.3g} | {t:.3g} | {100*t/tot_all:.1f}% |')
    L.append(f'| buildings | | | | {s.income["buildings"]:.3g} | {100*s.income["buildings"]/tot_all:.1f}% |')
    L.append('')
    return '\n'.join(L)

def compare_modes(results):
    L = ['| preset | mode | first Small Cargo | Metal 10 | first Cruiser | endgame | pilot share of income | losses (value) | final econ+fleet pts |',
         '|---|---|---|---|---|---|---|---|---|']
    for p in PRESETS:
        for m in ('none', 'near', 'far'):
            s = results.get((p.name, m))
            if not s: continue
            i = s.income
            tot = i['buildings'] + i['ore'] + i['salv'] + i['der']
            pil = i['ore'] + i['salv'] + i['der']
            L.append(f"| {p.name} | {m} | {fmt_t(s.milestones.get('first_small_cargo'))} | {fmt_t(s.milestones.get('metal10'))} | "
                     f"{fmt_t(s.milestones.get('first_cruiser'))} | {fmt_t(s.milestones.get('endgame'))} | {100*pil/tot:.0f}% | "
                     f"{s.loss_value:.3g} | {s.econ_pts+s.fleet_pts+s.def_pts:.0f} |")
    return '\n'.join(L)

def pace_table(results):
    ref = results.get(('persistent', 'near'))
    if not ref: return ''
    m = ref.milestones
    E, C = m.get('endgame'), m.get('first_cruiser')
    if E is None or C is None: return ''
    L = ['| world arc you want | pace P needed | first Cruiser | endgame kit |', '|---|---|---|---|']
    rows = [('persistent baseline', 1),
            ('endgame in 30 days', E / 86400 / 30), ('endgame in 14 days', E / 86400 / 14),
            ('endgame in 7 days (season)', E / 86400 / 7), ('endgame in 3 days', E / 86400 / 3),
            ('endgame in 24 h', E / 86400 / 1), ('Cruiser at 45 min (blitz)', C / 2700.0),
            ('endgame in 15 min (test)', E / 900.0)]
    for name, P in rows:
        L.append(f'| {name} | x{P:.0f} | {fmt_t(C / P)} | {fmt_t(E / P)} |')
    return '\n'.join(L)

def score_table(results):
    L = ['| preset | econ pts (build+research) | fleet pts | defence pts | total |', '|---|---|---|---|---|']
    for p in PRESETS:
        s = results.get((p.name, 'near'))
        if not s: continue
        s0 = s.runs[0]
        L.append(f'| {p.name} | {s0.econ_pts:.0f} | {s0.fleet_pts:.0f} | {s0.def_pts:.0f} | {s0.score():.0f} |')
    return '\n'.join(L)

def ring_snapshot_table(s, label, when):
    S = s.samples
    x = S[min(len(S) - 1, int(when * (len(S) - 1)))]
    if not x.get('rings'):
        return ''
    L = [f'**{label}** at {fmt_t(x["t"])}: pilot yield per real hour by ring (all resources) against building income {sum(x["prod"]):.0f}/h',
         '', '| ring | threat | ore /h | salvage /h | derelict /h | total /h | vs buildings |', '|---|---|---|---|---|---|---|']
    bands = [(3, 6), (7, 8), (9, 11), (12, 14), (15, 20), (21, 40)]
    bld = sum(x['prod'])
    for lo, hi in bands:
        o = sum(v for d, v in x['rings']['ore'].items() if lo <= d <= hi)
        sv = sum(v for d, v in x['rings']['salv'].items() if lo <= d <= hi)
        dv = sum(v for d, v in x['rings']['der'].items() if lo <= d <= hi)
        t = o + sv + dv
        L.append(f'| {lo}-{hi} | T{threat(lo)}-T{threat(hi)} | {o:.0f} | {sv:.0f} | {dv:.0f} | {t:.0f} | {100*t/bld:.0f}% |')
    L.append('')
    return '\n'.join(L)

def day_summary(s, title):
    """Per-day list of what completed: new building levels (max per building), research, ships, defences."""
    L = [f'**{title}**', '', '| day | buildings reached | research done | ships/defences finished | score |', '|---|---|---|---|---|']
    days = {}
    for t, k, tx in s.log:
        days.setdefault(int(t // 86400) + 1, []).append((k, tx))
    nd = max(days) if days else 0
    for d in range(1, nd + 1):
        ev = days.get(d, [])
        bl = {}; rs = {}; sh = {}
        for k, tx in ev:
            if k == 's':
                sh[tx] = sh.get(tx, 0) + 1
            else:
                n, l = tx.split()
                (bl if k == 'b' else rs)[n] = int(l)
        sc = '-'
        for t, scr in s.daily:
            if int(t // 86400) == d:
                sc = f'{scr:.0f}'
        L.append(f"| {d} | {levels_text(bl, bl)} | {levels_text(rs, rs)} | {counts_text(sh, sh)} | {sc} |")
    return '\n'.join(L)

def full_report(results, presets):
    out = []
    out.append('### The player and the endgame kit\n')
    out.append(player_text())
    out.append('\n### The presets\' estimates, in seconds (competent player, median of 3 session-schedule offsets)\n')
    out.append(estimates_table(results))
    out.append('\n### Milestones, competent player (mode `near`); median of 3 session-schedule offsets\n')
    out.append(milestone_table(results, 'near'))
    out.append('\n### Milestones, greedy far-mining player (mode `far`); one session schedule\n')
    out.append(milestone_table(results, 'far'))
    out.append('\n### Milestones, buildings-only player (mode `none`, no piloting at all); one session schedule\n')
    out.append(milestone_table(results, 'none'))
    out.append('\n### Pace needed for a target arc (derived from the persistent run)\n')
    out.append(pace_table(results))
    out.append('\n### Mode comparison: none = buildings only, near = competent, far = greedy far-mining\n')
    out.append(compare_modes(results))
    out.append('\n### Income split over the whole run, competent (near)\n')
    out.append(income_table(results, 'near'))
    out.append('\n### Income split over the whole run, greedy far-mining\n')
    out.append(income_table(results, 'far'))
    out.append('\n### Storage-cap pressure and energy (competent)\n')
    out.append(storage_table(results, 'near'))
    out.append('\n### Score at end of run (competent)\n')
    out.append(score_table(results))
    out.append('\n### What a competent player owns: every preset at the game time of day 3 and day 7 of a season, and at those real days while a world is younger\n')
    out.append(owns_summary(results, 'near'))
    for pn in ('season', 'persistent', 'blitz'):
        if (pn, 'near') in results:
            out.append('\n### What a competent player owns, in full: ' + pn + '\n')
            out.append(owns_table(results[(pn, 'near')].runs[0]))
    if ('season', 'near') in results:
        out.append('\n### Season: what completes each day (competent player)\n')
        out.append(day_summary(results[('season', 'near')].runs[0], 'season / near'))
    for pn in ('persistent', 'season', 'blitz'):
        if (pn, 'near') in results:
            out.append('\n### Resource balance over time: ' + pn + '\n')
            out.append(balance_table(results[(pn, 'near')].runs[0], 10))
    for pn in ('persistent',):
        if (pn, 'near') in results:
            out.append('\n### Where pilot income comes from, by distance (' + pn + ')\n')
            for m, nm in (('near', 'competent'), ('far', 'greedy far-mining')):
                if (pn, m) in results:
                    for when in (0.25, 0.5):
                        out.append(ring_snapshot_table(results[(pn, m)].runs[0], nm, when))
                    out.append(ring_income_table(results, pn, m))
    return '\n'.join(out)

def long_run():
    """Persistent world for a year with the same competent player, not stopping at the endgame kit."""
    p = Preset('persistent', PACE['persistent'], 600.0, 24 * 365, 4, 12)
    s = Sim(p, mode='near')
    end = p.horizon_h * 3600.0
    marks = [d * 86400.0 for d in (30, 60, 74, 90, 120, 150, 180, 240, 300, 365)]
    L = ['| day | Metal/Crystal/Deut | Solar / satellites | Robotics | Shipyard | Lab | stores m/c/d | Cruisers | Battleships | defences | score | production /h |',
         '|---|---|---|---|---|---|---|---|---|---|---|---|']
    mi = 0
    while s.t < end and mi < len(marks):
        s.step(p.dt)
        if s.t >= marks[mi]:
            b = s.b
            L.append(f"| {marks[mi]/86400:.0f} | {b['metal']}/{b['crystal']}/{b['deut']} | {b['solar']} / {s.ships['satellite']} | {b['robotics']} | {b['shipyard']} | {b['lab']} | "
                     + '/'.join(str(b[n]) for n in STORES)
                     + f" | {s.ships['cruiser']} | {s.ships['battleship']} | {sum(s.defs.values())} | {s.score():.0f} | {sum(s.prod_h()):.0f} |")
            mi += 1
    return '\n'.join(L)

def sensitivity():
    L = ['| world | sessions/day x minutes | first Small Cargo | slot 2 | first Cruiser | first Battleship | endgame | wasted production |', '|---|---|---|---|---|---|---|---|']
    cases = [('persistent', [(1, 15), (2, 15), (4, 12), (8, 12)]), ('season', [(2, 15), (4, 15), (8, 15), (16, 15)])]
    for name, ss in cases:
        dt, hz, _, _ = SCHEDULE[name]
        for ns, ml in ss:
            p = Preset(name, PACE[name], dt, hz, ns, ml)
            r = run_all(('near',), [p])[(name, 'near')]
            s0 = r.runs[0]
            prod = s0.income['buildings'] + s0.income['ore'] + s0.income['salv'] + s0.income['der']
            L.append(f"| {name} | {ns} x {ml} min | " + ' | '.join(fmt_t(r.milestones.get(k)) for k in
                     ('first_small_cargo', 'slot2', 'first_cruiser', 'first_battleship', 'endgame')) +
                     f" | {100*sum(s0.waste)/prod:.1f}% |")
    return '\n'.join(L)

COMPARE_MODES = (('none', 'builder'), ('explore', 'explorer'), ('raid', 'raider'), ('near', 'mixed'))

def compare_scores(cases=(('blitz', 3600.0), ('season', 7 * 86400.0)), phases=(0.0, 6.0, 12.0)):
    """Score at a fixed time (the arc a player is judged on) for the four play styles, with what the field paid
    them: wreckage, derelicts and charts, each in the points it buys once built with."""
    L = ['| world | style | score | wreckage | derelicts | charts | vs builder |', '|---|---|---|---|---|---|---|']
    for pname, until in cases:
        p = next(x for x in PRESETS if x.name == pname)
        rows = {}
        for mode, label in COMPARE_MODES:
            acc = []
            # season: three session-schedule offsets; blitz has none, so average the
            # score over the last 20 percent of the hour (levels land in steps)
            runs = [(ph, until) for ph in phases] if not p.continuous else [(0.0, until * f) for f in (0.9, 1.0, 1.1)]
            for ph, stop in runs:
                sm = Sim(p, mode=mode, phase_h=ph)
                while sm.t < stop:
                    sm.step(p.dt)
                paid = [sm.income[src] * LOOT_WEIGHT / 1000.0 for src in ('salv', 'der', 'charts')]
                acc.append((sm.score(), *paid))
            n = len(acc)
            rows[label] = tuple(sum(a[i] for a in acc) / n for i in range(4))
        base = rows['builder'][0]
        for _, label in COMPARE_MODES:
            score, salv, der, charts = rows[label]
            L.append(f"| {pname} | {label} | {score:.0f} | {salv:.0f} | {der:.0f} | {charts:.0f} | {100 * score / base:.0f}% |")
    return '\n'.join(L)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--compare', action='store_true', help='builder / explorer / raider / mixed score at 1 h blitz and 7 d season')
    ap.add_argument('--preset')
    ap.add_argument('--pace', type=float, help='custom pace multiplier (adds a preset named x<pace>)')
    ap.add_argument('--horizon-h', type=float, help='horizon in real hours for --pace')
    ap.add_argument('--tables', action='store_true')
    ap.add_argument('--sensitivity', action='store_true')
    ap.add_argument('--longrun', action='store_true')
    ap.add_argument('--modes', default='none,near,far')
    ap.add_argument('--set', action='append', default=[], help='override module constant NAME=python-expr')
    ap.add_argument('--brief', action='store_true')
    a = ap.parse_args()
    for kv in a.set:
        k, v = kv.split('=', 1)
        globals()[k] = eval(v, globals())
    if a.compare:
        print(compare_scores()); return
    if a.longrun:
        print(long_run()); return
    if a.sensitivity:
        print(sensitivity()); return
    if a.tables:
        for table in (static_tables, prod_tables, time_table, storage_cap_table, flight_table, fuel_table, defence_table):
            print(table())
        print('### The presets\' estimates, in seconds (competent player, median of 3 session-schedule offsets)\n')
        print(f'The endgame kit: {kit_text()}.\n')
        print(estimates_table(run_all(('near',))))
        return
    presets = [p for p in PRESETS if not a.preset or p.name == a.preset]
    if a.pace:
        hz = a.horizon_h or max(0.5, 24 * 150 / a.pace * 1.5)
        dt = max(0.25, min(300.0, 3600.0 / a.pace * 2))
        cont = a.pace >= 300
        presets = [Preset('x%g' % a.pace, a.pace, dt, hz, 0 if cont else 6, 0 if cont else 15, continuous=cont)]
        PRESETS[:] = presets
    modes = a.modes.split(',')
    results = run_all(modes, presets)
    if a.brief:
        print(milestone_table(results, modes[0]));
        if len(modes) > 1: print(compare_modes(results))
        return
    print(full_report(results, presets))

if __name__ == '__main__':
    main()
