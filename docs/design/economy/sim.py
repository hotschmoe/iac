#!/usr/bin/env python3
"""
IAC economy / pacing / exploration simulation (Python stdlib only).

Simulates one scripted "competent player" per pacing preset using the formulas
proposed in spec.md.  Every constant below is a constant in the spec (Appendix A);
if you change one, change the other.

    python3 sim.py                    # full markdown report (about 25 s)
    python3 sim.py --preset season-7d # one preset
    python3 sim.py --pace 25          # any pace (custom preset)
    python3 sim.py --tables           # static formula tables only
    python3 sim.py --sensitivity      # how session frequency changes the arcs
    python3 sim.py --longrun          # persistent world for a year
    python3 sim.py --modes none,near,far --brief
    python3 sim.py --set BETA=0.4     # override any module constant for experiments

Player modes: none = buildings only, near = competent (engage at 3x power, mine the
inner ring, hunt and board up to ring 20), far = greedy (engage at 1.5x, mine the
farthest ring whose net value is within 80% of the best).  Milestone times are the
median of three session-schedule offsets.  Piloting is modelled as expected values
(ore refill supply, respawn supply, active-time caps, attrition, refuel cost), not
as individual fights.  The planner is heuristic: expect about +-15% run-to-run noise.
"""
import math, sys, argparse
from collections import defaultdict

# =====================================================================
# 1. PROPOSED CONSTANTS (pace-1 baseline: the "persistent world")
#    Times below are GAME hours at pace 1.  Real time = game time / P.
# =====================================================================
PROD_BASE = {'metal': 10.0, 'crystal': 7.0, 'deut': 5.0}   # per game-hour * L * 1.1^L
PROD_GROWTH = 1.10

# (L1 cost m,c,d), cost growth per level, max level
BUILD = {
    'metal':    ((60, 15, 0),   1.50),
    'crystal':  ((48, 24, 0),   1.60),
    'deut':     ((150, 50, 0),  1.50),
    'shipyard': ((200, 100, 50), 1.80),
    'lab':      ((100, 200, 50), 1.80),
    'depot':    ((150, 50, 100), 1.70),
    'sensor':   ((100, 150, 75), 1.70),
    'grid':     ((300, 200, 100), 1.80),
    'vault':    ((200, 100, 0),  1.70),
    'fab':      ((120, 80, 40),  1.90),
}
BUILD_PREREQ = {
    'shipyard': [('metal', 2)], 'lab': [('crystal', 2)], 'depot': [('deut', 2)],
    'sensor': [('lab', 1)], 'grid': [('shipyard', 3)], 'vault': [('metal', 2)],
    'fab': [('shipyard', 2)],
}
BUILD_RATE = 700.0     # (metal+crystal) of build cost per game-hour of build time
FAB_SPEED = 0.15       # build time divisor 1 + 0.15 * fab
RES_RATE = 500.0       # research: (m+c) per game-hour
LAB_SPEED = 0.10       # research time divisor 1 + 0.10 * lab
SHIP_RATE = 1200.0     # ships: (m+c) per game-hour per ship
SY_SPEED = 0.10        # ship time divisor 1 + 0.10 * shipyard

RESEARCH = {   # L1 cost, growth, max, prereqs (b=building, r=research)
    'corvette': ((400, 200, 100),  1.0, 1, [('b', 'shipyard', 1)]),   # decisions.md: Corvettes unlock at Shipyard 1
    'hauler':   ((600, 300, 200),  1.0, 1, [('b', 'shipyard', 3)]),
    'frigate':  ((2000, 1200, 600), 1.0, 1, [('r', 'corvette', 1), ('b', 'shipyard', 4)]),
    'cruiser':  ((8000, 5000, 2500), 1.0, 1, [('r', 'frigate', 1), ('b', 'shipyard', 6)]),
    'fuel_eff': ((200, 100, 100),  1.6, 5, [('b', 'deut', 3)]),
    'tanks':    ((150, 75, 150),   1.6, 5, [('b', 'depot', 2)]),
    'hulls':    ((300, 100, 0),    1.6, 5, [('b', 'shipyard', 2)]),
    'shields':  ((100, 300, 50),   1.6, 5, [('b', 'lab', 3)]),
    'weapons':  ((200, 200, 100),  1.6, 5, [('b', 'lab', 3)]),
    'nav':      ((100, 150, 50),   1.6, 5, [('b', 'lab', 2)]),
    'harvest':  ((150, 100, 75),   1.6, 5, [('b', 'lab', 2)]),
    'modfab':   ((1500, 1200, 500), 2.0, 2, [('b', 'lab', 4)]),     # NEW: build slots
}

SHIPS = {  # cost, hull, shield, weapon, cargo, fuel
    'scout':    ((200, 50, 30),    30, 10, 5, 20, 60),
    'corvette': ((400, 100, 60),   50, 20, 15, 10, 50),
    'frigate':  ((1000, 400, 200), 120, 60, 30, 30, 80),
    'cruiser':  ((3000, 1500, 800), 200, 100, 80, 50, 120),
    'hauler':   ((600, 200, 150),  80, 20, 5, 200, 100),
}
SHIP_TECH = {'corvette': 'corvette', 'frigate': 'frigate', 'cruiser': 'cruiser', 'hauler': 'hauler'}

DEFS = {   # NEW defence structures: cost, power(weapon+(hull+shield)/10), prereq
    'turret':  ((120, 20, 0),    12 + (40 + 10) / 10,   {'grid': 1}),
    'lancer':  ((400, 120, 20),  35 + (120 + 40) / 10,  {'grid': 2, 'weapons': 2}),
    'bastion': ((1200, 500, 100), 90 + (300 + 150) / 10, {'grid': 4, 'weapons': 4, 'shields': 3}),
}
GRID_VIRTUAL_UNITS = {0: 0, 1: 1.0, 2: 2.5, 3: 4.0}   # x scout power (9); halved vs today

START_RES = (500.0, 300.0, 100.0)
DAILY_EVERY_S = 86400.0
# the "endgame kit" used as the end of the arc in the simulation
KIT_B = dict(metal=19, crystal=16, deut=13, shipyard=11, lab=11, fab=8, grid=5)
KIT_R = dict(modfab=2, cruiser=1, fuel_eff=3, tanks=3, weapons=4, shields=4, hulls=3)
KIT_CRUISERS = 12
# storage
VAULT_BASE_CAP = {'metal': 5000.0, 'crystal': 3500.0, 'deut': 2500.0}
VAULT_GROWTH = 1.5
CAP_PACE_EXP = 0.75          # cap multiplier = P ** this up to CAP_KNEE ...
CAP_KNEE = 10.0              # ... and 10**0.75 * (P / 10) ** CAP_EXP_HIGH above it
CAP_EXP_HIGH = 0.25
def cap_mult(P):
    if P <= CAP_KNEE:
        return P ** CAP_PACE_EXP
    return CAP_KNEE ** CAP_PACE_EXP * (P / CAP_KNEE) ** CAP_EXP_HIGH

# pilot-economy scale: ore/loot/salvage values and fuel price scale with P ** BETA
BETA = 0.5

# NPC / threat
def npc_power(d):            # smooth: ~1.22x per ring step
    return 4.0 * 1.22 ** (d - 1)
def threat(d):
    return max(1, min(9, 1 + int(math.floor(math.log2(npc_power(d) / 4.0)))))
def npc_class(d):
    return 'scout' if d <= 5 else 'corvette' if d <= 12 else 'frigate' if d <= 20 else 'cruiser'
def ship_power(c):
    s = SHIPS[c]
    return s[3] + (s[1] + s[2]) / 10.0
def ship_cost_tot(c):
    return sum(SHIPS[c][0])
def npc_cost(d):             # build-cost value (units) of the NPC group at ring d
    c = npc_class(d)
    return ship_cost_tot(c) / ship_power(c) * npc_power(d)
def npc_presence(d):
    return max(0.25, min(0.85, 0.25 + 0.025 * d))
def npc_passive_frac(d):
    return max(0.0, 0.5 - 0.08 * max(0, d - 8))
def npc_respawn_h(d):        # game hours
    return 2.0 + 0.25 * d
KILL_LOOT_K, KILL_LOOT_EXP = 1.2, 0.80
def kill_loot_value(d):      # total resource value of one kill's wreck (units)
    return KILL_LOOT_K * npc_power(d) ** KILL_LOOT_EXP

# ore
DENS_RESERVE = {'S': 3.0, 'M': 10.0, 'R': 22.0, 'P': 42.0}   # total reserve per density
ORE_RING_MULT = lambda d: 1.10 ** (min(d, 20) - 3) if d >= 3 else 1.0
ORE_REGEN_BASE_H, ORE_REGEN_PER_RING_H = 120.0, 24.0
def ore_regen_h(d):
    return ORE_REGEN_BASE_H + ORE_REGEN_PER_RING_H * d
def ring_coverage(d):         # share of a ring's sectors that are connected and inside a fleet's reach
    return 0.5 if d <= 8 else 0.4 if d <= 20 else 0.25      # game hours for a stripped tile to refill completely
def density_mix(d):
    dl = min(0.30, 0.012 * d)
    return {'S': 0.40 - dl, 'M': 0.30 - 0.2 * dl, 'R': 0.20 + 0.6 * dl, 'P': 0.10 + 0.6 * dl}
def mean_reserve(d):
    mix = density_mix(d)
    return sum(mix[k] * DENS_RESERVE[k] for k in mix)
def zone_terrain(d):
    if d <= 8:   return dict(ast=.55, neb=.15, deb=.10, emp=.20, ano=0.0), 30
    if d <= 20:  return dict(ast=.35, neb=.15, deb=.20, emp=.25, ano=.05), 40
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

# derelicts
DERELICT_PCT = 0.08
DERELICT_RESPAWN_H = 120.0
DERELICT_PACE_EXP = 0.5      # derelict respawn scales with P**0.5 (finds are not an economy timer)
def derelict_value(d):       # total loot value of one site (metal+crystal+deut units)
    return 240.0 * 1.14 ** (d - 4)
def derelict_tier(d):        # tier follows threat: T1-3 -> 1, T4-6 -> 2, T7-9 -> 3
    return (threat(d) + 2) // 3
def derelict_ambush(d):
    return min(0.35, 0.10 + 0.03 * (threat(d) - 1))
def derelict_sites_per_ring(d):
    t, _ = zone_terrain(d)
    return 6 * d * (t['deb'] + t['emp']) * DERELICT_PCT

FUEL_RATE = 0.1
FUEL_PRICE_DEUT = 0.08       # deuterium per fuel unit refuelled (x pilot scale)
HOME_DIST = 4

# combat expectation
def win_prob(r):
    return 1.0 / (1.0 + math.exp(-4.5 * math.log(max(r, 1e-3))))
def attrition(r):            # expected ship-loss value per unit of loot taken, at power ratio r = mine/theirs
    return min(10.0, 0.1 * (3.0 / max(r, 0.3)) ** 3)

# raids / defence
RAID_MIN_AGE_S = 86400.0     # 24 h at pace 1; scaled by P**0.75
def raid_power(econ_pts):
    return 5.0 + 11.0 * econ_pts ** 0.75

# score
W_RES = (1.0, 1.5, 2.0)
SCORE_EXTRA_CAP = 0.25     # combat+exploration points may add at most 25% of core points
KILL_SHARE = 1.0           # a kill pays this share of the weight of the pile it leaves
FULL_PAY_RATIO, MIN_PAY = 4.0, 0.05   # full pay up to 4x the NPC power, then 4/ratio down to 5%
BOARD_VALUE_SHARE = 0.5    # first boarding: share of the loot weight ...
DISCOVERY_POINTS = 1.5     # ... plus this * T^1.5
CHART_BASE_POINTS, CHART_THREAT_POINTS = 0.15, 0.10   # per delivered new sector: base + threat * T^1.5
RELIC_POINTS = 30.0
def relic_chance(t):
    return min(0.08, max(0.0, 0.02 * (t - 5)))
FARM_DECAY = 0.75          # each recent kill in a sector multiplies the next kill's loot and score by this
FARM_HALF_LIFE_RESPAWNS = 4.0
def farm_steady(u):
    """Mean loot/score share of a sector worked at utilisation u (kills taken / respawns offered)."""
    if u <= 0: return 1.0
    d = 0.5 ** (1.0 / (FARM_HALF_LIFE_RESPAWNS * u))
    h = d / (1.0 - d) if d < 1 else 1e9
    return FARM_DECAY ** h
def kill_pay(r):
    return max(MIN_PAY, min(1.0, FULL_PAY_RATIO / max(r, 1e-3)))
def chart_points(t):
    return CHART_BASE_POINTS + CHART_THREAT_POINTS * t ** 1.5
PILOT_SKILL = 0.5     # share of the ideal piloting tempo a skilled human or agent reaches (assumption; the live test checks it)
ATTENTION_PENALTY = 0.20   # building income lost by a player who spends all his attention in the field (scaled by his pilot share)
PILOT_SHARE = {'none': 0.0, 'near': 0.4, 'explore': 0.85, 'raid': 0.85, 'far': 0.85}   # share of attention spent piloting
CHART_RATE_PER_FLEET_H = 60.0   # new sectors a scouting fleet charts and delivers per active hour (assumption; live test checks it)
LOOT_WEIGHT = 0.55 * W_RES[0] + 0.30 * W_RES[1] + 0.15 * W_RES[2]   # weight per unit of loot

def wv(c):
    return (c[0] * W_RES[0] + c[1] * W_RES[1] + c[2] * W_RES[2]) / 1000.0

# =====================================================================
# 2. PACING PRESETS
# =====================================================================
class Preset:
    def __init__(self, name, P, dt, horizon_h, sessions, session_min, active_frac_extra=0.0,
                 continuous=False, latency=0.0):
        self.name, self.P, self.dt, self.horizon_h = name, P, dt, horizon_h
        self.sessions, self.session_min = sessions, session_min
        self.continuous = continuous
        self.latency = latency

PRESETS = [
    #       name         P       dt     horizon(h)  sessions/day  min  continuous
    Preset('persistent',  1.0,    300.0, 24 * 220,  4, 12),
    Preset('season-7d',   10.0,   30.0,  24 * 14,   8, 15),
    Preset('dev-100x',    100.0,  5.0,   24 * 3,    0, 0, continuous=True, latency=5.0),
    Preset('blitz-1h',    600.0,  1.0,   5.0,       0, 0, continuous=True, latency=5.0),
    Preset('test-8000x',  8000.0, 0.25,  0.6,       0, 0, continuous=True, latency=1.0),
]

# =====================================================================
# 3. SIMULATION
# =====================================================================
class Sim:
    def __init__(self, preset, mode='near', max_depth=3, pilot=True, phase_h=0.0):
        self.phase = phase_h * 3600.0
        self.p = preset
        self.P = preset.P
        self.mode = mode           # 'none' builder | 'near' mixed | 'explore' explorer | 'raid' raider | 'far' greedy far-miner
        self.t = 0.0               # real seconds
        self.res = list(START_RES)
        self.b = defaultdict(int); self.b['metal'] = 1; self.b['crystal'] = 1
        self.r = defaultdict(int)
        self.ships = defaultdict(int); self.ships['scout'] = 2
        self.defs = defaultdict(int)
        self.bact, self.bplan = [], []
        self.ract, self.rplan = None, []
        self.sact, self.splan = None, []
        self.depth = max_depth
        self.spent = {'b': 1.0, 'r': 1.0, 's': 1.0}
        self.share = {'b': 0.55, 'r': 0.15, 's': 0.30}
        if mode in ('explore', 'raid', 'far'):
            self.share = {'b': 0.40, 'r': 0.10, 's': 0.50}   # fleet first: ships instead of mines
        self.charted = 3 * HOME_DIST * (HOME_DIST + 1)       # sectors within the sensors at start
        self.boarded = defaultdict(float)                    # first boardings per ring
        self.milestones = {}
        self.income = defaultdict(float)     # source -> units (value-weighted by raw sum)
        self.income_by_ring = defaultdict(float)
        self.waste = [0.0, 0.0, 0.0]
        self.cap_pressure_time = 0.0         # seconds with any stock > 90% cap
        self.cap_full_time = 0.0
        self.loss_value = 0.0
        self.econ_pts = 0.0; self.fleet_pts = 0.0; self.def_pts = 0.0
        self.combat_raw = 0.0; self.explore_raw = 0.0
        self.samples = []
        self.next_sample = 0.0
        self.pilot_rate = None; self.pilot_dirty = True; self.pilot_next = 0.0
        self.last_attend = -1e9
        self.attend_flag = True
        self.script_b = self.build_script()
        self.script_b_idx = 0
        self.ship_target_log = []
        self.reach = 0
        self.sample_times = []
        self.log = []          # (t, kind, text)
        self.daily = []

    # ------------------------------------------------------------------ helpers
    def slots(self):
        return 1 + self.r['modfab']
    def cap(self):
        s = cap_mult(self.P)
        g = VAULT_GROWTH ** self.b['vault']
        return [VAULT_BASE_CAP['metal'] * g * s, VAULT_BASE_CAP['crystal'] * g * s, VAULT_BASE_CAP['deut'] * g * s]
    def prod_h(self):   # per game hour
        return [PROD_BASE['metal'] * self.b['metal'] * PROD_GROWTH ** self.b['metal'] if self.b['metal'] else 0,
                PROD_BASE['crystal'] * self.b['crystal'] * PROD_GROWTH ** self.b['crystal'] if self.b['crystal'] else 0,
                PROD_BASE['deut'] * self.b['deut'] * PROD_GROWTH ** self.b['deut'] if self.b['deut'] else 0]
    def b_cost(self, name, L):
        base, g = BUILD[name]
        return tuple(x * g ** (L - 1) for x in base)
    def r_cost(self, name, L):
        base, g, _, _ = RESEARCH[name]
        return tuple(x * g ** (L - 1) for x in base)
    def b_time(self, name, L):   # real seconds
        c = self.b_cost(name, L)
        h = (c[0] + c[1]) / BUILD_RATE / (1 + FAB_SPEED * self.b['fab']) / self.P
        return max(1.0, h * 3600)
    def r_time(self, name, L):
        c = self.r_cost(name, L)
        h = (c[0] + c[1]) / RES_RATE / (1 + LAB_SPEED * self.b['lab']) / self.P
        return max(1.0, h * 3600)
    def s_time(self, cls):
        c = SHIPS[cls][0] if cls in SHIPS else DEFS[cls][0]
        h = (c[0] + c[1]) / SHIP_RATE / (1 + SY_SPEED * self.b['shipyard']) / self.P
        return max(1.0, h * 3600)
    def can_pay(self, c, reserve=(0, 0, 0)):
        return all(self.res[i] - reserve[i] >= c[i] - 1e-9 for i in range(3))
    def pay(self, c):
        for i in range(3):
            self.res[i] -= c[i]
    def hours(self):
        return self.t / 3600.0
    def mark(self, key):
        if key not in self.milestones:
            self.milestones[key] = self.t

    # ------------------------------------------------------------------ planning scripts
    def build_script(self):
        prof = dict(crystal=0.80, deut=0.66, shipyard=0.65, lab=0.55, fab=0.40, depot=0.25, sensor=0.20, grid=0.30)
        mins = dict(crystal=2, deut=1, shipyard=1, lab=1, fab=1, depot=1, sensor=1, grid=1)
        start = dict(metal=1, crystal=1)
        cur = defaultdict(int, start)
        out = []
        def ensure(name, target):
            target = min(target, 20)
            while cur[name] < target:
                cur[name] += 1
                out.append((name, cur[name]))
        for r in range(2, 31):
            ensure('metal', r)
            for nm in ('crystal', 'deut', 'lab', 'shipyard', 'fab', 'depot', 'sensor', 'grid'):
                if nm in ('depot', 'sensor', 'grid', 'fab') and r < 5:
                    continue
                ensure(nm, max(mins[nm], int(round(prof[nm] * r))))
        return out

    def research_order(self):
        return [('corvette', 1), ('hauler', 1), ('modfab', 1), ('fuel_eff', 1), ('harvest', 1), ('frigate', 1),
                ('tanks', 1), ('weapons', 1), ('cruiser', 1), ('hulls', 1), ('shields', 1), ('nav', 1),
                ('fuel_eff', 2), ('tanks', 2), ('harvest', 2), ('weapons', 2), ('shields', 2), ('hulls', 2),
                ('modfab', 2), ('fuel_eff', 3), ('tanks', 3), ('weapons', 3), ('shields', 3), ('hulls', 3),
                ('harvest', 3), ('nav', 2), ('weapons', 4), ('shields', 4), ('hulls', 4), ('tanks', 4),
                ('fuel_eff', 4), ('harvest', 4), ('weapons', 5), ('shields', 5), ('hulls', 5), ('tanks', 5),
                ('fuel_eff', 5), ('harvest', 5), ('nav', 3), ('nav', 4), ('nav', 5)]

    def b_prereq_ok(self, name, lv_map):
        for (n, l) in BUILD_PREREQ.get(name, []):
            if lv_map.get(n, 0) < l:
                return False
        return True
    def r_prereq_ok(self, name, blv, rlv):
        for kind, n, l in RESEARCH[name][3]:
            if kind == 'b' and blv.get(n, 0) < l: return False
            if kind == 'r' and rlv.get(n, 0) < l: return False
        return RESEARCH_LAB_OK(blv)

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

    def want_vault(self):
        c = self.cap()
        pb = self.proj_b()
        if pb.get('vault', 0) >= 14: return False
        hi = max(self.res[i] / c[i] for i in range(3))
        if hi > 0.75 and pb.get('vault', 0) <= self.b['vault'] + 0:
            return True
        # next scripted item bigger than 85% of cap
        nxt = self.next_scripted_building()
        if nxt:
            cc = self.b_cost(*nxt)
            if any(cc[i] > 0.85 * c[i] for i in range(3)) and pb.get('vault', 0) <= self.b['vault']:
                return True
        return False

    def next_scripted_building(self):
        pb = self.proj_b()
        for (n, l) in self.script_b:
            if pb.get(n, 0) < l:
                if pb.get(n, 0) == l - 1 and self.b_prereq_ok(n, pb):
                    return (n, l)
        return None

    def scarce_mine(self, pb):
        ph = self.prod_h()
        names = ('metal', 'crystal', 'deut')
        hrs = [self.res[i] / max(ph[i], 5.0) for i in range(3)]
        lo = min(range(3), key=lambda i: hrs[i]); hi = max(range(3), key=lambda i: hrs[i])
        if hrs[lo] < 0.35 * hrs[hi] and pb.get(names[lo], 0) < 20 and pb.get(names[lo], 0) <= pb.get('metal', 0):
            n = names[lo]
            if not any(x[0] == n for x in self.bplan) and not any(x[0] == n for x in self.bact):
                return (n, pb.get(n, 0) + 1)
        return None

    def plan_buildings(self):
        pb = self.proj_b()
        while len(self.bact) + len(self.bplan) < self.depth:
            item = None
            sc = self.scarce_mine(pb)
            if sc and self.b['metal'] >= 3:
                self.bplan.append(sc); pb[sc[0]] = sc[1]
                continue
            if self.want_vault():
                item = ('vault', pb.get('vault', 0) + 1)
                if not self.b_prereq_ok('vault', pb):
                    item = None
            if item is None:
                item = self.next_scripted_building()
            if item is None:
                break
            # skip duplicates already planned
            if item in self.bplan or any(a[0] == item[0] and a[1] == item[1] for a in self.bact):
                break
            self.bplan.append(item)
            pb[item[0]] = item[1]

    def plan_research(self):
        if not self.b['lab']:
            return
        pb, pr = self.proj_b(), self.proj_r()
        for (n, l) in self.research_order():
            if len(([self.ract] if self.ract else []) + self.rplan) >= self.depth:
                break
            if pr.get(n, 0) >= l:
                continue
            if pr.get(n, 0) != l - 1:
                continue
            if l > RESEARCH[n][2]:
                continue
            if not self.r_prereq_ok(n, pb, pr):
                continue
            self.rplan.append((n, l)); pr[n] = l

    def combat_power(self, count=None):
        wm, hm, sm = 1 + 0.1 * self.r['weapons'], 1 + 0.1 * self.r['hulls'], 1 + 0.1 * self.r['shields']
        tot = 0.0
        for c in ('corvette', 'frigate', 'cruiser'):
            n = self.ships[c] if count is None else count.get(c, 0)
            s = SHIPS[c]
            tot += n * (s[3] * wm + (s[1] * hm + s[2] * sm) / 10.0)
        return tot

    def def_power(self):
        grid = self.b['grid']
        v = GRID_VIRTUAL_UNITS.get(grid, 4.0 * 1.3 ** max(0, grid - 3)) * ship_power('scout')
        st = sum(DEFS[k][1] * n for k, n in self.defs.items()) * (1 + 0.04 * grid)
        return v + st

    def raid_eligible(self):
        return self.b['shipyard'] >= 2 and self.t >= RAID_MIN_AGE_S / self.P ** 0.75

    def plan_ships(self):
        if not self.b['shipyard']:
            return
        if self.splan or self.sact:
            return
        sh = self.ships
        item = None
        # defence first when a raid would beat us
        if self.raid_eligible() and self.b['grid'] >= 1:
            need = 1.15 * raid_power(self.econ_pts)
            if self.def_power() < need:
                for k in ('bastion', 'lancer', 'turret'):
                    pre = DEFS[k][2]
                    if all((self.b.get(a, 0) if a in self.b else self.r.get(a, 0)) >= l for a, l in pre.items()):
                        qty = max(1, min(10, int((need - self.def_power()) / DEFS[k][1]) + 1))
                        item = ('def', k, qty)
                        break
        if item is None:
            if self.r['corvette'] and sh['corvette'] < 2:
                item = ('ship', 'corvette', 2 - sh['corvette'])
            elif self.mode != 'none' and self.r['hauler'] and sh['hauler'] < 2:
                item = ('ship', 'hauler', 2 - sh['hauler'])
            elif self.r['cruiser'] and sh['cruiser'] < max(1, (self.b['metal'] - 8) * 1.5 + 1 + max(0, self.b['shipyard'] - 11) * 2):
                item = ('ship', 'cruiser', 1)
            elif self.r['frigate'] and sh['frigate'] < 4:
                item = ('ship', 'frigate', 1)
            elif self.r['corvette'] and sh['corvette'] < 6 and self.mode != 'none':
                item = ('ship', 'corvette', 1)
        if item:
            self.splan.append(item)

    # ------------------------------------------------------------------ queue engine
    def try_start(self):
        # fair-share ordering of queues
        order = sorted(('b', 'r', 's'), key=lambda q: self.spent[q] / self.share[q])
        reserve = [0.0, 0.0, 0.0]
        reserved = False
        for q in order:
            if q == 'b':
                started = True
                while started and len(self.bact) < self.slots() and self.bplan:
                    started = False
                    for idx, (n, l) in enumerate(list(self.bplan)):
                        pb = {k: v for k, v in self.b.items()}
                        if not self.b_prereq_ok(n, pb):
                            continue
                        c = self.b_cost(n, l)
                        cp_ = self.cap()
                        if any(c[i] > cp_[i] for i in range(3)) and n != 'vault':
                            vi = next((k for k, it in enumerate(self.bplan) if it[0] == 'vault'), None)
                            if vi is None:
                                pb2 = self.proj_b()
                                self.bplan.insert(0, ('vault', pb2.get('vault', 0) + 1))
                                started = True      # re-scan with the vault first
                                break
                            if vi != 0:
                                self.bplan.insert(0, self.bplan.pop(vi))
                                started = True
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
                        self.pay(c); self.spent['b'] += wv(c)
                        self.bplan.pop(idx)
                        self.bact.append([n, l, self.t + self.b_time(n, l), c])
                        started = True
                        break
            elif q == 'r':
                if self.ract is None and self.rplan:
                    n, l = self.rplan[0]
                    if not self.r_prereq_ok(n, dict(self.b), dict(self.r)):
                        self.rplan.pop(0)  # replan later
                        continue
                    c = self.r_cost(n, l)
                    if self.need_vault(c):
                        continue
                    if not self.can_pay(c, reserve):
                        if not reserved:
                            reserve = list(c); reserved = True
                        continue
                    self.pay(c); self.spent['r'] += wv(c)
                    self.rplan.pop(0)
                    self.ract = [n, l, self.t + self.r_time(n, l), c]
            else:
                if self.sact is None and self.splan:
                    kind, cls, qty = self.splan[0]
                    unit = SHIPS[cls][0] if kind == 'ship' else DEFS[cls][0]
                    if self.need_vault(unit):
                        continue
                    # buy as many as affordable (min 1) up to qty, respecting reserve
                    n = 0
                    for k in range(qty, 0, -1):
                        c = tuple(u * k for u in unit)
                        if self.can_pay(c, reserve):
                            n = k; break
                    if n == 0:
                        if not reserved:
                            reserve = [u for u in unit]; reserved = True
                        continue
                    c = tuple(u * n for u in unit)
                    self.pay(c); self.spent['s'] += wv(c)
                    self.splan.pop(0)
                    self.sact = [kind, cls, n, self.t + self.s_time(cls), n]
                    if n < qty:
                        self.splan.insert(0, (kind, cls, qty - n))
                        self.sact[4] = n

    def need_vault(self, c):
        cp_ = self.cap()
        if any(c[i] > cp_[i] for i in range(3)):
            pb2 = self.proj_b()
            if not any(it[0] == 'vault' for it in self.bplan) and not any(a[0] == 'vault' for a in self.bact):
                self.bplan.insert(0, ('vault', pb2.get('vault', 0) + 1))
            return True
        return False

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
        if self.sact and self.sact[3] <= self.t:
            kind, cls, n, _, tot = self.sact
            self.log.append((self.t, 's', cls))
            if kind == 'ship':
                self.ships[cls] += 1
                if cls == 'corvette': self.mark('first_corvette')
                if cls == 'cruiser': self.mark('first_cruiser')
                if cls == 'frigate': self.mark('first_frigate')
                if cls == 'hauler': self.mark('first_hauler')
                self.fleet_pts += wv(SHIPS[cls][0])
            else:
                self.defs[cls] += 1
                self.def_pts += 0.5 * wv(DEFS[cls][0])
            self.pilot_dirty = True
            self.sact[2] -= 1
            if self.sact[2] <= 0:
                self.sact = None
            else:
                self.sact[3] = self.t + self.s_time(cls)

    def on_building(self, n, l):
        m = self.milestones
        if n == 'lab' and l == 2: self.mark('lab2')
        if n == 'shipyard' and l == 4: self.mark('shipyard4')
        if n == 'metal' and l == 10: self.mark('metal10')
        if n == 'metal' and l == 5: self.mark('metal5')
        if n == 'shipyard' and l == 2: self.mark('shipyard2')
        if n == 'vault' and l == 1: self.mark('vault1')
        if n == 'grid' and l == 1: self.mark('grid1')
        self.check_endgame()
    def on_research(self, n, l):
        if n == 'modfab' and l == 1: self.mark('slot2')
        if n == 'cruiser': self.mark('cruiser_tech')
        if n == 'corvette': self.mark('corvette_tech')
        self.check_endgame()
    def check_endgame(self):
        if 'endgame' in self.milestones: return
        b, r = self.b, self.r
        if all(b[k] >= v for k, v in KIT_B.items()) and all(r[k] >= v for k, v in KIT_R.items()) \
                and self.ships['cruiser'] >= KIT_CRUISERS:
            self.mark('endgame')

    # ------------------------------------------------------------------ pilot economy
    def fuel_params(self):
        eff = 1 - 0.1 * self.r['fuel_eff']
        cap = (1 + 0.15 * self.r['tanks']) * (1 + 0.25 * self.b['depot'])
        return eff, cap
    def pilot_scale(self):
        return self.P ** BETA
    def pilot_rates(self):
        """Expected pilot income per REAL hour: ore[3], salv[3], der[3], fuel (deut), loss (value), reach ring.
        mode 'near' = competent (engage at >=2x power, mine <=ring 8, avoid guarded tiles);
        mode 'far'  = greedy (engage at >=1x, pick the farthest ring whose net value is within 80% of best)."""
        out = dict(ore=[0, 0, 0], salv=[0, 0, 0], der=[0, 0, 0], fuel=0.0, loss=0.0, reach=HOME_DIST,
                   ore_ring={}, salv_ring={}, der_ring={}, hunt_d=0, kills_ring={}, sites_ring={}, farm={})
        if self.mode == 'none':
            return out
        s = self.pilot_scale(); P = self.P
        eff, capm = self.fuel_params()
        sh = self.ships
        far = self.mode == 'far'
        ratio_req = 1.5 if far else 3.0
        hunt_ring_cap = 40 if far else 20
        harvest_mod = 1 + 0.2 * self.r['harvest']
        price = FUEL_PRICE_DEUT
        active = self.active_frac()
        # mining fleet: up to 3 haulers, else scouts
        nh = min(sh['hauler'], 3); ns = sh['scout'] if nh < 3 else 0
        cargo = 200 * nh + 20 * ns
        mass = 80 * nh + 30 * ns
        mp = nh * ship_power('hauler') + ns * ship_power('scout')
        fuel_tank = (100 * nh + 60 * ns) * capm
        # combat fleet
        cps = self.combat_power()
        cval = sum(sh[c] * ship_cost_tot(c) for c in ('corvette', 'frigate', 'cruiser'))
        cm = sum(sh[c] * SHIPS[c][1] for c in ('corvette', 'frigate', 'cruiser'))
        ctank = sum(sh[c] * SHIPS[c][5] for c in ('corvette', 'frigate', 'cruiser')) * capm
        chop = FUEL_RATE * cm * eff
        cH = int((ctank / chop) // 2) if chop > 0 else 0
        creach = HOME_DIST + cH
        fuel_hop = FUEL_RATE * mass * eff
        H = int((fuel_tank / fuel_hop) // 2) if fuel_hop > 0 else 0
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
                sup = [n_tiles * avail * tr[k] / ore_regen_h(d) * P * harvest_mod
                       for k in ('metal', 'crystal', 'deut')]
                for i in range(3): tot[i] += sup[i]
                ring[d] = sum(sup)
                sorties = sum(sup) / max(cargo * 0.9, 1)
                fuel += sorties * 2 * hops * fuel_hop * FUEL_PRICE_DEUT
                if guarded > 0 and far:
                    p_enc = 1 - (1 - guarded * 0.5) ** hops
                    r = (esc + 1e-9) / npc_power(d)
                    loss += p_enc * attrition(r) * sum(sup) / 1.0 * 0.5
            cap_units = cargo * 0.9 * 3600 / (40 + 10 * max(1, ring_hi - HOME_DIST)) * 0.7
            t = sum(tot)
            if t > cap_units and t > 0:
                f = cap_units / t
                tot = [x * f for x in tot]; fuel *= f; loss *= f
                ring = {k: v * f for k, v in ring.items()}
            return tot, fuel, loss, ring

        if far:
            best = None; cands = []
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
            o, f, l, rg = ore_for(min(8, reach))
            out['ore'], out['fuel'], out['loss'], out['ore_ring'] = o, f, l, rg

        # ---- salvage (hunting): needs combat ships; capped by active time
        if cps > 0:
            hunt_cap = 3600.0 / 60.0 * active
            cand = [d for d in range(2, min(creach, hunt_ring_cap) + 1) if cps / npc_power(d) >= ratio_req]
            best_d = max(cand) if cand else 0
            kills = []
            for d in cand:
                hops = max(1, abs(d - HOME_DIST) + 1)
                nsec = min(6 * d, 6 * cH) * ring_coverage(d) * npc_presence(d) * (1 - npc_passive_frac(d))
                kills.append((d, nsec / npc_respawn_h(d) * P, hops))
            kills.sort(key=lambda x: -kill_loot_value(x[0]))
            room = hunt_cap; salv = 0.0
            for d, sup, hops in kills:
                k = min(sup, room)
                if k <= 0: break
                room -= k
                util = k / sup if sup > 0 else 0.0
                f = farm_steady(util)
                out['farm'][d] = f
                val = kill_loot_value(d) * s * k * f
                salv += val
                out['salv_ring'][d] = val
                out['kills_ring'][d] = k
                r = cps / npc_power(d)
                out['loss'] += val * attrition(r)
                squad_mass = min(cm, 2.0 * ratio_req * npc_power(d))
                out['fuel'] += k / 4.0 * (2 * hops + 4) * FUEL_RATE * squad_mass * eff * price
            out['salv'] = [salv * 0.55, salv * 0.30, salv * 0.15]
            out['hunt_d'] = best_d
            # ---- derelicts: boarded by the combat fleet (hauler in company doubles loot -> ignored)
            der_cap = 3600.0 / 48.0 * active
            room = der_cap; der = 0.0
            for d in range(HOME_DIST, min(creach, hunt_ring_cap) + 1):
                if cps / npc_power(d) < ratio_req * 0.8:
                    continue
                sites = derelict_sites_per_ring(d) * ring_coverage(d)
                sup = sites / DERELICT_RESPAWN_H * P ** DERELICT_PACE_EXP
                k = min(sup, room)
                if k <= 0: break
                room -= k
                val = derelict_value(d) * s * k * (1 - derelict_ambush(d) * 0.5)
                der += val
                out['der_ring'][d] = val
                out['sites_ring'][d] = k
                r = cps / (npc_power(d) * 0.6)
                out['loss'] += val * derelict_ambush(d) * attrition(r)
                hops = max(1, abs(d - HOME_DIST) + 1)
                squad_mass = min(cm, 2.0 * ratio_req * npc_power(d))
                out['fuel'] += k / 2.0 * (2 * hops + 2) * FUEL_RATE * squad_mass * eff * price
            out['der'] = [der * 0.55, der * 0.30, der * 0.15]
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
            r = self.pilot_rate
            if any(d >= 12 and v > 0 for d, v in r['der_ring'].items()) and 'tier2_loot' not in self.milestones:
                self.mark('tier2_loot')
            if any(d >= 22 and v > 0 for d, v in r['der_ring'].items()) and 'tier3_loot' not in self.milestones:
                self.mark('tier3_loot')
        r = self.pilot_rate
        if not r: return
        need = r['fuel'] * dt / 3600.0
        f = 1.0
        if need > 0:
            allow = 0.5 * self.prod_h()[2] * self.P * dt / 3600.0 + max(0.0, self.res[2] - 0.08 * self.cap()[2])
            f = max(0.0, min(1.0, allow / (need * PILOT_SKILL)))
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
        tot = self.res[0] + self.res[1] + self.res[2] + 1e-9
        self.loss_value += loss
        for i in range(3):
            self.res[i] -= loss * (0.55, 0.3, 0.15)[i]
        cps = self.combat_power()
        for d, k in r['kills_ring'].items():
            ratio = cps / npc_power(d)
            v = kill_loot_value(d) * self.pilot_scale()
            self.combat_raw += PILOT_SKILL * k * dt / 3600.0 * KILL_SHARE * v * LOOT_WEIGHT / 1000.0 * kill_pay(ratio) * r['farm'].get(d, 1.0)
        for d, k in r['sites_ring'].items():
            t = threat(d)
            n = k * dt / 3600.0
            n *= PILOT_SKILL
            self.explore_raw += n * relic_chance(t) * RELIC_POINTS
            total = derelict_sites_per_ring(d) * ring_coverage(d)
            new = max(0.0, min(n, total - self.boarded[d]))
            self.boarded[d] += new
            v = derelict_value(d) * self.pilot_scale()
            self.explore_raw += new * (BOARD_VALUE_SHARE * v * LOOT_WEIGHT / 1000.0 + DISCOVERY_POINTS * t ** 1.5)
        fleets = {'explore': 2.0, 'near': 1.0}.get(self.mode, 0.0)
        if fleets and self.charted < 3 * (r['reach'] + 2) * (r['reach'] + 3):
            n = PILOT_SKILL * fleets * CHART_RATE_PER_FLEET_H * self.active_frac() * dt / 3600.0 * (0.5 if self.mode == 'near' else 1.0)
            d = 1
            while 3 * d * (d + 1) < self.charted:
                d += 1
            self.charted += n
            self.explore_raw += n * chart_points(threat(d))
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
        day = self.t / 86400.0
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
        else:
            # offline: queued entries keep running; planner cannot add
            pass
        self.try_start()
        self.t += dt

    def score(self):
        core = self.econ_pts + self.fleet_pts + self.def_pts
        extra = min(self.combat_raw + self.explore_raw, SCORE_EXTRA_CAP * core)
        return core, extra

    def snapshot(self):
        cap = self.cap()
        pb = self.prod_h()
        self.samples.append(dict(t=self.t, res=list(self.res), cap=list(cap), prod=[x * self.P for x in pb],
                                 b=dict(self.b), r=dict(self.r), ships=dict(self.ships), defs=dict(self.defs),
                                 econ_pts=self.econ_pts, fleet_pts=self.fleet_pts, def_pts=self.def_pts,
                                 inc=dict(self.income), pil=self.pilot_rate and dict(
                                     ore=sum(self.pilot_rate['ore']), salv=sum(self.pilot_rate['salv']),
                                     der=sum(self.pilot_rate['der']), reach=self.pilot_rate['reach']),
                                 slots=self.slots(), score=self.score(),
                                 rings=self.pilot_rate and dict(ore=dict(self.pilot_rate['ore_ring']), salv=dict(self.pilot_rate['salv_ring']), der=dict(self.pilot_rate['der_ring']))))

    def run(self, until_endgame_plus=0.0):
        end = self.p.horizon_h * 3600.0
        dt = self.p.dt
        stop_at = None
        n = 0
        sample_every = end / 60.0
        next_s = 0.0
        next_day = DAILY_EVERY_S
        while self.t < end:
            self.step(dt)
            if self.t >= next_s:
                self.snapshot(); next_s += sample_every
            if self.t >= next_day:
                self.daily.append((self.t, dict(self.b), dict(self.r), dict(self.ships), dict(self.defs), self.score()))
                next_day += DAILY_EVERY_S
            if 'endgame' in self.milestones and stop_at is None:
                stop_at = self.t * 1.0 + until_endgame_plus
            if stop_at is not None and self.t >= stop_at:
                break
        self.snapshot()
        return self


def RESEARCH_LAB_OK(blv):
    return blv.get('lab', 0) >= 1


# =====================================================================
# 4. REPORTING
# =====================================================================
def fmt_t(sec):
    if sec is None: return '-'
    if sec < 90: return f'{sec:.0f}s'
    if sec < 5400: return f'{sec/60:.0f}m'
    if sec < 86400 * 2: return f'{sec/3600:.1f}h'
    return f'{sec/86400:.1f}d'

MILESTONES = [('corvette_tech', 'Corvette Tech'), ('first_corvette', 'First Corvette'), ('lab2', 'Lab 2'),
              ('shipyard4', 'Shipyard 4'), ('slot2', 'Build slot 2 (research)'), ('metal10', 'Metal Mine 10'),
              ('tier2_loot', 'First tier-2 derelict loot (ring 12+)'), ('first_cruiser', 'First Cruiser'), ('tier3_loot', 'First tier-3 derelict loot (ring 22+, competent never goes)'),
              ('endgame', 'Endgame kit complete')]

PHASES = (0.0, 3.0, 6.0)      # session-schedule offsets (hours) used for the median
def run_all(modes=('near',), presets=None):
    out = {}
    for p in presets or PRESETS:
        for m in modes:
            runs = [Sim(p, mode=m, phase_h=(ph if not p.continuous else 0.0)).run()
                    for ph in (PHASES if not p.continuous else (0.0,))]
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
    L.append('| dist | zone | threat | NPC group (class x count) | NPC power | old NPC power | kill loot | derelict tier / loot | ore reserve per ore tile (any) | ore refill (game h) | NPC respawn (game h) |')
    L.append('|---|---|---|---|---|---|---|---|---|---|---|')
    def old_power(d):
        if d <= 8: return 0.6 * ship_power('scout')
        if d <= 15: return 5.5 * 0.8 * ship_power('corvette')
        if d <= 25: return 10 * ship_power('frigate')
        return 20 * 1.2 * ship_power('cruiser')
    for d in (1, 2, 4, 6, 8, 9, 10, 12, 15, 18, 20, 21, 25, 30, 35):
        z = 'inner' if d <= 8 else 'outer' if d <= 20 else 'wander'
        c = npc_class(d)
        m = min(1.3, 0.6 + 0.02 * d)
        cnt = max(1, round(npc_power(d) / (ship_power(c) * m)))
        tr = tile_reserve(d)
        L.append(f'| {d} | {z} | T{threat(d)} | {c} x{cnt} (x{m:.2f} stats) | {npc_power(d):.0f} | {old_power(d):.0f} | {kill_loot_value(d):.0f} | '
                 f'{derelict_tier(d)} / {derelict_value(d):.0f} | {mean_reserve(d)*ORE_RING_MULT(d):.0f} | {ore_regen_h(d):.0f} | {npc_respawn_h(d):.1f} |')
    L.append('')
    return '\n'.join(L)

def storage_cap_table():
    L = ['### Storage caps (metal / crystal / deut) by Vault level and pace', '',
         '| Vault level | cost (m/c) | pace 1 | pace 10 | pace 600 |', '|---|---|---|---|---|']
    for lv in (0, 1, 2, 3, 4, 5, 6, 8, 10, 14):
        g = VAULT_GROWTH ** lv
        cost = '-' if lv == 0 else '%d/%d' % (200 * 1.7 ** (lv - 1), 100 * 1.7 ** (lv - 1))
        row = [f'{lv}', cost]
        for P in (1, 10, 600):
            f = g * P ** CAP_PACE_EXP
            row.append('%d / %d / %d' % (5000 * f, 3500 * f, 2500 * f))
        L.append('| ' + ' | '.join(row) + ' |')
    L.append('')
    return '\n'.join(L)

def time_table():
    L = ['### Real time to build selected items (no Fabricator / Lab / Shipyard bonus), by pace', '',
         '| item | cost (m/c/d) | persistent x1 | season x10 | day x70 | blitz x600 | test x8000 |', '|---|---|---|---|---|---|---|']
    s = Sim(PRESETS[0])
    def row(name, cost, hrs):
        cells = [fmt_t(max(1.0, hrs * 3600 / P)) for P in (1, 10, 70, 600, 8000)]
        L.append(f'| {name} | {cost[0]:.0f}/{cost[1]:.0f}/{cost[2]:.0f} | ' + ' | '.join(cells) + ' |')
    for nm, lv in (('metal', 2), ('metal', 5), ('metal', 10), ('metal', 15), ('shipyard', 4), ('shipyard', 6), ('lab', 8)):
        c = s.b_cost(nm, lv); row(f'{nm} L{lv}', c, (c[0] + c[1]) / BUILD_RATE)
    for nm in ('corvette', 'frigate', 'cruiser'):
        c = RESEARCH[nm][0]; row(f'{nm} tech', c, (c[0] + c[1]) / RES_RATE)
    for nm in ('corvette', 'cruiser'):
        c = SHIPS[nm][0]; row(f'{nm} (one ship)', c, (c[0] + c[1]) / SHIP_RATE)
    L.append('')
    return '\n'.join(L)

def defence_table():
    L = ['### Defence structures', '', '| structure | cost (m/c/d) | power | res per power | requires |', '|---|---|---|---|---|']
    for k, (c, pw, pre) in DEFS.items():
        L.append(f'| {k} | {c[0]}/{c[1]}/{c[2]} | {pw:.0f} | {sum(c)/pw:.1f} | ' + ', '.join(f'{a} {b}' for a, b in pre.items()) + ' |')
    for c in ('scout', 'corvette', 'frigate', 'cruiser'):
        L.append(f'| (ship) {c} | {SHIPS[c][0][0]}/{SHIPS[c][0][1]}/{SHIPS[c][0][2]} | {ship_power(c):.0f} | {sum(SHIPS[c][0])/ship_power(c):.1f} | |')
    L.append('')
    L.append('Raid power by econ points S: ' + ', '.join(f'S={x}: {raid_power(x):.0f}' for x in (1, 10, 100, 500, 1000, 2000)))
    L.append('')
    return '\n'.join(L)

def prod_tables():
    L = ['### Building cost / time / production by level (pace 1, no Fabricator)', '',
         '| L | Metal prod /h | Metal cost (m/c) | Metal time | Crystal prod /h | Deut prod /h | Shipyard cost (m/c/d) | Shipyard time | Lab time |',
         '|---|---|---|---|---|---|---|---|---|']
    s = Sim(PRESETS[0])
    for l in (1, 2, 3, 5, 8, 10, 12, 15, 18, 20):
        mc = s.b_cost('metal', l); sc = s.b_cost('shipyard', l); lc = s.b_cost('lab', l)
        mt = s.b_time('metal', l); st = s.b_time('shipyard', l); lt = s.b_time('lab', l)
        L.append(f'| {l} | {PROD_BASE["metal"]*l*PROD_GROWTH**l:.0f} | {mc[0]:.0f}/{mc[1]:.0f} | {fmt_t(mt)} | '
                 f'{PROD_BASE["crystal"]*l*PROD_GROWTH**l:.0f} | {PROD_BASE["deut"]*l*PROD_GROWTH**l:.0f} | '
                 f'{sc[0]:.0f}/{sc[1]:.0f}/{sc[2]:.0f} | {fmt_t(st)} | {fmt_t(lt)} |')
    L.append('')
    return '\n'.join(L)

def milestone_table(results, mode='near'):
    names = [p.name for p in PRESETS]
    names = [p.name for p in PRESETS]
    L = ['| Milestone | ' + ' | '.join(names) + ' |', '|---|' + '---|' * len(names)]
    for key, label in MILESTONES:
        row = [label]
        for nm in names:
            s = results.get((nm, mode))
            row.append(fmt_t(s.milestones.get(key)) if s else '-')
        L.append('| ' + ' | '.join(row) + ' |')
    return '\n'.join(L)

def balance_table(s, n=10):
    L = [f'**{s.p.name} / {s.mode}**', '', '| time | metal | crystal | deut | cap m/c/d | prod/h m+c+d (real) | levels M/C/D/SY/Lab/Vault/Fab | ships s/co/fr/cr/ha | defs | slots | econ pts |',
         '|---|---|---|---|---|---|---|---|---|---|---|']
    S = s.samples
    idxs = sorted(set(int(i * (len(S) - 1) / n) for i in range(n + 1)))
    for i in idxs:
        x = S[i]
        pr = sum(x['prod'])
        b = x['b']; sh = x['ships']
        L.append(f"| {fmt_t(x['t'])} | {x['res'][0]:.0f} | {x['res'][1]:.0f} | {x['res'][2]:.0f} | "
                 f"{x['cap'][0]:.0f}/{x['cap'][1]:.0f}/{x['cap'][2]:.0f} | {pr:.0f} | "
                 f"{b.get('metal',0)}/{b.get('crystal',0)}/{b.get('deut',0)}/{b.get('shipyard',0)}/{b.get('lab',0)}/{b.get('vault',0)}/{b.get('fab',0)} | "
                 f"{sh.get('scout',0)}/{sh.get('corvette',0)}/{sh.get('frigate',0)}/{sh.get('cruiser',0)}/{sh.get('hauler',0)} | "
                 f"{sum(x['defs'].values())} | {x['slots']} | {x['econ_pts']:.1f} |")
    L.append('')
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
    L = ['| preset | time at >90% cap | time at 100% cap | wasted units (m/c/d) | wasted vs produced |', '|---|---|---|---|---|']
    for p in PRESETS:
        s = results.get((p.name, mode))
        if not s: continue
        tot = s.t
        prod_total = s.income['buildings'] + s.income['ore'] + s.income['salv'] + s.income['der']
        w = sum(s.waste)
        L.append(f'| {p.name} | {100*s.cap_pressure_time/tot:.1f}% | {100*s.cap_full_time/tot:.1f}% | '
                 f'{s.waste[0]:.3g}/{s.waste[1]:.3g}/{s.waste[2]:.3g} | {100*w/max(prod_total,1):.2f}% |')
    return '\n'.join(L)

def ring_income_table(results, preset_name, mode, at_hours=None):
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
    L = ['| preset | mode | first Corvette | Metal 10 | first Cruiser | endgame | pilot share of income | losses (value) | final econ+fleet pts |',
         '|---|---|---|---|---|---|---|---|---|']
    for p in PRESETS:
        for m in ('none', 'near', 'far'):
            s = results.get((p.name, m))
            if not s: continue
            i = s.income
            tot = i['buildings'] + i['ore'] + i['salv'] + i['der']
            pil = i['ore'] + i['salv'] + i['der']
            L.append(f"| {p.name} | {m} | {fmt_t(s.milestones.get('first_corvette'))} | {fmt_t(s.milestones.get('metal10'))} | "
                     f"{fmt_t(s.milestones.get('first_cruiser'))} | {fmt_t(s.milestones.get('endgame'))} | {100*pil/tot:.0f}% | "
                     f"{s.loss_value:.3g} | {s.econ_pts+s.fleet_pts+s.def_pts:.0f} |")
    return '\n'.join(L)

def pace_table(results):
    ref = results.get(('persistent', 'near'))
    if not ref: return ''
    m = ref.milestones
    E, C = m.get('endgame'), m.get('first_cruiser')
    L = ['| world arc you want | pace P needed | first Cruiser | endgame kit | notes |', '|---|---|---|---|---|']
    rows = [('persistent baseline', 1), ('owner guess: weekly "2x"', 2), ('owner guess: blitz "5x"', 5),
            ('endgame in 30 days', E / 86400 / 30), ('endgame in 14 days', E / 86400 / 14),
            ('endgame in 7 days (season)', E / 86400 / 7), ('endgame in 3 days', E / 86400 / 3),
            ('endgame in 24 h', E / 86400 / 1), ('Cruiser at 45 min (blitz)', C / 2700.0),
            ('endgame in 15 min (test)', E / 900.0)]
    for name, P in rows:
        L.append(f'| {name} | x{P:.0f} | {fmt_t(C / P)} | {fmt_t(E / P)} | |')
    return '\n'.join(L)

def score_table(results):
    L = ['| preset | econ pts (build+research) | fleet pts | defence pts | combat+explore (capped) | total |', '|---|---|---|---|---|---|']
    for p in PRESETS:
        s = results.get((p.name, 'near'))
        if not s: continue
        s0 = s.runs[0]
        core, extra = s0.score()
        L.append(f'| {p.name} | {s0.econ_pts:.0f} | {s0.fleet_pts:.0f} | {s0.def_pts:.0f} | {extra:.0f} | {core+extra:.0f} |')
    return '\n'.join(L)

def ring_snapshot_table(s, label, when):
    S = s.samples
    x = S[min(len(S) - 1, int(when * (len(S) - 1)))]
    if not x.get('rings'):
        return ''
    L = [f'**{label}** at {fmt_t(x["t"])}: pilot yield per real hour by ring (all resources) against building income {sum(x["prod"]):.0f}/h',
         '', '| ring | threat | ore /h | salvage /h | derelict /h | total /h | vs buildings |', '|---|---|---|---|---|---|---|']
    ds = sorted(set(x['rings']['ore']) | set(x['rings']['salv']) | set(x['rings']['der']))
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
    """Per-day list of what completed: new building levels (max per building), techs, ships, defences."""
    L = [f'**{title}**', '', '| day | buildings reached | research done | ships/defences finished | score (core+extra) |', '|---|---|---|---|---|']
    days = {}
    for t, k, tx in s.log:
        days.setdefault(int(t // 86400) + 1, []).append((k, tx))
    nd = max(days) if days else 0
    for d in range(1, nd + 1):
        ev = days.get(d, [])
        bl = {}
        for k, tx in ev:
            if k == 'b':
                n, l = tx.split(); bl[n] = max(bl.get(n, 0), int(l))
        rs = [tx for k, tx in ev if k == 'r']
        sh = {}
        for k, tx in ev:
            if k == 's': sh[tx] = sh.get(tx, 0) + 1
        sc = '-'
        for (t, b, r, shp, dfs, scr) in s.daily:
            if int(t // 86400) == d:
                sc = f'{scr[0]:.0f}+{scr[1]:.0f}'
        L.append(f"| {d} | {', '.join(f'{n} {l}' for n, l in bl.items())} | {', '.join(rs)} | "
                 f"{', '.join(f'{c}x{n}' for c, n in sh.items())} | {sc} |")
    return '\n'.join(L)

def full_report(results, presets):
    out = []
    out.append('### Milestones, competent player (mode `near`); median of 3 session-schedule offsets\n')
    out.append(milestone_table(results, 'near'))
    out.append('\n### Milestones, greedy far-mining player (mode `far`)\n')
    out.append(milestone_table(results, 'far'))
    out.append('\n### Milestones, buildings-only player (mode `none`, no piloting at all)\n')
    out.append(milestone_table(results, 'none'))
    out.append('\n### Pace needed for a target arc (derived from the persistent run)\n')
    out.append(pace_table(results))
    out.append('\n### Mode comparison: none = buildings only, near = competent, far = greedy far-mining\n')
    out.append(compare_modes(results))
    out.append('\n### Income split over the whole run, competent (near)\n')
    out.append(income_table(results, 'near'))
    out.append('\n### Income split over the whole run, greedy far-mining\n')
    out.append(income_table(results, 'far'))
    out.append('\n### Storage-cap pressure (competent)\n')
    out.append(storage_table(results, 'near'))
    out.append('\n### Score at end of run (competent)\n')
    out.append(score_table(results))
    if ('season-7d', 'near') in results:
        out.append('\n### Season-7d: what completes each day (competent player)\n')
        out.append(day_summary(results[('season-7d', 'near')].runs[0], 'season-7d / near'))
    for pn in ('persistent', 'season-7d', 'blitz-1h'):
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
    p = Preset('persistent', 1.0, 600.0, 24 * 365, 4, 12)
    s = Sim(p, mode='near')
    end = p.horizon_h * 3600.0
    marks = [d * 86400.0 for d in (30, 60, 74, 90, 120, 150, 180, 240, 300, 365)]
    L = ['| day | Metal/Crystal/Deut | Shipyard | Lab | Fabricator | Vault | Cruisers | defence structures | score (core+extra) | production /h |', '|---|---|---|---|---|---|---|---|---|---|']
    mi = 0
    while s.t < end and mi < len(marks):
        s.step(p.dt)
        if s.t >= marks[mi]:
            b = s.b; core, extra = s.score()
            L.append(f"| {marks[mi]/86400:.0f} | {b['metal']}/{b['crystal']}/{b['deut']} | {b['shipyard']} | {b['lab']} | {b['fab']} | {b['vault']} | {s.ships['cruiser']} | {sum(s.defs.values())} | {core:.0f}+{extra:.0f} | {sum(s.prod_h()):.0f} |")
            mi += 1
    return '\n'.join(L)

def sensitivity():
    L = ['| world | sessions/day x minutes | first Corvette | Shipyard 4 | slot 2 | first Cruiser | endgame | wasted production |', '|---|---|---|---|---|---|---|---|']
    cases = [('persistent', 1.0, 300.0, 24 * 220, [(1, 15), (2, 15), (4, 12), (8, 12)]),
             ('season-7d', 10.0, 30.0, 24 * 21, [(2, 15), (4, 15), (8, 15), (16, 15)])]
    for name, P, dt, hz, ss in cases:
        for ns, ml in ss:
            p = Preset(name, P, dt, hz, ns, ml)
            r = run_all(('near',), [p])[(name, 'near')]
            s0 = r.runs[0]
            prod = s0.income['buildings'] + s0.income['ore'] + s0.income['salv'] + s0.income['der']
            L.append(f"| {name} | {ns} x {ml} min | " + ' | '.join(fmt_t(r.milestones.get(k)) for k in
                     ('first_corvette', 'shipyard4', 'slot2', 'first_cruiser', 'endgame')) +
                     f" | {100*sum(s0.waste)/prod:.1f}% |")
    return '\n'.join(L)

COMPARE_MODES = (('none', 'builder'), ('explore', 'explorer'), ('raid', 'raider'), ('near', 'mixed'))

def compare_scores(cases=(('blitz-1h', 3600.0), ('season-7d', 7 * 86400.0)), phases=(0.0, 6.0, 12.0)):
    """Score at a fixed time (the arc a player is judged on) for the four play styles."""
    L = ['| world | style | core | combat pts | explore pts | extra after cap | total | vs builder |', '|---|---|---|---|---|---|---|---|']
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
                core, extra = sm.score()
                acc.append((core, sm.combat_raw, sm.explore_raw, extra))
            n = len(acc)
            rows[label] = tuple(sum(a[i] for a in acc) / n for i in range(4))
        base = rows['builder'][0]
        for _, label in COMPARE_MODES:
            core, c, e, extra = rows[label]
            L.append(f"| {pname} | {label} | {core:.0f} | {c:.0f} | {e:.0f} | {extra:.0f} | {core + extra:.0f} | {100 * (core + extra) / base:.0f}% |")
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
        print(static_tables()); print(prod_tables()); print(time_table()); print(storage_cap_table()); print(defence_table()); return
    presets = [p for p in PRESETS if not a.preset or p.name == a.preset]
    if a.pace:
        hz = a.horizon_h or max(0.5, 24 * 90 / a.pace * 1.5)
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
