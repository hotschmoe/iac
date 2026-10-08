import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';

class ResourceBar extends StatelessWidget {
  final String label;
  final int value;
  final double rate;

  const ResourceBar({
    super.key,
    required this.label,
    required this.value,
    required this.rate,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          SizedBox(
            width: 52,
            child: Text(label, style: Amber.mono(size: 11, color: Amber.dim)),
          ),
          SizedBox(
            width: 60,
            child: Text(
              _formatNumber(value),
              style: Amber.mono(size: 11, color: Amber.full),
              textAlign: TextAlign.right,
            ),
          ),
          const SizedBox(width: 8),
          SizedBox(
            width: 64,
            child: Text(
              '+${rate.toStringAsFixed(1)}/t',
              style: Amber.mono(size: 10, color: Amber.dim),
            ),
          ),
        ],
      ),
    );
  }

  String _formatNumber(int n) {
    if (n < 1000) return n.toString();
    if (n < 1000000) {
      return '${(n / 1000).toStringAsFixed(n % 1000 == 0 ? 0 : 1)}k';
    }
    return '${(n / 1000000).toStringAsFixed(1)}M';
  }
}
