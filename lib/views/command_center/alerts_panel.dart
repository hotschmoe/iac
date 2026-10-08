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
            if (alert.level == AlertLevel.glow)
              _PulsingAlert(alert: alert)
            else
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

class _PulsingAlert extends StatefulWidget {
  final Alert alert;
  const _PulsingAlert({required this.alert});

  @override
  State<_PulsingAlert> createState() => _PulsingAlertState();
}

class _PulsingAlertState extends State<_PulsingAlert>
    with SingleTickerProviderStateMixin {
  late final AnimationController _ctrl;

  @override
  void initState() {
    super.initState();
    _ctrl = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1800),
    )..repeat(reverse: true);
  }

  @override
  void dispose() {
    _ctrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _ctrl,
      builder: (context, child) {
        final t = _ctrl.value;
        final alpha = 0.6 + 0.4 * t;
        final color = Amber.full.withValues(alpha: alpha);
        final glowRadius = 2.0 + 4.0 * t;
        return Container(
          decoration: BoxDecoration(
            boxShadow: [
              BoxShadow(
                color: Amber.full.withValues(alpha: 0.15 * t),
                blurRadius: glowRadius,
              ),
            ],
          ),
          child: Text(
            '${widget.alert.icon} ${widget.alert.message}',
            style: Amber.mono(size: 11, color: color),
          ),
        );
      },
    );
  }
}
