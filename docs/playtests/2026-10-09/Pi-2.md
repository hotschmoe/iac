# Pi-2 feedback — In Amber Clad live playtest

## 1. Result
- **Resources (final, at ~5-min mark):** 251 metal / 406 crystal / 224 deuterium; production 2.00 / 0.73 / 0.60 per tick. (FuelEfficiency research was queued in the final minute and completed right at the bell.)
- **Buildings:** MetalMine 3, CrystalMine 2, DeuteriumSynthesizer 3, Shipyard 2, ResearchLab 2. No Fuel Depot, Sensor Array, or Defense Grid (no time — everything is gated behind Shipyard 3 / long build chains).
- **Research:** CorvetteTech 1, Navigation 1, HarvestingEfficiency 1, plus FuelEfficiency 1 completed at the very end (4 techs; all others at 0).
- **Ships:** 3 corvettes docked at home (fleet 195). **1 scout + 2 corvettes permanently lost** — stranded at 1 hop from home with 2% fuel (see Bugs).
- **Combat record:** my 2 starting scouts destroyed ~2 NPC bands early; the rebuilt fleet destroyed at least 3 more (scout bands) with easy victories via corvette rapid-fire, before stranding. Never had a working mining fleet in the second half, so late-game economy was home-mine-only.

## 2. Strategy
- Opened with the prerequisite chain (MetalMine 2 → Shipyard, CrystalMine 2 → Research Lab), put the starting scouts on `prospect`, then `mine_and_return`.
- **What worked:** `mine_and_return` is a great autopilot early: it found seams, shuttled ore, and even picked up easy NPC fights on the way. Corvettes are a strong early-mid unit — one-shotting NPC scout bands with rapid fire was satisfying and low-risk.
- **What did not:** `prospect` is nearly useless as written (see Bugs — it loops between known sectors and burns fuel with zero economic output). Deuterium was the silent bottleneck: refueling fleets drains deuterium and DeutSyn 1 only makes 0.17/t, so I spent ~15 minutes waiting for deuterium to fund research/ships. I never built a second active fleet, never explored derelicts, and never got a Defense Grid, so the "survival/raids" and "salvage" systems never really engaged for me.

## 3. Fun and clarity
- The rules doc and `play status` output are excellent — the prerequisite chain, costs, and times were all readable, and I could plan the whole session from the source formulas (though I had to read the Rust source for costs/prereqs; the in-game text never states them).
- **Confusing points:**
  - `PrerequisitesNotMet` and `NoResources` errors give zero detail. When my Shipyard and ResearchLab builds were rejected, I had to grep the server source to learn "Shipyard needs MetalMine 2" and "this needs 100 deuterium, you have 67". A one-line reason ("requires MetalMine L2") would have saved 5+ minutes of my session.
  - `PolicyParams` requires all four fields with no defaults on the client side, but the rules example shows only some — a partial params object fails with a misleading `unparseable input: missing field 'action'` (the fallback parser's error, not the real one).
  - Fleet fuel units are opaque: my first fleet showed `fuel 50000/50000`, later fleets `50/50` or `110/110`. Same ship class, wildly different scales — I couldn't tell if a 3-hop run was affordable until I saw the fleet get stranded.
  - "Known sectors: 1/0/2" in status is misleading: it seems to count only sectors with *fresh* intel, so the number wildly oscillates as intel expires, making it look like my map knowledge is being deleted.

## 4. Bugs and oddities
- **Stranded fleet = permanent total loss (the big one).** Fleet 131 (scout + 2 corvettes, 40/40 cargo) ended at sector 0,3 — **one hop from home — with fuel 4/160 (2%)** and sat there for 15 minutes spamming:
  `"PolicyAction ... action":"hold","reason":"cannot move (InsufficientFuel): returning: fuel 2%"`
  The min-fuel return threshold (20%) triggered too late for the return leg; a single hop costs ~16 fuel and it had 4. No rescue exists: fleets don't merge outside the home sector, `recall` needs 2× the jump cost it doesn't have, and there is no refuel-anywhere. Three ships and a full cargo were lost for zero gain, with no timeout, scrapping, or insurance.
- **`mine_and_return` violated its own `max_range`.** With `max_range: 3` the fleet repeatedly claimed seams 4–6 hops from home ("heading to claim" to 3,-2 / 6,-2) and eventually stranded because the round trip exceeded its tank.
- **`prospect` thrashes.** When all sectors within `max_range` were known, it ping-ponged between two or three of them every ~2 ticks ("pushing frontier") for minutes, burning fuel and producing nothing.
- **Phantom rejected command:** a `build_ship` sent at a moment when resources *looked* sufficient (403/416/236 vs 400/100/60 cost) returned `NoResources`; a retry ~20s later succeeded. Either the affordability check raced the tick or the status display and the server disagree — worth auditing.
- **Event spam:** every `CombatRound` for *every* fleet in the galaxy is broadcast to every player (I watched a 5v6 NPC brawl across the map in full per-ship, per-round detail). The `ResourceHarvested` stream is similar (one event per resource per tick). This drowns out the few events that matter (my own fleet's actions).

## 5. Balance
- **Corvettes are too dominant / scouts too fragile.** A lone NPC scout band was trivially deleted, but a single NPC "heavy" band (weapon ~13-17, rapid fire) instantly annihilated 2 scouts and even out-damaged my 2-corvette fleet in one round. The power curve between scout (weapon ~5) and the heavy class is a cliff, not a slope — scout-based fleets have no realistic role after minute 5, and 2-corvette fleets die in one round to heavies, so there's no gradual scaling into mid-game units.
- **Fuel economy is brutal relative to map scale.** Jump cost ≈ 16/hop against tanks of 50–160 means a mining fleet has a hard radius of ~2–3 hops, but the richest signals my scans reported were 4–6 hops out. With DeutSyn 1 at 0.17/t, even refueling for a 3-hop shuttle loop took minutes. Either tanks, jump cost, or deuterium production needs rebalancing so the map actually connects.
- **Build queue depth 1** makes long chains (Lab 2 = 270 ticks, DeutSyn 3 = 304 ticks) a hard serial bottleneck; with 60 minutes total I could only fit ~4 building upgrades. A 2-deep queue or shorter late-level times would help.
- **Salvage is unreachable.** NPC kills drop 200/50/30 in random sectors, but I never had a fleet positioned to `collect_salvage` them, and the policy presets don't route to known salvage. The reward table looks great on paper but is effectively dead for an agent.
- **Derelicts were never reachable** for the same reason (all are "beyond the shipping lanes"); the whole explore_site/ambush feature is a non-starter for AI play unless the prospect policy actively hunts them.
- **Raids:** I never saw a RaidIncoming (my empire maybe wasn't "worth" it until Shipyard 2 late in the game), so the defense/raid loop never tested. Docked ships as the only defense, with no Defense Grid accessible before Shipyard 3, feels like a trap for fast players.

## 6. Top 3 suggestions
1. **Make fleets un-losable to fuel:** auto-recall at the point where the return leg is unaffordable (i.e., check *remaining route* cost, not a fuel percentage), allow fleets to merge in any sector (pooling fuel), or grant stranded fleets a slow fuel trickle / timed emergency beamer. Losing a fleet to a bug-like stranding felt like the single worst moment of the session.
2. **Filter the event stream per player:** only broadcast CombatRound/ResourceHarvested detail for my own fleets; aggregate everything else into one line per battle ("NPC 218 defeated NPC fleet 248 in 6,-2, salvage 200/50/30"). As an agent I was paying token cost for noise 10× more frequent than signal.
3. **Verbose errors with numbers:** every `PrerequisitesNotMet`, `NoResources`, and client-side parse failure should name the missing requirement or shortfall ("needs 100 deuterium, have 67"; "requires MetalMine L2, have L1"; the *actual* parse error instead of the fallback one). This alone would have sped up play by a large factor.

## 7. The `play` tool
- `do` printing "sent" *before* the server response makes it hard to tell whether the command took; interleaving the response directly (which it does) is good, but the 2.5 s fixed sleep means slow-to-fail commands (queue validation) still look ambiguous. A `--wait 0` raw mode or an explicit `accepted/rejected` summary line would help.
- `wait` capping at 90 s forced a lot of busy-polling for 300-tick builds; a `wait_until <event>` (e.g., "BuildingCompleted") or a longer cap (up to ~180 s) would cut my command count in half.
- `status` should print **costs and next-affordable tick** for builds/research/ships I can target (I kept doing the arithmetic by hand), and it should distinguish "fleet stranded (no fuel)" as a prominent warning instead of a quiet `Idle`.
- `map`/`sector` should show fuel-cost-per-exit so route planning isn't guesswork.
- The `NUDGE`/idle tracking is a nice touch — it never fired on me, but it would have saved me had I stalled.
