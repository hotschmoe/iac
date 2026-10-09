import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:path_drawing/path_drawing.dart';

/// 24x24 line glyphs for rail, HUD and action dock.
class Icons24 {
  Icons24._();

  static Path _rot(Path p, double a, double cx, double cy) =>
      p.transform((Matrix4.identity()
            ..translateByDouble(cx, cy, 0, 1)
            ..rotateZ(a)
            ..translateByDouble(-cx, -cy, 0, 1))
          .storage);

  static final Map<String, Path> _m = {
    'overview': parseSvgPathData('M3 3h8v8H3zM13 3h8v5h-8zM13 10h8v11h-8zM3 13h8v8H3z'),
    'windshield': parseSvgPathData('M3 18L7 6H17L21 18ZM12 6V18M5 12H19M10 12H14'),
    'map': parseSvgPathData(
        'M12 2l4 2.3v4.6L12 11.2 8 8.9V4.3zM7 12.2l4 2.3v4.6L7 21.4 3 19.1v-4.6zM17 12.2l4 2.3v4.6L17 21.4l-4-2.3v-4.6z'),
    'homeworld': Path()
      ..addOval(Rect.fromCircle(center: const Offset(12, 12), radius: 6))
      ..addPath(_rot(Path()..addOval(Rect.fromCenter(center: const Offset(12, 12), width: 20, height: 6.8)), -math.pi / 9, 12, 12), Offset.zero)
      ..addPath(parseSvgPathData('M12 6V18M6 12H18'), Offset.zero),
    'fleets': parseSvgPathData('M12 3L18 12H6zM12 11L19 21H5z'),
    'comms': parseSvgPathData('M4 5H20V16H11L6 20V16H4zM8 9H16M8 12H13'),
    'rank': parseSvgPathData('M5 20V12M12 20V5M19 20V15M3 20H21'),
    'bell': parseSvgPathData('M6 17V11a6 6 0 0 1 12 0v6l2 2H4zM10 21H14'),
    'sound': parseSvgPathData('M4 9v6h4l5 4V5L8 9zM16 9a4 4 0 0 1 0 6'),
    'soundoff': parseSvgPathData('M4 9v6h4l5 4V5L8 9zM16 9l5 6M21 9l-5 6'),
    'help': parseSvgPathData('M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.7.4-1 .9-1 1.7M12 17h.01'),
    'scan': parseSvgPathData('M9 12a3 3 0 1 0 6 0a3 3 0 1 0-6 0M12 2v5M12 17v5M2 12h5M17 12h5'),
    'harvest': parseSvgPathData('M4 20L14 10M12 6l6 6M10 4l10 10'),
    'fire': parseSvgPathData('M12 3v6M12 15v6M3 12h6M15 12h6M10 10h4v4h-4z'),
    'collect': parseSvgPathData('M5 8h14l-1.5 11h-11zM9 8V5h6v3'),
    'board': parseSvgPathData('M3 12h13M12 7l5 5-5 5M20 4v16'),
    'recall': parseSvgPathData('M4 12a8 8 0 1 0 3-6.2M4 4v4h4'),
    'stop': parseSvgPathData('M6 6h12v12H6z'),
    'palette': parseSvgPathData('M4 7l5 5-5 5M12 18h8'),
  };

  static Path of(String name) => _m[name]!;
}

class LineIcon extends StatelessWidget {
  final String name;
  final double size;
  final Color color;
  final double stroke;
  const LineIcon(this.name, {super.key, this.size = 20, required this.color, this.stroke = 1.4});

  @override
  Widget build(BuildContext context) =>
      CustomPaint(size: Size(size, size), painter: _IconPainter(Icons24.of(name), color, stroke));
}

class _IconPainter extends CustomPainter {
  final Path path;
  final Color color;
  final double stroke;
  _IconPainter(this.path, this.color, this.stroke);

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 24);
    canvas.drawPath(
        path,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = stroke * 24 / size.width
          ..strokeCap = StrokeCap.square
          ..strokeJoin = StrokeJoin.miter
          ..color = color);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_IconPainter o) => o.color != color || o.path != path || o.stroke != stroke;
}
