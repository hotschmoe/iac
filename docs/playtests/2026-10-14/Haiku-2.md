# Feedback from Haiku-2

## 1. Result
Rank 1 of 5 at the end, score 1113.6 (core 1096.8, explore 16.8, combat 0). Next: Grok-2 960, Grok-1 927, Haiku-1 883. The best built-in bot, BOT-raider-13, scored 1597.9 but is unranked.
At the last check: stock 31k metal, 24k crystal, 17k deuterium unspent (unspent stock scores 0, so this was a mistake I could not fully fix in the last minutes), income about 370 metal, 210 crystal, 56 deuterium per second.
Built: Metal Mine 18 queued (level 17 was done), Crystal Mine 15, Deuterium Synthesizer 11, Solar Plant 14, Fusion Reactor 3, Robotics Factory 4, Shipyard 2, Research Lab 6, Metal and Crystal Storage 4 and 3, Rocket Launchers about 25, Solar Satellites about 12 (rebuilt after raids), Light Fighters about 40. Research: Energy 4, Computer 3, Laser 5, Armour 3, Weapons 4, Shielding 3, Impulse 3, Combustion 1, Ion 1, Espionage 1.
No fleet fight beyond home raids (4, 8 and 10 Light Fighters), all repelled.

## 2. Strategy
- Mines and Deuterium Synthesizer first. Metal piled up while deuterium gated Robotics 2, Shipyard, Research Lab and Combustion Drive.
- The 2 starting Light Fighters harvested the rich crystal and deuterium tiles at -4,-1 (about 22 per 11 ticks), then went home to unload.
- The hub exchange was the best sink. Hauling 2000 to 3500 deuterium or metal to the hub (3 hops each way, about 40 ticks round trip) turned a surplus into what I lacked. Fees were about 27-28%.
- Solar Plant stacking fixed an energy deficit (fraction 0.35 at tick 530).
- Rocket Launchers (metal only) and Light Fighters were the metal sinks; both score at cost.
- What did not work: standing orders (policy_update is not reachable through ./play); trading an empty hold (a load failed and I flew 3 hops with nothing); Research Lab 6 which needed 12k crystal and sat in the queue for 15 minutes.

## 3. Fun and clarity
The rules are long but the state line and `play status --brief` made it playable. The scoring line ("unspent stock scores 0") was the most useful sentence. Confusing: policy/standing orders are described in detail but ./play has no way to send them. "reserve: true" is described as holding the line, but the queue stall it causes is not obvious from `status`.

## 4. Bugs and oddities
- `./play do` rejected `policy_update` as "unknown variant", so no standing order could be set at all, although RULES.md documents it.
- `load` error text was clear, but my stock was lower than I assumed when I ordered in one batch with the move that followed. The move still ran with an empty hold.
- `reserve: true` on a building waiting for deuterium blocked all later building orders, including cheap metal ones, until deuterium arrived.
- A raid forecast of LOST appeared when the only fleet was away from home. The fleet was home soon after, so this stayed mostly theoretical, but the forecast number gives no sense of what is lost.
- Mine costs grow to 25k+ metal at level 16, so one mine level takes minutes; late metal ran to 30k+ unspent. The fleet and defence sinks are the only ones that keep up.
- Scan reports "RichOre" signals that were already harvested after one or two harvest cycles; the tile emptied in about 3 cycles.

## 5. Balance
- Deuterium is the early bottleneck: 100 deuterium at start, Robotics 2 needs 400, Research Lab 200, Combustion Drive 600, Energy 2 800. Deuterium synthesizers at level 1 to 4 do not keep up for about 10 minutes. Metal arrives much faster, so the early game is starved of one resource while others pile up.
- Hub exchange at ~27% fee is the best deuterium source for an idle fleet; it felt too good relative to the mines. Consider a rising fee on repeated trades (the meter already does) and keep it that way.
- Rocket Launchers at 2000 metal, counting half their cost in score, made metal sinks easy. Probably too easy late: they always count.
- Raids at home were small all game (4 to 10 ships). They never threatened the stock, so defence mostly served the score.
- The bots (BOT-raider-13 at 1598) reached far beyond any human-style player. Ranked players had no real chance against them in combat-based score; that may be intended.

## 6. Top 3 suggestions
1. Make the deuterium early game less starved: a cheap early deuterium source, or raise base synthesizer output, so Robotics 2 and Research Lab come before tick 300.
2. Let the stock in a unit show when metal and crystal are far above cost, e.g. a "spend suggestion" line in status, and make sure unspent metal is never 20k+ with nothing to buy. Mine costs are huge; a sink that scales (or an upgrade queue that holds several levels) would help.
3. Expose standing orders (policy_update) in ./play, or remove them from RULES.md until they are reachable. Explain why reserve blocks cheap orders.

## 7. The play tool
- A `./play policy` subcommand or `do` support for policy_update.
- A "hub trip" helper: load, move the route, exchange_quote and exchange, with the right amount checked against the stock first.
- Show which resource gates the next build or research in one line, with the time to cover it (partly there in `status`).
- Warn when a `load` is short (do not move on a failed load).
