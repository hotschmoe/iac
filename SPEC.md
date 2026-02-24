# In Amber Clad — Technical Specification

**Version:** 0.1.0
**Status:** M1 in progress -- core systems implemented, integration ongoing

---

## 1. World Model

### 1.1 Hex Grid

The game world is an infinite hex grid using axial coordinates `(q, r)`. The central hub is at `(0, 0)`.

**Coordinate system:** Axial (q, r) with cube coordinate conversion for distance calculations. Hex orientation is flat-top.

**Distance from center:** `dist = max(|q|, |r|, |q+r|)` (cube distance from origin).

**Zones** are defined by distance from center:

| Zone | Distance | Character |
|---|---|---|
| Central Hub | 0 | Trade, social, safe harbor |
| Inner Ring | 1–8 | Safe PvE, starter resources, homeworld claims |
| Outer Ring | 9–20 | Moderate PvE, decent resources |
| The Wandering | 21+ | Dangerous, high reward, increasingly sparse |

Zone boundaries are tunable and these values are initial estimates.

### 1.2 Procedural Generation

Sectors are generated deterministically from their coordinates using a seeded PRNG: `seed = hash(world_seed, q, r)`. This means the universe is consistent without storing every sector — only sectors with modified state (player structures, depleted resources, etc.) need database entries.

**Sector contents** are generated per-seed:
- **Terrain type:** empty space, asteroid field, nebula, debris field, anomaly
- **Resource density:** none / sparse / moderate / rich / pristine (probability weighted by zone)
- **NPC presence:** probability and strength scaled by distance from center
- **Edge connectivity:** see §1.3

### 1.3 Edge Pruning (Dead Ends)

A standard hex has 6 neighbors. Not all edges are traversable.

**Connectivity algorithm:**
1. For each hex, generate its 6 potential edges.
2. Each edge has a survival probability based on the *further* of the two hexes from center:
   - Inner Ring: 95% (nearly fully connected)
   - Outer Ring: 80%
   - The Wandering: 60%, decreasing to 40% at extreme distances
3. Edge pruning is symmetric — computed once using `min(hash(A,B), hash(B,A))` so both hexes agree.
4. **Guarantee:** Every hex retains at least 1 edge (no orphaned sectors). If pruning would orphan a hex, the last edge survives.
5. **Hub connections:** The central hub `(0,0)` always has all 6 edges open.

This creates a well-connected inner region that becomes an increasingly maze-like, dead-end-rich network as you push outward.

### 1.4 Sector State

```
Sector {
    q: i32
    r: i32
    terrain: TerrainType
    base_resources: {metal: Density, crystal: Density, deuterium: Density}
    current_resources: {metal: f32, crystal: f32, deuterium: f32}  // depletes with harvesting, regenerates slowly
    npc_fleets: []Fleet        // spawned procedurally, respawn on timer
    player_fleets: []FleetRef  // currently present player fleets
    connections: []HexCoord    // traversable neighbor list (post-pruning)
    discovered_by: []PlayerID  // fog of war tracking
}
```

Resource regeneration: depleted sectors slowly regenerate resources over time (hours/days scale), encouraging players to move on rather than camp one rich sector forever.

---

## 2. Tick System

### 2.1 Tick Rate

The server runs a fixed simulation tick at **1 tick per second** (1 Hz). This is the atomic unit of game time.

All game actions resolve on tick boundaries. Client input between ticks is queued and processed on the next tick.

### 2.2 Tick Loop

Each server tick:

```
1. Receive all queued player commands since last tick
2. Evaluate auto-action policies for players with no queued command
3. Resolve movement (sector transitions)
4. Resolve combat (one round per tick for active engagements)
5. Resolve harvesting (resource extraction for mining fleets)
6. Update NPC behavior (patrol movement, aggro, spawning)
7. Update homeworld production timers
8. Compute state deltas
9. Broadcast deltas to connected clients
10. Persist dirty state to database (batched, not every tick)
```

### 2.3 Cooldowns

Actions have cooldown timers measured in ticks:

| Action | Cooldown (ticks) | Notes |
|---|---|---|
| Move to adjacent sector | 3–5 | Based on fleet speed (slowest ship) |
| Initiate harvest | 1 | Harvesting is continuous until stopped/interrupted |
| Fire weapons | 2–6 | Per ship class; combat auto-resolves each tick during engagement |
| Scan sector | 5 | Reveal hidden info in current/adjacent sector |
| Emergency recall | 1 (instant) | But has fleet damage risk |
| Deploy from homeworld | 10 | Fleet launch preparation |

Cooldowns are the mechanism that hides the discrete tick rate from human players. Between actions, the TUI shows smooth timers counting down.

---

## 3. Ships & Fleets

### 3.1 Ship Classes

Initial ship classes (expandable through research):

| Class | Role | Hull | Shield | Weapon | Speed | Cargo | Rapid-Fire vs. |
|---|---|---|---|---|---|---|---|
| **Scout** | Exploration, fast | 30 | 10 | 5 | 10 | 20 | — |
| **Corvette** | Light combat, swarm | 50 | 20 | 15 | 8 | 10 | Scout (x3) |
| **Frigate** | Tanky, mid-range | 120 | 60 | 30 | 5 | 30 | Corvette (x2) |
| **Cruiser** | Heavy hitter | 200 | 100 | 80 | 4 | 50 | Frigate (x2) |
| **Hauler** | Cargo transport | 80 | 20 | 5 | 6 | 200 | — |

**Speed** determines movement cooldown: `move_cooldown = base_move_ticks * (10 / fleet_min_speed)`. The fleet moves at the speed of its slowest ship.

**Rapid-fire:** When a ship fires and has a rapid-fire bonus against the target's class, it has a `(1 - 1/multiplier)` chance to fire again. This chains — a corvette with x3 rapid-fire vs. scouts fires an average of 1.5 shots per tick against scouts. This is the OGame mechanic.

### 3.2 Ship Stats

Each ship instance has:
```
Ship {
    id: u64
    class: ShipClass
    hull: f32          // current / max
    hull_max: f32
    shield: f32        // current / max, regenerates between combats
    shield_max: f32
    weapon_power: f32
    speed: u8
    cargo_capacity: u32
    // component slots (future: loot-driven upgrades)
}
```

### 3.3 Fleets

A fleet is a collection of ships deployed together.

```
Fleet {
    id: u64
    owner: PlayerID
    ships: []Ship
    location: HexCoord
    cargo: {metal: f32, crystal: f32, deuterium: f32}
    fuel: f32           // current deuterium fuel
    fuel_max: f32       // sum of ship fuel capacities + depot bonus
    state: FleetState   // idle, moving, harvesting, in_combat, returning
    auto_policy: []PolicyRule  // agent auto-actions
}
```

**Fuel consumption:** Moving one hex costs `fuel_cost = fleet_total_mass * fuel_rate`. Research improves `fuel_rate`. Fleet range is `fuel_max / fuel_cost_per_hex`, giving a hard radius of operation.

### 3.4 Starting Conditions

New players begin with:
- 1 Scout ship
- Homeworld claim in the inner ring (random unoccupied hex, dist 3–6 from center)
- Starting resources: 500 metal, 300 crystal, 100 deuterium
- Basic mine levels (metal 1, crystal 1, deuterium 0)

---

## 4. Combat

### 4.1 Combat Initiation

Combat begins when:
- A player fleet enters a sector with hostile NPCs (auto-aggro based on NPC type)
- A player manually targets a fleet in their sector
- (Future) PvP engagement in dark forest zones

Combat is resolved **one round per tick** while active. Players can issue commands during combat (focus fire, retreat, recall) that take effect next tick.

### 4.2 Combat Round (per tick)

For each ship in the engagement (both sides), in random order:

1. **Select target:** Random enemy ship (weighted by size — larger ships are easier to hit).
2. **Calculate damage:** `damage = weapon_power * (0.8 + random(0.0, 0.4))` — 80–120% of base weapon power.
3. **Apply to shield:** `shield_damage = min(damage, target.shield)`. Remaining damage passes through.
4. **Apply to hull:** `hull_damage = passthrough_damage`. If hull reaches 0, ship is destroyed.
5. **Rapid-fire check:** If attacker has rapid-fire vs. target class, probability `(1 - 1/rf_multiplier)` to repeat from step 2 against a new random target.

After all ships have fired:
- Remove destroyed ships.
- Check victory conditions: one side has no remaining ships → combat ends.
- If both sides still have ships, continue next tick.

### 4.3 Shield Regeneration

Shields regenerate to full between combats (when not engaged for 10+ ticks). During combat, shields do not regenerate. This encourages hit-and-run tactics and makes sustained deep-space expeditions risky.

### 4.4 Retreat & Emergency Recall

**Retreat:** Fleet attempts to move to an adjacent sector. Takes normal movement cooldown. Enemy fleet gets one "free shot" round as you leave (attacks resolve, your ships don't fire back that tick).

**Emergency recall:** Instant jump to homeworld. Costs 2x the deuterium of the actual distance. Each ship in the fleet has a `damage_chance` based on distance:
- `damage_chance = min(0.6, distance * 0.02)` — 2% per hex, capped at 60%.
- Ships that "fail" the check take `random(20%, 80%)` hull damage.
- Ships reduced to 0 hull are destroyed.

This makes recall always available but increasingly costly from deep in the wandering.

---

## 5. Resources & Economy

### 5.1 Homeworld Production

Mines produce resources passively each tick:

```
production_per_tick(level) = base_rate * level * 1.1^level
```

| Mine | Base Rate (per tick) | Cost to Build Level N |
|---|---|---|
| Metal Mine | 0.5 | N * 60 metal, N * 15 crystal |
| Crystal Mine | 0.3 | N * 48 metal, N * 24 crystal |
| Deuterium Synthesizer | 0.15 | N * 225 metal, N * 75 crystal |

Build times scale with level: `build_ticks = base_ticks * level * 1.5^level`.

### 5.2 Active Harvesting

Fleets in resource-bearing sectors can harvest all available resources simultaneously:

```
For each resource (metal, crystal, deuterium):
    harvest_per_tick = fleet_harvest_power * resource_density_multiplier
```

`fleet_harvest_power` is sum of all ships' harvest contribution (haulers: 5.0, scouts: 1.0, others: 0.5). Each resource is harvested independently up to remaining cargo capacity. `resource_density_multiplier`:
- Sparse: 0.5x
- Moderate: 1.0x
- Rich: 2.0x
- Pristine: 4.0x

Harvesting depletes sector resources. Each resource tracks a harvest accumulator per sector. When accumulated extraction exceeds a threshold, the density downgrades one level:

| Density Level | Threshold (units extracted before downgrade) |
|---|---|
| Pristine -> Rich | 40 |
| Rich -> Moderate | 30 |
| Moderate -> Sparse | 20 |
| Sparse -> None | 10 |

Accumulators reset on each downgrade. Sectors regenerate slowly over real-time hours.

### 5.3 Salvage (Combat Loot)

Destroying an NPC fleet yields salvage proportional to the fleet's total resource value:
- 30% of destroyed fleet's build cost drops as salvageable resources (split metal/crystal/deut by original build ratios)
- Salvage floats in the sector for 60 ticks before despawning
- Must be collected (requires cargo space)

### 5.4 Loot Tables (Components & Data Fragments)

NPC fleets have a loot table based on zone and fleet strength:

```
LootDrop {
    type: Component | DataFragment
    // Component: applies to a specific ship class, improves a stat
    // DataFragment: contributes to a research project
    rarity: Common | Uncommon | Rare | Exotic
    drop_chance: f32  // scaled by enemy difficulty
}
```

**Components** (examples):
- Reinforced Corvette Plating: +10% corvette hull
- Overcharged Frigate Shields: +15% frigate shield capacity
- Precision Cruiser Targeting: +10% cruiser weapon power

**Data Fragments** act as a secondary currency for research. Some research requires not just resources but also a number of specific fragment types — tying research progression to active play, not just passive waiting.

---

## 6. Homeworld Buildings

### 6.1 Building List

| Building | Function | Prerequisite |
|---|---|---|
| Metal Mine | Passive metal production | — |
| Crystal Mine | Passive crystal production | — |
| Deuterium Synthesizer | Passive deuterium production | — |
| Shipyard | Build and repair ships | Metal Mine 2 |
| Research Lab | Unlock technologies | Crystal Mine 2 |
| Fuel Depot | Increase fleet fuel capacity | Deut Synth 2 |
| Sensor Array | Reveal adjacent sectors, detect incoming fleets | Research Lab 1 |
| Defense Grid | Automated homeworld defenses (future PvP) | Shipyard 3 |

Each building has levels. Higher levels cost exponentially more but provide increasing benefits.

### 6.2 Build Queue

Only **one building** can be constructed or upgraded at a time (like OGame). Build times are in real-time seconds/minutes, scaled by level.

The shipyard has its own separate queue — you can build/upgrade buildings and queue ships simultaneously.

---

## 7. Research

### 7.1 Research Tree

Research requires the Research Lab and costs resources + data fragments.

| Technology | Effect | Prerequisites |
|---|---|---|
| Fuel Efficiency I–V | Reduce fleet fuel consumption 10% per level | Deut Synth 3 |
| Extended Fuel Tanks I–V | Increase fleet fuel capacity 15% per level | Fuel Depot 2 |
| Reinforced Hulls I–V | +10% hull HP for all ships per level | Shipyard 2 |
| Advanced Shields I–V | +10% shield HP for all ships per level | Research Lab 3 |
| Weapons Research I–V | +10% weapon power for all ships per level | Research Lab 3 |
| Navigation I–V | Reduce movement cooldown by 1 tick per level | Research Lab 2 |
| Harvesting Efficiency I–V | +20% harvest rate per level | Research Lab 2 |
| Corvette Tech | Unlock Corvette construction | Shipyard 2 |
| Frigate Tech | Unlock Frigate construction | Corvette Tech, Shipyard 4 |
| Cruiser Tech | Unlock Cruiser construction | Frigate Tech, Shipyard 6 |
| Hauler Tech | Unlock Hauler construction | Shipyard 3 |
| Emergency Jump I–III | Reduce recall damage chance by 5% per level | Nav I, Research Lab 4 |

### 7.2 Data Fragment Requirements

Advanced research (level III+) requires data fragments in addition to resources. Fragment types correspond to the zone/enemy type where they were found — incentivizing exploration of specific regions.

---

## 8. NPC Faction: Morning Light Mountain

### 8.1 Scaling

Morning Light Mountain (MLM) fleets are procedurally generated based on sector distance from center:

| Distance | Fleet Composition | Behavior | Stat Multiplier |
|---|---|---|---|
| 3–8 | 1 scout (30% presence) | 50% passive / 50% patrol | 0.6x |
| 9–15 | 3–8 corvettes | Patrol, aggro on proximity | 0.8x |
| 16–25 | 5–15 mixed corvettes/frigates | Aggressive, will pursue 1 sector | 1.0x |
| 26–40 | 10–30 mixed with cruisers | Swarm tactics, pursue 2 sectors | 1.2x |
| 41+ | Large swarms, elite variants | Relentless, may guard rare resources | 1.2x+ |

The stat multiplier scales NPC hull, shield, and weapon power relative to the ship class base stats. Speed is unaffected. Inner ring scouts at 0.6x have hull=18, shield=6, weapon=3 -- beatable by a single player scout.

### 8.2 Spawning

NPCs spawn when a sector is first visited (generated from seed) and respawn on a timer after being cleared (30–120 minutes real-time, based on zone). Deep wandering sectors may have NPCs that *don't* respawn — once you clear them, they're cleared, but the initial encounter is formidable.

### 8.3 Elite Variants (Future)

Named MLM entities with unique abilities, better loot tables, and higher difficulty. These are discovery moments — the first time your fleet scans and gets something unexpected on the amber display.

---

## 9. Networking & Protocol

### 9.1 Client-Server Model

The server is authoritative. Clients are thin renderers + input devices.

**Connection:** WebSocket (ws:// for local dev, wss:// for production).

**Authentication:** Simple token-based. Player registers a name, gets a session token. (Full auth system is out of scope for early milestones.)

### 9.2 Server → Client Messages

```json
{
    "type": "tick_update",
    "tick": 4821,
    "fleet_state": { ... },
    "sector_state": { ... },
    "homeworld_state": { ... },
    "events": [
        {"type": "combat_round", "details": { ... }},
        {"type": "resource_harvested", "details": { ... }},
        {"type": "ship_destroyed", "details": { ... }}
    ]
}
```

State deltas are sent — only fields that changed since last tick. Full state sync on initial connection and on request.

### 9.3 Client → Server Messages

```json
{"type": "command", "action": "move", "target": [12, -3]}
{"type": "command", "action": "harvest", "resource": "metal"}
{"type": "command", "action": "attack", "target_fleet_id": 8831}
{"type": "command", "action": "recall"}
{"type": "command", "action": "build", "building": "metal_mine"}
{"type": "command", "action": "research", "tech": "fuel_efficiency_2"}
{"type": "command", "action": "build_ship", "class": "corvette"}
{"type": "policy_update", "policy": [ ... ]}
```

### 9.4 Auto-Action Policy Protocol

Agents submit a policy table (ordered list of condition→action rules). The server evaluates top-to-bottom each tick when no explicit command is queued. First matching rule fires.

```json
{
    "type": "policy_update",
    "policy": [
        {
            "condition": "in_combat AND fleet_shield_pct < 0.3",
            "action": "recall"
        },
        {
            "condition": "in_combat",
            "action": "attack_nearest"
        },
        {
            "condition": "sector_has_resources AND cargo_pct < 0.9",
            "action": "harvest"
        },
        {
            "condition": "cargo_pct >= 0.9",
            "action": "navigate_home"
        }
    ]
}
```

**Condition language:** Simple boolean expressions over game state variables. AND/OR/NOT, comparison operators. No function calls or complex logic — this is a policy table, not a programming language.

Supported variables (initial set, expandable):
- `in_combat`, `sector_clear`
- `hostile_in_sector`, `hostile_count`
- `fleet_shield_pct`, `fleet_hull_pct`
- `cargo_pct`, `fuel_pct`
- `sector_has_resources`, `sector_resource_density`
- `distance_from_home`
- `fleet_ship_count`

---

## 10. TUI Design

### 10.1 Aesthetic

**Pure amber monochrome.** All rendering uses a single hue (amber, approximately #FFB000) at varying brightness levels against a black background.

**Brightness levels as information hierarchy:**
- **Dim** (30% brightness): borders, chrome, decorative elements, inactive UI
- **Normal** (60%): standard text, labels, values
- **Bright** (85%): important values, active elements, player ship
- **Full** (100%): alerts, critical warnings, combat events

**CRT effects** (stretch goals for zithril):
- Slight scanline simulation (alternating dim rows)
- Bloom on bright characters (render bright text with dim halo)
- Subtle vignette (corners slightly dimmer)

### 10.2 Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│ IN AMBER CLAD v0.1.0                    TICK: 4821   [12, -3]     │
├──────────────────────────────────┬──────────────────────────────────┤
│                                  │ FLEET STATUS                    │
│        HEX MAP VIEWPORT          │ Ships: 4/4  Shield: 87%        │
│                                  │ Hull:  94%  Fuel:   68%        │
│     Renders ~7 hex radius        │                                │
│     around player position       │ Cargo: 124/280                 │
│                                  │  Metal:   67                   │
│     Uses hex box-drawing or      │  Crystal: 41                   │
│     ASCII hex approximation      │  Deut:    16                   │
│                                  │                                │
│     Player fleet = bright ◆      ├──────────────────────────────────┤
│     NPCs = dim ▲                 │ SECTOR INFO                    │
│     Resources = ·                │ Asteroid Field (Rich)          │
│     Unexplored = ░              │ MLM Patrol: 3 corvettes        │
│     Dead-end edge = no line      │ Connections: 3 (N, NE, S)     │
│                                  │                                │
├──────────────────────────────────┼──────────────────────────────────┤
│ COMBAT LOG / EVENT FEED          │ COMMANDS                       │
│                                  │ [m]ove  [a]ttack  [h]arvest   │
│ > MLM corvette fires → Scout     │ [s]can  [r]ecall  [b]uild     │
│   Shield absorbs 12 dmg          │ [p]olicy [i]nventory          │
│ > Corvette fires → MLM scout     │                                │
│   HIT: 14 hull damage            │ > _                           │
│ > MLM scout destroyed!           │                                │
└──────────────────────────────────┴──────────────────────────────────┘
```

### 10.3 Input

**Single-key commands** for common actions (roguelike style). Modal sub-screens for complex actions (build menu, research tree, policy editor, inventory).

**Command input line** at bottom-right for typed commands — same commands an LLM agent would send, making the TUI a superset of the CLI interface.

---

## 11. Database Schema (SQLite via zqlite)

### 11.1 Core Tables (Implemented)

```sql
CREATE TABLE server_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE players (
    id INTEGER PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    homeworld_q INTEGER NOT NULL,
    homeworld_r INTEGER NOT NULL,
    metal REAL DEFAULT 500,
    crystal REAL DEFAULT 300,
    deuterium REAL DEFAULT 100
);

CREATE TABLE fleets (
    id INTEGER PRIMARY KEY,
    player_id INTEGER REFERENCES players(id),
    q INTEGER NOT NULL,
    r INTEGER NOT NULL,
    state TEXT DEFAULT 'idle',
    fuel REAL NOT NULL,
    fuel_max REAL NOT NULL,
    cargo_metal REAL DEFAULT 0,
    cargo_crystal REAL DEFAULT 0,
    cargo_deuterium REAL DEFAULT 0
);

CREATE TABLE ships (
    id INTEGER PRIMARY KEY,
    fleet_id INTEGER REFERENCES fleets(id),
    player_id INTEGER REFERENCES players(id),
    class TEXT NOT NULL,
    hull REAL NOT NULL,
    hull_max REAL NOT NULL,
    shield REAL NOT NULL,
    shield_max REAL NOT NULL,
    weapon_power REAL NOT NULL,
    speed INTEGER NOT NULL
);

CREATE TABLE sectors_modified (
    q INTEGER,
    r INTEGER,
    metal_density INTEGER,
    crystal_density INTEGER,
    deut_density INTEGER,
    metal_harvested REAL DEFAULT 0,
    crystal_harvested REAL DEFAULT 0,
    deut_harvested REAL DEFAULT 0,
    PRIMARY KEY (q, r)
);

CREATE TABLE explored_edges (
    player_id INTEGER REFERENCES players(id),
    q1 INTEGER NOT NULL,
    r1 INTEGER NOT NULL,
    q2 INTEGER NOT NULL,
    r2 INTEGER NOT NULL,
    discovered_tick INTEGER NOT NULL,
    PRIMARY KEY (player_id, q1, r1, q2, r2)
);
```

Resources are stored as `i64 * 1000` internally for f32 precision.

### 11.2 Deferred Tables (M2+)

```sql
-- Buildings and research tables deferred to M2
-- Auto-policies table deferred to M4
```

Only modified sectors are stored. Unvisited/unmodified sectors are generated on the fly from coordinates.

---

## 12. Milestone 1 Detailed Scope

### What's In
- Zig server with 1 Hz tick loop
- WebSocket server accepting connections (webzocket)
- Single hex grid with procedural generation (terrain, resources, edge pruning)
- Player spawns with 1 scout at a random inner ring hex
- Movement between connected hexes with cooldown
- NPC fleet generation and basic combat (stochastic, per-tick rounds, rapid-fire)
- Resource harvesting in asteroid fields
- Amber TUI client rendering via zithril: command center, windshield, star map views
- JSON command interface (same WebSocket, no TUI) for LLM/CLI clients
- SQLite persistence via zqlite for player state, fleets, ships, and modified sectors
- Single player (server supports one connection -- multiplayer networking exists but untested with concurrent players)

### Implementation Status
- [x] WebSocket networking (server accept/broadcast, client connect/poll)
- [x] SQLite schema and CRUD (players, fleets, ships, sectors, server state)
- [x] Stochastic combat with rapid-fire chains and ship destruction
- [x] Zithril TUI with three views (command center, windshield, star map)
- [x] Event log ring buffer with formatted display
- [x] Tick loop with periodic persistence (dirty tracking, batch writes every 30 ticks)
- [x] Movement with cooldowns (speed-based, fuel consumption)
- [x] Harvest command processing (all resources: metal/crystal/deut, density-based yield, cargo limits)
- [x] Emergency recall (fuel cost, stochastic hull damage scaled by distance)
- [x] NPC encounters on sector entry (procedural from seed, auto-combat, balanced for inner ring)
- [x] Full state sync on connect/reconnect
- [x] Player reconnection (existing players resume by name, not re-created)
- [x] Graceful shutdown with final state persist
- [x] Leak-free memory management (server and client)
- [x] Server sends sector state (terrain, resources, connections) per-tick and on full sync
- [x] Windshield hex compass with fixed direction keys (1=E, 2=NE, 3=NW, 4=W, 5=SW, 6=SE)
- [x] Client deep-copies sector connections to survive parse arena cleanup
- [x] Combat visual feedback (!! COMBAT !! title flash, damage numbers in event log)
- [x] Death state handling (0 ships blocks commands client+server, DESTROYED display)
- [x] Resource depletion (harvest accumulators per sector, density downgrades at thresholds, persisted)
- [x] Harvest validation (reject while moving, no-resource sectors, full cargo; errors sent to client)
- [x] Command error feedback (server sends error messages, client displays in event log)
- [x] Windshield event log (bottom panel shows recent events in windshield view)
- [x] NPC stat scaling by zone (0.6x inner ring, 0.8x mid, 1.0x outer, 1.2x wandering)
- [x] Hostile fleet info in sector state (ship class, count, behavior displayed per-fleet)
- [ ] Hex map rendering in star map view (placeholder only)
- [ ] NPC patrol AI (spawning works, no movement/aggro behavior yet)
- [ ] Resource regeneration

### What's Out (Deferred)
- Homeworld buildings, shipyard, research (M2)
- Fleet composition beyond starting scout (M2)
- Loot system — components, data fragments (M4)
- Auto-action policies (M4)
- Multiple concurrent players (M3)
- PvP (M5)
- CRT visual effects (ongoing zithril improvement)

### Definition of Done (M1)
A human player can connect via TUI, see the amber hex map, navigate between sectors, encounter and fight Morning Light Mountain scouts, mine resources from asteroid fields, and see their state persist across sessions. Simultaneously, a script can connect via WebSocket, send JSON commands, and receive JSON state updates — proving the dual-interface architecture works.

---

## Appendix A: Open Questions

1. **Hex rendering in terminal** — what's the best ASCII/Unicode approximation for a hex grid that looks good in a monospace font? Flat-top hexes with `/` `\` `_` characters? Or abstract to a node graph where hexes are represented as labeled points with connection lines?

2. **Policy condition language** — how expressive should this be? A simple enum of conditions with AND/OR might be enough. A mini expression language adds power but also parser complexity.

3. **Fleet splitting** — can a player split a fleet into two? This adds tactical depth (send scouts ahead, keep main fleet back) but complicates the UI and state management. Defer to M3+?

4. **Fog of war** — sectors you haven't visited are unexplored. Do adjacent sectors get revealed by proximity? Does the Sensor Array building reveal a radius around home? How much map info does a player get by default?

5. **Resource trade** — can players trade resources with each other at the hub? This is a natural multiplayer feature but needs economic balancing.

6. **Time acceleration** — for testing and development, should the server support variable tick rates? Useful for simulating long build times.

## Appendix B: LLM Agent Reference Strategy

A reference LLM agent implementation should demonstrate:

1. **Connection and state parsing** — connect via WebSocket, parse tick updates.
2. **Policy-based play** — maintain an auto-action table for routine decisions.
3. **Strategic planning** — periodically evaluate overall strategy (what to build, where to explore, when to upgrade policy).
4. **Context window management** — keep a rolling window of last N tick states for decision-making, not full history.
5. **Cost efficiency** — minimize API calls by relying on the policy table, only actively reasoning when the policy signals "novel situation."

Example agent loop:
```
while connected:
    state = receive_tick_update()
    if policy_handles(state):
        continue  // auto-action table handles this tick
    if state.has_alert or state.novel_situation:
        decision = llm_reason(last_5_states, current_policy, fleet_status)
        if decision.update_policy:
            send(policy_update)
        if decision.immediate_action:
            send(command)
    every N minutes:
        strategic_review = llm_reason(homeworld_status, research_tree, economy)
        send(build_commands or research_commands)
```
