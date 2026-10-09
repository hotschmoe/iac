import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/intel.dart';
import '../../console/route_planner.dart';
import '../../console/services.dart';
import '../../console/threat.dart';
import '../../design/draw.dart';
import '../../design/gauges.dart';
import '../../design/panel.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../models/fleet.dart' as ui;
import '../../protocol/protocol.dart' as proto;
import '../../scenes/map/map_state.dart';

class FleetChips extends StatelessWidget {
  final Console con;
  final bool narrow;
  const FleetChips({super.key, required this.con, required this.narrow});

  @override
  Widget build(BuildContext context) {
    final fleets = con.state.fleets;
    final sel = con.game.currentFleet.id;
    return Row(children: [
      for (var i = 0; i < fleets.length && i < 9; i++) ...[
        if (i > 0) const SizedBox(width: 6),
        Flexible(child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 190), child: _chip(i, fleets[i], fleets[i].id == sel))),
      ],
    ]);
  }

  Widget _chip(int i, ui.FleetState f, bool on) {
    final hops = f.jumpFuel > 0 ? f.fuel ~/ f.jumpFuel : 0;
    final col = on ? C.own : C.a200;
    return GestureDetector(
      key: Key('fleet-chip-${f.id}'),
      onTap: () => con.game.selectFleet(i),
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        child: ConsolePanel(
          border: on ? C.own : null,
          mark: on ? C.own : null,
          fill: on ? const Color(0xF0091412) : null,
          padding: const EdgeInsets.fromLTRB(9, 6, 9, 6),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
            Row(children: [
              SymIcon((c, s) => Draw.symbol(c, Sym.fleet, s.width / 2, s.height / 2, s.width / 2, color: col, lw: 1.8, bloom: false), size: 11),
              const SizedBox(width: 6),
              Flexible(child: Text(f.name.toUpperCase(), overflow: TextOverflow.ellipsis, style: T.cond(size: 12, color: col, spacing: 1.4))),
              const Spacer(),
              KeyCap('${i + 1}', size: 15),
            ]),
            if (!narrow) ...[
              const SizedBox(height: 2),
              Text('${f.status.label} - $hops hops fuel', overflow: TextOverflow.ellipsis, style: T.mono(size: 9.5, color: C.text3)),
            ],
          ]),
        ),
      ),
    );
  }
}

class LayerBar extends StatelessWidget {
  final MapLayers layers;
  final bool scroll;
  const LayerBar({super.key, required this.layers, this.scroll = false});
  static const items = [
    ('threat', 'Threat', 'L'),
    ('ore', 'Ore', 'O'),
    ('age', 'Intel age', 'I'),
    ('lanes', 'Lanes', 'N'),
    ('derelict', 'Derelicts', 'D'),
    ('zones', 'Zones', 'Z'),
  ];

  @override
  Widget build(BuildContext context) {
    final chips = [
      for (final it in items)
        _LayerChip(label: it.$2, hint: it.$3, on: layers.get(it.$1), threat: it.$1 == 'threat', id: it.$1, onTap: () => layers.toggle(it.$1)),
    ];
    final box = Container(
      decoration: BoxDecoration(color: const Color(0xE6080604), border: Border.all(color: C.line)),
      child: scroll ? Row(mainAxisSize: MainAxisSize.min, children: chips) : Wrap(alignment: WrapAlignment.end, children: chips),
    );
    return scroll ? SingleChildScrollView(scrollDirection: Axis.horizontal, child: box) : box;
  }
}

class _LayerChip extends StatefulWidget {
  final String label, hint, id;
  final bool on, threat;
  final VoidCallback onTap;
  const _LayerChip({required this.label, required this.hint, required this.id, required this.on, required this.threat, required this.onTap});
  @override
  State<_LayerChip> createState() => _LayerChipState();
}

class _LayerChipState extends State<_LayerChip> {
  bool _hover = false;
  @override
  Widget build(BuildContext context) {
    final col = widget.on ? (widget.threat ? const Color(0xFFFFB3A3) : C.a200) : (_hover ? C.a300 : C.text3);
    return Semantics(
      button: true,
      toggled: widget.on,
      label: '${widget.label} layer',
      excludeSemantics: true,
      child: FocusableActionDetector(
        mouseCursor: SystemMouseCursors.click,
        onShowHoverHighlight: (v) => setState(() => _hover = v),
        actions: {ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) {
          widget.onTap();
          return null;
        })},
        child: GestureDetector(
          key: Key('layer-${widget.id}'),
          onTap: widget.onTap,
          child: Container(
            padding: const EdgeInsets.fromLTRB(9, 6, 10, 6),
            decoration: BoxDecoration(color: _hover ? C.a(C.a500, .06) : null, border: const Border(right: BorderSide(color: C.lineLo))),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              Container(
                width: 6,
                height: 6,
                decoration: BoxDecoration(color: widget.on ? col : null, border: Border.all(color: col), boxShadow: widget.on ? [BoxShadow(color: col.withValues(alpha: .6), blurRadius: 4)] : null),
              ),
              const SizedBox(width: 6),
              Text(widget.label.toUpperCase(), style: T.cond(size: 10.5, color: col, weight: FontWeight.w500, spacing: 1.4)),
              const SizedBox(width: 6),
              KeyCap(widget.hint, size: 14),
            ]),
          ),
        ),
      ),
    );
  }
}

class ZoomBar extends StatelessWidget {
  final MapViewport vp;
  final VoidCallback onHome, onFleet;
  final String coords;
  final bool column;
  const ZoomBar({super.key, required this.vp, required this.onHome, required this.onFleet, required this.coords, this.column = false});

  Widget _b(String l, VoidCallback f, {String? tip, double w = 32, Key? key}) => Semantics(
        button: true,
        label: tip ?? l,
        excludeSemantics: true,
        child: Tooltip(
          message: tip ?? l,
          child: FocusableActionDetector(
            mouseCursor: SystemMouseCursors.click,
            actions: {ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) {
              f();
              return null;
            })},
            child: GestureDetector(
              key: key,
              behavior: HitTestBehavior.opaque,
              onTap: f,
              child: Container(
                width: w == 0 ? null : w,
                height: 32,
                padding: w == 0 ? const EdgeInsets.symmetric(horizontal: 10) : null,
                alignment: Alignment.center,
                decoration: BoxDecoration(border: column ? const Border(bottom: BorderSide(color: C.lineLo)) : const Border(right: BorderSide(color: C.lineLo))),
                child: Text(l, style: w == 0 ? T.cond(size: 10, color: C.a300, spacing: 1.4) : T.mono(size: 16, color: C.a300)),
              ),
            ),
          ),
        ),
      );

  @override
  Widget build(BuildContext context) {
    final out = _b('−', () => vp.zoomAt(vp.size.center(Offset.zero), 1 / 1.3), tip: 'Zoom out (-)', key: const Key('zoom-out'));
    final inn = _b('+', () => vp.zoomAt(vp.size.center(Offset.zero), 1.3), tip: 'Zoom in (+)', key: const Key('zoom-in'));
    final home = _b('HOME', onHome, tip: 'Centre on homeworld', w: 0);
    final fleet = _b('FLEET', onFleet, tip: 'Centre on fleet (F)', w: 0);
    return IntrinsicWidth(
        child: ConsolePanel(
      flat: column,
      child: column
          ? Column(mainAxisSize: MainAxisSize.min, children: [inn, out, home, fleet])
          : Row(mainAxisSize: MainAxisSize.min, children: [
              out,
              SizedBox(
                  width: 200,
                  child: Center(
                      child: ListenableBuilder(
                          listenable: vp,
                          builder: (_, _) => Text(coords.isEmpty ? 'ZOOM ${vp.s.toStringAsFixed(2)}' : coords, style: T.mono(size: 10.5, color: C.text2, spacing: .4))))),
              inn,
              home,
              fleet,
            ]),
    ));
  }
}

class LegendPanel extends StatelessWidget {
  const LegendPanel({super.key});

  Widget _sym(void Function(Canvas, Size) f, String label, Color col, [double s = 14]) => Row(mainAxisSize: MainAxisSize.min, children: [
        SymIcon(f, size: s),
        const SizedBox(width: 4),
        Text(label.toUpperCase(), style: T.mono(size: 10, color: col)),
      ]);

  @override
  Widget build(BuildContext context) {
    return ConsolePanel(
      title: 'Legend',
      padding: const EdgeInsets.fromLTRB(12, 6, 12, 10),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
        const Lbl('Threat rating'),
        const SizedBox(height: 4),
        Row(children: [
          for (var i = 1; i <= 9; i++)
            Expanded(
                child: Container(
              height: 14,
              margin: const EdgeInsets.only(right: 1),
              alignment: Alignment.center,
              color: C.threat(i),
              child: Text('$i', style: T.cond(size: 9, color: const Color(0xFF120A02), weight: FontWeight.w700, spacing: 0)),
            )),
        ]),
        Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
            Text('LOW', style: T.mono(size: 8.5, color: C.text3)),
            Text('NPC POWER x2 PER STEP', style: T.mono(size: 8.5, color: C.text3)),
            Text('LETHAL', style: T.mono(size: 8.5, color: C.text3)),
          ]),
        ),
        const SizedBox(height: 9),
        const Lbl('Intel age'),
        Container(
          height: 6,
          margin: const EdgeInsets.symmetric(vertical: 3),
          decoration: BoxDecoration(
              border: Border.all(color: C.line),
              gradient: const LinearGradient(stops: [0, .4, .8, 1], colors: [Color(0xFFFFCF5A), Color(0xFFD99A2A), Color(0xFF8A7552), Color(0xFF4A4030)])),
        ),
        Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
          for (final t in const ['LIVE', '5m', '30m', 'FOSSIL']) Text(t, style: T.mono(size: 9, color: C.text3)),
        ]),
        const SizedBox(height: 9),
        Wrap(spacing: 10, runSpacing: 5, children: [
          _sym((c, s) => Draw.symbol(c, Sym.fleet, s.width / 2, s.height / 2, 7, color: C.own, lw: 1.6, bloom: false), 'fleet', C.own),
          _sym((c, s) => Draw.symbol(c, Sym.diamond, s.width / 2, s.height / 2, 7, color: C.threat(6), lw: 1.6, bloom: false), 'hostile', C.threat(6)),
          _sym((c, s) => Draw.symbol(c, Sym.salvage, s.width / 2, s.height / 2, 7, color: C.a400, lw: 1.8, bloom: false), 'salvage', C.a400),
          _sym((c, s) => Draw.symbol(c, Sym.derelict, s.width / 2, s.height / 2, 8, color: C.rare, lw: 1.6, bloom: false), 'derelict', C.rare, 16),
          _sym((c, s) => Draw.symbol(c, Sym.ore, s.width / 2, s.height / 2, 7, color: C.metal, lw: 1.6, bloom: false), 'ore', C.metal),
        ]),
      ]),
    );
  }
}

String zoneOf(proto.Hex h) {
  final d = h.distFromOrigin;
  return d == 0 ? 'Central Hub' : d <= 8 ? 'Inner Ring' : d <= 20 ? 'Outer Ring' : 'The Wandering';
}

/// "2 hops direct (17 charted), through unexplored space, 12 fuel".
String _wayHome(proto.MovePreview p) {
  final charted = p.chartedHopsHome;
  final route = charted == null
      ? '${p.hopsHome} hops direct, none charted'
      : charted == p.hopsHome
          ? '${p.hopsHome} hops'
          : '${p.hopsHome} hops direct ($charted charted)';
  return '$route${p.routeUnexplored ? ', through unexplored space' : ''}, ${p.fuelToReturn.round()} fuel${p.canReturn ? '' : ' - CANNOT RETURN'}';
}

class Inspector extends StatelessWidget {
  final Console con;
  final MapPlan plan;
  final proto.Hex? hex;
  final VoidCallback onRoute, onWaypoint;
  const Inspector({super.key, required this.con, required this.plan, required this.hex, required this.onRoute, required this.onWaypoint});

  @override
  Widget build(BuildContext context) {
    final h = hex;
    if (h == null) {
      return const ConsolePanel(
        title: 'Sector',
        padding: EdgeInsets.fromLTRB(12, 8, 12, 12),
        child: _Voice('Hover a sector to read it. Click to plan a route. Drag to pan, scroll to zoom.'),
      );
    }
    final st = con.state;
    final sec = st.sectors[h];
    final it = Intel.of(sec, st.tick);
    final th = SectorThreat.of(sec);
    final fleet = con.game.currentFleet;
    final pw = fleet.power;
    final pv = fleet.id == 0 ? null : st.previewFor(fleet.id, h);
    final odds = pv != null ? RatioInfo.fromPreview(pv) : th.hostile ? RatioInfo.of(pw, th.power) : null;
    final oddsPower = pv?.threat.estPower ?? th.power;
    final d = h.distFromOrigin;
    final rich = sec == null ? 0 : sec.resources.metal.index + sec.resources.crystal.index + sec.resources.deuterium.index;
    final expect = 1.2 + d * .11;
    final selected = plan.selected == h;
    final onFleet = fleet.sector == h;
    return ConsolePanel(
      title: selected ? 'Selected' : 'Sector',
      subText: zoneOf(h),
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Row(crossAxisAlignment: CrossAxisAlignment.center, children: [
          Text('${h.q}, ${h.r}', style: T.cond(size: 26, color: C.a100, weight: FontWeight.w700, spacing: 1)),
          const SizedBox(width: 10),
          Flexible(child: Tag(sec == null ? 'Uncharted' : sec.terrain.label)),
        ]),
        if (sec == null)
          _box('No data. A scout must visit, or a scan from an adjacent sector will reveal it.', C.rare, C.rareDim, fill: false)
        else if (it.live)
          _box('LIVE in sensor range. Hostiles shown are real.', C.own, C.ownDim, fill: false)
        else
          _box('SEEN ${ago(it.ticksOld).toUpperCase()} AGO. HOSTILES MAY HAVE MOVED; ORE MAY HAVE BEEN MINED.', const Color(0xFFC9B284), const Color(0xFF6F5C3E), fossil: true),
        if (sec != null) ...[
          Kv('Threat',
              vw: Row(mainAxisSize: MainAxisSize.min, children: [
                ThreatBadge(th.ringRating),
                const SizedBox(width: 8),
                Text(th.basisLabel, key: const Key('inspector-basis'), style: T.mono(size: 9.5, color: C.text3, spacing: .6)),
              ])),
          if (th.hostile) Kv(th.compLabel, vc: C.threat(th.rating), v: th.behavior?.wire.toUpperCase() ?? ''),
          if (odds != null) ...[
            Kv('Ratio',
                vw: Text.rich(key: const Key('inspector-ratio'), TextSpan(children: [
                  TextSpan(text: '${odds.r.toStringAsFixed(2)} ${odds.label}', style: T.mono(size: 11, color: odds.color, weight: FontWeight.w500)),
                  TextSpan(text: '  ${oddsPower.round()} vs ${pw.round()} (${fleet.name})', style: T.mono(size: 11, color: C.text3)),
                ]))),
            RatioGauge(odds.r),
          ],
          if (pv != null)
            Kv('Way home',
                vw: Text(_wayHome(pv), key: const Key('inspector-home'), style: T.mono(size: 10.5, color: pv.canReturn ? C.text2 : C.threat(7)))),
          _oreRow('Metal', sec.resources.metal, C.metal, sec.oreReserve?.metal),
          _oreRow('Crystal', sec.resources.crystal, C.crystal, sec.oreReserve?.crystal),
          _oreRow('Deut', sec.resources.deuterium, C.deut, sec.oreReserve?.deuterium),
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 4),
            child: Text('${rich >= expect * 3 ? 'RICH' : rich <= expect ? 'POOR' : 'TYPICAL'} FOR THIS DISTANCE - LOOT TIER x${1 + math.min(3, d ~/ 6)}',
                style: T.mono(size: 10, color: C.text3)),
          ),
          if (salvagePinVisible(sec, st.tick))
            Kv('Salvage',
                vw: Text.rich(TextSpan(children: [
                  TextSpan(text: '${sec.salvage!.metal.round()} ', style: T.mono(size: 11, color: C.metal)),
                  TextSpan(text: '${sec.salvage!.crystal.round()} ', style: T.mono(size: 11, color: C.crystal)),
                  TextSpan(text: '${sec.salvage!.deuterium.round()}', style: T.mono(size: 11, color: C.deut)),
                ]))),
          if (salvagePinVisible(sec, st.tick) && despawnLabel(sec, st.tick) != null)
            Kv('Despawns',
                v: clockFmt(salvageSecondsLeft(sec, st.tick)!),
                vc: salvageSecondsLeft(sec, st.tick)! <= 60 ? C.ember : C.text2),
          if (sec.site != null)
            Kv('Derelict', v: 'tier ${sec.site!.tier} - risk ${sec.site!.risk.label}', vc: sec.site!.risk == proto.SiteRisk.hot ? C.ember : sec.site!.risk == proto.SiteRisk.uneasy ? C.threat(5) : C.ok),
          if (sec.pinsStale && (salvagePinVisible(sec, st.tick) || sec.site != null))
            Kv('Pins', v: 'STALE - unconfirmed since ${ago(it.ticksOld)} ago', vc: C.fossil),
        ],
        Kv('Distance', v: '${proto.Hex.distance(fleet.sector, h)} from ${fleet.name}\n${proto.Hex.distance(h, st.homeworld)} from home - $d from hub'),
        if (plan.why != null) Padding(padding: const EdgeInsets.only(top: 6), child: Text(plan.why!, key: const Key('route-why'), style: T.mono(size: 10.5, color: C.ember, height: 1.3))),
        const SizedBox(height: 8),
        Wrap(spacing: 6, runSpacing: 6, children: [
          ConsoleButton('Route here', small: true, primary: true, onPressed: onRoute),
          ConsoleButton('+ Waypoint', small: true, onPressed: onWaypoint),
          if (sec != null && !it.live)
            ConsoleButton('Refresh intel',
                small: true,
                onPressed: onFleet && fleet.id != 0 ? () => con.game.sendCommand(proto.ScanCommand(fleetId: fleet.id)) : null,
                reason: 'Needs a fleet in this sector: ${fleet.name} is ${proto.Hex.distance(fleet.sector, h)} hops away'),
        ]),
      ]),
    );
  }

  Widget _oreRow(String k, proto.Density d, Color col, proto.TileReserve? r) => Kv(k,
      vw: Row(mainAxisSize: MainAxisSize.min, children: [
        if (r != null && r.maxUnits > 0) ...[
          Text(reserveText(r), key: Key('ore-reserve-$k'), style: T.mono(size: 10, color: r.units < r.maxUnits ? C.text2 : C.text3)),
          const SizedBox(width: 8),
        ],
        SegBar(4, d.index, color: col, w: 20, h: 7),
      ]));

  Widget _box(String text, Color col, Color border, {bool fossil = false, bool fill = true}) => Container(
        margin: const EdgeInsets.symmetric(vertical: 6),
        padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 6),
        decoration: BoxDecoration(
          border: Border.all(color: border),
          color: fossil ? const Color(0x1F8A7552) : null,
        ),
        child: Text(text, style: T.mono(size: 10.5, color: col, height: 1.3)),
      );
}

class _Voice extends StatelessWidget {
  final String t;
  const _Voice(this.t);
  @override
  Widget build(BuildContext context) => Text.rich(TextSpan(children: [
        TextSpan(text: 'NOTE ', style: T.mono(size: 11, color: C.a700, weight: FontWeight.w600, spacing: 1)),
        TextSpan(text: t, style: T.mono(size: 11, color: C.text3)),
      ]));
}

class RouteInfo {
  final RoutePlan plan;
  final Map<RouteMode, RoutePlan?> alts;
  final proto.Hex dest;
  final bool pending;
  const RouteInfo(this.plan, this.alts, this.dest, this.pending);
}

class RoutePanel extends StatelessWidget {
  final Console con;
  final RouteInfo info;
  final MapPlan state;
  final VoidCallback onEngage, onClear;
  const RoutePanel({super.key, required this.con, required this.info, required this.state, required this.onEngage, required this.onClear});

  @override
  Widget build(BuildContext context) {
    final plan = info.plan;
    final st = con.state;
    final fleet = con.game.currentFleet;
    final secs = [for (final h in plan.path) (h, st.sectors[h])];
    final worst = plan.worstHop >= 0 ? plan.path[plan.worstHop] : null;
    final worstTh = worst == null ? SectorThreat.clear : SectorThreat.of(st.sectors[worst]);
    // The server rates the jump out of the fleet's sector; hops beyond it
    // are rated from the server's threat power.
    final first = fleet.id == 0 || plan.path.isEmpty ? null : st.previewFor(fleet.id, plan.path.first);
    final wi = plan.worstHop == 0 && first != null
        ? RatioInfo.fromPreview(first)
        : worstTh.hostile ? RatioInfo.of(plan.fleetPower, worstTh.power) : null;
    final destSec = st.sectors[info.dest];
    final dth = SectorThreat.of(destSec);
    final di = plan.hops == 1 && first != null
        ? RatioInfo.fromPreview(first)
        : dth.hostile ? RatioInfo.of(plan.fleetPower, dth.power) : null;
    final retOk = plan.hops == 1 && first != null ? first.canReturn : plan.canReturn;
    final fuelPct = fleet.fuelMax > 0 ? math.max(0, plan.fuelLeft) / fleet.fuelMax : 0.0;
    final per = math.max(1, fleet.jumpFuel);
    final hopsLeft = math.max(0, plan.fuelLeft ~/ per);
    final stale = plan.intel.where((i) => i.age == IntelAge.stale || i.age == IntelAge.aging).length;
    return ConsolePanel(
      title: info.pending ? 'Route plan' : 'Route preview',
      subText: '${fleet.name} > ${info.dest.q},${info.dest.r}',
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Padding(
          padding: const EdgeInsets.only(bottom: 8),
          child: Row(children: [
            for (final m in RouteMode.values)
              Expanded(
                child: Padding(
                  padding: EdgeInsets.only(right: m == RouteMode.safe ? 0 : 4),
                  child: GestureDetector(
                    key: Key('mode-${m.name}'),
                    onTap: () => state.setMode(m),
                    child: MouseRegion(
                      cursor: SystemMouseCursors.click,
                      child: Container(
                        alignment: Alignment.center,
                        padding: const EdgeInsets.symmetric(vertical: 5),
                        decoration: BoxDecoration(
                            border: Border.all(color: m == state.mode ? C.a500 : C.line), color: m == state.mode ? C.a(C.a500, .14) : null),
                        child: Text('${m.name.toUpperCase()} ${info.alts[m]?.hops ?? '-'}',
                            style: T.cond(size: 10.5, color: m == state.mode ? C.a200 : C.text3, weight: FontWeight.w500, spacing: 1.4)),
                      ),
                    ),
                  ),
                ),
              ),
          ]),
        ),
        Kv('Hops', vw: Text('${plan.hops}', key: const Key('route-hops'), style: T.mono(size: 11.5, weight: FontWeight.w500))),
        Kv('Fuel',
            vw: Text.rich(key: const Key('route-fuel'), TextSpan(children: [
              TextSpan(text: '${plan.fuel}', style: T.mono(size: 11.5, color: plan.canMake ? C.text : C.threat(7), weight: FontWeight.w500)),
              TextSpan(text: ' of ${fleet.fuel}', style: T.mono(size: 11.5, color: C.text3)),
            ]))),
        Padding(padding: const EdgeInsets.symmetric(vertical: 4), child: LedBar(fuelPct, color: fuelPct < .2 ? C.ember : C.a400, height: 7)),
        Kv('Return trip',
            vw: Text(
                plan.hops == 1 && first != null
                    ? (first.canReturn ? 'OK, ${first.fuelToReturn.round()} fuel home' : 'NO FUEL TO COME HOME (needs ${first.fuelToReturn.round()})')
                    : (plan.canReturn ? 'OK, margin ${plan.returnMargin} fuel' : 'NO FUEL TO COME HOME (needs ${plan.backFuel})'),
                key: const Key('route-return'), style: T.mono(size: 10.5, color: retOk ? C.threat(1) : C.threat(7)))),
        if (first != null)
          Kv('Way home', vw: Text(_wayHome(first), key: const Key('route-home'), style: T.mono(size: 10.5, color: C.text2))),
        Kv('ETA', vw: Text(clockFmt(plan.etaTicks), style: T.mono(size: 11.5, weight: FontWeight.w500))),
        Kv('Threat along route', vw: ThreatBadge(plan.maxThreat, sm: true)),
        _Strip(plan: plan, secs: secs),
        Text('${plan.hostileSectors} HOSTILE SECTOR${plan.hostileSectors == 1 ? '' : 'S'} - $stale ON STALE OR MISSING INTEL', style: T.mono(size: 10, color: C.text3)),
        if (wi != null) ...[
          const SizedBox(height: 6),
          Kv('Worst ratio',
              vw: Text.rich(TextSpan(children: [
                TextSpan(text: '${wi.r.toStringAsFixed(2)} ${wi.label}', style: T.mono(size: 11, color: wi.color, weight: FontWeight.w500)),
                TextSpan(text: ' vs T${worstTh.rating}', style: T.mono(size: 11, color: C.text3)),
              ]))),
          RatioGauge(wi.r),
        ],
        Container(
          margin: const EdgeInsets.only(top: 8),
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
          decoration: BoxDecoration(color: const Color(0x66000000), border: Border.all(color: C.line)),
          child: Text.rich(
            key: const Key('route-preview-text'),
            TextSpan(children: [
              TextSpan(
                  text: '> SECTOR (${info.dest.q},${info.dest.r}) THREAT ${destSec == null ? 'UNKNOWN' : dth.hostile ? 'T${dth.rating}' : 'CLEAR'}'
                      '${dth.hostile ? ', EST NPC POWER ${dth.power.round()}, FLEET ${plan.fleetPower.round()}, RATIO ${di!.r.toStringAsFixed(1)} ' : ''}'),
              if (di != null) TextSpan(text: di.label, style: TextStyle(color: di.color)),
              if (first != null && plan.hops > 1) TextSpan(text: '. NEXT HOP (${first.target.q},${first.target.r}) T${first.threat.rating} ${first.threat.basis.wire.toUpperCase()}, RATIO ${first.ratio.toStringAsFixed(1)} ${first.label.label}'),
              TextSpan(text: '. FUEL ${plan.fuel} OF ${fleet.fuel} ($hopsLeft HOPS LEFT TO RETURN)'),
            ]),
            style: T.mono(size: 10.5, color: C.text2, height: 1.55),
          ),
        ),
        const SizedBox(height: 8),
        _HoldCheck(key: const Key('hold-check'), on: state.hold, count: plan.holds.length, onChanged: state.setHold),
        if (info.pending) ...[
          const SizedBox(height: 10),
          Wrap(spacing: 6, runSpacing: 6, children: [
            ConsoleButton('Engage', keyHint: 'Enter', primary: true, onPressed: plan.canMake ? onEngage : null, reason: 'Route needs ${plan.fuel} fuel, fleet has ${fleet.fuel}'),
            ConsoleButton('Clear', keyHint: 'Esc', onPressed: onClear),
          ]),
          if (!plan.canMake) WhyText('Route needs ${plan.fuel} fuel, fleet has ${fleet.fuel}.'),
          const Padding(padding: EdgeInsets.only(top: 6), child: Text('Shift+click to add a waypoint.', style: TextStyle(fontSize: 10, color: C.text3, fontFamily: 'IBMPlexMono'))),
        ] else
          const Padding(padding: EdgeInsets.only(top: 8), child: Text('Click to set destination. Shift+click adds a waypoint.', style: TextStyle(fontSize: 10, color: C.text3, fontFamily: 'IBMPlexMono'))),
      ]),
    );
  }
}

class _HoldCheck extends StatelessWidget {
  final bool on;
  final int count;
  final ValueChanged<bool> onChanged;
  const _HoldCheck({super.key, required this.on, required this.count, required this.onChanged});
  @override
  Widget build(BuildContext context) => Semantics(
        checked: on,
        label: 'Hold before T5 plus sectors',
        excludeSemantics: true,
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: () => onChanged(!on),
          child: MouseRegion(
            cursor: SystemMouseCursors.click,
            child: Row(children: [
              Container(width: 13, height: 13, decoration: BoxDecoration(border: Border.all(color: C.a500), color: on ? C.a500 : null), child: on ? const Icon(Icons.check, size: 11, color: Color(0xFF1C1000)) : null),
              const SizedBox(width: 8),
              Flexible(child: Text('HOLD BEFORE T5+ SECTORS ($count)', style: T.mono(size: 11, color: C.text2, spacing: .3))),
            ]),
          ),
        ),
      );
}

class _Strip extends StatelessWidget {
  final RoutePlan plan;
  final List<(proto.Hex, proto.SectorState?)> secs;
  const _Strip({required this.plan, required this.secs});
  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 6),
        child: SizedBox(
          height: 18,
          child: Semantics(
            label: 'Threat per hop: ${plan.threats.join(', ')}',
            excludeSemantics: true,
            child: CustomPaint(painter: _StripPainter(plan), size: Size.infinite),
          ),
        ),
      );
}

class _StripPainter extends CustomPainter {
  final RoutePlan plan;
  _StripPainter(this.plan);
  @override
  void paint(Canvas c, Size size) {
    final n = plan.hops;
    if (n == 0) return;
    final w = (size.width - (n - 1) * 2) / n;
    for (var i = 0; i < n; i++) {
      final t = plan.threats[i];
      final it = plan.intel[i];
      final r = Rect.fromLTWH(i * (w + 2), 0, w, size.height);
      final col = t == 0 ? const Color(0xFF2F6B62) : C.threat(t);
      c.drawRect(r, Paint()..color = col.withValues(alpha: it.age == IntelAge.stale ? .7 : 1));
      if (it.age == IntelAge.stale || it.age == IntelAge.aging) {
        c.save();
        c.clipRect(r);
        final hp = Draw.line(Colors.black, 1.5, .55);
        for (var k = -size.height; k < w; k += 5) {
          c.drawLine(Offset(r.left + k, r.bottom), Offset(r.left + k + size.height, r.top), hp);
        }
        c.restore();
      }
      if (plan.holds.contains(i)) c.drawRect(Rect.fromLTWH(r.right - 1, -4, 2, size.height + 8), Paint()..color = C.a100);
      if (t > 0 && w >= 8) Draw.text(c, '$t', r.center.dx, r.center.dy, T.cond(size: 9, color: const Color(0xFF120A02), weight: FontWeight.w700, spacing: 0), anchor: .5);
    }
  }

  @override
  bool shouldRepaint(_StripPainter o) => o.plan != plan;
}

class HoldPanel extends StatelessWidget {
  final Console con;
  final proto.Hex ahead;
  final VoidCallback onContinue, onReroute, onAbort;
  const HoldPanel({super.key, required this.con, required this.ahead, required this.onContinue, required this.onReroute, required this.onAbort});

  @override
  Widget build(BuildContext context) {
    final fleet = con.game.currentFleet;
    final th = SectorThreat.of(con.state.sectors[ahead]);
    final o = RatioInfo.of(fleet.power, th.power);
    return ConsolePanel(
      key: const Key('hold-panel'),
      title: 'Route held',
      titleColor: C.ember,
      border: C.ember,
      mark: C.ember,
      subText: fleet.name,
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Row(children: [
          Flexible(child: Text('> HOLD. ', style: T.mono(size: 11.5, color: C.text))),
          ThreatBadge(th.rating, sm: true),
          Flexible(child: Text(' CONTACTS AHEAD AT ${ahead.q},${ahead.r}', style: T.mono(size: 11.5, color: C.text))),
        ]),
        const SizedBox(height: 8),
        Kv('Hostiles', v: th.compLabel, vc: C.threat(th.rating)),
        Kv('Their power', v: '${th.power.round()} vs your ${fleet.power.round()}'),
        Kv('Ratio', v: '${o.r.toStringAsFixed(2)} ${o.label}', vc: o.color),
        RatioGauge(o.r),
        const SizedBox(height: 8),
        Wrap(spacing: 6, runSpacing: 6, children: [
          ConsoleButton('Continue', danger: true, onPressed: onContinue),
          ConsoleButton('Reroute safe', onPressed: onReroute),
          ConsoleButton('Abort', onPressed: onAbort),
        ]),
      ]),
    );
  }
}

class ActiveRoutePanel extends StatelessWidget {
  final Console con;
  const ActiveRoutePanel({super.key, required this.con});
  @override
  Widget build(BuildContext context) {
    final r = con.runner;
    return ConsolePanel(
      title: 'Route engaged',
      subText: '${r.done} of ${r.full.length} hops',
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        LedBar(r.full.isEmpty ? 0 : r.done / r.full.length, color: C.own, height: 7),
        const SizedBox(height: 8),
        Align(alignment: Alignment.centerLeft, child: ConsoleButton('Abort', keyHint: 'Esc', danger: true, small: true, onPressed: () => r.abort())),
      ]),
    );
  }
}
