import 'package:flutter/foundation.dart';

import '../models/fleet.dart' as ui;
import '../protocol/protocol.dart' as proto;
import '../state/game_controller.dart';
import 'route_planner.dart';
import 'threat.dart';
import 'toasts.dart';

enum RunPhase { idle, running, held }

/// Executes a multi-hop route client-side, one move per idle tick (as the
/// TUI does): the server only knows single-hop moves.
class RouteRunner extends ChangeNotifier {
  RouteRunner(this.ctrl, this.toasts) {
    ctrl.addListener(_onCtrl);
  }

  final GameController ctrl;
  final ToastController toasts;

  RunPhase phase = RunPhase.idle;
  int? fleetId;
  List<proto.Hex> remaining = [];
  List<proto.Hex> full = [];
  bool holdEnabled = true;
  bool holdConfirmed = false;
  proto.Hex? pendingTarget;
  int _lastTick = -1;
  int _sentTick = -1;

  bool get active => phase != RunPhase.idle;
  proto.Hex? get heldBefore => phase == RunPhase.held && remaining.isNotEmpty ? remaining.first : null;
  int get done => full.length - remaining.length;

  void start(RoutePlan plan, int fleet, {bool hold = true}) {
    fleetId = fleet;
    full = List.of(plan.path);
    remaining = List.of(plan.path);
    holdEnabled = hold;
    holdConfirmed = false;
    pendingTarget = null;
    phase = RunPhase.running;
    _lastTick = -1;
    toasts.show('Route engaged', '${plan.hops} hops to ${plan.to}', tone: ToastTone.teal);
    notifyListeners();
    _onCtrl();
  }

  void abort([String? why]) {
    if (!active) return;
    phase = RunPhase.idle;
    remaining = [];
    full = [];
    pendingTarget = null;
    final id = fleetId;
    if (why != null) toasts.show('Route aborted', why, tone: ToastTone.red);
    if (id != null && why == null) ctrl.sendCommand(proto.StopCommand(fleetId: id));
    notifyListeners();
  }

  void continueHold() {
    if (phase != RunPhase.held) return;
    holdConfirmed = true;
    phase = RunPhase.running;
    notifyListeners();
    _lastTick = -1;
    _onCtrl();
  }

  void _onCtrl() {
    if (phase == RunPhase.idle) return;
    final st = ctrl.state;
    if (st.tick == _lastTick) return;
    _lastTick = st.tick;
    ui.FleetState? f;
    for (final x in st.fleets) {
      if (x.id == fleetId) f = x;
    }
    if (f == null) {
      abort('fleet lost');
      return;
    }
    while (remaining.isNotEmpty && remaining.first == f.sector) {
      remaining.removeAt(0);
      holdConfirmed = false;
      pendingTarget = null;
    }
    if (remaining.isEmpty) {
      phase = RunPhase.idle;
      toasts.show('Route complete', '${f.name} at ${f.sector}', tone: ToastTone.teal);
      notifyListeners();
      return;
    }
    if (f.status == ui.FleetStatus.combat) {
      abort('combat engaged');
      return;
    }
    final next = remaining.first;
    if (proto.Hex.distance(f.sector, next) != 1) {
      abort('fleet left the planned course');
      return;
    }
    if (f.status != ui.FleetStatus.idle && f.status != ui.FleetStatus.docked) return;
    if (f.cooldown > 0) return;
    if (st.tick - _sentTick < 2 && pendingTarget == next) return;
    if (holdEnabled && !holdConfirmed && SectorThreat.of(st.sectors[next]).rating >= holdThreat) {
      if (phase != RunPhase.held) {
        phase = RunPhase.held;
        toasts.show('Route held', 'Threat ahead at $next. Continue, reroute or abort.', tone: ToastTone.red);
        notifyListeners();
      }
      return;
    }
    if (f.fuel < f.jumpFuel) {
      abort('out of fuel');
      return;
    }
    pendingTarget = next;
    _sentTick = st.tick;
    ctrl.sendCommand(proto.MoveCommand(fleetId: f.id, target: next));
    notifyListeners();
  }

  @override
  void dispose() {
    ctrl.removeListener(_onCtrl);
    super.dispose();
  }
}
