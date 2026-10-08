import 'package:flutter/material.dart';

import '../../models/homeworld.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';
import '../../widgets/progress_bar.dart';

class ShipyardPanel extends StatelessWidget {
  final List<QueueItem> queue;
  final String docked;
  const ShipyardPanel({super.key, required this.queue, required this.docked});

  @override
  Widget build(BuildContext context) {
    return AmberPanel(
      title: 'SHIPYARD',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final item in queue) _buildItem(item),
          const SizedBox(height: 6),
          RichText(
            text: TextSpan(children: [
              TextSpan(
                text: 'Docked: ',
                style: Amber.mono(size: 11, color: Amber.dim),
              ),
              TextSpan(
                text: docked,
                style: Amber.mono(size: 11, color: Amber.normal),
              ),
            ]),
          ),
        ],
      ),
    );
  }

  Widget _buildItem(QueueItem item) {
    if (!item.active) {
      return Padding(
        padding: const EdgeInsets.only(bottom: 2),
        child: Text(
          '  ${item.name}     ${item.time}',
          style: Amber.mono(size: 11, color: Amber.dim),
        ),
      );
    }
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          RichText(
            text: TextSpan(children: [
              TextSpan(
                text: '> ${item.name}',
                style: Amber.mono(size: 11, color: Amber.bright),
              ),
              TextSpan(
                text: '   ${item.time}',
                style: Amber.mono(size: 11, color: Amber.full),
              ),
            ]),
          ),
          const SizedBox(height: 4),
          ProgressRow(pct: item.pct),
        ],
      ),
    );
  }
}
