import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';

enum AmberLevel { full, bright, normal, dim, faint, glow, danger }

class AmberText extends StatelessWidget {
  final String text;
  final AmberLevel level;
  final double? size;
  final FontWeight? weight;

  const AmberText(
    this.text, {
    super.key,
    this.level = AmberLevel.normal,
    this.size,
    this.weight,
  });

  const AmberText.full(this.text, {super.key, this.size, this.weight})
      : level = AmberLevel.full;
  const AmberText.bright(this.text, {super.key, this.size, this.weight})
      : level = AmberLevel.bright;
  const AmberText.dim(this.text, {super.key, this.size, this.weight})
      : level = AmberLevel.dim;
  const AmberText.faint(this.text, {super.key, this.size, this.weight})
      : level = AmberLevel.faint;
  const AmberText.danger(this.text, {super.key, this.size, this.weight})
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
    return Text(
      text,
      style: Amber.mono(
        color: _color,
        size: size ?? 12,
        weight: weight ?? FontWeight.w400,
      ),
    );
  }
}
