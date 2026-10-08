import 'package:flutter/material.dart';

import '../../protocol/protocol.dart' as proto;
import '../../theme/amber_theme.dart';

/// One building, tech or ship class on the homeworld grid. Plain data in, so
/// the same card serves all three tabs.
class HomeworldCard extends StatelessWidget {
  final String title;

  /// Right-aligned status, e.g. "Lv 4" or "MAX".
  final String badge;

  /// What the card does when queued, e.g. "to Lv 5" or "per ship".
  final String? subtitle;

  /// Null when there is nothing left to buy (maxed).
  final proto.Resources? cost;
  final String? time;
  final List<proto.Requirement> requires;
  final proto.Resources stock;
  final bool selected;
  final bool maxed;
  final VoidCallback onTap;

  const HomeworldCard({
    super.key,
    required this.title,
    required this.badge,
    this.subtitle,
    required this.cost,
    this.time,
    required this.requires,
    required this.stock,
    required this.selected,
    required this.maxed,
    required this.onTap,
  });

  bool get locked => requires.any((r) => !r.met);

  @override
  Widget build(BuildContext context) {
    final titleColor = locked ? Amber.dim : Amber.full;
    return GestureDetector(
      onTap: onTap,
      behavior: HitTestBehavior.opaque,
      child: Container(
        padding: const EdgeInsets.all(10),
        decoration: BoxDecoration(
          color: selected ? const Color(0xFF14100A) : Amber.bgPanel,
          border: Border.all(
            color: selected ? Amber.full : (locked ? Amber.faint : Amber.dim),
            width: selected ? 1.5 : 1,
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    title.toUpperCase(),
                    overflow: TextOverflow.ellipsis,
                    style: Amber.mono(size: 11, color: titleColor, weight: FontWeight.w700)
                        .copyWith(letterSpacing: 1),
                  ),
                ),
                Text(badge, style: Amber.mono(size: 11, color: maxed ? Amber.bright : Amber.normal)),
              ],
            ),
            if (subtitle != null)
              Text(subtitle!, style: Amber.mono(size: 10, color: Amber.dim)),
            const SizedBox(height: 6),
            if (cost != null) _costRow(cost!) else Text('--', style: Amber.mono(size: 11, color: Amber.dim)),
            if (time != null) Text(time!, style: Amber.mono(size: 10, color: Amber.dim)),
            if (requires.isNotEmpty) const SizedBox(height: 6),
            for (final r in requires) _requirement(r),
          ],
        ),
      ),
    );
  }

  /// Components the stockpile cannot cover are red; the rest stay amber.
  Widget _costRow(proto.Resources c) {
    TextSpan part(String tag, double need, double have) => TextSpan(
          text: '$tag ${need.round()}  ',
          style: Amber.mono(
            size: 11,
            color: locked
                ? Amber.dim
                : (have >= need ? Amber.bright : Amber.danger),
          ),
        );
    return RichText(
      text: TextSpan(children: [
        part('Fe', c.metal, stock.metal),
        part('Cr', c.crystal, stock.crystal),
        if (c.deuterium > 0) part('De', c.deuterium, stock.deuterium),
      ]),
    );
  }

  Widget _requirement(proto.Requirement r) => Text(
        '${r.met ? '[OK]' : '[--]'} ${r.label}',
        style: Amber.mono(size: 10, color: r.met ? Amber.dim : Amber.danger),
      );
}
