import 'package:flutter/material.dart';

/// Amber Console tokens (docs/design/ui-console/style-guide.md section 2).
class C {
  C._();

  static const void_ = Color(0xFF08060A);
  static const deep = Color(0xFF0B0809);
  static const panel = Color(0xEE0C0906);
  static const panelHi = Color(0xFF17110B);
  static const inset = Color(0xFF070504);
  static const hud = Color(0xFF0A0705);
  static const rail = Color(0xFF090604);

  static const a100 = Color(0xFFFFF3D6);
  static const a200 = Color(0xFFFFE2A3);
  static const a300 = Color(0xFFFFCC66);
  static const a400 = Color(0xFFFFBD33);
  static const a500 = Color(0xFFFFB000);
  static const a600 = Color(0xFFD98F00);
  static const a700 = Color(0xFF9A6500);
  static const a800 = Color(0xFF5E3D06);
  static const a900 = Color(0xFF2E1E06);

  static const line = Color(0x3DFFB000);
  static const lineHi = Color(0x8CFFB000);
  static const lineLo = Color(0x1AFFB000);

  static const text = Color(0xFFF4DFB2);
  static const text2 = Color(0xFFBFA370);
  static const text3 = Color(0xFF8A7752);
  static const text4 = Color(0xFF4D4230);

  static const own = Color(0xFF5EE0CF);
  static const ownDim = Color(0xFF2A6F68);
  static const ember = Color(0xFFFF5A3C);
  static const rare = Color(0xFFB793FF);
  static const rareDim = Color(0xFF5C4690);
  static const ok = Color(0xFF8FE388);
  static const gold = Color(0xFFFFCC66);

  static const metal = Color(0xFFD9B46A);
  static const crystal = Color(0xFF7FD4FF);
  static const deut = Color(0xFF6FE6B6);

  static const fossil = Color(0xFF8A7552);
  static const fossilBg = Color(0xFF1C160D);

  static const _threat = [
    Color(0xFF4A6E68),
    Color(0xFF4FB9A8),
    Color(0xFF7FCF8A),
    Color(0xFFC9D85A),
    Color(0xFFFFD24A),
    Color(0xFFFF9A2E),
    Color(0xFFFF6A3C),
    Color(0xFFFF4040),
    Color(0xFFFF2E86),
    Color(0xFFD04BFF),
  ];

  /// Threat T1..T9; 0 is "clear".
  static Color threat(int t) => _threat[t.clamp(0, 9)];

  static const ratioDeadly = Color(0xFFFF2E86);
  static const ratioRisky = Color(0xFFFF9A2E);
  static const ratioFav = Color(0xFF8FE388);
  static const ratioSafe = Color(0xFF5EE0CF);

  static Color a(Color c, double opacity) => c.withValues(alpha: opacity);

  static Color ore(String k) => switch (k) {
        'm' || 'metal' => metal,
        'c' || 'crystal' => crystal,
        _ => deut,
      };
}

/// Text styles. Plex Mono for data, Plex Sans Condensed for labels and titles.
class T {
  T._();

  static const _tab = [FontFeature.tabularFigures()];

  static TextStyle mono(
          {double size = 12, Color color = C.text, FontWeight weight = FontWeight.w400, double spacing = 0, double? height}) =>
      TextStyle(
        fontFamily: 'IBMPlexMono',
        fontSize: size,
        color: color,
        fontWeight: weight,
        letterSpacing: spacing,
        height: height,
        fontFeatures: _tab,
        decoration: TextDecoration.none,
      );

  static TextStyle cond(
          {double size = 11, Color color = C.text2, FontWeight weight = FontWeight.w600, double spacing = 1.6, double? height}) =>
      TextStyle(
        fontFamily: 'IBMPlexSansCondensed',
        fontSize: size,
        color: color,
        fontWeight: weight,
        letterSpacing: spacing,
        height: height,
        fontFeatures: _tab,
        decoration: TextDecoration.none,
      );

  /// Small caps label (use with a string already upper-cased or via [label]).
  static TextStyle get lbl => cond(size: 10, color: C.text3, weight: FontWeight.w500, spacing: 1.8);
}

/// Layout breakpoints.
class Bp {
  Bp._();
  static const narrow = 820.0;
  static const windshieldNarrow = 900.0;
}
