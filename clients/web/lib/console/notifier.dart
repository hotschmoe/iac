import 'notifier_stub.dart' if (dart.library.js_interop) 'notifier_web.dart' as platform;

enum NotifyPermission { unsupported, undecided, granted, denied }

/// Operating-system notifications through the browser. [request] must be
/// called from a user gesture; [show] does nothing unless permission is
/// [NotifyPermission.granted].
abstract class Notifier {
  factory Notifier() => platform.createNotifier();
  NotifyPermission get permission;
  Future<NotifyPermission> request();

  /// A notice with the same [tag] replaces the one already showing.
  void show(String title, String body, {required String tag});
}

class NoNotifier implements Notifier {
  @override
  NotifyPermission get permission => NotifyPermission.unsupported;
  @override
  Future<NotifyPermission> request() async => NotifyPermission.unsupported;
  @override
  void show(String title, String body, {required String tag}) {}
}
