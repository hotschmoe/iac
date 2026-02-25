import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';

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
