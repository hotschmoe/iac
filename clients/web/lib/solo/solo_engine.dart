import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:stream_channel/stream_channel.dart';

import 'solo_port_stub.dart' if (dart.library.js_interop) 'solo_port_web.dart' as platform;

/// Whether this build is the single-player bundle (`scripts/play.sh
/// --single-player`): the engine worker ships beside it and is the front door.
const singlePlayerBuild = bool.fromEnvironment('IAC_SINGLE_PLAYER');

/// Where the engine worker stands (`solo/web/worker.js`).
enum SoloPhase {
  loading,

  /// No world: a new game can be started.
  idle,

  /// A save is loaded and its clock has not been started.
  held,
  running,

  /// Another tab is running this browser's world.
  locked,
  failed;

  static SoloPhase parse(Object? name) => values.firstWhere((p) => p.name == name, orElse: () => failed);
}

class SoloPreset {
  final String name;
  final double pace;

  /// Seconds a competent player needs for the first Cruiser and the endgame.
  final int firstCruiserS, endgameS;
  const SoloPreset({required this.name, required this.pace, required this.firstCruiserS, required this.endgameS});

  factory SoloPreset.fromJson(Map<String, dynamic> m) => SoloPreset(
        name: m['name'] as String,
        pace: (m['pace'] as num).toDouble(),
        firstCruiserS: (m['first_cruiser_s'] as num).toInt(),
        endgameS: (m['endgame_s'] as num).toInt(),
      );
}

/// What a new world may be made of, as the engine lists it.
class SoloOptions {
  final List<SoloPreset> presets;
  final List<String> personalities;
  final List<String> attentions;
  final int maxBots;

  /// The most ticks an absence is caught up.
  final int catchUpLimit;
  const SoloOptions({
    required this.presets,
    required this.personalities,
    required this.attentions,
    required this.maxBots,
    required this.catchUpLimit,
  });

  factory SoloOptions.fromJson(Map<String, dynamic> m) => SoloOptions(
        presets: [for (final p in m['presets'] as List) SoloPreset.fromJson(p as Map<String, dynamic>)],
        personalities: (m['personalities'] as List).cast<String>(),
        attentions: (m['attentions'] as List).cast<String>(),
        maxBots: (m['max_bots'] as num).toInt(),
        catchUpLimit: (m['catch_up_limit'] as num).toInt(),
      );
}

class SoloWorld {
  final String commander;
  final double pace;
  final String? preset;
  final String seed;
  final int tick;
  final int bots;

  /// Ticks the wall clock is ahead of the world.
  final int behind;

  /// Ticks played and still to play while an absence is being caught up.
  final ({int played, int remaining})? catchUp;

  /// What the last absence did, until the commander has logged in and read it.
  final List<String> notices;
  const SoloWorld({
    required this.commander,
    required this.pace,
    this.preset,
    required this.seed,
    required this.tick,
    required this.bots,
    required this.behind,
    this.catchUp,
    this.notices = const [],
  });

  factory SoloWorld.fromJson(Map<String, dynamic> m) {
    final c = m['catch_up'] as Map<String, dynamic>?;
    return SoloWorld(
      commander: m['commander'] as String,
      pace: (m['pace'] as num).toDouble(),
      preset: m['preset'] as String?,
      seed: m['seed'] as String,
      tick: (m['tick'] as num).toInt(),
      bots: (m['bots'] as num).toInt(),
      behind: (m['behind'] as num).toInt(),
      catchUp: c == null ? null : (played: (c['played'] as num).toInt(), remaining: (c['remaining'] as num).toInt()),
      notices: (m['notices'] as List? ?? const []).cast<String>(),
    );
  }
}

class SoloStatus {
  final SoloPhase phase;
  final SoloOptions? options;
  final SoloWorld? world;

  /// Why the save in this browser was not loaded.
  final String? refused;

  /// Why the last request was not carried out, or why the engine stopped.
  final String? error;

  /// Saves are not being written, and why.
  final String? warning;

  /// Ticks per second while catching up.
  final double rate;
  const SoloStatus({
    required this.phase,
    this.options,
    this.world,
    this.refused,
    this.error,
    this.warning,
    this.rate = 0,
  });

  static const loading = SoloStatus(phase: SoloPhase.loading);

  factory SoloStatus.failed(String error) => SoloStatus(phase: SoloPhase.failed, error: error);

  factory SoloStatus.fromJson(Map<String, dynamic> m) => SoloStatus(
        phase: SoloPhase.parse(m['phase']),
        options: m['options'] == null ? null : SoloOptions.fromJson(m['options'] as Map<String, dynamic>),
        world: m['world'] == null ? null : SoloWorld.fromJson(m['world'] as Map<String, dynamic>),
        refused: m['refused'] as String?,
        error: m['error'] as String?,
        warning: m['warning'] as String?,
        rate: (m['rate'] as num? ?? 0).toDouble(),
      );

  /// The world is caught up and its clock runs: a session can log in.
  bool get ready => phase == SoloPhase.running && world != null && world!.catchUp == null;
}

/// The line to the engine worker: typed text messages each way.
abstract interface class SoloPort {
  /// Called for every message from the worker.
  set onMessage(void Function(String type, String text) handler);
  void post(String type, [String text = '']);
  void close();
}

/// The engine that runs the world in this tab: `iac-solo` (the Rust engine
/// and its bots, as WebAssembly) in a worker, saved in this browser. It
/// speaks the server's wire protocol over [connect], so a session with it
/// is a session with a server; what a server has no word for (new game,
/// the save, catching up after an absence) goes through [status] and the
/// calls here.
class SoloEngine {
  SoloEngine([SoloPort? port]) : _port = port ?? platform.openPort() {
    _port.onMessage = _onMessage;
  }

  final SoloPort _port;
  final ValueNotifier<SoloStatus> _status = ValueNotifier(SoloStatus.loading);
  StreamController<String>? _session;

  ValueListenable<SoloStatus> get status => _status;

  void _onMessage(String type, String text) {
    switch (type) {
      case 'frame':
        _session?.add(text);
      case 'status':
        final SoloStatus next;
        try {
          next = SoloStatus.fromJson(jsonDecode(text) as Map<String, dynamic>);
        } catch (e) {
          _status.value = SoloStatus.failed('unreadable engine status: $e');
          return;
        }
        _status.value = next;
        if (next.phase == SoloPhase.failed) _endSession();
    }
  }

  void _endSession() {
    final session = _session;
    _session = null;
    session?.close();
  }

  /// Start a new world. [bots] are seats as the server's `--bot` takes them:
  /// `<personality>:<attention>[:<count>]`. A refusal comes back in
  /// [SoloStatus.error].
  void newGame({required String name, required String pace, required List<String> bots}) =>
      _port.post('new_game', jsonEncode({'name': name, 'pace': pace, 'bots': bots}));

  /// Start the clock of the loaded save: the ticks missed since it was
  /// written are played first.
  void resume() => _port.post('continue');

  /// Stop catching up and play from where the world stands.
  void takeCommand() => _port.post('take_command');

  /// Drop the world and its save.
  void abandon() => _port.post('abandon');

  /// Write the save now instead of at the next interval.
  void saveNow() => _port.post('save');

  /// A new session with the world; a session opened earlier ends.
  Future<StreamChannel<dynamic>> connect() async {
    _endSession();
    if (!_status.value.ready) throw StateError('the single-player world is not running');
    final incoming = StreamController<String>();
    final outgoing = StreamController<String>();
    _session = incoming;
    outgoing.stream.listen(
      (frame) {
        if (identical(_session, incoming)) _port.post('frame', frame);
      },
      onDone: () {
        if (!identical(_session, incoming)) return;
        _session = null;
        _port.post('close');
        incoming.close();
      },
    );
    _port.post('open');
    return StreamChannel<dynamic>(incoming.stream, outgoing.sink);
  }

  void dispose() {
    _endSession();
    _port.close();
    _status.dispose();
  }
}
