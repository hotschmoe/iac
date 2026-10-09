# Feedback - Builder (pure builder, no fleets sent out)

## 1. Result
At ~8 min left: score ~231 (all core, combat 0, explore 0), rank 6/6 (leader ~300+ with exploration/combat bonuses). Buildings: Metal 11, Crystal 9, Deut 8, Shipyard 5, Lab 5, Depot 3, Sensor 3, Grid 4, Fabricator 4, Vault 2. Research: most techs maxed at 5, Frigate/Hauler/Corvette 1, ModularFab 1. ~63 Corvettes, 2 Scouts, defence power ~3700 vs raid ~450 (every raid repelled with no losses). Stock at end: crystal ~3900 and deuterium ~5500 idle, metal always the limit.

## 2. Strategy
Mines first, then lab/shipyard, research of everything cheap, mass Corvettes as defence and score. A background python bot (state -> ./play do) kept queues full. Worked: fast early compounding, zero raid damage. Did not work: the bot's naive priorities. It spammed corvettes that starved mine upgrades, chose metal-hungry mines when metal was the bottleneck, and I could not stop it (no kill allowed) so I had to jam the ship queue with a 300x Scout order. Pure building is outscored by bonus-earning strategies: cap is 25% of core, and early explore/combat points were large relative to a small early core.

## 3. Fun and clarity
Rules are clear. Economy costs rise steeply (metal mine L12 ~5k metal) so late growth stalls on one resource while others pile up with no sink.

## 4. Bugs and oddities
- `./play do` crashed with FileNotFoundError on the tmp view cache (state/Builder.view.tmp -> .view.json) when two play calls ran concurrently.
- Displayed costs seemed to jump between reads (Crystal Mine L10 1679 metal then 2136, 2296, 3299 later) - possibly intended inflation, but unexplained.
- Missing result line for a queued 300x Scout order (accepted silently).

## 5. Balance
- Idle crystal/deuterium with no conversion or metal-free sink; trade/exchange would help.
- Corvette spam is very score-efficient (cost counts 1:1) and gives huge defence; raids are trivial after that.
- Mines' payback becomes poor by mid-game; ships are the main late score sink.

## 6. Top 3 suggestions
1. Add a resource converter/market or metal-free sinks so lopsided stockpiles are usable.
2. Show cost growth rule and a "best next spend" hint accounting for the binding resource.
3. Make builder strategy competitive (e.g. higher cap or defence kill-credit for repelled raids).

## 7. The play tool
`state` JSON and `do` batching were great. Wanted: a built-in `autopilot` for queue refilling, safe concurrent calls, and a `status` column showing which resource is the bottleneck.
