import 'dart:math' as math;
import 'dart:ui';

import '../../console/battle.dart' show markScale, marksFor, shortCount, volleysByWeight;
import '../../console/prefs.dart';
import '../../models/fleet.dart' as ui;
import '../../protocol/protocol.dart' as proto;
import '../../design/symbols.dart';
import 'geometry.dart';

const tau = math.pi * 2;

/// One mark on the scope. It stands for [WindshieldSim.scale] ships of its
/// class, or for the last few of them: the scope never draws a ship each.
class Unit {
  final int id;
  final proto.ShipClass cls;
  final bool enemy;
  double x = 0, y = 0, tx = 0, ty = 0, ang = 0, flash = 0;

  /// A shield that took a hit lights up and fades.
  double screen = 0;
  bool dead = false;

  /// When a round's loss takes this mark off the scope, after the fire that
  /// did it has landed.
  double? dieAt;

  /// It has been given its place on the scope.
  bool placed = false;
  double ph = 0, bx = 0, by = 0;
  Unit(this.id, this.cls, this.enemy);

  bool get gone => dead || dieAt != null;
}

class Asteroid {
  double x, y, r, rot, vr, vx, vy;
  final String? ore;
  final double seed;
  final int n;
  Asteroid(this.x, this.y, this.r, this.rot, this.vr, this.vx, this.vy, this.ore, this.seed, this.n);
}

/// A stroke of fire from one mark to another, standing for a volley of the
/// round: [hull] when the volley reached hulls, otherwise shields took it.
class Shot {
  final Unit a, t;
  final bool side;
  final double t0, dur;
  final bool hull;
  Shot(this.a, this.t, this.side, this.t0, this.dur, {required this.hull});
}

class Spark {
  double x, y, vx, vy, l;
  final double m, len;
  final Color c;
  Spark(this.x, this.y, this.vx, this.vy, this.l, this.m, this.c, this.len);
}

class Frag {
  double x, y, vx, vy, rot, vr, l;
  final double len, m;
  final Color col;
  Frag(this.x, this.y, this.vx, this.vy, this.rot, this.vr, this.l, this.len, this.m, this.col);
}

class Ring {
  final double x, y, t0, r;
  Ring(this.x, this.y, this.t0, this.r);
}

class Floater {
  final double x, y, t0;
  final String text;
  final Color color;
  Floater(this.x, this.y, this.text, this.color, this.t0);
}

class Warp {
  final double ang;
  final Dir? dir;
  double t0;
  bool swapped;
  bool waiting;
  Warp(this.ang, this.dir, this.t0, {this.swapped = false, this.waiting = false});
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

double _rnd(math.Random r, double a, double b) => a + r.nextDouble() * (b - a);

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

  /// The most marks either side is drawn with, which every class of ship
  /// fits in with one mark each.
  static const marksASide = 16;

  /// The most volleys of a side a round draws, and strokes of fire a volley.
  static const _volleysDrawn = 6, _strokesAVolley = 3;

  /// Ships one mark stands for, the same for both sides so that the scope
  /// shows which is the larger: 1 while both fit in [marksASide].
  int scale = 1;

  List<Unit> own = [];
  List<Unit> hostiles = [];
  int _nextMark = 1;
  List<Asteroid> ast = [];
  List<Gas> gas = [];
  List<Bit> bits = [];
  List<double> anomalyRings = [];
  SalvagePile? salvage;
  Derelict? derelict;

  final List<Shot> shots = [];
  final List<Spark> sparks = [];
  final List<Frag> frags = [];
  final List<Ring> rings = [];
  final List<Floater> floats = [];
  final List<List<Offset>> ghosts = [];

  Warp? warp;
  ({double t0})? pulse;
  double shake = 0;
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

  bool reduced = false;
  Quality q = Quality.high;

  void Function(String h, String p, bool red)? onBanner;
  void Function()? onRetrace;
  void Function(String cue)? onCue;

  Geo? _geo;
  void layout(Geo g) => _geo = g;

  // ── sync from server state ───────────────────────────────────

  /// Called whenever the controller notifies.
  void sync({required ui.FleetState? f, required proto.SectorState? s, required proto.Hex homeHex}) {
    home = homeHex;
    fleet = f;
    sector = s;
    if (f == null) return;
    final newLoc = f.sector;
    final changedFleet = fleetId != f.id;
    final moved = loc != null && loc != newLoc;
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
        build(f, s);
        if (warp != null) {
          placeArrival(warp!.ang);
          _arrived(newLoc, s);
        }
        return;
      }
      build(f, s);
      return;
    }
    update(f, s);
  }

  void _arrived(proto.Hex l, proto.SectorState? s) {
    if (!reduced) onRetrace?.call();
  }

  void build(ui.FleetState f, proto.SectorState? s) {
    final l = f.sector;
    final r = _seeded(l);
    atHome = l == home;
    ast = [];
    gas = [];
    bits = [];
    anomalyRings = [];
    salvage = null;
    derelict = null;
    hostiles = [];
    target = null;
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
    _syncMarks(f, s, fresh: true);
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

  /// Set the marks from the state: so many of each class as [scale] makes
  /// of the fleet's rows and of the hostile groups' rows. A mark the state
  /// no longer accounts for goes without a sound; a loss in a fight is
  /// shown by the round that reports it ([onEvent]), which comes first.
  void _syncMarks(ui.FleetState f, proto.SectorState? s, {required bool fresh}) {
    final ownRows = [for (final g in f.ships) (g.shipClass, g.count)];
    final hostileRows = [
      for (final h in s?.hostiles ?? const <proto.NpcFleetInfo>[])
        for (final g in h.ships) (g.shipClass, g.count),
    ];
    scale = markScale([ownRows.map((r) => r.$2), hostileRows.map((r) => r.$2)], most: marksASide);
    own = _marks(fresh ? const [] : own, ownRows, enemy: false);
    hostiles = _marks(fresh ? const [] : hostiles, hostileRows, enemy: true);
    _formOwn(snap: fresh);
    _formHostiles(snap: fresh);
  }

  /// The marks of one side for [rows], keeping those of [old] that still
  /// stand for something. A mark a round is taking off the scope stays
  /// until it has gone, and counts for nothing.
  List<Unit> _marks(List<Unit> old, List<(proto.ShipClass, int)> rows, {required bool enemy}) {
    final want = <proto.ShipClass, int>{};
    for (final (cls, count) in rows) {
      want[cls] = (want[cls] ?? 0) + marksFor(count, scale);
    }
    final out = <Unit>[];
    for (final MapEntry(key: cls, value: n) in want.entries) {
      final standing = [for (final u in old) if (u.cls == cls && !u.gone) u];
      out.addAll(standing.take(n));
      for (var i = standing.length; i < n; i++) {
        out.add(Unit(_nextMark++, cls, enemy));
      }
    }
    out.addAll(old.where((u) => u.dieAt != null && !u.dead));
    return out;
  }

  /// How large the marks of each side are drawn, 1 at their full size:
  /// smaller when a side has more of them than its ground holds.
  double ownZoom = 1, hostileZoom = 1;

  /// What a mark takes of the scope at full size, as a share of the hex's
  /// radius for each pixel of [_pxOf]: its width and its length.
  static const _markWide = .0105, _markLong = .0145;

  /// The sizes [_rows] tries, largest first.
  static const _zooms = [1.0, .85, .7, .6, .5, .42, .35];

  /// [marks] set out in rows across a ground [width] wide, at the largest
  /// size whose rows are no deeper than [depth] in all: where each stands
  /// across and how far in, and the size.
  static (List<Offset>, double) _rows(List<Unit> marks, {required double width, required double depth}) {
    const gap = .012;
    for (final zoom in _zooms) {
      final places = <Offset>[];
      var row = <double>[], across = 0.0, deep = 0.0, long = 0.0;
      void close() {
        var x = -across / 2;
        for (final w in row) {
          places.add(Offset(x + w / 2, deep + long / 2));
          x += w + gap;
        }
        deep += long + gap;
        row = [];
        across = 0;
        long = 0;
      }

      for (final u in marks) {
        final w = _pxOf(u.cls) * _markWide * zoom;
        if (row.isNotEmpty && across + w > width) close();
        row.add(w);
        across += w + (row.length > 1 ? gap : 0);
        long = math.max(long, _pxOf(u.cls) * _markLong * zoom);
      }
      if (row.isNotEmpty) close();
      if (deep <= depth || zoom == _zooms.last) return (places, zoom);
    }
    return (const [], 1);
  }

  /// The fleet stands in rows below the centre of the scope.
  void _formOwn({required bool snap}) {
    final live = [for (final u in own) if (!u.gone) u];
    final (places, zoom) = _rows(live, width: 1.0, depth: .6);
    ownZoom = zoom;
    for (final (i, u) in live.indexed) {
      u.tx = places[i].dx;
      u.ty = .16 + places[i].dy;
      if (snap || !u.placed) {
        u.x = u.tx;
        u.y = u.ty;
        u.placed = true;
      }
    }
  }

  /// Hostile marks stand in rows above it, and drift about their places
  /// until a fight.
  void _formHostiles({required bool snap}) {
    final r = _seeded(loc ?? proto.Hex.origin);
    final live = [for (final u in hostiles) if (!u.gone) u];
    final (places, zoom) = _rows(live, width: 1.0, depth: .46);
    hostileZoom = zoom;
    for (final (i, u) in live.indexed) {
      final phase = r.nextDouble() * tau;
      u.tx = u.bx = places[i].dx;
      u.ty = u.by = -.24 - places[i].dy;
      if (snap || !u.placed) {
        u.x = u.bx;
        u.y = u.by;
        u.ang = math.pi;
        u.ph = phase;
        u.placed = true;
      }
    }
  }

  void update(ui.FleetState f, proto.SectorState? s) {
    atHome = f.sector == home;
    final r = _seeded(f.sector);
    _syncProps(s, r);
    _syncMarks(f, s, fresh: false);
    final wasHarvest = harvesting;
    harvesting = f.status == ui.FleetStatus.harvesting;
    if (harvesting && !wasHarvest) harvestRes = 'ore';
    if (f.status != ui.FleetStatus.exploring) {
      boarding = null;
    } else if (boarding?.left != f.cooldown) {
      boarding = (left: f.cooldown, at: now);
    }
    combat = f.underFire;
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

  void setCombat(bool v) => combat = v;

  // ── actions ──────────────────────────────────────────────────

  void startScan() {
    pulse = (t0: now);
    onCue?.call('scan');
  }

  void startWarp(Dir d) {
    if (warp != null) return;
    warp = Warp(d.angle, d, now, waiting: true);
    drift0 = -math.cos(d.angle);
    drift1 = -math.sin(d.angle);
    onCue?.call('jump');
  }

  /// End a jump the fleet never made: called off, refused, or not answered.
  /// The ships stand in formation again.
  void cancelWarp() {
    if (warp == null) return;
    warp = null;
    for (final u in own) {
      u.x = u.tx;
      u.y = u.ty;
    }
  }

  void placeArrival(double ang) {
    for (final u in own) {
      if (u.dead) continue;
      u.x = -math.cos(ang) * 1.15 + u.tx;
      u.y = -math.sin(ang) * 1.15 + u.ty;
    }
  }

  // ── server events ────────────────────────────────────────────

  /// A mark of [cls] still standing on [side], any of them.
  Unit? _markOf(List<Unit> side, proto.ShipClass? cls) {
    final marks = [for (final u in side) if (u.cls == cls && !u.gone) u];
    return marks.isEmpty ? null : marks[rng.nextInt(marks.length)];
  }

  /// A round of the fight in this sector is the scope's one source of fire
  /// and of losses: its volleys are drawn as strokes between the marks of
  /// the classes that exchanged them, and the units it reports lost take
  /// their marks with them.
  void onEvent(proto.GameEvent e) {
    final k = e.kind;
    switch (k) {
      case proto.BattleRoundKind(battleRound: final round):
        if (round.sector != loc) return;
        combat = true;
        _fire(round.fleets.volleys, own, hostiles, ours: true);
        _fire(round.hostiles.volleys, hostiles, own, ours: false);
        for (final g in round.fleets.groups) {
          if (g.fleetId == fleetId) _take(g.units, own);
        }
        _take([for (final g in round.hostiles.groups) ...g.units], hostiles);
        onCue?.call('fire');
      case proto.CombatEndedEvent():
        if (k.sector != loc) return;
        combat = false;
        for (final u in hostiles) {
          if (k.playerVictory && !u.dead) {
            u.dead = true;
            _explode(u);
          }
        }
      default:
        break;
    }
  }

  /// Strokes of fire for the heaviest of [volleys], more of them for a
  /// volley of many shots.
  void _fire(List<proto.Volley> volleys, List<Unit> from, List<Unit> at, {required bool ours}) {
    for (final v in volleysByWeight(volleys).take(_volleysDrawn)) {
      final strokes = v.shots >= 100 ? _strokesAVolley : v.shots >= 10 ? 2 : 1;
      for (var i = 0; i < strokes; i++) {
        final a = _markOf(from, v.from.ship), t = _markOf(at, v.at.ship);
        if (a == null || t == null) break;
        shots.add(Shot(a, t, ours, now + rng.nextDouble() * .45, .26, hull: v.hullDamage > 0));
      }
    }
  }

  /// What a round took from one side: the marks that no longer stand for
  /// any ship leave the scope once the fire has landed, and the loss is
  /// printed over the class.
  void _take(Iterable<proto.BattleUnits> units, List<Unit> side) {
    final left = <proto.ShipClass, int>{}, lost = <proto.ShipClass, int>{};
    for (final u in units) {
      final cls = u.unit.ship;
      if (cls == null) continue;
      left[cls] = (left[cls] ?? 0) + marksFor(u.count, scale);
      lost[cls] = (lost[cls] ?? 0) + u.lost;
    }
    for (final MapEntry(key: cls, value: n) in lost.entries) {
      if (n == 0) continue;
      final marks = [for (final u in side) if (u.cls == cls && !u.gone) u];
      if (marks.isEmpty) continue;
      for (final u in marks.reversed.take(math.max(0, marks.length - left[cls]!))) {
        u.dieAt = now + .75 + rng.nextDouble() * .3;
      }
      final over = marks.last;
      floats.add(Floater(over.x + .02, over.y - .07, '-${shortCount(n)}', over.enemy ? const Color(0xFFFFCC66) : const Color(0xFFFF8A5C), now + .75));
    }
  }

  void _burst(double x, double y, Color c, int n, [double sp = 1]) {
    for (var i = 0; i < n; i++) {
      final a = rng.nextDouble() * tau, v = _rnd(rng, .08, .45) * sp;
      sparks.add(Spark(x, y, math.cos(a) * v, math.sin(a) * v, _rnd(rng, .3, .8), .8, c, _rnd(rng, .02, .07)));
    }
  }

  void _explode(Unit t) {
    final sz = markPx(t) / 300 * 1.2;
    _burst(t.x, t.y, const Color(0xFFFFCC66), 22, 1.5);
    _burst(t.x, t.y, const Color(0xFFFF5A3C), 14, 1);
    rings.add(Ring(t.x, t.y, now, sz));
    final def = t.enemy ? Sym.hostile(t.cls) : Sym.own(t.cls);
    final vs = Sym.verts(def);
    for (var i = 0; i < vs.length && frags.length < 80; i++) {
      final a = math.atan2(vs[i].dy, vs[i].dx);
      frags.add(Frag(t.x, t.y, math.cos(a) * _rnd(rng, .08, .3), math.sin(a) * _rnd(rng, .08, .3), _rnd(rng, 0, tau), _rnd(rng, -4, 4),
          _rnd(rng, .7, 1.3), _rnd(rng, .01, .035) * (_geo?.rh ?? 280) / 280, 1.3, t.enemy ? const Color(0xFFFF5A3C) : const Color(0xFF5EE0CF)));
    }
    if (!reduced) shake = 10;
    onCue?.call('hit');
  }

  /// How large a class is drawn on the scope, in pixels at the reference
  /// scale: a drawing size, larger for the heavier hulls.
  static double _pxOf(proto.ShipClass c) => switch (c) {
        proto.ShipClass.espionageProbe => 8,
        proto.ShipClass.solarSatellite => 10,
        proto.ShipClass.lightFighter => 11,
        proto.ShipClass.heavyFighter => 15,
        proto.ShipClass.smallCargo => 16,
        proto.ShipClass.cruiser => 21,
        proto.ShipClass.largeCargo => 21,
        proto.ShipClass.recycler => 22,
        proto.ShipClass.battlecruiser => 25,
        proto.ShipClass.battleship => 28,
        proto.ShipClass.bomber => 28,
        proto.ShipClass.destroyer => 31,
        proto.ShipClass.deathstar => 44,
      };

  /// How large [u] is drawn, in pixels at the reference scale.
  double markPx(Unit u) => _pxOf(u.cls) * (u.enemy ? hostileZoom : ownZoom);

  void _impact(Shot s) {
    final t = s.t;
    if (t.dead) return;
    t.flash = 1;
    if (!s.hull) t.screen = 1;
    _burst(t.x, t.y, s.hull ? const Color(0xFFFFCC66) : const Color(0xFF7FD4FF), s.hull ? 9 : 5);
    onCue?.call('hit');
  }

  // ── frame update ─────────────────────────────────────────────

  void tick(double dt, double nowS) {
    now = nowS;
    time = nowS;
    final w = warp;
    if (w != null) {
      var p = (now - w.t0) / 1.1;
      if (w.waiting && p >= .5) {
        p = .5;
        if (now - w.t0 > 2.6 && fleet?.fleeing == null) cancelWarp();
      }
      if (p >= 1) {
        warp = null;
        for (final u in own) {
          u.x = u.tx;
          u.y = u.ty;
        }
      }
    }
    for (final u in [...own, ...hostiles]) {
      final at = u.dieAt;
      if (at != null && !u.dead && now >= at) {
        u.dead = true;
        _explode(u);
      }
    }
    for (final u in hostiles) {
      if (u.dead) continue;
      u.flash *= .9;
      u.screen *= .92;
      if (!combat) {
        u.x = u.bx + math.sin(time * .4 + u.ph) * .02;
        u.y = u.by + math.cos(time * .3 + u.ph) * .012;
        u.ang = math.pi + math.sin(time * .4 + u.ph) * .2;
      } else {
        final m = own.where((o) => !o.dead).firstOrNull;
        if (m != null) u.ang = math.atan2(m.x - u.x, -(m.y - u.y));
        u.y += (u.ty - u.y) * .04;
        u.x += (u.tx - u.x) * .04;
      }
    }
    final ww = warp;
    for (final u in own) {
      if (u.dead) continue;
      u.flash *= .9;
      u.screen *= .92;
      if (ww != null) {
        var p = ((now - ww.t0) / 1.1).clamp(0.0, 1.0);
        if (ww.waiting) p = math.min(p, .5);
        final dx = math.cos(ww.ang), dy = math.sin(ww.ang);
        if (p < .5) {
          final e = math.pow(p / .5, 2.2).toDouble();
          u.x = u.tx + dx * e * 1.0;
          u.y = u.ty + dy * e * 1.0;
        } else {
          final e = math.min(1.0, (p - .5) / .5), k = 1 - math.pow(1 - e, 3).toDouble();
          u.x = (u.tx - dx * 1.15) * (1 - k) + u.tx * k;
          u.y = (u.ty - dy * 1.15) * (1 - k) + u.ty * k;
        }
        u.ang = math.atan2(dy, dx) + math.pi / 2;
      } else {
        u.ang = combat ? 0 : u.ang * .9;
        u.x += (u.tx - u.x) * .08;
        u.y += (u.ty - u.y) * .08;
      }
    }
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
    for (var i = shots.length - 1; i >= 0; i--) {
      final s = shots[i];
      if (s.a.dead || s.t.dead) {
        shots.removeAt(i);
        continue;
      }
      if ((now - s.t0) / s.dur >= 1) {
        _impact(s);
        shots.removeAt(i);
      }
    }
    for (var i = sparks.length - 1; i >= 0; i--) {
      final p = sparks[i];
      p.l -= dt;
      if (p.l <= 0) {
        sparks.removeAt(i);
        continue;
      }
      p.x += p.vx * dt;
      p.y += p.vy * dt;
    }
    for (var i = frags.length - 1; i >= 0; i--) {
      final f = frags[i];
      f.l -= dt;
      if (f.l <= 0) {
        frags.removeAt(i);
        continue;
      }
      f.x += f.vx * dt;
      f.y += f.vy * dt;
      f.rot += f.vr * dt;
    }
    rings.removeWhere((r) => now - r.t0 > .7);
    floats.removeWhere((f) => now - f.t0 > 1.1);
    final pl = pulse;
    if (pl != null && now - pl.t0 > 1.6) pulse = null;
    if (shake > 0) {
      shake *= .86;
      if (shake < .4) shake = 0;
    }
    if (harvesting && ast.isNotEmpty && rng.nextDouble() < .3) {
      final a = ast[rng.nextInt(ast.length)];
      final u = own.where((o) => !o.dead).firstOrNull;
      if (u != null && a.ore != null) {
        sparks.add(Spark(a.x, a.y, (u.x - a.x) * .9, (u.y - a.y) * .9, .5, .5, _oreColor(a.ore!), .015));
      }
    }
    if (!reduced && (ww != null || combat)) {
      ghosts.add([for (final u in own) Offset(u.x, u.y)]);
      if (ghosts.length > 3) ghosts.removeAt(0);
    } else if (ghosts.isNotEmpty) {
      ghosts.clear();
    }
    _stars(dt);
  }

  static Color _oreColor(String k) => k == 'metal' ? const Color(0xFFD9B46A) : k == 'crystal' ? const Color(0xFF7FD4FF) : const Color(0xFF6FE6B6);

  double warpAmount() {
    final w = warp;
    if (w == null) return 0;
    var p = ((now - w.t0) / 1.1).clamp(0.0, 1.0);
    if (w.waiting) p = math.min(p, .5);
    return math.sin(p * math.pi);
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
