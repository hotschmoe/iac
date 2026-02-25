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
    _controller.start();
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
        ],
      ),
    );
  }
}
