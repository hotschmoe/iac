import 'package:flutter/material.dart';

import '../../models/game_state.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';

class EventLogPanel extends StatelessWidget {
  final List<GameEvent> events;
  const EventLogPanel({super.key, required this.events});

  @override
  Widget build(BuildContext context) {
    return AmberPanel(
      title: 'EVENT LOG',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final event in events)
            RichText(
              text: TextSpan(children: [
                TextSpan(
                  text: '${event.tick} ',
                  style: Amber.mono(size: 11, color: Amber.dim),
                ),
                TextSpan(
                  text: event.message,
                  style: Amber.mono(size: 11, color: event.level.color),
                ),
              ]),
            ),
        ],
      ),
    );
  }
}
