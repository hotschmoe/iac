import '../protocol/protocol.dart' as proto;
import 'battle.dart' show shortCount;

/// Threat model (economy spec 5.1 and 7.2). A sector's rating and power are
/// the server's `threat` (observed groups count 25 percent more when they
/// are Aggressive or Swarm). The ship table is the engine's and comes with
/// the generated [proto.ShipClass]: its hull, shield, weapon, cargo and power.
/// [SectorThreat.of] is the one place that reads a threat. How a fleet fares
/// against a sector is never worked out on the client: it is the server's
/// `preview_move` and `odds` answers, read with [GameState.oddsFor].
String shipAbbr(proto.ShipClass c) => switch (c) {
      proto.ShipClass.smallCargo => 'SCG',
      proto.ShipClass.largeCargo => 'LCG',
      proto.ShipClass.lightFighter => 'LFT',
      proto.ShipClass.heavyFighter => 'HFT',
      proto.ShipClass.cruiser => 'CRU',
      proto.ShipClass.battleship => 'BSH',
      proto.ShipClass.recycler => 'REC',
      proto.ShipClass.espionageProbe => 'PRB',
      proto.ShipClass.bomber => 'BMB',
      proto.ShipClass.solarSatellite => 'SAT',
      proto.ShipClass.destroyer => 'DES',
      proto.ShipClass.deathstar => 'DTH',
      proto.ShipClass.battlecruiser => 'BCR',
    };

/// The same three letters for a unit of a fight, ship or defence: what a
/// mark on the scope is named by.
String unitAbbr(proto.ShipyardItem u) => switch (u.ship) {
      final ship? => shipAbbr(ship),
      null => switch (u.defence!) {
          proto.DefenceKind.rocketLauncher => 'RKL',
          proto.DefenceKind.lightLaser => 'LLS',
          proto.DefenceKind.heavyLaser => 'HLS',
          proto.DefenceKind.gaussCannon => 'GAU',
          proto.DefenceKind.ionCannon => 'ION',
          proto.DefenceKind.plasmaTurret => 'PLT',
          proto.DefenceKind.smallShieldDome => 'SDM',
          proto.DefenceKind.largeShieldDome => 'LDM',
        },
    };

class SectorThreat {
  /// 0 = clear, else 1..9.
  final int rating;
  final double power;

  /// The hostile groups in the sector, each by class as the server lists it.
  final List<proto.NpcFleetInfo> groups;

  /// The server's rating for the sector even when no hostile is in it (the
  /// ring's group power, or the rating last seen on the chart).
  final int ringRating;

  /// What the server's threat for the sector rests on, and its power.
  final proto.ThreatBasis? basis;
  final double estPower;
  const SectorThreat(this.rating, this.power, this.groups, [this.ringRating = 1, this.basis, this.estPower = 0]);

  static const clear = SectorThreat(0, 0, []);
  bool get hostile => rating > 0;

  static SectorThreat of(proto.SectorState? s) {
    final hs = s?.hostiles;
    if (s == null || hs == null || hs.isEmpty) {
      return s == null ? clear : SectorThreat(0, 0, const [], s.threat.rating, s.threat.basis, s.threat.estPower);
    }
    return SectorThreat(s.threat.rating, s.threat.estPower, hs, s.threat.rating, s.threat.basis, s.threat.estPower);
  }

  /// "OBSERVED", "TEMPLATE" or "ESTIMATE": how far to trust [ringRating].
  String get basisLabel => basis?.wire.toUpperCase() ?? '';

  /// "32LFT 4CRU", group after group: what a narrow card has room for.
  String get compLabel =>
      [for (final g in groups) g.ships.map((s) => '${shortCount(s.count)}${shipAbbr(s.shipClass)}').join(' ')].join(' + ');
}
