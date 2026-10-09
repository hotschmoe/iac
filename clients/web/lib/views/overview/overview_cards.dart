import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/economy_view.dart';
import '../../console/intel.dart';
import '../../console/services.dart';
import '../../design/beat.dart';
import '../../design/gauges.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';
import 'ov_panel.dart';
import 'overview_logic.dart';

/// Commands the overview can issue; each echoes to the log like the command bar.
class OvActions {
  final Console con;
  OvActions(this.con);
  GameController get g => con.game;

  void research(proto.ResearchType t) {
    g.note('> research ${t.label}', level: EventLevel.full);
    g.sendCommand(proto.ResearchCommand(tech: t));
    con.play('ok');
  }

  void build(proto.BuildingType t) {
    g.note('> build ${t.label}', level: EventLevel.full);
    g.sendCommand(proto.BuildCommand(buildingType: t));
    con.play('ok');
  }

  void ship(proto.ShipClass c, int n) {
    g.note('> ship ${c.label} x$n', level: EventLevel.full);
    g.sendCommand(proto.BuildShipCommand(shipClass: c, count: n));
    con.play('ok');
  }

  void cancel(proto.QueueType q) => g.cancelHomeworldQueue(q);

  void openTab(HomeworldTab t) {
    g.selectHomeworldTab(t);
    con.go(Screen.homeworld);
  }
}

Widget costRow(proto.Resources c, {double size = 11.5}) => Wrap(spacing: 10, children: [
      if (c.metal > 0) Text('${fmtInt(c.metal)} Fe', style: T.mono(size: size, color: C.metal)),
      if (c.crystal > 0) Text('${fmtInt(c.crystal)} Cr', style: T.mono(size: size, color: C.crystal)),
      if (c.deuterium > 0) Text('${fmtInt(c.deuterium)} De', style: T.mono(size: size, color: C.deut)),
    ]);

// ── resource tiles ──────────────────────────────────────────────

class ResourceTiles extends StatelessWidget {
  final bool narrow;
  const ResourceTiles({super.key, required this.narrow});

  @override
  Widget build(BuildContext context) {
    final tiles = [for (final k in const ['metal', 'crystal', 'deut']) ResourceTile(k: k, narrow: narrow)];
    if (narrow) {
      return Column(children: [for (final t in tiles) Padding(padding: const EdgeInsets.only(bottom: 10), child: t)]);
    }
    return Row(children: [
      for (var i = 0; i < 3; i++) ...[if (i > 0) const SizedBox(width: 12), Expanded(child: tiles[i])],
    ]);
  }
}

class ResourceTile extends StatefulWidget {
  final String k;
  final bool narrow;
  const ResourceTile({super.key, required this.k, required this.narrow});
  @override
  State<ResourceTile> createState() => _ResourceTileState();
}

class _ResourceTileState extends State<ResourceTile> {
  final List<double> _hist = [];
  int _tick = -1;

  void _sample(double stock, double rate, int tick) {
    if (_hist.isEmpty) {
      for (var i = 40; i > 0; i--) {
        _hist.add(math.max(0, stock - rate * i));
      }
    }
    if (tick != _tick) {
      _tick = tick;
      _hist.add(stock);
      if (_hist.length > 40) _hist.removeAt(0);
    }
  }

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    final s = con.state;
    final k = widget.k;
    final (gem, color, name, stock, rate) = switch (k) {
      'metal' => ('Fe', C.metal, 'Metal', s.stock.metal, s.resources.metal.rate),
      'crystal' => ('Cr', C.crystal, 'Crystal', s.stock.crystal, s.resources.crystal.rate),
      _ => ('De', C.deut, 'Deuterium', s.stock.deuterium, s.resources.deut.rate),
    };
    _sample(stock, rate, s.tick);
    final st = con.economy.storage;
    final cap = st?.cap[k];
    final pct = cap == null || cap <= 0 ? 0.0 : (stock / cap).clamp(0.0, 1.0);
    final full = cap != null && pct > .97;
    final near = cap != null && pct >= .85 && !full;
    final left = st?.fullIn(k, stock, rate);
    final narrow = widget.narrow;
    final warnCol = full ? C.ember : C.threat(5);
    final lampText = full ? 'FULL' : 'NEAR CAP ${(pct * 100).round()}%';

    return Semantics(
      label: '$name ${stock.floor()}${cap != null ? ' of ${cap.floor()}' : ', uncapped'}${near || full ? ', $lampText' : ''}',
      child: ConsolePanel(
        key: Key('res-tile-$k'),
        border: full ? C.ember : near ? C.threat(5) : null,
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 14),
        child: Stack(children: [
          if (!narrow)
            Positioned(right: 0, top: 18, child: SizedBox(width: 132, height: 46, child: CustomPaint(painter: _TracePainter(List.of(_hist), color)))),
          Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              Container(padding: const EdgeInsets.symmetric(horizontal: 5), color: color, child: Text(gem.toUpperCase(), style: T.cond(size: 11, color: const Color(0xFF120A02), weight: FontWeight.w700, spacing: .7))),
              const SizedBox(width: 10),
              Lbl(name, size: 11, color: C.text2),
              const Spacer(),
              if (near || full)
                Lamp(lampText, color: warnCol, blink: true)
              else if (st != null)
                Text('VAULT L${st.vaultLevel}', style: T.mono(size: 9.5, color: C.text3)),
            ]),
            const SizedBox(height: 8),
            Wrap(crossAxisAlignment: WrapCrossAlignment.end, children: [
              Register(stock, size: narrow ? 30 : 36, glow: true),
              const SizedBox(width: 8),
              Padding(
                  padding: const EdgeInsets.only(bottom: 4),
                  child: Text(cap == null ? 'UNCAPPED' : '/ ${fmtInt(cap)}', style: T.mono(size: 12, color: C.text3))),
            ]),
            const SizedBox(height: 2),
            Text.rich(TextSpan(children: [
              TextSpan(text: '+${rate.toStringAsFixed(2)} / tick', style: T.mono(size: 11.5, color: color)),
              TextSpan(text: '  ${fmtInt(rate * 3600)} / hour', style: T.mono(size: 11.5, color: C.text3)),
            ])),
            const SizedBox(height: 12),
            if (cap != null) LedBar(pct, color: color, height: 10) else Container(height: 10, decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: C.lineLo)))),
            const SizedBox(height: 8),
            if (cap == null)
              Text('No storage cap on this world yet', style: T.mono(size: 10.5, color: C.text3))
            else if (full)
              Text('STORAGE FULL. PRODUCTION IS WASTED.', style: T.mono(size: 10.5, color: C.ember, weight: FontWeight.w600))
            else
              Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                Text(near ? 'NEAR CAP ${(pct * 100).round()}%' : '${(pct * 100).round()}% FULL',
                    style: T.mono(size: 10.5, color: near ? C.threat(5) : C.text3, weight: near ? FontWeight.w600 : FontWeight.w400)),
                Text('FULL IN ${left == null ? '--' : clockFmt(left)}', style: T.mono(size: 10.5, color: near ? C.threat(5) : C.text3)),
              ]),
          ]),
        ]),
      ),
    );
  }
}

class _TracePainter extends CustomPainter {
  final List<double> h;
  final Color col;
  _TracePainter(this.h, this.col);
  @override
  void paint(Canvas c, Size s) {
    final g = Paint()..color = C.a500.withValues(alpha: .12)..strokeWidth = 1;
    for (var k = 1; k < 6; k++) {
      c.drawLine(Offset(s.width * k / 6, 0), Offset(s.width * k / 6, s.height), g);
    }
    for (var k = 1; k < 4; k++) {
      c.drawLine(Offset(0, s.height * k / 4), Offset(s.width, s.height * k / 4), g);
    }
    if (h.length < 2) return;
    final mx = h.reduce(math.max), mn = h.reduce(math.min);
    final span = math.max(1e-6, mx - mn);
    final p = Path();
    for (var i = 0; i < h.length; i++) {
      final x = 1 + i / (h.length - 1) * (s.width - 2);
      final y = s.height - 3 - (h[i] - mn) / span * (s.height - 8);
      i == 0 ? p.moveTo(x, y) : p.lineTo(x, y);
    }
    c.drawPath(p, Paint()..style = PaintingStyle.stroke..strokeWidth = 7..blendMode = BlendMode.plus..color = col.withValues(alpha: .12));
    c.drawPath(p, Paint()..style = PaintingStyle.stroke..strokeWidth = 1.5..color = col);
  }

  @override
  bool shouldRepaint(_TracePainter o) => true;
}

// ── queue cards ─────────────────────────────────────────────────

class QueueCards extends StatelessWidget {
  final int columns;
  const QueueCards({super.key, required this.columns});

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    final cards = <Widget>[
      _BuildSlotA(con),
      _BuildSlotB(con),
      _ResearchCard(con),
      _ShipyardCard(con),
    ];
    if (columns == 1) {
      return Column(children: [for (final c in cards) Padding(padding: const EdgeInsets.only(bottom: 12), child: c)]);
    }
    final rows = <Widget>[];
    for (var i = 0; i < cards.length; i += columns) {
      final slice = cards.skip(i).take(columns).toList();
      rows.add(Padding(
        padding: EdgeInsets.only(bottom: i + columns < cards.length ? 12 : 0),
        child: IntrinsicHeight(
          child: OvFill(
            child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              for (var j = 0; j < columns; j++) ...[
                if (j > 0) const SizedBox(width: 12),
                Expanded(child: j < slice.length ? slice[j] : const SizedBox()),
              ],
            ]),
          ),
        ),
      ));
    }
    return Column(children: rows);
  }
}

Widget _idleBadge() => Blink(
    period: 12,
    onSteps: 6,
    builder: (_, on) => Opacity(
        opacity: on ? 1 : .35,
        child: Container(
            color: C.a400,
            padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
            child: Text('IDLE', style: T.cond(size: 9.5, color: const Color(0xFF1C1000), weight: FontWeight.w700, spacing: 1.5)))));

Widget _qTitle(String t, {Color color = C.a100}) => Text(t.toUpperCase(), style: T.cond(size: 16, color: color, spacing: 1));
Widget _qSub(String t) => Padding(
    padding: const EdgeInsets.only(top: 2, bottom: 8),
    child: Text(t.toUpperCase(), style: T.mono(size: 10.5, color: C.text3, spacing: .3)));

Widget _ringRow(double frac, String center, String clock, String sub) => Row(children: [
      TickRing(frac: frac, center: center, size: 72),
      const SizedBox(width: 14),
      Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(clock, style: T.mono(size: 24, color: C.a100, weight: FontWeight.w500, spacing: 1)),
        Text(sub.toUpperCase(), style: T.mono(size: 10.5, color: C.text3)),
      ])),
    ]);

/// A queue-able suggestion: name, note and cost; disabled with the reason printed on it.
class _SugTile extends StatefulWidget {
  final String name, note;
  final proto.Resources cost;
  final String? why;
  final VoidCallback onTap;
  final Key? tileKey;
  const _SugTile({this.tileKey, required this.name, required this.note, required this.cost, required this.onTap, this.why});
  @override
  State<_SugTile> createState() => _SugTileState();
}

class _SugTileState extends State<_SugTile> {
  bool _hover = false;
  @override
  Widget build(BuildContext context) {
    final off = widget.why != null;
    return Padding(
      padding: const EdgeInsets.only(bottom: 5),
      child: Semantics(
        button: true,
        enabled: !off,
        label: '${widget.name}, ${costText(widget.cost)}${off ? ', unavailable: ${widget.why}' : ''}',
        excludeSemantics: true,
        onTap: off ? null : widget.onTap,
        child: FocusableActionDetector(
          enabled: !off,
          mouseCursor: off ? SystemMouseCursors.basic : SystemMouseCursors.click,
          onShowHoverHighlight: (v) => setState(() => _hover = v),
          actions: {ActivateIntent: CallbackAction<ActivateIntent>(onInvoke: (_) {
            widget.onTap();
            return null;
          })},
          child: GestureDetector(
            key: widget.tileKey,
            behavior: HitTestBehavior.opaque,
            onTap: off ? null : widget.onTap,
            child: Opacity(
              opacity: off ? .6 : 1,
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 6),
                decoration: BoxDecoration(border: Border.all(color: _hover && !off ? C.a500 : C.line), color: _hover && !off ? C.a(C.a500, .1) : null),
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text(widget.name, style: T.mono(size: 11, color: C.text)),
                  Text(widget.note.toUpperCase(), style: T.mono(size: 9.5, color: C.text3)),
                  const SizedBox(height: 2),
                  costRow(widget.cost, size: 10.5),
                  if (off) Text(widget.why!, style: T.mono(size: 9.5, color: C.threat(5))),
                ]),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

String? _lvl(String name) {
  return RegExp(r'Lv\.(\d+)').firstMatch(name)?.group(1);
}

String _base(String name) => name.replaceAll(RegExp(r'\s*Lv\.\d+'), '');

class _BuildSlotA extends StatelessWidget {
  final Console con;
  const _BuildSlotA(this.con);

  @override
  Widget build(BuildContext context) {
    final s = con.state;
    final slots = con.economy.slots;
    final q = s.buildQueue.isEmpty ? null : s.buildQueue.first;
    final depth = slots?.depth ?? (q == null ? 0 : 1);
    final sub = slots == null ? null : 'queue $depth/${slots.maxDepth}';
    final act = OvActions(con);
    if (q != null) {
      final to = int.tryParse(_lvl(q.name) ?? '');
      return OvPanel(
        key: const Key('card-slot-a'),
        title: 'Build slot A',
        subText: sub,
        footer: Align(alignment: Alignment.centerLeft, child: ConsoleButton('Cancel', small: true, onPressed: () => act.cancel(proto.QueueType.building))),
        padding: const EdgeInsets.fromLTRB(14, 8, 14, 12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _ringRow(q.pct / 100, '${q.pct.round()}%', q.time, 'remaining'),
          const SizedBox(height: 10),
          _qTitle(_base(q.name)),
          _qSub(to == null ? 'in progress' : 'Level ${to - 1} -> $to'),
          if (slots != null) _queuedRows(slots.queued),
        ]),
      );
    }
    final choices = buildingChoices(s.catalog);
    return OvPanel(
      key: const Key('card-slot-a'),
      title: 'Build slot A',
      subText: sub,
      trailing: _idleBadge(),
      border: C.a500,
      padding: const EdgeInsets.fromLTRB(14, 8, 14, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        _qTitle('Queue an upgrade'),
        _qSub('Mines compound. An idle slot is lost production.'),
        if (choices.isEmpty) const WhyText('Nothing to build: every available building is maxed or locked.'),
        for (final o in choices)
          _SugTile(
            tileKey: Key('sug-build-${o.buildingType.name}'),
            name: '${o.buildingType.label} L${o.next!.level}',
            note: '${o.next!.ticks ~/ 60 > 0 ? '${o.next!.ticks ~/ 60}m' : '${o.next!.ticks}s'} build',
            cost: o.next!.cost,
            why: canAfford(s.stock, o.next!.cost) ? null : 'Short by ${shortBy(s.stock, o.next!.cost)}',
            onTap: () => act.build(o.buildingType),
          ),
      ]),
    );
  }
}

Widget _queuedRows(List<QueuedItemView> items) {
  if (items.isEmpty) return const SizedBox.shrink();
  return Container(
    decoration: const BoxDecoration(border: Border(top: BorderSide(color: C.line))),
    child: Column(children: [
      for (var i = 0; i < items.length; i++)
        Container(
          padding: const EdgeInsets.symmetric(vertical: 3),
          decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: C.lineLo))),
          child: Row(children: [
            SizedBox(width: 22, child: Text('Q${i + 2}', style: T.mono(size: 10, color: C.a600))),
            Expanded(child: Text(items[i].name.toUpperCase(), overflow: TextOverflow.ellipsis, style: T.mono(size: 10, color: C.text2))),
            Text(items[i].waits, style: T.mono(size: 10, color: C.text3)),
          ]),
        ),
    ]),
  );
}

class _BuildSlotB extends StatelessWidget {
  final Console con;
  const _BuildSlotB(this.con);

  @override
  Widget build(BuildContext context) {
    final slots = con.economy.slots;
    final act = OvActions(con);
    if (slots != null && slots.slotBUnlocked && slots.slotB != null) {
      final b = slots.slotB!;
      return OvPanel(
        key: const Key('card-slot-b'),
        title: 'Build slot B',
        padding: const EdgeInsets.fromLTRB(14, 8, 14, 12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _ringRow(b.frac, '${(b.frac * 100).round()}%', clockFmt(b.etaTicks), 'remaining'),
          const SizedBox(height: 10),
          _qTitle(b.name),
          _qSub('Level ${b.toLevel - 1} -> ${b.toLevel}'),
        ]),
      );
    }
    final why = slots?.slotBRequirement ?? 'Not available on this server yet';
    return Opacity(
      opacity: .85,
      child: OvPanel(
        key: const Key('card-slot-b'),
        title: 'Build slot B',
        subText: 'locked',
        padding: const EdgeInsets.fromLTRB(14, 14, 14, 14),
        child: SizedBox(
          width: double.infinity,
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            const Icon(Icons.lock_outline, size: 30, color: C.text3),
            const SizedBox(height: 8),
            _qTitle('Second build slot', color: C.text2),
            const SizedBox(height: 6),
            Text(why, textAlign: TextAlign.center, style: T.mono(size: 10.5, color: C.text3, height: 1.4)),
            const SizedBox(height: 10),
            ConsoleButton('View tech', small: true, onPressed: () => act.openTab(HomeworldTab.research)),
          ]),
        ),
      ),
    );
  }
}

class _ResearchCard extends StatelessWidget {
  final Console con;
  const _ResearchCard(this.con);

  @override
  Widget build(BuildContext context) {
    final s = con.state;
    final act = OvActions(con);
    final r = s.research;
    final slots = con.economy.slots;
    final sub = slots == null ? null : 'queue ${(r.name == 'Idle' ? 0 : 1) + slots.researchQueued.length}/${slots.maxDepth}';
    if (r.name != 'Idle') {
      final to = int.tryParse(_lvl(r.name) ?? '');
      return OvPanel(
        key: const Key('card-research'),
        title: 'Research',
        subText: sub,
        footer: Align(alignment: Alignment.centerLeft, child: ConsoleButton('Cancel', small: true, onPressed: () => act.cancel(proto.QueueType.research))),
        padding: const EdgeInsets.fromLTRB(14, 8, 14, 12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _ringRow(r.pct / 100, '${r.pct.round()}%', r.time, 'remaining'),
          const SizedBox(height: 10),
          _qTitle(_base(r.name)),
          _qSub(to == null ? 'in progress' : 'Level ${to - 1} -> $to'),
          if (slots != null) _queuedRows(slots.researchQueued),
        ]),
      );
    }
    final choices = researchChoices(s.catalog);
    return OvPanel(
      key: const Key('card-research'),
      title: 'Research',
      subText: sub,
      trailing: _idleBadge(),
      border: C.a500,
      padding: const EdgeInsets.fromLTRB(14, 8, 14, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        _qTitle('Nothing in the lab'),
        _qSub('Pick one. Grey = cannot afford yet.'),
        if (choices.isEmpty) const WhyText('No research is available: all maxed or locked.'),
        for (final o in choices)
          _SugTile(
            tileKey: Key('sug-research-${o.tech.name}'),
            name: '${o.tech.label} L${o.next!.level}',
            note: clockFmt(o.next!.ticks),
            cost: o.next!.cost,
            why: canAfford(s.stock, o.next!.cost) ? null : 'Short by ${shortBy(s.stock, o.next!.cost)}',
            onTap: () => act.research(o.tech),
          ),
      ]),
    );
  }
}

class _ShipyardCard extends StatelessWidget {
  final Console con;
  const _ShipyardCard(this.con);

  @override
  Widget build(BuildContext context) {
    final s = con.state;
    final act = OvActions(con);
    final slots = con.economy.slots;
    final q = s.shipyard.isEmpty ? null : s.shipyard.first;
    final sub = slots == null ? null : 'queue ${(q == null ? 0 : 1) + slots.shipQueued.length}/${slots.maxDepth}';
    if (q != null) {
      final m = RegExp(r'^(.*?) x(\d+) \((\d+) built\)').firstMatch(q.name);
      final label = m?.group(1) ?? q.name;
      final n = int.tryParse(m?.group(2) ?? '') ?? 0;
      final built = int.tryParse(m?.group(3) ?? '') ?? 0;
      final cls = proto.ShipClass.values.where((c) => c.label == label).firstOrNull;
      final batch = con.game.shipBatch;
      final cost = s.catalog?.ships.where((o) => o.shipClass == cls).firstOrNull?.unitCost;
      String? why;
      if (cls == null || cost == null) {
        why = 'Ship class not in the catalog';
      } else {
        final total = proto.Resources(metal: cost.metal * batch, crystal: cost.crystal * batch, deuterium: cost.deuterium * batch);
        if (!canAfford(s.stock, total)) why = 'Short by ${shortBy(s.stock, total)}';
      }
      return OvPanel(
        key: const Key('card-shipyard'),
        title: 'Shipyard',
        subText: sub,
        footer: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Wrap(spacing: 6, runSpacing: 4, children: [
            ConsoleButton('+$batch more', small: true, reason: why, onPressed: why == null ? () => act.ship(cls!, batch) : null),
            ConsoleButton('Cancel', small: true, onPressed: () => act.cancel(proto.QueueType.ship)),
          ]),
          if (why != null) WhyText(why),
        ]),
        padding: const EdgeInsets.fromLTRB(14, 8, 14, 12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _ringRow(n > 0 ? built / n : q.pct / 100, n > 0 ? '$built/$n' : '${q.pct.round()}%', q.time, n > built ? 'next ship ~${clockFmt(_secs(q.time) ~/ (n - built))}' : 'finishing'),
          const SizedBox(height: 10),
          _qTitle('${n}x $label'),
          _qSub('Ships join the dock on completion'),
          if (slots != null) _queuedRows(slots.shipQueued),
        ]),
      );
    }
    final choices = shipChoices(s.catalog);
    return OvPanel(
      key: const Key('card-shipyard'),
      title: 'Shipyard',
      subText: sub,
      trailing: _idleBadge(),
      border: C.a500,
      padding: const EdgeInsets.fromLTRB(14, 8, 14, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        _qTitle('Yard idle'),
        _qSub('Build ships for the dock'),
        if (choices.isEmpty) const WhyText('No ship class is unlocked yet. Upgrade the Shipyard.'),
        for (final o in choices)
          _SugTile(
            tileKey: Key('sug-ship-${o.shipClass.name}'),
            name: '${o.shipClass.label} x1',
            note: '${clockFmt(o.ticksPerShip)} each',
            cost: o.unitCost,
            why: canAfford(s.stock, o.unitCost) ? null : 'Short by ${shortBy(s.stock, o.unitCost)}',
            onTap: () => act.ship(o.shipClass, 1),
          ),
      ]),
    );
  }
}

int _secs(String t) {
  final p = t.split(':').map((e) => int.tryParse(e) ?? 0).toList();
  var v = 0;
  for (final x in p) {
    v = v * 60 + x;
  }
  return v;
}
