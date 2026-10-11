import 'package:flutter/material.dart';

import '../design/panel.dart';
import '../design/tokens.dart';
import '../solo/solo_engine.dart';

/// Opponent counts the new-game form offers, as far as the world has seats.
const soloBotCounts = [0, 4, 8, 12, 16, 20, 23];

/// Every attention pattern in turn, one bot after the other.
const soloMixedAttention = 'mixed';

/// At this pace and above a world is over in a day or less: bots that look
/// in a few times a day would hardly play, so they are flown hands-on.
const soloShortWorldPace = 70.0;

/// The seats of a new world: [count] bots, the personalities in turn, each
/// with [attention] (or every pattern in turn for [soloMixedAttention]), as
/// the engine's `<personality>:<attention>` specs.
List<String> soloSeats(SoloOptions options, int count, String attention) => [
      for (var i = 0; i < count; i++)
        '${options.personalities[i % options.personalities.length]}:'
            '${attention == soloMixedAttention ? options.attentions[i % options.attentions.length] : attention}',
    ];

/// "3d 4h", "8h 02m", "4m 10s".
String soloSpan(int seconds) {
  final d = seconds ~/ 86400, h = seconds ~/ 3600 % 24, m = seconds ~/ 60 % 60, s = seconds % 60;
  if (d > 0) return '${d}d ${h}h';
  if (h > 0) return '${h}h ${m.toString().padLeft(2, '0')}m';
  return '${m}m ${s.toString().padLeft(2, '0')}s';
}

/// The front door of the single-player build, and everything a server has
/// no word for: starting a world, continuing or abandoning the saved one,
/// the wait while an absence is caught up, and the account of it. Calls
/// [onEnter] once the world is running and the player should be in it,
/// saying whether the world was just started.
class SoloScreen extends StatefulWidget {
  final SoloEngine engine;
  final void Function({required bool newWorld}) onEnter;

  /// Leave single player for a server's login form.
  final VoidCallback? onMultiplayer;
  const SoloScreen({super.key, required this.engine, required this.onEnter, this.onMultiplayer});

  @override
  State<SoloScreen> createState() => _SoloScreenState();
}

class _SoloScreenState extends State<SoloScreen> {
  final TextEditingController _name = TextEditingController(text: 'Admiral');
  String _pace = 'season';
  int _bots = 8;
  String _attention = 'check-in';
  bool _confirmAbandon = false;
  bool _asked = false;
  bool _newWorld = false;
  bool _entered = false;

  @override
  void initState() {
    super.initState();
    widget.engine.status.addListener(_onStatus);
    WidgetsBinding.instance.addPostFrameCallback((_) => _onStatus());
  }

  @override
  void dispose() {
    widget.engine.status.removeListener(_onStatus);
    _name.dispose();
    super.dispose();
  }

  SoloStatus get _status => widget.engine.status.value;

  /// A world the player asked for (new, or continued with nothing to
  /// report) is entered as soon as it runs; a report waits to be read.
  void _onStatus() {
    if (!mounted) return;
    final s = _status;
    if (s.error != null && s.phase != SoloPhase.running) _asked = false;
    if (_asked && s.ready && s.world!.notices.isEmpty) _enter();
    setState(() {});
  }

  void _enter() {
    if (_entered) return;
    _entered = true;
    widget.onEnter(newWorld: _newWorld);
  }

  void _start() {
    final options = _status.options;
    final name = _name.text.trim();
    if (options == null || name.isEmpty) return;
    _asked = true;
    _newWorld = true;
    widget.engine.newGame(name: name, pace: _pace, bots: soloSeats(options, _bots, _attention));
  }

  void _continue() {
    _asked = true;
    _newWorld = false;
    widget.engine.resume();
  }

  void _setPace(SoloPreset preset) => setState(() {
        final wasShort = (_status.options?.presets.where((p) => p.name == _pace).firstOrNull?.pace ?? 0) >= soloShortWorldPace;
        final isShort = preset.pace >= soloShortWorldPace;
        _pace = preset.name;
        if (isShort != wasShort) _attention = isShort ? 'hands-on' : 'check-in';
      });

  @override
  Widget build(BuildContext context) {
    final s = _status;
    final world = s.world;
    final List<Widget> body = switch (s.phase) {
      SoloPhase.loading => [_line('LOADING THE ENGINE ...')],
      SoloPhase.failed => [
          _line('THE ENGINE STOPPED', color: C.ember),
          _note(s.error ?? 'no reason given'),
          _note('The save in this browser is as it was last written. Reload the page to start the engine again.'),
        ],
      SoloPhase.locked => [
          _line('THIS WORLD IS OPEN IN ANOTHER TAB', color: C.ember),
          _note('One tab runs the world at a time. Close the other tab, then reload this page.'),
        ],
      SoloPhase.idle => _newGame(s),
      SoloPhase.held => _saved(world!),
      SoloPhase.running when world?.catchUp != null => _catchingUp(s, world!),
      SoloPhase.running => _arrived(world),
    };
    return Container(
      color: C.void_,
      alignment: Alignment.center,
      child: SingleChildScrollView(
        child: SizedBox(
          width: 560,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('SINGLE PLAYER', style: T.cond(size: 18, color: C.a100, weight: FontWeight.w700, spacing: 4)),
                const SizedBox(height: 4),
                Text('THE WORLD RUNS IN THIS TAB AND IS SAVED IN THIS BROWSER', style: T.mono(size: 10, color: C.text3)),
                const SizedBox(height: 18),
                ...body,
                if (s.warning != null) ...[const SizedBox(height: 14), _note(s.warning!, color: C.ember, key: const Key('solo-warning'))],
                if (!s.exclusive) ...[
                  const SizedBox(height: 14),
                  _note(
                      'This page is not served over HTTPS or from this machine, so the browser cannot keep a second tab out of this world: '
                      'play it in one tab, or the two will overwrite each other\'s save.',
                      color: C.a300,
                      key: const Key('solo-one-tab')),
                ],
                if (!widget.engine.canProtectStorage) ...[
                  const SizedBox(height: 14),
                  _note(
                      'This page cannot ask the browser to keep its save, as one served over HTTPS or from this machine can: '
                      'the browser may clear the save when the device runs short of storage.',
                      color: C.a300,
                      key: const Key('solo-save-unkept')),
                ],
                if (widget.onMultiplayer != null && (s.phase == SoloPhase.idle || s.phase == SoloPhase.held || s.phase == SoloPhase.failed || s.phase == SoloPhase.locked)) ...[
                  const SizedBox(height: 22),
                  ConsoleButton('Connect to a server instead', onPressed: widget.onMultiplayer, small: true),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _line(String text, {Color color = C.a300}) =>
      Padding(padding: const EdgeInsets.only(bottom: 8), child: Text(text, style: T.cond(size: 13, color: color, spacing: 2)));

  Widget _note(String text, {Color color = C.text2, Key? key}) => Padding(
      padding: const EdgeInsets.only(bottom: 6), child: Text(text, key: key, style: T.mono(size: 11, color: color, height: 1.4)));

  Widget _label(String text) => Padding(padding: const EdgeInsets.only(top: 14, bottom: 6), child: Text(text, style: T.lbl));

  Widget _choices<V>(Iterable<V> values, V selected, String Function(V) label, void Function(V) pick) => Wrap(
        spacing: 6,
        runSpacing: 6,
        children: [
          for (final v in values)
            ConsoleButton(label(v), onPressed: () => pick(v), active: v == selected, small: true),
        ],
      );

  List<Widget> _newGame(SoloStatus s) {
    final options = s.options;
    if (options == null) return [_line('LOADING THE ENGINE ...')];
    final preset = options.presets.firstWhere((p) => p.name == _pace, orElse: () => options.presets.first);
    final counts = [for (final n in soloBotCounts) if (n <= options.maxBots) n];
    return [
      if (s.refused != null) ...[
        _line('THE SAVE IN THIS BROWSER WAS NOT LOADED', color: C.ember),
        _note(s.refused!, key: const Key('solo-refused')),
        _note('It is still stored. Starting a new game replaces it.'),
        const SizedBox(height: 10),
      ],
      _line('NEW GAME'),
      Row(
        children: [
          Text('>', style: T.mono(size: 14, color: C.a500)),
          const SizedBox(width: 8),
          Expanded(
            child: TextField(
              key: const Key('solo-name'),
              controller: _name,
              autofocus: true,
              maxLength: 24,
              style: T.mono(size: 14, color: C.a100),
              cursorColor: C.a500,
              decoration: InputDecoration(
                isDense: true,
                counterText: '',
                hintText: 'commander name',
                hintStyle: T.mono(size: 14, color: C.text4),
                enabledBorder: const UnderlineInputBorder(borderSide: BorderSide(color: C.line)),
                focusedBorder: const UnderlineInputBorder(borderSide: BorderSide(color: C.a500)),
              ),
              onSubmitted: (_) => _start(),
            ),
          ),
        ],
      ),
      _label('PACE (FIXED FOR THE LIFE OF THE WORLD)'),
      _choices<SoloPreset>(options.presets, preset, (p) => '${p.name} x${_multiplier(p.pace)}', _setPace),
      const SizedBox(height: 6),
      _note('A competent player reaches the first Cruiser in about ${soloSpan(preset.firstCruiserS)} and the endgame in about ${soloSpan(preset.endgameS)}.',
          color: C.text3),
      _label('BUILT-IN BOTS'),
      _choices<int>(counts, _bots, (n) => n == 0 ? 'none' : '$n', (n) => setState(() => _bots = n)),
      if (_bots > 0) ...[
        _label('HOW THE BOTS PLAY'),
        _choices<String>([...options.attentions, soloMixedAttention], _attention, (a) => a, (a) => setState(() => _attention = a)),
        const SizedBox(height: 6),
        _note(
            switch (_attention) {
              'check-in' => 'A few short look-ins a day: they queue, hand their fleets to standing orders and leave.',
              'hybrid' => 'Check-ins, and one session a day flown by hand.',
              'hands-on' => 'Always at the controls, through every hour you are away too. The most to watch, and the slowest to catch up after an absence.',
              _ => 'Every pattern in turn: check-in, hybrid and hands-on bots side by side.',
            },
            color: C.text3),
      ],
      if (s.error != null) ...[const SizedBox(height: 8), _note(s.error!, color: C.ember, key: const Key('solo-error'))],
      const SizedBox(height: 16),
      ConsoleButton('Start', onPressed: _start, primary: true),
      const SizedBox(height: 12),
      _note(
          'While this tab is closed the world waits, and the time away is played when you come back '
          '(up to ${soloSpan(options.catchUpLimit)}): the bots and your standing orders carry on through it.',
          color: C.text3),
    ];
  }

  String _multiplier(double pace) => pace == pace.roundToDouble() ? pace.round().toString() : pace.toString();

  List<Widget> _saved(SoloWorld w) => [
        _line('SAVED WORLD'),
        _note('${w.commander}  -  pace x${_multiplier(w.pace)}${w.preset == null ? '' : ' ${w.preset}'}  -  ${w.bots} bots  -  tick ${w.tick} (${soloSpan(w.tick)} played)',
            key: const Key('solo-saved')),
        _note(
            w.behind > 0
                ? 'Away ${soloSpan(w.behind)}: that time is played before you take command, with the bots and your standing orders acting through it.'
                : 'Saved a moment ago.',
            color: C.text3),
        const SizedBox(height: 14),
        if (!_confirmAbandon)
          Row(children: [
            ConsoleButton('Continue', onPressed: _continue, primary: true),
            const SizedBox(width: 8),
            ConsoleButton('Abandon this world', onPressed: () => setState(() => _confirmAbandon = true), danger: true),
          ])
        else ...[
          _note('Abandon ${w.commander}\'s world? The save is deleted from this browser and cannot be brought back.', color: C.ember),
          Row(children: [
            ConsoleButton('Delete the save', onPressed: () {
              setState(() => _confirmAbandon = false);
              widget.engine.abandon();
            }, danger: true),
            const SizedBox(width: 8),
            ConsoleButton('Keep it', onPressed: () => setState(() => _confirmAbandon = false)),
          ]),
        ],
      ];

  List<Widget> _catchingUp(SoloStatus s, SoloWorld w) {
    final c = w.catchUp!;
    final total = c.played + c.remaining;
    final eta = s.rate > 0 ? (c.remaining / s.rate).ceil() : null;
    return [
      _line('CATCHING UP'),
      _note('Away ${soloSpan(total)}. Played ${soloSpan(c.played)} of it, ${soloSpan(c.remaining)} to go.', key: const Key('solo-progress')),
      const SizedBox(height: 6),
      LinearProgressIndicator(
        value: total == 0 ? null : c.played / total,
        minHeight: 6,
        backgroundColor: C.a900,
        valueColor: const AlwaysStoppedAnimation(C.a500),
      ),
      const SizedBox(height: 8),
      _note(
          s.rate > 0
              ? '${s.rate.round()} ticks a second (a tick is a second of the world)${eta == null ? '' : ', about ${soloSpan(eta)} left'}.'
              : 'Measuring ...',
          color: C.text3),
      const SizedBox(height: 12),
      ConsoleButton('Take command now', onPressed: widget.engine.takeCommand),
      const SizedBox(height: 6),
      _note('Taking command now plays from tick ${w.tick}: the ${soloSpan(c.remaining)} still to go are skipped and never happen.', color: C.text3),
    ];
  }

  List<Widget> _arrived(SoloWorld? w) => [
        _line(w == null || w.notices.isEmpty ? 'STARTING ...' : 'WHILE YOU WERE AWAY'),
        if (w != null) ...[
          for (final n in w.notices) _note(n),
          if (w.notices.isNotEmpty) ...[
            const SizedBox(height: 12),
            ConsoleButton('Take command', onPressed: _enter, primary: true),
          ],
        ],
      ];
}
