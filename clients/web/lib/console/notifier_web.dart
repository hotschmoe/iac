import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:web/web.dart' as web;

import 'notifier.dart';

Notifier createNotifier() => WebNotifier();

class WebNotifier implements Notifier {
  bool get _supported => globalContext.has('Notification');

  NotifyPermission _map(String p) => switch (p) {
        'granted' => NotifyPermission.granted,
        'denied' => NotifyPermission.denied,
        _ => NotifyPermission.undecided,
      };

  @override
  NotifyPermission get permission {
    try {
      return _supported ? _map(web.Notification.permission) : NotifyPermission.unsupported;
    } catch (_) {
      return NotifyPermission.unsupported;
    }
  }

  @override
  Future<NotifyPermission> request() async {
    if (!_supported) return NotifyPermission.unsupported;
    try {
      return _map((await web.Notification.requestPermission().toDart).toDart);
    } catch (_) {
      return permission;
    }
  }

  @override
  void show(String title, String body, {required String tag}) {
    if (permission != NotifyPermission.granted) return;
    try {
      final n = web.Notification(title, web.NotificationOptions(body: body, tag: tag, icon: 'icons/Icon-192.png'));
      n.onclick = ((web.Event _) {
        web.window.focus();
        n.close();
      }).toJS;
    } catch (_) {}
  }
}
