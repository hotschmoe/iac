import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:iac_client/console/intel.dart' show trimmed;
import 'package:iac_client/protocol/protocol.dart';

import '../fixtures_util.dart';

/// The engine's numbers as the scripted world reads them, from the two
/// files Rust renders and the tests of Rust keep current: the catalogue of
/// an empire at each level (`fixtures/protocol/catalog_by_level.json`) and
/// the economy's numbers (`docs/design/economy/numbers.json`). Both hold a
/// world at the pace of the season preset, which is the scripted world's.
/// Nothing of the game's is typed in this file: a number it needs is read
/// here, or is a column or a constant of the generated wire types.
class _Book {
  _Book._(this.byLevel, Map<String, dynamic> numbers)
      : _game = numbers['game'] as Map<String, dynamic>,
        _tables = numbers['tables'] as Map<String, dynamic>;

  static final _Book it = () {
    final root = fixturesDir();
    final numbers = jsonDecode(File('${root.parent.path}/docs/design/economy/numbers.json').readAsStringSync()) as Map<String, dynamic>;
    final levels = jsonDecode(File('${root.path}/protocol/catalog_by_level.json').readAsStringSync()) as List<dynamic>;
    return _Book._([for (final c in levels) HomeworldCatalog.fromJson(c)], numbers);
  }();

  static const preset = 'season';

  /// The catalogue of an empire whose every building and research stands
  /// at the index's level, without a Nanite Factory.
  final List<HomeworldCatalog> byLevel;
  final Map<String, dynamic> _game;
  final Map<String, dynamic> _tables;

  double constant(String name) => ((_game['constants'] as Map<String, dynamic>)[name] as num).toDouble();

  List<double> constants(String name) => [for (final v in (_game['constants'] as Map<String, dynamic>)[name] as List<dynamic>) (v as num).toDouble()];

  late final double pace = ((_game['presets'] as Map<String, dynamic>)[preset] as num).toDouble();

  late final int satelliteEnergy = _game['satellite_energy'] as int;

  static Resources _amounts(dynamic triple) {
    final v = [for (final x in triple as List<dynamic>) (x as num).toDouble()];
    return Resources(metal: v[0], crystal: v[1], deuterium: v[2]);
  }

  Map<String, dynamic> _row(String table, bool Function(Map<String, dynamic>) where) =>
      (_tables[table] as List<dynamic>).cast<Map<String, dynamic>>().firstWhere(where);

  Map<String, dynamic> _ring(int ring) => _row('rings', (r) => r['ring'] == max(1, ring));

  /// What stands guard a ring out, and what it is rated.
  double npcPower(int ring) => (_ring(ring)['npc_power'] as num).toDouble();
  int ringRating(int ring) => _ring(ring)['threat_rating'] as int;
  int npcCount(int ring) => _ring(ring)['count'] as int;
  double oreRingMult(int ring) => (_ring(ring)['ring_mult'] as num).toDouble();
  double outpostRaidPower(int ring) => (_ring(ring)['outpost_raid_power'] as num).toDouble();
  double derelictValue(int ring) => (_ring(ring)['derelict_value'] as num).toDouble();
  int derelictTier(int ring) => _ring(ring)['derelict_tier'] as int;

  /// Ticks one jump takes the group a ring spawns.
  int npcJumpTicks(int ring) => _ring(ring)['npc_jump_ticks'] as int;

  /// The most jumps a group of [behavior] makes after a fleet that left it.
  int chaseHexes(NpcBehavior behavior) => constants('CHASE_HEXES')[behavior.index].round();

  /// The steps a wounded group mends in, and the ticks between two of them
  /// for a group whose lair is at [ring], at this pace.
  late final int healSteps = constant('NPC_HEAL_STEPS').round();
  int healStepTicks(int ring) => _row('group_healing', (r) => r['ring'] == max(1, ring) && r['pace'] == pace)['step_ticks'] as int;

  Map<String, dynamic> _ringRule(int ring) => (_game['rings'] as List<dynamic>).cast<Map<String, dynamic>>().firstWhere((r) => r['ring'] == max(1, ring));

  ShipClass npcClass(int ring) => ShipClass.fromJson(_ringRule(ring)['npc_class']);

  /// The chance that a sector of [ring] holds a group, and the share of
  /// those that are passive, both 0 to 1.
  double npcPresence(int ring) => (_ringRule(ring)['npc_presence_pct'] as num).toDouble() / 100;
  double npcPassiveShare(int ring) => (_ringRule(ring)['npc_passive_share'] as num).toDouble();

  Map<String, dynamic> _density(Density d) => (_game['densities'] as Map<String, dynamic>)[d.wire] as Map<String, dynamic>;

  /// Units a full tile of [d] holds before its ring's growth, and what a
  /// harvest of it is multiplied by; nothing for no ore.
  double reserveUnits(Density d) => d == Density.none ? 0 : (_density(d)['reserve_units'] as num).toDouble();
  double harvestMultiplier(Density d) => d == Density.none ? 0 : (_density(d)['harvest_multiplier'] as num).toDouble();

  Map<String, dynamic> _outpost(int level) => _row('outpost_levels', (r) => r['level'] == level && r['pace'] == pace);

  Resources outpostCost(int level) => _amounts(_outpost(level)['cost']);
  Resources outpostStoreCap(int level) => _amounts(_outpost(level)['store_cap']);
  int outpostTicks(int level) => _outpost(level)['ticks'] as int;
  int outpostRepairTicks(int level) => _outpost(level)['repair_ticks'] as int;
  double outpostRepairPrice(int level) => (_outpost(level)['repair_price'] as num).toDouble();
  double outpostPlatform(int level) => (_outpost(level)['platform'] as num).toDouble().abs();

  /// The defences an outpost's platform is at [level], by kind.
  List<UnitCount> outpostPlatformUnits(int level) => [
        for (final row in (_outpost(level)['platform_units'] as List<dynamic>).cast<List<dynamic>>())
          UnitCount(unit: ShipyardItem.fromJson(row[0]), count: row[1] as int),
      ];

  /// What the extractor of an outpost of [level] makes of a seam of [d] a
  /// tick, [ring] rings out. The table is an hour's yield at pace 1, and a
  /// rate goes with the pace.
  double outpostYield(Density d, int ring, int level) {
    if (d == Density.none || level == 0) return 0;
    final row = _row('outpost_yields', (r) => r['ring'] == ring.clamp(outpostMinRing, outpostMaxRing) && r['level'] == level && r['pace'] == 1);
    return ((row['per_hour'] as List<dynamic>)[d.index - 1] as num).toDouble() * pace / 3600;
  }

  /// Which class fires again at what, as `WorldInfo.rapid_fire` lists it.
  late final List<RapidFire> rapidFire = [
    for (final c in ShipClass.values)
      for (final e in (((_game['ships'] as Map<String, dynamic>)[c.wire] as Map<String, dynamic>)['rapid_fire'] as Map<String, dynamic>).entries)
        RapidFire(shipClass: c, against: ShipyardItem.fromJson(e.key), shots: e.value as int),
  ];

  /// The exchange's window at this pace, in ticks.
  late final int exchangeWindow = _row('exchange_spans', (r) => r['pace'] == pace)['window'] as int;

  /// The power of a raid on a homeworld the script calls for: the rows of
  /// the raid law the planning simulation reads, weakest first.
  late final List<double> raidPowers = [for (final r in (_tables['raid_power'] as List<dynamic>)) ((r as Map<String, dynamic>)['power'] as num).toDouble()];
}

/// The ships of one class in a scripted fleet or group, or the defences of
/// one kind in a battery, as the engine keeps them: a count, and the hull of
/// each unit that is not whole, lowest first.
class _Squad {
  final ShipyardItem unit;
  int count;
  final List<int> wounds;

  /// What one of its units fights with, for a squad taken into a raid's
  /// round: there a side is fleets of different refits and a battery.
  final _Arms? arms;
  _Squad(ShipClass cls, this.count, [List<int>? wounds])
      : unit = ShipyardItem.ship(cls),
        wounds = wounds ?? [],
        arms = null;
  _Squad.of(this.unit, this.count, [List<int>? wounds, this.arms]) : wounds = wounds ?? [];

  /// The class of a squad of ships, which is all a fleet or a group holds.
  ShipClass get cls => unit.ship!;

  _Squad copy() => _Squad.of(unit, count, [...wounds], arms);

  /// A copy that fights with [arms].
  _Squad armed(_Arms arms) => _Squad.of(unit, count, [...wounds], arms);
}

/// What one ship of a class fires, raises and holds in a round.
typedef _Arms = ({int attack, int shield, int hull});

/// What one side fired in a round: its volleys by pair of units, the attack
/// of every shot summed, and the hull it took.
typedef _Fire = ({List<Volley> volleys, int power, int hull});

/// A fight between one fleet and one group, a round a tick.
class _Combat {
  final int fleetId;
  final Hex sector;
  final NpcFleetInfo npc;

  /// What the group is called, as it stood when the fight began.
  final String label;
  int round = 0;

  /// The last round fought, for a client that arrives in the middle.
  BattleRoundEvent? lastRound;

  /// What the fleet and the group have lost so far, by class.
  final Map<ShipClass, int> fleetLost = {}, groupLost = {};

  /// The sector the fleet is jumping out to and the rounds it is still
  /// fired on before it is gone; null while it fights.
  Hex? fleeTo;
  int fleeRounds = 0;
  _Combat(this.fleetId, this.sector, this.npc, this.label);
}

/// A raid on its way: the tick it arrives, the power it was announced with
/// and what it flies, fixed when it is announced.
class _Raid {
  final int arrival;
  final double power;
  final List<_Squad> ships;
  const _Raid(this.arrival, this.power, this.ships);
}

/// A raid being fought at the homeworld or at the outpost: the raiders
/// against every fleet standing there and the battery, a round a tick.
class _Siege {
  final int id;
  final Hex sector;
  final bool outpost;

  /// The power it was announced with, and what stood in its way when it
  /// arrived, as a sum on the same scale.
  final double power;
  final double met;

  /// The raiders as they arrived, and as the fight has left them, armed.
  final List<_Squad> muster;
  List<_Squad> raiders;

  /// What defends that is no fleet, as the fight has left it: at home the
  /// Solar Satellites and the defences, at the outpost its platform.
  List<_Squad> battery;
  int round = 0;
  BattleRoundEvent? lastRound;

  /// What each fleet, the battery (0) and the raiders ([id]) have lost so
  /// far, by unit.
  final Map<int, Map<ShipyardItem, int>> lost = {};

  /// The fleets charging a jump out: where to, and the rounds each is
  /// still fired on before it is gone.
  final Map<int, (Hex, int)> leaving = {};
  _Siege(this.id, this.sector, this.power, this.muster, this.battery, {required this.met, required this.outpost})
      : raiders = [for (final s in muster) s.copy()];

  /// How the raiders are named, as the engine's `Siege::label` names them.
  String get label => 'raid of ${[for (final s in muster) '${s.count} ${s.cls.label}'].join(', ')}';

  String get batteryLabel => outpost ? 'outpost platform' : 'home defences';
}

/// A jump a hostile group is making: after the fleet [quarry], or back to
/// its lair when that is null.
class _Errand {
  final int? quarry;
  final Hex to;
  final int arrives;
  const _Errand(this.quarry, this.to, this.arrives);
}

/// What the scripted world keeps of a hostile group beside its ships: the
/// sector it lairs in and the one it stands in, what it musters when whole,
/// how long it has rested since its last step of mending, how many jumps it
/// has made after a fleet since it was last home, and the jump it is making.
class _Group {
  final Hex lair;
  final NpcBehavior behavior;
  final List<_Squad> muster;

  /// What it is called, from what it musters: the name does not change
  /// with its losses.
  final String label;
  Hex at;
  int rested = 0;
  int strayed = 0;
  _Errand? errand;
  _Group(this.lair, this.behavior, this.muster, this.label) : at = lair;
}

class _Explore {
  final int fleetId;
  final Hex sector;
  final int endTick;
  _Explore(this.fleetId, this.sector, this.endTick);
}

/// The player's outpost in the scripted world.
class _Outpost {
  final Hex at;
  int level = 0;
  Resources store = const Resources();
  OutpostWorkState? work;
  bool breached = false;
  bool repairWarned = false;

  /// The raid on its way, when a test has announced one.
  _Raid? raid;
  final Set<ResourceKind> fullSaid = {};
  _Outpost(this.at);
}

/// The scripted world the widget tests run on (`DemoController`): the Dart
/// VM cannot run the real engine, and the tests want the same events at the
/// same ticks every time. It is not shipped; offline play is the
/// single-player build, on the real engine. It produces the protocol's own
/// messages (full_state, tick_update, error, game events) and simulates
/// move, scan, harvest, recall, stop, queues, policies (as labels), NPC
/// fights, salvage, derelict boarding and a raid cycle, the homeworld's
/// energy, its three stockpiles with their caps, build slots, pending
/// queues, defences, Solar Satellites and raids.
///
/// It is a world at the season preset's pace, and its numbers are the
/// engine's ([_Book]). Every row of its catalogue is the engine's for that
/// item at its level, in an empire whose other levels all stand there too:
/// a building's or a research's row is taken at its own level, a ship's and
/// a defence's at the Shipyard's, and its requirements are judged against
/// the levels this empire has. A build or research time is that row's,
/// carried from the speed the row was worked out with to the speed this
/// empire's factory or lab gives. It has no Nanite Factory.
///
/// It has one outpost to found: the survey with the engine's refusals, the
/// store with load and unload, the levels, abandoning, and a raid a test
/// announces ([raidOutpost]), which breaches it when lost and is repaired
/// from the store. Its levels take seconds, not the hours of the engine's.
/// A harvest takes no ore out of the ground.
///
/// A raid is a fight, shaped as the engine's: it is announced with what it
/// flies and a forecast, stands at the dock on the tick it arrives and fires
/// from the next, a round a tick for at most the engine's count of rounds,
/// the raiders against every fleet standing there and a battery (at home
/// the Solar Satellites and the defences at the empire's research, at the
/// outpost its level's platform at the catalogue's figures). It ends
/// destroyed, with the engine's share of the raiders' cost as wreckage at
/// home; outlasted, when the rounds are over or a round took no hull; or
/// lost, when no defender is left, and then takes the engine's shares of the
/// stockpile above what storage protects, or of the outpost's store, and
/// breaches the outpost. Defences and Solar Satellites that fell at home
/// are rebuilt at once at the engine's rate, cut off to a whole number
/// where the engine rolls for it.
/// A fleet in it may be ordered out as from any fight, and nothing follows.
/// Where this world is simpler than the engine: the fight is the one
/// without dice described below; a raid flies one class, Light Fighters at
/// home and its ring's class at the outpost, at the class's base figures;
/// its forecast is that one fight played forward, so its three shares are
/// each all or nothing and it is never `at_risk`; a fleet docked at home
/// does not scoop the wreckage; and what a destroyed ship carried is lost.
///
/// A fleet is the engine's: a count a class and the hull of each damaged
/// ship, fighting at the Weapons, Shielding and Armour levels it last
/// docked at home with. A fight is a round a tick and its report is shaped
/// as the engine's, but it is played without dice ([_volley]): every ship
/// fires once, and as often again as its rapid fire against what it hits
/// says, at the other side's classes in proportion to their numbers; a
/// shot under a hundredth of the shield it meets bounces, each other one
/// meets a full shield, and a ship hit at or under the engine's share of
/// its hull explodes. Pirates fight on their class's base figures, without
/// their ring's multiplier. The docks mend as the engine's do.
///
/// A fleet in a fight may be given a `move`: it pays the jump at once, stops
/// firing, is fired on for the engine's count of rounds and lands a tick
/// after the last, and the fight ends `fled`. `stop` calls the jump off and
/// the engine's alert says what it cost. A group that is not destroyed
/// keeps its losses and its damage and mends in the engine's steps, at rest
/// in its lair. One that was beating the fleet follows it as far as its
/// behaviour's leash, never into home ground, the hub or the outpost's
/// sector, fires on a fleet it finds, and goes home when it finds none.
/// Every group of a ring jumps in the ticks the ring's own class takes.
/// A jump out of a fight is charged before it is made, as the engine's
/// jumps are; every other jump here is made at once and its time is a
/// cooldown after it. So a fleet that jumps on the moment it lands is gone
/// before its pursuer lands, whatever the two speeds, and one that stays,
/// or whose cooldown has not run out, is caught. The odds of a fight as it
/// stands are power against power, as every odds figure of this world is.
///
/// Layout (home H=(4,-2); fleet 101 starts at S0=(6,-4)):
///   N (6,-5) T3 fighter patrol     NE (7,-5) nebula, derelict tier 1
///   SE (7,-4) debris, salvage      SW (5,-3) toward home
///   NW (5,-4) T5 aggressive (uncharted until scanned)
///   (7,-3) passive fighter         (8,-6) T3 fighter patrol, hot derelict
///   (9,-7) T7 swarm (stale chart)
class DemoWorld {
  static const _playerId = 1;
  static const _home = Hex(4, -2);
  static const _s0 = Hex(6, -4);
  static const _hubRadius = 24;
  static final _book = _Book.it;
  static const _raidPeriod = 300;

  /// What the scripted raid on the homeworld is announced at: a row of the
  /// raid law well under what the empire's defences hold.
  static double get _raidEstimate => _book.raidPowers[2];

  /// Scripted only: an outpost's levels and repairs go this many times
  /// faster than the engine's, so a test sees one finished.
  static const _outpostHaste = 270;

  final void Function(List<ServerMessage>) _emit;
  final Random _rng = Random(7);
  Timer? _timer;

  int _tick = 4821;
  final List<GameEvent> _pending = [];

  /// Last tick each known sector was observed; presence means "charted".
  final Map<int, int> _seen = {};
  final Map<int, int> _liveUntil = {};
  final Map<int, SectorState> _baseCache = {};
  final Map<int, List<NpcFleetInfo>> _hostiles = {};
  final Map<int, (Resources, int)> _salvage = {};
  final Map<int, SiteBrief> _sites = {};
  int _queueId = 500;
  int _newQueueId() => _queueId++;
  final Set<int> _dirty = {};
  Set<int> _liveNow = {};

  final Map<int, _Combat> _combats = {};
  final Map<int, _Explore> _explores = {};
  final Map<int, HarvestResource> _harvest = {};
  final Map<int, List<Hex>> _recall = {};
  final Set<int> _provoked = {};

  /// The fleets given orders through `policy_update`: the tick each one's
  /// hold was last said (0 before the first).
  final Map<int, int> _holdSaid = {};
  int _nextNpcFleet = 9100;

  /// The raid on its way to the homeworld; null while one is fought.
  _Raid? _raid;

  /// The raids being fought, by sector key: at the homeworld and at the
  /// outpost.
  final Map<int, _Siege> _sieges = {};
  int _nextSiege = 7000;

  /// The ships of every fleet and every hostile group, by its id, and the
  /// levels each of the player's fleets fights with.
  final Map<int, List<_Squad>> _squads = {};
  final Map<int, Refit> _refits = {};

  /// Every hostile group, by its id.
  final Map<int, _Group> _groups = {};

  /// The fleets the home dock has already seen to since they came in.
  final Set<int> _tended = {};
  final Set<ResourceKind> _capAlerted = {};
  final Set<ResourceKind> _fullAlerted = {};
  _Outpost? _outpost;

  late Resources _resources = const Resources(metal: 73400, crystal: 69000, deuterium: 8800);
  late List<FleetState> _fleets;
  late HomeworldState _hw;
  int _satellites = _startingSatellites;

  DemoWorld(this._emit) {
    final buildings = [
      for (final t in BuildingType.values) BuildingState(buildingType: t, level: _buildingLevels[t] ?? 0),
    ];
    final research = [
      for (final t in ResearchType.values) ResearchState(tech: t, level: _researchLevels[t] ?? 0),
    ];
    final catalog = _catalogFor(buildings, research, const {}, 0);
    _hw = HomeworldState(
      location: _home,
      production: const Resources(),
      energy: const EnergyState(made: 0, needed: 0, fraction: 1, perSatellite: 0, fusionBurn: 0, fusionStarved: false),
      satellites: _satellites,
      storage: const StorageState(cap: Resources(), protected: Resources(), fullInS: ResourceEta(), capped: []),
      buildings: buildings,
      research: research,
      catalog: catalog,
      buildSlots: 1,
      queueWaitingMax: 3,
      buildQueue: [BuildQueueItem(id: 401, buildingType: BuildingType.crystalMine, targetLevel: 8, startTick: _tick - 420, endTick: _tick + 260)],
      buildPending: const [],
      shipyardQueue:
          ShipyardQueueItem(id: 402, item: const ShipyardItem.ship(ShipClass.lightFighter), count: 3, built: 1, startTick: _tick - 200, endTick: _tick + 170),
      shipyardPending: const [],
      researchActive: ResearchItem(id: 403, tech: ResearchType.weaponsTechnology, targetLevel: 4, startTick: _tick - 300, endTick: _tick + 410),
      researchPending: const [],
      dockedShips: const [],
      dockedShipCount: 0,
      defences: [
        for (final o in catalog.defences) DefenceState(kind: o.kind, count: _defenceCount[o.kind] ?? 0, power: o.power),
      ],
      homeDefencePower: 0,
      dockedPower: 0,
      satellitePower: 0,
      structurePower: 0,
      nextRaidEstimatePower: _raidEstimate,
      raid: const RaidForecast(
          power: 0, ships: [], odds: RaidOdds(label: RaidLabel.repelled, destroyed: 1, outlasted: 0, lost: 0, kept: 1, raidersKept: 0)),
    );

    final wing = [_Squad(ShipClass.smallCargo, 1), _Squad(ShipClass.lightFighter, 3), _Squad(ShipClass.espionageProbe, 1)];
    final freighters = [_Squad(ShipClass.smallCargo, 2)];
    final prospectors = [_Squad(ShipClass.lightFighter, 2)];
    _fleets = [
      _fleet(101, _s0, FleetStatus.idle, wing, tank: .8, cargo: const Resources(metal: 34, crystal: 12)),
      _fleet(102, _home, FleetStatus.docked, freighters),
      _fleet(103, const Hex(3, -1), FleetStatus.harvesting, prospectors,
          tank: .75,
          cargo: const Resources(metal: 19, crystal: 6),
          policy: const StandingOrders(preset: PolicyPreset.prospect, params: PolicyParams(maxRange: 6, engageRatioX10: 20, fuelBudgetPct: 35))),
    ];
    _harvest[103] = HarvestResource.auto;
    _tended.addAll([for (final f in _fleets) if (_atHomeDock(f)) f.id]);

    final pendingBuilds = <QueuedBuild>[];
    for (final t in const [BuildingType.metalMine, BuildingType.metalStorage]) {
      final (cost, ticks) = _buildingStep(t, (_buildingLevels[t] ?? 0) + 1);
      pendingBuilds.add(QueuedBuild(
          id: _newQueueId(), buildingType: t, targetLevel: (_buildingLevels[t] ?? 0) + 1, cost: cost, ticks: ticks, waitingFor: _missing(cost), waitingOn: WaitReason.slot));
    }
    _hw = _hwWith(buildPending: pendingBuilds);

    _seedWorld();
    _liveNow = _computeLive();
    for (final k in _liveNow) {
      _seen[k] = _tick;
    }
    _announceRaid((_tick ~/ _raidPeriod + 1) * _raidPeriod);
    _hw = _hwWith();
  }

  /// The empire as the script opens on it: mines that the Solar Plant just
  /// powers, a yard that builds the first four hulls, and research some way
  /// into the tree, with most of it still locked.
  static const _buildingLevels = {
    BuildingType.metalMine: 9,
    BuildingType.crystalMine: 7,
    BuildingType.deuteriumSynthesizer: 5,
    BuildingType.solarPlant: 10,
    BuildingType.roboticsFactory: 2,
    BuildingType.shipyard: 4,
    BuildingType.metalStorage: 2,
    BuildingType.crystalStorage: 1,
    BuildingType.researchLab: 4,
  };
  static const _researchLevels = {
    ResearchType.espionageTechnology: 2,
    ResearchType.computerTechnology: 1,
    ResearchType.weaponsTechnology: 3,
    ResearchType.armourTechnology: 2,
    ResearchType.energyTechnology: 3,
    ResearchType.combustionDrive: 4,
    ResearchType.impulseDrive: 2,
    ResearchType.laserTechnology: 3,
  };
  static const _defenceCount = {DefenceKind.rocketLauncher: 6};
  static const _startingSatellites = 2;

  ShipOption _shipOption(ShipClass c) => _hw.catalog.ships.firstWhere((o) => o.shipClass == c);

  /// The Weapons, Shielding and Armour levels the empire has now, which a
  /// fleet takes when it docks at home.
  Refit get _levels {
    int level(ResearchType t) => _hw.research.firstWhere((r) => r.tech == t).level;
    return Refit(
        weapons: level(ResearchType.weaponsTechnology), shielding: level(ResearchType.shieldingTechnology), armour: level(ResearchType.armourTechnology));
  }

  /// One ship of [c] as a fleet refitted at [r] fights: the engine's row of
  /// the class at each of the three levels.
  static _Arms _arms(ShipClass c, Refit r) {
    ShipOption at(int level) => _book.byLevel[level].ships.firstWhere((o) => o.shipClass == c);
    return (attack: at(r.weapons).weapon.round(), shield: at(r.shielding).shield.round(), hull: at(r.armour).hull.round());
  }

  /// One ship of [c] in a hostile group: the class's base figures.
  static _Arms _npcArms(ShipClass c) => (attack: c.weapon.round(), shield: c.shield.round(), hull: c.hull.round());

  /// A ship holding [hull] of [full] is at risk: it may explode when a shot
  /// next breaks its shield.
  static bool _atRisk(int hull, int full) => hull * 100 <= explodeBelowPct * full;

  /// [squads] as the wire lists them, a row a class in catalogue order.
  static List<ShipGroupState> _rows(List<_Squad> squads, _Arms Function(ShipClass) arms) => [
        for (final s in [...squads]..sort((a, b) => a.cls.index.compareTo(b.cls.index)))
          if (s.count > 0)
            () {
              final unit = arms(s.cls);
              return ShipGroupState(
                shipClass: s.cls,
                count: s.count,
                damaged: s.wounds.length,
                atRisk: s.wounds.where((hull) => _atRisk(hull, unit.hull)).length,
                hull: (s.count - s.wounds.length) * unit.hull + s.wounds.fold(0, (n, hull) => n + hull),
                hullMax: s.count * unit.hull,
                unitAttack: unit.attack,
                unitShield: unit.shield,
                unitHull: unit.hull,
              );
            }(),
      ];

  static int _shipsIn(List<_Squad> squads) => squads.fold(0, (n, s) => n + s.count);

  /// [taken] ships moved out of [from]: whole ships first and then the
  /// least damaged, or with [damagedFirst] the most damaged first.
  static _Squad _take(_Squad from, int taken, {bool damagedFirst = false}) {
    final whole = from.count - from.wounds.length;
    final wounded = damagedFirst ? min(taken, from.wounds.length) : max(0, taken - whole);
    final start = damagedFirst ? 0 : from.wounds.length - wounded;
    final out = _Squad(from.cls, taken, from.wounds.sublist(start, start + wounded));
    from.wounds.removeRange(start, start + wounded);
    from.count -= taken;
    return out;
  }

  /// [more] folded into [into], class by class.
  static void _absorb(List<_Squad> into, List<_Squad> more) {
    for (final s in more) {
      final same = into.where((e) => e.cls == s.cls).firstOrNull;
      if (same == null) {
        into.add(s.copy());
      } else {
        same.count += s.count;
        same.wounds
          ..addAll(s.wounds)
          ..sort();
      }
    }
  }

  /// A fleet of [ships] with [fuel] aboard, or the share [tank] of what its
  /// tanks hold, refitted at the empire's levels of now. What the ships
  /// make of it is worked out in [_norm].
  FleetState _fleet(int id, Hex at, FleetStatus state, List<_Squad> ships,
      {double? fuel, double tank = 1, Resources cargo = const Resources(), StandingOrders? policy, Refit? refit}) {
    _squads[id] = ships;
    _refits[id] = refit ?? _levels;
    return _norm(FleetState(
      id: id,
      location: at,
      state: state,
      ships: const [],
      shipCount: 0,
      hull: 0,
      hullMax: 0,
      attack: 0,
      shield: 0,
      refit: _refits[id]!,
      cargo: cargo,
      cargoCapacity: 0,
      fuel: fuel ?? _tank(ships) * tank,
      fuelMax: 0,
      jumpTicks: 0,
      scanRange: scanBaseRange,
      hopsHome: 0,
      policy: policy,
    ));
  }


  // ── World ─────────────────────────────────────────────────────

  static int _hash(int a, int b, [int c = 0]) {
    var x = ((a * 73856093) ^ (b * 19349663) ^ (c * 83492791) ^ 0x5bd1e9) & 0x7fffffff;
    x = ((x ^ (x >> 13)) * 1103515) & 0x7fffffff;
    x = (x ^ (x >> 16)) & 0x7fffffff;
    return x;
  }

  static double _h01(int a, int b, int c) => _hash(a, b, c) / 2147483648.0;

  static const _forcedOpen = [
    (Hex(6, -4), Hex(6, -5)),
    (Hex(6, -4), Hex(7, -5)),
    (Hex(6, -4), Hex(7, -4)),
    (Hex(6, -4), Hex(5, -3)),
    (Hex(6, -4), Hex(5, -4)),
    (Hex(7, -4), Hex(7, -3)),
    (Hex(7, -5), Hex(8, -6)),
    (Hex(8, -6), Hex(9, -7)),
    (Hex(5, -3), Hex(4, -2)),
    (Hex(5, -3), Hex(4, -2)),
    (Hex(4, -2), Hex(3, -1)),
  ];
  static const _forcedClosed = [(Hex(6, -4), Hex(6, -3))];

  static bool _same(Hex a, Hex b, (Hex, Hex) e) => (e.$1 == a && e.$2 == b) || (e.$1 == b && e.$2 == a);

  /// Symmetric pseudo-random lanes, denser near the hub.
  static bool _edge(Hex a, Hex b) {
    for (final e in _forcedClosed) {
      if (_same(a, b, e)) return false;
    }
    for (final e in _forcedOpen) {
      if (_same(a, b, e)) return true;
    }
    final lo = a.toKey() < b.toKey() ? a : b;
    final hi = identical(lo, a) ? b : a;
    final m = max(a.distFromOrigin, b.distFromOrigin);
    return _h01(lo.toKey(), hi.toKey(), 21) < (m <= 1 ? 1.0 : m <= 8 ? .78 : .62);
  }

  static List<Hex> _lanes(Hex h) => [for (final n in h.neighbors()) if (_edge(h, n)) n];

  /// The ring's group power and its rating, as the server's `npc_power` and
  /// `threat_rating` give them.
  static ThreatInfo _ringThreat(Hex h) =>
      ThreatInfo(rating: _book.ringRating(h.distFromOrigin), estPower: _book.npcPower(h.distFromOrigin), basis: ThreatBasis.estimate);

  /// The server's `threat_rating`: one step for each doubling of [power]
  /// over the power of a group at ring 1.
  static int _rating(double power) {
    final base = _book.constant('NPC_POWER_BASE');
    return power <= base ? 1 : (1 + (log(power / base) / ln2).floor()).clamp(1, maxThreat);
  }

  /// What a force's rows add up to on the scale raids and threat ratings
  /// share: each ship counts its attack and a tenth of its shield and of
  /// the hull it holds.
  static double _power(List<ShipGroupState> rows) =>
      rows.fold(0.0, (n, g) => n + g.count * g.unitAttack + (g.count * g.unitShield + g.hull) / 10);

  static double _groupPower(NpcFleetInfo g) => _power(g.ships);

  /// Hostile groups rated by what their ships add up to. The engine rates
  /// an Aggressive or a Swarm group higher than that; the script does not.
  static ThreatInfo _observedThreat(List<NpcFleetInfo> groups) {
    final power = groups.fold(0.0, (n, g) => n + _groupPower(g));
    return ThreatInfo(rating: _rating(power), estPower: power, basis: ThreatBasis.observed);
  }

  /// What a full tile of [den] holds `d` rings out.
  static TileReserve _tile(Density den, Hex h) {
    final max = _book.reserveUnits(den) * _book.oreRingMult(h.distFromOrigin);
    return TileReserve(units: max, maxUnits: max);
  }

  /// Authored opening neighbourhood (terrain, ore m/c/d, hostiles, salvage, site).
  static final Map<int, ({TerrainType t, List<int> ore, List<(ShipClass, int)>? npc, NpcBehavior? b, int tier, SiteRisk? risk, Resources? salv})> _authored = {
    const Hex(6, -4).toKey(): (t: TerrainType.asteroidField, ore: [3, 2, 0], npc: null, b: null, tier: 0, risk: null, salv: null),
    const Hex(6, -5).toKey(): (t: TerrainType.empty, ore: [1, 0, 1], npc: [(ShipClass.lightFighter, 2)], b: NpcBehavior.patrol, tier: 0, risk: null, salv: null),
    const Hex(7, -5).toKey(): (t: TerrainType.nebula, ore: [0, 1, 3], npc: null, b: null, tier: 1, risk: SiteRisk.uneasy, salv: null),
    const Hex(7, -4).toKey(): (t: TerrainType.debrisField, ore: [2, 0, 0], npc: null, b: null, tier: 0, risk: null, salv: Resources(metal: 400, crystal: 100, deuterium: 40)),
    const Hex(5, -3).toKey(): (t: TerrainType.empty, ore: [1, 1, 0], npc: null, b: null, tier: 0, risk: null, salv: null),
    const Hex(5, -4).toKey(): (t: TerrainType.anomaly, ore: [0, 3, 1], npc: [(ShipClass.heavyFighter, 2), (ShipClass.lightFighter, 3)], b: NpcBehavior.aggressive, tier: 0, risk: null, salv: null),
    const Hex(3, -1).toKey(): (t: TerrainType.asteroidField, ore: [2, 1, 0], npc: null, b: null, tier: 0, risk: null, salv: null),
    const Hex(7, -3).toKey(): (t: TerrainType.empty, ore: [1, 1, 0], npc: [(ShipClass.lightFighter, 1)], b: NpcBehavior.passive, tier: 0, risk: null, salv: null),
    const Hex(8, -6).toKey(): (t: TerrainType.asteroidField, ore: [4, 2, 1], npc: [(ShipClass.lightFighter, 2)], b: NpcBehavior.patrol, tier: 1, risk: SiteRisk.hot, salv: null),
    const Hex(9, -7).toKey(): (t: TerrainType.empty, ore: [1, 1, 1], npc: [(ShipClass.cruiser, 3), (ShipClass.heavyFighter, 4), (ShipClass.lightFighter, 6)], b: NpcBehavior.swarm, tier: 0, risk: null, salv: null),
  };

  /// Terrain, ore and lanes only; hostiles, salvage and sites are mutable state.
  SectorState _base(Hex h) => _baseCache.putIfAbsent(h.toKey(), () {
        final au = _authored[h.toKey()];
        final d = h.distFromOrigin;
        final x = _h01(h.q, h.r, 1);
        final terrain = au?.t ??
            (x < .42 ? TerrainType.empty : x < .64 ? TerrainType.asteroidField : x < .78 ? TerrainType.nebula : x < .9 ? TerrainType.debrisField : TerrainType.anomaly);
        final rich = 1.2 + d * .11;
        int o(int s, double bonus) => max(0, min(4, (_h01(h.q, h.r, s) * rich + bonus).floor()));
        final ore = au?.ore ??
            [
              o(2, terrain == TerrainType.asteroidField ? 1 : 0),
              o(3, terrain == TerrainType.asteroidField ? .6 : 0),
              o(4, terrain == TerrainType.nebula ? 1 : 0),
            ];
        return SectorState(
          location: h,
          terrain: terrain,
          resources: SectorResources(metal: Density.values[ore[0]], crystal: Density.values[ore[1]], deuterium: Density.values[ore[2]]),
          connections: _lanes(h),
          oreReserve: terrain == TerrainType.empty
              ? null
              : OreReserve(
                  metal: _tile(Density.values[ore[0]], h),
                  crystal: _tile(Density.values[ore[1]], h),
                  deuterium: _tile(Density.values[ore[2]], h)),
          threat: _ringThreat(h),
          lastSeen: 0,
          live: false,
        );
      });

  void _seedWorld() {
    _authored.forEach((k, au) {
      if (au.npc != null) _hostiles[k] = [_npcGroup(au.npc!, au.b!, Hex.fromKey(k))];
      if (au.salv != null) _salvage[k] = (au.salv!, _tick + 2400);
      if (au.risk != null) _sites[k] = SiteBrief(tier: au.tier, risk: au.risk!);
    });
    // Procedural NPCs, salvage and sites in the rest of the charted area.
    for (final h in hexSpiral(_home, 12)) {
      final k = h.toKey();
      if (_authored.containsKey(k)) continue;
      final d = h.distFromOrigin;
      // Whether a ring's sector holds a group, what flies in it and how
      // often it is passive are the engine's; the rolls are the script's,
      // and a group it scatters that is not passive patrols. Only the
      // authored ones come at a fleet.
      if (Hex.distance(h, _home) > 2 && _h01(h.q, h.r, 5) < _book.npcPresence(d)) {
        final beh = _h01(h.q, h.r, 6) < _book.npcPassiveShare(d) ? NpcBehavior.passive : NpcBehavior.patrol;
        _hostiles[k] = [_npcGroup([(_book.npcClass(d), _book.npcCount(d))], beh, h)];
      }
      if (_h01(h.q, h.r, 8) < .08) {
        _salvage[k] = (
          Resources(
              metal: (200 + _h01(h.q, h.r, 9) * 600 + d * 40).roundToDouble(),
              crystal: (50 + _h01(h.q, h.r, 10) * 200).roundToDouble(),
              deuterium: (_h01(h.q, h.r, 11) * 120).roundToDouble()),
          _tick + 1500 + _hash(h.q, h.r, 3) % 3000
        );
      }
      if (d > 2 && _h01(h.q, h.r, 12) < .05 + d * .004) {
        _sites[k] = SiteBrief(tier: _book.derelictTier(d), risk: SiteRisk.values[(_h01(h.q, h.r, 13) * 3).floor()]);
      }
    }
    // Chart memory: ages from live to fossil.
    const trail = [Hex(5, -3), Hex(6, -4), Hex(7, -5), Hex(8, -6), Hex(9, -7), Hex(10, -8), Hex(3, -1), Hex(2, 1), Hex(1, 3), Hex(0, 5), Hex(8, -3), Hex(10, -4)];
    for (final h in hexSpiral(_home, 6)) {
      if (h == const Hex(5, -4)) continue; // stays uncharted until scanned
      final dh = Hex.distance(h, _home);
      double? age;
      if (dh <= 3) {
        age = 30 + _h01(h.q, h.r, 31) * 500;
      } else {
        for (final t in trail) {
          final dd = Hex.distance(h, t);
          if (dd <= 1) {
            age = 100 + _h01(h.q, h.r, 32) * 900 + t.distFromOrigin * 150;
            break;
          }
        }
        if (age == null && _h01(h.q, h.r, 33) < .55) age = 900 + _h01(h.q, h.r, 34) * 2500;
      }
      if (age != null) _seen[h.toKey()] = _tick - age.floor();
    }
    _seen[const Hex(8, -6).toKey()] = _tick - 2400;
    _seen[const Hex(9, -7).toKey()] = _tick - 5200;
    for (final h in [_s0, ..._s0.neighbors(), ..._fleets.map((f) => f.location), _home]) {
      if (h != const Hex(5, -4)) _seen.putIfAbsent(h.toKey(), () => _tick - 60);
    }
  }

  NpcFleetInfo _npcGroup(List<(ShipClass, int)> comp, NpcBehavior b, Hex lair) {
    final id = _nextNpcFleet++;
    _squads[id] = [for (final c in comp) _Squad(c.$1, c.$2)];
    _groups[id] = _Group(lair, b, [for (final c in comp) _Squad(c.$1, c.$2)], _groupLabel(comp, b));
    return _npcInfo(id);
  }

  /// Jumps group [g] may still make after a fleet before it turns back.
  int _leash(_Group g) => max(0, _book.chaseHexes(g.behavior) - g.strayed);

  int _groupJumpTicks(_Group g) => _book.npcJumpTicks(g.lair.distFromOrigin);

  /// The engine's `heal_steps`: steps of mending until group [id] is its
  /// muster again, 0 for one that is whole.
  int _healSteps(int id) {
    final g = _groups[id]!;
    var steps = 0;
    for (final theirs in g.muster) {
      final full = _npcArms(theirs.cls).hull;
      final here = _squads[id]!.where((s) => s.cls == theirs.cls).firstOrNull;
      final short = theirs.count - (here?.count ?? 0);
      final lowest = here == null || here.wounds.isEmpty ? full : here.wounds.first;
      final ships = (short / _healShips(theirs.count)).ceil();
      final hull = ((full - lowest) / _healHull(full)).ceil();
      steps = max(steps, max(ships, hull));
    }
    return steps;
  }

  int _healShips(int muster) => max(1, (muster / _book.healSteps).ceil());
  int _healHull(int full) => max(1, (full / _book.healSteps).ceil());

  /// The engine's `group_whole_at`: the tick group [id] is whole again if
  /// it mends undisturbed from now; null for one that is whole.
  int? _wholeAt(int id) {
    final steps = _healSteps(id);
    if (steps == 0) return null;
    final g = _groups[id]!;
    final resting = g.errand == null && g.at == g.lair && !_combats.values.any((c) => c.npc.id == id);
    return _tick + max(0, steps * _book.healStepTicks(g.lair.distFromOrigin) - (resting ? g.rested : 0));
  }

  /// A hostile group as a sector lists it, from the ships it has now.
  NpcFleetInfo _npcInfo(int id) {
    final g = _groups[id]!;
    final errand = g.errand;
    return NpcFleetInfo(
      id: id,
      ships: _rows(_squads[id]!, _npcArms),
      shipCount: _shipsIn(_squads[id]!),
      behavior: g.behavior,
      muster: _shipsIn(g.muster),
      wholeAt: _wholeAt(id),
      jumpTicks: _groupJumpTicks(g),
      chaseHexes: _book.chaseHexes(g.behavior),
      errand: errand == null && g.at == g.lair
          ? null
          : GroupErrand(
              home: g.lair,
              quarry: errand?.quarry,
              quarryOwner: errand?.quarry == null ? null : 'Demo',
              to: errand?.to,
              arrivesAt: errand?.arrives,
              hexesLeft: _leash(g)),
    );
  }

  /// List group [id] again where it stands, as it now is.
  void _relist(int id) {
    final k = _groups[id]!.at.toKey();
    _hostiles[k] = [for (final h in _hostiles[k] ?? const <NpcFleetInfo>[]) h.id == id ? _npcInfo(id) : h];
    _dirty.add(k);
  }

  Set<int> _computeLive() {
    final out = <int>{};
    for (final f in _fleets) {
      for (final h in hexSpiral(f.location, 1)) {
        if (_seen.containsKey(h.toKey()) || h == f.location) out.add(h.toKey());
      }
    }
    _liveUntil.removeWhere((_, until) => until <= _tick);
    out.addAll(_liveUntil.keys);
    return out;
  }

  SectorState _sector(Hex h, {bool? live}) {
    final b = _base(h);
    final k = h.toKey();
    final isLive = live ?? _liveNow.contains(k);
    final hs = _hostiles[k];
    final sv = _salvage[k];
    final quiet = _outpost?.at == h && (hs == null || hs.isEmpty);
    return SectorState(
      location: h,
      terrain: b.terrain,
      resources: b.resources,
      connections: b.connections,
      oreReserve: b.oreReserve,
      threat: quiet
          ? const ThreatInfo(rating: 0, estPower: 0, basis: ThreatBasis.observed)
          : hs == null || hs.isEmpty
              ? b.threat
              : _observedThreat(hs),
      hostiles: hs == null || hs.isEmpty ? null : hs,
      salvage: sv?.$1,
      salvageDespawnTick: sv?.$2,
      site: _sites[k],
      lastSeen: isLive ? _tick : (_seen[k] ?? 0),
      live: isLive,
      pinsStale: !isLive && (sv != null || _sites[k] != null),
      outpost: _outpost?.at == h ? OutpostBrief(owner: 'Demo', level: _outpost!.level, breached: _outpost!.breached, mine: true) : null,
    );
  }

  List<SectorState> _knownSectors() => [for (final k in _seen.keys) _sector(Hex.fromKey(k))];

  // ── Lifecycle ─────────────────────────────────────────────────

  GameState initial() => GameState(
        tick: _tick,
        player: _player(),
        fleets: [for (final f in _fleets) _norm(f)],
        homeworld: _hw,
        outpost: _outpostState(),
        knownSectors: _knownSectors(),
        battles: [
          for (final c in _combats.values) _battle(c, withRound: true),
          for (final siege in _sieges.values) _siegeState(siege, withRound: true),
        ],
        world: WorldInfo(
          pace: _book.pace,
          preset: _Book.preset,
          tickHz: 1,
          economyVersion: 2,
          worldgenVersion: 2,
          hub: const [HubSite(location: Hex.origin, services: [HubService.exchange])],
          rapidFire: _book.rapidFire,
        ),
      );

  PlayerState _player() => PlayerState(
      id: _playerId,
      name: 'Demo',
      resources: _resources,
      homeworld: _home,
      shipCount: _fleets.fold(0, (n, f) => n + _shipsIn(_squads[f.id]!)),
      exchange: _meter(_pressureNow));

  // ── Exchange (the server's `exchange_span`, `exchange_fee` and `exchange_return`) ──

  static int get _exchangeWindow => _book.exchangeWindow;
  static double get _feeMin => _book.constant('EXCHANGE_FEE_MIN');
  static double get _feeMax => _book.constant('EXCHANGE_FEE_MAX');
  double _pressure = 0;
  int _pressureTick = 0;

  /// What the meter holds: a share of what the homeworld makes in a window,
  /// each resource at its weight.
  double get _span {
    final p = _hw.production;
    final made = ResourceKind.values.fold(0.0, (n, k) => n + k.weight * max(0.0, k.of(p)));
    return _book.constant('EXCHANGE_METER_SHARE') * _exchangeWindow * made;
  }

  double get _pressureNow => max(0.0, _pressure - _span * (_tick - _pressureTick) / _exchangeWindow);

  ExchangeMeter _meter(double pressure) {
    final span = _span;
    return ExchangeMeter(
      pressure: pressure,
      span: span,
      fee: span > 0 ? _feeMin + (_feeMax - _feeMin) * min(1.0, pressure / span) : _feeMax,
      windowTicks: _exchangeWindow,
      clearIn: pressure <= 0 ? 0 : (span > 0 ? (pressure / span * _exchangeWindow).ceil() : null),
    );
  }

  /// What comes back for everything sold up to [pressure]: the fee rises
  /// along the meter and stays at its most past the end of it.
  double _kept(double pressure) {
    final span = _span;
    final ramp = min(pressure, span);
    final onRamp = span > 0 ? (1 - _feeMin) * ramp - (_feeMax - _feeMin) * ramp * ramp / (2 * span) : 0.0;
    return onRamp + (1 - _feeMax) * (pressure - ramp);
  }

  List<ServerMessage> _exchange(int fleetId, ResourceKind give, ResourceKind want, double? amount, {required bool quote}) {
    final f = _find(fleetId);
    if (f == null) return [_err(ErrorCode.fleetNotFound, 'fleet $fleetId not found (or it has no ships left)')];
    if (give == want) {
      return [_err(ErrorCode.invalidCommand, 'the exchange trades one resource for another; give and want are both ${give.name}')];
    }
    final held = give.of(f.cargo);
    final offered = amount ?? held;
    if (offered <= 0) return [_err(ErrorCode.noResources, 'fleet $fleetId holds no ${give.name} to trade')];
    final before = _pressureNow;
    final value = offered * give.weight;
    final receive = ((_kept(before + value) - _kept(before)) / want.weight).floorToDouble();
    final par = value / want.weight;
    final terms = ExchangeTerms(
      fleetId: fleetId,
      give: give,
      want: want,
      amount: offered,
      receive: receive,
      par: par,
      fee: 1 - receive / par,
      meter: _meter(before),
      meterAfter: _meter(before + value),
    );
    final hexes = Hex.distance(f.location, Hex.origin);
    ErrorMessage? refused;
    if (hexes > 0) {
      refused = ErrorMessage(
          code: ErrorCode.notInSector,
          message: 'fleet $fleetId is at ${f.location}, $hexes hex${hexes == 1 ? '' : 'es'} from the exchange at the hub ${Hex.origin}; '
              'a fleet trades what is in its hold, on the spot');
    } else if (offered > held) {
      refused = ErrorMessage(
          code: ErrorCode.noResources,
          message: 'fleet $fleetId holds ${held.toStringAsFixed(1)} ${give.name}; it cannot give ${offered.toStringAsFixed(1)}');
    } else if (receive < 1) {
      refused = ErrorMessage(code: ErrorCode.invalidCommand, message: '${offered.toStringAsFixed(1)} ${give.name} is too little to trade');
    } else if (receive > offered && f.cargo.total - offered + receive > f.cargoCapacity) {
      refused = ErrorMessage(
          code: ErrorCode.cargoFull,
          message: '${offered.round()} ${give.name} returns ${receive.round()} ${want.name}, and the hold of fleet $fleetId has no room for it');
    }
    if (quote) return [ExchangeQuote(terms: terms, refusal: refused?.message)];
    if (refused != null) return [refused];
    double after(ResourceKind k) => k.of(f.cargo) - (k == give ? offered : 0) + (k == want ? receive : 0);
    _setFleet(_copy(f,
        cargo: Resources(metal: after(ResourceKind.metal), crystal: after(ResourceKind.crystal), deuterium: after(ResourceKind.deuterium))));
    _pressure = before + value;
    _pressureTick = _tick;
    _ev(ExchangedEvent(sector: f.location, terms: terms));
    return [_tickUpdate()];
  }

  /// The server's `load`, with its refusal sentences.
  List<ServerMessage> _load(int fleetId, ResourceKind resource, double? amount) {
    final f = _find(fleetId);
    if (f == null) return [_err(ErrorCode.fleetNotFound, 'fleet $fleetId not found (or it has no ships left)')];
    final away = _awayFromDocks(f);
    if (away != null) return [away];
    final post = f.location == _home ? null : _outpost;
    if (f.policy != null) {
      return [
        _err(
            ErrorCode.invalidCommand,
            'fleet $fleetId is under standing orders (${f.policy!.preset.label}), which ${post == null ? 'unload at the dock' : 'do not use the outpost'}; '
            'set it to manual before loading its hold by hand'),
      ];
    }
    final source = post == null ? 'the stockpile' : 'the outpost\'s store';
    final stock = resource.of(post?.store ?? _resources);
    final aboard = f.cargo.total;
    final room = max(0.0, f.cargoCapacity - aboard);
    final full = _err(ErrorCode.cargoFull, 'the hold of fleet $fleetId is full: it carries ${aboard.round()} of ${f.cargoCapacity.round()}');
    if (amount != null && amount > stock) {
      return [
        _err(ErrorCode.noResources,
            '$source holds ${stock.toStringAsFixed(1)} ${resource.name}; fleet $fleetId cannot load ${amount.toStringAsFixed(1)}'),
      ];
    }
    if (amount == null && stock <= 0) return [_err(ErrorCode.noResources, '$source holds no ${resource.name} to load')];
    if (room <= 0) return [full];
    if (amount != null && amount > room) {
      return [
        _err(
            ErrorCode.cargoFull,
            'the hold of fleet $fleetId has room for ${room.toStringAsFixed(1)} more (it carries ${aboard.round()} of '
            '${f.cargoCapacity.round()}); it cannot load ${amount.toStringAsFixed(1)} ${resource.name}'),
      ];
    }
    final loaded = amount ?? min(stock, room);
    final taken = Resources(
      metal: resource == ResourceKind.metal ? loaded : 0,
      crystal: resource == ResourceKind.crystal ? loaded : 0,
      deuterium: resource == ResourceKind.deuterium ? loaded : 0,
    );
    if (post == null) {
      _resources = _less(_resources, taken);
    } else {
      post.store = _less(post.store, taken);
    }
    _setFleet(_copy(f, laden: post == null, cargo: _plus(f.cargo, taken)));
    final carries = 'it carries ${(aboard + loaded).round()} of ${f.cargoCapacity.round()}';
    _ev(AlertEvent(
        level: AlertLevel.info,
        message: post == null
            ? 'fleet $fleetId loaded ${loaded.round()} ${resource.name} from the stockpile: $carries and keeps it aboard until it next returns home, or until unload'
            : 'fleet $fleetId loaded ${loaded.round()} ${resource.name} from the outpost\'s store: $carries',
        fleetId: fleetId,
        sector: f.location));
    return [_tickUpdate()];
  }

  static Resources _plus(Resources a, Resources b) =>
      Resources(metal: a.metal + b.metal, crystal: a.crystal + b.crystal, deuterium: a.deuterium + b.deuterium);

  static Resources _less(Resources a, Resources b) =>
      Resources(metal: max(0, a.metal - b.metal), crystal: max(0, a.crystal - b.crystal), deuterium: max(0, a.deuterium - b.deuterium));

  static Resources _least(Resources a, Resources b) =>
      Resources(metal: min(a.metal, b.metal), crystal: min(a.crystal, b.crystal), deuterium: min(a.deuterium, b.deuterium));

  /// The server's refusal for a fleet that is at neither dock, or null when
  /// it stands at the homeworld or at the outpost.
  ServerMessage? _awayFromDocks(FleetState f) {
    final post = _outpost?.at;
    if (f.location == _home || f.location == post) return null;
    String hexesTo(Hex dock) {
      final hexes = Hex.distance(f.location, dock);
      return '$hexes hex${hexes == 1 ? '' : 'es'}';
    }

    return _err(
        ErrorCode.notInSector,
        post == null
            ? 'fleet ${f.id} is at ${f.location}, ${hexesTo(_home)} from home $_home; '
                'a hold is filled from the stockpile, and emptied into it, at the homeworld only'
            : 'fleet ${f.id} is at ${f.location}, ${hexesTo(_home)} from home $_home and ${hexesTo(post)} from your outpost $post; '
                'a hold is filled and emptied by hand at those two docks only');
  }

  /// Empty the laden hold of [f] into the stockpile as far as the caps
  /// allow and end the mark; returns what went back.
  Resources _unload(FleetState f) {
    final back = _least(f.cargo, _less(_hw.storage.cap, _resources));
    _resources = _plus(_resources, back);
    _setFleet(_copy(_find(f.id)!, laden: false, cargo: _less(f.cargo, back)));
    return back;
  }

  // ── Outpost (the server's numbers; levels and repairs at the script's own speed) ──

  static int get _outpostMaxLevel => _book.constant('OUTPOST_MAX_LEVEL').toInt();
  static Resources get _outpostCost => _book.outpostCost(1);

  static Resources _levelCost(int level) => _book.outpostCost(level);
  static int _levelTicks(int level) => max(1, (_book.outpostTicks(level) / _outpostHaste).round());
  static int _repairTicks(int level) => max(1, (_book.outpostRepairTicks(level) / _outpostHaste).round());

  /// What a repair of [level] costs, in metal.
  static double _repairPrice(int level) => _book.outpostRepairPrice(level);

  static String _weight(ResourceKind k) => trimmed(k.weight);

  static String _repairWords(int level) => '${_repairPrice(level).ceil()} metal, or crystal at ${_weight(ResourceKind.crystal)} and deuterium at '
      '${_weight(ResourceKind.deuterium)} a unit in its place';

  /// What a repair of [level] takes from [store], metal first, then crystal
  /// and deuterium at their weights, and the metal the store is short of
  /// paying it.
  static (Resources, double) _repairPayment(int level, Resources store) => _payByWeight(_repairPrice(level), store);

  /// The engine's `pay_by_weight`: what a price counted in metal takes from
  /// [stock], metal first, then crystal and deuterium at their weights, and
  /// the metal the stock is short of paying it. An outpost's own repair and
  /// the mending of ships at a dock are both paid this way.
  static (Resources, double) _payByWeight(double price, Resources stock) {
    var owed = price;
    double take(ResourceKind k) {
      final held = max(0.0, k.of(stock));
      if (held >= owed / k.weight) {
        final paid = owed / k.weight;
        owed = 0;
        return paid;
      }
      owed -= held * k.weight;
      return held;
    }

    final metal = take(ResourceKind.metal);
    final crystal = take(ResourceKind.crystal);
    final deuterium = take(ResourceKind.deuterium);
    return (Resources(metal: metal, crystal: crystal, deuterium: deuterium), owed);
  }

  static Resources _storeCap(int level) => _book.outpostStoreCap(level);
  static double _platform(int level) => _book.outpostPlatform(level);

  /// The least and the most a raid on an outpost at [at] can be: its ring's
  /// raid at the two ends of the roll.
  static PowerRange _outpostRaids(Hex at) {
    final base = _book.outpostRaidPower(at.distFromOrigin);
    return PowerRange(least: base * _book.constant('RAID_POWER_ROLL_MIN'), most: base * _book.constant('RAID_POWER_ROLL_MAX'));
  }

  Resources _siteYield(Hex at, int level) {
    final ore = _base(at).resources;
    double perTick(Density d) => _book.outpostYield(d, at.distFromOrigin, level);
    return Resources(metal: perTick(ore.metal), crystal: perTick(ore.crystal), deuterium: perTick(ore.deuterium));
  }

  /// A site's grade: what a harvest of each of its three seams is multiplied by, added up.
  double _siteGrade(Hex at) {
    final ore = _base(at).resources;
    return _book.harvestMultiplier(ore.metal) + _book.harvestMultiplier(ore.crystal) + _book.harvestMultiplier(ore.deuterium);
  }

  bool get _inService => _outpost != null && _outpost!.level >= 1 && !_outpost!.breached;

  OutpostState? _outpostState() {
    final o = _outpost;
    if (o == null) return null;
    final building = o.work?.kind == OutpostWork.build ? o.work!.level : null;
    final next = (building ?? o.level) + 1;
    final platform = o.breached ? 0.0 : _platform(o.level);
    final ships = _fleets.where((f) => f.location == o.at).fold(0.0, (n, f) => n + f.power);
    final cap = _storeCap(o.level);
    final makes = _inService ? _siteYield(o.at, o.level) : const Resources();
    final repair = o.breached && o.work == null ? _repairPayment(o.level, o.store) : null;
    return OutpostState(
      location: o.at,
      level: o.level,
      maxLevel: _outpostMaxLevel,
      inService: _inService,
      breached: o.breached,
      work: o.work,
      store: o.store,
      storeCap: cap,
      protected: _repairPayment(building ?? o.level, o.store).$1,
      production: makes,
      yieldLost: [for (final k in ResourceKind.values) if (k.of(makes) > 0 && k.of(o.store) >= k.of(cap)) k],
      grade: _siteGrade(o.at),
      platformPower: platform,
      platform: o.breached ? const [] : _book.outpostPlatformUnits(o.level),
      shipsPower: ships,
      defencePower: platform + ships,
      raidPower: _outpostRaids(o.at),
      raidArrivalTick: o.raid?.arrival,
      raid: o.level < 1 ? null : _raidAt(o.at, o.raid, () => _outpostRaid(o.at, 0)),
      nextLevel: next > _outpostMaxLevel
          ? null
          : OutpostLevel(
              level: next,
              cost: _levelCost(next),
              ticks: _levelTicks(next),
              storeCap: _storeCap(next),
              platformPower: _platform(next),
              platform: _book.outpostPlatformUnits(next),
              production: _siteYield(o.at, next)),
      upgradeRefusal: _upgradeRefusal()?.$2,
      repairCost: repair == null ? null : _plus(repair.$1, Resources(metal: repair.$2)),
      repairWaitingFor: repair != null && repair.$2 > 0 ? Resources(metal: repair.$2) : null,
      hopsHome: Hex.distance(o.at, _home),
    );
  }

  static const _outpostRings = 'an outpost stands in rings $outpostMinRing to $outpostMaxRing';

  /// Why [f] cannot found an outpost in [at] now, in the order and the
  /// sentences of the server's `outpost_site`; null when it can.
  (ErrorCode, String)? _siteRefusal(FleetState f, Hex at) {
    final id = f.id;
    final here = f.location == at;
    final ring = at.distFromOrigin;
    if (ring < outpostMinRing || ring > outpostMaxRing) {
      return (ErrorCode.invalidTarget, '${here ? 'fleet $id' : '$at'} is at ring $ring; $_outpostRings');
    }
    if (_base(at).terrain == TerrainType.anomaly) return (ErrorCode.invalidTarget, '$at is an Anomaly; no outpost stands in one');
    if (_sites.containsKey(at.toKey())) {
      return (ErrorCode.invalidTarget, 'a derelict drifts at $at; no outpost stands beside one, stripped or not');
    }
    if (_outpost case final mine?) {
      return (ErrorCode.invalidCommand, 'you already have an outpost at ${mine.at}; an empire has one, and abandon_outpost gives it up');
    }
    if (!here) {
      final hexes = Hex.distance(f.location, at);
      return (
        ErrorCode.notInSector,
        'fleet $id is at ${f.location}, $hexes hex${hexes == 1 ? '' : 'es'} from $at; an outpost is founded by a fleet standing in its sector',
      );
    }
    switch (f.state) {
      case FleetStatus.moving:
        return (ErrorCode.onCooldown, 'fleet $id is in transit; it can found an outpost once it stands idle');
      case FleetStatus.inCombat:
        return (ErrorCode.onCooldown, 'fleet $id is in combat; an outpost is founded on a sector its founder has won');
      case FleetStatus.harvesting || FleetStatus.returning || FleetStatus.exploring:
        return (ErrorCode.onCooldown, 'fleet $id is busy; it can found an outpost once it is idle');
      case FleetStatus.idle || FleetStatus.docked:
        break;
    }
    final group = _hostiles[at.toKey()]?.firstOrNull;
    if (group != null) {
      return (ErrorCode.invalidTarget, 'a ${group.behavior.wire.toLowerCase()} group holds $at; an outpost is founded on a sector its founder has won');
    }
    if (f.policy case final orders?) {
      return (ErrorCode.invalidCommand, 'fleet $id is under standing orders (${orders.preset.label}); set it to manual before founding an outpost');
    }
    final short = _less(_outpostCost, f.cargo);
    if (short.total > 0) {
      return (
        ErrorCode.noResources,
        'the hold of fleet $id lacks ${_amount(short)} of the ${_amount(_outpostCost)} an outpost costs; '
            'it is built from what the founding fleet carries, loaded at home',
      );
    }
    return null;
  }

  /// The server's `outpost_survey`: the site's figures, and why it cannot be
  /// founded now.
  List<ServerMessage> _survey(int fleetId, Hex? sector) {
    final f = _find(fleetId);
    if (f == null) return [_err(ErrorCode.fleetNotFound, 'fleet $fleetId not found (or it has no ships left)')];
    final at = sector ?? f.location;
    final seen = at == f.location || _seen.containsKey(at.toKey());
    final ring = at.distFromOrigin;
    return [
      OutpostSurvey(
        fleetId: fleetId,
        sector: at,
        ring: ring,
        site: seen
            ? SurveySite(densities: _base(at).resources, grade: _siteGrade(at), production: _siteYield(at, 1), hopsHome: Hex.distance(at, _home))
            : null,
        cost: _outpostCost,
        buildTicks: _levelTicks(1),
        raidPower: _outpostRaids(at),
        platformPower: _platform(1),
        platform: _book.outpostPlatformUnits(1),
        raid: ring < outpostMinRing || ring > outpostMaxRing
            ? null
            : () {
                final raid = _outpostRaid(at, 0);
                return RaidForecast(
                    power: raid.power,
                    ships: _shipCounts(raid.ships),
                    odds: _raidOdds([for (final s in _platformSquads(1)) s.armed(_baseArms(s))], [for (final s in raid.ships) s.armed(_baseArms(s))], raidRounds));
              }(),
        refusal: seen
            ? _siteRefusal(f, at)?.$2
            : ring < outpostMinRing || ring > outpostMaxRing
                ? '$at is at ring $ring; $_outpostRings'
                : '$at is not on your chart; a survey reads the site of a sector you have seen',
      ),
    ];
  }

  void _outpostAlert(AlertLevel level, Hex at, String message) => _ev(AlertEvent(level: level, message: message, sector: at));

  List<ServerMessage> _found(int fleetId) {
    final f = _find(fleetId);
    if (f == null) return [_err(ErrorCode.fleetNotFound, 'fleet $fleetId not found (or it has no ships left)')];
    final at = f.location;
    if (_siteRefusal(f, at) case (final code, final why)?) return [_err(code, why)];
    final ticks = _levelTicks(1);
    _setFleet(_copy(f, cargo: _less(f.cargo, _outpostCost)));
    _outpost = _Outpost(at)..work = OutpostWorkState(kind: OutpostWork.build, level: 1, startTick: _tick, endTick: _tick + ticks);
    _dirty.add(at.toKey());
    _ev(AlertEvent(
        level: AlertLevel.info,
        message: 'fleet $fleetId founded your outpost at $at for ${_amount(_outpostCost)}: level 1 is finished at tick ${_tick + ticks}, in $ticks ticks. '
            'Until then it has a store and nothing else',
        fleetId: fleetId,
        sector: at));
    return [_tickUpdate()];
  }

  /// Why `upgrade_outpost` would be refused now, in the order and the
  /// sentences of the server's `outpost_upgrade`; null when it would not.
  (ErrorCode, String)? _upgradeRefusal() {
    final o = _outpost;
    if (o == null) return (ErrorCode.invalidCommand, 'you have no outpost to upgrade');
    if (o.work case final job?) {
      final doing = job.kind == OutpostWork.build ? 'building level ${job.level}' : 'under repair';
      return (ErrorCode.invalidCommand, 'your outpost at ${o.at} is $doing until tick ${job.endTick}; it does one job at a time');
    }
    if (o.breached) {
      return (
        ErrorCode.invalidCommand,
        'your outpost at ${o.at} is breached; it repairs itself first, once its store covers the repair: ${_repairWords(o.level)}',
      );
    }
    if (o.level >= _outpostMaxLevel) {
      return (ErrorCode.maxLevelReached, 'your outpost at ${o.at} is at level $_outpostMaxLevel, the highest');
    }
    final next = o.level + 1;
    final cost = _levelCost(next);
    final short = _less(cost, o.store);
    if (short.total > 0) {
      return (
        ErrorCode.insufficientResources,
        'outpost level $next costs ${_amount(cost)}; the store at ${o.at} is short by ${_amount(short)}. '
            'A level is paid in full from the outpost\'s own store when it is ordered',
      );
    }
    return null;
  }

  List<ServerMessage> _upgradeOutpost() {
    if (_upgradeRefusal() case (final code, final why)?) return [_err(code, why)];
    final o = _outpost!;
    final next = o.level + 1;
    final cost = _levelCost(next);
    final ticks = _levelTicks(next);
    o.store = _less(o.store, cost);
    o.work = OutpostWorkState(kind: OutpostWork.build, level: next, startTick: _tick, endTick: _tick + ticks);
    _outpostAlert(
        AlertLevel.info,
        o.at,
        'your outpost at ${o.at} began level $next, paid ${_amount(cost)} from its store: finished at tick ${_tick + ticks}, in $ticks ticks. '
        'It works at level ${o.level} until then');
    return [_tickUpdate()];
  }

  List<ServerMessage> _abandonOutpost() {
    final o = _outpost;
    if (o == null) return [_err(ErrorCode.invalidCommand, 'you have no outpost to abandon')];
    _outpost = null;
    final k = o.at.toKey();
    if (o.store.total > 0) _salvage[k] = (_plus(_salvage[k]?.$1 ?? const Resources(), o.store), _tick + 180);
    _dirty.add(k);
    final left = o.store.total > 0 ? '${_amount(o.store)} from its store drifts there as wreckage for 180 ticks' : 'its store was empty';
    _outpostAlert(AlertLevel.info, o.at, 'your outpost at ${o.at} is abandoned: $left. Nothing is refunded, and you may found another');
    return [_tickUpdate()];
  }

  /// The server's `unload` at the outpost: the whole hold into the store, as
  /// far as the store has room.
  List<ServerMessage> _unloadIntoStore(FleetState f, _Outpost o) {
    if (f.cargo.total <= 0) {
      return [_err(ErrorCode.noResources, 'the hold of fleet ${f.id} is empty; there is nothing to unload into the outpost\'s store')];
    }
    final moved = _least(_less(_storeCap(o.level), o.store), f.cargo);
    final left = _less(f.cargo, moved);
    o.store = _plus(o.store, moved);
    _setFleet(_copy(f, cargo: left, laden: false));
    _ev(AlertEvent(
        level: AlertLevel.info,
        message: 'fleet ${f.id} unloaded ${_amount(moved)} into the outpost\'s store'
            '${left.total > 0 ? '; ${_amount(left)} stays aboard, the store is full' : ''}',
        fleetId: f.id,
        sector: f.location));
    return [_tickUpdate()];
  }

  /// Announce a raid of [power] on the outpost, arriving in [inTicks]: the
  /// scripted world sends none by itself, so a test decides when one comes
  /// and whether the outpost holds.
  void raidOutpost(double power, {int inTicks = 10}) {
    final o = _outpost!;
    final raid = _outpostRaid(o.at, _tick + inTicks, power: power);
    o.raid = raid;
    _ev(_raidIncoming(raid, o.at));
  }

  /// Announce a raid of [power] on the homeworld, arriving in [inTicks], in
  /// place of the one the script had on its way.
  void raidHome(double power, {int inTicks = 10}) => _announceRaid(_tick + inTicks, power);

  /// Scripted only: what stands at the homeworld that is no fleet becomes
  /// [defences] and [satellites], so a test can stage a raid's ending.
  void garrison(Map<DefenceKind, int> defences, {int satellites = 0}) {
    _satellites = satellites;
    _hw = _hwWith(defences: [for (final d in _hw.defences) DefenceState(kind: d.kind, count: defences[d.kind] ?? 0, power: d.power)]);
    _hw = _hwWith();
  }

  /// The outpost's tick, in the server's order: a finished job, the
  /// extractor, the repair of a breach, what is said about the store, then
  /// a raid that has arrived.
  void _stepOutpost() {
    final o = _outpost;
    if (o == null) return;
    final job = o.work;
    if (job != null && _tick >= job.endTick) {
      o.work = null;
      _dirty.add(o.at.toKey());
      if (job.kind == OutpostWork.build) {
        o.level = job.level;
        _ev(OutpostCompletedEvent(playerId: _playerId, sector: o.at, level: job.level));
      } else {
        o.breached = false;
        o.repairWarned = false;
        _outpostAlert(AlertLevel.info, o.at, 'your outpost at ${o.at} is repaired: its extractor, its tanks and its platform work again');
      }
    }
    final cap = _storeCap(o.level);
    if (_inService) o.store = _least(_plus(o.store, _siteYield(o.at, o.level)), cap);
    if (o.breached && o.work == null) {
      final (cost, short) = _repairPayment(o.level, o.store);
      if (short <= 0) {
        final ticks = _repairTicks(o.level);
        o.store = _less(o.store, cost);
        o.work = OutpostWorkState(kind: OutpostWork.repair, level: o.level, startTick: _tick, endTick: _tick + ticks);
        o.repairWarned = false;
        _outpostAlert(AlertLevel.info, o.at,
            'your outpost at ${o.at} began its repair, paid ${_amount(cost)} from its store: it is whole at tick ${_tick + ticks}, in $ticks ticks');
      } else if (!o.repairWarned) {
        o.repairWarned = true;
        _outpostAlert(
            AlertLevel.warning,
            o.at,
            'your outpost at ${o.at} is breached and cannot repair itself: the repair costs ${_repairWords(o.level)}, and its store '
            'is short by ${short.ceil()} metal. It makes nothing, fills no tank and has no platform until a fleet unloads the rest');
      }
    }
    final makes = _inService ? _siteYield(o.at, o.level) : const Resources();
    for (final k in ResourceKind.values) {
      if (k.of(o.store) < k.of(cap)) {
        o.fullSaid.remove(k);
      } else if (o.fullSaid.add(k) && k.of(makes) > 0) {
        _outpostAlert(
            AlertLevel.warning,
            o.at,
            'your outpost at ${o.at} holds all the ${k.name} its store takes (${k.of(cap).round()}): the extractor makes no more '
            'of it until a fleet loads some');
      }
    }
    final raid = o.raid;
    if (raid != null && _tick >= raid.arrival) {
      o.raid = null;
      _beginSiege(o.at, raid, met: _outpostState()!.defencePower);
    }
  }

  // ── Raids ─────────────────────────────────────────────────────

  /// The engine's word for a forecast's label, as `RaidIncoming.threat`
  /// carries it.
  static String _threatWord(RaidLabel label) => switch (label) {
        RaidLabel.repelled => 'light',
        RaidLabel.held => 'moderate',
        RaidLabel.atRisk || RaidLabel.lost => 'heavy',
      };

  static List<ShipCount> _shipCounts(List<_Squad> ships) => [for (final s in ships) ShipCount(shipClass: s.cls, count: s.count)];

  /// A raid of [power] flown as [cls]: as many as that power buys at the
  /// class's own, and one at the least.
  static List<_Squad> _raiders(ShipClass cls, double power) => [_Squad(cls, max(1, (power / cls.power).floor()))];

  /// A raid on an outpost at [at] arriving at [arrival], flown as its
  /// ring's class: of [power], or at the middle of the ring's roll.
  static _Raid _outpostRaid(Hex at, int arrival, {double? power}) {
    final ring = at.distFromOrigin;
    final middle = (_book.constant('RAID_POWER_ROLL_MIN') + _book.constant('RAID_POWER_ROLL_MAX')) / 2;
    final flown = power ?? _book.outpostRaidPower(ring) * middle;
    return _Raid(arrival, flown, _raiders(_book.npcClass(ring), flown));
  }

  /// A raid on the homeworld is on its way, to arrive at [arrival]: Light
  /// Fighters, of [power] or of what the script announces.
  void _announceRaid(int arrival, [double? power]) {
    final flown = power ?? _raidEstimate;
    final raid = _Raid(arrival, flown, _raiders(ShipClass.lightFighter, flown));
    _raid = raid;
    _ev(_raidIncoming(raid, null));
  }

  RaidIncomingEvent _raidIncoming(_Raid raid, Hex? outpost) {
    final odds = _forecast(outpost ?? _home, raid).odds;
    return RaidIncomingEvent(
        playerId: _playerId,
        arrivalTick: raid.arrival,
        threat: _threatWord(odds.label),
        estPower: raid.power,
        ships: _shipCounts(raid.ships),
        odds: odds,
        outpost: outpost);
  }

  /// One unit of a battery at the levels [r]: the engine's row of the unit
  /// at each of the three levels.
  static _Arms _unitArms(ShipyardItem unit, Refit r) {
    if (unit.ship case final cls?) return _arms(cls, r);
    DefenceOption at(int level) => _book.byLevel[level].defences.firstWhere((o) => o.kind == unit.defence);
    return (attack: at(r.weapons).weapon.round(), shield: at(r.shielding).shield.round(), hull: at(r.armour).hull.round());
  }

  /// One unit at the catalogue's figures, as an outpost's platform and
  /// every raider fights.
  static _Arms _baseArms(_Squad s) => switch (s.unit.defence) {
        null => _npcArms(s.cls),
        final kind => (attack: kind.weapon.round(), shield: kind.shield.round(), hull: kind.hull.round()),
      };

  static _Arms _armed(_Squad s) => s.arms!;

  static List<_Squad> _platformSquads(int level) => [for (final u in _book.outpostPlatformUnits(level)) _Squad.of(u.unit, u.count)];

  /// What defends [sector], the homeworld or the outpost, that is no fleet,
  /// whole and armed: at home the Solar Satellites and every defence at the
  /// empire's research, at the outpost its platform at the catalogue's
  /// figures, and none while it is breached.
  List<_Squad> _batteryAt(Hex sector) {
    if (sector == _home) {
      final levels = _levels;
      return [
        for (final s in [
          if (_satellites > 0) _Squad(ShipClass.solarSatellite, _satellites),
          for (final d in _hw.defences)
            if (d.count > 0) _Squad.of(ShipyardItem.defence(d.kind), d.count),
        ])
          s.armed(_unitArms(s.unit, levels)),
      ];
    }
    final o = _outpost!;
    return o.breached ? [] : [for (final s in _platformSquads(o.level)) s.armed(_baseArms(s))];
  }

  /// The fleets a raid at [sector] fights: every one of the player's that
  /// stands there, and of those in transit the ones leaving this fight.
  List<FleetState> _garrison(Hex sector, [_Siege? siege]) => [
        for (final f in _fleets)
          if (f.location == sector && (f.state != FleetStatus.moving || (siege?.leaving.containsKey(f.id) ?? false))) f,
      ];

  /// The ships of fleet [id] as a raid's round takes them: copies in
  /// catalogue order, armed with the fleet's refit.
  List<_Squad> _fleetArmed(int id) {
    final refit = _refits[id]!;
    return [for (final s in [..._squads[id]!]..sort((a, b) => a.cls.index.compareTo(b.cls.index))) s.armed(_arms(s.cls, refit))];
  }

  /// What [squads] are worth: each unit its cost at the score's weights,
  /// times the share of its hull it holds.
  double _worth(List<_Squad> squads) => squads.fold(0.0, (n, s) {
        final cost = _unitCostOf(s.unit);
        final each = ResourceKind.values.fold(0.0, (sum, k) => sum + k.of(cost) * k.weight);
        final held = s.wounds.fold(0.0, (sum, hull) => sum + hull / s.arms!.hull);
        return n + each * (s.count - s.wounds.length + held);
      });

  /// How a fight between [defenders] and [raiders], both armed, would end
  /// over [rounds] rounds: the fight itself played forward on copies. It is
  /// played once, without dice, so each of the three shares is all or
  /// nothing.
  RaidOdds _raidOdds(List<_Squad> defenders, List<_Squad> raiders, int rounds) {
    var ours = [for (final s in defenders) s.copy()], theirs = [for (final s in raiders) s.copy()];
    final (oursAtFirst, theirsAtFirst) = (_worth(ours), _worth(theirs));
    for (var round = 0; round < rounds && ours.isNotEmpty && theirs.isNotEmpty; round++) {
      final (firing, raiding) = ([for (final s in ours) s.copy()], [for (final s in theirs) s.copy()]);
      final hurt = _volleys(firing, _armed, theirs, _armed).hull + _volleys(raiding, _armed, ours, _armed).hull;
      ours = [for (final s in ours) if (s.count > 0) s];
      theirs = [for (final s in theirs) if (s.count > 0) s];
      if (hurt == 0) break;
    }
    final destroyed = theirs.isEmpty, lost = !destroyed && ours.isEmpty;
    return RaidOdds(
      label: lost
          ? RaidLabel.lost
          : destroyed
              ? RaidLabel.repelled
              : RaidLabel.held,
      destroyed: destroyed ? 1 : 0,
      outlasted: destroyed || lost ? 0 : 1,
      lost: lost ? 1 : 0,
      kept: oursAtFirst > 0 ? _worth(ours) / oursAtFirst : 0,
      raidersKept: theirsAtFirst > 0 ? _worth(theirs) / theirsAtFirst : 0,
    );
  }

  /// The forecast of [raid] on [sector], the homeworld or the outpost,
  /// against the fleets that would fire and the battery standing there now.
  RaidForecast _forecast(Hex sector, _Raid raid) => RaidForecast(
      arrivalTick: raid.arrival > 0 ? raid.arrival : null,
      power: raid.power,
      ships: _shipCounts(raid.ships),
      odds: _raidOdds(
          [for (final f in _garrison(sector)) ..._fleetArmed(f.id), ..._batteryAt(sector)], [for (final s in raid.ships) s.armed(_baseArms(s))], raidRounds));

  /// The odds of [siege] from where it stands, over the rounds left.
  RaidOdds _siegeOdds(_Siege siege) => _raidOdds([
        for (final f in _garrison(siege.sector, siege))
          if (!siege.leaving.containsKey(f.id)) ..._fleetArmed(f.id),
        ...siege.battery,
      ], siege.raiders, max(1, raidRounds - siege.round));

  /// The raid on [sector], the homeworld or the outpost, as the state
  /// forecasts it: the one being fought, else [onItsWay], else [typical],
  /// one of the usual size were it announced now.
  RaidForecast _raidAt(Hex sector, _Raid? onItsWay, _Raid Function() typical) {
    if (_sieges[sector.toKey()] case final siege?) {
      return RaidForecast(roundsLeft: raidRounds - siege.round, power: siege.power, ships: _shipCounts(siege.muster), odds: _siegeOdds(siege));
    }
    return _forecast(sector, onItsWay ?? typical());
  }

  RaidForecast _homeForecast() => _raidAt(_home, _raid, () => _Raid(0, _raidEstimate, _raiders(ShipClass.lightFighter, _raidEstimate)));

  /// [raid] has arrived at [sector]. With nothing in its way it is lost on
  /// the spot. Otherwise it stands there as a fight: every fleet of the
  /// player's at the dock is in it, with the battery, and the first round
  /// is fought on the next tick. [met] is what stood there, as a sum.
  void _beginSiege(Hex sector, _Raid raid, {required double met}) {
    final siege = _Siege(_nextSiege++, sector, raid.power, [for (final s in raid.ships) s.armed(_baseArms(s))], _batteryAt(sector),
        met: met, outpost: sector != _home);
    final fleets = _garrison(sector);
    if (fleets.isEmpty && siege.battery.isEmpty) {
      _settleRaid(siege, null);
      return;
    }
    _sieges[sector.toKey()] = siege;
    for (final f in fleets) {
      _stopActivities(f);
      _setFleet(_copy(f, state: FleetStatus.inCombat));
    }
    final ships = fleets.fold(0, (n, f) => n + f.shipCount);
    final satellites = siege.battery.where((s) => s.unit.ship != null).fold(0, (n, s) => n + s.count);
    final defences = siege.battery.where((s) => s.unit.defence != null).fold(0, (n, s) => n + s.count);
    _ev(AlertEvent(
        level: AlertLevel.warning,
        message: 'the ${siege.label} has arrived at ${siege.outpost ? 'your outpost' : 'your homeworld'} at $sector: it meets $ships ship(s), '
            '$satellites Solar Satellite(s) and $defences defence(s) and fights for at most $raidRounds rounds, the first next tick',
        sector: sector));
  }

  /// What a jump out of [siege] costs the fleet [id], which burns [jump] a
  /// jump, takes [jumpTicks] over one and holds [fuel]: the price of any
  /// flight, the odds of the fight as it stands, power against power, and
  /// nobody who follows.
  FlightQuote _raidFlight(_Siege siege, int id, {required double fuel, required double jump, required int jumpTicks}) {
    double power(List<_Squad> squads) => squads.fold(0.0, (n, s) => n + s.count * (s.unit.ship?.power ?? s.unit.defence!.power));
    final rounds = max(jumpTicks - 1, fleeMinRounds);
    final burn = jump * fleeFuelMultiplier;
    final ours = power([for (final f in _garrison(siege.sector, siege)) ..._squads[f.id]!, ...siege.battery]);
    final ratio = ours / max(1, power(siege.raiders));
    return FlightQuote(
      fuel: burn,
      ticks: rounds + 1,
      rounds: rounds,
      ratio: ratio,
      label: ratioLabelFor(ratio),
      refusal: fuel >= burn
          ? null
          : 'fleet $id has ${_tenths(fuel)} fuel and the jump out of the fight at ${siege.sector} burns ${_tenths(burn)}: it cannot leave, '
              'except by recall if it holds the fuel for that',
    );
  }

  /// A raid's fight as the state lists it, with the odds from where it
  /// stands.
  BattleState _siegeState(_Siege siege, {required bool withRound}) => BattleState(
      sector: siege.sector,
      rounds: siege.round,
      lastRound: withRound ? siege.lastRound : null,
      losses: _siegeLosses(siege),
      raid: RaidBattle(
          owner: 'Demo',
          mine: true,
          outpost: siege.outpost,
          power: siege.power,
          ships: _shipCounts(siege.muster),
          roundsLeft: raidRounds - siege.round,
          odds: _siegeOdds(siege)));

  /// What every fleet, the battery and the raiders of [siege] have lost so
  /// far, fleets first.
  List<BattleLosses> _siegeLosses(_Siege siege) {
    List<UnitCount> units(int group) => [
          for (final cls in ShipClass.values)
            if (siege.lost[group]?[ShipyardItem.ship(cls)] case final n?) UnitCount(unit: ShipyardItem.ship(cls), count: n),
          for (final kind in DefenceKind.values)
            if (siege.lost[group]?[ShipyardItem.defence(kind)] case final n?) UnitCount(unit: ShipyardItem.defence(kind), count: n),
        ];
    return [
      for (final id in siege.lost.keys)
        if (id != 0 && id != siege.id) BattleLosses(fleetId: id, owner: 'Demo', mine: true, lost: units(id), destroyed: _find(id) == null),
      BattleLosses(fleetId: 0, owner: 'Demo', label: siege.batteryLabel, mine: true, battery: true, lost: units(0), destroyed: siege.battery.isEmpty),
      BattleLosses(fleetId: siege.id, label: siege.label, lost: units(siege.id), destroyed: siege.raiders.isEmpty),
    ];
  }

  /// A raid on the homeworld that has arrived, and a round of every raid
  /// being fought: a fleet whose rounds under fire are over lands where it
  /// was bound; every fleet at the dock and the battery fire at the raiders
  /// and the raiders at them, all at once; what fell is counted out; and the
  /// fight ends when a side is gone, a round took no hull, or the rounds
  /// are over.
  void _stepRaids() {
    for (final siege in _sieges.values.toList()) {
      for (final MapEntry(key: id, value: (to, rounds)) in siege.leaving.entries.toList()) {
        final f = _find(id);
        if (f == null || rounds > 0) continue;
        siege.leaving.remove(id);
        _hop(f, to, paid: true);
      }
      final fleets = _garrison(siege.sector, siege);
      if (fleets.isEmpty && siege.battery.isEmpty) {
        _endSiege(siege, BattleOutcome.fled);
        continue;
      }
      for (final f in fleets) {
        if (f.state != FleetStatus.inCombat && f.state != FleetStatus.moving) _setFleet(_copy(f, state: FleetStatus.inCombat));
      }
      siege.round++;
      final before = {for (final f in fleets) f.id: _fleetArmed(f.id)};
      final after = {for (final MapEntry(key: id, value: squads) in before.entries) id: [for (final s in squads) s.copy()]};
      final battery = [for (final s in siege.battery) s.copy()];
      final raiders = [for (final s in siege.raiders) s.copy()];
      final ours = _volleys(
          [for (final f in fleets) if (!siege.leaving.containsKey(f.id)) ...before[f.id]!, ...siege.battery], _armed, raiders, _armed);
      final theirs = _volleys(siege.raiders, _armed, [for (final f in fleets) ...after[f.id]!, ...battery], _armed);
      for (final id in siege.leaving.keys.toList()) {
        final (to, rounds) = siege.leaving[id]!;
        siege.leaving[id] = (to, rounds - 1);
      }
      final round = BattleRoundEvent(
        sector: siege.sector,
        round: siege.round,
        fleets: _side([
          for (final f in fleets)
            BattleGroup(
                fleetId: f.id, owner: 'Demo', mine: true, fleeing: siege.leaving.containsKey(f.id), units: _unitsAfter(before[f.id]!, after[f.id]!)),
          BattleGroup(fleetId: 0, owner: 'Demo', label: siege.batteryLabel, mine: true, battery: true, units: _unitsAfter(siege.battery, battery)),
        ], [for (final squads in after.values) ...squads, ...battery], ours),
        hostiles: _side([BattleGroup(fleetId: siege.id, label: siege.label, units: _unitsAfter(siege.raiders, raiders))], raiders, theirs),
        mine: true,
        raid: true,
      );
      for (final g in [...round.fleets.groups, ...round.hostiles.groups]) {
        for (final u in g.units) {
          if (u.lost > 0) siege.lost.putIfAbsent(g.fleetId, () => {}).update(u.unit, (n) => n + u.lost, ifAbsent: () => u.lost);
        }
      }
      for (final f in fleets) {
        siege.lost.putIfAbsent(f.id, () => {});
        final left = _standing(after[f.id]!);
        if (left.isNotEmpty) {
          _squads[f.id] = left;
          _setFleet(_copy(f));
          continue;
        }
        siege.leaving.remove(f.id);
        _fleets = [for (final e in _fleets) if (e.id != f.id) e];
        _squads.remove(f.id);
        _refits.remove(f.id);
        _ev(FleetDestroyedEvent(fleetId: f.id, isNpc: false, sector: siege.sector, owner: 'Demo', mine: true, salvage: const Resources()));
      }
      siege
        ..lastRound = round
        ..battery = [for (final s in battery) if (s.count > 0) s]
        ..raiders = [for (final s in raiders) if (s.count > 0) s];
      if (!siege.outpost) _standAtHome(siege.battery);
      _ev(BattleRoundKind(round));
      final raidersGone = siege.raiders.isEmpty;
      final defendersGone = siege.battery.isEmpty && _garrison(siege.sector, siege).isEmpty;
      final quiet = round.fleets.hullDamage + round.hostiles.hullDamage == 0;
      if (raidersGone) {
        _endSiege(siege, defendersGone ? BattleOutcome.bothDestroyed : BattleOutcome.hostilesDestroyed);
      } else if (defendersGone) {
        _endSiege(siege, BattleOutcome.fleetsDestroyed);
      } else if (quiet || siege.round >= raidRounds) {
        _endSiege(siege, BattleOutcome.outlasted);
      }
    }
    final raid = _raid;
    if (raid != null && _tick >= raid.arrival) {
      _raid = null;
      _beginSiege(_home, raid, met: _defence(_hw.defences).total);
    }
  }

  /// What stands at home that is no fleet becomes what [battery] has left:
  /// the homeworld's counts fall as its units do.
  void _standAtHome(List<_Squad> battery) {
    int left(ShipyardItem unit) => battery.where((s) => s.unit == unit).fold(0, (n, s) => n + s.count);
    _satellites = left(const ShipyardItem.ship(ShipClass.solarSatellite));
    _hw = _hwWith(defences: [for (final d in _hw.defences) DefenceState(kind: d.kind, count: left(ShipyardItem.defence(d.kind)), power: d.power)]);
  }

  /// The fight of [siege] is over, [outcome] is how: its fleets stand at
  /// the dock again, the end is told as any fight's, and the raid is
  /// settled.
  void _endSiege(_Siege siege, BattleOutcome outcome) {
    _sieges.remove(siege.sector.toKey());
    for (final f in _garrison(siege.sector, siege)) {
      if (siege.leaving[f.id] case (final to, _)?) {
        _hop(f, to, paid: true);
      } else if (f.state == FleetStatus.inCombat) {
        _setFleet(_copy(f, state: siege.outpost ? FleetStatus.idle : FleetStatus.docked));
      }
    }
    _settleRaid(siege, outcome);
  }

  /// What follows a raid once its fight is over, or at once when nothing
  /// stood in its way ([outcome] null): Solar Satellites and defences
  /// rebuilt at the engine's rate, the wreckage of a raid destroyed at
  /// home, what a lost raid takes and the breach it leaves at an outpost,
  /// the two events that say so, and at home the next raid of the script.
  void _settleRaid(_Siege siege, BattleOutcome? outcome) {
    final home = !siege.outpost;
    final key = siege.sector.toKey();
    final raidersGone = siege.raiders.isEmpty;
    final defended = raidersGone || outcome == BattleOutcome.outlasted;
    final fell = siege.lost[0] ?? const <ShipyardItem, int>{};
    const satellite = ShipyardItem.ship(ShipClass.solarSatellite);
    final rebuilt = [
      if (home)
        for (final unit in [satellite, for (final kind in DefenceKind.values) ShipyardItem.defence(kind)])
          if ((fell[unit] ?? 0) * defenceRebuildPct ~/ 100 case final back when back > 0) UnitCount(unit: unit, count: back),
    ];
    if (rebuilt.isNotEmpty) {
      _satellites += rebuilt.where((u) => u.unit == satellite).fold(0, (n, u) => n + u.count);
      _hw = _hwWith(defences: [
        for (final d in _hw.defences)
          DefenceState(
              kind: d.kind, count: d.count + rebuilt.where((u) => u.unit.defence == d.kind).fold(0, (n, u) => n + u.count), power: d.power),
      ]);
    }
    Resources? pile;
    int? despawn;
    if (home && raidersGone) {
      final cost = siege.muster.fold(const Resources(), (sum, s) => _plus(sum, _times(_unitCostOf(s.unit), s.count)));
      pile = _times(cost, _book.constant('RAID_DEFENSE_SALVAGE_FRACTION'));
      despawn = _tick + 600;
      _salvage[key] = (_plus(pile, _salvage[key]?.$1 ?? const Resources()), despawn);
      _dirty.add(key);
    }
    var taken = const Resources(), kept = const Resources();
    if (!defended && home) {
      final above = _less(_resources, _hw.storage.protected);
      taken = Resources(
          metal: above.metal * _book.constant('RAID_LOSS_CAP_METAL'),
          crystal: above.crystal * _book.constant('RAID_LOSS_CAP_CRYSTAL'),
          deuterium: above.deuterium * _book.constant('RAID_LOSS_CAP_DEUT'));
      _resources = _less(_resources, taken);
      kept = _least(_resources, _hw.storage.protected);
    } else if (!defended) {
      final o = _outpost!;
      final floor = _outpostState()!.protected;
      taken = _times(_less(o.store, floor), _book.constant('OUTPOST_RAID_LOSS'));
      o
        ..store = _less(o.store, taken)
        ..breached = true
        ..repairWarned = false;
      kept = _least(o.store, floor);
      _dirty.add(key);
    }
    final losses = _siegeLosses(siege);
    List<UnitCount> lostBy(bool Function(BattleLosses) whose) => [
          for (final cls in ShipClass.values) ShipyardItem.ship(cls),
          for (final kind in DefenceKind.values) ShipyardItem.defence(kind),
        ].map((unit) => UnitCount(unit: unit, count: losses.where(whose).expand((l) => l.lost).where((u) => u.unit == unit).fold(0, (n, u) => n + u.count))).where((u) => u.count > 0).toList();
    final ours = lostBy((l) => l.mine);
    int lostOf(bool Function(UnitCount) which) => ours.where(which).fold(0, (n, u) => n + u.count);
    if (outcome != null) {
      _ev(CombatEndedEvent(
          sector: siege.sector,
          playerVictory: outcome == BattleOutcome.hostilesDestroyed,
          outcome: outcome,
          raid: true,
          rounds: siege.round,
          losses: losses,
          owners: const ['Demo'],
          mine: true,
          salvage: pile,
          salvageDespawnTick: despawn));
    }
    _ev(RaidResolvedEvent(
        playerId: _playerId,
        outpost: home ? null : siege.sector,
        defended: defended,
        outcome: outcome,
        rounds: siege.round,
        breached: !defended && !home,
        resourcesLost: taken,
        salvageDropped: pile,
        salvageDespawnTick: despawn,
        raidPower: siege.power,
        defensePower: siege.met,
        raiders: _shipCounts(siege.muster),
        raidersLost: [for (final u in lostBy((l) => !l.mine)) ShipCount(shipClass: u.unit.ship!, count: u.count)],
        lost: ours,
        rebuilt: rebuilt,
        satellitesLost: lostOf((u) => u.unit.ship == ShipClass.solarSatellite),
        structuresLost: lostOf((u) => u.unit.defence != null),
        structuresRestored: rebuilt.where((u) => u.unit.defence != null).fold(0, (n, u) => n + u.count),
        protectedKept: kept,
        shipsLost: lostOf((u) => u.unit.ship != null && u.unit.ship != ShipClass.solarSatellite)));
    if (home) _announceRaid(((_tick ~/ _raidPeriod) + 1) * _raidPeriod);
  }

  /// The server's `policy_update`, with the reports it makes: `clear` for
  /// manual, `engage` naming the settings the orders fly with, and the end
  /// of a hold loaded by hand.
  List<ServerMessage> _policy(int fleetId, PolicyPreset preset, PolicyParams asked) {
    final params = PolicyParams(
      minFuelPct: asked.minFuelPct,
      cargoReturnPct: asked.cargoReturnPct,
      maxRange: asked.maxRange,
      engageRatioX10: asked.engageRatioX10,
      fuelBudgetPct: asked.fuelBudgetPct,
      refine: asked.refine,
      disengageRatioX10: min(asked.disengageRatioX10, asked.engageRatioX10),
    );
    final f = _find(fleetId);
    if (f == null) {
      return [_err(ErrorCode.fleetNotFound, 'fleet $fleetId not found (or it has no ships left); its standing orders were not changed')];
    }
    if (preset == PolicyPreset.manual) {
      _holdSaid.remove(fleetId);
      _setFleet(_copy(f, clearPolicy: true));
      _ev(PolicyActionEvent(fleetId: fleetId, preset: PolicyPreset.manual, action: 'clear', reason: 'standing orders cleared'));
      return [_tickUpdate()];
    }
    _holdSaid[fleetId] = 0;
    _setFleet(_copy(f, policy: StandingOrders(preset: preset, params: params)));
    if (f.laden) _unload(f);
    final fuel = params.fuelBudgetPct >= 100 ? 'fuel unbounded' : 'fuel budget ${params.fuelBudgetPct}% of deuterium income';
    _ev(PolicyActionEvent(
        fleetId: fleetId,
        preset: preset,
        action: 'engage',
        reason: 'doctrine set: ${preset.label} (range ${params.maxRange}, engage ${(params.engageRatioX10 / 10).toStringAsFixed(1)}x, '
            'disengage ${(params.disengageRatioX10 / 10).toStringAsFixed(1)}x, $fuel'
            '${params.refine ? ', refining from the hold when short of fuel' : ''})'
            '${f.laden ? '; the hold loaded by hand goes back to the stockpile' : ''}'));
    return [_tickUpdate()];
  }

  /// The scripted world flies no doctrine. A fleet given orders here that
  /// stands free holds, and says why in the engine's sentence for a board
  /// with nothing on it: when the orders are first weighed and once a
  /// minute after.
  void _stepOrders() {
    for (final f in _fleets) {
      final saidAt = _holdSaid[f.id];
      final orders = f.policy;
      if (saidAt == null || orders == null) continue;
      final StandingOrders(:preset, :params) = orders;
      if (f.state != FleetStatus.idle && f.state != FleetStatus.docked) continue;
      if (saidAt != 0 && _tick < saidAt + 60) continue;
      _holdSaid[f.id] = _tick;
      final within = 'within ${params.maxRange} hops of home';
      _ev(PolicyActionEvent(
          fleetId: f.id,
          preset: preset,
          action: 'hold',
          reason: switch (preset) {
            PolicyPreset.prospect => 'every lane within 32 hops of ${f.location} is charted $within',
            PolicyPreset.mineAndReturn => 'no seam $within repays the fuel to fetch it (stripped tiles refill over hours)',
            PolicyPreset.salvageAndSites => 'board is clear $within: no derelict, no wreckage that repays the fuel',
            PolicyPreset.patrolHome => 'no contact $within that the fleet rates ${(params.engageRatioX10 / 10).toStringAsFixed(1)}x against',
            PolicyPreset.manual => '',
          }));
    }
  }

  /// Put [fleetId] at [at] with [cargo] aboard, for a test that needs a
  /// fleet somewhere the scripted layout does not reach.
  void place(int fleetId, Hex at, Resources cargo) {
    final f = _find(fleetId)!;
    _stopActivities(f);
    _setFleet(_copy(f, location: at, state: FleetStatus.idle, cargo: cargo, cooldown: 0));
    _reveal(at, 0);
  }

  void learn(ResearchType tech, int level) {
    _hw = _hwWith(research: [for (final e in _hw.research) e.tech == tech ? ResearchState(tech: tech, level: level) : e]);
  }

  /// Stand [building] at [level], for a test that needs an empire the
  /// script does not open on.
  void raise(BuildingType building, int level) {
    _hw = _hwWith(buildings: [for (final e in _hw.buildings) e.buildingType == building ? BuildingState(buildingType: building, level: level) : e]);
  }

  /// Put [stock] in the stockpile.
  void stock(Resources stock) {
    _resources = stock;
    _hw = _hwWith();
  }

  void start() {
    _timer?.cancel();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) => step());
  }

  void stop() {
    _timer?.cancel();
    _timer = null;
  }

  void _ev(EventKind k) => _pending.add(GameEvent(tick: _tick, kind: k));

  /// A stockpile ends at its cap, and one the Fusion Reactor drains at nothing.
  Resources _clampCaps(Resources r) {
    final cap = _hw.storage.cap;
    return Resources(metal: min(r.metal, cap.metal), crystal: min(r.crystal, cap.crystal), deuterium: max(0, min(r.deuterium, cap.deuterium)));
  }

  /// One server tick. Public so tests can drive the simulation.
  void step() {
    _tick++;
    final p = _hw.production;
    _resources = _clampCaps(Resources(
      metal: _resources.metal + p.metal,
      crystal: _resources.crystal + p.crystal,
      deuterium: _resources.deuterium + p.deuterium,
    ));
    _capAlerts();

    _fleets = [
      for (final f in _fleets) _copy(f, cooldown: max(0, f.cooldownRemaining - 1)),
    ];
    _stepHarvest();
    _stepRecall();
    _stepExplore();
    _stepGroups();
    _stepCombat();
    _stepQueues();
    _stepDocks();
    _stepSalvage();
    _stepRaids();
    _stepOutpost();
    _stepOrders();

    if (_tick % 120 == 0) {
      _ev(const AlertEvent(level: AlertLevel.info, message: 'Demo mode: no server connected'));
    }

    final before = _liveNow;
    _liveNow = _computeLive();
    for (final k in _liveNow) {
      _seen[k] = _tick;
    }
    _dirty
      ..addAll(_liveNow)
      ..addAll(before.difference(_liveNow));
    _emit([_tickUpdate()]);
  }

  void _capAlerts() {
    final cap = _hw.storage.cap;
    final eta = _hw.storage.fullInS;
    for (final e in [
      (ResourceKind.metal, _resources.metal, cap.metal, eta.metal),
      (ResourceKind.crystal, _resources.crystal, cap.crystal, eta.crystal),
      (ResourceKind.deuterium, _resources.deuterium, cap.deuterium, eta.deuterium),
    ]) {
      final ratio = e.$2 / e.$3;
      if (storageNearCap(e.$2, e.$3)) {
        if (_capAlerted.add(e.$1)) _ev(StorageNearCapEvent(playerId: _playerId, resource: e.$1, ratio: ratio, fullInS: e.$4));
      } else {
        _capAlerted.remove(e.$1);
      }
      if (e.$2 >= e.$3) {
        if (_fullAlerted.add(e.$1)) _ev(StorageFullEvent(playerId: _playerId, resource: e.$1));
      } else {
        _fullAlerted.remove(e.$1);
      }
    }
  }

  TickUpdate _tickUpdate() {
    final events = List.of(_pending);
    _pending.clear();
    final secs = [for (final k in _dirty) _sector(Hex.fromKey(k))];
    _dirty.clear();
    _fleets = [for (final f in _fleets) _norm(f)];
    _hw = _hwWith();
    return TickUpdate(
      tick: _tick,
      player: _player(),
      fleets: List.of(_fleets),
      homeworldUpdate: _hw,
      outpost: _outpostState(),
      sectorUpdates: secs.isEmpty ? null : secs,
      battles: [
        for (final c in _combats.values) _battle(c, withRound: false),
        for (final siege in _sieges.values) _siegeState(siege, withRound: false),
      ],
      events: events.isEmpty ? null : events,
    );
  }

  /// A fight as the state lists it: with its last round in a full state,
  /// without it in a tick update, whose events carry the round.
  BattleState _battle(_Combat c, {required bool withRound}) =>
      BattleState(sector: c.sector, rounds: c.round, lastRound: withRound ? c.lastRound : null, losses: _losses(c, fleetGone: false, groupGone: false));

  /// What the fleet and the group of [c] have lost so far, fleets first.
  static List<BattleLosses> _losses(_Combat c, {required bool fleetGone, required bool groupGone}) {
    List<UnitCount> units(Map<ShipClass, int> lost) => [
          for (final cls in ShipClass.values)
            if ((lost[cls] ?? 0) > 0) UnitCount(unit: ShipyardItem.ship(cls), count: lost[cls]!),
        ];
    return [
      BattleLosses(fleetId: c.fleetId, owner: 'Demo', mine: true, lost: units(c.fleetLost), destroyed: fleetGone),
      BattleLosses(fleetId: c.npc.id, label: c.label, lost: units(c.groupLost), destroyed: groupGone),
    ];
  }

  /// What a hostile group is called, in the words of the engine's
  /// `hostile_label`: "light fighter patrol", "3x cruiser pack". A group of
  /// several classes is named by its first.
  static String _groupLabel(List<(ShipClass, int)> comp, NpcBehavior behavior) {
    final noun = switch (behavior) {
      NpcBehavior.passive => 'convoy',
      NpcBehavior.patrol => 'patrol',
      NpcBehavior.aggressive => 'pack',
      NpcBehavior.swarm => 'swarm',
    };
    final cls = comp.first.$1.label.toLowerCase();
    final count = comp.fold(0, (n, c) => n + c.$2);
    return count > 1 ? '${count}x $cls $noun' : '$cls $noun';
  }

  // ── Per-tick systems ──────────────────────────────────────────

  /// Raw units of ore a tick the ships of [f] extract between them.
  double _harvestPower(FleetState f) => f.ships.fold(0.0, (n, g) => n + g.count * g.shipClass.harvest);

  Density _density(SectorState s, String k) => switch (k) {
        'metal' => s.resources.metal,
        'crystal' => s.resources.crystal,
        _ => s.resources.deuterium,
      };

  void _stepHarvest() {
    for (final f in List.of(_fleets)) {
      if (f.state != FleetStatus.harvesting) continue;
      final pick = _harvest[f.id] ?? HarvestResource.auto;
      final sec = _base(f.location);
      final keys = [
        for (final k in const ['metal', 'crystal', 'deut'])
          if ((pick == HarvestResource.auto || pick.name == k || (pick == HarvestResource.deuterium && k == 'deut')) && _density(sec, k) != Density.none) k
      ];
      final free = f.cargoCapacity - f.cargo.total;
      if (keys.isEmpty || free <= 0) {
        _setFleet(_copy(f, state: FleetStatus.idle));
        _ev(AlertEvent(level: AlertLevel.info, message: free <= 0 ? 'Cargo full: ${f.id} stops harvesting' : 'Nothing left to harvest', fleetId: f.id, sector: f.location));
        continue;
      }
      var m = 0.0, c = 0.0, d = 0.0;
      for (final k in keys) {
        final g = min(free / keys.length, _book.harvestMultiplier(_density(sec, k)) * _harvestPower(f));
        if (k == 'metal') m += g;
        if (k == 'crystal') c += g;
        if (k == 'deut') d += g;
      }
      final cargo = Resources(metal: f.cargo.metal + m, crystal: f.cargo.crystal + c, deuterium: f.cargo.deuterium + d);
      _setFleet(_copy(f, cargo: cargo));
      if (_tick % 10 == 0) {
        _ev(ResourceHarvestedEvent(fleetId: f.id, resources: Resources(metal: m * 10, crystal: c * 10, deuterium: d * 10), ticks: 10));
      }
    }
  }

  void _stepRecall() {
    for (final id in _recall.keys.toList()) {
      final f = _find(id);
      final path = _recall[id]!;
      if (f == null || f.state != FleetStatus.returning) {
        _recall.remove(id);
        continue;
      }
      if (f.cooldownRemaining > 0) continue;
      if (path.isEmpty) {
        _recall.remove(id);
        continue;
      }
      if (f.fuel < f.jumpFuel) {
        _recall.remove(id);
        _setFleet(_copy(f, state: FleetStatus.idle));
        _ev(AlertEvent(level: AlertLevel.warning, message: 'Out of fuel: fleet ${f.id} stranded', fleetId: f.id, sector: f.location));
        continue;
      }
      _hop(f, path.removeAt(0), returning: true);
    }
  }

  void _stepExplore() {
    for (final id in _explores.keys.toList()) {
      final x = _explores[id]!;
      final f = _find(id);
      if (f == null || f.state != FleetStatus.exploring) {
        _explores.remove(id);
        continue;
      }
      if (_tick < x.endTick) continue;
      _explores.remove(id);
      final k = x.sector.toKey();
      final site = _sites[k];
      if (site == null) {
        _setFleet(_copy(f, state: FleetStatus.idle));
        continue;
      }
      _sites.remove(k);
      _dirty.add(k);
      if (site.risk == SiteRisk.hot && _rng.nextDouble() < .6) {
        final g = _npcGroup([(_book.npcClass(x.sector.distFromOrigin), 2)], NpcBehavior.aggressive, x.sector);
        _hostiles[k] = [...?_hostiles[k], g];
        _ev(SiteAmbushEvent(fleetId: id, sector: x.sector, npcFleetId: g.id));
        _setFleet(_copy(f, state: FleetStatus.idle));
        continue;
      }
      final t = site.tier;
      final value = _book.derelictValue(x.sector.distFromOrigin);
      final split = _book.constants('LOOT_SPLIT');
      double part(int i) => (value * split[i] * (.7 + _rng.nextDouble() * .6)).roundToDouble();
      final res = Resources(metal: part(0), crystal: part(1), deuterium: part(2));
      final roll = _rng.nextDouble();
      ShipClass? ship;
      ResearchType? tech;
      if (roll > .6) {
        tech = ResearchType.combustionDrive;
      } else if (roll > .35) {
        // The engine recovers a hulk damaged; how badly is the script's roll.
        ship = ShipClass.lightFighter;
        final full = _arms(ship, _refits[id]!).hull;
        _absorb(_squads[id]!, [
          _Squad(ship, 1, [(full * (.4 + _rng.nextDouble() * .25)).round()]),
        ]);
      }
      _resources = _clampCaps(Resources(
        metal: _resources.metal + res.metal,
        crystal: _resources.crystal + res.crystal,
        deuterium: _resources.deuterium + res.deuterium,
      ));
      _ev(SiteExploredEvent(fleetId: id, sector: x.sector, tier: t, resources: res, recoveredShip: ship, techCache: tech));
      _setFleet(_copy(f, state: FleetStatus.idle));
    }
  }

  void _stepSalvage() {
    for (final k in _salvage.keys.toList()) {
      final e = _salvage[k]!;
      if (e.$2 <= _tick) {
        _salvage.remove(k);
        _dirty.add(k);
        _ev(SalvageDespawnedEvent(sector: Hex.fromKey(k), resources: e.$1));
      }
    }
  }

  void _stepQueues() {
    for (final b in _hw.buildQueue.where((b) => _tick >= b.endTick).toList()) {
      _ev(BuildingCompletedEvent(buildingType: b.buildingType, newLevel: b.targetLevel));
      _hw = _hwWith(
        buildings: [
          for (final e in _hw.buildings)
            e.buildingType == b.buildingType ? BuildingState(buildingType: e.buildingType, level: b.targetLevel) : e,
        ],
        buildQueue: [for (final q in _hw.buildQueue) if (q != b) q],
      );
    }
    final r = _hw.researchActive;
    if (r != null && _tick >= r.endTick) {
      _ev(ResearchCompletedEvent(tech: r.tech, newLevel: r.targetLevel));
      _hw = _hwWith(
        research: [for (final e in _hw.research) e.tech == r.tech ? ResearchState(tech: e.tech, level: r.targetLevel) : e],
        clearResearch: true,
      );
    }
    final s = _hw.shipyardQueue;
    if (s != null && _tick >= s.endTick) {
      if (s.item.ship case final cls?) {
        if (_shipOption(cls).flies) {
          _deliver(cls);
        } else {
          _satellites++;
        }
        _ev(ShipBuiltEvent(shipClass: cls, count: 1));
      } else {
        final k = s.item.defence!;
        _hw = _hwWith(defences: [
          for (final d in _hw.defences) d.kind == k ? DefenceState(kind: k, count: d.count + 1, power: d.power) : d,
        ]);
        _ev(DefenceBuiltEvent(defence: k, count: 1, playerId: _playerId));
      }
      // The batch was paid for when it was ordered: it holds the yard until
      // its last unit is delivered.
      if (s.built + 1 < s.count) {
        _hw = _hwWith(ship: ShipyardQueueItem(id: s.id, item: s.item, count: s.count, built: s.built + 1, startTick: _tick, endTick: _tick + _unitTicksOf(s.item)));
      } else {
        _hw = _hwWith(clearShip: true);
      }
    }
    _startWaiting();
  }

  /// A ship the yard delivers joins the fleet docked at home that flies
  /// under no orders, or forms one, as the engine's does.
  void _deliver(ShipClass cls) {
    final dock = _fleets.where((f) => f.location == _home && f.state == FleetStatus.docked && f.policy == null).firstOrNull;
    if (dock == null) {
      final id = 1 + _fleets.fold<int>(100, (n, f) => max(n, f.id));
      _fleets = [..._fleets, _fleet(id, _home, FleetStatus.docked, [_Squad(cls, 1)])];
    } else {
      _absorb(_squads[dock.id]!, [_Squad(cls, 1)]);
      _setFleet(_copy(dock));
    }
  }

  /// The ships of every fleet standing at the homeworld, a row a class, as
  /// the engine's homeworld state lists them.
  List<ShipGroupState> _dockedRows() {
    final rows = <ShipClass, ShipGroupState>{};
    for (final f in _fleets) {
      if (f.location != _home) continue;
      for (final g in f.ships) {
        rows.update(
            g.shipClass,
            (r) => ShipGroupState(
                shipClass: r.shipClass,
                count: r.count + g.count,
                damaged: r.damaged + g.damaged,
                atRisk: r.atRisk + g.atRisk,
                hull: r.hull + g.hull,
                hullMax: r.hullMax + g.hullMax,
                unitAttack: r.unitAttack,
                unitShield: r.unitShield,
                unitHull: r.unitHull),
            ifAbsent: () => g);
      }
    }
    return [for (final c in ShipClass.values) ?rows[c]];
  }

  /// What the ships standing at the homeworld add to its defence.
  double _dockedPower() => _fleets.where((f) => f.location == _home).fold(0.0, (n, f) => n + f.power);

  // ── Combat ────────────────────────────────────────────────────

  void _startCombat(FleetState f, NpcFleetInfo npc) {
    final label = _groups[npc.id]!.label;
    _combats[f.id] = _Combat(f.id, f.location, npc, label);
    _recall.remove(f.id);
    _explores.remove(f.id);
    _setFleet(_copy(f, state: FleetStatus.inCombat));
    _ev(CombatStartedEvent(playerFleetId: f.id, owner: 'Demo', enemyFleetId: npc.id, enemy: label, sector: f.location, mine: true));
  }

  // ── Flight, pursuit and mending ───────────────────────────────

  /// The engine's `is_refuge`: no group enters home ground, the hub or the
  /// sector an outpost stands in.
  bool _isRefuge(Hex h) => Hex.distance(h, _home) <= 1 || h == Hex.origin || _outpost?.at == h;

  double _groupPowerNow(int id) => _power(_rows(_squads[id]!, _npcArms));

  /// The engine's `flight_quote` for the fleet of fight [c]: what a jump
  /// out costs a fleet that burns [jump] a jump, takes [jumpTicks] over one,
  /// holds [fuel] and has [power], and the group that would follow it, to
  /// [to] if a sector is named.
  FlightQuote _flightQuote(_Combat c, {required double fuel, required double jump, required int jumpTicks, required double power, Hex? to}) {
    final g = _groups[c.npc.id]!;
    final rounds = max(jumpTicks - 1, fleeMinRounds);
    final burn = jump * fleeFuelMultiplier;
    final ratio = power / _observedThreat([_npcInfo(c.npc.id)]).estPower;
    final follows = (to == null || !_isRefuge(to)) && _leash(g) > 0 && _groupPowerNow(c.npc.id) >= chaseMinRatio * power;
    return FlightQuote(
      fuel: burn,
      ticks: rounds + 1,
      rounds: rounds,
      ratio: ratio,
      label: ratioLabelFor(ratio),
      refusal: fuel >= burn
          ? null
          : 'fleet ${c.fleetId} has ${_tenths(fuel)} fuel and the jump out of the fight at ${c.sector} burns ${_tenths(burn)}: it cannot leave, '
              'except by recall if it holds the fuel for that',
      chase: follows
          ? ChaseForecast(
              groupId: c.npc.id, label: g.label, hexes: _leash(g), jumpTicks: _groupJumpTicks(g), roundsPerHex: max(0, jumpTicks - _groupJumpTicks(g)))
          : null,
    );
  }

  /// The engine's `pursuit_of`: the first group to land of those on the
  /// trail of the fleet [id], which stands at [at] with [cooldown] ticks
  /// before it can jump.
  Pursuit? _pursuitOf(int id, Hex at, int cooldown) {
    final hunters = [
      for (final e in _groups.entries)
        if (e.value.errand?.quarry == id) e,
    ]..sort((a, b) => a.value.errand!.arrives.compareTo(b.value.errand!.arrives));
    final first = hunters.firstOrNull;
    if (first == null) return null;
    final errand = first.value.errand!;
    final arrivesIn = max(0, errand.arrives - _tick);
    return Pursuit(
        groupId: first.key,
        label: first.value.label,
        from: first.value.at,
        arrivesIn: arrivesIn,
        catches: errand.to == at && arrivesIn <= cooldown,
        hexesLeft: _leash(first.value));
  }

  /// The engine's `trail`: why group [g] does not go after the fleet [id],
  /// in its sentence; null when it does.
  String? _trail(_Group g, int id) {
    final f = _find(id);
    if (f == null) return 'fleet $id is gone';
    if (_isRefuge(f.location)) return 'fleet $id has reached ${f.location}, where no group follows';
    if (_leash(g) == 0) return 'it has followed ${g.strayed} jump${g.strayed == 1 ? '' : 's'} from its own sector at ${g.lair} and goes no further';
    if (!_lanes(g.at).contains(f.location)) return 'fleet $id is out of its reach at ${f.location}';
    return null;
  }

  void _pursuitEvent(int id, int quarry, PursuitPhase phase, {String? why}) {
    final g = _groups[id]!;
    _ev(PursuitEvent(
        groupId: id,
        label: g.label,
        fleetId: quarry,
        owner: 'Demo',
        mine: true,
        sector: g.at,
        phase: phase,
        to: g.errand?.to,
        arrivesIn: g.errand == null ? null : g.errand!.arrives - _tick,
        hexesLeft: _leash(g),
        why: why));
  }

  /// The engine's `chase`: group [id] starts its jump to where the fleet
  /// [quarry] stands.
  void _chase(int id, int quarry, PursuitPhase phase) {
    final g = _groups[id]!;
    g.errand = _Errand(quarry, _find(quarry)!.location, _tick + _groupJumpTicks(g));
    g.strayed++;
    g.rested = 0;
    _pursuitEvent(id, quarry, phase);
    _relist(id);
  }

  /// The engine's `settle_group`: a group with no fight and no fleet to
  /// follow starts the next jump of its way home, or is home.
  void _settle(int id) {
    final g = _groups[id]!;
    if (g.at == g.lair) {
      g.strayed = 0;
    } else {
      final lanes = _lanes(g.at);
      final step = lanes.contains(g.lair) ? g.lair : lanes.where((n) => !_isRefuge(n) && _lanes(n).contains(g.lair)).firstOrNull;
      if (step == null) {
        _stand(id, g.lair);
        g.strayed = 0;
      } else {
        g.errand = _Errand(null, step, _tick + _groupJumpTicks(g));
      }
    }
    _relist(id);
  }

  /// Take group [id] out of the sector it is listed in and list it in [at].
  void _stand(int id, Hex at) {
    final g = _groups[id]!;
    final from = g.at.toKey();
    _hostiles[from] = [for (final h in _hostiles[from] ?? const <NpcFleetInfo>[]) if (h.id != id) h];
    g.at = at;
    _hostiles[at.toKey()] = [...?_hostiles[at.toKey()], _npcInfo(id)];
    _dirty
      ..add(from)
      ..add(at.toKey());
  }

  /// The engine's `Ships::heal`: one step of group [id]'s mending.
  void _heal(int id) {
    final g = _groups[id]!;
    final squads = _squads[id]!;
    for (final theirs in g.muster) {
      final full = _npcArms(theirs.cls).hull;
      var here = squads.where((s) => s.cls == theirs.cls).firstOrNull;
      if (here == null) {
        here = _Squad(theirs.cls, 0);
        squads.add(here);
      }
      for (var i = 0; i < here.wounds.length; i++) {
        here.wounds[i] += _healHull(full);
      }
      here.wounds.removeWhere((hull) => hull >= full);
      here.count += min(theirs.count - here.count, _healShips(theirs.count));
    }
    squads.sort((a, b) => a.cls.index.compareTo(b.cls.index));
  }

  /// The groups' jumps and their mending. One that lands where a fleet
  /// stands fires on it; one that lands on an empty sector goes on after
  /// its fleet or turns back; one at rest in its lair mends a step when its
  /// time has come.
  void _stepGroups() {
    for (final id in _groups.keys.toList()) {
      final g = _groups[id]!;
      final errand = g.errand;
      final fighting = _combats.values.any((c) => c.npc.id == id);
      if (errand == null) {
        if (fighting || g.at != g.lair || _healSteps(id) == 0) continue;
        g.rested++;
        if (g.rested < _book.healStepTicks(g.lair.distFromOrigin)) continue;
        g.rested = 0;
        _heal(id);
        _relist(id);
        continue;
      }
      if (errand.arrives > _tick) {
        if (_healSteps(id) > 0) _relist(id);
        continue;
      }
      g.errand = null;
      _stand(id, errand.to);
      final met = g.behavior == NpcBehavior.passive || _isRefuge(g.at)
          ? null
          : ([for (final f in _fleets) if (f.location == g.at && f.state != FleetStatus.docked && !_combats.containsKey(f.id)) f]
                ..sort((a, b) => a.id == errand.quarry ? -1 : b.id == errand.quarry ? 1 : a.id.compareTo(b.id)))
              .firstOrNull;
      if (met != null) {
        _startCombat(met, _npcInfo(id));
        _relist(id);
        continue;
      }
      final quarry = errand.quarry;
      if (quarry == null) {
        _settle(id);
        continue;
      }
      final why = _trail(g, quarry);
      if (why == null) {
        _chase(id, quarry, PursuitPhase.continued);
      } else {
        _settle(id);
        _pursuitEvent(id, quarry, PursuitPhase.abandoned, why: why);
      }
    }
  }

  /// The fleet of fight [c] lands in [to]: it is out of the fight, which
  /// ends `fled`, and the group goes after it if it was beating it.
  void _landOutOfFight(_Combat c, FleetState f, Hex to) {
    _combats.remove(c.fleetId);
    _hop(f, to, paid: true);
    _ev(CombatEndedEvent(
        sector: c.sector,
        playerVictory: false,
        outcome: BattleOutcome.fled,
        rounds: c.round,
        losses: _losses(c, fleetGone: false, groupGone: false),
        owners: const ['Demo'],
        mine: true));
    final g = _groups[c.npc.id]!;
    g.rested = 0;
    final landed = _find(c.fleetId)!;
    if (_trail(g, c.fleetId) == null && _groupPowerNow(c.npc.id) >= chaseMinRatio * landed.power) {
      _chase(c.npc.id, c.fleetId, PursuitPhase.began);
    } else {
      _settle(c.npc.id);
    }
  }

  /// What [from] fires at [at] in one round, squad at squad, and what it
  /// does to [at], whose squads are changed. Without dice: every unit of
  /// [from] fires once, at the squads of [at] in proportion to their
  /// numbers, and a ship as often again as its rapid fire against what it
  /// hits says. A shot under a hundredth of the shield it meets bounces;
  /// each other one meets a full shield and takes the rest from the hull of
  /// the squad's most damaged unit, which explodes if it was already at
  /// risk. Two squads of one unit on a side, as two fleets at a dock have,
  /// share a line of the report.
  _Fire _volleys(List<_Squad> from, _Arms Function(_Squad) fromArms, List<_Squad> at, _Arms Function(_Squad) atArms) {
    final stood = [for (final t in at) t.count];
    final total = _shipsIn(at);
    final lines = <(ShipyardItem, ShipyardItem), Volley>{};
    var power = 0;
    if (total == 0) return (volleys: const [], power: 0, hull: 0);
    for (final a in from) {
      final attack = fromArms(a).attack;
      var spare = a.count - stood.fold(0, (n, count) => n + a.count * count ~/ total);
      for (final (i, t) in at.indexed) {
        final aimed = a.count * stood[i] ~/ total + (spare-- > 0 ? 1 : 0);
        final again = _book.rapidFire.where((r) => r.shipClass == a.unit.ship && r.against == t.unit).firstOrNull?.shots ?? 1;
        final shots = aimed * again;
        if (shots == 0) continue;
        final target = atArms(t);
        power += shots * attack;
        var bounced = 0, wasted = 0, absorbed = 0, hullDamage = 0, destroyed = 0;
        if (attack * 100 < target.shield) {
          bounced = shots;
        } else {
          for (var i = 0; i < shots; i++) {
            if (t.count == 0) {
              wasted = shots - i;
              break;
            }
            absorbed += min(attack, target.shield);
            final through = attack - target.shield;
            if (through <= 0) continue;
            final hull = t.wounds.isEmpty ? target.hull : t.wounds.removeAt(0);
            hullDamage += min(through, hull);
            if (hull <= through || _atRisk(hull, target.hull)) {
              t.count--;
              destroyed++;
            } else {
              t.wounds.insert(0, hull - through);
            }
          }
        }
        lines.update(
            (a.unit, t.unit),
            (v) => Volley(
                from: v.from,
                at: v.at,
                shots: v.shots + shots,
                bounced: v.bounced + bounced,
                wasted: v.wasted + wasted,
                absorbed: v.absorbed + absorbed,
                hullDamage: v.hullDamage + hullDamage,
                destroyed: v.destroyed + destroyed),
            ifAbsent: () => Volley(
                from: a.unit, at: t.unit, shots: shots, bounced: bounced, wasted: wasted, absorbed: absorbed, hullDamage: hullDamage, destroyed: destroyed));
      }
    }
    final volleys = lines.values.toList();
    return (volleys: volleys, power: power, hull: volleys.fold(0, (n, v) => n + v.hullDamage));
  }

  /// What a round has left of a fleet's or a group's [squads], as the world
  /// keeps them between rounds: those with a unit left, without the arms
  /// the round gave them.
  static List<_Squad> _standing(List<_Squad> squads) => [for (final s in squads) if (s.count > 0) _Squad.of(s.unit, s.count, s.wounds)];

  /// The squads of one fleet, group or battery as a round's report lists
  /// them: [after] the round, squad for squad with [before], which is armed,
  /// and what the round took from each.
  static List<BattleUnits> _unitsAfter(List<_Squad> before, List<_Squad> after) => [
        for (final (i, was) in before.indexed)
          () {
            final now = after[i];
            final unit = was.arms!;
            return BattleUnits(
              unit: was.unit,
              entered: was.count,
              count: now.count,
              damaged: now.wounds.length,
              atRisk: now.wounds.where((hull) => _atRisk(hull, unit.hull)).length,
              hull: (now.count - now.wounds.length) * unit.hull + now.wounds.fold(0, (n, hull) => n + hull),
              hullMax: now.count * unit.hull,
              lost: was.count - now.count,
            );
          }(),
      ];

  /// One side of a round as its report carries it: its [groups] as they
  /// stand after the round, all of them [standing], and what it [fired].
  static BattleSide _side(List<BattleGroup> groups, List<_Squad> standing, _Fire fired) => BattleSide(
        groups: groups,
        entered: groups.fold(0, (n, g) => n + g.units.fold(0, (m, u) => m + u.entered)),
        standing: _shipsIn(standing),
        volleys: fired.volleys,
        shots: fired.volleys.fold(0, (n, v) => n + v.shots),
        power: fired.power,
        absorbed: fired.volleys.fold(0, (n, v) => n + v.absorbed),
        hullDamage: fired.hull,
        destroyed: fired.volleys.fold(0, (n, v) => n + v.destroyed),
      );

  void _stepCombat() {
    // Aggressive groups engage fleets that share their sector.
    for (final f in List.of(_fleets)) {
      if (f.state == FleetStatus.docked || f.state == FleetStatus.inCombat || _combats.containsKey(f.id)) continue;
      final hs = _hostiles[f.location.toKey()];
      if (hs == null || hs.isEmpty) continue;
      final npc = hs.first;
      if (_groups[npc.id]!.errand != null) continue;
      final provoked = _provoked.remove(f.id);
      final engage = npc.behavior == NpcBehavior.aggressive ||
          npc.behavior == NpcBehavior.swarm ||
          (npc.behavior == NpcBehavior.patrol && provoked && _rng.nextDouble() < .4);
      if (engage && !_combats.values.any((c) => c.npc.id == npc.id)) _startCombat(f, npc);
    }
    for (final c in _combats.values.toList()) {
      var f = _find(c.fleetId);
      if (f == null) {
        _combats.remove(c.fleetId);
        continue;
      }
      final leaving = c.fleeTo;
      if (leaving != null && c.fleeRounds == 0) {
        _landOutOfFight(c, f, leaving);
        continue;
      }
      c.round++;
      final fleetBefore = _fleetArmed(f.id);
      final groupBefore = [for (final s in [..._squads[c.npc.id]!]..sort((a, b) => a.cls.index.compareTo(b.cls.index))) s.armed(_baseArms(s))];
      final fleet = [for (final s in fleetBefore) s.copy()], group = [for (final s in groupBefore) s.copy()];
      final _Fire ours = leaving == null ? _volleys(fleetBefore, _armed, group, _armed) : (volleys: const [], power: 0, hull: 0);
      final theirs = _volleys(groupBefore, _armed, fleet, _armed);
      if (leaving != null) c.fleeRounds--;
      final round = BattleRoundEvent(
        sector: c.sector,
        round: c.round,
        fleets: _side(
            [BattleGroup(fleetId: c.fleetId, owner: 'Demo', mine: true, fleeing: leaving != null, units: _unitsAfter(fleetBefore, fleet))], fleet, ours),
        hostiles: _side([BattleGroup(fleetId: c.npc.id, label: c.label, units: _unitsAfter(groupBefore, group))], group, theirs),
        mine: true,
      );
      for (final u in round.fleets.groups.single.units) {
        if (u.lost > 0) c.fleetLost.update(u.unit.ship!, (n) => n + u.lost, ifAbsent: () => u.lost);
      }
      for (final u in round.hostiles.groups.single.units) {
        if (u.lost > 0) c.groupLost.update(u.unit.ship!, (n) => n + u.lost, ifAbsent: () => u.lost);
      }
      c.lastRound = round;
      _ev(BattleRoundKind(round));
      _squads[f.id] = _standing(fleet);
      _squads[c.npc.id] = _standing(group);
      final k = c.sector.toKey();
      final fleetGone = _squads[f.id]!.isEmpty, groupGone = _squads[c.npc.id]!.isEmpty;
      final quiet = leaving == null && round.fleets.hullDamage + round.hostiles.hullDamage == 0;
      if (!groupGone) _relist(c.npc.id);
      if (!fleetGone && !groupGone && !quiet) {
        _setFleet(_copy(f));
        continue;
      }

      _combats.remove(c.fleetId);
      Resources? pile;
      int? despawn;
      if (fleetGone) {
        _fleets = [for (final e in _fleets) if (e.id != f.id) e];
        _squads.remove(f.id);
        _refits.remove(f.id);
        _ev(FleetDestroyedEvent(fleetId: f.id, isNpc: false, sector: c.sector, owner: 'Demo', mine: true, salvage: const Resources()));
      }
      if (groupGone) {
        final muster = _power(_rows(_groups[c.npc.id]!.muster, _npcArms));
        final worth = _book.constant('WRECK_VALUE_K') * pow(muster, _book.constant('WRECK_VALUE_EXPONENT'));
        final split = _book.constants('LOOT_SPLIT');
        final loot = Resources(metal: worth * split[0], crystal: worth * split[1], deuterium: worth * split[2]);
        _hostiles[k] = [for (final h in _hostiles[k] ?? const <NpcFleetInfo>[]) if (h.id != c.npc.id) h];
        _squads.remove(c.npc.id);
        _groups.remove(c.npc.id);
        pile = _plus(loot, _salvage[k]?.$1 ?? const Resources());
        despawn = _tick + 600;
        _salvage[k] = (pile, despawn);
        _dirty.add(k);
        _ev(FleetDestroyedEvent(fleetId: c.npc.id, isNpc: true, sector: c.sector, owner: null, mine: false, salvage: loot, label: c.label));
      } else {
        _groups[c.npc.id]!.rested = 0;
        _settle(c.npc.id);
      }
      _ev(CombatEndedEvent(
          sector: c.sector,
          playerVictory: groupGone && !fleetGone,
          outcome: groupGone
              ? (fleetGone ? BattleOutcome.bothDestroyed : BattleOutcome.hostilesDestroyed)
              : (fleetGone ? BattleOutcome.fleetsDestroyed : BattleOutcome.standoff),
          rounds: c.round,
          losses: _losses(c, fleetGone: fleetGone, groupGone: groupGone),
          owners: const ['Demo'],
          mine: true,
          salvage: pile,
          salvageDespawnTick: despawn));
      if (!fleetGone) _setFleet(_copy(f, state: FleetStatus.idle));
    }
  }

  // ── Fleet helpers ─────────────────────────────────────────────

  /// What a fleet of [ships] is, as the catalogue's rows for its classes
  /// add up: the fuel a jump burns, its tank and its hold are sums, and it
  /// jumps as slowly as its slowest ship.
  double _jump(List<_Squad> ships) => ships.fold(0.0, (n, s) => n + s.count * _shipOption(s.cls).jumpFuel);
  double _tank(List<_Squad> ships) => ships.fold(0.0, (n, s) => n + s.count * _shipOption(s.cls).tank);
  double _hold(List<_Squad> ships) => ships.fold(0.0, (n, s) => n + s.count * _shipOption(s.cls).cargo);
  int _jumpTicks(List<_Squad> ships) => ships.fold(1, (m, s) => max(m, _shipOption(s.cls).jumpTicks));

  /// The server's `scan_range`: farther with an Espionage Probe aboard.
  static int _scanRange(List<_Squad> ships) => ships.any((s) => s.cls == ShipClass.espionageProbe) ? scanProbeRange : scanBaseRange;

  /// A fleet standing at the homeworld and not leaving it is at the home
  /// dock: it takes the empire's levels and its damaged ships are made
  /// whole, for nothing.
  bool _atHomeDock(FleetState f) => f.location == _home && f.state != FleetStatus.moving && f.state != FleetStatus.inCombat;

  /// The engine's `mending`: what a dock that charges [share] of a ship's
  /// cost for a whole hull would do for the damaged ships of fleet [id]
  /// with [stock]. The most damaged ship is mended first, and a ship the
  /// stock does not cover is left as it is with every ship after it.
  ({DockRepair quote, List<(ShipClass, int)> mended}) _mending(int id, double share, Resources stock, {String? refusal}) {
    final refit = _refits[id]!;
    final worn = [
      for (final s in _squads[id]!)
        for (final hull in s.wounds) (cls: s.cls, hull: hull, full: _arms(s.cls, refit).hull),
    ]..sort((a, b) => (a.hull / a.full).compareTo(b.hull / b.full));
    double cost(({ShipClass cls, int hull, int full}) ship) {
      final at = _shipOption(ship.cls).unitCost;
      final par = ResourceKind.values.fold(0.0, (n, k) => n + k.weight * k.of(at));
      return par * share * (ship.full - ship.hull) / ship.full;
    }

    final price = worn.fold(0.0, (n, ship) => n + cost(ship));
    if (refusal != null) {
      return (quote: DockRepair(price: price, payment: const Resources(), ships: 0, hull: 0, refusal: refusal), mended: const []);
    }
    final (_, short) = _payByWeight(price, stock);
    var budget = price - short, spent = 0.0, hull = 0;
    final mended = <(ShipClass, int)>[];
    for (final ship in worn) {
      final due = cost(ship);
      if (short > 0 && due > budget) break;
      budget -= due;
      spent += due;
      hull += ship.full - ship.hull;
      mended.add((ship.cls, ship.hull));
    }
    return (
      quote: DockRepair(price: price, payment: _payByWeight(short > 0 ? spent : price, stock).$1, ships: mended.length, hull: hull),
      mended: mended,
    );
  }

  /// Why the outpost would mend nothing for [f], in the engine's words;
  /// null when it would.
  String? _outpostMendRefusal(FleetState f, _Outpost o) => o.level == 0
      ? 'the outpost at ${o.at} is not finished'
      : o.breached
          ? 'the outpost at ${o.at} is breached and mends nothing until it is repaired'
          : f.policy != null
              ? 'a fleet under standing orders does not use the outpost'
              : null;

  ({DockRepair quote, List<(ShipClass, int)> mended}) _homeMending(int id) =>
      _mending(id, _book.constant('REPAIR_COST_SHARE_HOME'), _resources);

  ({DockRepair quote, List<(ShipClass, int)> mended}) _outpostMending(FleetState f, _Outpost o) =>
      _mending(f.id, _book.constant('REPAIR_COST_SHARE_OUTPOST'), o.store, refusal: _outpostMendRefusal(f, o));

  /// The docks' work, each tick: the home dock refits a fleet standing at
  /// it to the empire's levels, saying so when it has just come in, and
  /// makes its damaged ships whole; the outpost's mends what its store
  /// pays for. Each says what it did in the engine's sentence.
  void _stepDocks() {
    _tended.removeWhere((id) => _fleets.every((f) => f.id != id || !_atHomeDock(f)));
    for (final f in List.of(_fleets)) {
      final o = _outpost;
      if (_atHomeDock(f)) {
        final was = _refits[f.id]!, now = _levels;
        if (was != now) {
          _refits[f.id] = now;
          _rearm(_squads[f.id]!, was, now);
          if (!_tended.contains(f.id)) {
            _ev(AlertEvent(
                level: AlertLevel.info,
                message: 'fleet ${f.id} was refitted at the home dock: Weapons ${now.weapons}, Shielding ${now.shielding}, Armour ${now.armour} '
                    '(it flew with ${was.weapons}, ${was.shielding}, ${was.armour})',
                fleetId: f.id,
                sector: f.location));
          }
        }
        _tended.add(f.id);
        _mend(f, _homeMending(f.id), 'the home dock', (paid) => _resources = _less(_resources, paid));
      } else if (o != null && f.location == o.at && f.state != FleetStatus.moving && f.state != FleetStatus.inCombat) {
        _mend(f, _outpostMending(f, o), 'your outpost', (paid) => o.store = _less(o.store, paid));
      }
    }
  }

  /// Ships that fought at the levels [was] now fight at [now]: a damaged
  /// one keeps the share of its hull it held.
  static void _rearm(List<_Squad> ships, Refit was, Refit now) {
    for (final s in ships) {
      final (old, full) = (_arms(s.cls, was).hull, _arms(s.cls, now).hull);
      for (var i = 0; i < s.wounds.length; i++) {
        s.wounds[i] = (s.wounds[i] * full / old).round().clamp(1, full - 1);
      }
    }
  }

  void _mend(FleetState f, ({DockRepair quote, List<(ShipClass, int)> mended}) work, String place, void Function(Resources) pay) {
    if (work.mended.isEmpty) return;
    final ships = _squads[f.id]!;
    for (final (cls, hull) in work.mended) {
      ships.firstWhere((s) => s.cls == cls).wounds.remove(hull);
    }
    pay(work.quote.payment);
    final left = ships.fold(0, (n, s) => n + s.wounds.length);
    _ev(AlertEvent(
        level: AlertLevel.info,
        message: 'fleet ${f.id} was mended at $place: ${work.quote.ships} ship(s) made whole, ${work.quote.hull} hull restored, '
            '${work.quote.payment.total > 0 ? 'for ${_amount(work.quote.payment)}' : 'for nothing'}'
            '${left > 0 ? '; $left more wait for the dock\'s stock to cover them' : ''}',
        fleetId: f.id,
        sector: f.location));
    _setFleet(_copy(f));
  }

  /// What mending the damaged ships of [f] would take at each dock, as the
  /// engine quotes it; null when none is damaged.
  RepairQuote? _repairQuote(FleetState f) {
    final ships = _squads[f.id]!;
    final worn = ships.fold(0, (n, s) => n + s.wounds.length);
    if (worn == 0) return null;
    final refit = _refits[f.id]!;
    final o = _outpost;
    return RepairQuote(
      ships: worn,
      hull: ships.fold(0, (n, s) => n + s.wounds.fold(0, (m, hull) => m + _arms(s.cls, refit).hull - hull)),
      home: _homeMending(f.id).quote,
      outpost: o == null ? null : _outpostMending(f, o).quote,
    );
  }

  FleetState _norm(FleetState f) {
    final ships = _squads[f.id]!;
    final refit = _refits[f.id]!;
    final rows = _rows(ships, (c) => _arms(c, refit));
    final jump = _jump(ships);
    final tank = _tank(ships);
    final homeHops = _pathHome(f.location)?.length ?? Hex.distance(f.location, _home);
    final homeFuel = homeHops * jump;
    final levels = _levels;
    final fight = _combats[f.id];
    final leaving = fight?.fleeTo;
    final fuel = min(f.fuel, tank);
    final siege = _sieges[f.location.toKey()];
    final bound = siege?.leaving[f.id];
    return FleetState(
        flight: fight != null && leaving == null
            ? _flightQuote(fight, fuel: fuel, jump: jump, jumpTicks: _jumpTicks(ships), power: _power(rows))
            : siege != null && bound == null && f.state == FleetStatus.inCombat
                ? _raidFlight(siege, f.id, fuel: fuel, jump: jump, jumpTicks: _jumpTicks(ships))
                : null,
        fleeing: switch ((leaving, bound)) {
          (final to?, _) => Fleeing(to: to, ticks: fight!.fleeRounds + 1, rounds: fight.fleeRounds),
          (null, (final to, final rounds)?) => Fleeing(to: to, ticks: rounds + 1, rounds: rounds),
          _ => null,
        },
        pursuedBy: _pursuitOf(f.id, f.location, f.cooldownRemaining),
        id: f.id,
        location: f.location,
        state: f.state,
        ships: rows,
        shipCount: _shipsIn(ships),
        hull: rows.fold(0, (n, g) => n + g.hull),
        hullMax: rows.fold(0, (n, g) => n + g.hullMax),
        attack: rows.fold(0, (n, g) => n + g.count * g.unitAttack),
        shield: rows.fold(0, (n, g) => n + g.count * g.unitShield),
        refit: refit,
        refitDue: refit.weapons < levels.weapons || refit.shielding < levels.shielding || refit.armour < levels.armour,
        repair: _repairQuote(f),
        cargo: f.cargo,
        cargoCapacity: _hold(ships),
        fuel: min(f.fuel, tank),
        fuelMax: tank,
        jumpFuel: jump,
        homeFuel: homeFuel,
        hopsHome: homeHops,
        jumpTicks: _jumpTicks(ships),
        scanRange: _scanRange(ships),
        atOutpost: _outpost?.at == f.location,
        outpostFuel: _inService ? Hex.distance(f.location, _outpost!.at) * jump : null,
        cooldownRemaining: f.cooldownRemaining,
        power: _power(rows),
        rangeHops: jump > 0 ? max(0, ((f.fuel - homeFuel) / (2 * jump)).floor()) : 0,
        cargoBlocked: f.cargoBlocked,
        laden: f.laden,
        refine: _refining(f, tank),
        policy: f.policy,
      );
  }

  /// The engine's `refining`: what a refine order would do now for [f],
  /// whose tank holds [fuelMax], null where the engine refuses it.
  static Refining? _refining(FleetState f, double fuelMax) {
    if (f.state == FleetStatus.inCombat || f.state == FleetStatus.moving) return null;
    final aboard = f.cargo.deuterium;
    final room = fuelMax - f.fuel;
    if (aboard <= 0 || room <= 0) return null;
    const rate = (1 - refineLoss) / fuelDeutPerUnit;
    final fuel = aboard * rate;
    return fuel <= room ? Refining(deuterium: aboard, fuel: fuel) : Refining(deuterium: room / rate, fuel: room);
  }

  /// The engine's `fuel_words`: tenths, and none for a whole number.
  static String _tenths(double v) {
    final t = (v * 10).round() / 10;
    return t == t.roundToDouble() ? t.toStringAsFixed(0) : t.toStringAsFixed(1);
  }

  /// The server's `refine`, with its refusal sentences.
  List<ServerMessage> _refine(int fleetId) {
    final f = _find(fleetId);
    if (f == null) return [_err(ErrorCode.fleetNotFound, 'fleet $fleetId not found (or it has no ships left)')];
    final offer = _refining(f, f.fuelMax);
    if (offer == null) {
      return [
        switch (f.state) {
          FleetStatus.inCombat => _err(ErrorCode.onCooldown, 'fleet $fleetId is in combat; nothing is refined in a fight'),
          FleetStatus.moving => _err(ErrorCode.onCooldown, 'fleet $fleetId is charging a jump; it can refine once it has arrived'),
          _ when f.cargo.deuterium <= 0 => _err(ErrorCode.noResources, 'the hold of fleet $fleetId carries no deuterium to refine'),
          _ => _err(ErrorCode.invalidCommand,
              'the tank of fleet $fleetId is full (${_tenths(f.fuel)} of ${_tenths(f.fuelMax)} fuel); refining would add nothing'),
        },
      ];
    }
    final left = max(0.0, f.cargo.deuterium - offer.deuterium);
    final fuel = offer.fuel >= f.fuelMax - f.fuel ? f.fuelMax : f.fuel + offer.fuel;
    _setFleet(_copy(f, fuel: fuel, cargo: Resources(metal: f.cargo.metal, crystal: f.cargo.crystal, deuterium: left)));
    final hold = left > 0 ? '${_tenths(left)} deuterium is left in the hold' : 'the hold has no deuterium left';
    _ev(AlertEvent(
        level: AlertLevel.info,
        message: 'fleet $fleetId refined ${_tenths(offer.deuterium)} deuterium into ${_tenths(offer.fuel)} fuel: '
            'the tank stands at ${_tenths(fuel)} of ${_tenths(f.fuelMax)}, and $hold',
        fleetId: fleetId,
        sector: f.location));
    return [_tickUpdate()];
  }

  FleetState _copy(
    FleetState f, {
    Hex? location,
    FleetStatus? state,
    Resources? cargo,
    StandingOrders? policy,
    bool clearPolicy = false,
    double? fuel,
    int? cooldown,
    bool? laden,
  }) =>
      _norm(FleetState(
        id: f.id,
        location: location ?? f.location,
        state: state ?? f.state,
        ships: f.ships,
        shipCount: f.shipCount,
        hull: f.hull,
        hullMax: f.hullMax,
        attack: f.attack,
        shield: f.shield,
        refit: f.refit,
        cargo: cargo ?? f.cargo,
        cargoCapacity: f.cargoCapacity,
        fuel: fuel ?? f.fuel,
        fuelMax: f.fuelMax,
        hopsHome: f.hopsHome,
        jumpTicks: f.jumpTicks,
        scanRange: f.scanRange,
        cooldownRemaining: cooldown ?? f.cooldownRemaining,
        laden: laden ?? f.laden,
        policy: clearPolicy ? null : (policy ?? f.policy),
      ));

  FleetState? _find(int id) {
    for (final f in _fleets) {
      if (f.id == id) return f;
    }
    return null;
  }

  void _setFleet(FleetState f) {
    _fleets = [for (final e in _fleets) e.id == f.id ? f : e];
  }

  ServerMessage _err(ErrorCode c, String m) => ErrorMessage(code: c, message: m);

  /// Chart [h] and its neighbours up to [radius]; the uncharted NW contact
  /// only appears through a scan.
  int _reveal(Hex h, int radius, {bool scan = false}) {
    var fresh = 0;
    for (final n in hexSpiral(h, radius)) {
      if (!scan && n == const Hex(5, -4)) continue;
      if (n.distFromOrigin > _hubRadius) continue;
      if (_seen.putIfAbsent(n.toKey(), () => _tick) == _tick) fresh++;
      _dirty.add(n.toKey());
    }
    return fresh;
  }

  /// The fleet stands in [target]; [paid] when its jump's fuel was burned
  /// as it was ordered, as a jump out of a fight is.
  void _hop(FleetState f, Hex target, {bool returning = false, bool paid = false}) {
    final first = !_seen.containsKey(target.toKey());
    final home = target == _home;
    final cur = _find(f.id) ?? f;
    var next = _copy(
      cur,
      location: target,
      state: home ? FleetStatus.docked : (returning ? FleetStatus.returning : FleetStatus.idle),
      fuel: paid ? cur.fuel : cur.fuel - cur.jumpFuel,
      cooldown: paid ? 0 : cur.jumpTicks,
      laden: false,
    );
    if (home) {
      _resources = _clampCaps(Resources(
        metal: _resources.metal + cur.cargo.metal,
        crystal: _resources.crystal + cur.cargo.crystal,
        deuterium: _resources.deuterium + cur.cargo.deuterium,
      ));
      next = _copy(next, cargo: const Resources(), fuel: cur.fuelMax, cooldown: 0);
      _recall.remove(cur.id);
    }
    _setFleet(next);
    _reveal(target, 1);
    _dirty.add(cur.location.toKey());
    _ev(FleetArrivedEvent(fleetId: cur.id, sector: target));
    _ev(SectorEnteredEvent(fleetId: cur.id, sector: target, firstVisit: first));
    final hs = _hostiles[target.toKey()];
    if (hs != null && hs.isNotEmpty && hs.first.behavior != NpcBehavior.passive) _provoked.add(cur.id);
  }

  List<Hex>? _pathHome(Hex from) {
    if (from == _home) return [];
    final prev = <Hex, Hex>{};
    final q = <Hex>[from];
    final seen = {from};
    for (var i = 0; i < q.length && i < 4000; i++) {
      final cur = q[i];
      if (cur == _home) break;
      for (final n in _lanes(cur)) {
        if (seen.add(n) && n.distFromOrigin <= _hubRadius) {
          prev[n] = cur;
          q.add(n);
        }
      }
    }
    if (!prev.containsKey(_home)) return null;
    final out = <Hex>[];
    for (var h = _home; h != from; h = prev[h]!) {
      out.insert(0, h);
    }
    return out;
  }

  // ── Catalog ───────────────────────────────────────────────────

  HomeworldState _hwWith({
    List<BuildingState>? buildings,
    List<ResearchState>? research,
    List<BuildQueueItem>? buildQueue,
    List<QueuedBuild>? buildPending,
    ShipyardQueueItem? ship,
    List<QueuedShip>? shipPending,
    ResearchItem? researchItem,
    List<QueuedResearch>? researchPending,
    List<DefenceState>? defences,
    bool clearShip = false,
    bool clearResearch = false,
  }) {
    final b = buildings ?? _hw.buildings;
    final r = research ?? _hw.research;
    final yard = clearShip ? null : (ship ?? _hw.shipyardQueue);
    final waiting = shipPending ?? _hw.shipyardPending;
    final standing = defences ?? _hw.defences;
    final energy = _energy(b);
    final production = _production(b, energy);
    final catalog = _catalogFor(b, r, {
      for (final d in standing)
        d.kind: (
          d.count,
          d.count +
              (yard?.item.defence == d.kind ? yard!.count - yard.built : 0) +
              waiting.where((q) => q.item.defence == d.kind).fold(0, (n, q) => n + q.count)
        ),
    }, energy.made);
    final defs = [
      for (final d in standing) DefenceState(kind: d.kind, count: d.count, power: catalog.defences.firstWhere((o) => o.kind == d.kind).power),
    ];
    final power = _defence(defs, catalog);
    final docked = _dockedRows();
    final computer = r.firstWhere((e) => e.tech == ResearchType.computerTechnology).level;
    final unlock = _slotUnlock(computer);
    return _hw.copyWith(
      homeDefencePower: power.total,
      dockedPower: power.docked,
      satellitePower: power.satellites,
      structurePower: power.structures,
      production: production,
      energy: energy,
      satellites: _satellites,
      storage: _storage(catalog, _resources, production),
      buildings: b,
      research: r,
      buildQueue: buildQueue,
      buildPending: buildPending,
      buildSlots: _slotsAt(computer),
      buildSlotUnlock: unlock,
      clearBuildSlotUnlock: unlock == null,
      shipyardQueue: ship,
      clearShipyardQueue: clearShip,
      shipyardPending: shipPending,
      researchActive: researchItem,
      clearResearchActive: clearResearch,
      researchPending: researchPending,
      dockedShips: docked,
      dockedShipCount: docked.fold<int>(0, (n, g) => n + g.count),
      defences: defs,
      raid: _homeForecast(),
      catalog: catalog,
    );
  }

  /// The home's defence in the three parts the server counts.
  ({double docked, double satellites, double structures, double total}) _defence(List<DefenceState> standing, [HomeworldCatalog? catalog]) {
    final docked = _dockedPower();
    final each = (catalog ?? _hw.catalog).ships.firstWhere((o) => o.shipClass == ShipClass.solarSatellite).power;
    final satellites = _satellites * each;
    final structures = standing.fold(0.0, (n, d) => n + d.count * d.power);
    return (docked: docked, satellites: satellites, structures: structures, total: docked + satellites + structures);
  }

  static Effect _effect(List<Effect> effects, EffectKind kind) => effects.firstWhere((e) => e.kind == kind);

  /// Building slots at a level of Computer Technology, which is what its
  /// row of the catalogue says it gives.
  static int _slotsAt(int computer) =>
      _effect(_book.byLevel[computer].research[ResearchType.computerTechnology.index].effects, EffectKind.buildSlots).now.round();

  /// What opens the next building slot: the first level of Computer
  /// Technology above [computer] that gives more of them; none past the last.
  static Requirement? _slotUnlock(int computer) {
    for (var level = computer + 1; level < _book.byLevel.length; level++) {
      if (_slotsAt(level) > _slotsAt(computer)) {
        return Requirement(name: ResearchType.computerTechnology.label, need: level, have: computer, met: false, energy: false);
      }
    }
    return null;
  }

  Resources _unitCostOf(ShipyardItem item) => item.ship != null
      ? _hw.catalog.ships.firstWhere((o) => o.shipClass == item.ship).unitCost
      : _hw.catalog.defences.firstWhere((o) => o.kind == item.defence).unitCost;

  int _unitTicksOf(ShipyardItem item) => item.ship != null
      ? _hw.catalog.ships.firstWhere((o) => o.shipClass == item.ship).ticksPerShip
      : _hw.catalog.defences.firstWhere((o) => o.kind == item.defence).ticksPerUnit;

  /// "200 metal, 50 crystal and 30 deuterium", as the server's `amount_words`.
  String _amount(Resources r) {
    final parts = [
      if (r.metal > 0) '${r.metal.ceil()} metal',
      if (r.crystal > 0) '${r.crystal.ceil()} crystal',
      if (r.deuterium > 0) '${r.deuterium.ceil()} deuterium',
    ];
    return switch (parts) {
      [] => 'nothing',
      [final one] => one,
      [...final first, final last] => '${first.join(', ')} and $last',
    };
  }

  /// A batch is accepted only if the stockpile covers every unit now, and
  /// it pays then; a paid batch waits only for the yard.
  List<ServerMessage> _orderYard(ShipyardItem item, int count) {
    if ((_hw.shipyardQueue == null ? 0 : 1) + _hw.shipyardPending.length >= 1 + _hw.queueWaitingMax) {
      return [_err(ErrorCode.queueFull, 'The shipyard queue is full (1 running + ${_hw.queueWaitingMax} waiting)')];
    }
    final unit = _unitCostOf(item);
    final cost = _times(unit, count);
    final label = '${item.label} x$count';
    final short = _missing(cost);
    if (short != null) {
      return [
        _err(ErrorCode.insufficientResources,
            '$label costs ${_amount(cost)}; the stockpile is short by ${_amount(short)}. Nothing was ordered: a batch is paid in full when it is ordered.'),
      ];
    }
    _pay(cost);
    final id = _newQueueId();
    if (_hw.shipyardQueue == null) {
      _hw = _hwWith(ship: ShipyardQueueItem(id: id, item: item, count: count, built: 0, startTick: _tick, endTick: _tick + _unitTicksOf(item)));
      _ev(QueueEvent(playerId: _playerId, queueType: QueueType.ship, id: id, item: label, action: QueueAction.started, paid: cost));
    } else {
      _hw = _hwWith(shipPending: [
        ..._hw.shipyardPending,
        QueuedShip(id: id, item: item, count: count, cost: cost, unitCost: unit, ticks: _unitTicksOf(item), waitingOn: BatchWait.slot),
      ]);
      _ev(QueueEvent(playerId: _playerId, queueType: QueueType.ship, id: id, item: label, action: QueueAction.waiting, paid: cost, waitingOn: WaitReason.slot));
    }
    return [_tickUpdate()];
  }

  void _announceWait(int id, QueueType type, String item, Resources? short, WaitReason on) {
    _ev(QueueEvent(playerId: _playerId, queueType: type, id: id, item: item, action: QueueAction.waiting, waitingFor: short, waitingOn: on));
  }

  int _bl(BuildingType t) => _hw.buildings.firstWhere((b) => b.buildingType == t).level;
  int _rl(ResearchType t) => _hw.research.firstWhere((r) => r.tech == t).level;

  int _projectedBuilding(BuildingType t) => [
        _bl(t),
        for (final q in _hw.buildQueue) if (q.buildingType == t) q.targetLevel,
        for (final q in _hw.buildPending) if (q.buildingType == t) q.targetLevel,
      ].reduce(max);

  int _projectedResearch(ResearchType t) => [
        _rl(t),
        if (_hw.researchActive?.tech == t) _hw.researchActive!.targetLevel,
        for (final q in _hw.researchPending) if (q.tech == t) q.targetLevel,
      ].reduce(max);

  /// The level of the building or the research the catalogue calls [name]:
  /// what stands, or with [projected] what stands once the queues are done.
  int _levelNamed(String name, {required bool projected}) {
    for (final t in BuildingType.values) {
      if (t.label == name) return projected ? _projectedBuilding(t) : _bl(t);
    }
    for (final t in ResearchType.values) {
      if (t.label == name) return projected ? _projectedResearch(t) : _rl(t);
    }
    throw StateError('the catalogue requires "$name", which is neither a building nor a research');
  }

  /// "Cruiser needs Shipyard level 5 (you have 2) and Ion Technology level
  /// 2 (you have 0)": the server's sentence for what [subject] still lacks
  /// of [requires], as they stand in the catalogue.
  static String _needs(String subject, List<Requirement> requires) {
    final parts = [
      for (final r in requires)
        if (!r.met) r.energy ? '${r.need} energy made (you make ${r.have})' : '${r.name} level ${r.need} (you have ${r.have})',
    ];
    return '$subject needs ${parts.length < 2 ? parts.join() : '${parts.sublist(0, parts.length - 1).join(', ')} and ${parts.last}'}';
  }

  /// Whether what the catalogue lists in [requires] is in place.
  bool _met(List<Requirement> requires, {bool projected = false}) =>
      requires.every((r) => r.energy ? _hw.energy.made >= r.need : _levelNamed(r.name, projected: projected) >= r.need);

  BuildingOption _buildingOption(BuildingType t) => _hw.catalog.buildings[t.index];
  ResearchOption _researchOption(ResearchType t) => _hw.catalog.research[t.index];

  bool _buildingReady(BuildingType t, int target) =>
      _hw.buildQueue.every((q) => q.buildingType != t) && _bl(t) + 1 == target && _met(_buildingOption(t).requires);

  bool _researchReady(ResearchType t, int target) => _rl(t) + 1 == target && _met(_researchOption(t).requires);

  WaitReason _waitOn({required bool ready, required Resources cost, required bool slotFree}) {
    if (!ready) return WaitReason.prerequisite;
    if (!_affordable(cost)) return WaitReason.resources;
    if (!slotFree) return WaitReason.slot;
    return WaitReason.order;
  }

  bool _affordable(Resources c) =>
      _resources.metal >= c.metal && _resources.crystal >= c.crystal && _resources.deuterium >= c.deuterium;

  Resources? _missing(Resources c) {
    final m = Resources(
      metal: max(0, c.metal - _resources.metal),
      crystal: max(0, c.crystal - _resources.crystal),
      deuterium: max(0, c.deuterium - _resources.deuterium),
    );
    return m.total > 0 ? m : null;
  }

  /// What [factory] divides a time by at [level], as its row says.
  static double _speedAt(int level, BuildingType factory, EffectKind kind) =>
      _effect(_book.byLevel[level].buildings[factory.index].effects, kind).now;

  /// A time worked out at the speed [was], at the speed [now] instead.
  static int _carried(int ticks, double was, double now) => max(1, (ticks * was / now).round());

  /// What level [level] of [t] costs and how long it takes with a Robotics
  /// Factory at [robotics]: the step out of the row one level below.
  static (Resources, int) _buildingStepAt(BuildingType t, int level, int robotics) {
    final step = _book.byLevel[level - 1].buildings[t.index].next!;
    const factory = BuildingType.roboticsFactory, kind = EffectKind.buildingSpeed;
    return (step.cost, _carried(step.ticks, _speedAt(level - 1, factory, kind), _speedAt(robotics, factory, kind)));
  }

  /// The same for a research, with a Research Lab at [lab].
  static (Resources, int) _researchStepAt(ResearchType t, int level, int lab) {
    final step = _book.byLevel[level - 1].research[t.index].next!;
    const factory = BuildingType.researchLab, kind = EffectKind.researchSpeed;
    return (step.cost, _carried(step.ticks, _speedAt(level - 1, factory, kind), _speedAt(lab, factory, kind)));
  }

  (Resources, int) _buildingStep(BuildingType t, int level) => _buildingStepAt(t, level, _bl(BuildingType.roboticsFactory));
  (Resources, int) _researchStep(ResearchType t, int level) => _researchStepAt(t, level, _bl(BuildingType.researchLab));

  void _startBuilding(int id, BuildingType t, int target) {
    final (cost, ticks) = _buildingStep(t, target);
    _pay(cost);
    _hw = _hwWith(buildQueue: [
      ..._hw.buildQueue,
      BuildQueueItem(id: id, buildingType: t, targetLevel: target, startTick: _tick, endTick: _tick + ticks),
    ]);
    _ev(QueueEvent(playerId: _playerId, queueType: QueueType.building, id: id, item: '${t.label} Lv.$target', action: QueueAction.started, paid: cost));
  }

  void _startResearch(int id, ResearchType t, int target) {
    final (cost, ticks) = _researchStep(t, target);
    _pay(cost);
    _hw = _hwWith(researchItem: ResearchItem(id: id, tech: t, targetLevel: target, startTick: _tick, endTick: _tick + ticks));
    _ev(QueueEvent(playerId: _playerId, queueType: QueueType.research, id: id, item: '${t.label} Lv.$target', action: QueueAction.started, paid: cost));
  }

  /// A free slot starts the earliest waiting order that can start now. An
  /// order that cannot keeps its place, unless it reserves: then nothing
  /// behind it starts while it is ready and short.
  int _firstStartable<T>(List<T> orders, bool Function(T) ready, bool Function(T) affordable, bool Function(T) reserve) {
    for (var i = 0; i < orders.length; i++) {
      if (!ready(orders[i])) continue;
      if (affordable(orders[i])) return i;
      if (reserve(orders[i])) return -1;
    }
    return -1;
  }

  void _startWaiting() {
    var again = true;
    while (again) {
      again = false;
      if (_hw.buildQueue.length >= _hw.buildSlots) break;
      final bi = _firstStartable<QueuedBuild>(
          _hw.buildPending, (q) => _buildingReady(q.buildingType, q.targetLevel), (q) => _affordable(q.cost), (q) => q.reserve);
      if (bi >= 0) {
        final q = _hw.buildPending[bi];
        _hw = _hwWith(buildPending: [for (var i = 0; i < _hw.buildPending.length; i++) if (i != bi) _hw.buildPending[i]]);
        _startBuilding(q.id, q.buildingType, q.targetLevel);
        again = true;
      }
    }
    if (_hw.researchActive == null) {
      final ri = _firstStartable<QueuedResearch>(
          _hw.researchPending, (q) => _researchReady(q.tech, q.targetLevel), (q) => _affordable(q.cost), (q) => q.reserve);
      if (ri >= 0) {
        final q = _hw.researchPending[ri];
        _hw = _hwWith(researchPending: [for (var i = 0; i < _hw.researchPending.length; i++) if (i != ri) _hw.researchPending[i]]);
        _startResearch(q.id, q.tech, q.targetLevel);
      }
    }
    if (_hw.shipyardQueue == null && _hw.shipyardPending.isNotEmpty) {
      final q = _hw.shipyardPending.first;
      _hw = _hwWith(
        shipPending: _hw.shipyardPending.sublist(1),
        ship: ShipyardQueueItem(id: q.id, item: q.item, count: q.count, built: 0, startTick: _tick, endTick: _tick + q.ticks),
      );
      _ev(QueueEvent(playerId: _playerId, queueType: QueueType.ship, id: q.id, item: '${q.item.label} x${q.count}', action: QueueAction.started));
    }
    _refreshWaits();
  }

  /// Bring every waiting order's shortfall and reason up to date.
  void _refreshWaits() {
    final slotFree = _hw.buildQueue.length < _hw.buildSlots;
    // A reserving order that is ready and short holds back everything behind it.
    WaitReason on<T>(List<T> line, int i, bool ready, Resources cost, bool slotFree, bool Function(T) reserve) {
      final base = _waitOn(ready: ready, cost: cost, slotFree: slotFree);
      if (base != WaitReason.slot && base != WaitReason.order) return base;
      return line.take(i).any((e) => reserve(e)) && slotFree ? WaitReason.order : base;
    }

    _hw = _hwWith(
      buildPending: [
        for (var i = 0; i < _hw.buildPending.length; i++)
          () {
            final q = _hw.buildPending[i];
            return QueuedBuild(
              id: q.id,
              buildingType: q.buildingType,
              targetLevel: q.targetLevel,
              reserve: q.reserve,
              cost: q.cost,
              ticks: q.ticks,
              waitingFor: _missing(q.cost),
              waitingOn: on<QueuedBuild>(_hw.buildPending, i, _buildingReady(q.buildingType, q.targetLevel), q.cost, slotFree, (e) => e.reserve),
            );
          }(),
      ],
      researchPending: [
        for (var i = 0; i < _hw.researchPending.length; i++)
          () {
            final q = _hw.researchPending[i];
            return QueuedResearch(
              id: q.id,
              tech: q.tech,
              targetLevel: q.targetLevel,
              reserve: q.reserve,
              cost: q.cost,
              ticks: q.ticks,
              waitingFor: _missing(q.cost),
              waitingOn: on<QueuedResearch>(_hw.researchPending, i, _researchReady(q.tech, q.targetLevel), q.cost, _hw.researchActive == null, (e) => e.reserve),
            );
          }(),
      ],
    );
  }

  static Resources _times(Resources r, num n) =>
      Resources(metal: r.metal * n, crystal: r.crystal * n, deuterium: r.deuterium * n);

  /// The three stockpiles' limits: each resource's cap and what a raid
  /// cannot take of it are what its own storage building's row says.
  static StorageState _storage(HomeworldCatalog catalog, Resources stock, Resources production) {
    double of(ResourceKind k, EffectKind kind) => _effect(catalog.storageOf(k)!.effects, kind).now;
    Resources each(EffectKind kind) =>
        Resources(metal: of(ResourceKind.metal, kind), crystal: of(ResourceKind.crystal, kind), deuterium: of(ResourceKind.deuterium, kind));
    final cap = each(EffectKind.storageCap);
    int? eta(double have, double limit, double rate) =>
        have >= limit ? 0 : (rate > 0 ? ((limit - have) / rate).ceil() : null);
    return StorageState(
      cap: cap,
      protected: each(EffectKind.storageProtected),
      fullInS: ResourceEta(
        metal: eta(stock.metal, cap.metal, production.metal),
        crystal: eta(stock.crystal, cap.crystal, production.crystal),
        deuterium: eta(stock.deuterium, cap.deuterium, production.deuterium),
      ),
      capped: [
        if (stock.metal >= cap.metal) ResourceKind.metal,
        if (stock.crystal >= cap.crystal) ResourceKind.crystal,
        if (stock.deuterium >= cap.deuterium) ResourceKind.deuterium,
      ],
    );
  }

  /// The catalogue of an empire with these levels, row by row out of the
  /// engine's ([_Book.byLevel]): a building's and a research's at its own
  /// level, a ship's and a defence's at the Shipyard's. What a row requires
  /// is judged against these levels and [energyMade]. [defences] is, for
  /// each kind, how many stand and how many its limit counts.
  static HomeworldCatalog _catalogFor(
      List<BuildingState> buildings, List<ResearchState> research, Map<DefenceKind, (int, int)> defences, int energyMade) {
    int bl(BuildingType t) => buildings.firstWhere((b) => b.buildingType == t).level;
    int rl(ResearchType t) => research.firstWhere((r) => r.tech == t).level;
    final levels = {
      for (final t in BuildingType.values) t.label: bl(t),
      for (final t in ResearchType.values) t.label: rl(t),
    };
    List<Requirement> judged(List<Requirement> requires) => [
          for (final r in requires)
            if (r.energy)
              Requirement(name: r.name, need: r.need, have: energyMade, met: energyMade >= r.need, energy: true)
            else
              Requirement(name: r.name, need: r.need, have: levels[r.name]!, met: levels[r.name]! >= r.need, energy: false),
        ];
    final yard = _book.byLevel[bl(BuildingType.shipyard)];

    return HomeworldCatalog(
      buildings: [
        for (final t in BuildingType.values)
          () {
            final row = _book.byLevel[bl(t)].buildings[t.index];
            return BuildingOption(
              buildingType: t,
              level: row.level,
              maxLevel: row.maxLevel,
              next: switch (row.next) {
                null => null,
                final step => UpgradeStep(level: step.level, cost: step.cost, ticks: _buildingStepAt(t, step.level, bl(BuildingType.roboticsFactory)).$2),
              },
              requires: judged(row.requires),
              effects: row.effects,
            );
          }(),
      ],
      research: [
        for (final t in ResearchType.values)
          () {
            final row = _book.byLevel[rl(t)].research[t.index];
            return ResearchOption(
              tech: t,
              level: row.level,
              maxLevel: row.maxLevel,
              next: switch (row.next) {
                null => null,
                final step => UpgradeStep(level: step.level, cost: step.cost, ticks: _researchStepAt(t, step.level, bl(BuildingType.researchLab)).$2),
              },
              requires: judged(row.requires),
              effects: row.effects,
            );
          }(),
      ],
      ships: [
        for (final o in yard.ships)
          ShipOption(
            shipClass: o.shipClass,
            unitCost: o.unitCost,
            ticksPerShip: o.ticksPerShip,
            requires: judged(o.requires),
            hull: o.hull,
            shield: o.shield,
            weapon: o.weapon,
            power: o.power,
            cargo: o.cargo,
            flies: o.flies,
            drive: o.drive,
            speed: o.speed,
            jumpTicks: o.jumpTicks,
            jumpFuel: o.jumpFuel,
            tank: o.tank,
            harvest: o.harvest,
          ),
      ],
      defences: [
        for (final o in yard.defences)
          DefenceOption(
            kind: o.kind,
            count: defences[o.kind]?.$1 ?? 0,
            max: o.max,
            room: switch (o.max) {
              null => null,
              final most => max(0, most - (defences[o.kind]?.$2 ?? 0)),
            },
            unitCost: o.unitCost,
            ticksPerUnit: o.ticksPerUnit,
            hull: o.hull,
            shield: o.shield,
            weapon: o.weapon,
            power: o.power,
            requires: judged(o.requires),
          ),
      ],
    );
  }

  /// Spend [cost] if the stockpile covers it; otherwise the error to send.
  ServerMessage? _pay(Resources cost) {
    if (_resources.metal < cost.metal ||
        _resources.crystal < cost.crystal ||
        _resources.deuterium < cost.deuterium) {
      return _err(ErrorCode.noResources, 'Not enough resources');
    }
    _resources = Resources(
      metal: _resources.metal - cost.metal,
      crystal: _resources.crystal - cost.crystal,
      deuterium: _resources.deuterium - cost.deuterium,
    );
    return null;
  }

  static const _mines = [BuildingType.metalMine, BuildingType.crystalMine, BuildingType.deuteriumSynthesizer];

  /// What the row of [t] at its level in [buildings] says of [kind].
  static double _figure(List<BuildingState> buildings, BuildingType t, EffectKind kind) =>
      _effect(_book.byLevel[buildings.firstWhere((b) => b.buildingType == t).level].buildings[t.index].effects, kind).now;

  /// The server's `home_power`: the mines need energy, the Solar Plant, the
  /// Fusion Reactor while it has deuterium to burn and the Solar Satellites
  /// make it, and short of it the mines run at the share there is.
  EnergyState _energy(List<BuildingState> buildings) {
    final needed = _mines.fold(0.0, (n, t) => n + _figure(buildings, t, EffectKind.energyUsed)).round();
    final reactor = buildings.firstWhere((b) => b.buildingType == BuildingType.fusionReactor).level;
    final starved = reactor > 0 && _resources.deuterium <= 0;
    final made = _figure(buildings, BuildingType.solarPlant, EffectKind.energyMade).round() +
        (starved ? 0 : _figure(buildings, BuildingType.fusionReactor, EffectKind.energyMade).round()) +
        _satellites * _book.satelliteEnergy;
    return EnergyState(
      made: made,
      needed: needed,
      fraction: needed == 0 ? 1 : min(1, made / needed),
      perSatellite: _book.satelliteEnergy,
      fusionBurn: reactor > 0 && !starved ? _figure(buildings, BuildingType.fusionReactor, EffectKind.fusionBurnPerTick) : 0,
      fusionStarved: starved,
    );
  }

  /// The server's `production_per_tick`: each mine at the share of its
  /// output the energy allows, the natural production, less what the
  /// Fusion Reactor burns.
  static Resources _production(List<BuildingState> buildings, EnergyState energy) {
    double natural(String name) => _book.constant(name) * _book.pace / 3600;
    double mined(BuildingType t, EffectKind kind) => _figure(buildings, t, kind) * energy.fraction;
    return Resources(
      metal: mined(BuildingType.metalMine, EffectKind.metalPerTick) + natural('NATURAL_PER_HOUR_METAL'),
      crystal: mined(BuildingType.crystalMine, EffectKind.crystalPerTick) + natural('NATURAL_PER_HOUR_CRYSTAL'),
      deuterium: mined(BuildingType.deuteriumSynthesizer, EffectKind.deuteriumPerTick) - energy.fusionBurn,
    );
  }

  // ── Commands ──────────────────────────────────────────────────

  /// Apply a client message locally; returns what the server would answer.
  List<ServerMessage> handle(ClientMessage msg) {
    switch (msg) {
      case CommandMessage(:final command):
        return _command(command);
      case PolicyUpdate(:final fleetId, :final preset, :final params):
        return _policy(fleetId, preset, params ?? const PolicyParams());
      case RequestFullStateMessage():
        return [initial()];
      case AuthRequest():
        return const [];
    }
  }

  void _stopActivities(FleetState f) {
    _recall.remove(f.id);
    _explores.remove(f.id);
    _harvest.remove(f.id);
    final c = _combats.remove(f.id);
    if (c != null) {
      _ev(CombatEndedEvent(
          sector: c.sector,
          playerVictory: false,
          outcome: BattleOutcome.withdrawn,
          rounds: c.round,
          losses: _losses(c, fleetGone: false, groupGone: false),
          owners: const ['Demo'],
          mine: true));
      _groups[c.npc.id]!.rested = 0;
      _settle(c.npc.id);
    }
  }

  List<ServerMessage> _command(Command c) {
    switch (c) {
      case MoveCommand(:final fleetId, :final target):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final here = _base(f.location);
        if (!here.connections.contains(target)) {
          return [_err(ErrorCode.noConnection, 'No lane from ${f.location} to $target')];
        }
        if (_combats[fleetId] case final fight?) {
          if (fight.fleeTo case final to?) {
            return [
              _err(ErrorCode.onCooldown,
                  'fleet $fleetId is already leaving the fight at ${f.location} for $to; stop calls the jump off, and its fuel is forfeit'),
            ];
          }
          final quote = f.flight!;
          if (quote.refusal case final why?) return [_err(ErrorCode.insufficientFuel, why)];
          fight
            ..fleeTo = target
            ..fleeRounds = quote.rounds;
          _setFleet(_copy(f, state: FleetStatus.moving, fuel: f.fuel - quote.fuel));
          return [_tickUpdate()];
        }
        if (_sieges[f.location.toKey()] case final siege? when f.state == FleetStatus.inCombat || siege.leaving.containsKey(fleetId)) {
          if (siege.leaving[fleetId] case (final to, _)?) {
            return [
              _err(ErrorCode.onCooldown,
                  'fleet $fleetId is already leaving the fight at ${f.location} for $to; stop calls the jump off, and its fuel is forfeit'),
            ];
          }
          final quote = f.flight!;
          if (quote.refusal case final why?) return [_err(ErrorCode.insufficientFuel, why)];
          siege.leaving[fleetId] = (target, quote.rounds);
          _setFleet(_copy(f, state: FleetStatus.moving, fuel: f.fuel - quote.fuel));
          return [_tickUpdate()];
        }
        if (f.cooldownRemaining > 0) return [_err(ErrorCode.onCooldown, 'Drive recharging: ${f.cooldownRemaining} tick(s)')];
        if (f.fuel < f.jumpFuel) return [_err(ErrorCode.insufficientFuel, 'Not enough fuel for a jump')];
        _stopActivities(f);
        _hop(f, target);
        return [_tickUpdate()];
      case HarvestCommand(:final fleetId, :final resource):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final sec = _base(f.location);
        final keys = [
          for (final k in const ['metal', 'crystal', 'deut'])
            if ((resource == HarvestResource.auto || resource.name == k || (resource == HarvestResource.deuterium && k == 'deut')) &&
                _density(sec, k) != Density.none)
              k
        ];
        if (_combats.containsKey(fleetId)) return [_err(ErrorCode.onCooldown, 'fleet $fleetId is in combat')];
        if (f.location == _home || keys.isEmpty) return [_err(ErrorCode.resourceNotPresent, 'No ${resource == HarvestResource.auto ? 'ore' : resource.name} to harvest here')];
        if (f.cargo.total >= f.cargoCapacity) return [_err(ErrorCode.cargoFull, 'Cargo hold is full')];
        _stopActivities(f);
        _harvest[fleetId] = resource;
        _setFleet(_copy(f, state: FleetStatus.harvesting));
        return [_tickUpdate()];
      case RecallCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final path = _pathHome(f.location);
        if (path == null) return [_err(ErrorCode.noConnection, 'No known lane home')];
        _stopActivities(f);
        if (path.isEmpty) {
          _setFleet(_copy(f, state: FleetStatus.docked));
        } else {
          _recall[fleetId] = path;
          _setFleet(_copy(f, state: FleetStatus.returning));
        }
        return [_tickUpdate()];
      case StopCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        if (_combats[fleetId] case final fight?) {
          final to = fight.fleeTo;
          if (to == null) return [_err(ErrorCode.onCooldown, 'fleet $fleetId is in combat')];
          fight
            ..fleeTo = null
            ..fleeRounds = 0;
          _setFleet(_copy(f, state: FleetStatus.inCombat));
          _ev(AlertEvent(
              level: AlertLevel.warning,
              message: 'fleet $fleetId called off its jump to $to: the ${_tenths(f.jumpFuel * fleeFuelMultiplier)} fuel it burned for the jump is forfeit, '
                  'and it fights on at ${f.location}',
              fleetId: fleetId,
              sector: f.location));
          return [_tickUpdate()];
        }
        if (_sieges[f.location.toKey()] case final siege? when f.state == FleetStatus.inCombat || siege.leaving.containsKey(fleetId)) {
          final bound = siege.leaving.remove(fleetId);
          if (bound == null) return [_err(ErrorCode.onCooldown, 'fleet $fleetId is in combat')];
          _setFleet(_copy(f, state: FleetStatus.inCombat));
          _ev(AlertEvent(
              level: AlertLevel.warning,
              message: 'fleet $fleetId called off its jump to ${bound.$1}: the ${_tenths(f.jumpFuel * fleeFuelMultiplier)} fuel it burned for the jump '
                  'is forfeit, and it fights on at ${f.location}',
              fleetId: fleetId,
              sector: f.location));
          return [_tickUpdate()];
        }
        _stopActivities(f);
        _setFleet(_copy(f, state: f.location == _home ? FleetStatus.docked : FleetStatus.idle));
        return [_tickUpdate()];
      case ScanCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        if (f.cooldownRemaining > 0) return [_err(ErrorCode.onCooldown, 'Scanner recharging: ${f.cooldownRemaining} tick(s)')];
        final range = f.scanRange;
        final fresh = _reveal(f.location, range, scan: true);
        var hostile = 0;
        for (final h in hexSpiral(f.location, range)) {
          _liveUntil[h.toKey()] = _tick + 120;
          hostile += (_hostiles[h.toKey()]?.length ?? 0);
        }
        _liveNow = _computeLive();
        for (final k in _liveNow) {
          _seen[k] = _tick;
        }
        final signals = <SignalContact>[];
        final far = [...hexRing(f.location, range + 1), ...hexRing(f.location, range + 2)]
            .where((h) => !_seen.containsKey(h.toKey()) && h.distFromOrigin <= _hubRadius)
            .toList()
          ..sort((a, b) => _hash(a.q, a.r, _tick ~/ 5).compareTo(_hash(b.q, b.r, _tick ~/ 5)));
        for (final h in far) {
          if (signals.length >= 3) break;
          final k = h.toKey();
          final b = _base(h);
          final kind = (_hostiles[k]?.isNotEmpty ?? false)
              ? SignalKind.hostileMass
              : _sites.containsKey(k)
                  ? SignalKind.derelict
                  : b.terrain == TerrainType.anomaly
                      ? SignalKind.anomaly
                      : (b.resources.metal.index + b.resources.crystal.index + b.resources.deuterium.index) >= 6
                          ? SignalKind.richOre
                          : null;
          if (kind != null) {
            final hs = _hostiles[k];
            final rating = hs == null || hs.isEmpty ? b.threat.rating : _observedThreat(hs).rating;
            signals.add(SignalContact(sector: h, signal: kind, threatBand: (rating + 2) ~/ 3));
          }
        }
        _setFleet(_copy(f, cooldown: scanCooldown));
        _ev(ScanCompletedEvent(
          fleetId: fleetId,
          sector: f.location,
          sectorsRevealed: fresh,
          hostilesDetected: hostile,
          threats: [for (final h in hexSpiral(f.location, range)) SectorThreat(sector: h, threat: _sector(h).threat)],
          signals: signals,
        ));
        return [_tickUpdate()];
      case AttackCommand(:final fleetId, :final targetFleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        if (f.state == FleetStatus.docked) return [_err(ErrorCode.invalidTarget, 'Docked fleets cannot attack')];
        if (_combats.containsKey(fleetId)) return [_err(ErrorCode.invalidCommand, 'Already in combat')];
        final hs = _hostiles[f.location.toKey()] ?? const <NpcFleetInfo>[];
        final npc = hs.where((h) => targetFleetId == 0 || h.id == targetFleetId).firstOrNull;
        if (npc == null) return [_err(ErrorCode.invalidTarget, 'No such hostile in this sector')];
        _stopActivities(f);
        _startCombat(_find(fleetId)!, npc);
        return [_tickUpdate()];
      case CollectSalvageCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final k = f.location.toKey();
        final pile = _salvage[k];
        if (pile == null) return [_err(ErrorCode.invalidTarget, 'No salvage in this sector')];
        final free = f.cargoCapacity - f.cargo.total;
        if (free <= 0) return [_err(ErrorCode.cargoFull, 'Cargo hold is full')];
        final res = pile.$1;
        final share = min(1.0, free / max(1, res.total));
        final take = Resources(metal: (res.metal * share).floorToDouble(), crystal: (res.crystal * share).floorToDouble(), deuterium: (res.deuterium * share).floorToDouble());
        final left = Resources(metal: res.metal - take.metal, crystal: res.crystal - take.crystal, deuterium: res.deuterium - take.deuterium);
        if (left.total <= 0) {
          _salvage.remove(k);
        } else {
          _salvage[k] = (left, pile.$2);
        }
        _dirty.add(k);
        _setFleet(_copy(f, cargo: Resources(metal: f.cargo.metal + take.metal, crystal: f.cargo.crystal + take.crystal, deuterium: f.cargo.deuterium + take.deuterium)));
        _ev(SalvageCollectedEvent(fleetId: fleetId, sector: f.location, resources: take, remaining: left.total > 0 ? left : null));
        return [_tickUpdate()];
      case ExploreSiteCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final k = f.location.toKey();
        final site = _sites[k];
        if (site == null) return [_err(ErrorCode.invalidTarget, 'no derelict to board in this sector')];
        if (f.state == FleetStatus.inCombat) return [_err(ErrorCode.onCooldown, 'fleet $fleetId is in combat')];
        if (f.cooldownRemaining > 0) {
          return [_err(ErrorCode.onCooldown, 'fleet $fleetId is on cooldown for ${f.cooldownRemaining} more ticks')];
        }
        _stopActivities(f);
        final end = _tick + exploreDurationTicks;
        _explores[fleetId] = _Explore(fleetId, f.location, end);
        _setFleet(_copy(f, state: FleetStatus.exploring, cooldown: exploreDurationTicks));
        _ev(SiteExplorationStartedEvent(fleetId: fleetId, sector: f.location, tier: site.tier, endTick: end));
        return [_tickUpdate()];
      case BuildCommand(:final buildingType, :final reserve):
        final proj = _projectedBuilding(buildingType);
        final option = _buildingOption(buildingType);
        if (proj >= option.maxLevel) return [_err(ErrorCode.maxLevelReached, '${buildingType.label} is maxed')];
        if (!_met(option.requires, projected: true)) {
          return [_err(ErrorCode.prerequisitesNotMet, _needs(buildingType.label, option.requires))];
        }
        if (_hw.buildQueue.length + _hw.buildPending.length >= _hw.buildSlots + _hw.queueWaitingMax) {
          return [_err(ErrorCode.queueFull, 'The building queue is full (${_hw.buildSlots} running + ${_hw.queueWaitingMax} waiting)')];
        }
        final (cost, ticks) = _buildingStep(buildingType, proj + 1);
        final id = _newQueueId();
        _hw = _hwWith(buildPending: [
          ..._hw.buildPending,
          QueuedBuild(
              id: id, buildingType: buildingType, targetLevel: proj + 1, reserve: reserve, cost: cost, ticks: ticks, waitingFor: _missing(cost), waitingOn: WaitReason.slot),
        ]);
        _startWaiting();
        final waiting = _hw.buildPending.where((q) => q.id == id).firstOrNull;
        if (waiting != null) _announceWait(id, QueueType.building, '${buildingType.label} Lv.${proj + 1}', waiting.waitingFor, waiting.waitingOn);
        return [_tickUpdate()];
      case ResearchCommand(:final tech, :final reserve):
        if (_bl(BuildingType.researchLab) == 0) return [_err(ErrorCode.noResearchLab, 'Build a research lab first')];
        final option = _researchOption(tech);
        final proj = _projectedResearch(tech);
        if (proj >= option.maxLevel) return [_err(ErrorCode.maxLevelReached, '${tech.label} is maxed')];
        if (option.requires.any((r) => !r.met)) {
          return [_err(ErrorCode.prerequisitesNotMet, _needs(tech.label, option.requires))];
        }
        if ((_hw.researchActive == null ? 0 : 1) + _hw.researchPending.length >= 1 + _hw.queueWaitingMax) {
          return [_err(ErrorCode.queueFull, 'The research queue is full (1 running + ${_hw.queueWaitingMax} waiting)')];
        }
        final (cost, ticks) = _researchStep(tech, proj + 1);
        final id = _newQueueId();
        _hw = _hwWith(researchPending: [
          ..._hw.researchPending,
          QueuedResearch(id: id, tech: tech, targetLevel: proj + 1, reserve: reserve, cost: cost, ticks: ticks, waitingFor: _missing(cost), waitingOn: WaitReason.slot),
        ]);
        _startWaiting();
        final waiting = _hw.researchPending.where((q) => q.id == id).firstOrNull;
        if (waiting != null) _announceWait(id, QueueType.research, '${tech.label} Lv.${proj + 1}', waiting.waitingFor, waiting.waitingOn);
        return [_tickUpdate()];
      case BuildShipCommand(:final shipClass, :final count):
        final o = _hw.catalog.ships.firstWhere((o) => o.shipClass == shipClass);
        if (_bl(BuildingType.shipyard) == 0) return [_err(ErrorCode.noShipyard, 'Build a shipyard first')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.shipLocked, _needs(shipClass.label, o.requires))];
        return _orderYard(ShipyardItem.ship(shipClass), count);
      case BuildDefenceCommand(:final kind, :final count):
        final o = _hw.catalog.defences.firstWhere((o) => o.kind == kind);
        if (_bl(BuildingType.shipyard) == 0) return [_err(ErrorCode.noShipyard, 'Build a shipyard first')];
        if (o.requires.any((r) => !r.met)) return [_err(ErrorCode.defenceLocked, _needs(kind.label, o.requires))];
        if (o.room case final room? when count > room) {
          return [_err(ErrorCode.maxLevelReached, 'no more than ${o.max} ${kind.label} may stand; ${o.max! - room} stand or are on order')];
        }
        return _orderYard(ShipyardItem.defence(kind), count);
      case CancelBuildCommand(:final queueType, :final id):
        late final String label;
        late final Resources paid;
        switch (queueType) {
          case QueueType.building:
            final gone = _hw.buildQueue.where((q) => q.id == id).firstOrNull;
            if (gone == null) return [_err(ErrorCode.invalidTarget, 'No order $id is under way in the building queue')];
            label = '${gone.buildingType.label} Lv.${gone.targetLevel}';
            paid = _buildingStep(gone.buildingType, gone.targetLevel).$1;
            _hw = _hwWith(
              buildQueue: [for (final q in _hw.buildQueue) if (q.id != id) q],
              buildPending: [
                for (final q in _hw.buildPending)
                  q.buildingType == gone.buildingType && q.targetLevel > gone.targetLevel
                      ? QueuedBuild(
                          id: q.id,
                          buildingType: q.buildingType,
                          targetLevel: q.targetLevel - 1,
                          reserve: q.reserve,
                          cost: q.cost,
                          ticks: q.ticks,
                          waitingFor: q.waitingFor,
                          waitingOn: WaitReason.slot)
                      : q,
              ],
            );
          case QueueType.ship:
            final gone = _hw.shipyardQueue;
            if (gone == null || gone.id != id) return [_err(ErrorCode.invalidTarget, 'No order $id is under way in the shipyard queue')];
            label = gone.built > 0 ? '${gone.item.label} x${gone.count} (${gone.built} already built and kept)' : '${gone.item.label} x${gone.count}';
            paid = _times(_unitCostOf(gone.item), gone.count - gone.built);
            _hw = _hwWith(clearShip: true);
          case QueueType.research:
            final gone = _hw.researchActive;
            if (gone == null || gone.id != id) return [_err(ErrorCode.invalidTarget, 'No order $id is under way in the research queue')];
            label = '${gone.tech.label} Lv.${gone.targetLevel}';
            paid = _researchStep(gone.tech, gone.targetLevel).$1;
            _hw = _hwWith(clearResearch: true, researchPending: [
              for (final q in _hw.researchPending)
                q.tech == gone.tech && q.targetLevel > gone.targetLevel
                    ? QueuedResearch(
                        id: q.id,
                        tech: q.tech,
                        targetLevel: q.targetLevel - 1,
                        reserve: q.reserve,
                        cost: q.cost,
                        ticks: q.ticks,
                        waitingFor: q.waitingFor,
                        waitingOn: WaitReason.slot)
                    : q,
            ]);
        }
        final refund = _times(paid, 0.5);
        _resources = _clampCaps(Resources(
          metal: _resources.metal + refund.metal,
          crystal: _resources.crystal + refund.crystal,
          deuterium: _resources.deuterium + refund.deuterium,
        ));
        _ev(QueueEvent(playerId: _playerId, queueType: queueType, id: id, item: label, action: QueueAction.cancelled, refunded: refund));
        _startWaiting();
        return [_tickUpdate()];
      case CancelQueuedCommand(:final queueType, :final id):
        late final String label;
        var refund = const Resources();
        switch (queueType) {
          case QueueType.building when _hw.buildPending.any((q) => q.id == id):
            final q = _hw.buildPending.firstWhere((q) => q.id == id);
            label = '${q.buildingType.label} Lv.${q.targetLevel}';
            _hw = _hwWith(buildPending: [
              for (final e in _hw.buildPending)
                if (e.id != id) e,
            ]);
          case QueueType.ship when _hw.shipyardPending.any((q) => q.id == id):
            final q = _hw.shipyardPending.firstWhere((q) => q.id == id);
            label = '${q.item.label} x${q.count}';
            refund = _times(q.cost, 0.5);
            _hw = _hwWith(shipPending: [
              for (final e in _hw.shipyardPending)
                if (e.id != id) e,
            ]);
          case QueueType.research when _hw.researchPending.any((q) => q.id == id):
            final q = _hw.researchPending.firstWhere((q) => q.id == id);
            label = '${q.tech.label} Lv.${q.targetLevel}';
            _hw = _hwWith(researchPending: [
              for (final e in _hw.researchPending)
                if (e.id != id) e,
            ]);
          default:
            return [_err(ErrorCode.invalidTarget, 'No order $id is waiting in the ${queueType.name} queue')];
        }
        _resources = _clampCaps(Resources(
          metal: _resources.metal + refund.metal,
          crystal: _resources.crystal + refund.crystal,
          deuterium: _resources.deuterium + refund.deuterium,
        ));
        _ev(QueueEvent(playerId: _playerId, queueType: queueType, id: id, item: label, action: QueueAction.cancelled, refunded: refund));
        _startWaiting();
        return [_tickUpdate()];
      case SplitCommand(:final fleetId, :final ships, :final damagedFirst):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'fleet $fleetId not found (or it has no ships left)')];
        if (_combats.containsKey(fleetId)) return [_err(ErrorCode.onCooldown, 'fleet $fleetId is in combat')];
        if (_splitRefusal(f, ships) case final why?) return [_err(ErrorCode.invalidTarget, why)];
        if (_fleets.length >= 8) return [_err(ErrorCode.fleetLimitReached, 'Fleet limit reached')];
        final rest = _squads[fleetId]!;
        final (tank, hold) = (f.fuelMax, f.cargoCapacity);
        final taken = [for (final order in ships) _take(rest.firstWhere((s) => s.cls == order.shipClass), order.count, damagedFirst: damagedFirst)];
        rest.removeWhere((s) => s.count == 0);
        final tankShare = tank > 0 ? _tank(taken) / tank : 0.0, holdShare = hold > 0 ? _hold(taken) / hold : 0.0;
        final newId = 1 + _fleets.map((e) => e.id).reduce(max);
        _fleets = [
          for (final e in _fleets)
            e.id == fleetId ? _copy(e, fuel: e.fuel * (1 - tankShare), cargo: _times(e.cargo, 1 - holdShare)) : e,
          _fleet(newId, f.location, FleetStatus.idle, taken, fuel: f.fuel * tankShare, cargo: _times(f.cargo, holdShare), refit: _refits[fleetId]),
        ];
        _pending.add(GameEvent(
          tick: _tick,
          kind: AlertEvent(
            level: AlertLevel.info,
            message: 'fleet $fleetId split: ${_shipsIn(taken)} ship(s) now fly as fleet $newId',
            fleetId: newId,
            sector: f.location,
          ),
        ));
        return [_tickUpdate()];
      case MergeCommand(:final fleetId, :final otherFleetId):
        final a = _find(fleetId);
        final b = _find(otherFleetId);
        if (a == null || b == null) return [_err(ErrorCode.fleetNotFound, 'fleet ${a == null ? fleetId : otherFleetId} not found (or it has no ships left)')];
        if (a.id == b.id) return [_err(ErrorCode.invalidTarget, 'a fleet cannot merge with itself')];
        if (a.location != b.location) return [_err(ErrorCode.notInSector, 'fleets $fleetId and $otherFleetId are not in the same sector')];
        if (a.shipCount + b.shipCount > maxFleetShips) return [_err(ErrorCode.invalidCommand, 'the merged fleet would exceed $maxFleetShips ships')];
        final (ra, rb) = (_refits[fleetId]!, _refits[otherFleetId]!);
        final refit = Refit(weapons: min(ra.weapons, rb.weapons), shielding: min(ra.shielding, rb.shielding), armour: min(ra.armour, rb.armour));
        final joining = _squads.remove(otherFleetId)!;
        _rearm(_squads[fleetId]!, ra, refit);
        _rearm(joining, rb, refit);
        _absorb(_squads[fleetId]!, joining);
        _refits
          ..remove(otherFleetId)
          ..[fleetId] = refit;
        _fleets = [
          for (final e in _fleets)
            if (e.id == fleetId) _copy(a, cargo: _plus(a.cargo, b.cargo), fuel: a.fuel + b.fuel) else if (e.id != otherFleetId) e,
        ];
        final lowered = (ra != refit || rb != refit) && a.location != _home;
        _pending.add(GameEvent(
          tick: _tick,
          kind: AlertEvent(
            level: AlertLevel.info,
            message: 'fleet $otherFleetId merged into fleet $fleetId: ${a.shipCount + b.shipCount} ship(s)'
                '${lowered ? ", fighting at the lower of the two fleets' levels (Weapons ${refit.weapons}, Shielding ${refit.shielding}, "
                    "Armour ${refit.armour}) until the home dock refits them" : ''}'
                '${b.policy != null ? "; the absorbed fleet's standing orders ended" : ''}',
            fleetId: fleetId,
            sector: a.location,
          ),
        ));
        return [_tickUpdate()];
      case ExchangeCommand(:final fleetId, :final give, :final want, :final amount):
        return _exchange(fleetId, give, want, amount, quote: false);
      case ExchangeQuoteCommand(:final fleetId, :final give, :final want, :final amount):
        return _exchange(fleetId, give, want, amount, quote: true);
      case FoundOutpostCommand(:final fleetId):
        return _found(fleetId);
      case OutpostSurveyCommand(:final fleetId, :final sector):
        return _survey(fleetId, sector);
      case UpgradeOutpostCommand():
        return _upgradeOutpost();
      case AbandonOutpostCommand():
        return _abandonOutpost();
      case LoadCommand(:final fleetId, :final resource, :final amount):
        return _load(fleetId, resource, amount);
      case RefineCommand(:final fleetId):
        return _refine(fleetId);
      case UnloadCommand(:final fleetId):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'fleet $fleetId not found (or it has no ships left)')];
        final post = _outpost;
        if (post != null && f.location == post.at) return _unloadIntoStore(f, post);
        if (!f.laden) {
          return [_err(ErrorCode.invalidCommand, 'fleet $fleetId was not loaded by hand; a docked fleet unloads by itself as the stockpile has room')];
        }
        final back = _unload(f);
        _ev(AlertEvent(
            level: AlertLevel.info, message: 'fleet $fleetId unloaded ${_amount(back)} into the stockpile', fleetId: fleetId, sector: f.location));
        return [_tickUpdate()];
      case LeaderboardCommand(:final limit):
        const rows = [
          LeaderboardEntry(rank: 1, name: 'Vega', agent: true, score: 14.2, core: 12.8, combat: 1.4, explore: 0),
          LeaderboardEntry(rank: 2, name: 'Kestrel', agent: false, score: 11.6, core: 9.4, combat: 0.9, explore: 1.3),
          LeaderboardEntry(rank: 3, name: 'Orrery', agent: true, score: 9.9, core: 8.7, combat: 0.6, explore: 0.6),
          LeaderboardEntry(rank: 4, name: 'Demo', agent: false, score: 9.6, core: 9.1, combat: 0.5, explore: 0),
          LeaderboardEntry(rank: 5, name: 'Orrin', agent: false, score: 4.1, core: 4.1, combat: 0, explore: 0),
          LeaderboardEntry(rank: 6, name: 'Pike-2', agent: true, score: 3.4, core: 3.0, combat: 0.4, explore: 0),
        ];
        return [LeaderboardReply(tick: _tick, entries: rows.take(limit).toList(), you: rows[3])];
      case PreviewMoveCommand(:final fleetId, :final target):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        final here = _base(f.location);
        if (!here.connections.contains(target)) {
          return [_err(ErrorCode.noConnection, 'No lane from ${f.location} to $target')];
        }
        final hops = Hex.distance(target, _home);
        final left = f.fuel - f.jumpFuel;
        final odds = _demoOdds(f, target);
        return [
          MovePreview(
            fleetId: fleetId,
            target: target,
            fuelCost: f.jumpFuel,
            fuelAfter: max(0, left),
            canJump: left >= 0,
            hopsHome: hops,
            chartedHopsHome: hops,
            fuelToReturn: hops * f.jumpFuel,
            canReturn: left >= hops * f.jumpFuel,
            threat: odds.threat,
            fleetPower: f.power,
            ratio: odds.ratio,
            label: odds.label,
            flight: switch (_combats[fleetId]) {
              final fight? when fight.fleeTo == null =>
                _flightQuote(fight, fuel: f.fuel, jump: f.jumpFuel, jumpTicks: f.jumpTicks, power: f.power, to: target),
              _ => null,
            },
          ),
        ];
      case OddsCommand(:final fleetId, :final sectors):
        final f = _find(fleetId);
        if (f == null) return [_err(ErrorCode.fleetNotFound, 'No such fleet')];
        return [FleetOdds(fleetId: fleetId, sectors: [for (final s in sectors) _demoOdds(f, s)])];
    }
  }

  /// The engine's `split_refusal`: why a split naming [ships] is refused
  /// for [f]; null when it is not.
  String? _splitRefusal(FleetState f, List<ShipCount> ships) {
    if (ships.isEmpty) return 'split needs at least one class and a count of its ships to take';
    for (final (i, order) in ships.indexed) {
      final label = order.shipClass.label;
      if (order.count == 0) return 'split names $label with a count of 0';
      if (ships.take(i).any((earlier) => earlier.shipClass == order.shipClass)) return 'split names $label twice; give each class one count';
      final aboard = _squads[f.id]!.where((s) => s.cls == order.shipClass).firstOrNull?.count ?? 0;
      if (order.count > aboard) return 'fleet ${f.id} has $aboard $label, not the ${order.count} the split names';
    }
    if (ships.fold(0, (n, order) => n + order.count) >= f.shipCount) {
      return 'the split would take all ${f.shipCount} ships of fleet ${f.id}; at least one ship must stay behind';
    }
    return null;
  }

  /// The demo world's stand-in for the server's odds: power against power.
  /// The real server plays the fight out; nothing outside the demo uses this.
  SectorOdds _demoOdds(FleetState f, Hex sector) {
    final threat = _sector(sector, live: _liveNow.contains(sector.toKey())).threat;
    final ratio = f.power / threat.estPower;
    return SectorOdds(sector: sector, threat: threat, ratio: ratio, label: ratioLabelFor(ratio));
  }
}
