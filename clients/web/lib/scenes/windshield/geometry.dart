import 'dart:math' as math;
import 'dart:ui';

import '../../protocol/protocol.dart' as proto;

/// Screen-space compass of the windshield: key, vector, bearing label.
/// Directions are named by screen position; the vector is the contract.
class Dir {
  final int i;
  final String name;
  final proto.Hex v;
  final String key;
  final String bearing;
  const Dir(this.i, this.name, this.v, this.key, this.bearing);

  /// Angle on screen of this direction (flat-top axial projection).
  double get angle => math.atan2(math.sqrt(3) / 2 * v.q + math.sqrt(3) * v.r, 1.5 * v.q);
}

const dirs = [
  Dir(0, 'N', proto.Hex(0, -1), 'w', '000'),
  Dir(1, 'NE', proto.Hex(1, -1), 'e', '060'),
  Dir(2, 'SE', proto.Hex(1, 0), 'd', '120'),
  Dir(3, 'S', proto.Hex(0, 1), 's', '180'),
  Dir(4, 'SW', proto.Hex(-1, 1), 'a', '240'),
  Dir(5, 'NW', proto.Hex(-1, 0), 'q', '300'),
];

/// Arena placement inside the main area (prototype resize()). The desktop
/// layout keeps a column each side of the arena and sizes the hex so that
/// the gate cards stand between them. An area too narrow for that with a hex
/// of [_leastHex] gets the phone layout ([narrow]), which has no columns: the
/// fleets and the dock run along the bottom and the panels open as a sheet.
/// Its gate cards are the full ones while the arena has the width for them
/// and the small ones ([compact]) on a phone.
class Geo {
  final Size size;
  final bool narrow;
  final bool compact;
  late final double cx, cy, rh, sc;

  static const _left = 236.0, _right = 324.0;
  static const _leastHex = 130.0;
  static const _cardHalfWidth = 75.0, _cardHalfHeight = 36.0, _cardGap = 14.0;
  static const _cos30 = 0.8660254037844386;

  /// How far past three quarters of the hex radius the outer edge of a
  /// card on a slanted gate reaches from the arena's centre line, with a
  /// little air beyond it.
  static const _cardReach = _cos30 * (_cardHalfWidth * _cos30 + _cardHalfHeight * .5 + _cardGap) + _cardHalfWidth + 8;

  /// How far past the hex's flat side the far edge of the card on the north
  /// or south gate reaches, at the card's tallest.
  static const _cardRise = _cardHalfHeight + _cardGap + 50;

  /// The narrowest area the desktop layout fits.
  static const desktopLeast = _left + _right + 2 * (.75 * _leastHex + _cardReach);

  /// What the phone layout keeps free under the arena for the fleets and the
  /// dock, and above it when the cards are full ones.
  static const _phoneFoot = 116.0, _phoneHead = 8.0;

  /// The least area the phone layout shows full gate cards in: narrower, the
  /// card on the north gate would stand on the sector head beside it;
  /// lower, the cards on the north and south gates would not clear a hex of
  /// [_leastHex].
  static const fullCardsWidth = 704.0;
  static const fullCardsHeight = _phoneHead + _phoneFoot + 2 * (_cos30 * _leastHex + _cardRise);

  Geo(this.size)
      : narrow = size.width < desktopLeast,
        compact = size.width < fullCardsWidth || size.width < desktopLeast && size.height < fullCardsHeight {
    final w = size.width, h = size.height;
    final double most;
    if (compact) {
      final ah = h - 150;
      cx = w / 2;
      cy = 62 + ah / 2;
      most = math.min(w / 2.8, ah / 2.3);
    } else if (narrow) {
      final ah = h - _phoneHead - _phoneFoot;
      cx = w / 2;
      cy = _phoneHead + ah / 2;
      most = math.min((w / 2 - _cardReach) / .75, (ah / 2 - _cardRise) / _cos30);
    } else {
      final aw = w - _left - _right, ah = h - 112;
      cx = _left + aw / 2;
      cy = 32 + ah / 2;
      most = math.min(math.min(aw / 2.7, ah / 2.45), (aw / 2 - _cardReach) / .75);
    }
    rh = math.max(_leastHex, math.min(most, 300.0));
    sc = rh / 300;
  }

  Offset wp(double x, double y) => Offset(cx + x * rh, cy + y * rh);

  Offset gatePoint(Dir d) => Offset(cx + math.cos(d.angle) * rh * .866, cy + math.sin(d.angle) * rh * .866);

  /// Centre of the gate card, pushed outside the hex.
  Offset cardCenter(Dir d) {
    final a = d.angle;
    final g = gatePoint(d);
    final gw = compact ? 46.0 : _cardHalfWidth, gh = compact ? 14.0 : _cardHalfHeight;
    final dd = gw * math.cos(a).abs() + gh * math.sin(a).abs() + (compact ? 8 : _cardGap);
    var kx = g.dx + math.cos(a) * dd;
    final ky = g.dy + math.sin(a) * dd;
    if (compact) kx = kx.clamp(48.0, size.width - 48);
    return Offset(kx, ky);
  }
}
