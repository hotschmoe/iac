import 'dart:math' as math;
import 'dart:ui';

import 'package:flutter/foundation.dart';

import '../../console/route_planner.dart';
import '../../console/threat.dart';
import '../../hex/hex_math.dart' as hm;
import '../../models/fleet.dart' as ui;
import '../../protocol/protocol.dart' as proto;

const double mapU = 26;
const int mapRMax = 22;
const double minZoom = .25;
const double maxZoom = 4;

/// Pan and zoom. World units are hexToPixel pixels at size [mapU].
class MapViewport extends ChangeNotifier {
  double x = 0, y = 0, s = 1.15;
  Size size = Size.zero;

  Offset toScreen(int q, int r) {
    final p = hm.hexToPixel(q, r, mapU);
    return Offset(size.width / 2 + (p.dx - x) * s, size.height / 2 + (p.dy - y) * s);
  }

  Offset worldToScreen(Offset p) => Offset(size.width / 2 + (p.dx - x) * s, size.height / 2 + (p.dy - y) * s);

  proto.Hex toHex(Offset sp) {
    final wx = (sp.dx - size.width / 2) / s + x, wy = (sp.dy - size.height / 2) / s + y;
    final q = (2 / 3 * wx) / mapU, r = (-1 / 3 * wx + hm.sqrt3 / 3 * wy) / mapU;
    var rq = q.round(), rr = r.round();
    final rs = (-q - r).round();
    final dq = (rq - q).abs(), dr = (rr - r).abs(), ds = (rs + q + r).abs();
    if (dq > dr && dq > ds) {
      rq = -rr - rs;
    } else if (dr > ds) {
      rr = -rq - rs;
    }
    return proto.Hex(rq, rr);
  }

  void panBy(Offset screenDelta) {
    x -= screenDelta.dx / s;
    y -= screenDelta.dy / s;
    notifyListeners();
  }

  void zoomAt(Offset sp, double f) {
    final wx = (sp.dx - size.width / 2) / s + x, wy = (sp.dy - size.height / 2) / s + y;
    s = (s * f).clamp(minZoom, maxZoom);
    x = wx - (sp.dx - size.width / 2) / s;
    y = wy - (sp.dy - size.height / 2) / s;
    notifyListeners();
  }

  void setScale(double v) {
    s = v.clamp(minZoom, maxZoom);
    notifyListeners();
  }

  void centerOn(proto.Hex h, {double? scale}) {
    final p = hm.hexToPixel(h.q, h.r, mapU);
    x = p.dx;
    y = p.dy;
    if (scale != null) s = scale.clamp(minZoom, maxZoom);
    notifyListeners();
  }

  void resize(Size v) {
    if (v == size) return;
    size = v;
  }
}

class MapLayers extends ChangeNotifier {
  bool threat = false, ore = false, age = true, lanes = true, derelict = true, zones = true;
  int version = 0;

  bool get(String k) => switch (k) {
        'threat' => threat,
        'ore' => ore,
        'age' => age,
        'lanes' => lanes,
        'derelict' => derelict,
        _ => zones,
      };

  void toggle(String k) {
    switch (k) {
      case 'threat':
        threat = !threat;
      case 'ore':
        ore = !ore;
      case 'age':
        age = !age;
      case 'lanes':
        lanes = !lanes;
      case 'derelict':
        derelict = !derelict;
      default:
        zones = !zones;
    }
    version++;
    notifyListeners();
  }
}

/// What the painter needs from the 1 Hz game state.
class MapData {
  final Map<proto.Hex, proto.SectorState> sectors;
  final Map<proto.Hex, proto.SignalKind> signals;
  final List<ui.FleetState> fleets;
  final int selectedFleet;
  final proto.Hex home;
  final int tick;
  final int liveCount;
  final Map<int, MapMove> moves;
  final Map<int, proto.Hex> headings;

  MapData({
    required this.sectors,
    required this.signals,
    required this.fleets,
    required this.selectedFleet,
    required this.home,
    required this.tick,
    required this.moves,
    required this.headings,
  }) : liveCount = sectors.values.where((s) => s.live).length;

  int get staticKey => Object.hash(sectors.length, tick ~/ 15, liveCount, signals.length);
}

class MapMove {
  final proto.Hex from, to;
  final double t0;
  const MapMove(this.from, this.to, this.t0);
}

enum MapMode { idle, planning }

/// Selection, hover and the route being planned. Waypoints are client-side only.
class MapPlan extends ChangeNotifier {
  proto.Hex? selected;
  proto.Hex? hover;
  List<proto.Hex> waypoints = [];
  RouteMode mode = RouteMode.balanced;
  bool hold = true;
  RoutePlan? plan;
  RoutePlan? preview;
  String? why;

  bool get planning => waypoints.isNotEmpty;

  void select(proto.Hex? h) {
    selected = h;
    notifyListeners();
  }

  void setHover(proto.Hex? h) {
    if (h == hover) return;
    hover = h;
    notifyListeners();
  }

  void setMode(RouteMode m) {
    mode = m;
    notifyListeners();
  }

  void setHold(bool v) {
    hold = v;
    notifyListeners();
  }

  void set(List<proto.Hex> wps, RoutePlan? p, {String? reason}) {
    waypoints = wps;
    plan = p;
    why = reason;
    notifyListeners();
  }

  void clear() {
    waypoints = [];
    plan = null;
    preview = null;
    why = null;
    notifyListeners();
  }

  void touch() => notifyListeners();
}

extension FleetPower on ui.FleetState {
  double get power {
    var p = 0.0;
    for (final g in ships) {
      final c = proto.ShipClass.values.where((e) => e.label == g.shipClass).firstOrNull;
      if (c != null) p += shipStats[c]!.power * g.count;
    }
    return p;
  }
}

double dist(proto.Hex a, proto.Hex b) => math.max((a.q - b.q).abs(), math.max((a.r - b.r).abs(), (a.s - b.s).abs())).toDouble();
