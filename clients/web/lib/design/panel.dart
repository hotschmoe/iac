import 'package:flutter/material.dart';

import 'tokens.dart';

/// Boxed panel with 2px amber corner marks (top-left, bottom-right).
class ConsolePanel extends StatelessWidget {
  final String? title;
  final Widget? sub;
  final String? subText;
  final Widget child;
  final bool flat;
  final Color? border;
  final Color? mark;
  final Color? fill;
  final EdgeInsets padding;
  final Color? titleColor;
  final Widget? trailing;

  const ConsolePanel({
    super.key,
    this.title,
    this.sub,
    this.subText,
    required this.child,
    this.flat = false,
    this.border,
    this.mark,
    this.fill,
    this.padding = EdgeInsets.zero,
    this.titleColor,
    this.trailing,
  });

  @override
  Widget build(BuildContext context) {
    final body = Padding(padding: padding, child: child);
    return CustomPaint(
      foregroundPainter: flat ? null : _CornerPainter(mark ?? C.a500),
      child: DecoratedBox(
        decoration: BoxDecoration(color: fill ?? C.panel, border: Border.all(color: border ?? C.line)),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (title != null) PanelHeader(title!, sub: sub, subText: subText, color: titleColor, trailing: trailing),
            body,
          ],
        ),
      ),
    );
  }
}

class _CornerPainter extends CustomPainter {
  final Color color;
  _CornerPainter(this.color);
  @override
  void paint(Canvas canvas, Size s) {
    final p = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2
      ..color = color.withValues(alpha: .85);
    canvas.drawPath(Path()..moveTo(0, 9)..lineTo(0, 0)..lineTo(9, 0), p);
    canvas.drawPath(Path()..moveTo(s.width, s.height - 9)..lineTo(s.width, s.height)..lineTo(s.width - 9, s.height), p);
  }

  @override
  bool shouldRepaint(_CornerPainter o) => o.color != color;
}

class PanelHeader extends StatelessWidget {
  final String text;
  final Widget? sub;
  final String? subText;
  final Color? color;
  final Widget? trailing;
  const PanelHeader(this.text, {super.key, this.sub, this.subText, this.color, this.trailing});

  @override
  Widget build(BuildContext context) {
    final col = color ?? C.a400;
    return Container(
      padding: const EdgeInsets.fromLTRB(11, 7, 11, 6),
      decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: C.lineLo))),
      child: Row(children: [
        Container(width: 5, height: 5, color: col),
        const SizedBox(width: 8),
        Flexible(child: Text(text.toUpperCase(), overflow: TextOverflow.ellipsis, style: T.cond(size: 10.5, color: col, spacing: 2))),
        const Spacer(),
        ?sub,
        if (subText != null)
          Flexible(
              child: Text(subText!,
                  overflow: TextOverflow.ellipsis, style: T.mono(size: 10, color: C.text3))),
        ?trailing,
      ]),
    );
  }
}

/// Caption: small caps label.
class Lbl extends StatelessWidget {
  final String text;
  final Color? color;
  final double size;
  const Lbl(this.text, {super.key, this.color, this.size = 9.5});
  @override
  Widget build(BuildContext context) => Text(text.toUpperCase(),
      style: T.cond(size: size, color: color ?? C.text3, weight: FontWeight.w500, spacing: size * .18));
}

/// Dotted-leader label/value row.
class Kv extends StatelessWidget {
  final String k;
  final String? v;
  final Widget? vw;
  final Color? vc;
  final double size;
  const Kv(this.k, {super.key, this.v, this.vw, this.vc, this.size = 11});

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(vertical: 3),
        decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: C.lineLo))),
        child: Row(crossAxisAlignment: CrossAxisAlignment.baseline, textBaseline: TextBaseline.alphabetic, children: [
          Lbl(k),
          const SizedBox(width: 8),
          Expanded(
              child: Align(
                  alignment: Alignment.centerRight,
                  child: vw ?? Text(v ?? '', textAlign: TextAlign.right, style: T.mono(size: size, color: vc ?? C.text)))),
        ]),
      );
}

class KeyCap extends StatelessWidget {
  final String label;
  final Color? color;
  final double size;
  const KeyCap(this.label, {super.key, this.color, this.size = 16});
  @override
  Widget build(BuildContext context) => Container(
        constraints: BoxConstraints(minWidth: size, minHeight: size),
        padding: const EdgeInsets.symmetric(horizontal: 3),
        alignment: Alignment.center,
        decoration: BoxDecoration(border: Border.all(color: color == null ? C.lineHi : color!.withValues(alpha: .5))),
        child: Text(label.toUpperCase(), style: T.mono(size: 9.5, color: color ?? C.a300, weight: FontWeight.w500, height: 1.1)),
      );
}

class Tag extends StatelessWidget {
  final String text;
  final Color color;
  final bool on;
  const Tag(this.text, {super.key, this.color = C.text2, this.on = false});
  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 1),
        decoration: BoxDecoration(
            border: Border.all(color: on ? C.a500 : color == C.text2 ? C.line : color.withValues(alpha: .7)),
            color: on ? C.a(C.a500, .1) : null),
        child: Text(text.toUpperCase(), style: T.cond(size: 10, color: on ? C.a200 : color, weight: FontWeight.w500, spacing: 1.2)),
      );
}

/// Flat, boxed button: 1px border, caps label, inverse video on hover or focus.
/// A disabled button must carry a [reason]; it is shown as a tooltip and read
/// by screen readers. Views also print it as a caption.
class ConsoleButton extends StatefulWidget {
  final String label;
  final String? keyHint;
  final VoidCallback? onPressed;
  final bool primary;
  final bool danger;
  final bool small;
  final bool active;
  final String? reason;
  final Widget? leading;
  final String? semanticsLabel;

  const ConsoleButton(this.label,
      {super.key,
      this.keyHint,
      this.onPressed,
      this.primary = false,
      this.danger = false,
      this.small = false,
      this.active = false,
      this.reason,
      this.leading,
      this.semanticsLabel});

  @override
  State<ConsoleButton> createState() => _ConsoleButtonState();
}

class _ConsoleButtonState extends State<ConsoleButton> {
  bool _hover = false, _focus = false, _down = false;

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onPressed != null;
    final base = widget.danger ? C.ember : C.a300;
    final inverse = enabled && (_hover || _focus || _down);
    final fill = !enabled
        ? Colors.transparent
        : inverse
            ? (widget.danger ? C.ember : _down ? C.a300 : C.a500)
            : (widget.primary || widget.active)
                ? (widget.active && !widget.primary ? C.a(C.a500, .18) : C.a500)
                : Colors.transparent;
    final onFill = enabled && (inverse || (widget.primary && !widget.active));
    final fg = !enabled ? C.text3 : onFill ? const Color(0xFF1C1000) : base;
    final borderCol = !enabled ? C.line : (inverse || widget.primary) ? (widget.danger ? C.ember : C.a500) : widget.danger ? C.ember : C.lineHi;

    Widget btn = AnimatedContainer(
      duration: const Duration(milliseconds: 90),
      padding: EdgeInsets.symmetric(horizontal: widget.small ? 8 : 12, vertical: widget.small ? 3 : 6),
      decoration: BoxDecoration(color: fill, border: Border.all(color: _focus && !inverse ? C.a100 : borderCol, width: _focus ? 1.5 : 1)),
      child: Row(mainAxisSize: MainAxisSize.min, children: [
        if (widget.leading != null) ...[widget.leading!, const SizedBox(width: 6)],
        Flexible(
            child: Text(widget.label.toUpperCase(),
                overflow: TextOverflow.ellipsis, style: T.cond(size: widget.small ? 10 : 11, color: fg, spacing: 1.5))),
        if (widget.keyHint != null) ...[const SizedBox(width: 8), KeyCap(widget.keyHint!, color: fg, size: 15)],
      ]),
    );
    if (!enabled) btn = Opacity(opacity: .5, child: btn);
    btn = FocusableActionDetector(
      enabled: enabled,
      mouseCursor: enabled ? SystemMouseCursors.click : SystemMouseCursors.basic,
      onShowHoverHighlight: (v) => setState(() => _hover = v),
      onShowFocusHighlight: (v) => setState(() => _focus = v),
      actions: {ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) {
        widget.onPressed?.call();
        return null;
      })},
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTapDown: enabled ? (_) => setState(() => _down = true) : null,
        onTapCancel: () => setState(() => _down = false),
        onTapUp: (_) => setState(() => _down = false),
        onTap: widget.onPressed,
        child: btn,
      ),
    );
    final tip = !enabled ? widget.reason : null;
    if (tip != null) btn = Tooltip(message: tip, waitDuration: const Duration(milliseconds: 300), child: btn);
    return Semantics(
      button: true,
      enabled: enabled,
      label: widget.semanticsLabel ?? widget.label,
      hint: tip,
      excludeSemantics: true,
      onTap: widget.onPressed,
      child: btn,
    );
  }
}

/// Caption printed under a disabled control, in the same voice everywhere.
class WhyText extends StatelessWidget {
  final String text;
  const WhyText(this.text, {super.key});
  @override
  Widget build(BuildContext context) => Padding(
      padding: const EdgeInsets.only(top: 3),
      child: Text(text, style: T.mono(size: 10, color: C.text3, height: 1.3)));
}
