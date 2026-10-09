import 'package:flutter/material.dart';

import '../console/services.dart';
import '../design/icons.dart';
import '../design/tokens.dart';

const _items = [
  (Screen.overview, 'overview', 'Overview', 'O'),
  (Screen.windshield, 'windshield', 'Windshield', 'W'),
  (Screen.map, 'map', 'Map', 'M'),
  (Screen.homeworld, 'homeworld', 'Homeworld', 'H'),
];

class Rail extends StatelessWidget {
  final Screen active;
  final bool bottom;
  final ValueChanged<Screen> onGo;
  final Map<Screen, int> badges;
  const Rail({super.key, required this.active, required this.bottom, required this.onGo, this.badges = const {}});

  @override
  Widget build(BuildContext context) {
    final tiles = [
      for (final it in _items)
        _RailTile(
          icon: it.$2,
          label: it.$3,
          hint: it.$4,
          on: active == it.$1,
          bottom: bottom,
          badge: badges[it.$1] ?? 0,
          onTap: () => onGo(it.$1),
        ),
    ];
    return Container(
      width: bottom ? null : 78,
      height: bottom ? 54 : null,
      decoration: BoxDecoration(
          color: C.rail,
          border: bottom ? const Border(top: BorderSide(color: C.lineHi)) : const Border(right: BorderSide(color: C.line))),
      child: bottom
          ? Row(children: [for (final t in tiles) Expanded(child: t)])
          : Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [for (final t in tiles) t, const Spacer()]),
    );
  }
}

class _RailTile extends StatefulWidget {
  final String icon, label, hint;
  final bool on, bottom;
  final int badge;
  final VoidCallback onTap;
  const _RailTile({required this.icon, required this.label, required this.hint, required this.on, required this.bottom, required this.badge, required this.onTap});
  @override
  State<_RailTile> createState() => _RailTileState();
}

class _RailTileState extends State<_RailTile> {
  bool _hover = false, _focus = false;
  @override
  Widget build(BuildContext context) {
    final on = widget.on;
    final col = on ? C.a100 : (_hover || _focus) ? C.a300 : C.text3;
    return Semantics(
      button: true,
      selected: on,
      label: '${widget.label} (G then ${widget.hint})',
      excludeSemantics: true,
      onTap: widget.onTap,
      child: FocusableActionDetector(
        mouseCursor: SystemMouseCursors.click,
        onShowHoverHighlight: (v) => setState(() => _hover = v),
        onShowFocusHighlight: (v) => setState(() => _focus = v),
        actions: {ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) {
          widget.onTap();
          return null;
        })},
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: widget.onTap,
          child: Container(
            padding: EdgeInsets.only(top: widget.bottom ? 8 : 10, bottom: widget.bottom ? 4 : 8),
            decoration: BoxDecoration(
              color: on ? C.a(C.a500, .10) : (_hover ? C.a(C.a500, .05) : null),
              border: Border(
                bottom: widget.bottom ? BorderSide.none : const BorderSide(color: C.lineLo),
                right: widget.bottom ? const BorderSide(color: C.lineLo) : BorderSide.none,
                left: !widget.bottom && on ? const BorderSide(color: C.a500, width: 3) : BorderSide.none,
                top: widget.bottom && on ? const BorderSide(color: C.a500, width: 3) : BorderSide.none,
              ),
            ),
            child: Stack(clipBehavior: Clip.none, alignment: Alignment.center, children: [
              Column(mainAxisSize: MainAxisSize.min, children: [
                LineIcon(widget.icon, size: widget.bottom ? 20 : 22, color: col, stroke: 1.3),
                const SizedBox(height: 4),
                Text(widget.label.toUpperCase(), style: T.cond(size: widget.bottom ? 8.5 : 9, color: col, weight: FontWeight.w500, spacing: widget.bottom ? .6 : 1.3)),
              ]),
              if (!widget.bottom)
                Positioned(top: -6, right: 4, child: Text(widget.hint, style: T.mono(size: 8, color: C.text3))),
              if (widget.badge > 0)
                Positioned(
                  top: -6,
                  left: widget.bottom ? null : 44,
                  right: widget.bottom ? 18 : null,
                  child: Container(
                    constraints: const BoxConstraints(minWidth: 14, minHeight: 14),
                    alignment: Alignment.center,
                    padding: const EdgeInsets.symmetric(horizontal: 3),
                    color: C.ember,
                    child: Text('${widget.badge}', style: T.mono(size: 9, color: const Color(0xFF1A0602), weight: FontWeight.w600)),
                  ),
                ),
            ]),
          ),
        ),
      ),
    );
  }
}
