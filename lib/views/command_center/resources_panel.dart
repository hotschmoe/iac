import 'package:flutter/material.dart';

import '../../models/resources.dart';
import '../../widgets/amber_panel.dart';
import '../../widgets/resource_bar.dart';

class ResourcesPanel extends StatelessWidget {
  final Resources resources;
  const ResourcesPanel({super.key, required this.resources});

  @override
  Widget build(BuildContext context) {
    return AmberPanel(
      title: 'RESOURCES',
      child: Column(
        children: [
          ResourceBar(
            label: 'Metal',
            value: resources.metal.amount,
            rate: resources.metal.rate,
            fraction: resources.metal.fraction,
          ),
          ResourceBar(
            label: 'Crystal',
            value: resources.crystal.amount,
            rate: resources.crystal.rate,
            fraction: resources.crystal.fraction,
          ),
          ResourceBar(
            label: 'Deut',
            value: resources.deut.amount,
            rate: resources.deut.rate,
            fraction: resources.deut.fraction,
          ),
        ],
      ),
    );
  }
}
