import 'package:web/web.dart' as web;

import 'sfx.dart';

Sfx createSfx() => WebSfx();

class WebSfx implements Sfx {
  @override
  bool enabled = false;
  web.AudioContext? _ac;

  void _tone(double f0, double f1, double dur, String type, double vol, [double delay = 0]) {
    final ac = _ac;
    if (ac == null) return;
    final t = ac.currentTime + delay;
    final o = ac.createOscillator();
    final g = ac.createGain();
    o.type = type;
    o.frequency.setValueAtTime(f0, t);
    o.frequency.exponentialRampToValueAtTime(f1 < 20 ? 20 : f1, t + dur);
    g.gain.setValueAtTime(vol, t);
    g.gain.exponentialRampToValueAtTime(.0001, t + dur);
    o.connect(g);
    g.connect(ac.destination);
    o.start(t);
    o.stop(t + dur + .02);
  }

  @override
  void play(String cue) {
    if (!enabled) return;
    try {
      _ac ??= web.AudioContext();
      switch (cue) {
        case 'click':
          _tone(900, 700, .05, 'square', .03);
        case 'jump':
          _tone(90, 1400, .7, 'sawtooth', .05);
          _tone(60, 300, .8, 'sine', .08);
        case 'scan':
          _tone(700, 700, .9, 'sine', .07);
          _tone(1400, 1400, .6, 'sine', .03, .15);
        case 'hit':
          _tone(220, 60, .12, 'square', .05);
        case 'fire':
          _tone(1200, 200, .14, 'sawtooth', .03);
        case 'alert':
          _tone(520, 520, .15, 'square', .05);
          _tone(390, 390, .25, 'square', .05, .18);
        case 'ok':
          _tone(660, 990, .12, 'triangle', .06);
          _tone(990, 1320, .14, 'triangle', .05, .1);
        case 'harvest':
          _tone(300, 420, .06, 'triangle', .03);
      }
    } catch (_) {}
  }
}
