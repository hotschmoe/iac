import 'package:flutter/material.dart';

import '../theme/amber_theme.dart';
import '../widgets/amber_text.dart';

/// Name prompt shown before connecting (skipped when `?name=` is in the URL).
/// The server identifies players by name: a known name resumes that game.
class LoginScreen extends StatefulWidget {
  final String url;
  final String initialName;
  final void Function(String name) onConnect;
  final VoidCallback onDemo;

  const LoginScreen({
    super.key,
    required this.url,
    required this.initialName,
    required this.onConnect,
    required this.onDemo,
  });

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  late final TextEditingController _name = TextEditingController(text: widget.initialName);

  void _submit() {
    final n = _name.text.trim();
    if (n.isNotEmpty) widget.onConnect(n);
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      color: Amber.bg,
      alignment: Alignment.center,
      child: SizedBox(
        width: 420,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const AmberText.full('IDENTIFY YOURSELF', size: 15, weight: FontWeight.w700, letterSpacing: 4),
              const SizedBox(height: 4),
              Text('uplink: ${widget.url}', style: Amber.mono(size: 10, color: Amber.dim)),
              const SizedBox(height: 16),
              Row(
                children: [
                  const AmberText.full('>', size: 14),
                  const SizedBox(width: 8),
                  Expanded(
                    child: TextField(
                      controller: _name,
                      autofocus: true,
                      maxLength: 24,
                      style: Amber.mono(size: 14, color: Amber.bright),
                      cursorColor: Amber.full,
                      decoration: InputDecoration(
                        isDense: true,
                        counterText: '',
                        hintText: 'admiral name',
                        hintStyle: Amber.mono(size: 14, color: Amber.faint),
                        enabledBorder: const UnderlineInputBorder(borderSide: BorderSide(color: Amber.dim)),
                        focusedBorder: const UnderlineInputBorder(borderSide: BorderSide(color: Amber.full)),
                      ),
                      onSubmitted: (_) => _submit(),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 16),
              Row(
                children: [
                  _button('CONNECT', _submit, primary: true),
                  const SizedBox(width: 8),
                  _button('OFFLINE DEMO', widget.onDemo),
                ],
              ),
              const SizedBox(height: 12),
              Text(
                'Known name = resume your game. New name = new homeworld.',
                style: Amber.mono(size: 10, color: Amber.dim),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _button(String label, VoidCallback onTap, {bool primary = false}) {
    return GestureDetector(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
        decoration: BoxDecoration(border: Border.all(color: primary ? Amber.full : Amber.dim)),
        child: Text(
          label,
          style: Amber.mono(size: 11, color: primary ? Amber.full : Amber.dim).copyWith(letterSpacing: 1),
        ),
      ),
    );
  }
}
