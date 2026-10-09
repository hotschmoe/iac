import 'dart:async';

import 'package:flutter/widgets.dart';

/// Shared 10 Hz stepped clock for blinks and flickers, so the whole UI does
/// not need an AnimationController per lamp. Ticks only while listened to.
class Beat extends ChangeNotifier {
  Beat._();
  static final Beat instance = Beat._();

  int step = 0;
  Timer? _timer;
  int _listeners = 0;
  Duration _interval = const Duration(milliseconds: 100);

  /// Step length; the quality governor slows it on weak machines.
  set interval(Duration d) {
    if (d == _interval) return;
    _interval = d;
    if (_timer != null) {
      _timer!.cancel();
      _timer = _start();
    }
  }

  Timer _start() => Timer.periodic(_interval, (_) {
        step++;
        notifyListeners();
      });

  @override
  void addListener(VoidCallback listener) {
    super.addListener(listener);
    _listeners++;
    _timer ??= _start();
  }

  @override
  void removeListener(VoidCallback listener) {
    super.removeListener(listener);
    if (--_listeners <= 0) {
      _listeners = 0;
      _timer?.cancel();
      _timer = null;
    }
  }
}

bool reducedMotion(BuildContext context) => MediaQuery.maybeOf(context)?.disableAnimations ?? false;

/// Builds with a stepped boolean: on for [onSteps] of every [period] steps (100 ms each).
class Blink extends StatelessWidget {
  final int period;
  final int onSteps;
  final Widget Function(BuildContext, bool on) builder;
  const Blink({super.key, this.period = 10, this.onSteps = 5, required this.builder});

  @override
  Widget build(BuildContext context) {
    if (reducedMotion(context)) return builder(context, true);
    return ListenableBuilder(
      listenable: Beat.instance,
      builder: (c, _) => builder(c, Beat.instance.step % period < onSteps),
    );
  }
}
