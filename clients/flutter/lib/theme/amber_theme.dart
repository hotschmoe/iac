import 'package:flutter/material.dart';
import 'package:google_fonts/google_fonts.dart';

class Amber {
  Amber._();

  static const Color full = Color(0xFFFFB000);
  static const Color bright = Color(0xD9FFB000);
  static const Color normal = Color(0x99FFB000);
  static const Color dim = Color(0x4DFFB000);
  static const Color faint = Color(0x1FFFB000);
  static const Color glow = Color(0x0FFFB000);
  static const Color danger = Color(0xFFFF6B35);

  static const Color bg = Color(0xFF060500);
  static const Color bgPanel = Color(0xFF0A0800);
  static const Color bgInset = Color(0xFF040300);
  static const Color border = Color(0x14FFB000);

  static TextStyle mono({
    double size = 12,
    Color color = normal,
    FontWeight weight = FontWeight.w400,
  }) {
    return GoogleFonts.jetBrainsMono(
      fontSize: size,
      color: color,
      fontWeight: weight,
      height: 1.5,
    );
  }

  static ThemeData themeData() {
    return ThemeData.dark().copyWith(
      scaffoldBackgroundColor: bg,
      colorScheme: const ColorScheme.dark(
        primary: full,
        surface: bgPanel,
      ),
      textTheme: TextTheme(
        bodyMedium: mono(),
        bodySmall: mono(size: 10),
        titleLarge: mono(size: 22, color: full, weight: FontWeight.w700),
        titleMedium: mono(size: 13, color: full),
        labelSmall: mono(size: 9, color: dim),
      ),
    );
  }
}
