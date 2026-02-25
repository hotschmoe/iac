import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';

class AmberPanel extends StatelessWidget {
  final String title;
  final Widget child;
  final bool wide;

  const AmberPanel({
    super.key,
    required this.title,
    required this.child,
    this.wide = false,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: Amber.bgPanel,
        border: Border.all(color: Amber.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
            decoration: const BoxDecoration(
              border: Border(bottom: BorderSide(color: Amber.border)),
            ),
            child: Text(
              title,
              style: Amber.mono(size: 9, color: Amber.dim).copyWith(
                letterSpacing: 2,
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(10),
            child: child,
          ),
        ],
      ),
    );
  }
}
