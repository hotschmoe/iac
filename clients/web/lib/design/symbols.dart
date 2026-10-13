import 'dart:math' as math;
import 'dart:ui';

import 'package:path_drawing/path_drawing.dart';

import '../protocol/protocol.dart' as proto;

/// Symbol set: one path per symbol in a +-50 box, nose up
/// (IAC.SYM in the prototype). Parsed once and cached.
class SymbolDef {
  final Path outline;
  final Path? detail;
  final String outlineSvg;
  const SymbolDef(this.outline, this.detail, this.outlineSvg);
}

SymbolDef _d(String o, [String? d]) => SymbolDef(parseSvgPathData(o), d == null ? null : parseSvgPathData(d), o);

const _smallCargoHull = 'M-12-36H12L17-24V30L11 36H-11L-17 30V-24Z';
const _largeCargoHull = 'M-16-44H16L22-30V40L14 46H-14L-22 40V-30Z';
const _recyclerHull = 'M-26-46H-16V-20L-8-12H8L16-20V-46H26V-14L20-4V40L12 46H-12L-20 40V-4L-26-14Z';
const _satelliteHull = 'M-8-10H8V10H-8ZM-46-16H-14V16H-46ZM14-16H46V16H14Z';
const _deathstarHull = 'M-44 0A44 44 0 1 0 44 0A44 44 0 1 0-44 0';

class Sym {
  Sym._();

  static final _ownSmallCargo = _d(_smallCargoHull, 'M-9-22H9V2H-9ZM-9 10H9V26H-9ZM0-36V-22');
  static final _ownLargeCargo = _d(_largeCargoHull, 'M-12-30H12V-8H-12ZM-12 0H12V30H-12ZM0-44V-30M0 30V46');
  static final _ownLightFighter = _d('M0-46L12-6L26 34L9 24L0 36L-9 24L-26 34L-12-6Z', 'M0-30V24M-12-6H12');
  static final _ownHeavyFighter =
      _d('M0-48L10-24L14 4L34 30V42L14 34L8 44H-8L-14 34L-34 42V30L-14 4L-10-24Z', 'M-10-24H10M-14 4H14M0-48V44M-4-34H4');
  static final _ownCruiser = _d(
      'M0-50L9-34L12-8L40 12V36L16 30L12 46H-12L-16 30L-40 36V12L-12-8L-9-34Z', 'M-9-34H9M-12-8H12M0-50V46M-40 12V4M40 12V4M-26 20H26');
  static final _ownBattleship = _d('M0-52L8-40L11-14L22-8L44 14V40L20 34L14 50H-14L-20 34L-44 40V14L-22-8L-11-14L-8-40Z',
      'M-11-14H11M-14 10H14M-14 30H14M0-52V50M-34 22H-26V28H-34ZM26 22H34V28H26Z');
  static final _ownRecycler = _d(_recyclerHull, 'M-14 4H14V32H-14ZM0-12V4M-14 18H14M-21-46V-34M21-46V-34');
  static final _ownProbe = _d('M0-30L12 0L0 18L-12 0Z', 'M0 18V40M-12 0H-26M12 0H26M0-30V-44');
  static final _ownBomber = _d('M0-40L10-20L48 14V30L16 22L10 44H-10L-16 22L-48 30V14L-10-20Z',
      'M0-40V44M-10-20H10M-34 16V26M34 16V26M-22 12V22M22 12V22');
  static final _ownSatellite = _d(_satelliteHull, 'M-14 0H-8M8 0H14M-30-16V16M30-16V16M0-10V-28M-6-28H6');
  static final _ownDestroyer = _d('M-6-52V-30H6V-52L14-40L18-10L36 6V34L20 30L16 50H-16L-20 30L-36 34V6L-18-10L-14-40Z',
      'M-18-10H18M-20 12H20M-16 30H16M0-30V50M-30 14V26M30 14V26');
  static final _ownDeathstar =
      _d(_deathstarHull, 'M-43 8H43M-27-18A11 11 0 1 0-5-18A11 11 0 1 0-27-18M-18-18H-14M-30 24H30');
  static final _ownBattlecruiser = _d('M0-52L7-34L10-6L28 10L32 38L14 30L10 48H-10L-14 30L-32 38L-28 10L-10-6L-7-34Z',
      'M-7-34H7M-10-6H10M-12 16H12M0-52V48M-22 18V30M22 18V30');

  static final _hostileSmallCargo = _d(_smallCargoHull, 'M-9-22H9V26H-9ZM0-36V36');
  static final _hostileLargeCargo = _d(_largeCargoHull, 'M-12-30H12V30H-12ZM0-44V46');
  static final _hostileLightFighter = _d('M0-44L16-4L30-14L22 30L0 14L-22 30L-30-14L-16-4Z', 'M0-44V14M-16-4H16');
  static final _hostileHeavyFighter =
      _d('M0-46L14-18L38-34L30 8L12 26L0 16L-12 26L-30 8L-38-34L-14-18Z', 'M0-46V16M-14-18H14M-30 8L-12 0M30 8L12 0');
  static final _hostileCruiser = _d('M0-48L10-26L36-44L44-6L28 22L12 40L0 28L-12 40L-28 22L-44-6L-36-44L-10-26Z',
      'M0-48V28M-10-26H10M-28 22L-16 6M28 22L16 6M-44-6L-28-10M44-6L28-10');
  static final _hostileBattleship = _d('M0-52L12-30L30-50L50-14L46 20L24 44L8 36L0 50L-8 36L-24 44L-46 20L-50-14L-30-50L-12-30Z',
      'M0-52V50M-12-30H12M-30-50L-24-14M30-50L24-14M-46 20L-22 8M46 20L22 8M-8 36H8');
  static final _hostileRecycler = _d(_recyclerHull, 'M-14 4H14V32H-14ZM0-12V46');
  static final _hostileProbe = _d('M0-30L14 0L0 18L-14 0Z', 'M0-44V40M-26-8L-14 0M26-8L14 0');
  static final _hostileBomber = _d('M0-40L12-16L48 6L40 30L18 20L10 44L0 34L-10 44L-18 20L-40 30L-48 6L-12-16Z',
      'M0-40V34M-12-16H12M-40 30L-24 14M40 30L24 14');
  static final _hostileSatellite = _d(_satelliteHull, 'M-14 0H-8M8 0H14M-46-16L-14 16M14-16L46 16');
  static final _hostileDestroyer = _d('M-6-52V-28H6V-52L18-34L40-44L46-4L30 24L14 42L0 30L-14 42L-30 24L-46-4L-40-44L-18-34Z',
      'M0-28V30M-18-34L-12-8M18-34L12-8M-30 24L-16 8M30 24L16 8');
  static final _hostileDeathstar =
      _d(_deathstarHull, 'M-43-8H43M5 18A11 11 0 1 0 27 18A11 11 0 1 0 5 18M-30-30L-18-18M30-30L18-18');
  static final _hostileBattlecruiser = _d('M0-52L10-30L26-46L40-12L34 20L20 40L8 32L0 48L-8 32L-20 40L-34 20L-40-12L-26-46L-10-30Z',
      'M0-52V48M-10-30H10M-26-46L-20-12M26-46L20-12M-34 20L-18 10M34 20L18 10');

  /// The glyph of a ship of the player's.
  static SymbolDef own(proto.ShipClass c) => switch (c) {
        proto.ShipClass.smallCargo => _ownSmallCargo,
        proto.ShipClass.largeCargo => _ownLargeCargo,
        proto.ShipClass.lightFighter => _ownLightFighter,
        proto.ShipClass.heavyFighter => _ownHeavyFighter,
        proto.ShipClass.cruiser => _ownCruiser,
        proto.ShipClass.battleship => _ownBattleship,
        proto.ShipClass.recycler => _ownRecycler,
        proto.ShipClass.espionageProbe => _ownProbe,
        proto.ShipClass.bomber => _ownBomber,
        proto.ShipClass.solarSatellite => _ownSatellite,
        proto.ShipClass.destroyer => _ownDestroyer,
        proto.ShipClass.deathstar => _ownDeathstar,
        proto.ShipClass.battlecruiser => _ownBattlecruiser,
      };

  /// The glyph of a hostile ship: the same class, drawn barbed.
  static SymbolDef hostile(proto.ShipClass c) => switch (c) {
        proto.ShipClass.smallCargo => _hostileSmallCargo,
        proto.ShipClass.largeCargo => _hostileLargeCargo,
        proto.ShipClass.lightFighter => _hostileLightFighter,
        proto.ShipClass.heavyFighter => _hostileHeavyFighter,
        proto.ShipClass.cruiser => _hostileCruiser,
        proto.ShipClass.battleship => _hostileBattleship,
        proto.ShipClass.recycler => _hostileRecycler,
        proto.ShipClass.espionageProbe => _hostileProbe,
        proto.ShipClass.bomber => _hostileBomber,
        proto.ShipClass.solarSatellite => _hostileSatellite,
        proto.ShipClass.destroyer => _hostileDestroyer,
        proto.ShipClass.deathstar => _hostileDeathstar,
        proto.ShipClass.battlecruiser => _hostileBattlecruiser,
      };

  static final diamond = _d('M0-34L34 0L0 34L-34 0Z');
  static final fleet = _d('M0-30L24 24L0 12L-24 24Z');
  static final salvage = _d('M-10 0A10 10 0 1 0 10 0A10 10 0 1 0-10 0M-26 0H-14M14 0H26M0-26V-14M0 14V26M-3 0H3M0-3V3');
  static final ore = _d('M-24-6L-8-26L14-22L26 0L14 24L-14 22Z', 'M-8-26L-2 0L26 0M-2 0L14 24M-24-6L-2 0');
  static final derelict = _d(
      'M-46 4L-26-14L-8-14M6-14L22-14L48-6L38 12L24 16M10 18L-28 16L-46 4', 'M-2-22L5-8L-4 4L3 24M-24-2H-14M10 0H20M26 2H34');
  static final home = _d('M-18 0A18 18 0 1 0 18 0A18 18 0 1 0-18 0M-42 6A42 15 -20 1 0 42-6A42 15 -20 1 0-42 6M0-18V18M-18 0H18');
  static final hub = _d(
      'M-10 0A10 10 0 1 0 10 0A10 10 0 1 0-10 0M-24 0A24 24 0 1 0 24 0A24 24 0 1 0-24 0M0-24V-44M0 24V44M-24 0H-44M24 0H44M-17-17L-31-31M17-17L31-31M-17 17L-31 31M17 17L31 31');
  static final outpost = _d('M-24-24H24V24H-24Z', 'M0-24V-46M0 24V46M-24 0H-46M24 0H46M-10-10H10V10H-10Z');
  static final gate = _d('M-34-14V14M34-14V14M-14-14L0 0L-14 14M4-14L18 0L4 14');
  static final unknown = _d(
      'M-26-26V-34H-18M18-34H26V-26M26 26V34H18M-18 34H-26V26M-8-10C-8-22 10-22 10-10C10-2 0-2 0 8M0 18V22');

  /// Vertices of a hull outline, for the explosion fragments.
  static List<Offset> verts(SymbolDef s) {
    final out = <Offset>[];
    for (final m in s.outline.computeMetrics()) {
      final n = math.max(3, (m.length / 18).round());
      for (var i = 0; i < n; i++) {
        out.add(m.getTangentForOffset(m.length * i / n)!.position);
      }
    }
    return out;
  }
}
