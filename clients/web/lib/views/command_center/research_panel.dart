import 'package:flutter/material.dart';

import '../../models/homeworld.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';
import '../../widgets/progress_bar.dart';

class ResearchPanel extends StatelessWidget {
  final ResearchState research;
  const ResearchPanel({super.key, required this.research});

  @override
  Widget build(BuildContext context) {
    return AmberPanel(
      title: 'RESEARCH LAB',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          RichText(
            text: TextSpan(children: [
              TextSpan(
                text: '> ${research.name}',
                style: Amber.mono(size: 11, color: Amber.bright),
              ),
              TextSpan(
                text: '  ${research.time}',
                style: Amber.mono(size: 11, color: Amber.full),
              ),
            ]),
          ),
          const SizedBox(height: 4),
          ProgressRow(pct: research.pct),
          const SizedBox(height: 4),
          RichText(
            text: TextSpan(children: [
              TextSpan(
                text: '  Fragments: ',
                style: Amber.mono(size: 11, color: Amber.dim),
              ),
              TextSpan(
                text: '${research.fragments} required',
                style: Amber.mono(size: 11, color: Amber.normal),
              ),
            ]),
          ),
          const SizedBox(height: 8),
          Text('Completed:', style: Amber.mono(size: 11, color: Amber.dim)),
          const SizedBox(height: 2),
          Text(
            '  ${research.completed.join(' | ')}',
            style: Amber.mono(size: 11, color: Amber.dim),
          ),
        ],
      ),
    );
  }
}
