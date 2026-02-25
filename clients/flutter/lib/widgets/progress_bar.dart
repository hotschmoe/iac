import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';

class ProgressRow extends StatelessWidget {
  final double pct;
  final String? label;

  const ProgressRow({
    super.key,
    required this.pct,
    this.label = 'Progress',
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        if (label != null)
          SizedBox(
            width: 60,
            child: Text(label!, style: Amber.mono(size: 10, color: Amber.dim)),
          ),
        Expanded(child: AmberProgressBar(fraction: pct / 100)),
        const SizedBox(width: 6),
        SizedBox(
          width: 30,
          child: Text(
            '${pct.toStringAsFixed(0)}%',
            style: Amber.mono(size: 10, color: Amber.dim),
            textAlign: TextAlign.right,
          ),
        ),
      ],
    );
  }
}

class AmberProgressBar extends StatelessWidget {
  final double fraction;
  final double height;
  final Color? color;

  const AmberProgressBar({
    super.key,
    required this.fraction,
    this.height = 3,
    this.color,
  });

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: height,
      child: ClipRRect(
        borderRadius: BorderRadius.circular(1),
        child: LinearProgressIndicator(
          value: fraction.clamp(0, 1),
          backgroundColor: Amber.faint,
          valueColor: AlwaysStoppedAnimation(color ?? Amber.normal),
          minHeight: height,
        ),
      ),
    );
  }
}
