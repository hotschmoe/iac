import 'dart:math' as math;
import 'dart:ui';

import '../../console/prefs.dart';
import '../../console/threat.dart';
import '../../models/fleet.dart' as ui;
import '../../protocol/protocol.dart' as proto;
import '../../design/symbols.dart';
import 'geometry.dart';

const tau = math.pi * 2;

class Unit {
  final int id;
  final proto.ShipClass cls;
  final bool enemy;
  double hull, hullMax, shield, shieldMax;
  double x = 0, y = 0, tx = 0, ty = 0, ang = 0, flash = 0;
  bool dead = false;
  double ph = 0, bx = 0, by = 0;
  Unit(this.id, this.cls, this.enemy, this.hull, this.hullMax, this.shield, this.shieldMax);
}

class Asteroid {
  double x, y, r, rot, vr, vx, vy;
  final String? ore;
  final double seed;
  final int n;
  Asteroid(this.x, this.y, this.r, this.rot, this.vr, this.vx, this.vy, this.ore, this.seed, this.n);
}

class Shot {
  final Unit a, t;
  final bool side;
  final double t0, dur;
  final double shield, hull;
  Shot(this.a, this.t, this.side, this.t0, this.dur, this.shield, this.hull);
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
  proto.SectorState? sector;
  ui.FleetState? fleet;

  List<Unit> own = [];
  List<Unit> hostiles = [];
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
  ({double t0, double dur})? boarding;
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
    _buildOwn(f, keepPos: false);
    _syncHostiles(s, fresh: true);
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

  void _buildOwn(ui.FleetState f, {required bool keepPos}) {
    final old = {for (final u in own) u.id: u};
    final out = <Unit>[];
    var synth = 1;
    for (final g in f.ships) {
      final cls = proto.ShipClass.values.firstWhere((c) => c.label.toLowerCase() == g.shipClass.toLowerCase(), orElse: () => proto.ShipClass.scout);
      final st = shipStats[cls]!;
      for (var i = 0; i < g.count; i++) {
        final id = i < g.ids.length ? g.ids[i] : f.id * 1000 + synth++;
        final prev = old[id];
        final hull = g.hull / g.count, hm = g.hullMax / g.count;
        final u = prev ?? Unit(id, cls, false, hull, hm, st.shield, st.shield);
        u.hull = hull;
        u.hullMax = hm;
        out.add(u);
      }
    }
    own = out;
    final live = own.where((u) => !u.dead).toList();
    for (var i = 0; i < live.length; i++) {
      final fm = _formation(i);
      live[i].tx = fm.dx * .1;
      live[i].ty = .22 + fm.dy * .075;
      if (!keepPos || (live[i].x == 0 && live[i].y == 0)) {
        live[i].x = live[i].tx;
        live[i].y = live[i].ty;
      }
      live[i].ang = 0;
    }
  }

  Offset _formation(int i) {
    const row = [Offset(0, 0), Offset(-1, .9), Offset(1, .9), Offset(-2, 1.8), Offset(2, 1.8), Offset(0, 1.8), Offset(-1, 2.7), Offset(1, 2.7)];
    return i < row.length ? row[i] : Offset(0, i * .4);
  }

  void _syncHostiles(proto.SectorState? s, {bool fresh = false}) {
    final hs = s?.hostiles;
    final want = <(proto.ShipClass, int)>[];
    for (final h in hs ?? const <proto.NpcFleetInfo>[]) {
      for (final sh in h.ships) {
        want.add((sh.shipClass, sh.count));
      }
    }
    final alive = hostiles.where((u) => !u.dead).length;
    final total = want.fold<int>(0, (a, e) => a + e.$2);
    if (total == 0) {
      if (!combat) hostiles = [];
      return;
    }
    if (fresh || hostiles.isEmpty || (!combat && alive != total)) {
      final r = _seeded(loc ?? proto.Hex.origin);
      var i = 0;
      final list = <Unit>[];
      for (final e in want) {
        for (var k = 0; k < e.$2; k++) {
          final st = shipStats[e.$1]!;
          final u = Unit(900 + i++, e.$1, true, st.hull, st.hull, st.shield, st.shield)
            ..x = (r.nextDouble() - .5) * .5
            ..y = -.5 + r.nextDouble() * .18
            ..ang = math.pi
            ..ph = r.nextDouble() * tau;
          u.bx = u.x;
          u.by = u.y;
          list.add(u);
        }
      }
      hostiles = list;
    }
  }

  void update(ui.FleetState f, proto.SectorState? s) {
    atHome = f.sector == home;
    final r = _seeded(f.sector);
    _syncProps(s, r);
    _buildOwn(f, keepPos: true);
    _syncHostiles(s);
    final wasHarvest = harvesting;
    harvesting = f.status == ui.FleetStatus.harvesting;
    if (harvesting && !wasHarvest) harvestRes = 'ore';
    final exploring = f.status == ui.FleetStatus.exploring;
    if (exploring && boarding == null) boarding = (t0: now, dur: 8);
    if (!exploring && boarding != null && derelict != null) boarding = null;
    if (!exploring && derelict == null) boarding = null;
    final nowCombat = f.status == ui.FleetStatus.combat;
    if (nowCombat != combat) {
      combat = nowCombat;
    }
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

  void cancelWarp() => warp = null;

  void placeArrival(double ang) {
    for (final u in own) {
      if (u.dead) continue;
      u.x = -math.cos(ang) * 1.15 + u.tx;
      u.y = -math.sin(ang) * 1.15 + u.ty;
    }
  }

  // ── server events ────────────────────────────────────────────

  Unit? _ownById(int id) {
    for (final u in own) {
      if (u.id == id && !u.dead) return u;
    }
    return null;
  }

  Unit? _pick(List<Unit> l, int salt) {
    final a = l.where((u) => !u.dead).toList();
    if (a.isEmpty) return null;
    return a[(salt.abs() + rng.nextInt(3)) % a.length];
  }

  void onEvent(proto.GameEvent e) {
    final k = e.kind;
    switch (k) {
      case proto.CombatRoundEvent():
        if (!k.mine) return;
        combat = true;
        final mineAttacks = k.attackerFleetId == fleetId;
        final a = mineAttacks ? (_ownById(k.attackerShipId) ?? _pick(own, k.attackerShipId)) : _pick(hostiles, k.attackerShipId);
        final t = mineAttacks ? _pick(hostiles, k.targetShipId) : (_ownById(k.targetShipId) ?? _pick(own, k.targetShipId));
        if (a == null || t == null) return;
        final delay = rng.nextDouble() * .5;
        shots.add(Shot(a, t, mineAttacks, now + delay, .26, k.shieldAbsorbed, k.hullDamage));
        onCue?.call('fire');
      case proto.ShipDestroyedEvent():
        if (k.sector != loc) return;
        Unit? u;
        if (k.isNpc) {
          final c = hostiles.where((x) => !x.dead && x.cls == k.shipClass).toList();
          u = c.isEmpty ? _pick(hostiles, 0) : c.first;
        } else {
          u = _ownById(k.shipId) ?? _pick(own, k.shipId);
        }
        if (u != null) {
          u.dead = true;
          _explode(u);
        }
      case proto.CombatEndedEvent():
        combat = false;
        for (final u in hostiles) {
          if (k.playerVictory && !u.dead) {
            u.dead = true;
            _explode(u);
          }
        }
      case proto.ScanCompletedEvent():
        break;
      default:
        break;
    }
  }

  void _burst(double x, double y, Color c, int n, [double sp = 1]) {
    for (var i = 0; i < n; i++) {
      final a = rng.nextDouble() * tau, v = _rnd(rng, .08, .45) * sp;
      sparks.add(Spark(x, y, math.cos(a) * v, math.sin(a) * v, _rnd(rng, .3, .8), .8, c, _rnd(rng, .02, .07)));
    }
  }

  void _explode(Unit t) {
    final sz = _pxOf(t.cls) / 300 * 1.2;
    _burst(t.x, t.y, const Color(0xFFFFCC66), 22, 1.5);
    _burst(t.x, t.y, const Color(0xFFFF5A3C), 14, 1);
    rings.add(Ring(t.x, t.y, now, sz));
    final def = t.enemy ? Sym.hostile[t.cls]! : Sym.own[t.cls]!;
    final vs = Sym.verts(def);
    for (var i = 0; i < vs.length && frags.length < 80; i++) {
      final a = math.atan2(vs[i].dy, vs[i].dx);
      frags.add(Frag(t.x, t.y, math.cos(a) * _rnd(rng, .08, .3), math.sin(a) * _rnd(rng, .08, .3), _rnd(rng, 0, tau), _rnd(rng, -4, 4),
          _rnd(rng, .7, 1.3), _rnd(rng, .01, .035) * (_geo?.rh ?? 280) / 280, 1.3, t.enemy ? const Color(0xFFFF5A3C) : const Color(0xFF5EE0CF)));
    }
    if (!reduced) shake = 10;
    onCue?.call('hit');
  }

  static double _pxOf(proto.ShipClass c) => switch (c) {
        proto.ShipClass.scout => 11,
        proto.ShipClass.corvette => 15,
        proto.ShipClass.frigate => 21,
        proto.ShipClass.cruiser => 29,
        proto.ShipClass.hauler => 19,
      };

  static double shipPx(proto.ShipClass c) => _pxOf(c);

  void _impact(Shot s) {
    final t = s.t;
    if (t.dead) return;
    t.flash = 1;
    t.shield -= math.min(t.shield, s.shield);
    if (t.enemy) t.hull -= s.hull;
    if (s.shield > 0) floats.add(Floater(t.x, t.y - .05, '-${s.shield.round()}', const Color(0xFF7FD4FF), now));
    if (s.hull > 0) floats.add(Floater(t.x + .02, t.y - .08, '-${s.hull.round()}', const Color(0xFFFF8A5C), now));
    final shieldOnly = s.shield > 0 && s.hull <= 0;
    _burst(t.x, t.y, shieldOnly ? const Color(0xFF7FD4FF) : const Color(0xFFFFCC66), shieldOnly ? 5 : 9);
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
        if (now - w.t0 > 2.6) {
          warp = null;
        }
      }
      if (p >= 1) {
        warp = null;
        for (final u in own) {
          u.x = u.tx;
          u.y = u.ty;
        }
      }
    }
    for (final u in hostiles) {
      if (u.dead) continue;
      u.flash *= .9;
      if (!combat) {
        u.x = u.bx + math.sin(time * .4 + u.ph) * .06;
        u.y = u.by + math.cos(time * .3 + u.ph) * .03;
        u.ang = math.pi + math.sin(time * .4 + u.ph) * .2;
      } else {
        final m = own.where((o) => !o.dead).firstOrNull;
        if (m != null) u.ang = math.atan2(m.x - u.x, -(m.y - u.y));
        final i = hostiles.indexOf(u);
        u.y += (-.34 + ((i % 2) * .1) - u.y) * .02;
        u.x += ((i - (hostiles.length - 1) / 2) * .24 - u.x) * .02;
      }
    }
    final ww = warp;
    for (final u in own) {
      if (u.dead) continue;
      u.flash *= .9;
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
