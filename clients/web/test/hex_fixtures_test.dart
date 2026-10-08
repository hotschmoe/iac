// Dart hex math vs. the Rust-generated results in fixtures/hex.
// Rings are compared cell-by-cell IN ORDER (and checked for membership),
// which catches the class of bug where a ring walk drifts off the ring.

import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:iac_client/protocol/hex.dart';

import 'fixtures_util.dart';

Hex hx(Object? j) => Hex.fromJson(j);
List<Hex> hexes(Object? j) => [for (final e in j as List) hx(e)];

List<dynamic> load(String name) => readJson(File('${fixturesDir().path}/hex/$name.json')) as List<dynamic>;

void main() {
  test('directions match Rust order, vectors and opposites', () {
    final dirs = load('directions');
    expect(HexDirection.values.length, dirs.length);
    for (var i = 0; i < dirs.length; i++) {
      final d = HexDirection.values[i];
      expect(d.label, dirs[i]['label']);
      expect(d.vec, hx(dirs[i]['vec']));
      expect(d.opposite.label, dirs[i]['opposite']);
    }
  });

  test('neighbors', () {
    for (final c in load('neighbors')) {
      expect(hx(c['center']).neighbors(), hexes(c['neighbors']), reason: 'center ${c['center']}');
    }
  });

  test('distance', () {
    for (final c in load('distance')) {
      final a = hx(c['a']), b = hx(c['b']);
      expect(Hex.distance(a, b), c['distance']);
      expect(Hex.distance(b, a), c['distance']);
    }
  });

  test('s, distance from origin and packed key (incl. negatives)', () {
    for (final c in load('origin_and_key')) {
      final h = hx(c['hex']);
      expect(h.s, c['s']);
      expect(h.distFromOrigin, c['dist_from_origin']);
      expect(h.toKey(), c['key']);
      expect(Hex.fromKey(c['key'] as int), h);
    }
  });

  group('ring', () {
    for (final c in load('ring')) {
      final center = hx(c['center']);
      final radius = c['radius'] as int;
      test('center $center radius $radius', () {
        final cells = hexRing(center, radius);
        // Ordered, exact match with Rust.
        expect(cells, hexes(c['cells']));
        // And independently: every cell sits exactly on the ring, no
        // duplicates, contiguous, covering every side (incl. west).
        expect(cells.length, radius == 0 ? 1 : radius * 6);
        expect(cells.toSet().length, cells.length);
        for (final h in cells) {
          expect(Hex.distance(center, h), radius, reason: 'off-ring cell $h');
        }
        if (radius > 0) {
          for (var i = 0; i < cells.length; i++) {
            expect(Hex.distance(cells[i], cells[(i + 1) % cells.length]), 1, reason: 'gap after ${cells[i]}');
          }
          expect(cells.any((h) => h.q < center.q && h.r == center.r), isTrue, reason: 'west side missing');
        }
      });
    }
  });

  group('disc (spiral)', () {
    for (final c in load('disc')) {
      final center = hx(c['center']);
      final radius = c['radius'] as int;
      test('center $center radius $radius', () {
        final cells = hexSpiral(center, radius);
        expect(cells, hexes(c['cells']));
        final set = cells.toSet();
        expect(set.length, cells.length);
        for (var dq = -radius; dq <= radius; dq++) {
          for (var dr = -radius; dr <= radius; dr++) {
            final h = Hex(center.q + dq, center.r + dr);
            expect(set.contains(h), Hex.distance(center, h) <= radius, reason: '$h');
          }
        }
      });
    }
  });
}
