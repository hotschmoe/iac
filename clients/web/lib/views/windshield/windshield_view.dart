import 'dart:async';
import 'dart:math' as math;

import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';

import '../../console/intel.dart';
import '../../console/outpost.dart' show inOutpostRings;
import '../../console/route_planner.dart';
import '../../console/services.dart';
import '../../console/threat.dart';
import '../../console/toasts.dart';
import '../../design/beat.dart';
import '../../design/gauges.dart';
import '../../design/lens.dart';
import '../../design/panel.dart';
import '../../design/tokens.dart';
import '../../models/fleet.dart' as ui;
import '../../protocol/protocol.dart' as proto;
import '../../scenes/windshield/geometry.dart';
import '../../scenes/windshield/painter.dart';
import '../../scenes/windshield/sim.dart';
import '../../state/game_controller.dart' show HomeworldTab;
import '../homeworld/economy_tabs.dart' show HoldDock, HoldPanel;
import '../onboarding.dart';
import '../outpost_view.dart' show FoundingPanel, OutpostPanel;
import 'parts.dart';

class WindshieldView extends StatefulWidget {
  const WindshieldView({super.key});

  @override
  State<WindshieldView> createState() => _WindshieldViewState();
}

class _WindshieldViewState extends State<WindshieldView> with TickerProviderStateMixin {
  final WindshieldSim sim = WindshieldSim();
  final ValueNotifier<int> _frame = ValueNotifier(0);
  late final Ticker _ticker;
  late final AnimationController _retrace = AnimationController(vsync: this, duration: const Duration(milliseconds: 500));
  late final AnimationController _banner = AnimationController(vsync: this, duration: const Duration(milliseconds: 2800));
  Console? _con;
  StreamSubscription<proto.GameEvent>? _evSub;
  VoidCallback? _unreg;
  Duration _last = Duration.zero;

  String _resPick = 'auto';
  bool _hsub = false;
  bool _sheet = false;
  bool _manage = false;
  bool _trade = false;
  bool _orders = false;
  bool _outpost = false;
  DateTime? _armedUntil;
  DateTime? _scanAt;
  String? _dockHint;
  String _bannerH = '', _bannerP = '';
  bool _bannerRed = false;
  final Set<proto.Hex> _firstVisits = {};
  final ValueNotifier<double> _scanFill = ValueNotifier(0);
  final ValueNotifier<int> _scanSecs = ValueNotifier(0);
  static const _scanDur = proto.scanCooldown;

  /// The most the status sheet of the phone layout takes of a wide arena.
  static const _sheetWidth = 380.0;

  /// Where the desktop layout's fleet rail starts, under the sector head.
  static const _railTop = 230.0;

  /// The panels stand in the desktop layout's column or in the phone
  /// layout's sheet. The key carries them from one to the other when the
  /// window is resized across the two, with what the player had begun in
  /// them: a doctrine picked and not yet set, an Abandon asked.
  final GlobalKey _panels = GlobalKey();

  Console get con => _con!;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker(_onTick)..start();
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_con == null) {
      _con = Console.of(context);
      con.game.addListener(_onGame);
      _evSub = con.game.gameEvents.listen(_onEvent);
      _unreg = con.registerKeys(Screen.windshield, _onKey);
      sim.onBanner = _showBanner;
      sim.onRetrace = _doRetrace;
      sim.onCue = con.play;
      _syncSim();
    }
    sim.reduced = reducedMotion(context) || con.prefs.reduceMotion;
  }

  @override
  void dispose() {
    _ticker.dispose();
    _retrace.dispose();
    _banner.dispose();
    _evSub?.cancel();
    _unreg?.call();
    _con?.game.removeListener(_onGame);
    super.dispose();
  }

  // ── frame loop ──────────────────────────────────────────────

  void _onTick(Duration d) {
    final dt = math.min(.05, (d - _last).inMicroseconds / 1e6);
    _last = d;
    sim.q = con.prefs.quality;
    sim.tick(dt, d.inMicroseconds / 1e6);
    final at = _scanAt;
    if (at != null) {
      final p = clock.now().difference(at).inMilliseconds / 1000 / _scanDur;
      if (p >= 1) {
        _scanAt = null;
        _scanFill.value = 0;
      } else {
        _scanFill.value = 1 - p;
        _scanSecs.value = ((1 - p) * _scanDur).ceil();
      }
    }
    _frame.value++;
  }

  void _doRetrace() {
    if (con.prefs.retrace && !sim.reduced) _retrace.forward(from: 0);
  }

  void _showBanner(String h, String p, bool red) {
    setState(() {
      _bannerH = h;
      _bannerP = p;
      _bannerRed = red;
    });
    _banner.forward(from: 0);
  }

  // ── server data ─────────────────────────────────────────────

  ui.FleetState? get _fleet {
    final f = con.game.currentFleet;
    return f.id == 0 ? null : f;
  }

  proto.SectorState? _sec(proto.Hex h) => con.state.sectors[h];

  void _syncSim() {
    final f = _fleet;
    final before = sim.loc;
    sim.sync(f: f, s: f == null ? null : _sec(f.sector), homeHex: con.state.homeworld);
    sim.atExchange = f != null && con.state.exchangeAt(f.sector);
    sim.outpost = f == null ? null : _sec(f.sector)?.outpost;
    if (f != null && before != null && before != sim.loc && sim.warp != null) {
      final first = _firstVisits.remove(f.sector);
      final s = _sec(f.sector);
      final th = SectorThreat.of(s);
      if (sim.atExchange) {
        _showBanner('THE HUB', 'Exchange open. Trade cargo for cargo.', false);
      } else if (sim.outpost case final post?) {
        _showBanner(post.mine ? 'YOUR OUTPOST' : 'OUTPOST', post.mine ? 'Its store and dock are open to this fleet.' : 'A station of ${post.owner}.', false);
      } else {
        _showBanner(first ? 'NEW SECTOR' : 'SECTOR ${f.sector.q}, ${f.sector.r}',
            first ? 'First visit. ${voice[s?.terrain ?? proto.TerrainType.empty]}' : '${s?.terrain.label ?? ''} / ${f.sector.zone.label}', th.rating >= 5);
      }
    }
  }

  void _onGame() {
    if (!mounted) return;
    _syncSim();
    setState(() {});
  }

  void _onEvent(proto.GameEvent e) {
    final k = e.kind;
    if (k is proto.SectorEnteredEvent && k.firstVisit) _firstVisits.add(k.sector);
    sim.onEvent(e);
    if (!mounted) return;
    switch (k) {
      case proto.CombatStartedEvent():
        if (k.mine) {
          _showBanner('ENGAGED', 'Weapons free.', true);
          con.play('alert');
        }
      case proto.CombatEndedEvent():
        if (k.mine) {
          _showBanner(k.playerVictory ? 'VICTORY' : 'DEFEAT', k.playerVictory ? 'Hostiles destroyed. Salvage drifts in the wake.' : 'Fleet withdrew or was lost.', !k.playerVictory);
          con.play(k.playerVictory ? 'ok' : 'alert');
        }
      case proto.SiteExploredEvent():
        _showBanner('BOARDING COMPLETE', k.recoveredShip != null ? 'Recovered a ${k.recoveredShip!.label} hull.' : 'Hulk stripped.', false);
      case proto.SiteAmbushEvent():
        _showBanner('AMBUSH', 'Whatever killed the crew is still aboard.', true);
      case proto.SalvageCollectedEvent():
        sim.salvage?.fly = sim.now;
        con.play('ok');
      case proto.ExchangedEvent(:final terms):
        _showBanner('TRADED', '${terms.amount.round()} ${terms.give.name} for ${terms.receive.round()} ${terms.want.name}, fee ${(terms.fee * 100).round()}%.', false);
        con.play('ok');
      case proto.OutpostCompletedEvent(:final sector, :final level):
        _showBanner(level == 1 ? 'OUTPOST OPEN' : 'OUTPOST LEVEL $level', 'Level $level finished at ${sector.q}, ${sector.r}.', false);
        con.play('ok');
      case proto.ScanCompletedEvent():
        final f = _fleet;
        if (f != null && k.fleetId == f.id) {
          final near = {for (final d in dirs) f.sector + d.v};
          var worst = 0;
          for (final t in k.threats) {
            if (near.contains(t.sector) && (_sec(t.sector)?.hostiles?.isNotEmpty ?? false)) worst = math.max(worst, t.threat.rating);
          }
          final faint = k.signals.where((c) => c.signal == proto.SignalKind.hostileMass).fold<int>(0, (m, c) => math.max(m, c.threatBand));
          if (worst >= 5) {
            con.play('alert');
            _showBanner('CONTACT', 'T$worst mass reading on the scope.', true);
          } else if (faint >= 3) {
            con.play('alert');
            _showBanner('CONTACT', 'Faint hostile mass at the edge of range, danger band $faint.', true);
          }
        }
      default:
        break;
    }
  }

  // ── actions ─────────────────────────────────────────────────

  void _toast(String t, String m, {ToastTone tone = ToastTone.amber}) => con.toasts.show(t, m, tone: tone);

  void jump(Dir d) {
    final f = _fleet;
    if (f == null || sim.warp != null) return;
    final here = _sec(f.sector);
    final target = f.sector + d.v;
    if (here == null || !here.connections.contains(target)) {
      _toast('No lane', 'No lane ${d.name} from ${f.sector.q}, ${f.sector.r}.', tone: ToastTone.red);
      return;
    }
    if (f.cooldown > 0) {
      _toast('Drive recharging', 'Jump drive ready in ${f.cooldown} tick${f.cooldown > 1 ? 's' : ''}.');
      return;
    }
    if (f.jumpFuel > 0 && f.fuel < f.jumpFuel) {
      _toast('Out of fuel', 'Fleet cannot jump. ${f.fuel} fuel, ${f.jumpFuel} needed.', tone: ToastTone.red);
      return;
    }
    if (f.status == ui.FleetStatus.combat) {
      _toast('Engaged', 'Cannot jump while in combat.', tone: ToastTone.red);
      return;
    }
    sim.startWarp(d);
    con.game.sendCommand(proto.MoveCommand(fleetId: f.id, target: target));
  }

  String? _reason(String id, ui.FleetState f, SectorThreat th, proto.SectorState? s) {
    final ore = (s?.resources.metal.index ?? 0) + (s?.resources.crystal.index ?? 0) + (s?.resources.deuterium.index ?? 0);
    switch (id) {
      case 'scan':
        return _scanAt != null ? 'Scanner cooling down (${_scanSecs.value}s).' : null;
      case 'harvest':
        if (f.status == ui.FleetStatus.harvesting) return null;
        if (ore == 0) return 'No ore in this sector.';
        if (f.cargo.fraction >= 1) return 'Cargo hold full. Recall to unload.';
        return null;
      case 'fire':
        return th.hostile ? null : 'No hostile contact here.';
      case 'collect':
        final sv = s?.salvage;
        return sv != null && sv.total > 0 ? null : 'No salvage here.';
      case 'board':
        return s?.site == null ? 'No derelict here.' : null;
      case 'recall':
        return f.sector == con.state.homeworld ? 'Already at the homeworld.' : null;
      case 'load':
        if (f.sector == con.state.homeworld || f.atOutpost) return null;
        return con.state.outpost == null ? 'A hold is loaded from the stockpile at the homeworld only.' : 'A hold is loaded at the homeworld and at your outpost only.';
    }
    return null;
  }

  void act(String id, {bool fromSub = false}) {
    if (id == 'outpost') {
      _toggleOutpost();
      return;
    }
    final f = _fleet;
    if (f == null) return;
    final jumping = sim.warp;
    if (jumping != null && !(id == 'stop' && jumping.waiting)) return;
    final s = _sec(f.sector);
    final th = SectorThreat.of(s);
    final why = _reason(id, f, th, s);
    if (why != null) {
      _toast('Unavailable', why, tone: ToastTone.red);
      return;
    }
    switch (id) {
      case 'scan':
        _scanAt = clock.now();
        _scanFill.value = 1;
        _scanSecs.value = _scanDur;
        sim.startScan();
        con.game.sendCommand(proto.ScanCommand(fleetId: f.id));
      case 'harvest':
        if (f.status == ui.FleetStatus.harvesting) {
          con.game.sendCommand(proto.StopCommand(fleetId: f.id));
          setState(() => _hsub = false);
          return;
        }
        if (!fromSub) {
          setState(() => _hsub = !_hsub);
          return;
        }
        final r = switch (_resPick) { 'm' => proto.HarvestResource.metal, 'c' => proto.HarvestResource.crystal, 'd' => proto.HarvestResource.deuterium, _ => proto.HarvestResource.auto };
        final has = switch (r) {
          proto.HarvestResource.metal => (s?.resources.metal.index ?? 0) > 0,
          proto.HarvestResource.crystal => (s?.resources.crystal.index ?? 0) > 0,
          proto.HarvestResource.deuterium => (s?.resources.deuterium.index ?? 0) > 0,
          _ => true,
        };
        setState(() => _hsub = false);
        if (!has) {
          _toast('No ${r.name}', 'This sector has none of that resource.', tone: ToastTone.red);
          return;
        }
        con.play('ok');
        con.game.sendCommand(proto.HarvestCommand(fleetId: f.id, resource: r));
      case 'fire':
        _fire(f, th);
      case 'collect':
        con.game.sendCommand(proto.CollectSalvageCommand(fleetId: f.id));
      case 'board':
        con.game.sendCommand(proto.ExploreSiteCommand(fleetId: f.id));
        con.play('scan');
      case 'recall':
        con.game.sendCommand(proto.RecallCommand(fleetId: f.id));
        con.play('ok');
      case 'stop':
        sim.cancelWarp();
        con.game.sendCommand(proto.StopCommand(fleetId: f.id));
      case 'trade':
        setState(() {
          _trade = !_trade;
          if (_trade) {
            _sheet = true;
            _orders = false;
            _outpost = false;
          }
        });
        if (_trade) _clearForSheet();
        con.play('click');
      case 'load':
        if (f.sector == con.state.homeworld) {
          con.game.selectHomeworldTab(HomeworldTab.storage);
          con.go(Screen.homeworld);
        } else if (!_outpost) {
          _toggleOutpost();
        }
        return;
    }
    if (id != 'harvest') setState(() => _hsub = false);
  }

  /// The phone layout opens its panels in a sheet whose top lies under the
  /// toasts, so the toasts that ask nothing make way for it. The desktop
  /// layout has its panels in the right-hand column, which no toast reaches.
  void _clearForSheet() {
    final size = context.size;
    if (size != null && Geo(size).narrow) con.toasts.dismissPlain();
  }

  /// Open or shut the standing orders of the selected fleet. It is a panel,
  /// not an act of the fleet: a jump under way does not keep it shut.
  void _toggleOrders() {
    if (_fleet == null) return;
    setState(() {
      _orders = !_orders;
      if (_orders) {
        _sheet = true;
        _trade = false;
        _outpost = false;
      }
    });
    if (_orders) _clearForSheet();
    con.play('click');
  }

  /// Open or shut the outpost: the founding survey of this sector while the
  /// player has none, the outpost's own panel and its dock once it stands.
  /// Like the orders it is a panel, and a jump under way does not keep it
  /// shut.
  void _toggleOutpost() {
    if (_fleet == null) return;
    setState(() {
      _outpost = !_outpost;
      if (_outpost) {
        _sheet = true;
        _trade = false;
        _orders = false;
      }
    });
    if (_outpost) _clearForSheet();
    con.play('click');
  }

  void _fire(ui.FleetState f, SectorThreat th) {
    final s = _sec(f.sector);
    final id = s?.hostiles?.firstOrNull?.id;
    if (id == null || f.status == ui.FleetStatus.combat) return;
    final odds = con.state.oddsFor(f.id, f.sector);
    final o = odds == null ? null : RatioInfo.fromOdds(odds);
    final armed = _armedUntil != null && clock.now().isBefore(_armedUntil!);
    if ((o == null || o.deadly) && !armed) {
      _armedUntil = clock.now().add(const Duration(seconds: 3));
      if (o == null) {
        con.game.requestOdds(f.id, [f.sector]);
        _toast('No odds yet', 'This fight has not been rated yet (T${th.rating}, power ${th.power.round()}). Press F again to commit, or wait a moment.', tone: ToastTone.red);
      } else {
        _toast('Poor odds: ${o.label}', 'Ratio ${o.r.toStringAsFixed(2)} against T${th.rating}, power ${th.power.round()}: ${o.outcome}. Press F again to commit, or jump away.', tone: ToastTone.red);
      }
      con.play('alert');
      return;
    }
    _armedUntil = null;
    con.game.sendCommand(proto.AttackCommand(fleetId: f.id, targetFleetId: id));
  }

  bool _onKey(KeyEvent e) {
    final ch = e.character?.toLowerCase();
    if (e.logicalKey == LogicalKeyboardKey.escape) {
      if (sim.target != null || _hsub || _sheet || _trade || _orders || _outpost) {
        setState(() {
          sim.target = null;
          _hsub = false;
          _sheet = false;
          _trade = false;
          _orders = false;
          _outpost = false;
        });
        return true;
      }
      return false;
    }
    if (ch == null) return false;
    final di = dirs.where((d) => d.key == ch).firstOrNull;
    if (di != null) {
      jump(di);
      return true;
    }
    if (ch == 'p') {
      _toggleOrders();
      return true;
    }
    if (ch == 'o') {
      _toggleOutpost();
      return true;
    }
    const m = {'v': 'scan', 'h': 'harvest', 'f': 'fire', 'c': 'collect', 'b': 'board', 'r': 'recall', 'x': 'stop', 't': 'trade', 'l': 'load'};
    final a = m[ch];
    if (a != null) {
      act(a);
      return true;
    }
    final n = int.tryParse(ch);
    if (n != null && n >= 1 && n <= 9) {
      if (n <= con.state.fleets.length) con.game.selectFleet(n - 1);
      return true;
    }
    return false;
  }

  // ── build ───────────────────────────────────────────────────

  @override
  Widget build(BuildContext context) {
    final f = _fleet;
    if (f == null) {
      return Container(color: C.void_, alignment: Alignment.center, child: Text('NO FLEET. BUILD OR LAUNCH SHIPS FROM THE HOMEWORLD.', style: T.mono(size: 12, color: C.text3)));
    }
    return LayoutBuilder(builder: (context, box) {
      final size = Size(box.maxWidth, box.maxHeight);
      final g = Geo(size);
      final narrow = g.narrow;
      final st = con.state;
      final s = _sec(f.sector);
      final th = SectorThreat.of(s);
      if (th.hostile) con.game.requestOdds(f.id, [f.sector]);
      sim.layout(g);
      con.onboarding.gateCards(small: g.compact);
      final lanes =[for (final d in dirs) s?.connections.contains(f.sector + d.v) ?? false];
      final homePath = f.sector == st.homeworld
          ? null
          : RoutePlanner.plan(sectors: st.sectors, tick: st.tick, fleet: f, from: f.sector, to: st.homeworld, home: st.homeworld, mode: RouteMode.fast);
      final homeNext = homePath?.path.firstOrNull;
      final gateInfos = <GateInfo?>[];
      final cards = <Widget>[];
      for (final d in dirs) {
        if (!lanes[d.i]) {
          gateInfos.add(null);
          continue;
        }
        final nh = f.sector + d.v;
        final ns = st.sectors[nh];
        final intel = Intel.of(ns, st.tick);
        final nt = SectorThreat.of(ns);
        final known = ns != null;
        final col = !known ? C.rare : nt.rating > 0 ? C.threat(nt.rating) : C.a500;
        gateInfos.add(GateInfo(d, col));
        final cc = g.cardCenter(d);
        cards.add(Positioned(
          left: cc.dx,
          top: cc.dy,
          child: FractionalTranslation(
            translation: const Offset(-.5, -.5),
            child: GuideTarget(con.onboarding.pointsAtGate(nh) ? 'gates' : 'gate', child: GateCard(
              dir: d,
              narrow: g.compact,
              known: known,
              sector: ns,
              threat: nt,
              intel: intel,
              fuel: f.jumpFuel,
              isHome: homeNext == nh,
              hint: !known && st.signals[nh] == proto.SignalKind.hostileMass,
              tick: st.tick,
              band: known ? null : st.signalBands[nh],
              preview: st.previewFor(f.id, nh),
              hover: sim.hover == d.i,
              onTap: () => jump(d),
              onHover: (v) {
                sim.hover = v ? d.i : (sim.hover == d.i ? -1 : sim.hover);
                if (v) {
                  con.play('click');
                  con.game.requestPreview(f.id, nh);
                } else {
                  con.game.cancelPreview(f.id, nh);
                }
              },
            )),
          ),
        ));
      }
      final hasOre = (s?.resources.metal.index ?? 0) + (s?.resources.crystal.index ?? 0) + (s?.resources.deuterium.index ?? 0) > 0 && sim.ast.any((a) => a.ore != null);
      final hasSalv = s?.salvage != null && s!.salvage!.total > 0;
      final hasDer = s?.site != null;
      final combat = f.status == ui.FleetStatus.combat;
      final harvesting = f.status == ui.FleetStatus.harvesting;
      final exploring = f.status == ui.FleetStatus.exploring;
      final shield = _shieldFrac();
      final mine = st.outpost;
      final atOutpost = f.atOutpost;

      DockAction da(String id, String label, String key, {bool active = false, bool fire = false}) =>
          DockAction(id, label, key, reason: _reason(id, f, th, s), active: active, fire: fire);
      final dockActions = [
        da('scan', 'Scan', 'v'),
        da('harvest', 'Harvest', 'h', active: harvesting),
        da('fire', 'Fire', 'f', active: combat, fire: true),
        da('collect', 'Collect', 'c'),
        da('board', 'Board', 'b', active: exploring),
        da('recall', 'Recall', 'r'),
        da('stop', 'Stop', 'x'),
        da('trade', 'Trade', 't', active: _trade),
        if (mine == null ? inOutpostRings(f.sector) : atOutpost) da('outpost', mine == null ? 'Found' : 'Outpost', 'o', active: _outpost),
      ];
      final atExchange = st.exchangeAt(f.sector);
      final feedWidth = math.min(340.0, (size.width - dockActions.length * Dock.tileWidth) / 2 - 30);
      final feedCeiling = g.cardCenter(dirs[4]).dy + GateCard.tallest / 2;

      final rightCol = Column(key: _panels, mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        if (_orders) ...[OrdersPanel(controller: con.game), const SizedBox(height: 8)],
        if (_trade) ...[ExchangePanel(controller: con.game), const SizedBox(height: 8)],
        if (_outpost && mine == null) ...[FoundingPanel(controller: con.game), const SizedBox(height: 8)],
        if (_outpost && mine != null) ...[
          OutpostPanel(controller: con.game),
          const SizedBox(height: 8),
          HoldPanel(state: st, ctrl: con.game, dock: HoldDock.outpost),
          const SizedBox(height: 8),
        ],
        StatusPanel(
          f: f,
          shieldFrac: shield,
          atHome: f.sector == st.homeworld,
          holding: st.holdOf(f),
          onRefine: () => con.game.refineHold(f.id),
          footer: Wrap(spacing: 6, runSpacing: 6, children: [
            GuideTarget('orders', child: ConsoleButton(_orders ? 'Hide orders' : 'Orders', key: const ValueKey('orders-toggle'), small: true, keyHint: 'P', active: _orders, onPressed: _toggleOrders)),
            ConsoleButton(_manage ? 'Hide fleet' : 'Manage fleet', key: const ValueKey('manage-toggle'), small: true, active: _manage, onPressed: () => setState(() => _manage = !_manage)),
          ]),
        ),
        if (_manage) ...[const SizedBox(height: 8), FleetManagePanel(controller: con.game)],
        const SizedBox(height: 8),
        ContactsPanel(
          threat: th,
          odds: st.oddsFor(f.id, f.sector),
          combat: combat,
          atHome: f.sector == st.homeworld,
          sector: s,
          tick: st.tick,
          fleet: f,
          hasOre: hasOre,
          hasSalvage: hasSalv,
          hasDerelict: hasDer,
          harvesting: harvesting,
          boarding: exploring,
          exchange: atExchange ? st.exchange : null,
          trading: _trade,
          outpost: s?.outpost,
          station: atOutpost ? mine : null,
          outpostOpen: _outpost,
          target: sim.target,
          onTarget: (k) => setState(() {
            sim.target = sim.target == k ? null : k;
            con.play('click');
          }),
          onHover: (k) => sim.hoverT = k,
          onHoverEnd: () => sim.hoverT = null,
          onAct: act,
          onOpenHome: () => con.go(Screen.homeworld),
          onLoadHold: () => act('load'),
        ),
      ]);

      final geoStack = <Widget>[
        Positioned.fill(
          child: AnimatedBuilder(
            animation: _retrace,
            builder: (_, child) {
              final v = _retrace.value;
              final sy = !_retrace.isAnimating ? 1.0 : v < .38 ? 1 - .988 * (v / .38) : v < .62 ? .012 : .012 + .988 * ((v - .62) / .38);
              Widget w = Transform(alignment: Alignment.center, transform: Matrix4.diagonal3Values(1, sy, 1), child: child);
              if (_retrace.isAnimating && sy < .9) {
                w = ColorFiltered(colorFilter: const ColorFilter.matrix([3.2, 0, 0, 0, 0, 0, 3.2, 0, 0, 0, 0, 0, 3.2, 0, 0, 0, 0, 0, 1, 0]), child: w);
              }
              return w;
            },
            child: MouseRegion(
              cursor: sim.hoverT != null ? SystemMouseCursors.click : SystemMouseCursors.basic,
              onHover: (e) => _pick(e.localPosition, g),
              child: GestureDetector(
                key: const ValueKey('arena'),
                behavior: HitTestBehavior.opaque,
                onTap: () {
                  setState(() {
                    if (sim.hoverT != null) {
                      sim.target = sim.target == sim.hoverT ? null : sim.hoverT;
                    } else {
                      sim.target = null;
                    }
                  });
                },
                child: RepaintBoundary(
                  child: CustomPaint(painter: ArenaPainter(sim, gateInfos, lanes, con.prefs.quality, sim.reduced, _frame), size: Size.infinite),
                ),
              ),
            ),
          ),
        ),
        Positioned.fill(child: ScanOverlay(prefs: con.prefs)),
        if (combat) Positioned.fill(child: IgnorePointer(child: Blink(period: 13, onSteps: 7, builder: (_, on) => _Alarm(on: on)))),
        ...cards,
        Positioned(
            left: 16,
            top: 14,
            width: narrow ? math.min(SectorHead.phoneWidth, size.width - 96) : math.min(300, g.cardCenter(dirs[5]).dx - GateCard.width / 2 - 26),
            child: SectorHead(loc: f.sector, home: st.homeworld, sector: s, threat: th, narrow: narrow, exchange: atExchange, outpost: s?.outpost)),
        if (!narrow)
          Positioned(
              left: 16,
              top: _railTop,
              width: 208,
              child: ConstrainedBox(
                  constraints: BoxConstraints(maxHeight: math.max(FleetRail.cardHeight, feedCeiling - _railTop)),
                  child: FleetRail(ctrl: con.game, narrow: false, combat: combat))),
        if (narrow) Positioned(left: 8, right: 8, bottom: 70, child: FleetRail(ctrl: con.game, narrow: true, combat: combat)),
        if (narrow)
          Positioned(
              top: 8,
              right: 10,
              child: GuideTarget(_sheet ? 'status-sheet' : 'orders',
                  child: ConsoleButton('Status', key: const ValueKey('status-sheet'), small: true, active: _sheet, onPressed: () {
                    setState(() => _sheet = !_sheet);
                    if (_sheet) _clearForSheet();
                  }))),
        if (!narrow)
          Positioned(
              right: 16,
              top: 14,
              width: 300,
              bottom: (size.width + dockActions.length * Dock.tileWidth) / 2 + 8 <= size.width - 316 ? 14 : 100,
              child: SingleChildScrollView(child: rightCol)),
        Positioned(
            key: const ValueKey('banner'),
            left: 0,
            right: narrow && _sheet && size.width > _sheetWidth + 16 ? _sheetWidth + 8 : 0,
            top: narrow ? 90 : size.height * .21,
            child: IgnorePointer(child: Center(child: _BannerBox(c: _banner, h: _bannerH, p: _bannerP, red: _bannerRed)))),
        if (narrow && _sheet)
          Positioned(
              key: const ValueKey('status-sheet-open'),
              left: size.width > _sheetWidth + 16 ? null : 8,
              right: 8,
              width: size.width > _sheetWidth + 16 ? _sheetWidth : null,
              top: 40,
              bottom: 130,
              child: SingleChildScrollView(child: ColoredBox(color: C.deep, child: rightCol))),
        if (!narrow && feedWidth >= 200)
          Positioned(left: 16, bottom: 14, width: feedWidth, child: Feed(events: st.events, maxHeight: size.height - 14 - 8 - feedCeiling)),
        if (_hsub)
          Positioned(left: 0, right: 0, bottom: narrow ? 66 : 80, child: Center(child: IntrinsicWidth(child: ConsolePanel(flat: true, child: Row(mainAxisSize: MainAxisSize.min, children: [
            for (final k in const ['auto', 'm', 'c', 'd'])
              ConsoleButton(k == 'auto' ? 'Auto' : k == 'm' ? 'Fe' : k == 'c' ? 'Cr' : 'De', key: ValueKey('res-$k'), small: true, primary: _resPick == k, onPressed: () {
                _resPick = k;
                act('harvest', fromSub: true);
              }),
          ]))))),
        Positioned(
          left: narrow ? 6 : 0,
          right: narrow ? 6 : 0,
          bottom: narrow ? 6 : 14,
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            if (_dockHint != null)
              Padding(padding: const EdgeInsets.only(bottom: 4), child: Container(color: const Color(0xE607050A), padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2), child: Text(_dockHint!, style: T.mono(size: 10.5, color: C.text2)))),
            Align(alignment: Alignment.bottomCenter, child: _wrapDock(narrow, Dock(actions: dockActions, narrow: narrow, scanCooldown: _scanFill, scanSecs: _scanSecs, onAct: act, onHint: (h) => setState(() => _dockHint = h)))),
          ]),
        ),
      ];
      return ClipRect(
        child: Container(
          decoration: BoxDecoration(
            gradient: RadialGradient(radius: .75, colors: [_zoneTint(f.sector).withValues(alpha: .085), Colors.transparent]),
            color: const Color(0xFF07050A),
          ),
          child: Stack(children: geoStack),
        ),
      );
    });
  }

  Widget _wrapDock(bool narrow, Widget d) => narrow ? d : IntrinsicWidth(child: d);

  Color _zoneTint(proto.Hex h) => switch (h.zone) {
        proto.WorldZone.centralHub => C.a500,
        proto.WorldZone.innerRing => const Color(0xFFFF961E),
        proto.WorldZone.outerRing => const Color(0xFFFF5A28),
        proto.WorldZone.wandering => const Color(0xFFA05AFF),
      };

  double _shieldFrac() {
    var s = 0.0, m = 0.0;
    for (final u in sim.own) {
      s += u.shield;
      m += u.shieldMax;
    }
    return m > 0 ? s / m : 0;
  }

  void _pick(Offset p, Geo g) {
    String? h;
    bool near(Offset o, double r) => (p - o).distance < r;
    if (sim.hostiles.any((u) => !u.dead && near(g.wp(u.x, u.y), WindshieldSim.shipPx(u.cls) * g.sc * 2))) {
      h = 'h';
    } else if (sim.salvage != null && near(g.wp(sim.salvage!.x, sim.salvage!.y), g.rh * .08)) {
      h = 's';
    } else if (sim.derelict != null && near(g.wp(sim.derelict!.x, sim.derelict!.y), g.rh * .3)) {
      h = 'd';
    } else if (sim.atExchange && near(g.wp(.38, -.26), g.rh * .17)) {
      h = 'x';
    } else if (sim.outpost != null && near(g.wp(.38, -.26), g.rh * .17)) {
      h = 'p';
    } else if (sim.ast.any((a) => a.ore != null && near(g.wp(a.x, a.y), a.r * g.rh * 1.3))) {
      h = 'o';
    }
    if (h != sim.hoverT) setState(() => sim.hoverT = h);
  }
}

class _Alarm extends StatelessWidget {
  final bool on;
  const _Alarm({required this.on});
  @override
  Widget build(BuildContext context) {
    const c = Color(0x61FF3C1E);
    Widget edge(Alignment a, Alignment b, {double? w, double? h}) => Align(alignment: a, child: SizedBox(width: w, height: h, child: DecoratedBox(decoration: BoxDecoration(gradient: LinearGradient(begin: a, end: b, colors: const [c, Color(0x00FF3C1E)])))));
    return Opacity(
      opacity: on ? 1 : .45,
      child: Stack(fit: StackFit.expand, children: [
        edge(Alignment.topCenter, Alignment.bottomCenter, h: 90),
        edge(Alignment.bottomCenter, Alignment.topCenter, h: 90),
        edge(Alignment.centerLeft, Alignment.centerRight, w: 90),
        edge(Alignment.centerRight, Alignment.centerLeft, w: 90),
      ]),
    );
  }
}

class _BannerBox extends StatelessWidget {
  final AnimationController c;
  final String h, p;
  final bool red;
  const _BannerBox({required this.c, required this.h, required this.p, required this.red});
  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: c,
        builder: (_, _) {
          final v = c.value;
          if (!c.isAnimating || h.isEmpty) return const SizedBox.shrink();
          final still = reducedMotion(context);
          final op = v < .08 ? v / .08 : v < .84 ? 1.0 : (1 - v) / .16;
          return Opacity(
            opacity: op.clamp(0.0, 1.0),
            child: ClipRect(
              child: Align(
                heightFactor: still || v >= .08 ? 1 : .04 + .96 * (v / .08),
                child: Container(
                  padding: const EdgeInsets.fromLTRB(34, 12, 34, 11),
                  decoration: BoxDecoration(color: const Color(0xC707050A), border: Border.all(color: red ? C.ember : C.lineHi)),
                  child: Column(mainAxisSize: MainAxisSize.min, children: [
                    Text(h, style: T.cond(size: 32, color: red ? const Color(0xFFFFD6CC) : C.a100, weight: FontWeight.w700, spacing: 10).copyWith(shadows: [Shadow(color: red ? const Color(0x99FF3C1E) : const Color(0x73FFB000), blurRadius: 14)])),
                    const SizedBox(height: 6),
                    Text(p.toUpperCase(), style: T.mono(size: 11.5, color: red ? const Color(0xFFFF9A86) : C.a300, spacing: .7)),
                  ]),
                ),
              ),
            ),
          );
        },
      );
}
