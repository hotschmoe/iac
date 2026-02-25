import 'package:flutter/material.dart';

import '../../hex/hex_math.dart';
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
    final dist = hexDist(fleet.sector.q, fleet.sector.r);
    final zone = dist < 8 ? 'Inner Ring' : dist < 20 ? 'Outer Ring' : 'The Wandering';

    return SingleChildScrollView(
      child: Column(
        children: [
          _fleetPanel(zone),
          _cargoPanel(),
          _sectorPanel(),
        ],
      ),
    );
  }

  Widget _fleetPanel(String zone) {
    return AmberPanel(
      title: 'FLEET ${fleet.name.toUpperCase()} -- ACTIVE',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _row('Sector', '${fleet.sector}', Amber.full),
          _row('Zone', zone, Amber.normal),
          _row('Status', fleet.status.label, Amber.full),
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
                _row('Fe', '${fleet.cargo.metal}', Amber.normal),
                _row('Cr', '${fleet.cargo.crystal}', Amber.normal),
                _row('De', '${fleet.cargo.deut}', Amber.normal),
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
          _row('Terrain', sector.terrain, Amber.normal),
          _row('Metal', sector.metal, Amber.bright),
          _row('Crystal', sector.crystal, Amber.normal),
          _row('Deut', sector.deut, Amber.dim),
          const SizedBox(height: 6),
          _row('Hostile', sector.hostile, Amber.dim),
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
          _row('Exits', sector.exits, Amber.normal),
        ],
      ),
    );
  }

  Widget _row(String label, String value, Color valueColor) {
    return RichText(
      text: TextSpan(children: [
        TextSpan(
          text: label.padRight(9),
          style: Amber.mono(size: 11, color: Amber.dim),
        ),
        TextSpan(
          text: value,
          style: Amber.mono(size: 11, color: valueColor),
        ),
      ]),
    );
  }
}
