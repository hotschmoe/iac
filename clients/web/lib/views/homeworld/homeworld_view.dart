import 'package:flutter/material.dart';

import '../../models/homeworld.dart';
import '../../protocol/protocol.dart' as proto;
import '../../state/game_controller.dart';
import '../../theme/amber_theme.dart';
import '../../widgets/amber_panel.dart';
import '../../widgets/progress_bar.dart';
import 'homeworld_card.dart';

/// Base management: pick what to build, research or launch from the
/// server's catalog, and watch the three queues.
class HomeworldView extends StatelessWidget {
  final GameController controller;
  const HomeworldView({super.key, required this.controller});

  static const _cardWidth = 250.0;

  @override
  Widget build(BuildContext context) {
    final state = controller.state;
    final catalog = state.catalog;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 4),
          child: Text(
            'HOMEWORLD -- BUILDINGS, SHIPYARD & RESEARCH',
            style: Amber.mono(size: 9, color: Amber.dim).copyWith(letterSpacing: 1),
          ),
        ),
        _tabs(),
        Expanded(
          child: catalog == null
              ? Center(child: Text('No homeworld data yet', style: Amber.mono(size: 11, color: Amber.dim)))
              : LayoutBuilder(builder: (context, c) => _grid(c.maxWidth, catalog)),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(8, 0, 8, 8),
          child: _queues(),
        ),
      ],
    );
  }

  Widget _tabs() {
    final shipyard = controller.hwTab == HomeworldTab.shipyard;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
      child: Wrap(
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: 6,
        runSpacing: 4,
        children: [
          for (final t in HomeworldTab.values) _tab(t),
          if (shipyard) ...[
            const SizedBox(width: 10),
            Text('BATCH', style: Amber.mono(size: 9, color: Amber.dim).copyWith(letterSpacing: 1)),
            for (final n in shipBatches) _batchChip(n),
          ],
          const SizedBox(width: 10),
          Text(
            'ARROWS select  ENTER queue  [ ] tab  +/- batch  DEL cancel',
            style: Amber.mono(size: 9, color: Amber.faint),
          ),
        ],
      ),
    );
  }

  Widget _tab(HomeworldTab t) {
    final active = controller.hwTab == t;
    return GestureDetector(
      key: ValueKey('hw-tab-${t.name}'),
      onTap: () => controller.selectHomeworldTab(t),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
        decoration: BoxDecoration(
          border: Border.all(color: active ? Amber.full : Amber.dim, width: active ? 1 : 0.5),
        ),
        child: Text(
          t.label,
          style: Amber.mono(size: 10, color: active ? Amber.full : Amber.dim).copyWith(letterSpacing: 1),
        ),
      ),
    );
  }

  Widget _batchChip(int n) {
    final active = controller.shipBatch == n;
    return GestureDetector(
      key: ValueKey('hw-batch-$n'),
      onTap: () => controller.setShipBatch(n),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
        decoration: BoxDecoration(
          border: Border.all(color: active ? Amber.full : Amber.dim, width: active ? 1 : 0.5),
        ),
        child: Text('x$n', style: Amber.mono(size: 10, color: active ? Amber.full : Amber.dim)),
      ),
    );
  }

  Widget _grid(double width, proto.HomeworldCatalog catalog) {
    const gap = 6.0;
    final usable = width - 16;
    final cols = ((usable + gap) / (_cardWidth + gap)).floor().clamp(1, 6);
    controller.hwColumns = cols;
    final cardWidth = (usable - gap * (cols - 1)) / cols;

    final cards = _cards(catalog);
    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(8, 4, 8, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _stockStrip(),
          const SizedBox(height: gap),
          Wrap(
            spacing: gap,
            runSpacing: gap,
            children: [
              for (var i = 0; i < cards.length; i++)
                SizedBox(
                  key: ValueKey('hw-card-$i'),
                  width: cardWidth,
                  child: cards[i],
                ),
            ],
          ),
        ],
      ),
    );
  }

  Widget _stockStrip() {
    final r = controller.state.resources;
    Widget stock(String label, int amount, double rate) => RichText(
          text: TextSpan(children: [
            TextSpan(text: '$label ', style: Amber.mono(size: 10, color: Amber.dim)),
            TextSpan(text: '$amount', style: Amber.mono(size: 12, color: Amber.full)),
            TextSpan(text: '  +${rate.toStringAsFixed(1)}/t', style: Amber.mono(size: 10, color: Amber.dim)),
          ]),
        );
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      decoration: BoxDecoration(color: Amber.bgPanel, border: Border.all(color: Amber.border)),
      child: Wrap(
        spacing: 24,
        runSpacing: 4,
        children: [
          stock('METAL', r.metal.amount, r.metal.rate),
          stock('CRYSTAL', r.crystal.amount, r.crystal.rate),
          stock('DEUT', r.deut.amount, r.deut.rate),
        ],
      ),
    );
  }

  List<Widget> _cards(proto.HomeworldCatalog catalog) {
    final stock = controller.state.stock;
    HomeworldCard card(int i, {
      required String title,
      required String badge,
      String? subtitle,
      required proto.Resources? cost,
      String? time,
      required List<proto.Requirement> requires,
      bool maxed = false,
    }) =>
        HomeworldCard(
          title: title,
          badge: badge,
          subtitle: subtitle,
          cost: cost,
          time: time,
          requires: requires,
          stock: stock,
          selected: controller.hwCursor == i,
          maxed: maxed,
          onTap: () => controller.activateHomeworldCard(i),
        );

    switch (controller.hwTab) {
      case HomeworldTab.buildings:
        return [
          for (var i = 0; i < catalog.buildings.length; i++)
            card(
              i,
              title: catalog.buildings[i].buildingType.label,
              badge: catalog.buildings[i].next == null
                  ? 'MAX ${catalog.buildings[i].level}'
                  : 'Lv ${catalog.buildings[i].level}',
              subtitle: catalog.buildings[i].next == null ? 'fully upgraded' : 'upgrade to Lv ${catalog.buildings[i].next!.level}',
              cost: catalog.buildings[i].next?.cost,
              time: _ticks(catalog.buildings[i].next?.ticks),
              requires: catalog.buildings[i].requires,
              maxed: catalog.buildings[i].next == null,
            ),
        ];
      case HomeworldTab.research:
        return [
          for (var i = 0; i < catalog.research.length; i++)
            card(
              i,
              title: catalog.research[i].tech.label,
              badge: catalog.research[i].next == null
                  ? 'MAX ${catalog.research[i].level}'
                  : 'Lv ${catalog.research[i].level}/${catalog.research[i].maxLevel}',
              subtitle: catalog.research[i].next == null ? 'fully researched' : 'research Lv ${catalog.research[i].next!.level}',
              cost: catalog.research[i].next?.cost,
              time: _ticks(catalog.research[i].next?.ticks),
              requires: catalog.research[i].requires,
              maxed: catalog.research[i].next == null,
            ),
        ];
      case HomeworldTab.shipyard:
        final n = controller.shipBatch;
        return [
          for (var i = 0; i < catalog.ships.length; i++)
            card(
              i,
              title: catalog.ships[i].shipClass.label,
              badge: 'x$n',
              subtitle: n == 1 ? 'one ship' : '$n ships, built one by one',
              cost: _scale(catalog.ships[i].unitCost, n),
              time: '${catalog.ships[i].ticksPerShip * n} ticks (${catalog.ships[i].ticksPerShip} each)',
              requires: catalog.ships[i].requires,
            ),
        ];
    }
  }

  static proto.Resources _scale(proto.Resources r, int n) =>
      proto.Resources(metal: r.metal * n, crystal: r.crystal * n, deuterium: r.deuterium * n);

  static String? _ticks(int? t) => t == null ? null : '$t ticks';

  // ── Queues + log ──────────────────────────────────────────────

  Widget _queues() {
    final s = controller.state;
    final log = s.events.take(2).toList();
    return AmberPanel(
      title: 'QUEUES',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _queueRow('BUILD', s.buildQueue.firstOrNull, proto.QueueType.building),
          _queueRow('SHIPS', s.shipyard.firstOrNull, proto.QueueType.ship),
          _queueRow(
            'RESEARCH',
            s.research.name == 'Idle'
                ? null
                : QueueItem(name: s.research.name, time: s.research.time, pct: s.research.pct, active: true),
            proto.QueueType.research,
          ),
          if (log.isNotEmpty) const SizedBox(height: 6),
          for (var i = 0; i < log.length; i++)
            Text(
              log[i].message,
              key: ValueKey('hw-log-$i'),
              overflow: TextOverflow.ellipsis,
              style: Amber.mono(size: 10, color: log[i].level.color),
            ),
        ],
      ),
    );
  }

  Widget _queueRow(String label, QueueItem? item, proto.QueueType queue) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          SizedBox(width: 70, child: Text(label, style: Amber.mono(size: 10, color: Amber.dim))),
          if (item == null)
            Expanded(child: Text('idle', style: Amber.mono(size: 10, color: Amber.faint)))
          else ...[
            Expanded(
              flex: 3,
              child: Text(
                item.name,
                overflow: TextOverflow.ellipsis,
                style: Amber.mono(size: 10, color: Amber.bright),
              ),
            ),
            SizedBox(width: 52, child: Text(item.time, style: Amber.mono(size: 10, color: Amber.full))),
            Expanded(flex: 2, child: AmberProgressBar(fraction: item.pct / 100)),
            const SizedBox(width: 8),
            GestureDetector(
              key: ValueKey('hw-cancel-${queue.name}'),
              onTap: () => controller.cancelHomeworldQueue(queue),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
                decoration: BoxDecoration(border: Border.all(color: Amber.dim, width: 0.5)),
                child: Text('CANCEL', style: Amber.mono(size: 9, color: Amber.normal)),
              ),
            ),
          ],
        ],
      ),
    );
  }
}
