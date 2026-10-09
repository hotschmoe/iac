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

class Sym {
  Sym._();

  static final Map<proto.ShipClass, SymbolDef> own = {
    proto.ShipClass.scout: _d('M0-46L12-6L26 34L9 24L0 36L-9 24L-26 34L-12-6Z', 'M0-30V24M-12-6H12'),
    proto.ShipClass.corvette:
        _d('M0-48L10-24L14 4L34 30V42L14 34L8 44H-8L-14 34L-34 42V30L-14 4L-10-24Z', 'M-10-24H10M-14 4H14M0-48V44M-4-34H4'),
    proto.ShipClass.frigate: _d(
        'M0-50L9-34L12-8L40 12V36L16 30L12 46H-12L-16 30L-40 36V12L-12-8L-9-34Z', 'M-9-34H9M-12-8H12M0-50V46M-40 12V4M40 12V4M-26 20H26'),
    proto.ShipClass.cruiser: _d('M0-52L8-40L11-14L22-8L44 14V40L20 34L14 50H-14L-20 34L-44 40V14L-22-8L-11-14L-8-40Z',
        'M-11-14H11M-14 10H14M-14 30H14M0-52V50M-34 22H-26V28H-34ZM26 22H34V28H26Z'),
    proto.ShipClass.hauler:
        _d('M-16-44H16L22-30V40L14 46H-14L-22 40V-30Z', 'M-12-30H12V-8H-12ZM-12 0H12V30H-12ZM0-44V-30M0 30V46'),
  };

  static final Map<proto.ShipClass, SymbolDef> hostile = {
    proto.ShipClass.scout: _d('M0-44L16-4L30-14L22 30L0 14L-22 30L-30-14L-16-4Z', 'M0-44V14M-16-4H16'),
    proto.ShipClass.corvette:
        _d('M0-46L14-18L38-34L30 8L12 26L0 16L-12 26L-30 8L-38-34L-14-18Z', 'M0-46V16M-14-18H14M-30 8L-12 0M30 8L12 0'),
    proto.ShipClass.frigate: _d('M0-48L10-26L36-44L44-6L28 22L12 40L0 28L-12 40L-28 22L-44-6L-36-44L-10-26Z',
        'M0-48V28M-10-26H10M-28 22L-16 6M28 22L16 6M-44-6L-28-10M44-6L28-10'),
    proto.ShipClass.cruiser: _d('M0-52L12-30L30-50L50-14L46 20L24 44L8 36L0 50L-8 36L-24 44L-46 20L-50-14L-30-50L-12-30Z',
        'M0-52V50M-12-30H12M-30-50L-24-14M30-50L24-14M-46 20L-22 8M46 20L22 8M-8 36H8'),
    proto.ShipClass.hauler: _d('M-16-44H16L22-30V40L14 46H-14L-22 40V-30Z', 'M-12-30H12V30H-12ZM0-44V46'),
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
