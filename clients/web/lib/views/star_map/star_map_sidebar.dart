import 'package:flutter/material.dart';

import '../../hex/hex_math.dart';
import '../../models/fleet.dart';
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';

class StarMapSidebar extends StatelessWidget {
  final GameController controller;
  const StarMapSidebar({super.key, required this.controller});

  @override
  Widget build(BuildContext context) {
    final state = controller.state;
    final cursor = controller.cursorHex;
    final sec = controller.sectorAt(cursor);
    final signal = state.signals[cursor];

    return SingleChildScrollView(
      child: Column(
        children: [
          _cursorPanel(sec, signal),
          _waypointsPanel(state.waypoints),
          _fleetsPanel(state.fleets, controller.activeFleet),
        ],
      ),
    );
  }

  Widget _cursorPanel(proto.SectorState? sec, proto.SignalKind? signal) {
    final hostiles = sec?.hostiles?.fold<int>(0, (n, h) => n + h.shipCount) ?? 0;
    return AmberPanel(
      title: 'CURSOR',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          LabeledRow(label: 'Sector', value: '${controller.cursorHex}', valueColor: Amber.full),
          LabeledRow(label: 'Zone', value: controller.cursorHex.zone, valueColor: Amber.normal),
          LabeledRow(label: 'Dist', value: '${controller.cursorHex.distFromOrigin} from hub', valueColor: Amber.normal),
          const SizedBox(height: 6),
          if (sec != null) ...[
            LabeledRow(label: 'Terrain', value: sec.terrain.label, valueColor: Amber.normal),
            LabeledRow(
              label: 'Metal',
              value: sec.resources.metal.label,
              valueColor: sec.resources.metal.index >= 3 ? Amber.bright : Amber.normal,
            ),
            LabeledRow(label: 'Crystal', value: sec.resources.crystal.label, valueColor: Amber.normal),
            LabeledRow(label: 'Deut', value: sec.resources.deuterium.label, valueColor: Amber.dim),
            const SizedBox(height: 6),
            LabeledRow(
              label: 'Threat',
              value: hostiles > 0 ? '$hostiles hostile ships' : 'Clear',
              valueColor: hostiles > 0 ? Amber.danger : Amber.dim,
            ),
            LabeledRow(label: 'Exits', value: '${sec.connections.length} of 6', valueColor: Amber.normal),
            if (sec.site != null)
              LabeledRow(
                label: 'Derelict',
                value: 'tier ${sec.site!.tier} ${sec.site!.risk.label}',
                valueColor: Amber.bright,
              ),
            if (sec.salvage != null && sec.salvage!.total > 0)
              LabeledRow(label: 'Salvage', value: '${sec.salvage!.total.round()}', valueColor: Amber.normal),
          ] else ...[
            Text('UNEXPLORED', style: Amber.mono(size: 11, color: Amber.faint)),
            if (signal != null)
              Text('Contact: ${signal.label}', style: Amber.mono(size: 11, color: Amber.bright)),
            Text(
              'Fog of war -- send a\nfleet or scan (v).',
              style: Amber.mono(size: 11, color: Amber.dim),
            ),
          ],
        ],
      ),
    );
  }

  Widget _waypointsPanel(List<Waypoint> waypoints) {
    return AmberPanel(
      title: 'WAYPOINTS',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (int i = 0; i < waypoints.length; i++)
            RichText(
              text: TextSpan(
                style: Amber.mono(size: 11),
                children: [
                  TextSpan(
                    text: '${i == 0 ? '>' : ' '} ${waypoints[i].id}',
                    style: Amber.mono(
                        size: 11,
                        color: i == 0 ? Amber.bright : Amber.dim),
                  ),
                  TextSpan(
                    text: ' ${waypoints[i].coord}',
                    style: Amber.mono(size: 11, color: Amber.normal),
                  ),
                  TextSpan(
                    text: ' ${waypoints[i].note}',
                    style: Amber.mono(size: 11, color: Amber.dim),
                  ),
                ],
              ),
            ),
          const SizedBox(height: 6),
          Text(
            'Lanes shown are the ones\nthe server has revealed.',
            style: Amber.mono(size: 11, color: Amber.dim),
          ),
        ],
      ),
    );
  }

  Widget _fleetsPanel(List<FleetState> fleets, int activeIdx) {
    return AmberPanel(
      title: 'FLEET POSITIONS',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (int i = 0; i < fleets.length; i++)
            RichText(
              text: TextSpan(
                style: Amber.mono(size: 11),
                children: [
                  TextSpan(
                    text: i == activeIdx
                        ? '<> '
                        : fleets[i].status == FleetStatus.docked
                            ? 'H '
                            : '< > ',
                    style: Amber.mono(
                        size: 11,
                        color: i == activeIdx ? Amber.full : Amber.dim),
                  ),
                  TextSpan(
                    text: fleets[i].name,
                    style: Amber.mono(
                        size: 11,
                        color: i == activeIdx ? Amber.bright : Amber.dim),
                  ),
                  TextSpan(
                    text: ' ${fleets[i].sector}',
                    style: Amber.mono(size: 11, color: Amber.normal),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }

}
