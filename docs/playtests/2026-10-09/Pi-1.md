# Pi-1 feedback

## 1. Result
- **Resources (final, session end tick 3749):** 863 metal / 629 crystal / 90 deuterium; production 2.00 / 1.20 / 0.36 per tick (mines kept ticking through the final ~2 min wrap-up).
- **Buildings:** MetalMine 3, CrystalMine 3, DeuteriumSynthesizer 2, Shipyard 2, ResearchLab 2, SensorArray 1, FuelDepot 1 (queued late, finished within the last minute). No DefenseGrid (the only thing I never built — gated behind deuterium I had never banked).
- **Research:** CorvetteTech 1, HarvestingEfficiency 1, Navigation 1, ReinforcedHulls 1.
- **Ships:** 9 total — 2 Scouts + 7 Corvettes (fleet hull 415, fuel cap 470), all in one fleet (id 2); fuel fully topped at end (517/517, after FuelDepot L1 finished late: 470 × 1.10 = 517).
- **Achievements:** destroyed ~25 NPC fleets (all wins, zero ships lost in my own fights), boarded derelict sites (tier-1 loot, no ambushes hit), collected constant wreckage, repelled a "moderate" raid (defense 128 vs raid 52, +360/90/54 salvage), revealed ~30+ sectors via scans, established a passive SensorArray.

## 2. Strategy
- Early: chained single-slot building queue through DeutSynth → mines → Lab → Shipyard → Lab2 → DeutSynth2.
- Kept my one fleet on **salvage_and_sites** standing orders from ~tick 600 until the end. It was excellent: it autonomously hopped between wreckage and derelict sites, fought and won every engagement it met, and came home when fuel/cargo thresholds hit. I never lost a ship to it.
- Built corvettes steadily (6 builds) as metal accumulated; docked at home so they auto-defended the raid.
- Final stretch: queued FuelDepot L1 (cheap, metal-only) for a fuel-cap bump — it completed in the last minute; it did work (+10% → fuel cap 517) but arrived too late to matter; still closed the only gap I noticed in my build line.
- What did not work: **prospect** doctrine — it bounced the scout back and forth between two empty sectors (-4,7 ↔ -3,6) for minutes ("pushing frontier" both ways) without expanding anything useful. Manual mining at home was more productive.

## 3. Fun and clarity
- The `play` tool is genuinely good: `status`/`map`/`sector`/`events`/`wait` cover everything, and the PolicyAction audit trail ("scooping wreckage", "board is clear", "returning: fuel 25%") made the autopilot very legible.
- Confusing points:
  - The `policy_update` example in RULES.md (`{"type":"policy_update",...}`) fails with a cryptic `unparseable input: missing field 'action'` **unless all four params are provided** — the error never says which field is missing. With partial params the headless client falls through to a second parser and the real error is lost.
  - Ship/hour scale is opaque: with ~60 min sessions, late-game builds (Shipyard L3 = 607 ticks, Lab L3 = 675 ticks) make it impossible to plan "what level can I reach" without reading the balance source. A `play cost <thing> <level>` or showing next-level cost/time in `status` would fix this.
  - Events don't say which player owns ships in CombatRound; I spent time figuring out which fight was mine (my scouts were ids 3/4; later combat showed ids 11/12 — same fleet, re-registered?).
  - Known-sector list silently decays (was 32, later 1) — intel expiry is ~2 min and `map` shows only live intel, which is not stated in the rules.

## 4. Bugs and oddities
- **Fuel cap cliff (biggest one):** starting fuel is 50000/50000, but after the first refuel at home, `fuel_max` collapses to the sum of ship tanks (2 scouts = 120). Quote: status showed `fuel 50000/50000` at tick 82 and `fuel 120/120` at tick 770 after docking. The initial 50000 looks like a free pass that quietly evaporates; every doctrine's effective range is governed by this hidden cliff.
- **Combat events leak across players:** my `events` stream is full of CombatRound events between ships I don't own (e.g. ship ids 15/16 vs 36/37 vs 79-84), including what looked like other players losing ships. It's a firehose that buries my own events (the 25+ NPC kills make it very noisy — see below).
- `play do` example in help says `{"type":"policy_update",...}` but the client requires the bare-command form to be wrapped with `action`, and policy params all-4-mandatory — neither documented.
- Salvage "scooping wreckage" emitted as a separate PolicyAction every 3 ticks while the fleet just stood still collecting (fine, but it's the only event type that spams).

## 5. Balance
- **NPC salvage is the real economy:** every destroyed pirate dropped a flat 200/50/30, and my fleet collected far more metal that way than my 3-level mines produced. Pirate density around home was high enough that salvage_and_sites was essentially a metal farm. This probably dominates everything else; consider varying drop sizes or tying them to the NPC's class.
- **Corvette cost (400 metal) is the bottleneck, not the shipyard:** ship build time at SY2 (46 ticks) is trivial next to the 2.0/t metal income; I queued ships as fast as the stockpile allowed.
- **Fuel system is inconsistent:** refueling at home appears free (no deuterium cost observed) while `recall` burns 2× fuel and hull risk exists — the FuelDepot (+10%/lvl) and ExtendedFuelTanks (+15%/lvl) are the only levers and they're tiny vs the tank-sum cap. With 517 max fuel (470 + FuelDepot L1) my 9-ship fleet only ever did 1–2 hops.
- **Derelict ambushes never triggered for me** (Scout aboard reads risk down — maybe too safe; I boarded ~5 sites, zero ambushes).
- **Late-game dead end:** with 60-minute sessions, Weapon/Shield research (Lab L3, 675 ticks) and Shipyard L3+ (607+ ticks) are unreachable, so the entire top of the tech tree never matters.

## 6. Top 3 suggestions
1. **Fix the fuel model:** make refuel cost deuterium and/or make `fuel_max` scale smoothly (depot + research should dominate, not be cosmetic). The 50000 → 120 cliff silently changes the whole game at tick ~150.
2. **Filter/annotate events by player ownership:** include `owner`/`fleet_id` in CombatRound/FleetDestroyed, or let `play events` take a `--mine` filter. Right now ~90% of my event stream is other players' and NPC combat.
3. **Publish cost/time tables in-tool** (`status` or a `play economy` command): next-level building costs, research prerequisites, and ship stats/costs. Every balance decision I made required reading the Rust source.

## 7. The `play` tool
- `wait N` printing the event digest was perfect for the cadence.
- `policy_update` handling: accept partial params (fill defaults) and surface the *actual* serde error instead of the fallback "missing field `action`".
- `status` should show fleet fuel as % and the doctrine's next-decision thresholds; "fuel 517/517" tells me nothing about how many hops that is.
- `map` decaying silently is disorienting — either persist discovered sectors with a "stale" marker or print "N sectors expired since last look".
- A `play queue <action...>` (batch several commands) would cut my round-trips when chaining builds, since the build queue is single-slot and I had to re-poll every completion.
