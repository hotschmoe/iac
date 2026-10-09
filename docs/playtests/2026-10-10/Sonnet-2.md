# Feedback - Sonnet-2

## 1. Result
Ended ~3rd of 6 (score ~330, core ~330, combat 0.2, explore 0.3), within ~10% of the leader.
Buildings: MetalMine 13, CrystalMine 11, Deut 9-10, Shipyard 5, Lab 5, Depot/Sensor/Grid/Vault/Fabricator all 4-7. Research: most techs at/near cap incl. Frigate, Hauler, ModularFab 2, EmergencyJump. Fleet: ~40 Corvettes, 2 Frigates, 2 Scouts, 1 Hauler, ~60 Pulse Turrets. One raid repelled with no loss.

## 2. Strategy
Explorer/raider intent, but economy dominated. Early mine spam compounded well; mine cost (x1.5/level) made levels 13+ unprofitable by ~minute 30. Then spent on every building/research for score, corvettes (metal sink), pulse turrets (metal-only). Exploring and fighting paid nothing measurable: T1 derelict gave 0 loot; kills at T1-3 give ~150 metal salvage and no score because my force was >8x; salvage piles of thousands exist but scouts/corvettes carry 40.

## 3. Fun and clarity
Rules are dense but good. Confusing: failed build commands print NoResources yet the item sometimes also sits in the queue "waiting"; errors from earlier queued items surface later in the output of unrelated commands. Queued items block the 3-slot queue with unaffordable upgrades (I accidentally queued Shipyard L5-7).

## 4. Bugs and oddities
- "2 x cargo 40" fleets: cargo cap 40 for scouts, 40 for 4 corvettes, makes salvage/ore useless (a 3498/1930/965 pile was visible).
- Cancel responded "(no new events)" with no confirmation of what was cancelled.
- Derelict T1 "SiteExplored" resources all 0, only 0.12 points.
- Tick rate seemed >1/s relative to wall clock; minutes-left vs ticks inconsistent.

## 5. Balance
- Exploration/combat reward is far too low compared with core score; the 25% cap is never approached. No incentive to leave home.
- Mine levels beyond ~12 are too expensive for a 1h game; deuterium is the scarce resource and corvettes/frigates need it.
- Storage Vault leveling is a pure score sink with no use at blitz caps (caps millions).
- Pulse turrets (metal only) are the best cheap score sink; slightly exploity.

## 6. Top 3 suggestions
1. Make exploration/derelicts/combat pay real resources (and bigger cargo for Scouts/Haulers) so raiding is a viable strategy.
2. Clearer queue semantics: unaffordable orders should be queued or rejected consistently; show queue in command reply.
3. Show next-level payback (production gain / cost) in `costs`, and warn when force >8x so combat score is zero.

## 7. The play tool
Wanted: `status --brief`, a `costs` marginal-income column, `fleets` summary, a combined `autobuild` priority list, and command replies that echo queue state.
