import 'package:flutter/material.dart';

import '../design/panel.dart';
import '../design/tokens.dart';

/// Name prompt shown before connecting (skipped when `?name=` is in the URL),
/// and again when the server refuses the login. A new name creates an
/// account; a known name needs the token issued when it was created (usually
/// remembered by the browser, otherwise typed here).
class LoginScreen extends StatefulWidget {
  final String url;
  final String initialName;

  /// Why the last login was refused, if it was.
  final String? error;

  /// Always offer the token field, even before a refusal.
  final bool askToken;
  final void Function(String name, String token) onConnect;
  final VoidCallback onDemo;

  const LoginScreen({
    super.key,
    required this.url,
    required this.initialName,
    this.error,
    this.askToken = false,
    required this.onConnect,
    required this.onDemo,
  });

  @override
  State<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends State<LoginScreen> {
  late final TextEditingController _name = TextEditingController(text: widget.initialName);
  final TextEditingController _token = TextEditingController();

  void _submit() {
    final n = _name.text.trim();
    if (n.isNotEmpty) widget.onConnect(n, _token.text.trim());
  }

  @override
  void dispose() {
    _name.dispose();
    _token.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      color: C.void_,
      alignment: Alignment.center,
      child: SizedBox(
        width: 420,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('IDENTIFY YOURSELF', style: T.cond(size: 18, color: C.a100, weight: FontWeight.w700, spacing: 4)),
              const SizedBox(height: 4),
              Text('UPLINK ${widget.url}', style: T.mono(size: 10, color: C.text3)),
              const SizedBox(height: 16),
              Row(
                children: [
                  Text('>', style: T.mono(size: 14, color: C.a500)),
                  const SizedBox(width: 8),
                  Expanded(
                    child: TextField(
                      controller: _name,
                      autofocus: true,
                      maxLength: 24,
                      style: T.mono(size: 14, color: C.a100),
                      cursorColor: C.a500,
                      decoration: InputDecoration(
                        isDense: true,
                        counterText: '',
                        hintText: 'admiral name',
                        hintStyle: T.mono(size: 14, color: C.text4),
                        enabledBorder: const UnderlineInputBorder(borderSide: BorderSide(color: C.line)),
                        focusedBorder: const UnderlineInputBorder(borderSide: BorderSide(color: C.a500)),
                      ),
                      onSubmitted: (_) => _submit(),
                    ),
                  ),
                ],
              ),
              if (widget.error != null || widget.askToken) ...[
                const SizedBox(height: 12),
                Row(
                  children: [
                    Text('#', style: T.mono(size: 14, color: C.a500)),
                    const SizedBox(width: 8),
                    Expanded(
                      child: TextField(
                        key: const Key('token-field'),
                        controller: _token,
                        obscureText: true,
                        style: T.mono(size: 14, color: C.a100),
                        cursorColor: C.a500,
                        decoration: InputDecoration(
                          isDense: true,
                          hintText: 'account token',
                          hintStyle: T.mono(size: 14, color: C.text4),
                          enabledBorder: const UnderlineInputBorder(borderSide: BorderSide(color: C.line)),
                          focusedBorder: const UnderlineInputBorder(borderSide: BorderSide(color: C.a500)),
                        ),
                        onSubmitted: (_) => _submit(),
                      ),
                    ),
                  ],
                ),
              ],
              if (widget.error != null) ...[
                const SizedBox(height: 10),
                Text(widget.error!, key: const Key('login-error'), style: T.mono(size: 11, color: C.ember)),
              ],
              const SizedBox(height: 16),
              Row(
                children: [
                  _button('Connect', _submit, primary: true),
                  const SizedBox(width: 8),
                  _button('Offline demo', widget.onDemo),
                ],
              ),
              const SizedBox(height: 12),
              Text(
                'New name = new homeworld; its token is shown once, then remembered here.\n'
                'Known name = enter its token if this browser does not have it.',
                style: T.mono(size: 10, color: C.text3),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _button(String label, VoidCallback onTap, {bool primary = false}) =>
      ConsoleButton(label, onPressed: onTap, primary: primary);
}
