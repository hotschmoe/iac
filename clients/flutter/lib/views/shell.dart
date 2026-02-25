import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../state/game_controller.dart';
import '../theme/amber_theme.dart';
import '../theme/crt_overlay.dart';
import 'command_center/command_center_view.dart';
import 'star_map/star_map_view.dart';
import 'windshield/windshield_view.dart';

enum GameView { commandCenter, windshield, starMap }

class Shell extends StatefulWidget {
  final GameController controller;
  const Shell({super.key, required this.controller});

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  GameView _currentView = GameView.commandCenter;
  final _cmdController = TextEditingController();
  final _cmdFocus = FocusNode();

  GameController get ctrl => widget.controller;

  void _switchView(GameView view) {
    setState(() => _currentView = view);
  }

  void _handleCommand(String cmd) {
    cmd = cmd.trim().toLowerCase();
    if (cmd.isEmpty) return;
    _cmdController.clear();

    if (cmd == '1' || cmd == 'cc') {
      _switchView(GameView.commandCenter);
      return;
    }
    if (cmd == '2' || cmd == 'ws') {
      _switchView(GameView.windshield);
      return;
    }
    if (cmd == '3' || cmd == 'map' || cmd == 'sm') {
      _switchView(GameView.starMap);
      return;
    }

    ctrl.handleCommand(cmd);
  }

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;

    if (_cmdFocus.hasFocus) {
      if (event.logicalKey == LogicalKeyboardKey.escape) {
        _cmdFocus.unfocus();
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    }

    // View switching
    if (event.logicalKey == LogicalKeyboardKey.digit1) {
      _switchView(GameView.commandCenter);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.digit2) {
      _switchView(GameView.windshield);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.digit3) {
      _switchView(GameView.starMap);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      _switchView(GameView.commandCenter);
      return KeyEventResult.handled;
    }

    // Star map navigation
    if (_currentView == GameView.starMap) {
      if (event.logicalKey == LogicalKeyboardKey.arrowLeft) {
        ctrl.panMap(-1, 0);
        return KeyEventResult.handled;
      }
      if (event.logicalKey == LogicalKeyboardKey.arrowRight) {
        ctrl.panMap(1, 0);
        return KeyEventResult.handled;
      }
      if (event.logicalKey == LogicalKeyboardKey.arrowUp) {
        ctrl.panMap(0, -1);
        return KeyEventResult.handled;
      }
      if (event.logicalKey == LogicalKeyboardKey.arrowDown) {
        ctrl.panMap(0, 1);
        return KeyEventResult.handled;
      }
      if (event.logicalKey == LogicalKeyboardKey.equal ||
          event.logicalKey == LogicalKeyboardKey.add) {
        ctrl.zoomIn();
        return KeyEventResult.handled;
      }
      if (event.logicalKey == LogicalKeyboardKey.minus) {
        ctrl.zoomOut();
        return KeyEventResult.handled;
      }
      if (event.logicalKey == LogicalKeyboardKey.home) {
        ctrl.recenterMap();
        return KeyEventResult.handled;
      }
    }

    // Windshield movement (numpad keys since digit 1-3 switch views)
    if (_currentView == GameView.windshield) {
      int? dir;
      if (event.logicalKey == LogicalKeyboardKey.numpad1) dir = 0;
      if (event.logicalKey == LogicalKeyboardKey.numpad2) dir = 1;
      if (event.logicalKey == LogicalKeyboardKey.numpad3) dir = 2;
      if (event.logicalKey == LogicalKeyboardKey.numpad4) dir = 3;
      if (event.logicalKey == LogicalKeyboardKey.numpad5) dir = 4;
      if (event.logicalKey == LogicalKeyboardKey.numpad6) dir = 5;
      if (dir != null) {
        ctrl.moveFleet(dir);
        return KeyEventResult.handled;
      }
    }

    // Focus command bar on letter keys
    if (event.character != null &&
        event.character!.length == 1 &&
        !HardwareKeyboard.instance.isControlPressed &&
        !HardwareKeyboard.instance.isMetaPressed) {
      _cmdFocus.requestFocus();
    }

    return KeyEventResult.ignored;
  }

  @override
  void dispose() {
    _cmdController.dispose();
    _cmdFocus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return CrtOverlay(
      child: Focus(
        autofocus: true,
        onKeyEvent: _handleKey,
        child: ListenableBuilder(
          listenable: ctrl,
          builder: (context, _) {
            final state = ctrl.state;
            return Column(
              children: [
                _buildHeader(state.tick, state.clockDisplay),
                _buildTabBar(),
                Expanded(child: _buildView()),
                _buildCommandBar(),
              ],
            );
          },
        ),
      ),
    );
  }

  Widget _buildHeader(int tick, String clock) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      decoration: const BoxDecoration(
        color: Amber.bgPanel,
        border: Border(bottom: BorderSide(color: Amber.border)),
      ),
      child: Column(
        children: [
          Row(
            children: [
              Text(
                'IN AMBER CLAD',
                style: Amber.mono(
                  size: 15,
                  color: Amber.full,
                  weight: FontWeight.w700,
                ).copyWith(letterSpacing: 3),
              ),
              const Spacer(),
              Text(
                'TICK $tick',
                style: Amber.mono(size: 11, color: Amber.dim),
              ),
              Text(
                ' | $clock',
                style: Amber.mono(size: 11, color: Amber.dim),
              ),
            ],
          ),
          const SizedBox(height: 2),
          Align(
            alignment: Alignment.centerLeft,
            child: Text(
              "FLEET COMMAND INTERFACE -- ADMIRAL'S CONSOLE",
              style: Amber.mono(size: 9, color: Amber.dim).copyWith(
                letterSpacing: 1,
              ),
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildTabBar() {
    return Container(
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: Amber.border)),
      ),
      child: Row(
        children: [
          _tab('1', 'COMMAND CENTER', GameView.commandCenter),
          _tab('2', 'WINDSHIELD', GameView.windshield),
          _tab('3', 'STAR MAP', GameView.starMap),
        ],
      ),
    );
  }

  Widget _tab(String key, String label, GameView view) {
    final active = _currentView == view;
    return GestureDetector(
      onTap: () => _switchView(view),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 7),
        decoration: BoxDecoration(
          border: Border(
            bottom: BorderSide(
              color: active ? Amber.full : Colors.transparent,
              width: 1,
            ),
          ),
        ),
        child: Row(
          children: [
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 1),
              decoration: BoxDecoration(
                border: Border.all(
                  color: active ? Amber.full : Amber.dim,
                  width: 0.5,
                ),
              ),
              child: Text(
                key,
                style: Amber.mono(
                  size: 9,
                  color: active ? Amber.full : Amber.dim,
                ),
              ),
            ),
            const SizedBox(width: 6),
            Text(
              label,
              style: Amber.mono(
                size: 10,
                color: active ? Amber.full : Amber.dim,
              ).copyWith(letterSpacing: 1),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildView() {
    switch (_currentView) {
      case GameView.commandCenter:
        return CommandCenterView(controller: ctrl);
      case GameView.windshield:
        return WindshieldView(controller: ctrl);
      case GameView.starMap:
        return StarMapView(controller: ctrl);
    }
  }

  Widget _buildCommandBar() {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 6),
      decoration: const BoxDecoration(
        color: Amber.bgPanel,
        border: Border(top: BorderSide(color: Amber.faint)),
      ),
      child: Row(
        children: [
          Text(
            '>',
            style: Amber.mono(size: 13, color: Amber.full),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: TextField(
              controller: _cmdController,
              focusNode: _cmdFocus,
              style: Amber.mono(size: 12, color: Amber.bright),
              cursorColor: Amber.full,
              decoration: InputDecoration(
                border: InputBorder.none,
                isDense: true,
                contentPadding: EdgeInsets.zero,
                hintText: 'enter command...',
                hintStyle: Amber.mono(size: 12, color: Amber.faint),
              ),
              onSubmitted: _handleCommand,
            ),
          ),
          Text(
            '[1]CC [2]WS [3]MAP | [h]arvest [a]ttack [b]uild [r]esearch [f]leet',
            style: Amber.mono(size: 9, color: Amber.dim).copyWith(
              letterSpacing: 0.5,
            ),
          ),
        ],
      ),
    );
  }
}
