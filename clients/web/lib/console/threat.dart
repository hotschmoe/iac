import '../protocol/protocol.dart' as proto;

/// Threat model (economy spec 5.1 and 7.2). A sector's rating and power are
/// the server's `threat` (observed groups count 25 percent more when they
/// are Aggressive or Swarm); only the ship table and the ratio label are
/// still computed here. [SectorThreat.of] is the one place that reads it.
/// Odds for a jump come from the server's `preview_move`, kept in
/// [GameState.previews]; [RatioInfo.of] rates only the sector a fleet is in
/// and the far hops of a route, from the server's own power figures.
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

  /// What the server's threat for the sector rests on, and its power.
  final proto.ThreatBasis? basis;
  final double estPower;
  const SectorThreat(this.rating, this.power, this.shipCount, this.behavior, this.composition, [this.ringRating = 1, this.basis, this.estPower = 0]);

  static const clear = SectorThreat(0, 0, 0, null, []);
  bool get hostile => rating > 0;

  static SectorThreat of(proto.SectorState? s) {
    final hs = s?.hostiles;
    if (s == null || hs == null || hs.isEmpty) {
      return s == null ? clear : SectorThreat(0, 0, 0, null, const [], s.threat.rating, s.threat.basis, s.threat.estPower);
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
    return SectorThreat(s.threat.rating, s.threat.estPower, n, beh, [for (final e in comp.entries) (e.key, e.value)], s.threat.rating, s.threat.basis, s.threat.estPower);
  }

  /// "OBSERVED", "TEMPLATE" or "ESTIMATE": how far to trust [ringRating].
  String get basisLabel => basis?.wire.toUpperCase() ?? '';

  String get compLabel => composition.map((e) => '${e.$2}${shipAbbr(e.$1)}').join(' ');
}
