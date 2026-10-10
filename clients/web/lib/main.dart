import 'package:flutter/material.dart';

import 'design/panel.dart';
import 'design/theme.dart';
import 'design/tokens.dart';
import 'solo/solo_engine.dart';
import 'state/connection_provider.dart';
import 'state/game_controller.dart';
import 'views/boot_screen.dart';
import 'views/login_screen.dart';
import 'views/shell.dart';
import 'views/solo_screen.dart';

void main() {
  runApp(const IacApp());
}

class IacApp extends StatelessWidget {
  const IacApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: Shell.title,
      theme: consoleTheme(),
      debugShowCheckedModeBanner: false,
      home: const GameScreen(),
    );
  }
}

/// Boot animation, then the way into a world: the login form of a server,
/// or in the single-player bundle the engine in this tab. A page URL that
/// names a server or a player (`?ws=`, `?name=`) always means the server.
class GameScreen extends StatefulWidget {
  /// The engine to play single player on; by default the bundle's own, when
  /// this is the single-player build.
  final SoloEngine? solo;
  final ConnectParams? params;
  final bool boot;
  const GameScreen({super.key, this.solo, this.params, this.boot = true});

  @override
  State<GameScreen> createState() => _GameScreenState();
}

class _GameScreenState extends State<GameScreen> {
  /// Keys what the browser remembers of the single-player world.
  static const _soloWorld = 'single-player';

  final _controller = GameController();
  late final ConnectParams _params = widget.params ?? ConnectParams.fromUri(Uri.base);
  late final SoloEngine? _solo = widget.solo ?? (singlePlayerBuild && !_params.explicit ? SoloEngine() : null);
  late final AppLifecycleListener _lifecycle;
  late bool _booting = widget.boot;

  /// The player left single player for a server's login form.
  bool _multiplayer = false;

  /// True once the player has chosen a name (or the URL did).
  bool _started = false;

  /// True once the single-player world is running and has been entered.
  bool _entered = false;

  @override
  void initState() {
    super.initState();
    _lifecycle = AppLifecycleListener(onStateChange: _onLifecycle);
    final name = _params.name;
    if (_solo == null && name != null) _connect(name);
  }

  /// A hidden page may never come back: write the save while it still runs.
  void _onLifecycle(AppLifecycleState s) {
    if (_entered && s != AppLifecycleState.resumed) _solo?.saveNow();
  }

  void _connect(String name, [String? token]) {
    _started = true;
    _controller.start(params: _params, name: name, token: token);
  }

  void _enterSolo() {
    final solo = _solo!;
    final world = solo.status.value.world;
    if (world == null) return;
    setState(() => _entered = true);
    _controller.open(
      Uplink(id: _soloWorld, label: 'SINGLE PLAYER', feed: 'The engine in this tab', dial: solo.connect),
      name: world.commander,
    );
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    _controller.stop();
    _controller.dispose();
    if (widget.solo == null) _solo?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: C.void_,
      body: Stack(
        children: [
          if (!_booting) _body(),
          if (_booting) BootScreen(onComplete: () => setState(() => _booting = false)),
        ],
      ),
    );
  }

  Widget _body() {
    final solo = _solo;
    return ListenableBuilder(
      listenable: Listenable.merge([_controller, solo?.status]),
      builder: (context, _) {
        if (solo != null && !_multiplayer) {
          if (!_entered || solo.status.value.phase == SoloPhase.failed) {
            return SoloScreen(
              engine: solo,
              onEnter: _enterSolo,
              onMultiplayer: () => setState(() => _multiplayer = true),
            );
          }
          if (!_controller.hasState) return _connecting('STARTING THE WORLD ...');
          return Shell(controller: _controller);
        }
        final rejection = _controller.authRejection;
        if (!_started || rejection != null) {
          return LoginScreen(
            url: _params.url,
            initialName: rejection == null ? 'Admiral' : _controller.playerName,
            error: rejection?.message,
            onConnect: (n, t) => setState(() => _connect(n, t)),
            onSolo: solo == null ? null : () => setState(() => _multiplayer = false),
          );
        }
        if (!_controller.hasState) return _connecting('ESTABLISHING UPLINK ... ${_params.url}');
        return Shell(controller: _controller);
      },
    );
  }

  Widget _connecting(String what) {
    final error = _controller.connectionError;
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(what, style: T.mono(size: 11, color: C.text3, spacing: 1)),
          if (error != null) ...[
            const SizedBox(height: 10),
            Text('$error. Trying again ...', key: const Key('uplink-error'), style: T.mono(size: 11, color: C.ember)),
            if (_started) ...[
              const SizedBox(height: 14),
              ConsoleButton('Back to login', onPressed: () {
                _controller.stop();
                setState(() => _started = false);
              }),
            ],
          ],
        ],
      ),
    );
  }
}
