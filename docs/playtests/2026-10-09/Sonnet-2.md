# Sonnet-2 feedback

## 1. Result
~tick 3130. Resources 153 metal / 1037 crystal / 376 deut. Buildings: MetalMine 3, CrystalMine 3, DeutSynth 2, Shipyard 2, ResearchLab 2. Research: Navigation 2, ReinforcedHulls 1, CorvetteTech 1, HarvestingEfficiency 1 (L2 finishing). Ships: 1 Scout + 2-3 Corvettes (one fleet). My original 4-scout fleet was lost mid-game.

## 2. Strategy
Explorer/salvage style: scouts on salvage_and_sites/prospect/mine_and_return doctrines, killed several NPC patrols with scouts (cheap salvage). Never managed to board a derelict: the one signal I chased (3,-3) was gone on arrival; the other (0,-4) was guarded. The economy was far too slow: metal at 1-2/tick made every build a multi-minute wait. Heavy roaming cost me the fleet.

## 3. Fun and clarity
RULES.md does not say what the prerequisites are. Nearly every build/research returns an opaque "PrerequisitesNotMet" and I probed by trial and error (Shipyard needs MetalMine 2; ResearchLab needs CrystalMine 2; at Lab 2 only Navigation/HarvestingEfficiency were open; Shipyard 2 unlocked ReinforcedHulls/CorvetteTech). I also never saw costs before spending.

## 4. Bugs and oddities
- Fleet disappeared with no event I could see; status just showed "Docked ships: none, Known sectors: 0". Need a FleetDestroyed event for player fleets.
- Scout fuel shows 50000/50000 at start and then 120/120 later.
- policy_update with partial params gives "unparseable input: missing field `action`" which is misleading (it needs all four params).
- `./play wait 90` twice in one call exceeds the 120s tool timeout.
- Research/build "NoResources" errors do not say which resource is short.
- Derelict/site intel on `map` goes stale, so a "Derelict" signal can be gone when you arrive.

## 5. Balance
Construction times (260-300 ticks for level 2-3) are long relative to a 1-hour session; metal mine output is tiny. Scouts killing pirate patrols is surprisingly strong. Cancel refund 50% is harsh when the queue is a single slot.

## 6. Top 3 suggestions
1. Show prerequisites and costs in status (and in errors, name the missing resource/prereq).
2. Allow a parallel/second build queue or cut times; boost early metal production.
3. Notify when a player fleet is destroyed and report where; mark derelicts as claimed.

## 7. play tool
A `./play costs` / `./play tech` command, batch commands, a wait that auto-caps under the timeout, and a clear fleet list including destroyed fleets.
