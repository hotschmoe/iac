# Haiku-2 notes

- Early game: queued mines and deuterium synth; metal piled up (2400+) while deuterium gated everything (Robotics 2 needs 400 deut, Shipyard 100, Research Lab 200, Combustion Drive 600).
- Fleet 82 (2 Light Fighters) harvested rich crystal and deuterium at -4,-1 (about 22 units per 11 ticks). Harvest ends alone when a tile empties, with a warning.
- Policy (prospect/mine_and_return) is NOT available via ./play: the "policy_update" action is rejected as unknown. Could not run standing orders at all. Rules describe them as a separate message type; ./play does not seem to send it.
- Raid forecast flips to LOST while the only fleet is away from home (defence power 0). No clear warning of how much is lost.
- Mistake: queued ResearchLab twice (one became Lv.2). Harmless but confusing.
- Shipyard needs Robotics 2 (400 deut); LightFighter locked by Combustion Drive 1 which needs a Research Lab.

## Mid game (tick ~400-1850)
- Solar Plant fell behind: energy fraction fell to 0.35 at tick ~530 because mines were built faster than energy. Stacking Solar Plants fixed it. Consider a warning when made/needed < 1.
- Deuterium was the gate for Robotics 2, Shipyard, Research Lab, Combustion Drive, Energy Tech and Computer Tech. Metal piled up to 11k+ while deuterium sat near 0.
- The hub exchange was the best use of the fleet: deuterium or crystal -> metal/crystal at 26-28% fee (par 1000-4000 scale). Each round trip (3 hops each way) takes about 40 ticks. Worth doing every few minutes. The fee meter rises with volume and decays over about 45 s.
- Mistake: load failed (stock lower than I thought) and I flew the fleet to the hub empty. Check the stock in the same call before moving.
- Reserved builds hold the whole build queue: a reserve:true item waiting on deuterium blocks every item behind it, even cheap metal ones. Used it anyway.
- Raids: a home raid of 4 Light Fighters was repelled with the fleet home. With the fleet away, the forecast is LOST.
- Rocket Launcher (2000 metal) is the only metal-only defence early and it counts half toward the core score: a good sink for surplus metal.
- Solar Satellite: 2000 crystal + 500 deuterium, 30 energy. Expensive but useful for energy.
- Score so far: rank 1 at about 354 (Haiku-1 219, Grok-2 212).
- Standing orders (policy_update) are not reachable from ./play at all.

## Late game (tick 1850-2960)
- Score climbed to about 943 at 10 min left, rank 1 (next: Grok-2 about 690 at last check). Bots reached about 1137 but are unranked.
- Income at 2900: about 370 metal, 179 crystal, 45 deuterium per second. Costs of the mines (metal 25k+ at level 16) outrun the stock; large metal went into rocket launchers (metal only) and fighters.
- Hub trades kept paying: about 27-28% fee, par rate scaled up with the meter and fell back within about 70 s. Deuterium to metal gave about 1.45 metal per deut on average.
- Problems noticed: the reserve flag stalls the whole build queue (not just the item); Research Lab 6 needed 12k+ crystal and sat in the queue for 15 min; a Metal Mine level near 16 costs about 26k metal, so metal sat at 20k+ without a sink that scales.
- Fleet stayed in the homeworld. Two raids (4 and 8 Light Fighters) were repelled, with salvage afterwards.
