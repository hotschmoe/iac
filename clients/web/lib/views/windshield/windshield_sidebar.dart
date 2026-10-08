import 'package:flutter/material.dart';

import '../../models/fleet.dart';
import '../../models/game_state.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';

class WindshieldSidebar extends StatelessWidget {
  final FleetState fleet;
  final SectorInfo sector;

  const WindshieldSidebar({
    super.key,
    required this.fleet,
    required this.sector,
  });

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      child: Column(
        children: [
          _fleetPanel(),
          _cargoPanel(),
          _sectorPanel(),
        ],
      ),
    );
  }

  Widget _fleetPanel() {
    return AmberPanel(
      title: 'FLEET ${fleet.name.toUpperCase()} -- ACTIVE',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          LabeledRow(label: 'Sector', value: '${fleet.sector}', valueColor: Amber.full),
          LabeledRow(label: 'Zone', value: fleet.sector.zone, valueColor: Amber.normal),
          LabeledRow(label: 'Status', value: fleet.status.label, valueColor: Amber.full),
          const SizedBox(height: 6),
          Text('Ships', style: Amber.mono(size: 11, color: Amber.dim)),
          for (final ship in fleet.ships) _shipRow(ship),
        ],
      ),
    );
  }

  Widget _shipRow(ShipState ship) {
    final bars = (ship.hullFraction * 5).round();
    final empty = 5 - bars;
    return Padding(
      padding: const EdgeInsets.only(left: 4),
      child: RichText(
        text: TextSpan(
          style: Amber.mono(size: 11),
          children: [
            TextSpan(
              text: '${ship.shipClass.padRight(9)}x${ship.count} ',
              style: Amber.mono(size: 11, color: Amber.normal),
            ),
            TextSpan(
              text: 'hull',
              style: Amber.mono(size: 11, color: Amber.dim),
            ),
            TextSpan(
              text: String.fromCharCodes(List.filled(bars, 0x2588)),
              style: Amber.mono(size: 11, color: Amber.full),
            ),
            TextSpan(
              text: String.fromCharCodes(List.filled(empty, 0x2591)),
              style: Amber.mono(size: 11, color: Amber.faint),
            ),
          ],
        ),
      ),
    );
  }

  Widget _cargoPanel() {
    final fuelBars = (fleet.fuel / fleet.fuelMax * 10).round();
    return AmberPanel(
      title: 'CARGO & FUEL',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          RichText(
            text: TextSpan(children: [
              TextSpan(text: 'Cargo ', style: Amber.mono(size: 11, color: Amber.dim)),
              TextSpan(
                  text: '${fleet.cargo.total}',
                  style: Amber.mono(size: 11, color: Amber.normal)),
              TextSpan(
                  text: '/${fleet.cargo.capacity}',
                  style: Amber.mono(size: 11, color: Amber.dim)),
            ]),
          ),
          Padding(
            padding: const EdgeInsets.only(left: 4),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                LabeledRow(label: 'Fe', value: '${fleet.cargo.metal}', valueColor: Amber.normal),
                LabeledRow(label: 'Cr', value: '${fleet.cargo.crystal}', valueColor: Amber.normal),
                LabeledRow(label: 'De', value: '${fleet.cargo.deut}', valueColor: Amber.normal),
              ],
            ),
          ),
          const SizedBox(height: 6),
          RichText(
            text: TextSpan(children: [
              TextSpan(text: 'Fuel  ', style: Amber.mono(size: 11, color: Amber.dim)),
              TextSpan(
                  text: '${fleet.fuel}',
                  style: Amber.mono(size: 11, color: Amber.bright)),
              TextSpan(
                  text: '/${fleet.fuelMax}',
                  style: Amber.mono(size: 11, color: Amber.dim)),
            ]),
          ),
          Padding(
            padding: const EdgeInsets.only(left: 4),
            child: RichText(
              text: TextSpan(children: [
                TextSpan(
                  text: String.fromCharCodes(List.filled(fuelBars, 0x2588)),
                  style: Amber.mono(size: 11, color: Amber.full),
                ),
                TextSpan(
                  text: String.fromCharCodes(List.filled(10 - fuelBars, 0x2591)),
                  style: Amber.mono(size: 11, color: Amber.faint),
                ),
              ]),
            ),
          ),
          Padding(
            padding: const EdgeInsets.only(left: 4),
            child: Text(
              '~${fleet.fuel ~/ 15} jumps remaining',
              style: Amber.mono(size: 11, color: Amber.dim),
            ),
          ),
        ],
      ),
    );
  }

  Widget _sectorPanel() {
    return AmberPanel(
      title: 'SECTOR SCAN',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          LabeledRow(label: 'Terrain', value: sector.terrain, valueColor: Amber.normal),
          LabeledRow(label: 'Metal', value: sector.metal, valueColor: Amber.bright),
          LabeledRow(label: 'Crystal', value: sector.crystal, valueColor: Amber.normal),
          LabeledRow(label: 'Deut', value: sector.deut, valueColor: Amber.dim),
          const SizedBox(height: 6),
          LabeledRow(label: 'Hostile', value: sector.hostile, valueColor: Amber.dim),
          RichText(
            text: TextSpan(children: [
              TextSpan(
                  text: '! Adjacent: ',
                  style: Amber.mono(size: 11, color: Amber.bright)),
              TextSpan(
                  text: sector.adjacent,
                  style: Amber.mono(size: 11, color: Amber.bright)),
            ]),
          ),
          const SizedBox(height: 6),
          LabeledRow(label: 'Exits', value: sector.exits, valueColor: Amber.normal),
        ],
      ),
    );
  }
}
