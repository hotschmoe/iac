import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';

class LabeledRow extends StatelessWidget {
  final String label;
  final String value;
  final Color valueColor;
  final int padWidth;

  const LabeledRow({
    super.key,
    required this.label,
    required this.value,
    required this.valueColor,
    this.padWidth = 9,
  });

  @override
  Widget build(BuildContext context) {
    return RichText(
      text: TextSpan(children: [
        TextSpan(
          text: label.padRight(padWidth),
          style: Amber.mono(size: 11, color: Amber.dim),
        ),
        TextSpan(
          text: value,
          style: Amber.mono(size: 11, color: valueColor),
        ),
      ]),
    );
  }
}

class AmberPanel extends StatelessWidget {
  final String title;
  final Widget child;

  const AmberPanel({
    super.key,
    required this.title,
    required this.child,
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
