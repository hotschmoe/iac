import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../hex/hex_math.dart';
import '../../hex/world_gen.dart';
import '../../models/fleet.dart';
import '../../models/game_state.dart';
import '../../models/hex.dart';
import '../../state/game_controller.dart';

class StarMapPainter extends CustomPainter {
  final MapZoom zoom;
  final int centerQ;
  final int centerR;
  final List<FleetState> fleets;
  final int activeFleet;
  final Hex cursorHex;
  final Hex homeworld;
  final List<Waypoint> waypoints;

  StarMapPainter({
    required this.zoom,
    required this.centerQ,
    required this.centerR,
    required this.fleets,
    required this.activeFleet,
    required this.cursorHex,
    required this.homeworld,
    required this.waypoints,
  });

  static const _amberFull = Color(0xFFFFB000);
  static const _danger = Color(0xFFFF6B35);

  @override
  void paint(Canvas canvas, Size size) {
    final hexSize = zoom.hexSize.toDouble();
    final radius = zoom.radius;
    final cx = size.width / 2;
    final cy = size.height / 2;
    final isRegion = zoom == MapZoom.region;

    _drawStarsBg(canvas, size);

    // Draw edges
    for (int dq = -radius; dq <= radius; dq++) {
      for (int dr = -radius; dr <= radius; dr++) {
        if ((dq + dr).abs() > radius) continue;
        final q = centerQ + dq;
        final r = centerR + dr;
        final pos = hexToPixel(dq, dr, hexSize);
        final px = cx + pos.dx;
        final py = cy + pos.dy;

        for (final dir in Hex.directions) {
          final nq = q + dir.q;
          final nr = r + dir.r;
          if (!getEdge(q, r, nq, nr)) continue;
          final ndq = nq - centerQ;
          final ndr = nr - centerR;
          if (ndq.abs() > radius || ndr.abs() > radius || (ndq + ndr).abs() > radius) continue;
          final npos = hexToPixel(ndq, ndr, hexSize);
          final npx = cx + npos.dx;
          final npy = cy + npos.dy;

          final sec = getSectorData(q, r);
          final nsec = getSectorData(nq, nr);
          final visible = sec.explored || nsec.explored;

          final paint = Paint()
            ..color = visible
                ? _amberFull.withValues(alpha: 0.08)
                : _amberFull.withValues(alpha: 0.025)
            ..strokeWidth = 0.8
            ..style = PaintingStyle.stroke;

          canvas.drawLine(Offset(px, py), Offset(npx, npy), paint);
        }
      }
    }

    // Draw hexes
    for (int dq = -radius; dq <= radius; dq++) {
      for (int dr = -radius; dr <= radius; dr++) {
        if ((dq + dr).abs() > radius) continue;
        final q = centerQ + dq;
        final r = centerR + dr;
        final pos = hexToPixel(dq, dr, hexSize);
        final px = cx + pos.dx;
        final py = cy + pos.dy;
        final sec = getSectorData(q, r);

        final isFleetHere = fleets.any((f) => f.sector.q == q && f.sector.r == r);
        final isHome = q == homeworld.q && r == homeworld.r;
        final isCursor = q == cursorHex.q && r == cursorHex.r;

        // Hex shape
        if (!isRegion) {
          final path = Path();
          for (int i = 0; i < 6; i++) {
            final angle = math.pi / 180 * (60.0 * i);
            final hx = px + (hexSize * 0.8) * math.cos(angle);
            final hy = py + (hexSize * 0.8) * math.sin(angle);
            if (i == 0) {
              path.moveTo(hx, hy);
            } else {
              path.lineTo(hx, hy);
            }
          }
          path.close();

          Color fillColor;
          Color strokeColor;
          double strokeWidth;

          if (isCursor) {
            fillColor = _amberFull.withValues(alpha: 0.08);
            strokeColor = _amberFull.withValues(alpha: 0.5);
            strokeWidth = 1.5;
          } else if (isFleetHere) {
            fillColor = _amberFull.withValues(alpha: 0.06);
            strokeColor = _amberFull.withValues(alpha: 0.3);
            strokeWidth = 1;
          } else if (sec.explored) {
            fillColor = sec.hasHostile
                ? _danger.withValues(alpha: 0.03)
                : _amberFull.withValues(alpha: 0.02);
            strokeColor = sec.hasHostile
                ? _danger.withValues(alpha: 0.12)
                : _amberFull.withValues(alpha: 0.06);
            strokeWidth = 0.5;
          } else {
            fillColor = _amberFull.withValues(alpha: 0.005);
            strokeColor = _amberFull.withValues(alpha: 0.03);
            strokeWidth = 0.3;
          }

          canvas.drawPath(path, Paint()..color = fillColor);
          canvas.drawPath(
            path,
            Paint()
              ..color = strokeColor
              ..style = PaintingStyle.stroke
              ..strokeWidth = strokeWidth,
          );
        }

        // Symbols
        if (isFleetHere) {
          final fi = fleets.indexWhere((f) => f.sector.q == q && f.sector.r == r);
          final isActive = fi == activeFleet;
          final sym = isActive ? '<>' : '< >';
          final fontSize = isRegion ? 10.0 : 13.0;
          _drawText(
            canvas,
            sym,
            px,
            py + (isRegion ? 4 : 5),
            fontSize,
            isActive ? _amberFull : _amberFull.withValues(alpha: 0.6),
            center: true,
            bold: true,
          );
        } else if (isHome) {
          _drawText(canvas, 'H', px, py + 4, isRegion ? 10.0 : 12.0,
              _amberFull.withValues(alpha: 0.7),
              center: true);
        } else if (sec.explored) {
          if (sec.hasHostile) {
            _drawText(
              canvas,
              isRegion ? '!' : 'T${sec.hostileCount}',
              px,
              py + 3,
              isRegion ? 8.0 : 10.0,
              _danger.withValues(alpha: 0.7),
              center: true,
            );
          } else if (sec.terrain.index > 0 && sec.resMetal.index > 0) {
            _drawText(
              canvas,
              isRegion ? '.' : sec.resMetal.label[0].toUpperCase(),
              px,
              py + 3,
              isRegion ? 6.0 : 9.0,
              _amberFull.withValues(alpha: 0.35),
              center: true,
            );
          } else {
            _drawText(canvas, '.', px, py + 2, isRegion ? 3.0 : 6.0,
                _amberFull.withValues(alpha: 0.15),
                center: true);
          }
        } else {
          if (!isRegion) {
            _drawText(canvas, '?', px, py + 3, 8, _amberFull.withValues(alpha: 0.06),
                center: true);
          }
        }

        // Cursor crosshair (only when no fleet symbol is already drawn)
        if (isCursor && !isFleetHere && !isHome && !isRegion) {
          final cPaint = Paint()
            ..color = _amberFull.withValues(alpha: 0.6)
            ..strokeWidth = 0.8
            ..style = PaintingStyle.stroke;
          const arm = 4.0;
          const gap = 2.0;
          canvas.drawLine(Offset(px - arm - gap, py), Offset(px - gap, py), cPaint);
          canvas.drawLine(Offset(px + gap, py), Offset(px + arm + gap, py), cPaint);
          canvas.drawLine(Offset(px, py - arm - gap), Offset(px, py - gap), cPaint);
          canvas.drawLine(Offset(px, py + gap), Offset(px, py + arm + gap), cPaint);
        }

        // Waypoint markers
        final wp = waypoints.where((w) => w.coord.q == q && w.coord.r == r);
        if (wp.isNotEmpty && !isFleetHere) {
          _drawText(
            canvas,
            'v',
            px,
            py - (isRegion ? 6 : hexSize * 0.6),
            isRegion ? 7.0 : 9.0,
            _amberFull.withValues(alpha: 0.5),
            center: true,
          );
        }
      }
    }
  }

  void _drawStarsBg(Canvas canvas, Size size) {
    drawStarField(canvas, size, 7, 100, _amberFull.withValues(alpha: 0.015));
  }

  void _drawText(Canvas canvas, String text, double x, double y, double size,
      Color color,
      {bool center = false, bool bold = false}) {
    drawMapText(canvas, text, x, y, size, color, center: center, bold: bold);
  }

  @override
  bool shouldRepaint(StarMapPainter oldDelegate) =>
      oldDelegate.zoom != zoom ||
      oldDelegate.centerQ != centerQ ||
      oldDelegate.centerR != centerR ||
      oldDelegate.cursorHex != cursorHex ||
      oldDelegate.activeFleet != activeFleet;
}
