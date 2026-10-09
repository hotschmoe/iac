import 'package:flutter/material.dart';

import 'state/connection_provider.dart';
import 'state/game_controller.dart';
import 'design/theme.dart';
import 'design/tokens.dart';
import 'views/boot_screen.dart';
import 'views/login_screen.dart';
import 'views/shell.dart';

void main() {
  runApp(const IacApp());
}

class IacApp extends StatelessWidget {
  const IacApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'IN AMBER CLAD',
      theme: consoleTheme(),
      debugShowCheckedModeBanner: false,
      home: const GameScreen(),
    );
  }
}

class GameScreen extends StatefulWidget {
  const GameScreen({super.key});

  @override
  State<GameScreen> createState() => _GameScreenState();
}

class _GameScreenState extends State<GameScreen> {
  final _controller = GameController();
  final _params = ConnectParams.fromUri(Uri.base);
  bool _booting = true;

  /// True once the player has chosen a name (or the URL did).
  bool _started = false;

  @override
  void initState() {
    super.initState();
    final name = _params.name;
    if (name != null) _connect(name);
  }

  void _connect(String name, [String? token]) {
    _started = true;
    _controller.start(params: _params, name: name, token: token);
  }

  @override
  void dispose() {
    _controller.stop();
    _controller.dispose();
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
    return ListenableBuilder(
      listenable: _controller,
      builder: (context, _) {
        final rejection = _controller.authRejection;
        if (!_started || rejection != null) {
          return LoginScreen(
            url: _params.url,
            initialName: rejection == null ? 'Admiral' : _controller.playerName,
            error: rejection?.message,
            onConnect: (n, t) => setState(() => _connect(n, t)),
            onDemo: () => setState(() {
              _started = true;
              _controller.startDemo();
            }),
          );
        }
        if (!_controller.hasState) return _connecting();
        return Stack(
          children: [
            Shell(controller: _controller),
            Positioned(
              right: 12,
              bottom: 48,
              child: IgnorePointer(child: _status()),
            ),
          ],
        );
      },
    );
  }

  Widget _connecting() {
    return Center(
      child: Text(
        'ESTABLISHING UPLINK ... ${_params.url}',
        style: T.mono(size: 11, color: C.text3, spacing: 1),
      ),
    );
  }

  Widget _status() {
    final err = _controller.connectionError;
    final live = _controller.isLive;
    final label = '${live ? '●' : '○'} ${_controller.linkLabel}${err != null && !live ? ' ($err)' : ''}';
    return Text(
      label,
      style: T.mono(size: 9, color: live ? C.own : C.text3, spacing: 1),
    );
  }
}
