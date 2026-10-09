import 'package:flutter/material.dart';

import '../../console/economy_view.dart';
import '../../console/prefs.dart';
import '../../console/services.dart';
import '../../design/draw.dart';
import '../../design/gauges.dart';
import '../../design/panel.dart';
import '../../design/symbols.dart';
import '../../design/tokens.dart';
import '../../hex/hex_math.dart';
import '../../models/fleet.dart' as ui;
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;
import '../../scenes/overview/overview_scenes.dart';
import '../../state/game_controller.dart';
import 'ov_panel.dart';
import 'overview_cards.dart';
import 'overview_logic.dart';

class OverviewView extends StatefulWidget {
  const OverviewView({super.key});

  @override
  State<OverviewView> createState() => _OverviewViewState();
}

class _OverviewViewState extends State<OverviewView> {
  EventFilter _filter = EventFilter.all;
  VoidCallback? _unreg;
  Console? _con;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final con = Console.of(context);
    if (_con != con) {
      _unreg?.call();
      _con = con;
      _unreg = con.registerKeys(Screen.overview, _onKey);
    }
  }

  @override
  void dispose() {
    _unreg?.call();
    super.dispose();
  }

  bool _onKey(KeyEvent e) {
    final ch = e.character;
    if (ch == null) return false;
    final n = int.tryParse(ch);
    final con = _con;
    if (n == null || n < 1 || con == null) return false;
    if (n > con.state.fleets.length) return false;
    con.game.selectFleet(n - 1);
    return true;
  }

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    return ListenableBuilder(
      listenable: con.game,
      builder: (context, _) => LayoutBuilder(builder: (context, box) {
        final w = box.maxWidth;
        final narrow = w < 820;
        final wide = w >= 1100;
        final pad = narrow ? 10.0 : 18.0;
        final eco = con.economy;
        final gap = narrow ? 10.0 : 12.0;

        Widget row(List<Widget> kids, List<int> flex) {
          if (!wide) return Column(children: [for (final k in kids) Padding(padding: EdgeInsets.only(bottom: gap), child: k)]);
          return Padding(
            padding: EdgeInsets.only(bottom: gap),
            child: IntrinsicHeight(
              child: OvFill(
                child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  for (var i = 0; i < kids.length; i++) ...[if (i > 0) SizedBox(width: gap), Expanded(flex: flex[i], child: kids[i])],
                ]),
              ),
            ),
          );
        }

        final hasRank = eco.rank != null;
        final content = Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          row([_Hero(narrow: narrow), _NextCard(con: con)], [8, 4]),
          Padding(padding: EdgeInsets.only(bottom: gap), child: ResourceTiles(narrow: narrow)),
          Padding(padding: EdgeInsets.only(bottom: gap), child: QueueCards(columns: narrow ? 1 : (wide ? 4 : 2))),
          row([_FleetsPanel(con: con, avail: (wide ? (w - 2 * pad - gap) * 8 / 12 : w - 2 * pad) - 28), _RadarPanel(con: con)], [8, 4]),
          row([_AlertsPanel(con: con), _LogPanel(con: con, filter: _filter, onFilter: (f) => setState(() => _filter = f))], [5, 7]),
          if (wide)
            Padding(
              padding: EdgeInsets.only(bottom: gap),
              child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Expanded(child: hasRank ? _RankPanel(rank: eco.rank!) : const SizedBox.shrink()),
                SizedBox(width: gap),
                const Expanded(child: SizedBox.shrink(key: Key('slot-comms'))),
                SizedBox(width: gap),
                const Expanded(child: SizedBox.shrink(key: Key('slot-teams'))),
              ]),
            )
          else if (hasRank)
            _RankPanel(rank: eco.rank!),
        ]);
        return SingleChildScrollView(
          key: const Key('overview-scroll'),
          padding: EdgeInsets.fromLTRB(pad, narrow ? 10 : 16, pad, 40),
          child: Center(child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 1500), child: content)),
        );
      }),
    );
  }
}

// ── hero ────────────────────────────────────────────────────────

class _Hero extends StatelessWidget {
  final bool narrow;
  const _Hero({required this.narrow});

  @override
  Widget build(BuildContext context) {
    final con = Console.of(context);
    final s = con.state;
    final eco = con.economy;
    final cat = s.catalog;
    int lvl(proto.BuildingType t) => cat?.buildings.where((b) => b.buildingType == t).firstOrNull?.level ?? 0;
    final buildingLevels = cat?.buildings.fold<int>(0, (a, b) => a + b.level) ?? 0;
    final researchLevels = cat?.research.fold<int>(0, (a, b) => a + b.level) ?? 0;
    final dockedShips = RegExp(r'(\d+)x').allMatches(s.docked).fold<int>(0, (a, m) => a + int.parse(m.group(1)!));
    final ships = s.fleets.fold<int>(0, (a, f) => a + f.shipCount) + dockedShips;
    final zone = s.homeworld.zone.toUpperCase();
    final name = con.game.playerName.isEmpty ? 'Homeworld' : con.game.playerName;
    final chips = <Widget>[
      Tag(zone, on: true),
      Tag('Sensor L${lvl(proto.BuildingType.sensorArray)}'),
      Tag('Defence Grid L${lvl(proto.BuildingType.defenseGrid)}', color: C.own),
      if (eco.storage != null) Tag('Vault L${eco.storage!.vaultLevel}'),
      if (eco.world != null) Tag('Pace x${eco.world!.pace == eco.world!.pace.roundToDouble() ? eco.world!.pace.toInt() : eco.world!.pace}'),
    ];
    final stats = <(int, String)>[
      (buildingLevels, 'Building levels'),
      (researchLevels, 'Research'),
      (ships, 'Ships'),
      if (eco.defence != null) (eco.defence!.total.round(), 'Defence power'),
    ];
    return ConsolePanel(
      key: const Key('hero'),
      child: SizedBox(
        height: narrow ? 210 : 244,
        child: ClipRect(
          child: Stack(children: [
            Positioned.fill(child: SceneClock(prefs: con.prefs, painter: (t, still) => PlanetPainter(t, still, bloom: con.prefs.quality != Quality.low))),
            Positioned(
              left: 22,
              top: 16,
              right: narrow ? 12 : null,
              width: narrow ? null : 560,
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text('HOMEWORLD - SECTOR ${s.homeworld.q}, ${s.homeworld.r}', style: T.cond(size: 10, color: C.text3, weight: FontWeight.w400, spacing: 2.8)),
                const SizedBox(height: 6),
                Text(name.toUpperCase(),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: T.cond(size: narrow ? 28 : 40, color: C.a100, weight: FontWeight.w700, spacing: narrow ? 2.8 : 4).copyWith(shadows: const [Shadow(color: Color(0x4DFFB000), blurRadius: 14)])),
                Padding(
                    padding: const EdgeInsets.only(top: 12, bottom: 10),
                    child: Wrap(spacing: 6, runSpacing: 6, children: narrow ? chips.take(2).toList() : chips)),
                Text.rich(TextSpan(children: [
                  TextSpan(text: 'NOTE ', style: T.mono(size: 11, color: C.a700, weight: FontWeight.w600, spacing: 1)),
                  TextSpan(text: hqNote(s)),
                ]), style: T.mono(size: 11, color: C.text3)),
              ]),
            ),
            if (!narrow)
              Positioned(
                left: 22,
                bottom: 14,
                child: Container(
                  decoration: BoxDecoration(border: Border.all(color: C.line), color: const Color(0xCC080604)),
                  child: Row(mainAxisSize: MainAxisSize.min, children: [
                    for (var i = 0; i < stats.length; i++)
                      Container(
                        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 5),
                        decoration: BoxDecoration(border: Border(right: i == stats.length - 1 ? BorderSide.none : const BorderSide(color: C.lineLo))),
                        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                          Register(stats[i].$1, width: 3, size: 20, color: C.a200),
                          Lbl(stats[i].$2, size: 8.5),
                        ]),
                      ),
                  ]),
                ),
              ),
          ]),
        ),
      ),
    );
  }
}

// ── next up / raid ──────────────────────────────────────────────

class _NextCard extends StatelessWidget {
  final Console con;
  const _NextCard({required this.con});

  @override
  Widget build(BuildContext context) {
    final s = con.state;
    final eco = con.economy;
    final n = nextUp(s, eco);
    final act = OvActions(con);
    final d = eco.defence;
    void run() {
      switch (n.action) {
        case NextAction.openDefence:
          act.openTab(HomeworldTab.defence);
        case NextAction.queueResearch:
          act.research(n.tech!);
        case NextAction.queueBuild:
          act.build(n.building!);
        case NextAction.openWindshield:
          con.go(Screen.windshield);
        case NextAction.openHomeworld:
          con.go(Screen.homeworld);
        case NextAction.openMap:
          con.go(Screen.map);
        case NextAction.none:
          break;
      }
    }

    final raid = n.urgent && n.action == NextAction.openDefence && d != null;
    return OvPanel(
      key: Key(raid ? 'raid-card' : 'next-card'),
      title: n.kicker,
      titleColor: n.urgent ? C.ember : C.a400,
      padding: const EdgeInsets.fromLTRB(16, 10, 16, 0),
      footer: Align(
        alignment: Alignment.centerLeft,
        child: ConsoleButton(n.button, primary: true, keyHint: n.action == NextAction.queueResearch || n.action == NextAction.queueBuild ? 'Ret' : null, reason: n.disabledReason, onPressed: n.disabledReason == null ? run : null),
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(n.title.toUpperCase(), style: T.cond(size: 20, color: C.a100, spacing: 1.2, height: 1.2)),
        const SizedBox(height: 6),
        Text(n.why, style: T.mono(size: 11.5, color: C.text2, height: 1.5)),
        const SizedBox(height: 12),
        if (raid) ...[
          _kv('Your defence', '${d.total.round()}', C.threat(1)),
          LedBar(1, color: C.own, height: 8),
          const SizedBox(height: 10),
          _kv('Raid (est.)', '${d.raidEstimate.round()}', C.threat(5)),
          LedBar((d.raidEstimate / (d.total <= 0 ? 1 : d.total)).clamp(0.0, 1.0), color: C.threat(5), height: 8),
          const SizedBox(height: 10),
          _kv('Ratio', '${d.ratio.toStringAsFixed(2)} ${d.ratio >= 1 ? 'HELD' : 'BREACH'}', d.ratio >= 1 ? C.own : C.ember),
          const SizedBox(height: 8),
        ],
        if (n.cost != null) Padding(padding: const EdgeInsets.only(bottom: 8), child: costRow(n.cost!)),
        if (n.disabledReason != null) WhyText(n.disabledReason!),
      ]),
    );
  }

  Widget _kv(String k, String v, Color c) => Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
        Text(k.toUpperCase(), style: T.mono(size: 10.5, color: C.text3)),
        Text(v, style: T.mono(size: 10.5, color: c, weight: FontWeight.w600)),
      ]));
}

// ── fleets ──────────────────────────────────────────────────────

proto.ShipClass? _cls(String label) => proto.ShipClass.values.where((c) => c.label == label).firstOrNull;

class _FleetsPanel extends StatelessWidget {
  final Console con;
  final double avail;
  const _FleetsPanel({required this.con, required this.avail});

  @override
  Widget build(BuildContext context) {
    final fleets = con.state.fleets;
    return OvPanel(
      key: const Key('fleets-panel'),
      title: 'Fleets',
      subText: '${fleets.length} fleet${fleets.length == 1 ? '' : 's'}',
      padding: const EdgeInsets.fromLTRB(14, 10, 14, 14),
      child: Builder(builder: (context) {
        final cols = avail >= 640 ? 3 : (avail >= 420 ? 2 : 1);
        if (fleets.isEmpty) return Text('No fleets. Build ships at the shipyard.', style: T.mono(size: 11, color: C.text3));
        final cw = (avail - (cols - 1) * 10) / cols;
        return Wrap(spacing: 10, runSpacing: 10, children: [
          for (var i = 0; i < fleets.length; i++) SizedBox(width: cw, child: _FleetCard(con: con, index: i, f: fleets[i])),
        ]);
      }),
    );
  }
}

class _FleetCard extends StatelessWidget {
  final Console con;
  final int index;
  final ui.FleetState f;
  const _FleetCard({required this.con, required this.index, required this.f});

  @override
  Widget build(BuildContext context) {
    final home = proto.Hex.distance(f.sector, con.state.homeworld);
    final hops = f.jumpFuel > 0 ? f.fuel ~/ f.jumpFuel : null;
    final low = f.homeFuel > 0 && f.fuel < f.homeFuel * 1.3;
    final status = f.status;
    final chipCol = status == ui.FleetStatus.docked ? C.own : C.text2;
    final recallWhy = home == 0 ? 'Already at the homeworld' : null;
    final ships = <proto.ShipClass>[];
    for (final g in f.ships) {
      final c = _cls(g.shipClass);
      if (c != null) {
        for (var i = 0; i < g.count; i++) {
          ships.add(c);
        }
      }
    }
    void pilot(Screen s) {
      con.game.selectFleet(index);
      con.go(s);
    }

    return Container(
      key: Key('fleet-card-${f.id}'),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(border: Border.all(color: C.line), color: const Color(0x40000000)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Row(children: [
          SymIcon((c, s) => Draw.symbol(c, Sym.fleet, s.width / 2, s.height / 2, 6, color: C.a200, lw: 1.8, bloom: false), size: 14),
          const SizedBox(width: 8),
          Expanded(child: Text(f.name.toUpperCase(), overflow: TextOverflow.ellipsis, style: T.cond(size: 14, color: C.a200, spacing: 1.7))),
          Tag(status.label, on: status == ui.FleetStatus.harvesting, color: chipCol),
        ]),
        const SizedBox(height: 4),
        Text('${f.sector.q},${f.sector.r} - ${f.sector.zone.toUpperCase()} - ${home == 0 ? 'AT HOME' : '$home HOPS HOME'}', style: T.mono(size: 10, color: C.text3)),
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 9),
          child: SizedBox(
            height: 24,
            child: SymIcon((c, s) {
              for (var i = 0; i < ships.length && i < 10; i++) {
                Draw.symbol(c, Sym.own[ships[i]]!, 12 + i * 26.0, s.height / 2, 11, color: C.own, lw: 1.3, bloom: false);
              }
            }, size: 24),
          ),
        ),
        _kvr('Fuel', hops == null ? '${f.fuel}' : '$hops hops', warn: low ? ' LOW' : null),
        LedBar(f.fuelMax > 0 ? f.fuel / f.fuelMax : 0, color: low ? C.ember : C.own, height: 6),
        _kvr('Cargo', '${f.cargo.total} / ${f.cargo.capacity}'),
        LedBar(f.cargo.fraction, color: C.a400, height: 6),
        _kvr('Orders', (f.policy ?? 'manual')),
        const SizedBox(height: 8),
        Wrap(spacing: 6, runSpacing: 4, children: [
          ConsoleButton('Pilot', small: true, onPressed: () => pilot(Screen.windshield)),
          ConsoleButton('Map', small: true, onPressed: () => pilot(Screen.map)),
          ConsoleButton('Recall', small: true, reason: recallWhy, onPressed: recallWhy == null ? () {
            con.game.sendCommand(proto.RecallCommand(fleetId: f.id));
            con.game.note('> recall ${f.name}', level: EventLevel.full);
          } : null),
        ]),
        if (recallWhy != null) WhyText(recallWhy),
      ]),
    );
  }

  Widget _kvr(String k, String v, {String? warn}) => Padding(
      padding: const EdgeInsets.only(top: 6, bottom: 3),
      child: Row(mainAxisAlignment: MainAxisAlignment.spaceBetween, children: [
        Lbl(k, size: 10),
        Text.rich(TextSpan(children: [
          TextSpan(text: v, style: T.mono(size: 10, color: C.text)),
          if (warn != null) TextSpan(text: warn, style: T.mono(size: 10, color: C.threat(6), weight: FontWeight.w600)),
        ])),
      ]));
}

// ── nearby space ────────────────────────────────────────────────

class _RadarPanel extends StatelessWidget {
  final Console con;
  const _RadarPanel({required this.con});

  @override
  Widget build(BuildContext context) {
    final s = con.state;
    return Semantics(
      button: true,
      label: 'Nearby space. Open map',
      child: GestureDetector(
        key: const Key('radar-panel'),
        onTap: () => con.go(Screen.map),
        child: MouseRegion(
          cursor: SystemMouseCursors.click,
          child: OvPanel(
            title: 'Nearby space',
            subText: 'click to open map',
            child: SizedBox(
              height: 232,
              child: SceneClock(
                prefs: con.prefs,
                painter: (t, still) => PpiPainter(t, still, s.homeworld, s.sectors, s.fleets, s.tick),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

// ── alerts and log ──────────────────────────────────────────────

class _AlertsPanel extends StatelessWidget {
  final Console con;
  const _AlertsPanel({required this.con});

  @override
  Widget build(BuildContext context) {
    final items = buildAlerts(con.state, con.economy);
    final act = OvActions(con);
    return OvPanel(
      key: const Key('alerts-panel'),
      title: 'Alerts',
      subText: '${items.length} active',
      child: Column(children: [
        if (items.isEmpty) Padding(padding: const EdgeInsets.all(12), child: Text('No active alerts.', style: T.mono(size: 11, color: C.text3))),
        for (final a in items)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
            decoration: BoxDecoration(color: a.critical ? C.a(C.ember, .07) : null, border: const Border(top: BorderSide(color: C.lineLo))),
            child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Padding(
                  padding: const EdgeInsets.only(top: 4, right: 10),
                  child: a.critical ? Lamp('', color: C.ember, blink: true) : Container(width: 8, height: 8, color: C.threat(5))),
              Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(a.title.toUpperCase(), style: T.cond(size: 12.5, color: C.text, spacing: 1)),
                if (a.body.isNotEmpty) Padding(padding: const EdgeInsets.only(top: 2), child: Text(a.body, style: T.mono(size: 10.5, color: C.text2, height: 1.45))),
                if (a.tab != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 7),
                    child: ConsoleButton(a.tab == 'defence' ? 'Defence' : a.tab == 'storage' ? 'Vault' : 'Queue', small: true,
                        onPressed: () => act.openTab(HomeworldTab.values.firstWhere((t) => t.name == a.tab))),
                  ),
              ])),
              if (a.time != null) Text(a.time!, style: T.mono(size: 14, color: a.critical ? C.ember : C.threat(5), weight: FontWeight.w500)),
            ]),
          ),
      ]),
    );
  }
}

class _LogPanel extends StatelessWidget {
  final Console con;
  final EventFilter filter;
  final ValueChanged<EventFilter> onFilter;
  const _LogPanel({required this.con, required this.filter, required this.onFilter});

  @override
  Widget build(BuildContext context) {
    final events = [for (final e in con.state.events) if (eventMatches(filter, e.message)) e];
    return OvPanel(
      key: const Key('log-panel'),
      title: 'Event log',
      child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(10, 6, 10, 2),
          child: Wrap(spacing: 4, runSpacing: 4, children: [
            for (final f in EventFilter.values)
              ConsoleButton(f.label, small: true, active: f == filter, onPressed: () => onFilter(f), semanticsLabel: 'Filter ${f.label}'),
          ]),
        ),
        ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 300),
          child: events.isEmpty
              ? Padding(padding: const EdgeInsets.all(12), child: Text('No ${filter.label.toLowerCase()} events yet.', style: T.mono(size: 11, color: C.text3)))
              : SingleChildScrollView(
                  padding: const EdgeInsets.fromLTRB(10, 4, 10, 8),
                  child: Column(children: [
                    for (final e in events)
                      Padding(
                        padding: const EdgeInsets.symmetric(vertical: 2),
                        child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                          Text('> ', style: T.mono(size: 10.5, color: e.message.startsWith('!') ? C.ember : C.a700)),
                          SizedBox(width: 50, child: Text((e.tick % 100000).toString().padLeft(5, '0'), style: T.mono(size: 10.5, color: C.text4))),
                          Expanded(child: Text(e.message.toUpperCase(), style: T.mono(size: 10.5, color: e.message.startsWith('!') ? const Color(0xFFFF9A86) : C.text2, spacing: .2))),
                        ]),
                      ),
                  ]),
                ),
        ),
      ]),
    );
  }
}

// ── ranking (only when the data exists) ─────────────────────────

class _RankPanel extends StatelessWidget {
  final RankView rank;
  const _RankPanel({required this.rank});

  @override
  Widget build(BuildContext context) {
    return OvPanel(
      key: const Key('rank-panel'),
      title: 'Ranking',
      subText: 'one world, humans + agents',
      padding: const EdgeInsets.only(bottom: 6),
      child: Column(children: [
        for (final r in rank.board)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 5),
            decoration: BoxDecoration(color: r.$1 == rank.rank ? C.a(C.own, .08) : null, border: const Border(top: BorderSide(color: C.lineLo))),
            child: Row(children: [
              SizedBox(width: 30, child: Text(r.$1.toString().padLeft(3, '0'), style: T.mono(size: 11, color: C.text3))),
              Text(r.$2, style: T.mono(size: 11.5, color: r.$1 == rank.rank ? C.own : C.text)),
              const SizedBox(width: 8),
              Tag(r.$3, color: r.$3 == 'AI' ? C.rare : C.text3),
              const Spacer(),
              Text(fmtInt(r.$4), style: T.mono(size: 11.5, color: r.$1 == rank.rank ? C.own : C.a300)),
            ]),
          ),
      ]),
    );
  }
}
