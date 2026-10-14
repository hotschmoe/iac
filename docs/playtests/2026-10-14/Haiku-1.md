# Feedback from Haiku-1

## 1. Result
Final score about 846 (core 820, explore 25.5, combat 0.1), rank 4 of 5 at 6 minutes left (Haiku-2 was first at roughly 950 at the last leaderboard I read). Homeworld at the end: Metal Mine 15, Crystal Mine 14, Deuterium Synthesizer 6, Solar Plant 15, Fusion Reactor 1, Robotics Factory 3, Shipyard 3, Deuterium Tank 2, Research Lab 5. Research: Armour 4, Espionage 3, Computer 3, Energy 3, Impulse Drive 2, Weapons 1, Combustion Drive 1. Fleet: one fleet of about 40 Light Fighters, 2 Solar Satellites built, 9 Rocket Launchers. Stock at the end was still about 4000 metal, 14600 crystal, 2500 deuterium.

## 2. Strategy
- Early: two Light Fighters harvested crystal and deuterium nearby. The home sectors had almost no deuterium, so deuterium was the bottleneck for about the first 300 ticks (Robotics, Research Lab, Energy all need it).
- Mid: the hub exchange was the main lever. Crystal in the hold traded to deuterium at about 30% fee; later metal to crystal and deuterium to metal, each trip about 90 s of ticks and 10 or so commands. Refining deuterium from the hold into fuel rescued a stranded fleet.
- Late: the stock swung between metal and crystal shortages, so I made roughly eight hub trips. Most of the score came from fighters, rocket launchers and buildings at cost.
- What did not work: sending the fleet on long walks with too little fuel (it stranded at 20 fuel), two trips made with stock I had already spent (load refused), and a queue that held an unaffordable 10000+ metal solar plant ahead of mines.

## 3. Fun and clarity
The rules are very long and mostly clear once read, but the first hour of learning is slow. The state and status output is informative. Hard to keep track of: which resource is short at any moment, and the fact that stock printed in `status` changed between calls in ways I could not explain from the events.

## 4. Bugs and oddities
- `exchange` and `load` report results inconsistently: one `load` printed an InsufficientResources line about Light Fighter when the actual call was load; one `exchange` of metal with an empty hold printed "ok exchange" with nothing moved.
- Stock jumped by several thousand metal and crystal between calls after hub trips (for example metal 1982 to 11206 across one trip plus waits). Hard to tell whether the exchange or the dock put it there; more explicit events would help.
- A waiting item at the head of a one-slot queue blocks the items behind it, with no reserve or skip. Cancelling it was the only fix.
- `build_ship SolarSatellite` reports "ordered, paid 0/4000/1000" while waiting for a slot, and the stock at that moment looks already spent.
- Mine costs grow so fast (Crystal Mine 14 ~ 15700 metal) that late mines are effectively unbuyable; the cost list is not clear about this.

## 5. Balance
- Deuterium is extremely scarce near a new homeworld; without a hub trip I would have stayed stuck for many minutes.
- Metal and crystal income of several hundred per second late is far above what the queues can spend: the queue, not the economy, is the limit, which makes stock pile up and score nothing.
- Research costs (Armour 4 at 8000 metal, Impulse 2 at 8000 crystal and 1200 deuterium) are large next to fleet costs.
- Raids: forecast was REPELLED most of the late game, so defences mattered little; early raids forecast LOST for long.

## 6. Top 3 suggestions
1. A spend-by-value queue, or a `reserve`-like skip, so one unaffordable item does not block the slot.
2. Make a hub trade one command that carries the whole loop (load, fly, trade, fly back, unload) or give a fleet a "trade route" preset; the manual walk was most of my play time.
3. Show the stock change reasons (exchange, dock, raid, refund) in one event line so resource jumps are explicable.

## 7. The play tool
- `./play status --brief` lines that mean a shortage in one term (e.g. "needs 13216 metal") would help more if they also said which item blocks the queue.
- A `./play trade` helper for hub round trips, and a fuel-aware `move` that refuses a route it cannot finish, would have saved several mistakes.
- `load` and `exchange` error text should name the command that failed.
