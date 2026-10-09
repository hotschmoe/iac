import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/economy_view.dart';
import 'package:iac_client/state/game_controller.dart';

void main() {
  test('economy view maps the server economy blocks (demo feeds the real protocol)', () {
    final c = GameController()..startDemo();
    addTearDown(() {
      c.stop();
      c.dispose();
    });
    final e = EconomyView.of(c.state);
    expect(e.world, isNotNull);
    expect(e.world!.label, contains('x1'));
    final st = e.storage!;
    expect(st.cap['metal'], greaterThan(0));
    expect(st.protected['metal'], lessThan(st.cap['metal']!));
    expect(st.vaultLevel, greaterThan(0));
    final sl = e.slots!;
    expect(sl.maxDepth, greaterThan(0));
    expect(sl.slotBUnlocked, isFalse);
    expect(sl.slotBRequirement, startsWith('Needs Modular Fabrication'));
    expect(sl.queued, isNotEmpty);
    expect(sl.queued.first.waits, anyOf(isEmpty, startsWith('waits: ')));
    final d = e.defence!;
    expect(d.rows, isNotEmpty);
    expect(d.total, greaterThanOrEqualTo(d.structures));
    expect(d.raidEstimate, greaterThan(0));
  });

  test('fleet power comes from the server field', () {
    final c = GameController()..startDemo();
    addTearDown(() {
      c.stop();
      c.dispose();
    });
    expect(c.state.fleets.every((f) => f.power > 0), isTrue);
  });

  test('without a homeworld snapshot the view is empty but safe', () {
    final c = GameController();
    addTearDown(c.dispose);
    final e = EconomyView.of(c.state);
    expect(e.storage, isNull);
    expect(e.slots, isNull);
    expect(e.defence, isNull);
  });
}
