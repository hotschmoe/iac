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
