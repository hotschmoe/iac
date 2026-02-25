import 'package:flutter/material.dart';

import '../../models/game_state.dart';
import '../../state/game_controller.dart';
import '../../theme/amber_theme.dart';
import 'alerts_panel.dart';
import 'build_queue_panel.dart';
import 'event_log_panel.dart';
import 'fleets_panel.dart';
import 'research_panel.dart';
import 'resources_panel.dart';
import 'shipyard_panel.dart';

class CommandCenterView extends StatelessWidget {
  final GameController controller;
  const CommandCenterView({super.key, required this.controller});

  @override
  Widget build(BuildContext context) {
    final state = controller.state;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 4),
          child: Text(
            "DEFAULT VIEW -- HOMEWORLD MANAGEMENT & FLEET OVERVIEW -- THE ADMIRAL'S DESK",
            style: Amber.mono(size: 9, color: Amber.dim).copyWith(
              letterSpacing: 1,
            ),
          ),
        ),
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(8),
            child: LayoutBuilder(
              builder: (context, constraints) {
                if (constraints.maxWidth > 600) {
                  return _wideLayout(state);
                }
                return _narrowLayout(state);
              },
            ),
          ),
        ),
      ],
    );
  }

  Widget _wideLayout(GameState state) {
    return Column(
      children: [
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Column(
                  children: [
                    ResourcesPanel(resources: state.resources),
                    const SizedBox(height: 4),
                    BuildQueuePanel(queue: state.buildQueue),
                    const SizedBox(height: 4),
                    ResearchPanel(research: state.research),
                  ],
                ),
              ),
              const SizedBox(width: 4),
              Expanded(
                child: Column(
                  children: [
                    FleetsPanel(fleets: state.fleets),
                    const SizedBox(height: 4),
                    ShipyardPanel(
                        queue: state.shipyard, docked: state.docked),
                    const SizedBox(height: 4),
                    AlertsPanel(alerts: state.alerts),
                  ],
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 4),
        EventLogPanel(events: state.events),
      ],
    );
  }

  Widget _narrowLayout(GameState state) {
    return Column(
      children: [
        ResourcesPanel(resources: state.resources),
        const SizedBox(height: 4),
        FleetsPanel(fleets: state.fleets),
        const SizedBox(height: 4),
        BuildQueuePanel(queue: state.buildQueue),
        const SizedBox(height: 4),
        ShipyardPanel(queue: state.shipyard, docked: state.docked),
        const SizedBox(height: 4),
        ResearchPanel(research: state.research),
        const SizedBox(height: 4),
        AlertsPanel(alerts: state.alerts),
        const SizedBox(height: 4),
        EventLogPanel(events: state.events),
      ],
    );
  }
}
