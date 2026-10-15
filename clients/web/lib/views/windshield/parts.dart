import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../console/battle.dart';
import '../../console/intel.dart';
import '../../console/outpost.dart';
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
import '../battle_view.dart';
import '../onboarding.dart';

const voice = {
  proto.TerrainType.empty: 'Sector quiet. No signal above background.',
  proto.TerrainType.asteroidField: 'Rock field. Ore signatures logged.',
  proto.TerrainType.nebula: 'Ionised gas. Sensor range reduced.',
  proto.TerrainType.debrisField: 'Hull fragments. Salvage possible.',
  proto.TerrainType.anomaly: 'Instruments disagree. Readings unreliable.',
};

const hubVoice = 'Neutral post. No ore, no hostiles. The exchange is open.';

const outpostVoice = 'A station holds this sector. Raiders come for it.';

/// What the dock does for a fleet and what it does not: nothing in the field
/// reaches a docked fleet, a raid does, and the fleet is in its fight.
const dockVoice = 'A DOCKED FLEET REFUELS, UNLOADS, IS MENDED AND REFITTED, AND NOTHING IN THE FIELD REACHES IT.\n'
    'A RAID IS FOUGHT HERE: A FLEET DOCKED WHEN ONE ARRIVES IS IN THE FIGHT.';
const dockVoiceLaden = 'A DOCKED FLEET REFUELS, IS MENDED AND REFITTED, AND NOTHING IN THE FIELD REACHES IT.\n'
    'HOLD LADEN: IT STAYS ABOARD UNTIL THE FLEET HAS BEEN AWAY.\n'
    'A RAID IS FOUGHT HERE: A FLEET DOCKED WHEN ONE ARRIVES IS IN THE FIGHT, AND WHAT A SHIP LOST IN IT CARRIED FALLS TO THE STOCKPILE.';

/// What refining is, for the control that orders it while it is greyed.
const refineRule = 'Refining turns the deuterium in the hold into fuel, ${proto.refineLossPct}% less of it than a dock gives. '
    'It takes deuterium aboard and room in the tank, and is not done in a fight or while a jump charges.';

String _tenths(double v) {
  final t = (v * 10).round() / 10;
  return t == t.roundToDouble() ? t.toStringAsFixed(0) : t.toStringAsFixed(1);
}

/// "20 DE MAKES 212.5 FUEL": what the server says a refine order would do.
String refineLine(proto.Refining r) => '${_tenths(r.deuterium)} DE MAKES ${_tenths(r.fuel)} FUEL';

// ── Sector head ────────────────────────────────────────────────

class SectorHead extends StatelessWidget {
  final proto.Hex loc;
  final proto.Hex home;
  final proto.SectorState? sector;
  final SectorThreat threat;
  final bool narrow;

  /// The sector is a hub sector with the exchange.
  final bool exchange;

  /// The outpost standing in the sector, whoever owns it.
  final proto.OutpostBrief? outpost;

  /// The most the head takes of the phone layout, where it has no rows:
  /// the coordinates and the threat line.
  static const phoneWidth = 250.0;
  const SectorHead(
      {super.key, required this.loc, required this.home, required this.sector, required this.threat, required this.narrow, this.exchange = false, this.outpost});

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
        Lbl(atHome
            ? 'Homeworld orbit'
            : exchange
                ? 'The hub - exchange'
                : outpost == null
                    ? 'Sector'
                    : outpost!.mine
                        ? 'Your outpost'
                        : 'Outpost of ${outpost!.owner}'),
        const SizedBox(height: 3),
        coord,
        if (!narrow) ...[
          const SizedBox(height: 8),
          Kv('Zone', v: loc.zone.label.toUpperCase(), size: 11),
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
                TextSpan(text: exchange ? hubVoice : outpost != null ? outpostVoice : voice[terr]),
              ]),
              style: T.mono(size: 11, color: C.text3, height: 1.45),
            ),
          ),
      ]),
    );
  }
}

// ── Fleet rail ─────────────────────────────────────────────────

/// The player's fleets, one card each. On a desktop they stand in a column
/// that takes the height it is given and scrolls when the fleets need more,
/// keeping the selected one in view.
class FleetRail extends StatefulWidget {
  final GameController ctrl;
  final bool narrow;
  final bool combat;
  const FleetRail({super.key, required this.ctrl, required this.narrow, required this.combat});

  /// Room for one card of the column, the least the rail is given.
  static const cardHeight = 86.0;

  @override
  State<FleetRail> createState() => _FleetRailState();
}

class _FleetRailState extends State<FleetRail> {
  final Map<int, GlobalKey> _places = {};
  int? _inView;

  GameController get ctrl => widget.ctrl;
  bool get narrow => widget.narrow;
  bool get combat => widget.combat;

  @override
  Widget build(BuildContext context) {
    final fleets = ctrl.state.fleets;
    final cards = [
      for (var i = 0; i < fleets.length; i++) _card(i, fleets[i]),
    ];
    if (narrow) {
      return Row(children: [for (final c in cards) Expanded(child: Padding(padding: const EdgeInsets.only(right: 4), child: c))]);
    }
    _places.removeWhere((id, _) => fleets.every((f) => f.id != id));
    final selected = ctrl.activeFleetId;
    if (selected != _inView) {
      _inView = selected;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        final card = _places[selected]?.currentContext;
        if (card == null) return;
        Scrollable.ensureVisible(card, alignmentPolicy: ScrollPositionAlignmentPolicy.keepVisibleAtEnd);
        Scrollable.ensureVisible(card, alignmentPolicy: ScrollPositionAlignmentPolicy.keepVisibleAtStart);
      });
    }
    return SingleChildScrollView(
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        for (var i = 0; i < cards.length; i++)
          Padding(key: _places.putIfAbsent(fleets[i].id, GlobalKey.new), padding: const EdgeInsets.only(bottom: 6), child: cards[i]),
      ]),
    );
  }

  Widget _card(int i, ui.FleetState f) {
    final on = i == ctrl.activeFleet;
    final hops = f.fuelForHops ?? f.fuel;
    final pol = f.policy == null || f.policy == 'manual' ? 'manual' : f.policy!;
    final pressed = f.pursuedBy != null || f.fleeing != null;
    return Hover(
      builder: (hv) => GestureDetector(
        key: ValueKey('fleet-card-${f.id}'),
        behavior: HitTestBehavior.opaque,
        onTap: () => ctrl.selectFleet(i),
        child: ConsolePanel(
          mark: pressed ? C.ember : on ? C.own : null,
          border: pressed ? C.ember : on ? C.own : (hv ? C.lineHi : null),
          fill: on ? const Color(0xF20D1816) : (hv ? const Color(0xF2191208) : null),
          padding: EdgeInsets.fromLTRB(narrow ? 6 : 10, narrow ? 5 : 7, narrow ? 6 : 10, narrow ? 5 : 6),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
            Row(children: [
              if (!narrow) ...[
                SymIcon((c, s) => Draw.symbol(c, Sym.fleet, s.width / 2, s.height / 2, 6, color: on ? C.own : C.a200, lw: 1.6, bloom: false), size: 12),
                const SizedBox(width: 7),
              ],
              Expanded(
                  child: Text(f.name.toUpperCase(),
                      overflow: TextOverflow.ellipsis,
                      style: T.cond(size: narrow ? 10.5 : 12.5, color: pressed ? C.ember : on ? C.own : C.a200, spacing: narrow ? .5 : 1.5))),
              if (!narrow) KeyCap('${i + 1}', size: 15),
            ]),
            if (!narrow) ...[
              const SizedBox(height: 3),
              Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                Text('${f.statusLabel}${on && combat && !f.underFire ? ' / COMBAT' : ''}',
                    key: Key('fleet-card-status-${f.id}'), style: T.mono(size: 10, color: f.fleeing != null ? damagedColor : C.text2, spacing: .4)),
                Text('${f.sector.q},${f.sector.r}', style: T.mono(size: 10, color: C.text3)),
              ]),
              if (f.fleeing case final leaving?)
                Text('TO ${leaving.to.q},${leaving.to.r} IN ${leaving.ticks} - ${roundsOf(leaving.rounds).toUpperCase()} MORE',
                    key: Key('fleet-card-fleeing-${f.id}'), overflow: TextOverflow.ellipsis, style: T.mono(size: 10, color: damagedColor)),
              if (f.pursuedBy case final hunter?)
                Text('PURSUED - LANDS IN ${hunter.arrivesIn}',
                    key: Key('fleet-card-pursued-${f.id}'), overflow: TextOverflow.ellipsis, style: T.mono(size: 10, color: atRiskColor, weight: FontWeight.w600)),
              const SizedBox(height: 5),
            ] else
              const SizedBox(height: 3),
            LedBar(f.fuelMax > 0 ? f.fuel / f.fuelMax : 0, color: hops < 4 ? C.ember : C.own, height: 7),
            if (!narrow)
              Padding(
                  padding: const EdgeInsets.only(top: 2),
                  child: Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
                    Text(hopCount(hops).toUpperCase(), style: T.mono(size: 10, color: C.text3)),
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

  /// The fleet stands in its homeworld's sector.
  final bool atHome;

  /// The hold its standing orders last announced, while it still stands.
  final PolicyAct? holding;

  /// Orders the fleet to refine; the control is lit while the state carries
  /// what that would do.
  final VoidCallback? onRefine;
  final Widget? footer;
  const StatusPanel({super.key, required this.f, required this.atHome, this.holding, this.onRefine, this.footer});

  /// What a fleet under orders keeps free under its Orders row for the
  /// reason of a hold, four lines of it, so that a hold that starts or ends
  /// moves neither the buttons below nor the panels under this one.
  static const holdRoom = 55.0;

  @override
  Widget build(BuildContext context) {
    final hullFrac = hullShare(f.hull, f.hullMax);
    final hops = f.fuelForHops ?? f.fuel;
    final need = f.homeFuel;
    final low = need > 0 && f.fuel < need * 1.2;
    final cargo = f.cargo.total;
    final pol = f.policy == null ? 'MANUAL' : f.policy!.toUpperCase();
    return ConsolePanel(
      title: 'Fleet status',
      subText: '${f.name} / ${shipsOf(f.shipCount)}',
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 6, 4, 0),
          child: Row(mainAxisAlignment: MainAxisAlignment.spaceAround, children: [
            Dial(frac: f.fuelMax > 0 ? f.fuel / f.fuelMax : 0, warn: .2, label: 'FUEL', value: '${f.fuel}/${f.fuelMax}'),
            Dial(frac: hullFrac, warn: .3, label: 'HULL', value: '${(hullFrac * 100).floor()}%', color: C.own),
          ]),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 2, 12, 10),
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            const SizedBox(height: 7),
            Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
              Lbl(f.laden ? 'Cargo - laden' : 'Cargo', size: 10),
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
            const SizedBox(height: 6),
            Row(children: [
              ConsoleButton('Refine',
                  key: const ValueKey('status-refine'), small: true, onPressed: f.refine == null ? null : onRefine, reason: refineRule),
              const SizedBox(width: 8),
              Expanded(
                  child: Text(switch (f.refine) { null => 'NOTHING TO REFINE', final r => refineLine(r) },
                      key: const Key('status-refine-yield'), textAlign: TextAlign.right, style: T.mono(size: 10, color: C.text3))),
            ]),
            const SizedBox(height: 8),
            Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
              const Lbl('Ships', size: 10),
              Text('HULL ${hullOf(f.hull, f.hullMax)}', key: const Key('status-hull'), style: T.mono(size: 10, color: C.text3)),
            ]),
            ShipRows(f.ships, key: const Key('status-ships'), tag: 'status-ship'),
            const SizedBox(height: 7),
            FleetCondition(refit: f.refit, refitDue: f.refitDue, repair: f.repair),
            Kv('A round', key: const Key('status-round'), v: 'FIRES ${shortCount(f.attack)}  SHIELD ${shortCount(f.shield)}'),
            Kv('Fuel for', v: hopCount(hops).toUpperCase()),
            if (atHome)
              const Kv('Way home', v: 'AT THE DOCK')
            else
              Kv('Way home needs', key: const Key('status-way-home'), v: '$need FUEL${low ? '  LOW' : ''}', vc: low ? C.threat(6) : null),
            if (f.atOutpost)
              const Kv('Way to outpost', key: Key('status-way-outpost'), v: 'AT THE OUTPOST')
            else if (f.outpostFuel case final toOutpost?)
              Kv('Way to outpost needs',
                  key: const Key('status-way-outpost'),
                  v: '$toOutpost FUEL${toOutpost > f.fuel ? '  SHORT' : ''}',
                  vc: toOutpost > f.fuel ? C.threat(6) : null),
            Kv('Fleet power', v: shortCount(f.power.round())),
            Kv('Scan range', v: '${f.scanRange}'),
            Kv('Orders', key: const Key('status-orders'), v: holding == null ? pol : '$pol - HOLDING', vc: C.a300),
            if (f.policy != null)
              ConstrainedBox(
                  constraints: const BoxConstraints(minHeight: holdRoom),
                  child: holding == null ? null : WhyText(holding!.reason, key: const Key('status-hold'))),
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

  /// The server's odds for this fleet in this sector; null until it answers.
  final proto.SectorOdds? odds;
  final bool combat;
  final bool atHome;
  final proto.SectorState? sector;

  /// Current tick, for despawn countdowns.
  final int tick;
  final ui.FleetState fleet;

  /// The ids of the player's fleets, for a group that is after one of them.
  final Iterable<int> own;
  final bool hasOre;
  final bool hasSalvage;
  final bool hasDerelict;
  final bool harvesting;
  final bool boarding;

  /// The player's meter at the exchange when the fleet stands on it; null
  /// anywhere else.
  final proto.ExchangeMeter? exchange;

  /// The exchange panel is open.
  final bool trading;

  /// The outpost standing in this sector, whoever owns it.
  final proto.OutpostBrief? outpost;

  /// The player's own outpost when it is the one standing here.
  final proto.OutpostState? station;

  /// The outpost panel is open.
  final bool outpostOpen;
  final String? target;
  final void Function(String k) onTarget;
  final void Function(String k) onHover;
  final VoidCallback onHoverEnd;
  final void Function(String act) onAct;
  final VoidCallback onOpenHome;

  /// Open the dock's load control for this fleet.
  final VoidCallback onLoadHold;
  const ContactsPanel({
    super.key,
    required this.threat,
    required this.odds,
    required this.combat,
    required this.atHome,
    required this.sector,
    this.tick = 0,
    required this.fleet,
    this.own = const [],
    required this.hasOre,
    required this.hasSalvage,
    required this.hasDerelict,
    required this.harvesting,
    required this.boarding,
    this.exchange,
    this.trading = false,
    this.outpost,
    this.station,
    this.outpostOpen = false,
    required this.target,
    required this.onTarget,
    required this.onHover,
    required this.onHoverEnd,
    required this.onAct,
    required this.onOpenHome,
    required this.onLoadHold,
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
      final o = odds == null ? null : RatioInfo.fromOdds(odds!);
      final th = threat.rating;
      rows.add(_row(
        'h',
        _Diamond(th),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t(threat.groups.length == 1 ? 'Hostile group' : '${threat.groups.length} hostile groups', trail: ThreatBadge(th, sm: true)),
          _d(Text('POWER ${shortCount(threat.power.round())}${combat ? ' - ENGAGED' : ''}',
              key: const Key('contact-hostile-power'), style: _ds.copyWith(color: combat ? C.threat(6) : C.text3))),
          for (final g in threat.groups) ...[
            Text(groupHeading(g), key: Key('contact-hostile-${g.id}'), style: _ds.copyWith(color: g.shipCount < g.muster ? damagedColor : C.text2)),
            GroupBrief(g, tick: tick, live: sector?.live ?? true, own: own, beside: fleet, tag: 'contact-hostile-${g.id}'),
          ],
          if (o == null)
            _d(Text('ODDS: REQUESTED', key: const Key('contact-odds-pending'), style: _ds))
          else ...[
            Padding(padding: const EdgeInsets.only(top: 6), child: Text.rich(key: const Key('contact-odds'), TextSpan(children: [
              TextSpan(text: o.label, style: TextStyle(color: o.color, fontWeight: FontWeight.w600)),
              TextSpan(text: '  ratio ${o.r.toStringAsFixed(2)} - ${o.outcome}', style: const TextStyle(color: C.text3)),
            ]), style: T.mono(size: 10.5))),
            RatioGauge(o.r),
          ],
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
            for (final e in [('metal', 'Fe', s?.oreReserve?.metal), ('crystal', 'Cr', s?.oreReserve?.crystal), ('deuterium', 'De', s?.oreReserve?.deuterium)])
              if (e.$3 != null && e.$3!.maxUnits > 0)
                Text('${e.$2.toUpperCase()} LEFT ${reserveText(e.$3!)}', key: Key('contact-reserve-${e.$1}'), style: _ds.copyWith(color: C.ore(e.$1))),
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
          if (despawnLabel(sector, tick) case final d?)
            Text(d.toUpperCase(),
                key: const Key('contact-salvage-despawn'),
                style: _ds.copyWith(color: (salvageSecondsLeft(sector, tick) ?? 999) <= 60 ? C.ember : C.text3)),
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
    if (exchange case final meter?) {
      rows.add(_row(
        'x',
        SymIcon((c, sz) => Draw.symbol(c, Sym.hub, sz.width / 2, sz.height / 2, 13, color: C.gold, lw: 1.2, bloom: false), size: 26),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t('Hub exchange'),
          _d(Text(
              'TRADES CARGO FOR CARGO, FOR A FEE.\nNEXT UNIT AT ${(meter.fee * 100).round()}% - METER ${meter.pressure.round()} OF ${meter.span.round()}',
              key: const Key('contact-exchange'),
              style: _ds)),
          Padding(
              padding: const EdgeInsets.only(top: 7),
              child: ConsoleButton(trading ? 'Close exchange' : 'Open exchange',
                  key: const ValueKey('contact-trade'), small: true, keyHint: 'T', active: trading, onPressed: () => onAct('trade'))),
        ]),
      ));
    }
    if (outpost case final post?) {
      final own = station;
      rows.add(_row(
        'p',
        SymIcon((c, sz) => Draw.symbol(c, Sym.outpost, sz.width / 2, sz.height / 2, 12, color: post.mine ? C.own : C.a300, lw: 1.2, bloom: false), size: 26),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t(post.mine ? 'Your outpost' : 'Outpost of ${post.owner}'),
          _d(Text(
              own != null
                  ? '${outpostStanding(own, tick)}\n${outpostDockVoice(own)}'
                  : '${post.level == 0 ? 'NOT YET OPEN' : 'LEVEL ${post.level}'}${post.breached ? ' - BREACHED' : ''}',
              key: const Key('contact-outpost'),
              style: _ds.copyWith(color: post.breached ? C.threat(6) : C.text3))),
          if (own != null)
            Padding(
                padding: const EdgeInsets.only(top: 7),
                child: ConsoleButton(outpostOpen ? 'Close outpost' : 'Open outpost',
                    key: const ValueKey('contact-outpost-open'), small: true, keyHint: 'O', active: outpostOpen, onPressed: () => onAct('outpost'))),
        ]),
      ));
    }
    if (atHome) {
      rows.add(_row(
        'home',
        SymIcon((c, sz) => Draw.symbol(c, Sym.home, sz.width / 2, sz.height / 2, 15, color: C.own, lw: 1.2, bloom: false), size: 30),
        Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          _t('Homeworld'),
          _d(Text(
              fleet.laden ? dockVoiceLaden : dockVoice,
              key: const Key('contact-dock'),
              style: _ds)),
          Padding(
              padding: const EdgeInsets.only(top: 7),
              child: Wrap(spacing: 6, runSpacing: 6, children: [
                ConsoleButton('Open homeworld', small: true, onPressed: onOpenHome),
                ConsoleButton('Load hold', key: const ValueKey('contact-load'), small: true, keyHint: 'L', onPressed: onLoadHold),
              ])),
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

// ── Flight and pursuit ─────────────────────────────────────────

/// One lane out of a fight: the gate, where it leads, whether it is the way
/// home, and the server's quote for that jump once it has answered.
class FlightWay {
  final Dir dir;
  final proto.Hex to;
  final bool home;
  final proto.FlightQuote? quote;
  const FlightWay(this.dir, this.to, {this.home = false, this.quote});
}

TextStyle get _flightText => T.mono(size: 10.5, color: C.text2, height: 1.35);

/// The way out of a fight for a fleet that stands in one and fires: the
/// odds of the fight as it stands, what a jump out costs, who would follow,
/// and a button for every lane. All of it is the state's `flight`; a lane
/// the server has quoted says whether the group follows there. Greyed with
/// the engine's sentence when it would refuse the order.
class FlightPanel extends StatelessWidget {
  final ui.FleetState fleet;
  final proto.FlightQuote quote;
  final List<FlightWay> ways;
  final void Function(Dir) onLeave;

  /// The panel stands on the windshield, where each lane has its gate key
  /// and Recall its own. Off for a fleet in a raid's fight read from the
  /// homeworld's or the outpost's screen.
  final bool keys;
  const FlightPanel({super.key, required this.fleet, required this.quote, required this.ways, required this.onLeave, this.keys = true});

  @override
  Widget build(BuildContext context) {
    final o = RatioInfo.ofLabel(quote.ratio, quote.label);
    final refuges = [for (final w in ways) if (quote.chase != null && w.quote != null && w.quote!.chase == null) w];
    return ConsolePanel(
      key: const Key('flight-panel'),
      title: 'Leave the fight',
      titleColor: C.a300,
      mark: C.a300,
      subText: fleet.name,
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Text.rich(
            key: const Key('flight-odds'),
            TextSpan(children: [
              const TextSpan(text: 'AS IT STANDS  ', style: TextStyle(color: C.text3)),
              TextSpan(text: o.label, style: TextStyle(color: o.color, fontWeight: FontWeight.w600)),
              TextSpan(text: '  ratio ${o.r.toStringAsFixed(2)} - ${o.outcome}', style: const TextStyle(color: C.text3)),
            ]),
            style: T.mono(size: 10.5, height: 1.35)),
        RatioGauge(o.r),
        const SizedBox(height: 6),
        const Lbl('A jump out costs'),
        Text(flightPrice(quote).toUpperCase(), key: const Key('flight-price'), style: _flightText.copyWith(color: C.text)),
        Text('THE FLEET STOPS FIRING WHEN IT IS ORDERED OUT, AND IS FIRED ON UNTIL IT IS GONE.', style: _flightText.copyWith(color: C.text3, fontSize: 10)),
        const SizedBox(height: 5),
        Text(chaseLine(quote.chase).toUpperCase(),
            key: const Key('flight-chase'), style: _flightText.copyWith(color: quote.chase == null ? C.ok : atRiskColor)),
        if (refuges.isNotEmpty)
          Text('NOTHING FOLLOWS TO ${[for (final w in refuges) '${w.to.q}, ${w.to.r} (${w.dir.key.toUpperCase()})'].join(' OR ')}.',
              key: const Key('flight-refuge'), style: _flightText.copyWith(color: C.ok)),
        const SizedBox(height: 7),
        const Lbl('Leave by'),
        const SizedBox(height: 4),
        if (ways.isEmpty)
          Text('NO LANE LEADS OUT OF THIS SECTOR.', style: _flightText)
        else
          Wrap(spacing: 6, runSpacing: 6, children: [
            for (final w in ways)
              ConsoleButton('${w.to.q}, ${w.to.r}${w.home ? ' HOME' : ''}',
                  key: ValueKey('flee-${w.dir.key}'),
                  small: true,
                  keyHint: keys ? w.dir.key.toUpperCase() : null,
                  semanticsLabel: 'Leave the fight ${w.dir.name}',
                  onPressed: quote.refusal == null ? () => onLeave(w.dir) : null,
                  reason: quote.refusal),
          ]),
        if (quote.refusal case final why?)
          Padding(
              padding: const EdgeInsets.only(top: 5),
              child: Text(why, key: const Key('flight-refusal'), style: T.mono(size: 10.5, color: C.ember, height: 1.3))),
        if (keys) const WhyText('Recall (R) is the other way out: home at once, for more fuel and at a risk to every ship.'),
      ]),
    );
  }
}

/// A fleet charging its jump out of a fight: where to, how long, how many
/// more rounds it is fired on, and what calling it off costs. The state's
/// `fleeing`.
class LeavingPanel extends StatelessWidget {
  final ui.FleetState fleet;
  final proto.Fleeing leaving;
  final VoidCallback onStop;

  /// As [FlightPanel.keys]: Stop has its key on the windshield only.
  final bool keys;
  const LeavingPanel({super.key, required this.fleet, required this.leaving, required this.onStop, this.keys = true});

  @override
  Widget build(BuildContext context) => ConsolePanel(
        key: const Key('leaving-panel'),
        title: 'Leaving the fight',
        titleColor: C.a300,
        border: C.a300,
        mark: C.a300,
        subText: fleet.name,
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
          Text('JUMP IN ${leaving.ticks}',
              key: const Key('leaving-count'), style: T.cond(size: 22, color: C.a100, weight: FontWeight.w700, spacing: 2.4, height: 1.1)),
          Text('${fleeingLine(leaving).toUpperCase()}.', key: const Key('leaving-line'), style: _flightText.copyWith(color: C.text)),
          Text('GUNS SILENT: IT IS FIRED ON AND DOES NOT ANSWER.', style: _flightText.copyWith(color: atRiskColor)),
          const SizedBox(height: 7),
          Align(
              alignment: Alignment.centerLeft,
              child: ConsoleButton('Stop and fight on',
                  key: const ValueKey('leaving-stop'), small: true, danger: true, keyHint: keys ? 'X' : null, onPressed: onStop)),
          const WhyText(stopFlightRule, key: Key('leaving-stop-rule')),
        ]),
      );
}

/// The ways out of a raid's fight at the dock [at], the homeworld or the
/// outpost, for a screen that is not the windshield: for every fleet of the
/// player's standing there, the panel to leave by a lane while the state
/// quotes its flight, and the one to stop while it charges the jump. A
/// press sends the fleet's own `move` or `stop`; nothing of it is decided
/// here.
class RaidExits extends StatelessWidget {
  final GameController controller;
  final proto.Hex at;
  const RaidExits({super.key, required this.controller, required this.at});

  @override
  Widget build(BuildContext context) {
    final st = controller.state;
    final lanes = st.sectors[at]?.connections ?? const <proto.Hex>[];
    final ways = [
      for (final d in dirs)
        if (lanes.contains(at + d.v)) FlightWay(d, at + d.v),
    ];
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      for (final f in st.fleets)
        if (f.sector == at) ...[
          if (f.fleeing case final leaving?)
            Padding(
                padding: const EdgeInsets.only(top: 10),
                child: LeavingPanel(fleet: f, leaving: leaving, keys: false, onStop: () => controller.sendCommand(proto.StopCommand(fleetId: f.id)))),
          if (f.flight case final quote?)
            Padding(
                padding: const EdgeInsets.only(top: 10),
                child: FlightPanel(
                    fleet: f,
                    quote: quote,
                    ways: ways,
                    keys: false,
                    onLeave: (d) => controller.sendCommand(proto.MoveCommand(fleetId: f.id, target: at + d.v)))),
        ],
    ]);
  }
}

/// The group on the fleet's trail: what it is, when it lands and from
/// where, whether it lands before the fleet can be gone and how much
/// further it follows. The state's `pursued_by`.
class PursuitPanel extends StatelessWidget {
  final ui.FleetState fleet;
  final proto.Pursuit pursuit;
  const PursuitPanel({super.key, required this.fleet, required this.pursuit});

  @override
  Widget build(BuildContext context) => ConsolePanel(
        key: const Key('pursuit-panel'),
        title: 'Pursuit',
        titleColor: C.ember,
        border: C.ember,
        mark: C.ember,
        subText: fleet.name,
        trailing: const Lamp('INBOUND', color: C.ember, blink: true),
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
          Text('ARRIVES IN ${pursuit.arrivesIn}',
              key: const Key('pursuit-count'),
              style: T.cond(size: 22, color: const Color(0xFFFFD6CC), weight: FontWeight.w700, spacing: 2.4, height: 1.1)),
          Text('${pursuitLine(pursuit).toUpperCase()}.', key: const Key('pursuit-line'), style: _flightText.copyWith(color: C.text)),
          Text(pursuitVerdict(pursuit, charging: fleet.status == ui.FleetStatus.enRoute).toUpperCase(),
              key: const Key('pursuit-verdict'), style: _flightText.copyWith(color: pursuit.catches ? atRiskColor : C.ok)),
        ]),
      );
}

/// One line over the fleets of the phone layout, which keeps its panels in
/// a sheet: a fleet that may leave a fight, one that is leaving, one that is
/// followed. A tap opens the sheet.
class NoticeStrip extends StatelessWidget {
  final String id;
  final String head;
  final String text;
  final Color color;
  final VoidCallback onTap;
  const NoticeStrip({super.key, required this.id, required this.head, required this.text, required this.color, required this.onTap});

  @override
  Widget build(BuildContext context) => GestureDetector(
        key: ValueKey(id),
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: ConsolePanel(
          flat: true,
          border: color,
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
          child: Text.rich(
            TextSpan(children: [
              TextSpan(text: '$head  ', style: TextStyle(color: color, fontWeight: FontWeight.w600)),
              TextSpan(text: text),
            ]),
            overflow: TextOverflow.ellipsis,
            style: T.mono(size: 11, color: C.text),
          ),
        ),
      );
}

// ── Fleet management ───────────────────────────────────────────

class FleetManagePanel extends StatefulWidget {
  final GameController controller;
  const FleetManagePanel({super.key, required this.controller});
  @override
  State<FleetManagePanel> createState() => _FleetManagePanelState();
}

/// The steps the split picker's - and + move a class's count by: one for a
/// fleet of five, a hundred for a fleet of thousands.
const splitSteps = [1, 10, 100];

class _FleetManagePanelState extends State<FleetManagePanel> {
  final Map<proto.ShipClass, int> _pick = {};
  int? _forFleet;
  int _step = splitSteps.first;
  bool _damagedFirst = false;
  GameController get c => widget.controller;

  @override
  Widget build(BuildContext context) {
    final fleet = c.currentFleet;
    if (_forFleet != fleet.id) {
      _forFleet = fleet.id;
      _pick.clear();
      _damagedFirst = false;
    }
    _pick.removeWhere((cls, _) => fleet.ships.every((g) => g.shipClass != cls));
    for (final g in fleet.ships) {
      _pick.update(g.shipClass, (n) => n.clamp(0, g.count), ifAbsent: () => 0);
    }
    final picked = _pick.values.fold<int>(0, (a, b) => a + b);
    final canSplit = picked > 0 && picked < fleet.shipCount;
    final why = picked == 0 ? 'Pick ships with + first.' : 'One ship must stay in the fleet.';
    final partners = c.mergeCandidates;
    return ConsolePanel(
      key: const Key('fleet-manage'),
      title: 'Fleet management',
      subText: '${fleet.name} / ${shipsOf(fleet.shipCount)}',
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Row(children: [
          const Expanded(child: Lbl('Split off')),
          for (final s in splitSteps)
            Padding(
                padding: const EdgeInsets.only(left: 4),
                child: ConsoleButton('x$s',
                    key: ValueKey('fm-step-$s'), small: true, active: _step == s, semanticsLabel: 'Split step $s', onPressed: () => setState(() => _step = s))),
        ]),
        for (final g in fleet.ships) _pickRow(g),
        const SizedBox(height: 8),
        Wrap(spacing: 6, runSpacing: 6, children: [
          ConsoleButton('Whole first', key: const ValueKey('fm-whole-first'), small: true, active: !_damagedFirst, onPressed: () => setState(() => _damagedFirst = false)),
          ConsoleButton('Damaged first', key: const ValueKey('fm-damaged-first'), small: true, active: _damagedFirst, onPressed: () => setState(() => _damagedFirst = true)),
        ]),
        WhyText(_damagedFirst
            ? 'The most damaged ships of each class go: how the wounded are sent home.'
            : 'Whole ships of each class go first, damaged ones when the whole run out.'),
        const SizedBox(height: 8),
        Align(
          alignment: Alignment.centerLeft,
          child: ConsoleButton(picked == 0 ? 'Split off' : 'Split off ${groupDigits(picked)}',
              key: const ValueKey('fm-split'), small: true, primary: canSplit, onPressed: canSplit ? () => _split(fleet) : null, reason: why),
        ),
        if (!canSplit) WhyText(why),
        const SizedBox(height: 10),
        const Lbl('Merge in (same sector)'),
        if (partners.isEmpty) const WhyText('No other fleet here.') else for (final p in partners) _mergeRow(p),
      ]),
    );
  }

  Widget _pickRow(proto.ShipGroupState g) {
    final cls = g.shipClass;
    final n = _pick[cls] ?? 0;
    void set(int to) => setState(() => _pick[cls] = to.clamp(0, g.count));
    return Padding(
      padding: const EdgeInsets.only(top: 5),
      child: Row(children: [
        Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
          Text(cls.label.toUpperCase(), overflow: TextOverflow.ellipsis, style: T.cond(size: 11.5, color: C.text2, spacing: 1.1)),
          Text.rich(
            TextSpan(children: [
              TextSpan(text: 'OF ${groupDigits(g.count)}'),
              if (g.damaged > 0) TextSpan(text: '  ${groupDigits(g.damaged)} DAMAGED', style: const TextStyle(color: damagedColor)),
            ]),
            style: T.mono(size: 10, color: C.text3),
          ),
        ])),
        ConsoleButton('-',
            key: ValueKey('fm-minus-${cls.wire}'),
            small: true,
            semanticsLabel: 'Split off fewer ${cls.label}',
            onPressed: n > 0 ? () => set(n - _step) : null,
            reason: 'None picked'),
        SizedBox(width: 44, child: Text(groupDigits(n), key: Key('fm-count-${cls.wire}'), textAlign: TextAlign.center, style: T.mono(size: 11, color: n > 0 ? C.a300 : C.text3))),
        ConsoleButton('+',
            key: ValueKey('fm-plus-${cls.wire}'),
            small: true,
            semanticsLabel: 'Split off more ${cls.label}',
            onPressed: n < g.count ? () => set(n + _step) : null,
            reason: 'All picked'),
        const SizedBox(width: 4),
        ConsoleButton('All',
            key: ValueKey('fm-all-${cls.wire}'),
            small: true,
            active: n == g.count,
            semanticsLabel: 'Split off all ${cls.label}',
            onPressed: () => set(n == g.count ? 0 : g.count)),
      ]),
    );
  }

  Widget _mergeRow(ui.FleetState p) => Padding(
        padding: const EdgeInsets.only(top: 4),
        child: Row(children: [
          Expanded(child: Text('${p.name.toUpperCase()} - ${shipsOf(p.shipCount).toUpperCase()}', style: T.mono(size: 11, color: C.text2))),
          ConsoleButton('Merge', key: ValueKey('fm-merge-${p.id}'), small: true, semanticsLabel: 'Merge ${p.name} in', onPressed: () => c.mergeFleet(p.id)),
        ]),
      );

  void _split(ui.FleetState fleet) {
    final ships = [
      for (final g in fleet.ships)
        if ((_pick[g.shipClass] ?? 0) > 0) proto.ShipCount(shipClass: g.shipClass, count: _pick[g.shipClass]!),
    ];
    final damagedFirst = _damagedFirst;
    setState(_pick.clear);
    c.splitFleet(ships, damagedFirst: damagedFirst);
  }
}

// ── Standing orders ────────────────────────────────────────────

/// Least odds ratios the panel steps through, times ten, up to the most the
/// server takes. Any ratio it takes can be typed (`policy ... engage=`).
const engageSteps = [5, 9, 11, 13, 15, 20, 25, 30, 40, 60, proto.policyEngageRatioX10Max];

/// Fuel budgets the panel steps through, in percent of deuterium income.
/// The last lifts the bound; any percentage can be typed (`policy ... fuel=`).
const fuelBudgets = [10, 20, 35, 50, 75, proto.policyFuelUnboundedPct];

proto.PolicyPreset presetOf(ui.FleetState f) => f.orders?.preset ?? proto.PolicyPreset.manual;

/// What a doctrine does and when it goes home, as RULES.md has it.
String doctrineLine(proto.PolicyPreset p) => switch (p) {
      proto.PolicyPreset.manual => 'You fly it. Clears the orders.',
      proto.PolicyPreset.prospect => 'Scans and charts the sectors in range. Home when none is left.',
      proto.PolicyPreset.mineAndReturn => 'Strips the seams that repay their fuel. Home with the ore when the hold fills.',
      proto.PolicyPreset.salvageAndSites => 'Scoops wreckage, boards derelicts, nearest first. Home when the board is clear.',
      proto.PolicyPreset.patrolHome => 'Hunts hostiles in range at its least odds, scoops wreckage. Home when the hold fills.',
    };

String _hexes(int n) => '$n HEX${n == 1 ? '' : 'ES'}';

String _budget(int pct) => pct >= proto.policyFuelUnboundedPct ? 'NO BOUND' : '$pct%';

/// Odds below which a doctrine leaves a fight that the panel steps
/// through, times ten, between the least and the most the server takes. Any
/// ratio it takes can be typed (`policy ... disengage=`).
const disengageSteps = [proto.policyDisengageRatioX10Min, 3, 5, 7, 9, 11, 13, 15, 20, 30, proto.policyDisengageRatioX10Max];

String _ratio(int x10) => '${(x10 / 10).toStringAsFixed(1)}X';

/// "RANGE 8 HEXES - ENGAGE 1.5X - FUEL 40%" and, on a line of its own,
/// "LEAVES A FIGHT BELOW 0.9X": the settings in force, as the server holds
/// them.
String settingsLine(proto.PolicyParams p) =>
    'RANGE ${_hexes(p.maxRange)} - ENGAGE ${_ratio(p.engageRatioX10)} - FUEL ${_budget(p.fuelBudgetPct)}\n'
    'LEAVES A FIGHT BELOW ${_ratio(p.disengageRatioX10)}';

/// The standing orders of the selected fleet: the doctrine in force and what
/// it last reported, above all why it holds when it holds, and a form that
/// gives new orders. The reports are the engine's sentences; the form sends
/// what the typed `policy` sends.
class OrdersPanel extends StatefulWidget {
  final GameController controller;
  const OrdersPanel({super.key, required this.controller});
  @override
  State<OrdersPanel> createState() => _OrdersPanelState();
}

class _OrdersPanelState extends State<OrdersPanel> {
  /// Lines the report keeps room for whatever it says, so that a hold that
  /// starts or ends moves nothing of the form under it. A sentence longer
  /// than that at the width of the desktop column grows the box.
  static const _reportLines = 5;
  static const _reportSize = 10.5, _reportLeading = 1.35;

  /// The settings have their two rows whether or not orders stand.
  static const _settingsHeight = 30.0;

  proto.PolicyPreset? _preset;
  int? _range, _engage, _disengage, _fuel;
  bool? _refine;
  int? _forFleet;
  proto.StandingOrders? _stood;
  GameController get c => widget.controller;

  void _forget() {
    _preset = null;
    _range = null;
    _engage = null;
    _disengage = null;
    _fuel = null;
    _refine = null;
  }

  @override
  Widget build(BuildContext context) {
    final st = c.state;
    final fleet = c.currentFleet;
    final standing = fleet.orders;
    final inForce = presetOf(fleet);
    if (_forFleet != fleet.id || _stood != standing) {
      _forFleet = fleet.id;
      _stood = standing;
      _forget();
    }
    final usual = standing?.params ?? const proto.PolicyParams();
    final preset = _preset ?? inForce;
    final manual = preset == proto.PolicyPreset.manual;
    final form = proto.PolicyParams(
      minFuelPct: usual.minFuelPct,
      cargoReturnPct: usual.cargoReturnPct,
      maxRange: _range ?? usual.maxRange,
      engageRatioX10: _engage ?? usual.engageRatioX10,
      fuelBudgetPct: _fuel ?? usual.fuelBudgetPct,
      refine: _refine ?? usual.refine,
      disengageRatioX10: _disengage ?? usual.disengageRatioX10,
    );
    final same =preset == inForce && (manual || form == usual);
    final why = !same
        ? null
        : manual
            ? '${fleet.name} has no standing orders.'
            : 'These are the orders in force.';
    final acts = st.policyActs[fleet.id] ?? const <PolicyAct>[];

    return ConsolePanel(
      key: const ValueKey('orders-panel'),
      title: 'Standing orders',
      subText: fleet.name,
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        Kv('In force', key: const Key('orders-in-force'), v: inForce.label.toUpperCase(), vc: C.a300),
        SizedBox(
            height: _settingsHeight,
            child: standing == null
                ? null
                : Align(
                    alignment: Alignment.bottomLeft,
                    child: Text(settingsLine(standing.params), key: const Key('orders-settings'), maxLines: 2, style: T.mono(size: 10, color: C.text2, height: 1.3)))),
        const SizedBox(height: 6),
        _report(st.tick, fleet, acts.firstOrNull),
        const SizedBox(height: 8),
        const Lbl('Doctrine'),
        for (final p in proto.PolicyPreset.values) _doctrine(p, on: p == preset, standing: p == inForce),
        if (!manual) ...[
          const SizedBox(height: 6),
          _stepper('Range from home', 'range', _hexes(form.maxRange),
              down: form.maxRange > proto.policyRangeMin ? form.maxRange - 1 : null,
              up: form.maxRange < proto.policyRangeMax ? form.maxRange + 1 : null,
              set: (v) => _range = v),
          _stepper('Least odds to fight', 'engage', _ratio(form.engageRatioX10),
              down: engageSteps.where((s) => s < form.engageRatioX10).lastOrNull,
              up: engageSteps.where((s) => s > form.engageRatioX10).firstOrNull,
              set: (v) => _engage = v),
          _stepper('Leave a fight below', 'disengage', _ratio(form.disengageRatioX10),
              down: disengageSteps.where((s) => s < form.disengageRatioX10).lastOrNull,
              up: disengageSteps.where((s) => s > form.disengageRatioX10).firstOrNull,
              set: (v) => _disengage = v),
          RatioGauge(form.engageRatioX10 / 10),
          const WhyText(
              'Below the first no doctrine enters a sector, hostiles there or not. Below the second, in a fight, it jumps out for home. '
              'The engine never holds the second above the first.',
              key: Key('orders-disengage-note')),
          _stepper('Fuel budget, of income', 'fuel', _budget(form.fuelBudgetPct),
              down: fuelBudgets.where((b) => b < form.fuelBudgetPct).lastOrNull,
              up: fuelBudgets.where((b) => b > form.fuelBudgetPct).firstOrNull,
              set: (v) => _fuel = v),
          WhyText(form.fuelBudgetPct >= proto.policyFuelUnboundedPct
              ? 'No bound: the tank takes what the stockpile holds, ahead of every queue.'
              : 'It leaves the dock on a full tank only, paid for from this share of your deuterium income.'),
          WhyText('It turns home at ${form.cargoReturnPct}% hold, or with fuel down to the way back plus ${form.minFuelPct}% of the tank.'),
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Row(children: [
              const Expanded(child: Lbl('Refine the hold before turning home for fuel')),
              ConsoleButton(form.refine ? 'On' : 'Off',
                  key: const ValueKey('orders-refine'),
                  small: true,
                  active: form.refine,
                  semanticsLabel: 'Refine the hold before turning home for fuel, ${form.refine ? 'on' : 'off'}. $refineRule',
                  onPressed: () => setState(() => _refine = !form.refine)),
            ]),
          ),
        ],
        if (fleet.laden && !manual)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Text(
                'The hold of ${fleet.name} was loaded by hand. Standing orders unload at the dock: giving them ends that, and the hold goes back to the stockpile.',
                key: const Key('orders-laden'),
                style: T.mono(size: 10, color: C.threat(5), height: 1.3)),
          ),
        const SizedBox(height: 10),
        Align(
          alignment: Alignment.centerLeft,
          child: ConsoleButton(manual ? 'Clear orders' : 'Set ${preset.label}',
              key: const ValueKey('orders-set'), small: true, primary: why == null, onPressed: why == null ? () => c.setOrders(preset, form) : null, reason: why),
        ),
        if (why != null) WhyText(why, key: const Key('orders-why')),
        if (acts.length > 1) ...[
          const SizedBox(height: 10),
          const Lbl('Before that'),
          const SizedBox(height: 2),
          for (final a in acts.skip(1)) _earlier(a),
        ],
      ]),
    );
  }

  /// The newest report: the hold the fleet stands in and its reason, or the
  /// last thing its orders did.
  Widget _report(int tick, ui.FleetState fleet, PolicyAct? act) {
    final hold = act != null && act.isHold && fleet.policy != null;
    final String head, body;
    if (act == null) {
      head = 'NO REPORT YET';
      body = fleet.policy == null
          ? 'Under orders, every act and every hold shows here with its reason.'
          : 'Nothing from these orders since this console was opened. A fleet that stands still says why within a minute.';
    } else {
      head = hold ? 'HOLDING FOR ${clockFmt(tick - act.since)}' : 'LAST REPORT: ${act.heading.toUpperCase()}, ${clockFmt(tick - act.tick)} AGO';
      body = act.reason;
    }
    return Container(
      key: const Key('orders-report'),
      padding: const EdgeInsets.fromLTRB(9, 6, 9, 7),
      decoration: BoxDecoration(
          color: hold ? C.a(C.a500, .08) : C.inset,
          border: Border(left: BorderSide(color: hold ? C.a500 : C.a700, width: 3), top: const BorderSide(color: C.lineLo), right: const BorderSide(color: C.lineLo), bottom: const BorderSide(color: C.lineLo))),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
        Text(head, key: const Key('orders-report-head'), style: T.cond(size: 10.5, color: hold ? C.a200 : C.text2, spacing: 1.4)),
        const SizedBox(height: 3),
        ConstrainedBox(
            constraints: const BoxConstraints(minHeight: _reportLines * _reportSize * _reportLeading),
            child: Text(body, key: const Key('orders-report-reason'), style: T.mono(size: _reportSize, color: act == null ? C.text3 : C.text, height: _reportLeading))),
      ]),
    );
  }

  Widget _earlier(PolicyAct a) => Padding(
        padding: const EdgeInsets.only(top: 2),
        child: Text.rich(
          TextSpan(children: [
            TextSpan(text: '${(a.tick % 100000).toString().padLeft(5, '0')}  ', style: const TextStyle(color: C.text4)),
            TextSpan(text: '${a.heading.toUpperCase()}  ', style: TextStyle(color: a.isFlight ? damagedColor : C.a600)),
            TextSpan(text: a.reason),
          ]),
          style: T.mono(size: 10, color: C.text2, height: 1.3),
        ),
      );

  Widget _doctrine(proto.PolicyPreset p, {required bool on, required bool standing}) => Padding(
        padding: const EdgeInsets.only(top: 3),
        child: Semantics(
          button: true,
          selected: on,
          label: 'Doctrine ${p.label}${standing ? ', in force' : ''}. ${doctrineLine(p)}',
          excludeSemantics: true,
          onTap: () => setState(() => _preset = p),
          child: Hover(
            builder: (hv) => GestureDetector(
              key: ValueKey('orders-doctrine-${p.name}'),
              behavior: HitTestBehavior.opaque,
              onTap: () => setState(() => _preset = p),
              child: Container(
                padding: const EdgeInsets.fromLTRB(8, 3, 8, 4),
                decoration: BoxDecoration(color: on ? C.a(C.a500, .14) : (hv ? C.a(C.a500, .06) : null), border: Border.all(color: on ? C.a500 : (hv ? C.lineHi : C.line))),
                child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
                  Row(children: [
                    Expanded(child: Text(p.label.toUpperCase(), style: T.cond(size: 11, color: on ? C.a100 : C.a300, spacing: 1.4))),
                    if (standing) const Tag('in force', color: C.own),
                  ]),
                  const SizedBox(height: 2),
                  Text(doctrineLine(p), style: T.mono(size: 10, color: on ? C.text2 : C.text3, height: 1.3)),
                ]),
              ),
            ),
          ),
        ),
      );

  Widget _stepper(String label, String id, String value, {required int? down, required int? up, required void Function(int) set}) => Padding(
        padding: const EdgeInsets.only(top: 4),
        child: Row(children: [
          Expanded(child: Lbl(label)),
          ConsoleButton('-', key: ValueKey('orders-$id-down'), small: true, semanticsLabel: '$label down', onPressed: down == null ? null : () => setState(() => set(down)), reason: 'At the least the engine takes'),
          SizedBox(width: 78, child: Text(value, key: Key('orders-$id'), textAlign: TextAlign.center, style: T.mono(size: 11, color: C.a300))),
          ConsoleButton('+', key: ValueKey('orders-$id-up'), small: true, semanticsLabel: '$label up', onPressed: up == null ? null : () => setState(() => set(up)), reason: 'At the most the engine takes'),
        ]),
      );
}

// ── Exchange ───────────────────────────────────────────────────

const _resAbbr = {proto.ResourceKind.metal: 'Fe', proto.ResourceKind.crystal: 'Cr', proto.ResourceKind.deuterium: 'De'};

Color _resColor(proto.ResourceKind k) => C.ore(k.name);

/// What the meter is, as RULES.md has it: the weight sold and not yet
/// forgotten, which sets the fee and falls with time whether or not the
/// player trades. Its span is a share of the player's own mine output, so
/// with none the fee is at its top and nothing falls.
String meterNote(proto.ExchangeMeter m) => m.span > 0
    ? 'The meter is what you have sold here lately. The fuller it reads the higher the fee on the next unit, and it empties by itself with time.'
    : 'The meter is what you have sold here lately. With no mine output the fee is at its highest, and the meter does not fall.';

/// Shares of the hold of the chosen resource a trade offers, in percent. The
/// whole of it is sent with no amount, which the server reads as all aboard.
const exchangeShares = [25, 50, 75, 100];

/// Quote and make a trade at the hub's exchange for the selected fleet. The
/// numbers are the server's: the panel only picks what to ask.
class ExchangePanel extends StatefulWidget {
  final GameController controller;
  const ExchangePanel({super.key, required this.controller});
  @override
  State<ExchangePanel> createState() => _ExchangePanelState();
}

class _ExchangePanelState extends State<ExchangePanel> {
  proto.ResourceKind? _give;
  proto.ResourceKind? _want;
  int _share = 100;
  int? _forFleet;
  GameController get c => widget.controller;

  @override
  Widget build(BuildContext context) {
    final st = c.state;
    final fleet = c.currentFleet;
    if (_forFleet != fleet.id) {
      _forFleet = fleet.id;
      _give = null;
      _want = null;
      _share = 100;
    }
    final hold = st.holds[fleet.id] ?? const proto.Resources();
    var give = _give ?? proto.ResourceKind.metal;
    if (_give == null) {
      for (final k in proto.ResourceKind.values) {
        if (k.of(hold) > give.of(hold)) give = k;
      }
    }
    final want = _want != null && _want != give ? _want! : proto.ResourceKind.values.firstWhere((k) => k != give);
    final held = give.of(hold);
    final amount = _share == 100 ? null : held * _share / 100;
    final offered = amount ?? held;
    if (held > 0) c.requestQuote(fleet.id, give, want, amount: amount);
    final latest = st.quoteFor(fleet.id, give, want);
    final quote = latest != null && held > 0 && (latest.terms.amount - offered).abs() <= math.max(.01, offered * .05) ? latest : null;
    final meter = quote?.terms.meter ?? st.exchange;
    final after = quote?.terms.meterAfter;
    final here = st.exchangeAt(fleet.sector);
    final why = held <= 0
        ? 'The hold carries no ${give.name}.'
        : quote == null
            ? 'Waiting for the quote.'
            : quote.refusal;

    Widget pick(proto.ResourceKind k, {required bool on, required String label, required VoidCallback onPressed, String? reason, required String id}) =>
        ConsoleButton(label, key: ValueKey(id), small: true, primary: on, onPressed: reason == null ? onPressed : null, reason: reason, semanticsLabel: '$id ${k.name}');

    return ConsolePanel(
      title: 'Exchange',
      subText: here ? 'at the hub' : 'quote only',
      padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
        const Lbl('Give from the hold'),
        const SizedBox(height: 4),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final k in proto.ResourceKind.values)
            pick(k,
                id: 'ex-give-${k.name}',
                on: k == give,
                label: '${_resAbbr[k]} ${k.of(hold).floor()}',
                reason: k.of(hold) > 0 || k == give ? null : 'None aboard',
                onPressed: () => setState(() => _give = k)),
        ]),
        const SizedBox(height: 8),
        const Lbl('Take'),
        const SizedBox(height: 4),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final k in proto.ResourceKind.values)
            pick(k,
                id: 'ex-want-${k.name}',
                on: k == want,
                label: _resAbbr[k]!,
                reason: k == give ? 'That is what the fleet gives' : null,
                onPressed: () => setState(() => _want = k)),
        ]),
        const SizedBox(height: 8),
        const Lbl('How much'),
        const SizedBox(height: 4),
        Wrap(spacing: 6, runSpacing: 6, children: [
          for (final s in exchangeShares)
            ConsoleButton(s == 100 ? 'All' : '$s%', key: ValueKey('ex-share-$s'), small: true, primary: s == _share, onPressed: () => setState(() => _share = s)),
        ]),
        const SizedBox(height: 10),
        if (quote != null) ...[
          Text.rich(
            key: const Key('ex-quote'),
            TextSpan(children: [
              TextSpan(text: '${quote.terms.amount.floor()} ${_resAbbr[give]}', style: TextStyle(color: _resColor(give))),
              const TextSpan(text: '  GIVES  ', style: TextStyle(color: C.text3)),
              TextSpan(text: '${quote.terms.receive.round()} ${_resAbbr[want]}', style: TextStyle(color: _resColor(want), fontWeight: FontWeight.w600)),
            ]),
            style: T.mono(size: 13),
          ),
          const SizedBox(height: 4),
          Kv('Fee', v: '${(quote.terms.fee * 100).round()}% OF PAR ${quote.terms.par.round()} ${_resAbbr[want]!.toUpperCase()}'),
        ] else
          Text(held <= 0 ? 'NOTHING TO GIVE' : 'ASKING THE EXCHANGE', key: const Key('ex-pending'), style: T.mono(size: 11, color: C.text3)),
        Kv('Meter',
            key: const Key('ex-meter'),
            v: after == null
                ? '${meter.pressure.round()} OF ${meter.span.round()}'
                : '${meter.pressure.round()} TO ${after.pressure.round()} OF ${meter.span.round()}'),
        const SizedBox(height: 4),
        LedBar((after ?? meter).fill, color: (after ?? meter).fill >= 1 ? C.ember : C.a400, mark: C.text2, markAt: meter.fill),
        const SizedBox(height: 4),
        Kv('Fee on the next unit',
            v: after == null ? '${(meter.fee * 100).round()}%' : '${(meter.fee * 100).round()}% TO ${(after.fee * 100).round()}%'),
        Kv('Meter falls', key: const Key('ex-recovery'), v: _recovery(after ?? meter)),
        WhyText(meterNote(after ?? meter), key: const Key('ex-meter-note')),
        const SizedBox(height: 10),
        Align(
          alignment: Alignment.centerLeft,
          child: ConsoleButton('Trade',
              key: const ValueKey('ex-trade'),
              small: true,
              primary: why == null,
              onPressed: why == null ? () => c.trade(give, want, amount: amount) : null,
              reason: why),
        ),
        if (why != null) WhyText(why, key: const Key('ex-why')),
        if (fleet.refuel case final quote?) ..._refuel(fleet, quote),
      ]),
    );
  }

  /// The refuel offers of the hub the fleet stands at, each as the server
  /// quoted it, and the server's sentence when none would be accepted.
  List<Widget> _refuel(ui.FleetState fleet, proto.RefuelQuote quote) => [
        const SizedBox(height: 12),
        Lbl('Refuel from the hold - tank lacks ${quote.missing.round()}'),
        const SizedBox(height: 4),
        for (final offer in quote.offers)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Row(children: [
              Expanded(
                child: Text(
                  '${offer.amount.round()} ${_resAbbr[offer.give]} BUYS ${offer.fuel.round()} FUEL'
                  '${offer.fee > 0 ? ' - FEE ${(offer.fee * 100).round()}%' : ''}',
                  key: Key('refuel-offer-${offer.give.name}'),
                  style: T.mono(size: 11, color: C.text2),
                ),
              ),
              ConsoleButton('Refuel',
                  key: ValueKey('refuel-${offer.give.name}'),
                  small: true,
                  primary: true,
                  onPressed: () => c.refuel(fleet.id, offer.give)),
            ]),
          ),
        if (quote.refusal case final why?) WhyText(why, key: const Key('refuel-why')),
      ];

  /// "4185 PER 4:16:04 - EMPTY IN 38:41".
  static String _recovery(proto.ExchangeMeter m) {
    if (m.span <= 0) return 'NEVER: NO MINE OUTPUT';
    final rate = '${m.span.round()} PER ${clockFmt(m.windowTicks)}';
    final left = m.clearIn;
    return left == null || left == 0 ? rate : '$rate - EMPTY IN ${clockFmt(left)}';
  }
}

// ── Teleprinter ────────────────────────────────────────────────

/// The newest lines of the log, printed over the arena's lower left. It
/// takes at most [mostLines] rows of text and never more than [maxHeight],
/// which the windshield sets so that it ends below the south-west gate card.
/// An entry is shown whole or not at all, the newest first to be kept; what
/// does not fit is in the Overview's log.
class Feed extends StatelessWidget {
  final List<LogEntry> events;
  final double maxHeight;
  const Feed({super.key, required this.events, required this.maxHeight});

  static const mostLines = 8;
  static const _size = 10.5, _leading = 1.35;
  static const _rowHeight = _size * _leading;
  static const _entryChrome = 3.0;
  static const _sidePadding = 12.0;

  static TextStyle get _style => T.mono(size: _size, color: C.text2, height: _leading, spacing: .2);

  static List<InlineSpan> _spans(LogEntry e, Color color) => [
        const TextSpan(text: '> ', style: TextStyle(color: C.a600)),
        TextSpan(text: '${(e.tick % 100000).toString().padLeft(5, '0')}  ', style: const TextStyle(color: C.text4)),
        TextSpan(text: e.message.toUpperCase().replaceFirst(RegExp(r'^!+\s*'), ''), style: TextStyle(color: color)),
      ];

  /// The entries that fit, oldest first, and how many rows the newest may
  /// take when it is too long to fit whole.
  (List<(String, LogEntry)>, int) _fitting(BuildContext context, double width) {
    final rows = math.min(mostLines, ((maxHeight - _entryChrome) / _rowHeight).floor());
    final scaler = MediaQuery.textScalerOf(context);
    final shown = <(String, LogEntry)>[];
    var usedRows = 0, usedHeight = 0.0;
    for (final (name, e) in namedLog(events)) {
      final painter = TextPainter(
          text: TextSpan(children: [..._spans(e, C.text2), if (shown.isEmpty) const TextSpan(text: '   ')], style: _style),
          textDirection: TextDirection.ltr,
          textScaler: scaler)
        ..layout(maxWidth: math.max(1, width - _sidePadding));
      final need = painter.computeLineMetrics().length;
      painter.dispose();
      final height = need * _rowHeight + _entryChrome;
      if (shown.isNotEmpty && (usedRows + need > rows || usedHeight + height > maxHeight)) break;
      shown.add((name, e));
      usedRows += need;
      usedHeight += height;
      if (usedRows >= rows) break;
    }
    return (shown.reversed.toList(), rows);
  }

  Color _col(String m) {
    if (m.startsWith('!') || m.contains('LOST') || m.contains('destroyed') && m.contains('Your')) return const Color(0xFFFF9A86);
    if (m.contains('harvested') || m.contains('collected') || m.contains('traded') || m.contains('complete') || m.contains('ready') || m.contains('arrived') || m.contains('entered')) return const Color(0xFF8EEADF);
    if (m.contains('derelict') || m.contains('first visit')) return const Color(0xFFCDB7FF);
    return C.text2;
  }

  @override
  Widget build(BuildContext context) {
    if (maxHeight < _rowHeight + _entryChrome) return const SizedBox.shrink();
    return IgnorePointer(
      child: LayoutBuilder(builder: (context, box) {
        final (shown, rows) = _fitting(context, box.maxWidth);
        return Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          for (final (i, (name, e)) in shown.indexed)
            _TypeLine(key: ValueKey(name), entry: e, color: _col(e.message), rows: rows, last: i == shown.length - 1),
        ]);
      }),
    );
  }
}

class _TypeLine extends StatefulWidget {
  final LogEntry entry;
  final Color color;

  /// The most rows of text the line may take.
  final int rows;
  final bool last;
  const _TypeLine({super.key, required this.entry, required this.color, required this.rows, required this.last});
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
          ...Feed._spans(widget.entry, widget.color),
          if (widget.last) WidgetSpan(alignment: PlaceholderAlignment.middle, child: Blink(period: 11, onSteps: 6, builder: (_, on) => Container(width: 6, height: 10, margin: const EdgeInsets.only(left: 6), color: on ? C.a400 : Colors.transparent))),
        ]),
        maxLines: widget.rows,
        overflow: TextOverflow.ellipsis,
        style: Feed._style,
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
  final int tick;

  /// Danger band (1 to 3) of the faint contact heard in an uncharted gate.
  final int? band;

  /// The server's `preview_move` for this jump, once asked for (on hover).
  final proto.MovePreview? preview;

  /// What a jump out of the fight the fleet stands in costs, while it
  /// stands in one: the gate is then a way out under fire. The quote of the
  /// preview for this gate once the server has answered, the fleet's own
  /// until then.
  final proto.FlightQuote? flight;
  final bool hover;
  final VoidCallback onTap;
  final void Function(bool) onHover;
  const GateCard({super.key, required this.dir, required this.narrow, required this.known, required this.sector, required this.threat, required this.intel, required this.fuel, required this.isHome, required this.hint, this.tick = 0, this.band, this.preview, this.flight, required this.hover, required this.onTap, required this.onHover});

  /// "FLEE 1 RND", then whether a group follows through this gate once the
  /// server has quoted the jump, or that the order would be refused.
  Widget _flightLine(proto.FlightQuote q) {
    final style = T.mono(size: 9.5, color: C.text3);
    return Wrap(key: ValueKey('gate-flight-${dir.key}'), spacing: 5, children: [
      if (q.refusal != null)
        Text('CANNOT LEAVE', style: style.copyWith(color: C.ember))
      else ...[
        Text('FLEE ${q.rounds} RND', style: style.copyWith(color: C.a300, fontWeight: FontWeight.w600)),
        if (preview?.flight != null)
          Text(q.chase == null ? 'NO CHASE' : 'CHASED ${q.chase!.hexes}H', style: style.copyWith(color: q.chase == null ? C.ok : C.ember)),
      ],
    ]);
  }

  /// Width of a card on a desktop arena.
  static const width = 150.0;

  /// The most a card on a desktop arena grows to, with its odds line and a
  /// last line that wraps.
  static const tallest = 100.0;

  @override
  Widget build(BuildContext context) {
    final dashed = !known || intel.aged;
    final w = narrow ? 92.0 : width;
    final th = known ? threat.rating : 0;
    final hot = th >= 5;
    final burn = flight?.fuel.round() ?? fuel;
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
        body.add(Padding(padding: const EdgeInsets.fromLTRB(6, 3, 6, 0), child: Text(hint ? 'FAINT MASS READING${band == null ? '' : ' / BAND $band'}' : 'SCAN TO REVEAL', style: T.mono(size: 9.5, color: hint ? C.threat(6) : C.text3))));
        body.add(Padding(
            padding: const EdgeInsets.fromLTRB(6, 3, 6, 4),
            child: Row(children: [
              Expanded(child: flight == null ? const SizedBox.shrink() : _flightLine(flight!)),
              Text('-$burn FUEL', style: T.mono(size: 9.5, color: C.text3)),
            ])));
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
        final pv = preview;
        if (pv != null) {
          final o = RatioInfo.fromPreview(pv);
          body.add(Padding(
            key: ValueKey('gate-odds-${dir.key}'),
            padding: const EdgeInsets.fromLTRB(6, 3, 6, 0),
            child: Text.rich(TextSpan(children: [
              TextSpan(text: '${pv.ratio.toStringAsFixed(1)}X ${o.label}', style: TextStyle(color: o.color, fontWeight: FontWeight.w600)),
              TextSpan(text: pv.canReturn ? '  HOME ${pv.hopsHome}H' : '  NO RETURN', style: TextStyle(color: pv.canReturn ? C.text3 : C.threat(7))),
            ]), overflow: TextOverflow.ellipsis, style: lineStyle),
          ));
        }
        body.add(Padding(
          padding: const EdgeInsets.fromLTRB(6, 3, 6, 4),
          child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
            Expanded(
              child: Wrap(crossAxisAlignment: WrapCrossAlignment.center, children: [
                if (flight case final q?)
                  _flightLine(q)
                else if (intel.live)
                  const Lamp('LIVE', color: C.own, blink: true)
                else
                  Text('SEEN ${ago(intel.ticksOld).toUpperCase()} AGO', style: T.mono(size: 9.5, color: intel.aged ? C.fossil : C.text3)),
                if (salvagePinVisible(sector, tick))
                  Text(
                      despawnLabel(sector, tick) == null
                          ? ' +SALV'
                          : ' +SALV ${clockFmt(salvageSecondsLeft(sector, tick)!)}',
                      style: T.mono(size: 9.5, color: C.a400)),
                if (sector?.site != null) Text(' +DRLCT', style: T.mono(size: 9.5, color: C.rare)),
              ]),
            ),
            Text('-$burn FUEL', style: T.mono(size: 9.5, color: C.text3)),
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
      label: '${flight == null ? 'Jump' : 'Leave the fight by'} ${dir.name}, ${known ? terr : 'uncharted'}${th > 0 ? ', threat $th' : ''}',
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

  /// What the action costs or does that its name does not say, shown with
  /// it while it is on offer.
  final String? note;
  final bool active;
  final bool fire;
  const DockAction(this.id, this.label, this.key, {this.reason, this.note, this.active = false, this.fire = false});
  bool get ok => reason == null;
  String? get hint => reason ?? note;
}

class Dock extends StatelessWidget {
  final List<DockAction> actions;
  final bool narrow;
  final ValueNotifier<double> scanCooldown;
  final ValueNotifier<int> scanSecs;
  final void Function(String) onAct;
  final void Function(String?) onHint;
  const Dock({super.key, required this.actions, required this.narrow, required this.scanCooldown, required this.scanSecs, required this.onAct, required this.onHint});

  /// Width of one action on a desktop arena.
  static const tileWidth = 76.0;

  @override
  Widget build(BuildContext context) {
    final tiles = [for (final a in actions) GuideTarget('dock-${a.id}', child: _tile(a))];
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
          hint: a.hint,
          excludeSemantics: true,
          onTap: a.ok ? () => onAct(a.id) : null,
          child: Tooltip(
            message: a.hint ?? a.label,
            child: MouseRegion(
              onEnter: (_) => onHint(a.hint),
              onExit: (_) => onHint(null),
              child: GestureDetector(
                key: ValueKey('dock-${a.id}'),
                behavior: HitTestBehavior.opaque,
                onTap: a.ok ? () => onAct(a.id) : () => onHint(a.reason),
                child: Container(
                  width: narrow ? null : tileWidth,
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
