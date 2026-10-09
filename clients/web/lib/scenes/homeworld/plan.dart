import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:path_drawing/path_drawing.dart';

import '../../design/draw.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;

final Map<proto.BuildingType, Path> _buildingIcons = {
  proto.BuildingType.metalMine: parseSvgPathData('M4 20L14 10M12 6l6 6M10 4l10 10'),
  proto.BuildingType.crystalMine: parseSvgPathData('M12 3L20 9L12 21L4 9ZM4 9H20M9 9L12 21M15 9L12 21'),
  proto.BuildingType.deuteriumSynthesizer: parseSvgPathData('M12 3C12 3 6 11 6 15a6 6 0 0 0 12 0C18 11 12 3 12 3Z'),
  proto.BuildingType.shipyard: parseSvgPathData('M3 20H21M5 20V12H19V20M9 12V8H15V12'),
  proto.BuildingType.researchLab: parseSvgPathData('M9 3H15M10 3V9L5 20H19L14 9V3M7.5 15H16.5'),
  proto.BuildingType.fuelDepot: parseSvgPathData('M8 4H16V20H8ZM8 9H16M16 7L19 9V15'),
  proto.BuildingType.sensorArray: parseSvgPathData('M4 14A8 8 0 0 0 20 14ZM12 14V20M8 20H16M12 6V3'),
  proto.BuildingType.defenseGrid: parseSvgPathData('M12 3L20 6V12C20 17 12 21 12 21C12 21 4 17 4 12V6ZM12 8V14M9 11H15'),
  proto.BuildingType.storageVault: parseSvgPathData('M4 7H20V20H4ZM4 7L12 3L20 7M8 11H16M8 15H16'),
  proto.BuildingType.fabricator: parseSvgPathData('M4 20H20M6 20V10L12 6L18 10V20M10 20V14H14V20M3 10H21'),
};

/// Axial plan positions: core at the centre, six around it, two outer.
const Map<proto.BuildingType, (int, int)> planSlots = {
  proto.BuildingType.metalMine: (0, -1),
  proto.BuildingType.crystalMine: (1, -1),
  proto.BuildingType.deuteriumSynthesizer: (1, 0),
  proto.BuildingType.shipyard: (0, 1),
  proto.BuildingType.researchLab: (-1, 1),
  proto.BuildingType.fuelDepot: (-1, 0),
  proto.BuildingType.sensorArray: (2, -1),
  proto.BuildingType.defenseGrid: (-2, 1),
  proto.BuildingType.storageVault: (2, 0),
  proto.BuildingType.fabricator: (-2, 0),
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
    CustomPaint(size: Size.square(size), painter: PathIconPainter(_buildingIcons[t]!, color));

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
    final centres = {for (final e in planSlots.entries) e.key: origin + planCenter(e.value.$1, e.value.$2, s)};
    for (final e in planSlots.entries) {
      final a = e.value;
      final near = (a.$1.abs() + a.$2.abs() + (a.$1 + a.$2).abs()) ~/ 2 <= 1;
      if (near) {
        Draw.dashedLine(c, origin, centres[e.key]!, cp, 2, 5);
      } else {
        proto.BuildingType? best;
        var bd = 1e9;
        for (final o in planSlots.entries) {
          if (o.key == e.key) continue;
          final d = (centres[o.key]! - centres[e.key]!).distance;
          if (d < bd) {
            bd = d;
            best = o.key;
          }
        }
        Draw.dashedLine(c, centres[best]!, centres[e.key]!, cp, 2, 5);
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
    Draw.symbol(c, Sym.own[cls]!, ctr.dx, ctr.dy, r, color: selected ? C.own : C.own.withValues(alpha: .85), lw: 1.3);
    final dim = Draw.line(C.text3, 1, .7);
    final y = ctr.dy + r + 8;
    c.drawLine(Offset(ctr.dx - r, y), Offset(ctr.dx + r, y), dim);
    c.drawLine(Offset(ctr.dx - r, y - 3), Offset(ctr.dx - r, y + 3), dim);
    c.drawLine(Offset(ctr.dx + r, y - 3), Offset(ctr.dx + r, y + 3), dim);
  }

  @override
  bool shouldRepaint(ShipDraftPainter o) => o.cls != cls || o.selected != selected;
}

/// Orthogonal schematic edges between tech nodes.
class TechEdgesPainter extends CustomPainter {
  final List<(Rect, Rect, bool)> edges;
  TechEdgesPainter(this.edges);

  @override
  void paint(Canvas c, Size s) {
    for (final e in edges) {
      final a = e.$1, b = e.$2;
      final p = Path();
      if (b.left > a.right) {
        final mx = (a.right + b.left) / 2;
        p
          ..moveTo(a.right, a.center.dy)
          ..lineTo(mx, a.center.dy)
          ..lineTo(mx, b.center.dy)
          ..lineTo(b.left, b.center.dy);
      } else {
        final my = (a.bottom + b.top) / 2;
        p
          ..moveTo(a.center.dx, a.bottom)
          ..lineTo(a.center.dx, my)
          ..lineTo(b.center.dx, my)
          ..lineTo(b.center.dx, b.top);
      }
      final paint = Draw.line(C.a500, 1, e.$3 ? .55 : .2);
      if (e.$3) {
        c.drawPath(p, paint);
      } else {
        Draw.dashedPath(c, p, paint, 3, 4);
      }
    }
  }

  @override
  bool shouldRepaint(TechEdgesPainter o) => o.edges.length != edges.length || o.edges.toString() != edges.toString();
}
