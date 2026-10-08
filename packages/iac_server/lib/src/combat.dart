/// Simplified stochastic combat resolution.
library;

import 'dart:math' as math;

import 'package:iac_shared/iac_shared.dart';

import 'entities.dart';

class CombatRoundResult {
  final List<GameEvent> events;
  final bool concluded;
  final bool playerWon;

  const CombatRoundResult({
    required this.events,
    required this.concluded,
    required this.playerWon,
  });
}

class _ShipRef {
  final Ship ship;
  final int fleetId;
  final bool isNpc;

  _ShipRef(this.ship, this.fleetId, this.isNpc);
}

/// One combat tick: every living ship fires at a random living enemy.
CombatRoundResult resolveCombatRound({
  required Combat activeCombat,
  required List<Fleet> playerFleets,
  required List<NpcFleet> npcFleets,
  required int tick,
  math.Random? random,
}) {
  activeCombat.round++;
  final rng = random ?? math.Random();
  final events = <GameEvent>[];

  final npcTargets = <_ShipRef>[];
  for (final npc in npcFleets) {
    for (final ship in npc.ships) {
      if (ship.hull > 0) {
        npcTargets.add(_ShipRef(ship, npc.id, true));
      }
    }
  }

  final playerTargets = <_ShipRef>[];
  for (final pf in playerFleets) {
    for (final ship in pf.ships) {
      if (ship.hull > 0) {
        playerTargets.add(_ShipRef(ship, pf.id, false));
      }
    }
  }

  for (final pf in playerFleets) {
    for (final attacker in pf.ships) {
      if (attacker.hull <= 0) continue;
      _fireShip(attacker, npcTargets, tick, rng, events);
    }
  }

  for (final npc in npcFleets) {
    for (final attacker in npc.ships) {
      if (attacker.hull <= 0) continue;
      _fireShip(attacker, playerTargets, tick, rng, events);
    }
  }

  // Compact destroyed ships
  for (final pf in playerFleets) {
    pf.ships.removeWhere((s) => s.hull <= 0);
  }
  for (final npc in npcFleets) {
    npc.ships.removeWhere((s) => s.hull <= 0);
  }

  final anyPlayerAlive = playerFleets.any((pf) => pf.ships.isNotEmpty);
  final anyNpcAlive = npcFleets.any((npc) => npc.ships.isNotEmpty);
  final concluded = !anyPlayerAlive || !anyNpcAlive;

  if (concluded) {
    if (!anyNpcAlive) {
      for (final npc in npcFleets) {
        events.add(GameEvent.fleetDestroyed(
          tick: tick,
          fleetId: npc.id,
          isNpc: true,
          salvage: activeCombat.npcValue,
        ));
      }
    }
    for (final pf in playerFleets) {
      if (pf.ships.isEmpty) {
        events.add(GameEvent.fleetDestroyed(
          tick: tick,
          fleetId: pf.id,
          isNpc: false,
        ));
      }
    }
  }

  return CombatRoundResult(
    events: events,
    concluded: concluded,
    playerWon: concluded && anyPlayerAlive,
  );
}

void _fireShip(
  Ship attacker,
  List<_ShipRef> targets,
  int tick,
  math.Random rng,
  List<GameEvent> events,
) {
  // Simple: one shot per tick (rapid-fire optional extra shot)
  var shots = 0;
  const maxShots = 3;
  while (shots < maxShots) {
    shots++;
    final targetRef = _selectTarget(targets, rng);
    if (targetRef == null) return;
    final target = targetRef.ship;

    final damage = _rollDamage(attacker.weaponPower, rng);
    final result = _applyDamage(target, damage);
    final rapid = _checkRapidFire(attacker.class_, target.class_, rng);

    events.add(GameEvent.combatRound(
      tick: tick,
      attackerShipId: attacker.id,
      targetShipId: target.id,
      damage: damage,
      shieldAbsorbed: result.$1,
      hullDamage: result.$2,
      rapidFire: rapid,
    ));

    if (target.hull <= 0) {
      events.add(GameEvent.shipDestroyed(
        tick: tick,
        shipId: target.id,
        shipClass: target.class_,
        ownerFleetId: targetRef.fleetId,
        isNpc: targetRef.isNpc,
      ));
    }

    if (!rapid) break;
  }
}

_ShipRef? _selectTarget(List<_ShipRef> targets, math.Random rng) {
  var totalWeight = 0.0;
  for (final ref in targets) {
    if (ref.ship.hull > 0) totalWeight += ref.ship.hullMax;
  }
  if (totalWeight <= 0) return null;

  var roll = rng.nextDouble() * totalWeight;
  for (final ref in targets) {
    if (ref.ship.hull <= 0) continue;
    roll -= ref.ship.hullMax;
    if (roll <= 0) return ref;
  }
  // Fallback
  for (var i = targets.length - 1; i >= 0; i--) {
    if (targets[i].ship.hull > 0) return targets[i];
  }
  return null;
}

double _rollDamage(double weaponPower, math.Random rng) {
  final variance = damageVarianceMin +
      rng.nextDouble() * (damageVarianceMax - damageVarianceMin);
  return weaponPower * variance;
}

(double, double) _applyDamage(Ship target, double damage) {
  final shieldAbsorbed = math.min(damage, target.shield);
  target.shield -= shieldAbsorbed;
  final passthrough = damage - shieldAbsorbed;
  final hullDamage = math.min(passthrough, target.hull);
  target.hull -= hullDamage;
  return (shieldAbsorbed, hullDamage);
}

bool _checkRapidFire(ShipClass attacker, ShipClass target, math.Random rng) {
  final rf = attacker.rapidFireVs(target);
  if (rf <= 0) return false;
  // rf = N means N:1 chance-ish; use 1 - 1/N probability of extra shot
  return rng.nextDouble() < (1.0 - 1.0 / rf);
}
