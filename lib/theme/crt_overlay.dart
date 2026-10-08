import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import 'amber_theme.dart';

/// Simulated legacy Plasma / CRT amber terminal glass.
///
/// Layers (bottom → top):
/// 1. Child UI
/// 2. Fine phosphor grain (subtle noise)
/// 3. Horizontal scanlines
/// 4. Soft amber phosphor bloom
/// 5. Edge vignette / barrel falloff
/// 6. Occasional micro-flicker
class CrtOverlay extends StatefulWidget {
  final Widget child;
  const CrtOverlay({super.key, required this.child});

  @override
  State<CrtOverlay> createState() => _CrtOverlayState();
}

class _CrtOverlayState extends State<CrtOverlay>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker;
  double _flicker = 0;
  double _time = 0;
  final _rng = math.Random();

  @override
  void initState() {
    super.initState();
    _ticker = createTicker((elapsed) {
      _time = elapsed.inMilliseconds / 1000.0;
      // Rare micro-flicker spikes (old plasma panel instability)
      if (_rng.nextDouble() < 0.012) {
        setState(() => _flicker = 0.04 + _rng.nextDouble() * 0.06);
      } else if (_flicker > 0) {
        setState(() => _flicker = math.max(0, _flicker - 0.02));
      }
    })
      ..start();
  }

  @override
  void dispose() {
    _ticker.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Stack(
      fit: StackFit.expand,
      children: [
        widget.child,
        // Phosphor grain
        const Positioned.fill(
          child: IgnorePointer(child: _PhosphorGrain()),
        ),
        // Scanlines
        const Positioned.fill(
          child: IgnorePointer(child: _Scanlines()),
        ),
        // Soft amber bloom wash
        Positioned.fill(
          child: IgnorePointer(
            child: DecoratedBox(
              decoration: BoxDecoration(
                gradient: RadialGradient(
                  center: Alignment.center,
                  radius: 1.15,
                  colors: [
                    Amber.full.withValues(alpha: 0.03 + _flicker * 0.15),
                    Colors.transparent,
                  ],
                  stops: const [0.0, 0.75],
                ),
              ),
            ),
          ),
        ),
        // Vignette / barrel falloff
        const Positioned.fill(
          child: IgnorePointer(child: _Vignette()),
        ),
        // Rolling brightness bar (very subtle CRT retrace ghost)
        Positioned.fill(
          child: IgnorePointer(
            child: CustomPaint(
              painter: _RetracePainter(time: _time, intensity: 0.025),
            ),
          ),
        ),
        // Flicker flash
        if (_flicker > 0)
          Positioned.fill(
            child: IgnorePointer(
              child: ColoredBox(
                color: Amber.full.withValues(alpha: _flicker),
              ),
            ),
          ),
      ],
    );
  }
}

class _Scanlines extends StatelessWidget {
  const _Scanlines();

  @override
  Widget build(BuildContext context) {
    return CustomPaint(painter: _ScanlinePainter());
  }
}

class _ScanlinePainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    // Dark scanline every other pixel row
    final dark = Paint()..color = const Color(0x0D000000);
    for (double y = 0; y < size.height; y += 2) {
      canvas.drawRect(Rect.fromLTWH(0, y + 1, size.width, 1), dark);
    }
    // Faint amber phosphor line glow between scanlines
    final glow = Paint()..color = const Color(0x05FFB000);
    for (double y = 0; y < size.height; y += 4) {
      canvas.drawRect(Rect.fromLTWH(0, y, size.width, 1), glow);
    }
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

class _PhosphorGrain extends StatelessWidget {
  const _PhosphorGrain();

  @override
  Widget build(BuildContext context) {
    return CustomPaint(painter: _GrainPainter());
  }
}

class _GrainPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    // Static-ish grain: deterministic pseudo-noise from coordinates
    final paint = Paint();
    const step = 3.0;
    for (double y = 0; y < size.height; y += step) {
      for (double x = 0; x < size.width; x += step) {
        final n = _hash(x, y);
        if (n > 0.92) {
          paint.color = Color.fromRGBO(255, 176, 0, 0.03 + (n - 0.92) * 0.4);
          canvas.drawRect(Rect.fromLTWH(x, y, 1.5, 1.5), paint);
        }
      }
    }
  }

  double _hash(double x, double y) {
    final v = math.sin(x * 12.9898 + y * 78.233) * 43758.5453;
    return v - v.floorToDouble();
  }

  @override
  bool shouldRepaint(covariant CustomPainter oldDelegate) => false;
}

class _Vignette extends StatelessWidget {
  const _Vignette();

  @override
  Widget build(BuildContext context) {
    return const DecoratedBox(
      decoration: BoxDecoration(
        gradient: RadialGradient(
          center: Alignment.center,
          radius: 1.05,
          colors: [
            Colors.transparent,
            Color(0x66000000),
            Color(0xCC000000),
          ],
          stops: [0.45, 0.82, 1.0],
        ),
      ),
    );
  }
}

class _RetracePainter extends CustomPainter {
  final double time;
  final double intensity;

  _RetracePainter({required this.time, required this.intensity});

  @override
  void paint(Canvas canvas, Size size) {
    // Slow vertical roll of a faint brightness band
    final period = 8.0; // seconds for full sweep
    final t = (time % period) / period;
    final y = size.height * t;
    final bandH = size.height * 0.08;

    final rect = Rect.fromLTWH(0, y - bandH / 2, size.width, bandH);
    final paint = Paint()
      ..shader = ui.Gradient.linear(
        Offset(0, rect.top),
        Offset(0, rect.bottom),
        [
          Colors.transparent,
          Amber.full.withValues(alpha: intensity),
          Colors.transparent,
        ],
        const [0.0, 0.5, 1.0],
      );
    canvas.drawRect(rect, paint);
  }

  @override
  bool shouldRepaint(covariant _RetracePainter oldDelegate) =>
      oldDelegate.time != time;
}
