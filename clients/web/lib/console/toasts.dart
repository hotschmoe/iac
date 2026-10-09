import 'package:flutter/foundation.dart';

enum ToastTone { amber, red, teal, violet }

class Toast {
  final int id;
  final String title;
  final String message;
  final ToastTone tone;
  final DateTime until;
  Toast(this.id, this.title, this.message, this.tone, this.until);
}

class ToastController extends ChangeNotifier {
  final List<Toast> items = [];
  int _id = 0;

  void show(String title, String message, {ToastTone tone = ToastTone.amber, Duration life = const Duration(milliseconds: 4200)}) {
    items.add(Toast(++_id, title, message, tone, DateTime.now().add(life)));
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
