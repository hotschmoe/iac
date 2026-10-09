import 'package:flutter/material.dart';

import '../../console/services.dart';
import '../../design/tokens.dart';

class MapView extends StatefulWidget {
  const MapView({super.key});

  @override
  State<MapView> createState() => _MapViewState();
}

class _MapViewState extends State<MapView> {
  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return ListenableBuilder(
      listenable: con.game,
      builder: (context, _) => Center(child: Text('Map t=${con.state.tick}', style: T.mono())),
    );
  }
}
