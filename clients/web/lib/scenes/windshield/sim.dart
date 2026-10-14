import 'dart:math' as math;
import 'dart:ui';

import '../../console/prefs.dart';
import '../../models/fleet.dart' as ui;
import '../../protocol/protocol.dart' as proto;
import '../battle/field.dart';
import 'geometry.dart';

const tau = math.pi * 2;

class Asteroid {
  double x, y, r, rot, vr, vx, vy;
  final String? ore;
  final double seed;
  final int n;
  Asteroid(this.x, this.y, this.r, this.rot, this.vr, this.vx, this.vy, this.ore, this.seed, this.n);
}

class Warp {
  final double ang;
  final Dir? dir;
  double t0;
  bool swapped;
  bool waiting;

  /// It began because the state said the fleet is leaving a fight, not with
  /// a jump ordered on this screen: it ends when the state stops saying so.
  final bool flight;
  Warp(this.ang, this.dir, this.t0, {this.swapped = false, this.waiting = false, this.flight = false});
}

class Star {
  double x, y;
  final double s, tw;
  Star(this.x, this.y, this.s, this.tw);
}

class Gas {
  final double x, y, r, o;
  final int n;
  Gas(this.x, this.y, this.r, this.o, this.n);
}

class Bit {
  final double x, y, r, v;
  double a;
  Bit(this.x, this.y, this.r, this.a, this.v);
}

class SalvagePile {
  final double x, y;
  final proto.Resources res;
  double fly = -1;
  SalvagePile(this.x, this.y, this.res);
}

class Derelict {
  final double x, y;
  final int tier;
  final proto.SiteRisk risk;
  Derelict(this.x, this.y, this.tier, this.risk);
}

/// What the scope prints on its centre line while a fight is on: the round
/// the fight stands at and how it stands, both the state's.
class FightStamp {
  final String round;
  final String? verdict;
  final Color color;
  const FightStamp(this.round, {this.verdict, this.color = const Color(0xFFFF5A3C)});
}

math.Random _seeded(proto.Hex h) => math.Random((h.q * 73856093) ^ (h.r * 19349663) ^ 0x1f2e3d);

/// Presentation-only scene state for the sector the active fleet is in.
/// Everything shown is derived from server data; this class owns the motion.
class WindshieldSim {
  WindshieldSim({int seed = 11}) : rng = math.Random(seed) {
    final r = math.Random(3);
    for (var l = 0; l < 3; l++) {
      stars.add([for (var i = 0; i < 70 + l * 20; i++) Star(r.nextDouble(), r.nextDouble(), r.nextDouble() * 1.4 + .3, r.nextDouble() * tau)]);
    }
  }

  final math.Random rng;
  final List<List<Star>> stars = [];

  int? fleetId;
  proto.Hex? loc;
  proto.Hex home = proto.Hex.origin;
  bool atHome = false;

  /// The fleet stands on a hub sector with the exchange (set by the view).
  bool atExchange = false;

  /// The outpost standing in the fleet's sector, whoever owns it (set by the
  /// view).
  proto.OutpostBrief? outpost;
  proto.SectorState? sector;
  ui.FleetState? fleet;

  /// The fleet and what it faces as marks, a mark a class: the scope's
  /// picture of both sides, and of a round's fire between them.
  final BattleField field = BattleField();

  /// Set by the view while the state lists a fight here.
  FightStamp? stamp;

  /// The mark under the pointer.
  Mark? hoverMark;
  List<Asteroid> ast = [];
  List<Gas> gas = [];
  List<Bit> bits = [];
  List<double> anomalyRings = [];
  SalvagePile? salvage;
  Derelict? derelict;

  Warp? warp;
  ({double t0})? pulse;
  int hover = -1;
  String? hoverT;
  String? target;
  bool combat = false;
  bool harvesting = false;
  String harvestRes = 'ore';

  /// A boarding under way: the ticks the server last said it has left, and
  /// when it said so.
  ({int left, double at})? boarding;
  double drift0 = 0, drift1 = -1;
  double now = 0;
  double time = 0;

  bool get reduced => field.reduced;
  set reduced(bool still) => field.reduced = still;
  Quality q = Quality.high;

  void Function(String h, String p, bool red)? onBanner;
  void Function()? onRetrace;

  set onCue(void Function(String cue)? play) {
    _cue = play;
    field.onCue = play;
  }

  void Function(String cue)? _cue;

  Geo? _geo;

  /// The scope's ground: each side has its half of the hex, which narrows
  /// away from the centre line.
  void layout(Geo g) {
    _geo = g;
    final rh = g.rh;
    double half(double from) => math.max(36.0, (rh - from / math.sqrt(3)) * .9);
    field.ground(
        ours: Ground(near: rh * .15, far: rh * .8, halfWidth: half),
        theirs: Ground(near: rh * .13, far: rh * .8, halfWidth: half),
        scale: (rh / 170).clamp(.8, 1.8));
  }

  /// Where the pursuer of the last pursuit comes in, and the tick the state
  /// last named it: the group lands a tick after the state stops saying so.
  ({Offset at, int tick})? _trail;

  // ── sync from server state ───────────────────────────────────

  /// Called whenever the controller notifies. [sides] is what stands in the
  /// fleet's sector as the state has it at [tick].
  void sync({required ui.FleetState? f, required proto.SectorState? s, required proto.Hex homeHex, Sides sides = Sides.none, int tick = 0}) {
    home = homeHex;
    fleet = f;
    sector = s;
    if (f == null) return;
    final newLoc = f.sector;
    final changedFleet = fleetId != f.id;
    final moved = loc != null && loc != newLoc;
    _followPursuit(f, tick);
    if (changedFleet || loc == null || moved) {
      final prev = loc;
      fleetId = f.id;
      loc = newLoc;
      atHome = newLoc == home;
      final w = warp;
      if (!changedFleet && moved) {
        if (w != null) {
          w.swapped = true;
          w.waiting = false;
          w.t0 = now - .55;
        } else if (prev != null) {
          final d = proto.Hex(newLoc.q - prev.q, newLoc.r - prev.r);
          final di = dirs.where((x) => x.v == d).firstOrNull;
          final ang = di?.angle ?? 0;
          warp = Warp(ang, di, now - .55, swapped: true);
          drift0 = -math.cos(ang);
          drift1 = -math.sin(ang);
        }
        build(f, s, sides);
        if (warp != null) _arrived(newLoc, s);
        return;
      }
      warp = null;
      build(f, s, sides);
      return;
    }
    update(f, s, sides);
  }

  void _followPursuit(ui.FleetState f, int tick) {
    final hunter = f.pursuedBy;
    final rh = _geo?.rh;
    if (hunter != null && rh != null) {
      final lane = dirs.where((d) => d.v == proto.Hex(hunter.from.q - f.sector.q, hunter.from.r - f.sector.r)).firstOrNull;
      if (lane != null) _trail = (at: Offset(math.cos(lane.angle), math.sin(lane.angle)) * rh * .866, tick: tick);
    }
    final trail = _trail;
    if (trail != null && tick - trail.tick > 1) _trail = null;
    field.hostileEntry = _trail?.at;
  }

  void _arrived(proto.Hex l, proto.SectorState? s) {
    if (!reduced) onRetrace?.call();
  }

  void build(ui.FleetState f, proto.SectorState? s, Sides sides) {
    final l = f.sector;
    final r = _seeded(l);
    atHome = l == home;
    ast = [];
    gas = [];
    bits = [];
    anomalyRings = [];
    salvage = null;
    derelict = null;
    target = null;
    hoverMark = null;
    boarding = null;
    final ore = _ore(s);
    final oreKeys = [for (final e in ore.entries) if (e.value > 0) e.key];
    final oreSum = ore.values.fold<int>(0, (a, b) => a + b);
    final terr = s?.terrain ?? proto.TerrainType.empty;
    final na = terr == proto.TerrainType.asteroidField ? 7 + oreSum : terr == proto.TerrainType.debrisField ? 3 : math.min(4, oreSum);
    for (var i = 0; i < na; i++) {
      final a = r.nextDouble() * tau, rr = .28 + r.nextDouble() * .55;
      final pick = oreKeys.isEmpty ? null : oreKeys[r.nextInt(oreKeys.length)];
      if ((math.cos(a - math.pi / 2)).abs() > .9 && rr < .4 && math.sin(a) > 0) continue;
      ast.add(Asteroid(math.cos(a) * rr, math.sin(a) * rr * .9 - .05, (.035 + r.nextDouble() * .05) * (terr == proto.TerrainType.asteroidField ? 1.2 : .8),
          r.nextDouble() * tau, (r.nextDouble() - .5) * .25, (r.nextDouble() - .5) * .004, (r.nextDouble() - .5) * .004, pick, r.nextDouble(), 7 + r.nextInt(4)));
    }
    if (terr == proto.TerrainType.nebula) {
      for (var i = 0; i < 3; i++) {
        gas.add(Gas(r.nextDouble() * 1.2 - .6, r.nextDouble() - .5, .18 + r.nextDouble() * .22, r.nextDouble() * tau, 5 + r.nextInt(3)));
      }
    }
    if (terr == proto.TerrainType.debrisField) {
      for (var i = 0; i < 46; i++) {
        bits.add(Bit(r.nextDouble() * 1.8 - .9, r.nextDouble() * 1.6 - .8, .004 + r.nextDouble() * .012, r.nextDouble() * tau, (r.nextDouble() - .5) * .3));
      }
    }
    if (terr == proto.TerrainType.anomaly) {
      for (var i = 0; i < 4; i++) {
        anomalyRings.add(.12 + i * .09);
      }
    }
    _syncProps(s, r);
    field.marks.clear();
    field.beams.clear();
    field.haze.clear();
    field.firing = false;
    field.setSides(sides, snap: true);
    _status(f);
  }

  Map<String, int> _ore(proto.SectorState? s) => {
        'metal': s?.resources.metal.index ?? 0,
        'crystal': s?.resources.crystal.index ?? 0,
        'deuterium': s?.resources.deuterium.index ?? 0,
      };

  void _syncProps(proto.SectorState? s, math.Random r) {
    final sv = s?.salvage;
    if (sv != null && sv.total > 0) {
      salvage ??= SalvagePile(-.35 + r.nextDouble() * .15, .12, sv);
      if (salvage!.res != sv) salvage = SalvagePile(salvage!.x, salvage!.y, sv)..fly = salvage!.fly;
    } else {
      salvage = null;
    }
    final site = s?.site;
    if (site != null) {
      derelict ??= Derelict(.36, -.2, site.tier, site.risk);
    } else {
      derelict = null;
    }
  }

  void update(ui.FleetState f, proto.SectorState? s, Sides sides) {
    atHome = f.sector == home;
    final r = _seeded(f.sector);
    _syncProps(s, r);
    field.setSides(sides);
    _status(f);
  }

  void _status(ui.FleetState f) {
    final wasHarvest = harvesting;
    harvesting = f.status == ui.FleetStatus.harvesting;
    if (harvesting && !wasHarvest) harvestRes = 'ore';
    if (f.status != ui.FleetStatus.exploring) {
      boarding = null;
    } else if (boarding?.left != f.cooldown) {
      boarding = (left: f.cooldown, at: now);
    }
    combat = f.underFire;
    final leaving = f.fleeing;
    final w = warp;
    if (leaving != null && w == null) {
      final lane = dirs.where((d) => d.v == proto.Hex(leaving.to.q - f.sector.q, leaving.to.r - f.sector.r)).firstOrNull;
      if (lane != null) _charge(lane, flight: true);
    } else if (leaving == null && w != null && w.flight && w.waiting) {
      cancelWarp();
    }
  }

  /// How far the boarding has come, 0 to 1, of the engine's boarding time:
  /// the ticks the fleet's state says are left, and the part of a tick that
  /// has passed since.
  double? get boardingProgress {
    final b = boarding;
    if (b == null) return null;
    final done = proto.exploreDurationTicks - b.left + math.min(1.0, now - b.at);
    return (done / proto.exploreDurationTicks).clamp(0.0, 1.0);
  }

  // ── actions ──────────────────────────────────────────────────

  void startScan() {
    pulse = (t0: now);
    _cue?.call('scan');
  }

  void startWarp(Dir d) {
    if (warp != null) return;
    _charge(d);
    _cue?.call('jump');
  }

  void _charge(Dir d, {bool flight = false}) {
    warp = Warp(d.angle, d, now, waiting: true, flight: flight);
    drift0 = -math.cos(d.angle);
    drift1 = -math.sin(d.angle);
  }

  /// End a jump the fleet never made: called off, refused, or not answered.
  /// The fleet stands in formation again.
  void cancelWarp() => warp = null;

  /// The lane the fleet is leaving by while it charges a jump out of a
  /// fight, and the group on its trail comes down.
  Dir? get exitLane => fleet?.fleeing == null ? null : warp?.dir;

  // ── server events ────────────────────────────────────────────

  /// A round of the fight in this sector is the scope's one source of fire
  /// and of losses: its volleys are drawn between the marks of the kinds
  /// that exchanged them, and what it destroyed leaves the marks when that
  /// fire has landed.
  void onEvent(proto.GameEvent e) {
    final k = e.kind;
    switch (k) {
      case proto.BattleRoundKind(battleRound: final round):
        if (round.sector != loc) return;
        combat = true;
        field.play(round);
      case proto.CombatEndedEvent():
        if (k.sector != loc) return;
        combat = false;
      default:
        break;
    }
  }

  // ── frame update ─────────────────────────────────────────────

  /// How far along its jump the fleet's formation is drawn, as a share of
  /// the hex's radius along the lane: out toward the gate while it charges,
  /// and in from the far side once it has landed. A fleet charging its way
  /// out of a fight stays in sight, under the fire it is taking.
  double _reach(Warp w) {
    var p = ((now - w.t0) / 1.1).clamp(0.0, 1.0);
    if (w.waiting) p = math.min(p, .5);
    if (p < .5) return math.pow(p / .5, 2.2).toDouble() * (fleet?.fleeing != null ? .2 : 1.0);
    final k = 1 - math.pow(1 - math.min(1.0, (p - .5) / .5), 3).toDouble();
    return -1.15 * (1 - k);
  }

  void tick(double dt, double nowS) {
    now = nowS;
    time = nowS;
    final w = warp;
    if (w != null) {
      final p = (now - w.t0) / 1.1;
      if (w.waiting && now - w.t0 > 2.6 && fleet?.fleeing == null) {
        cancelWarp();
      } else if (!w.waiting && p >= 1) {
        warp = null;
      }
    }
    final own = fleetId == null ? null : Formation.fleetId(fleetId!);
    final ww = warp;
    final rh = _geo?.rh ?? 280;
    field.offsets.clear();
    field.headings.clear();
    if (ww != null && own != null) {
      field.offsets[own] = Offset(math.cos(ww.ang), math.sin(ww.ang)) * rh * _reach(ww);
      field.headings[own] = ww.ang;
    }
    field.tick(dt, nowS);
    for (final a in ast) {
      a.rot += a.vr * dt;
      a.x += a.vx * dt;
      a.y += a.vy * dt;
      if (math.sqrt(a.x * a.x + a.y * a.y) > .9) {
        a.vx *= -1;
        a.vy *= -1;
      }
    }
    for (final b in bits) {
      b.a += b.v * dt;
    }
    final pl = pulse;
    if (pl != null && now - pl.t0 > 1.6) pulse = null;
    if (harvesting && ast.isNotEmpty && !reduced && rng.nextDouble() < .3) {
      final a = ast[rng.nextInt(ast.length)];
      final u = field.ours.where((m) => m.placed).firstOrNull;
      if (u != null && a.ore != null) {
        final from = Offset(a.x, a.y) * rh, to = u.lead;
        field.sparks.add(Spark(from.dx, from.dy, (to.dx - from.dx) * .9, (to.dy - from.dy) * .9, .5, .5, _oreColor(a.ore!), 4));
      }
    }
    _stars(dt);
  }

  static Color _oreColor(String k) => k == 'metal' ? const Color(0xFFD9B46A) : k == 'crystal' ? const Color(0xFF7FD4FF) : const Color(0xFF6FE6B6);

  double warpAmount() {
    final w = warp;
    if (w == null) return 0;
    var p = ((now - w.t0) / 1.1).clamp(0.0, 1.0);
    if (w.waiting) p = math.min(p, .5);
    return math.sin(p * math.pi) * (fleet?.fleeing != null ? .12 : 1);
  }

  void _stars(double dt) {
    final wv = warpAmount();
    for (var li = 0; li < stars.length; li++) {
      final sp = (li + 1) * .003;
      for (final st in stars[li]) {
        st.x = (st.x + drift0 * sp * (1 + wv * 40) * .02 + 1) % 1;
        st.y = (st.y + drift1 * sp * (1 + wv * 40) * .02 + 1) % 1;
      }
    }
  }
}
