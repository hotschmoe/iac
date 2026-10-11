import '../../console/economy_view.dart';
import '../../console/intel.dart';
import '../../models/game_state.dart';
import '../../protocol/protocol.dart' as proto;

enum EventFilter { all, mine, combat, economy, intel }

extension EventFilterLabel on EventFilter {
  String get label => switch (this) {
        EventFilter.all => 'All',
        EventFilter.mine => 'Mine',
        EventFilter.combat => 'Combat',
        EventFilter.economy => 'Economy',
        EventFilter.intel => 'Intel',
      };
}

final _combat = RegExp(r'combat|raid|destroyed|ambush|vs hostile|\blost\b|repelled', caseSensitive: false);
final _economy = RegExp(r'harvest|salvage|research|build|built|shipyard|collected|yielded|wreckage|storage', caseSensitive: false);
final _intel = RegExp(r'scan|entered|sector|reveal|sync|arrived|uplink|derelict|token|boarding', caseSensitive: false);
final _mine = RegExp(r'^(>|!+ ?your|f\d+ |your )|your |^!! your', caseSensitive: false);

/// Whether a log line belongs under [f]. Mine means the player's own
/// fleets and commands (the `F12 ...` prefix, `> command` echoes, `Your ...`).
bool eventMatches(EventFilter f, String msg) => switch (f) {
      EventFilter.all => true,
      EventFilter.mine => _mine.hasMatch(msg),
      EventFilter.combat => _combat.hasMatch(msg),
      EventFilter.economy => _economy.hasMatch(msg) && !_combat.hasMatch(msg),
      EventFilter.intel => _intel.hasMatch(msg) && !_combat.hasMatch(msg) && !_economy.hasMatch(msg),
    };

String fmtInt(num v) => groupDigits(v.round());

String costText(proto.Resources c) {
  final o = <String>[];
  if (c.metal > 0) o.add('${fmtInt(c.metal)} Fe');
  if (c.crystal > 0) o.add('${fmtInt(c.crystal)} Cr');
  if (c.deuterium > 0) o.add('${fmtInt(c.deuterium)} De');
  return o.join('  ');
}

double _sum(proto.Resources c) => c.metal + c.crystal + c.deuterium;

/// Research options that could be queued now (prerequisites met, not maxed),
/// cheapest first.
List<proto.ResearchOption> researchChoices(proto.HomeworldCatalog? c, {int limit = 3}) {
  if (c == null) return const [];
  final l = [
    for (final o in c.research)
      if (o.next != null && o.requires.every((r) => r.met)) o,
  ]..sort((a, b) => _sum(a.next!.cost).compareTo(_sum(b.next!.cost)));
  return l.take(limit).toList();
}

List<proto.BuildingOption> buildingChoices(proto.HomeworldCatalog? c, {int limit = 2}) {
  if (c == null) return const [];
  final l = [
    for (final o in c.buildings)
      if (o.next != null && o.requires.every((r) => r.met)) o,
  ]..sort((a, b) => _sum(a.next!.cost).compareTo(_sum(b.next!.cost)));
  return l.take(limit).toList();
}

/// "Scout needs Shipyard level 1": of [options], each a name and what it
/// requires, the locked one with the fewest levels still to build or
/// research. Null when none of them is locked.
String? nearestLocked(Iterable<(String, List<proto.Requirement>)> options) {
  (String, List<proto.Requirement>, int)? nearest;
  for (final (name, requires) in options) {
    final unmet = [for (final r in requires) if (!r.met) r];
    final levels = unmet.fold(0, (sum, r) => sum + r.need - r.have);
    if (unmet.isNotEmpty && (nearest == null || levels < nearest.$3)) nearest = (name, unmet, levels);
  }
  return nearest == null ? null : '${nearest.$1} needs ${nearest.$2.map((r) => r.label).join(', ')}';
}

List<proto.ShipOption> shipChoices(proto.HomeworldCatalog? c, {int limit = 3}) {
  if (c == null) return const [];
  final l = [for (final o in c.ships) if (o.requires.every((r) => r.met)) o]
    ..sort((a, b) => _sum(a.unitCost).compareTo(_sum(b.unitCost)));
  return l.take(limit).toList();
}

enum NextAction { none, openDefence, queueResearch, queueBuild, openWindshield, openHomeworld, openMap }

class NextUp {
  final String kicker;
  final bool urgent;
  final String title;
  final String why;
  final proto.Resources? cost;
  final String button;
  final NextAction action;
  final proto.ResearchType? tech;
  final proto.BuildingType? building;
  final String? disabledReason;
  const NextUp(this.kicker, this.title, this.why, this.button, this.action,
      {this.urgent = false, this.cost, this.tech, this.building, this.disabledReason});
}

/// The single most useful thing to do now, in priority order: raid, stranded
/// fleet, storage near cap, idle lab, idle build slot, unscanned lanes.
NextUp nextUp(GameState s, EconomyView eco) {
  final d = eco.defence;
  final raidIn = d?.raidInTicks;
  if (d != null && raidIn != null && raidIn > 0 && raidIn < 600) {
    return NextUp(
      'RAID WARNING',
      'Raid in ${clockFmt(raidIn)}',
      'Raid fleet vectoring for ${s.homeworld}. Home defence is ${d.total.round()} power: docked fleet and grid ${d.other.round()}, structures ${d.structures.round()}. Estimated raid power ${d.raidEstimate.round()}.',
      'Open defence',
      NextAction.openDefence,
      urgent: true,
    );
  }
  for (final f in s.fleets) {
    if (f.stranded) {
      return NextUp('FLEET STRANDED', '${f.name} is out of fuel', 'It cannot jump from ${f.sector}. Send fuel or wait for a tanker.', 'Open windshield', NextAction.openWindshield, urgent: true);
    }
  }
  final st = eco.storage;
  if (st != null) {
    for (final e in {'metal': (s.stock.metal, 'Metal'), 'crystal': (s.stock.crystal, 'Crystal'), 'deut': (s.stock.deuterium, 'Deuterium')}.entries) {
      final cap = st.cap[e.key] ?? 0;
      if (cap > 0 && e.value.$1 / cap >= .85) {
        return NextUp('STORAGE', '${e.value.$2} is near its cap', 'Production above the cap is discarded. Spend it or raise the Storage Vault.', 'Open homeworld', NextAction.openHomeworld);
      }
    }
  }
  final stock = s.stock;
  final labIdle = s.research.name == 'Idle';
  final rc = researchChoices(s.catalog);
  if (labIdle && rc.isNotEmpty) {
    final o = rc.first;
    final short = shortReason(stock, o.next!.cost);
    return NextUp('NEXT UP', 'Research ${o.tech.label} L${o.next!.level}', 'The lab is idle. ${short == null ? 'You can afford it now.' : '$short.'}', 'Queue it', NextAction.queueResearch,
        cost: o.next!.cost, tech: o.tech, disabledReason: short);
  }
  if (s.buildQueue.isEmpty) {
    final bc = buildingChoices(s.catalog, limit: 1);
    if (bc.isNotEmpty) {
      final o = bc.first;
      return NextUp('NEXT UP', 'Upgrade ${o.buildingType.label} L${o.next!.level}', 'The build slot is idle. Mines compound; idle slots are lost production.', 'Queue it', NextAction.queueBuild,
          cost: o.next!.cost, building: o.buildingType, disabledReason: shortReason(stock, o.next!.cost));
    }
  }
  for (final f in s.fleets) {
    final sec = s.sectors[f.sector];
    if (sec != null && sec.connections.any((h) => !s.sectors.containsKey(h))) {
      return NextUp('NEXT UP', 'Scan from ${f.name}', 'Uncharted lanes next to ${f.sector}. Scanning reveals neighbours and hostiles.', 'Open windshield', NextAction.openWindshield);
    }
  }
  return const NextUp('ALL SYSTEMS', 'Everything is running', 'Queues are full. Send a fleet out: exploring pays more the further you go.', 'Open windshield', NextAction.openWindshield);
}

class OvAlert {
  final String title;
  final String body;
  final bool critical;
  final String? time;
  final String? tab;
  const OvAlert(this.title, this.body, {this.critical = false, this.time, this.tab});
}

List<OvAlert> buildAlerts(GameState s, EconomyView eco) {
  final out = <OvAlert>[];
  final d = eco.defence;
  final raidIn = d?.raidInTicks;
  if (d != null && raidIn != null && raidIn > 0) {
    out.add(OvAlert('Raid incoming', 'Power est. ${d.raidEstimate.round()} vs your defence ${d.total.round()}.', critical: true, time: clockFmt(raidIn), tab: 'defence'));
  }
  final st = eco.storage;
  if (st != null) {
    final rates = {'metal': s.resources.metal.rate, 'crystal': s.resources.crystal.rate, 'deut': s.resources.deut.rate};
    final stock = {'metal': s.stock.metal, 'crystal': s.stock.crystal, 'deut': s.stock.deuterium};
    final names = {'metal': 'Metal', 'crystal': 'Crystal', 'deut': 'Deuterium'};
    for (final k in stock.keys) {
      final cap = st.cap[k] ?? 0;
      if (cap > 0 && stock[k]! / cap >= .85) {
        final f = st.fullIn(k, stock[k]!, rates[k]!);
        out.add(OvAlert('${names[k]} storage near cap', '${(stock[k]! / cap * 100).round()}% of the Vault L${st.vaultLevel} cap. Production above the cap is discarded.', time: f == null ? null : clockFmt(f), tab: 'storage'));
      }
    }
  }
  for (final f in s.fleets) {
    if (f.shortOfHomeFuel && f.homeFuel > 0) {
      out.add(OvAlert('${f.name} fuel is low', 'Fuel ${f.fuel} of ${f.fuelMax}; the way home needs ${f.homeFuel}.'));
    }
  }
  if (labIdle(s)) out.add(const OvAlert('Research lab idle', 'Queue something. Idle labs waste the longest timers.', tab: 'research'));
  final hasRaid = out.any((a) => a.tab == 'defence');
  final hasStorage = out.any((a) => a.tab == 'storage');
  for (final a in s.alerts) {
    final m = a.message.toLowerCase();
    if ((hasRaid && m.contains('raid')) || (hasStorage && m.contains('storage'))) continue;
    out.add(OvAlert(a.message, a.detail, critical: a.level == AlertTone.glow));
  }
  return out;
}

/// The lab stands idle with something it could research: without a Research
/// Lab, or with every tech locked or maxed, there is nothing to queue.
bool labIdle(GameState s) => s.research.name == 'Idle' && researchChoices(s.catalog, limit: 1).isNotEmpty;

/// "Two fleets out, one at home. Lab idle." style status line.
String hqNote(GameState s) {
  final home = s.fleets.where((f) => f.sector == s.homeworld).length;
  final out = s.fleets.length - home;
  final parts = <String>[];
  const words = ['No', 'One', 'Two', 'Three', 'Four', 'Five', 'Six'];
  String w(int n) => n < words.length ? words[n] : '$n';
  String fleets(int n) => '${w(n)} fleet${n == 1 ? '' : 's'}';
  if (s.fleets.isNotEmpty) {
    parts.add(home == 0
        ? '${fleets(out)} out.'
        : out == 0
            ? '${fleets(home)} at home.'
            : '${fleets(out)} out, ${w(home).toLowerCase()} at home.');
  }
  if (labIdle(s)) parts.add('Lab idle.');
  if (s.buildQueue.isEmpty) parts.add('Build slot idle.');
  return parts.isEmpty ? 'All quiet on the surface.' : parts.join(' ');
}
