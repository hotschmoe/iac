// Decode -> encode round trips over the Rust-generated golden fixtures: the
// proof that the generated wire types (lib/protocol/protocol.g.dart) read
// and write what serde does. If these fail after a change to the Rust types,
// regenerate both with `UPDATE_FIXTURES=1 cargo test -p iac-shared`.

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/battle.dart' show RaidEnding, raidLine, raidRebuilt, raidSummary, raidTaken;
import 'package:iac_client/console/economy_view.dart' show EconomyView;
import 'package:iac_client/protocol/protocol.dart';
import 'package:iac_client/state/state_mapper.dart';

import 'fixtures_util.dart';

/// Structural equality for decoded JSON (numbers compare by value, so
/// `100` == `100.0`; map key order is irrelevant).
bool jsonEq(Object? a, Object? b) {
  if (a is Map && b is Map) {
    if (a.length != b.length) return false;
    for (final k in a.keys) {
      if (!b.containsKey(k) || !jsonEq(a[k], b[k])) return false;
    }
    return true;
  }
  if (a is List && b is List) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (!jsonEq(a[i], b[i])) return false;
    }
    return true;
  }
  if (a is num && b is num) return a == b;
  return a == b;
}

/// Re-encode through a real JSON string, as it would go over the wire.
Object? wire(Object? v) => jsonDecode(jsonEncode(v));

void roundTrip(String dir, Object? Function(Object?) decodeEncode) {
  group(dir, () {
    for (final f in fixtureFiles('protocol/$dir')) {
      test(baseName(f), () {
        final original = readJson(f);
        final out = wire(decodeEncode(original));
        expect(jsonEq(out, original), isTrue, reason: 'round trip changed JSON\n  in:  ${jsonEncode(original)}\n  out: ${jsonEncode(out)}');
      });
    }
  });
}

void main() {
  roundTrip('client_message', (j) => ClientMessage.fromJson(j).toJson());
  roundTrip('command', (j) => Command.fromJson(j).toJson());
  roundTrip('server_message', (j) => ServerMessage.fromJson(j).toJson());
  roundTrip('event_kind', (j) => GameEvent.fromJson(j).toJson());

  group('synthesised messages (every type, variant and field; each input must come back as serde re-emits it)', () {
    for (final (dir, decodeEncode) in <(String, Object? Function(Object?))>[
      ('client_message', (j) => ClientMessage.fromJson(j).toJson()),
      ('server_message', (j) => ServerMessage.fromJson(j).toJson()),
    ]) {
      for (final f in fixtureFiles('protocol/synth/$dir')) {
        test('$dir ${baseName(f)}', () {
          final cases = readJson(f) as List;
          expect(cases, isNotEmpty);
          for (final c in cases) {
            final out = wire(decodeEncode(c['input']));
            expect(jsonEq(out, c['canonical']), isTrue, reason: 'in ${jsonEncode(c['input'])}\n  out ${jsonEncode(out)}\n  want ${jsonEncode(c['canonical'])}');
          }
        });
      }
    }
  });

  group('decoded types', () {
    test('every fixture decodes to the sealed variant its name says', () {
      final expected = <String, Type>{
        'client_message/auth': AuthRequest,
        'client_message/command': CommandMessage,
        'client_message/policy_update': PolicyUpdate,
        'client_message/request_full_state': RequestFullStateMessage,
        'server_message/auth_result': AuthResult,
        'server_message/tick_update': TickUpdate,
        'server_message/full_state': GameState,
        'server_message/event': EventMessage,
        'server_message/error__server_error': ErrorMessage,
        'server_message/preview_move': MovePreview,
        'server_message/leaderboard': LeaderboardReply,
        'server_message/exchange_quote': ExchangeQuote,
        'server_message/outpost_survey': OutpostSurvey,
        'server_message/relay_survey': RelaySurvey,
      };
      for (final e in expected.entries) {
        final dir = e.key.split('/')[0];
        final name = e.key.split('/')[1];
        final f = fixtureFiles('protocol/$dir').firstWhere((f) => baseName(f) == name);
        final j = readJson(f);
        final decoded = dir == 'client_message' ? ClientMessage.fromJson(j) : ServerMessage.fromJson(j);
        expect(decoded.runtimeType, e.value, reason: e.key);
      }
    });

    test('event_kind fixtures decode to distinct kinds covering all 32 variants', () {
      final kinds = {
        for (final f in fixtureFiles('protocol/event_kind')) GameEvent.fromJson(readJson(f)).kind.runtimeType,
      };
      expect(kinds.length, 32);
    });

    test('command fixtures cover all 30 commands', () {
      final kinds = {for (final f in fixtureFiles('protocol/command')) Command.fromJson(readJson(f)).runtimeType};
      expect(kinds.length, 30);
    });

    test('a state with an outpost carries it, its sector mark and the fuel to it', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state__with_outpost');
      final state = ServerMessage.fromJson(readJson(f)) as GameState;
      final outpost = state.outpost!;
      expect((outpost.level, outpost.maxLevel, outpost.inService, outpost.breached), (1, 3, true, false));
      expect((outpost.work!.kind, outpost.work!.level, outpost.nextLevel!.level), (OutpostWork.build, 2, 3));
      expect((outpost.raidArrivalTick, outpost.repairCost, outpost.hopsHome), (4990, null, 7));
      expect((outpost.raidPower.least.round(), outpost.raidPower.most.round()), (383, 551));
      expect(outpost.platform.map((u) => (u.unit.defence, u.count)), [(DefenceKind.rocketLauncher, 4)], reason: 'the platform is defences, by kind');
      expect(outpost.nextLevel!.platform.map((u) => (u.unit.defence, u.count)),
          [(DefenceKind.rocketLauncher, 16), (DefenceKind.lightLaser, 8), (DefenceKind.heavyLaser, 4)]);
      final raid = outpost.raid!;
      expect((raid.arrivalTick, raid.roundsLeft, raid.odds.label, raid.odds.lost), (4990, null, RaidLabel.atRisk, 0.375));
      expect(raid.ships.map((s) => (s.shipClass, s.count)), [(ShipClass.cruiser, 1)]);
      expect(outpost.yieldLost, isEmpty);
      expect(outpost.repairWaitingFor, isNull);
      expect(outpost.upgradeRefusal, 'your outpost at [10,-2] is building level 2 until tick 18200; it does one job at a time');
      expect(state.fleets.map((fleet) => (fleet.atOutpost, fleet.outpostFuel)), [(true, 0.0), (false, 220.5)]);
      final marks = state.knownSectors.map((s) => (s.outpost!.owner, s.outpost!.level, s.outpost!.mine, s.live, s.threat.rating));
      expect(marks, [('Admiral', 1, true, true, 0), ('Rival', 0, false, false, 0)]);

      final mapped = StateMapper()..apply(state);
      expect(mapped.outpost!.location, outpost.location);
      final t = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'tick_update__outpost_breached');
      mapped.apply(ServerMessage.fromJson(readJson(t)));
      expect((mapped.outpost!.breached, mapped.outpost!.repairCost!.metal), (true, 2187.5));
      expect((mapped.outpost!.repairWaitingFor!.metal, mapped.outpost!.repairWaitingFor!.deuterium), (987.5, 0));
      expect((mapped.fleets.single.atOutpost, mapped.fleets.single.outpostFuel), (true, null), reason: 'at a breached outpost, which fills no tank');
      final full = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'tick_update__outpost_store_full');
      mapped.apply(ServerMessage.fromJson(readJson(full)));
      expect(mapped.outpost!.yieldLost, [ResourceKind.metal]);
      mapped.apply(const TickUpdate(tick: 20001, fleets: []));
      expect(mapped.outpost, null, reason: 'a tick without an outpost means the player has none');
    });

    test('a survey of a sector never seen carries no site and says why', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'outpost_survey__unseen');
      final survey = ServerMessage.fromJson(readJson(f)) as OutpostSurvey;
      expect((survey.site, survey.ring, survey.raidPower.most.round()), (null, 10, 551));
      expect((survey.platform.single.unit.defence, survey.platform.single.count, survey.raid!.arrivalTick, survey.raid!.odds.label),
          (DefenceKind.rocketLauncher, 4, null, RaidLabel.atRisk),
          reason: 'what follows from where the sector lies is told of a sector never seen: the platform and how a raid of the ring would go against it');
      expect(survey.refusal, '[10,-2] is not on your chart; a survey reads the site of a sector you have seen');
    });

    test('full_state names the hub and the player meter at its exchange', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final s = ServerMessage.fromJson(readJson(f)) as GameState;
      expect(s.world.servicesAt(Hex.origin), [HubService.exchange]);
      expect(s.world.servicesAt(const Hex(1, 0)), isEmpty);
      expect(s.world.nearest(HubService.exchange, const Hex(4, -2)), Hex.origin);
      expect((s.player.exchange.pressure, s.player.exchange.span, s.player.exchange.clearIn), (632.5, 4185, 2322));
      expect(s.player.exchange.fill, closeTo(632.5 / 4185, 1e-9));
    });

    test('a quote is kept for the fleet and the pair it was asked for, a refused one with its reason', () {
      final files = fixtureFiles('protocol/server_message');
      final mapper = StateMapper();
      for (final name in ['exchange_quote', 'exchange_quote__refused']) {
        mapper.apply(ServerMessage.fromJson(readJson(files.firstWhere((f) => baseName(f) == name))));
      }
      final open = mapper.quotes[(3, ResourceKind.metal, ResourceKind.deuterium)]!.quote;
      expect((open.terms.amount, open.terms.receive, open.refusal), (632.5, 225, null));
      final refused = mapper.quotes[(3, ResourceKind.deuterium, ResourceKind.metal)]!.quote;
      expect(refused.refusal, contains('3 hexes from the exchange at the hub'));
      expect(refused.terms.meterAfter.clearIn, isNull);
      expect(mapper.log, isEmpty, reason: 'quotes are asked for by the panel; they do not write to the log');
    });

    test('full_state exposes sector connections', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final s = ServerMessage.fromJson(readJson(f)) as GameState;
      expect(s.knownSectors.first.connections, isNotEmpty);
      expect(
          s.fleets.first.policy,
          const StandingOrders(
              preset: PolicyPreset.salvageAndSites,
              params: PolicyParams(minFuelPct: 25, cargoReturnPct: 70, maxRange: 9, engageRatioX10: 22, fuelBudgetPct: 100)),
          reason: 'a fleet under orders carries the doctrine and every setting in force');
      expect(s.fleets[1].policy, isNull, reason: 'and one flown by hand carries none');
      expect(s.homeworld.buildings.length, BuildingType.values.length);
      expect(s.homeworld.storage.cap.metal, 150000);
      expect(s.homeworld.storage.capped, [ResourceKind.deuterium]);
      expect(s.homeworld.storage.fullInS.crystal, isNull);
      final energy = s.homeworld.energy;
      expect((energy.made, energy.needed, energy.perSatellite, energy.fusionStarved), (412, 520, 30, false));
      expect((energy.short, energy.sharePct), (true, 79), reason: 'short of energy the mines run at the share the server states');
      expect((s.homeworld.satellites, s.homeworld.production.deuterium), (6, -0.4), reason: 'the Fusion Reactor may burn more deuterium than is made');
      expect(s.homeworld.buildSlotUnlock!.label, 'Computer Technology level 6');
      expect(s.world.rapidFire, isNotEmpty);
      expect(s.world.rapidFireOf(ShipClass.cruiser).map((r) => (r.against.label, r.shots)), contains(('Light Fighter', 6)));
      expect(s.world.rapidFireAgainst(const ShipyardItem.defence(DefenceKind.rocketLauncher)).map((r) => r.shipClass), contains(ShipClass.cruiser));
    });

    test('a fleet says when its hold was loaded at the dock, and load says what it takes', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'tick_update');
      final u = ServerMessage.fromJson(readJson(f)) as TickUpdate;
      expect([for (final fleet in u.fleets) fleet.laden], [false, true]);
      final commands = {for (final f in fixtureFiles('protocol/command')) baseName(f): Command.fromJson(readJson(f))};
      final exact = commands['load'] as LoadCommand;
      final all = commands['load__all_that_fits'] as LoadCommand;
      expect((exact.fleetId, exact.resource, exact.amount), (3, ResourceKind.metal, 1200.5));
      expect((all.resource, all.amount), (ResourceKind.deuterium, null));
      expect((commands['unload'] as UnloadCommand).fleetId, 3);
    });

    test('a fleet says what refining the deuterium in its hold would make, and the mapper hands it on', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'tick_update');
      final u = ServerMessage.fromJson(readJson(f)) as TickUpdate;
      expect([for (final fleet in u.fleets) (fleet.refine?.deuterium, fleet.refine?.fuel)], [(null, null), (20.0, 212.5)]);
      final mapped = StateMapper()..apply(u);
      expect([for (final fleet in mapped.fleets) fleet.refine?.fuel], [null, 212.5]);
      final commands = {for (final f in fixtureFiles('protocol/command')) baseName(f): Command.fromJson(readJson(f))};
      expect((commands['refine'] as RefineCommand).fleetId, 3);
    });

    test('the ratio verdict bands match the server', () {
      final cases = readJson(File('${fixturesDir().path}/protocol/ratio_labels.json')) as List;
      expect(cases, isNotEmpty);
      for (final c in cases) {
        final r = (c['ratio'] as num).toDouble();
        expect(ratioLabelFor(r).wire, c['label'], reason: 'ratio $r');
      }
    });

    test('a move preview is kept for the screens, with both ways home and the verdict', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'preview_move__stranding');
      final msg = ServerMessage.fromJson(readJson(f)) as MovePreview;
      final mapper = StateMapper()..apply(msg);
      final kept = mapper.previews[(msg.fleetId, msg.target.toKey())]!.preview;
      expect(kept.fuelCost, 31.5);
      expect(kept.canJump, isFalse);
      expect(kept.hopsHome, 4);
      expect(kept.chartedHopsHome, 17);
      expect(kept.routeUnexplored, isTrue);
      expect(kept.label, RatioLabel.even);
      expect(mapper.log, isEmpty, reason: 'previews are asked for by hovering; they do not write to the log');
    });

    test('an odds answer is kept per fleet and sector, with the server ratio and verdict untouched', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'odds');
      final msg = ServerMessage.fromJson(readJson(f)) as FleetOdds;
      final mapper = StateMapper()..apply(msg);
      expect(mapper.odds.keys.map((k) => k.$1).toSet(), {3});
      final far = mapper.odds[(3, const Hex(3, -6).toKey())]!.odds;
      expect(far.ratio, closeTo(0.84, 1e-6));
      expect((far.label, far.threat.estPower), (RatioLabel.deadly, 46.5));
      expect(mapper.odds[(3, const Hex(2, -4).toKey())]!.odds.label, RatioLabel.safe);
      expect(mapper.log, isEmpty);
    });

    test('the homeworld lists its defences and the forecast of the raid on its way: what it flies, the label, the three shares and what is kept', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final state = ServerMessage.fromJson(readJson(f)) as GameState;
      final hw = state.homeworld;
      expect(hw.defences.first.kind, DefenceKind.rocketLauncher);
      expect(hw.defences.first.count, 12);
      final raid = hw.raid;
      expect((raid.arrivalTick! - state.tick, raid.roundsLeft, raid.power), (170, null, 1350.5));
      expect(raid.ships.map((s) => (s.shipClass, s.count)), [(ShipClass.lightFighter, 12), (ShipClass.cruiser, 2)]);
      expect((raid.odds.label, raid.odds.label.label, raid.odds.label.outcome),
          (RaidLabel.held, 'HELD', 'a defender stands when the raiders give up; what falls before that is lost'));
      expect((raid.odds.destroyed, raid.odds.outlasted, raid.odds.lost), (0.25, 0.75, 0));
      expect([raid.odds.kept, raid.odds.raidersKept], [closeTo(0.62, 1e-6), closeTo(0.31, 1e-6)]);
      final view = EconomyView.of((StateMapper()..apply(state)).toUiState()).defence!;
      expect((view.raid.odds.label, view.raidInTicks, view.fought, view.pressing), (RaidLabel.held, 170, false, true));

      final idle = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state__empty');
      final quiet = EconomyView.of((StateMapper()..apply(ServerMessage.fromJson(readJson(idle)))).toUiState()).defence!;
      expect((quiet.raid.arrivalTick, quiet.raidInTicks, quiet.pressing, quiet.raid.odds.label), (null, null, false, RaidLabel.lost),
          reason: 'with no raid on its way the forecast is of one of typical size, and nothing counts down');
      expect(hw.catalog.defences.map((d) => d.count), [12, 0, 0, 3, 0, 0, 1, 0]);
      expect(hw.catalog.defences.map((d) => (d.max, d.room)), [for (var i = 0; i < 6; i++) (null, null), (1, 0), (1, 0)],
          reason: 'a dome is one of a kind, and one that stands or is on order leaves room for no more');
      expect(hw.catalog.defences.first.requires.first.label, 'Shipyard level 1');
    });

    test('the mapper keeps the table, and the log has its lines with the agent label for a player who asks', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'leaderboard');
      final mapper = StateMapper()..apply(ServerMessage.fromJson(readJson(f)));
      expect(mapper.leaderboard!.entries.length, 2);
      expect(mapper.leaderboard!.entries[1].agent, isTrue);
      expect(mapper.leaderboard!.you!.rank, 14);
      expect(boardLines(mapper.leaderboard!), contains(startsWith('#2 Claude-7 [AI]')));
      expect(mapper.log, isEmpty, reason: 'a board nobody typed for feeds the ranking panel, not the log');
      expect(mapper.toUiState().leaderboard, same(mapper.leaderboard));
    });

    test('built-in bots are listed apart from the ranking, tagged by their name', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'leaderboard__with_bots');
      final mapper = StateMapper()..apply(ServerMessage.fromJson(readJson(f)));
      final board = mapper.leaderboard!;
      expect(board.entries.map((e) => e.name), ['Admiral']);
      expect(board.bots.map((e) => e.name), ['BOT-explorer-2', 'BOT-turtle-1']);
      expect(board.bots.every((e) => isBotName(e.name)), isTrue);
      expect(board.entries.any((e) => isBotName(e.name)), isFalse);
      expect(boardLines(board), contains('2 built-in bots, unranked; best BOT-explorer-2 505.0'));
      final rank = EconomyView.of(mapper.toUiState(), me: 'Admiral').rank!;
      expect(rank.board.map((r) => (r.$1, r.$2, r.$3)), [(1, 'Admiral', 'HUM'), (0, 'BOT-explorer-2', 'BOT'), (0, 'BOT-turtle-1', 'BOT')]);
    });

    test('a sector says whether its chart is aboard or delivered', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final marks = (ServerMessage.fromJson(readJson(f)) as GameState).knownSectors.map((s) => s.chart).toSet();
      expect(marks, {null, ChartMark.aboard, ChartMark.delivered});
    });

    test('fleets carry power and range', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final fleet = (ServerMessage.fromJson(readJson(f)) as GameState).fleets.first;
      expect(fleet.power, 61.5);
      expect(fleet.rangeHops, 4);
    });

    test('a client that arrives in the middle of a fight has its last round from the full state; a tick update leaves the round to its event', () {
      ServerMessage message(String name) => ServerMessage.fromJson(readJson(fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == name)));
      const at = Hex(-5, 7);
      final mapper = StateMapper()..apply(message('full_state'));
      final seeded = mapper.toUiState().battleAt(at)!;
      expect((seeded.rounds, seeded.round!.round, seeded.round!.fleets.standing, seeded.round!.hostiles.standing, seeded.over), (3, 3, 57, 83, false));
      expect(seeded.losses, isNotEmpty);
      expect(mapper.toUiState().shipCount, 888, reason: 'the empire\'s ships are the state\'s own count');

      final update = message('tick_update') as TickUpdate;
      expect(update.battles.single.lastRound, isNull);
      mapper.apply(update);
      final later = mapper.toUiState().battleAt(at)!;
      expect((later.rounds, later.round?.round), (3, 3), reason: 'the round it was seeded with stands until an event brings the next');
    });

    test('a raid being fought is the homeworld\'s forecast and a fight with a battery; its end is kept with what the raid came to', () {
      ServerMessage message(String name) => ServerMessage.fromJson(readJson(fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == name)));
      GameEvent event(String name) => GameEvent.fromJson(readJson(fixtureFiles('protocol/event_kind').firstWhere((f) => baseName(f) == name)));
      const home = Hex(4, -2);
      final mapper = StateMapper()..apply(message('full_state'));
      final fought = message('tick_update__raid_fought') as TickUpdate;
      mapper.apply(fought);
      final state = mapper.toUiState();
      final forecast = state.hw!.raid;
      expect((forecast.arrivalTick, forecast.roundsLeft, forecast.odds.label), (null, 4, RaidLabel.held),
          reason: 'while the raid is fought the forecast is that raid, with the rounds it has left');
      final fight = state.battleAt(home)!;
      expect((fight.isRaid, fight.raid!.mine, fight.raid!.outpost, fight.raid!.roundsLeft, fight.rounds), (true, true, false, 4, 2));
      final round = fight.round!;
      expect((round.raid, round.round), (true, 2), reason: 'the round came as the update\'s event');
      final battery = round.fleets.groups.singleWhere((g) => g.battery);
      expect((battery.fleetId, battery.label, battery.mine), (0, 'home defences', true));
      expect(battery.units.map((u) => (u.unit.ship, u.unit.defence, u.count, u.lost)), [
        (ShipClass.solarSatellite, null, 2, 2),
        (null, DefenceKind.rocketLauncher, 9, 11),
        (null, DefenceKind.lightLaser, 10, 2),
        (null, DefenceKind.smallShieldDome, 1, 0),
      ]);
      expect(round.hostiles.groups.single.label, 'raid of 12 Light Fighter, 2 Cruiser');
      expect(mapper.fleets.single.flight!.chase, isNull, reason: 'a fleet in a raid\'s fight may leave, and nothing follows');

      final ended = event('combat_ended__raid_outlasted').kind as CombatEndedEvent;
      final resolved = event('raid_resolved__defended').kind as RaidResolvedEvent;
      expect((ended.raid, ended.outcome, ended.playerVictory, resolved.defended), (true, BattleOutcome.outlasted, false, true),
          reason: 'an outlasted raid is repelled, though the fight was not won');
      mapper
        ..apply(EventMessage(GameEvent(tick: 1646, kind: ended)))
        ..apply(EventMessage(GameEvent(tick: 1646, kind: resolved)));
      final over = mapper.toUiState();
      expect((over.raidAtHome!.over, identical(over.raidAtHome!.resolved, resolved), identical(over.lastRaid!.raid, resolved), over.lastRaid!.tick),
          (true, true, true, 1646));
      expect(RaidEnding.of(resolved), RaidEnding.outlasted);
      expect(mapper.log.first.message, startsWith('! Raid on your homeworld outlasted after 6 rounds: the raiders gave up with a defender still standing'));
      expect(mapper.log.where((line) => line.message.contains('Combat ended')), isEmpty, reason: 'a raid\'s end is told once, by what it came to');

      final destroyed = event('raid_resolved__destroyed').kind as RaidResolvedEvent;
      expect((destroyed.defended, destroyed.outcome, destroyed.rounds, RaidEnding.of(destroyed)), (true, BattleOutcome.hostilesDestroyed, 3, RaidEnding.destroyed));
      expect(raidSummary(destroyed, tick: 1943), [
        'The raiders flew 12x Light Fighter, 2x Cruiser and lost 12x Light Fighter, 2x Cruiser',
        'You lost 5x Rocket Launcher',
        'Rebuilt at once: 3x Rocket Launcher',
        'Wreckage left: 22,800 Fe, 7,800 Cr, 1,200 De, which drifts away in 10:00 unless a fleet docked at home scoops it',
      ]);
      final lost = event('raid_resolved').kind as RaidResolvedEvent;
      expect((RaidEnding.of(lost), raidTaken(lost), raidRebuilt(lost)), (RaidEnding.lost, 'Taken: 2,500,000 Fe, 50 Cr, 25 De', 'Rebuilt at once: 1x Solar Satellite, 4x Rocket Launcher'));
      final undefended = event('raid_resolved__undefended').kind as RaidResolvedEvent;
      expect((undefended.outcome, undefended.rounds, RaidEnding.of(undefended)), (null, 0, RaidEnding.unopposed),
          reason: 'a raid that met nothing was lost without a round');
      expect(
          raidLine(undefended),
          'Raid on your homeworld lost without a fight: nothing stood in its way. The raiders flew 2x Light Fighter and lost nothing. '
          'You lost nothing. Taken: 812 Fe, 407 Cr, 90 De');
      final breached = event('raid_resolved__outpost').kind as RaidResolvedEvent;
      expect(raidLine(breached), allOf(startsWith('Raid on your outpost at 10, -2 lost after 3 rounds: no defender was left standing.'), contains('The outpost is breached')));
    });

    test('full_state carries running and waiting queue items', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final hw = (ServerMessage.fromJson(readJson(f)) as GameState).homeworld;
      expect(hw.buildSlots, 2);
      expect(hw.queueWaitingMax, 3);
      expect(hw.buildQueue.length, 2);
      expect(hw.buildPending.first.waitingFor!.crystal, 120.5);
      expect(hw.buildPending.last.waitingFor, isNull);
      expect(hw.shipyardPending.map((q) => q.item.label), ['Cruiser', 'Rocket Launcher']);
      expect(hw.shipyardPending.first.count, 2);
      expect(hw.researchPending.single.tech, ResearchType.ionTechnology);
    });

    test('the mapper lists waiting items after the running ones and says what they wait for', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final mapper = StateMapper()..apply(ServerMessage.fromJson(readJson(f)));
      final ui = mapper.toUiState();
      expect(ui.buildQueue.map((q) => q.active), [true, true, false, false]);
      expect(ui.buildQueue[2].time, 'short by 121 Cr, starts in about 1:35');
      expect(ui.buildQueue[3].time, 'held by a reserved order, starts in about 1:35');
      expect(ui.shipyard.map((q) => q.name).skip(1), ['Cruiser x2', 'Rocket Launcher x10']);
      expect(ui.shipyard.map((q) => q.time).skip(1), ['paid, waiting for the yard, starts in about 4:00', 'paid, waiting for a prerequisite']);
      expect(ui.research.waiting.single.time, 'short by 1,200 De, starts in about 30:00');
    });

    test('full_state carries the homeworld catalog the server computed', () {
      final f = fixtureFiles('protocol/server_message').firstWhere((f) => baseName(f) == 'full_state');
      final cat = (ServerMessage.fromJson(readJson(f)) as GameState).homeworld.catalog;
      expect(cat.buildings.map((b) => b.buildingType), BuildingType.values);
      expect(cat.research.map((r) => r.tech), ResearchType.values);
      expect(cat.ships.map((s) => s.shipClass), ShipClass.values);
      final yard = cat.buildings.firstWhere((b) => b.buildingType == BuildingType.shipyard);
      expect(yard.next, isNotNull);
      expect(yard.requires.single.label, 'Robotics Factory level 2');
      expect(cat.ships.first.requires.first.label, 'Shipyard level 2');
      expect(cat.defences.map((d) => d.kind), DefenceKind.values);
      expect(cat.buildings.first.effects.map((e) => e.kind), [EffectKind.metalPerTick, EffectKind.energyUsed]);
      final satellite = cat.ships.firstWhere((s) => s.shipClass == ShipClass.solarSatellite);
      expect((satellite.flies, satellite.drive, satellite.jumpTicks), (false, null, 0), reason: 'a Solar Satellite stays at home');
    });
  });

  group('lenient inputs (serde defaults)', () {
    for (final f in fixtureFiles('protocol/lenient')) {
      test(baseName(f), () {
        final doc = readJson(f) as Map<String, dynamic>;
        final input = doc['input'];
        final out = switch (doc['type']) {
          'Command' => Command.fromJson(input).toJson(),
          'ClientMessage' => ClientMessage.fromJson(input).toJson(),
          'ServerMessage' => ServerMessage.fromJson(input).toJson(),
          final t => fail('unknown lenient type $t'),
        };
        expect(jsonEq(wire(out), doc['canonical']), isTrue, reason: 'in ${jsonEncode(input)} out ${jsonEncode(out)}');
      });
    }
  });

  group('enums', () {
    final Map<String, dynamic> enums = readJson(File('${fixturesDir().path}/protocol/enums.json')) as Map<String, dynamic>;

    void check<T>(String name, T Function(Object?) from, Object? Function(T) to) {
      test(name, () {
        final names = enums[name] as List;
        expect(names, isNotEmpty);
        for (final n in names) {
          expect(to(from(n)), n, reason: '$name.$n');
        }
      });
    }

    check('ShipClass', ShipClass.fromJson, (ShipClass e) => e.toJson());
    check('Density', Density.fromJson, (Density e) => e.toJson());
    check('TerrainType', TerrainType.fromJson, (TerrainType e) => e.toJson());
    check('FleetStatus', FleetStatus.fromJson, (FleetStatus e) => e.toJson());
    check('NpcBehavior', NpcBehavior.fromJson, (NpcBehavior e) => e.toJson());
    check('SiteRisk', SiteRisk.fromJson, (SiteRisk e) => e.toJson());
    check('SignalKind', SignalKind.fromJson, (SignalKind e) => e.toJson());
    check('AlertLevel', AlertLevel.fromJson, (AlertLevel e) => e.toJson());
    check('HarvestResource', HarvestResource.fromJson, (HarvestResource e) => e.toJson());
    check('QueueType', QueueType.fromJson, (QueueType e) => e.toJson());
    check('WaitReason', WaitReason.fromJson, (WaitReason e) => e.toJson());
    check('BatchWait', BatchWait.fromJson, (BatchWait e) => e.toJson());
    check('PolicyPreset', PolicyPreset.fromJson, (PolicyPreset e) => e.toJson());
    check('BuildingType', BuildingType.fromJson, (BuildingType e) => e.toJson());
    check('ResearchType', ResearchType.fromJson, (ResearchType e) => e.toJson());
    check('ResourceKind', ResourceKind.fromJson, (ResourceKind e) => e.toJson());
    check('DefenceKind', DefenceKind.fromJson, (DefenceKind e) => e.toJson());
    check('ErrorCode', ErrorCode.fromJson, (ErrorCode e) => e.toJson());
    check('ChartMark', ChartMark.fromJson, (ChartMark e) => e.toJson());
    check('HubService', HubService.fromJson, (HubService e) => e.toJson());
    check('OutpostWork', OutpostWork.fromJson, (OutpostWork e) => e.toJson());

    test('Dart enums have no values Rust lacks', () {
      void same(String name, int dartCount) => expect(dartCount, (enums[name] as List).length, reason: name);
      same('ShipClass', ShipClass.values.length);
      same('Density', Density.values.length);
      same('TerrainType', TerrainType.values.length);
      same('FleetStatus', FleetStatus.values.length);
      same('NpcBehavior', NpcBehavior.values.length);
      same('SiteRisk', SiteRisk.values.length);
      same('SignalKind', SignalKind.values.length);
      same('AlertLevel', AlertLevel.values.length);
      same('HarvestResource', HarvestResource.values.length);
      same('QueueType', QueueType.values.length);
      same('WaitReason', WaitReason.values.length);
      same('BatchWait', BatchWait.values.length);
      same('PolicyPreset', PolicyPreset.values.length);
      same('BuildingType', BuildingType.values.length);
      same('ResearchType', ResearchType.values.length);
      same('ResourceKind', ResourceKind.values.length);
      same('DefenceKind', DefenceKind.values.length);
      same('ErrorCode', ErrorCode.values.length);
      same('ChartMark', ChartMark.values.length);
      same('HubService', HubService.values.length);
      same('OutpostWork', OutpostWork.values.length);
    });

    test('error code numbers are not on the wire but match Rust repr', () {
      expect(ErrorCode.invalidCommand.code, 1000);
      expect(ErrorCode.authFailed.code, 2000);
      expect(ErrorCode.serverError.code, 5000);
    });
  });

  test('unknown tags are rejected, not silently ignored', () {
    expect(() => ServerMessage.fromJson({'type': 'nope'}), throwsFormatException);
    expect(() => Command.fromJson({'action': 'nope'}), throwsFormatException);
    expect(() => EventKind.fromJson({'kind': 'Nope'}), throwsFormatException);
    expect(() => ShipClass.fromJson('Dreadnought'), throwsFormatException);
  });
}
