import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../console/prefs.dart';
import '../console/services.dart';
import '../console/sfx.dart';
import '../console/toasts.dart';
import '../design/beat.dart';
import '../design/lens.dart';
import '../design/tokens.dart';
import '../models/game_state.dart';
import '../state/game_controller.dart';
import 'homeworld/homeworld_view.dart';
import 'hud.dart';
import 'map/map_view.dart';
import 'overlays.dart';
import 'overview/overview_view.dart';
import 'rail.dart';
import 'windshield/windshield_view.dart';

/// The operator-console frame: HUD on top, rail left (bottom bar when narrow),
/// the four screens, and the overlays (toasts, help, alerts, `/` palette).
class Shell extends StatefulWidget {
  final GameController controller;
  final ConsolePrefs? prefs;
  final Screen initial;
  const Shell({super.key, required this.controller, this.prefs, this.initial = Screen.overview});

  @override
  State<Shell> createState() => _ShellState();
}

class _ShellState extends State<Shell> {
  late final ConsolePrefs _prefs = widget.prefs ?? ConsolePrefs();
  late final ToastController _toasts = ToastController();
  late final Console _con = Console(game: widget.controller, prefs: _prefs, toasts: _toasts, sfx: Sfx());
  late final FrameGovernor _gov = FrameGovernor(_prefs);

  late Screen _screen = widget.initial;
  bool _help = false, _alerts = false, _palette = false;
  String _paletteText = '';
  DateTime? _gAt;
  String _lastLogKey = '';

  @override
  void initState() {
    super.initState();
    _con.screen = _screen;
    _con.go = _go;
    _con.openPalette = ([t = '']) => setState(() {
          _palette = true;
          _paletteText = t;
        });
    _prefs.addListener(_onPrefs);
    _onPrefs();
    HardwareKeyboard.instance.addHandler(_onKey);
    widget.controller.addListener(_onGame);
    _lastLogKey = _logKey();
  }

  @override
  void dispose() {
    _prefs.removeListener(_onPrefs);
    HardwareKeyboard.instance.removeHandler(_onKey);
    widget.controller.removeListener(_onGame);
    _gov.dispose();
    _con.dispose();
    super.dispose();
  }

  void _onPrefs() => Beat.instance.interval = Duration(milliseconds: _prefs.quality == Quality.low ? 250 : 100);

  String _logKey() {
    final e = widget.controller.state.events;
    return e.isEmpty ? '' : '${e.first.tick}|${e.first.message}';
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
    if (!mounted || _palette || _typing()) return false;
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
      setState(() {
        _palette = true;
        _paletteText = '';
      });
      return true;
    }
    final lc = ch?.toLowerCase();
    if (_gAt != null && DateTime.now().difference(_gAt!) < const Duration(milliseconds: 1500)) {
      _gAt = null;
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
      _gAt = DateTime.now();
      _toasts.show('Go to', 'O overview, W windshield, M map, H homeworld', life: const Duration(milliseconds: 1500));
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
      'ov' || 'cc' || 'overview' => Screen.overview,
      'ws' || 'windshield' => Screen.windshield,
      'map' || 'sm' => Screen.map,
      'hw' || 'home' || 'homeworld' => Screen.homeworld,
      _ => null,
    };
    if (screen != null) {
      _go(screen);
      return;
    }
    widget.controller.handleCommand(cmd);
    final log = widget.controller.state.events;
    if (log.isNotEmpty) _toasts.show('Command', log.first.message);
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
            listenable: widget.controller,
            builder: (context, _) {
              final badges = {Screen.overview: widget.controller.state.alerts.where((a) => a.level == AlertTone.glow).length};
              return Stack(children: [
                Column(children: [
                  Hud(
                    narrow: narrow,
                    alertsOpen: _alerts,
                    onSound: _toggleSound,
                    onHelp: () => setState(() => _help = !_help),
                    onAlerts: () => setState(() => _alerts = !_alerts),
                  ),
                  Expanded(
                    child: Row(children: [
                      if (!narrow) Rail(active: _screen, bottom: false, onGo: _go, badges: badges),
                      Expanded(child: _screens()),
                    ]),
                  ),
                  if (narrow) Rail(active: _screen, bottom: true, onGo: _go, badges: badges),
                ]),
                ToastLayer(toasts: _toasts, narrow: narrow),
                if (_alerts) AlertsPopover(con: _con, narrow: narrow, onClose: () => setState(() => _alerts = false)),
                if (_help) HelpOverlay(con: _con, governor: _gov, onClose: () => setState(() => _help = false)),
                if (_palette)
                  CommandPalette(
                    narrow: narrow,
                    initial: _paletteText,
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
