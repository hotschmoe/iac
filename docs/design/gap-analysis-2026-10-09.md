# In Amber Clad: gap analysis against OGame and The Infinite Black

Date: 2026-10-09. Scope: research and writing only; no code was changed.

**Source note.** OGame facts come from Wikipedia, Gameforge's own moon page, and the ogame.life guides. The Fandom wiki and wiki.ogame.org were unreachable (HTTP 402, no DNS). The Infinite Black (TIB) is thinly documented on the open web, and I could not find its death-penalty rules or ship-class tables. Anything marked **[inferred]** is my own design judgment or recall, not a cited fact. Anything marked **[unverified]** is a number I remember but could not confirm. IAC numbers are quoted from `shared/src/scaling.rs`, `shared/src/constants.rs`, the README and the 2026-10-09 playtest.

---

## 1. The two inspirations

**OGame.** Core loop: mines produce metal, crystal and deuterium continuously. Energy gates the mines, and an energy shortfall throttles all of them proportionally ([calculator notes](https://west-games.com/ogame-resource-calculator/)). Base metal output is 30 x level x 1.1^level per game-hour, times the universe speed. You spend resources on a build queue, a research queue and a shipyard queue, then send fleets for attack, transport, deploy, espionage, recycling, colonisation and expeditions. Combat is automatic, up to six rounds with no unit control ([Wikipedia](https://en.wikipedia.org/wiki/OGame)). Wrecks become a debris field worth 30% of destroyed ship cost in standard universes (widely cited community figure; some universes use 40%; I did not reach a primary page), which can spawn a moon (1% at 100k debris, up to 20% at 5M; [Gameforge](https://gameforge.com/en-GB/games/ogame-moon.html)). Pillars: (1) compounding economy with exponential costs, (2) "if it sits, it gets hit": stockpiles and parked fleets are loot, so you must act or hide, (3) a strong spy/phalanx information game, (4) alliances, (5) a long-horizon meta with slot-limited expeditions as a risk/reward gamble. **Time:** pure build and flight timers, no real-time control. Sessions are 5 to 15 minutes, several times a day. The designed rhythm is "queue something that finishes when you will next log in." Speed universes compress the same curve. The known pain is the fleet-save ritual: sending fleets out to land after you wake, and being hit at predictable times ([fleet-save guide](https://ogame.life/ogame/blog/ogame-fleet-save-guide-how-to-protect-your-fleet-while-offline)). Players also complain of attacks "spoiling the fun" for casuals and of a slow, time-consuming start ([German reviews via yopi.de](https://www.yopi.de/testbericht/ogame_de_Websites/265878), anecdotal).

**The Infinite Black.** Core loop: pilot your own ship through a network of linked sector nodes (an 80x80 sector galaxy per [community wiki](https://www.spellbook.com/tib/wiki/index.php?title=Sectors) as summarised by search; maps are mostly 15x15 with missing sections and sectors appear as you travel, per a [Steam guide](https://steamcommunity.com/sharedfiles/filedetails/?id=3025450355)). You tap targets, auto-follow NPC ships that hop around, and carry a jump-drive cooldown and a waypoint route for long trips ([Android blog](http://androidmmorpggames.blogspot.com/2013/07/the-infinite-black.html)). Safe zone around Earth, then "the grey" (PvE farming), then "the black" (PvP-flagged; stronger NPCs, better drops; players are advised to reach about level 10 to 15 first; [MMOs.com](https://mmos.com/review/the-infinite-black)). Security ratings S1 to S9 scale NPC strength, level and maximum count per sector. Wrecks drop salvage and items sellable at starports or a server auction house. Alliances build garrisons in the black to repair and store goods and to keep outsiders out of a "yard." Pillars: (1) danger rises with distance from the safe core, (2) a tangible per-ship feel (loot, parts, many stat combinations), (3) corp/alliance politics, (4) the "mugged in the black" tension of carrying farmed loot. Pain points reported: slow pace, plain visuals, hardcore audience, a basic mobile UI where chat is a separate window and you can be attacked while chatting, and a developer perceived as having abandoned it. **Time:** real-time with jump cooldowns (seconds), so it is a "play now" game, but its sessions are short bursts like a phone game. **[inferred]** Its death penalty details are not in the sources I could reach, so I do not rely on them.

**The hybrid IAC is aiming at.** OGame supplies the economy, timers, research tree and the "my base is waiting" pull. TIB supplies the where-am-I-and-how-dangerous-is-it map and the loot/salvage hunt. IAC's tick-hidden-in-cooldowns idea (1 Hz server, 1 to 5 tick cooldowns) is TIB-flavoured piloting. Its 130 to 675 tick building timers (minutes) are OGame-flavoured, but scaled for a one-hour session, not days. That mismatch is the main pacing question in section 4.

---

## 2. What IAC has, mapped to both games

Quality: **good** = works and is fun in playtest; **ok** = present but thin; **weak** = present but undermines play; **none** = absent.

| Mechanic | OGame | TIB | IAC today | Quality |
|---|---|---|---|---|
| Mines producing 3 resources | Yes, energy-gated, exponential cost | Not a core loop | Metal/Crystal/Deut mines, `0.5/0.3/0.15 x L x 1.1^L` per tick, no energy, no storage cap | ok (no cap or energy means no tension) |
| Build queue | 1 queue per planet, reducible by robotics/nanites | n/a | 1 slot, `base x L x 1.5^L` ticks (Metal L3 = 303, Shipyard L3 = 607) | weak (serial, steep) |
| Research | Large tree, labs, shared queue | Item/upgrade grind | 12 techs, max 5 levels, `60 x L x 1.5^L` ticks | ok (tiny tree) |
| Shipyard | Many ships + defences, queue | Buy/unlock ships | 5 classes, build time `base / (1 + 0.1 x SY)` | ok |
| Fleet missions | 8+ mission types | Piloting only | Move, harvest, scan, attack, salvage, boarding, recall | ok (no transport/deploy/espionage vs players) |
| Real-time piloting | None | Core | Hex lanes, move cooldowns, 3 fleets max | good |
| Map and danger gradient | Galaxy coordinates, no gradient | Safe, grey, black; S1 to S9 | Hub / inner (<=8) / outer (<=20) / wandering, edge pruning, NPC respawns 300/600/1200 | ok (cliff at ring 8 to 9) |
| Combat | Rapid fire, 6 rounds, auto | Targeted real-time | Rounds, shields first, rapid fire (Corvette x3 vs Scout, Frigate x2 vs Corvette, Cruiser x2 vs Frigate) | good vs NPCs; PvP absent |
| Debris/wreck salvage | Debris 30% of ship cost | Wreck pickup | `SALVAGE_FRACTION 0.30`, despawn 60 ticks | good (but event/pile mismatch bug) |
| Loot and items | Expedition finds | Core (parts, items) | Derelicts tier 1 to 3 with loot, ship recovery, data core | ok (resources only, no items) |
| Expeditions | Slot-limited gamble (about 1 in 3 resources, 1 in 5 ships; [ogame.life](https://ogame.life/ogame/blog/ogame-expeditions-the-complete-guide-to-fleet-composition-rewards-and-automation)) | Exploring unknown sectors | Prospect + derelicts approximate this | ok |
| PvP | Core (attack, farm, raid) | The black | None (fleets in sector are allied; raids are NPC) | none |
| Defence | Defence buildings, rebuilt 70% | Garrisons | Defense Grid, NPC raids with 60-tick warning | ok |
| Newbie protection | Point-ratio rules | Safe zones | Not needed yet (no PvP) | n/a |
| Colonies/moons | 9+ planets, moons, phalanx, jump gate | Alliance planets | One homeworld only | none |
| Alliances/corps | Core | Core | None | none |
| Trade/market | Merchant, trader | Auction house | None | none |
| Repair/upkeep | Instant after battle for defences | Starport repair | Hull damage persists, only docking helps (Grok-1: recall damage "never healed") | weak |
| Standing orders/automation | Scripts banned, third-party | Auto-follow | 5 policy presets with audit trail | **good, a differentiator** |
| Info/espionage | Probes, phalanx | Fog, grey/black visibility | Scan with 2-hop reveal, signal pings, SensorArray | good |
| Energy and storage | Core constraints | n/a | None | none |

---

## 3. Gaps

Sizing: S = a day or less, M = a few days, L = a week or more. Evidence tags refer to the six playtest feedback files in `docs/playtests/2026-10-09/`.

**G1. No economic tension: no energy, no storage caps, no consumer sink for deuterium (M).**
*Why it matters.* OGame's loop is fun because the economy has shape: energy competes with mines, storage caps force spending, raiders punish hoarding. IAC mines just accumulate. The only reason to spend is "everything is slow." Deuterium (0.15 base) is the silent bottleneck in four of six reports (Sonnet-1, Grok-1, Pi-2, Grok-2) because everything costs it and nothing stable makes it.
*Proposal.* Add storage caps per resource (a Depot-style building; cap at roughly 2 hours of production at base, rising 1.5x per level), which also gives raids something meaningful to skim. Skip energy for now **[inferred]**: it adds a fourth resource and another UI tab for modest depth. Instead, make deuterium a real flow: refuelling costs deuterium (Pi-1 observed refuel looks free), and Fuel Depot raises capacity, not cost. Rebalance deuterium production to about 0.25 base (see section 4).

**G2. One build queue, steep exponential times, tiny tree (M).**
*Evidence.* Every player: "about four upgrades an hour." Sonnet-1 and Sonnet-2 asked for a second queue. Frigate, Cruiser and Lab 3 were out of reach for everyone.
*Why it matters.* OGame's fun is queue depth: you queue and leave. IAC's single slot with 600-tick levels creates dead time without a decision. Costs are linear (`n x 60`) but times are `1.5^L`, so after L4 time, not resources, is the binding constraint and resource decisions vanish.
*Proposal.* (a) Add a **Fabricator** building (OGame's robotics factory, **[inferred]** mapping) that divides building time by `1 + 0.15 x L`. (b) Raise queue depth to 3 entries (queue-ahead, only the front consumes resources, as OGame does). (c) Parallel slot per category: one building, one research, one ship queue already exist; keep them separate. (d) Move the time exponent from 1.5 to about 1.35 and the cost exponent from linear to 1.25 so both pull on decisions.

**G3. The outer ring is a cliff, not a gradient (S to M).**
*Evidence.* Grok-1 lost its fleet at distance ~10; Grok-2 lost 5 scouts at distance 9 in one tick; Pi-2 saw a "heavy" band out-damage 2 corvettes in one round. The weapon progression Scout 5, Corvette 15, Cruiser 80 is steep.
*Why it matters.* TIB's whole identity is a readable danger curve (security rating S1 to S9, safe/grey/black). IAC has zones, but the player cannot see danger until it kills them.
*Proposal.* Show a **threat rating** per sector (S-rating style, 1 to 9 derived from distance and NPC power) on the map, via scan, and in the move confirmation: "Sector 9,-8: threat 5; your fleet power 14; est. NPC power 40." Also smooth NPC tiers: add a mid tier at distance 6 to 8 so power ramps about 1.5x per ring step, not 3x at once. Preventing silent death by information beats nerfing.

**G4. Fleets can be lost to logistics, not combat (S).**
*Evidence.* Pi-2: 3 ships and full cargo permanently stranded one hop from home with 4/160 fuel. Pi-1/Grok: `mine_and_return` wandered out of `max_range`. Starting fleets show 50000/50000 fuel, then collapse. (Journal says autopilot range and stranding recovery are already addressed; this item is about making recovery a feature.)
*Proposal.* Add **rescue**: a docked fleet may dispatch a tanker (a Hauler) to a stranded fleet in range, carrying fuel. This is cheap and creates a story. Also a visible "fuel to return" gauge on every fleet card. Keep emergency recall as the expensive escape hatch.

**G5. No player-vs-player interaction at all (L, and identity-defining).**
*Why it matters.* Both inspirations are fundamentally multiplayer-conflict games. In IAC, humans and agents share a world but meet only as event noise. Raids are NPC-only and "never a random tax," a good rule that makes the game safe but also flat.
*Proposal (staged).* Stage 1 (M): **espionage and visibility**: a `scout_player` command returning homeworld snapshot (buildings, defence, fleet), plus a leaderboard. Stage 2 (M): **trade**: send a Hauler fleet to another homeworld with a transfer (cargo-limited). This is the simplest cooperative primitive and it exposes the risk of being intercepted later. Stage 3 (L): **opt-in PvP in the Wandering only** (distance > 20, TIB "black" analogue), with wreck-loot and a cargo-drop rule (section 8, Q2). Protection rules need numbers; mimic OGame's idea that a strong player should not farm a weak one (a 5x power-ratio rule is cited for sub-5000 points in [OGame discussion](https://ogame.fandom.com/wiki/Talk:Newbie_Protection), unverified for current game).

**G6. Single homeworld, no expansion (L).**
*Why it matters.* OGame's mid-game is "more planets." IAC's mid-game, per the playtest, is "wait on one queue." There is nothing to reach for after hour one.
*Proposal.* **Outposts**: claim a rich sector with an Outpost Module ship; it gets its own small mine (cap L5), its own storage, and is raid-able. This also gives `mine_and_return` a better target than a seam that depletes to None. Prefer outposts over full second planets **[inferred]** because they fit the hex map and TIB's garrison idea.

**G7. Sector regeneration is effectively zero; ore near home runs out (S).**
*Evidence.* Grok-2: `SECTOR_REGEN_RATE 0.0001` is invisible in a one-hour game; stripped tiles stay None. Grok-1: all six neighbours read None by mid-game. Pi-1: NPC salvage (200/50/30 flat advertised, 60/15/9 collected) beats the mines.
*Proposal.* Regenerate toward the base density on a time scale of about 30 to 60 minutes for Sparse to Moderate (about 0.0005 to 0.001 per tick), and add **richness drift** so new Rich tiles appear at the frontier. Tie NPC loot to the NPC's class and ring (a ring-9 corvette pack should pay about 4x a ring-2 scout). Without this, salvage farming dominates the economy.

**G8. Information design: opaque costs, errors, events (S, mostly done).**
*Evidence.* The most reported finding in the playtest (all six). The journal says a homeworld catalog and specific errors are fixed. Remaining: event ownership (fixed per-player scoping), `FleetDestroyed` advertised salvage vs pile (fixed event numbers), `NoResources` should name the missing resource.
*Proposal.* Keep an `affordable_now` and `next_unlock` block in `full_state` (see section 6). Test with a fresh agent that never reads the source.

**G9. No repair or upkeep loop (S to M).**
*Evidence.* Grok-1: "hull damaged from recall never healed." Pi-2: no consequence of damage besides loss.
*Proposal.* Docked fleets repair at a rate set by Shipyard level (for example 1% hull per tick per level up to full). Add a small deuterium cost to repair above 50% damage. This restores TIB's "limp home" tension and adds a sink.

**G10. No long-term goals or reasons to log in tomorrow (M to L).**
*Why it matters.* OGame is compelling over weeks via ranking points, alliance wars and the next tech tier. IAC has no scoring.
*Proposal.* A **points score** (resource value of buildings, research and ships, as OGame does) with a leaderboard, plus a **daily bounty** (a world event: "Tier-3 derelict transponder appeared in sector X") that creates a shared goal across humans and agents. **[inferred]** Cheap to build from existing derelict worldgen.

**G11. Combat has too little decision space (M).**
*Evidence.* Corvette dominance (Pi-2, Pi-1: 14 corvettes, zero losses). Only five ships; Frigate and Cruiser unreachable.
*Proposal.* A rock-paper-scissors layer: add Bomber/Interceptor-style variants or a **stance** per fleet (aggressive / defensive / retreat at X% hull). A retreat threshold directly helps agents. Rapid-fire already gives a loose counter table; extend it so Frigates counter Corvettes and Haulers are not free kills.

**G12. Expedition-like gamble missing (M).**
*Proposal.* A **long-range survey** command: send a fleet to an unexplored node beyond range; it returns after N ticks with a probabilistic outcome table (resources, a derelict location, ship recovery, ambush, loss). OGame's expedition table is the template (resources about 1 in 3, abandoned ships about 1 in 5, black hole 0.3% [unverified current rates]). IAC already has the derelict ambush logic to reuse.

---

## 4. Pacing and balance targets

**Current baselines (quoted):** start 500 / 300 / 100; production at Metal L3 = 2.00/tick (`0.5 x 3 x 1.1^3`), Crystal L3 about 1.20, Deut L3 about 0.60; metal mine L1 = 0.55/tick; build time `30 x L x 1.5^L` for mines (L1 = 45, L2 = 135, L3 = 303, L4 = 607, L5 = 1,139, L6 = 2,050 ticks); Shipyard/Lab base 60 (Lab L2 = 270, Shipyard L3 = 607); ship build times base 30/60/120/240/90 ticks for Scout/Corvette/Frigate/Cruiser/Hauler, divided by `1 + 0.1 x SY`; ship costs Scout 200/50/30, Corvette 400/100/60, Frigate 1000/400/200, Cruiser 3000/1500/800, Hauler 600/200/150; cancel refund 50%; sector regen 0.0001/tick; raid rolls every 300 ticks at 20% (min interval 900); NPC respawn 300 / 600 / 1200 ticks by ring; scan cooldown 5 ticks; fuel 0.1 per mass per hex.

**What the numbers produce.** In a one-hour session (3,600 ticks) a single queue yields about 12 to 14 total building levels (confirmed: Pi-1 14, Grok-1 13, others 12 to 13). A Cruiser needs 3,000 metal at 2/tick, which is 1,500 ticks of mine income. Buildings extrapolate wildly: Metal L10 takes 17,300 ticks (4.8 h), L14 about 122,000 (34 h), L16 about 315,000 (3.6 days), L18 about 800,000 (9 days). So on a single queue the exponential 1.5 is only viable for a one-hour world, while a week-long world (about 600,000 ticks) would stall at roughly L15 to L17 for each mine, and the queue could not complete a whole tree.

**Recommended frame: one tuning constant, `WORLD_SPEED`.** Divide all build, research and ship times, flight cooldowns and NPC respawns by it, and multiply production by it, like OGame speed universes **[inferred mapping]**. Define two shipped presets:

| Preset | Purpose | Session feel | WORLD_SPEED vs today |
|---|---|---|---|
| **Blitz (1 hour)** | Playtests, agent evals, demos | Whole game arc in 60 min: first mine in 20 s, first fleet at 5 min, Cruiser by 50 min | x3 to x4 on time |
| **Standard (7 days)** | Real shared world | Log in 3 to 6 times a day for 10 minutes; each login: collect, re-queue, one expedition | x1 on production, x0.1 on time (i.e. longer) |

Within either preset, the shape matters more than absolute values:

- **First 5 minutes:** one visible decision every ~30 s. Target: Metal L2 in about 40 ticks, first Scout shuttle running by tick 120. Today Metal L2 takes 135 ticks (L1 to L2 only).
- **Queue depth:** 3 queued items per queue (today 1). The front consumes resources.
- **Build-time curve:** change exponent 1.5 to about 1.35 (L5 = about 700 instead of 1,139; L10 = about 4,300 instead of 17,300 ticks). With a Fabricator at L5 (divisor 1.75) that is about 2,500 ticks at L10, i.e. about 40 minutes: right for a blitz world where mines top out near L10 at the end of the hour.
- **Cost curve:** costs are linear in level while times are exponential. Move mine costs to `base x 1.25^L` so resources, not just waiting, gate progress. Metal L10 would then cost about 60 x 9.3 = 560 vs 600 today at that level, but the early levels stay cheap: this barely changes costs near L10 but raises them above L12, preventing runaway.
- **Resource sinks and sources:** *Sources:* mines (primary), harvesting (secondary, depletes), NPC salvage (30% of ship cost, dominant today per Pi-1), derelicts (tier loot ranges 300 to 700 metal at tier 1; 2,000 to 3,500 at tier 3). *Sinks:* build, research, ships, fuel (add refuel cost), repair (new), cancel refund (50%), raid loss (cap 8% / 8% / 5%). Today the main sink is the build queue, which is time-limited, so stockpiles pile up. Target: stockpile worth at most about 20 minutes of production at all times (needs the storage cap from G1).
- **Deuterium:** raise base from 0.15 to about 0.25 and cut Deut Synthesizer cost from 225/75 per level to about 150/50, so L3 yields about 1.0 per tick instead of 0.6. Pair this with a refuel cost so the shift is a sink not a free gain. Rationale: it was the bottleneck for four of six players at 0.17 to 0.6/tick.
- **NPC salvage vs mines:** make drops depend on class and ring. A ring-1 scout drop worth about 60/15/9 (already the pile) should stay; a ring-9 corvette pack should pay about 250/60/40 so risk is rewarded. Today advertised 200/50/30 flat is a bug-shaped incentive.
- **Risk gradient by distance** (blitz preset, relative to current NPC respawn of 300/600/1200 ticks by ring): ring 1 to 3: threat 1 to 2, scouts only, respawn 300; ring 4 to 8: threat 2 to 4, lone corvette packs (mid tier), respawn 450; ring 9 to 20: threat 4 to 7, heavy bands, respawn 600; wandering: threat 7 to 9, respawn 1,200. Derelict tier already follows 1/2/3 at 0 to 8 / 9 to 20 / 21+; tie the ambush chance (10/20/30%) to the same threat rating so scan shows it.
- **Fuel:** keep 0.1 per mass per hex; make the Fuel Depot raise capacity by 25% per level (today +10%) because Pi-1 measured it as marginal (470 to 517 tank). Show "hops of range" on each fleet card.
- **Raids:** keep (they are well designed), but lower the minimum player age from 600 ticks to about 300 so they actually occur in a one-hour session (Playtest: "raids barely happened"; Grok-1 deliberately avoided Defense Grid to avoid them, which is a perverse incentive and should be countered by offering a bonus for repelling, e.g. above the 30% salvage fraction).
- **One-hour vs week feel:** in the one-hour world the player should reach "first Cruiser or tier-2 derelict loot" by minute 45 and have a decision queued at all times. In the week world, a returning player should find things worth doing (a finished queue, a raid report, a derelict ping) in under 10 seconds from login, and should never *need* to log in during the first 8 hours (OGame-style "sleep safe").

---

## 5. Human UX, web-first

The web client should feel like a TIB sector view plus an OGame overview, not a command bar.

**What the two games teach.** OGame's strength is the **overview**: planets, queues with countdowns, incoming fleets and a single "next thing" flag, readable in seconds ([fleet-save guide](https://ogame.life/ogame/blog/ogame-fleet-save-guide-how-to-protect-your-fleet-while-offline) shows how much players rely on arrival times). TIB's strength is **spatial awareness**: a node graph with red highlighted hostiles, tap-to-target, auto-follow and waypoints ([blog](http://androidmmorpggames.blogspot.com/2013/07/the-infinite-black.html)). Its weakness, per the same source, was a plain UI with chat in a separate window.

**Web UI needs (priority order):**
1. **Overview screen** (home): resource bars with per-tick rate and time-to-cap, three queue cards with live countdowns and a "queue next" button, a fleet strip with fuel-in-hops, an alerts rail (raid ETA, fleet lost, queue idle). Idle queue should be a visible, slightly alarming state.
2. **Costed build/research cards** showing cost, time, prerequisite, and reason if disabled ("needs Metal Mine L2"). Pi-2 and Sonnet-1 asked for exactly this; the catalog now exists, so the UI should render it directly.
3. **Sector map as a real map**: pan/zoom hex graph, lane edges visible (the TUI map already draws these), threat colouring (G3), freshness fading, ping glyphs, click-to-route (BFS exists in the TUI client and should be shared), a route cost preview (fuel, ticks, threat).
4. **Fleet panel**: split/merge buttons, doctrine dropdown with parameter sliders, and a per-fleet audit log (the PolicyAction trail is a differentiator; present it as a feed with reasons).
5. **Event feed filters**: mine only, combat, economy. Per-player scoping is fixed; add filtering and sound/vibration on important alerts.
6. **Notifications/session hooks**: browser notification or web push when a queue finishes or a raid is within 60 ticks. This is the mobile-session behaviour both games rely on **[inferred]**.
7. **Onboarding**: a 3-step guided start (first mine upgrade, first scan, first harvest), since the playtest showed new players guessing.

Keep the amber CRT look, but treat it as a theme, not a constraint: legibility first (contrast, big tap targets, no tiny monospace in tables on phone).

**TUI: reduce to a thin operator console.** The owner may simplify it. Recommend keeping: (a) the headless NDJSON mode as the primary supported non-web client, (b) a small `status` view (bank, queues, fleets, alerts) and a `map` view for SSH use. Drop or freeze: homeworld tab trees, zoom levels, and the full windshield with node graph. These are duplicated in the web UI. The TUI's tactical plot work in the journal is valuable as reference for the web map, not as a user-facing product **[inferred]**.

---

## 6. LLM-agent UX (CLI)

**What helped agents (playtest).** `play status/map/sector/events/wait` was called "genuinely good" (Pi-1). `salvage_and_sites` with the PolicyAction audit trail ("scooping wreckage", "returning: fuel 25%") produced the best result: Pi-1 had no ships lost and about 25 NPC kills. The `RULES.md` doc was praised by Grok-2 and Pi-2 for explaining doctrines, raids and boarding. Corvette rapid-fire was legible to agents.

**What hurt agents.**
- Hidden numbers: costs, prerequisites, tank scale (50000 vs 120), error text with no cause. Two agents read the Rust source. Quote: "`PrerequisitesNotMet` and `NoResources` errors give zero detail."
- Parse errors that point at the wrong field (`missing field action` for a partial `params`). (Journal says defaults are in progress.)
- Firehose events: all `CombatRound` events galaxy-wide, `ResourceHarvested` one per resource per tick.
- Forgetting: known sectors dropped after about two minutes, so agents flew "from memory" with an empty map. (Fixed by persistent map memory.)
- Silent loss: fleet vanished with no event (Sonnet-1/2).
- Harness: `wait` cap and tool timeout mismatch (two `wait 90` calls in one command exceed 120 s).
- Doctrine thrash: `prospect` ping-pong, `mine_and_return` relative claim drift.

**A first-class agent CLI should offer** (all **[inferred]** proposals; sizes S unless stated):
1. `iac state --compact`: a one-screen JSON/text snapshot with `affordable_now[]`, `next_unlock[]` (name, cost, missing prereq), `idle_queues[]`, `fleet_range_hops`, `threat_here`. This directly encodes the "what can I do now" question that Sonnet-1/2 asked for with `costs`/`available`. (M)
2. `iac costs <thing> [level]` and `iac tree`: static rules and numbers from the engine's own functions. (S)
3. **Idempotent, structured results** from every command: `{ok, error:{code, field, have, need}, cooldown_ticks}`, so the agent never greps source.
4. `iac wait --until <event|tick|queue_idle> --max 100`: capped under typical tool timeouts, returns on the first event relevant to the player. (S)
5. **Event subscriptions with filters**: `--mine`, `--min-severity`, and a digest mode (`since_tick`) that summarises batches ("3 harvests, 1 scan") to protect context windows. (S)
6. **A dry-run or `plan` command**: `iac plan move fleet 2 -> 9,-8` returns fuel, ticks, threat, expected combat. This lets weaker local models (the Qwen 27B pi agents) avoid cliff deaths. (M)
7. **Fixed, documented policy schema** with defaults and a `policy_describe` command, plus a doctrine "kill-switch" like `stop_all`.
8. **Benchmarkable scenarios**: seeded world presets (Blitz) with a score output (building levels + research + ships - losses) so agent runs are comparable. The existing playtest table already uses that score implicitly. (M)
9. **Shared-world etiquette**: agents and humans share one world; add `owner` and `agent: true` flags to player names so humans can see who is automated, and rate-limit command spam per account.

Design principle **[inferred]**: the agent API is the game's canonical interface. If the web UI can show a field, the NDJSON must contain it; generate both from the same `full_state`.

---

## 7. Prioritised roadmap

**Milestone 1: next (make the existing loop fun and fair; about 1 to 2 weeks).**
1. Threat rating on map/scan/move preview (M). Reason: the largest cause of fleet losses; also required before any PvP.
2. Storage caps + deuterium rebalance + refuel cost (M). Reason: creates economic tension; fixes the top bottleneck complaint.
3. Build queue depth 3, Fabricator, time exponent 1.35 (M). Reason: kills dead time, makes blitz and standard presets possible. Balance pass lands here.
4. NPC loot scaled by class/ring; regen to 30 to 60 minute scale (S). Reason: salvage currently outearns mines.
5. Agent CLI: `state --compact`, `costs`, `wait --until`, event filters (M). Reason: unlocks reliable agent play and testing of everything else.
6. Repair at dock + rescue tanker (S to M).

**Milestone 2: soon (give the web UI and the world a reason to return; 3 to 5 weeks).**
1. Web overview screen, costed cards, map with route preview, notifications (L). Reason: the owner wants web as main; this is the largest player-facing delta.
2. `WORLD_SPEED` constant and Blitz/Standard presets (S to M).
3. Score + leaderboard + daily world bounty (M).
4. Fleet stance/retreat threshold and a second combat dimension (M).
5. Survey/expedition command (M).
6. Player espionage and trade shipment (M).

**Milestone 3: later (identity-defining; 6+ weeks).**
1. Outposts (L).
2. Opt-in PvP in the Wandering with protection rules and loot rules (L).
3. Corporations (shared storage, shared map memory, chat) (L). Reason: both inspirations hinge on alliances, but the base must be fun solo first.
4. Item/part system for ships (L). Reason: TIB's depth, but only after the economy is stable. **[inferred]** it is the lowest priority because it multiplies balance work.
5. TUI reduction (S) can happen anytime in M2.

Order rationale: information and economy first (they are cheap and fix measured pain), then web (the owner's priority and the delivery vehicle for everything else), then social and PvP (they need a stable economy, threat visibility and a score to be fair).

---

## 8. Open design questions for the owner

1. **Persistent world or seasons?** OGame universes run for years and suffer from veterans farming newcomers (see the 5000-point protection rules in [OGame discussion](https://ogame.fandom.com/wiki/Talk:Newbie_Protection)). Seasons (about 7 days, reset, a hall of fame) fit agent evaluation and casual humans far better, and make balance mistakes recoverable. Which do you want?
2. **PvP rules and death penalty.** Options: no PvP (co-op only); opt-in PvP in the Wandering; full open PvP outside the inner ring. And what is lost on death: the ship only, cargo only, or both? Salvage 30% already exists. Elite-style insurance (rebuy at a % fee) is a softer option.
3. **Are agents and humans competing in the same ranking?** If agents outplay humans on repetitive work, humans need a reason to play (stronger real-time play, politics) or separate leaderboards.
4. **How automated should play be?** The standing orders are brilliant for agents but OGame explicitly bans scripts for fleet saves. Should doctrines be capped (for example by slots or by research) so humans can compete?
5. **Real-time vs timers.** Is IAC primarily a short-session timer game (OGame) with piloting as flavour, or a piloting game (TIB) with a base as a hub? This decides whether to speed up cooldowns or lengthen build timers.
6. **Scale of the sim.** Does one world stay small (tens of players, one homeworld each) or grow to a galaxy of hundreds? That decides outposts, alliances and server architecture.
7. **Monetisation or none?** Affects dark-matter-style premium items, officers, and time skips.
8. **Item and loot depth.** Is TIB-style gear (parts, auction house) a core pillar or out of scope? It multiplies content and balance load.
9. **Session pacing for the main preset.** Should the default world be Blitz (1 hour) for demos, or Standard (7 days)? Everything in section 4 hangs on this.
