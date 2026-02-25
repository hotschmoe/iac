import 'package:flutter/material.dart';

import '../../hex/world_gen.dart';
import '../../models/fleet.dart';
import '../../models/game_state.dart';
import '../../models/sector.dart';
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
    final sec = getSectorData(cursor.q, cursor.r);

    return SingleChildScrollView(
      child: Column(
        children: [
          _cursorPanel(sec),
          _waypointsPanel(state.waypoints),
          _fleetsPanel(state.fleets, controller.activeFleet),
        ],
      ),
    );
  }

  Widget _cursorPanel(SectorState sec) {
    return AmberPanel(
      title: 'CURSOR',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          LabeledRow(label: 'Sector', value: '${controller.cursorHex}', valueColor: Amber.full),
          LabeledRow(label: 'Zone', value: controller.cursorHex.zone, valueColor: Amber.normal),
          LabeledRow(label: 'Dist', value: '${sec.dist} from hub', valueColor: Amber.normal),
          const SizedBox(height: 6),
          if (sec.explored) ...[
            LabeledRow(label: 'Terrain', value: sec.terrain.label, valueColor: Amber.normal),
            LabeledRow(
              label: 'Metal',
              value: sec.resMetal.label,
              valueColor: sec.resMetal.index >= 3 ? Amber.bright : Amber.normal,
            ),
            LabeledRow(label: 'Crystal', value: sec.resCrystal.label, valueColor: Amber.normal),
            LabeledRow(label: 'Deut', value: sec.resDeut.label, valueColor: Amber.dim),
            const SizedBox(height: 6),
            LabeledRow(
              label: 'Threat',
              value: sec.hasHostile ? '${sec.hostileCount} MLM hostiles' : 'Clear',
              valueColor: sec.hasHostile ? Amber.danger : Amber.dim,
            ),
          ] else ...[
            Text('UNEXPLORED', style: Amber.mono(size: 11, color: Amber.faint)),
            Text(
              'Fog of war -- send a\nfleet to reveal.',
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
            'Routes calculated from\nexplored paths only.',
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
