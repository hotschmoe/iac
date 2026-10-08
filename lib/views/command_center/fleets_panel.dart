import 'package:flutter/material.dart';

import '../../models/fleet.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';

class FleetsPanel extends StatelessWidget {
  final List<FleetState> fleets;
  const FleetsPanel({super.key, required this.fleets});

  Color _statusColor(FleetStatus status) {
    switch (status) {
      case FleetStatus.harvesting:
        return Amber.full;
      case FleetStatus.enRoute:
        return Amber.normal;
      case FleetStatus.combat:
        return Amber.danger;
      default:
        return Amber.dim;
    }
  }

  @override
  Widget build(BuildContext context) {
    return AmberPanel(
      title: 'FLEETS',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final f in fleets) ...[
            RichText(
              text: TextSpan(
                style: Amber.mono(size: 11),
                children: [
                  TextSpan(
                    text: 'Fleet ${f.name}',
                    style: Amber.mono(size: 11, color: Amber.bright),
                  ),
                  TextSpan(
                    text: '   Sector ',
                    style: Amber.mono(size: 11, color: Amber.dim),
                  ),
                  TextSpan(
                    text: '${f.sector}',
                    style: Amber.mono(size: 11, color: Amber.normal),
                  ),
                  TextSpan(text: '  '),
                  TextSpan(
                    text: f.status.label,
                    style: Amber.mono(size: 11, color: _statusColor(f.status)),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.only(left: 8, bottom: 8),
              child: Text(
                '${f.shipCount} ships | cargo ${f.cargoPercent}% | fuel ${f.fuelPercent}%',
                style: Amber.mono(size: 11, color: Amber.dim),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
