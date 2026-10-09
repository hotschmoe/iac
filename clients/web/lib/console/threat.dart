import '../protocol/protocol.dart' as proto;

/// Threat model (economy spec 5.1 and 7.2). A sector's rating and power are
/// the server's `threat` (observed groups count 25 percent more when they
/// are Aggressive or Swarm); only the ship table and the ratio label are
/// still computed here. [SectorThreat.of] is the one place that reads it.
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

class SectorThreat {
  /// 0 = clear, else 1..9.
  final int rating;
  final double power;
  final int shipCount;
  final proto.NpcBehavior? behavior;
  final List<(proto.ShipClass, int)> composition;

  /// The server's rating for the sector even when no hostile is in it (the
  /// ring's group power, or the rating last seen on the chart).
  final int ringRating;
  const SectorThreat(this.rating, this.power, this.shipCount, this.behavior, this.composition, [this.ringRating = 1]);

  static const clear = SectorThreat(0, 0, 0, null, []);
  bool get hostile => rating > 0;

  static SectorThreat of(proto.SectorState? s) {
    final hs = s?.hostiles;
    if (s == null || hs == null || hs.isEmpty) {
      return s == null ? clear : SectorThreat(0, 0, 0, null, const [], s.threat.rating);
    }
    final comp = <proto.ShipClass, int>{};
    proto.NpcBehavior? beh;
    var n = 0;
    for (final h in hs) {
      beh ??= h.behavior;
      for (final sh in h.ships) {
        comp[sh.shipClass] = (comp[sh.shipClass] ?? 0) + sh.count;
        n += sh.count;
      }
    }
    return SectorThreat(s.threat.rating, s.threat.estPower, n, beh, [for (final e in comp.entries) (e.key, e.value)], s.threat.rating);
  }

  String get compLabel => composition.map((e) => '${e.$2}${shipAbbr(e.$1)}').join(' ');
}

String winLabel(double ratio) => ratio >= 3 ? 'SAFE' : ratio >= 2 ? 'FAVOURABLE' : ratio >= 1.2 ? 'RISKY' : 'DEADLY';
