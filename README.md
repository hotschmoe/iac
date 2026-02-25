# IN AMBER CLAD

**A multiplayer space strategy game played through an amber terminal.**

You are an admiral at a CRT console, commanding fleets across an infinite hex grid. Mine resources, build your homeworld, research technology, and push deeper into the wandering — where the map grows sparse, dead-ends can trap you, and Morning Light Mountain swarms hunt in the dark.

Humans play through a retro amber TUI. LLM agents connect over WebSocket and play the same game via JSON. Both are first-class citizens.

## Vision

In Amber Clad (IAC) sits at the intersection of classic browser space strategy (OGame), mobile space MMOs (The Infinite Black), and a new idea: games designed from the ground up to be played by both humans and AI agents.

The game is built on a few core beliefs:

- **The terminal is beautiful.** A single amber palette, CRT glow, box-drawing characters, and thoughtful layout can be more evocative than a full 3D engine.
- **Strategy over reflexes.** Fleet command, resource optimization, and exploration decisions matter more than click speed. A 1–2 second tick rate hidden inside cooldowns makes real-time play feel responsive to humans and tractable for LLMs.
- **AI players are real players.** LLM agents don't get a dumbed-down API. They see the same game state, obey the same rules, and compete in the same universe. Their unique advantage — tireless optimization — is balanced against their weakness: slow reaction and high cost per decision.

## Game Overview

### The Map

An infinite hex grid radiates outward from a central hub. Players spawn on the **inner ring** — a relatively safe zone with easy enemies and modest resources, meant for early progression. Moving inward toward the **central hub** connects you to trade and community. Moving outward takes you into **the wandering** — procedurally generated, increasingly dangerous, and increasingly rewarding deep space.

Not every hex edge connects. Dead-end branches, chokepoints, and isolated pockets are generated procedurally, growing more common the further you venture outward. The inner ring is well-connected and navigable. The wandering is a maze.

### Progression

You start with **two scout ships** and a **homeworld claim** on the inner ring.

**Early game:** Mine asteroids, fight weak NPC fleets near your homeworld, learn the combat and navigation systems. The inner ring is forgiving.

**Mid game:** Your homeworld mines produce baseline resources. You build a shipyard, research lab, and grow your fleet to 5–10 ships. Inner ring resources become negligible compared to homeworld production. The wandering calls.

**Late game:** Deep expeditions into the wandering for rare loot, dangerous Morning Light Mountain encounters, and eventually — PvP in the dark forest zones.

The game never pushes you forward. You outgrow each phase naturally as your economy scales.

### Resources

Three resources drive everything, inspired by OGame:

| Resource | Role |
|---|---|
| **Metal** | Primary building material. Ships, structures, defenses. Abundant. |
| **Crystal** | Electronics and advanced components. Research, upgrades. Less common. |
| **Deuterium** | Fuel. Fleets consume it on every expedition. The strategic bottleneck. |

Resources come from two sources: **passive production** (homeworld mines, always running) and **active harvesting** (mining in sectors and salvaging combat wreckage).

### Deuterium & Fleet Range

Fleets consume deuterium when deployed. How far you can push into the wandering is directly limited by your fuel capacity and efficiency — both improvable through research. This is the natural leash on expansion.

**Emergency Recall:** At any time, a deployed fleet can attempt a blind jump directly back to the homeworld. This burns extra deuterium and carries a stochastic risk of damaging or destroying ships in the fleet. It's the panic button — expensive, dangerous, but better than losing everything.

### Combat

Fleet-based, stochastic, resolved in rounds that map to game ticks.

Each tick during combat: every ship in both fleets fires at a random enemy target. Damage is calculated against shields, then hull. Ships with rapid-fire bonuses get chances for additional shots (à la OGame). Ship composition matters — corvettes swarm, frigates tank, cruisers hit hard. Rock-paper-scissors dynamics emerge from stat profiles.

### Loot

Destroyed enemies yield three categories of loot:

- **Salvage** — raw resources (metal, crystal, deuterium) scaling with enemy strength
- **Components** — ship-class-specific upgrades (better corvette shields, better cruiser weapons)
- **Data Fragments** — contribute toward research unlocks at your homeworld lab

Every fight feeds multiple progression systems. Optimizing what to farm and where is a core strategic decision.

### Homeworld

Your persistent base on the inner ring. OGame-style building and upgrade system:

- **Mines** (metal, crystal, deuterium) — passive resource production
- **Shipyard** — build queue for new ships
- **Research Lab** — unlock technologies using data fragments + resources
- **Fuel Depot** — multiplicative bonus to fleet fuel capacity (+10%/level)
- **Defenses** — protect your homeworld (relevant once PvP exists)

Build timers are measured in minutes to hours. This is the slow, strategic layer — ideal for LLM agents who check in periodically.

### The Enemy: Morning Light Mountain

The procedural NPC faction occupying the wandering. Swarm-based, scaling in power with distance from the hub. Encounters range from minor patrols (a few light ships) near the inner ring to full swarm incursions deep in the wandering that can overwhelm unprepared fleets.

## Architecture

```
┌─────────────────────────────────────┐
│           GAME SERVER (Zig)         │
│                                     │
│  ┌───────────┐  ┌───────────────┐   │
│  │ Simulation │  │   SQLite DB   │   │
│  │  Tick Loop │  │  (persistent  │   │
│  │  (1-2s)   │  │    state)     │   │
│  └─────┬─────┘  └───────┬───────┘   │
│        │                │           │
│  ┌─────┴────────────────┴─────┐     │
│  │      WebSocket Server      │     │
│  └─────┬──────────────┬───────┘     │
└────────┼──────────────┼─────────────┘
         │              │
    ┌────┴────┐   ┌─────┴─────┐
    │ TUI     │   │ LLM/CLI   │
    │ Client  │   │ Client    │
    │ (zithril│   │ (JSON)    │
    │ + rich_ │   │           │
    │   zig)  │   │           │
    └─────────┘   └───────────┘
```

**Server:** Zig. Runs the authoritative simulation as a fixed-rate tick loop. All game logic lives here. Persists state to SQLite. Broadcasts state deltas to connected clients over WebSocket.

**TUI Client:** Zig, built on zithril and rich_zig. Renders the amber terminal interface. Sends player commands to the server. This is the human interface.

**LLM/CLI Client:** Connects over the same WebSocket protocol. Sends JSON commands, receives JSON state. Supports an **auto-action policy table** — a set of if/then rules the agent maintains and updates, so it doesn't need to respond every tick. The agent actively intervenes only when novel situations arise.

### Auto-Action Policy System

LLM agents (and human players) can define conditional action rules:

```json
{
  "policy": [
    {"if": "hostile_in_sector AND fleet_shield < 0.3", "then": "emergency_recall"},
    {"if": "hostile_in_sector AND fleet_shield >= 0.3", "then": "attack_nearest"},
    {"if": "sector_clear AND cargo < cargo_max", "then": "harvest"},
    {"if": "cargo >= cargo_max", "then": "navigate_home"}
  ]
}
```

The server evaluates these policies each tick when no explicit command is queued. Agents review and revise their policy based on recent outcomes. This transforms LLM gameplay from "respond every second" to "think strategically, update policy, observe results."

## Tech Stack

- **Language:** Zig 0.15.2
- **TUI Rendering:** [zithril](https://github.com/hotschmoe/zithril) (wraps [rich_zig](https://github.com/hotschmoe/rich_zig) internally)
- **Database:** SQLite via [zqlite](https://github.com/hotschmoe/zqlite)
- **Networking:** WebSocket via [webzocket](https://github.com/hotschmoe/webzocket)
- **Reliability:** RaptorQ for state broadcast (optional/future)

## Development Milestones

### M1: Core Loop (Complete)
Single player, client-server architecture over WebSocket. Hex grid with procedural generation and edge pruning. Movement, NPC combat (stochastic rapid-fire), resource harvesting. Amber TUI via zithril. JSON CLI interface for LLM agents. SQLite persistence via zqlite. Windshield hex node graph, star map with zoom levels, event log, NPC patrol AI, resource regeneration, emergency recall.

### M2: Economy & Progression (Complete)
Homeworld buildings with leveled production (metal mine, crystal mine, deut synthesizer, shipyard, research lab, fuel depot, sensor array, defense grid). Ship construction via shipyard queue. 12-technology research tree gating ship classes and providing stat modifiers. All balance formulas centralized in `scaling.zig`.

**Implemented:** `scaling.zig` as single source of truth for all production rates, build costs/times, prerequisites, research costs/times/prerequisites, and modifier functions. Building levels with `production_per_tick = base_rate * level * 1.1^level`. Research modifiers: fuel efficiency (-10%/lvl), extended tanks (+15%/lvl), reinforced hulls/shields/weapons (+10%/lvl), navigation (-1 tick cooldown/lvl), harvesting efficiency (+20%/lvl), emergency jump damage reduction (-5%/lvl). Ship stats baked at creation time with research bonuses (OGame model). Three independent queues (building, ship, research) processed each tick. Prerequisite chains enforced server-side. Cancel refunds 50% of remaining cost. DB persistence for buildings, research, and queues. Tab-based homeworld UI with card selection grid (Buildings/Shipyard/Research panels), prerequisite indicators, cost/time display, tech tree overlay, and queue status bar. Research levels wired from server to client. Full command routing for build/research/build_ship/cancel_build via WebSocket.

### M3: Multiplayer (Complete)
Multiple concurrent players in a shared universe. Homeworld minimum 2-hex separation. 3-fleet cap per player with docked ship auto-merge on return. Shared sector state (depletion, NPC kills visible to all). Player visibility (see other fleets in your sector with ship class breakdown via `FleetBrief`). Co-op combat (pooled allied ships vs pooled enemies, single engagement per sector). Per-player event visibility filtering (own fleet/homeworld + sectors where fleet is physically present). Allied fleet rendering on star map (`A` symbol) and in sector info panel.

**Implemented:** Multi-fleet `Combat` struct with bounded arrays. `resolveCombatRound` takes fleet pointer slices with `ShipRef` indirection for cross-fleet targeting. `startCombat` joins existing sector combats or creates new ones; `enrollAllPlayerFleetsInSectorCombat` sweeps idle fleets into active engagements. `buildSectorState` populates `player_fleets` (excluding own) with per-class ship counts. `broadcastUpdates` sends sector state for all fleet locations (not just first). `isEventRelevant` filters by fleet ownership and sector presence. Fleet cap enforced in `handleMove` (homeworld departure only). Auto-merge in `dockFleet` absorbs all other homeworld fleets. `deleteFleet` DB method for merged fleet cleanup. Auth catches registration failure when no homeworld locations available.

### M4: Deep Systems
Loot system (salvage, components, data fragments). Morning Light Mountain faction with scaling difficulty. Auto-action policy system. LLM agent reference implementation.

### M5: PvP & The Dark Forest
PvP zones in the outer wandering. Corporation/alliance system. Homeworld defenses. Territory control.

## Playing

### Build
```bash
zig build              # Build both server and client
zig build server       # Build server only
zig build client       # Build client only
```

### Run
```bash
# Start the server (default port 7777)
zig-out/bin/iac-server

# Connect with the TUI client
zig-out/bin/iac-client
```

### LLM Agent (JSON/WebSocket)
```bash
# Connect via WebSocket to ws://localhost:7777
# Authenticate, then send commands and receive state as JSON
```

Example agent interaction:
```json
// Server → Agent (state update each tick)
{
  "tick": 4821,
  "fleet": {
    "sector": [12, -3],
    "ships": [{"class": "corvette", "hull": 100, "shield": 80}],
    "cargo": {"metal": 45, "crystal": 12, "deuterium": 8},
    "fuel": {"current": 340, "max": 500}
  },
  "sector": {
    "type": "asteroid_field",
    "hostiles": [],
    "resources": {"metal": "rich", "crystal": "sparse"},
    "connections": [[12,-2], [13,-3], [11,-2]]
  }
}

// Agent → Server (command)
{"action": "harvest", "target": "metal"}
```

## Name

**In Amber Clad** — named for the amber glow of the CRT terminal through which you command your fleet. Everything you see, every tactical readout and sector scan, arrives clad in amber light.

**IAC** for short.

## License

TBD
