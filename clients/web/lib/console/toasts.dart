import 'package:clock/clock.dart';
import 'package:flutter/foundation.dart';

enum ToastTone { amber, red, teal, violet }

/// A button on a toast; pressing it dismisses the toast first.
class ToastAction {
  final String label;
  final VoidCallback onPressed;
  final bool primary;
  const ToastAction(this.label, this.onPressed, {this.primary = false});
}

class Toast {
  final int id;
  final String title;
  final String message;
  final ToastTone tone;
  final DateTime until;
  final List<ToastAction> actions;
  Toast(this.id, this.title, this.message, this.tone, this.until, this.actions);
}

class ToastController extends ChangeNotifier {
  static const _most = 4;

  final List<Toast> items = [];
  int _id = 0;

  /// Raises a toast and returns its id, for [dismiss]. Past [_most] the
  /// oldest toast makes room, a plain one before one that asks something: a
  /// question stays until it is answered or runs out.
  int show(String title, String message,
      {ToastTone tone = ToastTone.amber, Duration life = const Duration(milliseconds: 4200), List<ToastAction> actions = const []}) {
    items.add(Toast(++_id, title, message, tone, clock.now().add(life), actions));
    while (items.length > _most) {
      final plain = items.indexWhere((t) => t.actions.isEmpty);
      items.removeAt(plain < 0 ? 0 : plain);
    }
    notifyListeners();
    return _id;
  }

  void dismiss(int id) {
    items.removeWhere((t) => t.id == id);
    notifyListeners();
  }

  /// Takes down the toasts that ask nothing, for a panel that opens where
  /// they stand. What they said is in the log; a question stays.
  void dismissPlain() {
    final before = items.length;
    items.removeWhere((t) => t.actions.isEmpty);
    if (items.length != before) notifyListeners();
  }
}
