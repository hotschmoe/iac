import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';

import '../../console/route_planner.dart';
import '../../console/services.dart';
import '../../console/threat.dart';
import '../../console/toasts.dart';
import '../../design/beat.dart';
import '../../design/lens.dart';
import '../../design/tokens.dart';
import '../../console/prefs.dart';
import '../../protocol/protocol.dart' as proto;
import '../../console/route_runner.dart';
import '../../scenes/map/map_painter.dart';
import '../../scenes/map/map_state.dart';
import 'map_panels.dart';

class MapView extends StatefulWidget {
  const MapView({super.key});

  @override
  State<MapView> createState() => _MapViewState();
}

class _MapViewState extends State<MapView> with SingleTickerProviderStateMixin {
  final vp = MapViewport();
  final layers = MapLayers();
  final plan = MapPlan();
  final clock = ValueNotifier<double>(0);
  final cache = MapStaticCache();
  final Stopwatch _sw = Stopwatch()..start();

  late Console con;
  bool _init = false;
  Ticker? _ticker;
  double _lastClock = 0;
  VoidCallback? _unreg;

  late MapData data;
  final Map<int, proto.Hex> _lastPos = {};
  final Map<int, MapMove> _moves = {};
  int _selFleet = -1;
  bool _sheet = false;
  String _coords = '';
  Map<RouteMode, RoutePlan?> _alts = {};

  final Map<int, Offset> _pointers = {};
  Offset? _downAt;
  bool _moved = false;
  double? _pinchD;
  double _pinchS = 1;

  bool get still => reducedMotion(context) || con.prefs.reduceMotion;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_init) return;
    _init = true;
    con = Console.of(context);
    data = _data();
    _selFleet = con.game.currentFleet.id;
    vp.centerOn(con.game.currentFleet.sector, scale: 1.15);
    con.game.addListener(_onGame);
    con.runner.addListener(_onRunner);
    con.prefs.addListener(_onPrefs);
    _unreg = con.registerKeys(Screen.map, _onKey);
    if (!still) {
      _ticker = createTicker((_) {
        final t = _sw.elapsedMicroseconds / 1e6;
        if (t - _lastClock >= .05) {
          _lastClock = t;
          clock.value = t;
        }
      })
        ..start();
    }
  }

  @override
  void dispose() {
    con.game.removeListener(_onGame);
    con.runner.removeListener(_onRunner);
    con.prefs.removeListener(_onPrefs);
    _unreg?.call();
    _ticker?.dispose();
    cache.dispose();
    for (final c in [vp, layers, plan, clock]) {
      c.dispose();
    }
    super.dispose();
  }

  void _onPrefs() {
    if (!mounted) return;
    setState(() {});
  }

  void _onRunner() {
    if (!mounted) return;
    setState(() => data = _data());
  }

  MapData _data() {
    final st = con.state;
    final r = con.runner;
    final heads = <int, proto.Hex>{};
    if (r.active && r.fleetId != null && r.remaining.isNotEmpty) heads[r.fleetId!] = r.remaining.first;
    return MapData(
      sectors: st.sectors,
      signals: st.signals,
      fleets: st.fleets,
      selectedFleet: con.game.currentFleet.id,
      home: st.homeworld,
      tick: st.tick,
      moves: _moves,
      headings: heads,
    );
  }

  void _onGame() {
    if (!mounted) return;
    final now = _sw.elapsedMicroseconds / 1e6;
    for (final f in con.state.fleets) {
      final prev = _lastPos[f.id];
      if (prev != null && prev != f.sector && !still) _moves[f.id] = MapMove(prev, f.sector, now);
      _lastPos[f.id] = f.sector;
    }
    final sel = con.game.currentFleet.id;
    if (sel != _selFleet) {
      _selFleet = sel;
      plan.clear();
    }
    _recompute();
    _askServer(plan.hover);
    setState(() => data = _data());
  }

  // ---- planning ----

  proto.Hex get _from => con.game.currentFleet.sector;

  RoutePlan? _chain(List<proto.Hex> stops, RouteMode m) {
    final f = con.game.currentFleet;
    if (f.id == 0 || stops.isEmpty) return null;
    return RoutePlanner.chain(
        sectors: con.state.sectors, tick: con.state.tick, fleet: f, from: _from, stops: stops, home: con.state.homeworld, power: f.power, mode: m);
  }

  void _recompute() {
    if (plan.waypoints.isEmpty) {
      _alts = {};
      return;
    }
    _alts = {for (final m in RouteMode.values) m: _chain(plan.waypoints, m)};
    plan.plan = _alts[plan.mode];
  }

  void _setDest(proto.Hex h, {bool append = false}) {
    final f = con.game.currentFleet;
    final stops = append && plan.planning ? [...plan.waypoints, h] : [h];
    String? why;
    if (f.id == 0) {
      why = 'No fleet to route.';
    } else if (h == f.sector && stops.length == 1) {
      why = '${f.name} is already at ${h.q},${h.r}.';
    } else if (!con.state.sectors.containsKey(h)) {
      why = 'No charted lane to ${h.q},${h.r}. Scan from a nearby sector first.';
    }
    final p = why == null ? _chain(stops, plan.mode) : null;
    if (why == null && p == null) why = 'No known lane reaches ${h.q},${h.r}. Scan from a nearby sector first.';
    if (why != null) {
      plan.why = why;
      plan.touch();
      con.toasts.show('No route', why, tone: ToastTone.red);
      return;
    }
    plan.set(stops, p);
    plan.selected = h;
    _alts = {for (final m in RouteMode.values) m: _chain(stops, m)};
    _askServer(plan.hover);
    con.play('ok');
    setState(() => _sheet = true);
  }

  void _engage() {
    final p = plan.plan;
    if (p == null) return;
    final f = con.game.currentFleet;
    if (!p.canMake) {
      con.toasts.show('Not enough fuel', 'Route needs ${p.fuel}, fleet has ${f.fuel}', tone: ToastTone.red);
      return;
    }
    con.play('jump');
    con.runner.start(p, f.id, hold: plan.hold);
    plan.clear();
    setState(() => data = _data());
  }

  void _clear() {
    plan.clear();
    setState(() {});
  }

  void _reroute() {
    final r = con.runner;
    if (r.full.isEmpty || r.fleetId == null) return;
    final dest = r.full.last;
    r.abort('rerouting');
    final p = _chain([dest], RouteMode.safe);
    if (p == null) {
      con.toasts.show('No route', 'No safer lane to ${dest.q},${dest.r}', tone: ToastTone.red);
      return;
    }
    plan.mode = RouteMode.safe;
    plan.set([dest], p);
    _alts = {for (final m in RouteMode.values) m: _chain([dest], m)};
    con.toasts.show('Rerouted', 'Safest known lane, ${p.hops} hops. Engage to go.', tone: ToastTone.teal);
    setState(() => _sheet = true);
  }

  // ---- input ----

  bool _onKey(KeyEvent e) {
    final k = e.logicalKey;
    final ch = e.character?.toLowerCase();
    const l = {'l': 'threat', 'o': 'ore', 'i': 'age', 'n': 'lanes', 'd': 'derelict', 'z': 'zones'};
    if (ch != null && l.containsKey(ch)) {
      layers.toggle(l[ch]!);
      con.play('click');
      return true;
    }
    if (k == LogicalKeyboardKey.enter || k == LogicalKeyboardKey.numpadEnter) {
      if (con.runner.phase == RunPhase.held) return false;
      if (plan.plan == null) return false;
      _engage();
      return true;
    }
    if (k == LogicalKeyboardKey.escape) {
      if (plan.planning) {
        plan.selected = null;
        _clear();
      } else if (plan.selected != null) {
        plan.select(null);
        plan.why = null;
      } else if (con.runner.active) {
        con.runner.abort();
      } else {
        return false;
      }
      return true;
    }
    if (ch == 'h') {
      vp.centerOn(con.state.homeworld, scale: 1.3);
      return true;
    }
    if (ch == 'f') {
      vp.centerOn(_from, scale: 1.5);
      return true;
    }
    if (ch == '+' || ch == '=') {
      vp.zoomAt(vp.size.center(Offset.zero), 1.25);
      return true;
    }
    if (ch == '-') {
      vp.zoomAt(vp.size.center(Offset.zero), .8);
      return true;
    }
    if (ch == '0') {
      vp.centerOn(proto.Hex.origin, scale: .5);
      return true;
    }
    if (ch != null && ch.length == 1 && ch.codeUnitAt(0) >= 0x31 && ch.codeUnitAt(0) <= 0x39) {
      final i = ch.codeUnitAt(0) - 0x31;
      if (i < con.state.fleets.length) {
        con.game.selectFleet(i);
        vp.centerOn(con.game.currentFleet.sector);
        return true;
      }
      return false;
    }
    const step = 60.0;
    if (k == LogicalKeyboardKey.arrowLeft) {
      vp.panBy(const Offset(step, 0));
    } else if (k == LogicalKeyboardKey.arrowRight) {
      vp.panBy(const Offset(-step, 0));
    } else if (k == LogicalKeyboardKey.arrowUp) {
      vp.panBy(const Offset(0, step));
    } else if (k == LogicalKeyboardKey.arrowDown) {
      vp.panBy(const Offset(0, -step));
    } else {
      return false;
    }
    return true;
  }

  void _down(PointerDownEvent e) {
    _pointers[e.pointer] = e.localPosition;
    if (_pointers.length == 2) {
      final p = _pointers.values.toList();
      _pinchD = (p[0] - p[1]).distance;
      _pinchS = vp.s;
      _moved = true;
    } else {
      _downAt = e.localPosition;
      _moved = false;
    }
  }

  void _move(PointerMoveEvent e) {
    final prev = _pointers[e.pointer];
    _pointers[e.pointer] = e.localPosition;
    if (_pointers.length == 2 && _pinchD != null && _pinchD! > 0) {
      final p = _pointers.values.toList();
      final target = (_pinchS * (p[0] - p[1]).distance / _pinchD!).clamp(minZoom, maxZoom);
      vp.zoomAt((p[0] + p[1]) / 2, target / vp.s);
      return;
    }
    if (_pointers.length == 1 && prev != null) {
      if (!_moved && _downAt != null && (e.localPosition - _downAt!).distance > 5) _moved = true;
      if (_moved) vp.panBy(e.localPosition - prev);
    }
  }

  void _up(PointerUpEvent e) {
    final was = _pointers.length;
    _pointers.remove(e.pointer);
    if (was == 1 && !_moved) _tap(e.localPosition);
    if (_pointers.length < 2) _pinchD = null;
  }

  void _tap(Offset p) {
    final h = vp.toHex(p);
    if (h.distFromOrigin > mapRMax) return;
    plan.why = null;
    plan.select(h);
    setState(() => _sheet = true);
    final shift = HardwareKeyboard.instance.isShiftPressed;
    if (shift && plan.planning) {
      _setDest(h, append: true);
    } else {
      _setDest(h);
    }
  }

  void _hover(PointerHoverEvent e) {
    final h = vp.toHex(e.localPosition);
    final ok = h.distFromOrigin <= mapRRange;
    plan.setHover(ok ? h : null);
    _coords = ok ? '${h.q}, ${h.r}  /  R${h.distFromOrigin.toString().padLeft(2, '0')}  /  ZOOM ${vp.s.toStringAsFixed(2)}' : '';
    if (!plan.planning && ok && !con.runner.active && plan.preview?.to != h) {
      final f = con.game.currentFleet;
      plan.preview = f.id == 0 || h == f.sector
          ? null
          : RoutePlanner.plan(
              sectors: con.state.sectors, tick: con.state.tick, fleet: f, from: f.sector, to: h, home: con.state.homeworld, power: f.power, mode: plan.mode);
    }
    _askServer(ok ? h : null);
    setState(() {});
  }

  proto.Hex? _asked;

  /// Ask the server to rate the first hop of the route on show, and the
  /// hovered sector when it is next door; the controller debounces.
  void _askServer([proto.Hex? hovered]) {
    final f = con.game.currentFleet;
    if (f.id == 0) return;
    final p = plan.planning ? plan.plan : plan.preview;
    final want = <proto.Hex>{
      if (p != null && p.path.isNotEmpty) p.path.first,
      if (hovered != null && proto.Hex.distance(hovered, f.sector) == 1 && (con.state.sectors[f.sector]?.connections.contains(hovered) ?? false)) hovered,
    };
    final old = _asked;
    if (old != null && !want.contains(old)) con.game.cancelPreview(f.id, old);
    for (final h in want) {
      con.game.requestPreview(f.id, h);
    }
    _asked = want.isEmpty ? null : want.last;
  }

  static const mapRRange = mapRMax;

  // ---- build ----

  RouteDraw? _routeDraw() {
    final r = con.runner;
    if (r.active && r.remaining.isNotEmpty) {
      final holds = <int>[];
      if (r.holdEnabled) {
        for (var i = 0; i < r.remaining.length; i++) {
          if (SectorThreat.of(con.state.sectors[r.remaining[i]]).rating >= holdThreat) holds.add(i);
        }
      }
      return RouteDraw(con.game.currentFleet.sector, r.remaining, const [], holds, active: true);
    }
    final p = plan.plan;
    if (p == null) return null;
    return RouteDraw(p.from, p.path, plan.waypoints, plan.hold ? p.holds : const []);
  }

  RouteDraw? _previewDraw() {
    final p = plan.preview;
    if (p == null || plan.planning) return null;
    return RouteDraw(p.from, p.path, const [], const []);
  }

  @override
  Widget build(BuildContext context) {
    final con = this.con;
    return LayoutBuilder(builder: (context, box) {
      final narrow = box.maxWidth < Bp.narrow;
      return ListenableBuilder(
        listenable: Listenable.merge([plan, layers]),
        builder: (context, _) {
          final lowQ = con.prefs.quality == Quality.low;
          return ClipRect(
            child: Container(
              decoration: const BoxDecoration(
                  color: Color(0xFF07050A),
                  gradient: RadialGradient(radius: .75, colors: [Color(0x0DFF961E), Color(0x00FF961E)], stops: [0, 1])),
              child: Stack(children: [
                Positioned.fill(
                  child: MouseRegion(
                    cursor: SystemMouseCursors.precise,
                    onHover: _hover,
                    onExit: (_) {
                      plan.setHover(null);
                      plan.preview = null;
                    },
                    child: Listener(
                      onPointerDown: _down,
                      onPointerMove: _move,
                      onPointerUp: _up,
                      onPointerCancel: (e) => _pointers.remove(e.pointer),
                      onPointerSignal: (s) {
                        if (s is PointerScrollEvent) {
                          vp.zoomAt(s.localPosition, math.exp(-s.scrollDelta.dy * .0015));
                        }
                      },
                      child: RepaintBoundary(
                        child: CustomPaint(
                          key: const Key('map-canvas'),
                          painter: MapPainter(
                            data: data,
                            vp: vp,
                            layers: layers,
                            plan: plan,
                            clock: clock,
                            cache: cache,
                            route: _routeDraw(),
                            preview: _previewDraw(),
                            still: still,
                            lowQuality: lowQ,
                          ),
                          size: Size.infinite,
                        ),
                      ),
                    ),
                  ),
                ),
                if (!lowQ) Positioned.fill(child: ScanOverlay(prefs: con.prefs)),
                ..._overlays(narrow, box),
              ]),
            ),
          );
        },
      );
    });
  }

  Widget _routePanel() {
    final r = con.runner;
    if (r.phase == RunPhase.held && r.heldBefore != null) {
      return HoldPanel(
        con: con,
        ahead: r.heldBefore!,
        onContinue: r.continueHold,
        onReroute: _reroute,
        onAbort: () => r.abort(),
      );
    }
    if (r.active) return ActiveRoutePanel(con: con);
    final p = plan.planning ? plan.plan : plan.preview;
    if (p == null) return const SizedBox.shrink();
    final dest = p.to;
    return RoutePanel(
      con: con,
      info: RouteInfo(p, plan.planning ? _alts : {plan.mode: p}, dest, plan.planning),
      state: plan,
      onEngage: _engage,
      onClear: _clear,
    );
  }

  bool get _hasRoutePanel {
    final r = con.runner;
    return r.active || (plan.planning ? plan.plan != null : plan.preview != null);
  }

  List<Widget> _overlays(bool narrow, BoxConstraints box) {
    final hex = plan.selected ?? plan.hover;
    final insp = Inspector(
      con: con,
      plan: plan,
      hex: hex,
      onRoute: () => hex == null ? null : _setDest(hex),
      onWaypoint: () => hex == null ? null : _setDest(hex, append: plan.planning),
    );
    final fleetChips = FleetChips(con: con, narrow: narrow);
    void home() => vp.centerOn(con.state.homeworld, scale: 1.3);
    void fleet() => vp.centerOn(_from, scale: 1.5);
    final zoom = ZoomBar(vp: vp, onHome: home, onFleet: fleet, coords: _coords, column: narrow);
    if (!narrow) {
      return [
        Positioned(left: 14, top: 12, width: math.min(470, box.maxWidth - 560), child: fleetChips),
        Positioned(
          right: 14,
          top: 12,
          child: Column(crossAxisAlignment: CrossAxisAlignment.end, children: [
            SizedBox(width: math.min(box.maxWidth - 520, 660), child: Align(alignment: Alignment.topRight, child: LayerBar(layers: layers))),
            const SizedBox(height: 8),
            SizedBox(width: 326, child: insp),
          ]),
        ),
        if (_hasRoutePanel)
          Positioned(
              right: 14,
              bottom: 14,
              width: 326,
              child: ConstrainedBox(constraints: BoxConstraints(maxHeight: math.max(160, box.maxHeight - 470)), child: SingleChildScrollView(child: _routePanel()))),
        const Positioned(left: 14, bottom: 14, width: 262, child: LegendPanel()),
        Positioned(left: 0, right: 0, bottom: 14, child: Center(child: zoom)),
      ];
    }
    final showSheet = _sheet && (hex != null || _hasRoutePanel);
    return [
      Positioned(left: 10, right: 10, top: 8, child: fleetChips),
      Positioned(left: 8, right: 8, top: 62, child: Row(children: [
        Expanded(child: LayerBar(layers: layers, scroll: true)),
        const SizedBox(width: 6),
        GestureDetector(
          key: const Key('sheet-toggle'),
          onTap: () => setState(() => _sheet = !_sheet),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
            decoration: BoxDecoration(color: const Color(0xE6080604), border: Border.all(color: _sheet ? C.a500 : C.line)),
            child: Text('PANEL', style: T.cond(size: 10.5, color: _sheet ? C.a200 : C.text3, spacing: 1.4)),
          ),
        ),
      ])),
      Positioned(right: 8, top: 104, child: zoom),
      if (showSheet)
        Positioned(
          left: 8,
          right: 8,
          bottom: 8,
          child: ConstrainedBox(
            constraints: BoxConstraints(maxHeight: box.maxHeight * .5),
            child: SingleChildScrollView(
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                if (_hasRoutePanel) _routePanel() else insp,
                if (_hasRoutePanel && hex != null) ...[const SizedBox(height: 6), insp],
              ]),
            ),
          ),
        ),
    ];
  }
}
