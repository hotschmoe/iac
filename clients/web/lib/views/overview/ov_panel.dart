import 'package:flutter/material.dart';

import '../../design/panel.dart';
import '../../design/tokens.dart';

/// Set by rows that stretch their cards to equal height.
class OvFill extends InheritedWidget {
  const OvFill({super.key, required super.child});
  static bool of(BuildContext c) => c.getInheritedWidgetOfExactType<OvFill>() != null;
  @override
  bool updateShouldNotify(OvFill o) => false;
}

/// ConsolePanel variant for card rows: when the parent gives it a bounded
/// height (equal-height rows) the body fills it and [footer] sits at the bottom.
class OvPanel extends StatelessWidget {
  final String? title;
  final String? subText;
  final Widget? trailing;
  final Widget child;
  final Widget? footer;
  final Color? border;
  final Color? titleColor;
  final EdgeInsets padding;
  const OvPanel({super.key, this.title, this.subText, this.trailing, required this.child, this.footer, this.border, this.titleColor, this.padding = EdgeInsets.zero});

  @override
  Widget build(BuildContext context) {
    return CustomPaint(
      foregroundPainter: _Corners(),
      child: DecoratedBox(
        decoration: BoxDecoration(color: C.panel, border: Border.all(color: border ?? C.line)),
        child: Builder(builder: (context) {
          final bounded = OvFill.of(context);
          final body = Padding(padding: padding, child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [child]));
          final foot = footer == null ? null : Padding(padding: EdgeInsets.fromLTRB(padding.left, 8, padding.right, padding.bottom), child: footer);
          return Column(
            mainAxisSize: bounded ? MainAxisSize.max : MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (title != null) PanelHeader(title!, subText: subText, trailing: trailing, color: titleColor),
              if (bounded) Expanded(child: body) else body,
              ?foot,
            ],
          );
        }),
      ),
    );
  }
}

class _Corners extends CustomPainter {
  @override
  void paint(Canvas canvas, Size s) {
    final p = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2
      ..color = C.a500.withValues(alpha: .85);
    canvas.drawPath(Path()..moveTo(0, 9)..lineTo(0, 0)..lineTo(9, 0), p);
    canvas.drawPath(Path()..moveTo(s.width, s.height - 9)..lineTo(s.width, s.height)..lineTo(s.width - 9, s.height), p);
  }

  @override
  bool shouldRepaint(_Corners o) => false;
}
