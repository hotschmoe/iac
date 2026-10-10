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
  final List<Toast> items = [];
  int _id = 0;

  void show(String title, String message,
      {ToastTone tone = ToastTone.amber, Duration life = const Duration(milliseconds: 4200), List<ToastAction> actions = const []}) {
    items.add(Toast(++_id, title, message, tone, clock.now().add(life), actions));
    while (items.length > 4) {
      items.removeAt(0);
    }
    notifyListeners();
  }

  void dismiss(int id) {
    items.removeWhere((t) => t.id == id);
    notifyListeners();
  }
}
