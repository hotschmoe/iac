import 'package:flutter/material.dart';

import '../../state/game_controller.dart';
import '../../theme/amber_theme.dart';
import 'windshield_painter.dart';
import 'windshield_sidebar.dart';

class WindshieldView extends StatelessWidget {
  final GameController controller;
  const WindshieldView({super.key, required this.controller});

  @override
  Widget build(BuildContext context) {
    final state = controller.state;
    final fleet = state.fleets[controller.activeFleet];

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 4),
          child: Text(
            'FLEET CONTROL -- DIRECT PILOTING -- THE WINDSHIELD',
            style: Amber.mono(size: 9, color: Amber.dim).copyWith(
              letterSpacing: 1,
            ),
          ),
        ),
        Expanded(
          child: Row(
            children: [
              Expanded(
                child: Container(
                  decoration: BoxDecoration(
                    color: Amber.bgInset,
                    border: const Border(
                      right: BorderSide(color: Amber.border),
                    ),
                  ),
                  child: ClipRect(
                    child: CustomPaint(
                      painter: WindshieldPainter(fleetSector: fleet.sector),
                      child: const SizedBox.expand(),
                    ),
                  ),
                ),
              ),
              SizedBox(
                width: 220,
                child: WindshieldSidebar(
                  fleet: fleet,
                  sector: state.sector,
                ),
              ),
            ],
          ),
        ),
        Container(
          decoration: const BoxDecoration(
            border: Border(top: BorderSide(color: Amber.border)),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 6),
          child: Text(
            'move: [1]E  [2]NE  [3]NW  [4]W  [5]SW  [6]SE     [h]arvest [a]ttack [r]ecall [esc]back',
            style: Amber.mono(size: 10, color: Amber.dim),
          ),
        ),
      ],
    );
  }
}
