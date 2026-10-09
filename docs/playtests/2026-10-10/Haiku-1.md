# Feedback from Haiku-1

## 1. Result
- Final score about 342 (rank 3 of 6 at tick ~3308, 5 min left). Sonnet-1 at 367, Sonnet-2 at 343, Haiku-2 at 316. Peak was rank 1 at tick ~1340 (score 73).
- Buildings: Metal Mine 13, Crystal Mine 12, Deut Synth 8, Shipyard 6, Research Lab 5, Fuel Depot 2, Defense Grid 2, Storage Vault 1, Fabricator 1.
- Research: Cruiser Tech unlocked, most others at 2-5. Fleet at home: about 12 Corvette, 2 Hauler, 10+ Frigate, Cruisers queued. Lost the first scout (fleet 18) to prospecting.

## 2. Strategy
- Mines first, then Shipyard and Research Lab, then Corvette Tech. Metal was the bottleneck until about tick 800.
- Used Prospect on the scout early (fine for the first few sectors). It walked into a DEADLY T3 area and the scout died.
- Corvette fleet hunted a scout patrol (won, salvage 269 metal). Cargo capacity is only 10 per ship, so harvesting Rich metal is weak.
- Mid-game: spent surplus on Frigates and Cruisers, since ships and research count fully in score.

## 3. Fun and clarity
- The status output is dense but useful. Score and leaderboard are clear.
- Confusing: a build sent while unaffordable sometimes queues silently (as "waiting") and sometimes errors (NoResources). Research never queues when short.
- `events` output floods with ScanCompleted payloads (hundreds of chars each), which makes wait output hard to read.

## 4. Bugs and oddities
- Salvage at -1,-2 was partly collected (40 cargo), then the rest despawned unnoticed.
- Move to a sector two hops away was accepted when I had meant a single hop; `NoConnection` only shows when the target is not adjacent, so it is easy to misstep.
- Costs rise very steeply late: Metal Mine L13 costs 11677 metal, Crystal Mine L13 13511. Production (75/tick metal) barely keeps up, so mine upgrades stop mattering after about tick 2000.
- Deuterium piled up to 8000+ with nothing to spend it on before Cruiser Tech.

## 5. Balance
- Score-per-cost: Crystal and Deut weigh 1.5x and 2x in score, so deut-heavy items (Cruiser, Cruiser Tech) are efficient for score.
- Economy growth flattens hard around level 12 mines. Late game is decided by ship spending, not mines.
- The first scout was too cheap to be a safe prospecting tool.

## 6. Top 3 suggestions
1. Make `prospect` avoid sectors above the fleet's safe ratio (DEADLY or RISKY) instead of walking in.
2. Make queue behaviour uniform: either queue unaffordable items or report them clearly, and explain waiting items in one line.
3. Let `events` collapse ScanCompleted payloads into a count and sector list.

## 7. The play tool
- A `play cycle` or batch command would help a lot (many builds plus status in one call).
- `wait` should accept up to 90s without the process being killed (I got exit 144 with `wait 90` and a long chain).
- Show the time left plus a one-line score in `status`.
