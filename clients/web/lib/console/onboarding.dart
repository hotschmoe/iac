import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/scheduler.dart';

import '../models/fleet.dart';
import '../models/game_state.dart';
import '../protocol/protocol.dart' as proto;
import '../state/game_controller.dart';
import 'prefs.dart';
import 'services.dart' show Screen;

/// One thing the guide teaches. The guided lessons are walked in order, one
/// at a time, on the screen where they happen; the rest are first-hour goals
/// listed on the Overview. Every lesson is ticked by what the player does in
/// the game, in any order.
enum Lesson {
  token('Save your account token', '', guided: true, listed: false),
  build('Queue a building', 'Mines compound. Keep the build slot busy.', guided: true, screen: Screen.overview, target: 'build'),
  scan('Scan the lanes', 'Charts the lanes and contacts around a fleet.', guided: true, screen: Screen.windshield, target: 'dock-scan', keys: 'V'),
  jump('Jump through a gate', 'One hop per jump. Read the threat first.', guided: true, screen: Screen.windshield, target: 'gates', keys: 'Q W E A S D'),
  harvest('Harvest ore', 'Ore fills the hold every tick.', guided: true, screen: Screen.windshield, target: 'dock-harvest', keys: 'H'),
  unload('Recall and unload', 'Docking at home empties the hold into storage.', guided: true, screen: Screen.windshield, target: 'dock-recall', keys: 'R'),
  orders('Set standing orders', 'A fleet with orders flies itself.', guided: true, screen: Screen.windshield, target: 'orders', keys: 'P'),
  mine('Metal Mine to level 3', 'Metal pays for everything else.'),
  corvette('Build a Corvette', 'Needs a Shipyard. Your first combat hull.'),
  range('Fly 6 sectors from home', 'Ore and loot improve with distance. Watch the fuel home.'),
  board('Board a derelict', 'A violet hull on the scope. Clear hostiles, then board.'),
  fight('Win a fight', 'Read the ratio gauge first: FAVOURABLE or better.');

  final String title;
  final String hint;
  final bool guided;

  /// Shown on the first-hour list.
  final bool listed;

  /// Where the lesson is done, and the control it points at there.
  final Screen? screen;
  final String? target;
  final String? keys;
  const Lesson(this.title, this.hint, {this.guided = false, this.listed = true, this.screen, this.target, this.keys});
}

/// The first-five-minutes guide: which lessons this account has done, and
/// which one is next. Progress is read from the game state and events, and
/// remembered per account in [ConsolePrefs]. It starts by itself only for an
/// account created in this session; anyone else turns it on from the key
/// reference.
class Onboarding extends ChangeNotifier {
  Onboarding(this.game, this.prefs) {
    game.addListener(_onGame);
    _events = game.gameEvents.listen(_onEvent);
    _onGame();
  }

  final GameController game;
  final ConsolePrefs prefs;
  StreamSubscription<proto.GameEvent>? _events;

  /// A lesson was just done by play (not restored from an earlier session).
  void Function(Lesson)? onLearned;

  String? _account;
  bool _active = false;
  final Set<Lesson> _done = {};

  /// The fleet that harvested, which [Lesson.unload] waits to see at home.
  int? _hauler;

  /// Whether the windshield shows its small gate cards, which name a lane's
  /// key and terrain and nothing more. The notes that read a gate card
  /// follow it.
  bool _smallGates = false;
  bool _gone = false;

  /// The windshield says which gate cards it has laid out. It says so while
  /// it is laid out, so whoever listens hears after the frame.
  void gateCards({required bool small}) {
    if (small == _smallGates) return;
    _smallGates = small;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      if (!_gone) notifyListeners();
    });
  }

  bool get active => _active;
  bool done(Lesson l) => _done.contains(l);

  List<Lesson> get listed => [for (final l in Lesson.values) if (l.listed) l];

  /// The guided lessons that apply: the token only while there is one to save.
  List<Lesson> get guided => [
        for (final l in Lesson.values)
          if (l.guided && (l != Lesson.token || game.issuedToken != null || done(l))) l,
      ];

  /// The lesson the guide is on, or null when it is off or through.
  Lesson? get current => _active ? guided.where((l) => !done(l)).firstOrNull : null;

  String? get target => current?.target;

  /// Turning the guide on starts it over; turning it off is "skip all".
  void setActive(bool on) {
    if (_account == null || on == _active) return;
    _active = on;
    if (on) {
      _done.clear();
      _hauler = null;
      _observe(game.state, quiet: true);
    }
    _save();
    notifyListeners();
  }

  void tokenSaved() {
    game.acknowledgeToken();
    _learn(Lesson.token);
  }

  /// The sentence for [l] as things stand, seen from screen [at].
  String note(Lesson l, Screen at) {
    final s = game.state;
    if (l.screen == Screen.windshield) {
      if (s.fleets.isEmpty) return 'No fleet to fly. Build a Scout at the Homeworld shipyard.';
      if (at != Screen.windshield) return '${_ahead(l)} Open the Windshield.';
    }
    final fleet = game.currentFleet;
    switch (l) {
      case Lesson.token:
        final stored = game.issuedToken?.saved ?? true;
        return 'This token is the password for ${game.playerName}, and the server shows it once. '
            '${stored ? 'This browser has stored it; keep a copy for any other device.' : 'This browser could not store it: copy it now.'}';
      case Lesson.build:
        return 'Mines produce by themselves, and each level adds output. '
            '${at == Screen.overview ? 'Queue an upgrade from a marked card.' : 'Queue an upgrade on the Overview.'}';
      case Lesson.scan:
        return 'A scan charts the lanes and contacts around the fleet. Press SCAN.';
      case Lesson.jump:
        if (_smallGates) {
          return 'Each gate card is a lane, named by its key. An amber gate is quiet; any other colour is hostile or uncharted, and the Map says which. '
              '${_quietGates().isEmpty ? 'No gate here is quiet.' : 'The marked gates are quiet.'} Tap a gate to jump.';
        }
        return 'Each gate card is a lane: the T digit is its threat, the bars its ore. '
            '${_quietGates().isEmpty ? 'No gate here is quiet: hover each for your odds' : 'The marked gates are quiet. Hover any gate for your odds'}'
            ' and stay out of RISKY and DEADLY. Click a gate to jump.';
      case Lesson.harvest:
        final ore = s.sectors[fleet.sector]?.resources;
        final here = ore != null && ore.metal.index + ore.crystal.index + ore.deuterium.index > 0;
        if (here) return 'Ore in this sector. Press HARVEST, then AUTO: the hold fills every tick.';
        return _smallGates
            ? 'No ore in this sector. Jump on: the Ore layer of the Map shows which sectors have it.'
            : 'No ore in this sector. Jump on: ore shows as Fe, Cr and De bars on the gate cards.';
      case Lesson.unload:
        final hauler = s.fleets.where((f) => f.id == _hauler).firstOrNull ?? fleet;
        return '${hauler.cargo.fraction >= .8 ? 'Hold nearly full. ' : ''}'
            'RECALL flies the fleet home, and docking empties the hold into storage.';
      case Lesson.orders:
        return 'Tired of flying? A fleet with standing orders flies itself. '
            'Press ORDERS under Fleet status, choose MINE + RETURN and set it: the fleet harvests and comes home when the hold fills.';
      case Lesson.mine || Lesson.corvette || Lesson.range || Lesson.board || Lesson.fight:
        return l.hint;
    }
  }

  /// Whether the jump lesson points at the gate into [h]: the charted
  /// neighbours with no hostile contact, or every gate when none is quiet.
  bool pointsAtGate(proto.Hex h) {
    final quiet = _quietGates();
    return quiet.isEmpty || quiet.contains(h);
  }

  Set<proto.Hex> _quietGates() {
    final s = game.state;
    return {
      for (final h in s.sectors[game.currentFleet.sector]?.connections ?? const <proto.Hex>[])
        if (s.sectors[h] case final there? when there.hostiles?.isEmpty ?? true) h,
    };
  }

  String _ahead(Lesson l) => switch (l) {
        Lesson.scan => 'Your fleet is docked at home. Take the wheel.',
        Lesson.jump => 'The lanes are charted. Take a gate out.',
        Lesson.harvest => 'Ore is cargo you can bring home.',
        Lesson.orders => 'A fleet with standing orders flies itself.',
        _ => 'Bring the cargo home.',
      };

  void _onGame() {
    if (!game.hasState) return;
    final account = game.account;
    if (account == null) return;
    final loaded = account != _account;
    if (loaded) _load(account);
    if (_active) _observe(game.state, quiet: loaded);
  }

  void _load(String account) {
    _account = account;
    _done.clear();
    _hauler = null;
    final record = prefs.guide(account)?.split('|');
    if (record == null) {
      _active = game.newAccount;
      if (_active) _save();
    } else {
      _active = record.first == 'on';
      final names = record.elementAtOrNull(1)?.split(',') ?? const <String>[];
      _done.addAll(Lesson.values.where((l) => names.contains(l.name)));
      _hauler = int.tryParse(record.elementAtOrNull(2) ?? '');
    }
    notifyListeners();
  }

  void _save() {
    final account = _account;
    if (account == null) return;
    prefs.setGuide(account, '${_active ? 'on' : 'off'}|${_done.map((l) => l.name).join(',')}|${_hauler ?? ''}');
  }

  void _learn(Lesson l, {bool quiet = false}) {
    if (!_active || !_done.add(l)) return;
    _save();
    notifyListeners();
    if (!quiet) onLearned?.call(l);
  }

  void _observe(GameState s, {required bool quiet}) {
    void learn(Lesson l) => _learn(l, quiet: quiet);
    final home = s.homeworld;
    final hw = s.hw;
    if (hw != null) {
      if (hw.buildQueue.isNotEmpty || hw.buildPending.isNotEmpty) learn(Lesson.build);
      if (hw.buildings.any((b) => b.buildingType == proto.BuildingType.metalMine && b.level >= 3)) learn(Lesson.mine);
      if (hw.dockedShips.any((ship) => ship.shipClass == proto.ShipClass.corvette)) learn(Lesson.corvette);
    }
    for (final f in s.fleets) {
      if (f.sector != home) learn(Lesson.jump);
      if (f.status == FleetStatus.harvesting) {
        _hauler ??= f.id;
        learn(Lesson.harvest);
      }
      if (f.policy != null && f.policy != proto.PolicyPreset.manual.label) learn(Lesson.orders);
      if (proto.Hex.distance(f.sector, home) >= 6) learn(Lesson.range);
      if (f.ships.any((g) => g.shipClass == proto.ShipClass.corvette.label)) learn(Lesson.corvette);
    }
    if (done(Lesson.harvest) && !done(Lesson.unload)) {
      final hauler = s.fleets.where((f) => f.id == _hauler).firstOrNull;
      if (hauler == null || hauler.sector == home) learn(Lesson.unload);
    }
  }

  void _onEvent(proto.GameEvent e) {
    if (!_active) return;
    switch (e.kind) {
      case proto.ScanCompletedEvent():
        _learn(Lesson.scan);
      case proto.ResourceHarvestedEvent(:final fleetId):
        _hauler ??= fleetId;
        _learn(Lesson.harvest);
      case proto.BuildingCompletedEvent():
        _learn(Lesson.build);
      case proto.QueueEvent(:final queueType, :final action)
          when queueType == proto.QueueType.building && action != proto.QueueAction.cancelled:
        _learn(Lesson.build);
      case proto.ShipBuiltEvent(:final shipClass) when shipClass == proto.ShipClass.corvette:
        _learn(Lesson.corvette);
      case proto.SiteExploredEvent():
        _learn(Lesson.board);
      case proto.CombatEndedEvent(:final mine, :final playerVictory) when mine && playerVictory:
        _learn(Lesson.fight);
      case proto.PolicyActionEvent():
        _learn(Lesson.orders);
      default:
        break;
    }
  }

  @override
  void dispose() {
    _gone = true;
    game.removeListener(_onGame);
    _events?.cancel();
    super.dispose();
  }
}
