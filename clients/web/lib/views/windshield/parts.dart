import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/intel.dart';
import '../../console/threat.dart';
import '../../design/beat.dart';
import '../../design/gauges.dart';
import '../../design/icons.dart';
import '../../design/panel.dart';
import '../../design/symbols.dart';
import '../../design/draw.dart';
import '../../design/tokens.dart';
import '../../models/fleet.dart' as ui;
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;
import '../../scenes/windshield/geometry.dart';
import '../../state/game_controller.dart';

const voice = {
  proto.TerrainType.empty: 'Sector quiet. No signal above background.',
  proto.TerrainType.asteroidField: 'Rock field. Ore signatures logged.',
  proto.TerrainType.nebula: 'Ionised gas. Sensor range reduced.',
  proto.TerrainType.debrisField: 'Hull fragments. Salvage possible.',
  proto.TerrainType.anomaly: 'Instruments disagree. Readings unreliable.',
};

String zoneName(proto.Hex h) {
  final d = h.distFromOrigin;
  return d == 0 ? 'Central Hub' : d <= 8 ? 'Inner Ring' : d <= 20 ? 'Outer Ring' : 'The Wandering';
}

ui.ShipState? groupOf(ui.FleetState f, proto.ShipClass c) {
  for (final g in f.ships) {
    if (g.shipClass.toLowerCase() == c.label.toLowerCase()) return g;
  }
  return null;
}

double uiFleetPower(ui.FleetState f) {
  var p = 0.0;
  for (final g in f.ships) {
    final cls = proto.ShipClass.values.firstWhere((c) => c.label.toLowerCase() == g.shipClass.toLowerCase(), orElse: () => proto.ShipClass.scout);
    final st = shipStats[cls]!;
    p += g.count * st.weapon + (g.hull + g.count * st.shield) / 10;
  }
  return p;
}

// ── Sector head ────────────────────────────────────────────────

class SectorHead extends StatelessWidget {
  final proto.Hex loc;
  final proto.Hex home;
  final proto.SectorState? sector;
  final SectorThreat threat;
  final bool narrow;
  const SectorHead({super.key, required this.loc, required this.home, required this.sector, required this.threat, required this.narrow});

  @override
  Widget build(BuildContext context) {
    final atHome = loc == home;
    final fromHome = proto.Hex.distance(loc, home);
    final terr = sector?.terrain ?? proto.TerrainType.empty;
    final th = threat.rating;
    final coord = Text('${loc.q}, ${loc.r}',
        style: T.cond(size: narrow ? 22 : 36, color: C.a100, weight: FontWeight.w700, spacing: narrow ? 1.3 : 2.2, height: 1)
            .copyWith(shadows: const [Shadow(color: Color(0x4DFFB000), blurRadius: 12)]));
    final thrLine = Row(mainAxisSize: MainAxisSize.min, children: [
      const Lbl('Threat'),
      const SizedBox(width: 10),
      ThreatBadge(th),
      const SizedBox(width: 10),
      Text(th == 0 ? 'CLEAR' : '', style: T.cond(size: 11, color: C.threat(th), spacing: 1.5)),
    ]);
    return IgnorePointer(
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
        Lbl(atHome ? 'Homeworld orbit' : 'Sector'),
        const SizedBox(height: 3),
        coord,
        if (!narrow) ...[
          const SizedBox(height: 8),
          Kv('Zone', v: zoneName(loc).toUpperCase(), size: 11),
          Kv('Terrain', v: (sector?.terrain.label ?? 'Unknown').toUpperCase(), size: 11),
          Kv('Range', v: '${loc.distFromOrigin} FROM HUB - ${fromHome == 0 ? 'AT HOME' : '$fromHome FROM HOME'}', size: 11),
        ],
        Padding(padding: EdgeInsets.only(top: narrow ? 3 : 8, bottom: 4), child: thrLine),
        if (!narrow)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text.rich(
              TextSpan(children: [
                TextSpan(text: 'NOTE ', style: T.mono(size: 11, color: C.a700, weight: FontWeight.w600, spacing: 1)),
                TextSpan(text: voice[terr]),
              ]),
              style: T.mono(size: 11, color: C.text3, height: 1.45),
            ),
          ),
      ]),
    );
  }
}

// ── Fleet rail ─────────────────────────────────────────────────

class FleetRail extends StatelessWidget {
  final GameController ctrl;
  final bool narrow;
  final bool combat;
  const FleetRail({super.key, required this.ctrl, required this.narrow, required this.combat});

  @override
  Widget build(BuildContext context) {
    final fleets = ctrl.state.fleets;
    final cards = [
      for (var i = 0; i < fleets.length; i++) _card(i, fleets[i]),
    ];
    if (narrow) {
      return Row(children: [for (final c in cards) Expanded(child: Padding(padding: const EdgeInsets.only(right: 4), child: c))]);
    }
    return Column(mainAxisSize: MainAxisSize.min, children: [for (final c in cards) Padding(padding: const EdgeInsets.only(bottom: 6), child: c)]);
  }

  Widget _card(int i, ui.FleetState f) {
    final on = i == ctrl.activeFleet;
    final hops = f.jumpFuel > 0 ? f.fuel ~/ f.jumpFuel : f.fuel;
    final pol = f.policy == null || f.policy == 'manual' ? 'manual' : f.policy!;
    return Hover(
      builder: (hv) => GestureDetector(
        key: ValueKey('fleet-card-${f.id}'),
        behavior: HitTestBehavior.opaque,
        onTap: () => ctrl.selectFleet(i),
        child: ConsolePanel(
          mark: on ? C.own : null,
          border: on ? C.own : (hv ? C.lineHi : null),
          fill: on ? const Color(0xF20D1816) : (hv ? const Color(0xF2191208) : null),
          padding: EdgeInsets.fromLTRB(narrow ? 6 : 10, narrow ? 5 : 7, narrow ? 6 : 10, narrow ? 5 : 6),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
            Row(children: [
              if (!narrow) ...[
                SymIcon((c, s) => Draw.symbol(c, Sym.fleet, s.width / 2, s.height / 2, 6, color: on ? C.own : C.a200, lw: 1.6, bloom: false), size: 12),
                const SizedBox(width: 7),
              ],
              Expanded(child: Text(f.name.toUpperCase(), overflow: TextOverflow.ellipsis, style: T.cond(size: narrow ? 10.5 : 12.5, color: on ? C.own : C.a200, spacing: narrow ? .5 : 1.5))),
              if (!narrow) KeyCap('${i + 1}', size: 15),
            ]),
            if (!narrow) ...[
              const SizedBox(height: 3),
              Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                Text('${f.status.label}${on && combat ? ' / COMBAT' : ''}', style: T.mono(size: 10, color: C.text2, spacing: .4)),
                Text('${f.sector.q},${f.sector.r}', style: T.mono(size: 10, color: C.text3)),
              ]),
              const SizedBox(height: 5),
            ] else
              const SizedBox(height: 3),
            LedBar(f.fuelMax > 0 ? f.fuel / f.fuelMax : 0, color: hops < 4 ? C.ember : C.own, height: 7),
            if (!narrow)
              Padding(
                  padding: const EdgeInsets.only(top: 2),
                  child: Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                    Text('$hops HOPS', style: T.mono(size: 10, color: C.text3)),
                    Text(pol.toUpperCase(), style: T.mono(size: 10, color: C.text3)),
                  ])),
          ]),
        ),
      ),
    );
  }
}

class Hover extends StatefulWidget {
  final Widget Function(bool hover) builder;
  const Hover({super.key, required this.builder});
  @override
  State<Hover> createState() => _HoverState();
}

class _HoverState extends State<Hover> {
  bool h = false;
  @override
  Widget build(BuildContext context) => MouseRegion(cursor: SystemMouseCursors.click, onEnter: (_) => setState(() => h = true), onExit: (_) => setState(() => h = false), child: widget.builder(h));
}

// ── Status panel ───────────────────────────────────────────────

class StatusPanel extends StatelessWidget {
  final ui.FleetState f;
  final double shieldFrac;
  final Widget? footer;
  const StatusPanel({super.key, required this.f, required this.shieldFrac, this.footer});

  @override
  Widget build(BuildContext context) {
    var hull = 0, hm = 0;
    for (final g in f.ships) {
      hull += g.hull;
      hm += g.hullMax;
    }
    final hullFrac = hm > 0 ? hull / hm : 0.0;
    final hops = f.jumpFuel > 0 ? f.fuel ~/ f.jumpFuel : f.fuel;
    final need = f.homeFuel;
    final scanRange = f.ships.any((g) => g.shipClass.toLowerCase() == 'scout') ? 2 : 1;
    final cargo = f.cargo.total;
    final pol = f.policy == null ? 'MANUAL' : f.policy!.toUpperCase();
    return ConsolePanel(
      title: 'Fleet status',
      subText: '${f.name} / ${f.shipCount} ships',
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 6, 4, 0),
          child: Row(mainAxisAlignment: MainAxisAlignment.spaceAround, children: [
            Dial(frac: f.fuelMax > 0 ? f.fuel / f.fuelMax : 0, warn: .2, label: 'FUEL', value: '${f.fuel}/${f.fuelMax}'),
            Dial(frac: hullFrac, warn: .3, label: 'HULL', value: '${(hullFrac * 100).round()}%', color: C.own),
          ]),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 2, 12, 10),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            const SizedBox(height: 7),
            Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
              const Lbl('Cargo', size: 10),
              Row(mainAxisSize: MainAxisSize.min, children: [Register(cargo, width: 3, size: 11, color: C.text), Text('/${f.cargo.capacity}', style: T.mono(size: 11, color: C.text3))]),
            ]),
            const SizedBox(height: 3),
            LedBar(f.cargo.fraction, color: C.own),
            const SizedBox(height: 5),
            Text.rich(TextSpan(children: [
              TextSpan(text: 'Fe ${f.cargo.metal}', style: const TextStyle(color: C.metal)),
              const TextSpan(text: '  '),
              TextSpan(text: 'Cr ${f.cargo.crystal}', style: const TextStyle(color: C.crystal)),
              const TextSpan(text: '  '),
              TextSpan(text: 'De ${f.cargo.deut}', style: const TextStyle(color: C.deut)),
            ]), style: T.mono(size: 10)),
            const SizedBox(height: 7),
            Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
              const Lbl('Shield', size: 10),
              Text('${(shieldFrac * 100).round()}%', style: T.mono(size: 11)),
            ]),
            const SizedBox(height: 3),
            LedBar(shieldFrac, color: C.crystal),
            const SizedBox(height: 8),
            Kv('Hops / home needs', v: '$hops / ${need}F${need > 0 && f.fuel < need * 1.2 ? '  LOW' : ''}', vc: need > 0 && f.fuel < need * 1.2 ? C.threat(6) : null),
            Kv('Fleet power', v: uiFleetPower(f).round().toString()),
            Kv('Scan range', v: '$scanRange'),
            Kv('Orders', v: pol, vc: C.a300),
            if (footer != null) Padding(padding: const EdgeInsets.only(top: 10), child: Align(alignment: Alignment.centerLeft, child: footer)),
          ]),
        ),
      ]),
    );
  }
}

// ── Contacts ───────────────────────────────────────────────────

class ContactsPanel extends StatelessWidget {
  final SectorThreat threat;
  final double myPower;
  final bool combat;
  final bool atHome;
  final proto.SectorState? sector;
  final ui.FleetState fleet;
  final bool hasOre;
  final bool hasSalvage;
  final bool hasDerelict;
  final bool harvesting;
  final bool boarding;
  final String? target;
  final void Function(String k) onTarget;
  final void Function(String k) onHover;
  final VoidCallback onHoverEnd;
  final void Function(String act) onAct;
  final VoidCallback onOpenHome;
  const ContactsPanel({
    super.key,
    required this.threat,
    required this.myPower,
    required this.combat,
    required this.atHome,
    required this.sector,
    required this.fleet,
    required this.hasOre,
    required this.hasSalvage,
    required this.hasDerelict,
    required this.harvesting,
    required this.boarding,
    required this.target,
    required this.onTarget,
    required this.onHover,
    required this.onHoverEnd,
    required this.onAct,
    required this.onOpenHome,
  });

  Widget _row(String k, Widget icon, Widget body, {Widget? actions}) {
    final sel = target == k;
    return Hover(
      builder: (hv) => GestureDetector(
        key: ValueKey('contact-$k'),
        behavior: HitTestBehavior.opaque,
        onTap: () => onTarget(k),
        child: MouseRegion(
          onEnter: (_) => onHover(k),
          onExit: (_) => onHoverEnd(),
          child: Container(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
            decoration: BoxDecoration(
              color: sel ? C.a(C.a500, .12) : (hv ? C.a(C.a500, .07) : null),
              border: Border(top: const BorderSide(color: C.lineLo), left: sel ? const BorderSide(color: C.a500, width: 3) : BorderSide.none),
            ),
            child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              SizedBox(width: 28, child: Center(child: icon)),
              const SizedBox(width: 10),
              Expanded(child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [body, if (sel && actions != null) Padding(padding: const EdgeInsets.only(top: 7), child: actions)])),
            ]),
          ),
        ),
      ),
    );
  }

  Widget _t(String s, {Widget? trail}) => Row(children: [Flexible(child: Text(s.toUpperCase(), style: T.cond(size: 12.5, color: C.text, spacing: 1.2))), if (trail != null) ...[const SizedBox(width: 6), trail]]);
  Widget _d(Widget w) => Padding(padding: const EdgeInsets.only(top: 3), child: w);
  TextStyle get _ds => T.mono(size: 10.5, color: C.text3, height: 1.5, spacing: .2);

  @override
  Widget build(BuildContext context) {
    final rows = <Widget>[];
    if (threat.hostile) {
      final o = RatioInfo.of(myPower, threat.power);
      final th = threat.rating;
      rows.add(_row(
        'h',
        _Diamond(th),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t(threat.composition.map((e) => '${e.$2}x ${shipAbbr(e.$1)}').join(' + '), trail: ThreatBadge(th, sm: true)),
          _d(Text('${threat.behavior?.wire.toUpperCase() ?? ''} - POWER ${threat.power.round()} VS YOUR ${myPower.round()}${combat ? ' - ENGAGED' : ''}', style: _ds.copyWith(color: combat ? C.threat(6) : C.text3))),
          Padding(padding: const EdgeInsets.only(top: 6), child: Text.rich(TextSpan(children: [
            TextSpan(text: o.label, style: TextStyle(color: o.color, fontWeight: FontWeight.w600)),
            TextSpan(text: '  ratio ${o.r.toStringAsFixed(2)} - est win ${o.winPct}%', style: const TextStyle(color: C.text3)),
          ]), style: T.mono(size: 10.5))),
          RatioGauge(o.r),
        ]),
        actions: ConsoleButton('Fire', small: true, danger: true, keyHint: 'F', onPressed: () => onAct('fire')),
      ));
    }
    if (hasOre) {
      final s = sector;
      final dens = {'metal': s?.resources.metal.index ?? 0, 'crystal': s?.resources.crystal.index ?? 0, 'deuterium': s?.resources.deuterium.index ?? 0};
      final free = fleet.cargo.capacity - fleet.cargo.total;
      rows.add(_row(
        'o',
        SymIcon((c, sz) => Draw.symbol(c, Sym.ore, sz.width / 2, sz.height / 2, 12, color: C.metal, bloom: false), size: 24),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t('Ore deposits'),
          _d(Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Wrap(spacing: 10, children: [
              for (final e in dens.entries)
                if (e.value > 0)
                  Text.rich(TextSpan(children: [
                    TextSpan(text: '${e.key == 'metal' ? 'Fe' : e.key == 'crystal' ? 'Cr' : 'De'} '.toUpperCase()),
                    TextSpan(text: '|' * e.value),
                    TextSpan(text: '|' * (4 - e.value), style: TextStyle(color: C.ore(e.key).withValues(alpha: .25))),
                  ]), style: _ds.copyWith(color: C.ore(e.key))),
            ]),
            Text(harvesting ? 'HARVESTING' : 'IDLE - HOLD FREE $free', style: _ds.copyWith(color: harvesting ? C.threat(1) : C.text3)),
          ])),
        ]),
        actions: ConsoleButton('Harvest', small: true, keyHint: 'H', onPressed: () => onAct('harvest')),
      ));
    }
    if (hasSalvage) {
      final r = sector?.salvage;
      rows.add(_row(
        's',
        SymIcon((c, sz) => Draw.symbol(c, Sym.salvage, sz.width / 2, sz.height / 2, 12, color: C.a400, bloom: false), size: 24),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t('Salvage pile'),
          _d(Text.rich(TextSpan(children: [
            TextSpan(text: 'Fe ${r?.metal.round() ?? 0} ', style: const TextStyle(color: C.metal)),
            TextSpan(text: 'Cr ${r?.crystal.round() ?? 0} ', style: const TextStyle(color: C.crystal)),
            TextSpan(text: 'De ${r?.deuterium.round() ?? 0}', style: const TextStyle(color: C.deut)),
          ]), style: _ds)),
        ]),
        actions: ConsoleButton('Collect', small: true, keyHint: 'C', onPressed: () => onAct('collect')),
      ));
    }
    if (hasDerelict) {
      final d = sector?.site;
      final rc = d?.risk == proto.SiteRisk.hot ? C.ember : d?.risk == proto.SiteRisk.uneasy ? C.threat(5) : C.ok;
      rows.add(_row(
        'd',
        SymIcon((c, sz) => Draw.symbol(c, Sym.derelict, sz.width / 2, sz.height / 2, 13, color: C.rare, bloom: false), size: 26),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t('Derelict hulk - tier ${d?.tier ?? 1}'),
          _d(Text.rich(TextSpan(children: [
            const TextSpan(text: 'BOARDING RISK: '),
            TextSpan(text: (d?.risk.label ?? '').toUpperCase(), style: TextStyle(color: rc, fontWeight: FontWeight.w600)),
            TextSpan(text: boarding ? '\nBOARDING PARTY ABOARD...' : '\nLOOT SCALES WITH DISTANCE'),
          ]), style: _ds)),
        ]),
        actions: ConsoleButton('Board', small: true, keyHint: 'B', onPressed: () => onAct('board')),
      ));
    }
    if (atHome) {
      rows.add(_row(
        'home',
        SymIcon((c, sz) => Draw.symbol(c, Sym.home, sz.width / 2, sz.height / 2, 15, color: C.own, lw: 1.2, bloom: false), size: 30),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t('Homeworld'),
          _d(Text('DOCKED FLEETS ARE SAFE.', style: _ds)),
          Padding(padding: const EdgeInsets.only(top: 7), child: ConsoleButton('Open homeworld', small: true, onPressed: onOpenHome)),
        ]),
      ));
    }
    if (rows.isEmpty) {
      rows.add(Padding(padding: const EdgeInsets.all(12), child: Text('NO CONTACTS. SCAN TO LOOK BEYOND THE GATES.', style: _ds)));
    }
    final n = rows.length;
    return ConsolePanel(
      title: 'Contacts',
      subText: '$n signal${n == 1 ? '' : 's'}',
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: rows),
    );
  }
}

class _Diamond extends StatelessWidget {
  final int t;
  const _Diamond(this.t);
  @override
  Widget build(BuildContext context) {
    final col = C.threat(t);
    return SizedBox(
      width: 28,
      height: 28,
      child: Stack(alignment: Alignment.center, children: [
        Transform.rotate(angle: math.pi / 4, child: Container(width: 20, height: 20, decoration: BoxDecoration(border: Border.all(color: col, width: 1.4)))),
        Text('$t', style: T.cond(size: 13, color: col, weight: FontWeight.w700)),
      ]),
    );
  }
}

// ── Fleet management ───────────────────────────────────────────

class FleetManagePanel extends StatefulWidget {
  final GameController controller;
  const FleetManagePanel({super.key, required this.controller});
  @override
  State<FleetManagePanel> createState() => _FleetManagePanelState();
}

class _FleetManagePanelState extends State<FleetManagePanel> {
  final Map<String, int> _pick = {};
  int? _forFleet;
  GameController get c => widget.controller;

  @override
  Widget build(BuildContext context) {
    final fleet = c.currentFleet;
    if (_forFleet != fleet.id) {
      _forFleet = fleet.id;
      _pick.clear();
    }
    for (final g in fleet.ships) {
      _pick.update(g.shipClass, (n) => n.clamp(0, g.count), ifAbsent: () => 0);
    }
    final picked = _pick.values.fold<int>(0, (a, b) => a + b);
    final canSplit = picked > 0 && picked < fleet.shipCount;
    final partners = c.mergeCandidates;
    return ConsolePanel(
      title: 'Fleet management',
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        const Lbl('Split off'),
        for (final g in fleet.ships) _pickRow(g),
        const SizedBox(height: 8),
        Align(
          alignment: Alignment.centerLeft,
          child: ConsoleButton(picked == 0 ? 'Split off' : 'Split off $picked',
              key: const ValueKey('fm-split'),
              small: true,
              primary: canSplit,
              onPressed: canSplit ? () => _split(fleet) : null,
              reason: picked == 0 ? 'Pick ships with + first' : 'One ship must stay in the fleet'),
        ),
        if (!canSplit) WhyText(picked == 0 ? 'Pick ships with + first.' : 'One ship must stay in the fleet.'),
        const SizedBox(height: 10),
        const Lbl('Merge in (same sector)'),
        if (partners.isEmpty) const WhyText('No other fleet here.') else for (final p in partners) _mergeRow(p),
      ]),
    );
  }

  Widget _pickRow(ui.ShipState g) {
    final n = _pick[g.shipClass] ?? 0;
    return Padding(
      padding: const EdgeInsets.only(top: 4),
      child: Row(children: [
        Expanded(child: Text('${g.shipClass.toUpperCase()} x${g.count}', style: T.mono(size: 11, color: C.text2))),
        ConsoleButton('-', key: ValueKey('fm-minus-${g.shipClass}'), small: true, onPressed: n > 0 ? () => setState(() => _pick[g.shipClass] = n - 1) : null, reason: 'None picked'),
        SizedBox(width: 26, child: Text('$n', textAlign: TextAlign.center, style: T.mono(size: 11, color: C.a300))),
        ConsoleButton('+', key: ValueKey('fm-plus-${g.shipClass}'), small: true, onPressed: n < g.count ? () => setState(() => _pick[g.shipClass] = n + 1) : null, reason: 'All picked'),
      ]),
    );
  }

  Widget _mergeRow(ui.FleetState p) => Padding(
        padding: const EdgeInsets.only(top: 4),
        child: Row(children: [
          Expanded(child: Text('${p.name.toUpperCase()} - ${p.shipCount} SHIP${p.shipCount == 1 ? '' : 'S'}', style: T.mono(size: 11, color: C.text2))),
          ConsoleButton('Merge', key: ValueKey('fm-merge-${p.id}'), small: true, onPressed: () => c.mergeFleet(p.id)),
        ]),
      );

  void _split(ui.FleetState fleet) {
    final ids = <int>[];
    for (final g in fleet.ships) {
      ids.addAll(g.ids.take(_pick[g.shipClass] ?? 0));
    }
    setState(_pick.clear);
    c.splitShipIds(ids);
  }
}

// ── Teleprinter ────────────────────────────────────────────────

class Feed extends StatelessWidget {
  final List<LogEntry> events;
  const Feed({super.key, required this.events});

  Color _col(String m) {
    if (m.startsWith('!') || m.contains('LOST') || m.contains('destroyed') && m.contains('Your')) return const Color(0xFFFF9A86);
    if (m.contains('harvested') || m.contains('collected') || m.contains('complete') || m.contains('ready') || m.contains('arrived') || m.contains('entered')) return const Color(0xFF8EEADF);
    if (m.contains('derelict') || m.contains('first visit')) return const Color(0xFFCDB7FF);
    return C.text2;
  }

  @override
  Widget build(BuildContext context) {
    final recent = events.take(7).toList().reversed.toList();
    final seen = <String, int>{};
    return IgnorePointer(
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        for (var i = 0; i < recent.length; i++)
          () {
            final e = recent[i];
            final k = '${e.tick}|${e.message}';
            final n = seen[k] = (seen[k] ?? 0) + 1;
            return _TypeLine(key: ValueKey('$k#$n'), entry: e, color: _col(e.message), dim: i < recent.length - 7 + 0 && recent.length > 6 && i == 0 ? true : false, last: i == recent.length - 1);
          }(),
      ]),
    );
  }
}

class _TypeLine extends StatefulWidget {
  final LogEntry entry;
  final Color color;
  final bool dim, last;
  const _TypeLine({super.key, required this.entry, required this.color, required this.dim, required this.last});
  @override
  State<_TypeLine> createState() => _TypeLineState();
}

class _TypeLineState extends State<_TypeLine> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(vsync: this, duration: const Duration(milliseconds: 400))..forward();
  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final still = reducedMotion(context);
    final line = Container(
      margin: const EdgeInsets.only(top: 1),
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
      color: const Color(0x8C07050A),
      child: Text.rich(
        TextSpan(children: [
          const TextSpan(text: '> ', style: TextStyle(color: C.a600)),
          TextSpan(text: '${(widget.entry.tick % 100000).toString().padLeft(5, '0')}  ', style: const TextStyle(color: C.text4)),
          TextSpan(text: widget.entry.message.toUpperCase().replaceFirst(RegExp(r'^!+\s*'), ''), style: TextStyle(color: widget.color)),
          if (widget.last) WidgetSpan(alignment: PlaceholderAlignment.middle, child: Blink(period: 11, onSteps: 6, builder: (_, on) => Container(width: 6, height: 10, margin: const EdgeInsets.only(left: 6), color: on ? C.a400 : Colors.transparent))),
        ]),
        style: T.mono(size: 10.5, color: C.text2, height: 1.35, spacing: .2),
      ),
    );
    if (still) return line;
    return AnimatedBuilder(animation: _c, builder: (_, child) => ClipRect(child: Align(alignment: Alignment.centerLeft, widthFactor: ((_c.value * 26).floor() / 26).clamp(.02, 1.0), child: child)), child: line);
  }
}

// ── Gate cards ─────────────────────────────────────────────────

class GateCard extends StatelessWidget {
  final Dir dir;
  final bool narrow;
  final bool known;
  final proto.SectorState? sector;
  final SectorThreat threat;
  final Intel intel;
  final int fuel;
  final bool isHome;
  final bool hint;
  final bool hover;
  final VoidCallback onTap;
  final void Function(bool) onHover;
  const GateCard({super.key, required this.dir, required this.narrow, required this.known, required this.sector, required this.threat, required this.intel, required this.fuel, required this.isHome, required this.hint, required this.hover, required this.onTap, required this.onHover});

  @override
  Widget build(BuildContext context) {
    final dashed = !known || intel.aged;
    final w = narrow ? 92.0 : 150.0;
    final th = known ? threat.rating : 0;
    final hot = th >= 5;
    final terr = known ? (sector?.terrain.label.split(' ').first ?? '') : 'Uncharted';
    final top = Row(children: [
      KeyCap(dir.key, size: 15),
      const SizedBox(width: 6),
      Expanded(child: Text(terr.toUpperCase(), overflow: TextOverflow.ellipsis, style: T.cond(size: narrow ? 9.5 : 10.5, color: !known ? C.text3 : intel.aged ? C.fossil : C.a200, spacing: narrow ? .6 : .9))),
      if (isHome && !narrow) ...[
        const SizedBox(width: 5),
        Container(padding: const EdgeInsets.symmetric(horizontal: 4), decoration: BoxDecoration(border: Border.all(color: C.ownDim)), child: Text('HOME', style: T.mono(size: 9, color: C.own, spacing: 1))),
      ],
      if (!narrow) ...[Text(dir.bearing, style: T.mono(size: 9, color: C.text3, spacing: .4))],
    ]);
    final lineStyle = T.mono(size: 10, color: C.text2, height: 1.25, spacing: .3);
    final body = <Widget>[Padding(padding: EdgeInsets.fromLTRB(6, 4, 6, narrow ? 3 : 3), child: top)];
    if (!narrow) {
      body.add(Container(height: 1, color: C.lineLo));
      if (!known) {
        body.add(Padding(padding: const EdgeInsets.fromLTRB(6, 3, 6, 0), child: Text(hint ? 'FAINT MASS READING' : 'SCAN TO REVEAL', style: T.mono(size: 9.5, color: hint ? C.threat(6) : C.text3))));
        body.add(Padding(padding: const EdgeInsets.fromLTRB(6, 3, 6, 4), child: Text('${dir.name} - -${fuel}F', style: T.mono(size: 9.5, color: C.text3))));
      } else {
        final dens = [sector?.resources.metal.index ?? 0, sector?.resources.crystal.index ?? 0, sector?.resources.deuterium.index ?? 0];
        body.add(Padding(
          padding: const EdgeInsets.fromLTRB(6, 3, 6, 0),
          child: Row(children: [
            Expanded(child: th > 0
                ? Text.rich(TextSpan(children: [TextSpan(text: 'T$th ', style: TextStyle(color: C.threat(th), fontWeight: FontWeight.w600)), TextSpan(text: threat.compLabel)]), overflow: TextOverflow.ellipsis, style: lineStyle)
                : Text('NO CONTACT', style: lineStyle)),
            SizedBox(
              height: 10,
              child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
                for (var i = 0; i < 3; i++)
                  Container(width: 4, height: dens[i] > 0 ? 2.0 + dens[i] * 2 : 1, margin: const EdgeInsets.only(left: 2), color: C.ore(['metal', 'crystal', 'deut'][i]).withValues(alpha: dens[i] > 0 ? 1 : .3)),
              ]),
            ),
          ]),
        ));
        final sv = sector?.salvage;
        body.add(Padding(
          padding: const EdgeInsets.fromLTRB(6, 3, 6, 4),
          child: Row(children: [
            if (intel.live) const Lamp('LIVE', color: C.own, blink: true) else Text('SEEN ${ago(intel.ticksOld).toUpperCase()} AGO', style: T.mono(size: 9.5, color: intel.aged ? C.fossil : C.text3)),
            if (sv != null && sv.total > 0) Text(' +SALV', style: T.mono(size: 9.5, color: C.a400)),
            if (sector?.site != null) Text(' +DRLCT', style: T.mono(size: 9.5, color: C.rare)),
            const Spacer(),
            Text('-${fuel}F', style: T.mono(size: 9.5, color: C.text3)),
          ]),
        ));
      }
    } else {
      body.add(const SizedBox(height: 0));
    }
    final border = hover ? C.a400 : (hot && known ? C.threat(th).withValues(alpha: .7) : C.line);
    Widget card(Color b) => CustomPaint(
          foregroundPainter: _BoxBorder(b, dashed),
          child: Container(width: w, color: hover ? const Color(0xF5281C0A) : (known ? const Color(0xF0090604) : const Color(0xCC070504)), child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: body)),
        );
    return Semantics(
      button: true,
      label: 'Jump ${dir.name}, ${known ? terr : 'uncharted'}${th > 0 ? ', threat $th' : ''}',
      excludeSemantics: true,
      onTap: onTap,
      child: MouseRegion(
        cursor: SystemMouseCursors.click,
        onEnter: (_) => onHover(true),
        onExit: (_) => onHover(false),
        child: GestureDetector(
          key: ValueKey('gate-${dir.key}'),
          behavior: HitTestBehavior.opaque,
          onTap: onTap,
          child: hot && known ? Blink(period: 13, onSteps: 7, builder: (_, on) => card(on ? border : border.withValues(alpha: .3))) : card(border),
        ),
      ),
    );
  }
}

class _BoxBorder extends CustomPainter {
  final Color color;
  final bool dashed;
  _BoxBorder(this.color, this.dashed);
  @override
  void paint(Canvas canvas, Size s) {
    final r = Rect.fromLTWH(.5, .5, s.width - 1, s.height - 1);
    final p = Paint()
      ..style = PaintingStyle.stroke
      ..color = color;
    if (dashed) {
      Draw.dashedPath(canvas, Path()..addRect(r), p, 4, 3);
    } else {
      canvas.drawRect(r, p);
    }
  }

  @override
  bool shouldRepaint(_BoxBorder o) => o.color != color || o.dashed != dashed;
}

// ── Action dock ────────────────────────────────────────────────

class DockAction {
  final String id, label, key;
  final String? reason;
  final bool active;
  final bool fire;
  const DockAction(this.id, this.label, this.key, {this.reason, this.active = false, this.fire = false});
  bool get ok => reason == null;
}

class Dock extends StatelessWidget {
  final List<DockAction> actions;
  final bool narrow;
  final ValueNotifier<double> scanCooldown;
  final ValueNotifier<int> scanSecs;
  final void Function(String) onAct;
  final void Function(String?) onHint;
  const Dock({super.key, required this.actions, required this.narrow, required this.scanCooldown, required this.scanSecs, required this.onAct, required this.onHint});

  @override
  Widget build(BuildContext context) {
    final tiles = [for (final a in actions) _tile(a)];
    return ConsolePanel(
      child: narrow ? Row(children: [for (final t in tiles) Expanded(child: t)]) : Row(mainAxisSize: MainAxisSize.min, children: tiles),
    );
  }

  Widget _tile(DockAction a) {
    return Hover(
      builder: (hv) {
        final on = hv && a.ok;
        final col = !a.ok ? C.a300.withValues(alpha: .28) : on ? const Color(0xFF1C1000) : a.active ? C.own : a.fire ? C.ember : C.a300;
        final bg = on ? (a.fire ? C.ember : C.a500) : a.active ? C.a(C.own, .14) : null;
        return Semantics(
          button: true,
          enabled: a.ok,
          label: a.label,
          hint: a.reason,
          excludeSemantics: true,
          onTap: a.ok ? () => onAct(a.id) : null,
          child: Tooltip(
            message: a.reason ?? a.label,
            child: MouseRegion(
              onEnter: (_) => onHint(a.reason),
              onExit: (_) => onHint(null),
              child: GestureDetector(
                key: ValueKey('dock-${a.id}'),
                behavior: HitTestBehavior.opaque,
                onTap: a.ok ? () => onAct(a.id) : () => onHint(a.reason),
                child: Container(
                  width: narrow ? null : 76,
                  height: narrow ? 52 : 60,
                  decoration: BoxDecoration(color: bg, border: Border(right: const BorderSide(color: C.lineLo), bottom: a.active && !on ? const BorderSide(color: C.own, width: 2) : BorderSide.none)),
                  child: Stack(alignment: Alignment.center, children: [
                    Column(mainAxisAlignment: MainAxisAlignment.center, children: [
                      LineIcon(a.id, size: 20, color: col),
                      const SizedBox(height: 4),
                      Text(a.label.toUpperCase(), style: T.cond(size: narrow ? 8 : 10, color: col, spacing: narrow ? .3 : 1.6)),
                    ]),
                    if (!narrow) Positioned(top: 3, right: 3, child: KeyCap(a.key, color: col, size: 14)),
                    if (a.id == 'scan')
                      ValueListenableBuilder<double>(
                        valueListenable: scanCooldown,
                        builder: (_, v, _) => v <= 0
                            ? const SizedBox.shrink()
                            : Positioned.fill(
                                child: Stack(children: [
                                  Align(alignment: Alignment.bottomCenter, child: FractionallySizedBox(heightFactor: v, widthFactor: 1, child: Container(color: const Color(0xB8000000)))),
                                  Center(child: ValueListenableBuilder<int>(valueListenable: scanSecs, builder: (_, s, _) => Text('$s', style: T.mono(size: 16, color: C.a100, weight: FontWeight.w500)))),
                                ]),
                              ),
                      ),
                  ]),
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}
