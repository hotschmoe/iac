import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui';

import '../../console/battle.dart' show groupName, shortCount, volleysByWeight;
import '../../design/symbols.dart';
import '../../models/fleet.dart' as ui;
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;

const _tau = math.pi * 2;

/// The units of one kind in a formation, as the state or a round counts
/// them. [hullMax] is 0 where the state gives a count and no hull.
class Figures {
  final proto.ShipyardItem unit;
  final int count, damaged, atRisk, hull, hullMax;
  const Figures(this.unit, this.count, {this.damaged = 0, this.atRisk = 0, this.hull = 0, this.hullMax = 0});

  Figures.ofRow(proto.ShipGroupState g)
      : this(proto.ShipyardItem.ship(g.shipClass), g.count, damaged: g.damaged, atRisk: g.atRisk, hull: g.hull, hullMax: g.hullMax);

  Figures.ofUnits(proto.BattleUnits u) : this(u.unit, u.count, damaged: u.damaged, atRisk: u.atRisk, hull: u.hull, hullMax: u.hullMax);
}

/// One fleet, hostile group or battery as a scene draws it: a mark for each
/// kind of unit it has.
class Formation {
  /// The same from tick to tick, so that a mark keeps its place: a fleet's
  /// id, a group's, [batteryId] for the defences of the place raided and
  /// [raidId] for the raiders.
  final String id;
  final String name;
  final bool hostile;
  final bool mine;
  final bool battery;

  /// It is leaving the fight: fired on, and firing nothing.
  final bool fleeing;
  final List<Figures> units;
  const Formation(
      {required this.id, required this.name, required this.units, this.hostile = false, this.mine = false, this.battery = false, this.fleeing = false});

  static const raidId = 'raid', batteryId = 'battery';

  static String fleetId(int id) => 'f$id';

  /// The id of a round's fleet, group or battery.
  static String idOf(proto.BattleGroup g, {required bool hostile, required bool raid}) =>
      g.battery ? batteryId : hostile ? (raid ? raidId : 'g${g.fleetId}') : fleetId(g.fleetId);

  Formation.ofFleet(ui.FleetState f)
      : this(id: fleetId(f.id), name: f.name, mine: true, fleeing: f.fleeing != null, units: [for (final g in f.ships) Figures.ofRow(g)]);

  Formation.ofGroup(proto.NpcFleetInfo g)
      : this(id: 'g${g.id}', name: g.behavior.wire.toUpperCase(), hostile: true, units: [for (final row in g.ships) Figures.ofRow(row)]);

  Formation.ofBattleGroup(proto.BattleGroup g, {required bool hostile, required bool raid})
      : this(
            id: idOf(g, hostile: hostile, raid: raid),
            name: groupName(g).toUpperCase(),
            hostile: hostile,
            mine: g.mine,
            battery: g.battery,
            fleeing: g.fleeing,
            units: [for (final u in g.units) Figures.ofUnits(u)]);
}

/// The two sides of a scene.
class Sides {
  final List<Formation> ours;
  final List<Formation> theirs;
  const Sides(this.ours, this.theirs);

  static const none = Sides([], []);

  /// Both sides as [round] left them, every fleet, group and battery of it.
  Sides.ofRound(proto.BattleRoundEvent round)
      : this([for (final g in round.fleets.groups) Formation.ofBattleGroup(g, hostile: false, raid: round.raid)],
            [for (final g in round.hostiles.groups) Formation.ofBattleGroup(g, hostile: true, raid: round.raid)]);

  /// What stands at [at], as the state has it. While a fight is fought
  /// there it is the fight's last round, and before its first round the
  /// fleets, the battery and the raiders or groups that will stand in it.
  /// At rest it is [fleet], the fleet the scene is watched from, and the
  /// hostile groups in sight.
  factory Sides.at(GameState st, proto.Hex at, {ui.FleetState? fleet}) {
    final fight = st.battles[at];
    final live = fight != null && !fight.over;
    final watched = fleet != null && fleet.sector == at ? fleet : null;
    final round = live ? fight.round : null;
    if (round != null) {
      final fought = Sides.ofRound(round);
      final listed = {for (final f in fought.ours) f.id};
      return Sides([
        ...fought.ours,
        if (watched != null && !listed.contains(Formation.fleetId(watched.id))) Formation.ofFleet(watched),
      ], fought.theirs);
    }
    final raid = live ? fight.raid : null;
    if (raid != null) {
      return Sides([
        for (final f in st.fleets)
          if (f.sector == at) Formation.ofFleet(f),
        ?_battery(st, at),
      ], [
        Formation(id: Formation.raidId, name: 'RAIDERS', hostile: true, units: [
          for (final s in raid.ships) Figures(proto.ShipyardItem.ship(s.shipClass), s.count),
        ]),
      ]);
    }
    return Sides([
      if (watched != null) Formation.ofFleet(watched),
    ], [
      for (final g in st.sectors[at]?.hostiles ?? const <proto.NpcFleetInfo>[]) Formation.ofGroup(g),
    ]);
  }

  /// The defences a raid at [at] will meet before its first round is
  /// fought: the homeworld's Solar Satellites and defences, or the
  /// outpost's platform.
  static Formation? _battery(GameState st, proto.Hex at) {
    final units = <Figures>[];
    var name = 'HOME DEFENCES';
    if (at == st.homeworld) {
      final hw = st.hw;
      if (hw == null) return null;
      if (hw.satellites > 0) units.add(Figures(const proto.ShipyardItem.ship(proto.ShipClass.solarSatellite), hw.satellites));
      units.addAll([
        for (final d in hw.defences)
          if (d.count > 0) Figures(proto.ShipyardItem.defence(d.kind), d.count),
      ]);
    } else if (st.outpost case final post? when post.location == at) {
      name = 'OUTPOST PLATFORM';
      units.addAll([for (final u in post.platform) Figures(u.unit, u.count)]);
    }
    return units.isEmpty ? null : Formation(id: Formation.batteryId, name: name, mine: true, battery: true, units: units);
  }
}

/// The short name a mark carries: the command bar's where it has one of
/// two or three letters.
String tagOf(proto.ShipyardItem u) => switch (u.ship) {
      proto.ShipClass.smallCargo => 'SC',
      proto.ShipClass.largeCargo => 'LC',
      proto.ShipClass.lightFighter => 'LF',
      proto.ShipClass.heavyFighter => 'HF',
      proto.ShipClass.cruiser => 'CR',
      proto.ShipClass.battleship => 'BS',
      proto.ShipClass.recycler => 'REC',
      proto.ShipClass.espionageProbe => 'PRB',
      proto.ShipClass.bomber => 'BB',
      proto.ShipClass.solarSatellite => 'SAT',
      proto.ShipClass.destroyer => 'DST',
      proto.ShipClass.deathstar => 'DS',
      proto.ShipClass.battlecruiser => 'BC',
      null => switch (u.defence!) {
          proto.DefenceKind.rocketLauncher => 'RL',
          proto.DefenceKind.lightLaser => 'LL',
          proto.DefenceKind.heavyLaser => 'HL',
          proto.DefenceKind.gaussCannon => 'GC',
          proto.DefenceKind.ionCannon => 'IC',
          proto.DefenceKind.plasmaTurret => 'PT',
          proto.DefenceKind.smallShieldDome => 'SD',
          proto.DefenceKind.largeShieldDome => 'LD',
        },
    };

/// Half the height a kind of unit's glyph is drawn at, in pixels at full
/// size: a drawing size, larger for the heavier hulls.
double glyphPx(proto.ShipyardItem u) => switch (u.ship) {
      proto.ShipClass.espionageProbe => 8,
      proto.ShipClass.solarSatellite => 8.5,
      proto.ShipClass.lightFighter => 9.5,
      proto.ShipClass.smallCargo => 10.5,
      proto.ShipClass.heavyFighter => 11,
      proto.ShipClass.largeCargo => 12.5,
      proto.ShipClass.cruiser => 13,
      proto.ShipClass.recycler => 13,
      proto.ShipClass.battlecruiser => 14.5,
      proto.ShipClass.battleship => 15.5,
      proto.ShipClass.bomber => 15.5,
      proto.ShipClass.destroyer => 16.5,
      proto.ShipClass.deathstar => 19,
      null => switch (u.defence!) {
          proto.DefenceKind.rocketLauncher => 9,
          proto.DefenceKind.lightLaser => 9,
          proto.DefenceKind.heavyLaser => 10.5,
          proto.DefenceKind.ionCannon => 10.5,
          proto.DefenceKind.gaussCannon => 12.5,
          proto.DefenceKind.smallShieldDome => 12.5,
          proto.DefenceKind.plasmaTurret => 14,
          proto.DefenceKind.largeShieldDome => 16,
        },
    };

/// How a mark stands for its count. Up to [literal] units are drawn a glyph
/// each. More are one glyph leading a block of pips that grows with the
/// square root of the count, so that two, two hundred and two thousand are
/// told apart at a glance and five thousand is still a block and not a
/// screen. A drawing rule: the count is printed under it.
///
/// The shape is laid out as the player's own marks stand, the lead glyph
/// toward the enemy at the top; a hostile mark is the same turned over.
class MarkShape {
  /// The most units a mark draws a glyph each for.
  static const literal = 4;

  /// How many pips stand behind the lead glyph of [count] units.
  static int pipsFor(int count) => count <= literal ? 0 : (2 * math.sqrt(count)).round();

  /// Where each glyph stands about the cluster's centre, and half its height.
  final List<Offset> glyphs;
  final double glyph;

  /// The pips' places about the cluster's centre, x and y in turn, and the
  /// side of one.
  final Float32List pips;
  final double pip;

  /// The glyphs and the pips together.
  final Size cluster;

  /// The label under the cluster, its text size, and the whole mark.
  final String label;
  final double font;
  final Size size;

  /// The label has no room for the unit's short name.
  final bool dense;

  MarkShape._(this.glyphs, this.glyph, this.pips, this.pip, this.cluster, this.label, this.font, this.size, this.dense);

  /// The shape of [count] units of [unit] at [zoom], 1 for full size, on a
  /// scene whose pixels are [scale] of the ones the sizes are given in.
  /// [dense] is for a ground with more marks than it has room for: the
  /// block is a strip two pips deep and the label is the count alone.
  factory MarkShape(proto.ShipyardItem unit, int count, {double zoom = 1, double scale = 1, bool dense = false}) {
    final label = dense ? shortCount(count) : '${tagOf(unit)} ${shortCount(count)}';
    final font = (10.5 * (.6 + .4 * zoom) * scale.clamp(.9, 1.14)).clamp(8.0, 12.0);
    final labelWide = label.length * font * .62;
    final shown = count.clamp(1, literal);
    final k = zoom * scale;
    final s = math.max(4.5, glyphPx(unit) * k * (shown > 1 ? .82 : 1));
    final gap = math.max(2.0, 4 * k);
    final glyphs = <Offset>[];
    Size whole(Size cluster) => Size(math.max(cluster.width, labelWide) + 2, cluster.height + font + 9);
    if (count <= literal) {
      final front = shown >= 4 ? 2 : shown == 3 ? 1 : shown;
      final back = shown - front;
      final step = s * 2 + gap;
      final height = back == 0 ? s * 2 : s * 3.3;
      final cluster = Size(step * math.max(front, back) - gap, height);
      for (var i = 0; i < front; i++) {
        glyphs.add(Offset((i - (front - 1) / 2) * step, -height / 2 + s));
      }
      for (var i = 0; i < back; i++) {
        glyphs.add(Offset((i - (back - 1) / 2) * step, height / 2 - s));
      }
      return MarkShape._(glyphs, s, Float32List(0), 0, cluster, label, font, whole(cluster), dense);
    }
    final all = pipsFor(count);
    final n = dense ? math.max(1, (all / 6).ceil()) : all;
    final pitch = math.max(2.0, (dense ? 3.2 : 4.0) * (.5 + .5 * zoom) * scale);
    final cols = dense ? (n / 2).ceil() : math.max(2, math.sqrt(n * 1.9).ceil());
    final rows = (n / cols).ceil();
    final height = s * 2 + gap + (rows - 1) * pitch + pitch * .5;
    final cluster = Size(math.max(s * 2, (cols - 1) * pitch + pitch), height);
    glyphs.add(Offset(0, -height / 2 + s));
    final pips = Float32List(n * 2);
    final top = -height / 2 + s * 2 + gap;
    for (var i = 0; i < n; i++) {
      final row = i ~/ cols, col = i % cols;
      final inRow = row == rows - 1 ? n - row * cols : cols;
      pips[i * 2] = (col - (inRow - 1) / 2) * pitch;
      pips[i * 2 + 1] = top + row * pitch;
    }
    return MarkShape._(glyphs, s, pips, math.max(1.3, pitch * .5), cluster, label, font, whole(cluster), dense);
  }
}

/// One kind of unit of one formation on a scene.
class Mark {
  final String key;
  final String formation;
  final proto.ShipyardItem unit;
  final bool hostile;
  final bool battery;
  final bool mine;

  /// What is drawn, and what the state says now: the second takes the
  /// place of the first once the fire of the round that changed it has
  /// landed. Null once the state no longer lists it.
  Figures shown;
  Figures? next;
  bool fleeing = false;
  MarkShape shape;

  /// Where it stands about the scene's centre, in pixels, and where it is
  /// going. It is drawn once it has been given a place.
  double x = 0, y = 0, tx = 0, ty = 0;
  bool placed = false;

  /// A hit flashes the glyph, and a shield that took fire shows an arc
  /// toward where it came from.
  double flash = 0, screen = 0, screenFrom = 0;

  /// What the round being drawn took from it, printed when its fire lands.
  int lost = 0;
  final double phase;

  Mark(this.key, this.formation, this.unit, this.shown, {required this.hostile, required this.battery, required this.mine, required this.phase})
      : next = shown,
        shape = MarkShape(unit, shown.count);

  bool get fallen => shown.count == 0;

  Offset get at => Offset(x, y);

  /// The centre of its glyphs and pips, which stand on the enemy's side of
  /// its label.
  Offset get cluster => Offset(x, y + (hostile ? 1 : -1) * (shape.size.height - shape.cluster.height) / 2);

  /// Its lead glyph: where its fire leaves from and the enemy's lands.
  Offset get lead => cluster + Offset(shape.glyphs.first.dx, shape.glyphs.first.dy * (hostile ? -1 : 1));

  /// One of its pips, or its lead glyph when it has none.
  Offset pipAt(int i) {
    final pips = shape.pips;
    if (pips.isEmpty) return lead;
    final k = i % (pips.length ~/ 2);
    return cluster + Offset(pips[k * 2], pips[k * 2 + 1] * (hostile ? -1 : 1));
  }
}

/// A volley of a round as fire between two marks. The heaviest of a side
/// are beams with a weight and an order; the rest are drawn together as a
/// faint haze.
class Beam {
  final Mark from, to;
  final bool ours;
  final double t0, dur;

  /// Stroke width, from the volley's shots, and how bright, from its rank
  /// among the side's volleys.
  final double weight, alpha;
  final int pulses;

  /// The volley reached a hull. Without it the fire stops on the shield.
  final bool hull;

  /// The share of its shots that bounced.
  final double bounced;
  bool landed = false;
  Beam(this.from, this.to, this.ours, this.t0, this.dur,
      {required this.weight, required this.alpha, required this.pulses, required this.hull, required this.bounced});
}

class Spark {
  double x, y, vx, vy, life;
  final double span, length;
  final Color color;
  Spark(this.x, this.y, this.vx, this.vy, this.life, this.span, this.color, this.length);
}

class Frag {
  double x, y, vx, vy, rot, vr, life;
  final double length, span;
  final Color color;
  Frag(this.x, this.y, this.vx, this.vy, this.rot, this.vr, this.life, this.length, this.span, this.color);
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

/// The ground one side's marks stand on: how far from the scene's centre
/// line its first row begins and its last may reach, and half the width
/// it has at a distance from that line. All in pixels.
class Ground {
  final double near, far;
  final double Function(double) halfWidth;
  const Ground({required this.near, required this.far, required this.halfWidth});

  (double, double, double, double) get _measure => (near, far, halfWidth(near), halfWidth(far));
}

/// The marks of a scene and their motion: the two sides as the state lists
/// them, a mark a kind of unit, and the fire of a round between them.
/// Nothing here is counted: a mark's figures are the state's or the
/// round's, and what moves between two rounds is what the round said.
class BattleField {
  BattleField({int seed = 7}) : rng = math.Random(seed);

  final math.Random rng;
  final List<Mark> marks = [];
  final List<Beam> beams = [];

  /// The volleys of the round being drawn that are no beam, as the two
  /// marks of each: a faint line a piece.
  final List<(Mark, Mark, bool)> haze = [];
  final List<Spark> sparks = [];
  final List<Frag> frags = [];
  final List<Ring> rings = [];
  final List<Floater> floats = [];

  /// The most volleys of a side a round draws as beams.
  static const beamsASide = 8;

  /// Seconds from a round's report to its fire having landed: until then
  /// the marks stand as they stood before it.
  static const landing = .62;

  /// Seconds a loss stays printed over its mark: gone before the next
  /// round's is.
  static const floatSeconds = .95;

  /// Draw the marks of units a round wiped out, dimmed: for a round that
  /// is looked at after it was fought.
  bool showFallen = false;
  bool reduced = false;

  /// Where a formation is drawn away from its place, and the way it faces
  /// when that is not at its enemy: a fleet leaving, by its id.
  final Map<String, Offset> offsets = {};
  final Map<String, double> headings = {};

  /// Where hostile marks new to the scene come in from, about its centre.
  Offset? hostileEntry;

  /// The name printed by a formation, by its id: every formation of a side
  /// that has more than one, and a battery always.
  final Map<String, String> captions = {};

  /// A round's fire is in the air.
  bool firing = false;
  double hazeFrom = 0;
  double shake = 0;
  double now = 0;
  double _settleAt = 0;
  Sides _sides = Sides.none;
  Ground? _ours, _theirs;

  /// How large the marks are drawn, 1 at full size, and whether the ground
  /// is too full for whole blocks and labels: one size for both sides, so
  /// that a block on one side is told against a block on the other.
  double zoom = 1;
  bool dense = false;

  void Function(String cue)? onCue;

  Iterable<Mark> get ours => marks.where((m) => !m.hostile);
  Iterable<Mark> get theirs => marks.where((m) => m.hostile);

  Mark? mark(String formation, proto.ShipyardItem unit) => marks.where((m) => m.formation == formation && m.unit == unit).firstOrNull;

  /// How large the scene is: what a mark's sizes are multiplied by.
  double scale = 1;

  /// The ground each side stands on, and the scene's [scale]. Called
  /// whenever the scene is laid out.
  void ground({required Ground ours, required Ground theirs, double scale = 1}) {
    final same = _ours?._measure == ours._measure && _theirs?._measure == theirs._measure && this.scale == scale;
    _ours = ours;
    _theirs = theirs;
    this.scale = scale;
    if (!same) _arrange(snap: true);
  }

  /// Set the marks from the state. While a round's fire is in the air the
  /// marks stand as they stood; they take the new figures when it lands.
  /// [snap] puts every mark in its place at once, for a scene just opened.
  void setSides(Sides sides, {bool snap = false}) {
    _sides = sides;
    final listed = <String>{};
    for (final f in [...sides.ours, ...sides.theirs]) {
      for (final u in f.units) {
        final key = '${f.id}/${u.unit.toJson()}';
        listed.add(key);
        final known = marks.where((m) => m.key == key).firstOrNull;
        if (known != null) {
          known.next = u;
          known.fleeing = f.fleeing;
        } else if (u.count > 0 || showFallen) {
          marks.add(Mark(key, f.id, u.unit, u, hostile: f.hostile, battery: f.battery, mine: f.mine, phase: rng.nextDouble() * _tau)..fleeing = f.fleeing);
        }
      }
    }
    for (final m in marks) {
      if (!listed.contains(m.key)) m.next = null;
    }
    if (snap || !firing) _settle(snap: snap);
  }

  /// The marks take what the state says of them. One the state no longer
  /// lists is gone; one whose last unit the round just drawn destroyed
  /// breaks up, and what that round took from each is printed over it.
  void _settle({bool snap = false}) {
    final struck = firing;
    firing = false;
    for (final m in [...marks]) {
      final next = m.next;
      if (struck && m.lost > 0 && m.placed) {
        floats.add(Floater(m.lead.dx, m.lead.dy, '-${shortCount(m.lost)}', m.hostile ? const Color(0xFFFFCC66) : const Color(0xFFFF8A5C), now));
        _burn(m);
      }
      final wiped = struck && m.lost > 0 && next != null && next.count == 0;
      m.lost = 0;
      if (wiped && m.placed) _explode(m);
      if (next == null || (next.count == 0 && !showFallen)) {
        marks.remove(m);
      } else {
        m.shown = next;
      }
    }
    _arrange(snap: snap);
  }

  /// A round of the fight this scene shows: its volleys are fire between
  /// the marks of the kinds that exchanged them, the heaviest as beams in
  /// their order and the rest as haze, and what it destroyed is taken from
  /// the marks when that fire has landed.
  void play(proto.BattleRoundEvent round) {
    if (firing) _settle();
    beams.clear();
    haze.clear();
    _fire(round.fleets.volleys, ours: true, start: now + .02);
    _fire(round.hostiles.volleys, ours: false, start: now + .1);
    for (final (side, hostile) in [(round.fleets, false), (round.hostiles, true)]) {
      for (final g in side.groups) {
        final id = Formation.idOf(g, hostile: hostile, raid: round.raid);
        for (final u in g.units) {
          mark(id, u.unit)?.lost = u.lost;
        }
      }
    }
    firing = true;
    hazeFrom = now;
    _settleAt = now + (reduced ? .3 : landing);
    onCue?.call('fire');
  }

  /// The mark that fires or is fired on for a volley's [unit] on one side:
  /// the largest of that kind still standing and, for the one firing, of a
  /// formation that is not leaving.
  Mark? _markFor(proto.ShipyardItem unit, {required bool hostile, required bool firing}) {
    Mark? best;
    for (final m in marks) {
      if (m.hostile != hostile || m.unit != unit || m.fallen || (firing && m.fleeing)) continue;
      if (best == null || m.shown.count > best.shown.count) best = m;
    }
    return best;
  }

  void _fire(List<proto.Volley> volleys, {required bool ours, required double start}) {
    final ranked = volleysByWeight(volleys);
    final drawn = math.min(ranked.length, beamsASide);
    for (final (i, v) in ranked.indexed) {
      final from = _markFor(v.from, hostile: !ours, firing: true), to = _markFor(v.at, hostile: ours, firing: false);
      if (from == null || to == null) continue;
      if (i >= drawn) {
        haze.add((from, to, ours));
        continue;
      }
      final magnitude = math.log(math.max(1, v.shots)) / math.ln10;
      beams.add(Beam(from, to, ours, start + i * .035, .26,
          weight: (1 + magnitude * .75).clamp(1.0, 4.2),
          alpha: 1 - .5 * i / math.max(1, drawn),
          pulses: v.shots >= 100 ? 3 : v.shots >= 10 ? 2 : 1,
          hull: v.hullDamage > 0,
          bounced: v.shots > 0 ? v.bounced / v.shots : 0));
    }
  }

  void _burst(Offset at, Color color, int n, [double speed = 1]) {
    if (reduced) return;
    for (var i = 0; i < n && sparks.length < 400; i++) {
      final a = rng.nextDouble() * _tau, v = (20 + rng.nextDouble() * 90) * speed;
      sparks.add(Spark(at.dx, at.dy, math.cos(a) * v, math.sin(a) * v, .3 + rng.nextDouble() * .5, .8, color, 5 + rng.nextDouble() * 12));
    }
  }

  /// The pips a round's losses took from [m] go out as sparks.
  void _burn(Mark m) {
    final n = math.min(10, 2 + (math.log(m.lost) / math.ln10 * 3).round());
    for (var i = 0; i < n; i++) {
      _burst(m.pipAt(rng.nextInt(1 << 20)), const Color(0xFFFF5A3C), 2, .6);
    }
    m.flash = 1;
    if (!reduced) shake = math.max(shake, 3);
  }

  void _explode(Mark m) {
    final r = m.shape.glyph;
    _burst(m.lead, const Color(0xFFFFCC66), 22, 1.5);
    _burst(m.lead, const Color(0xFFFF5A3C), 14, 1);
    rings.add(Ring(m.lead.dx, m.lead.dy, now, r));
    if (!reduced) {
      for (final v in Sym.verts(Sym.unit(m.unit, hostile: m.hostile))) {
        if (frags.length >= 90) break;
        final a = math.atan2(v.dy, v.dx);
        final speed = 25 + rng.nextDouble() * 70;
        frags.add(Frag(m.lead.dx, m.lead.dy, math.cos(a) * speed, math.sin(a) * speed, rng.nextDouble() * _tau, -4 + rng.nextDouble() * 8,
            .7 + rng.nextDouble() * .6, r * (.25 + rng.nextDouble() * .5), 1.3, m.hostile ? const Color(0xFFFF5A3C) : const Color(0xFF5EE0CF)));
      }
      shake = 10;
    }
    onCue?.call('hit');
  }

  void _impact(Beam b) {
    final t = b.to;
    final toward = math.atan2(b.from.lead.dy - t.lead.dy, b.from.lead.dx - t.lead.dx);
    if (b.hull) {
      t.flash = 1;
      _burst(t.lead, const Color(0xFFFFCC66), (3 + b.weight * 2).round());
    }
    if (!b.hull || b.bounced > 0) {
      t.screen = 1;
      t.screenFrom = toward;
      final r = t.shape.glyph * 1.5;
      _burst(t.lead + Offset(math.cos(toward) * r, math.sin(toward) * r), const Color(0xFF7FD4FF), 2 + (b.bounced * 5).round(), .5);
    }
    onCue?.call('hit');
  }

  void _arrange({bool snap = false}) {
    final (ourGround, theirGround) = (_ours, _theirs);
    if (ourGround == null || theirGround == null) return;
    captions.clear();
    final ours = _groups(_sides.ours, hostile: false), theirs = _groups(_sides.theirs, hostile: true);
    for (final (zoom, dense) in _sizes) {
      final last = (zoom, dense) == _sizes.last;
      final ourPlaces = _rows(ours, ourGround, zoom, dense, force: last), theirPlaces = _rows(theirs, theirGround, zoom, dense, force: last);
      if (ourPlaces == null || theirPlaces == null) continue;
      this.zoom = zoom;
      this.dense = dense;
      for (final MapEntry(key: m, value: at) in {...ourPlaces, ...theirPlaces}.entries) {
        m.tx = at.dx;
        m.ty = m.hostile ? -at.dy : at.dy;
        if (snap || !m.placed) {
          final entry = m.hostile && !snap ? hostileEntry : null;
          m.x = entry?.dx ?? m.tx;
          m.y = entry?.dy ?? m.ty;
          m.placed = true;
        }
      }
      return;
    }
  }

  /// The sizes a scene is tried at, largest first: larger than full for a
  /// handful of marks on a wide ground, and [dense] once whole blocks and
  /// labels no longer fit.
  static const _sizes = [
    (1.5, false),
    (1.25, false),
    (1.0, false),
    (.86, false),
    (.74, false),
    (.62, false),
    (.52, false),
    (.52, true),
    (.42, true),
    (.34, true),
  ];

  /// One side's marks by formation, in the order the state lists them,
  /// with the names of the formations that are to be captioned.
  List<List<Mark>> _groups(List<Formation> formations, {required bool hostile}) {
    final groups = [
      for (final f in formations) [for (final m in marks) if (m.formation == f.id && m.hostile == hostile) m],
    ].where((ms) => ms.isNotEmpty).toList();
    if (groups.length > 1 || (groups.isNotEmpty && groups.first.first.battery)) {
      captions.addAll({
        for (final f in formations)
          if (groups.any((ms) => ms.first.formation == f.id)) f.id: f.name,
      });
    }
    return groups;
  }

  /// One side's marks stood in rows on its ground at one size, formation
  /// by formation from the centre line outward: where each mark stands,
  /// across and how far out. A formation that fits on a row is kept whole
  /// on one. Null when the rows do not fit the ground, unless [force]d.
  Map<Mark, Offset>? _rows(List<List<Mark>> groups, Ground g, double zoom, bool dense, {required bool force}) {
    final places = <Mark, Offset>{};
    if (groups.isEmpty) return places;
    for (final m in groups.expand((ms) => ms)) {
      m.shape = MarkShape(m.unit, m.shown.count, zoom: zoom, scale: scale, dense: dense);
    }
    final captioned = captions.containsKey(groups.first.first.formation);
    final gap = math.max(5.0, 12 * zoom * scale), between = gap * 2.4, caption = captioned ? 15.0 : 0.0, rowGap = gap * .6;
    var depth = g.near;
    var row = <(Mark, double)>[];
    var across = 0.0, tall = 0.0;
    var fits = true;
    double room(double height) => g.halfWidth(depth + math.max(tall, height)) * 2;
    void close() {
      var x = -across / 2;
      for (final (m, lead) in row) {
        x += lead;
        places[m] = Offset(x + m.shape.size.width / 2, depth + tall / 2);
        x += m.shape.size.width;
      }
      if (across > room(tall)) fits = false;
      depth += tall + caption + rowGap;
      row = [];
      across = 0;
      tall = 0;
    }

    for (final ms in groups) {
      final whole = ms.fold(0.0, (w, m) => w + m.shape.size.width + gap) - gap;
      final tallest = ms.fold(0.0, (h, m) => math.max(h, m.shape.size.height));
      if (row.isNotEmpty && across + between + whole > room(tallest)) close();
      for (final (i, m) in ms.indexed) {
        final w = m.shape.size.width;
        if (row.isNotEmpty && across + gap + w > room(m.shape.size.height)) close();
        final lead = row.isEmpty ? 0.0 : i == 0 ? between : gap;
        row.add((m, lead));
        across += lead + w;
        tall = math.max(tall, m.shape.size.height);
      }
    }
    if (row.isNotEmpty) close();
    return force || (fits && depth - rowGap <= g.far) ? places : null;
  }

  /// Take everything off the scene, for another sector or another fight.
  void clear() {
    marks.clear();
    beams.clear();
    haze.clear();
    sparks.clear();
    frags.clear();
    rings.clear();
    floats.clear();
    firing = false;
    shake = 0;
  }

  /// The mark under [p], a point about the scene's centre.
  Mark? markAt(Offset p) {
    for (final m in marks) {
      if (!m.placed) continue;
      final at = Offset(m.x, m.y) + (offsets[m.formation] ?? Offset.zero);
      if (Rect.fromCenter(center: at, width: m.shape.size.width + 6, height: m.shape.size.height + 6).contains(p)) return m;
    }
    return null;
  }

  /// The box about every mark of [formation], about the scene's centre.
  Rect? boundsOf(String formation) {
    Rect? box;
    for (final m in marks) {
      if (m.formation != formation || !m.placed) continue;
      final r = Rect.fromCenter(center: Offset(m.x, m.y), width: m.shape.size.width, height: m.shape.size.height);
      box = box?.expandToInclude(r) ?? r;
    }
    return box;
  }

  void tick(double dt, double t) {
    now = t;
    if (firing && now >= _settleAt) _settle();
    final ease = 1 - math.exp(-dt * 7);
    for (final m in marks) {
      m.flash *= math.exp(-dt * 6);
      m.screen *= math.exp(-dt * 4.5);
      m.x += (m.tx - m.x) * ease;
      m.y += (m.ty - m.y) * ease;
    }
    for (final b in beams) {
      if (!b.landed && now >= b.t0 + b.dur) {
        b.landed = true;
        _impact(b);
      }
    }
    beams.removeWhere((b) => now > b.t0 + b.dur + b.pulses * .09 + .1);
    if (haze.isNotEmpty && now - hazeFrom > .5) haze.clear();
    for (var i = sparks.length - 1; i >= 0; i--) {
      final p = sparks[i];
      p.life -= dt;
      if (p.life <= 0) {
        sparks.removeAt(i);
        continue;
      }
      p.x += p.vx * dt;
      p.y += p.vy * dt;
    }
    for (var i = frags.length - 1; i >= 0; i--) {
      final f = frags[i];
      f.life -= dt;
      if (f.life <= 0) {
        frags.removeAt(i);
        continue;
      }
      f.x += f.vx * dt;
      f.y += f.vy * dt;
      f.rot += f.vr * dt;
    }
    rings.removeWhere((r) => now - r.t0 > .7);
    floats.removeWhere((f) => now - f.t0 > floatSeconds);
    if (shake > 0) {
      shake *= math.exp(-dt * 9);
      if (shake < .4) shake = 0;
    }
  }
}
