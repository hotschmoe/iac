import '../protocol/protocol.dart' as proto;

enum IntelAge { unknown, live, fresh, aging, stale }

class Intel {
  final IntelAge age;
  final int ticksOld;
  const Intel(this.age, this.ticksOld);

  static const unknown = Intel(IntelAge.unknown, 1 << 30);

  bool get known => age != IntelAge.unknown;
  bool get live => age == IntelAge.live;
  bool get aged => age == IntelAge.aging || age == IntelAge.stale;

  /// 1.0 live down to .12 fossil, for fading and hatching.
  double get strength => known ? (1 - ticksOld / 6000).clamp(.12, 1.0) : 0;

  static Intel of(proto.SectorState? s, int tick) {
    if (s == null) return unknown;
    if (s.live) return const Intel(IntelAge.live, 0);
    final old = (tick - s.lastSeen).clamp(0, 1 << 30);
    final a = old <= 6
        ? IntelAge.live
        : old <= 300
            ? IntelAge.fresh
            : old <= 1800
                ? IntelAge.aging
                : IntelAge.stale;
    return Intel(a, old);
  }

  String get label => live ? 'LIVE' : 'seen ${ago(ticksOld)} ago';
}

/// Seconds until the salvage pile in [s] despawns, null when there is no pile
/// or no countdown. Zero or less means the pile is gone.
int? salvageSecondsLeft(proto.SectorState? s, int tick) {
  if (s == null || s.salvage == null || s.salvageDespawnTick == null) return null;
  return s.salvageDespawnTick! - tick;
}

/// A salvage pin worth drawing: a pile that exists and has not run out of time.
bool salvagePinVisible(proto.SectorState? s, int tick) {
  final pile = s?.salvage;
  if (pile == null || pile.total <= 0) return false;
  final left = salvageSecondsLeft(s, tick);
  return left == null || left > 0;
}

/// "drifts away in 1:05" for a pile with a countdown.
String? despawnLabel(proto.SectorState? s, int tick) {
  final left = salvageSecondsLeft(s, tick);
  return left == null || left <= 0 ? null : 'drifts away in ${clockFmt(left)}';
}

String ago(int t) => t < 60
    ? '${t}s'
    : t < 3600
        ? '${t ~/ 60}m'
        : t < 86400
            ? '${t ~/ 3600}h'
            : '${t ~/ 86400}d';

/// "1 hop", "20 hops".
String hopCount(int n) => '$n hop${n == 1 ? '' : 's'}';

/// "1,234,567".
String groupDigits(int v) {
  final s = v.toString();
  final b = StringBuffer();
  for (var i = 0; i < s.length; i++) {
    if (i > 0 && (s.length - i) % 3 == 0) b.write(',');
    b.write(s[i]);
  }
  return b.toString();
}

/// Ticks in a game hour, which is what the rules quote a rate by.
const ticksAnHour = 3600;

/// "77.9" or "1,204": what [perTick] comes to in a game hour, unsigned.
String anHour(double perTick) {
  final v = (perTick * ticksAnHour).abs();
  return v >= 100 ? groupDigits(v.round()) : v.toStringAsFixed(1);
}

/// "+77.9" or "-1,204": [anHour] with its sign, for a stockpile that may fall.
String signedAnHour(double perTick) => '${perTick < 0 ? '-' : '+'}${anHour(perTick)}';

/// "12", "1.5": [v] without decimals it does not have, at most [places].
String trimmed(double v, [int places = 2]) {
  final s = v.toStringAsFixed(places);
  return s.contains('.') ? s.replaceFirst(RegExp(r'\.?0+$'), '') : s;
}

/// A relic's share as a percentage: "12.5%", "8%".
String relicPct(double share) => '${trimmed(share * 100, 1)}%';

/// A relic's figure as the panel reads it: "+12.5% metal output" for a kind
/// that raises a figure, "-8% fuel burn" for one that cuts it.
String relicBonus(proto.RelicKind kind, double share) =>
    '${kind.effect == 'raises' ? '+' : '-'}${relicPct(share)} ${kind.label}';

/// "0:40", "1:02:05", "3d 4:10:22".
String clockFmt(int t) {
  t = t < 0 ? 0 : t;
  final d = t ~/ 86400, h = t % 86400 ~/ 3600, m = t % 3600 ~/ 60, s = t % 60;
  final ss = s.toString().padLeft(2, '0');
  final hms = '$h:${m.toString().padLeft(2, '0')}:$ss';
  return d > 0 ? '${d}d $hms' : h > 0 ? hms : '$m:$ss';
}
