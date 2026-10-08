import '../protocol/protocol.dart';

/// What the command bar needs to know to resolve a typed command.
class CommandContext {
  final int? fleetId;
  final Hex? fleetSector;
  final Map<Hex, SectorState> sectors;
  final List<FleetState> fleets;
  final Hex cursor;
  final int tick;
  const CommandContext({
    required this.fleetId,
    required this.fleetSector,
    required this.sectors,
    required this.fleets,
    required this.cursor,
    required this.tick,
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
  'base:   b[uild] <metal|crystal|deut|shipyard|lab|fuel|sensor|defense>',
  '        research <fuel|tanks|hulls|shields|weapons|nav|harvest|corvette|frigate|cruiser|hauler|jump>',
  '        ship <scout|corvette|frigate|cruiser|hauler> [count]   cancel <building|ship|research>',
  'fleets: f[leet] [n]  select/list   p[olicy] [manual|prospect|mine|salvage|patrol]  (no arg cycles)',
  'other:  status  refresh  help      views: 1|cc  2|ws  3|map   TAB cycles fleets',
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
};

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

    case 'refresh' || 'sync':
      return const ParsedSend([RequestFullState()]);

    case 'f' || 'fleet' || 'fleets':
      if (args.isEmpty) {
        if (ctx.fleets.isEmpty) return const ParsedLocal(['No fleets']);
        return ParsedLocal([
          for (var i = 0; i < ctx.fleets.length; i++)
            '${ctx.fleets[i].id == fid ? '>' : ' '}${i + 1}: F${ctx.fleets[i].id} ${ctx.fleets[i].state.wire} '
                '${ctx.fleets[i].location} ships ${ctx.fleets[i].ships.length}'
                '${ctx.fleets[i].policy == null ? '' : ' [${ctx.fleets[i].policy!.label}]'}',
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
      if (q == null) return const ParsedError('cancel: building|ship|research');
      return ParsedSend([CommandMessage(CancelBuildCommand(queueType: q))]);

    case 'p' || 'policy':
      if (fid == null) return const ParsedError('No fleet selected (you have none)');
      PolicyPreset? preset;
      if (args.isEmpty) {
        final cur = ctx.fleet?.policy ?? PolicyPreset.manual;
        preset = PolicyPreset.values[(cur.index + 1) % PolicyPreset.values.length];
      } else {
        preset = _policies[args.first];
      }
      if (preset == null) return const ParsedError('policy: manual|prospect|mine|salvage|patrol');
      return ParsedSend([PolicyUpdate(fleetId: fid, preset: preset)]);
  }
  return ParsedError('Unknown: "${words.join(' ')}" -- type help');
}
