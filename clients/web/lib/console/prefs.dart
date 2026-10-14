import 'package:flutter/foundation.dart';

import 'kv_store.dart';

/// Rendering quality, driven by the frame governor unless pinned.
enum Quality { low, medium, high }

class ConsolePrefs extends ChangeNotifier {
  ConsolePrefs([KvStore? store]) : _kv = store ?? KvStore() {
    sound = _kv.read('iac.sound') == '1';
    retrace = _kv.read('iac.retrace') != '0';
    scanlines = _kv.read('iac.scanlines') != '0';
    reduceMotion = _kv.read('iac.reduce') == '1';
    notify = _kv.read('iac.notify') == '1';
    notifyAsked = _kv.read('iac.notify.asked') == '1';
    battleView = _kv.read('iac.battleview') == '1';
    final q = _kv.read('iac.quality');
    if (q != null && q != 'auto') {
      pinned = Quality.values.where((e) => e.name == q).firstOrNull;
    }
  }

  final KvStore _kv;
  bool sound = false;
  bool retrace = true;
  bool scanlines = true;

  /// Player override on top of the OS `prefers-reduced-motion`.
  bool reduceMotion = false;

  /// Browser notifications while the page is in the background. Only the
  /// player turns this on, and the browser's own permission still applies.
  bool notify = false;

  /// The in-context offer to turn [notify] on is made once per browser.
  bool notifyAsked = false;

  /// The battle view opens by itself when a fight of the player's begins.
  /// Off, a fight is watched where it is fought and the view is a press away.
  bool battleView = false;
  Quality? pinned;
  Quality governed = Quality.high;

  Quality get quality => pinned ?? governed;

  void _set(String k, String v) {
    _kv.write(k, v);
    notifyListeners();
  }

  void setSound(bool v) {
    sound = v;
    _set('iac.sound', v ? '1' : '0');
  }

  void setRetrace(bool v) {
    retrace = v;
    _set('iac.retrace', v ? '1' : '0');
  }

  void setScanlines(bool v) {
    scanlines = v;
    _set('iac.scanlines', v ? '1' : '0');
  }

  void setReduceMotion(bool v) {
    reduceMotion = v;
    _set('iac.reduce', v ? '1' : '0');
  }

  void setNotify(bool v) {
    notify = v;
    _set('iac.notify', v ? '1' : '0');
  }

  void setBattleView(bool v) {
    battleView = v;
    _set('iac.battleview', v ? '1' : '0');
  }

  void setNotifyAsked() {
    notifyAsked = true;
    _set('iac.notify.asked', '1');
  }

  /// Onboarding progress of one account (`GameController.account`).
  String? guide(String account) => _kv.read('iac.guide|$account');

  void setGuide(String account, String record) => _kv.write('iac.guide|$account', record);

  void setQuality(Quality? q) {
    pinned = q;
    _set('iac.quality', q?.name ?? 'auto');
  }

  void govern(Quality q) {
    if (q == governed) return;
    governed = q;
    if (pinned == null) notifyListeners();
  }
}
