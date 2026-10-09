import 'package:flutter/material.dart';

import 'tokens.dart';

ThemeData consoleTheme() => ThemeData.dark(useMaterial3: true).copyWith(
      scaffoldBackgroundColor: C.void_,
      visualDensity: VisualDensity.compact,
      splashFactory: NoSplash.splashFactory,
      colorScheme: const ColorScheme.dark(primary: C.a500, surface: C.deep, onSurface: C.text, error: C.ember),
      textTheme: ThemeData.dark().textTheme.apply(fontFamily: 'IBMPlexMono', bodyColor: C.text, displayColor: C.text),
      textSelectionTheme: TextSelectionThemeData(cursorColor: C.a500, selectionColor: C.a(C.a500, .3), selectionHandleColor: C.a500),
      scrollbarTheme: ScrollbarThemeData(
        thumbColor: WidgetStateProperty.all(C.a800),
        trackColor: WidgetStateProperty.all(C.a(C.a500, .04)),
        radius: Radius.zero,
        thickness: WidgetStateProperty.all(6),
      ),
      tooltipTheme: TooltipThemeData(
        decoration: BoxDecoration(color: const Color(0xFF0B0805), border: Border.all(color: C.lineHi)),
        textStyle: T.mono(size: 10.5, color: C.text),
        waitDuration: const Duration(milliseconds: 400),
      ),
    );
