import 'package:flutter/material.dart';

import '../../models/game_state.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';

class AlertsPanel extends StatelessWidget {
  final List<Alert> alerts;
  const AlertsPanel({super.key, required this.alerts});

  @override
  Widget build(BuildContext context) {
    return AmberPanel(
      title: 'ALERTS',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final alert in alerts) ...[
            Text(
              '${alert.icon} ${alert.message}',
              style: Amber.mono(size: 11, color: alert.level.color),
            ),
            if (alert.detail.isNotEmpty)
              Padding(
                padding: const EdgeInsets.only(left: 8),
                child: Text(
                  alert.detail,
                  style: Amber.mono(size: 11, color: Amber.dim),
                ),
              ),
            const SizedBox(height: 6),
          ],
        ],
      ),
    );
  }
}
