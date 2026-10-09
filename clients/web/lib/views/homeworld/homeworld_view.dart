import 'package:flutter/material.dart';

import '../../console/services.dart';
import '../../design/tokens.dart';

class HomeworldView extends StatefulWidget {
  const HomeworldView({super.key});

  @override
  State<HomeworldView> createState() => _HomeworldViewState();
}

class _HomeworldViewState extends State<HomeworldView> {
  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return ListenableBuilder(
      listenable: con.game,
      builder: (context, _) => Center(child: Text('Homeworld t=${con.state.tick}', style: T.mono())),
    );
  }
}
