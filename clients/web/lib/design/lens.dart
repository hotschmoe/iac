import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';

import '../console/prefs.dart';
import 'beat.dart';

/// Scanlines, vignette and a slow beam band. Static layers are painted once
/// (the painter never repaints unless resized); the band is a translated
/// child, so no per-frame painting.
class ScanOverlay extends StatefulWidget {
  final ConsolePrefs prefs;
  const ScanOverlay({super.key, required this.prefs});
  @override
  State<ScanOverlay> createState() => _ScanOverlayState();
}

class _ScanOverlayState extends State<ScanOverlay> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(vsync: this, duration: const Duration(seconds: 9))..repeat();

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.prefs,
      builder: (context, _) {
        if (!widget.prefs.scanlines || widget.prefs.quality == Quality.low) return const SizedBox.shrink();
        final still = reducedMotion(context) || widget.prefs.reduceMotion;
        return IgnorePointer(
          child: Stack(fit: StackFit.expand, children: [
            RepaintBoundary(child: CustomPaint(painter: _ScanPainter())),
            if (!still)
              LayoutBuilder(
                builder: (context, c) => AnimatedBuilder(
                  animation: _c,
                  builder: (_, _) => Transform.translate(
                    offset: Offset(0, -90 + (c.maxHeight + 180) * _c.value),
                    child: Align(
                      alignment: Alignment.topCenter,
                      child: Container(
                        height: 90,
                        decoration: const BoxDecoration(
                            gradient: LinearGradient(
                                begin: Alignment.topCenter,
                                end: Alignment.bottomCenter,
                                colors: [Color(0x00FFB000), Color(0x09FFB000), Color(0x00FFB000)])),
                      ),
                    ),
                  ),
                ),
              ),
          ]),
        );
      },
    );
  }
}

class _ScanPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final p = Path();
    for (var y = 0.0; y < size.height; y += 3) {
      p.addRect(Rect.fromLTWH(0, y, size.width, 1));
    }
    canvas.drawPath(p, Paint()..color = const Color(0x29000000));
    final r = Rect.fromLTWH(0, 0, size.width, size.height);
    canvas.drawRect(
        r,
        Paint()
          ..shader = ui.Gradient.radial(r.center, math.max(size.width, size.height) * .72, const [Color(0x00000000), Color(0x80000000)], const [.6, 1]));
  }

  @override
  bool shouldRepaint(_ScanPainter o) => false;
}

/// Watches frame times and steps the render quality down or up.
/// Keeps a rolling window for the stats the settings panel shows.
class FrameGovernor {
  FrameGovernor(this.prefs) {
    SchedulerBinding.instance.addTimingsCallback(_onTimings);
  }

  final ConsolePrefs prefs;
  final List<double> _window = [];
  int _since = 0;
  int _calm = 0;

  double avgMs = 0, p95Ms = 0, worstMs = 0;
  int frames = 0;

  void _onTimings(List<ui.FrameTiming> ts) {
    for (final t in ts) {
      final ms = t.totalSpan.inMicroseconds / 1000;
      _window.add(ms);
      frames++;
      if (_window.length > 240) _window.removeAt(0);
    }
    _since += ts.length;
    if (_since < 60 || _window.length < 60) return;
    _since = 0;
    final sorted = List.of(_window)..sort();
    avgMs = _window.reduce((a, b) => a + b) / _window.length;
    p95Ms = sorted[(sorted.length * .95).floor().clamp(0, sorted.length - 1)];
    worstMs = sorted.last;
    final q = prefs.governed;
    if (p95Ms > 24 && q != Quality.low) {
      prefs.govern(Quality.values[q.index - 1]);
      _calm = 0;
    } else if (p95Ms < 11 && q != Quality.high) {
      if (++_calm >= 5) {
        prefs.govern(Quality.values[q.index + 1]);
        _calm = 0;
      }
    } else {
      _calm = 0;
    }
  }

  void dispose() => SchedulerBinding.instance.removeTimingsCallback(_onTimings);

  String get summary => frames < 60 ? 'FRAME --' : 'FRAME ${avgMs.toStringAsFixed(1)} AVG  ${p95Ms.toStringAsFixed(1)} P95 MS';
}
