import 'dart:math' as math;

import '../models/fleet.dart';
import '../protocol/protocol.dart' as proto;
import 'intel.dart';
import 'threat.dart';

enum RouteMode { fast, balanced, safe }

typedef SectorMap = Map<proto.Hex, proto.SectorState>;

/// Hold threshold: the route pauses before entering a sector of this rating or worse.
const holdThreat = 5;

class RoutePlan {
  final proto.Hex from;
  final proto.Hex to;
  final RouteMode mode;
  final List<proto.Hex> path;
  final List<int> threats;
  final List<Intel> intel;
  final int fuel;
  final int fuelHave;
  final int backFuel;
  final int etaTicks;
  final List<int> holds;
  final double fleetPower;
  final double worstPower;
  final int worstHop;

  const RoutePlan({
    required this.from,
    required this.to,
    required this.mode,
    required this.path,
    required this.threats,
    required this.intel,
    required this.fuel,
    required this.fuelHave,
    required this.backFuel,
    required this.etaTicks,
    required this.holds,
    required this.fleetPower,
    required this.worstPower,
    required this.worstHop,
  });

  int get hops => path.length;
  int get maxThreat => threats.isEmpty ? 0 : threats.reduce(math.max);
  int get hostileSectors => threats.where((t) => t > 0).length;
  int get blindSectors => intel.where((i) => !i.live && i.age != IntelAge.fresh).length;
  int get fuelLeft => fuelHave - fuel;
  bool get canMake => fuelHave >= fuel;
  bool get canReturn => fuelLeft >= backFuel;
  int get returnMargin => fuelLeft - backFuel;
  int get hopsLeftToReturn => fuel == 0 || hops == 0 ? 0 : (fuelLeft ~/ math.max(1, fuel ~/ hops));
  double get worstRatio => worstPower <= 0 ? 99 : fleetPower / worstPower;
  bool get hasHold => holds.isNotEmpty;
}

class RoutePlanner {
  RoutePlanner._();

  static bool lane(SectorMap m, proto.Hex a, proto.Hex b) =>
      (m[a]?.connections.contains(b) ?? false) || (m[b]?.connections.contains(a) ?? false);

  /// Cheapest path over known lanes; null when none.
  static RoutePlan? plan({
    required SectorMap sectors,
    required int tick,
    required FleetState fleet,
    required proto.Hex from,
    required proto.Hex to,
    required proto.Hex home,
    required double power,
    RouteMode mode = RouteMode.fast,
  }) {
    if (from == to || !sectors.containsKey(to)) return null;
    double pen(int t) => t == 0
        ? 0
        : switch (mode) {
            RouteMode.fast => 0,
            RouteMode.balanced => 1 + t * t * .22,
            RouteMode.safe => 2 + t * t * .7,
          };
    final best = <proto.Hex, double>{from: 0};
    final prev = <proto.Hex, proto.Hex>{};
    final open = <(double, proto.Hex)>[(0, from)];
    final threatCache = <proto.Hex, SectorThreat>{};
    SectorThreat thr(proto.Hex h) => threatCache.putIfAbsent(h, () => SectorThreat.of(sectors[h]));
    while (open.isNotEmpty) {
      open.sort((a, b) => a.$1.compareTo(b.$1));
      final cur = open.removeAt(0);
      if (cur.$2 == to) break;
      if (cur.$1 > (best[cur.$2] ?? 1e9)) continue;
      for (final n in cur.$2.neighbors()) {
        if (!sectors.containsKey(n) || !lane(sectors, cur.$2, n)) continue;
        final c = cur.$1 + 1 + pen(thr(n).rating);
        if (c < (best[n] ?? 1e9)) {
          best[n] = c;
          prev[n] = cur.$2;
          open.add((c, n));
        }
      }
    }
    if (!prev.containsKey(to)) return null;
    final path = <proto.Hex>[];
    for (var h = to; h != from; h = prev[h]!) {
      path.insert(0, h);
    }
    final threats = <int>[], intel = <Intel>[], holds = <int>[];
    var worstPower = 0.0, worstHop = -1;
    for (var i = 0; i < path.length; i++) {
      final s = sectors[path[i]];
      final t = thr(path[i]);
      threats.add(t.rating);
      intel.add(Intel.of(s, tick));
      if (t.rating >= holdThreat) holds.add(i);
      if (t.power > worstPower) {
        worstPower = t.power;
        worstHop = i;
      }
    }
    final per = math.max(1, fleet.jumpFuel);
    final fuel = per * path.length;
    final back = proto.Hex.distance(to, home) * per;
    return RoutePlan(
      from: from,
      to: to,
      mode: mode,
      path: path,
      threats: threats,
      intel: intel,
      fuel: fuel,
      fuelHave: fleet.fuel,
      backFuel: back,
      etaTicks: path.length * 2,
      holds: holds,
      fleetPower: power,
      worstPower: worstPower,
      worstHop: worstHop,
    );
  }

  /// The three modes at once, for the route panel tabs.
  static Map<RouteMode, RoutePlan?> all({
    required SectorMap sectors,
    required int tick,
    required FleetState fleet,
    required proto.Hex from,
    required proto.Hex to,
    required proto.Hex home,
    required double power,
  }) =>
      {
        for (final m in RouteMode.values)
          m: plan(sectors: sectors, tick: tick, fleet: fleet, from: from, to: to, home: home, power: power, mode: m)
      };

  /// Waypoint chain: plan leg by leg and join.
  static RoutePlan? chain({
    required SectorMap sectors,
    required int tick,
    required FleetState fleet,
    required proto.Hex from,
    required List<proto.Hex> stops,
    required proto.Hex home,
    required double power,
    RouteMode mode = RouteMode.fast,
  }) {
    final path = <proto.Hex>[];
    var at = from;
    for (final s in stops) {
      final leg = plan(sectors: sectors, tick: tick, fleet: fleet, from: at, to: s, home: home, power: power, mode: mode);
      if (leg == null) return null;
      path.addAll(leg.path);
      at = s;
    }
    if (path.isEmpty) return null;
    return _rebuild(sectors, tick, fleet, from, path, home, power, mode);
  }

  static RoutePlan _rebuild(SectorMap sectors, int tick, FleetState fleet, proto.Hex from, List<proto.Hex> path, proto.Hex home,
      double power, RouteMode mode) {
    final threats = <int>[], intel = <Intel>[], holds = <int>[];
    var worstPower = 0.0, worstHop = -1;
    for (var i = 0; i < path.length; i++) {
      final t = SectorThreat.of(sectors[path[i]]);
      threats.add(t.rating);
      intel.add(Intel.of(sectors[path[i]], tick));
      if (t.rating >= holdThreat) holds.add(i);
      if (t.power > worstPower) {
        worstPower = t.power;
        worstHop = i;
      }
    }
    final per = math.max(1, fleet.jumpFuel);
    return RoutePlan(
      from: from,
      to: path.last,
      mode: mode,
      path: path,
      threats: threats,
      intel: intel,
      fuel: per * path.length,
      fuelHave: fleet.fuel,
      backFuel: proto.Hex.distance(path.last, home) * per,
      etaTicks: path.length * 2,
      holds: holds,
      fleetPower: power,
      worstPower: worstPower,
      worstHop: worstHop,
    );
  }
}
