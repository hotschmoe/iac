import 'dart:math';

import 'package:flutter/material.dart';

import '../design/tokens.dart';

class BootScreen extends StatefulWidget {
  final VoidCallback onComplete;
  const BootScreen({super.key, required this.onComplete});

  @override
  State<BootScreen> createState() => _BootScreenState();
}

class _BootScreenState extends State<BootScreen> {
  static const _coreLines = [
    'BIOS POST ................. OK',
    'MEM CHECK 64K ............. OK',
    'LOADING OPERATOR CONSOLE MOD 4',
    'INIT WEBSOCKET UPLINK ..... STANDBY',
    'SYNC UNIVERSE STATE ....... PENDING',
    'DECRYPTING SECTOR DATA .... OK',
    'CALIBRATING NAV ARRAY ..... OK',
    'WEAPONS SYSTEMS ........... STANDBY',
    'PHOSPHOR .................. AMBER',
  ];

  static const _glitchVariants = [
    'SECTOR CACHE .............. 3.2% CORRUPTED -- REBUILDING',
    'NAV BEACON [12,-4] ........ SIGNAL LOST -- REROUTING',
    'HULL SENSOR ARRAY ......... RECALIBRATING (thermal drift)',
    'COMMS RELAY ............... TIMEOUT -- RETRY OK',
    'SHIELD HARMONIC ........... DRIFT DETECTED -- COMPENSATING',
    'DEUTERIUM SENSOR .......... VARIANCE +0.3% -- ACCEPTABLE',
    'MLM SIGNATURE DATABASE .... 847 ENTRIES -- UPDATED',
    'TRANSPONDER ECHO .......... STALE -- REFRESHED',
    'STAR CHART DELTA .......... 14 SECTORS REVISED',
    'POWER GRID ................ FLUCTUATION -- STABILIZED',
  ];

  late final List<String> _bootLines;
  late final List<bool> _lineVisible;

  bool _titleVisible = false;
  bool _subVisible = false;
  bool _fadeOut = false;

  @override
  void initState() {
    super.initState();
    _bootLines = _generateBootSequence();
    _lineVisible = List.filled(_bootLines.length, false);
    _runBoot();
  }

  List<String> _generateBootSequence() {
    final rng = Random();
    final lines = List<String>.from(_coreLines);

    // Insert 1-2 random glitch/warning lines at varying positions
    final shuffled = List<String>.from(_glitchVariants)..shuffle(rng);
    final glitchCount = 1 + rng.nextInt(2);
    for (int i = 0; i < glitchCount; i++) {
      final insertAt = 3 + rng.nextInt(lines.length - 3);
      lines.insert(insertAt, shuffled[i]);
    }

    lines.add('');
    lines.add('WELCOME ABOARD, ADMIRAL.');
    lines.add('');
    return lines;
  }

  bool _isGlitchLine(String line) {
    return _glitchVariants.contains(line);
  }

  Future<void> _runBoot() async {
    await Future.delayed(const Duration(milliseconds: 100));
    if (!mounted) return;
    setState(() => _titleVisible = true);

    await Future.delayed(const Duration(milliseconds: 200));
    if (!mounted) return;
    setState(() => _subVisible = true);

    await Future.delayed(const Duration(milliseconds: 200));

    for (int i = 0; i < _bootLines.length; i++) {
      if (!mounted) return;
      await Future.delayed(const Duration(milliseconds: 15));
      setState(() => _lineVisible[i] = true);
      final line = _bootLines[i];
      final isGlitch = _isGlitchLine(line);
      final delay = line.isEmpty ? 30 : isGlitch ? 120 : 20 + (15 * (i % 3));
      await Future.delayed(Duration(milliseconds: delay));
    }

    await Future.delayed(const Duration(milliseconds: 150));
    if (!mounted) return;
    setState(() => _fadeOut = true);

    await Future.delayed(const Duration(milliseconds: 300));
    if (mounted) widget.onComplete();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedOpacity(
      opacity: _fadeOut ? 0 : 1,
      duration: const Duration(milliseconds: 300),
      child: GestureDetector(
       behavior: HitTestBehavior.opaque,
       onTap: widget.onComplete,
       child: Container(
        color: C.void_,
        child: Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              AnimatedOpacity(
                opacity: _titleVisible ? 1 : 0,
                duration: const Duration(seconds: 1),
                child: Text('IN AMBER CLAD', style: T.cond(size: 26, color: C.a100, weight: FontWeight.w700, spacing: 8)),
              ),
              const SizedBox(height: 12),
              AnimatedOpacity(
                opacity: _subVisible ? 1 : 0,
                duration: const Duration(milliseconds: 800),
                child: Text(
                  'OPERATOR CONSOLE / MOD 4',
                  style: T.mono(size: 10, color: C.text3, spacing: 4),
                ),
              ),
              const SizedBox(height: 16),
              SizedBox(
                width: 500,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    for (int i = 0; i < _bootLines.length; i++)
                      AnimatedOpacity(
                        opacity: _lineVisible[i] ? 1 : 0,
                        duration: const Duration(milliseconds: 300),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(vertical: 2),
                          child: Text(
                            _bootLines[i],
                            style: T.mono(size: 11, color: _isGlitchLine(_bootLines[i]) ? C.a300 : C.text3, height: 1.7),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
      ),
    );
  }
}
