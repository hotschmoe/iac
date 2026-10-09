import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/console/services.dart';

import 'support/harness.dart';

void main() {
  testWidgets('shell shot', (tester) async {
    final c = await pumpShell(tester, screen: Screen.overview);
    await shot(tester, 'shell-overview');
    await endShell(tester, c);
  });
}
