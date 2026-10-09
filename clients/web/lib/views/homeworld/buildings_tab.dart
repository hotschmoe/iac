import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/prefs.dart';
import '../../design/beat.dart';
import '../../design/draw.dart';
import '../../design/gauges.dart';
import '../../design/tokens.dart';
import '../../protocol/protocol.dart' as proto;
import '../../scenes/homeworld/plan.dart';
import '../../state/game_controller.dart';
import 'spec.dart';

class BuildingsTab extends StatelessWidget {
  final GameController ctrl;
  final ValueChanged<int> onSelect;
  final ConsolePrefs? prefs;
  const BuildingsTab({super.key, required this.ctrl, required this.onSelect, this.prefs});

  String _sub(proto.BuildingType t) {
    final r = ctrl.state.resources;
    return switch (t) {
      proto.BuildingType.metalMine => '+${r.metal.rate.toStringAsFixed(2)} FE/T',
      proto.BuildingType.crystalMine => '+${r.crystal.rate.toStringAsFixed(2)} CR/T',
      proto.BuildingType.deuteriumSynthesizer => '+${r.deut.rate.toStringAsFixed(2)} DE/T',
      proto.BuildingType.shipyard => 'BUILDS SHIPS',
      proto.BuildingType.researchLab => 'RESEARCH',
      proto.BuildingType.fuelDepot => 'FUEL',
      proto.BuildingType.sensorArray => 'SENSORS',
      proto.BuildingType.defenseGrid => 'DEFENCE',
      proto.BuildingType.storageVault => 'STORAGE',
      proto.BuildingType.fabricator => 'SLOT B',
    };
  }

  @override
  Widget build(BuildContext context) {
    final cat = ctrl.state.catalog;
    if (cat == null) return const SizedBox.shrink();
    final still = reducedMotion(context) || (prefs?.reduceMotion ?? false);
    return LayoutBuilder(builder: (context, box) {
      final w = math.max(300.0, box.maxWidth);
      // plan spans q -2..2 -> 8 s wide; r spans about 5.2 s tall
      final s = math.min(66.0, (w - 16) / 8.4);
      final h = s * 6.2 + 56;
      final origin = Offset(w / 2, s * 2.9 + 10);
      final tileW = s * 2 * .96;
      final tileH = tileW * math.sqrt(3) / 2;
      final narrow = s < 52;
      return SizedBox(
        height: math.max(h, 360),
        width: w,
        child: Stack(clipBehavior: Clip.none, children: [
          Positioned.fill(child: RepaintBoundary(child: CustomPaint(painter: PlanBackdropPainter(s * 1.04, origin)))),
          // core
          Positioned(
            left: origin.dx - tileW / 2,
            top: origin.dy - tileH / 2,
            width: tileW,
            height: tileH,
            child: IgnorePointer(
              child: CustomPaint(
                painter: HexTilePainter(TileState.normal, 0, core: true),
                child: Center(
                    child: Column(mainAxisSize: MainAxisSize.min, children: [
                  Icon24Home(size: narrow ? 18 : 28),
                  Text('HOMEWORLD', style: T.cond(size: narrow ? 9 : 10.5, color: C.a200, spacing: 1.2)),
                ])),
              ),
            ),
          ),
          for (var i = 0; i < cat.buildings.length; i++)
            _tile(context, i, cat.buildings[i], origin, s * 1.04, tileW, tileH, narrow, still),
          Positioned(right: 8, bottom: 8, child: _TitleBlock(ctrl)),
        ]),
      );
    });
  }

  Widget _tile(BuildContext context, int i, proto.BuildingOption o, Offset origin, double s, double tw, double th, bool narrow, bool still) {
    final slot = planSlots[o.buildingType]!;
    final c = origin + planCenter(slot.$1, slot.$2, s);
    final spec = specFor(ctrl, index: i);
    final locked = o.requires.any((r) => !r.met);
    final busy = ctrl.state.buildQueue.isNotEmpty && ctrl.state.buildQueue.first.name.startsWith(o.buildingType.label);
    final sel = ctrl.hwCursor == i;
    final st = sel
        ? TileState.selected
        : busy
            ? TileState.busy
            : locked
                ? TileState.locked
                : (spec?.canQueue ?? false)
                    ? TileState.can
                    : TileState.normal;
    final col = locked ? C.text4 : C.a400;
    Widget paintTile(int phase) => CustomPaint(
          painter: HexTilePainter(busy && !sel ? TileState.busy : st, phase),
          child: Center(
            child: Padding(
              padding: EdgeInsets.symmetric(horizontal: tw * .17),
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                buildingIcon(o.buildingType, narrow ? 16 : 24, col),
                Text(o.buildingType.label.toUpperCase(),
                    maxLines: 2, textAlign: TextAlign.center, style: T.cond(size: 9, color: C.text2, weight: FontWeight.w500, spacing: narrow ? .2 : 1)),
                Register(o.level, width: 2, size: narrow ? 14 : 20, color: locked ? C.text3 : C.a100),
                if (!narrow) Text(_sub(o.buildingType), style: T.mono(size: 8.5, color: C.text3)),
              ]),
            ),
          ),
        );
    return Positioned(
      key: ValueKey('hw-card-$i'),
      left: c.dx - tw / 2,
      top: c.dy - th / 2,
      width: tw,
      height: th,
      child: Semantics(
        button: true,
        selected: sel,
        label: '${o.buildingType.label} level ${o.level}${locked ? ', locked' : ''}',
        excludeSemantics: true,
        onTap: () => onSelect(i),
        child: MouseRegion(
          cursor: SystemMouseCursors.click,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: () => onSelect(i),
            child: RepaintBoundary(
              child: busy && !still
                  ? ListenableBuilder(listenable: Beat.instance, builder: (_, _) => paintTile(Beat.instance.step))
                  : paintTile(0),
            ),
          ),
        ),
      ),
    );
  }
}

class Icon24Home extends StatelessWidget {
  final double size;
  const Icon24Home({super.key, required this.size});
  @override
  Widget build(BuildContext context) => CustomPaint(
      size: Size.square(size),
      painter: _HomePainter());
}

class _HomePainter extends CustomPainter {
  @override
  void paint(Canvas c, Size s) {
    final k = s.width / 2;
    c.translate(k, k);
    c.drawCircle(Offset.zero, k * .5, Draw.line(C.a300, 1.2));
    c.drawOval(Rect.fromCenter(center: Offset.zero, width: k * 1.9, height: k * .6), Draw.line(C.own, 1, .8));
    c.drawLine(Offset(0, -k * .5), Offset(0, k * .5), Draw.line(C.a300, 1, .6));
  }

  @override
  bool shouldRepaint(_HomePainter o) => false;
}

class _TitleBlock extends StatelessWidget {
  final GameController ctrl;
  const _TitleBlock(this.ctrl);
  Widget _c(String l, String v, {Color vc = C.text2}) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: const BoxDecoration(border: Border(right: BorderSide(color: C.lineLo))),
        child: Row(mainAxisSize: MainAxisSize.min, children: [
          Text(l, style: T.mono(size: 9, color: C.text3, spacing: 1)),
          const SizedBox(width: 8),
          Text(v, style: T.mono(size: 9, color: vc, weight: FontWeight.w500)),
        ]),
      );

  @override
  Widget build(BuildContext context) {
    final hw = ctrl.state.homeworld;
    return Container(
      decoration: BoxDecoration(border: Border.all(color: C.lineHi), color: C.void_),
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
        Row(mainAxisSize: MainAxisSize.min, children: [_c('DRAWING', 'IAC-HW-001', vc: C.a300), _c('SHEET', '1/1', vc: C.a300)]),
        Container(height: 1, color: C.lineLo),
        Row(mainAxisSize: MainAxisSize.min, children: [_c('PLAN', 'HOMEWORLD, SECTOR ${hw.q},${hw.r}'), _c('SCALE', 'NTS')]),
      ]),
    );
  }
}
