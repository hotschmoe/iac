import 'package:flutter_test/flutter_test.dart';

import 'package:iac_client/models/hex.dart';
import 'package:iac_client/hex/hex_math.dart';

void main() {
  test('Hex distance calculates correctly', () {
    expect(const Hex(0, 0).distance, 0);
    expect(const Hex(1, 0).distance, 1);
    expect(const Hex(3, -1).distance, 3);
    expect(const Hex(14, -6).distance, 14);
  });

  test('Hex directions produce 6 neighbors', () {
    expect(Hex.directions.length, 6);
    for (final dir in Hex.directions) {
      expect(dir.distance, 1);
    }
  });

  test('hexToPixel returns correct offsets', () {
    final origin = hexToPixel(0, 0, 30);
    expect(origin.dx, 0);
    expect(origin.dy, 0);

    final east = hexToPixel(1, 0, 30);
    expect(east.dx, 45); // 30 * 1.5
  });

  test('mulberry32 is deterministic', () {
    final a = mulberry32(42);
    final b = mulberry32(42);
    expect(a(), b());
    expect(a(), b());
  });

  test('getEdge is symmetric', () {
    expect(getEdge(0, 0, 1, 0), getEdge(1, 0, 0, 0));
    expect(getEdge(5, -3, 6, -3), getEdge(6, -3, 5, -3));
  });
}
