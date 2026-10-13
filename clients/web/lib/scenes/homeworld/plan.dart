import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:path_drawing/path_drawing.dart';

import '../../design/draw.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;

final _metalMineIcon = parseSvgPathData('M4 20L14 10M12 6l6 6M10 4l10 10');
final _crystalMineIcon = parseSvgPathData('M12 3L20 9L12 21L4 9ZM4 9H20M9 9L12 21M15 9L12 21');
final _synthesizerIcon = parseSvgPathData('M12 3C12 3 6 11 6 15a6 6 0 0 0 12 0C18 11 12 3 12 3Z');
final _solarPlantIcon = parseSvgPathData(
    'M8 12A4 4 0 1 0 16 12A4 4 0 1 0 8 12M12 3V6M12 18V21M3 12H6M18 12H21M5.6 5.6L7.8 7.8M16.2 16.2L18.4 18.4M5.6 18.4L7.8 16.2M16.2 7.8L18.4 5.6');
final _fusionReactorIcon = parseSvgPathData(
    'M10 12A2 2 0 1 0 14 12A2 2 0 1 0 10 12M5.6 5.6A9 4 45 1 0 18.4 18.4A9 4 45 1 0 5.6 5.6M18.4 5.6A9 4 -45 1 0 5.6 18.4A9 4 -45 1 0 18.4 5.6');
final _roboticsIcon = parseSvgPathData('M4 20H14M9 20V14L15 8L19 12M19 12L22 11M19 12L20 15M7 14H11');
final _naniteIcon =
    parseSvgPathData('M12 3L16 5.5V10.5L12 13L8 10.5V5.5ZM7 12L11 14.5V19.5L7 22L3 19.5V14.5ZM17 12L21 14.5V19.5L17 22L13 19.5V14.5Z');
final _shipyardIcon = parseSvgPathData('M3 20H21M5 20V12H19V20M9 12V8H15V12');
final _metalStorageIcon = parseSvgPathData('M4 7H20V20H4ZM4 7L12 3L20 7M8 11H16M8 15H16');
final _crystalStorageIcon = parseSvgPathData('M4 7H20V20H4ZM4 7L12 3L20 7M12 10L15 13.5L12 17L9 13.5Z');
final _deuteriumTankIcon = parseSvgPathData('M7 6A5 2 0 1 0 17 6A5 2 0 1 0 7 6M7 6V18A5 2 0 0 0 17 18V6M7 12A5 2 0 0 0 17 12');
final _researchLabIcon = parseSvgPathData('M9 3H15M10 3V9L5 20H19L14 9V3M7.5 15H16.5');

Path _buildingIcon(proto.BuildingType t) => switch (t) {
      proto.BuildingType.metalMine => _metalMineIcon,
      proto.BuildingType.crystalMine => _crystalMineIcon,
      proto.BuildingType.deuteriumSynthesizer => _synthesizerIcon,
      proto.BuildingType.solarPlant => _solarPlantIcon,
      proto.BuildingType.fusionReactor => _fusionReactorIcon,
      proto.BuildingType.roboticsFactory => _roboticsIcon,
      proto.BuildingType.naniteFactory => _naniteIcon,
      proto.BuildingType.shipyard => _shipyardIcon,
      proto.BuildingType.metalStorage => _metalStorageIcon,
      proto.BuildingType.crystalStorage => _crystalStorageIcon,
      proto.BuildingType.deuteriumTank => _deuteriumTankIcon,
      proto.BuildingType.researchLab => _researchLabIcon,
    };

/// Where a building stands on the plan, in axial coordinates around the
/// core: the mines, the power and the lab in the ring around it, the
/// factories and the yard in a column to the west, storage in one to the east.
(int, int) planSlot(proto.BuildingType t) => switch (t) {
      proto.BuildingType.metalMine => (0, -1),
      proto.BuildingType.crystalMine => (1, -1),
      proto.BuildingType.deuteriumSynthesizer => (1, 0),
      proto.BuildingType.fusionReactor => (0, 1),
      proto.BuildingType.solarPlant => (-1, 1),
      proto.BuildingType.researchLab => (-1, 0),
      proto.BuildingType.roboticsFactory => (-2, 0),
      proto.BuildingType.naniteFactory => (-2, 1),
      proto.BuildingType.shipyard => (-2, 2),
      proto.BuildingType.metalStorage => (2, -2),
      proto.BuildingType.crystalStorage => (2, -1),
      proto.BuildingType.deuteriumTank => (2, 0),
    };

Offset planCenter(int q, int r, double s) => Offset(1.5 * s * q, s * (math.sqrt(3) / 2 * q + math.sqrt(3) * r));

class PathIconPainter extends CustomPainter {
  final Path path;
  final Color color;
  final double stroke;
  PathIconPainter(this.path, this.color, {this.stroke = 1.4});
  @override
  void paint(Canvas c, Size s) {
    c.scale(s.width / 24);
    c.drawPath(
        path,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = stroke * 24 / s.width
          ..strokeJoin = StrokeJoin.miter
          ..strokeCap = StrokeCap.square
          ..color = color);
  }

  @override
  bool shouldRepaint(PathIconPainter o) => o.color != color || o.path != path;
}

Widget buildingIcon(proto.BuildingType t, double size, Color color) =>
    CustomPaint(size: Size.square(size), painter: PathIconPainter(_buildingIcon(t), color));

enum TileState { normal, can, locked, busy, selected }

/// One flat-top hex tile. [phase] animates the dashed outline of a busy tile.
class HexTilePainter extends CustomPainter {
  final TileState state;
  final int phase;
  final bool core;
  HexTilePainter(this.state, this.phase, {this.core = false});

  @override
  void paint(Canvas c, Size s) {
    final r = s.width / 2;
    final hex = Draw.hexPath(Offset(s.width / 2, s.height / 2), r - 1);
    final fill = switch (state) {
      TileState.selected => const Color(0xF53C280A),
      TileState.locked => const Color(0xE6080604),
      _ => core ? const Color(0xE6140E08) : const Color(0xF50C0906),
    };
    c.drawPath(hex, Paint()..color = fill);
    final col = switch (state) {
      TileState.selected => C.a200,
      TileState.can => C.own,
      TileState.busy => C.a500,
      TileState.locked => C.line,
      TileState.normal => core ? C.a(C.a500, .6) : C.lineHi,
    };
    final p = Draw.line(col, state == TileState.selected ? 2.2 : 1.4);
    if (state == TileState.busy) {
      Draw.dashedPath(c, hex, p, 10, 6, phase * 2.0);
    } else if (state == TileState.locked) {
      Draw.dashedPath(c, hex, p, 4, 4);
      Draw.hatch(c, hex, Offset.zero & s, C.a500, .06, gap: 7);
    } else {
      c.drawPath(hex, p);
    }
  }

  @override
  bool shouldRepaint(HexTilePainter o) => o.state != state || o.phase != phase;
}

/// Drafting grid and conduit lines behind the plan (static).
class PlanBackdropPainter extends CustomPainter {
  final double s;
  final Offset origin;
  PlanBackdropPainter(this.s, this.origin);

  @override
  void paint(Canvas c, Size size) {
    final g = Paint()
      ..color = C.a(C.a500, .045)
      ..strokeWidth = 1;
    for (var x = 0.0; x < size.width; x += 24) {
      c.drawLine(Offset(x, 0), Offset(x, size.height), g);
    }
    for (var y = 0.0; y < size.height; y += 24) {
      c.drawLine(Offset(0, y), Offset(size.width, y), g);
    }
    final cp = Draw.line(C.a500, 1, .3);
    final slots = {for (final t in proto.BuildingType.values) t: planSlot(t)};
    final centres = {for (final e in slots.entries) e.key: origin + planCenter(e.value.$1, e.value.$2, s)};
    for (final e in slots.entries) {
      final a = e.value;
      final near = (a.$1.abs() + a.$2.abs() + (a.$1 + a.$2).abs()) ~/ 2 <= 1;
      if (near) {
        Draw.dashedLine(c, origin, centres[e.key]!, cp, 2, 5);
      } else {
        var best = origin;
        var bd = double.infinity;
        for (final o in slots.entries) {
          final inner = (o.value.$1.abs() + o.value.$2.abs() + (o.value.$1 + o.value.$2).abs()) ~/ 2 <= 1;
          if (!inner) continue;
          final d = (centres[o.key]! - centres[e.key]!).distance;
          if (d < bd) {
            bd = d;
            best = centres[o.key]!;
          }
        }
        Draw.dashedLine(c, best, centres[e.key]!, cp, 2, 5);
      }
    }
  }

  @override
  bool shouldRepaint(PlanBackdropPainter o) => o.s != s || o.origin != origin;
}

/// Ship wireframe on a drafting grid with dimension ticks.
class ShipDraftPainter extends CustomPainter {
  final proto.ShipClass cls;
  final bool selected;
  ShipDraftPainter(this.cls, this.selected);

  @override
  void paint(Canvas c, Size s) {
    final g = Paint()
      ..color = C.a(C.a500, .06)
      ..strokeWidth = 1;
    for (var x = 0.0; x < s.width; x += 16) {
      c.drawLine(Offset(x, 0), Offset(x, s.height), g);
    }
    for (var y = 0.0; y < s.height; y += 16) {
      c.drawLine(Offset(0, y), Offset(s.width, y), g);
    }
    final ctr = s.center(Offset.zero);
    final ax = Draw.line(C.a500, 1, .14);
    c.drawLine(Offset(ctr.dx, 4), Offset(ctr.dx, s.height - 4), ax);
    c.drawLine(Offset(4, ctr.dy), Offset(s.width - 4, ctr.dy), ax);
    final r = math.min(s.width, s.height) * .36;
    Draw.symbol(c, Sym.own(cls), ctr.dx, ctr.dy, r, color: selected ? C.own : C.own.withValues(alpha: .85), lw: 1.3);
    final dim = Draw.line(C.text3, 1, .7);
    final y = ctr.dy + r + 8;
    c.drawLine(Offset(ctr.dx - r, y), Offset(ctr.dx + r, y), dim);
    c.drawLine(Offset(ctr.dx - r, y - 3), Offset(ctr.dx - r, y + 3), dim);
    c.drawLine(Offset(ctr.dx + r, y - 3), Offset(ctr.dx + r, y + 3), dim);
  }

  @override
  bool shouldRepaint(ShipDraftPainter o) => o.cls != cls || o.selected != selected;
}

/// Orthogonal schematic edges between tech nodes, from what is required to
/// what requires it: along the row between two nodes of one row, otherwise
/// out of the first node toward the other's row, along the gap beside that
/// row and into it. The flag draws an edge bright; the rest are faint, and
/// the bright ones are drawn over them.
class TechEdgesPainter extends CustomPainter {
  final List<(Rect, Rect, bool)> edges;
  TechEdgesPainter(this.edges);

  static const _gap = 9.0;

  Path _route(Rect a, Rect b) {
    final p = Path();
    if ((a.center.dy - b.center.dy).abs() < 1) {
      final right = b.left >= a.right;
      return p
        ..moveTo(right ? a.right : a.left, a.center.dy)
        ..lineTo(right ? b.left : b.right, b.center.dy);
    }
    final down = b.center.dy > a.center.dy;
    final along = down ? b.top - _gap : b.bottom + _gap;
    return p
      ..moveTo(a.center.dx, down ? a.bottom : a.top)
      ..lineTo(a.center.dx, along)
      ..lineTo(b.center.dx, along)
      ..lineTo(b.center.dx, down ? b.top : b.bottom);
  }

  @override
  void paint(Canvas c, Size s) {
    for (final bright in const [false, true]) {
      for (final e in edges) {
        if (e.$3 == bright) c.drawPath(_route(e.$1, e.$2), Draw.line(C.a500, bright ? 1.5 : 1, bright ? .85 : .2));
      }
    }
  }

  @override
  bool shouldRepaint(TechEdgesPainter o) => o.edges.length != edges.length || o.edges.toString() != edges.toString();
}
