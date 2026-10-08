import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../../hex/hex_math.dart';
import '../../protocol/hex.dart';
import '../../protocol/protocol.dart' as proto;

class WindshieldPainter extends CustomPainter {
  final Hex fleetSector;

  /// Server-provided sectors; lanes come from the fleet sector's `connections`.
  final Map<Hex, proto.SectorState> sectors;

  WindshieldPainter({required this.fleetSector, required this.sectors});

  static const _amberFull = Color(0xFFFFB000);
  static const _amberNormal = Color(0x99FFB000);
  static const _amberDim = Color(0x4DFFB000);
  static const _amberFaint = Color(0x14FFB000);
  static const _danger = Color(0xFFFF6B35);

  @override
  void paint(Canvas canvas, Size size) {
    final cx = size.width / 2;
    final cy = size.height / 2;
    final fq = fleetSector.q;
    final fr = fleetSector.r;
    final nodeR = 28.0;
    final here = sectors[fleetSector];
    final armLen = math.min(size.width, size.height) * 0.3;

    _drawStarsBg(canvas, size);

    final dirAngles = [0, 60, 120, 180, 240, 300]
        .map((d) => d * math.pi / 180)
        .toList();

    for (int i = 0; i < 6; i++) {
      final target = fleetSector.neighbor(HexDirection.values[i]);
      final nq = target.q;
      final nr = target.r;
      final connected = here?.connections.contains(target) ?? false;
      final angle = dirAngles[i];
      final ex = cx + math.cos(angle) * armLen;
      final ey = cy + math.sin(angle) * armLen;

      if (connected) {
        final sec = sectors[target];
        final explored = sec != null;
        final hostileCount = sec?.hostiles?.fold<int>(0, (n, h) => n + h.shipCount) ?? 0;
        final isHostile = hostileCount > 0;

        // Connection line
        final linePaint = Paint()
          ..color = isHostile
              ? _danger.withValues(alpha: 0.25)
              : explored
                  ? _amberNormal.withValues(alpha: 0.25)
                  : _amberFaint
          ..strokeWidth = isHostile ? 2 : 1.5
          ..style = PaintingStyle.stroke;

        final path = Path()
          ..moveTo(cx, cy)
          ..lineTo(ex, ey);

        if (explored) {
          canvas.drawPath(path, linePaint);
        } else {
          _drawDashedLine(canvas, Offset(cx, cy), Offset(ex, ey), linePaint);
        }

        // Direction label
        final labelDist = armLen * 0.42;
        final lx = cx + math.cos(angle) * labelDist;
        final ly = cy + math.sin(angle) * labelDist;
        _drawText(canvas, '[${i + 1}]${HexDirection.values[i].label}', lx - 14,
            ly + 3, 9, _amberDim.withValues(alpha: 0.6));

        // Neighbor node
        final nodeFill = Paint()
          ..color = isHostile
              ? _danger.withValues(alpha: 0.08)
              : explored
                  ? _amberFull.withValues(alpha: 0.06)
                  : _amberFull.withValues(alpha: 0.02);

        final nodeStroke = Paint()
          ..color = isHostile
              ? _danger.withValues(alpha: 0.4)
              : explored
                  ? _amberFull.withValues(alpha: 0.2)
                  : _amberFull.withValues(alpha: 0.08)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1;

        canvas.drawCircle(Offset(ex, ey), nodeR, nodeFill);
        canvas.drawCircle(Offset(ex, ey), nodeR, nodeStroke);

        // Node content
        if (sec != null) {
          String symbol;
          if (isHostile) {
            symbol = 'T$hostileCount';
          } else if (sec.site != null) {
            symbol = 'D${sec.site!.tier}';
          } else if (sec.resources.metal != proto.Density.none && sec.terrain == proto.TerrainType.asteroidField) {
            symbol = '.Fe';
          } else if (sec.resources.metal != proto.Density.none && sec.terrain == proto.TerrainType.nebula) {
            symbol = '.Nb';
          } else {
            symbol = '.';
          }
          _drawText(
            canvas,
            symbol,
            ex,
            ey + 1,
            10,
            isHostile ? _danger.withValues(alpha: 0.8) : _amberFull.withValues(alpha: 0.5),
            center: true,
          );
          _drawText(canvas, '$nq,$nr', ex, ey + nodeR + 10, 8,
              _amberFull.withValues(alpha: 0.2),
              center: true);
        } else {
          _drawText(canvas, '???', ex, ey + 1, 10, _amberFull.withValues(alpha: 0.1),
              center: true);
        }
      } else {
        // Dead end marker
        final dx = cx + math.cos(angle) * (armLen * 0.25);
        final dy = cy + math.sin(angle) * (armLen * 0.25);
        _drawText(canvas, 'X', dx - 4, dy + 3, 9, _amberFull.withValues(alpha: 0.08));
      }
    }

    // Center node glow
    final grd = ui.Gradient.radial(
      Offset(cx, cy),
      nodeR * 2.5,
      [_amberFull.withValues(alpha: 0.12), _amberFull.withValues(alpha: 0)],
    );
    canvas.drawCircle(
        Offset(cx, cy), nodeR * 2.5, Paint()..shader = grd);

    // Center node
    canvas.drawCircle(
      Offset(cx, cy),
      nodeR,
      Paint()..color = _amberFull.withValues(alpha: 0.1),
    );
    canvas.drawCircle(
      Offset(cx, cy),
      nodeR,
      Paint()
        ..color = _amberFull.withValues(alpha: 0.6)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2,
    );

    // Center diamond symbol
    _drawText(canvas, '<>', cx, cy + 5, 14, _amberFull, center: true, bold: true);

    // Center coords
    _drawText(canvas, '$fq,$fr', cx, cy + nodeR + 12, 9,
        _amberFull.withValues(alpha: 0.7),
        center: true);
  }

  void _drawStarsBg(Canvas canvas, Size size) {
    drawStarField(canvas, size, 42, 60, _amberFull.withValues(alpha: 0.04));
  }

  void _drawDashedLine(
      Canvas canvas, Offset start, Offset end, Paint paint) {
    final dx = end.dx - start.dx;
    final dy = end.dy - start.dy;
    final length = math.sqrt(dx * dx + dy * dy);
    final nx = dx / length;
    final ny = dy / length;

    const dashLen = 4.0;
    const gapLen = 4.0;
    double pos = 0;
    while (pos < length) {
      final segEnd = math.min(pos + dashLen, length);
      canvas.drawLine(
        Offset(start.dx + nx * pos, start.dy + ny * pos),
        Offset(start.dx + nx * segEnd, start.dy + ny * segEnd),
        paint,
      );
      pos += dashLen + gapLen;
    }
  }

  void _drawText(Canvas canvas, String text, double x, double y, double size,
      Color color,
      {bool center = false, bool bold = false}) {
    drawMapText(canvas, text, x, y, size, color, center: center, bold: bold);
  }

  @override
  bool shouldRepaint(WindshieldPainter oldDelegate) =>
      oldDelegate.fleetSector != fleetSector || !identical(oldDelegate.sectors, sectors);
}
