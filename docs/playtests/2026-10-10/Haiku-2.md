# Haiku-2 feedback

## 1. Result
Final (about tick 3330): 24 PulseTurret, 16 LancerBattery, ~13 Frigates, ~5 Cruisers, 2 Scouts. Buildings: Metal 14, Crystal 12, Deut 7, Shipyard 6, Research Lab 4, Fuel Depot 2, Defense Grid 3, Fabricator 1, Modular Fab 1. Research: CruiserTech 1, FrigateTech 1, HaulerTech 1, AdvancedShields 3, WeaponsResearch 3, others 1-2. Ended around 4th on the board (about 309 score at tick ~3200, 1st was about 367).

## 2. Strategy
Scouts on salvage_and_sites (decent early metal, kills scout patrols). Mines first, then lab/shipyard, then corvette/frigate/cruiser tech. Battle fleet on patrol_home killed patrols with no losses. Defences were a cheap way to dump metal into score.

## 3. Fun and clarity
Rules are dense but the status output was clear. The "short X" error messages were very useful.

## 4. Bugs and oddities
- NoResources errors happened when a build was attempted right after another spend, even when status showed plenty of stock. Stock shown in status did not always match what a command saw a few seconds later.
- Failed commands are refused and not queued, so "queue it and let it wait" does not work for buildings/ships/research. The rules say waiting items cost nothing, which only applies once an item is accepted.
- Salvage scoops are 40-unit trips, so scouts spend most time shuttling small amounts.

## 5. Balance
- Mine upgrades are very expensive by level 12+ (13k+ metal); metal is the bottleneck all game, crystal piles up unused (25k+).
- Defences were cheap score per metal and easy to add.

## 6. Top 3 suggestions
1. Let queue items wait for resources (or report that clearly) instead of refusing them.
2. Make crystal useful or cheapen its use; crystal capped out unused.
3. Show cost of the next item in status so players don't guess.

## 7. The play tool
A "queue" command that retries until paid would help a lot. Status should list the next-affordable time.
