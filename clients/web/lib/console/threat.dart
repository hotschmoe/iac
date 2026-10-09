import 'dart:math' as math;

import '../protocol/protocol.dart' as proto;

/// Client-side threat model (economy spec 5.1 and 7.2). The server does not
/// send ratings or odds yet; when `preview_move` lands, [SectorThreat.of]
/// is the one place to replace with the server's numbers.
class ShipStats {
  final double hull, shield, weapon, cargo;
  const ShipStats(this.hull, this.shield, this.weapon, this.cargo);
  double get power => weapon + (hull + shield) / 10;
}

const Map<proto.ShipClass, ShipStats> shipStats = {
  proto.ShipClass.scout: ShipStats(30, 10, 5, 20),
  proto.ShipClass.corvette: ShipStats(50, 20, 15, 10),
  proto.ShipClass.frigate: ShipStats(120, 60, 30, 30),
  proto.ShipClass.cruiser: ShipStats(200, 100, 80, 50),
  proto.ShipClass.hauler: ShipStats(80, 20, 5, 200),
};

String shipAbbr(proto.ShipClass c) => switch (c) {
      proto.ShipClass.scout => 'SCT',
      proto.ShipClass.corvette => 'COR',
      proto.ShipClass.frigate => 'FRG',
      proto.ShipClass.cruiser => 'CRU',
      proto.ShipClass.hauler => 'HLR',
    };

int ratingOf(double power) => math.max(1, math.min(9, 1 + (math.log(math.max(1, power) / 4) / math.ln2).floor()));

class SectorThreat {
  /// 0 = clear, else 1..9.
  final int rating;
  final double power;
  final int shipCount;
  final proto.NpcBehavior? behavior;
  final List<(proto.ShipClass, int)> composition;
  const SectorThreat(this.rating, this.power, this.shipCount, this.behavior, this.composition);

  static const clear = SectorThreat(0, 0, 0, null, []);
  bool get hostile => rating > 0;

  static double npcPower(Iterable<proto.NpcShipInfo> ships) {
    var p = 0.0;
    for (final s in ships) {
      p += shipStats[s.shipClass]!.power * s.count;
    }
    return p;
  }

  static SectorThreat of(proto.SectorState? s) {
    final hs = s?.hostiles;
    if (hs == null || hs.isEmpty) return clear;
    final comp = <proto.ShipClass, int>{};
    proto.NpcBehavior? beh;
    var power = 0.0, n = 0;
    for (final h in hs) {
      power += npcPower(h.ships);
      beh ??= h.behavior;
      for (final sh in h.ships) {
        comp[sh.shipClass] = (comp[sh.shipClass] ?? 0) + sh.count;
        n += sh.count;
      }
    }
    final aggressive = beh == proto.NpcBehavior.aggressive || beh == proto.NpcBehavior.swarm;
    return SectorThreat(ratingOf(power * (aggressive ? 1.25 : 1)), power, n, beh, [for (final e in comp.entries) (e.key, e.value)]);
  }

  String get compLabel => composition.map((e) => '${e.$2}${shipAbbr(e.$1)}').join(' ');
}

double fleetPower(proto.FleetState f) {
  var p = 0.0;
  for (final s in f.ships) {
    p += s.weaponPower + (s.hull + s.shield) / 10;
  }
  return p;
}

String winLabel(double ratio) => ratio >= 3 ? 'SAFE' : ratio >= 2 ? 'FAVOURABLE' : ratio >= 1.2 ? 'RISKY' : 'DEADLY';
