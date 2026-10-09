import 'sfx_stub.dart' if (dart.library.js_interop) 'sfx_web.dart' as platform;

/// Synthesised cues. Off until the player turns sound on (browsers also
/// require a gesture before audio can start).
abstract class Sfx {
  factory Sfx() => platform.createSfx();
  bool enabled = false;
  void play(String cue);
}

class NoSfx implements Sfx {
  @override
  bool enabled = false;
  @override
  void play(String cue) {}
}
