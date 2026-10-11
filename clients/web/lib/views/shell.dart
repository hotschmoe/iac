import 'dart:async';

import 'package:clock/clock.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../console/notifier.dart';
import '../console/prefs.dart';
import '../console/services.dart';
import '../console/sfx.dart';
import '../console/toasts.dart';
import '../design/beat.dart';
import '../design/lens.dart';
import '../design/tokens.dart';
import '../models/game_state.dart';
import '../protocol/protocol.dart' as proto;
import '../state/game_controller.dart';
import 'homeworld/homeworld_view.dart';
import 'hud.dart';
import 'map/map_view.dart';
import 'onboarding.dart';
import 'overlays.dart';
import 'overview/overview_view.dart';
import 'rail.dart';
import 'windshield/windshield_view.dart';

/// The operator-console frame: HUD on top, the guide strip while a lesson is
/// open, rail left (bottom bar when narrow), the four screens, and the
/// overlays (toasts, help, alerts, `/` palette). It also tells the console's
/// `Attention` whether the page is in the foreground and puts its count of
/// missed notices in the tab title.
class Shell extends StatefulWidget {
  final GameController controller;
  final ConsolePrefs? prefs;
  final Notifier? notifier;
  final Screen initial;
  const Shell({super.key, required this.controller, this.prefs, this.notifier, this.initial = Screen.overview});

  static const title = 'IN AMBER CLAD';

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  late final ConsolePrefs _prefs = widget.prefs ?? ConsolePrefs();
  late final ToastController _toasts = ToastController();
  late final Console _con =
      Console(game: widget.controller, prefs: _prefs, toasts: _toasts, sfx: Sfx(), notifier: widget.notifier ?? Notifier());
  late final FrameGovernor _gov = FrameGovernor(_prefs);

  /// The toasts stand in another place on a phone; the key keeps the ones
  /// that are up when a window is dragged from one layout to the other.
  final GlobalKey _toastLayer = GlobalKey();
  late final AppLifecycleListener _lifecycle;

  late Screen _screen = widget.initial;
  bool _help = false, _alerts = false, _palette = false;
  DateTime? _gAt;
  int _gToast = 0;
  StreamSubscription<proto.ErrorMessage>? _refusals;
  String _lastLogKey = '';
  int _titled = 0;

  @override
  void initState() {
    super.initState();
    _con.screen = _screen;
    _con.go = _go;
    _prefs.addListener(_onPrefs);
    _onPrefs();
    HardwareKeyboard.instance.addHandler(_onKey);
    widget.controller.addListener(_onGame);
    _refusals = widget.controller.refusals.listen(_onRefusal);
    _lastLogKey = _logKey();
    _con.attention.addListener(_onAttention);
    _lifecycle = AppLifecycleListener(onStateChange: _onLifecycle);
    final now = WidgetsBinding.instance.lifecycleState;
    if (now != null) _onLifecycle(now);
  }

  void _onLifecycle(AppLifecycleState s) => _con.attention.away = s != AppLifecycleState.resumed;

  /// Set directly, not through a `Title` widget: a hidden page builds no frames.
  void _onAttention() {
    final unseen = _con.attention.unseen;
    if (unseen == _titled) return;
    _titled = unseen;
    SystemChrome.setApplicationSwitcherDescription(
        ApplicationSwitcherDescription(label: unseen > 0 ? '($unseen) ${Shell.title}' : Shell.title, primaryColor: C.a500.toARGB32()));
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    _con.attention.removeListener(_onAttention);
    _prefs.removeListener(_onPrefs);
    HardwareKeyboard.instance.removeHandler(_onKey);
    widget.controller.removeListener(_onGame);
    _refusals?.cancel();
    _gov.dispose();
    _con.dispose();
    super.dispose();
  }

  void _onPrefs() => Beat.instance.interval = Duration(milliseconds: _prefs.quality == Quality.low ? 250 : 100);

  String _logKey() {
    final e = widget.controller.state.events;
    return e.isEmpty ? '' : '${e.first.tick}|${e.first.message}';
  }

  /// A refused order is answered where the player is looking, in the world's
  /// own sentence, and stays long enough to be read.
  void _onRefusal(proto.ErrorMessage e) {
    _toasts.show(e.code == proto.ErrorCode.serverError ? 'Engine error' : 'Refused', e.message,
        tone: ToastTone.red, life: const Duration(seconds: 9));
    _con.play('alert');
  }

  /// Surface notable log lines as toasts.
  void _onGame() {
    final events = widget.controller.state.events;
    final key = _logKey();
    if (key == _lastLogKey) return;
    final fresh = <LogEntry>[];
    for (final e in events) {
      if ('${e.tick}|${e.message}' == _lastLogKey) break;
      fresh.add(e);
    }
    _lastLogKey = key;
    for (final e in fresh.take(2).toList().reversed) {
      if (e.message.startsWith('!')) {
        _toasts.show('Alert', e.message.replaceFirst(RegExp(r'^!+\s*'), ''), tone: ToastTone.red);
        _con.play('alert');
      } else if (e.message.startsWith('Building complete') || e.message.startsWith('Research complete') || e.message.startsWith('Shipyard:')) {
        _toasts.show('Complete', e.message, tone: ToastTone.teal);
        _con.play('ok');
      }
    }
  }

  void _go(Screen s) {
    if (s == _screen) return;
    setState(() {
      _screen = s;
      _con.screen = s;
      _alerts = false;
    });
  }

  bool _typing() {
    final ctx = FocusManager.instance.primaryFocus?.context;
    return ctx != null && (ctx.widget is EditableText || ctx.findAncestorWidgetOfExactType<EditableText>() != null);
  }

  void _toggleSound() {
    _prefs.setSound(!_prefs.sound);
    _con.play('click');
    _toasts.show('Sound', _prefs.sound ? 'On' : 'Muted');
  }

  bool _onKey(KeyEvent e) {
    if (e is! KeyDownEvent) return false;
    if (!mounted || _typing()) return false;
    if (_palette) {
      // The palette is open and its field has lost the keyboard (a click
      // elsewhere): Esc still closes it, or the hotkeys would stay off.
      if (e.logicalKey != LogicalKeyboardKey.escape) return false;
      setState(() => _palette = false);
      return true;
    }
    final hw = HardwareKeyboard.instance;
    if (hw.isControlPressed || hw.isMetaPressed || hw.isAltPressed) return false;
    final ch = e.character;
    final k = e.logicalKey;

    if (ch == '?') {
      setState(() {
        _help = !_help;
        _alerts = false;
      });
      return true;
    }
    if (k == LogicalKeyboardKey.escape) {
      if (_help || _alerts) {
        setState(() => _help = _alerts = false);
        return true;
      }
      return _con.dispatchKey(e);
    }
    if (_help || _alerts) return false;
    if (ch == '/') {
      setState(() => _palette = true);
      return true;
    }
    final lc = ch?.toLowerCase();
    if (_gAt != null && clock.now().difference(_gAt!) < const Duration(milliseconds: 1500)) {
      _gAt = null;
      _toasts.dismiss(_gToast);
      final t = switch (lc) {
        'o' => Screen.overview,
        'w' => Screen.windshield,
        'm' => Screen.map,
        'h' => Screen.homeworld,
        _ => null,
      };
      if (t != null) {
        _go(t);
        return true;
      }
    }
    if (lc == 'g') {
      _gAt = clock.now();
      _gToast = _toasts.show('Go to', 'O overview, W windshield, M map, H homeworld', life: const Duration(milliseconds: 1500));
      return true;
    }
    if (_con.dispatchKey(e)) return true;
    if (lc == 'm') {
      _toggleSound();
      return true;
    }
    return false;
  }

  void _runCommand(String raw) {
    final cmd = raw.trim();
    setState(() => _palette = false);
    if (cmd.isEmpty) return;
    final screen = switch (cmd.toLowerCase()) {
      '1' || 'ov' || 'cc' || 'overview' => Screen.overview,
      '2' || 'ws' || 'windshield' => Screen.windshield,
      '3' || 'map' || 'sm' => Screen.map,
      '4' || 'hw' || 'home' || 'homeworld' => Screen.homeworld,
      _ => null,
    };
    if (screen != null) {
      _go(screen);
      return;
    }
    final before = _logKey();
    widget.controller.handleCommand(cmd);
    final log = widget.controller.state.events;
    if (log.isEmpty) return;
    final added = log.indexWhere((e) => '${e.tick}|${e.message}' == before);
    final answer = (added < 0 ? log.length : added) - 1;
    _toasts.show(
        'Command',
        answer > 1
            ? 'The answer to "$cmd" is $answer lines: read it in the event log${_screen == Screen.overview ? '' : ' on the Overview (G then O)'}.'
            : log.first.message);
  }

  @override
  Widget build(BuildContext context) {
    return ConsoleScope(
      console: _con,
      child: Material(
        color: C.deep,
        child: LayoutBuilder(builder: (context, box) {
          final narrow = box.maxWidth < Bp.narrow;
          return ListenableBuilder(
            listenable: Listenable.merge([widget.controller, _con.onboarding]),
            builder: (context, _) {
              final badges = {Screen.overview: widget.controller.state.alerts.where((a) => a.level == AlertTone.glow).length};
              final lesson = _con.onboarding.current?.screen;
              final mark = lesson == _screen ? null : lesson;
              return Stack(children: [
                Column(children: [
                  Hud(
                    narrow: narrow,
                    alertsOpen: _alerts,
                    onSound: _toggleSound,
                    onHelp: () => setState(() => _help = !_help),
                    onAlerts: () => setState(() => _alerts = !_alerts),
                  ),
                  GuideStrip(screen: _screen, narrow: narrow),
                  Expanded(
                    child: Stack(fit: StackFit.expand, children: [
                      Row(children: [
                        if (!narrow) Rail(active: _screen, bottom: false, onGo: _go, badges: badges, mark: mark),
                        Expanded(child: _screens()),
                      ]),
                      if (narrow) ToastLayer(key: _toastLayer, toasts: _toasts, narrow: true),
                    ]),
                  ),
                  if (narrow) Rail(active: _screen, bottom: true, onGo: _go, badges: badges, mark: mark),
                ]),
                if (!narrow) ToastLayer(key: _toastLayer, toasts: _toasts, narrow: false),
                if (_alerts) AlertsPopover(con: _con, narrow: narrow, onClose: () => setState(() => _alerts = false)),
                if (_help) HelpOverlay(con: _con, governor: _gov, onClose: () => setState(() => _help = false)),
                if (_palette)
                  CommandPalette(
                    narrow: narrow,
                    onSubmit: _runCommand,
                    onClose: () => setState(() => _palette = false),
                  ),
              ]);
            },
          );
        }),
      ),
    );
  }

  Widget _screens() => IndexedStack(
        index: _screen.index,
        children: const [OverviewView(), WindshieldView(), MapView(), HomeworldView()],
      );
}
