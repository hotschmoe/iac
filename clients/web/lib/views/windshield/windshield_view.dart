import 'package:flutter/material.dart';

import '../../console/services.dart';
import '../../design/tokens.dart';

class WindshieldView extends StatefulWidget {
  const WindshieldView({super.key});

  @override
  State<WindshieldView> createState() => _WindshieldViewState();
}

class _WindshieldViewState extends State<WindshieldView> {
  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return ListenableBuilder(
      listenable: con.game,
      builder: (context, _) => Center(child: Text('Windshield t=${con.state.tick}', style: T.mono())),
    );
  }
}
