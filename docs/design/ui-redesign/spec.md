# In Amber Clad: web client redesign ("Amber Glass")

Status: design and prototype only. No game code was changed.
Date: 2026-10-09. Author: UI/UX design pass for the owner.
Binding input: `docs/design/decisions.md` (web is the main human interface; full redesign; TUI stays secondary; one ranking for humans and agents), gap analysis section 5, the 2026-10-09 playtest, `shared/src/protocol.rs`.

Files (all under `/tmp/claude-1001/-home-hotschmoe/9160508d-db54-4978-9479-a39b969d10f5/scratchpad/ui-redesign/`):

| Path | What |
|---|---|
| `spec.md` | this document |
| `prototype/index.html` | hub linking the prototypes (open this one; works from `file://`) |
| `prototype/windshield.html`, `map.html`, `overview.html`, `homeworld.html` | interactive prototypes, mock data, simulated 1 Hz tick |
| `prototype/iac.css`, `iac.js` | shared tokens, components, mock world, shell |
| `shots/prototype-*.png` | screenshots of the prototypes |
| `shots-current/*.png` | screenshots of the current Flutter client |
| `tools/shot.mjs` | the CDP screenshot driver (Node 22, no dependencies) |

---

## 1. Critique of the current client

Screens captured from the live release build (`shots-current/`). All are 1440x900.

### 1.1 Cross-cutting

1. **It is the TUI in a browser.** The shell is a tab strip, a command bar and a key-hint footer (`cc.png`, bottom). Every action is a typed word or a letter key. There is nothing to click that feels like a game control: no hover states, no buttons for scan, harvest, attack, build. A new human has to read the footer, guess, and type. The owner's brief ("not recreating the TUI") is the main finding.
2. **Legibility is the first thing the CRT look sacrifices.** Body text is amber at 30-60% alpha on near-black (`Amber.normal` 0x99, `dim` 0x4D) at 9-12 px monospace, under a scanline and noise overlay. Section titles ("RESOURCES", "BUILD QUEUE", `cc.png`) and every secondary value are close to invisible. Gap analysis section 5 already says "legibility first (contrast, big tap targets, no tiny monospace in tables on phone)"; the current build does the opposite.
3. **No hierarchy.** On the command center every panel has the same weight and the same border (`cc.png`). Nothing says "this is the thing you should do next". The empty Build Queue card is just blank, so an idle queue, the single most expensive mistake in an OGame-style loop, is invisible. The Research Lab shows "> Idle -" with a 0% bar.
4. **Half the screen is dead.** On a 1440 wide desktop the command center fills the top 55% and leaves the rest empty (`cc.png`). The windshield and map push their content into a ~120 px strip in the middle with a sidebar whose top 100 px is empty (`ws-scan.png`, `map.png`).
5. **Navigation hides screens.** The tab bar shows three tabs (Command Center, Windshield, Star Map). Homeworld, the screen where the whole economy is played, has no tab; it exists behind key `4` or the command bar (`shell.dart`: `GameView.homeworld`). In my session pressing `4` typed a "4" into the command bar and nothing happened (`hw.png`). Keystrokes after a view change landing in the command bar instead of the view is a recurring focus bug (`ws-scan.png` shows "v" sitting in the bar; the scan never fired).
6. **Credentials are shown as game text.** The account token appears twice, as an alert and as the first event-log line (`cc.png`). It is a password, shown once, in the least visible place, with no copy or download affordance.
7. **No feedback or motion.** The only animation is a global 80 ms flicker on view change. Scan, jump, harvest, combat, build completion: no visible event at all. The game's central fantasy (real-time piloting) is a table of numbers.
8. **No session hooks.** No alert list, no notification, no sound, no "while you were away". Playtest finding 2 (own losses buried in galaxy-wide combat spam) is a UI problem as much as a server one.

### 1.2 Windshield (`ws-scan.png`)

- The player's fleet is one 56 px circle with `<>` in it. The six neighbours are `???` circles whether scanned or not. There is no visual difference between a quiet sector, a hostile pack, a derelict or an ore field.
- **The compass is wrong.** Labels read `[1]E [2]NE [3]NW [4]W [5]SW [6]SE`, but the lines are drawn at 0/60/120/180/240/300 degrees with y pointing down, so `[2]NE` is drawn lower-right and `[6]SE` upper-right. The hex vectors in `hex.dart` (E=(1,0), NE=(1,-1), NW=(0,-1), W=(-1,0), SW=(-1,1), SE=(0,1)) are right for a flat-top map where E is actually lower-right and NW is straight up, so the windshield and the map disagree about which way "NW" is.
- Sector content lives in a sidebar of text ("Hostile #0 1xScout patrol", "Adjacent: none known", "Salvage none", "Derelict none"), not in the scene. The phantom `HOSTILE #0` passive scout from the playtest bug list is front and centre.
- Fuel reads `50000/50000` (known server bug) with a bar that is full; it gives no sense of hops left or whether home is reachable.
- Scan (`v`) has no pulse, no cooldown display, no reveal. Cooldown is a bare number in the sidebar.

### 1.3 Star map (`map.png`)

- On a fresh account the map is one hexagon. Known sectors drop out of `full_state` about two minutes after a scan (playtest finding 3), so the map forgets, and the client does not cache.
- Even when populated, the painter draws hex outlines without lane edges beyond the revealed ones, no threat colouring, no freshness, no route. The cursor panel is text ("Threat: Clear").
- Controls are keyboard cursor, `ENTER fly 1 hop to cursor`, and CLOSE/SECTOR/REGION zoom chips; no drag-pan, no wheel zoom, no click-to-route, no preview of fuel or time, no waypoints beyond a one-line list.

### 1.4 Homeworld (code review; the screen is hidden behind `4`)

`homeworld_view.dart` renders the catalog as a grid of 250 px cards in four keyboard-selected tabs. The data is right (the catalog solved playtest finding 1) but the presentation is a keyboard cursor over a grid, with no explicit "why not" copy beyond card state, no tech tree, no storage or defence.

### 1.5 What to keep

The amber identity is the asset: warm black, a single hot colour, monospaced numbers, hairline frames. The boot sequence and login screen have charm. The protocol layer, hex maths, demo provider (offline demo), token store and state mapper are sound and reusable (see section 9).

---

## 2. Design principles

**P1. Amber is the world, not a filter.** The CRT look becomes a *lens*: scanlines, vignette and glow live on the two spatial viewports (windshield, map) where they add atmosphere. Everything you read (numbers, tables, buttons) is crisp, high-contrast and on a flat panel. Prototype: `.scan` overlay exists only inside `.main` canvases.

**P2. Fun comes from feedback, not decoration.** Every verb has an anticipation, an action and a result you can see and hear: a gate flares before a jump, a radar pulse sweeps before the neighbour cards update, a boarding ring fills, a build ring bursts when it completes. Idle states are alive (ship bob, gate shimmer, ring tick) so the screen never reads as a spreadsheet.

**P3. Show danger before it hurts.** Threat is a first-class visual channel: tier colour, pip count, shape, pulse, and an odds label ("Favoured / Even / Risky / Outmatched") computed against *your current fleet*. The game's pitch is "danger grows with distance"; the UI must make that gradient readable at a glance on the map and felt in the windshield.

**P4. Information hierarchy: glance, scan, dig.**
- *Glance (1 s):* the single "next thing" card, resource state (ok / near full / full), alert count. Anything red or pulsing needs me.
- *Scan (5 s):* four queue cards, three fleet cards, alerts, rank.
- *Dig (on demand):* costs, requirements, logs, combat reports, audit trails behind a click or hover. No panel shows a detail the player did not ask for unless it is a warning.

**P5. Two time scales, one screen vocabulary.**
- *OGame side (slow):* the Overview and Homeworld are an orders desk. Timers, queues, "queue the next thing". The pacing flag stretches this from minutes (blitz) to days; all timers render as relative durations, never as raw tick counts alone.
- *TIB side (fast):* the Windshield is a cockpit. You act in 1 s beats against 1-5 tick cooldowns.
- *The bridge between them is "take the wheel / hand back the wheel".* Standing orders fly a fleet while you are away; opening its windshield offers "Take manual control" (pauses the doctrine, fully visible) and "Hand back to autopilot". The Overview's fleet cards, the alert rail and the map all open the windshield on that fleet in one click.

**P6. Stale is a state, not an error.** A sector last seen 40 minutes ago is not wrong; it is *fossil*: sepia, hatched, dashed, with an age label. A sector in sensor range is *live*: teal edge, pulse. The player should never wonder how old a number is (section 3.6).

**P7. Every disabled thing says why.** Disabled buttons always carry the reason as visible text ("Slot A busy, 4:17 left", "Short 120 Cr, in 2:10", "needs Metal Mine L2"), reading directly from `HomeworldCatalog.requires`. This directly answers playtest finding 1.

**P8. Mouse, touch and keyboard are equals.** Every action has a button (44 px minimum on touch), a hover state, a keycap hint, and a shortcut. The command bar becomes an optional palette (`/`), not the main way in.

**P9. Distance has a texture.** Inner ring is warm and safe, outer ring is hotter, the Wandering is cold violet and sparse. Zone colour tints the windshield background, the map rings, the ambient sound. You should *feel* you are further out before you read the number.

**P10. Honest about agents.** Humans and LLM agents share chat, trade and rank. Every named entity carries a small HUM / AI tag. Agents are players, not decorations (decision 5).

---

## 3. Visual language: "Amber Glass"

### 3.1 Palette tokens

Implemented as CSS custom properties in `prototype/iac.css` and to be mirrored as `const` in `design/tokens.dart`.

| Token | Value | Use |
|---|---|---|
| `void` | `#08060a` | page background (warm black with a faint violet bias so amber glows against it) |
| `deep` | `#0d0a0b` | gradient base |
| `panel` / `panel-hi` | `#15100c` / `#1d1610` | panel surfaces (rendered as a 86-90% alpha vertical gradient) |
| `inset` | `#0a0807` | wells, inputs |
| `line` / `line-hi` / `line-lo` | amber at 16% / 38% / 7% | hairlines, hover borders, row dividers |
| `a100` `#fff3d6` | | highest emphasis numbers and titles |
| `a200` `#ffe2a3`, `a300` `#ffcc66` | | headings, key values |
| `a400` `#ffbd33`, `a500` `#ffb000` | | primary accent, progress fills, active state |
| `a600` `#d98f00`, `a700` `#9a6500` | | pressed, borders on done items |
| `a800` `#5e3d06`, `a900` `#2e1e06` | | scrollbar thumb, ring tracks |
| `text` `#f4dfb2` | | body (contrast on `panel` about 12:1) |
| `text-2` `#bfa370` | | secondary (about 7:1) |
| `text-3` `#7d6a47` | | tertiary labels, never for values (about 3.6:1; labels are 10 px caps, so use only for decoration/metadata) |
| `text-4` `#4d4230` | | ghost hints, keycap numerals |
| `own` `#5ee0cf` | | *mine / live / friendly*: own fleets, live intel, positive deltas, "can afford" |
| `own-dim` `#2a6f68` | | borders in the teal family |
| `ember` `#ff5a3c` | | danger, destructive buttons, full-storage warning |
| `rare` `#b793ff` | | derelicts, anomalies, discoveries, unknown/uncharted |
| `ok` `#8fe388` | | quiet / safe boarding risk |
| `t0..t4` | `#4fb9a8` `#ffd24a` `#ff9a2e` `#ff5a3c` `#ff2e86` | threat tiers: Clear, Light, Moderate, Heavy, Lethal |
| `metal` `#d9b46a`, `crystal` `#7fd4ff`, `deut` `#6fe6b6` | | resource identity (always paired with a text label "Fe / Cr / De", never colour alone) |
| `fossil` `#8a7552`, `fossil-bg` `#2a2217` | | stale intel |

Colour roles are deliberately *few*: amber = the world and neutral UI, teal = yours/live, red-orange-magenta ramp = danger, violet = unknown/rare. Colour never carries meaning alone (see 3.5 for the redundant channels).

### 3.2 Typography

| Role | Face | Use | Sizes |
|---|---|---|---|
| Display | **Chakra Petch** 500/600/700 | titles, button labels, big numbers, section labels in caps with +0.14-0.3 em tracking | 10 (caps label), 12, 16, 22, 30, 38, 64 |
| Data | **JetBrains Mono** 400/500/700 | coordinates, costs, logs, tables, hints, tabular numerals | 11 min, 12-13 body |
| Voice | **Newsreader** italic 400/500 | sector flavour lines, banner subtitles, onboarding narration, empty states. Never for data. | 13-19 |

Rules: numerals are always `font-variant-numeric: tabular-nums` so tickers do not jitter; no text below 10 px, no 10 px body text on touch devices; all-caps only for labels (<= 14 chars); Flutter: `google_fonts` is already a dependency, but fonts should be bundled as assets so first paint does not wait on the network and the web build works offline from the game server.

### 3.3 Surfaces and shapes

- **Panels** have a 1 px `line` border, 6 px radius, and two *corner brackets* (top-left, bottom-right, 2 px amber, 10 px long). The bracket pair is the signature motif and replaces the TUI double-line frames. Sub-panels do not repeat brackets.
- **Hexagons** are the world shape. Flat-top, same orientation as the map: in the windshield the sector is a big hexagon whose six edges are the six gates; on the map each sector is a small hexagonal node; on the homeworld the buildings are hex tiles. Hex is used for *places*; rectangles for *information*.
- **Glow is semantic:** only live, active, or dangerous things glow. A glowing element is a thing to look at.
- **CRT lens:** scanlines (3 px period, 14% black), vignette, optional faint grain, applied only to the viewport canvas, off with `prefers-reduced-motion` or a settings toggle ("Lens: off / light / full").

### 3.4 Motion

Motion is information with a budget: durations are short, easing is `cubic-bezier(.2,.8,.2,1)` (fast out, long settle) unless stated.

| Event | Animation | Duration |
|---|---|---|
| Hover on card, gate, tile | lift 2-3 px (`translateY`) or scale 1.05, border to `a400`, glow | 120-180 ms |
| Press | scale .98 | 80 ms |
| Panel enter | rise 8 px + fade | 300 ms |
| **Jump** | ships accelerate toward the gate (ease-in 2.2) for 0.55 of the time, star layers stretch into streaks, white flash at the swap, new sector fades in, ships decelerate from the opposite gate; banner "SECTOR x,y" | 1.1 s total; swap at 0.55 |
| **Scan** | teal radar ring expands from the fleet (3 px line + 14 px soft ring) to beyond the arena; neighbour gate cards update as it passes; cooldown conic sweep on the button; contact banner if hostiles found | pulse 1.6 s; reveal at 1.15 s |
| **Harvest** | beams from each ship to nearest rock of the chosen ore, ore-coloured particles stream back to the ships, cargo bar ticks per tick | continuous, 1 Hz tick-synced |
| **Combat round** | each tick one volley: per-ship projectile (0.26 s flight, staggered 60-180 ms inside the second so volleys read as rattle, not a metronome), shield flash ring on hit, hull sparks, floating damage numbers (cyan for shield, orange for hull), screen shake 14 px (decays x0.88/frame) on a kill, red edge vignette that breathes at 8 rad/s | each round fills its 1 s |
| **Kill** | two particle bursts (gold 46, red 26), shake, bar vanishes | 1.1 s particles |
| **Build/research complete** | ring fills to 100%, burst, toast with `ok` chime, panel shine sweep | 700 ms |
| **Idle queue** | border and `IDLE` badge pulse (`a500`, 1.6 s). Deliberately attention-seeking. | looping |
| **Resource full** | chip border pulses `ember`, 1 s | looping |
| **Numbers** | tick-up tween on changes > 1 unit | 400 ms |
| **Tab / view change** | 160 ms cross-fade (the old 80 ms global flicker is retired) | 160 ms |

`prefers-reduced-motion`: no parallax, no streaks, no shake, pulses become static emphasis, particles halved; jump becomes a 200 ms cross-fade. A "Motion: full / reduced" setting in addition to the OS flag.

### 3.5 Danger and threat

Threat is a 5-step scale `0 Clear, 1 Light, 2 Moderate, 3 Heavy, 4 Lethal` (a server value, section 8). It is shown through **four redundant channels** so it survives colour blindness and small sizes:

| Tier | Colour | Pips (skewed bars) | Map glyph | Motion | Name |
|---|---|---|---|---|---|
| 0 | `t0` teal | 1 teal | none (clear nodes show nothing) | none | Clear |
| 1 | `t1` yellow | 1 | small hex, numeral I | none | Light |
| 2 | `t2` orange | 2 | hex filled, numeral II | none | Moderate |
| 3 | `t3` red | 3 | hex filled + outer ring, numeral III | ring pulses 4 rad/s | Heavy |
| 4 | `t4` magenta | 4 + glow | hex + double ring, numeral IV | pulse + the *screen edge* vignettes in the windshield | Lethal |

- **Odds are personal.** Every contact and every sector with hostiles shows `Favoured (>=1.6x power) / Even / Risky / Outmatched` plus an estimated win chance, computed from *your selected fleet's power* versus the server's `power` for the pack. Changing fleet changes the labels. (Prototype: `odds()` in `windshield.html` / `map.html`; the real value should come from the server so the estimate matches the combat model.)
- **Friction on bad ideas.** Firing at Risky/Outmatched needs a second press (toast explains), ambush warnings on derelicts with `risk: hot`, a route that crosses Heavy+ sectors holds *before* them (section 5.3). Danger is never a silent surprise.
- **Zone feeling.** Crossing a zone boundary (inner 8, outer 20) shows a one-line banner ("Leaving the Inner Ring") and shifts the background tint and ambient track.
- **Lethal pack in the arena:** the hex outline turns ember and pulses, the contact panel pins the pack to the top, gates on the far side glow brighter ("get out" affordance).

### 3.6 Live versus stale intel: "frozen in amber"

The game is called In Amber Clad; the visual metaphor for old information is literally fossilisation.

| State | Rule (in world-time; scale ticks by pacing) | Look |
|---|---|---|
| **Live** | inside a fleet's or Sensor Array's range now | teal edge + soft pulse, full colour, `LIVE` badge, hostile icons solid |
| **Fresh** | seen < 5 min ago | full amber, no decoration |
| **Aging** | 5-30 min | amber desaturating toward sepia, light hatch |
| **Stale** | > 30 min | `fossil` sepia, dashed outline, strong diagonal hatch, hostile icon at 60% alpha, age label `2h`; the inspector says "Hostiles may have moved; ore may have been mined." |
| **Unknown** | never seen, but adjacent to a known sector by a known lane | violet dotted ring with `?`; if a scan returned a `SignalContact`, the hint text is shown ("faint mass reading") |

Lanes follow the weaker end of their pair. Routes through stale sectors show stale segments hatched in the threat strip (section 5.3). A "Refresh intel" action sends the nearest scout or opens a scan prompt.

Prototype: `intel()` in `iac.js` (states) and the map's `hatP` hatch pattern.

### 3.7 Sound (opt-in, default muted until the first gesture)

Browsers require a gesture to start audio; the speaker button in the HUD unmutes, the preference persists. Cues are short synthesised or sample-based blips (prototype synthesises them with WebAudio, `IAC.sfx`).

| Cue | Trigger | Character |
|---|---|---|
| click | button, gate hover | 50 ms square blip |
| jump | jump | 0.7 s saw sweep 90 -> 1400 Hz + sub thump |
| scan | scan | 0.9 s sine ping with a 1.4 kHz echo |
| harvest | each tick while harvesting | soft triangle tick (rate-limited to 1 Hz) |
| salvage / board success | collect, loot | rising two-note chime |
| combat start | engage | low sting, two square tones |
| fire / hit | per projectile / impact | high zap / mid thud, volume scales to 8 simultaneous max |
| kill | ship destroyed | noise burst |
| alert | raid at -60 ticks, fleet lost, heavy contact | descending two-tone, repeats twice |
| build / research complete | completion | triangle arpeggio |
| chat mention | `@name` | soft blip |

Ambience: one quiet looping bed per zone (warm drone inside, colder and thinner outside, near silence plus high wind in the Wandering). Master, SFX and Ambience sliders in settings. Tab-title badge and `Notification` API alerts reuse the same triggers (section 8.8).

---

## 4. Navigation model

```
HUD (top, always)    logo | Fe Cr De chips with rate and fill bar | tick ring | sound | help | alerts bell | player + rank
Rail (left, always)  Overview | Windshield | Map | Homeworld | Fleets | Comms | Rank      (bottom bar on phones)
Palette (/)          power-user command bar (the current parser lives on here)
```

- The HUD resource chips are always visible: they are the OGame resource bar. Each chip has a thin fill bar for storage and pulses `ember` when full.
- The tick ring is a small circular progress (1 s) beside the tick count, so the player *sees* the 1 Hz heartbeat; the whole UI is quietly synchronised to it.
- **Global shortcuts:** `G` then `O W M H F C R` goes to a screen (Gmail style, 1.5 s window), `1 2 3` selects fleet 1-3 (control groups), `Tab` cycles fleets, `?` opens the key sheet, `/` opens the palette, `M` mutes, `Esc` closes or cancels. Digits are fleet selectors, not view switches, because the windshield and map need single keys.
- Unbuilt rail items (Fleets, Comms, Rank in the prototype) toast "Spec only".
- **Deep links:** `#/map?x=9&y=-8`, `#/windshield/fleet/1`, `#/homeworld/research` so alerts and events can open the right place, and back/forward works.

---

## 5. Screen-by-screen

Prototype references are given as `P:` where a working screen exists.

### 5.1 Overview (home)  P: `overview.html`

**Purpose:** the OGame overview: "what needs me, what is running, what are my fleets doing", readable in seconds, one click from any action.

**Layout (desktop 12-column grid):**
1. *Hero (8 col):* animated homeworld (planet with night-side lights, orbit rings with defence platforms), name, sector, ring, key building chips, a flavour line, and three totals (building levels, research, ships). Purely atmospheric but anchors "this is my place".
2. *Next-up card (4 col):* **one** prioritised recommendation with a primary button. Priority order: raid inbound (shows your defence versus estimated raid power bars) > resource at cap (waste) > idle queue (with an affordable suggestion and a *Queue it* button that starts it) > fleet in danger or low fuel > "everything is running; send a fleet out".
3. *Resource row:* three large tiles: amount / cap, `+rate/tick` and `/hour`, fill bar, **time to full** (turns amber under 10 min, `STORAGE FULL` in ember at 97%), 40-sample income sparkline.
4. *Queue row:* four cards: Building slot A (ring + countdown + cancel with refund note), Building slot B (locked card with the unlock research and a link; unlocked later), Research (ring or **IDLE** state with three costed, affordability-tagged suggestions), Shipyard (ring, batch progress "1/3", next ship timer, "+3 more").
5. *Fleets (8 col):* up to 3 cards (the game cap): name, state chip, location + ring + hops home, ship glyphs, **fuel in hops** (red when below 1.3x the return trip), cargo bar, orders, buttons Pilot / Map / Recall.
6. *Nearby space (4 col):* a small live radar (known sectors, threats, fleets, raid vector arriving as a red dashed line with a moving marker). Click opens the map.
7. *Alerts (5 col):* severity-ordered cards with countdown and a button that navigates to the cause.
8. *Event feed (7 col):* filter chips All / Mine / Combat / Economy / Intel. "Mine" is the default (playtest finding 2).
9. *Leaderboard, Comms, Team-ups (3 x 4 col):* compact previews (section 5.6, 5.7).

**Controls and interactions:** queue buttons (start, cancel), suggestion buttons (tap to queue; unaffordable ones are dimmed and say what is missing), fleet buttons, alert buttons, filter chips, radar click, hero click opens Homeworld.
**Shortcuts:** `Enter` runs the next-up primary action; `Q` focuses the first idle queue; `1 2 3` select a fleet (highlights its card); `W`/`M` with a fleet selected jumps to it.
**Narrow (<820):** single column in this order: hero (compressed), next-up, resources, alerts, queues, fleets, feed. Radar, rank, comms collapse into a "More" accordion at the bottom. Cards use minmax(0,1fr) so nothing overflows. Screenshot: `shots/prototype-mobile-overview.png`.

### 5.2 Windshield  P: `windshield.html` (priority 1)

**Concept:** the current sector is a *place* you are in. The sector is a hexagonal arena; its six edge midpoints are **gates** to the six neighbouring sectors. What is in the sector is drawn in the arena; what is *next to* it is on the gate cards. It is the TIB node-graph view made tangible: node graph for decisions, cockpit for feeling.

**Scene (canvas):**
- Parallax star layers (3), tinted by zone; nebula clouds for Nebula terrain; scattered fragments for Debris; slow ring ripples for Anomaly; the homeworld planet and orbit platforms when docked or at home.
- **Ore deposits** are rotating rocks with ore-coloured inclusions (count and size from terrain and density). **Salvage** is a gold crate cluster with a pulsing ring. **Derelict** is a violet hulk with blinking lights and a dashed risk ring coloured by `quiet/uneasy/hot`. **Hostiles** are red ships patrolling in loops with reticle brackets and, in combat, hull and shield bars. **Your fleet** is a teal formation at the arena's lower centre with engine flames; ship silhouettes differ per class (Scout dart, Corvette, Frigate, Cruiser, boxy Hauler).
- Everything procedural (rock positions, orbit phases) is seeded from the sector coordinate and entity ids, so the scene is identical every visit and needs no server state.
- Hover or select a contact in the list or on the canvas: its brackets show on both.

**HUD:**
- *Top-left:* sector coordinate in display type, chips (zone, terrain, hops from hub, hops from home), threat badge, a one-line Newsreader flavour per terrain.
- *Left rail:* fleet cards 1-3 (state, coordinates, hops of fuel, orders). Selecting one rebuilds the scene at that fleet's sector (including docked at home).
- *Right top:* **Fleet status** gauges: fuel (hops, and a "home needs N" line that turns LOW), cargo with per-resource breakdown, hull, shield, standing orders, fleet power, scan range.
- *Right:* **Contacts** list: grouped hostiles (composition, behaviour, power versus yours, odds, win %), ore deposits (density dots per resource), salvage (amounts), derelict (tier, risk), homeworld if present. Selecting a row reveals its contextual action button.
- *Gate cards* at each gate (only for real lanes; a missing lane is a bright wall segment): direction keycap, threat pips, terrain, hostile composition with tier numeral, three ore dots (opacity = density), live/age label, salvage or derelict glyph, fuel cost, a `HOME` tag on the next hop of the shortest path home, a dashed *uncharted* style with `?` and the scan-hint text. Hover draws a dashed line from the fleet to the gate and flares the pylon.
- *Dock:* seven action buttons with keycaps and unavailable-states explained by tooltip: Scan, Harvest (opens an Auto/Fe/Cr/De sub-bar), Fire, Collect, Board, Recall, Stop. Cooldowns render as a conic sweep with the remaining ticks.
- *Feed (bottom-left):* last seven events with tick, colour bars by kind. Own events only.
- *Banner:* centre-top, display caps with spaced tracking: `NEW SECTOR`, `CONTACT`, `ENGAGED`, `VICTORY`, `BOARDING COMPLETE`, `AMBUSH`, `FLEET LOST`.

**Interactions and keys**

| Action | Mouse / touch | Key |
|---|---|---|
| Jump through a gate | click a gate card | `Q W E / A S D` mapped to NW N NE / SW S SE (the key layout mirrors the hex's six edges); number row `1-6` still mapped for TUI muscle memory |
| Scan | Scan button | `V` |
| Harvest | Harvest, then choose resource | `H` |
| Fire | Fire (double press when odds are Risky or worse) | `F` |
| Collect salvage | Collect | `C` |
| Board derelict | Board (refused if hostiles present) | `B` |
| Recall (safest known lane home) | Recall | `R` |
| Stop (clears route, harvest, combat intent) | Stop | `X` |
| Select fleet | fleet card | `1 2 3`, `Tab` |
| Select contact | click on canvas or list | click again or `Esc` to clear |

**States worth designing:** docked at home (planet scene, "Docked fleets are safe"); out of fuel (toast, gates dim, a "call for help" button planned); drive recharging (toast with remaining ticks, button shake); arriving first time (violet "NEW SECTOR" banner, uncharted tag cleared); fleet lost (red banner, fleet reset); multiple fleets in one sector (formation groups with a combined bar).

**Narrow (<900):** the arena shrinks to the full width (Rh = width / 3.2); gate cards compress to icon + keycap; the fleet rail becomes a horizontal chip row above the dock; Contacts and Fleet status move to a slide-over sheet behind a "Contacts" button; the dock scrolls horizontally with 54 px buttons; the feed hides (events surface as toasts). Screenshot: `shots/prototype-mobile-windshield.png`. On touch there are no hover states, so a gate tap shows its card expanded first, second tap jumps.

**Desktop screenshots:** `prototype-windshield-01-idle.png` (idle), `-02-scan.png`, `-03-heavy-contact.png`, `-04-combat.png`.

### 5.3 Star map  P: `map.html` (priority 1)

**Concept:** a lane graph you can read and plan on, with two overlays that carry the game's main strategic questions: *where is it dangerous* and *how old is what I know*.

**Rendering:**
- Each known sector is a small hex *node* inside a faint hex *cell*. Lanes are drawn node to node (dashed when either end is stale or unknown). Cells carry terrain tint (nebula purple, asteroid brown, anomaly violet).
- Node look: threat >= 1 fills the node in the tier colour with its numeral; clear nodes are dark with an amber outline; live nodes have a teal outline and glow; stale nodes are hatched and dashed.
- Detail by zoom (LOD): far (< 0.62) heat blobs, zone rings and fleets only; mid (0.62-1.0) nodes, threat numerals, derelict dots; near (>= 1.0) ore pips (three bars), salvage and derelict glyphs, age labels at > 1.6.
- Zone rings (hexagonal, at 8 and 20) with spaced labels, dashed and animated. Hub glyph, home planet glyph (ringed teal), fleets as chevrons with heading and a selection ring.
- **Threat layer (`L`):** dims the base, adds large radial blobs per hostile sector (radius and strength by tier and freshness) and a faint teal tint on cleared sectors, so the safe corridors read as teal channels through red. See `prototype-map-03-threat-overlay.png`.
- **Intel-age layer (`I`, on by default):** the hatch and sepia system of 3.6.
- Other layers: Ore `O` (bigger ore bars), Lanes `N`, Derelicts `D`, Zones `Z`.

**Navigation:** drag to pan; wheel zooms at the cursor; pinch zoom on touch; arrow keys pan; `+ -` zoom; `H` centres home; `F` centres the selected fleet; `0` fits.

**Click-to-route (the headline feature):**
1. *Hover* any known (or frontier) sector: a dashed route from the selected fleet (or from the last waypoint) draws instantly, and the **Route preview** panel fills in.
2. *Click* sets the destination and turns the preview into a **Route plan**. *Shift+click* appends a waypoint (numbered flags). Clicking a Fast / Balanced / Safe segmented control re-plans with different threat penalties (client Dijkstra with per-tier penalties; the server should expose the same for authoritative quotes).
3. The plan panel shows: hops, **fuel** (cost, fleet total, bar), **return trip** ("OK, margin 21 fuel" or "NO FUEL TO COME HOME (needs 34)"), **ETA** (hops times the 1-tick move cooldown plus margin; server-quoted), max threat on route, a per-hop **threat strip** (hop colour = tier; hatched where intel is stale; a white marker where a hold will occur), count of stale/missing sectors on the path, the worst odds ("Favoured vs Moderate"), and a checkbox **Hold before Heavy+ sectors**.
4. *Engage* (`Enter`) sends the route; the fleet marker animates along it. A held route stops *before* a sector with a known Heavy or Lethal pack and offers **Continue / Reroute safe / Abort** with the pack's composition and power. It never auto-fights a heavy pack on a route the player planned.
5. A route is refused to engage when fuel is insufficient (button disabled, reason shown) and warns if the return trip would be impossible.

**Inspector (right):** coordinates, terrain, zone; a *live box* (teal) or a *fossil box* (sepia, dashed: "Seen 30m ago. Hostiles may have moved; ore may have been mined"); threat pips, composition, behaviour and **odds**; three ore bar-meters with a "rich / typical / poor for this distance" read and the loot tier; salvage; derelict with risk; distances (from fleet, from home, from hub); actions *Route here*, *+ Waypoint*, *Refresh intel*.

**Top-left:** fleet chips (select the routing fleet). **Top-right:** layer toggles with keycaps. **Bottom-left:** legend (tiers, intel gradient, glyphs). **Bottom-centre:** zoom, coordinates readout, Home, Fleet.

**Narrow:** the inspector becomes a bottom sheet (max 42% height), layer chips scroll horizontally above it, zoom controls stack on the right, two-finger pinch and one-finger pan; route plan is shown inside the sheet rather than in a separate panel (prototype hides the floating plan on narrow widths; the real build merges it into the inspector sheet). `prototype-mobile-map.png`.

**Screenshots:** `prototype-map-01-default.png`, `-02-route-preview.png` (balanced route plan), `-03-threat-overlay.png`, `-04-fast-route-holds.png` (Fast route crossing Heavy+ with two HOLD markers and "Outmatched vs Lethal").

### 5.4 Homeworld  P: `homeworld.html`

**Header:** the four queue pills (slot A, slot B locked, research, shipyard) with timers and a bottom progress line; idle pills are dashed and amber. Tabs: Buildings, Research, Shipyard, Defence, Storage. Keys `1-5` switch; the last two are marked **NEEDS SERVER** because the protocol has no data for them yet.

**Buildings:** a **hex base**: the planet at the centre, the eight buildings as hex tiles around it (level numeral, icon, current output). Tiles get a teal edge when the next level is affordable and a slot is free, an amber pulse while building, a dim lock when prerequisites are missing. Click selects; the right drawer shows level L -> L+1, the effect, a cost table (have / need, red when short), build time, requirements with check or cross, the **primary button with its reason when disabled**, and a flavour line. `Enter` upgrades. Costs, times and `requires` come straight from `HomeworldCatalog`.

**Research:** a **tech graph** grouped in branches (Drives, Hulls and arms, Economy). Nodes show level/max, a progress bar and a state: done, ready (teal), locked (dim, dashed edges), researching (pulsing). Edges are vertical for dependencies and horizontal inside a chain. The drawer is the same structure as buildings. Hovering a node highlights its prerequisite chain.

**Shipyard:** one card per class with a silhouette, stat bars (hull, shield, guns, hold), unit cost, time each, "can build up to N", a quantity selector (1, 5, 10, Max), total cost and a Build button; locked classes list their requirements with checks. Right drawer: fleet-slot usage (3/3), what is docked, the queue. The pacing note ("this timer is 2:46 here; a week on a seasonal world") teaches the world-speed flag.

**Defence (preview):** raid forecast (your defence power versus estimated raid power, next roll countdown, outcome estimate) and the defence structure list.
**Storage (preview):** per-resource vault with the stock split into *protected* and *raidable* (ember hatch) and an upgrade. Reinforces the OGame rule "if it sits, it gets hit".

**Narrow:** the drawer becomes a bottom sheet (46% height); the hex base scales to 62%; the tech graph scrolls horizontally; tabs lose keycaps and tighten.

### 5.5 Fleets (spec only)

**Purpose:** the place for the fleet-management features the playtest asked for: split, merge, standing orders and an audit trail (gap analysis section 5 item 4).

**Layout:** left list of up to three fleet cards (with the slot counter 3/3); right detail with tabs *Ships*, *Orders*, *Log*.

**Ships tab**
- Table of ships: class glyph, hull and shield bars, weapon, cargo share. Totals: power, cargo, fuel in hops, speed.
- **Split:** two columns "Stays" and "Splits off" with draggable ship chips (or checkboxes on touch); live preview of each side's fuel, cargo and power, and a warning if either side cannot get home. Disabled with a reason when the fleet cap (3) is reached. `Split` sends `split_fleet {fleet_id, ship_ids}`.
- **Merge:** shown when another own fleet is in the same sector ("Longhaul is here: Merge"); preview of the combined power; sends `merge_fleets`.
- Fleet name (rename), colour tag for map/windshield.

**Orders tab (standing orders / doctrine)**
- Doctrine segmented control: Manual, Prospect, Mine + Return, Salvage + Sites, Patrol Home, each with a two-line plain-language description and what it will do *right now* ("Would scan, then move to 7,-5").
- The four `PolicyParams` as controls with *inline consequences* (all four are mandatory server-side, so the UI always sends all four with defaults and never lets a partial update fail; playtest bug):
  - `min_fuel_pct`: slider 0-100, with a marker at "the return trip needs N%" and a warning when below it ("fleet could strand").
  - `cargo_return_pct`: slider 50-100.
  - `max_range`: stepper 1-20 with a mini-map ring preview from home *and from the work sector* (fixes the playtest `mine_and_return` re-claim confusion by showing where the range is measured from).
  - `engage_ratio_x10`: slider shown as "fight only if my power >= N x theirs", with a live "currently attacks packs up to X power" line.
- *Take manual control / Hand back to autopilot* buttons (also in the windshield).

**Log tab (the per-fleet audit log):** a feed built from `PolicyAction` events: verb icon (scan, move, harvest, return, attack), the `action`, and the server's `reason` in full, tick/time, a "where" chip that opens the map. Filters: by verb, by day. This is the explainability feature that differentiates the game; reasons are shown as plain sentences, never IDs. An "Override this" link takes manual control from that moment.

**Keys:** `1-3` select, `S` split, `M` merge, `O` orders, `L` log. **Narrow:** the list becomes a horizontal chip row; tabs become a segmented control; the split UI becomes two stacked lists with move buttons.

### 5.6 Comms (spec only)

**Purpose:** decision 4 (trade needs chat; PvE team-ups; PvP rules undecided). One panel, three jobs.

**Layout:** channel list (Global, Sector (people in your current sector), Team, Whispers) | thread | right context (trade or team).
- **Chat:** message rows with name, HUM/AI tag, tick/time, system lines (kills, discoveries: opt-in "brag" broadcasts). Mentions highlight and sound. `Enter` sends. Commands `/w name`, `/team`, `/mute`, `/trade`. Rate limit shown as a thin cooldown on the input. Agents are labelled and can be muted separately.
- **Trade:**
  - Structured **offer cards** inline in chat or in a *Market* tab: "Tessaly gives 500 Cr, wants 800 Fe, expires 14:00", with **Accept / Counter / Decline**; a "fair value" hint (ratio versus recent accepted trades) so new players are not robbed. A compose dialog with resource steppers and an expiry. States: open, accepted, expired, cancelled; escrow is shown as "reserved" in the HUD chips.
  - Open question for the owner: does goods transfer instantly (home to home) or physically (haulers fly it, so trades create raid risk)? The UI is designed for either; physical trade adds a "Cargo in transit" tracker on the map.
- **Team-ups (PvE):** an *Operations board*: target sector chip (opens the map), required power versus pledged power as a segmented bar, slots, roles (tank / salvage / escort), a countdown, `Pledge fleet`. Team roster with online dots and shared-intel toggle.
- PvP (undecided): reserve a `PvP` chip on the Wandering zone banner and a per-player *opt-in* switch in settings; no design commitment yet.
- Safety: block/mute, report, link previews off, profanity filter server-side.

**Keys:** `C` open, `/` focus input, `Esc` blur, `[` `]` switch channel. **Narrow:** channel list -> thread two-pane navigation; offer cards stack; compose in a bottom sheet.

### 5.7 Leaderboard (spec only)

- Tabs: Overall, Explorers (furthest distance reached), Economy (production), Combat (kills), plus a Season selector when pacing is seasonal.
- Filter: All / Humans / Agents (all on by default, because decision 5 is one ranking). Each row: rank, trend arrow with delta, name, **HUM / AI** tag, score, a tiny 7-day sparkline. Your row is pinned. Click opens a profile card (home distance, fleets, notable achievements, time online).
- A season banner with a countdown (pacing setting) and the reward line.
- **Narrow:** compact rows (rank, name, tag, score); filters collapse to a select.

### 5.8 Events and notifications

- **Bell drawer** (all screens): the full feed, chip filters (All / Mine / Combat / Economy / Intel / Social), grouping ("Harvested 41 Fe x 12"), each row actionable (opens the sector, fleet or building). Severity colours follow `AlertLevel`.
- **Toasts:** four on screen max, 4.2 s; `Critical` (raid, fleet lost) persists until dismissed and plays the alert cue; economic completions are single-line.
- **Combat report** (opens automatically when a fight ends *involving you*): timeline of rounds, damage dealt and taken, losses, salvage, replay (ships trace). Combat events from other players are not shown (playtest finding 2).
- **Raid warning ladder:** at `RaidIncoming` the Overview next-up becomes a raid card; at -60 ticks a toast + sound + browser notification; the radar and map draw the vector.
- **Session hooks:** tab title badge `(2) IAC`, favicon dot, optional browser notifications (permission requested in context after the first queue completes: "Tell me when this finishes?"), an optional email/webhook later. **While you were away:** on login, a summary card from persisted events since last logout (queues finished, fleets' log highlights, raids).
- **Keys:** `N` opens the bell drawer, `Esc` closes, `Shift+N` marks all read.

---

## 6. Onboarding: the first five minutes

Goals: a new human performs the three loops of the game (economy, piloting, autopilot) before the fifth minute, understands danger *before* meeting it, and never faces a blank panel. The tutorial is not a mode: it is a layer of coach-marks (spotlight + one sentence in Newsreader + a hint chip) driven by game state, on the real screens, always skippable and replayable from Help.

| Time | Beat | What the UI does |
|---|---|---|
| 0:00 | **Login** (name only; the existing boot sequence shortened to < 3 s and skippable) | New account: a **"Save your key"** card shows the token with *Copy* and *Download .txt* and a plain warning "this is your password". It replaces dumping the token into the log. Returning account: token auto-filled from local storage. |
| 0:10 | **Overview lands.** Hero planet, a 6-word welcome ("Ember Landing is yours. Mines work by themselves.") | Spotlight on the Metal Mine tile suggestion in the next-up card: "Upgrade your Metal Mine" with the cost chips and a green Queue button. Cost is affordable by design. |
| 0:30 | **First queue running.** Ring starts; toast "Metal Mine L2 in 0:45" | Coach-mark on the Windshield rail item: "Your Scouts are docked. Take the wheel." |
| 0:45 | **Windshield: scan.** The scene opens on the home planet with fleet docked. | Pulse on Scan (`V`). After the radar sweep the gates reveal; one carries an ore-rich asteroid sector with a *quiet* tag. The scan hint text is read aloud by the narrator line. |
| 1:15 | **First jump.** Coach-mark on the highlighted gate: "Q W E A S D or click". | Jump animation; banner NEW SECTOR; the fuel gauge shows hops. |
| 1:45 | **Harvest.** Coach-mark on Harvest -> Auto. | Beams, particles, cargo ticks. At 80% cargo a chip says "Hold nearly full. Recall to unload." |
| 2:30 | **Recall and unload.** Recall animates the fleet home; docking merges cargo into the HUD chips (numbers tick up with a chime). | Next-up card changes: "Metal Mine finished. Queue the next upgrade." |
| 3:15 | **Autopilot.** After the second trip: "Tired of flying? Let a doctrine do it." | Opens Fleets > Orders with *Mine + Return* preselected and parameter sliders explained ("come home at 25% fuel"). Handing the wheel back is shown. |
| 4:00 | **Danger, taught safely.** A Light, Favoured passive-scout contact (guaranteed adjacent to the first harvest site, server-seeded) | The odds label "Favoured" is explained once; the player presses Fire; combat plays slowly; a salvage pile drops; collect it. The Heavy gate on the far side is *shown* but not forced: the gate card has a tier III pip row and a line "Heavy packs destroy small fleets. Check the odds first." |
| 4:30 | **First discovery.** A tier-1 derelict (quiet) within 3 hops of home | Board (`B`), progress ring, loot or data core. The success banner. |
| 5:00 | **Checklist card** pinned on the Overview ("First hour"): Upgrade Metal Mine to 3, Scan, Harvest, Set autopilot, Build a Corvette, Say hi in chat, Reach distance 6. Items tick with a chime. | Comms and Rank appear in the rail with a dot once the first four are done (progressive disclosure; they are always reachable from the palette). |

Principles: never block input; every coach-mark has *Skip all*; no popup stacks (one at a time); the tutorial must tolerate the player doing things in a different order; disabled things still explain themselves (P7); idle-queue and low-fuel prompts do the teaching afterwards. Playtest evidence: players guessed at the tech tree, never boarded a derelict, lost fleets at the outer-ring edge. Beats 4:00-4:30 and the odds labels exist for exactly those.

---

## 7. Implementation notes shared by all screens

- **Empty, loading and error states** are designed: a skeleton panel with a pulse bar (not a spinner), "No data yet" lines in Newsreader italic, connection loss as an amber top banner "Link lost, retrying (3)" that freezes timers (so ghost countdowns never lie), and a reconnect resync toast.
- **Optimistic UI:** a queued build shows as running immediately with a "pending" shimmer; reconciled by the next `homeworld_update`; server errors surface as a toast with the server's message.
- **Time:** all durations come from ticks times the server's pacing factor; the UI never hard-codes "1 tick = 1 s".
- **Settings:** lens (off / light / full), motion (full / reduced), sound sliders, notifications, colour-blind mode (switches the threat ramp to a luminance-separated one and always shows numerals), UI scale 90-120%, language later.

---

## 8. Server data needed (beyond today's `protocol.rs`)

Present today and used as is: `FullState` / `TickUpdate` (fleets, per-tick `PlayerState`), `HomeworldState` + `HomeworldCatalog` (costs, ticks, `requires`), `FleetState` (fuel, cargo, cooldown, policy), `SectorState` (terrain, densities, `connections`, `hostiles`, `salvage`, `site`), `SiteBrief` (tier, risk), `SignalContact`, `RaidIncoming`/`RaidResolved`, `PolicyAction` (with `reason`), `Alert`.

Priority: **P0** unblocks the windshield and map as designed; **P1** needed for fidelity; **P2** new features.

### 8.1 Spatial and intel (windshield, map)

| Need | Why | Prio |
|---|---|---|
| `SectorState.last_seen_tick` (per sector, per player) and **persist known sectors indefinitely** in `full_state` (playtest finding 3). Client-side stopgap: cache `known_sectors` in IndexedDB keyed by account. | Freshness fading, fossil rendering, a map that remembers. | P0 |
| `SectorState.threat` (0-4) and `power` (numeric estimate) for the pack, plus `NpcFleetInfo.power`. A confidence flag (live vs remembered) is derived client-side from `last_seen_tick`. | Threat overlay, odds labels, route hold, gate cards. | P0 |
| `SectorState.zone`, `distance`, `loot_tier` (distance-scaled loot multiplier) and a per-resource `max_density` / `depleted` flag (ore regeneration). | "Rich for this distance", loot expectations, regenerating ore indicator. | P1 |
| Deterministic **ship catalog** in `full_state` (class stats: hull, shield, weapon, speed, fuel, cargo, per-hop fuel formula/`fuel_per_mass`) and each fleet's `fuel_per_hop`, plus move-cost modifiers by terrain (`nebula` etc). | Client route quotes without guessing; fixes the "50000/50000" display class of bugs by making the denominator explicit. | P0 |
| `route_quote {fleet_id, path|dest, mode}` -> `{hops, fuel, ticks, hazards[{hex, threat, intel_age}], can_return}` and `set_route {fleet_id, waypoints[], hold_threat_ge}` with server-side execution and `RouteHeld {fleet_id, hex, threat}` event. | Authoritative preview, multi-hop route that continues if the tab closes, hold-before-heavy. | P1 |
| `scan_result` persisted as sector hints (`SignalContact` stored on the sector as `hint`). | Gate cards need a hint after the event scrolls away. | P1 |
| `CombatStarted` carries both sides' snapshots (`ships:[{id,class,hull,shield,owner}]`), and all combat events carry `owner/player_id` or are filtered per player (playtest finding 2). | Draw both fleets with correct bars; own-only feed. | P0 |
| `SectorState.player_fleets` already lists others; add `kind` (human/agent) and `team`. | Contacts list, "friendly" colouring. | P2 |
| Derelict extras: expected boarding duration (ticks) and `risk` percentages; fleet requirements. | Board button copy and the ring timer. | P1 |

### 8.2 Fleets

| Need | Prio |
|---|---|
| Commands: `split_fleet {fleet_id, ship_ids}`, `merge_fleets {a, b}`, `rename_fleet`. | P1 |
| `FleetState.name`, `fuel_per_hop`, `home_hops`, `route` (remaining hops), `eta_tick`, `move_cooldown` rename for clarity. | P1 |
| `PolicyUpdate.params` all-optional with server defaults (playtest bug: partial params fail). | P0 |
| `fleet_log {fleet_id, since_tick, limit}` returning persisted `PolicyAction` events with reasons (the audit log must survive reloads). | P1 |
| Rescue/scrap commands for stranded fleets (playtest finding). | P2 |

### 8.3 Economy and homeworld

| Need | Prio |
|---|---|
| **Pacing:** `server_info {world_speed, tick_hz, season_ends_at, name}` and a `speed` factor applied to *displayed* durations. | P0 |
| **Storage:** `HomeworldState.storage {cap: Resources, protected: Resources}`; building/option types for vaults; production already present. Production should report net of cap (overflow waste). | P1 |
| **Defences:** `HomeworldState.defense {power, structures[], next_raid_roll_tick}`; `DefenseOption` entries in the catalog. `RaidIncoming` gains numeric `power_est`, `origin_hex`. | P1 |
| **Second build slot:** replace `build_queue: Option` with `build_queues: [{slot, unlocked, unlock_requires, active}]`; `Command::Build` gains `slot`. | P1 |
| `queue_idle` as an alert/event (so the client does not have to infer it, and so a notification can fire while the tab is closed). | P2 |
| Catalog effect text per upgrade (`effect: {kind, delta}`, e.g. `+0.31 Cr/t`) so the drawer can say what the upgrade does. | P1 |
| Cost "reservation" for trade offers. | P2 |

### 8.4 Social

| Need | Prio |
|---|---|
| **Chat:** `chat_send {channel, text}`, `chat_message {channel, from {id,name,kind}, tick, text}`, backlog on join (`chat_history {channel, before, limit}`), channels global/sector/team/whisper, rate limit and mute/block. | P2 |
| **Trade:** `trade_offer {give, want, to?, expires}`, `trade_accept/decline/cancel {offer_id}`, `trade_update` events, a market listing request, and a rule for delivery (instant vs hauler). | P2 |
| **Teams / PvE operations:** `team_create/join/leave/invite`, `team_state {members, shared_intel}`, `operation {id, sector, required_power, pledged, slots, expires}`, `pledge {operation_id, fleet_id}`. | P2 |
| **Leaderboard:** `leaderboard_get {board, season}` -> `{entries[{rank, player_id, name, kind: human|agent, score, delta}], you}`; a defined score formula; player `kind` on `PlayerState`. | P2 |
| **Presence:** `online` flag for team members. | P2 |

### 8.5 Session and onboarding

| Need | Prio |
|---|---|
| Event backlog since last logout: `events_since {tick}` or persisted per-player event log (for "while you were away"). | P1 |
| `player_flags` (cross-device onboarding progress, settings). Client localStorage is the fallback. | P2 |
| Guaranteed safe starting neighbourhood with a seeded passive contact and a quiet tier-1 derelict (spec section 6). Today no hostiles spawn within 2 of home, but the derelict and contact are not guaranteed. | P2 |

---

## 9. Implementation plan for Flutter

### 9.1 What exists (8,000 lines)

`lib/main.dart`; `protocol/` (1,863-line protocol, hex); `hex/hex_math.dart`; `models/`; `state/` (`game_controller` 599 lines, `state_mapper`, `connection_provider`, `demo_provider` for offline demo, `command_parser`, `token_store`); `theme/amber_theme.dart`, `crt_overlay.dart`; `views/` (shell, boot, login, help, `command_center/`, `homeworld/`, `star_map/`, `windshield/`); `widgets/`.

### 9.2 Reuse versus replace

| Keep as is | `protocol/*`, `hex/hex_math.dart`, `models/*`, `state/state_mapper.dart`, `connection_provider.dart`, `token_store*`, `demo_provider.dart` (becomes the design-time mock server and the "Offline demo" button), `command_parser.dart` (powers the `/` palette). |
| Keep and restyle | `boot_screen.dart`, `login_screen.dart` (add "Save your key" card), `help_overlay.dart` (becomes the key sheet from the prototype). |
| Refactor | `game_controller.dart` is a single 599-line `ChangeNotifier`. Split into slice stores (below); keep its command-sending code as `CommandService`. |
| Replace | `shell.dart` (new adaptive shell with rail, HUD and router), `theme/amber_theme.dart` (-> `design/tokens.dart` + `theme.dart`; keep `Amber.*` as deprecated aliases for one release), `theme/crt_overlay.dart` (-> a `LensOverlay` widget used only by viewports), `widgets/amber_*`, `progress_bar`, `resource_bar` (-> `design/widgets`), all of `views/command_center/*` (-> `features/overview`), `views/windshield/*`, `views/star_map/*`, `views/homeworld/*`. |

### 9.3 Proposed structure

```
lib/
  app/            app.dart, router.dart (go_router, deep links), adaptive_shell.dart (rail / bottom bar), hud.dart
  design/         tokens.dart, theme.dart, typography.dart, motion.dart, sfx.dart, lens_overlay.dart
    widgets/      panel.dart (bracket frame), keycap.dart, cost_chips.dart, threat_pips.dart, intel_badge.dart,
                  ring_progress.dart, gauge_bar.dart, toast_host.dart, banner.dart, sheet.dart, coach_mark.dart
  scene/          (rendering layer, no Flutter widgets inside)
    camera.dart (pan/zoom matrix, hex<->screen), paragraph_cache.dart, glow_sprites.dart
    windshield/   windshield_scene.dart (state + ticker), windshield_painter.dart, ships.dart (silhouettes),
                  entities.dart (rocks, salvage, hulk), fx.dart (particles, beams, floaters, warp), seeds.dart
    map/          map_painter.dart (layers), map_layers.dart, route_overlay.dart, hit_test.dart, lod.dart
  features/
    overview/ windshield/ map/ homeworld/ fleets/ comms/ rank/ events/ onboarding/
      (each: screen.dart, panels/, controllers (Riverpod notifiers))
  state/          stores (see 9.4), command_service.dart, intel_cache.dart (IndexedDB), prefs.dart
  protocol/ hex/ models/   (unchanged)
```

### 9.4 State management

Today: one `ChangeNotifier` rebuilds everything per tick. For a 1 Hz stream feeding six animated screens it should become **selector-based**.

- Recommendation: `flutter_riverpod` (one new dependency). Immutable state classes with value equality.
- Providers (all derived from a single `gameSnapshotProvider` fed by the websocket reducer):
  - `connectionProvider`, `worldInfoProvider` (pacing, tick).
  - `tickProvider` (an `int`), `resourcesProvider`, `rateProvider`.
  - `fleetListProvider`, `fleetProvider(id)`, `selectedFleetIdProvider`.
  - `homeworldProvider`, `catalogProvider`, `queuesProvider` (derived countdowns).
  - `sectorProvider(Hex)` over a `SectorStore` that merges server updates into the **client-side intel cache** and stamps `last_seen_tick` locally until the server supplies it.
  - `intelStateProvider(Hex)` -> live / fresh / aging / stale / unknown (pure function of tick and `last_seen`).
  - `eventsProvider` with filter and own-only scoping; `alertsProvider` derived from events and state (raid ETA, low fuel, idle queue, full storage).
  - `routePlannerProvider` (controller with `mode`, `waypoints`, pending plan; pure BFS/Dijkstra in `hex/` reusing the logic in `iac.js` `route()`).
  - `chatProvider`, `tradeProvider`, `teamProvider`, `leaderboardProvider` (stubs backed by the demo provider until the server speaks).
- Widgets `select()` the slice they need, so a tick that changes only `tick` and one resource rebuilds only the HUD chip and the tile showing it.
- **Animations are not state.** Tickers and `AnimationController`s live in scene objects; state only supplies *targets* (fleet positions, hostiles).

### 9.5 Rendering approach: recommendation

**Use CustomPainter + a `Ticker` for the windshield and map; plain widgets for all HUD and panels. Do not adopt Flame.**

Reasons:
1. *The scenes are small and vector.* The windshield has at most ~60 entities (rocks, ships, particles, stars); the map draws about 600 visible hex nodes and edges. Both are a few hundred draw calls per frame, comfortably inside CanvasKit/Skwasm budgets, and the existing painters (`windshield_painter.dart`, `star_map_painter.dart`) already use this approach.
2. *The hard parts are not game-engine parts.* The difficult UI work is text, layout, hit-testing, focus, accessibility, responsive sheets, and forms (costs, sliders, chat). Those are Flutter widget strengths, and Flame would force a second component tree and its own input handling alongside them.
3. *Bundle and web maturity.* Flame adds a dependency and a game loop to a web build where size and startup matter, for features the game does not use (collision, physics, sprite sheets, tilemaps).
4. *Upgrade path stays open.* Painters are isolated behind `scene/` with no widget imports. If a future mode needs thousands of sprites, one scene can be hosted in a `GameWidget` without touching the rest.

Performance techniques (the prototype already does most of them in canvas 2D):
- Static layers (hex cell fills, zone rings, lane lines) recorded into `ui.Picture` and re-recorded only when intel or camera LOD changes; dynamic layers (fleets, pulses, particles) drawn on top. Separate `RepaintBoundary` for scene, HUD and panels.
- Camera as a single `Matrix4`; cull hexes to the viewport; LOD thresholds as in 5.3.
- Cache `Paragraph`s by string+style; never lay out text inside `paint` per frame.
- No `BackdropFilter` and no per-frame `MaskFilter.blur` on many shapes (both are expensive on web). Glows are pre-rendered radial-gradient sprites drawn with `drawImage`/`drawAtlas`; stars and particles use `drawRawPoints` / `drawAtlas`.
- Optional `FragmentProgram` shaders for the lens (scanlines, vignette) and warp streaks; compile once; works with CanvasKit and Skwasm; falls back to gradients when unsupported.
- Painters take `repaint: Listenable.merge([ticker, sceneNotifier])` so they repaint only when the scene animates, and the ticker is stopped when the view is hidden, the tab is hidden (`visibilitychange`), or reduced-motion is set.
- Web build: the project's `scripts/play.sh` already builds wasm (Skwasm) with a JS fallback; keep that, bundle fonts as assets.

### 9.6 Performance at 1 Hz tick, 60 fps visuals

- **Decouple clocks.** Server state arrives at 1 Hz into the store; the scene runs its own 60 fps ticker. Entities hold `from`/`to` snapshots and interpolate by `(now - lastTick)`; the HUD tick ring uses the same value. No state mutation per frame.
- **Tick-synchronised effects.** Events carried by tick N are *scheduled* into `[N, N+1)` with a seeded jitter, so a combat round's volleys spread across the second (as the prototype does: `setTimeout` offsets per ship). Visuals trail the server by at most 1 s; the LIVE badge accounts for that.
- **Budgeting.** Targets: < 4 ms paint, < 2 ms build on a mid laptop; particles capped (200), projectiles capped (24 per side), floaters (12). An adaptive quality governor watches the p95 frame time over 2 s and steps `lens -> glow -> particles -> 30 fps` down; a settings override pins it.
- **Rebuild discipline.** `const` constructors, `select()`, `ListView.builder` for logs, immutable models with `==` so unchanged slices do not notify. The 1 Hz tick should rebuild only: the HUD chips, the tick ring text, queue countdowns, and whichever fleet card changed.
- **Memory.** Intel cache bounded (e.g. 5,000 sectors, LRU, versioned schema in IndexedDB).
- **Testing.** Golden tests for panels and gate cards; a deterministic `SceneFixture` (seeded sector + scripted events) that renders frames headlessly for visual regression; the existing `DemoProvider` extended to play scripted tutorial and combat sequences.

### 9.7 Phasing

| Phase | Deliverable | Server dependency |
|---|---|---|
| 0 | Tokens, fonts as assets, theme, adaptive shell, HUD, bell drawer, toast and banner system, key sheet, palette | none |
| 1 | Overview (all panels from existing data; "Queue it" suggestions; alerts derived client-side) | none |
| 2 | Windshield v1 on the current protocol: scene, gates, actions, contacts, combat from `CombatRound` events; scan hints in gate cards; client intel cache | P0 items that are quick: `last_seen`, threat |
| 3 | Map v1: camera, nodes, lanes, threat and age layers, hover preview and click-to-route using a client planner; route executed as a chain of `move` commands client-side until `set_route` exists | ship catalog; `route_quote` later |
| 4 | Homeworld (hex base, tech graph, shipyard) over the existing catalog; Defence and Storage tabs behind a feature flag | storage and defence data |
| 5 | Fleets (split/merge/orders/log) | split/merge commands, `fleet_log`, optional params |
| 6 | Onboarding layer | seeded safe contacts |
| 7 | Comms, Rank, team-ups | chat, trade, team, leaderboard |
| 8 | Polish: sound, ambience, notifications, quality governor, accessibility audit | none |

Each phase ships behind the existing demo provider first, so the design can be reviewed offline.

### 9.8 Accessibility and input

- Keyboard-first: every control reachable by Tab, visible focus ring in `a300`, all shortcuts listed on `?`.
- Canvas content mirrored in a visually hidden semantic list (contacts, gates with threat and fuel cost) for screen readers.
- Contrast: body text >= 7:1; no information in colour alone (pips, numerals, shapes, labels); a colour-blind palette option.
- Touch: 44 px targets, long-press for the hover content, no hover-only affordances; safe-area insets on phones.
- `prefers-reduced-motion` and a manual motion setting.

---

## 10. What the prototypes are, and are not

- They are static HTML/CSS/JS with mock data: a seeded procedural world (hex lanes, terrain, ore, hostiles scaled with distance), three mock fleets, a simulated 1 Hz tick that advances resources, queues, intel ages and cooldowns, and a shared shell. They use Google Fonts only; everything else is in the folder.
- Windshield: jump, scan, harvest, combat (with a poor-odds confirmation), collect, board (with an ambush chance), recall along the safest lane, fleet switching, sector arrival banners. Map: pan, zoom, pinch, layers, hover preview, click and shift-click routing with three modes, held routes, animated travel. Overview and Homeworld: working queue and spend logic with affordability and reasons.
- They are **not** wired to the server, do not persist beyond a page load, and the combat model, odds, route costs and threat are mock formulas to be replaced by server values (section 8).
- Open the hub: `prototype/index.html`. Global keys work across pages (`G` then `O W M H`, `?`, `M` for sound).
- Screenshots were taken with headless Chromium over the DevTools protocol (`tools/shot.mjs`).

## 11. Questions for the owner

1. Trade delivery: instant, or physical (haulers, raid risk)? The UI works for both.
2. PvP in the Wandering: opt-in or forced? The map reserves a banner and a settings switch.
3. Leaderboard score: a single composite, or per-board only? Humans and agents share the ranking; should agents be filterable by default?
4. Is a "hold before Heavy+" route default acceptable for agents-style automation, or should it be a human-only convenience?
5. Notifications: is browser notification plus tab-title enough for v1, or do we want email/webhook?
6. How much of the CRT lens should survive on by default, given the legibility goal? (This spec defaults it to light and viewport-only.)
