import 'dart:async';

import 'package:flutter/foundation.dart';

import '../models/game_state.dart';
import '../protocol/protocol.dart' as proto;
import '../state/game_controller.dart';
import '../state/state_mapper.dart' show amountsOf, shortBy;
import 'battle.dart' show ticksOf;
import 'intel.dart' show clockFmt;
import 'notifier.dart';
import 'prefs.dart';
import 'toasts.dart';

/// Something that needs the player. A later notice with the same [tag]
/// replaces the browser notification of an earlier one.
class Notice {
  final String tag;
  final String title;
  final String body;
  final bool critical;
  const Notice(this.tag, this.title, this.body, {this.critical = false});
}

/// Watches the game for what needs the player: a queue that finished or went
/// idle, a raid, a lost fleet, a group that sets out after a fleet, full
/// storage, and at the outpost a raid, a
/// breach, a finished level, a full store and a repair that waits for its
/// materials. While the page is in the
/// background each one is counted for the tab title and, if the player
/// turned notifications on, raised through the browser; back in the
/// foreground the missed ones are shown as toasts. In the foreground the log
/// and its toasts already report them.
class Attention extends ChangeNotifier {
  Attention({required this.game, required this.prefs, required this.toasts, required this.notifier}) {
    game.addListener(_onGame);
    _events = game.gameEvents.listen(_batch.add);
  }

  final GameController game;
  final ConsolePrefs prefs;
  final ToastController toasts;
  final Notifier notifier;
  StreamSubscription<proto.GameEvent>? _events;

  /// A raid this close gets a second warning.
  static const raidClosingTicks = 60;

  final List<proto.GameEvent> _batch = [];
  final List<Notice> _missed = [];
  int _tick = -1;
  Map<proto.QueueType, bool>? _idle;
  int? _raidWarned;
  bool _outpostWatched = false;
  Set<proto.ResourceKind> _storeFull = const {};
  bool _repairWaits = false;
  bool _away = false;

  /// Notices raised since the page went to the background.
  int unseen = 0;

  bool get away => _away;

  /// The page went to the background or came back (the shell reports it).
  set away(bool v) {
    if (v == _away) return;
    _away = v;
    if (v || unseen == 0) return;
    for (final n in _missed) {
      toasts.show(n.title, n.body, tone: n.critical ? ToastTone.red : ToastTone.teal, life: const Duration(seconds: 9));
    }
    _missed.clear();
    unseen = 0;
    notifyListeners();
  }

  /// Why notifications cannot be turned on here, or null when they can.
  String? get blocked => switch (notifier.permission) {
        NotifyPermission.unsupported => 'This browser cannot show notifications.',
        NotifyPermission.denied => 'Blocked by the browser. Allow notifications for this site in its settings (they need HTTPS or localhost).',
        NotifyPermission.undecided || NotifyPermission.granted => null,
      };

  bool get enabled => prefs.notify && notifier.permission == NotifyPermission.granted;

  /// Only ever called from a button: the browser asks for permission here.
  Future<void> setEnabled(bool on) async {
    if (!on) {
      prefs.setNotify(false);
      toasts.show('Notifications', 'Off.');
      return;
    }
    var permission = notifier.permission;
    if (permission == NotifyPermission.undecided) permission = await notifier.request();
    if (permission == NotifyPermission.granted) {
      prefs.setNotify(true);
      toasts.show('Notifications', 'On. While this tab is in the background you are told when a queue finishes, a raid is inbound or a fleet is lost.', tone: ToastTone.teal);
    } else {
      toasts.show('Notifications', blocked ?? 'Not turned on: the browser was not given permission.', tone: ToastTone.red);
    }
  }

  void _offer() {
    if (_away || prefs.notifyAsked || prefs.notify || blocked != null) return;
    prefs.setNotifyAsked();
    toasts.show(
      'Tell you when this finishes?',
      'The console can raise a browser notification when a queue finishes, a raid is inbound or a fleet is lost while this tab is in the background. Change it any time under Keys and settings (?).',
      life: const Duration(seconds: 30),
      actions: [ToastAction('Notify me', () => setEnabled(true), primary: true), ToastAction('Not now', () {})],
    );
  }

  static Map<proto.QueueType, bool> _idleQueues(proto.HomeworldState hw) => {
        proto.QueueType.building: hw.buildQueue.isEmpty && hw.buildPending.isEmpty,
        proto.QueueType.research: hw.researchActive == null && hw.researchPending.isEmpty,
        proto.QueueType.ship: hw.shipyardQueue == null && hw.shipyardPending.isEmpty,
      };

  /// Notice titles per queue: an order finished with more to do, and the queue ran dry.
  static const _queueTitles = {
    proto.QueueType.building: ('Building complete', 'Build queue idle'),
    proto.QueueType.research: ('Research complete', 'Research lab idle'),
    proto.QueueType.ship: ('Shipyard order complete', 'Shipyard idle'),
  };

  void _onGame() {
    if (!game.hasState) return;
    final s = game.state;
    if (s.tick == _tick && _batch.isEmpty) return;
    _tick = s.tick;
    final events = List.of(_batch);
    _batch.clear();

    final out = <Notice>[];
    final finished = <proto.QueueType, List<String>>{};
    final lost = {
      for (final e in events)
        if (e.kind case proto.FleetDestroyedEvent(mine: true, :final fleetId)) fleetId,
    };
    for (final e in events) {
      switch (e.kind) {
        case proto.BuildingCompletedEvent(:final buildingType, :final newLevel):
          (finished[proto.QueueType.building] ??= []).add('${buildingType.label} level $newLevel');
        case proto.ResearchCompletedEvent(:final tech, :final newLevel):
          (finished[proto.QueueType.research] ??= []).add('${tech.label} level $newLevel');
        case proto.ShipBuiltEvent(:final shipClass, :final count):
          (finished[proto.QueueType.ship] ??= []).add('$count x ${shipClass.label}');
        case proto.DefenceBuiltEvent(:final defence, :final count):
          (finished[proto.QueueType.ship] ??= []).add('$count x ${defence.label}');
        case proto.QueueEvent(action: proto.QueueAction.started):
          _offer();
        case proto.RaidIncomingEvent(outpost: null, :final arrivalTick, :final estPower):
          final left = arrivalTick - s.tick;
          if (left <= raidClosingTicks) _raidWarned = arrivalTick;
          out.add(Notice('raid', 'Raid incoming', 'A raid of estimated power ${estPower.round()} reaches your homeworld in ${clockFmt(left)}.${_defence(s)}', critical: true));
        case proto.RaidResolvedEvent(outpost: null, :final defended, :final raidPower, :final defensePower, :final resourcesLost, :final shipsLost):
          out.add(defended
              ? Notice('raid', 'Raid repelled', 'Your defence of ${defensePower.round()} held against a raid of ${raidPower.round()}.')
              : Notice(
                  'raid',
                  'Raid broke through',
                  'A raid of ${raidPower.round()} beat your defence of ${defensePower.round()}: lost ${resourcesLost.metal.round()} Fe, ${resourcesLost.crystal.round()} Cr, ${resourcesLost.deuterium.round()} De'
                      '${shipsLost > 0 ? ' and $shipsLost docked ship${shipsLost == 1 ? '' : 's'}' : ''}.',
                  critical: true));
        case proto.RaidIncomingEvent(outpost: final at?, :final arrivalTick, :final estPower):
          out.add(Notice('outpost-raid', 'Raid on your outpost',
              'A raid of estimated power ${estPower.round()} reaches your outpost at ${at.q}, ${at.r} in ${clockFmt(arrivalTick - s.tick)}.${_outpostDefence(s)}',
              critical: true));
        case proto.RaidResolvedEvent(outpost: final at?, :final defended, :final raidPower, :final defensePower, :final resourcesLost, :final shipsLost):
          out.add(defended
              ? Notice('outpost-raid', 'Outpost raid repelled', 'The defence of ${defensePower.round()} at your outpost at ${at.q}, ${at.r} held against a raid of ${raidPower.round()}.')
              : Notice(
                  'outpost-raid',
                  'Outpost breached',
                  'A raid of ${raidPower.round()} beat the defence of ${defensePower.round()} at your outpost at ${at.q}, ${at.r}: it took ${amountsOf(resourcesLost)}'
                      '${shipsLost > 0 ? ' and destroyed $shipsLost ship${shipsLost == 1 ? '' : 's'} standing there' : ''}. '
                      'It makes nothing, fills no tank and has no platform until it has repaired itself from its store.',
                  critical: true));
        case proto.OutpostCompletedEvent(:final sector, :final level):
          out.add(Notice('outpost-level', level == 1 ? 'Outpost open' : 'Outpost level finished', 'Level $level of your outpost at ${sector.q}, ${sector.r} is finished.'));
        case proto.PursuitEvent(mine: true, phase: proto.PursuitPhase.began, :final label, :final fleetId, :final arrivesIn):
          out.add(Notice('pursuit-$fleetId', 'Fleet pursued',
              'The $label is after fleet F$fleetId${arrivesIn == null ? '' : ' and lands where it stands in ${ticksOf(arrivesIn)}'}.',
              critical: true));
        case proto.FleetDestroyedEvent(mine: true, :final fleetId, :final sector):
          out.add(Notice('fleet-$fleetId', 'Fleet lost', 'Fleet F$fleetId was destroyed in sector ${sector.q}, ${sector.r}. Every ship is gone.', critical: true));
        case proto.AlertEvent(level: proto.AlertLevel.critical, :final message, :final fleetId) when fleetId == null || !lost.contains(fleetId):
          out.add(Notice(fleetId == null ? 'alert' : 'fleet-$fleetId', 'Alert', message, critical: true));
        case proto.StorageFullEvent(:final resource):
          out.add(Notice('storage-${resource.name}', 'Storage full', 'Your ${resource.name} storage is full: production is wasted until you spend it or raise the ${resource.storage}.'));
        default:
          break;
      }
    }

    final hw = s.hw;
    if (hw != null) {
      final idle = _idleQueues(hw);
      final before = _idle;
      _idle = idle;
      for (final q in proto.QueueType.values) {
        final done = finished[q];
        final wentIdle = idle[q]! && before != null && !before[q]!;
        if (done == null && !wentIdle) continue;
        final (complete, ranDry) = _queueTitles[q]!;
        out.add(Notice(
          'queue-${q.name}',
          idle[q]! ? ranDry : complete,
          '${done == null ? '' : '${done.join(', ')}. '}${idle[q]! ? 'Nothing else is queued.' : 'The next order is under way.'}',
        ));
      }
    }

    final post = s.outpost;
    final full = {...?post?.yieldLost};
    final short = post?.repairWaitingFor;
    if (_outpostWatched && post != null) {
      final at = '${post.location.q}, ${post.location.r}';
      for (final k in full.difference(_storeFull)) {
        out.add(Notice('outpost-store-${k.name}', 'Outpost store full',
            'Your outpost at $at holds all the ${k.name} its store takes: the extractor makes no more of it until a fleet loads some.'));
      }
      final cost = post.repairCost;
      if (short != null && !_repairWaits && cost != null) {
        out.add(Notice('outpost-repair', 'Outpost repair waiting',
            'Your outpost at $at is breached and cannot repair itself: the repair costs ${amountsOf(cost)} and its store is ${shortBy(short)}. '
                'It makes nothing, fills no tank and has no platform until a fleet unloads that, or crystal or deuterium in its place.',
            critical: true));
      }
    }
    _outpostWatched = true;
    _storeFull = full;
    _repairWaits = short != null;

    final raid = s.raid;
    if (raid != null && _raidWarned != raid.arrivalTick) {
      final left = raid.arrivalTick - s.tick;
      if (left > 0 && left <= raidClosingTicks) {
        _raidWarned = raid.arrivalTick;
        final text = 'Raid of estimated power ${raid.estPower.round()} arrives in ${clockFmt(left)}.${_defence(s)}';
        out.add(Notice('raid', 'Raid closing', text, critical: true));
        game.note('! $text', level: EventLevel.bright);
      }
    }

    if (!_away || out.isEmpty) return;
    for (final n in out) {
      unseen++;
      _missed.add(n);
      if (_missed.length > 4) _missed.removeAt(0);
      if (enabled) notifier.show('${n.title} - In Amber Clad', n.body, tag: n.tag);
    }
    notifyListeners();
  }

  static String _defence(GameState s) {
    final hw = s.hw;
    return hw == null ? '' : ' Home defence is ${hw.homeDefencePower.round()}.';
  }

  static String _outpostDefence(GameState s) {
    final post = s.outpost;
    return post == null ? '' : ' Its platform and the ships standing there come to ${post.defencePower.round()}.';
  }

  @override
  void dispose() {
    game.removeListener(_onGame);
    _events?.cancel();
    super.dispose();
  }
}
