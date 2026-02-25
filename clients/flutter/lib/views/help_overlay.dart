import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';
import '../views/shell.dart';

class HelpOverlay extends StatelessWidget {
  final GameView currentView;
  final VoidCallback onClose;

  const HelpOverlay({
    super.key,
    required this.currentView,
    required this.onClose,
  });

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onClose,
      child: Container(
        color: Amber.bg.withValues(alpha: 0.85),
        child: Center(
          child: GestureDetector(
            onTap: () {},
            child: Container(
              width: 520,
              padding: const EdgeInsets.all(24),
              decoration: BoxDecoration(
                color: Amber.bgPanel,
                border: Border.all(color: Amber.dim, width: 0.5),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Text(
                        'KEYBIND REFERENCE',
                        style: Amber.mono(
                          size: 13,
                          color: Amber.full,
                          weight: FontWeight.w700,
                        ).copyWith(letterSpacing: 3),
                      ),
                      const Spacer(),
                      Text(
                        'ESC / ? to close',
                        style: Amber.mono(size: 9, color: Amber.dim),
                      ),
                    ],
                  ),
                  const SizedBox(height: 4),
                  Container(height: 0.5, color: Amber.dim),
                  const SizedBox(height: 16),
                  _section('GLOBAL'),
                  _bind('1 / cc', 'Command Center'),
                  _bind('2 / ws', 'Windshield'),
                  _bind('3 / map', 'Star Map'),
                  _bind('ESC', 'Return to Command Center'),
                  _bind('?', 'Toggle this help'),
                  _bind('Any letter', 'Focus command bar'),
                  const SizedBox(height: 12),
                  if (currentView == GameView.starMap) ...[
                    _section('STAR MAP'),
                    _bind('Arrows', 'Move cursor'),
                    _bind('+ / -', 'Zoom in / out'),
                    _bind('HOME', 'Center on fleet'),
                    _bind('ENTER', 'Set waypoint'),
                    _bind('TAB', 'Cycle fleets'),
                  ],
                  if (currentView == GameView.windshield) ...[
                    _section('WINDSHIELD'),
                    _bind('Numpad 1-6', 'Move fleet (6 directions)'),
                    _bind('Layout', 'NW[4] NE[5]  W[3] E[6]  SW[1] SE[2]'),
                  ],
                  if (currentView == GameView.commandCenter) ...[
                    _section('COMMAND CENTER'),
                    _bind('h / harvest', 'Harvest resources'),
                    _bind('a / attack', 'Attack hostiles'),
                    _bind('b / build', 'Show build queue'),
                    _bind('r / research', 'Show research'),
                    _bind('f / fleet', 'Fleet status'),
                    _bind('p / policy', 'Policy table'),
                  ],
                  const SizedBox(height: 16),
                  Container(height: 0.5, color: Amber.faint),
                  const SizedBox(height: 8),
                  Text(
                    'FLEET COMMAND INTERFACE v0.2.0',
                    style: Amber.mono(size: 9, color: Amber.faint).copyWith(
                      letterSpacing: 2,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _section(String title) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Text(
        title,
        style: Amber.mono(size: 10, color: Amber.dim).copyWith(
          letterSpacing: 2,
        ),
      ),
    );
  }

  Widget _bind(String key, String desc) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 3),
      child: RichText(
        text: TextSpan(children: [
          TextSpan(
            text: key.padRight(16),
            style: Amber.mono(size: 11, color: Amber.bright),
          ),
          TextSpan(
            text: desc,
            style: Amber.mono(size: 11, color: Amber.normal),
          ),
        ]),
      ),
    );
  }
}
