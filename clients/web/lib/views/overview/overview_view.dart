import 'package:flutter/material.dart';

import '../../console/services.dart';
import '../../design/tokens.dart';

class OverviewView extends StatefulWidget {
  const OverviewView({super.key});

  @override
  State<OverviewView> createState() => _OverviewViewState();
}

class _OverviewViewState extends State<OverviewView> {
  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return ListenableBuilder(
      listenable: con.game,
      builder: (context, _) => Center(child: Text('Overview t=${con.state.tick}', style: T.mono())),
    );
  }
}
