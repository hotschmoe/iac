# Feedback - Sonnet-1

## 1. Result
Final (~tick 3300): metal ~320, crystal ~500, deut ~130. Buildings: MetalMine 4, CrystalMine 3 (->4 queued), DeuteriumSynthesizer 2, Shipyard 2, ResearchLab 2. Research: Navigation 2, HarvestingEfficiency 1, ReinforcedHulls 1, CorvetteTech 1. Ships: 3-4 Corvettes. My two starting Scouts vanished from status (lost on the mine_and_return doctrine, no clear loss event noticed).

## 2. Strategy
Economy first, then a research lab, then corvettes. Mostly slow: the single building queue (130-600 ticks per upgrade) and a single research queue dominated the hour. Scouts on mine_and_return ferried deuterium, which worked well until they died. Boarding a tier-1 derelict yielded 0 resources.

## 3. Fun and clarity
RULES.md never says tech/building prerequisites. I spent ~15 minutes guessing: ResearchLab needs CrystalMine 2; Lab L2 is needed before ANY tech (only Navigation, then HarvestingEfficiency, were open); CorvetteTech needed Shipyard 2 (I think). Errors just say "prerequisites not met" with no detail. Also costs are not shown before ordering.

## 4. Bugs and oddities
- `wait 90` twice in one command exceeds a 120s tool timeout; waits are real-time but ticks run faster than 1/s in places (tick ~3300 at 52 min).
- cancel_build refunds 50%; starting Navigation L3 (604 ticks) via my loop was an accident that the UI did not warn about.
- Scouts disappeared with "Docked ships" replaced by Corvettes only; no clear event.
- "CargoFull ... return home to unload" error while at home.
- Deuterium harvest gives 1-2 per tick, too slow vs. need.

## 5. Balance
Deuterium is the bottleneck (research and corvettes need it, production 0.17-0.36/tick). Build times scale steeply (MetalMine L4 ~600 ticks) for a 1-hour session. One building queue is very restrictive.

## 6. Top 3 suggestions
1. Show prerequisites and costs in `status`/errors (e.g. "needs ResearchLab L2").
2. Add a `./play costs` / tech tree command; allow a second build queue or shorter timers for a 1-hour session.
3. Make deuterium easier to obtain early; clearer fleet-loss events.

## 7. The play tool
Needs a `costs`/`available` command listing what can currently be built/researched/afforded, and a wait that never exceeds the tool timeout. Status output hiding an empty fleet list was confusing.
