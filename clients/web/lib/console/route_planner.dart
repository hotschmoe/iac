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
  /// Fuel the route burns, and what is aboard, before display rounding.
  final double fuelCost;
  final double fuelAboard;

  /// Fewest jumps from [to] to the homeworld over the lanes on the player's
  /// chart, and null when the chart joins them by none. The server counts
  /// the way home over every lane there is, charted or not, so its count is
  /// never more than this one: a return this plan calls possible is.
  final int? backHops;

  /// Fuel those jumps cost, before display rounding; null with [backHops].
  final double? backCost;

  /// Ticks the route takes flown without a pause, by the server's time for
  /// one jump of this fleet.
  final int etaTicks;
  final List<int> holds;

  const RoutePlan({
    required this.from,
    required this.to,
    required this.mode,
    required this.path,
    required this.threats,
    required this.intel,
    required this.fuelCost,
    required this.fuelAboard,
    required this.backHops,
    required this.backCost,
    required this.etaTicks,
    required this.holds,
  });

  int get hops => path.length;
  int get maxThreat => threats.isEmpty ? 0 : threats.reduce(math.max);
  int get hostileSectors => threats.where((t) => t > 0).length;
  int get blindSectors => intel.where((i) => !i.live && i.age != IntelAge.fresh).length;
  int get fuel => fuelCost.round();
  int get fuelHave => fuelAboard.round();
  int? get backFuel => backCost?.round();
  int get fuelLeft => fuelHave - fuel;
  int? get returnMargin => backCost == null ? null : (fuelAboard - fuelCost - backCost!).round();
  bool get canMake => fuelAboard >= fuelCost;
  bool get canReturn => backCost != null && fuelAboard - fuelCost >= backCost!;
  int? hopsAfter(double per) => per <= 0 ? null : math.max(0, ((fuelAboard - fuelCost) / per).floor());
  bool get hasHold => holds.isNotEmpty;

  /// The hops that hold hostiles: the ones worth asking the server's odds for.
  List<proto.Hex> get hostileHops => [for (var i = 0; i < path.length; i++) if (threats[i] > 0) path[i]];
}

class RoutePlanner {
  RoutePlanner._();

  static bool lane(SectorMap m, proto.Hex a, proto.Hex b) =>
      (m[a]?.connections.contains(b) ?? false) || (m[b]?.connections.contains(a) ?? false);

  /// Fewest jumps from [from] to [to] over known lanes; null when none joins them.
  static int? chartedHops(SectorMap sectors, proto.Hex from, proto.Hex to) {
    if (from == to) return 0;
    final seen = {from};
    var frontier = [from];
    for (var hops = 1; frontier.isNotEmpty; hops++) {
      final next = <proto.Hex>[];
      for (final at in frontier) {
        for (final n in at.neighbors()) {
          if (!sectors.containsKey(n) || !lane(sectors, at, n) || !seen.add(n)) continue;
          if (n == to) return hops;
          next.add(n);
        }
      }
      frontier = next;
    }
    return null;
  }

  /// Cheapest path over known lanes; null when none.
  static RoutePlan? plan({
    required SectorMap sectors,
    required int tick,
    required FleetState fleet,
    required proto.Hex from,
    required proto.Hex to,
    required proto.Hex home,
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
    return _rebuild(sectors, tick, fleet, from, path, home, mode);
  }

  /// The three modes at once, for the route panel tabs.
  static Map<RouteMode, RoutePlan?> all({
    required SectorMap sectors,
    required int tick,
    required FleetState fleet,
    required proto.Hex from,
    required proto.Hex to,
    required proto.Hex home,
  }) =>
      {
        for (final m in RouteMode.values)
          m: plan(sectors: sectors, tick: tick, fleet: fleet, from: from, to: to, home: home, mode: m)
      };

  /// Waypoint chain: plan leg by leg and join.
  static RoutePlan? chain({
    required SectorMap sectors,
    required int tick,
    required FleetState fleet,
    required proto.Hex from,
    required List<proto.Hex> stops,
    required proto.Hex home,
    RouteMode mode = RouteMode.fast,
  }) {
    final path = <proto.Hex>[];
    var at = from;
    for (final s in stops) {
      final leg = plan(sectors: sectors, tick: tick, fleet: fleet, from: at, to: s, home: home, mode: mode);
      if (leg == null) return null;
      path.addAll(leg.path);
      at = s;
    }
    if (path.isEmpty) return null;
    return _rebuild(sectors, tick, fleet, from, path, home, mode);
  }

  static RoutePlan _rebuild(
      SectorMap sectors, int tick, FleetState fleet, proto.Hex from, List<proto.Hex> path, proto.Hex home, RouteMode mode) {
    final threats = <int>[], intel = <Intel>[], holds = <int>[];
    for (var i = 0; i < path.length; i++) {
      final t = SectorThreat.of(sectors[path[i]]);
      threats.add(t.rating);
      intel.add(Intel.of(sectors[path[i]], tick));
      if (t.rating >= holdThreat) holds.add(i);
    }
    final per = fleet.jumpFuelExact ?? math.max(1, fleet.jumpFuel).toDouble();
    final back = chartedHops(sectors, path.last, home);
    return RoutePlan(
      from: from,
      to: path.last,
      mode: mode,
      path: path,
      threats: threats,
      intel: intel,
      fuelCost: per * path.length,
      fuelAboard: fleet.fuelExact ?? fleet.fuel.toDouble(),
      backHops: back,
      backCost: back == null ? null : back * per,
      etaTicks: path.length * fleet.jumpTicks,
      holds: holds,
    );
  }
}
