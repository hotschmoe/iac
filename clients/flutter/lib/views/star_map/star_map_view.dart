import 'package:flutter/material.dart';

import '../../state/game_controller.dart';
import '../../theme/amber_theme.dart';
import 'star_map_painter.dart';
import 'star_map_sidebar.dart';

class StarMapView extends StatelessWidget {
  final GameController controller;
  const StarMapView({super.key, required this.controller});

  @override
  Widget build(BuildContext context) {
    final state = controller.state;
    final fleet = state.fleets[controller.activeFleet];

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 4),
          child: Text(
            'STAR MAP -- STRATEGIC OVERVIEW & WAYPOINT NAVIGATION',
            style: Amber.mono(size: 9, color: Amber.dim).copyWith(
              letterSpacing: 1,
            ),
          ),
        ),
        _buildToolbar(),
        Expanded(
          child: Row(
            children: [
              Expanded(
                child: Container(
                  decoration: BoxDecoration(
                    color: Amber.bgInset,
                    border: const Border(
                      right: BorderSide(color: Amber.border),
                    ),
                  ),
                  child: ClipRect(
                    child: CustomPaint(
                      painter: StarMapPainter(
                        zoom: controller.mapZoom,
                        centerQ: fleet.sector.q + controller.mapOffsetX,
                        centerR: fleet.sector.r + controller.mapOffsetY,
                        fleets: state.fleets,
                        activeFleet: controller.activeFleet,
                        cursorHex: controller.cursorHex,
                        homeworld: state.homeworld,
                        waypoints: state.waypoints,
                      ),
                      child: const SizedBox.expand(),
                    ),
                  ),
                ),
              ),
              SizedBox(
                width: 200,
                child: StarMapSidebar(controller: controller),
              ),
            ],
          ),
        ),
        Container(
          decoration: const BoxDecoration(
            border: Border(top: BorderSide(color: Amber.border)),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 5),
          child: Text(
            'nav: ARROWS scroll | +/- zoom | ENTER waypoint | HOME center | TAB cycle | ESC close',
            style: Amber.mono(size: 9, color: Amber.dim),
          ),
        ),
      ],
    );
  }

  Widget _buildToolbar() {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 6),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: Amber.border)),
      ),
      child: Row(
        children: [
          Text(
            'ZOOM:',
            style: Amber.mono(size: 9, color: Amber.dim).copyWith(
              letterSpacing: 1,
            ),
          ),
          const SizedBox(width: 8),
          _zoomBtn('CLOSE', MapZoom.close),
          const SizedBox(width: 4),
          _zoomBtn('SECTOR', MapZoom.sector),
          const SizedBox(width: 4),
          _zoomBtn('REGION', MapZoom.region),
          const Spacer(),
          Text(
            'SCROLL: ARROWS | WAYPOINT: ENTER | HOME: CENTER',
            style: Amber.mono(size: 9, color: Amber.dim).copyWith(
              letterSpacing: 1,
            ),
          ),
        ],
      ),
    );
  }

  Widget _zoomBtn(String label, MapZoom zoom) {
    final active = controller.mapZoom == zoom;
    return GestureDetector(
      onTap: () => controller.setZoom(zoom),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        decoration: BoxDecoration(
          border: Border.all(
            color: active ? Amber.full : Amber.dim,
          ),
        ),
        child: Text(
          label,
          style: Amber.mono(
            size: 9,
            color: active ? Amber.full : Amber.dim,
          ).copyWith(letterSpacing: 1),
        ),
      ),
    );
  }
}
