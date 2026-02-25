import 'dart:ui';

import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';

enum AmberLevel { full, bright, normal, dim, faint, glow, danger }

class AmberText extends StatelessWidget {
  final String text;
  final AmberLevel level;
  final double? size;
  final FontWeight? weight;
  final bool bloom;

  const AmberText(
    this.text, {
    super.key,
    this.level = AmberLevel.normal,
    this.size,
    this.weight,
    this.bloom = false,
  });

  const AmberText.full(this.text, {super.key, this.size, this.weight, this.bloom = true})
      : level = AmberLevel.full;
  const AmberText.bright(this.text, {super.key, this.size, this.weight, this.bloom = false})
      : level = AmberLevel.bright;
  const AmberText.dim(this.text, {super.key, this.size, this.weight})
      : level = AmberLevel.dim,
        bloom = false;
  const AmberText.faint(this.text, {super.key, this.size, this.weight})
      : level = AmberLevel.faint,
        bloom = false;
  const AmberText.danger(this.text, {super.key, this.size, this.weight, this.bloom = true})
      : level = AmberLevel.danger;

  Color get _color {
    switch (level) {
      case AmberLevel.full:
        return Amber.full;
      case AmberLevel.bright:
        return Amber.bright;
      case AmberLevel.normal:
        return Amber.normal;
      case AmberLevel.dim:
        return Amber.dim;
      case AmberLevel.faint:
        return Amber.faint;
      case AmberLevel.glow:
        return Amber.glow;
      case AmberLevel.danger:
        return Amber.danger;
    }
  }

  @override
  Widget build(BuildContext context) {
    final style = Amber.mono(
      color: _color,
      size: size ?? 12,
      weight: weight ?? FontWeight.w400,
    );

    if (!bloom) return Text(text, style: style);

    return Stack(
      children: [
        ImageFiltered(
          imageFilter: ImageFilter.blur(sigmaX: 3, sigmaY: 1),
          child: Text(
            text,
            style: style.copyWith(
              color: _color.withValues(alpha: 0.4),
            ),
          ),
        ),
        Text(text, style: style),
      ],
    );
  }
}
