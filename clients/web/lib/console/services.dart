import 'package:flutter/widgets.dart';

import '../models/game_state.dart';
import '../protocol/protocol.dart' as proto;
import '../state/game_controller.dart';
import 'economy_view.dart';
import 'prefs.dart';
import 'route_runner.dart';
import 'sfx.dart';
import 'toasts.dart';

/// Everything a screen needs besides the [GameController]: preferences,
/// toasts, sound, the route runner, the economy view model. Provided by the
/// shell and read with [Console.of].
class Console {
  final GameController game;
  final ConsolePrefs prefs;
  final ToastController toasts;
  final Sfx sfx;
  late final RouteRunner runner = RouteRunner(game, toasts);

  /// Switch screens (set by the shell).
  void Function(Screen) go = (_) {};

  /// Open the `/` command palette with optional text (set by the shell).
  void Function([String text]) openPalette = ([_ = '']) {};

  Console({required this.game, required this.prefs, required this.toasts, required this.sfx}) {
    _lastTick = game.state.tick;
    lastTickAt = DateTime.now();
    game.addListener(_onGame);
  }

  int _lastTick = -1;
  DateTime lastTickAt = DateTime.now();

  void _onGame() {
    if (game.state.tick != _lastTick) {
      _lastTick = game.state.tick;
      lastTickAt = DateTime.now();
      if (game.hasState && (game.isLive || game.isDemo) && (_lastBoard < 0 || game.state.tick - _lastBoard >= 30)) {
        _lastBoard = game.state.tick;
        game.sendCommand(proto.LeaderboardCommand(limit: 10));
      }
    }
  }

  /// Seconds since the last server tick, for interpolating between 1 Hz snapshots.
  double get sinceTick => DateTime.now().difference(lastTickAt).inMicroseconds / 1e6;

  EconomyView get economy => EconomyView.of(game.state, me: game.playerName);

  int _lastBoard = -1;

  GameState get state => game.state;

  final Map<Screen, bool Function(KeyEvent)> _keys = {};
  Screen screen = Screen.overview;

  /// A screen registers its hotkeys here; the shell calls them only while
  /// that screen is showing and no text field has focus. Returns the unregister.
  VoidCallback registerKeys(Screen s, bool Function(KeyEvent) h) {
    _keys[s] = h;
    return () {
      if (_keys[s] == h) _keys.remove(s);
    };
  }

  bool dispatchKey(KeyEvent e) => _keys[screen]?.call(e) ?? false;

  void play(String cue) {
    sfx.enabled = prefs.sound;
    sfx.play(cue);
  }

  void dispose() {
    game.removeListener(_onGame);
    runner.dispose();
  }

  static Console of(BuildContext context) => context.getInheritedWidgetOfExactType<ConsoleScope>()!.console;
}

class ConsoleScope extends InheritedWidget {
  final Console console;
  const ConsoleScope({super.key, required this.console, required super.child});
  @override
  bool updateShouldNotify(ConsoleScope old) => old.console != console;
}

enum Screen { overview, windshield, map, homeworld }
