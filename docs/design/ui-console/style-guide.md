# In Amber Clad: "Amber Console" style guide

Status: design and prototype only. Supersedes the "Amber Glass" look in `ui-redesign/spec.md` section 3. Interactions, layout and mock data are unchanged except where noted. Prototype: `prototype/` (open `index.html`). Screenshots: `shots/`.

Brief: "tone down the cartoony; imagine an IBM engineer from the 1970s." The result is an operator console: vector line-art, labelled fields, instrument gauges, a teleprinter log. The palette and the motion budget are kept; the rendering language is replaced.

## 1. Principles

1. **Draw with lines, not fills.** Ships, planets, asteroids, gates and symbols are outline geometry. The only fills are flat: status lamps, bar-graph segments, inverse-video primary buttons.
2. **Information is instrumentation.** A number is a labelled field or a register; a quantity is a segmented bar graph or an instrument dial with ticks; danger is a rating digit plus a bar, not just a colour.
3. **Phosphor, not neon.** Bloom is a faint additive underlay on lines that matter. Motion is beam sweep, plotter strokes, afterglow and CRT retrace; nothing bounces or squashes.
4. **Rules and brackets, not rounded cards.** Zero border radius. 1 px rules, boxed fields, corner marks.
5. **Still a game.** Jump, scan, combat and threat keep their anticipation, action and result. Legibility rules: no text below 9 px, data at 11 to 13 px, caps only for labels.

## 2. Tokens

Colours are unchanged from Amber Glass except the threat ramp (now 9 steps) and flatter surfaces.

| Token | Value | Use |
|---|---|---|
| `void` / `deep` | `#08060a` / `#0b0809` | page, shell |
| `panel` | `rgba(12,9,6,.93)` flat | all panels (no gradient, no blur) |
| `line` / `line-hi` / `line-lo` | amber at 24 / 55 / 10 % | borders, rules, row dividers |
| `a100`..`a900` | `#fff3d6` .. `#2e1e06` | amber ramp; `a500 #ffb000` is the one accent |
| `text` / `text-2` / `text-3` | `#f4dfb2` / `#bfa370` / `#8a7752` | body / secondary / labels only |
| `own` | `#5ee0cf` | own fleets, live intel, safe |
| `ember` | `#ff5a3c` | hostile, danger, destructive |
| `rare` | `#b793ff` | derelicts, anomalies, uncharted |
| `t1`..`t9` | `#4fb9a8 #7fcf8a #c9d85a #ffd24a #ff9a2e #ff6a3c #ff4040 #ff2e86 #d04bff` | threat T1 to T9; `t0 #4a6e68` is "clear" |
| `metal` `crystal` `deut` | `#d9b46a #7fd4ff #6fe6b6` | always paired with Fe / Cr / De text |
| ratio zones | deadly `#ff2e86`, risky `#ff9a2e`, favourable `#8fe388`, safe `#5ee0cf` | ratio gauge (`<1.2`, `1.2-2`, `2-3`, `>=3`) |

**Threat is redundant:** colour, the digit `T5`, a 9-cell bar filled to N, and (map) outline count (extra ring at T4+, T7+) and hatch density. Never colour alone.

### Type
Two families only, both from Google Fonts (`IBM Plex Mono` 400/500/600, `IBM Plex Sans Condensed` 400/500/600/700).

| Role | Face | Spec |
|---|---|---|
| Data, logs, coordinates, costs | Plex Mono | 11 to 13 px; tabular figures; 500 for values |
| Labels, titles, buttons, tabs | Plex Sans Condensed | 9 to 12 px caps, tracking .14 to .24 em, 600; display 26 to 40 px, 700, tracking .06 to .12 em |
| Register numerals | Plex Mono 500 | zero-padded to 6 digits, leading zeros at 28 % opacity (`007,344` shown as `007344`) |
| "Voice" | Plex Mono 11 px, `text-3` | prefixed `NOTE`; terse log-entry only ("Sector quiet. No signal above background."). The italic serif is gone. |

### Line weights and shapes
| Element | Stroke |
|---|---|
| Graticule, range rings, hairlines | 1 px, 10 to 16 % alpha, dashed `2 5` for rings |
| Panel border, bezel ticks | 1 px at 24 %; focus 1 px at 55 % |
| Ship hull, asteroid, symbol outline | 1.3 px (own), 1.3 px (hostile); inner spar lines 1 px at 55 % of hull alpha |
| Sector boundary, gates, selection | 1.6 to 2.2 px |
| Bloom underlay | 3.6 x the core width, additive, 12 % alpha |
| Corners | 0 px radius everywhere. Panel signature: 2 px amber corner marks, top-left and bottom-right, 9 px. |

Components: `.panel` (boxed, corner marks, header strip with 5 px square lamp), `.btn` (1 px border, caps; hover = inverse video; primary = amber slab), `.key` (boxed key cap), `.chip` (bracketed tag), `.bar` (LED bar graph: 3 px segments, 1 px gaps, 10 % tick scale beneath), `.thr` (T-digit + 9-cell bar), `.lamp` (7 px square lamp, filled and bloomed when on), `.rg` (ratio gauge), `dial()` (240 degree dial: 24 ticks, red warn arc, needle), `.cur` (blinking block cursor), `.kv` (dotted-leader label/value rows).

## 3. Symbol set

All are single SVG path strings in a +-50 box, nose up (`IAC.SYM` in `prototype/iac.js`); the same strings render as DOM `<svg>` and as canvas `Path2D`. Hostile and own are separated by silhouette, colour and a diamond frame, so they read in monochrome.

| Symbol | Form | Colour |
|---|---|---|
| Own ship, per class | symmetric drafted hull with bulkhead/spar lines. Scout: slim dart. Corvette: twin nacelles. Frigate: wide swept wings with gun ticks. Cruiser: segmented hull with turret squares. Hauler: boxed container grid. | `own` teal |
| Hostile ship, per class | swept, barbed "batwing" outlines, spine and chevron detail; a rotated-square lock frame when targeted or in combat | `ember` |
| Own fleet (map, lists) | open chevron | `own` |
| Hostile contact (map) | diamond frame around the T digit | threat ramp |
| Derelict | hull outline with a break and a fracture line; flickering lamps | `rare` |
| Salvage | circle with cross-hair ticks (plus mark); octagon pulse | gold `#ffcc66` |
| Ore | faceted rock polygon with facet lines; grade as 1 to 4 ticks; resource named by Fe/Cr/De | resource colour |
| Home | wireframe sphere, orbit ellipse, square platforms | amber, orbit `own` |
| Hub | concentric rings with six spokes | pale amber |
| Gate | two end posts (boxed), march of three chevrons pointing out | zone/threat colour, `rare` if uncharted |
| Uncharted | dashed brackets with `?` | `rare` |
| Planet (viewport) | limb circle, rotating meridians, latitude ellipses, hatched night side | amber |
| Asteroid (viewport) | irregular polygon, 3 chord facets, ore box + code | amber |
| Explosion | 8 to 14 short radial strokes, hull fragments (outline edges flying and spinning), expanding octagon ring | gold / ember |

## 4. Motion rules

Budget stays small; durations as before. Easing: `cubic-bezier(.2,.8,.2,1)` for settles, `steps(n)` for anything that should feel digital (bars, log typing, tick bar).

| Event | Treatment |
|---|---|
| Idle | ship bob (0.4 %), 3-stroke engine flicker, gate chevrons marching outward, ore-box pulse, blinking cursor, a 9 s faint beam band across the viewport |
| Afterglow | moving vectors (ships, shots, sparks, scan) draw on a persistence layer faded by 34 % per frame |
| Jump (1.1 s) | ships accelerate to the gate, stars become streaks, at the swap the viewport collapses to a bright horizontal line and re-expands (CRT retrace, 0.5 s), arrival banner wipes open |
| Scan (1.6 s) | PPI beam sweep: rotating line with a 22-line fading wedge, hex range ring expanding; gate cards update at 1.15 s; button cooldown is a bottom-up fill with a digit |
| Combat | red inner-edge alarm flickering at 1.3 s (steps), thin tracer shots with a bright head, hull/shield ticks above ships, `-12` readouts, hit sparks, kill = fragments + octagon ring + 10 px shake |
| Log | new line types in (clip-path, `steps(26)`), cursor follows the last line |
| Panels and toasts | wipe-in, never slide or bounce |
| Alerts | square lamps flick 50 % on a 1 s step; borders flick, no glow pulses |
| `prefers-reduced-motion` | all CSS animation and transitions off, persistence layer cleared fully, no shake, no retrace, planet and gates static |

## 5. What changed per screen, and why

**Shell.** HUD is a strip of labelled register fields: three resource registers with dimmed leading zeros and a cap gauge each, a world field (`PERSISTENT x1`: the pacing preset is fixed per world and shown everywhere times matter), a 10-segment tick bar instead of a spinner. Left rail keeps its icons, now square line glyphs with a 3 px active bar. Why: the old pills and glossy gems read as toy UI; fields read as instruments.

**Windshield.** The sector is a thin-line hex scope with a graticule (centre axes with minor/major ticks, dashed range hexes, 6 degree bezel scale with bearings) and a drawn closed-wall hatch. Ships are wireframes; the home planet is a wireframe sphere. Gate cards are boxed readouts with key cap, bearing, `T5 1FRG 2COR`, ore bar ticks, LIVE lamp. Right column: FUEL and HULL dials, CARGO/SHIELD bar graphs, key-value rows. Contacts show a diamond with the threat digit and a ratio gauge with the economy spec's labels (`>=3 SAFE`, `2-3 FAVOURABLE`, `1.2-2 RISKY`, `<1.2 DEADLY`) and the estimated win %. Firing at DEADLY needs a second `F`. Log is a teleprinter in caps. Why: combat and scan keep their drama (sweep, afterglow, retrace, fragments) with none of the filled gradient hulls.

**Map.** Nodes are hairline hexes on a plotter-registration grid; zones are dashed rings with `R08`/`R20` labels and tick-marked spokes. Threat is T1 to T9: digit in a diamond, hatched fill and extra outlines at T4+ in the threat layer, stepped flicker (not glow) at T5+. Stale intel is hatched and dashed. Routes are dashed plotter lines per hop coloured by threat with numbered hop boxes; HOLD is a diamond marker (hold threshold moved to T5+). The route panel ends with the `preview_move` text line: "SECTOR (10,-8) THREAT T4, EST NPC POWER 50, FLEET 90, RATIO 1.8 RISKY. FUEL 43 OF 96 (10 HOPS LEFT TO RETURN)", plus a ratio gauge for the worst sector. Legend lists T1 to T9, intel age and the symbol set.

**Overview.** Hero is a wireframe orbit schematic. Resource tiles are big registers with caps (Storage Vault L2), time-to-full, a scope-style production trace, and a NEAR CAP lamp at 85 % (crystal in the demo) / FULL at 98 %. Queue cards show queue depth `n/3`, queued items with "waits: 3,420 Fe", build slot B locked behind Modular Fabrication (needs Lab 4), and progress as a 40-tick bezel ring. Raid card uses the new defence breakdown (docked fleet + grid + structures vs estimated raid power). "Nearby space" is a PPI scope with symbols and a sweep. Ranking is one list with HUM/AI tags.

**Homeworld.** Base is a plan drawing: hex tiles with line icons, dashed conduit lines, a drawing title block ("IAC-HW-001"). Buildings now include Storage Vault and Fabricator. Tech graph uses right-angle schematic edges and adds Modular Fabrication. Shipyard cards show a wireframe ship on a drafting grid with power and cost per power. Defence and Storage tabs are now real screens (no "preview" banners): raid forecast with S-based range, structure table (Pulse Turret, Lancer Battery, Ion Bastion, Aegis Dome with requirements), vault bars with protected / raidable split, 85 % alert mark, "full in". Every disabled button still states why, now including "Build queue full (3/3)" and "Needs Storage Vault level N".

## 6. Flutter implementation

The look is cheap to build: everything is stroked paths, text and flat rects.

**Primitives (`CustomPainter`)**
- Port `IAC.SYM` path strings as `Path` via `parseSvgPathData` (package `path_drawing`) once at startup; cache the `Path`s and draw with `canvas.translate/rotate/scale`. Set `Paint.style = stroke`, `strokeJoin = miter`, `strokeCap = butt`, `strokeWidth = lw / scale` so weight stays 1.3 logical px at any size.
- Dashes (range rings, stale intel, harvest beams): `path_drawing` `dashPath`, or `PathMetric.extractPath` for animated dash offset. Hatching: clip to the hex path, draw parallel `drawLine`s, or tile an 8 px hatch `ImageShader`.
- Wireframe sphere: `drawCircle` limb, `drawOval` meridians with `rx = r*|cos(a)|`, latitude ovals with `ry = rx*.16`; night side = clipped hatch lines.
- Dials: `drawArc` for the scale and warn zone, a tick loop of `drawLine`, needle as a rotated line with an `AnimatedRotation`-style tween (700 ms, `easeOutCubic`).
- Bar gauge: `drawRect` track outline, then segments in a loop (3 px on, 1 px off), 10 % ticks below. Drive width with `TweenAnimationBuilder` using a stepped curve (`Interval` with `Curves.linear` quantised to 14 steps).
- Scan wedge: 22 `drawLine`s from the fleet with alpha `(1 - k/22) * .5`, angle `t*TAU*1.5 - k*.045`; hex range ring as a 6-point path.
- Hex tiles (homeworld): `Path` hex stroked; busy state = `dashPath` with animated phase.

**Stroke styles.** Three paints reused everywhere: `hair` (1 px, amber 12 to 16 %), `line` (1.3 px, state colour, alpha .9), `heavy` (1.6 to 2.2 px). Dim intel = lower alpha plus dash, never blur.

**Phosphor bloom, cheaply.** Do not use `MaskFilter.blur` or `ImageFilter.blur` on whole layers (costly on web). Instead stroke the same `Path` twice: first wide (3.6 x width, alpha .12, `BlendMode.plus`), then the crisp core. Only do this for hulls, gates, sector boundary and selection; skip it for graticule and background. For text numerals use a single `Shadow(blurRadius: 6, color: amber 25 %)` on the 3 to 4 big registers only. On weak devices drop the underlay.

**Afterglow (persistence).** Render ships, shots and sparks into a separate `RepaintBoundary` layer. Each frame, instead of clearing, paint a full-rect `Color(0x57000000)` with `BlendMode.dstOut` (34 %), then draw the new frame. Use `saveLayer` once per frame on that layer only. Disable (clear fully) when `MediaQuery.disableAnimations` is true.

**CRT retrace on jump.** Wrap the viewport in a `Transform.scale(scaleY)` with a `TweenSequence`: 1 to 0.012 (38 %), hold (24 %), back to 1, with a `ColorFiltered` brightness boost while squashed. 500 ms, `Curves.easeInOutCubic`.

**Scanlines and beam band.** A single `CustomPaint` over each viewport: 1 px black lines every 3 px at 16 % (precompute into a 1x3 `ImageShader` tile), vignette `RadialGradient`, plus one 90 px gradient band translated over 9 s. Only on the windshield and map.

**Fonts.** Bundle `IBM Plex Mono` (400, 500, 600) and `IBM Plex Sans Condensed` (400, 500, 600, 700) as assets in `pubspec.yaml` (OFL licence) so first paint does not wait on the network and the web build works offline from the game server; do not rely on runtime `google_fonts` fetching. Enable `FontFeature.tabularFigures()` on all numerals. Letter spacing: labels `0.14 to 0.24 em`; log lines `0.02 em`, uppercase via `toUpperCase()` in the log widget.

**Tokens.** Mirror section 2 as `const Color`s in `design/tokens.dart`; `Color threat(int t)` returns the 10-step ramp; `ThemeData` with `shape: RoundedRectangleBorder(borderRadius: zero)` globally and `visualDensity: compact`. Corner marks are a 20-line `CustomPainter` decorating `Container`.

**Perf notes.** One `Ticker` per viewport. The map can cache the static layer (graticule, zone rings, lanes) to a `ui.Picture` and invalidate on pan/zoom end or when intel changes; draw nodes, route and fleets live. Cull nodes outside the viewport before drawing, as the prototype does.

## 7. Open points
See the report: threat-to-power calibration of the mock data, whether to keep the retrace effect on by default, and the "caps log" legibility call.
