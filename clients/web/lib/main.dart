import 'package:flutter/material.dart';

import 'state/game_controller.dart';
import 'theme/amber_theme.dart';
import 'views/boot_screen.dart';
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
      theme: Amber.themeData(),
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
  bool _booting = true;

  @override
  void initState() {
    super.initState();
    // Connect to live Dart server (falls back to offline demo if unreachable).
    _controller.start(name: 'Admiral');
  }

  @override
  void dispose() {
    _controller.stop();
    _controller.dispose();
    super.dispose();
  }

  void _onBootComplete() {
    setState(() => _booting = false);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Amber.bg,
      body: Stack(
        children: [
          if (!_booting) Shell(controller: _controller),
          if (_booting) BootScreen(onComplete: _onBootComplete),
          // Live / demo indicator
          if (!_booting)
            Positioned(
              right: 12,
              bottom: 48,
              child: ListenableBuilder(
                listenable: _controller,
                builder: (context, _) {
                  final live = _controller.isLive;
                  final err = _controller.connectionError;
                  final label = live
                      ? '● LIVE SERVER'
                      : (err != null ? '○ DEMO ($err)' : '○ DEMO');
                  return IgnorePointer(
                    child: Text(
                      label,
                      style: Amber.mono(
                        size: 9,
                        color: live ? Amber.full : Amber.dim,
                      ).copyWith(letterSpacing: 1),
                    ),
                  );
                },
              ),
            ),
        ],
      ),
    );
  }
}
