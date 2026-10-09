import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;
import '../../scenes/homeworld/plan.dart';
import '../../state/game_controller.dart';

const _lanes = <(String, List<proto.ResearchType>)>[
  ('Propulsion', [proto.ResearchType.fuelEfficiency, proto.ResearchType.extendedFuelTanks, proto.ResearchType.navigation, proto.ResearchType.emergencyJump]),
  ('Combat', [proto.ResearchType.reinforcedHulls, proto.ResearchType.advancedShields, proto.ResearchType.weaponsResearch]),
  ('Industry', [proto.ResearchType.harvestingEfficiency]),
  ('Hull classes', [proto.ResearchType.corvetteTech, proto.ResearchType.frigateTech, proto.ResearchType.cruiserTech, proto.ResearchType.haulerTech]),
];

class ResearchTab extends StatelessWidget {
  final GameController ctrl;
  final ValueChanged<int> onSelect;
  const ResearchTab({super.key, required this.ctrl, required this.onSelect});

  @override
  Widget build(BuildContext context) {
    final cat = ctrl.state.catalog;
    if (cat == null) return const SizedBox.shrink();
    final byTech = {for (var i = 0; i < cat.research.length; i++) cat.research[i].tech: i};
    final active = ctrl.state.research.name;
    return LayoutBuilder(builder: (context, box) {
      final w = math.max(300.0, box.maxWidth);
      final cols = w >= 620 ? 4 : 2;
      const gap = 24.0, pad = 12.0, nh = 64.0, head = 26.0;
      final nw = math.min(190.0, (w - 2 * pad - (cols - 1) * gap) / cols);
      final rects = <proto.ResearchType, Rect>{};
      final heads = <(String, double)>[];
      var y = pad;
      for (final lane in _lanes) {
        heads.add((lane.$1, y));
        y += head;
        final techs = [for (final t in lane.$2) if (byTech.containsKey(t)) t];
        for (var k = 0; k < techs.length; k++) {
          final cx = k % cols, cy = k ~/ cols;
          rects[techs[k]] = Rect.fromLTWH(pad + cx * (nw + gap), y + cy * (nh + 18), nw, nh);
        }
        y += ((techs.length + cols - 1) ~/ math.max(1, cols)) * (nh + 18) + 6;
      }
      final edges = <(Rect, Rect, bool)>[];
      for (final lane in _lanes) {
        final techs = [for (final t in lane.$2) if (rects.containsKey(t)) t];
        for (var k = 0; k + 1 < techs.length; k++) {
          edges.add((rects[techs[k]]!, rects[techs[k + 1]]!, false));
        }
      }
      for (final o in cat.research) {
        for (final r in o.requires) {
          for (final other in cat.research) {
            if (other.tech.label == r.name && rects.containsKey(other.tech) && rects.containsKey(o.tech)) {
              edges.add((rects[other.tech]!, rects[o.tech]!, true));
            }
          }
        }
      }
      return SizedBox(
        width: w,
        height: y + pad,
        child: Stack(children: [
          Positioned.fill(child: RepaintBoundary(child: CustomPaint(painter: TechEdgesPainter(edges)))),
          for (final h in heads) Positioned(left: pad, top: h.$2 + 2, child: Lbl(h.$1, color: C.a600, size: 10)),
          for (final e in rects.entries) _node(cat.research[byTech[e.key]!], byTech[e.key]!, e.value, active),
        ]),
      );
    });
  }

  Widget _node(proto.ResearchOption o, int i, Rect r, String active) {
    final locked = o.requires.any((q) => !q.met);
    final isActive = active.startsWith(o.tech.label);
    final done = o.next == null;
    final sel = ctrl.hwCursor == i;
    final state = isActive ? 'RESEARCHING' : done ? 'DONE' : locked ? 'LOCKED' : o.level > 0 ? 'L${o.level}' : 'AVAILABLE';
    final col = isActive ? C.a500 : done ? C.own : locked ? C.text3 : C.a300;
    return Positioned.fromRect(
      key: ValueKey('hw-card-$i'),
      rect: r,
      child: Semantics(
        button: true,
        selected: sel,
        label: '${o.tech.label} level ${o.level} of ${o.maxLevel}, $state',
        excludeSemantics: true,
        onTap: () => onSelect(i),
        child: MouseRegion(
          cursor: SystemMouseCursors.click,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: () => onSelect(i),
            child: Container(
              padding: const EdgeInsets.fromLTRB(8, 6, 8, 6),
              decoration: BoxDecoration(
                color: sel ? const Color(0xF53C280A) : C.panel,
                border: Border.all(color: sel ? C.a200 : col.withValues(alpha: locked ? .35 : .8), width: sel ? 2 : 1),
              ),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisAlignment: MainAxisAlignment.center, children: [
                Text(o.tech.label.toUpperCase(), maxLines: 1, overflow: TextOverflow.ellipsis, style: T.cond(size: 11, color: locked ? C.text3 : C.a200, spacing: .8)),
                const SizedBox(height: 4),
                Row(children: [
                  Text('L${o.level}/${o.maxLevel}', style: T.mono(size: 13, color: locked ? C.text3 : C.a100, weight: FontWeight.w500)),
                  const Spacer(),
                  Text(state, style: T.cond(size: 9, color: col, spacing: 1.2)),
                ]),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}
