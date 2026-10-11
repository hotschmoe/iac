import '../protocol/protocol.dart';

/// Stable queue ids of one queue: the running items, then the waiting ones.
class QueueIds {
  final List<int> running;
  final List<int> waiting;
  const QueueIds({this.running = const [], this.waiting = const []});
}

/// What the command bar needs to know to resolve a typed command.
class CommandContext {
  final int? fleetId;
  final Hex? fleetSector;
  final Map<Hex, SectorState> sectors;
  final List<FleetState> fleets;
  final Hex cursor;
  final int tick;
  final Map<QueueType, QueueIds> queues;
  const CommandContext({
    required this.fleetId,
    required this.fleetSector,
    required this.sectors,
    required this.fleets,
    required this.cursor,
    required this.tick,
    this.queues = const {},
  });

  FleetState? get fleet {
    for (final f in fleets) {
      if (f.id == fleetId) return f;
    }
    return null;
  }

  SectorState? get here => fleetSector == null ? null : sectors[fleetSector];
}

sealed class ParsedCommand {
  const ParsedCommand();
}

/// Messages to send to the server.
class ParsedSend extends ParsedCommand {
  final List<ClientMessage> messages;
  const ParsedSend(this.messages);
}

/// Client-side answer (help, status, ...).
class ParsedLocal extends ParsedCommand {
  final List<String> lines;
  const ParsedLocal(this.lines);
}

class ParsedSelectFleet extends ParsedCommand {
  final int index;
  const ParsedSelectFleet(this.index);
}

class ParsedError extends ParsedCommand {
  final String message;
  const ParsedError(this.message);
}

const helpLines = [
  'fleet:  h[arvest] [metal|crystal|deut]  a[ttack] [n]  r[ecall]  s[alvage]  v|scan  x|explore  stop',
  'move:   m[ove] <e|ne|nw|w|sw|se | 1-6 | q r>      (one hop along a known lane)',
  'base:   b[uild] <metal|crystal|deut|shipyard|lab|fuel|sensor|defense|vault|fab>',
  '        research <fuel|tanks|hulls|shields|weapons|nav|harvest|corvette|frigate|cruiser|hauler|jump|modfab>',
  '        ship <scout|corvette|frigate|cruiser|hauler> [count]   cancel <building|ship|research> [id]  [waiting]',
  '        buildings and research queue ahead of resources and pay when they start; a ship batch is paid in full when ordered',
  'fleets: f[leet] [n]  select/list   split <class> [count]  detach ships into a new fleet',
  '        merge <fleet id>  fold a fleet in this sector into the active one',
  'orders: p[olicy] <manual|prospect|mine|salvage|patrol>  standing orders: the active fleet flies itself and reports each act and each hold with its reason',
  '        prospect  scans, then charts the sectors in range',
  '        mine      strips the seams that repay their fuel and brings the ore home',
  '        salvage   scoops wreckage and boards derelicts, nearest first',
  '        patrol    hunts the hostiles it rates at its engage ratio, in range of home',
  '        manual    clears the orders (also: off); policy with no name steps to the next doctrine',
  '        settings, after the name, or alone to change the orders in force:  range=<hexes from home, 1-30>  engage=<least odds ratio, 0.1-10>',
  '                  fuel=<% of deuterium income it may burn, 100 = no bound>  cargo=<% of the hold that sends it home, 10-100>',
  '                  reserve=<% of the tank kept spare, 0-90>     e.g. policy mine range=8 fuel=40',
  'hub:    quote <give> <want> [amount|all]  ask the exchange what a trade would return, from anywhere   e.g. quote metal deut 400',
  '        trade <give> <want> [amount|all]  sell cargo from the hold, standing on the hub; no amount = all of it aboard',
  'hold:   load <metal|crystal|deut> [amount|all]  stockpile into the hold of the active fleet, idle at home with no standing orders;',
  '                  no amount = all that fits. The dock leaves a loaded hold alone until the fleet has been away',
  '        unload    put a hold loaded by hand back: what the stockpile has room for now, the rest as room opens',
  'other:  status  refresh  leaderboard [n]  help      views: 1|cc  2|ws  3|map  4|hw',
  'keys:   on the windshield 1-9 select a fleet and P opens its standing orders; ? lists every key',
];

const _dirs = {
  'e': HexDirection.east,
  'ne': HexDirection.northEast,
  'nw': HexDirection.northWest,
  'w': HexDirection.west,
  'sw': HexDirection.southWest,
  'se': HexDirection.southEast,
};

const _buildings = {
  'metal': BuildingType.metalMine,
  'mine': BuildingType.metalMine,
  'crystal': BuildingType.crystalMine,
  'deut': BuildingType.deuteriumSynthesizer,
  'deuterium': BuildingType.deuteriumSynthesizer,
  'shipyard': BuildingType.shipyard,
  'yard': BuildingType.shipyard,
  'lab': BuildingType.researchLab,
  'fuel': BuildingType.fuelDepot,
  'depot': BuildingType.fuelDepot,
  'sensor': BuildingType.sensorArray,
  'defense': BuildingType.defenseGrid,
  'grid': BuildingType.defenseGrid,
  'vault': BuildingType.storageVault,
  'storage': BuildingType.storageVault,
  'fab': BuildingType.fabricator,
  'fabricator': BuildingType.fabricator,
};

const _techs = {
  'fuel': ResearchType.fuelEfficiency,
  'efficiency': ResearchType.fuelEfficiency,
  'tanks': ResearchType.extendedFuelTanks,
  'hull': ResearchType.reinforcedHulls,
  'hulls': ResearchType.reinforcedHulls,
  'shield': ResearchType.advancedShields,
  'shields': ResearchType.advancedShields,
  'weapons': ResearchType.weaponsResearch,
  'nav': ResearchType.navigation,
  'navigation': ResearchType.navigation,
  'harvest': ResearchType.harvestingEfficiency,
  'corvette': ResearchType.corvetteTech,
  'frigate': ResearchType.frigateTech,
  'cruiser': ResearchType.cruiserTech,
  'hauler': ResearchType.haulerTech,
  'jump': ResearchType.emergencyJump,
  'modfab': ResearchType.modularFabrication,
  'slots': ResearchType.modularFabrication,
};

const _resources = {
  'metal': ResourceKind.metal,
  'm': ResourceKind.metal,
  'fe': ResourceKind.metal,
  'crystal': ResourceKind.crystal,
  'c': ResourceKind.crystal,
  'cr': ResourceKind.crystal,
  'deut': ResourceKind.deuterium,
  'deuterium': ResourceKind.deuterium,
  'd': ResourceKind.deuterium,
  'de': ResourceKind.deuterium,
};

const _tradeUsage = '<metal|crystal|deut> <metal|crystal|deut> [amount|all]  (give, want; no amount = all the hold carries)';

const _loadUsage = 'load: <metal|crystal|deut> [amount|all]  (stockpile into the hold of a fleet docked at home; no amount = all that fits)';

const _policies = {
  'manual': PolicyPreset.manual,
  'off': PolicyPreset.manual,
  'prospect': PolicyPreset.prospect,
  'mine': PolicyPreset.mineAndReturn,
  'mine+return': PolicyPreset.mineAndReturn,
  'salvage': PolicyPreset.salvageAndSites,
  'salvage+sites': PolicyPreset.salvageAndSites,
  'patrol': PolicyPreset.patrolHome,
};

ParsedCommand parseCommand(String input, CommandContext ctx) {
  final words = input.trim().toLowerCase().split(RegExp(r'[\s,]+')).where((w) => w.isNotEmpty).toList();
  if (words.isEmpty) return const ParsedLocal([]);
  final verb = words.first;
  final args = words.sublist(1);
  final fid = ctx.fleetId;

  ParsedCommand needFleet(Command Function(int id) build) =>
      fid == null ? const ParsedError('No fleet selected (you have none)') : ParsedSend([CommandMessage(build(fid))]);

  switch (verb) {
    case 'help' || '?':
      return const ParsedLocal(helpLines);

    case 'status':
      final f = ctx.fleet;
      return ParsedLocal([
        'Tick ${ctx.tick} | fleets ${ctx.fleets.length}'
            '${f == null ? '' : ' | active F${f.id} ${f.state.wire} at ${f.location}, fuel ${f.fuel.round()}/${f.fuelMax.round()}, cooldown ${f.cooldownRemaining}'}',
      ]);

    case 'leaderboard' || 'lb' || 'rank' || 'score':
      final n = args.isEmpty ? 10 : int.tryParse(args.first);
      if (n == null || n < 1 || n > 100) return const ParsedError('leaderboard: how many rows, 1-100');
      return ParsedSend([CommandMessage(LeaderboardCommand(limit: n))]);

    case 'refresh' || 'sync':
      return const ParsedSend([RequestFullState()]);

    case 'f' || 'fleet' || 'fleets':
      if (args.isEmpty) {
        if (ctx.fleets.isEmpty) return const ParsedLocal(['No fleets']);
        return ParsedLocal([
          for (var i = 0; i < ctx.fleets.length; i++)
            '${ctx.fleets[i].id == fid ? '>' : ' '}${i + 1}: F${ctx.fleets[i].id} ${ctx.fleets[i].state.wire} '
                '${ctx.fleets[i].location} ships ${ctx.fleets[i].ships.length}'
                '${ctx.fleets[i].policy == null ? '' : ' [${ctx.fleets[i].policy!.preset.label}]'}',
        ]);
      }
      final n = int.tryParse(args.first);
      if (n == null || n < 1 || n > ctx.fleets.length) {
        return ParsedError('fleet: choose 1-${ctx.fleets.length}');
      }
      return ParsedSelectFleet(n - 1);

    case 'h' || 'harvest':
      final res = switch (args.isEmpty ? 'auto' : args.first) {
        'auto' => HarvestResource.auto,
        'metal' || 'm' => HarvestResource.metal,
        'crystal' || 'c' => HarvestResource.crystal,
        'deut' || 'deuterium' || 'd' => HarvestResource.deuterium,
        _ => null,
      };
      if (res == null) return const ParsedError('harvest: metal|crystal|deut|auto');
      return needFleet((id) => HarvestCommand(fleetId: id, resource: res));

    case 'a' || 'attack':
      final hostiles = ctx.here?.hostiles ?? const <NpcFleetInfo>[];
      if (hostiles.isEmpty) return const ParsedError('attack: no known hostiles in this sector');
      var target = hostiles.first;
      if (args.isNotEmpty) {
        final n = int.tryParse(args.first);
        final byId = hostiles.where((h) => h.id == n);
        if (byId.isNotEmpty) {
          target = byId.first;
        } else if (n != null && n >= 1 && n <= hostiles.length) {
          target = hostiles[n - 1];
        } else {
          return ParsedError('attack: target 1-${hostiles.length} or a hostile fleet id');
        }
      }
      return needFleet((id) => AttackCommand(fleetId: id, targetFleetId: target.id));

    case 'r' || 'recall':
      return needFleet((id) => RecallCommand(fleetId: id));
    case 's' || 'salvage':
      return needFleet((id) => CollectSalvageCommand(fleetId: id));
    case 'v' || 'scan':
      return needFleet((id) => ScanCommand(fleetId: id));
    case 'x' || 'explore':
      if (ctx.here != null && ctx.here!.site == null) {
        return const ParsedError('explore: no derelict known in this sector');
      }
      return needFleet((id) => ExploreSiteCommand(fleetId: id));
    case 'stop':
      return needFleet((id) => StopCommand(fleetId: id));

    case 'm' || 'move' || 'go':
      final from = ctx.fleetSector;
      if (fid == null || from == null) return const ParsedError('No fleet selected (you have none)');
      Hex? target;
      if (args.length == 1) {
        final d = _dirs[args.first];
        final n = int.tryParse(args.first);
        if (d != null) {
          target = from.neighbor(d);
        } else if (n != null && n >= 1 && n <= 6) {
          target = from.neighbor(HexDirection.values[n - 1]);
        }
      } else if (args.length == 2) {
        final q = int.tryParse(args[0]);
        final r = int.tryParse(args[1]);
        if (q != null && r != null) target = Hex(q, r);
      }
      if (target == null) return const ParsedError('move: <e|ne|nw|w|sw|se>, <1-6> or <q r>');
      final here = ctx.here;
      if (here != null && !here.connections.contains(target)) {
        return ParsedError('move: no lane from $from to $target (one hop along lanes only)');
      }
      final t = target;
      return needFleet((id) => MoveCommand(fleetId: id, target: t));

    case 'b' || 'build':
      final t = args.isEmpty ? null : _buildings[args.first];
      if (t == null) return ParsedError('build: ${_buildings.keys.where((k) => k.length > 3).toSet().join('|')}');
      return ParsedSend([CommandMessage(BuildCommand(buildingType: t))]);

    case 'research' || 'res':
      final t = args.isEmpty ? null : _techs[args.first];
      if (t == null) return ParsedError('research: ${_techs.keys.join('|')}');
      return ParsedSend([CommandMessage(ResearchCommand(tech: t))]);

    case 'ship' || 'buildship':
      ShipClass? c;
      for (final s in ShipClass.values) {
        if (args.isNotEmpty && s.name == args.first) c = s;
      }
      if (c == null) return const ParsedError('ship: scout|corvette|frigate|cruiser|hauler [count]');
      final n = args.length > 1 ? int.tryParse(args[1]) : 1;
      if (n == null || n < 1 || n > 1000) return const ParsedError('ship: count must be 1-1000');
      return ParsedSend([CommandMessage(BuildShipCommand(shipClass: c, count: n))]);

    case 'cancel':
      final q = switch (args.isEmpty ? '' : args.first) {
        'building' || 'build' || 'b' => QueueType.building,
        'ship' || 'ships' || 'yard' => QueueType.ship,
        'research' || 'res' || 'r' => QueueType.research,
        _ => null,
      };
      if (q == null) return const ParsedError('cancel: building|ship|research [id] [waiting]');
      final waiting = args.contains('waiting') || args.contains('queued');
      final nums = args.skip(1).map(int.tryParse).whereType<int>();
      final ids = ctx.queues[q];
      final id = nums.isNotEmpty ? nums.first : (waiting ? ids?.waiting : ids?.running)?.firstOrNull;
      if (id == null) return ParsedError('cancel: nothing ${waiting ? 'waiting' : 'running'} in the ${q.name} queue');
      return ParsedSend([
        CommandMessage(waiting ? CancelQueuedCommand(queueType: q, id: id) : CancelBuildCommand(queueType: q, id: id)),
      ]);

    case 'split':
      final fleet = ctx.fleet;
      if (fid == null || fleet == null) return const ParsedError('No fleet selected (you have none)');
      ShipClass? c;
      for (final s in ShipClass.values) {
        if (args.isNotEmpty && s.name == args.first) c = s;
      }
      if (c == null) return const ParsedError('split: <scout|corvette|frigate|cruiser|hauler> [count]');
      final n = args.length > 1 ? int.tryParse(args[1]) : 1;
      final aboard = [for (final s in fleet.ships) if (s.shipClass == c) s.id];
      if (n == null || n < 1 || n > aboard.length) {
        return ParsedError('split: F$fid has ${aboard.length} ${c.name}');
      }
      if (n >= fleet.ships.length) return const ParsedError('split: at least one ship must stay in the fleet');
      return ParsedSend([CommandMessage(SplitCommand(fleetId: fid, shipIds: aboard.take(n).toList()))]);

    case 'merge':
      final fleet = ctx.fleet;
      if (fid == null || fleet == null) return const ParsedError('No fleet selected (you have none)');
      final others = [
        for (final f in ctx.fleets)
          if (f.id != fid && f.location == fleet.location) f,
      ];
      if (others.isEmpty) return const ParsedError('merge: no other fleet in this sector');
      var other = others.first;
      if (args.isNotEmpty) {
        final want = int.tryParse(args.first.replaceFirst('f', ''));
        final match = others.where((f) => f.id == want);
        if (match.isEmpty) return ParsedError('merge: choose ${others.map((f) => 'F${f.id}').join('|')}');
        other = match.first;
      }
      final target = other.id;
      return ParsedSend([CommandMessage(MergeCommand(fleetId: fid, otherFleetId: target))]);

    case 'trade' || 'quote':
      final parts = [for (final a in args) if (a != 'for' && a != 'to') a];
      final give = _resources[parts.elementAtOrNull(0)];
      final want = _resources[parts.elementAtOrNull(1)];
      if (give == null || want == null || parts.length > 3) return ParsedError('$verb: $_tradeUsage');
      if (give == want) return ParsedError('$verb: give and want are both ${give.name}');
      double? amount;
      if (parts.length == 3 && parts[2] != 'all') {
        amount = double.tryParse(parts[2]);
        if (amount == null || amount <= 0) return ParsedError('$verb: $_tradeUsage');
      }
      return needFleet((id) => verb == 'quote'
          ? ExchangeQuoteCommand(fleetId: id, give: give, want: want, amount: amount)
          : ExchangeCommand(fleetId: id, give: give, want: want, amount: amount));

    case 'load':
      final resource = _resources[args.elementAtOrNull(0)];
      if (resource == null || args.length > 2) return const ParsedError(_loadUsage);
      double? amount;
      if (args.length == 2 && args[1] != 'all') {
        amount = double.tryParse(args[1]);
        if (amount == null || amount <= 0) return const ParsedError(_loadUsage);
      }
      return needFleet((id) => LoadCommand(fleetId: id, resource: resource, amount: amount));

    case 'unload':
      return needFleet((id) => UnloadCommand(fleetId: id));

    case 'p' || 'policy':
      if (fid == null) return const ParsedError('No fleet selected (you have none)');
      final names = [for (final a in args) if (!a.contains('=')) a];
      final settings = [for (final a in args) if (a.contains('=')) a];
      final standing = ctx.fleet?.policy;
      PolicyPreset? preset;
      if (names.isEmpty) {
        final cur = standing?.preset ?? PolicyPreset.manual;
        preset = settings.isEmpty ? PolicyPreset.values[(cur.index + 1) % PolicyPreset.values.length] : cur;
      } else {
        preset = _policies[names.first];
      }
      if (preset == null) return const ParsedError(_policyUsage);
      if (settings.isEmpty) return ParsedSend([PolicyUpdate(fleetId: fid, preset: preset)]);
      final params = _policyParams(settings, over: names.isEmpty ? standing?.params : null);
      if (params == null || preset == PolicyPreset.manual) return const ParsedError(_policyUsage);
      return ParsedSend([PolicyUpdate(fleetId: fid, preset: preset, params: params)]);
  }
  return ParsedError('Unknown: "${words.join(' ')}" -- type help');
}

/// The line that gives the active fleet [preset] with [params], as a player
/// types it: a setting at the server's default is left unsaid.
String policyCommand(PolicyPreset preset, [PolicyParams params = const PolicyParams()]) {
  const usual = PolicyParams();
  final name = _policies.entries.firstWhere((e) => e.value == preset).key;
  if (preset == PolicyPreset.manual) return 'policy $name';
  return [
    'policy $name',
    if (params.maxRange != usual.maxRange) 'range=${params.maxRange}',
    if (params.engageRatioX10 != usual.engageRatioX10) 'engage=${params.engageRatioX10 / 10}',
    if (params.fuelBudgetPct != usual.fuelBudgetPct) 'fuel=${params.fuelBudgetPct}',
    if (params.cargoReturnPct != usual.cargoReturnPct) 'cargo=${params.cargoReturnPct}',
    if (params.minFuelPct != usual.minFuelPct) 'reserve=${params.minFuelPct}',
  ].join(' ');
}

const _policyUsage ='policy: manual|prospect|mine|salvage|patrol  [range=1-30] [engage=0.1-10] [fuel=0-100] [cargo=10-100] [reserve=0-90]';

/// Doctrine settings typed as `key=value` over [over], the settings the
/// fleet's orders have when the line names no doctrine, or else over the
/// server's defaults; null when one of them does not parse or lies outside
/// what the server takes, so the settings sent are the settings in force.
PolicyParams? _policyParams(List<String> settings, {PolicyParams? over}) {
  final preset = over ?? const PolicyParams();
  var (reserve, cargo, range) = (preset.minFuelPct, preset.cargoReturnPct, preset.maxRange);
  var (engage, fuel) = (preset.engageRatioX10, preset.fuelBudgetPct);
  for (final setting in settings) {
    final cut = setting.indexOf('=');
    final value = num.tryParse(setting.substring(cut + 1));
    if (value == null || value < 0) return null;
    switch (setting.substring(0, cut)) {
      case 'reserve':
        reserve = value.round();
      case 'cargo':
        cargo = value.round();
      case 'range':
        range = value.round();
      case 'engage':
        engage = (value * 10).round();
      case 'fuel':
        fuel = value.round();
      default:
        return null;
    }
  }
  bool within(int value, int least, int most) => value >= least && value <= most;
  if (!within(reserve, 0, 90) || !within(cargo, 10, 100) || !within(range, 1, 30) || !within(engage, 1, 100) || !within(fuel, 0, 100)) {
    return null;
  }
  return PolicyParams(
    minFuelPct: reserve,
    cargoReturnPct: cargo,
    maxRange: range,
    engageRatioX10: engage,
    fuelBudgetPct: fuel,
  );
}
