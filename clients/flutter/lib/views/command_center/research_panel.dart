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
          Row(
            children: [
              SizedBox(
                width: 60,
                child: Text('Progress',
                    style: Amber.mono(size: 10, color: Amber.dim)),
              ),
              Expanded(
                  child: AmberProgressBar(fraction: research.pct / 100)),
              const SizedBox(width: 6),
              SizedBox(
                width: 30,
                child: Text('${research.pct.toStringAsFixed(0)}%',
                    style: Amber.mono(size: 10, color: Amber.dim),
                    textAlign: TextAlign.right),
              ),
            ],
          ),
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
