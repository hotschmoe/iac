import 'package:flutter/material.dart';

import '../../models/fleet.dart';
import '../../state/game_controller.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';

/// Split ships off the active fleet and merge in fleets that share its sector.
/// Docking never merges fleets, so this is where the roster is managed.
class FleetManagePanel extends StatefulWidget {
  final GameController controller;
  const FleetManagePanel({super.key, required this.controller});

  @override
  State<FleetManagePanel> createState() => _FleetManagePanelState();
}

class _FleetManagePanelState extends State<FleetManagePanel> {
  final Map<String, int> _pick = {};
  int? _forFleet;

  GameController get c => widget.controller;

  @override
  Widget build(BuildContext context) {
    final fleet = c.currentFleet;
    if (_forFleet != fleet.id) {
      _forFleet = fleet.id;
      _pick.clear();
    }
    for (final g in fleet.ships) {
      _pick.update(g.shipClass, (n) => n.clamp(0, g.count), ifAbsent: () => 0);
    }
    final picked = _pick.values.fold<int>(0, (a, b) => a + b);
    final canSplit = picked > 0 && picked < fleet.shipCount;
    final partners = c.mergeCandidates;

    return AmberPanel(
      title: 'FLEET MANAGEMENT',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('Split off', style: Amber.mono(size: 11, color: Amber.dim)),
          for (final g in fleet.ships) _pickRow(g),
          const SizedBox(height: 4),
          _button(
            key: const ValueKey('fm-split'),
            label: picked == 0 ? 'SPLIT OFF' : 'SPLIT OFF $picked',
            enabled: canSplit,
            onTap: () => _split(fleet),
          ),
          if (picked > 0 && !canSplit)
            Text('one ship must stay', style: Amber.mono(size: 10, color: Amber.danger)),
          const SizedBox(height: 10),
          Text('Merge in (same sector)', style: Amber.mono(size: 11, color: Amber.dim)),
          if (partners.isEmpty)
            Text('none here', style: Amber.mono(size: 11, color: Amber.faint))
          else
            for (final p in partners) _mergeRow(p),
        ],
      ),
    );
  }

  Widget _pickRow(ShipState g) {
    final n = _pick[g.shipClass] ?? 0;
    return Padding(
      padding: const EdgeInsets.only(top: 3),
      child: Row(
        children: [
          Expanded(
            child: Text('${g.shipClass} x${g.count}', style: Amber.mono(size: 11, color: Amber.normal)),
          ),
          _chip(
            key: ValueKey('fm-minus-${g.shipClass}'),
            label: '-',
            enabled: n > 0,
            onTap: () => setState(() => _pick[g.shipClass] = n - 1),
          ),
          SizedBox(
            width: 24,
            child: Text('$n', textAlign: TextAlign.center, style: Amber.mono(size: 11, color: Amber.full)),
          ),
          _chip(
            key: ValueKey('fm-plus-${g.shipClass}'),
            label: '+',
            enabled: n < g.count,
            onTap: () => setState(() => _pick[g.shipClass] = n + 1),
          ),
        ],
      ),
    );
  }

  Widget _mergeRow(FleetState p) {
    return Padding(
      padding: const EdgeInsets.only(top: 3),
      child: Row(
        children: [
          Expanded(
            child: Text('${p.name} - ${p.shipCount} ship${p.shipCount == 1 ? '' : 's'}',
                style: Amber.mono(size: 11, color: Amber.normal)),
          ),
          _chip(
            key: ValueKey('fm-merge-${p.id}'),
            label: 'MERGE',
            enabled: true,
            onTap: () => c.mergeFleet(p.id),
          ),
        ],
      ),
    );
  }

  void _split(FleetState fleet) {
    final ids = <int>[];
    for (final g in fleet.ships) {
      ids.addAll(g.ids.take(_pick[g.shipClass] ?? 0));
    }
    setState(_pick.clear);
    c.splitShipIds(ids);
  }

  Widget _chip({required Key key, required String label, required bool enabled, required VoidCallback onTap}) {
    return GestureDetector(
      key: key,
      onTap: enabled ? onTap : null,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 1),
        decoration: BoxDecoration(border: Border.all(color: enabled ? Amber.normal : Amber.faint, width: 0.5)),
        child: Text(label, style: Amber.mono(size: 11, color: enabled ? Amber.full : Amber.faint)),
      ),
    );
  }

  Widget _button({required Key key, required String label, required bool enabled, required VoidCallback onTap}) {
    return GestureDetector(
      key: key,
      onTap: enabled ? onTap : null,
      child: Container(
        alignment: Alignment.center,
        padding: const EdgeInsets.symmetric(vertical: 4),
        decoration: BoxDecoration(border: Border.all(color: enabled ? Amber.full : Amber.faint)),
        child: Text(label, style: Amber.mono(size: 11, color: enabled ? Amber.full : Amber.faint).copyWith(letterSpacing: 1)),
      ),
    );
  }
}
