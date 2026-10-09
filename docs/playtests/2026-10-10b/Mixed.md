# Feedback (Mixed, balanced strategy)
1. Result: ~tick 3160, score ~360 (core 340, combat 12, explore 7), rank 3-4/6. Mines 12-13/10/10 metal/crystal/deut, Shipyard 6, Lab 5, ~60 frigates + corvettes, 40+ pulse turrets, most techs maxed.
2. Strategy: mines first (fast payback early), then labs/shipyard, then spent surplus on frigates (best points per resource). Fleets: scouts prospecting, corvettes patrol_home, big fleet on salvage_and_sites. Fleet income/explore contributed little (explore only 7). Late mine upgrades (L12+ costing 5-11k) are not worth it with <30 min left; spending directly on ships/research is equal score.
3. Clarity: rules good. Confusing: strict FIFO queue means one expensive item blocks cheaper ones; `wait` returns early on events so it rarely sleeps the stated time.
4. Oddities: ResearchLab order error message mentioned Modular Fabrication prereq; stock jumped faster than reported income/s (fleet harvesting?). Prospect doctrine stalls at RISKY T2 sectors for scouts.
5. Balance: mine costs escalate sharply (x1.5+/level), crystal was the constraint; deuterium piled up unused. Explore/combat bonus tiny vs core for a builder. Pulse turrets poor score per resource (half credit).
6. Suggestions: (a) allow queue skipping of unaffordable items; (b) uses for surplus deuterium; (c) clearer per-resource score value / a "best spend" hint.
7. Tool: a `wait --sleep N` that really sleeps; a single `spend` helper showing best affordable item.
